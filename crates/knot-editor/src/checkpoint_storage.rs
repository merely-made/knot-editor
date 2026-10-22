// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Local storage protection is independent of replicated group data epochs.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{KnotEncryptionProfile, KnotSyncError, KnotVault};

const FORMAT: &str = "mere.knot.sealed-local-record";
const VERSION: u16 = 1;
pub(crate) const MARKER: &[u8] = b"mere.knot.clean-checkpoint-database.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RecordKind {
    Checkpoint,
    CommunalKeyring,
    StorageMarker,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u16,
    space_id: [u8; 32],
    profile: KnotEncryptionProfile,
    kind: RecordKind,
    ciphertext: Vec<u8>,
}

fn aad(space: [u8; 32], profile: KnotEncryptionProfile, kind: RecordKind) -> Vec<u8> {
    let mut bytes = FORMAT.as_bytes().to_vec();
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&space);
    bytes.push(match profile {
        KnotEncryptionProfile::PersonalVaultV1 => 1,
        KnotEncryptionProfile::CommonsDataV1 => 2,
    });
    bytes.push(match kind {
        RecordKind::Checkpoint => 1,
        RecordKind::CommunalKeyring => 2,
        RecordKind::StorageMarker => 3,
    });
    bytes
}

pub(crate) fn seal(
    vault: &KnotVault,
    space_id: [u8; 32],
    profile: KnotEncryptionProfile,
    kind: RecordKind,
    plaintext: &[u8],
) -> Result<Vec<u8>, KnotSyncError> {
    let ciphertext = vault
        .seal_local_storage(&aad(space_id, profile, kind), plaintext)
        .map_err(KnotSyncError::CheckpointStorage)?;
    serde_json::to_vec(&Envelope {
        format: FORMAT.into(),
        version: VERSION,
        space_id,
        profile,
        kind,
        ciphertext,
    })
    .map_err(|error| KnotSyncError::CheckpointStorage(error.to_string()))
}

pub(crate) fn open(
    vault: &KnotVault,
    space_id: [u8; 32],
    profile: KnotEncryptionProfile,
    kind: RecordKind,
    bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>, KnotSyncError> {
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| KnotSyncError::CheckpointMigrationRequired)?;
    if envelope.format != FORMAT || envelope.version != VERSION {
        return Err(KnotSyncError::CheckpointStorage(
            "unsupported local storage envelope".into(),
        ));
    }
    if envelope.space_id != space_id || envelope.profile != profile || envelope.kind != kind {
        return Err(KnotSyncError::CheckpointStorage(
            "local storage envelope context does not match this record".into(),
        ));
    }
    vault
        .unseal_local_storage(&aad(space_id, profile, kind), &envelope.ciphertext)
        .map(Zeroizing::new)
        .map_err(KnotSyncError::CheckpointStorage)
}

pub(crate) fn marker_key(space: [u8; 32]) -> String {
    format!("knot-sync/sealed-storage/{}", crate::hex32(&space))
}

/// SecretBundle's serialized sequence comes from a HashMap. Compare its set
/// without losing timestamps, chronology, or any other serialized state.
pub(crate) fn keyrings_match(
    a: &stickleback::DataKeyring,
    b: &stickleback::DataKeyring,
) -> Result<bool, KnotSyncError> {
    struct Scratch(serde_json::Value);
    impl Drop for Scratch {
        fn drop(&mut self) {
            fn clear(value: &mut serde_json::Value) {
                match value {
                    serde_json::Value::String(text) => text.zeroize(),
                    serde_json::Value::Array(values) => values.iter_mut().for_each(clear),
                    serde_json::Value::Object(values) => values.values_mut().for_each(clear),
                    _ => *value = serde_json::Value::Null,
                }
            }
            clear(&mut self.0);
        }
    }
    fn view(keys: &stickleback::DataKeyring) -> Result<Scratch, KnotSyncError> {
        let mut value = Scratch(serde_json::to_value(keys).map_err(|_| {
            KnotSyncError::CheckpointStorage("could not compare keyring state".into())
        })?);
        let secrets = value
            .0
            .get_mut("secrets")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| {
                KnotSyncError::CheckpointStorage("unsupported keyring representation".into())
            })?;
        secrets.sort_unstable_by(|a, b| {
            let a = Zeroizing::new(serde_json::to_vec(a).expect("JSON Value serialization"));
            let b = Zeroizing::new(serde_json::to_vec(b).expect("JSON Value serialization"));
            a.as_slice().cmp(b.as_slice())
        });
        Ok(value)
    }
    Ok(view(a)?.0 == view(b)?.0)
}
