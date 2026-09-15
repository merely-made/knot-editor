// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Inline links with byte spans.
//!
//! inker's link walk is authoritative for *which* links carry a predicate, but
//! it returns no source ranges, and a rendered preview block has none either.
//! So the span comes from a bounded scan of the source's inline-link syntax and
//! the `rel` comes from the walk, joined by target URL in document order: the
//! n-th scanned occurrence of a URL pairs with the n-th statement for that URL.
//! An unmatched scan hit keeps `rel: None`; an unmatched statement is emitted
//! with `span: None`. A span is never guessed.

use crate::ReadingLinkV1;
use inker::{EngineDocument, link_statements};

/// Upper bound on scanned links, so a pathological source cannot make the scan
/// the expensive part of a bounded reading.
const MAX_SCANNED_LINKS: usize = 4_096;

struct Scanned {
    target: String,
    text: String,
    span: (usize, usize),
}

pub fn extract(source: &str, preview: &EngineDocument) -> Vec<ReadingLinkV1> {
    let scanned = scan(source);
    let statements = link_statements(preview);
    // Per target URL, the rel values still unclaimed, oldest first.
    let mut pending: Vec<(String, Vec<String>)> = Vec::new();
    for statement in &statements {
        match pending
            .iter_mut()
            .find(|(target, _)| target == &statement.target_url)
        {
            Some((_, rels)) => rels.push(statement.rel.clone()),
            None => pending.push((statement.target_url.clone(), vec![statement.rel.clone()])),
        }
    }
    let mut out = Vec::with_capacity(scanned.len());
    for hit in scanned {
        let rel = pending
            .iter_mut()
            .find(|(target, rels)| target == &hit.target && !rels.is_empty())
            .map(|(_, rels)| rels.remove(0));
        out.push(ReadingLinkV1 {
            target: hit.target,
            rel,
            text: hit.text,
            span: Some(hit.span),
        });
    }
    // Statements the scan could not place: real predicate links with no span.
    for (target, rels) in pending {
        for rel in rels {
            out.push(ReadingLinkV1 {
                target: target.clone(),
                rel: Some(rel),
                text: String::new(),
                span: None,
            });
        }
    }
    out
}

/// Scan `[text](url)` inline links, with the trailing `{...}` attribute block
/// folded into the span when present. Fenced code is skipped so a link inside a
/// code block is not offered as a selectable range.
fn scan(source: &str) -> Vec<Scanned> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut fenced = false;
    let mut line_start = 0;
    while line_start <= bytes.len() {
        let line_end = source[line_start..]
            .find('\n')
            .map_or(bytes.len(), |offset| line_start + offset);
        let line = &source[line_start..line_end];
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        } else if !fenced {
            scan_line(source, line_start, line_end, &mut out);
        }
        if out.len() >= MAX_SCANNED_LINKS {
            break;
        }
        line_start = line_end + 1;
    }
    out
}

fn scan_line(source: &str, line_start: usize, line_end: usize, out: &mut Vec<Scanned>) {
    let bytes = source.as_bytes();
    let mut index = line_start;
    while index < line_end {
        if bytes[index] != b'[' {
            index += 1;
            continue;
        }
        let Some(label_end) = find_byte(bytes, index + 1, line_end, b']') else {
            return;
        };
        if label_end + 1 >= line_end || bytes[label_end + 1] != b'(' {
            index = label_end + 1;
            continue;
        }
        let Some(url_end) = find_byte(bytes, label_end + 2, line_end, b')') else {
            return;
        };
        let mut span_end = url_end + 1;
        if span_end < line_end
            && bytes[span_end] == b'{'
            && let Some(attributes_end) = find_byte(bytes, span_end + 1, line_end, b'}')
        {
            span_end = attributes_end + 1;
        }
        out.push(Scanned {
            target: source[label_end + 2..url_end].to_owned(),
            text: source[index + 1..label_end].to_owned(),
            span: (index, span_end),
        });
        if out.len() >= MAX_SCANNED_LINKS {
            return;
        }
        index = span_end;
    }
}

fn find_byte(bytes: &[u8], from: usize, to: usize, needle: u8) -> Option<usize> {
    (from..to).find(|index| bytes[*index] == needle)
}
