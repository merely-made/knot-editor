// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The replicated, encrypted operation store over a backend.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::{Arc, RwLock};

use muniment::{Backend, RedbBackend, WriteOp};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_store::topics::TopicStore;
use stickleback::{
    CausalIndex, DataKeyring, MunimentStore, OperationProcessor, author_head, causal_projection,
    observed_frontier,
};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use super::storage::{self, RecordKind};
use super::cipher::operation_aad;
use super::{
    AssertedTime, EncryptionProfile, LOG_ID, LocalCheck, StoredOperation, SyncCipher, SyncDomain,
    SyncError, SyncExt, SyncPolicy, SyncSealer, causal_entries, seal_event,
};

/// Replicated encrypted event store for one space, for one domain.
#[derive(Clone)]
pub struct SyncStore<B, D> {
    pub(crate) store: MunimentStore<B, SyncExt>,
    pub(crate) policy: SyncPolicy,
    /// Coordinates managed mutations. Direct writes through [`Self::sync_store`]
    /// remain outside this guarantee.
    pub(crate) mutation_gate: Arc<Mutex<()>>,
    // Only a newly created file or a memory backend can be free of old redb
    // pages without an authenticated clean-format marker. Clones retain this
    // process-local fact; reopened files must authenticate their marker.
    pub(crate) storage_fresh: bool,
    pub(crate) domain: D,
}

impl<B, D: SyncDomain> SyncStore<B, D> {
    /// A store over a backend the caller opened, for example a reservoir's.
    ///
    /// `fresh` says the backend held no earlier redb pages (it was just
    /// created); a reopened backend passes `false` and must carry its
    /// authenticated clean-format marker.
    pub fn from_backend(
        backend: B,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: EncryptionProfile,
        domain: D,
        fresh: bool,
    ) -> Self {
        Self {
            store: MunimentStore::new(backend),
            policy: SyncPolicy {
                space_id,
                writers: Arc::new(RwLock::new(writers.into_iter().collect())),
                encryption,
                words: &D::VOCABULARY,
            },
            mutation_gate: Arc::new(Mutex::new(())),
            storage_fresh: fresh,
            domain,
        }
    }
}

impl<D: SyncDomain> SyncStore<RedbBackend, D> {
    /// Open (creating if absent) a redb file as the store's backend.
    ///
    /// # Errors
    /// Fails when the file cannot be created or opened.
    pub fn open_file(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: EncryptionProfile,
        domain: D,
    ) -> Result<Self, D::Error> {
        let path = path.as_ref();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let fresh = match options.open(path) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(SyncError::CheckpointStorage(error.to_string()).into()),
        };
        Ok(Self::from_backend(
            RedbBackend::open(path)?,
            space_id,
            writers,
            encryption,
            domain,
            fresh,
        ))
    }
}

