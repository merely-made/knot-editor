// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Writer-defined predicates: one minted identity, a mutable label, a
//! retirement that refuses new assertions and keeps the old ones readable.

use knot_editor::{
    KnotCorePredicateV1, KnotPredicateRefV1, KnotPredicateReplacementV1, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotSyncError, KnotSyncEvent, KnotSyncFileStore, KnotVault,
    VaultDocument, capture_endpoint_context, mint_predicate_iri,
};
use personae::{IdentityProvider, InMemoryProvider};
use std::path::Path;
use tempfile::tempdir;

const SPACE: [u8; 32] = [0x31; 32];
const VAULT_KEY: [u8; 32] = [0x32; 32];
const BODY: &str = "# Essay\nα holds throughout.\n";
const QUOTE: &str = "α holds";

fn document(id: &str) -> VaultDocument {
    VaultDocument {
        id: id.to_owned(),
        title: id.to_owned(),
        body: BODY.as_bytes().to_vec(),
        media_type: "text/vnd.knot".to_owned(),
    }
}

fn endpoint(id: &str, head: [u8; 32]) -> KnotRelationEndpointV1 {
    let start = BODY.find(QUOTE).unwrap();
    let end = start + QUOTE.len();
    let (prefix, suffix) = capture_endpoint_context(BODY, start, end);
    KnotRelationEndpointV1 {
        document_id: id.to_owned(),
        document_head: head,
        quote: QUOTE.to_owned(),
        position: Some(KnotRelationPositionV1 {
            start: start as u64,
            end: end as u64,
        }),
        prefix,
        suffix,
    }
}

fn define(label: &str, supersedes: Option<[u8; 32]>) -> KnotSyncEvent {
    KnotSyncEvent::DefinePredicate {
        slug: "corroborates".to_owned(),
        label: label.to_owned(),
        description: Some("a reading that strengthens another".to_owned()),
        subproperty_of: Some("http://purl.org/spar/cito/supports".to_owned()),
        lowers_to: Some("supports".to_owned()),
        supersedes,
        replaced_by: None,
    }
}

fn assert_with(predicate: KnotPredicateRefV1, head: [u8; 32]) -> KnotSyncEvent {
    KnotSyncEvent::AssertRelation {
        predicate,
        subject: endpoint("essay", head),
        object: endpoint("essay", head),
        evidence: None,
        qualification: None,
    }
}

fn open_vault(root: &Path) -> KnotVault {
    KnotVault::open(root.join("vault"), VAULT_KEY).expect("open predicate vault")
}

