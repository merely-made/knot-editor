// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The domain-free failures of the store.

use muniment::StoreError;
use stickleback::{CausalError, GroupCryptoError, ProcessError};

/// What the store itself can fail at; a domain's error absorbs these.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Causal(#[from] CausalError),
    #[error(transparent)]
    GroupCrypto(#[from] GroupCryptoError),
    #[error("sync payload: {0}")]
    Payload(String),
    #[error("sync cipher does not match the space's signed encryption profile")]
    WrongEncryptionProfile,
    #[error("local storage requires authorized closed-store repair before use")]
    CheckpointMigrationRequired,
    #[error("local storage: {0}")]
    CheckpointStorage(String),
}
