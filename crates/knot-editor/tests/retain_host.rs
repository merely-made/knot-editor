// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Host-bound desktop retention uses a resident-issued capability.

use knot_capture::{KnotPersonaDisplayV1, KnotRetainPort};
use knot_editor::{
    KnotCaptureGrant, KnotFileRevisionV1, KnotResidentRetainPort, KnotResidentSource,
    KnotSyncFileStore, KnotVault,
};
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;

fn revision() -> KnotFileRevisionV1 {
    KnotFileRevisionV1 {
        document_id: "file:host-bound-essay".into(),
        title: "Host-bound essay".into(),
        media_type: "text/x-djot".into(),
        body: b"the exact reviewed source\n".to_vec(),
    }
}

fn persona() -> KnotPersonaDisplayV1 {
    KnotPersonaDisplayV1 {
        stable_id: "persona:writer".into(),
        label: "Writer".into(),
    }
}

fn resident() -> (
    tempfile::TempDir,
    KnotResidentSource,
    KnotSyncFileStore,
    KnotVault,
    [u8; 32],
    [u8; 32],
) {
    let root = tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([0x51; 32]);
    let seed = identity.master_keypair().to_seed();
    let writer = identity.master_public_key().to_bytes();
    let vault = KnotVault::open(root.path().join("vault"), [0x52; 32]).unwrap();
    let store =
        KnotSyncFileStore::open(root.path().join("sync.redb"), [0x53; 32], [writer]).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
    let read_vault = KnotVault::open(root.path().join("vault"), [0x52; 32]).unwrap();
    pollster::block_on(store.save_checkpoint(&read_vault)).unwrap();
    (root, resident, store, read_vault, writer, seed)
}

fn adapter(resident: &KnotResidentSource) -> KnotResidentRetainPort {
    let port = resident
        .capture_retention(KnotCaptureGrant::new([revision().document_id], 4096))
        .unwrap();
    KnotResidentRetainPort::new(persona(), port).unwrap()
}

#[test]
fn host_adapter_retains_exact_reviewed_bytes_and_reuses_the_signed_operation() {
    let (root, resident, store, read_vault, writer, seed) = resident();
    let port = adapter(&resident);
    let target = port.target().clone();

    let first = port.retain_reviewed(&target, revision()).unwrap();
    assert_eq!(first.target, target);
    assert_eq!(first.document_id, revision().document_id);
    assert!(!first.already_retained);

    let second = port.retain_reviewed(&target, revision()).unwrap();
    assert!(second.already_retained);
    assert_eq!(second.operation, first.operation);
    assert_eq!(
        pollster::block_on(store.file_revision(&read_vault, &first.document_id, first.operation))
            .unwrap(),
        Some(revision())
    );

    drop(port);
    drop(resident);
    drop(store);
    let reopened =
        KnotSyncFileStore::open(root.path().join("sync.redb"), [0x53; 32], [writer]).unwrap();
    let reopened_vault = KnotVault::open(root.path().join("vault"), [0x52; 32]).unwrap();
    assert_eq!(
        pollster::block_on(reopened.file_revision(
            &reopened_vault,
            &first.document_id,
            first.operation,
        ))
        .unwrap(),
        Some(revision())
    );
    let reopened_resident =
        KnotResidentSource::from_synced_vault(reopened_vault, reopened, seed).unwrap();
    let reopened_port = adapter(&reopened_resident);
    let retry = reopened_port
        .retain_reviewed(reopened_port.target(), revision())
        .unwrap();
    assert!(retry.already_retained);
    assert_eq!(retry.operation, first.operation);
}

#[test]
fn host_adapter_refuses_target_mismatch_and_live_authority_loss() {
    let (_root, resident, store, _read_vault, writer, _seed) = resident();
    let port = adapter(&resident);
    let target = port.target().clone();
    let tail_before = pollster::block_on(store.tail_receipt()).unwrap();

    let mut wrong = target.clone();
    wrong.persona.label = "A different host choice".into();
    assert!(port.retain_reviewed(&wrong, revision()).is_err());

    let mut ungranted = revision();
    ungranted.document_id = "file:not-granted".into();
    assert!(port.retain_reviewed(&target, ungranted).is_err());

    let narrow_capture = resident
        .capture_retention(KnotCaptureGrant::new([revision().document_id], 1))
        .unwrap();
    let narrow = KnotResidentRetainPort::new(persona(), narrow_capture).unwrap();
    assert!(narrow.retain_reviewed(narrow.target(), revision()).is_err());
    assert_eq!(
        pollster::block_on(store.tail_receipt()).unwrap(),
        tail_before
    );

    store.deny_writer(&writer);
    assert!(port.retain_reviewed(&target, revision()).is_err());
    store.admit_writer(writer);

    resident.session(None).lock_vault();
    assert!(port.retain_reviewed(&target, revision()).is_err());
    assert_eq!(
        pollster::block_on(store.tail_receipt()).unwrap(),
        tail_before
    );
}

#[test]
fn revoked_capture_port_cannot_be_reused_by_the_host_adapter() {
    let (_root, resident, store, _read_vault, _writer, _seed) = resident();
    let capture = resident
        .capture_retention(KnotCaptureGrant::new([revision().document_id], 4096))
        .unwrap();
    let port = KnotResidentRetainPort::new(persona(), capture.clone()).unwrap();
    let tail_before = pollster::block_on(store.tail_receipt()).unwrap();
    capture.revoke();
    assert!(port.retain_reviewed(port.target(), revision()).is_err());
    assert_eq!(
        pollster::block_on(store.tail_receipt()).unwrap(),
        tail_before
    );
}

#[test]
fn adapter_clones_share_one_resident_capability_without_opening_another_owner() {
    let (_root, resident, store, _read_vault, _writer, _seed) = resident();
    let capture = resident
        .capture_retention(KnotCaptureGrant::new([revision().document_id], 4096))
        .unwrap();
    let first = KnotResidentRetainPort::new(persona(), capture.clone()).unwrap();
    let second = KnotResidentRetainPort::new(persona(), capture).unwrap();

    let receipt = first.retain_reviewed(first.target(), revision()).unwrap();
    let retry = second.retain_reviewed(second.target(), revision()).unwrap();
    assert!(retry.already_retained);
    assert_eq!(retry.operation, receipt.operation);
    assert_eq!(
        pollster::block_on(store.tail_receipt()).unwrap().operations,
        vec![receipt.operation]
    );
}
