// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Local storage protection is independent of replicated group data epochs.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use super::{EncryptionProfile, SyncDomain, SyncError, SyncSealer};

const VERSION: u16 = 1;

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
    profile: EncryptionProfile,
    kind: RecordKind,
    ciphertext: Vec<u8>,
}

fn aad<D: SyncDomain>(space: [u8; 32], profile: EncryptionProfile, kind: RecordKind) -> Vec<u8> {
    let mut bytes = D::VOCABULARY.storage_format.as_bytes().to_vec();
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&space);
    bytes.push(match profile {
        EncryptionProfile::PersonalVaultV1 => 1,
        EncryptionProfile::CommonsDataV1 => 2,
    });
    bytes.push(match kind {
        RecordKind::Checkpoint => 1,
        RecordKind::CommunalKeyring => 2,
        RecordKind::StorageMarker => 3,
    });
    bytes
}

pub(crate) fn seal<D: SyncDomain>(
    vault: &dyn SyncSealer,
    space_id: [u8; 32],
    profile: EncryptionProfile,
    kind: RecordKind,
    plaintext: &[u8],
) -> Result<Vec<u8>, SyncError> {
    let ciphertext = vault
        .seal_local_storage(&aad::<D>(space_id, profile, kind), plaintext)
        .map_err(SyncError::CheckpointStorage)?;
    serde_json::to_vec(&Envelope {
        format: D::VOCABULARY.storage_format.into(),
        version: VERSION,
        space_id,
        profile,
        kind,
        ciphertext,
    })
    .map_err(|error| SyncError::CheckpointStorage(error.to_string()))
}

pub(crate) fn open<D: SyncDomain>(
    vault: &dyn SyncSealer,
    space_id: [u8; 32],
    profile: EncryptionProfile,
    kind: RecordKind,
    bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SyncError> {
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| SyncError::CheckpointMigrationRequired)?;
    if envelope.format != D::VOCABULARY.storage_format || envelope.version != VERSION {
        return Err(SyncError::CheckpointStorage(
            "unsupported local storage envelope".into(),
        ));
    }
    if envelope.space_id != space_id || envelope.profile != profile || envelope.kind != kind {
        return Err(SyncError::CheckpointStorage(
            "local storage envelope context does not match this record".into(),
        ));
    }
    vault
        .unseal_local_storage(&aad::<D>(space_id, profile, kind), &envelope.ciphertext)
        .map(Zeroizing::new)
        .map_err(SyncError::CheckpointStorage)
}

/// The key of one of the store's own records: `<prefix>/<name>/<space hex>`.
pub(crate) fn record_key<D: SyncDomain>(name: &str, space: [u8; 32]) -> String {
    let hex: String = space.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{}/{name}/{hex}", D::VOCABULARY.key_prefix)
}

pub(crate) fn marker_key<D: SyncDomain>(space: [u8; 32]) -> String {
    record_key::<D>("sealed-storage", space)
}

/// SecretBundle's serialized sequence comes from a HashMap. Compare its set
/// without losing timestamps, chronology, or any other serialized state.
pub(crate) fn keyrings_match(
    a: &stickleback::DataKeyring,
    b: &stickleback::DataKeyring,
) -> Result<bool, SyncError> {
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
    fn view(keys: &stickleback::DataKeyring) -> Result<Scratch, SyncError> {
        let mut value = Scratch(serde_json::to_value(keys).map_err(|_| {
            SyncError::CheckpointStorage("could not compare keyring state".into())
        })?);
        let secrets = value
            .0
            .get_mut("secrets")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| {
                SyncError::CheckpointStorage("unsupported keyring representation".into())
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
