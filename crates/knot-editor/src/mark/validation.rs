// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Parse one complete bounded read-adapter request.
pub fn decode_mark_request(bytes: &[u8]) -> Result<MarkRequest, MarkAdapterError> {
    if bytes.len() > MARK_MAX_REQUEST_BYTES + MARK_MAX_METADATA_BYTES {
        return Err(MarkAdapterError::RequestTooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| MarkAdapterError::MalformedRequest)?;
    let Some(line_end) = text.find('\n') else {
        return Err(MarkAdapterError::MalformedRequest);
    };
    if line_end > MARK_MAX_REQUEST_BYTES || text[..line_end].contains('\r') {
        return Err(MarkAdapterError::MalformedRequest);
    }
    let line = &text[..line_end];
    let Some((verb, path)) = line.split_once(' ') else {
        return Err(MarkAdapterError::MalformedRequest);
    };
    if verb.is_empty() || path.is_empty() || path.contains(' ') {
        return Err(MarkAdapterError::MalformedRequest);
    }
    let path = validate_request_path(path)?;
    let (metadata, body) = parse_frontmatter(&text[line_end + 1..])?;
    if !body.is_empty() {
        return Err(MarkAdapterError::MalformedRequest);
    }
    let auth = metadata.get("auth").cloned();
    match verb {
        "FETCH" => Ok(MarkRequest::Fetch {
            path,
            auth,
            if_none_match: metadata.get("if-none-match").cloned(),
            if_modified_since: metadata
                .get("if-modified-since")
                .cloned()
                .map(MarkTimestamp::parse)
                .transpose()?,
        }),
        "VERSIONS" => Ok(MarkRequest::Versions { path, auth }),
        "LIST" | "PUBLISH" | "ARCHIVE" | "APPEND" | "LOOKUP" => Ok(MarkRequest::Other {
            verb: verb.into(),
            path,
        }),
        _ => Err(MarkAdapterError::MalformedRequest),
    }
}

pub(super) fn mark_stored_version(
    id: MarkVersionId,
    previous_hash: Option<[u8; 32]>,
    source: &KnotPublishedDocument,
    body: &[u8],
) -> Vec<u8> {
    let mut output = format!("---\nversion: {}\narchived: false\n", id.get());
    if let Some(previous_hash) = previous_hash {
        output.push_str(&format!("previous-hash: sha256-{}\n", hex(&previous_hash)));
    }
    output.push_str(&format!(
        "meta.mere-source-operation: {}\nmeta.mere-source-body-blake3: {}\nmeta.mere-source-media-type: {}\nmeta.mere-projection: canonical-commonmark\n",
        hex(&source.operation),
        hex(&source.body_digest),
        source.media_type,
    ));
    output.push_str("---\n");
    let mut stored = output.into_bytes();
    stored.extend_from_slice(body);
    stored
}

pub(super) fn mark_chain_is_valid(versions: &[MarkVersion]) -> bool {
    versions.windows(2).all(|pair| {
        let previous = &pair[0];
        let current = &pair[1];
        current.etag == sha256(&current.stored)
            && std::str::from_utf8(&current.stored).is_ok_and(|stored| {
                stored.contains(&format!("previous-hash: sha256-{}\n", previous.etag()))
            })
    })
}

pub(super) fn parse_frontmatter(
    tail: &str,
) -> Result<(BTreeMap<String, String>, &str), MarkAdapterError> {
    if !tail.starts_with("---\n") {
        return Ok((BTreeMap::new(), tail));
    }
    let remaining = &tail[4..];
    let Some(close) = remaining.find("\n---\n") else {
        return Err(MarkAdapterError::MalformedRequest);
    };
    let metadata_text = &remaining[..close];
    if metadata_text.len() > MARK_MAX_METADATA_BYTES {
        return Err(MarkAdapterError::RequestTooLarge);
    }
    let mut metadata = BTreeMap::new();
    for line in metadata_text.lines() {
        let Some((key, value)) = line.split_once(": ") else {
            return Err(MarkAdapterError::MalformedRequest);
        };
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || value.contains(['\r', '\n'])
            || metadata.insert(key.into(), value.into()).is_some()
        {
            return Err(MarkAdapterError::MalformedRequest);
        }
    }
    Ok((metadata, &remaining[close + 5..]))
}

pub(super) fn validate_request_path(path: &str) -> Result<String, MarkAdapterError> {
    if !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| byte == 0 || byte < 32 || byte == 127)
        || path.contains(['?', '#'])
        || path.split('/').any(|segment| matches!(segment, "." | ".."))
    {
        return Err(MarkAdapterError::InvalidPath);
    }
    Ok(path.into())
}

pub(super) fn validate_document_path(path: String) -> Result<String, MarkAdapterError> {
    let path = validate_request_path(&path)?;
    if !path.ends_with(".md") || path == "/.md" {
        return Err(MarkAdapterError::InvalidPath);
    }
    Ok(path)
}

pub(super) fn resolve_mark_path(
    path: &str,
) -> Result<(String, Option<MarkVersionId>), MarkAdapterError> {
    let path = validate_request_path(path)?;
    let Some((base, tail)) = path.rsplit_once('/') else {
        return Ok((path, None));
    };
    let Some(number) = tail.strip_prefix('v') else {
        return Ok((path, None));
    };
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok((path, None));
    }
    let version = number
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .map(MarkVersionId)
        .ok_or(MarkAdapterError::InvalidPath)?;
    Ok((base.into(), Some(version)))
}

pub(super) fn parse_content_hash(path: &str) -> Option<[u8; 32]> {
    let value = path.strip_prefix("/sha256-")?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut output = [0u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}

pub(super) fn is_utc_rfc3339_seconds(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
        || [0..4, 5..7, 8..10, 11..13, 14..16, 17..19]
            .into_iter()
            .flatten()
            .any(|index| !bytes[index].is_ascii_digit())
    {
        return false;
    }
    let number = |start: usize, end: usize| {
        bytes[start..end]
            .iter()
            .fold(0u8, |value, byte| value * 10 + (byte - b'0'))
    };
    (1..=12).contains(&number(5, 7))
        && (1..=31).contains(&number(8, 10))
        && number(11, 13) < 24
        && number(14, 16) < 60
        && number(17, 19) < 60
}

pub(super) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

pub(super) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub(super) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(super) fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}
