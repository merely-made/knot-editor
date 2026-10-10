// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Startup-unlocked personal Knot authority.
//!
//! This is the production seam between djinn's custody route and Knot's
//! sealed, signed document store. Knot never opens a wallet or a vault
//! (dramatis repo plan, D5): djinn derives the persona vault's keys from the
//! persona epoch and releases only those ([`PersonalVaultKeys`], D11). With
//! djinn absent or Locked the persona is pending; Knot holds no fallback key
//! and writes nothing (D12).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use graphshell::native::app_admission::AppId;
use graphshell::native::custody::{EpochKeyRequest, ReleasedEpochKey};
use graphshell::native::custody_client::{
    BlockingCustodyClient, CustodyClient, CustodyClientError,
};
use p2panda_core::SigningKey;
use personae::{Ed25519Keypair, PersonaId};
use zeroize::Zeroizing;

use crate::{
    KnotEndpoint, KnotPublishSource, KnotResidentSource, KnotSyncEvent, KnotSyncFileStore,
    KnotVault, KnotWriteGrant, VaultDocument,
};

/// The epoch derivation of a persona's Knot vault key. djinn's release
/// policy names it.
pub const VAULT_KEY_CONTEXT: &str = "mere.knot.persona-vault.root.v1";
/// The epoch derivation of a Knot writer: device-bound for the writer this
/// device authors with, unbound for the legacy writer, of which only the
/// public key is released.
pub const SIGNING_KEY_CONTEXT: &str = "mere.knot.persona-vault.writer.v1";
/// The application Knot calls djinn as.
pub const KNOT_APP_ID: &str = "knot-editor";
const SPACE_ID_CONTEXT: &str = "mere.knot.persona-vault.space.v1";
const KNOT_VAULT_DIR: &str = "vault";
const KNOT_SYNC_FILE: &str = "knot/sync.redb";

/// The keys one persona's Knot vault runs on, as djinn released them.
///
/// - the vault key, the same on every device of the persona;
/// - this device's writer seed, mixed with its device root, so two devices
///   are two writers and two nodes;
/// - the public key of the pre-device-scoped writer, admitted so operations
///   written before the device derivation existed still fold.
pub struct PersonalVaultKeys {
    vault_key: Zeroizing<[u8; 32]>,
    signing_seed: Zeroizing<[u8; 32]>,
    legacy_writer: [u8; 32],
}

impl PersonalVaultKeys {
    /// What Knot asks djinn to derive, in order.
    pub fn requests() -> Vec<EpochKeyRequest> {
        vec![
            EpochKeyRequest::secret(VAULT_KEY_CONTEXT, false),
            EpochKeyRequest::secret(SIGNING_KEY_CONTEXT, true),
            EpochKeyRequest::public(SIGNING_KEY_CONTEXT, false),
        ]
    }

    /// Take the keys out of a release answering [`Self::requests`].
    pub fn from_released(keys: &[ReleasedEpochKey]) -> Result<Self, String> {
        let requests = Self::requests();
        let [vault, signing, legacy] = [&requests[0], &requests[1], &requests[2]];
        let find = |request: &EpochKeyRequest| {
            keys.iter()
                .find(|key| key.request == *request)
                .map(|key| key.key)
                .ok_or_else(|| format!("djinn's release lacks {}", request.context))
        };
        Ok(Self {
            vault_key: Zeroizing::new(find(vault)?),
            signing_seed: Zeroizing::new(find(signing)?),
            legacy_writer: find(legacy)?,
        })
    }

    /// Ask djinn for `persona`'s keys, blocking. `device_label` lets djinn
    /// mint this device's identity if it has none; absent, a device with no
    /// identity is refused. Never call this from inside a tokio runtime.
    pub fn from_djinn(persona: PersonaId, device_label: Option<&str>) -> Result<Self, String> {
        let mut client =
            BlockingCustodyClient::open(AppId::new(KNOT_APP_ID)).map_err(custody_error)?;
        let keys = client
            .release_epoch_keys(
                *persona.as_uuid(),
                device_label.map(str::to_owned),
                Self::requests(),
            )
            .map_err(custody_error)?;
        Self::from_released(&keys)
    }

