// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Resident-owned retention of immutable catalog file observations.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use p2panda_core::SigningKey;

use crate::{KnotEncryptionProfile, KnotFileRevisionV1, KnotSyncCipher, KnotSyncError};

use super::{KnotResidentSource, VaultSyncAuthority};

/// Authority for which catalog documents a capture port may retain.
pub struct KnotCaptureGrant {
    document_ids: BTreeSet<String>,
    max_source_bytes: u64,
}

impl KnotCaptureGrant {
    pub fn new(document_ids: impl IntoIterator<Item = String>, max_source_bytes: u64) -> Self {
        Self {
            document_ids: document_ids.into_iter().collect(),
            max_source_bytes,
        }
    }
}

/// Destination identity and encryption contract for one resident sync space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotCaptureDestination {
    pub space_id: [u8; 32],
    pub writer: [u8; 32],
    pub encryption: KnotEncryptionProfile,
}

/// Errors returned while authorizing, preparing, or retaining a capture.
#[derive(Debug, thiserror::Error)]
pub enum KnotCaptureError {
    #[error("capture grant has been revoked")]
    Revoked,
    #[error("capture document is not authorized by the grant")]
    DocumentNotGranted,
    #[error("capture source exceeds the grant byte limit")]
    SourceTooLarge,
    #[error("capture revision is invalid: {0}")]
    InvalidRevision(String),
    #[error("capture requires an unlocked vault")]
    VaultLocked,
    #[error("capture requires a configured sync authority")]
    SyncUnavailable,
    #[error("capture requires available Commons data keys")]
    KeysUnavailable,
    #[error("capture writer is not currently admitted")]
    WriterNotAdmitted,
    #[error("capture destination changed while the capture was prepared")]
    DestinationChanged,
    #[error("capture sync operation failed: {0}")]
    Sync(#[from] KnotSyncError),
}

/// Cloneable resident-owned capture authority.
#[derive(Clone)]
pub struct KnotFileCapturePort {
    resident: KnotResidentSource,
    grant: Arc<Mutex<Option<KnotCaptureGrant>>>,
}

/// An immutable, authorized capture awaiting retention.
pub struct KnotPreparedFileCaptureV1 {
    destination: KnotCaptureDestination,
    revision: KnotFileRevisionV1,
}

/// Receipt for one retained file observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotCaptureReceipt {
    pub destination: KnotCaptureDestination,
    pub document_id: String,
    pub operation: [u8; 32],
    pub already_retained: bool,
}

impl KnotResidentSource {
    /// Issue a resident-owned port whose authority can be revoked in place.
    pub fn capture_retention(
        &self,
        grant: KnotCaptureGrant,
    ) -> Result<KnotFileCapturePort, KnotCaptureError> {
        destination_for_state(&self.state())?;
        Ok(KnotFileCapturePort {
            resident: self.clone(),
            grant: Arc::new(Mutex::new(Some(grant))),
        })
    }
}

impl KnotFileCapturePort {
    /// Return the currently live destination after checking resident authority.
    pub fn destination(&self) -> Result<KnotCaptureDestination, KnotCaptureError> {
        let grant = self
            .grant
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if grant.is_none() {
            return Err(KnotCaptureError::Revoked);
        }
        let state = self.resident.state();
        destination_for_state(&state)
    }

    /// Authorize and freeze one bounded file revision in memory.
    pub fn prepare(
        &self,
        revision: KnotFileRevisionV1,
    ) -> Result<KnotPreparedFileCaptureV1, KnotCaptureError> {
        revision
            .validate()
            .map_err(KnotCaptureError::InvalidRevision)?;
        let grant = self
            .grant
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let grant = grant.as_ref().ok_or(KnotCaptureError::Revoked)?;
        if !grant.document_ids.contains(&revision.document_id) {
            return Err(KnotCaptureError::DocumentNotGranted);
        }
        if revision.body.len() as u64 > grant.max_source_bytes {
            return Err(KnotCaptureError::SourceTooLarge);
        }
        let state = self.resident.state();
        let destination = destination_for_state(&state)?;
        Ok(KnotPreparedFileCaptureV1 {
            destination,
            revision,
        })
    }

