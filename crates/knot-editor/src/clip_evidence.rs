// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Endpoint-owned retention for source artifacts attached to clips.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chirograph::{
    KnotClipArtifactRoleV1, KnotClipArtifactV1, KnotClipSelectorV1, PortableContentRefV1,
};
use serde::{Deserialize, Serialize};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Portable content identity written into clip provenance.
///
/// New references serialize a shared [`PortableContentRefV1`]: RFC 6920
/// SHA-256 identity beside the BLAKE3 address iroh uses. The public normalized
/// fields preserve the established Knot API, while deserialization also
/// accepts the legacy `urn:blake3` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotClipEvidenceRef {
    pub content_uri: String,
    pub digest: String,
    pub byte_size: u64,
    pub media_type: String,
    pub canonical_uri: String,
    pub role: KnotClipArtifactRoleV1,
    portable: Option<PortableContentRefV1>,
}

impl KnotClipEvidenceRef {
    /// Read a selected text range from already retained bytes. This has no
    /// fetch path: size, portable SHA-256 and transport BLAKE3 are checked
    /// before the selector can expose any text.
    pub fn selected_text(
        &self,
        bytes: &[u8],
        selector: &KnotClipSelectorV1,
    ) -> Result<String, String> {
        self.verify_bytes(bytes)?;
        selected_text(selector, self.role, bytes)
    }
    fn portable(artifact: &KnotClipArtifactV1) -> Self {
        let content = PortableContentRefV1::of(&artifact.bytes);
        Self {
            content_uri: content.portable_id.to_string(),
            digest: content.transport.to_string(),
            byte_size: content.byte_size,
            media_type: artifact.media_type.clone(),
            canonical_uri: artifact.canonical_uri.clone(),
            role: artifact.role,
            portable: Some(content),
        }
    }

    /// The shared portable reference, absent only for a decoded legacy clip.
    pub fn portable_content(&self) -> Option<&PortableContentRefV1> {
        self.portable.as_ref()
    }

    /// The BLAKE3 address the portable URI names, checked against the
    /// normalized fields.
    fn blake3_address(&self) -> Result<[u8; 32], String> {
        if let Some(content) = &self.portable {
            if self.content_uri != content.portable_id.to_string()
                || self.digest != content.transport.to_string()
                || self.byte_size != content.byte_size
            {
                return Err("clip evidence portable and normalized fields disagree".into());
            }
            return Ok(*content.transport.as_bytes());
        }
        let named = self
            .content_uri
            .strip_prefix("urn:blake3:")
            .ok_or_else(|| "legacy clip evidence URI is not a urn:blake3 reference".to_string())?;
        if named != self.digest {
            return Err("legacy clip evidence URI and digest disagree".into());
        }
        parse_digest(&self.digest)
    }

    /// Resolve the portable URI into the transport blob hash it names.
    #[cfg(feature = "peers")]
    pub fn blob_hash(&self) -> Result<transport::BlobHash, String> {
        self.blake3_address().map(transport::BlobHash::from_bytes)
    }

