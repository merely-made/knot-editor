// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Host-issued retention capabilities for an immutable reviewed file revision.
//! This contract neither opens storage nor selects or creates an identity.

use knot_file_catalog::KnotFileRevisionV1;

/// Display identity bound to a capability by its owning host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotPersonaDisplayV1 {
    pub stable_id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotRetainEncryptionV1 {
    PersonalVaultV1,
    CommonsDataV1,
}

/// Immutable selection snapshot, not a promise of continuing authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotRetainTargetV1 {
    pub persona: KnotPersonaDisplayV1,
    pub space_id: [u8; 32],
    pub writer: [u8; 32],
    pub encryption: KnotRetainEncryptionV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotRetainReceiptV1 {
    pub target: KnotRetainTargetV1,
    pub document_id: String,
    pub operation: [u8; 32],
    pub already_retained: bool,
}

/// User-safe failure text. An error does not imply a write was rolled back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotRetainError(pub String);

impl std::fmt::Display for KnotRetainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KnotRetainError {}

pub trait KnotRetainPort: Send + Sync {
    /// Return the host-issued snapshot without I/O or waiting on storage locks.
    fn target(&self) -> &KnotRetainTargetV1;

    /// Blocking operation: run on a worker. Recheck live authorization and the
    /// expected destination before retaining precisely these reviewed bytes.
    /// Implementations must not reopen the source file or create another owner.
    fn retain_reviewed(
        &self,
        expected_target: &KnotRetainTargetV1,
        revision: KnotFileRevisionV1,
    ) -> Result<KnotRetainReceiptV1, KnotRetainError>;
}
