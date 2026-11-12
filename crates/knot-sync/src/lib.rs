// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot's sync grammar, fold and operation store, in two halves.
//!
//! [`generic`] is the store over a backend: the causal encrypted operation log,
//! writer admission, authoring, local sealed storage, migration and the peer
//! join. It names no domain; a [`generic::SyncDomain`] supplies the event type,
//! the local validation, the error and the strings. [`knot`] is Knot's domain:
//! its grammar, fold, relations, predicates and composition records, with
//! projection recipes judged by a [`RecipeValidator`] the domain is built with.
//! Nothing here names a graph or a host; the backend is the caller's, which
//! may be catchment's reservoir (feature `reservoir`).

pub mod generic;
pub mod knot;
#[cfg(feature = "reservoir")]
mod reservoir;
#[cfg(test)]
mod test_support;

// The Knot half keeps its old crate-root paths.
pub(crate) use knot::document;
pub use knot::{
    composition, did_key, predicates, recipe, relations,
};
pub use knot::composition::{
    AnchorError, ByteSpan, CollectionError, CollectionItem, CollectionOrganization, DocumentAnchor,
    ItemKind, MAX_ID_BYTES, MAX_ITEMS, MAX_LABEL_BYTES, MAX_PACK_SOURCES, MAX_STORE_BYTES,
    MAX_TAG_BYTES, MAX_TAGS, MAX_TEXT_BYTES, OrganizeComposition, PackSource, bounded_nonempty,
};
pub use knot::did_key::{did_key, did_key_verifying_key};
pub use knot::document::{VaultDocument, hex32};
pub use knot_file_catalog::KnotFileRevisionV1;
pub use knot::predicates::{
    KNOT_PREDICATE_IRI_PREFIX, KnotPredicateCatalogV1, KnotPredicateDefinitionV1,
    KnotPredicateReplacementV1, KnotRejectedPredicateV1, mint_predicate_iri,
};
pub use knot::recipe::{RecipeJson, RecipeValidator};
pub use knot::relations::{
    KNOT_REL_VOCAB, KNOT_RELATION_CONTEXT_CHARS, KnotCorePredicateV1, KnotPredicateRefV1,
    KnotRejectedRelationV1, KnotRelationAssertionV1, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotRelationRetractionV1, KnotUnverifiedRelationV1,
    capture_endpoint_context,
};
#[cfg(feature = "reservoir")]
pub use reservoir::{NOTES_DOMAIN, ReservoirOpenError, open_through_reservoir};
pub use generic::SyncSealer;
pub use generic::{
    AssertedTime as KnotAssertedTime, EncryptionProfile as KnotEncryptionProfile,
    SyncCipher as KnotSyncCipher, SyncExt as KnotSyncExt,
};
pub use knot::{
    KNOT_COMMONS_ENCRYPTION_PROFILE, KnotAutomaticTextMerge, KnotCheckpointRepairReceipt,
    KnotCheckpointSnapshot, KnotDocumentConflict, KnotDocumentProjection, KnotDocumentVersion,
    KnotDomain, KnotEpochExecutionReceipt, KnotOfflineMemberEpochHold, KnotOfflineMemberRecovery,
    KnotProjectionCheckpoint, KnotProjectionStrictness, KnotSyncError, KnotSyncEvent,
    KnotSyncFileStore, KnotSyncStore, KnotTailReceipt,
};
