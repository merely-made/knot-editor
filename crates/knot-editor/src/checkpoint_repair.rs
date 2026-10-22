// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Authorized regeneration before a file store is exposed to other handles.
use super::*;
use crate::checkpoint_migration::{self, MigrationDecision, MigrationScope};

fn checkpoint_digest(value: &KnotProjectionCheckpoint) -> Result<[u8; 32], KnotSyncError> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(value).map_err(|error| KnotSyncError::Payload(error.to_string()))?,
    );
    Ok(*blake3::hash(&bytes).as_bytes())
}

impl KnotSyncStore<RedbBackend> {
    /// Open local state under explicitly released custody. Legacy files are
    /// rebuilt from authenticated retained history into a verified fresh file.
    /// No store handle may already be alive. A prior checkpoint can advance;
    /// its old/new canonical identities are returned, never inferred from the
    /// randomized storage bytes. Missing history/epochs require recovery.
    pub async fn open_with_checkpoint_repair(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        cipher: KnotSyncCipher<'_>,
        storage_vault: &KnotVault,
    ) -> Result<(Self, KnotCheckpointRepairReceipt), KnotSyncError> {
        if storage_vault.is_locked() {
            return Err(KnotSyncError::CheckpointStorage(
                "local storage vault is locked".into(),
            ));
        }
        let path = path.as_ref();
        let profile = cipher.profile();
        let writers: BTreeSet<_> = writers.into_iter().collect();
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let store = Self::open_with_profile(path, space_id, writers, profile)?;
                if !store.storage_fresh {
                    return Err(KnotSyncError::CheckpointMigrationRequired);
                }
                let checkpoint = store
                    .save_checkpoint_with_cipher(cipher, storage_vault)
                    .await?;
                return Ok((
                    store,
                    KnotCheckpointRepairReceipt {
                        rewritten: false,
                        previous_checkpoint: None,
                        checkpoint: Some(checkpoint_digest(&checkpoint)?),
                        preserved_keys: 0,
                    },
                ));
            },
            Err(error) => return Err(KnotSyncError::CheckpointStorage(error.to_string())),
            Ok(_) => {},
        }
        let checkpoint_slot = checkpoint_key(space_id);
        let keyring_slot = communal_keyring_key(space_id);
        let marker_slot = checkpoint_storage::marker_key(space_id);
        let scope = MigrationScope {
            managed_prefixes: vec![
                "knot-sync/checkpoint/".into(),
                "knot-sync/commons-keyring/".into(),
                "knot-sync/sealed-storage/".into(),
            ],
            managed_keys: BTreeSet::from([
                checkpoint_slot.clone(),
                keyring_slot.clone(),
                marker_slot.clone(),
            ]),
        };
        let observed = Arc::new(std::sync::Mutex::new(None));
        let observed_prepare = observed.clone();
        let prepare_writers = writers.clone();
        let closed = checkpoint_migration::open_or_rewrite(path, &scope, |entries| async move {
            let prepare = async {
                let memory = MemoryBackend::new();
                // Never copy a legacy projection/keyring into another disk
                // backend, even briefly. Replay only the opaque source records.
                let writes: Vec<_> = entries.iter().filter(|(key, _)|
                    *key != &checkpoint_slot && *key != &keyring_slot && *key != &marker_slot)
                    .map(|(key, value)| WriteOp::Put { key: key.clone(), value: value.clone() }).collect();
                memory.apply(&writes).await?;
                let replica = KnotSyncStore {
                    store: MunimentStore::new(memory),
                    policy: KnotSyncPolicy { space_id, writers: Arc::new(RwLock::new(prepare_writers)), encryption: profile },
                    mutation_gate: Arc::new(Mutex::new(())), storage_fresh: true,
                };
                let mut current = false;
                if let Some(bytes) = entries.get(&marker_slot) {
                    let marker = replica.open_local(storage_vault, RecordKind::StorageMarker, bytes)?;
                    if marker.as_slice() != checkpoint_storage::MARKER {
                        return Err(KnotSyncError::CheckpointStorage("invalid clean storage marker".into()));
                    }
                    current = true;
                }
                let old = if let Some(bytes) = entries.get(&checkpoint_slot) {
                    let plaintext = match replica.open_local(storage_vault, RecordKind::Checkpoint, bytes) {
                        Ok(value) => value,
                        Err(KnotSyncError::CheckpointMigrationRequired) => {
                            current = false; Zeroizing::new(bytes.clone())
                        },
                        Err(error) => return Err(error),
                    };
                    let value: KnotProjectionCheckpoint = serde_json::from_slice(&plaintext)
                        .map_err(|_| KnotSyncError::CheckpointStorage("invalid legacy checkpoint; original retained for recovery".into()))?;
                    if value.version != 1 || value.space_id != space_id {
                        return Err(KnotSyncError::CheckpointStorage("legacy checkpoint version or space mismatch".into()));
                    }
                    Some(value)
                } else { None };
                let previous = old.as_ref().map(checkpoint_digest).transpose()?;
                let mut existing_keyring = None;
                if let Some(bytes) = entries.get(&keyring_slot) {
                    if profile != KnotEncryptionProfile::CommonsDataV1 { return Err(KnotSyncError::WrongEncryptionProfile); }
                    let plaintext = match replica.open_local(storage_vault, RecordKind::CommunalKeyring, bytes) {
                        Ok(value) => value,
                        Err(KnotSyncError::CheckpointMigrationRequired) => {
                            current = false; Zeroizing::new(bytes.clone())
                        },
                        Err(error) => return Err(error),
                    };
                    existing_keyring = Some(DataKeyring::from_bytes(&plaintext)?);
                }
                if current {
                    *observed_prepare.lock().unwrap() = Some((previous, previous));
                    return Ok(MigrationDecision::Current);
                }
                if let Some(legacy_keys) = existing_keyring {
                    let KnotSyncCipher::CommonsData(keys) = cipher else { return Err(KnotSyncError::WrongEncryptionProfile); };
                    if !checkpoint_storage::keyrings_match(keys, &legacy_keys)? {
                        return Err(KnotSyncError::CheckpointStorage(
                            "legacy group keyring differs from independently authorized keys; recover/reconcile through the membership owner".into()));
                    }
                }
                let records = replica.load_operations().await?;
                if let Some(old) = &old {
                    for head in &old.heads {
                        if !records.iter().any(|record| record.log_id == head.log_id
                            && record.operation.header.seq_num == head.seq_num
                            && record.operation.header.verifying_key.as_bytes() == &head.author
                            && record.operation.hash.as_bytes() == &head.operation) {
                            return Err(KnotSyncError::CheckpointStorage("legacy checkpoint head is absent from retained signed history; recover history before repair".into()));
                        }
                    }
                    if records.is_empty() && (!old.heads.is_empty() || !old.document_digests.is_empty()
                        || !old.conflict_ids.is_empty() || !old.pending.is_empty()
                        || old.snapshot.as_ref().is_some_and(|snapshot| snapshot != &KnotCheckpointSnapshot::default())) {
                        return Err(KnotSyncError::CheckpointStorage("legacy material has no retained signed history; recovery required".into()));
                    }
                }
                let rebuilt = replica.build_checkpoint_with_cipher(cipher).await?;
                if !rebuilt.pending.is_empty() {
                    return Err(KnotSyncError::CheckpointStorage("retained history is causally incomplete; recover missing operations before repair".into()));
                }
                let next = checkpoint_digest(&rebuilt)?;
                let plaintext = Zeroizing::new(serde_json::to_vec(&rebuilt)
                    .map_err(|error| KnotSyncError::Payload(error.to_string()))?);
                let mut replacements = BTreeMap::from([
                    (checkpoint_slot, Some(replica.seal_local(storage_vault, RecordKind::Checkpoint, &plaintext)?)),
                    (marker_slot, Some(replica.seal_local(storage_vault, RecordKind::StorageMarker, checkpoint_storage::MARKER)?)),
                    (keyring_slot.clone(), None),
                ]);
                if let KnotSyncCipher::CommonsData(keys) = cipher {
                    let plaintext = Zeroizing::new(keys.to_bytes()?);
                    replacements.insert(keyring_slot, Some(replica.seal_local(storage_vault, RecordKind::CommunalKeyring, &plaintext)?));
                }
                *observed_prepare.lock().unwrap() = Some((previous, Some(next)));
                Ok(MigrationDecision::Rewrite { replacements })
            }.await;
            prepare.map_err(|error: KnotSyncError| error.to_string())
        }).await.map_err(|error| {
            if error.replacement_committed() { KnotSyncError::CheckpointReplacementDurability(error.to_string()) }
            else { KnotSyncError::CheckpointStorage(error.to_string()) }
        })?;
        let (previous_checkpoint, checkpoint) = observed
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| KnotSyncError::CheckpointStorage("repair receipt missing".into()))?;
        let receipt = KnotCheckpointRepairReceipt {
            rewritten: closed.receipt.rewritten,
            previous_checkpoint,
            checkpoint,
            preserved_keys: closed.receipt.preserved_key_count,
        };
        Ok((
            Self {
                store: MunimentStore::new(closed.backend),
                policy: KnotSyncPolicy {
                    space_id,
                    writers: Arc::new(RwLock::new(writers)),
                    encryption: profile,
                },
                mutation_gate: Arc::new(Mutex::new(())),
                storage_fresh: false,
            },
            receipt,
        ))
    }
}
