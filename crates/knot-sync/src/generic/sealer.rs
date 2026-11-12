// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The sealing a store asks of its owner.

/// What a personal store needs from the vault that holds its key.
///
/// The application's vault implements it; the store never sees the vault's documents or
/// search index, only these five operations.
pub trait SyncSealer: Sync {
    /// Whether the key has been dropped.
    fn is_locked(&self) -> bool;

    /// Seal one sync payload under the sync key.
    ///
    /// # Errors
    /// Fails when the vault is locked or sealing fails.
    fn seal_sync_payload(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String>;

    /// Open one sync payload.
    ///
    /// # Errors
    /// Fails when the vault is locked or the payload does not authenticate.
    fn unseal_sync_payload(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String>;

    /// Seal one local storage record under the storage key.
    ///
    /// # Errors
    /// Fails when the vault is locked or sealing fails.
    fn seal_local_storage(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String>;

    /// Open one local storage record.
    ///
    /// # Errors
    /// Fails when the vault is locked or the record does not authenticate.
    fn unseal_local_storage(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String>;
}

impl<T: SyncSealer + Send + ?Sized> SyncSealer for std::sync::Arc<T> {
    fn is_locked(&self) -> bool {
        (**self).is_locked()
    }

    fn seal_sync_payload(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        (**self).seal_sync_payload(aad, plaintext)
    }

    fn unseal_sync_payload(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        (**self).unseal_sync_payload(aad, ciphertext)
    }

    fn seal_local_storage(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        (**self).seal_local_storage(aad, plaintext)
    }

    fn unseal_local_storage(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        (**self).unseal_local_storage(aad, ciphertext)
    }
}
