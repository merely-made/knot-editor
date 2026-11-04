// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Which embedder produces Knot's search vectors, and the identity every
//! derived index carries because of it.
//!
//! Three providers: the portable lexical hashing embedder that needs no model
//! and is the default, and BERT on CPU or WebGPU. Weight custody is the
//! writer's: a BERT provider is reachable only from a directory the writer
//! supplies. Knot hashes that directory's artifacts, records the digest as the
//! index's model identity, and refuses — with its reason — when the path is
//! absent, an artifact is unreadable, or the digest has moved. It never falls
//! back to the lexical provider under a BERT-labelled index, and it neither
//! fetches nor bundles weights.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use esp::embed::{
    EmbedError, EmbeddingProvider, LexicalEmbeddingProvider, SimilarityMetric, SparseVector,
};
use serde::{Deserialize, Serialize};

use crate::search::KnotSearchError;

/// Schema version of [`KnotIndexIdentity`]. A sealed index recorded under a
/// different version is refused with a rebuild offered, the same as any other
/// identity difference.
pub const KNOT_INDEX_IDENTITY_VERSION: u32 = 1;

/// Model name recorded for the hashing provider. It takes no weights, so its
/// vector space is pinned by the esp provider plus the dimension count, both
/// of which the identity already carries.
pub const KNOT_LEXICAL_MODEL: &str = "esp-lexical-hashing";

/// Artifacts a writer-supplied model directory must carry, in digest order.
/// esp's loader reads exactly these three, and all three move the vector
/// space: a swapped tokenizer changes embeddings as surely as swapped weights.
pub const KNOT_BERT_ARTIFACTS: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

/// Domain separation for the weights digest, so it can never collide with any
/// other blake3 Knot takes over the same bytes.
const KNOT_WEIGHTS_DIGEST_CONTEXT: &str = "mere.knot.embedding-weights-digest.v1";

/// Which embedder produces Knot's search vectors.
///
/// [`Lexical`](Self::Lexical) is the default and the only one an ordinary
/// install can reach: it needs no weights, no network, and no adapter, so a
/// first run indexes its vault with nothing fetched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KnotEmbeddingProvider {
    /// esp's feature-hashing embedder. Portable, model-free, wasm-clean.
    #[default]
    Lexical,
    /// esp's BERT provider on the portable ndarray backend.
    BertCpu,
    /// esp's BERT provider on the WebGPU backend.
    BertWgpu,
}

impl KnotEmbeddingProvider {
    /// Whether this provider is reachable only from writer-supplied weights.
    pub fn needs_weights(self) -> bool {
        !matches!(self, Self::Lexical)
    }

    /// Stable slug, as it serializes and as refusals name it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::BertCpu => "bert-cpu",
            Self::BertWgpu => "bert-wgpu",
        }
    }

    /// Cargo feature this Knot build needs before the provider exists at all.
    /// `None` for the provider that is always compiled in.
    pub fn required_feature(self) -> Option<&'static str> {
        match self {
            Self::Lexical => None,
            Self::BertCpu => Some("embed-bert"),
            Self::BertWgpu => Some("embed-bert-wgpu"),
        }
    }
}

impl fmt::Display for KnotEmbeddingProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A writer-supplied model directory, and the digest it is held to.
///
/// The path is custody, not configuration: Knot reads it, hashes it, and
/// records what it found. It never writes there and never populates it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct KnotModelWeights {
    /// Directory holding [`KNOT_BERT_ARTIFACTS`], in HuggingFace layout.
    pub path: PathBuf,
    /// The digest these artifacts are pinned to.
    ///
    /// `None` means the writer has not pinned one yet, so the first build
    /// records whatever the directory hashes to and every later open is
    /// verified against that sealed record. `Some` pins it now: a directory
    /// that hashes to anything else is refused before a provider is built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

