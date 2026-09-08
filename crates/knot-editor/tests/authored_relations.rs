// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! G1 receipt for attributable, vault-native relation assertions.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use knot_editor::{
    KnotAutomaticTextMerge, KnotDocumentConflict, KnotProjectionCheckpoint, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotSyncEvent, KnotSyncFileStore, KnotVault, VaultDocument,
};
use personae::{IdentityProvider, InMemoryProvider};
use serde::Serialize;
use tempfile::tempdir;

const SPACE: [u8; 32] = [0x51; 32];
const VAULT_KEY: [u8; 32] = [0x52; 32];

fn document(id: &str, body: &str) -> VaultDocument {
    VaultDocument {
        id: id.to_owned(),
        title: id.to_owned(),
        body: body.as_bytes().to_vec(),
        media_type: "text/vnd.knot".to_owned(),
    }
}

fn endpoint(id: &str, head: [u8; 32], quote: &str) -> KnotRelationEndpointV1 {
    KnotRelationEndpointV1 {
        document_id: id.to_owned(),
        document_head: head,
        quote: quote.to_owned(),
        position: Some(KnotRelationPositionV1 {
            start: 0,
            end: quote.len() as u64,
        }),
    }
}

fn open_vault(root: &Path) -> KnotVault {
    KnotVault::open(root.join("vault"), VAULT_KEY).expect("open relation vault")
}

#[derive(Serialize)]
struct LegacyCheckpoint {
    version: u16,
    space_id: [u8; 32],
    heads: Vec<LegacyAuthorHead>,
    document_digests: Vec<(String, [u8; 32])>,
    conflict_ids: Vec<String>,
    pending: Vec<([u8; 32], Vec<[u8; 32]>)>,
    snapshot: Option<LegacyCheckpointSnapshot>,
}

#[derive(Serialize)]
struct LegacyAuthorHead {
    author: [u8; 32],
    log_id: u64,
    seq_num: u32,
    operation: [u8; 32],
}

#[derive(Serialize)]
struct LegacyCheckpointSnapshot {
    documents: Vec<VaultDocument>,
    conflicts: Vec<KnotDocumentConflict>,
    automatic_merges: Vec<KnotAutomaticTextMerge>,
    document_heads: BTreeMap<String, [u8; 32]>,
}

