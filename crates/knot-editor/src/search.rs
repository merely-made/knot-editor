// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Capability-scoped search across files-in-place and sealed vault documents.
//!
//! Both lanes embed through the one provider the writer's
//! [`KnotEmbeddingPreference`] names. The vault lane's dense index is sealed
//! beside the [`KnotIndexIdentity`] that produced it, and an index sealed under
//! any other identity is refused with a rebuild offered: never reused, because
//! its vectors live in another space, and never discarded without the explicit
//! [`KnotSearch::rebuild_vault_index`] command.

use std::fs;

use esp::embed::{SemanticSearch, VectorIndex};
use serde::{Deserialize, Serialize};
use servitor::{AuthorityProvider, Cap, Mode, Subject};

use crate::embedding::{
    KnotEmbeddingPreference, KnotEmbeddingProvider, KnotIndexIdentity, SharedEmbeddings,
    resolve_embeddings,
};
use crate::vault::KnotSealedSearchIndex;
use crate::{DirectorySource, KnotVault};

const DISK_SCOPE: &str = "knot/search/disk";
const VAULT_SCOPE: &str = "knot/search/vault";

/// Which source lane produced a search result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchLane {
    /// A file whose bytes remain authoritative on disk.
    Disk,
    /// A document held by the sealed vault.
    Vault,
}

/// One capability-filtered search result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub lane: SearchLane,
    pub score: f32,
}

/// How far one lane's build has got.
///
/// `visited` of `total` is the status bar's `index · building 148/214`.
/// `indexed` is how many of the visited documents were embedded; the rest were
/// skipped as binary, undecodable, or over the size cap. A lane reports once
/// at zero before any work and once after every document, so its last report
/// always has `visited == total`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchProgress {
    pub lane: SearchLane,
    pub visited: usize,
    pub indexed: usize,
    pub total: usize,
}

/// Host-selected bounds and embedder for the local index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchConfig {
    /// Feature-hash buckets for the lexical provider. A model-backed provider
    /// takes its dimension from the model and ignores this.
    pub dimensions: usize,
    /// Largest disk document Knot will read for indexing.
    pub max_file_bytes: u64,
    /// Which embedder, and the weights custody it needs. Lexical by default.
    pub embedding: KnotEmbeddingPreference,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            dimensions: 512,
            max_file_bytes: 2 * 1024 * 1024,
            embedding: KnotEmbeddingPreference::default(),
        }
    }
}

/// A refusal or failure from building or querying Knot search.
///
/// The custody and identity variants are refusals with their reason attached.
/// None of them is ever answered by substituting the lexical provider.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KnotSearchError {
    /// The sealed vault index was produced under a different identity.
    ///
    /// The index is left exactly as it was. [`Self::offers_rebuild`] is true:
    /// [`KnotSearch::rebuild_vault_index`] replaces it under `requested`. When
    /// the refusal came from [`KnotSearch::build`], build without the vault
    /// first and rebuild from that instance.
    #[error(
        "the sealed Knot vault index was built by {sealed}, but this search embeds with \
         {requested}; the index was neither used nor discarded — rebuild the vault index to \
         replace it"
    )]
    IdentityMismatch {
        sealed: Box<KnotIndexIdentity>,
        requested: Box<KnotIndexIdentity>,
    },
    /// A model-backed provider was chosen with no model directory.
    #[error("{provider} embeddings need a writer-supplied model directory, and none is configured")]
    WeightsNotSupplied { provider: KnotEmbeddingProvider },
    /// The model directory, or one of its artifacts, could not be read.
    #[error("model weights are not readable at {path}: {reason}")]
    WeightsUnreadable { path: String, reason: String },
    /// The model directory hashes to something other than its pinned digest.
    #[error("model weights at {path} hash to {found}, not the pinned {recorded}")]
    WeightsDigestMismatch {
        path: String,
        recorded: String,
        found: String,
    },
    /// Custody passed, but this build did not compile the provider.
    #[error("{provider} embeddings are not compiled into this Knot build (feature `{feature}`)")]
    ProviderUnavailable {
        provider: KnotEmbeddingProvider,
        feature: &'static str,
    },
    /// Custody passed and the provider exists, but esp could not load the model.
    #[error("could not load the {provider} model at {path}: {reason}")]
    ModelLoad {
        provider: KnotEmbeddingProvider,
        path: String,
        reason: String,
    },
    #[error("{0}")]
    Configuration(String),
    #[error("{0}")]
    Vault(String),
    #[error("{0}")]
    Index(String),
}

