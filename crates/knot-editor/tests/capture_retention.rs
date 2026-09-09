// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Resident-owned capture authorization, retry, and source-authority receipts.

use knot_editor::{
    KnotCaptureGrant, KnotFileRevisionV1, KnotResidentSource, KnotSyncEvent, KnotSyncFileStore,
    KnotVault, VaultDocument,
};
use personae::{IdentityProvider, InMemoryProvider};
use stickleback::DataKeyring;
use tempfile::tempdir;

fn revision() -> KnotFileRevisionV1 {
    KnotFileRevisionV1 {
        document_id: "file:essay".into(),
        title: "Essay".into(),
        media_type: "text/x-djot".into(),
        body: b"exact reviewed source\n".to_vec(),
    }
}

fn grant() -> KnotCaptureGrant {
    KnotCaptureGrant::new([revision().document_id], 4096)
}

fn identity() -> ([u8; 32], [u8; 32]) {
    let author = InMemoryProvider::from_seed([0x91; 32]);
    (
        author.master_keypair().to_seed(),
        author.master_public_key().to_bytes(),
    )
}

#[test]
fn retention_preserves_vault_documents_and_retries_after_reopen() {
    let temp = tempdir().unwrap();
    let vault_path = temp.path().join("vault");
    let database = temp.path().join("sync.redb");
    let (seed, writer) = identity();
    let space = [0x92; 32];
    let key = [0x93; 32];
    let vault = KnotVault::open(&vault_path, key).unwrap();
    let store = KnotSyncFileStore::open(&database, space, [writer]).unwrap();
    let document = VaultDocument {
        id: "authored-note".into(),
        title: "Note".into(),
        media_type: "text/x-djot".into(),
        body: b"editable note".to_vec(),
    };
    pollster::block_on(store.author(seed, &vault, &KnotSyncEvent::Put(document.clone()))).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
    let before = KnotVault::open(&vault_path, key).unwrap();
    let before_revision = before.revision();
    let before_projection = pollster::block_on(store.projection(&before)).unwrap();
    pollster::block_on(store.save_checkpoint(&before)).unwrap();
    let port = resident.capture_retention(grant()).unwrap();
    let prepared = port.prepare(revision()).unwrap();
    assert_eq!(prepared.revision(), &revision());
    assert_eq!(prepared.destination().space_id, space);
    assert_eq!(prepared.destination().writer, writer);
    assert!(
        pollster::block_on(store.tail_receipt())
            .unwrap()
            .operations
            .is_empty()
    );
    assert_eq!(
        pollster::block_on(store.projection(&before)).unwrap(),
        before_projection
    );
    let receipt = port.retain(&prepared).unwrap();
    assert!(!receipt.already_retained);
    assert_eq!(receipt.document_id, revision().document_id);
    assert_eq!(receipt.destination, *prepared.destination());
    assert_eq!(
        pollster::block_on(store.file_revision(&before, &receipt.document_id, receipt.operation))
            .unwrap(),
        Some(revision())
    );
    let retry = port.clone().retain(&prepared).unwrap();
    assert!(retry.already_retained);
    assert_eq!(retry.operation, receipt.operation);
    assert_eq!(
        pollster::block_on(store.tail_receipt()).unwrap().operations,
        vec![receipt.operation]
    );
    let after = KnotVault::open(&vault_path, key).unwrap();
    assert_eq!(after.revision(), before_revision);
    assert_eq!(
        after.documents().cloned().collect::<Vec<_>>(),
        vec![document]
    );
    drop(port);
    drop(resident);
    drop(store);
    let store = KnotSyncFileStore::open(&database, space, [writer]).unwrap();
    let reopened = KnotResidentSource::from_synced_vault(after, store, seed).unwrap();
    let retry = reopened
        .capture_retention(grant())
        .unwrap()
        .retain(&prepared)
        .unwrap();
    assert!(retry.already_retained);
    assert_eq!(retry.operation, receipt.operation);
}

#[test]
fn concurrent_retries_share_one_signed_capture_and_metadata_changes_do_not() {
    let temp = tempdir().unwrap();
    let (seed, writer) = identity();
    let vault = KnotVault::open(temp.path().join("vault"), [3; 32]).unwrap();
    let store = KnotSyncFileStore::open(temp.path().join("sync.redb"), [2; 32], [writer]).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store, seed).unwrap();
    let port = resident.capture_retention(grant()).unwrap();
    let prepared = port.prepare(revision()).unwrap();
    let ports: Vec<_> = (0..4)
        .map(|_| resident.capture_retention(grant()).unwrap())
        .collect();
    let receipts = std::thread::scope(|scope| {
        let tasks: Vec<_> = ports
            .iter()
            .map(|port| scope.spawn(|| port.retain(&prepared).unwrap()))
            .collect();
        tasks
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        receipts
            .iter()
            .filter(|receipt| !receipt.already_retained)
            .count(),
        1
    );
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.operation == receipts[0].operation)
    );
    let mut renamed = revision();
    renamed.title = "New title, same bytes".into();
    let changed = port.retain(&port.prepare(renamed).unwrap()).unwrap();
    assert!(!changed.already_retained);
    assert_ne!(changed.operation, receipts[0].operation);
}

