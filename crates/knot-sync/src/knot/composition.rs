// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The records of Knot's retained composition material, moved from
//! `knot-composition` so the sync grammar can validate and fold them without a
//! scene or graph crate. The projection recipe is carried as opaque JSON text
//! ([`RecipeJson`]); its meaning is checked by the [`RecipeValidator`] the
//! store is built with.

use std::fmt;
use std::io;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::recipe::{RecipeJson, RecipeValidator};


pub const MAX_STORE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ITEMS: usize = 10_000;
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_LABEL_BYTES: usize = 16 * 1024;
pub const MAX_ID_BYTES: usize = 256;
pub const MAX_TAGS: usize = 64;
pub const MAX_TAG_BYTES: usize = 1024;
pub const MAX_PACK_SOURCES: usize = 64;

/// The author's classification of a copy; not an inferred linguistic meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Word,
    Passage,
    Sense,
    Pronunciation,
    Note,
    ProjectionRecipe,
    MetreAlignment,
}

/// A precise reference to the pack edition from which material was collected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackSource {
    pub pack_id: String,
    pub pack_version: String,
    pub entry_ref: String,
}

impl PackSource {
    pub fn new(
        pack_id: impl Into<String>,
        pack_version: impl Into<String>,
        entry_ref: impl Into<String>,
    ) -> Self {
        Self {
            pack_id: pack_id.into(),
            pack_version: pack_version.into(),
            entry_ref: entry_ref.into(),
        }
    }
}

/// A half-open UTF-8 byte range, independent of display columns or characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub start: u64,
    pub end: u64,
}

/// A copied quotation with its original document address and byte location.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentAnchor {
    pub document_address: String,
    pub exact_quote: String,
    pub byte_span: ByteSpan,
    /// Lowercase BLAKE3 hexadecimal digest of the complete captured UTF-8 source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
}

impl DocumentAnchor {
    /// Capture exact bytes. Invalid or split-codepoint ranges are refused.
    pub fn capture(
        document_address: impl Into<String>,
        source: &str,
        byte_range: Range<usize>,
    ) -> Result<Self, AnchorError> {
        let document_address = document_address.into();
        if document_address.trim().is_empty() || document_address.len() > MAX_LABEL_BYTES {
            return Err(AnchorError::InvalidAddress);
        }
        let exact_quote = source
            .get(byte_range.clone())
            .filter(|quote| !quote.is_empty() && quote.len() <= MAX_TEXT_BYTES)
            .ok_or(AnchorError::InvalidRange)?;
        Ok(Self {
            document_address,
            exact_quote: exact_quote.to_owned(),
            byte_span: ByteSpan {
                start: byte_range.start as u64,
                end: byte_range.end as u64,
            },
            source_hash: Some(blake3::hash(source.as_bytes()).to_hex().to_string()),
        })
    }

    /// Validate against explicitly supplied current source. Never search for a
    /// similar quote or retarget a stale anchor. Even an unchanged quotation is
    /// stale when the captured whole-source hash differs.
    pub fn validate_source(&self, source: &str) -> Result<(), AnchorError> {
        self.validate_shape()?;
        if self
            .source_hash
            .as_ref()
            .is_some_and(|hash| *hash != blake3::hash(source.as_bytes()).to_hex().as_str())
        {
            return Err(AnchorError::StaleSource);
        }
        let start = usize::try_from(self.byte_span.start).map_err(|_| AnchorError::InvalidRange)?;
        let end = usize::try_from(self.byte_span.end).map_err(|_| AnchorError::InvalidRange)?;
        let quote = source.get(start..end).ok_or(AnchorError::InvalidRange)?;
        if quote != self.exact_quote {
            return Err(AnchorError::QuoteMismatch);
        }
        Ok(())
    }

    pub fn validate_shape(&self) -> Result<(), AnchorError> {
        if self.document_address.trim().is_empty() || self.document_address.len() > MAX_LABEL_BYTES
        {
            return Err(AnchorError::InvalidAddress);
        }
        if self.exact_quote.is_empty()
            || self.exact_quote.len() > MAX_TEXT_BYTES
            || self.byte_span.end.checked_sub(self.byte_span.start)
                != Some(self.exact_quote.len() as u64)
        {
            return Err(AnchorError::InvalidRange);
        }
        if self.source_hash.as_ref().is_some_and(|hash| {
            hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err(AnchorError::InvalidHash);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorError {
    InvalidAddress,
    InvalidRange,
    InvalidHash,
    StaleSource,
    QuoteMismatch,
}

impl fmt::Display for AnchorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidAddress => "document address is empty or too large",
            Self::InvalidRange => {
                "quotation range is invalid, too large, or not a UTF-8 byte range"
            },
            Self::InvalidHash => "source hash is not a lowercase BLAKE3 digest",
            Self::StaleSource => "document source differs from the captured source",
            Self::QuoteMismatch => "original byte range no longer matches the copied quotation",
        })
    }
}

