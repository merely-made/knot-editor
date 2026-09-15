// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Catalog-to-signed-relation receipt through the public application APIs.

use std::collections::BTreeSet;
use std::fs;

use knot_editor::{
    DirectorySource, IgnorePolicy, KnotCorePredicateV1, KnotFileCatalog, KnotPredicateRefV1,
    KnotRelationEndpointV1, KnotRelationPositionV1, KnotSyncEvent, KnotSyncFileStore, KnotVault,
    capture_endpoint_context,
};
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;

#[tokio::test]
async fn catalog_capture_keeps_disk_authority_and_historical_relation_after_reopen() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("notes");
    fs::create_dir(&root).unwrap();
    let path = root.join("essay.djot");
    let original = "α supports β\n";
    fs::write(&path, original).unwrap();
    let catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
    let source = DirectorySource::with_catalog(&root, IgnorePolicy::none(), catalog).unwrap();
    let id = source.documents().next().unwrap().id.clone();
    let captured = source.capture_file_revision(&id, 4096).unwrap();
    assert_eq!(captured.document_id, id);
    assert_eq!(captured.body, original.as_bytes());

    // Preparation is a retained observation, not an instruction to reread later.
    fs::write(&path, "later source\n").unwrap();
    let author = InMemoryProvider::from_seed([0x71; 32]);
    let seed = author.master_keypair().to_seed();
    let writer = author.master_public_key().to_bytes();
    let space = [0x72; 32];
    let vault = KnotVault::open(temp.path().join("vault"), [0x73; 32]).unwrap();
    let database = temp.path().join("relations.redb");
    let store = KnotSyncFileStore::open(&database, space, [writer]).unwrap();
    let capture_op = store
        .author(
            seed,
            &vault,
            &KnotSyncEvent::CaptureFileRevision(captured.clone()),
        )
        .await
        .unwrap();
    let head = *capture_op.hash.as_bytes();
    assert_eq!(
        store.file_revision(&vault, &id, head).await.unwrap(),
        Some(captured.clone())
    );
    assert!(
        store
            .document_version(&vault, &id, head)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .file_revision(&vault, "another-id", head)
            .await
            .unwrap()
            .is_none()
    );
    // Context comes from the captured multibyte source, on char boundaries.
    let endpoint = |start: u64, end: u64, quote: &str| {
        let (prefix, suffix) = capture_endpoint_context(original, start as usize, end as usize);
        KnotRelationEndpointV1 {
            document_id: id.clone(),
            document_head: head,
            quote: quote.into(),
            position: Some(KnotRelationPositionV1 { start, end }),
            prefix,
            suffix,
        }
    };
    store
        .author(
            seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                subject: endpoint(0, 2, "α"),
                object: endpoint(12, 14, "β"),
                evidence: None,
                qualification: Some("captured file reading".into()),
            },
        )
        .await
        .unwrap();
    let newer = source.capture_file_revision(&id, 4096).unwrap();
    assert_ne!(captured.body, newer.body);
    store
        .author(seed, &vault, &KnotSyncEvent::CaptureFileRevision(newer))
        .await
        .unwrap();
    let projection = store.projection(&vault).await.unwrap();
    assert!(projection.documents.is_empty());
    assert!(projection.document_heads.is_empty());
    assert!(projection.conflicts.is_empty());
    assert_eq!(vault.documents().count(), 0);
    assert_eq!(projection.relations.len(), 1);
    assert!(projection.relations_visible_to(&BTreeSet::new()).is_empty());
    assert_eq!(
        projection
            .relations_visible_to(&BTreeSet::from([id.clone()]))
            .len(),
        1
    );
    assert_eq!(projection.relations[0].subject.document_head, head);
    assert_eq!(projection.relations[0].subject.suffix, " supports β\n");
    assert_eq!(projection.relations[0].object.prefix, "α supports ");
    assert_eq!(
        projection.predicate_label(&projection.relations[0]),
        Some("supports")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "later source\n");
    store.save_checkpoint(&vault).await.unwrap();
    drop(store);
    let reopened = KnotSyncFileStore::open(&database, space, [writer]).unwrap();
    assert_eq!(reopened.projection(&vault).await.unwrap(), projection);
    assert_eq!(
        reopened.file_revision(&vault, &id, head).await.unwrap(),
        Some(captured)
    );
}
