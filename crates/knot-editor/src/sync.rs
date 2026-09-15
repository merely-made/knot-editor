// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Causal personal and Commons document replication over Stickleback.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, RwLock};

use muniment::{Backend, MemoryBackend, RedbBackend, StoreError, WriteOp};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_net::{Endpoint, Gossip};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use proofs::Digest;
use serde::{Deserialize, Serialize};
use stickleback::CausalIndex;
use stickleback::{
    Admission, CausalEntry, CausalError, CausalLimits, DataKeyring, EpochCheckpointBasis,
    EpochHold, EpochHoldReason, EpochPruningProposal, EpochRetentionFacts, GroupCiphertext,
    GroupCryptoError, GroupEncryptionProfile, GroupSecretId, JoinError, JoinedSpace, MunimentStore,
    OperationPolicy, OperationProcessor, PendingCausalOperation, ProcessError, Reject, StoreTarget,
    author_head, causal_projection, observed_frontier, propose_epoch_pruning,
    validate_causal_metadata,
};
use tokio::sync::Mutex;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    KnotFileRevisionV1, KnotVault, VaultDocument,
    djot_merge::{automatic_text_merge, automatic_text_merge_head},
    predicates::{
        KnotPredicateCatalogV1, KnotPredicateDefinitionV1, KnotPredicateReplacementV1,
        KnotRejectedPredicateV1, mint_predicate_iri, validate_predicate_definition_payload,
    },
    relations::{
        KnotPredicateRefV1, KnotRejectedRelationV1, KnotRelationAssertionV1,
        KnotRelationEndpointV1, KnotRelationRetractionV1, KnotUnverifiedRelationV1,
        validate_relation_payload,
    },
};

const LOG_ID: u64 = 0;
const SYNC_AAD: &[u8] = b"mere.knot.sync-operation.v1";
const KNOT_CAUSAL_LIMITS: CausalLimits = CausalLimits {
    max_parents: 64,
    max_payload_bytes: 16 * 1024 * 1024,
};

fn file_revision_as_vault_document(revision: &KnotFileRevisionV1) -> VaultDocument {
    VaultDocument {
        id: revision.document_id.clone(),
        title: revision.title.clone(),
        body: revision.body.clone(),
        media_type: revision.media_type.clone(),
    }
}

/// Communal Knot uses the same retained-data floor as Commons chat.
pub const KNOT_COMMONS_ENCRYPTION_PROFILE: GroupEncryptionProfile =
    GroupEncryptionProfile::durable_data(8);

/// The signed encryption contract for one Knot space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnotEncryptionProfile {
    /// Personal device sync derives a key from the local vault root.
    #[default]
    PersonalVaultV1,
    /// Commons documents use the group's retained data-encryption epochs.
    CommonsDataV1,
}

/// Signed addressing extension for one Knot vault space.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotSyncExt {
    pub space_id: [u8; 32],
    #[serde(default)]
    pub encryption: KnotEncryptionProfile,
    /// Exact per-author frontier observed before this event was authored.
    #[serde(default)]
    pub parents: Vec<[u8; 32]>,
    /// Author-asserted wall clock, set at authoring and overridable before
    /// signing. Declared last and skipped when absent so an operation authored
    /// before this field reserializes to its original CBOR and keeps its hash.
    /// Inside the signed header bytes; informative only, it never orders a fold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_at_ms: Option<u64>,
}

/// What wall clock an authoring call binds into the signed header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KnotAssertedTime {
    /// This machine's clock now, or nothing at all if the clock fails.
    #[default]
    Now,
    /// An explicit time, so a transcribed note can carry its real date.
    At(u64),
    /// Assert no time.
    None,
}

impl KnotAssertedTime {
    fn resolve(self) -> Option<u64> {
        match self {
            // A failed clock asserts nothing; a time is never invented.
            Self::Now => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok()),
            Self::At(milliseconds) => Some(milliseconds),
            Self::None => None,
        }
    }
}

/// Plaintext event sealed inside the p2panda operation body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub enum KnotSyncEvent {
    Put(VaultDocument),
    /// An immutable, signed observation of a catalog-backed ordinary file.
    ///
    /// File revisions are relation-capture material only. They never enter the
    /// current vault-document projection or its publishing surface.
    CaptureFileRevision(KnotFileRevisionV1),
    Delete {
        id: String,
    },
    /// Replace the named causal document versions with one chosen value.
    Resolve {
        id: String,
        supersedes: Vec<[u8; 32]>,
        document: Option<VaultDocument>,
    },
    /// An attributable relation assertion between two observed document revisions.
    AssertRelation {
        predicate: KnotPredicateRefV1,
        subject: KnotRelationEndpointV1,
        object: KnotRelationEndpointV1,
        /// Opaque author-supplied reference; this does not fetch or verify evidence.
        evidence: Option<String>,
        qualification: Option<String>,
    },
    /// Author-only retraction of one signed relation assertion.
    RetractRelation {
        assertion: [u8; 32],
    },
    /// One writer's own predicate, minted or superseded. Appended last: an
    /// un-upgraded peer fails to decode this variant, which is why an
    /// undecodable closed event is now a rejected record, not a dead projection.
    DefinePredicate {
        slug: String,
        label: String,
        description: Option<String>,
        subproperty_of: Option<String>,
        lowers_to: Option<String>,
        supersedes: Option<[u8; 32]>,
        replaced_by: Option<KnotPredicateReplacementV1>,
    },
}

/// Encryption material used by a Knot replica.
#[derive(Clone, Copy)]
pub enum KnotSyncCipher<'a> {
    Personal(&'a KnotVault),
    CommonsData(&'a DataKeyring),
}

impl KnotSyncCipher<'_> {
    fn profile(self) -> KnotEncryptionProfile {
        match self {
            Self::Personal(_) => KnotEncryptionProfile::PersonalVaultV1,
            Self::CommonsData(_) => KnotEncryptionProfile::CommonsDataV1,
        }
    }

    /// The strictness a projection uses when the caller does not name one.
    ///
    /// Personal history is only ever undecodable because this peer has not
    /// learned a newer event variant, so it tolerates. Commons history is
    /// undecodable when the reader no longer holds the epoch, which is a
    /// membership fact the caller must see rather than silently lose.
    fn default_strictness(self) -> KnotProjectionStrictness {
        match self {
            Self::Personal(_) => KnotProjectionStrictness::Tolerant,
            Self::CommonsData(_) => KnotProjectionStrictness::Strict,
        }
    }
}

/// What a projection does with a causally closed event it cannot decode.
///
/// Carried by the projection call, never by store-wide state: one store is
/// read both ways, by a peer catching up on an unknown event variant and by a
/// caller who must learn it has lost an epoch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum KnotProjectionStrictness {
    /// Record the undecodable event as a rejected relation and project the
    /// readable remainder.
    #[default]
    Tolerant,
    /// Fail the whole projection with the decode error that stopped it.
    Strict,
}

/// Knot sync failures.
#[derive(Debug, thiserror::Error)]
pub enum KnotSyncError {
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
    #[error("invalid conflict resolution: {0}")]
    InvalidResolution(String),
    #[error("invalid relation assertion: {0}")]
    InvalidRelation(String),
    #[error("invalid predicate definition: {0}")]
    InvalidPredicate(String),
    #[error("invalid file revision capture: {0}")]
    InvalidFileRevision(String),
    #[error("Knot sync has no durable projection checkpoint")]
    MissingCheckpoint,
    #[error("reviewed Knot epoch proposal is stale")]
    StaleRetentionProposal,
    #[error("Knot epoch proposal is blocked")]
    BlockedRetentionProposal,
    #[error("document {0} has operations from more than one writer")]
    ConcurrentWriter(String),
}

#[derive(Clone)]
struct KnotSyncPolicy {
    space_id: [u8; 32],
    writers: Arc<RwLock<BTreeSet<[u8; 32]>>>,
    encryption: KnotEncryptionProfile,
}

impl OperationPolicy<KnotSyncExt> for KnotSyncPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<KnotSyncExt>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space_id != self.space_id {
            return Err(Reject::new(
                "wrong-knot-space",
                "operation addresses a different Knot vault",
            ));
        }
        if operation.header.extensions.encryption != self.encryption {
            return Err(Reject::new(
                "wrong-knot-encryption-profile",
                "operation uses a different Knot encryption profile",
            ));
        }
        if !self
            .writers
            .read()
            .is_ok_and(|writers| writers.contains(operation.header.verifying_key.as_bytes()))
        {
            return Err(Reject::new(
                "unrecognized-knot-writer",
                "operation author is not admitted to this Knot vault",
            ));
        }
        let body = operation.body.as_ref().ok_or_else(|| {
            Reject::new(
                "missing-knot-event",
                "Knot sync operations require a sealed body",
            )
        })?;
        if self.encryption == KnotEncryptionProfile::CommonsDataV1 {
            decode_cbor::<GroupCiphertext, _>(body.to_bytes().as_slice()).map_err(|error| {
                Reject::new(
                    "invalid-knot-group-ciphertext",
                    format!("Commons Knot body is not a data-envelope: {error}"),
                )
            })?;
        }
        validate_causal_metadata(
            operation,
            &operation.header.extensions.parents,
            KNOT_CAUSAL_LIMITS,
        )
        .map_err(|error| Reject::new("invalid-knot-causality", error.to_string()))?;
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.space_id),
            LOG_ID,
        )))
    }
}

/// One writer's current contribution to a conflicted document id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotDocumentVersion {
    pub writer: [u8; 32],
    pub operation: [u8; 32],
    /// `None` is that writer's current deletion.
    pub document: Option<VaultDocument>,
}

/// A document id touched by more than one writer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotDocumentConflict {
    pub id: String,
    pub versions: Vec<KnotDocumentVersion>,
}

/// A clean three-way merge derived from concurrent Knot text versions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotAutomaticTextMerge {
    pub id: String,
    pub base: [u8; 32],
    pub supersedes: Vec<[u8; 32]>,
    pub document: VaultDocument,
}

/// Knot's current causally closed document view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnotDocumentProjection {
    pub documents: Vec<VaultDocument>,
    pub conflicts: Vec<KnotDocumentConflict>,
    pub automatic_merges: Vec<KnotAutomaticTextMerge>,
    pub pending: Vec<PendingCausalOperation>,
    /// Exact current operation for every causally resolved document id.
    ///
    /// Consumers use this as an opaque optimistic-concurrency head rather
    /// than reducing a replicated document version to its plaintext digest.
    pub document_heads: BTreeMap<String, [u8; 32]>,
    /// All valid relation assertions in this space, including retracted history.
    /// This raw view is privileged; callers must filter it for endpoint admission.
    pub relations: Vec<KnotRelationAssertionV1>,
    /// Relation operations that failed replay validation without affecting documents.
    pub rejected_relations: Vec<KnotRejectedRelationV1>,
    /// Structurally valid assertions whose source captures could not be verified.
    pub unverified_relations: Vec<KnotUnverifiedRelationV1>,
    /// Writer-defined predicates folded from this space.
    pub predicates: KnotPredicateCatalogV1,
}

impl KnotDocumentProjection {
    /// Returns active assertions whose two captured documents are admitted to the caller.
    ///
    /// The caller supplies its revision-appropriate document admission set; this
    /// helper filters a projection and does not create an access-control policy.
    pub fn relations_visible_to(
        &self,
        admitted_document_ids: &BTreeSet<String>,
    ) -> Vec<KnotRelationAssertionV1> {
        self.relation_history_visible_to(admitted_document_ids)
            .into_iter()
            .filter(|relation| relation.retractions.is_empty())
            .collect()
    }

    /// The label to display for one assertion's predicate.
    ///
    /// An assertion signed under an older label displays the current one; the
    /// stored identity never changes.
    pub fn predicate_label(&self, relation: &KnotRelationAssertionV1) -> Option<&str> {
        self.predicates.label_of(&relation.predicate)
    }

    /// The IRI one assertion's predicate exports as.
    pub fn predicate_iri(&self, relation: &KnotRelationAssertionV1) -> Option<&str> {
        self.predicates.iri_of(&relation.predicate)
    }

    /// Returns assertion history whose two captured documents are admitted to the caller.
    ///
    /// Raw, rejected, and unverified relation records remain privileged inspection data.
    pub fn relation_history_visible_to(
        &self,
        admitted_document_ids: &BTreeSet<String>,
    ) -> Vec<KnotRelationAssertionV1> {
        self.relations
            .iter()
            .filter(|relation| {
                admitted_document_ids.contains(&relation.subject.document_id)
                    && admitted_document_ids.contains(&relation.object.document_id)
            })
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotAuthorHead {
    pub author: [u8; 32],
    pub log_id: u64,
    pub seq_num: u32,
    pub operation: [u8; 32],
}

/// Knot-native materialized base retained by a projection checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotCheckpointSnapshot {
    pub documents: Vec<VaultDocument>,
    pub conflicts: Vec<KnotDocumentConflict>,
    #[serde(default)]
    pub automatic_merges: Vec<KnotAutomaticTextMerge>,
    pub document_heads: BTreeMap<String, [u8; 32]>,
    /// Added after document-only checkpoints; old checkpoint snapshots remain readable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<KnotRelationAssertionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_relations: Vec<KnotRejectedRelationV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unverified_relations: Vec<KnotUnverifiedRelationV1>,
    /// Appended after the relation fields, and skipped when empty, so a
    /// checkpoint written before predicates reserializes byte-identically and
    /// keeps its blake3 identity. The checkpoint version stays 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predicates: Vec<KnotPredicateDefinitionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_predicates: Vec<KnotRejectedPredicateV1>,
}

/// Durable projection boundary required before domain-authorized pruning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotProjectionCheckpoint {
    pub version: u16,
    pub space_id: [u8; 32],
    pub heads: Vec<KnotAuthorHead>,
    pub document_digests: Vec<(String, [u8; 32])>,
    pub conflict_ids: Vec<String>,
    pub pending: Vec<([u8; 32], Vec<[u8; 32]>)>,
    /// Added after the original digest-only checkpoint. Legacy checkpoints
    /// remain readable but cannot authorize epoch pruning.
    #[serde(default)]
    pub snapshot: Option<KnotCheckpointSnapshot>,
}

/// Exact retained tail after a durable checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotTailReceipt {
    pub checkpoint: [u8; 32],
    pub operations: Vec<[u8; 32]>,
}

/// Recovery promise supplied by communal Knot's offline-member policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnotOfflineMemberEpochHold {
    pub member: [u8; 32],
    pub epoch: GroupSecretId,
}

/// Atomic host receipt for explicit communal Knot epoch erasure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotEpochExecutionReceipt {
    pub version: u16,
    pub space_id: [u8; 32],
    pub checkpoint: Digest,
    pub authority_revision: Digest,
    pub forgotten: Vec<GroupSecretId>,
    pub retained: Vec<GroupSecretId>,
    pub previous_keyring: Digest,
    pub persisted_keyring: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotOfflineMemberRecovery {
    Resume,
    BootstrapRequired { checkpoint: Option<Digest> },
}

#[derive(Clone)]
struct StoredKnotOperation {
    operation: Operation<KnotSyncExt>,
    log_id: u64,
}

/// Replicated encrypted event store for one personal Knot vault.
#[derive(Clone)]
pub struct KnotSyncStore<B> {
    store: MunimentStore<B, KnotSyncExt>,
    policy: KnotSyncPolicy,
    /// Coordinates Knot-managed mutations. Direct writes through [`Self::sync_store`]
    /// remain outside this guarantee.
    mutation_gate: Arc<Mutex<()>>,
}

