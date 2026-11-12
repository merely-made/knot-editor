// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot's half: its events, fold, relations, predicates, composition records,
//! checkpoints and epoch retention, built on the generic store through
//! [`KnotDomain`].

pub mod composition;
pub mod did_key;
mod djot_merge;
pub(crate) mod document;
pub mod predicates;
pub mod recipe;
pub mod relations;
mod sync;

pub use sync::{
    KNOT_COMMONS_ENCRYPTION_PROFILE, KnotAutomaticTextMerge, KnotCheckpointRepairReceipt,
    KnotCheckpointSnapshot, KnotDocumentConflict, KnotDocumentProjection, KnotDocumentVersion,
    KnotDomain, KnotEpochExecutionReceipt, KnotOfflineMemberEpochHold, KnotOfflineMemberRecovery,
    KnotProjectionCheckpoint, KnotProjectionStrictness, KnotSyncError, KnotSyncEvent,
    KnotSyncFileStore, KnotSyncStore, KnotTailReceipt,
};
