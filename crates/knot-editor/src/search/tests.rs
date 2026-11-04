// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::fs;

use esp::embed::SimilarityMetric;
use servitor::{Grant, GrantTable};
use tempfile::tempdir;

use super::*;
use crate::VaultDocument;
use crate::embedding::tests::canned_model_directory;
use crate::embedding::{KNOT_INDEX_IDENTITY_VERSION, KnotModelWeights};
use crate::vault::SEARCH_INDEX_PATH;

fn subject() -> Subject {
    Subject::new([0x61; 32])
}

fn grant(cap: Cap) -> GrantTable {
    GrantTable::new().with_grant(Grant::new(subject(), cap, Mode::Read))
}

fn both() -> GrantTable {
    GrantTable::new()
        .with_grant(Grant::new(subject(), KnotSearch::disk_cap(), Mode::Read))
        .with_grant(Grant::new(subject(), KnotSearch::vault_cap(), Mode::Read))
}

fn note(id: &str, title: &str, body: &str) -> VaultDocument {
    VaultDocument {
        id: id.into(),
        title: title.into(),
        body: body.as_bytes().to_vec(),
        media_type: "text/vnd.knot".into(),
    }
}

fn orchard_vault(root: &std::path::Path, key: u8) -> KnotVault {
    let mut vault = KnotVault::open(root, [key; 32]).unwrap();
    vault
        .put(note(
            "orchard",
            "Private orchard",
            "orchard pruning observations",
        ))
        .unwrap();
    vault
}

/// An identity as a BERT build would seal it, with no model in hand. The
/// custody path that would produce it is covered in `embedding`; here it
/// only has to be a different vector space from the lexical default.
fn canned_bert_identity(digest: &str) -> KnotIndexIdentity {
    KnotIndexIdentity {
        version: KNOT_INDEX_IDENTITY_VERSION,
        provider: KnotEmbeddingProvider::BertCpu,
        model: "all-MiniLM-L6-v2".into(),
        weights_digest: Some(digest.into()),
        dimensions: 384,
        metric: SimilarityMetric::Cosine,
    }
}

fn lexical_config(dimensions: usize) -> SearchConfig {
    SearchConfig {
        dimensions,
        ..SearchConfig::default()
    }
}

#[test]
fn search_spans_disk_and_vault_but_respects_lane_grants() {
    let temp = tempdir().unwrap();
    let disk_root = temp.path().join("files");
    let vault_root = temp.path().join("vault");
    fs::create_dir(&disk_root).unwrap();
    fs::write(disk_root.join("runtime.md"), "rust async runtime internals").unwrap();
    let directory = DirectorySource::open(&disk_root).unwrap();
    let vault = orchard_vault(&vault_root, 0x62);

    let search =
        KnotSearch::build(Some(&directory), Some(&vault), SearchConfig::default()).unwrap();
    let orchard = search
        .query(Some(&vault), "orchard observations", 2, subject(), &both())
        .unwrap();
    assert_eq!(orchard[0].lane, SearchLane::Vault);

    let disk_only = search
        .query(
            Some(&vault),
            "orchard observations",
            2,
            subject(),
            &grant(KnotSearch::disk_cap()),
        )
        .unwrap();
    assert!(disk_only.iter().all(|hit| hit.lane == SearchLane::Disk));

    let vault_only = search
        .query(
            Some(&vault),
            "rust async",
            2,
            subject(),
            &grant(KnotSearch::vault_cap()),
        )
        .unwrap();
    assert!(vault_only.iter().all(|hit| hit.lane == SearchLane::Vault));

    let nothing = search
        .query(
            Some(&vault),
            "orchard rust",
            2,
            subject(),
            &GrantTable::new(),
        )
        .unwrap();
    assert!(nothing.is_empty(), "no scope grant, no hits: {nothing:?}");
}