#[tokio::test]
async fn authored_relations_retain_authorship_retraction_replay_and_visibility() {
    let temp = tempdir().unwrap();
    let database = temp.path().join("relations.redb");
    let alice = InMemoryProvider::from_seed([0x61; 32]);
    let bob = InMemoryProvider::from_seed([0x62; 32]);
    let alice_seed = alice.master_keypair().to_seed();
    let bob_seed = bob.master_keypair().to_seed();
    let alice_author = alice.master_public_key().to_bytes();
    let bob_author = bob.master_public_key().to_bytes();
    let store = KnotSyncFileStore::open(&database, SPACE, [alice_author, bob_author]).unwrap();
    let vault = open_vault(temp.path());

    let subject = document("essay", "# Essay\n");
    let object = document("source", "# Source\n");
    let subject_operation = store
        .author(alice_seed, &vault, &KnotSyncEvent::Put(subject.clone()))
        .await
        .unwrap();
    let object_operation = store
        .author(alice_seed, &vault, &KnotSyncEvent::Put(object.clone()))
        .await
        .unwrap();
    let subject_endpoint = endpoint(&subject.id, *subject_operation.hash.as_bytes(), "# Essay\n");
    let object_endpoint = endpoint(&object.id, *object_operation.hash.as_bytes(), "# Source\n");

    let alice_assertion_operation = store
        .author(
            alice_seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: "supports".to_owned(),
                subject: subject_endpoint.clone(),
                object: object_endpoint.clone(),
                evidence: Some("Ada's reading".to_owned()),
                qualification: Some("quoted passage".to_owned()),
            },
        )
        .await
        .unwrap();
    let bob_assertion_operation = store
        .author(
            bob_seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: "contradicts".to_owned(),
                subject: subject_endpoint.clone(),
                object: object_endpoint.clone(),
                evidence: Some("Bo's reading".to_owned()),
                qualification: None,
            },
        )
        .await
        .unwrap();

    let before_retraction = store.projection(&vault).await.unwrap();
    assert_eq!(
        before_retraction.documents,
        vec![subject.clone(), object.clone()]
    );
    assert_eq!(before_retraction.relations.len(), 2);
    assert_ne!(
        alice_assertion_operation.hash, bob_assertion_operation.hash,
        "separate signed operations retain separate assertion identities"
    );
    let alice_relation = before_retraction
        .relations
        .iter()
        .find(|relation| relation.author == alice_author)
        .expect("Alice relation");
    let bob_relation = before_retraction
        .relations
        .iter()
        .find(|relation| relation.author == bob_author)
        .expect("Bo relation");
    assert_eq!(alice_relation.predicate, "supports");
    assert_eq!(bob_relation.predicate, "contradicts");
    assert_eq!(alice_relation.subject, subject_endpoint);
    assert_eq!(alice_relation.object, object_endpoint);
    assert_eq!(alice_relation.subject.document_id, "essay");
    assert_eq!(alice_relation.subject.quote, "# Essay\n");
    assert_eq!(alice_relation.subject.position.as_ref().unwrap().start, 0);
    assert_eq!(alice_relation.subject.position.as_ref().unwrap().end, 8);
    assert_eq!(alice_relation.object.document_id, "source");
    assert_eq!(alice_relation.object.quote, "# Source\n");
    assert_eq!(alice_relation.object.position.as_ref().unwrap().start, 0);
    assert_eq!(alice_relation.object.position.as_ref().unwrap().end, 9);
    assert_eq!(alice_relation.scope, SPACE);
    assert_ne!(alice_relation.author, bob_relation.author);
    assert_ne!(alice_relation.id, bob_relation.id);

    let document_heads_before = before_retraction.document_heads.clone();
    assert_eq!(
        document_heads_before,
        BTreeMap::from([
            (subject.id.clone(), *subject_operation.hash.as_bytes()),
            (object.id.clone(), *object_operation.hash.as_bytes()),
        ])
    );

    let only_essay = BTreeSet::from([subject.id.clone()]);
    assert!(
        before_retraction
            .relations_visible_to(&only_essay)
            .is_empty(),
        "a private source endpoint must not leak through a backlink"
    );
    let both_documents = BTreeSet::from([subject.id.clone(), object.id.clone()]);
    assert_eq!(
        before_retraction
            .relations_visible_to(&both_documents)
            .len(),
        2
    );

    assert!(
        store
            .author(
                bob_seed,
                &vault,
                &KnotSyncEvent::RetractRelation {
                    assertion: *alice_assertion_operation.hash.as_bytes(),
                },
            )
            .await
            .is_err(),
        "an author cannot retract another author's assertion"
    );
    store
        .author(
            alice_seed,
            &vault,
            &KnotSyncEvent::RetractRelation {
                assertion: *alice_assertion_operation.hash.as_bytes(),
            },
        )
        .await
        .unwrap();
    let after_retraction = store.projection(&vault).await.unwrap();
    assert_eq!(after_retraction.document_heads, document_heads_before);
    assert_eq!(after_retraction.relations.len(), 2);
    assert_eq!(
        after_retraction
            .relations
            .iter()
            .find(|relation| relation.id == *alice_assertion_operation.hash.as_bytes())
            .unwrap()
            .retractions
            .len(),
        1
    );
    assert!(
        after_retraction
            .relations
            .iter()
            .find(|relation| relation.id == *bob_assertion_operation.hash.as_bytes())
            .unwrap()
            .retractions
            .is_empty()
    );
    assert_eq!(
        after_retraction
            .relation_history_visible_to(&both_documents)
            .len(),
        2,
        "admitted history retains the retracted assertion"
    );
    assert_eq!(
        after_retraction.relations_visible_to(&both_documents).len(),
        1,
        "active visibility filters the retracted assertion"
    );

    let checkpoint = store.save_checkpoint(&vault).await.unwrap();
    assert_eq!(checkpoint.snapshot.as_ref().unwrap().relations.len(), 2);
    assert_eq!(
        checkpoint.snapshot.as_ref().unwrap().document_heads,
        document_heads_before
    );
    drop(vault);
    drop(store);

    let reopened = KnotSyncFileStore::open(&database, SPACE, [alice_author, bob_author]).unwrap();
    let reopened_vault = open_vault(temp.path());
    let replayed = reopened.projection(&reopened_vault).await.unwrap();
    assert_eq!(replayed.relations, after_retraction.relations);
    assert_eq!(replayed.document_heads, document_heads_before);
    assert_eq!(
        replayed
            .relations_visible_to(&both_documents)
            .iter()
            .filter(|relation| relation.retractions.is_empty())
            .count(),
        1
    );
    assert_eq!(
        reopened
            .load_checkpoint()
            .await
            .unwrap()
            .unwrap()
            .snapshot
            .unwrap()
            .relations,
        after_retraction.relations
    );
}

#[test]
fn legacy_checkpoint_without_relation_fields_decodes_with_empty_relation_history() {
    let legacy = LegacyCheckpoint {
        version: 1,
        space_id: SPACE,
        heads: vec![LegacyAuthorHead {
            author: [0x61; 32],
            log_id: 7,
            seq_num: 3,
            operation: [0x63; 32],
        }],
        document_digests: Vec::new(),
        conflict_ids: Vec::new(),
        pending: Vec::new(),
        snapshot: Some(LegacyCheckpointSnapshot {
            documents: Vec::new(),
            conflicts: Vec::new(),
            automatic_merges: Vec::new(),
            document_heads: BTreeMap::new(),
        }),
    };
    let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
    let checkpoint: KnotProjectionCheckpoint = serde_json::from_slice(&legacy_bytes).unwrap();
    let current_bytes = serde_json::to_vec(&checkpoint).unwrap();
    assert_eq!(current_bytes, legacy_bytes);
    let encoded = serde_json::to_value(&checkpoint).unwrap();
    let encoded_snapshot = encoded
        .get("snapshot")
        .and_then(serde_json::Value::as_object)
        .unwrap();
    assert!(!encoded_snapshot.contains_key("relations"));
    assert!(!encoded_snapshot.contains_key("rejected_relations"));
    assert!(!encoded_snapshot.contains_key("unverified_relations"));
    let snapshot = checkpoint.snapshot.unwrap();
    assert!(snapshot.relations.is_empty());
    assert!(snapshot.rejected_relations.is_empty());
    assert!(snapshot.unverified_relations.is_empty());
}