impl KnotSearchError {
    /// Whether an explicit rebuild of the vault index clears this refusal.
    pub fn offers_rebuild(&self) -> bool {
        matches!(self, Self::IdentityMismatch { .. })
    }

    /// The identity a rebuild would seal, when one is offered.
    pub fn rebuild_identity(&self) -> Option<&KnotIndexIdentity> {
        match self {
            Self::IdentityMismatch { requested, .. } => Some(requested),
            _ => None,
        }
    }
}

/// Search state. The disk index is live in memory; the vault index is sealed
/// and opened only for an authorized query while the vault is unlocked.
pub struct KnotSearch {
    identity: KnotIndexIdentity,
    provider: SharedEmbeddings,
    disk: SemanticSearch<String, SharedEmbeddings>,
}

impl KnotSearch {
    /// Build both lanes. Disk decoding skips binary and over-limit files.
    /// Vault embeddings are immediately sealed through Personae.
    ///
    /// Refuses, without touching it, a sealed vault index whose identity
    /// differs from the one this configuration produces.
    pub fn build(
        directory: Option<&DirectorySource>,
        vault: Option<&KnotVault>,
        config: SearchConfig,
    ) -> Result<Self, KnotSearchError> {
        Self::build_reporting(directory, vault, config, |_| {})
    }

    /// [`Self::build`], reporting each lane's progress as it goes.
    ///
    /// Synchronous: the host decides which thread this runs on and drives its
    /// status surface from `progress`.
    pub fn build_reporting(
        directory: Option<&DirectorySource>,
        vault: Option<&KnotVault>,
        config: SearchConfig,
        mut progress: impl FnMut(SearchProgress),
    ) -> Result<Self, KnotSearchError> {
        if vault.is_some_and(KnotVault::is_locked) {
            return Err(KnotSearchError::Vault(
                "cannot build the Knot vault index while locked".into(),
            ));
        }
        let (provider, identity) = resolve_embeddings(&config.embedding, config.dimensions)?;

        // Refuse before any embedding work, so a mismatch costs nothing.
        if let Some(vault) = vault
            && let Some(sealed) = vault.load_search_index().map_err(KnotSearchError::Vault)?
        {
            admit(sealed, &identity)?;
        }

        let mut disk = SemanticSearch::new(provider.clone());
        if let Some(directory) = directory {
            let total = directory.documents().count();
            let mut report = LaneProgress::start(SearchLane::Disk, total, &mut progress);
            for document in directory.documents() {
                let body = (document.byte_size <= config.max_file_bytes)
                    .then(|| fs::read_to_string(&document.path).ok())
                    .flatten();
                let Some(body) = body else {
                    report.visited(false, &mut progress);
                    continue;
                };
                let text = format!("{}\n{body}", document.container.title);
                disk.ingest(document.id.clone(), &text).map_err(|error| {
                    KnotSearchError::Index(format!(
                        "could not index {}: {error}",
                        document.path.display()
                    ))
                })?;
                report.visited(true, &mut progress);
            }
        }

        let search = Self {
            identity,
            provider,
            disk,
        };
        if let Some(vault) = vault {
            search.seal_vault_index(vault, &mut progress)?;
        }
        Ok(search)
    }