impl std::error::Error for AnchorError {}

/// An independent durable copy. Removing a pack or changing a document cannot
/// change this material. `collection` groups it across all readings/documents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionItem {
    pub id: String,
    pub collection: String,
    pub kind: ItemKind,
    pub label: String,
    pub text: String,
    /// Typed recipe and retained disclosure; `text` remains a readable summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_recipe: Option<RecipeJson>,
    /// Copied, validated metrical alignment; never a handle into a document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metre_scene: Option<sceno_backdrops::SceneInput>,
    #[serde(default)]
    pub pack_sources: Vec<PackSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_source: Option<DocumentAnchor>,
    #[serde(default)]
    pub author_notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Author-chosen ordering key. Storage also preserves insertion order.
    #[serde(default)]
    pub order: i64,
}

impl CollectionItem {
    pub fn new(kind: ItemKind, label: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            collection: "Commonplace".to_owned(),
            kind,
            label: label.into(),
            text: text.into(),
            projection_recipe: None,
            metre_scene: None,
            pack_sources: Vec::new(),
            document_source: None,
            author_notes: String::new(),
            tags: Vec::new(),
            order: 0,
        }
    }

    pub fn in_collection(mut self, collection: impl Into<String>) -> Self {
        self.collection = collection.into();
        self
    }

    pub fn with_projection_recipe_json(mut self, recipe: RecipeJson) -> Self {
        self.kind = ItemKind::ProjectionRecipe;
        self.projection_recipe = Some(recipe);
        self
    }

    pub fn with_metre_scene(mut self, scene: sceno_backdrops::SceneInput) -> Self {
        self.kind = ItemKind::MetreAlignment;
        self.metre_scene = Some(scene);
        self
    }

    pub fn with_pack_source(mut self, source: PackSource) -> Self {
        self.pack_sources.push(source);
        self
    }

    pub fn with_document_source(mut self, source: DocumentAnchor) -> Self {
        self.document_source = Some(source);
        self
    }

    pub fn with_author_notes(mut self, notes: impl Into<String>) -> Self {
        self.author_notes = notes.into();
        self
    }

    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_order(mut self, order: i64) -> Self {
        self.order = order;
        self
    }

    /// Check the item; `recipes` judges a carried projection recipe.
    pub fn validate_with(&self, recipes: &dyn RecipeValidator) -> Result<(), CollectionError> {
        match (&self.kind, &self.metre_scene) {
            (ItemKind::MetreAlignment, Some(scene)) => {
                if !matches!(scene.facts, sceno_backdrops::SceneFacts::MetreGrid(_)) {
                    return Err(CollectionError::Invalid(
                        "Metre alignment requires metrical facts".into(),
                    ));
                }
                scene
                    .validate()
                    .map_err(|e| CollectionError::Invalid(e.to_string()))?;
                if serde_json::to_vec(scene).map_err(CollectionError::Malformed)?.len() > sceno_backdrops::MAX_SCENE_INPUT_BYTES {
                    return Err(CollectionError::Invalid(
                        "Metre scene exceeds one MiB".into(),
                    ));
                }
            },
            (ItemKind::MetreAlignment, None) | (_, Some(_)) => {
                return Err(CollectionError::Invalid(
                    "Metre kind and payload must match".into(),
                ));
            },
            _ => {},
        }
        match (&self.kind, &self.projection_recipe) {
            (ItemKind::ProjectionRecipe, Some(recipe)) => recipes.validate(recipe)?,
            (ItemKind::ProjectionRecipe, None) | (_, Some(_)) => {
                return Err(CollectionError::Invalid(
                    "projection recipe kind and typed payload must match".into(),
                ));
            },
            (_, None) => {},
        }
        bounded_nonempty("item id", &self.id, MAX_ID_BYTES)?;
        bounded_nonempty("collection", &self.collection, MAX_ID_BYTES)?;
        bounded_nonempty("label", &self.label, MAX_LABEL_BYTES)?;
        if self.text.is_empty() || self.text.len() > MAX_TEXT_BYTES {
            return Err(CollectionError::Invalid(
                "copied text is empty or too large".into(),
            ));
        }
        if self.author_notes.len() > MAX_TEXT_BYTES {
            return Err(CollectionError::Invalid(
                "author notes are too large".into(),
            ));
        }
        if self.tags.len() > MAX_TAGS || self.pack_sources.len() > MAX_PACK_SOURCES {
            return Err(CollectionError::Invalid(
                "too many tags or pack references".into(),
            ));
        }
        for tag in &self.tags {
            bounded_nonempty("tag", tag, MAX_TAG_BYTES)?;
        }
        for source in &self.pack_sources {
            bounded_nonempty("pack id", &source.pack_id, MAX_LABEL_BYTES)?;
            bounded_nonempty("pack version", &source.pack_version, MAX_LABEL_BYTES)?;
            bounded_nonempty("pack entry reference", &source.entry_ref, MAX_LABEL_BYTES)?;
        }
        if let Some(source) = &self.document_source {
            source.validate_shape().map_err(CollectionError::Anchor)?;
        }
        Ok(())
    }
}

