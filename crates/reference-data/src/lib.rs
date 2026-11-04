// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Backend-neutral lexical contracts and an opt-in local pack registry.
//!
//! Packs contain only JSON data. Import verifies structure, bounds and a BLAKE3
//! entries-payload digest; a digest is integrity, not an authenticity signature.
//! Hosts own source discovery, trust decisions, download transport and UI.

mod registry;
mod validation;

pub use registry::SourceRegistry;
use serde::{Deserialize, Serialize};
use std::fmt;
pub use validation::{Limits, ValidatedPack, digest_entries};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceKey {
    pub source: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceId {
    pub source: String,
    pub version: String,
    pub entry: String,
}

impl ReferenceId {
    pub fn source_key(&self) -> SourceKey {
        SourceKey {
            source: self.source.clone(),
            version: self.version.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceFeature {
    Definitions,
    Examples,
    Relations,
    Pronunciations,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseInfo {
    pub name: String,
    pub text: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceManifest {
    pub id: String,
    pub version: String,
    pub label: String,
    /// BCP-47-shaped language tag, or `mul` for a multilingual pack.
    pub language: String,
    pub features: Vec<ReferenceFeature>,
    pub license: LicenseInfo,
    pub attribution: String,
    pub upstream: Option<String>,
    /// Lowercase BLAKE3 hex of `serde_json::to_vec(entries)`; None is catalog-only.
    pub digest: Option<String>,
}

impl SourceManifest {
    pub fn source_key(&self) -> SourceKey {
        SourceKey {
            source: self.id.clone(),
            version: self.version.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartOfSpeech {
    Noun,
    Verb,
    Adjective,
    Adverb,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Synonym,
    Antonym,
    Hypernym,
    Hyponym,
    Meronym,
    Holonym,
    Derivation,
    Related,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalRelation {
    pub kind: RelationKind,
    pub target: ReferenceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_sense: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalSense {
    pub id: String,
    /// Source-local concept or synset identity, not a filesystem path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concept_id: Option<String>,
    pub definition: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub relations: Vec<LexicalRelation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pronunciation {
    pub notation: String,
    pub value: String,
    pub variant: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalEntry {
    pub id: ReferenceId,
    pub lemma: String,
    pub language: String,
    pub part_of_speech: Option<PartOfSpeech>,
    #[serde(default)]
    pub senses: Vec<LexicalSense>,
    #[serde(default)]
    pub pronunciations: Vec<Pronunciation>,
    #[serde(default)]
    pub relations: Vec<LexicalRelation>,
}

/// Normalized, transport-independent JSON format. No scripts, URLs to fetch, or
/// engine/provider manifests are accepted as pack contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedPack {
    pub schema_version: u32,
    pub manifest: SourceManifest,
    pub entries: Vec<LexicalEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceStatus {
    pub manifest: SourceManifest,
    pub installed: bool,
    pub enabled: bool,
}

/// Exact, case-sensitive lemma lookup. No language folding or hidden source
/// fallback. An empty `sources` list deliberately returns no entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupQuery {
    pub lemma: String,
    pub language: Option<String>,
    pub sources: Vec<SourceKey>,
}

#[derive(Debug)]
pub enum ReferenceError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Limit(&'static str),
    DigestMismatch,
    Busy,
    NotInstalled,
    Conflict,
    /// Valid state was published, but directory sync failed. Drop and reopen
    /// before further mutations; commit durability is uncertain.
    CommitUncertain(std::io::Error),
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "reference store I/O: {e}"),
            Self::Json(e) => write!(f, "reference JSON: {e}"),
            Self::Invalid(e) => write!(f, "invalid reference data: {e}"),
            Self::Limit(e) => write!(f, "reference limit exceeded: {e}"),
            Self::DigestMismatch => f.write_str("reference payload digest mismatch"),
            Self::Busy => {
                f.write_str("reference registry is already open or cannot acquire a temporary name")
            },
            Self::NotInstalled => f.write_str("reference source is not installed"),
            Self::Conflict => {
                f.write_str("reference source/version already has different metadata or data")
            },
            Self::CommitUncertain(error) => write!(
                f,
                "reference commit durability uncertain; close and reopen: {error}"
            ),
        }
    }
}
impl std::error::Error for ReferenceError {}
impl From<std::io::Error> for ReferenceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for ReferenceError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, ReferenceError>;

#[cfg(test)]
mod tests;