pub type KnotSyncFileStore = KnotSyncStore<RedbBackend>;

impl KnotSyncStore<MemoryBackend> {
    pub fn in_memory(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self::in_memory_with_profile(space_id, writers, KnotEncryptionProfile::PersonalVaultV1)
    }

    pub fn in_memory_commons(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Self {
        Self::in_memory_with_profile(space_id, writers, KnotEncryptionProfile::CommonsDataV1)
    }

    pub fn in_memory_with_profile(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Self {
        Self {
            store: MunimentStore::new(MemoryBackend::new()),
            policy: KnotSyncPolicy {
                space_id,
                writers: Arc::new(RwLock::new(writers.into_iter().collect())),
                encryption,
            },
            mutation_gate: Arc::new(Mutex::new(())),
        }
    }
}

impl KnotSyncStore<RedbBackend> {
    pub fn open(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError> {
        Self::open_with_profile(
            path,
            space_id,
            writers,
            KnotEncryptionProfile::PersonalVaultV1,
        )
    }

    pub fn open_commons(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError> {
        Self::open_with_profile(
            path,
            space_id,
            writers,
            KnotEncryptionProfile::CommonsDataV1,
        )
    }

    pub fn open_with_profile(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Result<Self, KnotSyncError> {
        Ok(Self {
            store: MunimentStore::new(RedbBackend::open(path)?),
            policy: KnotSyncPolicy {
                space_id,
                writers: Arc::new(RwLock::new(writers.into_iter().collect())),
                encryption,
            },
            mutation_gate: Arc::new(Mutex::new(())),
        })
    }
}

impl<B> KnotSyncStore<B>
where
    B: Backend + Clone,
{
    pub fn space_id(&self) -> [u8; 32] {
        self.policy.space_id
    }

    pub fn encryption_profile(&self) -> KnotEncryptionProfile {
        self.policy.encryption
    }

    /// Writers currently admitted by this replica's materialized policy.
    pub fn admitted_writers(&self) -> Vec<[u8; 32]> {
        self.policy
            .writers
            .read()
            .map(|writers| writers.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Admit one newly paired writer without rebuilding the active LogSync
    /// session. Returns whether the materialized policy changed.
    pub fn admit_writer(&self, writer: [u8; 32]) -> bool {
        self.policy
            .writers
            .write()
            .map(|mut writers| writers.insert(writer))
            .unwrap_or(false)
    }

    /// Revoke one paired writer for future operation admission.
    pub fn deny_writer(&self, writer: &[u8; 32]) -> bool {
        self.policy
            .writers
            .write()
            .map(|mut writers| writers.remove(writer))
            .unwrap_or(false)
    }

    /// Replace communal writer admission from a newly materialized Gemot view.
    pub fn replace_admitted_writers(&self, writers: impl IntoIterator<Item = [u8; 32]>) -> bool {
        let next = writers.into_iter().collect::<BTreeSet<_>>();
        self.policy
            .writers
            .write()
            .map(|mut current| {
                if *current == next {
                    return false;
                }
                *current = next;
                true
            })
            .unwrap_or(false)
    }

    /// Seal, sign, admit, and store the next event in this device's log.
    pub async fn author(
        &self,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        event: &KnotSyncEvent,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.author_with_cipher(signing_seed, KnotSyncCipher::Personal(vault), event)
            .await
    }

    /// [`Self::author`] with an explicit author-asserted time.
    pub async fn author_at(
        &self,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        event: &KnotSyncEvent,
        asserted_at: KnotAssertedTime,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.author_with_cipher_at(
            signing_seed,
            KnotSyncCipher::Personal(vault),
            event,
            asserted_at,
        )
        .await
    }

    pub async fn author_communal(
        &self,
        signing_seed: [u8; 32],
        keys: &DataKeyring,
        event: &KnotSyncEvent,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.author_with_cipher(signing_seed, KnotSyncCipher::CommonsData(keys), event)
            .await
    }

    pub async fn author_communal_at(
        &self,
        signing_seed: [u8; 32],
        keys: &DataKeyring,
        event: &KnotSyncEvent,
        asserted_at: KnotAssertedTime,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.author_with_cipher_at(
            signing_seed,
            KnotSyncCipher::CommonsData(keys),
            event,
            asserted_at,
        )
        .await
    }

    pub async fn author_with_cipher(
        &self,
        signing_seed: [u8; 32],
        cipher: KnotSyncCipher<'_>,
        event: &KnotSyncEvent,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.author_with_cipher_at(signing_seed, cipher, event, KnotAssertedTime::Now)
            .await
    }

    pub async fn author_with_cipher_at(
        &self,
        signing_seed: [u8; 32],
        cipher: KnotSyncCipher<'_>,
        event: &KnotSyncEvent,
        asserted_at: KnotAssertedTime,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        let _gate = self.mutation_gate.lock().await;
        self.author_under_gate(signing_seed, cipher, event, asserted_at)
            .await
    }

    async fn author_under_gate(
        &self,
        signing_seed: [u8; 32],
        cipher: KnotSyncCipher<'_>,
        event: &KnotSyncEvent,
        asserted_at: KnotAssertedTime,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.require_cipher(cipher)?;
        let signing_key = SigningKey::from_bytes(&signing_seed);
        let author = signing_key.verifying_key();
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let parents = observed_frontier(&entries)?;
        let causal = CausalIndex::new(&entries);
        let closed = causal_projection(&entries)?;
        validate_local_relation_event(
            event,
            *author.as_bytes(),
            &records,
            &closed.order,
            cipher,
            &causal,
            &parents,
            self.policy.space_id,
        )?;
        let (seq_num, backlink) = author_head(&entries, *author.as_bytes(), &LOG_ID)?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(event).map_err(|error| KnotSyncError::Payload(error.to_string()))?,
        );
        // The AEAD AAD stays at v1: the asserted time is bound by the ed25519
        // signature over the CBOR header and by the operation hash, and the
        // body through payload_hash. Changing this would orphan sealed bodies.
        let aad = operation_aad(self.policy.space_id, author.as_bytes(), seq_num);
        let ciphertext = seal_event(cipher, &aad, plaintext.as_slice())?;
        let body = Body::from_bytes(&ciphertext);
        // p2panda 0.7.1 made the header's CBOR cache, size and digest private
        // and folded signing into the builder: `build` encodes, signs and
        // caches the digest in one step, so the struct-literal + `sign` pair
        // has no equivalent. The builder derives verifying_key from the
        // signing key -- the same value `author` already held.
        let header = Header::builder()
            .body(&ciphertext)
            .seq_num(seq_num)
            .backlink(backlink.map(Hash::from))
            .build(
                &signing_key,
                KnotSyncExt {
                    space_id: self.policy.space_id,
                    encryption: self.policy.encryption,
                    parents,
                    asserted_at_ms: asserted_at.resolve(),
                },
            );
        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        self.accept_under_gate(&operation).await?;
        Ok(operation)
    }

    pub async fn resolve_conflict(
        &self,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        conflict: &KnotDocumentConflict,
        document: Option<VaultDocument>,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.resolve_conflict_with_cipher(
            signing_seed,
            KnotSyncCipher::Personal(vault),
            conflict,
            document,
        )
        .await
    }

    pub async fn resolve_communal_conflict(
        &self,
        signing_seed: [u8; 32],
        keys: &DataKeyring,
        conflict: &KnotDocumentConflict,
        document: Option<VaultDocument>,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.resolve_conflict_with_cipher(
            signing_seed,
            KnotSyncCipher::CommonsData(keys),
            conflict,
            document,
        )
        .await
    }

    pub async fn resolve_conflict_with_cipher(
        &self,
        signing_seed: [u8; 32],
        cipher: KnotSyncCipher<'_>,
        conflict: &KnotDocumentConflict,
        document: Option<VaultDocument>,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        if document
            .as_ref()
            .is_some_and(|document| document.id != conflict.id)
        {
            return Err(KnotSyncError::InvalidResolution(
                "chosen document id does not match the conflict".into(),
            ));
        }
        let mut supersedes: Vec<_> = conflict
            .versions
            .iter()
            .map(|version| version.operation)
            .collect();
        supersedes.sort();
        supersedes.dedup();
        self.author_with_cipher(
            signing_seed,
            cipher,
            &KnotSyncEvent::Resolve {
                id: conflict.id.clone(),
                supersedes,
                document,
            },
        )
        .await
    }

    fn require_cipher(&self, cipher: KnotSyncCipher<'_>) -> Result<(), KnotSyncError> {
        if cipher.profile() != self.policy.encryption {
            return Err(KnotSyncError::WrongEncryptionProfile);
        }
        Ok(())
    }

    fn require_admitted_writer(&self, writer: [u8; 32]) -> Result<(), KnotSyncError> {
        if self
            .policy
            .writers
            .read()
            .is_ok_and(|writers| writers.contains(&writer))
        {
            return Ok(());
        }
        Err(KnotSyncError::Payload(
            "signing writer is not admitted to this Knot vault".into(),
        ))
    }

    /// The Knot-specific `accept` closure target used by Stickleback.
    pub async fn accept(&self, operation: &Operation<KnotSyncExt>) -> Result<bool, KnotSyncError> {
        let _gate = self.mutation_gate.lock().await;
        self.accept_under_gate(operation).await
    }

    async fn accept_under_gate(
        &self,
        operation: &Operation<KnotSyncExt>,
    ) -> Result<bool, KnotSyncError> {
        let processor = OperationProcessor::new(self.store.clone(), self.policy.clone());
        Ok(processor.process(operation).await?.inserted())
    }

    async fn load_operations(&self) -> Result<Vec<StoredKnotOperation>, KnotSyncError> {
        let logs: BTreeMap<VerifyingKey, Vec<u64>> = self
            .store
            .resolve(&Topic::from(self.policy.space_id))
            .await?;
        let mut records = Vec::new();
        for (author, mut log_ids) in logs {
            log_ids.sort_unstable();
            log_ids.dedup();
            for log_id in log_ids {
                let Some(entries) = self
                    .store
                    .get_log_entries(&author, &log_id, None, None)
                    .await?
                else {
                    continue;
                };
                for (operation, _) in entries {
                    records.push(StoredKnotOperation { operation, log_id });
                }
            }
        }
        Ok(records)
    }

    /// Fold the causally closed subset into documents while preserving
    /// document conflicts and missing-history diagnostics.
    ///
    /// Defaults to [`KnotProjectionStrictness::Tolerant`]: an event this peer
    /// cannot decode is a newer variant it has not learned, not a lost key.
    pub async fn projection(
        &self,
        vault: &KnotVault,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.projection_with_cipher(KnotSyncCipher::Personal(vault))
            .await
    }

    /// [`Self::projection`] with an explicit undecodable-event policy.
    pub async fn projection_with_strictness(
        &self,
        vault: &KnotVault,
        strictness: KnotProjectionStrictness,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.projection_with_cipher_and_strictness(KnotSyncCipher::Personal(vault), strictness)
            .await
    }

    /// Fold the causally closed Commons subset under a data keyring.
    ///
    /// Defaults to [`KnotProjectionStrictness::Strict`]: a commons member
    /// reading past an epoch it no longer holds is the case the strict form
    /// exists for, and a silently shortened document list is the wrong answer
    /// to it. Callers that want the readable subset plus rejected records ask
    /// for [`KnotProjectionStrictness::Tolerant`] through
    /// [`Self::communal_projection_with_strictness`].
    pub async fn communal_projection(
        &self,
        keys: &DataKeyring,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.projection_with_cipher(KnotSyncCipher::CommonsData(keys))
            .await
    }

    /// [`Self::communal_projection`] with an explicit undecodable-event policy.
    pub async fn communal_projection_with_strictness(
        &self,
        keys: &DataKeyring,
        strictness: KnotProjectionStrictness,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.projection_with_cipher_and_strictness(KnotSyncCipher::CommonsData(keys), strictness)
            .await
    }

    /// Project under an explicit cipher, with that cipher's default strictness
    /// (personal tolerates, Commons is strict).
    pub async fn projection_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.projection_with_cipher_and_strictness(cipher, cipher.default_strictness())
            .await
    }

    pub async fn projection_with_cipher_and_strictness(
        &self,
        cipher: KnotSyncCipher<'_>,
        strictness: KnotProjectionStrictness,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.require_cipher(cipher)?;
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let projection = causal_projection(&entries)?;
        // Under Tolerant an undecodable closed event is reported, not fatal: a
        // peer that has not learned a newer event variant still projects
        // everything it can read, the same tolerance pending undecipherable
        // history already has. Under Strict the decode error ends the fold.
        let mut undecodable = Vec::new();
        let mut events = BTreeMap::new();
        for &index in &projection.order {
            let operation = &records[index].operation;
            match decode_event(cipher, operation) {
                Ok(event) => {
                    events.insert(index, event);
                },
                Err(error) => match strictness {
                    KnotProjectionStrictness::Strict => return Err(error),
                    KnotProjectionStrictness::Tolerant => {
                        undecodable.push(KnotRejectedRelationV1 {
                            operation: *operation.hash.as_bytes(),
                            author: *operation.header.verifying_key.as_bytes(),
                            reason: format!("closed event could not be decoded: {error}"),
                        })
                    },
                },
            }
        }
        // Indexed once for the whole fold: the retain below asks a reachability
        // question per surviving version per operation, and rebuilding the hash
        // index inside each of those made a save cost O(n^2 log n) in history.
        let causal = CausalIndex::new(&entries);
        let relation_fold = fold_relations(
            &records,
            &events,
            &projection.order,
            &causal,
            self.policy.space_id,
        )?;
        let mut current = BTreeMap::<String, BTreeMap<[u8; 32], KnotDocumentVersion>>::new();
        let mut event_documents = BTreeMap::<[u8; 32], String>::new();
        let mut version_history = Vec::<([u8; 32], String, Option<VaultDocument>)>::new();

        for index in projection.order {
            let operation = &records[index].operation;
            let writer = *operation.header.verifying_key.as_bytes();
            let operation_id = *operation.hash.as_bytes();
            let Some(event) = events.get(&index).cloned() else {
                continue;
            };
            let (id, document, replaces_observed) = match event {
                KnotSyncEvent::Put(document) => (document.id.clone(), Some(document), true),
                KnotSyncEvent::Delete { id } => (id, None, true),
                KnotSyncEvent::Resolve {
                    id,
                    supersedes,
                    document,
                } => {
                    let targets = validate_resolution(
                        &causal,
                        &event_documents,
                        operation_id,
                        &id,
                        &supersedes,
                        document.as_ref(),
                    )?;
                    if let Some(versions) = current.get_mut(&id) {
                        versions.retain(|_, version| !targets.contains(&version.operation));
                    }
                    (id, document, false)
                },
                // None of these produce a document version.
                KnotSyncEvent::CaptureFileRevision(_)
                | KnotSyncEvent::AssertRelation { .. }
                | KnotSyncEvent::RetractRelation { .. }
                | KnotSyncEvent::DefinePredicate { .. } => {
                    continue;
                },
            };
            if replaces_observed && let Some(versions) = current.get_mut(&id) {
                versions
                    .retain(|_, version| !causal.happens_before(version.operation, operation_id));
            }
            event_documents.insert(operation_id, id.clone());
            version_history.push((operation_id, id.clone(), document.clone()));
            current.entry(id).or_default().insert(
                writer,
                KnotDocumentVersion {
                    writer,
                    operation: operation_id,
                    document,
                },
            );
        }

        let mut documents = Vec::new();
        let mut conflicts = Vec::new();
        let mut automatic_merges = Vec::new();
        let mut document_heads = BTreeMap::new();
        for (id, versions) in current {
            if versions.len() == 1 {
                let version = versions.into_values().next().unwrap();
                document_heads.insert(id, version.operation);
                if let Some(document) = version.document {
                    documents.push(document);
                }
            } else if let Some(automatic) =
                automatic_text_merge(&causal, &version_history, &id, &versions)
            {
                document_heads.insert(
                    id,
                    automatic_text_merge_head(
                        automatic.base,
                        &automatic.supersedes,
                        &automatic.document,
                    )?,
                );
                documents.push(automatic.document.clone());
                automatic_merges.push(automatic);
            } else {
                conflicts.push(KnotDocumentConflict {
                    id,
                    versions: versions.into_values().collect(),
                });
            }
        }
        let mut rejected_relations = relation_fold.rejected;
        rejected_relations.extend(undecodable);
        Ok(KnotDocumentProjection {
            documents,
            conflicts,
            automatic_merges,
            pending: projection.pending,
            document_heads,
            relations: relation_fold.relations,
            rejected_relations,
            unverified_relations: relation_fold.unverified,
            predicates: relation_fold.catalog,
        })
    }

    /// Materialize one exact retained document-producing operation.
    ///
    /// The operation must be in the causally closed projection and must name
    /// `document_id` itself. Deletes, resolutions to no document, operations
    /// for another document, and pending history all return `None`; callers do
    /// not need to infer history from a current endpoint projection.
    pub async fn document_version(
        &self,
        vault: &KnotVault,
        document_id: &str,
        operation_id: [u8; 32],
    ) -> Result<Option<VaultDocument>, KnotSyncError> {
        self.document_version_with_cipher(
            KnotSyncCipher::Personal(vault),
            document_id,
            operation_id,
        )
        .await
    }

    /// Cipher-generic form of [`Self::document_version`].
    pub async fn document_version_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
        document_id: &str,
        operation_id: [u8; 32],
    ) -> Result<Option<VaultDocument>, KnotSyncError> {
        self.require_cipher(cipher)?;
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let projection = causal_projection(&entries)?;

        for index in projection.order {
            let operation = &records[index].operation;
            if *operation.hash.as_bytes() != operation_id {
                continue;
            }
            let event = decode_event(cipher, operation)?;
            return Ok(match event {
                KnotSyncEvent::Put(document) if document.id == document_id => Some(document),
                KnotSyncEvent::Resolve {
                    id,
                    document: Some(document),
                    ..
                } if id == document_id && document.id == id => Some(document),
                _ => None,
            });
        }
        Ok(None)
    }

    /// Materialize one exact retained file-revision capture.
    ///
    /// Captures are deliberately distinct from vault documents: this getter
    /// never makes an ordinary file available through the document or publish
    /// projections.
    pub async fn file_revision(
        &self,
        vault: &KnotVault,
        document_id: &str,
        operation_id: [u8; 32],
    ) -> Result<Option<KnotFileRevisionV1>, KnotSyncError> {
        self.file_revision_with_cipher(KnotSyncCipher::Personal(vault), document_id, operation_id)
            .await
    }

    /// Cipher-generic form of [`Self::file_revision`].
    pub async fn file_revision_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
        document_id: &str,
        operation_id: [u8; 32],
    ) -> Result<Option<KnotFileRevisionV1>, KnotSyncError> {
        self.require_cipher(cipher)?;
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let projection = causal_projection(&entries)?;

        for index in projection.order {
            let operation = &records[index].operation;
            if *operation.hash.as_bytes() != operation_id {
                continue;
            }
            let event = decode_event(cipher, operation)?;
            return Ok(match event {
                KnotSyncEvent::CaptureFileRevision(revision)
                    if revision.document_id == document_id && revision.validate().is_ok() =>
                {
                    Some(revision)
                },
                _ => None,
            });
        }
        Ok(None)
    }

    /// Find an already-retained exact file revision by its signed writer.
    ///
    /// Retention callers use this before authoring so a retry after an
    /// uncertain response can return the original operation rather than append
    /// another observation. The lookup refuses incomplete causal history: a
    /// capture hidden behind an unavailable parent is not a safe idempotency
    /// receipt. If more than one closed operation matches, the lowest operation
    /// hash is returned so the result is independent of causal traversal order.
    pub(crate) async fn find_file_revision_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
        writer: [u8; 32],
        revision: &KnotFileRevisionV1,
    ) -> Result<Option<[u8; 32]>, KnotSyncError> {
        revision
            .validate()
            .map_err(KnotSyncError::InvalidFileRevision)?;
        self.require_cipher(cipher)?;
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let projection = causal_projection(&entries)?;
        if !projection.pending.is_empty() {
            return Err(KnotSyncError::Payload(
                "capture retention requires complete causal history".into(),
            ));
        }

        let mut matching = None;
        for index in projection.order {
            let operation = &records[index].operation;
            if *operation.header.verifying_key.as_bytes() != writer {
                continue;
            }
            let event = decode_event(cipher, operation)?;
            let KnotSyncEvent::CaptureFileRevision(candidate) = event else {
                continue;
            };
            if candidate.validate().is_ok() && candidate == *revision {
                let operation_id = *operation.hash.as_bytes();
                if matching.is_none_or(|current| operation_id < current) {
                    matching = Some(operation_id);
                }
            }
        }
        Ok(matching)
    }

    /// Retain one exact file revision, returning its operation id and whether
    /// the revision was already present for this writer.
    pub(crate) async fn retain_file_revision_with_cipher(
        &self,
        signing_seed: [u8; 32],
        cipher: KnotSyncCipher<'_>,
        revision: &KnotFileRevisionV1,
    ) -> Result<([u8; 32], bool), KnotSyncError> {
        let _gate = self.mutation_gate.lock().await;
        self.require_cipher(cipher)?;
        let signing_key = SigningKey::from_bytes(&signing_seed);
        let writer = *signing_key.verifying_key().as_bytes();
        self.require_admitted_writer(writer)?;
        if let Some(operation) = self
            .find_file_revision_with_cipher(cipher, writer, revision)
            .await?
        {
            return Ok((operation, true));
        }
        let operation = self
            .author_under_gate(
                signing_seed,
                cipher,
                &KnotSyncEvent::CaptureFileRevision(revision.clone()),
                KnotAssertedTime::Now,
            )
            .await?;
        Ok((*operation.hash.as_bytes(), false))
    }

    /// Compatibility view for existing callers. New consumers should use
    /// [`Self::projection`] so unrelated documents remain available beside an
    /// explicit conflict.
    pub async fn documents(&self, vault: &KnotVault) -> Result<Vec<VaultDocument>, KnotSyncError> {
        self.documents_with_cipher(KnotSyncCipher::Personal(vault))
            .await
    }

    pub async fn communal_documents(
        &self,
        keys: &DataKeyring,
    ) -> Result<Vec<VaultDocument>, KnotSyncError> {
        self.documents_with_cipher(KnotSyncCipher::CommonsData(keys))
            .await
    }

    pub async fn documents_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<Vec<VaultDocument>, KnotSyncError> {
        let projection = self.projection_with_cipher(cipher).await?;
        if let Some(conflict) = projection.conflicts.first() {
            return Err(KnotSyncError::ConcurrentWriter(conflict.id.clone()));
        }
        Ok(projection.documents)
    }

    /// Persist the current projection frontier. This is a prerequisite receipt,
    /// not permission to prune.
    pub async fn save_checkpoint(
        &self,
        vault: &KnotVault,
    ) -> Result<KnotProjectionCheckpoint, KnotSyncError> {
        self.save_checkpoint_with_cipher(KnotSyncCipher::Personal(vault))
            .await
    }

    pub async fn save_communal_checkpoint(
        &self,
        keys: &DataKeyring,
    ) -> Result<KnotProjectionCheckpoint, KnotSyncError> {
        self.save_checkpoint_with_cipher(KnotSyncCipher::CommonsData(keys))
            .await
    }

    pub async fn save_checkpoint_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<KnotProjectionCheckpoint, KnotSyncError> {
        let _gate = self.mutation_gate.lock().await;
        let checkpoint = self.build_checkpoint_with_cipher(cipher).await?;
        let bytes = serde_json::to_vec(&checkpoint)
            .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
        self.store
            .backend()
            .put(&checkpoint_key(self.policy.space_id), &bytes)
            .await?;
        Ok(checkpoint)
    }

    async fn build_checkpoint_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<KnotProjectionCheckpoint, KnotSyncError> {
        let projection = self.projection_with_cipher(cipher).await?;
        let records = self.load_operations().await?;
        let mut heads = BTreeMap::<([u8; 32], u64), KnotAuthorHead>::new();
        for record in records {
            let operation = &record.operation;
            let key = (*operation.header.verifying_key.as_bytes(), record.log_id);
            let candidate = KnotAuthorHead {
                author: key.0,
                log_id: key.1,
                seq_num: operation.header.seq_num,
                operation: *operation.hash.as_bytes(),
            };
            if heads
                .get(&key)
                .is_none_or(|current| candidate.seq_num > current.seq_num)
            {
                heads.insert(key, candidate);
            }
        }
        let mut document_digests = Vec::new();
        for document in &projection.documents {
            let bytes = serde_json::to_vec(document)
                .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
            document_digests.push((document.id.clone(), *blake3::hash(&bytes).as_bytes()));
        }
        let snapshot = KnotCheckpointSnapshot {
            documents: projection.documents.clone(),
            conflicts: projection.conflicts.clone(),
            automatic_merges: projection.automatic_merges.clone(),
            document_heads: projection.document_heads.clone(),
            relations: projection.relations.clone(),
            rejected_relations: projection.rejected_relations.clone(),
            unverified_relations: projection.unverified_relations.clone(),
            predicates: projection.predicates.definitions.clone(),
            rejected_predicates: projection.predicates.rejected.clone(),
        };
        Ok(KnotProjectionCheckpoint {
            version: 1,
            space_id: self.policy.space_id,
            heads: heads.into_values().collect(),
            document_digests,
            conflict_ids: projection
                .conflicts
                .into_iter()
                .map(|conflict| conflict.id)
                .collect(),
            pending: projection
                .pending
                .into_iter()
                .map(|pending| (pending.operation, pending.missing))
                .collect(),
            snapshot: Some(snapshot),
        })
    }

    pub async fn load_checkpoint(&self) -> Result<Option<KnotProjectionCheckpoint>, KnotSyncError> {
        let Some(bytes) = self
            .store
            .backend()
            .get(&checkpoint_key(self.policy.space_id))
            .await?
        else {
            return Ok(None);
        };
        let checkpoint: KnotProjectionCheckpoint = serde_json::from_slice(&bytes)
            .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
        if checkpoint.version != 1 || checkpoint.space_id != self.policy.space_id {
            return Err(KnotSyncError::Payload(
                "checkpoint version or space does not match this store".into(),
            ));
        }
        Ok(Some(checkpoint))
    }

    /// Name the exact operations newer than the last durable checkpoint.
    pub async fn tail_receipt(&self) -> Result<KnotTailReceipt, KnotSyncError> {
        let checkpoint = self
            .load_checkpoint()
            .await?
            .ok_or(KnotSyncError::MissingCheckpoint)?;
        let bytes = serde_json::to_vec(&checkpoint)
            .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
        let checkpoint_id = *blake3::hash(&bytes).as_bytes();
        let heads: BTreeMap<_, _> = checkpoint
            .heads
            .iter()
            .map(|head| ((head.author, head.log_id), head.seq_num))
            .collect();
        let mut tail = Vec::new();
        for record in self.load_operations().await? {
            let operation = &record.operation;
            let key = (*operation.header.verifying_key.as_bytes(), record.log_id);
            if heads
                .get(&key)
                .is_none_or(|seq_num| operation.header.seq_num > *seq_num)
            {
                tail.push((
                    key.0,
                    key.1,
                    operation.header.seq_num,
                    *operation.hash.as_bytes(),
                ));
            }
        }
        tail.sort();
        Ok(KnotTailReceipt {
            checkpoint: checkpoint_id,
            operations: tail
                .into_iter()
                .map(|(_, _, _, operation)| operation)
                .collect(),
        })
    }

    /// Produce communal Knot's dry-run proposal without translating document
    /// events through the Commons graph or chat grammars.
    pub async fn communal_epoch_pruning_proposal(
        &self,
        keys: &DataKeyring,
        checkpoint_authority_revision: Digest,
        current_authority_revision: Digest,
        authority_reevaluation_epochs: &[GroupSecretId],
        offline_members: &[KnotOfflineMemberEpochHold],
    ) -> Result<EpochPruningProposal, KnotSyncError> {
        if self.policy.encryption != KnotEncryptionProfile::CommonsDataV1 {
            return Err(KnotSyncError::WrongEncryptionProfile);
        }
        let records = self.load_operations().await?;
        let by_operation: BTreeMap<_, _> = records
            .iter()
            .map(|record| (*record.operation.hash.as_bytes(), record))
            .collect();
        let mut holds = Vec::new();

        let checkpoint = if let Some(checkpoint) = self.load_checkpoint().await? {
            let tail = self.tail_receipt().await?;
            // Checkpoints retain relation history for inspection but are not yet
            // executable replay bases. A closed relation or file capture needs
            // its older source revision for quote validation, so preserve every
            // closed operation epoch until snapshot-base replay includes those
            // captures. Pending events remain covered by the existing tail and
            // pending holds below.
            let causal_entries = causal_entries(&records);
            let closed = causal_projection(&causal_entries)?;
            let mut has_capture_material = false;
            for &index in &closed.order {
                if matches!(
                    decode_event(KnotSyncCipher::CommonsData(keys), &records[index].operation)?,
                    KnotSyncEvent::CaptureFileRevision(_)
                        | KnotSyncEvent::AssertRelation { .. }
                        | KnotSyncEvent::RetractRelation { .. }
                        // A definition is replay material: an assertion's
                        // display and IRI resolve through its chain.
                        | KnotSyncEvent::DefinePredicate { .. }
                ) {
                    has_capture_material = true;
                    break;
                }
            }
            if has_capture_material {
                for index in closed.order {
                    let record = &records[index];
                    holds.push(EpochHold {
                        epoch: communal_operation_epoch(&record.operation)?,
                        reason: EpochHoldReason::DecryptionReachability,
                    });
                }
            }
            for operation in &tail.operations {
                let record = by_operation.get(operation).ok_or_else(|| {
                    KnotSyncError::Payload(
                        "checkpoint tail names an operation absent from the retained store".into(),
                    )
                })?;
                holds.push(EpochHold {
                    epoch: communal_operation_epoch(&record.operation)?,
                    reason: EpochHoldReason::DecryptionReachability,
                });
            }
            for (operation, _) in &checkpoint.pending {
                let record = by_operation.get(operation).ok_or_else(|| {
                    KnotSyncError::Payload(
                        "checkpoint pending set names an operation absent from the retained store"
                            .into(),
                    )
                })?;
                holds.push(EpochHold {
                    epoch: communal_operation_epoch(&record.operation)?,
                    reason: EpochHoldReason::PendingCausality,
                });
            }
            let current = self
                .build_checkpoint_with_cipher(KnotSyncCipher::CommonsData(keys))
                .await?;
            let author_continuation_ready = checkpoint.snapshot.is_some()
                && tail.operations.is_empty()
                && current == checkpoint;
            let bytes = serde_json::to_vec(&checkpoint)
                .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
            Some(EpochCheckpointBasis {
                checkpoint: Digest::blake3(&bytes),
                authority_revision: checkpoint_authority_revision,
                current_authority_revision,
                author_continuation_ready,
            })
        } else {
            for record in &records {
                holds.push(EpochHold {
                    epoch: communal_operation_epoch(&record.operation)?,
                    reason: EpochHoldReason::DecryptionReachability,
                });
            }
            None
        };
        holds.extend(
            authority_reevaluation_epochs
                .iter()
                .copied()
                .map(|epoch| EpochHold {
                    epoch,
                    reason: EpochHoldReason::AuthorityReevaluation,
                }),
        );
        holds.extend(offline_members.iter().map(|hold| EpochHold {
            epoch: hold.epoch,
            reason: EpochHoldReason::OfflineMember(hold.member),
        }));
        Ok(propose_epoch_pruning(
            KNOT_COMMONS_ENCRYPTION_PROFILE,
            keys,
            &EpochRetentionFacts { checkpoint, holds },
        ))
    }

    /// Revalidate and explicitly execute a reviewed communal proposal.
    pub async fn execute_communal_epoch_pruning(
        &self,
        keys: &mut DataKeyring,
        reviewed: &EpochPruningProposal,
        checkpoint_authority_revision: Digest,
        current_authority_revision: Digest,
        authority_reevaluation_epochs: &[GroupSecretId],
        offline_members: &[KnotOfflineMemberEpochHold],
    ) -> Result<KnotEpochExecutionReceipt, KnotSyncError> {
        let _gate = self.mutation_gate.lock().await;
        let current = self
            .communal_epoch_pruning_proposal(
                keys,
                checkpoint_authority_revision.clone(),
                current_authority_revision,
                authority_reevaluation_epochs,
                offline_members,
            )
            .await?;
        if &current != reviewed {
            return Err(KnotSyncError::StaleRetentionProposal);
        }
        if !current.is_executable() {
            return Err(KnotSyncError::BlockedRetentionProposal);
        }
        let checkpoint = current
            .checkpoint
            .clone()
            .ok_or(KnotSyncError::BlockedRetentionProposal)?;
        let before = keys.to_bytes()?;
        let mut reduced = DataKeyring::from_bytes(&before)?;
        for epoch in &current.forget {
            if !reduced.forget_authorized(epoch) {
                return Err(KnotSyncError::StaleRetentionProposal);
            }
        }
        let after = reduced.to_bytes()?;
        let receipt = KnotEpochExecutionReceipt {
            version: 1,
            space_id: self.policy.space_id,
            checkpoint,
            authority_revision: checkpoint_authority_revision,
            forgotten: current.forget,
            retained: reduced
                .epochs_oldest_first()
                .ok_or_else(|| {
                    KnotSyncError::Payload(
                        "executed keyring lost its proven epoch chronology".into(),
                    )
                })?
                .to_vec(),
            previous_keyring: Digest::blake3(&before),
            persisted_keyring: Digest::blake3(&after),
        };
        self.store
            .backend()
            .apply(&[
                WriteOp::Put {
                    key: communal_keyring_key(self.policy.space_id),
                    value: after,
                },
                WriteOp::Put {
                    key: communal_epoch_receipt_key(self.policy.space_id),
                    value: serde_json::to_vec(&receipt)
                        .map_err(|error| KnotSyncError::Payload(error.to_string()))?,
                },
            ])
            .await?;
        *keys = reduced;
        Ok(receipt)
    }

    pub async fn restore_communal_keyring(
        &self,
        keys: &mut DataKeyring,
    ) -> Result<bool, KnotSyncError> {
        let Some(bytes) = self
            .store
            .backend()
            .get(&communal_keyring_key(self.policy.space_id))
            .await?
        else {
            return Ok(false);
        };
        *keys = DataKeyring::from_bytes(&bytes)?;
        Ok(true)
    }

    pub async fn communal_epoch_execution_receipt(
        &self,
    ) -> Result<Option<KnotEpochExecutionReceipt>, KnotSyncError> {
        let Some(bytes) = self
            .store
            .backend()
            .get(&communal_epoch_receipt_key(self.policy.space_id))
            .await?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| KnotSyncError::Payload(error.to_string()))
    }

    pub async fn communal_offline_member_recovery(
        &self,
        keys: &DataKeyring,
        required_epoch: GroupSecretId,
    ) -> Result<KnotOfflineMemberRecovery, KnotSyncError> {
        if keys.contains(&required_epoch) {
            return Ok(KnotOfflineMemberRecovery::Resume);
        }
        let checkpoint = self
            .load_checkpoint()
            .await?
            .map(|checkpoint| {
                serde_json::to_vec(&checkpoint)
                    .map(|bytes| Digest::blake3(&bytes))
                    .map_err(|error| KnotSyncError::Payload(error.to_string()))
            })
            .transpose()?;
        Ok(KnotOfflineMemberRecovery::BootstrapRequired { checkpoint })
    }

    /// Return the raw replication store for read and transport plumbing.
    ///
    /// Direct writes through this handle bypass Knot's mutation gate. Normal
    /// replication must submit accepted operations through [`Self::accept`].
    pub fn sync_store(&self) -> MunimentStore<B, KnotSyncExt> {
        self.store.clone()
    }
}

impl<B> KnotSyncStore<B>
where
    B: Backend + Clone + Send + Sync + 'static,
{
    /// Join the real p2panda LogSync lane with Knot's admission closure.
    pub async fn join(
        &self,
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Result<JoinedSpace<KnotSyncExt>, JoinError> {
        let accept_store = self.clone();
        JoinedSpace::join::<_, u64, _, _>(
            // Scoped to kind AND space: a persona's vault space and any other
            // Knot space on one endpoint would otherwise share a protocol id.
            stickleback::lane_id("knot/vault-space/v1", self.policy.space_id),
            self.sync_store(),
            endpoint,
            gossip,
            self.policy.space_id,
            move |operation: Operation<KnotSyncExt>| {
                let store = accept_store.clone();
                async move { matches!(store.accept(&operation).await, Ok(true)) }
            },
        )
        .await
    }
}

fn causal_entries(records: &[StoredKnotOperation]) -> Vec<CausalEntry<u64>> {
    records
        .iter()
        .map(|record| {
            CausalEntry::from_operation(
                &record.operation,
                record.log_id,
                record.operation.header.extensions.parents.clone(),
            )
        })
        .collect()
}

fn checkpoint_key(space_id: [u8; 32]) -> String {
    let hex: String = space_id.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("knot-sync/checkpoint/{hex}")
}

fn communal_keyring_key(space_id: [u8; 32]) -> String {
    let hex: String = space_id.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("knot-sync/commons-keyring/{hex}")
}

fn communal_epoch_receipt_key(space_id: [u8; 32]) -> String {
    let hex: String = space_id.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("knot-sync/commons-epoch-receipt/{hex}")
}

fn communal_operation_epoch(
    operation: &Operation<KnotSyncExt>,
) -> Result<GroupSecretId, KnotSyncError> {
    let body = operation
        .body
        .as_ref()
        .ok_or_else(|| KnotSyncError::Payload("operation body is absent".into()))?;
    let envelope: GroupCiphertext = decode_cbor(body.to_bytes().as_slice())
        .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
    Ok(envelope.epoch)
}

fn operation_aad(space_id: [u8; 32], author: &[u8; 32], seq_num: u32) -> Vec<u8> {
    let mut aad = Vec::with_capacity(SYNC_AAD.len() + 68);
    aad.extend_from_slice(SYNC_AAD);
    aad.extend_from_slice(&space_id);
    aad.extend_from_slice(author);
    aad.extend_from_slice(&seq_num.to_le_bytes());
    aad
}

fn seal_event(
    cipher: KnotSyncCipher<'_>,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, KnotSyncError> {
    match cipher {
        KnotSyncCipher::Personal(vault) => vault
            .seal_sync_payload(aad, plaintext)
            .map_err(KnotSyncError::Payload),
        KnotSyncCipher::CommonsData(keys) => {
            let envelope = keys.seal_random(plaintext)?;
            encode_cbor(&envelope).map_err(|error| KnotSyncError::Payload(error.to_string()))
        },
    }
}

fn decode_event(
    cipher: KnotSyncCipher<'_>,
    operation: &Operation<KnotSyncExt>,
) -> Result<KnotSyncEvent, KnotSyncError> {
    let body = operation
        .body
        .as_ref()
        .ok_or_else(|| KnotSyncError::Payload("operation body is absent".into()))?;
    let aad = operation_aad(
        operation.header.extensions.space_id,
        operation.header.verifying_key.as_bytes(),
        operation.header.seq_num,
    );
    let plaintext = Zeroizing::new(match cipher {
        KnotSyncCipher::Personal(vault) => vault
            .unseal_sync_payload(&aad, &body.to_bytes())
            .map_err(KnotSyncError::Payload)?,
        KnotSyncCipher::CommonsData(keys) => {
            let envelope: GroupCiphertext = decode_cbor(body.to_bytes().as_slice())
                .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
            keys.open(&envelope)?
        },
    });
    serde_json::from_slice(plaintext.as_slice())
        .map_err(|error| KnotSyncError::Payload(error.to_string()))
}

#[derive(Default)]
struct RelationFold {
    relations: Vec<KnotRelationAssertionV1>,
    rejected: Vec<KnotRejectedRelationV1>,
    unverified: Vec<KnotUnverifiedRelationV1>,
    catalog: KnotPredicateCatalogV1,
}

#[derive(Clone, Copy)]
enum RelationLocation {
    Valid(usize),
    Unverified(usize),
}

struct RelationState {
    author: [u8; 32],
    location: RelationLocation,
}

enum RelationCaptureError {
    Rejected(String),
    Unverified(String),
}

fn fold_relations(
    records: &[StoredKnotOperation],
    events: &BTreeMap<usize, KnotSyncEvent>,
    order: &[usize],
    causal: &CausalIndex<'_, u64>,
    scope: [u8; 32],
) -> Result<RelationFold, KnotSyncError> {
    let mut documents = BTreeMap::<[u8; 32], VaultDocument>::new();
    let mut assertions = BTreeMap::<[u8; 32], RelationState>::new();
    let mut fold = RelationFold::default();

    for &index in order {
        let operation = &records[index].operation;
        let operation_id = *operation.hash.as_bytes();
        let author = *operation.header.verifying_key.as_bytes();
        let asserted_at_ms = operation.header.extensions.asserted_at_ms;
        let Some(event) = events.get(&index).cloned() else {
            continue;
        };
        match event {
            KnotSyncEvent::Put(document) => {
                documents.insert(operation_id, document);
            },
            KnotSyncEvent::Resolve {
                document: Some(document),
                ..
            } => {
                documents.insert(operation_id, document);
            },
            KnotSyncEvent::CaptureFileRevision(revision) => {
                if revision.validate().is_ok() {
                    documents.insert(operation_id, file_revision_as_vault_document(&revision));
                }
            },
            KnotSyncEvent::DefinePredicate {
                slug,
                label,
                description,
                subproperty_of,
                lowers_to,
                supersedes,
                replaced_by,
            } => {
                let proposal = PredicateProposal {
                    slug,
                    label,
                    description,
                    subproperty_of,
                    lowers_to,
                    supersedes,
                    replaced_by,
                };
                let origin = PredicateOrigin {
                    operation: operation_id,
                    author,
                    scope,
                    asserted_at_ms,
                };
                match fold_predicate_definition(&fold.catalog, causal, origin, proposal) {
                    Ok(definition) => fold.catalog.definitions.push(definition),
                    Err(reason) => fold.catalog.rejected.push(KnotRejectedPredicateV1 {
                        operation: operation_id,
                        author,
                        reason,
                    }),
                }
            },
            KnotSyncEvent::AssertRelation {
                predicate,
                subject,
                object,
                evidence,
                qualification,
            } => {
                if let Err(reason) = validate_relation_payload(&predicate, &subject, &object) {
                    fold.rejected.push(KnotRejectedRelationV1 {
                        operation: operation_id,
                        author,
                        reason,
                    });
                    continue;
                }
                if let Err(reason) =
                    validate_asserted_predicate(&fold.catalog, causal, &predicate, operation_id)
                {
                    fold.rejected.push(KnotRejectedRelationV1 {
                        operation: operation_id,
                        author,
                        reason,
                    });
                    continue;
                }
                let assertion = KnotRelationAssertionV1 {
                    id: operation_id,
                    author,
                    operation: operation_id,
                    scope,
                    predicate,
                    subject,
                    object,
                    evidence,
                    qualification,
                    retractions: Vec::new(),
                    asserted_at_ms,
                };
                match validate_relation_capture(&assertion, &documents, causal) {
                    Ok(()) => {
                        let index = fold.relations.len();
                        assertions.insert(
                            operation_id,
                            RelationState {
                                author,
                                location: RelationLocation::Valid(index),
                            },
                        );
                        fold.relations.push(assertion);
                    },
                    Err(RelationCaptureError::Unverified(reason)) => {
                        let index = fold.unverified.len();
                        assertions.insert(
                            operation_id,
                            RelationState {
                                author,
                                location: RelationLocation::Unverified(index),
                            },
                        );
                        fold.unverified
                            .push(KnotUnverifiedRelationV1 { assertion, reason });
                    },
                    Err(RelationCaptureError::Rejected(reason)) => {
                        fold.rejected.push(KnotRejectedRelationV1 {
                            operation: operation_id,
                            author,
                            reason,
                        });
                    },
                }
            },
            KnotSyncEvent::RetractRelation { assertion } => {
                let Some(state) = assertions.get(&assertion) else {
                    fold.rejected.push(KnotRejectedRelationV1 {
                        operation: operation_id,
                        author,
                        reason: "retraction names no valid or unverified relation assertion".into(),
                    });
                    continue;
                };
                if state.author != author {
                    fold.rejected.push(KnotRejectedRelationV1 {
                        operation: operation_id,
                        author,
                        reason: "only the assertion's signed author may retract it".into(),
                    });
                    continue;
                }
                if !causal.happens_before(assertion, operation_id) {
                    fold.rejected.push(KnotRejectedRelationV1 {
                        operation: operation_id,
                        author,
                        reason: "retraction does not causally observe its assertion".into(),
                    });
                    continue;
                }
                let retraction = KnotRelationRetractionV1 {
                    operation: operation_id,
                    author,
                };
                match state.location {
                    RelationLocation::Valid(index) => {
                        fold.relations[index].retractions.push(retraction)
                    },
                    RelationLocation::Unverified(index) => {
                        fold.unverified[index]
                            .assertion
                            .retractions
                            .push(retraction);
                    },
                }
            },
            KnotSyncEvent::Delete { .. } | KnotSyncEvent::Resolve { document: None, .. } => {},
        }
    }
    Ok(fold)
}

/// The signed facts a definition takes from its operation rather than its payload.
#[derive(Clone, Copy)]
struct PredicateOrigin {
    operation: [u8; 32],
    author: [u8; 32],
    scope: [u8; 32],
    asserted_at_ms: Option<u64>,
}

/// One `DefinePredicate` payload.
struct PredicateProposal {
    slug: String,
    label: String,
    description: Option<String>,
    subproperty_of: Option<String>,
    lowers_to: Option<String>,
    supersedes: Option<[u8; 32]>,
    replaced_by: Option<KnotPredicateReplacementV1>,
}

/// Admit one predicate definition into a catalog, or say why not.
fn fold_predicate_definition(
    catalog: &KnotPredicateCatalogV1,
    causal: &CausalIndex<'_, u64>,
    origin: PredicateOrigin,
    proposal: PredicateProposal,
) -> Result<KnotPredicateDefinitionV1, String> {
    validate_predicate_definition_payload(
        &proposal.slug,
        &proposal.label,
        proposal.description.as_deref(),
        proposal.subproperty_of.as_deref(),
        proposal.lowers_to.as_deref(),
    )?;
    let (root, iri) = match proposal.supersedes {
        None => {
            let root = origin.operation;
            (root, mint_predicate_iri(&origin.author, root))
        },
        Some(previous) => {
            let previous = catalog.definition(previous).ok_or_else(|| {
                "definition supersedes an unavailable predicate definition".to_owned()
            })?;
            // Only the minting key renames or retires.
            if previous.author != origin.author {
                return Err(
                    "only the minting author may supersede a predicate definition".to_owned(),
                );
            }
            if !causal.happens_before(previous.id, origin.operation) {
                return Err(
                    "definition does not causally observe the definition it supersedes".to_owned(),
                );
            }
            // The slug is immutable across a chain; the label is the mutable face.
            if previous.slug != proposal.slug {
                return Err("a superseding definition keeps the chain's slug".to_owned());
            }
            (previous.root, previous.iri.clone())
        },
    };
    if proposal.supersedes.is_none()
        && catalog.definitions.iter().any(|definition| {
            definition.root == definition.id
                && definition.author == origin.author
                && definition.scope == origin.scope
                && definition.slug == proposal.slug
                && !catalog.is_retired(definition.id)
        })
    {
        // Cross-author collisions are allowed; the minted IRI disambiguates.
        return Err(format!(
            "predicate slug '{}' is already minted and live for this author in this scope",
            proposal.slug
        ));
    }
    Ok(KnotPredicateDefinitionV1 {
        id: origin.operation,
        author: origin.author,
        operation: origin.operation,
        scope: origin.scope,
        root,
        iri,
        slug: proposal.slug,
        label: proposal.label,
        description: proposal.description,
        subproperty_of: proposal.subproperty_of,
        lowers_to: proposal.lowers_to,
        supersedes: proposal.supersedes,
        replaced_by: proposal.replaced_by,
        asserted_at_ms: origin.asserted_at_ms,
    })
}

/// Catalog questions a `Defined` predicate reference has to answer at replay.
fn validate_asserted_predicate(
    catalog: &KnotPredicateCatalogV1,
    causal: &CausalIndex<'_, u64>,
    predicate: &KnotPredicateRefV1,
    assertion: [u8; 32],
) -> Result<(), String> {
    let KnotPredicateRefV1::Defined(id) = predicate else {
        return Ok(());
    };
    let Some(root) = catalog.root_of(*id) else {
        return Err("relation names an unavailable predicate definition".to_owned());
    };
    if !causal.happens_before(*id, assertion) {
        return Err("assertion does not causally observe its predicate definition".to_owned());
    }
    // A retirement concurrent with the assertion does not reject it.
    if catalog.definitions.iter().any(|definition| {
        definition.root == root
            && definition.replaced_by.is_some()
            && causal.happens_before(definition.id, assertion)
    }) {
        return Err("relation names a retired predicate definition".to_owned());
    }
    Ok(())
}

fn validate_relation_capture(
    assertion: &KnotRelationAssertionV1,
    documents: &BTreeMap<[u8; 32], VaultDocument>,
    causal: &CausalIndex<'_, u64>,
) -> Result<(), RelationCaptureError> {
    for (role, endpoint) in [
        ("subject", &assertion.subject),
        ("object", &assertion.object),
    ] {
        let Some(document) = documents.get(&endpoint.document_head) else {
            return Err(RelationCaptureError::Unverified(format!(
                "relation {role} document head is unavailable for capture verification"
            )));
        };
        if document.id != endpoint.document_id {
            return Err(RelationCaptureError::Rejected(format!(
                "relation {role} document id does not match its captured head"
            )));
        }
        if !causal.happens_before(endpoint.document_head, assertion.operation) {
            return Err(RelationCaptureError::Rejected(format!(
                "relation {role} assertion does not causally observe its document head"
            )));
        }
        validate_endpoint_capture(endpoint, document, role)?;
    }
    Ok(())
}

fn validate_endpoint_capture(
    endpoint: &KnotRelationEndpointV1,
    document: &VaultDocument,
    role: &str,
) -> Result<(), RelationCaptureError> {
    let Some(position) = endpoint.position else {
        return Ok(());
    };
    let source = std::str::from_utf8(&document.body).map_err(|_| {
        RelationCaptureError::Unverified(format!(
            "relation {role} document source is not UTF-8 for capture verification"
        ))
    })?;
    let start = usize::try_from(position.start).map_err(|_| {
        RelationCaptureError::Rejected(format!(
            "relation {role} UTF-8 byte start cannot fit this platform"
        ))
    })?;
    let end = usize::try_from(position.end).map_err(|_| {
        RelationCaptureError::Rejected(format!(
            "relation {role} UTF-8 byte end cannot fit this platform"
        ))
    })?;
    let Some(quote) = source.get(start..end) else {
        return Err(RelationCaptureError::Rejected(format!(
            "relation {role} position is not a valid UTF-8 source range"
        )));
    };
    if quote != endpoint.quote {
        return Err(RelationCaptureError::Rejected(format!(
            "relation {role} quote does not match the captured source range"
        )));
    }
    // Containment, never exact length: context truncates at the edges of the
    // source and at the character cap, so only the adjacency is checkable.
    if !endpoint.prefix.is_empty() && !source[..start].ends_with(&endpoint.prefix) {
        return Err(RelationCaptureError::Rejected(format!(
            "relation {role} prefix does not precede the captured source range"
        )));
    }
    if !endpoint.suffix.is_empty() && !source[end..].starts_with(&endpoint.suffix) {
        return Err(RelationCaptureError::Rejected(format!(
            "relation {role} suffix does not follow the captured source range"
        )));
    }
    Ok(())
}

fn document_versions(
    records: &[StoredKnotOperation],
    order: &[usize],
    cipher: KnotSyncCipher<'_>,
) -> Result<BTreeMap<[u8; 32], VaultDocument>, KnotSyncError> {
    let mut documents = BTreeMap::new();
    for &index in order {
        let operation = &records[index].operation;
        let operation_id = *operation.hash.as_bytes();
        match decode_event(cipher, operation)? {
            KnotSyncEvent::Put(document) => {
                documents.insert(operation_id, document);
            },
            KnotSyncEvent::Resolve {
                document: Some(document),
                ..
            } => {
                documents.insert(operation_id, document);
            },
            KnotSyncEvent::CaptureFileRevision(revision) => {
                if revision.validate().is_ok() {
                    documents.insert(operation_id, file_revision_as_vault_document(&revision));
                }
            },
            _ => {},
        }
    }
    Ok(documents)
}

fn frontier_observes(
    causal: &CausalIndex<'_, u64>,
    parents: &[[u8; 32]],
    operation: [u8; 32],
) -> bool {
    parents
        .iter()
        .any(|parent| *parent == operation || causal.happens_before(operation, *parent))
}

fn validate_local_relation_event(
    event: &KnotSyncEvent,
    author: [u8; 32],
    records: &[StoredKnotOperation],
    order: &[usize],
    cipher: KnotSyncCipher<'_>,
    causal: &CausalIndex<'_, u64>,
    parents: &[[u8; 32]],
    scope: [u8; 32],
) -> Result<(), KnotSyncError> {
    match event {
        KnotSyncEvent::CaptureFileRevision(revision) => {
            revision
                .validate()
                .map_err(KnotSyncError::InvalidFileRevision)?;
        },
        KnotSyncEvent::AssertRelation {
            predicate,
            subject,
            object,
            ..
        } => {
            validate_relation_payload(predicate, subject, object)
                .map_err(KnotSyncError::InvalidRelation)?;
            if let KnotPredicateRefV1::Defined(id) = predicate {
                let catalog = local_predicate_catalog(records, order, cipher, causal, scope)?;
                let root = catalog.root_of(*id).ok_or_else(|| {
                    KnotSyncError::InvalidRelation(
                        "relation names an unavailable predicate definition".into(),
                    )
                })?;
                if !frontier_observes(causal, parents, *id) {
                    return Err(KnotSyncError::InvalidRelation(
                        "assertion does not observe its predicate definition".into(),
                    ));
                }
                if catalog.definitions.iter().any(|definition| {
                    definition.root == root
                        && definition.replaced_by.is_some()
                        && frontier_observes(causal, parents, definition.id)
                }) {
                    return Err(KnotSyncError::InvalidRelation(
                        "relation names a retired predicate definition".into(),
                    ));
                }
            }
            let documents = document_versions(records, order, cipher)?;
            for (role, endpoint) in [("subject", subject), ("object", object)] {
                let document = documents.get(&endpoint.document_head).ok_or_else(|| {
                    KnotSyncError::InvalidRelation(format!(
                        "relation {role} document head is unavailable for capture verification"
                    ))
                })?;
                if document.id != endpoint.document_id {
                    return Err(KnotSyncError::InvalidRelation(format!(
                        "relation {role} document id does not match its captured head"
                    )));
                }
                if !frontier_observes(causal, parents, endpoint.document_head) {
                    return Err(KnotSyncError::InvalidRelation(format!(
                        "relation {role} assertion does not observe its document head"
                    )));
                }
                validate_endpoint_capture(endpoint, document, role).map_err(|error| {
                    KnotSyncError::InvalidRelation(match error {
                        RelationCaptureError::Rejected(message)
                        | RelationCaptureError::Unverified(message) => message,
                    })
                })?;
            }
        },
        KnotSyncEvent::RetractRelation { assertion } => {
            let events = order
                .iter()
                .map(|&index| {
                    decode_event(cipher, &records[index].operation).map(|event| (index, event))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            let fold = fold_relations(records, &events, order, causal, scope)?;
            let target = fold
                .relations
                .iter()
                .map(|relation| (&relation.id, relation.author))
                .chain(
                    fold.unverified
                        .iter()
                        .map(|relation| (&relation.assertion.id, relation.assertion.author)),
                )
                .find(|(id, _)| **id == *assertion)
                .ok_or_else(|| {
                    KnotSyncError::InvalidRelation(
                        "retraction names no valid or unverified relation assertion".into(),
                    )
                })?;
            if target.1 != author {
                return Err(KnotSyncError::InvalidRelation(
                    "only the assertion's signed author may retract it".into(),
                ));
            }
            if !frontier_observes(causal, parents, *assertion) {
                return Err(KnotSyncError::InvalidRelation(
                    "retraction does not causally observe its assertion".into(),
                ));
            }
        },
        KnotSyncEvent::DefinePredicate {
            slug,
            label,
            description,
            subproperty_of,
            lowers_to,
            supersedes,
            ..
        } => {
            validate_predicate_definition_payload(
                slug,
                label,
                description.as_deref(),
                subproperty_of.as_deref(),
                lowers_to.as_deref(),
            )
            .map_err(KnotSyncError::InvalidPredicate)?;
            if let Some(previous) = supersedes {
                let catalog = local_predicate_catalog(records, order, cipher, causal, scope)?;
                let previous = catalog.definition(*previous).ok_or_else(|| {
                    KnotSyncError::InvalidPredicate(
                        "definition supersedes an unavailable predicate definition".into(),
                    )
                })?;
                if previous.author != author {
                    return Err(KnotSyncError::InvalidPredicate(
                        "only the minting author may supersede a predicate definition".into(),
                    ));
                }
                if !frontier_observes(causal, parents, previous.id) {
                    return Err(KnotSyncError::InvalidPredicate(
                        "definition does not observe the definition it supersedes".into(),
                    ));
                }
                if previous.slug != *slug {
                    return Err(KnotSyncError::InvalidPredicate(
                        "a superseding definition keeps the chain's slug".into(),
                    ));
                }
            }
        },
        _ => {},
    }
    Ok(())
}

/// Fold the catalog the local writer can currently see.
fn local_predicate_catalog(
    records: &[StoredKnotOperation],
    order: &[usize],
    cipher: KnotSyncCipher<'_>,
    causal: &CausalIndex<'_, u64>,
    scope: [u8; 32],
) -> Result<KnotPredicateCatalogV1, KnotSyncError> {
    let events = order
        .iter()
        .map(|&index| decode_event(cipher, &records[index].operation).map(|event| (index, event)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(fold_relations(records, &events, order, causal, scope)?.catalog)
}

fn validate_resolution(
    causal: &CausalIndex<'_, u64>,
    event_documents: &BTreeMap<[u8; 32], String>,
    resolution: [u8; 32],
    id: &str,
    supersedes: &[[u8; 32]],
    document: Option<&VaultDocument>,
) -> Result<BTreeSet<[u8; 32]>, KnotSyncError> {
    if supersedes.is_empty() {
        return Err(KnotSyncError::InvalidResolution(
            "resolution names no document versions".into(),
        ));
    }
    if supersedes.len() > KNOT_CAUSAL_LIMITS.max_parents {
        return Err(KnotSyncError::InvalidResolution(format!(
            "resolution names {} versions; maximum is {}",
            supersedes.len(),
            KNOT_CAUSAL_LIMITS.max_parents
        )));
    }
    if document.is_some_and(|document| document.id != id) {
        return Err(KnotSyncError::InvalidResolution(
            "chosen document id does not match the resolution".into(),
        ));
    }
    let targets: BTreeSet<_> = supersedes.iter().copied().collect();
    if targets.len() != supersedes.len() {
        return Err(KnotSyncError::InvalidResolution(
            "resolution repeats a document version".into(),
        ));
    }
    for target in &targets {
        let Some(target_id) = event_documents.get(target) else {
            return Err(KnotSyncError::InvalidResolution(
                "resolution names an unavailable document version".into(),
            ));
        };
        if target_id != id {
            return Err(KnotSyncError::InvalidResolution(
                "resolution names a version of another document".into(),
            ));
        }
        if !causal.happens_before(*target, resolution) {
            return Err(KnotSyncError::InvalidResolution(
                "resolution names a version outside its causal history".into(),
            ));
        }
    }
    Ok(targets)
}

#[cfg(test)]
#[path = "sync_coordination_tests.rs"]
mod coordination_tests;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use personae::{IdentityProvider, InMemoryProvider};
    use tempfile::tempdir;
    use transport::{P2pandaTransport, PeerID, sync_overlay_topic};

    use super::*;
    use crate::relations::{KnotCorePredicateV1, capture_endpoint_context};

    const SPACE: [u8; 32] = [0x81; 32];
    const VAULT_KEY: [u8; 32] = [0x82; 32];

    fn doc(id: &str, body: &str) -> VaultDocument {
        VaultDocument {
            id: id.into(),
            title: id.into(),
            body: body.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }
    }

    fn file_revision(id: &str, body: &str) -> KnotFileRevisionV1 {
        KnotFileRevisionV1 {
            document_id: id.into(),
            title: "Catalog file".into(),
            media_type: "text/plain".into(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn identities() -> (InMemoryProvider, InMemoryProvider) {
        (
            InMemoryProvider::from_seed([0x83; 32]),
            InMemoryProvider::from_seed([0x84; 32]),
        )
    }

    fn captured_endpoint(
        document_id: &str,
        document_head: [u8; 32],
        source: &str,
        quote: &str,
    ) -> KnotRelationEndpointV1 {
        let start = source.find(quote).expect("fixture quote is present");
        let end = start + quote.len();
        let (prefix, suffix) = capture_endpoint_context(source, start, end);
        KnotRelationEndpointV1 {
            document_id: document_id.into(),
            document_head,
            quote: quote.into(),
            position: Some(crate::relations::KnotRelationPositionV1 {
                start: start as u64,
                end: end as u64,
            }),
            prefix,
            suffix,
        }
    }

    async fn author_unchecked<B>(
        store: &KnotSyncStore<B>,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        event: &KnotSyncEvent,
    ) -> Operation<KnotSyncExt>
    where
        B: Backend + Clone + Send + Sync + 'static,
    {
        let records = store.load_operations().await.unwrap();
        let entries = causal_entries(&records);
        let parents = observed_frontier(&entries).unwrap();
        author_unchecked_with_parents(store, signing_seed, vault, event, parents).await
    }

    async fn author_unchecked_with_parents<B>(
        store: &KnotSyncStore<B>,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        event: &KnotSyncEvent,
        parents: Vec<[u8; 32]>,
    ) -> Operation<KnotSyncExt>
    where
        B: Backend + Clone + Send + Sync + 'static,
    {
        let signing_key = SigningKey::from_bytes(&signing_seed);
        let author = signing_key.verifying_key();
        let records = store.load_operations().await.unwrap();
        let entries = causal_entries(&records);
        let (seq_num, backlink) = author_head(&entries, *author.as_bytes(), &LOG_ID).unwrap();
        let plaintext = Zeroizing::new(serde_json::to_vec(event).unwrap());
        let aad = operation_aad(store.space_id(), author.as_bytes(), seq_num);
        let ciphertext =
            seal_event(KnotSyncCipher::Personal(vault), &aad, plaintext.as_slice()).unwrap();
        let header = Header::builder()
            .body(&ciphertext)
            .seq_num(seq_num)
            .backlink(backlink.map(Hash::from))
            .build(
                &signing_key,
                KnotSyncExt {
                    space_id: store.space_id(),
                    encryption: KnotEncryptionProfile::PersonalVaultV1,
                    parents,
                    asserted_at_ms: None,
                },
            );
        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(Body::from_bytes(&ciphertext)),
        };
        store.accept(&operation).await.unwrap();
        operation
    }

    #[tokio::test]
    async fn pending_undecipherable_operation_does_not_block_closed_document_projection() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let writer = alice.master_public_key().to_bytes();
        let vault = KnotVault::open(roots.path().join("trusted"), VAULT_KEY).unwrap();
        let foreign_vault = KnotVault::open(roots.path().join("foreign"), [0x85; 32]).unwrap();
        let store = KnotSyncStore::in_memory(SPACE, [writer]);
        store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("closed", "trusted")),
            )
            .await
            .unwrap();
        author_unchecked_with_parents(
            &store,
            alice.master_keypair().to_seed(),
            &foreign_vault,
            &KnotSyncEvent::Put(doc("pending", "unreadable")),
            vec![[0xfe; 32]],
        )
        .await;

        let projection = store.projection(&vault).await.unwrap();
        assert_eq!(projection.documents, vec![doc("closed", "trusted")]);
        assert_eq!(projection.pending.len(), 1);
    }

    /// The replay half of the unknown-label receipt. It lives here because
    /// only the crate can author past local validation; the authoring half is
    /// in tests/predicate_definitions.rs.
    #[tokio::test]
    async fn an_unknown_bare_label_decodes_and_is_rejected_at_replay() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let seed = alice.master_keypair().to_seed();
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncStore::in_memory(SPACE, [alice.master_public_key().to_bytes()]);
        let essay = "# Essay\nα holds throughout.\n";
        let essay_op = store
            .author(seed, &vault, &KnotSyncEvent::Put(doc("essay", essay)))
            .await
            .unwrap();
        let captured = captured_endpoint("essay", *essay_op.hash.as_bytes(), essay, "α holds");
        let event = KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Unrecognized("corroborates".into()),
            subject: captured.clone(),
            object: captured,
            evidence: None,
            qualification: None,
        };
        assert!(matches!(
            store.author(seed, &vault, &event).await,
            Err(KnotSyncError::InvalidRelation(_))
        ));
        let remote = author_unchecked(&store, seed, &vault, &event).await;

        let projection = store.projection(&vault).await.unwrap();
        assert!(projection.relations.is_empty());
        assert!(
            projection.rejected_relations.iter().any(|rejected| {
                rejected.operation == *remote.hash.as_bytes()
                    && rejected.reason.contains("corroborates")
            }),
            "an unknown label decodes rather than killing the projection, then is refused"
        );
    }

    #[tokio::test]
    async fn a_forged_endpoint_context_is_refused_at_authoring_and_rejected_at_replay() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let seed = alice.master_keypair().to_seed();
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncStore::in_memory(SPACE, [alice.master_public_key().to_bytes()]);
        let essay = "# Essay\nα holds throughout.\n";
        let essay_op = store
            .author(seed, &vault, &KnotSyncEvent::Put(doc("essay", essay)))
            .await
            .unwrap();
        let honest = captured_endpoint("essay", *essay_op.hash.as_bytes(), essay, "α holds");
        assert_eq!(honest.prefix, "# Essay\n");
        assert_eq!(honest.suffix, " throughout.\n");
        let assert_with = |endpoint: KnotRelationEndpointV1| KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
            subject: endpoint.clone(),
            object: endpoint,
            evidence: None,
            qualification: None,
        };
        store
            .author(seed, &vault, &assert_with(honest.clone()))
            .await
            .unwrap();

        let forged = KnotRelationEndpointV1 {
            prefix: "# Preface\n".into(),
            ..honest
        };
        assert!(matches!(
            store
                .author(seed, &vault, &assert_with(forged.clone()))
                .await,
            Err(KnotSyncError::InvalidRelation(_))
        ));
        let remote = author_unchecked(&store, seed, &vault, &assert_with(forged)).await;
        let projection = store.projection(&vault).await.unwrap();
        assert_eq!(projection.relations.len(), 1);
        assert!(
            projection.rejected_relations.iter().any(|rejected| {
                rejected.operation == *remote.hash.as_bytes() && rejected.reason.contains("prefix")
            }),
            "a forged prefix is rejected against the captured source"
        );
    }