    /// Discard the sealed vault index and rebuild it under this search's
    /// identity.
    ///
    /// The explicit command an [`KnotSearchError::IdentityMismatch`] offers,
    /// and the only path by which Knot replaces an index sealed under another
    /// identity.
    pub fn rebuild_vault_index(
        &self,
        vault: &KnotVault,
        mut progress: impl FnMut(SearchProgress),
    ) -> Result<(), KnotSearchError> {
        if vault.is_locked() {
            return Err(KnotSearchError::Vault(
                "cannot rebuild the Knot vault index while locked".into(),
            ));
        }
        self.seal_vault_index(vault, &mut progress)
    }

    /// The identity this search embeds under and seals its vault index with.
    pub fn identity(&self) -> &KnotIndexIdentity {
        &self.identity
    }

    /// Search only lanes covered by the caller's read grants.
    ///
    /// A locked vault contributes nothing even if the subject holds its grant.
    /// The vault index's identity is checked only after the grant is: a
    /// subject without it learns nothing about the index, not even that it
    /// would have been refused.
    pub fn query(
        &self,
        vault: Option<&KnotVault>,
        query: &str,
        k: usize,
        subject: Subject,
        authority: &impl AuthorityProvider,
    ) -> Result<Vec<SearchHit>, KnotSearchError> {
        if k == 0 {
            return Err(KnotSearchError::Configuration(
                "Knot search result count must be positive".into(),
            ));
        }
        let mut hits = Vec::new();
        if covers(authority, subject, DISK_SCOPE)? {
            hits.extend(
                self.disk
                    .search(query, k)
                    .map_err(|error| {
                        KnotSearchError::Index(format!("could not search Knot disk index: {error}"))
                    })?
                    .into_iter()
                    .map(|(id, score)| SearchHit {
                        id,
                        lane: SearchLane::Disk,
                        score,
                    }),
            );
        }

        if covers(authority, subject, VAULT_SCOPE)?
            && let Some(vault) = vault.filter(|vault| !vault.is_locked())
            && let Some(sealed) = vault.load_search_index().map_err(KnotSearchError::Vault)?
        {
            let index = admit(sealed, &self.identity)?;
            let search =
                SemanticSearch::with_index(self.provider.clone(), index).map_err(|error| {
                    KnotSearchError::Index(format!("could not open Knot vault index: {error}"))
                })?;
            hits.extend(
                search
                    .search(query, k)
                    .map_err(|error| {
                        KnotSearchError::Index(format!(
                            "could not search Knot vault index: {error}"
                        ))
                    })?
                    .into_iter()
                    .map(|(id, score)| SearchHit {
                        id,
                        lane: SearchLane::Vault,
                        score,
                    }),
            );
        }

        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(k);
        Ok(hits)
    }

    /// Capability a subject needs to read disk search results.
    pub fn disk_cap() -> Cap {
        Cap::scope(DISK_SCOPE).expect("static Knot disk scope is valid")
    }

    /// Capability a subject needs to read vault search results.
    pub fn vault_cap() -> Cap {
        Cap::scope(VAULT_SCOPE).expect("static Knot vault scope is valid")
    }

    fn seal_vault_index(
        &self,
        vault: &KnotVault,
        progress: &mut impl FnMut(SearchProgress),
    ) -> Result<(), KnotSearchError> {
        // The sealed vault record stores a dense VectorIndex. Select that
        // representation explicitly even when lexical search offers sparse storage.
        let index = VectorIndex::new(self.identity.dimensions, self.identity.metric);
        let mut sealed =
            SemanticSearch::with_index(self.provider.clone(), index).map_err(|error| {
                KnotSearchError::Index(format!("could not build Knot vault index: {error}"))
            })?;
        let total = vault.documents().count();
        let mut report = LaneProgress::start(SearchLane::Vault, total, progress);
        for document in vault.documents() {
            let Ok(body) = std::str::from_utf8(&document.body) else {
                report.visited(false, progress);
                continue;
            };
            let text = format!("{}\n{body}", document.title);
            sealed.ingest(document.id.clone(), &text).map_err(|error| {
                KnotSearchError::Index(format!(
                    "could not index vault document {}: {error}",
                    document.id
                ))
            })?;
            report.visited(true, progress);
        }
        vault
            .store_search_index(&self.identity, sealed.index())
            .map_err(KnotSearchError::Vault)
    }
}

