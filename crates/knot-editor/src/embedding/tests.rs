// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::fs;

use tempfile::tempdir;

use super::*;

/// A model directory with the right shape and canned bytes. Enough to
/// exercise custody — presence, digest, refusal — with no real weights.
pub(crate) fn canned_model_directory(root: &Path, marker: &str) -> PathBuf {
    let directory = root.join("all-MiniLM-L6-v2");
    fs::create_dir_all(&directory).unwrap();
    for artifact in KNOT_BERT_ARTIFACTS {
        fs::write(directory.join(artifact), format!("{artifact}:{marker}")).unwrap();
    }
    directory
}

#[test]
fn lexical_is_the_default_and_needs_no_weights() {
    let preference = KnotEmbeddingPreference::default();
    assert_eq!(preference.provider, KnotEmbeddingProvider::Lexical);
    assert!(preference.weights.is_none());
    assert!(!KnotEmbeddingProvider::Lexical.needs_weights());
    assert!(KnotEmbeddingProvider::BertCpu.needs_weights());
    assert!(KnotEmbeddingProvider::BertWgpu.needs_weights());

    let (_, identity) = resolve_embeddings(&preference, 512).unwrap();
    assert_eq!(identity, KnotIndexIdentity::lexical(512, identity.metric));
    assert_eq!(identity.model, KNOT_LEXICAL_MODEL);
    assert!(identity.weights_digest.is_none());
    assert_eq!(identity.version, KNOT_INDEX_IDENTITY_VERSION);
}

#[test]
fn a_digest_covers_every_artifact_and_moves_when_one_does() {
    let temp = tempdir().unwrap();
    let first = canned_model_directory(&temp.path().join("a"), "one");
    let second = canned_model_directory(&temp.path().join("b"), "one");
    assert_eq!(
        KnotModelWeights::digest_of(&first).unwrap(),
        KnotModelWeights::digest_of(&second).unwrap(),
        "the same artifacts hash the same wherever they sit"
    );

    for artifact in KNOT_BERT_ARTIFACTS {
        let moved = canned_model_directory(&temp.path().join(artifact), "one");
        fs::write(moved.join(artifact), "changed").unwrap();
        assert_ne!(
            KnotModelWeights::digest_of(&first).unwrap(),
            KnotModelWeights::digest_of(&moved).unwrap(),
            "{artifact} moves the vector space, so it must move the digest"
        );
    }
}

#[test]
fn a_bert_preference_without_a_path_refuses_by_name() {
    let preference = KnotEmbeddingPreference {
        provider: KnotEmbeddingProvider::BertCpu,
        weights: None,
    };
    let error = resolve_embeddings(&preference, 512).unwrap_err();
    assert_eq!(
        error,
        KnotSearchError::WeightsNotSupplied {
            provider: KnotEmbeddingProvider::BertCpu
        }
    );
    assert!(error.to_string().contains("bert-cpu"));
}

#[test]
fn an_absent_or_incomplete_model_directory_refuses_with_its_reason() {
    let temp = tempdir().unwrap();
    let absent = temp.path().join("not-here");
    let error = resolve_embeddings(
        &KnotEmbeddingPreference::bert_cpu(KnotModelWeights::at(&absent)),
        512,
    )
    .unwrap_err();
    assert!(matches!(error, KnotSearchError::WeightsUnreadable { .. }));
    assert!(error.to_string().contains("no such model directory"));

    let partial = temp.path().join("partial");
    fs::create_dir_all(&partial).unwrap();
    fs::write(partial.join("config.json"), "{}").unwrap();
    let error = resolve_embeddings(
        &KnotEmbeddingPreference::bert_cpu(KnotModelWeights::at(&partial)),
        512,
    )
    .unwrap_err();
    let KnotSearchError::WeightsUnreadable { path, .. } = &error else {
        panic!("expected an unreadable-artifact refusal, got {error}");
    };
    assert!(path.ends_with("tokenizer.json"), "{path}");
}

#[test]
fn a_pinned_digest_that_moved_refuses_and_names_both() {
    let temp = tempdir().unwrap();
    let directory = canned_model_directory(temp.path(), "one");
    let actual = KnotModelWeights::digest_of(&directory).unwrap();
    let wrong = "0".repeat(64);

    let error = resolve_embeddings(
        &KnotEmbeddingPreference::bert_cpu(KnotModelWeights::pinned(&directory, &wrong)),
        512,
    )
    .unwrap_err();
    let KnotSearchError::WeightsDigestMismatch {
        recorded, found, ..
    } = &error
    else {
        panic!("expected a digest refusal, got {error}");
    };
    assert_eq!(recorded, &wrong);
    assert_eq!(found, &actual);
    let message = error.to_string();
    assert!(message.contains(&wrong) && message.contains(&actual));
}

#[test]
fn a_verified_directory_reaches_the_model_and_never_the_lexical_provider() {
    let temp = tempdir().unwrap();
    let directory = canned_model_directory(temp.path(), "one");
    let digest = KnotModelWeights::digest_of(&directory).unwrap();
    let error = resolve_embeddings(
        &KnotEmbeddingPreference::bert_cpu(KnotModelWeights::pinned(&directory, &digest)),
        512,
    )
    .unwrap_err();
    // Custody passed, so the next gate is the model itself: either this
    // build never compiled the provider, or esp rejected canned bytes.
    // Neither may quietly become a lexical index.
    assert!(
        matches!(
            error,
            KnotSearchError::ProviderUnavailable { .. } | KnotSearchError::ModelLoad { .. }
        ),
        "{error}"
    );
}