    /// [`Self::from_djinn`], from async code.
    pub async fn from_djinn_async(
        persona: PersonaId,
        device_label: Option<&str>,
    ) -> Result<Self, String> {
        let mut client = CustodyClient::open(AppId::new(KNOT_APP_ID))
            .await
            .map_err(custody_error)?;
        let keys = client
            .release_epoch_keys(
                *persona.as_uuid(),
                device_label.map(str::to_owned),
                Self::requests(),
            )
            .await
            .map_err(custody_error)?;
        Self::from_released(&keys)
    }

    /// This device's writer key, and so also its transport node id: what the
    /// other devices must admit before its operations will fold. Deriving it
    /// opens no Knot store, so pairing tools run beside a resident.
    pub fn writer(&self) -> [u8; 32] {
        *SigningKey::from_bytes(&self.signing_seed)
            .verifying_key()
            .as_bytes()
    }

    fn writers(&self, admitted: impl IntoIterator<Item = [u8; 32]>) -> Vec<[u8; 32]> {
        let mut writers = vec![self.writer(), self.legacy_writer];
        writers.extend(admitted);
        writers.sort_unstable();
        writers.dedup();
        writers
    }
}

/// A custody failure as Knot reports it: pending when djinn is absent or
/// Locked (D12), a failure otherwise.
fn custody_error(error: CustodyClientError) -> String {
    match error.is_pending() {
        true => format!("the Knot persona is pending until djinn is running and unlocked: {error}"),
        false => format!("djinn did not release the Knot persona's keys: {error}"),
    }
}

/// Unlocked authority held only long enough to seed or launch one endpoint.
pub struct StartupUnlockedPersonalVault {
    vault: KnotVault,
    store: KnotSyncFileStore,
    signing_seed: Zeroizing<[u8; 32]>,
}

impl StartupUnlockedPersonalVault {
    /// Attach an explicitly named existing personal mere without creating an
    /// identity/store, migrating documents, or starting network replication.
    /// The store is checked before djinn is asked, so a missing mere costs no
    /// release. Another resident owning the operation store is an error,
    /// never a second owner.
    pub fn open_existing(
        data_root: impl AsRef<Path>,
        persona: PersonaId,
        keys: impl FnOnce() -> Result<PersonalVaultKeys, String>,
    ) -> Result<Self, String> {
        let data_root = data_root.as_ref();
        let vault_root = persona_vault_root(data_root, persona);
        let store_path = vault_root.join(KNOT_SYNC_FILE);
        if !store_path.is_file() {
            return Err("the selected persona has no existing Knot mere operation store".into());
        }
        let keys = keys()?;
        let settings = crate::KnotSettings::load(&crate::knot_settings_path(data_root, persona))
            .map_err(|error| error.to_string())?;
        let admitted = settings
            .sync
            .map(|sync| sync.paired_writer_keys())
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        let writers = keys.writers(admitted);
        let space_id = blake3::derive_key(SPACE_ID_CONTEXT, persona.as_uuid().as_bytes());
        let store = KnotSyncFileStore::open(&store_path, space_id, writers).map_err(|error| {
            format!("could not attach Knot mere; another resident may own it: {error}")
        })?;
        let vault = KnotVault::open(&vault_root, *keys.vault_key)?;
        let projection = pollster::block_on(store.projection(&vault))
            .map_err(|error| format!("could not inspect existing Knot history: {error}"))?;
        if vault
            .documents()
            .any(|document| !projection.documents.contains(document))
        {
            return Err("existing vault material differs from recorded history; reconcile it through its owner before attaching composition retention".into());
        }
        Ok(Self {
            vault,
            store,
            signing_seed: keys.signing_seed,
        })
    }

