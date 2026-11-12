// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The signed header, the cipher and the sealed event body.

use p2panda_core::Operation;
use p2panda_core::cbor::{decode_cbor_strict, encode_cbor};
use serde::{Deserialize, Serialize};
use stickleback::{CausalEntry, DataKeyring, GroupCiphertext};
use zeroize::Zeroizing;

use super::{SyncDomain, SyncError, SyncSealer};

/// The signed encryption contract for one space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionProfile {
    /// Personal device sync derives a key from the local vault root.
    #[default]
    PersonalVaultV1,
    /// Commons documents use the group's retained data-encryption epochs.
    CommonsDataV1,
}

/// Signed addressing extension for one space.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncExt {
    pub space_id: [u8; 32],
    #[serde(default)]
    pub encryption: EncryptionProfile,
    /// Exact per-author frontier observed before this event was authored.
    #[serde(default)]
    pub parents: Vec<[u8; 32]>,
    /// Author-asserted wall clock, set at authoring and overridable before
    /// signing. Declared last and skipped when absent so an operation authored
    /// before this field reserializes to its original CBOR and keeps its hash.
    /// Inside the signed header bytes; informative only, it never orders a fold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_at_ms: Option<u64>,
}

/// What wall clock an authoring call binds into the signed header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssertedTime {
    /// This machine's clock now, or nothing at all if the clock fails.
    #[default]
    Now,
    /// An explicit time, so a transcribed note can carry its real date.
    At(u64),
    /// Assert no time.
    None,
}

impl AssertedTime {
    pub(crate) fn resolve(self) -> Option<u64> {
        match self {
            // A failed clock asserts nothing; a time is never invented.
            Self::Now => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok()),
            Self::At(milliseconds) => Some(milliseconds),
            Self::None => None,
        }
    }
}

/// Encryption material used by a replica.
#[derive(Clone, Copy)]
pub enum SyncCipher<'a> {
    Personal(&'a dyn SyncSealer),
    CommonsData(&'a DataKeyring),
}

impl SyncCipher<'_> {
    pub(crate) fn profile(self) -> EncryptionProfile {
        match self {
            Self::Personal(_) => EncryptionProfile::PersonalVaultV1,
            Self::CommonsData(_) => EncryptionProfile::CommonsDataV1,
        }
    }
}

/// One stored operation and the log it was stored under.
#[derive(Clone)]
pub struct StoredOperation {
    pub operation: Operation<SyncExt>,
    pub log_id: u64,
}

pub(crate) fn causal_entries(records: &[StoredOperation]) -> Vec<CausalEntry<u64>> {
    records
        .iter()
        .map(|record| {
            CausalEntry::from_operation(
                &record.operation,
                record.log_id,
                record.operation.header.extensions.parents.clone(),
            )
        })
        .collect()
}

pub(crate) fn operation_aad<D: SyncDomain>(
    space_id: [u8; 32],
    author: &[u8; 32],
    seq_num: u32,
) -> Vec<u8> {
    let context = D::VOCABULARY.operation_aad;
    let mut aad = Vec::with_capacity(context.len() + 68);
    aad.extend_from_slice(context);
    aad.extend_from_slice(&space_id);
    aad.extend_from_slice(author);
    aad.extend_from_slice(&seq_num.to_le_bytes());
    aad
}

/// Seal an event's plaintext for an operation body.
///
/// # Errors
/// Fails when the cipher cannot seal.
pub fn seal_event(
    cipher: SyncCipher<'_>,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, SyncError> {
    match cipher {
        SyncCipher::Personal(vault) => vault
            .seal_sync_payload(aad, plaintext)
            .map_err(SyncError::Payload),
        SyncCipher::CommonsData(keys) => {
            let envelope = keys.seal_random(plaintext)?;
            encode_cbor(&envelope).map_err(|error| SyncError::Payload(error.to_string()))
        },
    }
}

/// Open and decode one operation's event.
///
/// # Errors
/// Fails when the body is absent, will not open under the cipher or does not
/// decode as the domain's event.
pub fn decode_event<D: SyncDomain>(
    cipher: SyncCipher<'_>,
    operation: &Operation<SyncExt>,
) -> Result<D::Event, SyncError> {
    let body = operation
        .body
        .as_ref()
        .ok_or_else(|| SyncError::Payload("operation body is absent".into()))?;
    let aad = operation_aad::<D>(
        operation.header.extensions.space_id,
        operation.header.verifying_key.as_bytes(),
        operation.header.seq_num,
    );
    let plaintext = Zeroizing::new(match cipher {
        SyncCipher::Personal(vault) => vault
            .unseal_sync_payload(&aad, &body.to_bytes())
            .map_err(SyncError::Payload)?,
        SyncCipher::CommonsData(keys) => {
            let envelope: GroupCiphertext = decode_cbor_strict(body.to_bytes().as_slice())
                .map_err(|error| SyncError::Payload(error.to_string()))?;
            keys.open(&envelope)?
        },
    });
    serde_json::from_slice(plaintext.as_slice())
        .map_err(|error| SyncError::Payload(error.to_string()))
}
