// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Real signed-history and redb receiving cases for local checkpoint custody.

use std::collections::BTreeMap;
use std::path::Path;

use knot_editor::{
    KnotCorePredicateV1, KnotOfflineMemberRecovery, KnotPredicateRefV1, KnotProjectionCheckpoint,
    KnotRelationEndpointV1, KnotRelationPositionV1, KnotResidentSource, KnotSyncCipher,
    KnotSyncEvent, KnotSyncFileStore, KnotSyncStore, KnotVault, VaultDocument,
    capture_endpoint_context,
};
use muniment::{Backend, RedbBackend};
use p2panda_core::{Body, SigningKey, Topic};
use proofs::Digest;
use serde_json::Value;
use stickleback::DataKeyring;

const SPACE: [u8; 32] = [0xd1; 32];
const VAULT_KEY: [u8; 32] = [0xd2; 32];
const SEED: [u8; 32] = [0xd3; 32];
const TITLE: &str = "private checkpoint title 9ef8aa65";
const BODY: &str = "private checkpoint body 0bcd1b9f";
const EVIDENCE: &str = "private checkpoint evidence 24a56e7c";
const QUALIFICATION: &str = "private checkpoint qualification d46f1208";
const OBSOLETE: &str = "obsolete deleted checkpoint residue 5d8756d1";

fn writer() -> [u8; 32] {
    *SigningKey::from_bytes(&SEED).verifying_key().as_bytes()
}

fn key(namespace: &str) -> String {
    let hex = SPACE
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("knot-sync/{namespace}/{hex}")
}

fn document(id: &str) -> VaultDocument {
    VaultDocument {
        id: id.into(),
        title: TITLE.into(),
        body: BODY.as_bytes().to_vec(),
        media_type: "text/vnd.knot".into(),
    }
}

fn endpoint(head: [u8; 32]) -> KnotRelationEndpointV1 {
    let (prefix, suffix) = capture_endpoint_context(BODY, 0, BODY.len());
    KnotRelationEndpointV1 {
        document_id: "private-note".into(),
        document_head: head,
        quote: BODY.into(),
        position: Some(KnotRelationPositionV1 {
            start: 0,
            end: BODY.len() as u64,
        }),
        prefix,
        suffix,
    }
}

async fn author_fixture(store: &KnotSyncFileStore, cipher: KnotSyncCipher<'_>) -> [u8; 32] {
    let put = store
        .author_with_cipher(&SEED, cipher, &KnotSyncEvent::Put(document("private-note")))
        .await
        .unwrap();
    let capture = endpoint(*put.hash.as_bytes());
    let assertion = store
        .author_with_cipher(
            &SEED,
            cipher,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                subject: capture.clone(),
                object: capture,
                evidence: Some(EVIDENCE.into()),
                qualification: Some(QUALIFICATION.into()),
            },
        )
        .await
        .unwrap();
    *assertion.hash.as_bytes()
}

fn checkpoint_id(checkpoint: &KnotProjectionCheckpoint) -> [u8; 32] {
    *blake3::hash(&serde_json::to_vec(checkpoint).unwrap()).as_bytes()
}

fn keyring_value(keys: &DataKeyring) -> Value {
    let mut value = serde_json::to_value(keys).unwrap();
    // The engine serializes its secret HashMap as a sequence. Its order can
    // change on reopen, so compare all actual values after ordering that set.
    value["secrets"]
        .as_array_mut()
        .unwrap()
        .sort_by_cached_key(Value::to_string);
    value
}

fn keyring_secrets(keys: &DataKeyring) -> Vec<Vec<u8>> {
    keyring_value(keys)["secrets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|secret| {
            secret[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
                .collect()
        })
        .collect()
}