    /// Open this persona's sealed vault plus signed operation store with the
    /// keys djinn released.
    ///
    /// The writer is device-distinct because djinn mixes this machine's
    /// device root into it, and carrying the persona epoch to a second device
    /// is why that matters: the vault key and space must be identical across
    /// devices so both can decrypt the same space, but the writer must not
    /// be, because its public half is also the node identity. Two devices
    /// deriving one writer would be one node on the network and one author in
    /// a per-author log, and neither works.
    ///
    /// `admitted` carries the other devices' writer keys, which is how a
    /// second device's operations pass admission.
    pub fn open(
        data_root: impl AsRef<Path>,
        persona: PersonaId,
        keys: PersonalVaultKeys,
        admitted: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, String> {
        let data_root = data_root.as_ref();

        let vault_root = persona_vault_root(data_root, persona);
        fs::create_dir_all(vault_root.join("knot"))
            .map_err(|error| format!("could not create Knot persona vault: {error}"))?;
        let vault = KnotVault::open(&vault_root, *keys.vault_key)?;
        let space_id = blake3::derive_key(SPACE_ID_CONTEXT, persona.as_uuid().as_bytes());
        let writers = keys.writers(admitted);
        let store = KnotSyncFileStore::open(vault_root.join(KNOT_SYNC_FILE), space_id, writers)
            .map_err(|error| {
                format!(
                    "could not open Knot persona sync store; another resident may already own this persona: {error}"
                )
            })?;

        let authority = Self {
            vault,
            store,
            signing_seed: keys.signing_seed,
        };
        authority.migrate_unsynced_vault()?;
        Ok(authority)
    }

    /// Author one seed/import document through the same signed event path Save
    /// uses. This is an endpoint-owned setup seam, not a cleartext file write.
    pub fn author_document(&self, document: VaultDocument) -> Result<(), String> {
        pollster::block_on(self.store.author(
            &self.signing_seed,
            &self.vault,
            &KnotSyncEvent::Put(document),
        ))
        .map(|_| ())
        .map_err(|error| format!("could not author Knot persona document: {error}"))
    }

    /// This device's writer key, and so also its transport node id: what the
    /// other devices must admit before its operations will fold.
    pub fn writer(&self) -> [u8; 32] {
        *SigningKey::from_bytes(&self.signing_seed)
            .verifying_key()
            .as_bytes()
    }

    /// The signed operation store, for binding a transport to it.
    pub fn store(&self) -> &KnotSyncFileStore {
        &self.store
    }

    /// The seed this device signs and binds its transport with, lent: a copy
    /// taken into async code would outlive the unlock (Mere vault lock ruling 49).
    pub fn signing_seed(&self) -> &[u8; 32] {
        &self.signing_seed
    }

    /// Consume the recovered authority into a writable Graphshell endpoint.
    pub fn into_endpoint(self, grant: KnotWriteGrant) -> Result<KnotEndpoint, String> {
        Ok(self.into_resident_source()?.session(Some(grant)))
    }

    /// Consume the startup unlock into one cloneable resident source.
    pub fn into_resident_source(self) -> Result<KnotResidentSource, String> {
        KnotResidentSource::from_synced_vault(self.vault, self.store, &self.signing_seed)
    }

    /// Split one startup unlock between the mutable Graphshell editor endpoint
    /// and the independently retained read-only publishing host. Both handles
    /// retain the same synced source key, but only the endpoint receives the
    /// mutable vault handle and write grant.
    pub fn into_endpoint_and_publish_source(
        self,
        grant: KnotWriteGrant,
    ) -> Result<(KnotEndpoint, KnotPublishSource), String> {
        let (source, publish) = self.into_resident_source_and_publish_source()?;
        Ok((source.session(Some(grant)), publish))
    }

    /// Split one startup unlock between a cloneable authoring source and the
    /// independently retained read-only publishing source.
    pub fn into_resident_source_and_publish_source(
        self,
    ) -> Result<(KnotResidentSource, KnotPublishSource), String> {
        let publish_vault = Arc::new(self.vault.fork_read_handle()?);
        let publish_store = self.store.clone();
        let publish_identity = Ed25519Keypair::from_seed(*self.signing_seed);
        let source =
            KnotResidentSource::from_synced_vault(self.vault, self.store, &self.signing_seed)?;
        Ok((
            source,
            KnotPublishSource::from_unlocked(publish_identity, publish_store, publish_vault),
        ))
    }

    fn migrate_unsynced_vault(&self) -> Result<(), String> {
        let projection = pollster::block_on(self.store.projection(&self.vault))
            .map_err(|error| format!("could not inspect Knot persona sync store: {error}"))?;
        if !projection.documents.is_empty()
            || !projection.conflicts.is_empty()
            || !projection.pending.is_empty()
        {
            return Ok(());
        }
        let documents = self.vault.documents().cloned().collect::<Vec<_>>();
        for document in documents {
            self.author_document(document)?;
        }
        Ok(())
    }
}

pub fn persona_vault_root(data_root: &Path, persona: PersonaId) -> PathBuf {
    data_root
        .join(pandect::PERSONAS_DIR)
        .join(persona.as_uuid().to_string())
        .join(KNOT_VAULT_DIR)
}

/// A release built the way djinn derives one, for tests that have no djinn.
#[cfg(test)]
pub(crate) fn fixture_keys(epoch: u8, device: u8) -> PersonalVaultKeys {
    let device_root = *SigningKey::from_bytes(&[device; 32])
        .verifying_key()
        .as_bytes();
    let keys = PersonalVaultKeys::requests()
        .into_iter()
        .map(|request| {
            let mut material = vec![epoch; 32];
            if request.device_bound {
                material.extend_from_slice(&device_root);
            }
            let derived = blake3::derive_key(&request.context, &material);
            let key = match request.public_only {
                true => *SigningKey::from_bytes(&derived).verifying_key().as_bytes(),
                false => derived,
            };
            ReleasedEpochKey { request, key }
        })
        .collect::<Vec<_>>();
    PersonalVaultKeys::from_released(&keys).unwrap()
}

#[cfg(test)]
mod existing_attachment_tests {
    use super::*;