#[test]
fn grants_revocation_lock_and_writer_policy_are_checked_at_retention() {
    let temp = tempdir().unwrap();
    let (seed, writer) = identity();
    let vault = KnotVault::open(temp.path().join("vault"), [3; 32]).unwrap();
    let store = KnotSyncFileStore::open(temp.path().join("sync.redb"), [2; 32], [writer]).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
    let port = resident.capture_retention(grant()).unwrap();
    let prepared = port.prepare(revision()).unwrap();
    let narrow = resident
        .capture_retention(KnotCaptureGrant::new([revision().document_id], 1))
        .unwrap();
    assert!(narrow.prepare(revision()).is_err());
    assert!(narrow.retain(&prepared).is_err());
    let other = resident
        .capture_retention(KnotCaptureGrant::new(["another-file".into()], 4096))
        .unwrap();
    assert!(other.prepare(revision()).is_err());
    assert!(other.retain(&prepared).is_err());
    let mut invalid = revision();
    invalid.document_id.clear();
    assert!(port.prepare(invalid).is_err());
    port.retain(&prepared).unwrap();
    store.deny_writer(&writer);
    assert!(port.retain(&prepared).is_err());
    store.admit_writer(writer);
    let clone = port.clone();
    port.revoke();
    assert!(clone.retain(&prepared).is_err());
    assert!(clone.prepare(revision()).is_err());
    let fresh = resident.capture_retention(grant()).unwrap();
    resident.session(None).lock_vault();
    assert!(fresh.retain(&prepared).is_err());
}

#[test]
fn another_space_and_unsynced_resident_cannot_retain_review() {
    let temp = tempdir().unwrap();
    let (seed, writer) = identity();
    let create = |name: &str, space| {
        let vault = KnotVault::open(temp.path().join(name), [3; 32]).unwrap();
        let store =
            KnotSyncFileStore::open(temp.path().join(format!("{name}.redb")), space, [writer])
                .unwrap();
        KnotResidentSource::from_synced_vault(vault, store, seed).unwrap()
    };
    let first = create("first", [1; 32]);
    let second = create("second", [2; 32]);
    let prepared = first
        .capture_retention(grant())
        .unwrap()
        .prepare(revision())
        .unwrap();
    assert!(
        second
            .capture_retention(grant())
            .unwrap()
            .retain(&prepared)
            .is_err()
    );
    let unsynced = KnotResidentSource::from_vault(
        KnotVault::open(temp.path().join("unsynced"), [3; 32]).unwrap(),
    );
    assert!(unsynced.capture_retention(grant()).is_err());
}

#[test]
fn communal_capture_uses_current_resident_keys_and_respects_vault_lock() {
    let temp = tempdir().unwrap();
    let (seed, writer) = identity();
    let vault = KnotVault::open(temp.path().join("vault"), [3; 32]).unwrap();
    let store =
        KnotSyncFileStore::open_commons(temp.path().join("sync.redb"), [2; 32], [writer]).unwrap();
    let mut keys = DataKeyring::new();
    keys.rotate_random().unwrap();
    let old_keys = DataKeyring::from_bytes(&keys.to_bytes().unwrap()).unwrap();
    let resident = KnotResidentSource::from_communal_vault(
        vault,
        store.clone(),
        seed,
        DataKeyring::from_bytes(&keys.to_bytes().unwrap()).unwrap(),
    )
    .unwrap();
    let port = resident.capture_retention(grant()).unwrap();
    let prepared = port.prepare(revision()).unwrap();
    assert_eq!(
        prepared.destination().encryption,
        knot_editor::KnotEncryptionProfile::CommonsDataV1
    );
    keys.rotate_random().unwrap();
    let mut session = resident.session(None);
    assert!(session.replace_communal_keys(DataKeyring::new()).unwrap());
    assert!(port.destination().is_err());
    assert!(port.prepare(revision()).is_err());
    assert!(port.retain(&prepared).is_err());
    assert!(
        session
            .replace_communal_keys(DataKeyring::from_bytes(&keys.to_bytes().unwrap()).unwrap())
            .unwrap()
    );
    let receipt = port.retain(&prepared).unwrap();
    assert_eq!(
        pollster::block_on(store.file_revision_with_cipher(
            knot_editor::KnotSyncCipher::CommonsData(&keys),
            &receipt.document_id,
            receipt.operation,
        ))
        .unwrap(),
        Some(revision())
    );
    assert!(
        pollster::block_on(store.file_revision_with_cipher(
            knot_editor::KnotSyncCipher::CommonsData(&old_keys),
            &receipt.document_id,
            receipt.operation,
        ))
        .is_err()
    );
    assert!(port.retain(&prepared).unwrap().already_retained);
    assert!(session.lock_vault());
    assert!(port.retain(&prepared).is_err());
}
