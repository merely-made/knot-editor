// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Provider graduation as a host sees it across restarts: the sealed vault
//! index keeps the identity that built it, a changed preference is refused
//! with a rebuild offered, and the rebuild is an explicit command.

use std::fs;
use std::path::Path;

use knot_editor::{
    KNOT_BERT_ARTIFACTS, KnotEmbeddingPreference, KnotEmbeddingProvider, KnotIndexIdentity,
    KnotModelWeights, KnotSearch, KnotSearchError, KnotVault, SearchConfig, SearchLane,
    SearchProgress, VaultDocument,
};
use servitor::{Grant, GrantTable, Mode, Subject};
use tempfile::tempdir;

const VAULT_KEY: [u8; 32] = [0x41; 32];

fn subject() -> Subject {
    Subject::new([0x42; 32])
}

fn vault_reader() -> GrantTable {
    GrantTable::new().with_grant(Grant::new(subject(), KnotSearch::vault_cap(), Mode::Read))
}

fn field_notes(root: &Path) -> KnotVault {
    let mut vault = KnotVault::open(root, VAULT_KEY).unwrap();
    for (id, body) in [
        ("heron", "grey heron standing in the shallows at dawn"),
        ("quince", "quince blossom opened late after the frost"),
    ] {
        vault
            .put(VaultDocument {
                id: id.into(),
                title: id.into(),
                body: body.as_bytes().to_vec(),
                media_type: "text/vnd.knot".into(),
            })
            .unwrap();
    }
    vault
}

#[test]
fn a_changed_preference_is_refused_across_restart_until_the_writer_rebuilds() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("vault");

    // First run: the default preference seals a lexical index.
    {
        let vault = field_notes(&root);
        let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();
        assert_eq!(search.identity().provider, KnotEmbeddingProvider::Lexical);
    }

    // Restart under the same preference: the sealed identity answers.
    let vault = KnotVault::open(&root, VAULT_KEY).unwrap();
    let sealed = vault.load_search_index().unwrap().unwrap();
    let recorded = sealed
        .identity
        .expect("the first run recorded its identity");
    let search = KnotSearch::build(None, None, SearchConfig::default()).unwrap();
    assert_eq!(&recorded, search.identity());
    let hits = search
        .query(
            Some(&vault),
            "heron shallows",
            1,
            subject(),
            &vault_reader(),
        )
        .unwrap();
    assert_eq!(
        (hits[0].id.as_str(), hits[0].lane),
        ("heron", SearchLane::Vault)
    );

    // Restart under a different lexical shape stands in for any provider
    // change: both build and query refuse, name both identities, and leave
    // the sealed record as it was.
    let changed = SearchConfig {
        dimensions: 256,
        ..SearchConfig::default()
    };
    let refusal = KnotSearch::build(None, Some(&vault), changed.clone())
        .err()
        .expect("a build must not replace an index sealed under another identity");
    assert!(refusal.offers_rebuild(), "{refusal}");
    let message = refusal.to_string();
    assert!(
        message.contains("512d") && message.contains("256d"),
        "{message}"
    );

    let changed_search = KnotSearch::build(None, None, changed).unwrap();
    let refusal = changed_search
        .query(Some(&vault), "heron", 1, subject(), &vault_reader())
        .unwrap_err();
    assert!(matches!(refusal, KnotSearchError::IdentityMismatch { .. }));
    assert_eq!(refusal.rebuild_identity(), Some(changed_search.identity()));
    assert_eq!(
        vault.load_search_index().unwrap().unwrap().identity,
        Some(recorded)
    );

    // The writer accepts the offer.
    let mut reports: Vec<SearchProgress> = Vec::new();
    changed_search
        .rebuild_vault_index(&vault, |report| reports.push(report))
        .unwrap();
    let last = reports.last().unwrap();
    assert_eq!((last.visited, last.indexed, last.total), (2, 2, 2));
    drop(vault);

    // And the next restart finds the new identity sealed.
    let vault = KnotVault::open(&root, VAULT_KEY).unwrap();
    assert_eq!(
        vault
            .load_search_index()
            .unwrap()
            .unwrap()
            .identity
            .as_ref(),
        Some(changed_search.identity())
    );
    let hits = changed_search
        .query(Some(&vault), "quince frost", 1, subject(), &vault_reader())
        .unwrap();
    assert_eq!(hits[0].id, "quince");
}

