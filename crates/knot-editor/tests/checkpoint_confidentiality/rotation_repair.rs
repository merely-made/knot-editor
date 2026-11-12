// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[allow(unused_imports)]
use knot_editor::{KnotFileStoreExt as _, KnotMemoryStoreExt as _};
use super::*;

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
