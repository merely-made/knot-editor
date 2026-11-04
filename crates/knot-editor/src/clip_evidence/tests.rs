// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
        serde_json::to_value(chirograph::Sha256NamedInformation::of(b"different bytes")).unwrap();
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
}
