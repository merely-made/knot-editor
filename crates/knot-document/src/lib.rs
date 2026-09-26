// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Djot-first document authority and reusable Cambium presentation for Knot.
//!
//! The default dependency graph owns a single [`cambium::TextInput`] and native file
//! writes. Preview and conversion are opt-in through [`engine`].

mod document_surface;
mod document_view;
mod editor;
mod writer;

pub use document_surface::{
    KnotDiskComparisonV1, KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentRefusalV1,
    KnotDocumentSaveFailureV1, KnotDocumentSaveOutcomeV1, KnotDocumentSession,
    KnotDocumentSnapshotV1, KnotDocumentSourceKindV1, KnotDocumentSourceV1,
    KnotDocumentWritePostureV1, KnotOutlineItemV1, KnotOutlineSnapshotV1,
};
#[cfg(feature = "engine")]
pub use document_surface::{
    KnotFoldItemV1, KnotFoldKindV1, KnotFoldSnapshotV1, KnotPreviewSnapshotV1,
};
pub use document_view::{
    KNOT_DOCUMENT_CSS, KnotDocumentStatus, KnotDocumentSurfaceState, KnotDocumentView,
    knot_document_descriptor, knot_document_format_label, knot_document_posture_label,
    knot_document_refusal_label, knot_document_save_outcome_label, knot_document_surface,
    knot_document_view, knot_document_view_with_highlighting, knot_document_view_with_status,
};
pub use editor::{EditOutcome, KnotEditor, KnotEditorSaveError};
#[cfg(feature = "engine")]
pub use writer::AuthoredFile;
#[doc(hidden)]
pub use writer::write_if_distinct;
pub use writer::{DocumentFormat, SaveOutcome};