    /// Check bytes before they are exposed as the retained source artifact.
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        if u64::try_from(bytes.len()).ok() != Some(self.byte_size) {
            return Err("clip evidence byte length does not match its reference".into());
        }
        let actual = blake3::hash(bytes);
        if actual.as_bytes() != &self.blake3_address()? {
            return Err("clip evidence bytes do not match their BLAKE3 reference".into());
        }
        if let Some(content) = &self.portable {
            content
                .verify_bytes(bytes)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// One authored clip occurrence. Unlike the blob inventory, this preserves
/// document order and repeated citations, including equal text from different
/// source identities. The note owns these facts, not a live origin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotClipCitation {
    pub source_url: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub selectors: Vec<KnotClipSelectorV1>,
    pub evidence: Vec<KnotClipEvidenceRef>,
}

impl KnotClipCitation {
    /// Verify one retained artifact and resolve its single exact scalar
    /// range. A supplied quote must agree with that range. DOM selectors need
    /// their own capable reader and are never silently treated as text ranges.
    pub fn read_passage(&self, artifact: usize, bytes: &[u8]) -> Result<String, String> {
        let reference = self
            .evidence
            .get(artifact)
            .ok_or("The citation has no such artifact.")?;
        let positions: Vec<_> = self.selectors.iter().filter(|s| matches!(s,
            KnotClipSelectorV1::TextPosition { artifact_role, .. } if *artifact_role == reference.role)).collect();
        let [position] = positions.as_slice() else {
            return Err("The citation needs one exact text position for this artifact.".into());
        };
        let text = reference.selected_text(bytes, position)?;
        for quote in self.selectors.iter().filter(|s| {
            matches!(s,
            KnotClipSelectorV1::TextQuote { artifact_role, .. } if *artifact_role == reference.role)
        }) {
            if reference.selected_text(bytes, quote)? != text {
                return Err("The citation quote disagrees with its exact scalar range.".into());
            }
        }
        Ok(text)
    }
}

/// Decode evidence-bearing clip occurrences from the note itself. Unknown
/// fields remain forward-compatible; legacy reference-only notes still work
/// through `clip_evidence_references` without claiming an exact passage.
pub fn clip_citations(source: &[u8]) -> Result<Vec<KnotClipCitation>, String> {
    provenance_records(source)?
        .into_iter()
        .filter(|v| {
            v.get("schema").and_then(|s| s.as_str()) == Some(chirograph::KNOT_CLIP_INSERT_SCHEMA_V2)
        })
        .map(|value| {
            serde_json::from_value(value).map_err(|e| format!("Clip citation is malformed: {e}"))
        })
        .collect()
}

pub(crate) fn selected_text(
    selector: &KnotClipSelectorV1,
    role: KnotClipArtifactRoleV1,
    bytes: &[u8],
) -> Result<String, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| "A text selector requires a UTF-8 artifact.")?;
    match selector {
        KnotClipSelectorV1::TextPosition {
            artifact_role,
            start,
            end,
        } if *artifact_role == role => {
            let start = usize::try_from(*start).map_err(|_| "Text position is out of range.")?;
            let end = usize::try_from(*end).map_err(|_| "Text position is out of range.")?;
            if start >= end || end > text.chars().count() {
                return Err(
                    "Text position is not a nonempty Unicode scalar range in this artifact.".into(),
                );
            }
            Ok(text.chars().skip(start).take(end - start).collect())
        },
        KnotClipSelectorV1::TextQuote {
            artifact_role,
            exact,
            prefix,
            suffix,
        } if *artifact_role == role => {
            if exact.is_empty() {
                return Err("A text quote must be nonempty.".into());
            }
            let found = text.match_indices(exact).any(|(start, quote)| {
                prefix.as_deref().is_none_or(|p| text[..start].ends_with(p))
                    && suffix
                        .as_deref()
                        .is_none_or(|s| text[start + quote.len()..].starts_with(s))
            });
            if !found {
                return Err(
                    "The exact quote or its context is absent from the retained artifact.".into(),
                );
            }
            Ok(exact.clone())
        },
        _ => Err("The selector does not name a supported text range in this artifact role.".into()),
    }
}

fn provenance_records(source: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    const OPEN: &str = "```knot.clip.provenance\n";
    const CLOSE: &str = "\n```";
    let source = std::str::from_utf8(source)
        .map_err(|_| "Evidence references require a UTF-8 Djot source.")?;
    let mut rest = source;
    let mut records = Vec::new();
    while let Some(start) = rest.find(OPEN) {
        let body = &rest[start + OPEN.len()..];
        let end = body
            .find(CLOSE)
            .ok_or("Clip provenance fence is not closed.")?;
        records.push(
            serde_json::from_str(&body[..end])
                .map_err(|e| format!("Clip provenance is malformed: {e}"))?,
        );
        rest = &body[end + CLOSE.len()..];
    }
    Ok(records)
}

#[derive(Serialize)]
struct PortableEvidenceWire<'a> {
    content: &'a PortableContentRefV1,
    media_type: &'a str,
    canonical_uri: &'a str,
    role: KnotClipArtifactRoleV1,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EvidenceWire {
    Portable {
        content: PortableContentRefV1,
        media_type: String,
        canonical_uri: String,
        role: KnotClipArtifactRoleV1,
    },
    Legacy {
        content_uri: String,
        digest: String,
        byte_size: u64,
        media_type: String,
        canonical_uri: String,
        role: KnotClipArtifactRoleV1,
    },
}

impl Serialize for KnotClipEvidenceRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(content) = &self.portable {
            if self.content_uri != content.portable_id.to_string()
                || self.digest != content.transport.to_string()
                || self.byte_size != content.byte_size
            {
                return Err(serde::ser::Error::custom(
                    "clip evidence portable and normalized fields disagree",
                ));
            }
            PortableEvidenceWire {
                content,
                media_type: &self.media_type,
                canonical_uri: &self.canonical_uri,
                role: self.role,
            }
            .serialize(serializer)
        } else {
            serde_json::json!({
                "content_uri": self.content_uri,
                "digest": self.digest,
                "byte_size": self.byte_size,
                "media_type": self.media_type,
                "canonical_uri": self.canonical_uri,
                "role": self.role,
            })
            .serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for KnotClipEvidenceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match EvidenceWire::deserialize(deserializer)? {
            EvidenceWire::Portable {
                content,
                media_type,
                canonical_uri,
                role,
            } => Self {
                content_uri: content.portable_id.to_string(),
                digest: content.transport.to_string(),
                byte_size: content.byte_size,
                media_type,
                canonical_uri,
                role,
                portable: Some(content),
            },
            EvidenceWire::Legacy {
                content_uri,
                digest,
                byte_size,
                media_type,
                canonical_uri,
                role,
            } => Self {
                content_uri,
                digest,
                byte_size,
                media_type,
                canonical_uri,
                role,
                portable: None,
            },
        })
    }
}