pub fn bounded_nonempty(name: &str, value: &str, max: usize) -> Result<(), CollectionError> {
    if value.trim().is_empty() || value.len() > max {
        Err(CollectionError::Invalid(format!(
            "{name} is empty or too large"
        )))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub enum CollectionError {
    Io(io::Error),
    Malformed(serde_json::Error),
    UnsupportedVersion(u32),
    Invalid(String),
    Anchor(AnchorError),
    DuplicateId(String),
    TooLarge,
    ConcurrentChange,
    /// Atomic replacement succeeded and in-memory state was updated, but the
    /// directory could not be synchronized. Do not blindly retry `collect`.
    DurabilityUncertain(io::Error),
}

impl fmt::Display for CollectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "collection I/O failed: {error}"),
            Self::Malformed(error) => write!(f, "collection store is malformed: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported collection version {version}")
            },
            Self::Invalid(reason) => write!(f, "invalid collection material: {reason}"),
            Self::Anchor(error) => write!(f, "invalid document provenance: {error}"),
            Self::DuplicateId(id) => write!(f, "collection item already exists: {id}"),
            Self::TooLarge => {
                f.write_str("collection store exceeds its bounded size or item count")
            },
            Self::ConcurrentChange => {
                f.write_str("collection store changed externally; reopen before editing")
            },
            Self::DurabilityUncertain(error) => write!(
                f,
                "collection saved, but directory synchronization failed: {error}; reopen before retrying"
            ),
        }
    }
}

impl std::error::Error for CollectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::DurabilityUncertain(error) => Some(error),
            Self::Malformed(error) => Some(error),
            Self::Anchor(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CollectionError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionOrganization {
    pub label: String,
    pub collection: String,
    pub author_notes: String,
    pub tags: Vec<String>,
    pub order: i64,
    pub archived: bool,
}

impl CollectionOrganization {
    pub fn from_item(item: &CollectionItem) -> Self {
        Self {
            label: item.label.clone(),
            collection: item.collection.clone(),
            author_notes: item.author_notes.clone(),
            tags: item.tags.clone(),
            order: item.order,
            archived: false,
        }
    }

    pub fn validate(&self) -> Result<(), CollectionError> {
        bounded_nonempty("label", &self.label, MAX_LABEL_BYTES)?;
        bounded_nonempty("collection", &self.collection, MAX_ID_BYTES)?;
        if self.author_notes.len() > MAX_TEXT_BYTES || self.tags.len() > MAX_TAGS {
            return Err(CollectionError::Invalid(
                "organization exceeds metadata bounds".into(),
            ));
        }
        for tag in &self.tags {
            bounded_nonempty("tag", tag, MAX_TAG_BYTES)?;
        }
        Ok(())
    }
}


/// Compare-and-set against a retained item's organization, scoped by the
/// original writer and item identity rather than a transient row index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizeComposition {
    pub author: [u8; 32],
    pub item_id: String,
    pub expected_revision: [u8; 32],
    pub organization: CollectionOrganization,
}
