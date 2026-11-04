// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn an_index_sealed_under_another_identity_is_refused_naming_both_and_offers_rebuild() {
    let temp = tempdir().unwrap();
    let vault = orchard_vault(temp.path(), 0x66);
    let lexical = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();

    // A BERT index sealed by some other session over the same vault.
    let bert = canned_bert_identity(&"c".repeat(64));
    let bert_vectors: VectorIndex<String> = VectorIndex::new(384, SimilarityMetric::Cosine);
    vault.store_search_index(&bert, &bert_vectors).unwrap();

    let error = lexical
        .query(Some(&vault), "orchard", 1, subject(), &both())
        .unwrap_err();
    let KnotSearchError::IdentityMismatch { sealed, requested } = &error else {
        panic!("expected an identity refusal, got {error}");
    };
    assert_eq!(**sealed, bert);
    assert_eq!(requested.as_ref(), lexical.identity());
    let message = error.to_string();
    assert!(
        message.contains("bert-cpu") && message.contains("all-MiniLM-L6-v2"),
        "{message}"
    );
    assert!(
        message.contains("lexical") && message.contains("esp-lexical-hashing"),
        "{message}"
    );
    assert!(message.contains("rebuild"), "{message}");
    assert!(error.offers_rebuild());
    assert_eq!(error.rebuild_identity(), Some(lexical.identity()));

    // Refused means untouched: the BERT record is still there.
    let still = vault.load_search_index().unwrap().unwrap();
    assert_eq!(still.identity, Some(bert.clone()));

    // A build over it refuses the same way, before sealing anything.
    let error = KnotSearch::build(None, Some(&vault), SearchConfig::default())
        .err()
        .expect("a build must not silently replace another identity's index");
    assert!(error.offers_rebuild(), "{error}");
    assert_eq!(
        vault.load_search_index().unwrap().unwrap().identity,
        Some(bert)
    );

    // The offered rebuild is the explicit command, and it reports progress.
    let mut reports = Vec::new();
    lexical
        .rebuild_vault_index(&vault, |report| reports.push(report))
        .unwrap();
    assert_eq!(
        reports,
        vec![
            SearchProgress {
                lane: SearchLane::Vault,
                visited: 0,
                indexed: 0,
                total: 1,
            },
            SearchProgress {
                lane: SearchLane::Vault,
                visited: 1,
                indexed: 1,
                total: 1,
            },
        ]
    );
    assert_eq!(
        lexical
            .query(Some(&vault), "orchard", 1, subject(), &both())
            .unwrap()[0]
            .id,
        "orchard"
    );
}

#[test]
fn a_moved_weights_digest_is_refused_against_the_sealed_record() {
    let temp = tempdir().unwrap();
    let vault = orchard_vault(temp.path(), 0x67);
    let sealed = canned_bert_identity(&"d".repeat(64));
    let vectors: VectorIndex<String> = VectorIndex::new(384, SimilarityMetric::Cosine);
    vault.store_search_index(&sealed, &vectors).unwrap();

    // Same provider, model name and shape; only the weights moved.
    let mut requested = sealed.clone();
    requested.weights_digest = Some("e".repeat(64));
    let error = admit(vault.load_search_index().unwrap().unwrap(), &requested).unwrap_err();
    assert!(
        matches!(error, KnotSearchError::IdentityMismatch { .. }),
        "{error}"
    );
    assert!(error.offers_rebuild());
    assert_eq!(error.rebuild_identity(), Some(&requested));
    let message = error.to_string();
    assert!(
        message.contains("dddddddddddddddd") && message.contains("eeeeeeeeeeeeeeee"),
        "{message}"
    );

    // Moved and renamed: each side is still named by its own label.
    requested.model = "bge-micro-v2".into();
    let message = admit(vault.load_search_index().unwrap().unwrap(), &requested)
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("all-MiniLM-L6-v2") && message.contains("bge-micro-v2"),
        "{message}"
    );
}

#[test]
fn a_record_sealed_by_b7168a3_still_loads_and_compares_by_digest() {
    let temp = tempdir().unwrap();
    let vault = orchard_vault(temp.path(), 0x6c);
    let digest = "d".repeat(64);

    // The record b7168a3 sealed, written by hand rather than by today's
    // serializer, so a drift in either direction shows up here.
    let identity = serde_json::json!({
        "version": 1,
        "provider": "bert-cpu",
        "model": "all-MiniLM-L6-v2",
        "weights-digest": digest,
        "dimensions": 4,
        "metric": "Cosine",
    });
    vault
        .store_search_record_verbatim(&serde_json::json!({
            "identity": identity,
            "index": {
                "dimensions": 4,
                "metric": "Cosine",
                "entries": { "orchard": [1.0, 0.0, 0.0, 0.0] },
            },
        }))
        .unwrap();

    let sealed = vault.load_search_index().unwrap().unwrap();
    let recorded = sealed.identity.clone().expect("an identified record");
    assert_eq!(recorded.model, "all-MiniLM-L6-v2");
    assert_eq!(recorded.weights_digest.as_deref(), Some(digest.as_str()));
    assert_eq!(recorded.version, KNOT_INDEX_IDENTITY_VERSION);
    assert_eq!(serde_json::to_value(&recorded).unwrap(), identity);

    // The same weights under another directory name answer from it.
    let renamed = KnotIndexIdentity {
        model: "minilm".into(),
        dimensions: 4,
        ..canned_bert_identity(&digest)
    };
    let index = admit(sealed.clone(), &renamed).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        index.get(&"orchard".to_string()),
        Some(&vec![1.0, 0.0, 0.0, 0.0])
    );

    // The same name over other weights does not.
    let reweighted = KnotIndexIdentity {
        dimensions: 4,
        ..canned_bert_identity(&"e".repeat(64))
    };
    let error = admit(sealed, &reweighted).unwrap_err();
    assert!(
        matches!(error, KnotSearchError::IdentityMismatch { .. }),
        "{error}"
    );
    assert!(error.offers_rebuild());
}