#[test]
fn locked_vault_has_no_hits_and_its_derived_index_is_sealed() {
    let temp = tempdir().unwrap();
    let vault_root = temp.path().join("vault");
    let mut vault = KnotVault::open(&vault_root, [0x63; 32]).unwrap();
    vault
        .put(note(
            "private-orchard",
            "Private orchard",
            "confidential quince harvest",
        ))
        .unwrap();
    let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();

    let sealed = fs::read(vault_root.join(SEARCH_INDEX_PATH)).unwrap();
    for plaintext in [
        b"private-orchard".as_slice(),
        b"confidential".as_slice(),
        b"esp-lexical-hashing".as_slice(),
    ] {
        assert!(
            !sealed
                .windows(plaintext.len())
                .any(|window| window == plaintext)
        );
    }

    let authority = grant(KnotSearch::vault_cap());
    assert_eq!(
        search
            .query(Some(&vault), "quince harvest", 1, subject(), &authority,)
            .unwrap()[0]
            .id,
        "private-orchard"
    );
    vault.lock();
    assert!(
        search
            .query(Some(&vault), "quince harvest", 1, subject(), &authority,)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn lexical_is_the_default_and_seals_its_identity() {
    let temp = tempdir().unwrap();
    let vault = orchard_vault(temp.path(), 0x64);
    assert_eq!(
        SearchConfig::default().embedding.provider,
        KnotEmbeddingProvider::Lexical
    );
    let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();
    assert_eq!(
        search.identity(),
        &KnotIndexIdentity::lexical(512, SimilarityMetric::Cosine)
    );
    let sealed = vault.load_search_index().unwrap().unwrap();
    assert_eq!(sealed.identity.as_ref(), Some(search.identity()));
}

#[test]
fn a_vault_index_sealed_before_identity_still_loads_and_queries_as_lexical() {
    let temp = tempdir().unwrap();
    let vault = orchard_vault(temp.path(), 0x65);

    // Seal what the previous Knot sealed: the bare dense index, no identity.
    let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();
    let bare = vault.load_search_index().unwrap().unwrap().index;
    vault.store_search_index_unidentified(&bare).unwrap();
    let legacy = vault.load_search_index().unwrap().unwrap();
    assert!(legacy.identity.is_none());
    assert_eq!(legacy.index.dimensions(), 512);

    let hits = search
        .query(Some(&vault), "orchard pruning", 1, subject(), &both())
        .unwrap();
    assert_eq!(hits[0].id, "orchard");
    assert_eq!(hits[0].lane, SearchLane::Vault);

    // A fresh build over it refreshes in place rather than refusing, and
    // leaves the record identified.
    KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();
    assert!(
        vault
            .load_search_index()
            .unwrap()
            .unwrap()
            .identity
            .is_some()
    );

    // An unidentified record at other dimensions is still held to them.
    vault.store_search_index_unidentified(&bare).unwrap();
    let narrower = KnotSearch::build(None, None, lexical_config(256)).unwrap();
    let error = narrower
        .query(Some(&vault), "orchard", 1, subject(), &both())
        .unwrap_err();
    assert!(error.offers_rebuild(), "{error}");
}

#[test]
fn a_bert_preference_with_an_absent_path_refuses_and_never_builds_lexical() {
    let temp = tempdir().unwrap();
    let vault = orchard_vault(&temp.path().join("vault"), 0x68);
    let absent = temp.path().join("models").join("all-MiniLM-L6-v2");
    let config = SearchConfig {
        embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::at(&absent)),
        ..SearchConfig::default()
    };

    let error = KnotSearch::build(None, Some(&vault), config)
        .err()
        .expect("an absent model directory must refuse");
    assert!(
        matches!(error, KnotSearchError::WeightsUnreadable { .. }),
        "{error}"
    );
    assert!(!error.offers_rebuild());
    assert!(error.to_string().contains("all-MiniLM-L6-v2"), "{error}");
    assert!(
        vault.load_search_index().unwrap().is_none(),
        "nothing may be sealed, least of all a lexical index under a BERT preference"
    );

    let unsupplied = SearchConfig {
        embedding: KnotEmbeddingPreference {
            provider: KnotEmbeddingProvider::BertWgpu,
            weights: None,
        },
        ..SearchConfig::default()
    };
    let error = KnotSearch::build(None, Some(&vault), unsupplied)
        .err()
        .expect("a BERT preference with no path must refuse");
    assert_eq!(
        error,
        KnotSearchError::WeightsNotSupplied {
            provider: KnotEmbeddingProvider::BertWgpu
        }
    );
    assert!(vault.load_search_index().unwrap().is_none());
}

#[test]
fn a_pinned_digest_mismatch_refuses_before_any_index_exists() {
    let temp = tempdir().unwrap();
    let vault = orchard_vault(&temp.path().join("vault"), 0x69);
    let directory = canned_model_directory(&temp.path().join("models"), "writer");
    let config = SearchConfig {
        embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::pinned(
            &directory,
            "f".repeat(64),
        )),
        ..SearchConfig::default()
    };
    let error = KnotSearch::build(None, Some(&vault), config)
        .err()
        .expect("a moved digest must refuse");
    assert!(
        matches!(error, KnotSearchError::WeightsDigestMismatch { .. }),
        "{error}"
    );
    assert!(vault.load_search_index().unwrap().is_none());
}