/// Extract the portable evidence references authored by Knot clip v2 blocks.
///
/// These fenced JSON records are Knot-owned metadata inside an ordinary Djot
/// document. Unknown provenance fields remain forward-compatible; malformed or
/// internally inconsistent evidence references fail closed.
pub fn clip_evidence_references(source: &[u8]) -> Result<Vec<KnotClipEvidenceRef>, String> {
    let mut references = BTreeMap::<String, KnotClipEvidenceRef>::new();
    let mut identities = BTreeMap::<String, (u64, Option<PortableContentRefV1>)>::new();
    for value in provenance_records(source)? {
        if let Some(evidence) = value.get("evidence") {
            let entries = evidence
                .as_array()
                .ok_or_else(|| "clip provenance evidence must be an array".to_string())?;
            for entry in entries {
                let reference: KnotClipEvidenceRef = serde_json::from_value(entry.clone())
                    .map_err(|error| format!("clip evidence reference is malformed: {error}"))?;
                reference.blake3_address()?;
                if let Some((size, portable)) = identities.get(&reference.digest)
                    && (*size != reference.byte_size
                        || portable
                            .as_ref()
                            .zip(reference.portable.as_ref())
                            .is_some_and(|(a, b)| a != b))
                {
                    return Err(
                        "One clip evidence digest carries conflicting content identity.".into(),
                    );
                }
                let identity = identities
                    .entry(reference.digest.clone())
                    .or_insert((reference.byte_size, reference.portable.clone()));
                if identity.1.is_none() {
                    identity.1 = reference.portable.clone();
                }
                // A content digest owns bytes. URI, role and media type belong
                // to each citation, and may legitimately differ for those bytes.
                let key = serde_json::to_string(&reference).map_err(|e| e.to_string())?;
                references.entry(key).or_insert(reference);
            }
        }
    }
    Ok(references.into_values().collect())
}

/// Host-injected authority for retaining clip evidence.
pub trait KnotClipEvidenceStore: Send {
    /// Retain an artifact under its content identity. Implementations must be
    /// idempotent for identical bytes.
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String>;
}

/// A content-addressed local store rooted at an explicitly configured path.
pub struct FileClipEvidenceStore {
    root: PathBuf,
    max_artifact_bytes: u64,
}

impl FileClipEvidenceStore {
    pub fn new(root: impl Into<PathBuf>, max_artifact_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_artifact_bytes,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn artifact_path(&self, digest: &str) -> PathBuf {
        self.root.join("blake3").join(digest)
    }
}

impl KnotClipEvidenceStore for FileClipEvidenceStore {
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String> {
        let byte_size = u64::try_from(artifact.bytes.len())
            .map_err(|_| "clip artifact byte length does not fit u64".to_string())?;
        if byte_size > self.max_artifact_bytes {
            return Err(format!(
                "clip artifact is {byte_size} bytes; configured evidence limit is {}",
                self.max_artifact_bytes
            ));
        }
        let digest = blake3::hash(&artifact.bytes).to_hex().to_string();
        let path = self.artifact_path(&digest);
        if path.exists() {
            let existing = fs::read(&path)
                .map_err(|error| format!("could not verify retained clip evidence: {error}"))?;
            if existing != artifact.bytes {
                return Err("retained clip evidence does not match its BLAKE3 address".into());
            }
        } else {
            let parent = path
                .parent()
                .ok_or_else(|| "clip evidence path has no parent".to_string())?;
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create clip evidence directory: {error}"))?;
            let temporary = parent.join(format!(
                ".{digest}.{}.{}.tmp",
                std::process::id(),
                NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
            ));
            if let Err(error) = write_new(&temporary, &artifact.bytes) {
                let _ = fs::remove_file(&temporary);
                return Err(format!("could not stage clip evidence: {error}"));
            }
            match fs::rename(&temporary, &path) {
                Ok(()) => {},
                Err(error) if path.exists() => {
                    let _ = fs::remove_file(&temporary);
                    let existing = fs::read(&path).map_err(|read_error| {
                        format!(
                            "clip evidence raced with another writer ({error}) and could not be verified: {read_error}"
                        )
                    })?;
                    if existing != artifact.bytes {
                        return Err(
                            "retained clip evidence does not match its BLAKE3 address".into()
                        );
                    }
                },
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    return Err(format!("could not install clip evidence: {error}"));
                },
            }
        }
        let reference = KnotClipEvidenceRef::portable(artifact);
        debug_assert_eq!(reference.digest, digest);
        debug_assert_eq!(reference.byte_size, byte_size);
        Ok(reference)
    }
}