    /// Revoke every clone of this port.
    pub fn revoke(&self) {
        let mut grant = self
            .grant
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        grant.take();
    }

    /// Retain a prepared revision as a signed capture event.
    ///
    /// The grant mutex is acquired before the resident state mutex and both
    /// remain held through lookup and authoring. This serializes capture ports
    /// sharing a resident; callers that author directly through a cloned sync
    /// store remain outside this resident-owned serialization boundary.
    /// Exact document metadata and body bytes are used for retry lookup, so a
    /// matching same-writer operation is returned across reopen.
    pub fn retain(
        &self,
        prepared: &KnotPreparedFileCaptureV1,
    ) -> Result<KnotCaptureReceipt, KnotCaptureError> {
        let grant = self
            .grant
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let grant = grant.as_ref().ok_or(KnotCaptureError::Revoked)?;
        if !grant.document_ids.contains(&prepared.revision.document_id) {
            return Err(KnotCaptureError::DocumentNotGranted);
        }
        if prepared.revision.body.len() as u64 > grant.max_source_bytes {
            return Err(KnotCaptureError::SourceTooLarge);
        }
        let state = self.resident.state();
        let destination = destination_for_state(&state)?;
        if destination != prepared.destination {
            return Err(KnotCaptureError::DestinationChanged);
        }
        let (store, seed, cipher) = match state.sync.as_ref() {
            Some(VaultSyncAuthority::Personal {
                store,
                signing_seed,
            }) => (
                store,
                **signing_seed,
                KnotSyncCipher::Personal(&state.vault),
            ),
            Some(VaultSyncAuthority::Commons {
                store,
                signing_seed,
                keys,
            }) => (store, **signing_seed, KnotSyncCipher::CommonsData(keys)),
            None => return Err(KnotCaptureError::SyncUnavailable),
        };
        let existing = pollster::block_on(store.find_file_revision_with_cipher(
            cipher,
            destination.writer,
            &prepared.revision,
        ))?;
        let (operation, already_retained) = match existing {
            Some(operation) => (operation, true),
            None => {
                let event = crate::KnotSyncEvent::CaptureFileRevision(prepared.revision.clone());
                let operation = pollster::block_on(store.author_with_cipher(seed, cipher, &event))?;
                (*operation.hash.as_bytes(), false)
            },
        };
        Ok(KnotCaptureReceipt {
            destination,
            document_id: prepared.revision.document_id.clone(),
            operation,
            already_retained,
        })
    }
}

impl KnotPreparedFileCaptureV1 {
    pub fn destination(&self) -> &KnotCaptureDestination {
        &self.destination
    }

    pub fn revision(&self) -> &KnotFileRevisionV1 {
        &self.revision
    }
}

fn destination_for_state(
    state: &super::VaultSource,
) -> Result<KnotCaptureDestination, KnotCaptureError> {
    if state.vault.is_locked() {
        return Err(KnotCaptureError::VaultLocked);
    }
    let (store, seed) = match state.sync.as_ref() {
        Some(VaultSyncAuthority::Personal {
            store,
            signing_seed,
        }) => (store, **signing_seed),
        Some(VaultSyncAuthority::Commons {
            store,
            signing_seed,
            keys: _,
        }) => (store, **signing_seed),
        None => return Err(KnotCaptureError::SyncUnavailable),
    };
    if let Some(VaultSyncAuthority::Commons { keys, .. }) = state.sync.as_ref()
        && keys.current_epoch().is_none()
    {
        return Err(KnotCaptureError::KeysUnavailable);
    }
    let writer = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
    if !store
        .admitted_writers()
        .into_iter()
        .any(|candidate| candidate == writer)
    {
        return Err(KnotCaptureError::WriterNotAdmitted);
    }
    Ok(KnotCaptureDestination {
        space_id: store.space_id(),
        writer,
        encryption: store.encryption_profile(),
    })
}