impl<B, D> SyncStore<B, D>
where
    B: Backend + Clone,
    D: SyncDomain,
{
    pub fn space_id(&self) -> [u8; 32] {
        self.policy.space_id
    }

    pub fn encryption_profile(&self) -> EncryptionProfile {
        self.policy.encryption
    }

    /// The domain this store was built with.
    pub fn domain(&self) -> &D {
        &self.domain
    }

    /// Writers currently admitted by this replica's materialized policy.
    pub fn admitted_writers(&self) -> Vec<[u8; 32]> {
        self.policy
            .writers
            .read()
            .map(|writers| writers.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Admit one newly paired writer without rebuilding the active LogSync
    /// session. Returns whether the materialized policy changed.
    pub fn admit_writer(&self, writer: [u8; 32]) -> bool {
        self.policy
            .writers
            .write()
            .map(|mut writers| writers.insert(writer))
            .unwrap_or(false)
    }

    /// Revoke one paired writer for future operation admission.
    pub fn deny_writer(&self, writer: &[u8; 32]) -> bool {
        self.policy
            .writers
            .write()
            .map(|mut writers| writers.remove(writer))
            .unwrap_or(false)
    }

    /// Replace communal writer admission from a newly materialized Gemot view.
    pub fn replace_admitted_writers(&self, writers: impl IntoIterator<Item = [u8; 32]>) -> bool {
        let next = writers.into_iter().collect::<BTreeSet<_>>();
        self.policy
            .writers
            .write()
            .map(|mut current| {
                if *current == next {
                    return false;
                }
                *current = next;
                true
            })
            .unwrap_or(false)
    }

    /// Seal, sign, admit, and store the next event in this device's log.
    pub async fn author_with_cipher(
        &self,
        signing_seed: &[u8; 32],
        cipher: SyncCipher<'_>,
        event: &D::Event,
    ) -> Result<Operation<SyncExt>, D::Error> {
        self.author_with_cipher_at(signing_seed, cipher, event, AssertedTime::Now)
            .await
    }

    pub async fn author_with_cipher_at(
        &self,
        signing_seed: &[u8; 32],
        cipher: SyncCipher<'_>,
        event: &D::Event,
        asserted_at: AssertedTime,
    ) -> Result<Operation<SyncExt>, D::Error> {
        let _gate = self.mutation_gate.lock().await;
        self.author_under_gate(signing_seed, cipher, event, asserted_at)
            .await
    }

    pub(crate) async fn author_under_gate(
        &self,
        signing_seed: &[u8; 32],
        cipher: SyncCipher<'_>,
        event: &D::Event,
        asserted_at: AssertedTime,
    ) -> Result<Operation<SyncExt>, D::Error> {
        self.require_cipher(cipher)?;
        let signing_key = SigningKey::from_bytes(signing_seed);
        let author = signing_key.verifying_key();
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let parents = observed_frontier(&entries)?;
        let causal = CausalIndex::new(&entries);
        let closed = causal_projection(&entries)?;
        self.domain.validate_local(LocalCheck {
            event,
            author: *author.as_bytes(),
            records: &records,
            order: &closed.order,
            cipher,
            causal: &causal,
            parents: &parents,
            space_id: self.policy.space_id,
        })?;
        let (seq_num, backlink) = author_head(&entries, *author.as_bytes(), &LOG_ID)?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(event).map_err(|error| SyncError::Payload(error.to_string()))?,
        );
        // The AEAD AAD stays at v1: the asserted time is bound by the ed25519
        // signature over the CBOR header and by the operation hash, and the
        // body through payload_hash. Changing this would orphan sealed bodies.
        let aad = operation_aad::<D>(self.policy.space_id, author.as_bytes(), seq_num);
        let ciphertext = seal_event(cipher, &aad, plaintext.as_slice())?;
        let body = Body::from_bytes(&ciphertext);
        // p2panda 0.7.1 made the header's CBOR cache, size and digest private
        // and folded signing into the builder: `build` encodes, signs and
        // caches the digest in one step, so the struct-literal + `sign` pair
        // has no equivalent. The builder derives verifying_key from the
        // signing key -- the same value `author` already held.
        let header = Header::builder()
            .body(&ciphertext)
            .seq_num(seq_num)
            .backlink(backlink.map(Hash::from))
            .build(
                &signing_key,
                SyncExt {
                    space_id: self.policy.space_id,
                    encryption: self.policy.encryption,
                    parents,
                    asserted_at_ms: asserted_at.resolve(),
                },
            );
        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        self.accept_under_gate(&operation).await?;
        Ok(operation)
    }

    pub(crate) fn require_cipher(&self, cipher: SyncCipher<'_>) -> Result<(), D::Error> {
        if cipher.profile() != self.policy.encryption {
            return Err(SyncError::WrongEncryptionProfile.into());
        }
        Ok(())
    }

    pub(crate) fn require_admitted_writer(&self, writer: [u8; 32]) -> Result<(), D::Error> {
        if self
            .policy
            .writers
            .read()
            .is_ok_and(|writers| writers.contains(&writer))
        {
            return Ok(());
        }
        Err(SyncError::Payload(D::VOCABULARY.writer_refusal.into()).into())
    }

    /// The `accept` closure target used by Stickleback.
    pub async fn accept(&self, operation: &Operation<SyncExt>) -> Result<bool, D::Error> {
        let _gate = self.mutation_gate.lock().await;
        self.accept_under_gate(operation).await
    }

    pub(crate) async fn accept_under_gate(
        &self,
        operation: &Operation<SyncExt>,
    ) -> Result<bool, D::Error> {
        let processor = OperationProcessor::new(self.store.clone(), self.policy.clone());
        Ok(processor.process(operation).await?.inserted())
    }

    pub(crate) async fn load_operations(&self) -> Result<Vec<StoredOperation>, D::Error> {
        let logs: BTreeMap<VerifyingKey, Vec<u64>> = self
            .store
            .resolve(&Topic::from(self.policy.space_id))
            .await?;
        let mut records = Vec::new();
        for (author, mut log_ids) in logs {
            log_ids.sort_unstable();
            log_ids.dedup();
            for log_id in log_ids {
                let Some(entries) = self
                    .store
                    .get_log_entries(&author, &log_id, None, None)
                    .await?
                else {
                    continue;
                };
                for (operation, _) in entries {
                    records.push(StoredOperation { operation, log_id });
                }
            }
        }
        Ok(records)
    }

    pub fn sync_store(&self) -> MunimentStore<B, SyncExt> {
        self.store.clone()
    }

    // Local sealed storage: records the store keeps beside the log, sealed
    // under a vault independent of the sync key.

    pub(crate) fn seal_local(
        &self,
        vault: &dyn SyncSealer,
        kind: RecordKind,
        bytes: &[u8],
    ) -> Result<Vec<u8>, SyncError> {
        storage::seal::<D>(
            vault,
            self.space_id(),
            self.encryption_profile(),
            kind,
            bytes,
        )
    }

    pub(crate) fn open_local(
        &self,
        vault: &dyn SyncSealer,
        kind: RecordKind,
        bytes: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SyncError> {
        storage::open::<D>(
            vault,
            self.space_id(),
            self.encryption_profile(),
            kind,
            bytes,
        )
    }

    pub(crate) fn storage_marker_write(&self, vault: &dyn SyncSealer) -> Result<WriteOp, SyncError> {
        Ok(WriteOp::Put {
            key: storage::marker_key::<D>(self.space_id()),
            value: self.seal_local(
                vault,
                RecordKind::StorageMarker,
                D::VOCABULARY.storage_marker,
            )?,
        })
    }

    pub(crate) fn keyring_key(&self) -> String {
        storage::record_key::<D>("commons-keyring", self.space_id())
    }

    pub(crate) async fn require_clean_storage(
        &self,
        vault: &dyn SyncSealer,
    ) -> Result<(), D::Error> {
        if vault.is_locked() {
            return Err(SyncError::CheckpointStorage("local storage vault is locked".into()).into());
        }
        match self
            .store
            .backend()
            .get(&storage::marker_key::<D>(self.space_id()))
            .await?
        {
            Some(bytes) => {
                let marker = self.open_local(vault, RecordKind::StorageMarker, &bytes)?;
                if marker.as_slice() != D::VOCABULARY.storage_marker {
                    return Err(
                        SyncError::CheckpointStorage("invalid clean storage marker".into()).into(),
                    );
                }
                Ok(())
            },
            None if self.storage_fresh => Ok(()),
            None => Err(SyncError::CheckpointMigrationRequired.into()),
        }
    }

    pub(crate) async fn validate_existing_keyring(
        &self,
        vault: &dyn SyncSealer,
    ) -> Result<(), D::Error> {
        if let Some(bytes) = self.store.backend().get(&self.keyring_key()).await? {
            let plaintext = self.open_local(vault, RecordKind::CommunalKeyring, &bytes)?;
            DataKeyring::from_bytes(&plaintext)?;
        }
        Ok(())
    }

    pub(crate) async fn validate_keyring_matches(
        &self,
        vault: &dyn SyncSealer,
        supplied: &DataKeyring,
    ) -> Result<(), D::Error> {
        self.require_clean_storage(vault).await?;
        if let Some(bytes) = self.store.backend().get(&self.keyring_key()).await? {
            let plaintext = self.open_local(vault, RecordKind::CommunalKeyring, &bytes)?;
            let persisted = DataKeyring::from_bytes(&plaintext)?;
            if !storage::keyrings_match(supplied, &persisted)? {
                return Err(SyncError::CheckpointStorage(
                    "membership keys differ from persisted retention state; use the explicit membership handoff".into(),
                ).into());
            }
        }
        Ok(())
    }

    /// Persist an authorized membership handoff under independent local custody.
    pub async fn persist_communal_keyring(
        &self,
        keys: &DataKeyring,
        storage_vault: &dyn SyncSealer,
    ) -> Result<(), D::Error> {
        let _gate = self.mutation_gate.lock().await;
        if self.encryption_profile() != EncryptionProfile::CommonsDataV1 {
            return Err(SyncError::WrongEncryptionProfile.into());
        }
        self.require_clean_storage(storage_vault).await?;
        self.validate_existing_keyring(storage_vault).await?;
        let plaintext = Zeroizing::new(keys.to_bytes()?);
        let sealed = self.seal_local(storage_vault, RecordKind::CommunalKeyring, &plaintext)?;
        self.store
            .backend()
            .apply(&[
                self.storage_marker_write(storage_vault)?,
                WriteOp::Put {
                    key: self.keyring_key(),
                    value: sealed,
                },
            ])
            .await?;
        Ok(())
    }
}

#[cfg(feature = "peers")]
impl<B, D> SyncStore<B, D>
where
    B: Backend + Clone + Send + Sync + 'static,
    D: SyncDomain,
{
    /// Join the real p2panda LogSync lane with the store's admission closure.
    pub async fn join(
        &self,
        endpoint: murm_sync::Endpoint,
        gossip: murm_sync::Gossip,
    ) -> Result<murm_sync::JoinedSpace<SyncExt>, stickleback::JoinError> {
        let accept_store = self.clone();
        murm_sync::JoinedSpace::join::<_, u64, _, _>(
            // Scoped to kind AND space: a persona's vault space and any other
            // space on one endpoint would otherwise share a protocol id.
            stickleback::lane_id(D::VOCABULARY.lane_kind, self.policy.space_id),
            self.sync_store(),
            endpoint,
            gossip,
            self.policy.space_id,
            move |operation: Operation<SyncExt>| {
                let store = accept_store.clone();
                async move { matches!(store.accept(&operation).await, Ok(true)) }
            },
        )
        .await
    }
}