#[cfg(feature = "peers")]
mod blob;
#[cfg(feature = "peers")]
pub use blob::{BlobClipEvidenceStore, KnotContentRetentionPort};

fn parse_digest(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("clip evidence digest must be 64 lowercase hexadecimal characters".into());
    }
    let mut digest = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(pair).expect("ASCII hex was already checked");
        digest[index] = u8::from_str_radix(hex, 16)
            .map_err(|_| "clip evidence digest is not hexadecimal".to_string())?;
    }
    Ok(digest)
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn file_store_is_content_addressed_and_idempotent() {
        let temp = tempdir().unwrap();
        let mut store = FileClipEvidenceStore::new(temp.path(), 1024);
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/post".into(),
            bytes: b"<p>evidence</p>".to_vec(),
        };
        let first = store.retain(&artifact).unwrap();
        let second = store.retain(&artifact).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fs::read(store.artifact_path(&first.digest)).unwrap(),
            artifact.bytes
        );
        assert!(
            first
                .content_uri
                .starts_with(chirograph::Sha256NamedInformation::PREFIX)
        );
        assert_eq!(
            first.portable_content().unwrap().transport.to_string(),
            first.digest
        );
    }

    #[test]
    fn djot_provenance_yields_deduplicated_validated_references() {
        let bytes = b"portable evidence";
        let digest = blake3::hash(bytes).to_hex().to_string();
        let reference = KnotClipEvidenceRef {
            content_uri: format!("urn:blake3:{digest}"),
            digest,
            byte_size: bytes.len() as u64,
            media_type: "text/plain".into(),
            canonical_uri: "https://example.test/evidence".into(),
            role: KnotClipArtifactRoleV1::SourceResponse,
            portable: None,
        };
        let provenance = serde_json::json!({
            "schema": "knot.clip.insert/v2",
            "evidence": [reference.clone(), reference.clone()]
        });
        let source = format!(
            "# Note\n\n```knot.clip.provenance\n{}\n```\n",
            serde_json::to_string(&provenance).unwrap()
        );
        let decoded = clip_evidence_references(source.as_bytes()).unwrap();
        assert_eq!(decoded, vec![reference]);
        assert!(decoded[0].portable_content().is_none());
        decoded[0].verify_bytes(bytes).unwrap();
    }

    #[test]
    fn portable_and_transport_hashes_must_both_match() {
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/plain".into(),
            canonical_uri: "https://example.test/evidence".into(),
            bytes: b"portable evidence".to_vec(),
        };
        let reference = KnotClipEvidenceRef::portable(&artifact);
        let mut value = serde_json::to_value(&reference).unwrap();
        value["content"]["portable_id"] =
            serde_json::to_value(chirograph::Sha256NamedInformation::of(b"different bytes"))
                .unwrap();
        let conflicting: KnotClipEvidenceRef = serde_json::from_value(value).unwrap();
        assert!(
            conflicting.blake3_address().is_ok(),
            "the transport hash is valid"
        );
        assert!(
            conflicting.verify_bytes(&artifact.bytes).is_err(),
            "the conflicting portable identity fails closed",
        );
    }
    #[test]
    fn equal_bytes_from_distinct_saved_sources_keep_distinct_provenance() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = FileClipEvidenceStore::new(temp.path(), 4096);
        let mut artifacts = Vec::new();
        for source in ["A1", "B1"] {
            artifacts.push(KnotClipArtifactV1 {
                role: KnotClipArtifactRoleV1::ObservedRepresentation,
                media_type: "text/plain".into(),
                canonical_uri: format!("urn:saved:{source}"),
                bytes: "repeat café".as_bytes().to_vec(),
            });
        }
        let references: Vec<_> = artifacts.iter().map(|a| store.retain(a).unwrap()).collect();
        assert_eq!(references[0].content_uri, references[1].content_uri);
        let source = references.iter().map(|reference| format!("```knot.clip.provenance\n{}\n```\n",
            serde_json::json!({"schema":"knot.clip.insert/v2", "source_url":reference.canonical_uri,"selectors":[], "evidence":[reference]})))
            .collect::<Vec<_>>().join("\n");
        let reopened = clip_evidence_references(source.as_bytes()).unwrap();
        assert_eq!(
            reopened.len(),
            2,
            "deduplicate bytes, not distinct source citations"
        );
        for reference in &reopened {
            reference.verify_bytes(&artifacts[0].bytes).unwrap();
        }
        assert_eq!(fs::read_dir(temp.path().join("blake3")).unwrap().count(), 1);
    }}
