// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use knot_editor::{
    KnotAssertedTime, KnotCorePredicateV1, KnotPredicateRefV1, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotSyncEvent, KnotSyncFileStore, KnotVault, VaultDocument,
    capture_endpoint_context,
};
use personae::{IdentityProvider, InMemoryProvider};

#[test]
fn opened_link_swatch_reads_checkpointed_signed_knot_history() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/field_notes.djot");
    let author = InMemoryProvider::from_seed([0xa1; 32]);
    let seed = author.master_keypair().to_seed();
    let writer = author.master_public_key().to_bytes();
    let database = root.path().join("knot-relations.redb");
    let vault_path = root.path().join("relation-vault");
    let vault = KnotVault::open(&vault_path, [0xa2; 32]).unwrap();
    let store = KnotSyncFileStore::open(&database, [0xa3; 32], [writer]).unwrap();
    let essay_text = "# Signed essay\nA passage supports the source.\n";
    let source_text = "# Signed source\nA retained source passage.\n";
    let essay_put = pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::Put(VaultDocument {
            id: "essay".into(),
            title: "Signed essay".into(),
            body: essay_text.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }),
        KnotAssertedTime::At(1_600_000_000_000),
    ))
    .unwrap();
    let source_put = pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::Put(VaultDocument {
            id: "source".into(),
            title: "Signed source".into(),
            body: source_text.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }),
        KnotAssertedTime::At(1_600_000_000_100),
    ))
    .unwrap();
    let endpoint = |id: &str, head: [u8; 32], text: &str, quote: &str| {
        let start = text.find(quote).unwrap();
        let end = start + quote.len();
        let (prefix, suffix) = capture_endpoint_context(text, start, end);
        KnotRelationEndpointV1 {
            document_id: id.into(),
            document_head: head,
            quote: quote.into(),
            position: Some(KnotRelationPositionV1 {
                start: start as u64,
                end: end as u64,
            }),
            prefix,
            suffix,
        }
    };
    let essay = endpoint("essay", *essay_put.hash.as_bytes(), essay_text, "A passage");
    let source = endpoint(
        "source",
        *source_put.hash.as_bytes(),
        source_text,
        "retained source passage",
    );
    pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
            subject: essay.clone(),
            object: source.clone(),
            evidence: Some("urn:knot:test:evidence:signed".into()),
            qualification: Some("The highlighted passage supports this reading.".into()),
        },
        KnotAssertedTime::At(1_650_000_000_000),
    ))
    .unwrap();
    let withdrawn = pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Questions),
            subject: essay.clone(),
            object: source.clone(),
            evidence: None,
            qualification: Some("A second, later withdrawn reading.".into()),
        },
        KnotAssertedTime::At(1_660_000_000_000),
    ))
    .unwrap();
    pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::RetractRelation {
            assertion: *withdrawn.hash.as_bytes(),
        },
        KnotAssertedTime::At(1_670_000_000_000),
    ))
    .unwrap();
    pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Cites),
            subject: source,
            object: essay,
            evidence: None,
            qualification: Some("Reverse direction and unknown first time.".into()),
        },
        KnotAssertedTime::None,
    ))
    .unwrap();
    pollster::block_on(store.save_checkpoint(&vault)).unwrap();
    drop(store);
    drop(vault);

    let vault = KnotVault::open(&vault_path, [0xa2; 32]).unwrap();
    let reopened = KnotSyncFileStore::open(&database, [0xa3; 32], [writer]).unwrap();
    let projection = pollster::block_on(reopened.projection(&vault)).unwrap();
    assert_eq!(projection.relations.len(), 3);
    let with_retraction = projection
        .relations
        .iter()
        .find(|relation| !relation.retractions.is_empty())
        .unwrap();
    assert_eq!(with_retraction.asserted_at_ms, Some(1_660_000_000_000));
    assert_eq!(
        with_retraction.retractions[0].withdrawn_at_ms,
        Some(1_670_000_000_000)
    );
    let view = knot_desktop::knot_relations::KnotRelationView::from_projection(
        &projection,
        reopened.space_id(),
    )
    .unwrap();
    assert_eq!(view.held.links.len(), 1);
    assert_eq!(
        view.held.links[0]
            .buckets
            .iter()
            .map(|bucket| bucket.assertions.len())
            .sum::<usize>(),
        2
    );
    assert_eq!(
        view.history.links[0]
            .buckets
            .iter()
            .map(|bucket| bucket.assertions.len())
            .sum::<usize>(),
        3
    );
    let opened_link = &view.history.links[0];
    assert_eq!(
        opened_link.key.stratum,
        scenograph::link::LinkStratum::Surface
    );
    assert_eq!(
        opened_link.buckets.len(),
        2,
        "opposite directions stay distinct"
    );
    let assertions = opened_link
        .buckets
        .iter()
        .flat_map(|bucket| bucket.assertions.iter())
        .collect::<Vec<_>>();
    assert_eq!(
        assertions
            .iter()
            .map(|item| item.id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    assert!(
        assertions
            .iter()
            .all(|item| item.id.starts_with("urn:knot:assertion:"))
    );
    let signed_support = assertions
        .iter()
        .find(|item| {
            item.basis.note.as_deref() == Some("The highlighted passage supports this reading.")
        })
        .unwrap();
    assert_eq!(signed_support.first_asserted_ms, Some(1_650_000_000_000));
    assert_eq!(signed_support.asserted_at_ms, Some(1_650_000_000_000));
    assert_eq!(
        signed_support.basis.evidence,
        vec!["urn:knot:test:evidence:signed".to_owned()]
    );
    assert!(
        signed_support
            .asserter
            .as_deref()
            .unwrap()
            .starts_with("urn:knot:author:")
    );
    let held_undated = assertions
        .iter()
        .find(|item| {
            item.basis.note.as_deref() == Some("Reverse direction and unknown first time.")
        })
        .unwrap();
    assert_eq!(held_undated.first_asserted_ms, None);
    assert_eq!(held_undated.asserted_at_ms, None);
    assert!(held_undated.withdrawn.is_none());
    let withdrawn_view = assertions
        .iter()
        .find(|item| item.basis.note.as_deref() == Some("A second, later withdrawn reading."))
        .unwrap();
    assert_eq!(withdrawn_view.first_asserted_ms, Some(1_660_000_000_000));
    assert_eq!(
        withdrawn_view
            .withdrawn
            .as_ref()
            .and_then(|item| item.at_ms),
        Some(1_670_000_000_000)
    );
    assert!(
        withdrawn_view
            .withdrawn
            .as_ref()
            .and_then(|item| item.by.as_deref())
            .unwrap()
            .starts_with("urn:knot:author:")
    );
    let scenograph::link::LinkSource::Available { binding, revision } = &view.history.source else {
        panic!("the signed personal source binding is retained")
    };
    assert_eq!(binding.authority, "knot");
    assert_eq!(binding.domain, "personal");
    assert!(binding.resource.starts_with("urn:knot:space:"));
    assert!(matches!(
        revision,
        scenograph::link::LinkSourceRevision::Unavailable { .. }
    ));
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture),
    );
    state.set_knot_relation_view(Some(view));
    let receipt = run_without_native_capture_at_size(
        state,
        "opened_link_knot.scn",
        &root,
        1100.0,
        900.0,
        1.0,
        false,
    );
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
}
