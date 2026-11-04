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
mod tests;