/// Hold a sealed index to the identity a search embeds under.
///
/// A record sealed before Knot recorded identity could only have come from the
/// lexical provider, so it answers as lexical at its own dimensions and metric:
/// an existing vault keeps searching under the default, and is refused like any
/// other the moment the writer chooses a different provider.
fn admit(
    sealed: KnotSealedSearchIndex,
    requested: &KnotIndexIdentity,
) -> Result<VectorIndex<String>, KnotSearchError> {
    let recorded = sealed.identity.unwrap_or_else(|| {
        KnotIndexIdentity::lexical(sealed.index.dimensions(), sealed.index.metric())
    });
    if recorded.answers(requested) {
        Ok(sealed.index)
    } else {
        Err(KnotSearchError::IdentityMismatch {
            sealed: Box::new(recorded),
            requested: Box::new(requested.clone()),
        })
    }
}

/// Counter behind [`SearchProgress`], so both lanes report the same way.
struct LaneProgress {
    lane: SearchLane,
    visited: usize,
    indexed: usize,
    total: usize,
}

impl LaneProgress {
    fn start(lane: SearchLane, total: usize, progress: &mut impl FnMut(SearchProgress)) -> Self {
        let report = Self {
            lane,
            visited: 0,
            indexed: 0,
            total,
        };
        report.emit(progress);
        report
    }

    fn visited(&mut self, indexed: bool, progress: &mut impl FnMut(SearchProgress)) {
        self.visited += 1;
        self.indexed += usize::from(indexed);
        self.emit(progress);
    }

    fn emit(&self, progress: &mut impl FnMut(SearchProgress)) {
        progress(SearchProgress {
            lane: self.lane,
            visited: self.visited,
            indexed: self.indexed,
            total: self.total,
        });
    }
}