    #[test]
    fn missing_mere_refuses_before_asking_djinn_or_creating_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("absent");
        let error = StartupUnlockedPersonalVault::open_existing(&root, PersonaId::new(), || {
            Err("djinn was asked".into())
        })
        .err()
        .unwrap();
        assert!(error.contains("no existing Knot mere"), "{error}");
        assert!(!root.exists());
    }

    #[test]
    fn a_pending_persona_opens_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let persona = PersonaId::new();
        let store = persona_vault_root(temp.path(), persona).join(KNOT_SYNC_FILE);
        fs::create_dir_all(store.parent().unwrap()).unwrap();
        fs::write(&store, b"not opened without released keys").unwrap();
        let error = StartupUnlockedPersonalVault::open_existing(temp.path(), persona, || {
            Err("the Knot persona is pending".into())
        })
        .err()
        .unwrap();
        assert!(error.contains("pending"), "{error}");
        assert_eq!(
            fs::read(store).unwrap(),
            b"not opened without released keys"
        );
    }

    #[test]
    fn a_release_missing_a_key_is_refused() {
        let keys = PersonalVaultKeys::requests()
            .into_iter()
            .take(2)
            .map(|request| ReleasedEpochKey {
                request,
                key: [3; 32],
            })
            .collect::<Vec<_>>();
        assert!(PersonalVaultKeys::from_released(&keys).is_err());
    }
}

#[cfg(test)]
mod tests {
    use graphshell_endpoint::{ProjectionCatalog, ProjectionSource};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn released_keys_open_signed_sealed_persona_truth() {
        let root = tempdir().unwrap();
        let persona = PersonaId::new();

        let authority =
            StartupUnlockedPersonalVault::open(root.path(), persona, fixture_keys(1, 2), [])
                .unwrap();
        let duplicate = match StartupUnlockedPersonalVault::open(
            root.path(),
            persona,
            fixture_keys(1, 2),
            [],
        ) {
            Ok(_) => panic!("a second persona owner must be refused promptly"),
            Err(error) => error,
        };
        assert!(duplicate.contains("another resident may already own this persona"));
        assert_eq!(
            fixture_keys(1, 2).writer(),
            authority.writer(),
            "pairing facts derive beside the resident without reopening its stores",
        );
        assert_ne!(
            fixture_keys(1, 3).writer(),
            authority.writer(),
            "each device of a persona is its own writer",
        );
        authority
            .author_document(VaultDocument {
                id: "field-note".into(),
                title: "Field note".into(),
                body: b"# Private\n".to_vec(),
                media_type: "text/vnd.knot".into(),
            })
            .unwrap();
        let mut endpoint = authority.into_endpoint(KnotWriteGrant::new(4096)).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        assert!(!snapshot.scene.tables.items.is_empty());

        drop(endpoint);
        let reopened =
            StartupUnlockedPersonalVault::open(root.path(), persona, fixture_keys(1, 2), [])
                .unwrap()
                .into_endpoint(KnotWriteGrant::new(4096))
                .unwrap();
        drop(reopened);

        let clear = b"# Private\n";
        let mut found_cleartext = false;
        for entry in walk_files(root.path()) {
            let bytes = fs::read(entry).unwrap();
            found_cleartext |= bytes.windows(clear.len()).any(|window| window == clear);
        }
        assert!(!found_cleartext, "persona truth must remain opaque at rest");
    }

    fn walk_files(root: &Path) -> Vec<PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut files = Vec::new();
        while let Some(path) = pending.pop() {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        files
    }
}