fn assert_keys_private(bytes: &[u8], secrets: &[Vec<u8>]) {
    for secret in secrets {
        assert_eq!(secret.len(), 32);
        assert!(!contains(bytes, secret), "unsealed group epoch secret");
        assert!(
            !contains(bytes, &serde_json::to_vec(secret).unwrap()),
            "unsealed JSON group epoch secret"
        );
    }
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

fn assert_private(bytes: &[u8]) {
    for secret in [TITLE, BODY, EVIDENCE, QUALIFICATION, OBSOLETE] {
        assert!(
            !contains(bytes, secret.as_bytes()),
            "cleartext field: {secret}"
        );
    }
    // VaultDocument.body serializes as a JSON byte array. Searching only UTF-8
    // would miss a plaintext checkpoint even when its entire body leaked.
    assert!(
        !contains(bytes, &serde_json::to_vec(BODY.as_bytes()).unwrap()),
        "cleartext JSON body byte array"
    );
}

async fn raw_checkpoint(store: &KnotSyncFileStore) -> Vec<u8> {
    store
        .sync_store()
        .backend()
        .get(&key("checkpoint"))
        .await
        .unwrap()
        .unwrap()
}

async fn logical(backend: &RedbBackend) -> BTreeMap<String, Vec<u8>> {
    let mut entries = BTreeMap::new();
    for key in backend.list("").await.unwrap() {
        entries.insert(key.clone(), backend.get(&key).await.unwrap().unwrap());
    }
    entries
}

async fn legacy_checkpoint(store: &KnotSyncFileStore, checkpoint: &KnotProjectionCheckpoint) {
    let raw = store.sync_store();
    raw.backend()
        .put(&key("checkpoint"), &serde_json::to_vec(checkpoint).unwrap())
        .await
        .unwrap();
    raw.backend().delete(&key("sealed-storage")).await.unwrap();
}

fn empty_legacy_checkpoint() -> KnotProjectionCheckpoint {
    KnotProjectionCheckpoint {
        version: 1,
        space_id: SPACE,
        heads: Vec::new(),
        document_digests: Vec::new(),
        conflict_ids: Vec::new(),
        pending: Vec::new(),
        snapshot: None,
    }
}

#[tokio::test]
async fn personal_sealing_preserves_domain_and_tail_identity_and_refuses_wrong_keys() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sync.redb");
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let store = KnotSyncFileStore::open(&path, SPACE, [writer()]).unwrap();
    let assertion = author_fixture(&store, KnotSyncCipher::Personal(&vault)).await;
    let checkpoint = store.save_checkpoint(&vault).await.unwrap();
    assert_eq!(checkpoint.version, 1);
    assert_eq!(
        checkpoint.snapshot.as_ref().unwrap().relations[0].id,
        assertion
    );
    let first = raw_checkpoint(&store).await;
    assert_private(&first);
    assert_eq!(
        store.load_checkpoint(&vault).await.unwrap(),
        Some(checkpoint.clone())
    );
    let tail = store.tail_receipt(&vault).await.unwrap();
    assert_eq!(tail.checkpoint, checkpoint_id(&checkpoint));
    assert!(tail.operations.is_empty());

    assert_eq!(store.save_checkpoint(&vault).await.unwrap(), checkpoint);
    let second = raw_checkpoint(&store).await;
    assert_ne!(first, second, "fresh storage nonce on reseal");
    assert_eq!(store.tail_receipt(&vault).await.unwrap(), tail);

    let wrong = KnotVault::open(root.path().join("wrong"), [0xd4; 32]).unwrap();
    assert!(store.load_checkpoint(&wrong).await.is_err());
    assert!(store.tail_receipt(&wrong).await.is_err());
    assert!(store.save_checkpoint(&wrong).await.is_err());
    assert!(
        store
            .save_checkpoint_with_cipher(KnotSyncCipher::Personal(&wrong), &vault)
            .await
            .is_err(),
        "correct local custody must not bless history decrypted with the wrong event key"
    );
    let mut foreign_profile = DataKeyring::new();
    foreign_profile.rotate_random().unwrap();
    assert!(
        store
            .save_checkpoint_with_cipher(KnotSyncCipher::CommonsData(&foreign_profile), &vault)
            .await
            .is_err()
    );
    assert_eq!(
        raw_checkpoint(&store).await,
        second,
        "wrong event key must not replace good state"
    );
    let mut locked = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    locked.lock();
    assert!(store.load_checkpoint(&locked).await.is_err());
    assert!(store.save_checkpoint(&locked).await.is_err());
    assert_eq!(raw_checkpoint(&store).await, second);
    drop(store);
    assert_private(&std::fs::read(&path).unwrap());

    let reopened = KnotSyncFileStore::open(&path, SPACE, [writer()]).unwrap();
    assert_eq!(
        reopened.load_checkpoint(&vault).await.unwrap(),
        Some(checkpoint)
    );
    assert_eq!(
        reopened.projection(&vault).await.unwrap().relations[0].id,
        assertion
    );
}