#[test]
fn bert_is_reachable_only_through_verified_writer_custody() {
    let temp = tempdir().unwrap();
    let vault = field_notes(&temp.path().join("vault"));
    let models = temp.path().join("models");

    // No directory: refused, nothing sealed.
    let absent = SearchConfig {
        embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::at(
            models.join("all-MiniLM-L6-v2"),
        )),
        ..SearchConfig::default()
    };
    let refusal = KnotSearch::build(None, Some(&vault), absent)
        .err()
        .expect("absent weights refuse");
    assert!(matches!(refusal, KnotSearchError::WeightsUnreadable { .. }));
    assert!(vault.load_search_index().unwrap().is_none());

    // A directory with the right shape. The writer learns its digest, pins
    // it, and custody passes; a wrong pin does not.
    let directory = models.join("all-MiniLM-L6-v2");
    fs::create_dir_all(&directory).unwrap();
    for artifact in KNOT_BERT_ARTIFACTS {
        fs::write(directory.join(artifact), format!("canned {artifact}")).unwrap();
    }
    let digest = KnotModelWeights::digest_of(&directory).unwrap();

    let wrong_pin = SearchConfig {
        embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::pinned(
            &directory,
            "0".repeat(64),
        )),
        ..SearchConfig::default()
    };
    let refusal = KnotSearch::build(None, Some(&vault), wrong_pin)
        .err()
        .expect("a wrong pin refuses");
    assert!(
        matches!(refusal, KnotSearchError::WeightsDigestMismatch { ref found, .. } if *found == digest),
        "{refusal}"
    );

    let right_pin = SearchConfig {
        embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::pinned(&directory, &digest)),
        ..SearchConfig::default()
    };
    let refusal = KnotSearch::build(None, Some(&vault), right_pin)
        .err()
        .expect("canned bytes are not a model, and nothing may substitute for one");
    assert!(
        matches!(
            refusal,
            KnotSearchError::ProviderUnavailable { .. } | KnotSearchError::ModelLoad { .. }
        ),
        "{refusal}"
    );
    assert!(
        vault.load_search_index().unwrap().is_none(),
        "no lexical index under a BERT preference"
    );

    // Identity a BERT build would record, sealed by hand: a lexical search
    // refuses it by name rather than scoring 384-dimension vectors.
    let bert = KnotIndexIdentity {
        provider: KnotEmbeddingProvider::BertCpu,
        model: "all-MiniLM-L6-v2".into(),
        weights_digest: Some(digest),
        dimensions: 384,
        ..KnotIndexIdentity::lexical(384, esp::embed::SimilarityMetric::Cosine)
    };
    vault
        .store_search_index(&bert, &esp::embed::VectorIndex::new(384, bert.metric))
        .unwrap();
    let lexical = KnotSearch::build(None, None, SearchConfig::default()).unwrap();
    let refusal = lexical
        .query(Some(&vault), "heron", 1, subject(), &vault_reader())
        .unwrap_err();
    let message = refusal.to_string();
    assert!(
        message.contains("bert-cpu") && message.contains("lexical"),
        "{message}"
    );
    assert!(refusal.offers_rebuild());
}