#[test]
fn identity_round_trips_through_serde_and_compares_field_by_field() {
    let lexical = KnotIndexIdentity::lexical(512, SimilarityMetric::Cosine);
    let encoded = serde_json::to_string(&lexical).unwrap();
    assert!(!encoded.contains("weights-digest"), "{encoded}");
    assert_eq!(
        serde_json::from_str::<KnotIndexIdentity>(&encoded).unwrap(),
        lexical
    );

    let bert = KnotIndexIdentity {
        version: KNOT_INDEX_IDENTITY_VERSION,
        provider: KnotEmbeddingProvider::BertCpu,
        model: "all-MiniLM-L6-v2".into(),
        weights_digest: Some("a".repeat(64)),
        dimensions: 384,
        metric: SimilarityMetric::Cosine,
    };
    let encoded = serde_json::to_string(&bert).unwrap();
    assert!(encoded.contains("bert-cpu"), "{encoded}");
    let decoded = serde_json::from_str::<KnotIndexIdentity>(&encoded).unwrap();
    assert_eq!(decoded, bert);
    assert_eq!(decoded.model, bert.model);

    assert!(lexical.answers(&lexical.clone()));
    assert!(!lexical.answers(&bert));
    assert!(!lexical.answers(&KnotIndexIdentity::lexical(256, SimilarityMetric::Cosine)));
    let mut later = lexical.clone();
    later.version += 1;
    assert!(!lexical.answers(&later), "a version bump is a mismatch");
}

#[test]
fn identical_weights_are_the_same_model_under_any_directory_name() {
    let sealed = KnotIndexIdentity {
        version: KNOT_INDEX_IDENTITY_VERSION,
        provider: KnotEmbeddingProvider::BertCpu,
        model: "all-MiniLM-L6-v2".into(),
        weights_digest: Some("a".repeat(64)),
        dimensions: 384,
        metric: SimilarityMetric::Cosine,
    };
    let renamed = KnotIndexIdentity {
        model: "minilm-moved".into(),
        ..sealed.clone()
    };
    assert!(sealed.is_same_model(&renamed) && renamed.is_same_model(&sealed));
    assert!(sealed.answers(&renamed) && renamed.answers(&sealed));
    assert!(renamed.to_string().contains("minilm-moved"), "{renamed}");

    let reweighted = KnotIndexIdentity {
        weights_digest: Some("b".repeat(64)),
        ..sealed.clone()
    };
    assert!(
        !sealed.is_same_model(&reweighted),
        "same name, other weights"
    );
    assert!(!sealed.answers(&reweighted), "same name, other weights");

    // Every field but the label is still identity.
    for differs in [
        KnotIndexIdentity {
            provider: KnotEmbeddingProvider::BertWgpu,
            ..renamed.clone()
        },
        KnotIndexIdentity {
            dimensions: 768,
            ..renamed.clone()
        },
        KnotIndexIdentity {
            metric: SimilarityMetric::DotProduct,
            ..renamed.clone()
        },
        KnotIndexIdentity {
            version: KNOT_INDEX_IDENTITY_VERSION + 1,
            ..renamed.clone()
        },
        KnotIndexIdentity {
            weights_digest: None,
            ..renamed.clone()
        },
    ] {
        assert!(!sealed.is_same_model(&differs), "{differs}");
        assert!(!differs.is_same_model(&sealed), "{differs}");
        assert!(!sealed.answers(&differs), "{differs}");
    }

    // No digest, no stand-in for the name: lexical compares it as before.
    let lexical = KnotIndexIdentity::lexical(384, SimilarityMetric::Cosine);
    let relabelled = KnotIndexIdentity {
        model: "another-hashing".into(),
        ..lexical.clone()
    };
    assert!(!lexical.is_same_model(&relabelled));
    assert!(!lexical.answers(&relabelled));
}

#[test]
fn record_equality_sees_the_label_that_is_same_model_ignores() {
    let sealed = KnotIndexIdentity {
        version: KNOT_INDEX_IDENTITY_VERSION,
        provider: KnotEmbeddingProvider::BertCpu,
        model: "all-MiniLM-L6-v2".into(),
        weights_digest: Some("a".repeat(64)),
        dimensions: 384,
        metric: SimilarityMetric::Cosine,
    };
    let relabelled = KnotIndexIdentity {
        model: "minilm-moved".into(),
        ..sealed.clone()
    };
    assert_ne!(sealed, relabelled, "== is record equality");
    assert!(sealed.is_same_model(&relabelled), "the digest is the model");
    assert_eq!(sealed, sealed.clone());
}

#[test]
fn the_preference_round_trips_as_a_host_would_persist_it() {
    let preference = KnotEmbeddingPreference::bert_wgpu(KnotModelWeights::pinned(
        Path::new("models").join("bge-micro-v2"),
        "b".repeat(64),
    ));
    let encoded = serde_json::to_string(&preference).unwrap();
    assert_eq!(
        serde_json::from_str::<KnotEmbeddingPreference>(&encoded).unwrap(),
        preference
    );
    assert_eq!(
        serde_json::from_str::<KnotEmbeddingPreference>("{}").unwrap(),
        KnotEmbeddingPreference::lexical()
    );
}
