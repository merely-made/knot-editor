// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot: Mere's files-in-place authoring port.
//!
//! The first slice is deliberately read-only. [`DirectorySource`] discovers
//! files without storing their bytes in graph state, while [`KnotEndpoint`]
//! discloses those containers through Graphshell. The directory remains source
//! truth.

mod authority;
mod checkpoint_migration;
mod checkpoint_storage;
mod clip_evidence;
mod content_classes;
pub mod declaration;
mod did_key;
mod directory;
mod djot_merge;
mod embedding;
mod endpoint;
mod file_revision;
#[cfg(feature = "peers")]
mod mark;
#[cfg(feature = "mere-overlay")]
pub mod mere_overlay;
mod predicates;
mod publish;
#[cfg(feature = "peers")]
mod publish_carrier;
#[cfg(feature = "peers")]
mod publish_client;
#[cfg(feature = "peers")]
mod publish_host;
mod publish_wire;
mod relations;
#[cfg(feature = "peers")]
mod resident;
mod retain_host;
mod rosette;
mod search;
mod settings;
mod startup;
mod sync;
mod vault;
mod watcher;
mod web_annotation;
mod writer;

pub use authority::{KnotAuthoritySource, KnotSpaceAuthoritySnapshot};
pub use retain_host::KnotResidentRetainPort;
pub use endpoint::{CompositionGrant, KnotCompositionPort, KnotResidentCompositionPort};
#[cfg(feature = "peers")]
pub use clip_evidence::{BlobClipEvidenceStore, KnotContentRetentionPort};
pub use clip_evidence::{
    FileClipEvidenceStore, KnotClipCitation, KnotClipEvidenceRef, KnotClipEvidenceStore,
    clip_citations, clip_evidence_references,
};
pub use content_classes::{
    FILE_CLASS, FILE_DOCUMENT_FACET, KnotContentClasses, NOTE_CLASS, NOTE_DOCUMENT_FACET,
};
pub use did_key::{did_key, did_key_verifying_key};
pub use directory::{DirectorySource, DiskDocument, IgnorePolicy};
pub use embedding::{
    KNOT_BERT_ARTIFACTS, KNOT_INDEX_IDENTITY_VERSION, KNOT_LEXICAL_MODEL, KnotEmbeddingPreference,
    KnotEmbeddingProvider, KnotIndexIdentity, KnotModelWeights,
};
pub use endpoint::{
    KnotCaptureDestination, KnotCaptureError, KnotCaptureGrant, KnotCaptureReceipt,
    KnotFileCapturePort, KnotPreparedFileCaptureV1,
    KnotEffectAuthority, KnotEffectFetcher, KnotEffectMode, KnotEffectPolicy, KnotEndpoint,
    KnotResidentSource, KnotRosetteConfig, KnotWriteGrant,
};
pub use knot_document::{
    EditOutcome, KNOT_DOCUMENT_CSS, KnotDiskComparisonV1, KnotDocumentIntentErrorV1,
    KnotDocumentIntentV1, KnotDocumentRefusalV1, KnotDocumentSaveFailureV1,
    KnotDocumentSaveOutcomeV1, KnotDocumentSession, KnotDocumentSnapshotV1,
    KnotDocumentSourceKindV1, KnotDocumentSourceV1, KnotDocumentSurfaceState, KnotDocumentView,
    KnotDocumentWritePostureV1, KnotEditor, KnotEditorSaveError, KnotOutlineItemV1,
    KnotOutlineSnapshotV1, knot_document_descriptor, knot_document_surface, knot_document_view,
};
pub use knot_file_catalog::KnotFileRevisionV1;
pub use knot_file_catalog::{KnotFileCatalog, KnotFileCatalogAvailability, KnotFileCatalogRecord};
#[cfg(feature = "peers")]
pub use mark::{
    MARK_ALPN, MARK_DEFAULT_PORT, MARK_MAX_DOCUMENT_BYTES, MARK_MAX_METADATA_BYTES,
    MARK_MAX_REQUEST_BYTES, MarkAdapterError, MarkQuicHost, MarkReadAccess, MarkReadAdapter,
    MarkReadAdapterLimits, MarkRequest, MarkResponse, MarkServerError, MarkSnapshotOutcome,
    MarkTimestamp, MarkVersion, MarkVersionId, decode_mark_request, mark_server_config,
};
/// Admission-policy vocabulary belongs to the same Notochord instance as
/// Knot's publishing carrier. Product hosts should take these through Knot,
/// not add a second direct Notochord dependency.
pub use notochord::{NetworkId, ProfileRef, TrustedRoot};
pub use predicates::{
    KNOT_PREDICATE_IRI_PREFIX, KnotPredicateCatalogV1, KnotPredicateDefinitionV1,
    KnotPredicateReplacementV1, KnotRejectedPredicateV1, mint_predicate_iri,
};
pub use publish::{
    KNOT_PUBLISH_ALPN, KNOT_PUBLISH_DOMAIN, KNOT_PUBLISH_READ_ACTION, KNOT_PUBLISH_SERVICE,
    KNOT_SHARE_TICKET_VERSION, KnotPublication, KnotPublishCandidate, KnotPublishCatalog,
    KnotPublishEligibility, KnotPublishError, KnotPublishRead, KnotPublishedDocument,
    KnotShareControlError, KnotShareRecipient, KnotShareTicket, PublicationId, publication_path,
    revoke_share,
};
#[cfg(feature = "peers")]
pub use publish_carrier::{
    PublishCarrierError, PublishRefusal, accept_publish_session, publish_alpn, publish_policy,
};
#[cfg(feature = "peers")]
pub use publish_client::{
    KNOT_PUBLISH_READER_KEY_CONTEXT, KnotPublishClientError, decode_share_ticket,
    encode_share_ticket, fetch_published_document,
};
#[cfg(feature = "peers")]
pub use publish_host::{
    KnotPublishHost, KnotPublishHostError, KnotPublishHostLimits, KnotPublishServeOutcome,
    KnotPublishSource,
};
pub use publish_wire::{
    CandidateFixture, CandidateFixtureOutcome, HARD_MAX_CATALOG_ENTRIES, HARD_MAX_DOCUMENT_BYTES,
    HARD_MAX_REQUEST_BYTES, HARD_MAX_RESPONSE_BYTES, PublishRequest, PublishResponse,
    PublishWireError, PublishWireLimits, candidate_fixture_corpus, decode_request, decode_response,
    encode_request, encode_response,
};
pub use relations::{
    KNOT_REL_VOCAB, KNOT_RELATION_CONTEXT_CHARS, KnotCorePredicateV1, KnotPredicateRefV1,
    KnotRejectedRelationV1, KnotRelationAssertionV1, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotRelationRetractionV1, KnotUnverifiedRelationV1,
    capture_endpoint_context,
};
#[cfg(feature = "peers")]
pub use resident::{
    KnotEvidenceFetchReceipt, KnotEvidenceFetchStatus, KnotSyncHost, KnotSyncHostConfig,
    KnotSyncHostError,
};
pub use rosette::{
    CmudictPronunciations, LexiconCoverage, LineMeter, MetricalBeat, MetricalFoot,
    PronunciationLexicon, RosetteConfig, RosetteInterior, RosetteInteriorKind, RosetteProjection,
    UnresolvedToken, project_rosette,
};
pub use search::{
    KnotSearch, KnotSearchError, SearchConfig, SearchHit, SearchLane, SearchProgress,
};
pub use settings::{
    KnotSettings, KnotSettingsError, KnotSyncSettings, hex32, knot_settings_path, parse_hex32,
};
pub use startup::{
    KNOT_APP_ID, PersonalVaultKeys, SIGNING_KEY_CONTEXT, StartupUnlockedPersonalVault,
    VAULT_KEY_CONTEXT, persona_vault_root,
};
pub use sync::{
    KNOT_COMMONS_ENCRYPTION_PROFILE, KnotAssertedTime, KnotAutomaticTextMerge,
    KnotCheckpointRepairReceipt, KnotCheckpointSnapshot, KnotDocumentConflict,
    KnotDocumentProjection, KnotDocumentVersion, KnotEncryptionProfile, KnotEpochExecutionReceipt,
    KnotOfflineMemberEpochHold, KnotOfflineMemberRecovery, KnotProjectionCheckpoint,
    KnotProjectionStrictness, KnotSyncCipher, KnotSyncError, KnotSyncEvent, KnotSyncExt,
    KnotSyncFileStore, KnotSyncStore, KnotTailReceipt,
};
pub use vault::{KnotSealedSearchIndex, KnotVault, VaultDocument};
pub use watcher::DirectoryWatcher;
pub use web_annotation::{SpecificResource, SpecificResourceSelector};
pub use writer::{AuthoredFile, DocumentFormat, SaveOutcome};