#[tokio::test]
async fn local_envelope_rejects_tampered_context_version_kind_and_ciphertext() {
    let root = tempfile::tempdir().unwrap();
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let store = KnotSyncFileStore::open(root.path().join("sync.redb"), SPACE, [writer()]).unwrap();
    author_fixture(&store, KnotSyncCipher::Personal(&vault)).await;
    let expected = store.save_checkpoint(&vault).await.unwrap();
    let original = raw_checkpoint(&store).await;
    let envelope: Value = serde_json::from_slice(&original).unwrap();
    for field in [
        "format",
        "version",
        "space_id",
        "profile",
        "kind",
        "ciphertext",
    ] {
        let mut tampered = envelope.clone();
        match field {
            "format" => tampered[field] = Value::from("foreign.local.storage"),
            "version" => tampered[field] = Value::from(999),
            "space_id" => tampered[field][0] = Value::from(0),
            "profile" => tampered[field] = Value::from("CommonsDataV1"),
            "kind" => tampered[field] = Value::from("CommunalKeyring"),
            "ciphertext" => {
                let first = tampered[field][0].as_u64().unwrap();
                tampered[field][0] = Value::from(first ^ 1);
            },
            _ => unreachable!(),
        }
        let backend = store.sync_store();
        backend
            .backend()
            .put(&key("checkpoint"), &serde_json::to_vec(&tampered).unwrap())
            .await
            .unwrap();
        assert!(
            store.load_checkpoint(&vault).await.is_err(),
            "tampered {field}"
        );
        assert!(
            store.tail_receipt(&vault).await.is_err(),
            "tail accepted tampered {field}"
        );
        backend
            .backend()
            .put(&key("checkpoint"), &original)
            .await
            .unwrap();
    }
    assert_eq!(store.load_checkpoint(&vault).await.unwrap(), Some(expected));
}