    #[tokio::test]
    async fn relations_keep_independent_authors_and_retraction_history_across_checkpoint_reopen() {
        let roots = tempdir().unwrap();
        let database = roots.path().join("relations.redb");
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncFileStore::open(&database, SPACE, writers).unwrap();
        let essay = "Essay α";
        let source = "Source β";
        let essay_op = store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("essay", essay)),
            )
            .await
            .unwrap();
        let source_op = store
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("source", source)),
            )
            .await
            .unwrap();
        let relation_event = || KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
            subject: captured_endpoint("essay", *essay_op.hash.as_bytes(), essay, "α"),
            object: captured_endpoint("source", *source_op.hash.as_bytes(), source, "β"),
            evidence: Some("vault:evidence/one".into()),
            qualification: None,
        };
        let alice_relation = store
            .author(alice.master_keypair().to_seed(), &vault, &relation_event())
            .await
            .unwrap();
        let bob_relation = store
            .author(bob.master_keypair().to_seed(), &vault, &relation_event())
            .await
            .unwrap();
        let alice_retraction = store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::RetractRelation {
                    assertion: *alice_relation.hash.as_bytes(),
                },
            )
            .await
            .unwrap();

        assert!(matches!(
            store
                .author(
                    alice.master_keypair().to_seed(),
                    &vault,
                    &KnotSyncEvent::RetractRelation {
                        assertion: *bob_relation.hash.as_bytes(),
                    },
                )
                .await,
            Err(KnotSyncError::InvalidRelation(_))
        ));
        let foreign_retraction = author_unchecked(
            &store,
            alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::RetractRelation {
                assertion: *bob_relation.hash.as_bytes(),
            },
        )
        .await;
        let stale_capture = KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
            subject: KnotRelationEndpointV1 {
                quote: "stale".into(),
                ..captured_endpoint("essay", *essay_op.hash.as_bytes(), essay, "α")
            },
            object: captured_endpoint("source", *source_op.hash.as_bytes(), source, "β"),
            evidence: None,
            qualification: None,
        };
        assert!(matches!(
            store
                .author(alice.master_keypair().to_seed(), &vault, &stale_capture,)
                .await,
            Err(KnotSyncError::InvalidRelation(_))
        ));
        let stale_remote = author_unchecked(
            &store,
            alice.master_keypair().to_seed(),
            &vault,
            &stale_capture,
        )
        .await;

        let projection = store.projection(&vault).await.unwrap();
        assert_eq!(
            projection.documents,
            vec![doc("essay", essay), doc("source", source)]
        );
        assert_eq!(
            projection.document_heads,
            BTreeMap::from([
                ("essay".into(), *essay_op.hash.as_bytes()),
                ("source".into(), *source_op.hash.as_bytes()),
            ])
        );
        assert_eq!(projection.relations.len(), 2);
        let alice_assertion = projection
            .relations
            .iter()
            .find(|relation| relation.id == *alice_relation.hash.as_bytes())
            .unwrap();
        assert_eq!(alice_assertion.author, alice.master_public_key().to_bytes());
        assert_eq!(alice_assertion.scope, SPACE);
        assert_eq!(alice_assertion.retractions.len(), 1);
        assert_eq!(
            alice_assertion.retractions[0].operation,
            *alice_retraction.hash.as_bytes()
        );
        assert!(
            projection
                .relations
                .iter()
                .any(|relation| relation.id == *bob_relation.hash.as_bytes()
                    && relation.retractions.is_empty())
        );
        assert_eq!(projection.rejected_relations.len(), 2);
        assert!(
            projection
                .rejected_relations
                .iter()
                .any(|rejection| rejection.operation == *foreign_retraction.hash.as_bytes())
        );
        assert!(
            projection
                .rejected_relations
                .iter()
                .any(|rejection| rejection.operation == *stale_remote.hash.as_bytes())
        );
        assert_eq!(
            projection
                .relations_visible_to(&BTreeSet::from(["essay".into(), "source".into()]))
                .len(),
            1
        );
        assert!(
            projection
                .relation_history_visible_to(&BTreeSet::from(["essay".into(), "source".into()]))
                .len()
                == 2
        );
        assert!(
            projection
                .relations_visible_to(&BTreeSet::from(["essay".into()]))
                .is_empty()
        );

        let checkpoint = store.save_checkpoint(&vault).await.unwrap();
        assert_eq!(
            checkpoint.snapshot.as_ref().unwrap().relations,
            projection.relations
        );
        assert_eq!(
            checkpoint.snapshot.as_ref().unwrap().rejected_relations,
            projection.rejected_relations
        );
        drop(store);

        let reopened = KnotSyncFileStore::open(&database, SPACE, writers).unwrap();
        let rebuilt = reopened.projection(&vault).await.unwrap();
        assert_eq!(rebuilt, projection);
    }

    #[tokio::test]
    async fn communal_relation_history_holds_all_relation_epochs_after_a_checkpoint() {
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let writer = alice.master_public_key().to_bytes();
        let store = KnotSyncStore::in_memory_commons(SPACE, [writer]);
        let mut keys = DataKeyring::new();
        let document_epoch = keys.rotate_random().unwrap().id();
        let essay = "Essay α";
        let source = "Source β";
        let essay_op = store
            .author_communal(
                alice.master_keypair().to_seed(),
                &keys,
                &KnotSyncEvent::Put(doc("essay", essay)),
            )
            .await
            .unwrap();
        let source_op = store
            .author_communal(
                alice.master_keypair().to_seed(),
                &keys,
                &KnotSyncEvent::Put(doc("source", source)),
            )
            .await
            .unwrap();
        let assertion_epoch = keys.rotate_random().unwrap().id();
        let assertion = store
            .author_communal(
                alice.master_keypair().to_seed(),
                &keys,
                &KnotSyncEvent::AssertRelation {
                    predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                    subject: captured_endpoint("essay", *essay_op.hash.as_bytes(), essay, "α"),
                    object: captured_endpoint("source", *source_op.hash.as_bytes(), source, "β"),
                    evidence: None,
                    qualification: None,
                },
            )
            .await
            .unwrap();
        let retraction_epoch = keys.rotate_random().unwrap().id();
        store
            .author_communal(
                alice.master_keypair().to_seed(),
                &keys,
                &KnotSyncEvent::RetractRelation {
                    assertion: *assertion.hash.as_bytes(),
                },
            )
            .await
            .unwrap();
        for _ in 0..7 {
            keys.rotate_random().unwrap();
        }
        let checkpoint = store.save_communal_checkpoint(&keys).await.unwrap();
        assert!(store.tail_receipt().await.unwrap().operations.is_empty());
        assert_eq!(
            checkpoint.snapshot.as_ref().unwrap().relations[0]
                .retractions
                .len(),
            1
        );

        let revision = Digest::blake3(b"relation epoch holds");
        let proposal = store
            .communal_epoch_pruning_proposal(&keys, revision.clone(), revision, &[], &[])
            .await
            .unwrap();
        for epoch in [document_epoch, assertion_epoch, retraction_epoch] {
            assert!(proposal.retain.iter().any(|retained| {
                retained.epoch == epoch
                    && retained
                        .reasons
                        .contains(&stickleback::EpochRetentionReason::Domain(
                            EpochHoldReason::DecryptionReachability,
                        ))
            }));
        }
    }

    #[tokio::test]
    async fn communal_capture_before_any_relation_holds_its_epoch_after_checkpoint() {
        let alice = InMemoryProvider::from_seed([0x95; 32]);
        let writer = alice.master_public_key().to_bytes();
        let store = KnotSyncStore::in_memory_commons(SPACE, [writer]);
        let mut keys = DataKeyring::new();
        let capture_epoch = keys.rotate_random().unwrap().id();
        store
            .author_communal(
                alice.master_keypair().to_seed(),
                &keys,
                &KnotSyncEvent::CaptureFileRevision(file_revision("knot:document:capture", "old")),
            )
            .await
            .unwrap();
        for _ in 0..7 {
            keys.rotate_random().unwrap();
        }
        store.save_communal_checkpoint(&keys).await.unwrap();
        let revision = Digest::blake3(b"capture epoch holds");
        let proposal = store
            .communal_epoch_pruning_proposal(&keys, revision.clone(), revision, &[], &[])
            .await
            .unwrap();
        assert!(proposal.retain.iter().any(|retained| {
            retained.epoch == capture_epoch
                && retained
                    .reasons
                    .contains(&stickleback::EpochRetentionReason::Domain(
                        EpochHoldReason::DecryptionReachability,
                    ))
        }));
    }

    #[tokio::test]
    async fn malformed_and_unadmitted_file_captures_cannot_be_relation_targets() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let alice_writer = alice.master_public_key().to_bytes();
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncStore::in_memory(SPACE, [alice_writer]);
        let malformed = KnotFileRevisionV1 {
            document_id: "knot:document:malformed".into(),
            title: "Catalog file".into(),
            media_type: "text/plain\0".into(),
            body: b"quoted".to_vec(),
        };
        assert!(matches!(
            store
                .author(
                    alice.master_keypair().to_seed(),
                    &vault,
                    &KnotSyncEvent::CaptureFileRevision(malformed.clone()),
                )
                .await,
            Err(KnotSyncError::InvalidFileRevision(_))
        ));
        let unchecked = author_unchecked(
            &store,
            alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::CaptureFileRevision(malformed),
        )
        .await;
        assert!(
            store
                .file_revision(
                    &vault,
                    "knot:document:malformed",
                    *unchecked.hash.as_bytes()
                )
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            store
                .author(
                    alice.master_keypair().to_seed(),
                    &vault,
                    &KnotSyncEvent::AssertRelation {
                        predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                        subject: KnotRelationEndpointV1 {
                            document_id: "knot:document:malformed".into(),
                            document_head: *unchecked.hash.as_bytes(),
                            quote: String::new(),
                            position: None,
                            prefix: String::new(),
                            suffix: String::new(),
                        },
                        object: KnotRelationEndpointV1 {
                            document_id: "knot:document:malformed".into(),
                            document_head: *unchecked.hash.as_bytes(),
                            quote: String::new(),
                            position: None,
                            prefix: String::new(),
                            suffix: String::new(),
                        },
                        evidence: None,
                        qualification: None,
                    },
                )
                .await,
            Err(KnotSyncError::InvalidRelation(_))
        ));
        assert!(
            store
                .author(
                    bob.master_keypair().to_seed(),
                    &vault,
                    &KnotSyncEvent::CaptureFileRevision(file_revision(
                        "knot:document:other",
                        "body"
                    )),
                )
                .await
                .is_err()
        );

        let valid = store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::CaptureFileRevision(file_revision("knot:document:valid", "body")),
            )
            .await
            .unwrap();
        assert!(matches!(
            store
                .author(
                    alice.master_keypair().to_seed(),
                    &vault,
                    &KnotSyncEvent::AssertRelation {
                        predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                        subject: KnotRelationEndpointV1 {
                            document_id: "knot:document:valid".into(),
                            document_head: *valid.hash.as_bytes(),
                            quote: "wrong".into(),
                            position: Some(crate::relations::KnotRelationPositionV1 {
                                start: 0,
                                end: 4,
                            }),
                            prefix: String::new(),
                            suffix: String::new(),
                        },
                        object: KnotRelationEndpointV1 {
                            document_id: "knot:document:valid".into(),
                            document_head: *valid.hash.as_bytes(),
                            quote: "body".into(),
                            position: Some(crate::relations::KnotRelationPositionV1 {
                                start: 0,
                                end: 4,
                            }),
                            prefix: String::new(),
                            suffix: String::new(),
                        },
                        evidence: None,
                        qualification: None,
                    },
                )
                .await,
            Err(KnotSyncError::InvalidRelation(_))
        ));
    }

    fn paired_group_keys() -> (DataKeyring, DataKeyring) {
        let mut alice = DataKeyring::new();
        let secret = alice.rotate_random().unwrap();
        let mut bob = DataKeyring::new();
        bob.install(secret);
        (alice, bob)
    }

    #[tokio::test]
    async fn communal_proposal_keeps_pending_and_policy_held_epochs() {
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let mut keys = DataKeyring::new();
        let mut epochs = vec![keys.rotate_random().unwrap().id()];
        let parent_store = KnotSyncStore::in_memory_commons(SPACE, writers);
        let child_store = KnotSyncStore::in_memory_commons(SPACE, writers);
        let receiver = KnotSyncStore::in_memory_commons(SPACE, writers);
        let parent = parent_store
            .author_communal(
                alice.master_keypair().to_seed(),
                &keys,
                &KnotSyncEvent::Put(doc("shared", "parent")),
            )
            .await
            .unwrap();
        child_store.accept(&parent).await.unwrap();
        let child = child_store
            .author_communal(
                bob.master_keypair().to_seed(),
                &keys,
                &KnotSyncEvent::Put(doc("shared", "pending child")),
            )
            .await
            .unwrap();
        receiver.accept(&child).await.unwrap();
        for _ in 0..9 {
            epochs.push(keys.rotate_random().unwrap().id());
        }
        let checkpoint = receiver.save_communal_checkpoint(&keys).await.unwrap();
        assert_eq!(checkpoint.pending[0].0, *child.hash.as_bytes());

        let revision = Digest::blake3(b"commons authority");
        let pending = receiver
            .communal_epoch_pruning_proposal(&keys, revision.clone(), revision.clone(), &[], &[])
            .await
            .unwrap();
        assert!(pending.is_executable());
        assert_eq!(pending.forget, vec![epochs[1]]);
        assert!(pending.retain.iter().any(|retained| {
            retained.epoch == epochs[0]
                && retained
                    .reasons
                    .contains(&stickleback::EpochRetentionReason::Domain(
                        EpochHoldReason::PendingCausality,
                    ))
        }));

        let policy_held = receiver
            .communal_epoch_pruning_proposal(
                &keys,
                revision.clone(),
                revision,
                &[epochs[1]],
                &[KnotOfflineMemberEpochHold {
                    member: [0xc3; 32],
                    epoch: epochs[1],
                }],
            )
            .await
            .unwrap();
        assert!(policy_held.is_executable());
        assert!(policy_held.forget.is_empty());
    }

    #[tokio::test]
    async fn communal_execution_revalidates_commits_and_reopens() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("communal-retention.redb");
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let writer = alice.master_public_key().to_bytes();
        let seed = alice.master_keypair().to_seed();
        let mut keys = DataKeyring::new();
        let mut epochs = Vec::new();
        for _ in 0..10 {
            epochs.push(keys.rotate_random().unwrap().id());
        }
        let store = KnotSyncFileStore::open_commons(&database, SPACE, [writer]).unwrap();
        store
            .author_communal(
                seed,
                &keys,
                &KnotSyncEvent::Put(doc("one", "before checkpoint")),
            )
            .await
            .unwrap();
        store.save_communal_checkpoint(&keys).await.unwrap();
        let revision = Digest::blake3(b"commons authority");
        let stale = store
            .communal_epoch_pruning_proposal(&keys, revision.clone(), revision.clone(), &[], &[])
            .await
            .unwrap();

        store
            .author_communal(
                seed,
                &keys,
                &KnotSyncEvent::Put(doc("two", "new checkpoint")),
            )
            .await
            .unwrap();
        store.save_communal_checkpoint(&keys).await.unwrap();
        assert!(matches!(
            store
                .execute_communal_epoch_pruning(
                    &mut keys,
                    &stale,
                    revision.clone(),
                    revision.clone(),
                    &[],
                    &[],
                )
                .await,
            Err(KnotSyncError::StaleRetentionProposal)
        ));
        assert!(
            store
                .communal_epoch_execution_receipt()
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(keys.epoch_count(), 10);

        let reviewed = store
            .communal_epoch_pruning_proposal(&keys, revision.clone(), revision.clone(), &[], &[])
            .await
            .unwrap();
        let receipt = store
            .execute_communal_epoch_pruning(
                &mut keys,
                &reviewed,
                revision.clone(),
                revision,
                &[],
                &[],
            )
            .await
            .unwrap();
        assert_eq!(receipt.forgotten, epochs[..2]);
        assert_eq!(receipt.retained, epochs[2..]);
        assert_eq!(keys.epoch_count(), 8);
        drop(store);

        let reopened = KnotSyncFileStore::open_commons(&database, SPACE, [writer]).unwrap();
        let mut reopened_keys = DataKeyring::new();
        assert!(
            reopened
                .restore_communal_keyring(&mut reopened_keys)
                .await
                .unwrap()
        );
        assert_eq!(reopened_keys.epochs_oldest_first().unwrap(), &epochs[2..]);
        assert_eq!(
            reopened.communal_documents(&reopened_keys).await.unwrap(),
            vec![
                doc("one", "before checkpoint"),
                doc("two", "new checkpoint")
            ]
        );
        assert_eq!(
            reopened
                .communal_offline_member_recovery(&reopened_keys, epochs[2])
                .await
                .unwrap(),
            KnotOfflineMemberRecovery::Resume
        );
        assert!(matches!(
            reopened
                .communal_offline_member_recovery(&reopened_keys, epochs[0])
                .await
                .unwrap(),
            KnotOfflineMemberRecovery::BootstrapRequired {
                checkpoint: Some(_)
            }
        ));
        assert_eq!(
            reopened.communal_epoch_execution_receipt().await.unwrap(),
            Some(receipt)
        );
    }

    #[tokio::test]
    async fn two_memory_stores_converge_through_the_accept_seam() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let alice_seed = alice.master_keypair().to_seed();
        let bob_seed = bob.master_keypair().to_seed();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_vault = KnotVault::open(roots.path().join("alice"), VAULT_KEY).unwrap();
        let bob_vault = KnotVault::open(roots.path().join("bob"), VAULT_KEY).unwrap();

        let a_op = a
            .author(
                alice_seed,
                &alice_vault,
                &KnotSyncEvent::Put(doc("alice-note", "amber")),
            )
            .await
            .unwrap();
        let b_op = b
            .author(
                bob_seed,
                &bob_vault,
                &KnotSyncEvent::Put(doc("bob-note", "blue")),
            )
            .await
            .unwrap();
        assert!(a.accept(&b_op).await.unwrap());
        assert!(b.accept(&a_op).await.unwrap());

        assert_eq!(
            a.documents(&alice_vault).await.unwrap(),
            b.documents(&bob_vault).await.unwrap()
        );
    }

    #[tokio::test]
    async fn refreshed_writer_authority_changes_live_operation_admission() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let alice_writer = alice.master_public_key().to_bytes();
        let bob_writer = bob.master_public_key().to_bytes();
        let author = KnotSyncStore::in_memory(SPACE, [alice_writer]);
        let receiver = KnotSyncStore::in_memory(SPACE, [bob_writer]);
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let first = author
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("first", "admitted after pairing")),
            )
            .await
            .unwrap();

        assert!(receiver.accept(&first).await.is_err());
        assert!(receiver.admit_writer(alice_writer));
        assert!(receiver.accept(&first).await.unwrap());

        let second = author
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("second", "rejected after revocation")),
            )
            .await
            .unwrap();
        assert!(receiver.replace_admitted_writers([bob_writer]));
        assert!(receiver.accept(&second).await.is_err());
    }

    #[tokio::test]
    async fn commons_documents_use_group_epochs_instead_of_personal_vault_keys() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let (mut alice_keys, bob_keys) = paired_group_keys();
        let a = KnotSyncStore::in_memory_commons(SPACE, writers);
        let b = KnotSyncStore::in_memory_commons(SPACE, writers);
        let alice_vault = KnotVault::open(roots.path().join("alice"), [0x91; 32]).unwrap();
        let bob_vault = KnotVault::open(roots.path().join("bob"), [0x92; 32]).unwrap();

        let old = a
            .author_communal(
                alice.master_keypair().to_seed(),
                &alice_keys,
                &KnotSyncEvent::Put(doc("shared", "before removal")),
            )
            .await
            .unwrap();
        b.accept(&old).await.unwrap();
        assert_eq!(
            a.communal_documents(&alice_keys).await.unwrap(),
            b.communal_documents(&bob_keys).await.unwrap()
        );
        assert!(matches!(
            a.projection(&alice_vault).await,
            Err(KnotSyncError::WrongEncryptionProfile)
        ));
        assert!(matches!(
            b.projection(&bob_vault).await,
            Err(KnotSyncError::WrongEncryptionProfile)
        ));

        alice_keys.rotate_random().unwrap();
        let after_removal = a
            .author_communal(
                alice.master_keypair().to_seed(),
                &alice_keys,
                &KnotSyncEvent::Put(doc("new", "after removal")),
            )
            .await
            .unwrap();
        assert!(b.accept(&after_removal).await.unwrap());
        // The removed member's key opens nothing from the new epoch, and a
        // Commons projection is strict by default: losing an epoch is a
        // membership fact, not a quietly shortened document list.
        assert!(matches!(
            b.communal_projection(&bob_keys).await,
            Err(KnotSyncError::GroupCrypto(GroupCryptoError::UnknownEpoch(_)))
        ));
        assert_eq!(a.communal_documents(&alice_keys).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_tolerant_commons_projection_reads_the_subset_before_epoch_removal() {
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let (mut alice_keys, bob_keys) = paired_group_keys();
        let a = KnotSyncStore::in_memory_commons(SPACE, writers);
        let b = KnotSyncStore::in_memory_commons(SPACE, writers);

        let old = a
            .author_communal(
                alice.master_keypair().to_seed(),
                &alice_keys,
                &KnotSyncEvent::Put(doc("shared", "before removal")),
            )
            .await
            .unwrap();
        b.accept(&old).await.unwrap();

        alice_keys.rotate_random().unwrap();
        let after_removal = a
            .author_communal(
                alice.master_keypair().to_seed(),
                &alice_keys,
                &KnotSyncEvent::Put(doc("new", "after removal")),
            )
            .await
            .unwrap();
        assert!(b.accept(&after_removal).await.unwrap());

        // The same fixture, read tolerantly: what Bo could already read stays
        // readable, and the epoch it lost is a rejected record it can inspect.
        let removed = b
            .communal_projection_with_strictness(&bob_keys, KnotProjectionStrictness::Tolerant)
            .await
            .unwrap();
        assert_eq!(removed.documents, vec![doc("shared", "before removal")]);
        assert!(removed.rejected_relations.iter().any(|rejected| {
            rejected.operation == *after_removal.hash.as_bytes()
                && rejected.author == alice.master_public_key().to_bytes()
                && rejected.reason.contains("could not be decoded")
        }));
    }

    #[tokio::test]
    async fn a_personal_projection_defaults_to_tolerant() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let seed = alice.master_keypair().to_seed();
        let store = KnotSyncStore::in_memory(SPACE, [alice.master_public_key().to_bytes()]);
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let other = KnotVault::open(roots.path().join("other"), [0x5c; 32]).unwrap();
        store
            .author(seed, &vault, &KnotSyncEvent::Put(doc("first", "readable")))
            .await
            .unwrap();

        // A default personal projection is the tolerant one; the explicit
        // Tolerant call is the same projection.
        assert_eq!(
            store.projection(&vault).await.unwrap(),
            store
                .projection_with_strictness(&vault, KnotProjectionStrictness::Tolerant)
                .await
                .unwrap()
        );
        assert_eq!(
            store.projection(&vault).await.unwrap().documents,
            vec![doc("first", "readable")]
        );
        // The same history under a vault key that opens nothing: tolerant by
        // default records it, strict refuses it.
        let tolerated = store.projection(&other).await.unwrap();
        assert!(tolerated.documents.is_empty());
        assert_eq!(tolerated.rejected_relations.len(), 1);
        assert!(
            store
                .projection_with_strictness(&other, KnotProjectionStrictness::Strict)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_signed_encryption_profile_cannot_replay_into_another_knot_profile() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let writer = alice.master_public_key().to_bytes();
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let personal = KnotSyncStore::in_memory(SPACE, [writer]);
        let communal = KnotSyncStore::in_memory_commons(SPACE, [writer]);
        let operation = personal
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("note", "personal")),
            )
            .await
            .unwrap();

        assert!(communal.accept(&operation).await.is_err());
    }

    #[tokio::test]
    async fn concurrent_writers_for_one_document_are_refused_at_projection() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let alice_version = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        let bob_version = b
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "bob")),
            )
            .await
            .unwrap();
        a.accept(&bob_version).await.unwrap();
        b.accept(&alice_version).await.unwrap();
        a.author(
            alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("solo", "still visible")),
        )
        .await
        .unwrap();
        let projection = a.projection(&vault).await.unwrap();
        assert_eq!(projection.documents, vec![doc("solo", "still visible")]);
        assert_eq!(projection.conflicts.len(), 1);
        assert_eq!(projection.conflicts[0].id, "shared");
        assert_eq!(projection.conflicts[0].versions.len(), 2);
        assert!(projection.pending.is_empty());
        assert!(matches!(
            a.documents(&vault).await,
            Err(KnotSyncError::ConcurrentWriter(id)) if id == "shared"
        ));
    }

    #[tokio::test]
    async fn independent_text_edits_merge_and_a_later_put_makes_them_durable() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let base_store = KnotSyncStore::in_memory(SPACE, writers);
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let base = base_store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "first\nsecond\nthird\n")),
            )
            .await
            .unwrap();
        a.accept(&base).await.unwrap();
        b.accept(&base).await.unwrap();

        let left = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "FIRST\nsecond\nthird\n")),
            )
            .await
            .unwrap();
        let right = b
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "first\nsecond\nTHIRD\n")),
            )
            .await
            .unwrap();
        a.accept(&right).await.unwrap();
        b.accept(&left).await.unwrap();

        let a_projection = a.projection(&vault).await.unwrap();
        let b_projection = b.projection(&vault).await.unwrap();
        let merged = doc("shared", "FIRST\nsecond\nTHIRD\n");
        assert_eq!(a_projection.documents, vec![merged.clone()]);
        assert_eq!(a_projection, b_projection);
        assert!(a_projection.conflicts.is_empty());
        assert_eq!(a_projection.automatic_merges.len(), 1);
        assert_eq!(a_projection.automatic_merges[0].base, *base.hash.as_bytes());
        let mut supersedes = vec![*left.hash.as_bytes(), *right.hash.as_bytes()];
        supersedes.sort_unstable();
        assert_eq!(a_projection.automatic_merges[0].supersedes, supersedes);

        let durable = doc("shared", "FIRST\nsecond\nTHIRD\nafter merge\n");
        let durable_operation = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(durable.clone()),
            )
            .await
            .unwrap();
        b.accept(&durable_operation).await.unwrap();
        for projection in [
            a.projection(&vault).await.unwrap(),
            b.projection(&vault).await.unwrap(),
        ] {
            assert_eq!(projection.documents, vec![durable.clone()]);
            assert!(projection.conflicts.is_empty());
            assert!(projection.automatic_merges.is_empty());
            assert_eq!(
                projection.document_heads["shared"],
                *durable_operation.hash.as_bytes()
            );
        }
    }

    #[tokio::test]
    async fn overlapping_text_edits_remain_an_explicit_conflict() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let base_store = KnotSyncStore::in_memory(SPACE, writers);
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let base = base_store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "one\ntwo\n")),
            )
            .await
            .unwrap();
        a.accept(&base).await.unwrap();
        b.accept(&base).await.unwrap();
        let left = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice\ntwo\n")),
            )
            .await
            .unwrap();
        let right = b
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "bob\ntwo\n")),
            )
            .await
            .unwrap();
        a.accept(&right).await.unwrap();
        b.accept(&left).await.unwrap();

        for projection in [
            a.projection(&vault).await.unwrap(),
            b.projection(&vault).await.unwrap(),
        ] {
            assert!(projection.documents.is_empty());
            assert!(projection.automatic_merges.is_empty());
            assert_eq!(projection.conflicts.len(), 1);
            assert_eq!(projection.conflicts[0].id, "shared");
        }
    }

    #[tokio::test]
    async fn an_explicit_resolution_replaces_exact_conflicting_versions() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_op = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        let bob_op = b
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "bob")),
            )
            .await
            .unwrap();
        a.accept(&bob_op).await.unwrap();
        b.accept(&alice_op).await.unwrap();
        let conflict = a.projection(&vault).await.unwrap().conflicts.remove(0);
        let resolution = a
            .resolve_conflict(
                alice.master_keypair().to_seed(),
                &vault,
                &conflict,
                Some(doc("shared", "chosen")),
            )
            .await
            .unwrap();
        b.accept(&resolution).await.unwrap();

        assert_eq!(
            a.documents(&vault).await.unwrap(),
            vec![doc("shared", "chosen")]
        );
        assert_eq!(
            a.documents(&vault).await.unwrap(),
            b.documents(&vault).await.unwrap()
        );
    }

    #[tokio::test]
    async fn a_resolution_does_not_erase_an_unseen_concurrent_version() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_op = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        let local = KnotDocumentConflict {
            id: "shared".into(),
            versions: vec![KnotDocumentVersion {
                writer: alice.master_public_key().to_bytes(),
                operation: *alice_op.hash.as_bytes(),
                document: Some(doc("shared", "alice")),
            }],
        };
        a.resolve_conflict(
            alice.master_keypair().to_seed(),
            &vault,
            &local,
            Some(doc("shared", "alice resolved")),
        )
        .await
        .unwrap();
        let bob_op = b
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "bob unseen")),
            )
            .await
            .unwrap();
        a.accept(&bob_op).await.unwrap();

        let projection = a.projection(&vault).await.unwrap();
        assert_eq!(projection.conflicts.len(), 1);
        assert_eq!(projection.conflicts[0].versions.len(), 2);
    }

    #[tokio::test]
    async fn a_resolution_cannot_name_a_version_outside_its_causal_history() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_op = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        let forged_conflict = KnotDocumentConflict {
            id: "shared".into(),
            versions: vec![KnotDocumentVersion {
                writer: alice.master_public_key().to_bytes(),
                operation: *alice_op.hash.as_bytes(),
                document: Some(doc("shared", "alice")),
            }],
        };
        let forged_resolution = b
            .resolve_conflict(
                bob.master_keypair().to_seed(),
                &vault,
                &forged_conflict,
                Some(doc("shared", "forged")),
            )
            .await
            .unwrap();
        a.accept(&forged_resolution).await.unwrap();

        assert!(matches!(
            a.projection(&vault).await,
            Err(KnotSyncError::InvalidResolution(_))
        ));
    }

    #[tokio::test]
    async fn missing_history_blocks_only_its_document_branch() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let bob = InMemoryProvider::from_seed([0x84; 32]);
        let carol = InMemoryProvider::from_seed([0x85; 32]);
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
            carol.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let parent_store = KnotSyncStore::in_memory(SPACE, writers);
        let child_store = KnotSyncStore::in_memory(SPACE, writers);
        let unrelated_store = KnotSyncStore::in_memory(SPACE, writers);
        let receiver = KnotSyncStore::in_memory(SPACE, writers);

        let parent = parent_store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "parent")),
            )
            .await
            .unwrap();
        child_store.accept(&parent).await.unwrap();
        let child = child_store
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "child")),
            )
            .await
            .unwrap();
        let unrelated = unrelated_store
            .author(
                carol.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("solo", "visible")),
            )
            .await
            .unwrap();

        receiver.accept(&child).await.unwrap();
        receiver.accept(&unrelated).await.unwrap();
        let partial = receiver.projection(&vault).await.unwrap();
        assert_eq!(partial.documents, vec![doc("solo", "visible")]);
        assert_eq!(partial.pending.len(), 1);
        assert_eq!(partial.pending[0].operation, *child.hash.as_bytes());
        assert_eq!(partial.pending[0].missing, vec![*parent.hash.as_bytes()]);

        receiver.accept(&parent).await.unwrap();
        let complete = receiver.projection(&vault).await.unwrap();
        assert!(complete.pending.is_empty());
        assert!(complete.conflicts.is_empty());
        assert_eq!(
            complete.documents,
            vec![doc("shared", "child"), doc("solo", "visible")]
        );
    }

    #[tokio::test]
    async fn redb_reopen_restores_author_head_and_observed_frontier() {
        let roots = tempdir().unwrap();
        let database = roots.path().join("knot-sync.redb");
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let writer = alice.master_public_key().to_bytes();
        let seed = alice.master_keypair().to_seed();

        let second = {
            let store = KnotSyncFileStore::open(&database, SPACE, [writer]).unwrap();
            store
                .author(seed, &vault, &KnotSyncEvent::Put(doc("one", "first")))
                .await
                .unwrap();
            let second = store
                .author(seed, &vault, &KnotSyncEvent::Put(doc("two", "second")))
                .await
                .unwrap();
            let checkpoint = store.save_checkpoint(&vault).await.unwrap();
            assert_eq!(checkpoint.heads.len(), 1);
            assert_eq!(checkpoint.heads[0].operation, *second.hash.as_bytes());
            second
        };

        let reopened = KnotSyncFileStore::open(&database, SPACE, [writer]).unwrap();
        assert_eq!(
            reopened.load_checkpoint().await.unwrap().unwrap().heads[0].operation,
            *second.hash.as_bytes()
        );
        let third = reopened
            .author(seed, &vault, &KnotSyncEvent::Put(doc("three", "third")))
            .await
            .unwrap();
        assert_eq!(third.header.seq_num, second.header.seq_num + 1);
        assert_eq!(
            third.header.backlink.as_ref().map(|hash| *hash.as_bytes()),
            Some(*second.hash.as_bytes())
        );
        assert_eq!(
            third.header.extensions.parents,
            vec![*second.hash.as_bytes()]
        );
        assert_eq!(
            reopened.tail_receipt().await.unwrap().operations,
            vec![*third.hash.as_bytes()]
        );
        assert_eq!(
            reopened.projection(&vault).await.unwrap().documents.len(),
            3
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_instances_converge_over_real_p2panda_logsync() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let alice_id = PeerID::from_public_key(alice.master_public_key());
        let bob_id = PeerID::from_public_key(bob.master_public_key());
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let alice_transport = P2pandaTransport::builder(alice.master_keypair())
            .gossip()
            .bind()
            .await
            .unwrap();
        let bob_transport = P2pandaTransport::builder(bob.master_keypair())
            .gossip()
            .bind()
            .await
            .unwrap();
        let overlay = sync_overlay_topic(SPACE);
        alice_transport
            .add_peer(bob_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice_transport
            .set_topics(bob_id, &[overlay])
            .await
            .unwrap();
        bob_transport
            .add_peer(alice_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_transport
            .set_topics(alice_id, &[overlay])
            .await
            .unwrap();

        let alice_store = KnotSyncStore::in_memory(SPACE, writers);
        let bob_store = KnotSyncStore::in_memory(SPACE, writers);
        let alice_vault = Arc::new(KnotVault::open(roots.path().join("alice"), VAULT_KEY).unwrap());
        let bob_vault = Arc::new(KnotVault::open(roots.path().join("bob"), VAULT_KEY).unwrap());
        alice_store
            .author(
                alice.master_keypair().to_seed(),
                &alice_vault,
                &KnotSyncEvent::Put(doc("alice-note", "amber")),
            )
            .await
            .unwrap();
        bob_store
            .author(
                bob.master_keypair().to_seed(),
                &bob_vault,
                &KnotSyncEvent::Put(doc("bob-note", "blue")),
            )
            .await
            .unwrap();

        let (a_endpoint, a_gossip) = alice_transport.sync_parts().unwrap();
        let (b_endpoint, b_gossip) = bob_transport.sync_parts().unwrap();
        let alice_joined = alice_store.join(a_endpoint, a_gossip).await.unwrap();
        let bob_joined = bob_store.join(b_endpoint, b_gossip).await.unwrap();

        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if alice_store.documents(&alice_vault).await.unwrap().len() == 2
                    && bob_store.documents(&bob_vault).await.unwrap().len() == 2
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("Knot peers did not converge");

        assert_eq!(
            alice_store.documents(&alice_vault).await.unwrap(),
            bob_store.documents(&bob_vault).await.unwrap()
        );
        assert!(alice_joined.ops_received() >= 1);
        assert!(bob_joined.ops_received() >= 1);
    }

    #[tokio::test]
    async fn file_captures_verify_relations_without_replacing_same_id_vault_documents() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x94; 32]);
        let writer = alice.master_public_key().to_bytes();
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncStore::in_memory(SPACE, [writer]);
        let authored = doc("knot:document:catalog-file", "vault source");
        store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(authored.clone()),
            )
            .await
            .unwrap();
        let first = store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::CaptureFileRevision(file_revision(
                    "knot:document:catalog-file",
                    "old file quote",
                )),
            )
            .await
            .unwrap();
        let second = store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::CaptureFileRevision(file_revision(
                    "knot:document:catalog-file",
                    "new file quote",
                )),
            )
            .await
            .unwrap();

        let old = "old file quote";
        store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::AssertRelation {
                    predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                    subject: captured_endpoint(
                        "knot:document:catalog-file",
                        *first.hash.as_bytes(),
                        old,
                        "quote",
                    ),
                    object: captured_endpoint(
                        "knot:document:catalog-file",
                        *first.hash.as_bytes(),
                        old,
                        "old",
                    ),
                    evidence: None,
                    qualification: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(
            store
                .file_revision(&vault, "knot:document:catalog-file", *first.hash.as_bytes(),)
                .await
                .unwrap(),
            Some(file_revision("knot:document:catalog-file", old))
        );
        assert!(
            store
                .file_revision(
                    &vault,
                    "knot:document:catalog-file",
                    *second.hash.as_bytes(),
                )
                .await
                .unwrap()
                .is_some()
        );
        let projection = store.projection(&vault).await.unwrap();
        assert_eq!(projection.documents, vec![authored]);
        assert!(projection.conflicts.is_empty());
        assert_eq!(projection.relations.len(), 1);
    }

    #[tokio::test]
    async fn retained_file_revision_lookup_requires_same_writer_and_exact_metadata() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let alice_writer = alice.master_public_key().to_bytes();
        let bob_writer = bob.master_public_key().to_bytes();
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncStore::in_memory(SPACE, [alice_writer, bob_writer]);
        let revision = file_revision("knot:document:catalog-file", "quoted source");
        let alice_capture = store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::CaptureFileRevision(revision.clone()),
            )
            .await
            .unwrap();
        store
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::CaptureFileRevision(revision.clone()),
            )
            .await
            .unwrap();

        assert_eq!(
            store
                .find_file_revision_with_cipher(
                    KnotSyncCipher::Personal(&vault),
                    alice_writer,
                    &revision,
                )
                .await
                .unwrap(),
            Some(*alice_capture.hash.as_bytes())
        );

        for differing in [
            KnotFileRevisionV1 {
                title: "Different title".into(),
                ..revision.clone()
            },
            KnotFileRevisionV1 {
                media_type: "text/markdown".into(),
                ..revision.clone()
            },
            KnotFileRevisionV1 {
                body: b"different bytes".to_vec(),
                ..revision.clone()
            },
            KnotFileRevisionV1 {
                document_id: "knot:document:other-file".into(),
                ..revision.clone()
            },
        ] {
            assert_eq!(
                store
                    .find_file_revision_with_cipher(
                        KnotSyncCipher::Personal(&vault),
                        alice_writer,
                        &differing,
                    )
                    .await
                    .unwrap(),
                None
            );
        }
    }

    #[tokio::test]
    async fn retained_file_revision_lookup_refuses_pending_causal_history() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x96; 32]);
        let writer = alice.master_public_key().to_bytes();
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let store = KnotSyncStore::in_memory(SPACE, [writer]);
        let revision = file_revision("knot:document:catalog-file", "quoted source");
        store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::CaptureFileRevision(revision.clone()),
            )
            .await
            .unwrap();
        author_unchecked_with_parents(
            &store,
            alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("pending", "unavailable parent")),
            vec![[0xfe; 32]],
        )
        .await;

        assert!(matches!(
            store
                .find_file_revision_with_cipher(
                    KnotSyncCipher::Personal(&vault),
                    writer,
                    &revision,
                )
                .await,
            Err(KnotSyncError::Payload(message))
                if message == "capture retention requires complete causal history"
        ));
    }
}