impl KnotModelWeights {
    /// Weights at a path, digest recorded on first build.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            digest: None,
        }
    }

    /// Weights pinned to a digest the writer already holds.
    pub fn pinned(path: impl Into<PathBuf>, digest: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            digest: Some(digest.into()),
        }
    }

    /// Label recorded beside the digest: the directory's own name, never the
    /// absolute path, so a sealed record never carries the writer's filesystem
    /// layout. It names the model in refusals; the digest is the identity, so
    /// renaming or moving the directory changes neither.
    pub fn model_name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// Hash a model directory's artifacts.
    ///
    /// blake3 in derive-key mode over each artifact in [`KNOT_BERT_ARTIFACTS`]
    /// order, every name and body length-prefixed so no rearrangement of bytes
    /// across files can produce the same digest. Streamed, so a multi-hundred
    /// megabyte safetensors is never held in memory.
    ///
    /// Public because a writer pinning a digest has to be able to learn it
    /// without running an index build.
    pub fn digest_of(directory: &Path) -> Result<String, KnotSearchError> {
        if !directory.is_dir() {
            return Err(KnotSearchError::WeightsUnreadable {
                path: directory.display().to_string(),
                reason: "no such model directory".into(),
            });
        }
        let mut hasher = blake3::Hasher::new_derive_key(KNOT_WEIGHTS_DIGEST_CONTEXT);
        for artifact in KNOT_BERT_ARTIFACTS {
            let path = directory.join(artifact);
            let unreadable = |reason: String| KnotSearchError::WeightsUnreadable {
                path: path.display().to_string(),
                reason,
            };
            let mut file =
                std::fs::File::open(&path).map_err(|error| unreadable(error.to_string()))?;
            let length = file
                .metadata()
                .map_err(|error| unreadable(error.to_string()))?
                .len();
            hasher.update(&(artifact.len() as u64).to_le_bytes());
            hasher.update(artifact.as_bytes());
            hasher.update(&length.to_le_bytes());
            std::io::copy(&mut file, &mut hasher).map_err(|error| unreadable(error.to_string()))?;
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Hash this directory and hold it to its pin, if it has one.
    fn verify(&self) -> Result<String, KnotSearchError> {
        let found = Self::digest_of(&self.path)?;
        if let Some(recorded) = self.digest.as_deref()
            && recorded != found
        {
            return Err(KnotSearchError::WeightsDigestMismatch {
                path: self.path.display().to_string(),
                recorded: recorded.to_owned(),
                found,
            });
        }
        Ok(found)
    }
}

/// The writer's embedder preference: which provider, and the custody it needs.
///
/// Serializable so a host can persist it beside its other surface
/// preferences. Knot itself does not choose where that file lives.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct KnotEmbeddingPreference {
    /// Which embedder. Defaults to [`KnotEmbeddingProvider::Lexical`].
    pub provider: KnotEmbeddingProvider,
    /// Writer-supplied weights. Required by every model-backed provider and
    /// ignored by the lexical one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weights: Option<KnotModelWeights>,
}

impl KnotEmbeddingPreference {
    /// The portable default.
    pub fn lexical() -> Self {
        Self::default()
    }

    /// BERT on the portable CPU backend, from weights the writer supplies.
    pub fn bert_cpu(weights: KnotModelWeights) -> Self {
        Self {
            provider: KnotEmbeddingProvider::BertCpu,
            weights: Some(weights),
        }
    }

    /// BERT on the WebGPU backend, from weights the writer supplies.
    pub fn bert_wgpu(weights: KnotModelWeights) -> Self {
        Self {
            provider: KnotEmbeddingProvider::BertWgpu,
            weights: Some(weights),
        }
    }
}