#[tokio::test]
async fn a_predicate_is_minted_renamed_and_retired_under_one_iri() {
    let temp = tempdir().unwrap();
    let database = temp.path().join("predicates.redb");
    let alice = InMemoryProvider::from_seed([0x33; 32]);
    let bob = InMemoryProvider::from_seed([0x34; 32]);
    let alice_seed = alice.master_keypair().to_seed();
    let bob_seed = bob.master_keypair().to_seed();
    let alice_author = alice.master_public_key().to_bytes();
    let store = KnotSyncFileStore::open(
        &database,
        SPACE,
        [alice_author, bob.master_public_key().to_bytes()],
    )
    .unwrap();
    let vault = open_vault(temp.path());

    let put = store
        .author(alice_seed, &vault, &KnotSyncEvent::Put(document("essay")))
        .await
        .unwrap();
    let head = *put.hash.as_bytes();

    let minted = store
        .author(alice_seed, &vault, &define("corroborates", None))
        .await
        .unwrap();
    let root = *minted.hash.as_bytes();
    let minted_ref = KnotPredicateRefV1::Defined(root);

    // An assertion signed under the original label.
    let early = store
        .author(alice_seed, &vault, &assert_with(minted_ref.clone(), head))
        .await
        .unwrap();

    // Only the minting author renames the predicate.
    assert!(matches!(
        store
            .author(bob_seed, &vault, &define("hijacked", Some(root)))
            .await,
        Err(KnotSyncError::InvalidPredicate(_))
    ));
    // The slug is immutable across a chain; the label is the mutable face.
    assert!(matches!(
        store
            .author(
                alice_seed,
                &vault,
                &KnotSyncEvent::DefinePredicate {
                    slug: "corroborates-strongly".to_owned(),
                    label: "corroborates strongly".to_owned(),
                    description: None,
                    subproperty_of: None,
                    lowers_to: Some("supports".to_owned()),
                    supersedes: Some(root),
                    replaced_by: None,
                },
            )
            .await,
        Err(KnotSyncError::InvalidPredicate(_))
    ));
    // A core slug cannot be redefined.
    assert!(matches!(
        store
            .author(
                alice_seed,
                &vault,
                &KnotSyncEvent::DefinePredicate {
                    slug: "supports".to_owned(),
                    label: "supports".to_owned(),
                    description: None,
                    subproperty_of: None,
                    lowers_to: None,
                    supersedes: None,
                    replaced_by: None,
                },
            )
            .await,
        Err(KnotSyncError::InvalidPredicate(_))
    ));

    let renamed = store
        .author(
            alice_seed,
            &vault,
            &define("corroborates strongly", Some(root)),
        )
        .await
        .unwrap();

    let projection = store.projection(&vault).await.unwrap();
    assert_eq!(projection.predicates.definitions.len(), 2);
    assert!(projection.predicates.rejected.is_empty());
    let expected_iri = mint_predicate_iri(&alice_author, root);
    for definition in &projection.predicates.definitions {
        assert_eq!(definition.iri, expected_iri);
        assert_eq!(definition.root, root);
        assert_eq!(definition.slug, "corroborates");
    }
    let early_relation = projection
        .relations
        .iter()
        .find(|relation| relation.id == *early.hash.as_bytes())
        .unwrap();
    // The display shows the current label on an assertion signed under the old one.
    assert_eq!(
        projection.predicate_label(early_relation),
        Some("corroborates strongly")
    );
    assert_eq!(
        projection.predicate_iri(early_relation),
        Some(&*expected_iri)
    );
    assert_eq!(
        projection.predicates.lowers_to(&early_relation.predicate),
        Some("supports")
    );
    // A later id in the chain resolves to the same identity.
    assert_eq!(
        projection.predicates.root_of(*renamed.hash.as_bytes()),
        Some(root)
    );

    // Retirement: a superseding definition carrying a replacement.
    store
        .author(
            alice_seed,
            &vault,
            &KnotSyncEvent::DefinePredicate {
                slug: "corroborates".to_owned(),
                label: "corroborates strongly".to_owned(),
                description: None,
                subproperty_of: None,
                lowers_to: Some("supports".to_owned()),
                supersedes: Some(*renamed.hash.as_bytes()),
                replaced_by: Some(KnotPredicateReplacementV1::Predicate(
                    KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                )),
            },
        )
        .await
        .unwrap();

    // A retired predicate refuses a new assertion and keeps the earlier one.
    assert!(matches!(
        store
            .author(alice_seed, &vault, &assert_with(minted_ref.clone(), head))
            .await,
        Err(KnotSyncError::InvalidRelation(_))
    ));
    let retired = store.projection(&vault).await.unwrap();
    assert!(retired.predicates.is_retired(root));
    assert_eq!(retired.relations.len(), 1);
    assert_eq!(retired.relations[0].id, *early.hash.as_bytes());

    // The definition and its assertions survive a checkpoint and a reopen.
    let checkpoint = store.save_checkpoint(&vault).await.unwrap();
    let snapshot = checkpoint.snapshot.as_ref().unwrap();
    assert_eq!(snapshot.predicates.len(), 3);
    assert!(snapshot.rejected_predicates.is_empty());
    drop(vault);
    drop(store);

    let reopened = KnotSyncFileStore::open(
        &database,
        SPACE,
        [alice_author, bob.master_public_key().to_bytes()],
    )
    .unwrap();
    let reopened_vault = open_vault(temp.path());
    let replayed = reopened.projection(&reopened_vault).await.unwrap();
    assert_eq!(replayed.predicates, retired.predicates);
    assert_eq!(replayed.relations, retired.relations);
    assert_eq!(
        replayed.predicate_label(&replayed.relations[0]),
        Some("corroborates strongly")
    );
}

#[tokio::test]
async fn an_unknown_bare_label_and_an_unavailable_definition_are_refused_at_authoring() {
    let temp = tempdir().unwrap();
    let alice = InMemoryProvider::from_seed([0x35; 32]);
    let seed = alice.master_keypair().to_seed();
    let store = KnotSyncFileStore::open(
        temp.path().join("labels.redb"),
        SPACE,
        [alice.master_public_key().to_bytes()],
    )
    .unwrap();
    let vault = open_vault(temp.path());
    let put = store
        .author(seed, &vault, &KnotSyncEvent::Put(document("essay")))
        .await
        .unwrap();
    let head = *put.hash.as_bytes();

    let unknown = KnotPredicateRefV1::Unrecognized("corroborates".to_owned());
    assert!(matches!(
        store
            .author(seed, &vault, &assert_with(unknown, head))
            .await,
        Err(KnotSyncError::InvalidRelation(_))
    ));
    assert!(matches!(
        store
            .author(
                seed,
                &vault,
                &assert_with(KnotPredicateRefV1::Defined([0x99; 32]), head),
            )
            .await,
        Err(KnotSyncError::InvalidRelation(_))
    ));
    assert!(store.projection(&vault).await.unwrap().relations.is_empty());
}