#[tokio::test]
async fn communal_keyring_and_checkpoint_use_local_custody_across_rotation_and_pruning() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("commons.redb");
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let store = KnotSyncFileStore::open_commons(&path, SPACE, [writer()]).unwrap();
    let mut keys = DataKeyring::new();
    let mut epochs = Vec::new();
    for _ in 0..10 {
        epochs.push(keys.rotate_random().unwrap().id());
    }
    author_fixture(&store, KnotSyncCipher::CommonsData(&keys)).await;
    let checkpoint = store.save_communal_checkpoint(&keys, &vault).await.unwrap();
    let first = raw_checkpoint(&store).await;
    assert_private(&first);
    let tail = store.tail_receipt(&vault).await.unwrap();
    assert_eq!(tail.checkpoint, checkpoint_id(&checkpoint));
    assert_eq!(
        store.save_communal_checkpoint(&keys, &vault).await.unwrap(),
        checkpoint
    );
    assert_ne!(raw_checkpoint(&store).await, first);
    assert_eq!(store.tail_receipt(&vault).await.unwrap(), tail);
    // Rotating the replicated group key neither changes nor unlocks the local record.
    keys.rotate_random().unwrap();
    assert_eq!(
        store.load_checkpoint(&vault).await.unwrap(),
        Some(checkpoint.clone())
    );
    // Membership handoff is explicit. An ordinary checkpoint/pruning call
    // cannot infer that caller-supplied extra epochs supersede persisted custody.
    store.persist_communal_keyring(&keys, &vault).await.unwrap();
    let revision = Digest::blake3(b"checkpoint confidentiality test authority");
    let proposal = store
        .communal_epoch_pruning_proposal(
            &keys,
            &vault,
            revision.clone(),
            revision.clone(),
            &[],
            &[],
        )
        .await
        .unwrap();
    assert!(proposal.is_executable());
    assert!(!proposal.forget.is_empty());
    let before_keys = keys.to_bytes().unwrap();
    let all_secrets = keyring_secrets(&keys);
    let receipt = store
        .execute_communal_epoch_pruning(
            &mut keys,
            &vault,
            &proposal,
            revision.clone(),
            revision,
            &[],
            &[],
        )
        .await
        .unwrap();
    assert!(!receipt.forgotten.is_empty());
    let reduced = keys.to_bytes().unwrap();
    let stale = DataKeyring::from_bytes(&before_keys).unwrap();
    let durable = logical(store.sync_store().backend()).await;
    assert!(
        store
            .save_communal_checkpoint(&stale, &vault)
            .await
            .is_err(),
        "ordinary checkpoint save must not revive erased epoch authority"
    );
    let authority = Digest::blake3(b"checkpoint confidentiality test authority");
    assert!(
        store
            .communal_epoch_pruning_proposal(
                &stale,
                &vault,
                authority.clone(),
                authority,
                &[],
                &[],
            )
            .await
            .is_err(),
        "a stale caller keyring cannot authorize another pruning plan"
    );
    assert_eq!(logical(store.sync_store().backend()).await, durable);
    let stored = store
        .sync_store()
        .backend()
        .get(&key("commons-keyring"))
        .await
        .unwrap()
        .unwrap();
    assert_ne!(stored, reduced);
    assert!(!contains(&stored, &reduced));
    assert!(!contains(&stored, &before_keys));
    assert_keys_private(&stored, &all_secrets);
    let wrong = KnotVault::open(root.path().join("wrong"), [0xd4; 32]).unwrap();
    let mut untouched = DataKeyring::new();
    untouched.rotate_random().unwrap();
    let untouched_bytes = untouched.to_bytes().unwrap();
    assert!(
        store
            .restore_communal_keyring(&mut untouched, &wrong)
            .await
            .is_err()
    );
    assert_eq!(untouched.to_bytes().unwrap(), untouched_bytes);
    let mut locked = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    locked.lock();
    assert!(
        store
            .restore_communal_keyring(&mut untouched, &locked)
            .await
            .is_err()
    );
    assert_eq!(untouched.to_bytes().unwrap(), untouched_bytes);
    assert!(
        store
            .restore_communal_keyring(&mut untouched, &vault)
            .await
            .is_err(),
        "even correct local custody must not silently replace a different live membership state"
    );
    assert_eq!(untouched.to_bytes().unwrap(), untouched_bytes);
    let mut restored = DataKeyring::new();
    assert!(
        store
            .restore_communal_keyring(&mut restored, &vault)
            .await
            .unwrap()
    );
    assert_eq!(keyring_value(&restored), keyring_value(&keys));
    assert!(matches!(
        store
            .communal_offline_member_recovery(&keys, &vault, epochs[0])
            .await
            .unwrap(),
        KnotOfflineMemberRecovery::BootstrapRequired {
            checkpoint: Some(_)
        }
    ));
    let independent_group = DataKeyring::new();
    let protected = raw_checkpoint(&store).await;
    assert!(
        store
            .save_communal_checkpoint(&independent_group, &vault)
            .await
            .is_err()
    );
    assert_eq!(raw_checkpoint(&store).await, protected);
    drop(store);
    let bytes = std::fs::read(&path).unwrap();
    assert_private(&bytes);
    assert!(!contains(&bytes, &before_keys));
    assert!(!contains(&bytes, &reduced));
    assert_keys_private(&bytes, &all_secrets);
    let reopened = KnotSyncFileStore::open_commons(&path, SPACE, [writer()]).unwrap();
    let mut restored = DataKeyring::new();
    assert!(
        reopened
            .restore_communal_keyring(&mut restored, &vault)
            .await
            .unwrap()
    );
    assert_eq!(keyring_value(&restored), keyring_value(&keys));
    assert_eq!(
        reopened.load_checkpoint(&vault).await.unwrap(),
        Some(checkpoint)
    );
    assert_eq!(
        reopened
            .communal_projection(&restored)
            .await
            .unwrap()
            .documents,
        vec![document("private-note")]
    );
}