/// What produced an index's vectors, recorded with the index.
///
/// Two indices whose identities differ are not comparable: their vectors live
/// in different spaces, so a query embedded under one and scored against the
/// other returns confident nonsense. Knot therefore stores this beside every
/// sealed index and refuses the mismatch rather than guessing.
///
/// Deliberately not `deny_unknown_fields`: a record written by a later Knot
/// should reach the [`version`](Self::version) check and be refused there, by
/// name, rather than fail as a parse error.
///
/// `==` is plain record equality, label included. Whether two records name
/// the same model is [`is_same_model`](Self::is_same_model), and that is what
/// every reuse-or-refuse decision asks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KnotIndexIdentity {
    /// [`KNOT_INDEX_IDENTITY_VERSION`] at the time the index was sealed.
    pub version: u32,
    /// Which embedder produced the vectors.
    pub provider: KnotEmbeddingProvider,
    /// The writer's model directory name, or [`KNOT_LEXICAL_MODEL`].
    ///
    /// Beside a [`weights_digest`](Self::weights_digest) this is a display
    /// label only: refusals show it so a writer can tell the two sides apart,
    /// but [`is_same_model`](Self::is_same_model) never compares it.
    pub model: String,
    /// blake3 of the model artifacts, and the model's identity. `None` for
    /// providers that take no weights.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights_digest: Option<String>,
    /// Vector length.
    pub dimensions: usize,
    /// Metric the vectors are scored under.
    pub metric: SimilarityMetric,
}

impl KnotIndexIdentity {
    /// The identity a lexical index of this shape carries — and the identity
    /// assumed for a record sealed before Knot recorded one at all.
    pub fn lexical(dimensions: usize, metric: SimilarityMetric) -> Self {
        Self {
            version: KNOT_INDEX_IDENTITY_VERSION,
            provider: KnotEmbeddingProvider::Lexical,
            model: KNOT_LEXICAL_MODEL.to_owned(),
            weights_digest: None,
            dimensions,
            metric,
        }
    }

    /// Whether an index sealed under `self` can answer a query embedded under
    /// `requested`.
    pub fn answers(&self, requested: &Self) -> bool {
        self.is_same_model(requested)
    }

    /// Whether two records name the same vector space.
    ///
    /// A model is its weights digest: identical weights are the same model
    /// under any directory name, so beside a digest the label is ignored.
    /// Without a digest the name is all that distinguishes a model, so every
    /// field is compared, which keeps lexical identity exactly as strict as
    /// record equality.
    pub fn is_same_model(&self, other: &Self) -> bool {
        let label_identifies = self.weights_digest.is_none();
        self.version == other.version
            && self.provider == other.provider
            && self.weights_digest == other.weights_digest
            && self.dimensions == other.dimensions
            && self.metric == other.metric
            && (!label_identifies || self.model == other.model)
    }
}

impl fmt::Display for KnotIndexIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {:?} ({}d, {:?}, identity v{}",
            self.provider, self.model, self.dimensions, self.metric, self.version
        )?;
        if let Some(digest) = self.weights_digest.as_deref() {
            write!(formatter, ", weights {}", abbreviate(digest))?;
        }
        formatter.write_str(")")
    }
}

/// First 16 hex characters, which is plenty to tell two digests apart in a
/// refusal a person reads. The full pair appears in
/// [`KnotSearchError::WeightsDigestMismatch`].
fn abbreviate(digest: &str) -> String {
    match digest.char_indices().nth(16) {
        Some((cut, _)) => format!("{}…", &digest[..cut]),
        None => digest.to_owned(),
    }
}

/// One loaded embedder, shared by both search lanes.
///
/// esp's trait is implemented for `Box<dyn EmbeddingProvider>` but not for
/// `Arc`, and both are foreign here, so this newtype is what lets the disk
/// index and each vault query hold the *same* loaded model. Without it a BERT
/// query would load the weights again per call.
#[derive(Clone)]
pub(crate) struct SharedEmbeddings(Arc<dyn EmbeddingProvider>);

impl SharedEmbeddings {
    fn new(provider: impl EmbeddingProvider + 'static) -> Self {
        Self(Arc::new(provider))
    }

    fn from_boxed(provider: Box<dyn EmbeddingProvider>) -> Self {
        Self(Arc::from(provider))
    }
}