#[test]
fn a_subject_without_the_vault_grant_sees_neither_hits_nor_a_refusal() {
    let temp = tempdir().unwrap();
    let disk_root = temp.path().join("files");
    fs::create_dir(&disk_root).unwrap();
    fs::write(disk_root.join("orchard.md"), "orchard notes on disk").unwrap();
    let directory = DirectorySource::open(&disk_root).unwrap();
    let vault = orchard_vault(&temp.path().join("vault"), 0x6a);
    let search = KnotSearch::build(Some(&directory), None, SearchConfig::default()).unwrap();
    let vectors: VectorIndex<String> = VectorIndex::new(384, SimilarityMetric::Cosine);
    vault
        .store_search_index(&canned_bert_identity(&"a".repeat(64)), &vectors)
        .unwrap();

    // Access filtering comes before the identity check: without the vault
    // grant, the mismatched index is never opened and never refused.
    let disk_only = search
        .query(
            Some(&vault),
            "orchard",
            2,
            subject(),
            &grant(KnotSearch::disk_cap()),
        )
        .unwrap();
    assert!(!disk_only.is_empty());
    assert!(disk_only.iter().all(|hit| hit.lane == SearchLane::Disk));

    let stranger = Subject::new([0x7f; 32]);
    assert!(
        search
            .query(Some(&vault), "orchard", 2, stranger, &both())
            .unwrap()
            .is_empty(),
        "grants are per subject"
    );
}

#[test]
fn a_built_search_and_its_reports_cross_a_thread_boundary() {
    // The build lane is synchronous; a host that runs it off its own
    // thread has to be able to send the result and the reports back.
    fn sendable<T: Send + 'static>() {}
    sendable::<KnotSearch>();
    sendable::<SearchProgress>();
    sendable::<KnotSearchError>();

    let temp = tempdir().unwrap();
    let disk_root = temp.path().join("files");
    fs::create_dir(&disk_root).unwrap();
    fs::write(disk_root.join("field.md"), "field notes").unwrap();
    let (reports, received) = std::sync::mpsc::channel();
    let built = std::thread::spawn(move || {
        let directory = DirectorySource::open(&disk_root).unwrap();
        KnotSearch::build_reporting(Some(&directory), None, SearchConfig::default(), |report| {
            reports.send(report).unwrap()
        })
    })
    .join()
    .unwrap()
    .unwrap_or_else(|error| panic!("{error}"));
    let last = received.try_iter().last().unwrap();
    assert_eq!((last.visited, last.total), (1, 1));
    assert_eq!(
        built
            .query(None, "field", 1, subject(), &grant(KnotSearch::disk_cap()))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn building_reports_progress_per_lane_including_skipped_documents() {
    let temp = tempdir().unwrap();
    let disk_root = temp.path().join("files");
    fs::create_dir(&disk_root).unwrap();
    fs::write(disk_root.join("small.md"), "small note").unwrap();
    fs::write(disk_root.join("large.md"), "x".repeat(64)).unwrap();
    let directory = DirectorySource::open(&disk_root).unwrap();
    let mut vault = orchard_vault(&temp.path().join("vault"), 0x6b);
    vault
        .put(VaultDocument {
            id: "binary".into(),
            title: "Binary".into(),
            body: vec![0xff, 0xfe, 0xfd],
            media_type: "application/octet-stream".into(),
        })
        .unwrap();

    let mut reports = Vec::new();
    let config = SearchConfig {
        max_file_bytes: 32,
        ..SearchConfig::default()
    };
    KnotSearch::build_reporting(Some(&directory), Some(&vault), config, |report| {
        reports.push(report)
    })
    .unwrap();

    let last = |lane| {
        *reports
            .iter()
            .rev()
            .find(|report| report.lane == lane)
            .unwrap()
    };
    assert_eq!(
        last(SearchLane::Disk),
        SearchProgress {
            lane: SearchLane::Disk,
            visited: 2,
            indexed: 1,
            total: 2,
        }
    );
    assert_eq!(
        last(SearchLane::Vault),
        SearchProgress {
            lane: SearchLane::Vault,
            visited: 2,
            indexed: 1,
            total: 2,
        }
    );
    assert_eq!(
        reports.len(),
        3 + 3,
        "one zero report plus one per document, per lane"
    );
    assert!(
        reports
            .windows(2)
            .filter(|pair| pair[0].lane == pair[1].lane)
            .all(|pair| pair[1].visited == pair[0].visited + 1)
    );
}

#[path = "tests/identity.rs"]
mod identity;
