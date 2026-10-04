// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Personal composition material, independent of document tabs and installed packs.
//!
//! Items retain copied text and labels. Provenance is evidence, never an instruction
//! to fetch, replace, normalize, or relocate personal material. Collection storage
//! has no pack lookup or network behavior; the separate offline WordNet importer
//! uses the shared reference-data schema. Opening an absent store is read-only; writes
//! use a synchronized same-directory temporary file and atomic replacement.

use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod wordnet_import;
pub mod retention;

pub const STORE_VERSION: u32 = 1;
pub const MAX_STORE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ITEMS: usize = 10_000;
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_LABEL_BYTES: usize = 16 * 1024;
const MAX_ID_BYTES: usize = 256;
const MAX_TAGS: usize = 64;
const MAX_TAG_BYTES: usize = 1024;
const MAX_PACK_SOURCES: usize = 64;

/// The author's classification of a copy; not an inferred linguistic meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Word,
    Passage,
    Sense,
    Pronunciation,
    Note,
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

    fn validate_shape(&self) -> Result<(), AnchorError> {
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionItem {
    pub id: String,
    pub collection: String,
    pub kind: ItemKind,
    pub label: String,
    pub text: String,
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

    pub fn validate(&self) -> Result<(), CollectionError> {
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

fn bounded_nonempty(name: &str, value: &str, max: usize) -> Result<(), CollectionError> {
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreData {
    version: u32,
    items: Vec<CollectionItem>,
}

/// A local personal store. Cooperation between store handles uses a stable
/// sidecar file lock plus an exact on-disk digest check; stale handles must reopen.
/// This is not a collaborative or network synchronization protocol.
pub struct CollectionStore {
    path: PathBuf,
    items: Vec<CollectionItem>,
    disk_hash: Option<blake3::Hash>,
}

impl CollectionStore {
    /// Never creates, repairs or overwrites an existing file during open.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CollectionError> {
        let requested = path.as_ref();
        if requested.file_name().is_none() {
            return Err(CollectionError::Invalid(
                "store path must name a file".into(),
            ));
        }
        let path = if requested.is_absolute() {
            requested.to_owned()
        } else {
            std::env::current_dir()?.join(requested)
        };
        let bytes = read_store(&path)?;
        let items = if let Some(bytes) = &bytes {
            let data: StoreData =
                serde_json::from_slice(bytes).map_err(CollectionError::Malformed)?;
            if data.version != STORE_VERSION {
                return Err(CollectionError::UnsupportedVersion(data.version));
            }
            validate_items(&data.items)?;
            data.items
        } else {
            Vec::new()
        };
        Ok(Self {
            path,
            items,
            disk_hash: bytes.as_deref().map(blake3::hash),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn items(&self) -> &[CollectionItem] {
        &self.items
    }

    /// Adds a new independent copy. Duplicate identities never overwrite copies.
    /// Rejected writes leave both the previous file and in-memory items intact.
    pub fn collect(&mut self, item: CollectionItem) -> Result<(), CollectionError> {
        item.validate()?;
        if self.items.iter().any(|existing| existing.id == item.id) {
            return Err(CollectionError::DuplicateId(item.id));
        }
        let mut candidate = self.items.clone();
        candidate.push(item);
        self.persist(candidate)
    }

    pub fn remove(&mut self, id: &str) -> Result<bool, CollectionError> {
        if !self.items.iter().any(|item| item.id == id) {
            return Ok(false);
        }
        let candidate = self
            .items
            .iter()
            .filter(|item| item.id != id)
            .cloned()
            .collect();
        self.persist(candidate)?;
        Ok(true)
    }

    fn persist(&mut self, candidate: Vec<CollectionItem>) -> Result<(), CollectionError> {
        validate_items(&candidate)?;
        let data = StoreData {
            version: STORE_VERSION,
            items: candidate,
        };
        let bytes = serde_json::to_vec_pretty(&data).map_err(CollectionError::Malformed)?;
        if bytes.len() > MAX_STORE_BYTES {
            return Err(CollectionError::TooLarge);
        }
        let parent = self.path.parent().expect("absolute file path has a parent");
        fs::create_dir_all(parent)?;
        let mut lock_name = self
            .path
            .file_name()
            .expect("validated file name")
            .to_os_string();
        lock_name.push(".lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(parent.join(lock_name))?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => CollectionError::ConcurrentChange,
            std::fs::TryLockError::Error(error) => CollectionError::Io(error),
        })?;
        let current = read_store(&self.path)?;
        if current.as_deref().map(blake3::hash) != self.disk_hash {
            return Err(CollectionError::ConcurrentChange);
        }
        // Open the directory before publication so an open failure cannot make a
        // successful save look like a refused mutation.
        #[cfg(unix)]
        let directory = File::open(parent)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".knot-collection-")
            .tempfile_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|error| CollectionError::Io(error.error))?;
        self.items = data.items;
        self.disk_hash = Some(blake3::hash(&bytes));
        #[cfg(unix)]
        directory
            .sync_all()
            .map_err(CollectionError::DurabilityUncertain)?;
        // The OS releases the lock when this handle drops, including every error.
        drop(lock);
        Ok(())
    }
}

fn validate_items(items: &[CollectionItem]) -> Result<(), CollectionError> {
    if items.len() > MAX_ITEMS {
        return Err(CollectionError::TooLarge);
    }
    let mut identities = HashSet::with_capacity(items.len());
    for item in items {
        item.validate()?;
        if !identities.insert(&item.id) {
            return Err(CollectionError::DuplicateId(item.id.clone()));
        }
    }
    Ok(())
}

fn read_store(path: &Path) -> Result<Option<Vec<u8>>, CollectionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(CollectionError::Invalid(
                    "store must be a regular file, not a symlink or directory".into(),
                ));
            }
            if metadata.len() > MAX_STORE_BYTES as u64 {
                return Err(CollectionError::TooLarge);
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_STORE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_STORE_BYTES {
        return Err(CollectionError::TooLarge);
    }
    Ok(Some(bytes))
}
