// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot's sync store constructors, with the real projection-recipe judge.
//!
//! `knot-sync` builds a store only from a domain that carries a recipe
//! validator. Here, where the scene crates live, the constructors hand it
//! [`ScenomiseRecipes`], so a store built through them refuses exactly the
//! recipes the editor always refused.

use std::path::Path;
use std::sync::Arc;

use knot_composition::ScenomiseRecipes;
use knot_sync::{
    KnotCheckpointRepairReceipt, KnotEncryptionProfile, KnotSyncCipher, KnotSyncError,
    KnotSyncStore, RecipeValidator, SyncSealer,
};
use muniment::{MemoryBackend, RedbBackend};

fn recipes() -> Arc<dyn RecipeValidator> {
    Arc::new(ScenomiseRecipes)
}

/// The in-memory constructors.
pub trait KnotMemoryStoreExt: Sized {
    fn in_memory(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self;
    fn in_memory_commons(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self;
    fn in_memory_with_profile(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Self;
}

impl KnotMemoryStoreExt for KnotSyncStore<MemoryBackend> {
    fn in_memory(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self::in_memory_validated(space_id, writers, recipes())
    }

    fn in_memory_commons(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self::in_memory_commons_validated(space_id, writers, recipes())
    }

    fn in_memory_with_profile(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Self {
        Self::in_memory_with_profile_validated(space_id, writers, encryption, recipes())
    }
}

/// The redb file constructors.
#[allow(async_fn_in_trait)]
pub trait KnotFileStoreExt: Sized {
    fn open(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError>;
    fn open_commons(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError>;
    fn open_with_profile(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Result<Self, KnotSyncError>;
    async fn open_with_checkpoint_repair(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        cipher: KnotSyncCipher<'_>,
        storage_vault: &dyn SyncSealer,
    ) -> Result<(Self, KnotCheckpointRepairReceipt), KnotSyncError>;
}

impl KnotFileStoreExt for KnotSyncStore<RedbBackend> {
    fn open(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError> {
        Self::open_validated(path, space_id, writers, recipes())
    }

    fn open_commons(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError> {
        Self::open_commons_validated(path, space_id, writers, recipes())
    }

    fn open_with_profile(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Result<Self, KnotSyncError> {
        Self::open_with_profile_validated(path, space_id, writers, encryption, recipes())
    }

    async fn open_with_checkpoint_repair(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        cipher: KnotSyncCipher<'_>,
        storage_vault: &dyn SyncSealer,
    ) -> Result<(Self, KnotCheckpointRepairReceipt), KnotSyncError> {
        Self::open_with_checkpoint_repair_validated(
            path,
            space_id,
            writers,
            cipher,
            storage_vault,
            recipes(),
        )
        .await
    }
}
