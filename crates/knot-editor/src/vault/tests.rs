// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::fs;

use tempfile::tempdir;

use super::*;

fn note() -> VaultDocument {
    VaultDocument {
        id: "field-note".into(),
        title: "Field note".into(),
        body: b"private field observation".to_vec(),
        media_type: "text/vnd.knot".into(),
    }
}

#[test]
fn sealed_document_survives_reopen_without_plaintext_on_disk() {
    let temp = tempdir().unwrap();
    let key = [0x71; 32];
    let mut vault = KnotVault::open(temp.path(), key).unwrap();
    vault.put(note()).unwrap();
    drop(vault);

    let sealed = fs::read(temp.path().join(INDEX_PATH)).unwrap();
    assert!(
        !sealed
            .windows(b"private field observation".len())
            .any(|window| window == b"private field observation")
    );

    let vault = KnotVault::open(temp.path(), key).unwrap();
    assert_eq!(
        vault.body("field-note"),
        Some(&b"private field observation"[..])
    );
}

#[test]
fn lock_drops_documents_and_wrong_key_cannot_unlock() {
    let temp = tempdir().unwrap();
    let key = [0x72; 32];
    let mut vault = KnotVault::open(temp.path(), key).unwrap();
    vault.put(note()).unwrap();
    vault.lock();
    assert!(vault.is_locked());
    assert!(vault.documents().next().is_none());
    assert!(vault.body("field-note").is_none());
    assert!(vault.unlock([0x73; 32]).is_err());
    assert!(vault.unlock(key).is_ok());
    assert_eq!(
        vault.body("field-note"),
        Some(&b"private field observation"[..])
    );
}

#[test]
fn removing_a_projected_document_collects_its_derived_cache() {
    let temp = tempdir().unwrap();
    let mut vault = KnotVault::open(temp.path(), [0x74; 32]).unwrap();
    vault.put(note()).unwrap();
    vault
        .store_derived_cache("field-note", &"cached result".to_string())
        .unwrap();
    let public_id_hash = blake3::hash(b"field-note").to_hex();
    assert!(
        !temp
            .path()
            .join(format!("knot/derived-cache/{public_id_hash}.json"))
            .exists(),
        "cache paths must not expose a public dictionary hash of the document id"
    );
    assert_eq!(
        vault.load_derived_cache::<String>("field-note").unwrap(),
        Some("cached result".into())
    );

    assert!(vault.replace_projection(Vec::new()).unwrap());
    assert_eq!(
        vault.load_derived_cache::<String>("field-note").unwrap(),
        None
    );
}

#[test]
fn search_index_identity_round_trips_and_an_unidentified_record_still_loads() {
    use esp::embed::SimilarityMetric;

    use crate::embedding::{KNOT_INDEX_IDENTITY_VERSION, KnotEmbeddingProvider};

    let temp = tempdir().unwrap();
    let key = [0x75; 32];
    let vault = KnotVault::open(temp.path(), key).unwrap();
    let mut index: VectorIndex<String> = VectorIndex::new(4, SimilarityMetric::Cosine);
    index
        .insert("field-note".into(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();

    vault.store_search_index_unidentified(&index).unwrap();
    let legacy = vault.load_search_index().unwrap().unwrap();
    assert!(legacy.identity.is_none());
    assert_eq!(legacy.index.dimensions(), 4);
    assert_eq!(legacy.index.metric(), SimilarityMetric::Cosine);
    assert_eq!(legacy.index.len(), 1);

    let identity = KnotIndexIdentity {
        version: KNOT_INDEX_IDENTITY_VERSION,
        provider: KnotEmbeddingProvider::BertWgpu,
        model: "bge-micro-v2".into(),
        weights_digest: Some("9".repeat(64)),
        dimensions: 4,
        metric: SimilarityMetric::Cosine,
    };
    vault.store_search_index(&identity, &index).unwrap();
    drop(vault);

    let sealed_bytes = fs::read(temp.path().join(SEARCH_INDEX_PATH)).unwrap();
    for plaintext in [b"bge-micro-v2".as_slice(), b"bert-wgpu".as_slice()] {
        assert!(
            !sealed_bytes
                .windows(plaintext.len())
                .any(|window| window == plaintext),
            "identity is sealed with the index"
        );
    }

    let vault = KnotVault::open(temp.path(), key).unwrap();
    let sealed = vault.load_search_index().unwrap().unwrap();
    assert_eq!(
        sealed.identity.as_ref().map(|found| found.model.as_str()),
        Some("bge-micro-v2"),
        "the label is sealed too, though is_same_model does not compare it"
    );
    assert_eq!(sealed.identity, Some(identity));
    assert_eq!(
        sealed.index.get(&"field-note".to_string()),
        Some(&vec![1.0, 0.0, 0.0, 0.0])
    );
}