#[tokio::test]
async fn repair_rebuilds_stale_legacy_checkpoint_and_removes_deleted_plaintext_pages() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sync.redb");
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let store = KnotSyncFileStore::open(&path, SPACE, [writer()]).unwrap();
    let assertion = author_fixture(&store, KnotSyncCipher::Personal(&vault)).await;
    let previous = store.save_checkpoint(&vault).await.unwrap();
    let last = store
        .author(&SEED, &vault, &KnotSyncEvent::Put(document("later-note")))
        .await
        .unwrap();
    // Put obsolete private material through the actual checkpoint slot, then
    // overwrite and delete it. The rewrite must omit abandoned database pages.
    let mut obsolete = previous.clone();
    obsolete.snapshot.as_mut().unwrap().documents[0].title = OBSOLETE.repeat(2048);
    legacy_checkpoint(&store, &obsolete).await;
    store
        .sync_store()
        .backend()
        .delete(&key("checkpoint"))
        .await
        .unwrap();
    legacy_checkpoint(&store, &previous).await;
    assert!(
        store.load_checkpoint(&vault).await.is_err(),
        "legacy JSON must not be an authenticated read"
    );
    assert!(store.tail_receipt(&vault).await.is_err());
    let resident_vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let refusal = KnotResidentSource::from_synced_vault(resident_vault, store.clone(), &SEED)
        .err()
        .expect("personal resident must refuse legacy storage");
    assert!(refusal.contains("repair"), "{refusal}");
    drop(store);
    let before = std::fs::read(&path).unwrap();
    assert!(
        contains(&before, TITLE.as_bytes()),
        "live legacy plaintext control"
    );
    assert!(
        contains(&before, OBSOLETE.as_bytes()),
        "deleted-page plaintext control"
    );
    let (repaired, receipt) = KnotSyncFileStore::open_with_checkpoint_repair(
        &path,
        SPACE,
        [writer()],
        KnotSyncCipher::Personal(&vault),
        &vault,
    )
    .await
    .unwrap();
    assert!(receipt.rewritten);
    assert_eq!(receipt.previous_checkpoint, Some(checkpoint_id(&previous)));
    let checkpoint = repaired.load_checkpoint(&vault).await.unwrap().unwrap();
    assert_eq!(receipt.checkpoint, Some(checkpoint_id(&checkpoint)));
    assert_ne!(
        receipt.checkpoint, receipt.previous_checkpoint,
        "the stale frontier was regenerated"
    );
    assert!(receipt.preserved_keys > 0);
    assert_eq!(
        checkpoint.snapshot.as_ref().unwrap().relations[0].id,
        assertion
    );
    assert_eq!(checkpoint.snapshot.as_ref().unwrap().documents.len(), 2);
    assert!(
        repaired
            .tail_receipt(&vault)
            .await
            .unwrap()
            .operations
            .is_empty()
    );
    let resident_vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let resident = KnotResidentSource::from_synced_vault(resident_vault, repaired.clone(), &SEED)
        .expect("personal resident must accept repaired storage");
    drop(resident);
    drop(repaired);
    assert_private(&std::fs::read(&path).unwrap());

    // The child gets only paths and public operation/digest facts; fixture keys
    // are fixed test constants in its executable, never command-line secrets.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "checkpoint_confidentiality_child_reopen",
            "--nocapture",
        ])
        .env("KNOT_CHECKPOINT_CONFIDENTIALITY_ROOT", root.path())
        .env(
            "KNOT_CHECKPOINT_CONFIDENTIALITY_EXPECTED",
            serde_json::to_string(&(
                *last.hash.as_bytes(),
                last.header.seq_num,
                checkpoint_id(&checkpoint),
                assertion,
            ))
            .unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child stdout: {}\nchild stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("authenticated checkpoint reopened and author continued")
    );
    assert_private(&std::fs::read(&path).unwrap());
}

