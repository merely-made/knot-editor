// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Fail closed when retained signed history and its storage indexes disagree.
//! This follows the pinned Stickleback muniment store schema; it does not
//! reinterpret opaque content outside the log, operation and topic indexes.

use super::*;

fn history_error(message: &str) -> KnotSyncError {
    KnotSyncError::CheckpointStorage(format!(
        "retained signed history {message}; recover its indexes before checkpointing"
    ))
}

fn log_segment() -> Result<String, KnotSyncError> {
    let bytes = encode_cbor(&LOG_ID).map_err(|error| KnotSyncError::Payload(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

impl<B: Backend + Clone> KnotSyncStore<B> {
    /// Validate every retained body, including causally pending bodies, and
    /// require a complete, canonical accounting of this owned file's indexes.
    /// Historical writers can now be revoked; this does not reapply today's
    /// membership list to already retained signed operations.
    pub(super) async fn checkpoint_records(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<Vec<StoredKnotOperation>, KnotSyncError> {
        self.require_cipher(cipher)?;
        let records = self.load_operations().await?;
        let segment = log_segment()?;
        let mut log_keys = BTreeSet::new();
        let mut operation_keys = BTreeSet::new();
        let mut topic_keys = BTreeSet::new();
        for record in &records {
            let operation = &record.operation;
            if record.log_id != LOG_ID {
                return Err(history_error("uses an unexpected log identity"));
            }
            p2panda_core::operation::validate_operation(operation).map_err(|error| {
                KnotSyncError::Payload(format!("invalid retained operation: {error}"))
            })?;
            if operation.hash != operation.header.hash() {
                return Err(history_error(
                    "operation id does not match its signed header",
                ));
            }
            if operation.header.extensions.space_id != self.space_id()
                || operation.header.extensions.encryption != self.encryption_profile()
            {
                return Err(KnotSyncError::WrongEncryptionProfile);
            }
            validate_causal_metadata(
                operation,
                &operation.header.extensions.parents,
                KNOT_CAUSAL_LIMITS,
            )?;
            // Authenticate pending operations too: otherwise a wrong key could
            // turn pending-only history into an apparently empty checkpoint.
            decode_event(cipher, operation)?;

            // Stickleback 34105d63: log/<author>/<hex(CBOR(log))>/<seq:016x>,
            // op/<header-hash> -> that exact UTF-8 log key, and an empty-value
            // topic/<space>/<author>/<hex(CBOR(log))> association.
            let author = operation.header.verifying_key.to_hex();
            let log_key = format!("log/{author}/{segment}/{:016x}", operation.header.seq_num);
            let operation_key = format!("op/{}", operation.hash.to_hex());
            let topic_key = format!(
                "topic/{}/{author}/{segment}",
                Topic::from(self.space_id()).to_hex()
            );
            if !log_keys.insert(log_key.clone()) || !operation_keys.insert(operation_key.clone()) {
                return Err(history_error("contains duplicate storage identities"));
            }
            topic_keys.insert(topic_key.clone());
            if self.store.backend().get(&operation_key).await? != Some(log_key.clone().into_bytes())
            {
                return Err(history_error(
                    "operation pointer differs from its signed identity",
                ));
            }
            if self.store.backend().get(&topic_key).await? != Some(Vec::new()) {
                return Err(history_error(
                    "topic association is missing or noncanonical",
                ));
            }
            // Matching key sets alone would miss two log payloads swapped
            // between their keys. Follow the exact pointer back to its bytes.
            let indexed = self
                .store
                .get_operation(&operation.hash)
                .await?
                .ok_or_else(|| history_error("operation pointer has no retained log entry"))?;
            if indexed.hash != operation.hash
                || indexed.header.encode() != operation.header.encode()
                || indexed.body.as_ref().map(Body::to_bytes)
                    != operation.body.as_ref().map(Body::to_bytes)
            {
                return Err(history_error(
                    "log entry differs from its signed storage identity",
                ));
            }
        }
        let actual_logs: BTreeSet<_> = self
            .store
            .backend()
            .list("log/")
            .await?
            .into_iter()
            .collect();
        let actual_operations: BTreeSet<_> = self
            .store
            .backend()
            .list("op/")
            .await?
            .into_iter()
            .collect();
        let actual_topics: BTreeSet<_> = self
            .store
            .backend()
            .list("topic/")
            .await?
            .into_iter()
            .collect();
        if actual_logs != log_keys
            || actual_operations != operation_keys
            || actual_topics != topic_keys
        {
            return Err(history_error(
                "is unindexed, missing, or outside this file's space",
            ));
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPACE: [u8; 32] = [0x61; 32];
    const SEED: [u8; 32] = [0x62; 32];

    async fn fixture() -> (tempfile::TempDir, KnotVault, KnotSyncStore<MemoryBackend>) {
        let dir = tempfile::tempdir().unwrap();
        let vault = KnotVault::open(dir.path(), [0x63; 32]).unwrap();
        let writer = *SigningKey::from_bytes(&SEED).verifying_key().as_bytes();
        let store = KnotSyncStore::in_memory(SPACE, [writer]);
        for id in ["first", "second"] {
            store
                .author(
                    &SEED,
                    &vault,
                    &KnotSyncEvent::Put(VaultDocument {
                        id: id.into(),
                        title: id.into(),
                        body: b"signed checkpoint history".to_vec(),
                        media_type: "text/vnd.knot".into(),
                    }),
                )
                .await
                .unwrap();
        }
        (dir, vault, store)
    }

    #[tokio::test]
    async fn checkpoint_history_authenticates_retained_revoked_writer() {
        let (_dir, vault, store) = fixture().await;
        store.deny_writer(SigningKey::from_bytes(&SEED).verifying_key().as_bytes());
        assert_eq!(
            store
                .checkpoint_records(KnotSyncCipher::Personal(&vault))
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn checkpoint_history_refuses_unindexed_retained_logs() {
        let (_dir, vault, store) = fixture().await;
        let backend = store.store.backend();
        let topic = backend.list("topic/").await.unwrap().pop().unwrap();
        backend.delete(&topic).await.unwrap();
        assert!(store.load_operations().await.unwrap().is_empty());
        assert!(
            store
                .checkpoint_records(KnotSyncCipher::Personal(&vault))
                .await
                .is_err()
        );
        assert_eq!(backend.list("log/").await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn checkpoint_history_refuses_swapped_storage_payloads() {
        let (_dir, vault, store) = fixture().await;
        let backend = store.store.backend();
        let mut logs = backend.list("log/").await.unwrap();
        logs.sort();
        let first = backend.get(&logs[0]).await.unwrap().unwrap();
        let second = backend.get(&logs[1]).await.unwrap().unwrap();
        backend
            .apply(&[
                WriteOp::Put {
                    key: logs[0].clone(),
                    value: second,
                },
                WriteOp::Put {
                    key: logs[1].clone(),
                    value: first,
                },
            ])
            .await
            .unwrap();
        assert!(
            store
                .checkpoint_records(KnotSyncCipher::Personal(&vault))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn checkpoint_history_refuses_retained_body_corruption() {
        let (_dir, vault, store) = fixture().await;
        let backend = store.store.backend();
        let log = backend.list("log/").await.unwrap().pop().unwrap();
        let mut blob = backend.get(&log).await.unwrap().unwrap();
        *blob.last_mut().unwrap() ^= 1;
        backend.put(&log, &blob).await.unwrap();
        assert!(
            store
                .checkpoint_records(KnotSyncCipher::Personal(&vault))
                .await
                .is_err()
        );
    }
}