fn covers(
    authority: &impl AuthorityProvider,
    subject: Subject,
    scope: &str,
) -> Result<bool, KnotSearchError> {
    let cap = Cap::scope(scope).map_err(|error| {
        KnotSearchError::Configuration(format!("invalid Knot search scope: {error}"))
    })?;
    Ok(authority.covers(subject, &cap, Mode::Read))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use esp::embed::SimilarityMetric;
    use servitor::{Grant, GrantTable};
    use tempfile::tempdir;

    use super::*;
    use crate::VaultDocument;
    use crate::embedding::tests::canned_model_directory;
    use crate::embedding::{KNOT_INDEX_IDENTITY_VERSION, KnotModelWeights};
    use crate::vault::SEARCH_INDEX_PATH;

    fn subject() -> Subject {
        Subject::new([0x61; 32])
    }

    fn grant(cap: Cap) -> GrantTable {
        GrantTable::new().with_grant(Grant::new(subject(), cap, Mode::Read))
    }

    fn both() -> GrantTable {
        GrantTable::new()
            .with_grant(Grant::new(subject(), KnotSearch::disk_cap(), Mode::Read))
            .with_grant(Grant::new(subject(), KnotSearch::vault_cap(), Mode::Read))
    }

    fn note(id: &str, title: &str, body: &str) -> VaultDocument {
        VaultDocument {
            id: id.into(),
            title: title.into(),
            body: body.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }
    }

    fn orchard_vault(root: &std::path::Path, key: u8) -> KnotVault {
        let mut vault = KnotVault::open(root, [key; 32]).unwrap();
        vault
            .put(note(
                "orchard",
                "Private orchard",
                "orchard pruning observations",
            ))
            .unwrap();
        vault
    }

    /// An identity as a BERT build would seal it, with no model in hand. The
    /// custody path that would produce it is covered in `embedding`; here it
    /// only has to be a different vector space from the lexical default.
    fn canned_bert_identity(digest: &str) -> KnotIndexIdentity {
        KnotIndexIdentity {
            version: KNOT_INDEX_IDENTITY_VERSION,
            provider: KnotEmbeddingProvider::BertCpu,
            model: "all-MiniLM-L6-v2".into(),
            weights_digest: Some(digest.into()),
            dimensions: 384,
            metric: SimilarityMetric::Cosine,
        }
    }

    fn lexical_config(dimensions: usize) -> SearchConfig {
        SearchConfig {
            dimensions,
            ..SearchConfig::default()
        }
    }

    #[test]
    fn search_spans_disk_and_vault_but_respects_lane_grants() {
        let temp = tempdir().unwrap();
        let disk_root = temp.path().join("files");
        let vault_root = temp.path().join("vault");
        fs::create_dir(&disk_root).unwrap();
        fs::write(disk_root.join("runtime.md"), "rust async runtime internals").unwrap();
        let directory = DirectorySource::open(&disk_root).unwrap();
        let vault = orchard_vault(&vault_root, 0x62);

        let search =
            KnotSearch::build(Some(&directory), Some(&vault), SearchConfig::default()).unwrap();
        let orchard = search
            .query(Some(&vault), "orchard observations", 2, subject(), &both())
            .unwrap();
        assert_eq!(orchard[0].lane, SearchLane::Vault);

        let disk_only = search
            .query(
                Some(&vault),
                "orchard observations",
                2,
                subject(),
                &grant(KnotSearch::disk_cap()),
            )
            .unwrap();
        assert!(disk_only.iter().all(|hit| hit.lane == SearchLane::Disk));

        let vault_only = search
            .query(
                Some(&vault),
                "rust async",
                2,
                subject(),
                &grant(KnotSearch::vault_cap()),
            )
            .unwrap();
        assert!(vault_only.iter().all(|hit| hit.lane == SearchLane::Vault));

        let nothing = search
            .query(
                Some(&vault),
                "orchard rust",
                2,
                subject(),
                &GrantTable::new(),
            )
            .unwrap();
        assert!(nothing.is_empty(), "no scope grant, no hits: {nothing:?}");
    }

    #[test]
    fn locked_vault_has_no_hits_and_its_derived_index_is_sealed() {
        let temp = tempdir().unwrap();
        let vault_root = temp.path().join("vault");
        let mut vault = KnotVault::open(&vault_root, [0x63; 32]).unwrap();
        vault
            .put(note(
                "private-orchard",
                "Private orchard",
                "confidential quince harvest",
            ))
            .unwrap();
        let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();

        let sealed = fs::read(vault_root.join(SEARCH_INDEX_PATH)).unwrap();
        for plaintext in [
            b"private-orchard".as_slice(),
            b"confidential".as_slice(),
            b"esp-lexical-hashing".as_slice(),
        ] {
            assert!(
                !sealed
                    .windows(plaintext.len())
                    .any(|window| window == plaintext)
            );
        }

        let authority = grant(KnotSearch::vault_cap());
        assert_eq!(
            search
                .query(Some(&vault), "quince harvest", 1, subject(), &authority,)
                .unwrap()[0]
                .id,
            "private-orchard"
        );
        vault.lock();
        assert!(
            search
                .query(Some(&vault), "quince harvest", 1, subject(), &authority,)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn lexical_is_the_default_and_seals_its_identity() {
        let temp = tempdir().unwrap();
        let vault = orchard_vault(temp.path(), 0x64);
        assert_eq!(
            SearchConfig::default().embedding.provider,
            KnotEmbeddingProvider::Lexical
        );
        let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();
        assert_eq!(
            search.identity(),
            &KnotIndexIdentity::lexical(512, SimilarityMetric::Cosine)
        );
        let sealed = vault.load_search_index().unwrap().unwrap();
        assert_eq!(sealed.identity.as_ref(), Some(search.identity()));
    }

    #[test]
    fn a_vault_index_sealed_before_identity_still_loads_and_queries_as_lexical() {
        let temp = tempdir().unwrap();
        let vault = orchard_vault(temp.path(), 0x65);

        // Seal what the previous Knot sealed: the bare dense index, no identity.
        let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();
        let bare = vault.load_search_index().unwrap().unwrap().index;
        vault.store_search_index_unidentified(&bare).unwrap();
        let legacy = vault.load_search_index().unwrap().unwrap();
        assert!(legacy.identity.is_none());
        assert_eq!(legacy.index.dimensions(), 512);

        let hits = search
            .query(Some(&vault), "orchard pruning", 1, subject(), &both())
            .unwrap();
        assert_eq!(hits[0].id, "orchard");
        assert_eq!(hits[0].lane, SearchLane::Vault);

        // A fresh build over it refreshes in place rather than refusing, and
        // leaves the record identified.
        KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();
        assert!(
            vault
                .load_search_index()
                .unwrap()
                .unwrap()
                .identity
                .is_some()
        );

        // An unidentified record at other dimensions is still held to them.
        vault.store_search_index_unidentified(&bare).unwrap();
        let narrower = KnotSearch::build(None, None, lexical_config(256)).unwrap();
        let error = narrower
            .query(Some(&vault), "orchard", 1, subject(), &both())
            .unwrap_err();
        assert!(error.offers_rebuild(), "{error}");
    }

    #[test]
    fn an_index_sealed_under_another_identity_is_refused_naming_both_and_offers_rebuild() {
        let temp = tempdir().unwrap();
        let vault = orchard_vault(temp.path(), 0x66);
        let lexical = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();

        // A BERT index sealed by some other session over the same vault.
        let bert = canned_bert_identity(&"c".repeat(64));
        let bert_vectors: VectorIndex<String> = VectorIndex::new(384, SimilarityMetric::Cosine);
        vault.store_search_index(&bert, &bert_vectors).unwrap();

        let error = lexical
            .query(Some(&vault), "orchard", 1, subject(), &both())
            .unwrap_err();
        let KnotSearchError::IdentityMismatch { sealed, requested } = &error else {
            panic!("expected an identity refusal, got {error}");
        };
        assert_eq!(**sealed, bert);
        assert_eq!(requested.as_ref(), lexical.identity());
        let message = error.to_string();
        assert!(
            message.contains("bert-cpu") && message.contains("all-MiniLM-L6-v2"),
            "{message}"
        );
        assert!(
            message.contains("lexical") && message.contains("esp-lexical-hashing"),
            "{message}"
        );
        assert!(message.contains("rebuild"), "{message}");
        assert!(error.offers_rebuild());
        assert_eq!(error.rebuild_identity(), Some(lexical.identity()));

        // Refused means untouched: the BERT record is still there.
        let still = vault.load_search_index().unwrap().unwrap();
        assert_eq!(still.identity, Some(bert.clone()));

        // A build over it refuses the same way, before sealing anything.
        let error = KnotSearch::build(None, Some(&vault), SearchConfig::default())
            .err()
            .expect("a build must not silently replace another identity's index");
        assert!(error.offers_rebuild(), "{error}");
        assert_eq!(
            vault.load_search_index().unwrap().unwrap().identity,
            Some(bert)
        );

        // The offered rebuild is the explicit command, and it reports progress.
        let mut reports = Vec::new();
        lexical
            .rebuild_vault_index(&vault, |report| reports.push(report))
            .unwrap();
        assert_eq!(
            reports,
            vec![
                SearchProgress {
                    lane: SearchLane::Vault,
                    visited: 0,
                    indexed: 0,
                    total: 1,
                },
                SearchProgress {
                    lane: SearchLane::Vault,
                    visited: 1,
                    indexed: 1,
                    total: 1,
                },
            ]
        );
        assert_eq!(
            lexical
                .query(Some(&vault), "orchard", 1, subject(), &both())
                .unwrap()[0]
                .id,
            "orchard"
        );
    }

    #[test]
    fn a_moved_weights_digest_is_refused_against_the_sealed_record() {
        let temp = tempdir().unwrap();
        let vault = orchard_vault(temp.path(), 0x67);
        let sealed = canned_bert_identity(&"d".repeat(64));
        let vectors: VectorIndex<String> = VectorIndex::new(384, SimilarityMetric::Cosine);
        vault.store_search_index(&sealed, &vectors).unwrap();

        // Same provider, model name and shape; only the weights moved.
        let mut requested = sealed.clone();
        requested.weights_digest = Some("e".repeat(64));
        let error = admit(vault.load_search_index().unwrap().unwrap(), &requested).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("dddddddddddddddd") && message.contains("eeeeeeeeeeeeeeee"),
            "{message}"
        );
        assert!(error.offers_rebuild());
    }

    #[test]
    fn a_bert_preference_with_an_absent_path_refuses_and_never_builds_lexical() {
        let temp = tempdir().unwrap();
        let vault = orchard_vault(&temp.path().join("vault"), 0x68);
        let absent = temp.path().join("models").join("all-MiniLM-L6-v2");
        let config = SearchConfig {
            embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::at(&absent)),
            ..SearchConfig::default()
        };

        let error = KnotSearch::build(None, Some(&vault), config)
            .err()
            .expect("an absent model directory must refuse");
        assert!(
            matches!(error, KnotSearchError::WeightsUnreadable { .. }),
            "{error}"
        );
        assert!(!error.offers_rebuild());
        assert!(error.to_string().contains("all-MiniLM-L6-v2"), "{error}");
        assert!(
            vault.load_search_index().unwrap().is_none(),
            "nothing may be sealed, least of all a lexical index under a BERT preference"
        );

        let unsupplied = SearchConfig {
            embedding: KnotEmbeddingPreference {
                provider: KnotEmbeddingProvider::BertWgpu,
                weights: None,
            },
            ..SearchConfig::default()
        };
        let error = KnotSearch::build(None, Some(&vault), unsupplied)
            .err()
            .expect("a BERT preference with no path must refuse");
        assert_eq!(
            error,
            KnotSearchError::WeightsNotSupplied {
                provider: KnotEmbeddingProvider::BertWgpu
            }
        );
        assert!(vault.load_search_index().unwrap().is_none());
    }

    #[test]
    fn a_pinned_digest_mismatch_refuses_before_any_index_exists() {
        let temp = tempdir().unwrap();
        let vault = orchard_vault(&temp.path().join("vault"), 0x69);
        let directory = canned_model_directory(&temp.path().join("models"), "writer");
        let config = SearchConfig {
            embedding: KnotEmbeddingPreference::bert_cpu(KnotModelWeights::pinned(
                &directory,
                "f".repeat(64),
            )),
            ..SearchConfig::default()
        };
        let error = KnotSearch::build(None, Some(&vault), config)
            .err()
            .expect("a moved digest must refuse");
        assert!(
            matches!(error, KnotSearchError::WeightsDigestMismatch { .. }),
            "{error}"
        );
        assert!(vault.load_search_index().unwrap().is_none());
    }

    #[test]
    fn a_subject_without_the_vault_grant_sees_neither_hits_nor_a_refusal() {
        let temp = tempdir().unwrap();
        let disk_root = temp.path().join("files");
        fs::create_dir(&disk_root).unwrap();
        fs::write(disk_root.join("orchard.md"), "orchard notes on disk").unwrap();
        let directory = DirectorySource::open(&disk_root).unwrap();
        let vault = orchard_vault(&temp.path().join("vault"), 0x6a);
        let search = KnotSearch::build(Some(&directory), None, SearchConfig::default()).unwrap();
        let vectors: VectorIndex<String> = VectorIndex::new(384, SimilarityMetric::Cosine);
        vault
            .store_search_index(&canned_bert_identity(&"a".repeat(64)), &vectors)
            .unwrap();

        // Access filtering comes before the identity check: without the vault
        // grant, the mismatched index is never opened and never refused.
        let disk_only = search
            .query(
                Some(&vault),
                "orchard",
                2,
                subject(),
                &grant(KnotSearch::disk_cap()),
            )
            .unwrap();
        assert!(!disk_only.is_empty());
        assert!(disk_only.iter().all(|hit| hit.lane == SearchLane::Disk));

        let stranger = Subject::new([0x7f; 32]);
        assert!(
            search
                .query(Some(&vault), "orchard", 2, stranger, &both())
                .unwrap()
                .is_empty(),
            "grants are per subject"
        );
    }

    #[test]
    fn a_built_search_and_its_reports_cross_a_thread_boundary() {
        // The build lane is synchronous; a host that runs it off its own
        // thread has to be able to send the result and the reports back.
        fn sendable<T: Send + 'static>() {}
        sendable::<KnotSearch>();
        sendable::<SearchProgress>();
        sendable::<KnotSearchError>();

        let temp = tempdir().unwrap();
        let disk_root = temp.path().join("files");
        fs::create_dir(&disk_root).unwrap();
        fs::write(disk_root.join("field.md"), "field notes").unwrap();
        let (reports, received) = std::sync::mpsc::channel();
        let built = std::thread::spawn(move || {
            let directory = DirectorySource::open(&disk_root).unwrap();
            KnotSearch::build_reporting(Some(&directory), None, SearchConfig::default(), |report| {
                reports.send(report).unwrap()
            })
        })
        .join()
        .unwrap()
        .unwrap_or_else(|error| panic!("{error}"));
        let last = received.try_iter().last().unwrap();
        assert_eq!((last.visited, last.total), (1, 1));
        assert_eq!(
            built
                .query(None, "field", 1, subject(), &grant(KnotSearch::disk_cap()))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn building_reports_progress_per_lane_including_skipped_documents() {
        let temp = tempdir().unwrap();
        let disk_root = temp.path().join("files");
        fs::create_dir(&disk_root).unwrap();
        fs::write(disk_root.join("small.md"), "small note").unwrap();
        fs::write(disk_root.join("large.md"), "x".repeat(64)).unwrap();
        let directory = DirectorySource::open(&disk_root).unwrap();
        let mut vault = orchard_vault(&temp.path().join("vault"), 0x6b);
        vault
            .put(VaultDocument {
                id: "binary".into(),
                title: "Binary".into(),
                body: vec![0xff, 0xfe, 0xfd],
                media_type: "application/octet-stream".into(),
            })
            .unwrap();

        let mut reports = Vec::new();
        let config = SearchConfig {
            max_file_bytes: 32,
            ..SearchConfig::default()
        };
        KnotSearch::build_reporting(Some(&directory), Some(&vault), config, |report| {
            reports.push(report)
        })
        .unwrap();

        let last = |lane| {
            *reports
                .iter()
                .rev()
                .find(|report| report.lane == lane)
                .unwrap()
        };
        assert_eq!(
            last(SearchLane::Disk),
            SearchProgress {
                lane: SearchLane::Disk,
                visited: 2,
                indexed: 1,
                total: 2,
            }
        );
        assert_eq!(
            last(SearchLane::Vault),
            SearchProgress {
                lane: SearchLane::Vault,
                visited: 2,
                indexed: 1,
                total: 2,
            }
        );
        assert_eq!(
            reports.len(),
            3 + 3,
            "one zero report plus one per document, per lane"
        );
        assert!(
            reports
                .windows(2)
                .filter(|pair| pair[0].lane == pair[1].lane)
                .all(|pair| pair[1].visited == pair[0].visited + 1)
        );
    }
}