#[test]
#[ignore = "launched by the repaired signed-history test in a separate process"]
fn checkpoint_confidentiality_child_reopen() {
    let root = std::env::var_os("KNOT_CHECKPOINT_CONFIDENTIALITY_ROOT").expect("child root");
    let (last, seq, checkpoint, assertion): ([u8; 32], u32, [u8; 32], [u8; 32]) =
        serde_json::from_str(
            &std::env::var("KNOT_CHECKPOINT_CONFIDENTIALITY_EXPECTED").expect("child public facts"),
        )
        .unwrap();
    pollster::block_on(async {
        let root = Path::new(&root);
        let vault = KnotVault::open(root.join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncFileStore::open(root.join("sync.redb"), SPACE, [writer()]).unwrap();
        let loaded = store.load_checkpoint(&vault).await.unwrap().unwrap();
        assert_eq!(checkpoint_id(&loaded), checkpoint);
        assert_eq!(
            store.projection(&vault).await.unwrap().relations[0].id,
            assertion
        );
        let next = store
            .author(&SEED, &vault, &KnotSyncEvent::Put(document("child-note")))
            .await
            .unwrap();
        assert_eq!(next.header.seq_num, seq + 1);
        assert_eq!(
            next.header.backlink.as_ref().map(|hash| *hash.as_bytes()),
            Some(last)
        );
        assert_eq!(
            store.tail_receipt(&vault).await.unwrap().operations,
            vec![*next.hash.as_bytes()]
        );
        println!("authenticated checkpoint reopened and author continued");
    });
}

#[tokio::test]
async fn communal_legacy_keyring_is_resealed_with_checkpoint_after_authenticated_regeneration() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("commons.redb");
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let mut keys = DataKeyring::new();
    keys.rotate_random().unwrap();
    let store = KnotSyncFileStore::open_commons(&path, SPACE, [writer()]).unwrap();
    author_fixture(&store, KnotSyncCipher::CommonsData(&keys)).await;
    let checkpoint = store.save_communal_checkpoint(&keys, &vault).await.unwrap();
    legacy_checkpoint(&store, &checkpoint).await;
    let raw_keys = keys.to_bytes().unwrap();
    store
        .sync_store()
        .backend()
        .put(&key("commons-keyring"), &raw_keys)
        .await
        .unwrap();
    let mut refused = DataKeyring::new();
    assert!(
        store
            .restore_communal_keyring(&mut refused, &vault)
            .await
            .is_err()
    );
    assert_eq!(refused.epoch_count(), 0);
    drop(store);
    let (repaired, receipt) = KnotSyncFileStore::open_with_checkpoint_repair(
        &path,
        SPACE,
        [writer()],
        KnotSyncCipher::CommonsData(&keys),
        &vault,
    )
    .await
    .unwrap();
    assert!(receipt.rewritten);
    assert_eq!(receipt.previous_checkpoint, receipt.checkpoint);
    assert!(
        repaired
            .restore_communal_keyring(&mut refused, &vault)
            .await
            .unwrap()
    );
    assert_eq!(keyring_value(&refused), keyring_value(&keys));
    assert_eq!(
        repaired.load_checkpoint(&vault).await.unwrap(),
        Some(checkpoint)
    );
    drop(repaired);
    let bytes = std::fs::read(&path).unwrap();
    assert_private(&bytes);
    assert_keys_private(&bytes, &keyring_secrets(&keys));
    assert!(
        !contains(&bytes, &raw_keys),
        "legacy raw epoch secrets must not survive in replacement pages"
    );
}

#[tokio::test]
async fn unavailable_legacy_epoch_refuses_repair_without_changing_logical_values() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("commons.redb");
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let mut keys = DataKeyring::new();
    let erased = keys.rotate_random().unwrap().id();
    let store = KnotSyncFileStore::open_commons(&path, SPACE, [writer()]).unwrap();
    store
        .author_communal(&SEED, &keys, &KnotSyncEvent::Put(document("private-note")))
        .await
        .unwrap();
    let checkpoint = store.save_communal_checkpoint(&keys, &vault).await.unwrap();
    for _ in 0..9 {
        keys.rotate_random().unwrap();
    }
    assert!(keys.forget_authorized(&erased));
    legacy_checkpoint(&store, &checkpoint).await;
    store
        .sync_store()
        .backend()
        .put(&key("commons-keyring"), &keys.to_bytes().unwrap())
        .await
        .unwrap();
    let expected = logical(store.sync_store().backend()).await;
    drop(store);
    let failure = KnotSyncFileStore::open_with_checkpoint_repair(
        &path,
        SPACE,
        [writer()],
        KnotSyncCipher::CommonsData(&keys),
        &vault,
    )
    .await
    .err()
    .expect("missing source epoch must refuse regeneration");
    assert!(failure.to_string().contains("epoch"), "{failure}");
    let reopened = RedbBackend::open(&path).unwrap();
    assert_eq!(logical(&reopened).await, expected);
}