impl fmt::Debug for SharedEmbeddings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedEmbeddings")
            .field("dimensions", &self.0.dimensions())
            .field("metric", &self.0.metric())
            .finish_non_exhaustive()
    }
}

impl EmbeddingProvider for SharedEmbeddings {
    fn dimensions(&self) -> usize {
        self.0.dimensions()
    }

    fn metric(&self) -> SimilarityMetric {
        self.0.metric()
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        self.0.embed(texts)
    }

    fn embed_one(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        self.0.embed_one(text)
    }

    fn embed_sparse(&self, texts: &[&str]) -> Option<Result<Vec<SparseVector>, EmbedError>> {
        self.0.embed_sparse(texts)
    }
}

/// Build the embedder a preference names, and the identity an index built with
/// it carries.
///
/// Custody is checked before availability on purpose: a writer who names an
/// absent directory learns that, in a build with or without the BERT feature
/// compiled in. Nothing on this path can return the lexical provider for a
/// BERT preference — the only exits are the named provider or a refusal.
pub(crate) fn resolve_embeddings(
    preference: &KnotEmbeddingPreference,
    lexical_dimensions: usize,
) -> Result<(SharedEmbeddings, KnotIndexIdentity), KnotSearchError> {
    let provider = preference.provider;
    if !provider.needs_weights() {
        let lexical = LexicalEmbeddingProvider::new(lexical_dimensions).map_err(|error| {
            KnotSearchError::Configuration(format!("invalid Knot search configuration: {error}"))
        })?;
        let identity = KnotIndexIdentity::lexical(lexical.dimensions(), lexical.metric());
        return Ok((SharedEmbeddings::new(lexical), identity));
    }

    let weights = preference
        .weights
        .as_ref()
        .ok_or(KnotSearchError::WeightsNotSupplied { provider })?;
    let digest = weights.verify()?;
    let loaded = load_model(provider, weights)?;
    let identity = KnotIndexIdentity {
        version: KNOT_INDEX_IDENTITY_VERSION,
        provider,
        model: weights.model_name(),
        weights_digest: Some(digest),
        dimensions: loaded.dimensions(),
        metric: loaded.metric(),
    };
    Ok((SharedEmbeddings::from_boxed(loaded), identity))
}

/// Load a verified model directory on the provider's backend. A provider this
/// build never compiled refuses by name rather than degrading to a different
/// vector space.
#[cfg_attr(not(feature = "embed-bert"), allow(unused_variables))]
fn load_model(
    provider: KnotEmbeddingProvider,
    weights: &KnotModelWeights,
) -> Result<Box<dyn EmbeddingProvider>, KnotSearchError> {
    match provider {
        #[cfg(feature = "embed-bert")]
        KnotEmbeddingProvider::BertCpu => esp::embed::bert::load_cpu(&weights.path)
            .map_err(|error| model_load(provider, weights, error)),
        #[cfg(feature = "embed-bert-wgpu")]
        // Knot's standalone search embedder has no renderer device to share.
        // Boot its explicitly selected adapter; renderer hosts should pass
        // their already registered device through their own provider seam.
        KnotEmbeddingProvider::BertWgpu => esp::embed::bert::load_wgpu(
            &weights.path,
            esp::embed::bert::Device::wgpu(esp::embed::bert::DeviceKind::DiscreteGpu(0)),
        )
        .map_err(|error| model_load(provider, weights, error)),
        other => Err(match other.required_feature() {
            Some(feature) => KnotSearchError::ProviderUnavailable {
                provider: other,
                feature,
            },
            None => KnotSearchError::Configuration(format!("{other} loads no model")),
        }),
    }
}

#[cfg(feature = "embed-bert")]
fn model_load(
    provider: KnotEmbeddingProvider,
    weights: &KnotModelWeights,
    error: EmbedError,
) -> KnotSearchError {
    KnotSearchError::ModelLoad {
        provider,
        path: weights.path.display().to_string(),
        reason: error.to_string(),
    }
}

#[cfg(test)]
pub(crate) mod tests;
