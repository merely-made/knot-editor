// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Test-only stand-ins: a key-only sealer that derives its keys on its own
//! (so a store written by Knot's vault opens under it) and a validator that
//! accepts every recipe. Neither is part of the crate's API.

#![allow(dead_code)]

use std::sync::Arc;

use knot_sync::{CollectionError, RecipeJson, RecipeValidator, SyncSealer};
use personae::{seal_bytes, unseal_bytes};

pub struct KeySealer([u8; 32]);

impl KeySealer {
    pub fn new(root_key: [u8; 32]) -> Self {
        Self(blake3::derive_key(
            "mere.knot.personal-vault-sync.v1",
            &root_key,
        ))
    }

    fn storage_key(&self) -> [u8; 32] {
        blake3::derive_key("mere.knot.local-storage-protection.v1", &self.0)
    }
}

impl SyncSealer for KeySealer {
    fn is_locked(&self) -> bool {
        false
    }

    fn seal_sync_payload(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        seal_bytes(&self.0, aad, plaintext).map_err(|error| error.to_string())
    }

    fn unseal_sync_payload(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        unseal_bytes(&self.0, aad, ciphertext).map_err(|error| error.to_string())
    }

    fn seal_local_storage(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        seal_bytes(&self.storage_key(), aad, plaintext).map_err(|error| error.to_string())
    }

    fn unseal_local_storage(&self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        unseal_bytes(&self.storage_key(), aad, ciphertext).map_err(|error| error.to_string())
    }
}

pub struct AcceptRecipes;

impl RecipeValidator for AcceptRecipes {
    fn validate(&self, _recipe: &RecipeJson) -> Result<(), CollectionError> {
        Ok(())
    }
}

pub fn accept_recipes() -> Arc<dyn RecipeValidator> {
    Arc::new(AcceptRecipes)
}