#[tokio::test]
async fn incomplete_or_body_mismatched_signed_history_cannot_be_blessed_by_repair() {
    let root = tempfile::tempdir().unwrap();
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let second_seed = [0xd5; 32];
    let second_writer = *SigningKey::from_bytes(&second_seed)
        .verifying_key()
        .as_bytes();
    let donor = KnotSyncStore::in_memory(SPACE, [writer(), second_writer]);
    donor
        .author(&SEED, &vault, &KnotSyncEvent::Put(document("parent")))
        .await
        .unwrap();
    let child = donor
        .author(&second_seed, &vault, &KnotSyncEvent::Put(document("child")))
        .await
        .unwrap();
    for missing_parent in [true, false] {
        let path = root.path().join(if missing_parent {
            "pending.redb"
        } else {
            "body.redb"
        });
        let store = KnotSyncFileStore::open(&path, SPACE, [writer(), second_writer]).unwrap();
        if missing_parent {
            store.accept(&child).await.unwrap();
            assert_eq!(store.projection(&vault).await.unwrap().pending.len(), 1);
        } else {
            let clean = KnotSyncStore::in_memory(SPACE, [writer()]);
            let mut bad = clean
                .author(&SEED, &vault, &KnotSyncEvent::Put(document("bad-body")))
                .await
                .unwrap();
            // Bypass normal ingress only to model a corrupted old database:
            // preserve its valid signed header while substituting body bytes.
            bad.body = Some(Body::from_bytes(b"not the header's committed ciphertext"));
            store
                .sync_store()
                .insert_indexed_operation(&Topic::from(SPACE), &bad, &0_u64)
                .await
                .unwrap();
        }
        legacy_checkpoint(&store, &empty_legacy_checkpoint()).await;
        let expected = logical(store.sync_store().backend()).await;
        drop(store);
        assert!(
            KnotSyncFileStore::open_with_checkpoint_repair(
                &path,
                SPACE,
                [writer(), second_writer],
                KnotSyncCipher::Personal(&vault),
                &vault,
            )
            .await
            .is_err()
        );
        let reopened = RedbBackend::open(&path).unwrap();
        assert_eq!(
            logical(&reopened).await,
            expected,
            "failed repair must preserve source logical state"
        );
    }
}

#[tokio::test]
async fn orphaned_signed_log_rows_cannot_be_migrated_as_an_empty_store() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("orphan.redb");
    let vault = KnotVault::open(root.path().join("vault"), VAULT_KEY).unwrap();
    let store = KnotSyncFileStore::open(&path, SPACE, [writer()]).unwrap();
    store
        .author(
            &SEED,
            &vault,
            &KnotSyncEvent::Put(document("orphaned-note")),
        )
        .await
        .unwrap();
    let raw = store.sync_store();
    assert!(!raw.backend().list("log/").await.unwrap().is_empty());
    let topics = raw.backend().list("topic/").await.unwrap();
    assert!(!topics.is_empty());
    for topic in topics {
        raw.backend().delete(&topic).await.unwrap();
    }
    assert!(raw.backend().list("topic/").await.unwrap().is_empty());
    assert!(
        raw.backend()
            .get(&key("checkpoint"))
            .await
            .unwrap()
            .is_none()
    );
    let expected = logical(raw.backend()).await;
    drop(raw);
    drop(store);
    assert!(
        KnotSyncFileStore::open_with_checkpoint_repair(
            &path,
            SPACE,
            [writer()],
            KnotSyncCipher::Personal(&vault),
            &vault,
        )
        .await
        .is_err(),
        "missing topic indexes must not hide signed log rows during regeneration"
    );
    let reopened = RedbBackend::open(&path).unwrap();
    assert_eq!(logical(&reopened).await, expected);
}