/// A real BERT CPU index over writer-supplied weights. Ignored by default: it
/// needs a model directory, and Knot never fetches one. Run with
/// `KNOT_BERT_MODEL_DIR=<dir> cargo test -p knot-editor --features embed-bert
/// --test search_provider_identity -- --ignored --nocapture`.
#[cfg(feature = "embed-bert")]
#[test]
#[ignore = "needs writer-supplied weights named by KNOT_BERT_MODEL_DIR"]
fn a_real_bert_cpu_index_builds_queries_and_is_refused_by_the_lexical_default() {
    let model =
        std::path::PathBuf::from(std::env::var_os("KNOT_BERT_MODEL_DIR").unwrap_or_else(|| {
            panic!("set KNOT_BERT_MODEL_DIR to a directory holding {KNOT_BERT_ARTIFACTS:?}")
        }));
    let temp = tempdir().unwrap();
    let root = temp.path().join("vault");
    let digest = KnotModelWeights::digest_of(&model).unwrap();

    let vault = field_notes(&root);
    let mut reports: Vec<SearchProgress> = Vec::new();
    let started = std::time::Instant::now();
    let search = KnotSearch::build_reporting(
        None,
        Some(&vault),
        SearchConfig {
            embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::at(&model)),
            ..SearchConfig::default()
        },
        |report| reports.push(report),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let identity = search.identity().clone();
    eprintln!("built in {:?}: {identity}", started.elapsed());
    for report in &reports {
        eprintln!(
            "  {:?} {}/{} (indexed {})",
            report.lane, report.visited, report.total, report.indexed
        );
    }
    assert_eq!(identity.provider, KnotEmbeddingProvider::BertCpu);
    assert_eq!(
        Some(identity.model.as_str()),
        model.file_name().and_then(|name| name.to_str())
    );
    assert_eq!(identity.weights_digest.as_deref(), Some(digest.as_str()));
    assert_eq!(identity.metric, esp::embed::SimilarityMetric::Cosine);
    assert_eq!(
        vault
            .load_search_index()
            .unwrap()
            .unwrap()
            .identity
            .as_ref(),
        Some(&identity)
    );

    // Neither query shares a word with either note, so lexical similarity
    // has nothing to go on; and "quince" sorts after "heron", so a tie
    // broken by id cannot produce the first answer.
    for (query, expected) in [
        ("fruit tree flowers following a cold night", "quince"),
        ("wading bird near water", "heron"),
    ] {
        let hits = search
            .query(Some(&vault), query, 2, subject(), &vault_reader())
            .unwrap();
        eprintln!("  {query:?} -> {hits:?}");
        assert_eq!(hits[0].id, expected, "{query:?}");
    }

    let lexical = KnotSearch::build(None, None, SearchConfig::default()).unwrap();
    let refusal = lexical
        .query(Some(&vault), "heron", 1, subject(), &vault_reader())
        .unwrap_err();
    eprintln!("  lexical query refused: {refusal}");
    assert!(refusal.offers_rebuild());
    drop((search, vault));

    // Restart pinned to the digest the first build recorded.
    let vault = KnotVault::open(&root, VAULT_KEY).unwrap();
    let reopened = KnotSearch::build(
        None,
        None,
        SearchConfig {
            embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::pinned(&model, &digest)),
            ..SearchConfig::default()
        },
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let hits = reopened
        .query(
            Some(&vault),
            "fruit tree flowers following a cold night",
            1,
            subject(),
            &vault_reader(),
        )
        .unwrap();
    assert_eq!(hits[0].id, "quince");

    // The writer renames the directory. The same bytes under another name are
    // the same model, so the sealed index answers with no rebuild. Hard links,
    // so the weights are not duplicated unless temp is on another volume.
    let renamed = temp.path().join("minilm-renamed");
    fs::create_dir_all(&renamed).unwrap();
    for artifact in KNOT_BERT_ARTIFACTS {
        let (from, to) = (model.join(artifact), renamed.join(artifact));
        if let Err(error) = fs::hard_link(&from, &to) {
            eprintln!("  could not link {artifact} ({error}); copying");
            fs::copy(&from, &to).unwrap();
        }
    }
    let moved = KnotSearch::build(
        None,
        None,
        SearchConfig {
            embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::at(&renamed)),
            ..SearchConfig::default()
        },
    )
    .unwrap_or_else(|error| panic!("{error}"));
    eprintln!("  renamed: {}", moved.identity());
    assert_eq!(moved.identity().model, "minilm-renamed");
    assert!(
        moved.identity().is_same_model(&identity),
        "{} vs {identity}",
        moved.identity()
    );
    let hits = moved
        .query(
            Some(&vault),
            "wading bird near water",
            1,
            subject(),
            &vault_reader(),
        )
        .unwrap_or_else(|error| panic!("a rename must not force a rebuild: {error}"));
    assert_eq!(hits[0].id, "heron");
    assert_eq!(
        vault
            .load_search_index()
            .unwrap()
            .unwrap()
            .identity
            .map(|sealed| sealed.model),
        Some(identity.model.clone()),
        "answering leaves the sealed record, label included, as it was"
    );
}
