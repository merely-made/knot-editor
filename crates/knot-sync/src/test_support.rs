// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A key-only sealer for this crate's tests, standing where Knot's vault stands.
//! It derives the sync and storage keys the way the vault does, so a store
//! written by the vault opens under it.

use std::path::PathBuf;

use personae::{seal_bytes, unseal_bytes};
use zeroize::Zeroizing;

use crate::{RecipeJson, RecipeValidator, SyncSealer};
use crate::composition::CollectionError;

const SYNC_KEY_CONTEXT: &str = "mere.knot.personal-vault-sync.v1";
const LOCAL_STORAGE_KEY_CONTEXT: &str = "mere.knot.local-storage-protection.v1";

pub struct KnotVault {
    sync_key: Option<Zeroizing<[u8; 32]>>,
}

impl KnotVault {
    /// The path is ignored: this sealer holds no documents.
    pub fn open(_root: impl Into<PathBuf>, key: [u8; 32]) -> Result<Self, String> {
        Ok(Self::from_key(key))
    }

    pub fn from_key(key: [u8; 32]) -> Self {
        Self {
            sync_key: Some(Zeroizing::new(blake3::derive_key(SYNC_KEY_CONTEXT, &key))),
        }
    }

    fn key(&self) -> Result<&Zeroizing<[u8; 32]>, String> {
        self.sync_key
            .as_ref()
            .ok_or_else(|| "Knot vault is locked".to_string())
    }

    fn storage_key(&self) -> Result<Zeroizing<[u8; 32]>, String> {
        Ok(Zeroizing::new(blake3::derive_key(
            LOCAL_STORAGE_KEY_CONTEXT,
            &**self.key()?,
        )))
    }
}

impl SyncSealer for KnotVault {
    fn is_locked(&self) -> bool {
        self.sync_key.is_none()
    }

    fn seal_sync_payload(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        seal_bytes(self.key()?, aad, plaintext)
            .map_err(|error| format!("could not seal Knot sync payload: {error}"))
    }

    fn unseal_sync_payload(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        unseal_bytes(self.key()?, aad, ciphertext)
            .map_err(|error| format!("could not unseal Knot sync payload: {error}"))
    }

    fn seal_local_storage(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        seal_bytes(&*self.storage_key()?, aad, plaintext)
            .map_err(|_| "could not seal Knot local storage".to_string())
    }

    fn unseal_local_storage(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        unseal_bytes(&*self.storage_key()?, aad, ciphertext)
            .map_err(|_| "could not authenticate Knot local storage".to_string())
    }
}

/// Accepts every recipe: this crate's tests do not judge scene material.
pub struct AcceptRecipes;

impl RecipeValidator for AcceptRecipes {
    fn validate(&self, _recipe: &RecipeJson) -> Result<(), CollectionError> {
        Ok(())
    }
}

pub fn accept_recipes() -> std::sync::Arc<dyn RecipeValidator> {
    std::sync::Arc::new(AcceptRecipes)
}

/// The constructors the moved tests were written against, with recipes accepted.
pub trait TestMemoryStore: Sized {
    fn in_memory(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self;
    fn in_memory_commons(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self;
    fn in_memory_with_profile(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: crate::KnotEncryptionProfile,
    ) -> Self;
}

impl TestMemoryStore for crate::KnotSyncStore<muniment::MemoryBackend> {
    fn in_memory(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self::in_memory_validated(space_id, writers, accept_recipes())
    }

    fn in_memory_commons(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self::in_memory_commons_validated(space_id, writers, accept_recipes())
    }

    fn in_memory_with_profile(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: crate::KnotEncryptionProfile,
    ) -> Self {
        Self::in_memory_with_profile_validated(space_id, writers, encryption, accept_recipes())
    }
}

pub trait TestFileStore: Sized {
    fn open(
        path: impl AsRef<std::path::Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, crate::KnotSyncError>;
    fn open_commons(
        path: impl AsRef<std::path::Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, crate::KnotSyncError>;
    fn open_with_profile(
        path: impl AsRef<std::path::Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: crate::KnotEncryptionProfile,
    ) -> Result<Self, crate::KnotSyncError>;
}

impl TestFileStore for crate::KnotSyncStore<muniment::RedbBackend> {
    fn open(
        path: impl AsRef<std::path::Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, crate::KnotSyncError> {
        Self::open_validated(path, space_id, writers, accept_recipes())
    }

    fn open_commons(
        path: impl AsRef<std::path::Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, crate::KnotSyncError> {
        Self::open_commons_validated(path, space_id, writers, accept_recipes())
    }

    fn open_with_profile(
        path: impl AsRef<std::path::Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: crate::KnotEncryptionProfile,
    ) -> Result<Self, crate::KnotSyncError> {
        Self::open_with_profile_validated(path, space_id, writers, encryption, accept_recipes())
    }
}
