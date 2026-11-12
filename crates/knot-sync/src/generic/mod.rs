// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The domain-free half of the store: a causal, encrypted operation log over a
//! backend, its writer admission, authoring, local sealed storage and migration,
//! and the peer join. It names no domain; a [`SyncDomain`] supplies the event
//! type, the local validation, the error and the vocabulary of its own strings.

mod cipher;
mod domain;
mod error;
pub(crate) mod migration;
mod policy;
mod sealer;
mod store;
pub(crate) mod storage;

pub use cipher::{
    AssertedTime, EncryptionProfile, StoredOperation, SyncCipher, SyncExt, decode_event,
    seal_event,
};
pub use domain::{LocalCheck, SyncDomain, Vocabulary};
pub use error::SyncError;
pub use sealer::SyncSealer;
pub use store::SyncStore;
pub(crate) use cipher::{causal_entries, operation_aad};
pub(crate) use policy::{CAUSAL_LIMITS, SyncPolicy};
pub(crate) const LOG_ID: u64 = 0;
