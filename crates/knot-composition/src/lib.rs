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
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod item_ext;
pub mod retention;
pub mod selected_export;
pub mod wordnet_import;

pub use item_ext::{CollectionItemExt, ScenomiseRecipes};
pub use knot_sync::composition::{
    CollectionOrganization, OrganizeComposition,
    AnchorError, ByteSpan, CollectionError, CollectionItem, DocumentAnchor, ItemKind,
    MAX_ITEMS, MAX_STORE_BYTES, MAX_TEXT_BYTES, PackSource,
};

pub const STORE_VERSION: u32 = 1;

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
            items: candidate
                .iter()
                .map(item_ext::for_store_file)
                .collect::<Result<_, _>>()?,
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
