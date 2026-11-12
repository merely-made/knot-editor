// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Closed-store migration of checkpoint storage, without carrying obsolete
//! redb pages into the replacement file.
//!
//! The caller authenticates history and custody, then supplies sealed bytes.
//! This module owns filesystem exclusion, logical copying and replacement. It
//! never opens an existing store through a caller-supplied backend: a new redb
//! open must obtain the source file's exclusive lock before preparation starts.
//! The source and destination locks remain held through Unix replacement.
//!
//! The containing directory must be host-owned and trusted. The pinned storage
//! backend opens by pathname, so these alias and identity checks do not promise
//! protection against a hostile directory owner racing pathname resolution.
//! Replacing the active file does not erase snapshots, backups, other copies,
//! or storage-device remnants of the old file.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};

use muniment::{Backend, RedbBackend, StoreError};
use serde::{Deserialize, Serialize};

pub(crate) type MigrationEntries = BTreeMap<String, Vec<u8>>;

/// A space's protected namespaces and the exact keys its custody can rewrite.
pub(crate) struct MigrationScope {
    pub managed_prefixes: Vec<String>,
    pub managed_keys: BTreeSet<String>,
}

pub(crate) enum MigrationDecision {
    /// The caller authenticated the current format marker and protected values.
    Current,
    Rewrite {
        /// `None` removes an explicitly managed key. Every existing managed
        /// key must be named, preventing an accidental plaintext carry-through.
        replacements: BTreeMap<String, Option<Vec<u8>>>,
    },
}

pub(crate) struct ClosedStore {
    pub backend: RedbBackend,
    pub receipt: MigrationReceipt,
}

/// Filesystem evidence only. Authentication, checkpoint frontiers, and custody
/// decisions belong to the caller's separate migration receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReceipt {
    pub rewritten: bool,
    pub source_logical_hash: [u8; 32],
    pub destination_logical_hash: [u8; 32],
    pub source_key_count: usize,
    pub destination_key_count: usize,
    pub preserved_key_count: usize,
    /// True only after a replacement and successful parent-directory fsync.
    /// A current-format open makes no new directory durability claim.
    pub directory_synced: bool,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MigrationError {
    #[error("checkpoint storage migration refused: {0}")]
    Rejected(String),
    #[error("checkpoint storage migration preparation failed: {0}")]
    Preparation(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(
        "checkpoint storage migration failed during {step} (replacement committed: {replacement_committed}): {source}"
    )]
    Io {
        step: &'static str,
        #[source]
        source: io::Error,
        replacement_committed: bool,
    },
    #[error("checkpoint storage migration staged data did not match prepared data")]
    Verification,
    #[cfg(not(unix))]
    #[error("legacy checkpoint storage replacement is supported only on Unix")]
    UnsupportedPlatform,
}

impl MigrationError {
    /// A directory-sync error can occur after the new file became active. A
    /// caller must not describe that outcome as an unchanged original store.
    pub fn replacement_committed(&self) -> bool {
        matches!(
            self,
            Self::Io {
                replacement_committed: true,
                ..
            }
        )
    }
}

/// Open an EXISTING store and let custody choose authenticated current format
/// or a fresh-file rewrite. A new store must be created by the host separately.
///
/// The preparation future receives owned logical data, not a clone of the live
/// source backend. Its rewrite replacements must already be sealed before any
/// staging file is created. Opaque keys outside the protected scope are copied
/// byte-for-byte. A preparation refusal preserves the source's logical data;
/// opening redb can itself commit housekeeping, so raw byte identity is not a
/// refusal guarantee.
pub(crate) async fn open_or_rewrite<F, Fut>(
    path: &Path,
    scope: &MigrationScope,
    prepare: F,
) -> Result<ClosedStore, MigrationError>
where
    F: FnOnce(MigrationEntries) -> Fut,
    Fut: Future<Output = Result<MigrationDecision, String>>,
{
    open_or_rewrite_inner(path, scope, prepare, &mut NoFaults).await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MigrationStep {
    BeforeStage,
    AfterCopyEntry(usize),
    BeforeVerify,
    BeforeReplace,
    AfterReplaceBeforeDirectorySync,
}

trait Faults {
    fn at(&mut self, _step: MigrationStep) -> io::Result<()> {
        Ok(())
    }

    fn copy_value<'a>(&'a self, _key: &str, value: &'a [u8]) -> &'a [u8] {
        value
    }
}

struct NoFaults;
impl Faults for NoFaults {}

fn io_failure(step: &'static str, source: io::Error, committed: bool) -> MigrationError {
    MigrationError::Io {
        step,
        source,
        replacement_committed: committed,
    }
}

fn rejected(message: &str) -> MigrationError {
    MigrationError::Rejected(message.to_owned())
}

fn validate_scope(scope: &MigrationScope) -> Result<(), MigrationError> {
    if scope.managed_prefixes.is_empty()
        || scope.managed_prefixes.iter().any(String::is_empty)
        || scope.managed_keys.iter().any(|key| {
            !scope
                .managed_prefixes
                .iter()
                .any(|prefix| key.starts_with(prefix))
        })
    {
        return Err(rejected("invalid protected namespace scope"));
    }
    Ok(())
}

fn validate_known_keys(
    entries: &MigrationEntries,
    scope: &MigrationScope,
) -> Result<(), MigrationError> {
    if entries.keys().any(|key| {
        scope
            .managed_prefixes
            .iter()
            .any(|prefix| key.starts_with(prefix))
            && !scope.managed_keys.contains(key)
    }) {
        return Err(rejected(
            "database contains a protected key outside this custody scope",
        ));
    }
    Ok(())
}

fn source_metadata(path: &Path) -> Result<Metadata, MigrationError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_failure("inspect source", error, false))?;
    if !metadata.file_type().is_file() {
        return Err(rejected(
            "source must be an existing regular file, without a symbolic link",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(rejected("source must have exactly one hard link"));
        }
    }
    Ok(metadata)
}

fn verify_source_identity(path: &Path, expected: &Metadata) -> Result<(), MigrationError> {
    let actual = source_metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if actual.dev() != expected.dev() || actual.ino() != expected.ino() {
            return Err(rejected("source identity changed during migration"));
        }
    }
    #[cfg(not(unix))]
    let _ = (actual, expected);
    Ok(())
}

async fn read_entries(backend: &RedbBackend) -> Result<MigrationEntries, MigrationError> {
    let mut entries = BTreeMap::new();
    for key in backend.list("").await? {
        let value = backend
            .get(&key)
            .await?
            .ok_or_else(|| rejected("a source key disappeared during the exclusive read"))?;
        if entries.insert(key, value).is_some() {
            return Err(rejected("source listing contained duplicate keys"));
        }
    }
    Ok(entries)
}

fn logical_hash(entries: &MigrationEntries) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"knot/checkpoint-storage/logical-map/v1\0");
    for (key, value) in entries {
        hasher.update(&(key.len() as u64).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    *hasher.finalize().as_bytes()
}

/// Removes only this uniquely created stage file on any pre-replacement exit.
/// The backend is dropped before this guard, so redb is closed before removal.
struct StageFile {
    path: PathBuf,
    file: File,
    replaced: bool,
}

impl StageFile {
    fn create(parent: &Path) -> Result<Self, MigrationError> {
        let path = parent.join(format!(".knot-checkpoint-{}.redb", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|error| io_failure("create stage", error, false))?;
        Ok(Self {
            path,
            file,
            replaced: false,
        })
    }
}

impl Drop for StageFile {
    fn drop(&mut self) {
        if !self.replaced {
            let _ = fs::remove_file(&self.path);
        }
    }
}

async fn open_or_rewrite_inner<F, Fut, I>(
    path: &Path,
    scope: &MigrationScope,
    prepare: F,
    faults: &mut I,
) -> Result<ClosedStore, MigrationError>
where
    F: FnOnce(MigrationEntries) -> Fut,
    Fut: Future<Output = Result<MigrationDecision, String>>,
    I: Faults,
{
    validate_scope(scope)?;
    let source_identity = source_metadata(path)?;
    // A newly obtained redb lock refuses any already open source or clone.
    let source = RedbBackend::open(path)?;
    verify_source_identity(path, &source_identity)?;
    let source_entries = read_entries(&source).await?;
    validate_known_keys(&source_entries, scope)?;
    let source_hash = logical_hash(&source_entries);
    let source_count = source_entries.len();
    let decision = prepare(source_entries.clone())
        .await
        .map_err(MigrationError::Preparation)?;
    let MigrationDecision::Rewrite { replacements } = decision else {
        verify_source_identity(path, &source_identity)?;
        return Ok(ClosedStore {
            backend: source,
            receipt: MigrationReceipt {
                rewritten: false,
                source_logical_hash: source_hash,
                destination_logical_hash: source_hash,
                source_key_count: source_count,
                destination_key_count: source_count,
                preserved_key_count: source_count,
                directory_synced: false,
            },
        });
    };
    #[cfg(not(unix))]
    {
        let _ = replacements;
        return Err(MigrationError::UnsupportedPlatform);
    }
    #[cfg(unix)]
    {
        if replacements
            .keys()
            .any(|key| !scope.managed_keys.contains(key))
        {
            return Err(rejected(
                "replacement names a key outside this custody scope",
            ));
        }
        if source_entries
            .keys()
            .any(|key| scope.managed_keys.contains(key) && !replacements.contains_key(key))
        {
            return Err(rejected("replacement omitted an existing protected key"));
        }
        let preserved_count = source_entries
            .keys()
            .filter(|key| !replacements.contains_key(*key))
            .count();
        let mut expected = source_entries;
        for (key, value) in replacements {
            match value {
                Some(value) => {
                    expected.insert(key, value);
                },
                None => {
                    expected.remove(&key);
                },
            }
        }
        validate_known_keys(&expected, scope)?;
        let destination_hash = logical_hash(&expected);
        faults
            .at(MigrationStep::BeforeStage)
            .map_err(|error| io_failure("before stage", error, false))?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        // This guard must be declared first so a failed destination backend is
        // dropped before the stage file is unlinked.
        let mut stage = StageFile::create(parent)?;
        let destination = RedbBackend::open(&stage.path)?;
        for (index, (key, value)) in expected.iter().enumerate() {
            destination.put(key, faults.copy_value(key, value)).await?;
            faults
                .at(MigrationStep::AfterCopyEntry(index + 1))
                .map_err(|error| io_failure("copy stage", error, false))?;
        }
        faults
            .at(MigrationStep::BeforeVerify)
            .map_err(|error| io_failure("verify stage", error, false))?;
        if read_entries(&destination).await? != expected {
            return Err(MigrationError::Verification);
        }
        stage
            .file
            .sync_all()
            .map_err(|error| io_failure("sync stage", error, false))?;
        faults
            .at(MigrationStep::BeforeReplace)
            .map_err(|error| io_failure("replace source", error, false))?;
        verify_source_identity(path, &source_identity)?;
        // No exposed source clones exist, and both database locks remain held.
        // On Unix, replacement unlinks the old inode without a plaintext backup.
        fs::rename(&stage.path, path)
            .map_err(|error| io_failure("replace source", error, false))?;
        stage.replaced = true;
        faults
            .at(MigrationStep::AfterReplaceBeforeDirectorySync)
            .map_err(|error| io_failure("sync directory", error, true))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_failure("sync directory", error, true))?;
        // Source Drop can update its allocator state; that write addresses only
        // the old unlinked inode, never the replacement now held by destination.
        drop(source);
        Ok(ClosedStore {
            backend: destination,
            receipt: MigrationReceipt {
                rewritten: true,
                source_logical_hash: source_hash,
                destination_logical_hash: destination_hash,
                source_key_count: source_count,
                destination_key_count: expected.len(),
                preserved_key_count: preserved_count,
                directory_synced: true,
            },
        })
    }
}

#[cfg(test)]
#[derive(Default)]
#[allow(dead_code, reason = "used by the filesystem integration harness")]
pub(crate) struct MigrationFaults {
    pub fail_at: Option<MigrationStep>,
    pub corrupt_copy: Option<(String, Vec<u8>)>,
}

#[cfg(test)]
impl Faults for MigrationFaults {
    fn at(&mut self, step: MigrationStep) -> io::Result<()> {
        if self.fail_at == Some(step) {
            Err(io::Error::other("injected checkpoint migration failure"))
        } else {
            Ok(())
        }
    }

    fn copy_value<'a>(&'a self, key: &str, value: &'a [u8]) -> &'a [u8] {
        match &self.corrupt_copy {
            Some((corrupt_key, corrupt_value)) if corrupt_key == key => corrupt_value,
            _ => value,
        }
    }
}

#[cfg(test)]
#[allow(dead_code, reason = "used by the filesystem integration harness")]
pub(crate) async fn open_or_rewrite_with_faults<F, Fut>(
    path: &Path,
    scope: &MigrationScope,
    prepare: F,
    faults: &mut MigrationFaults,
) -> Result<ClosedStore, MigrationError>
where
    F: FnOnce(MigrationEntries) -> Fut,
    Fut: Future<Output = Result<MigrationDecision, String>>,
{
    open_or_rewrite_inner(path, scope, prepare, faults).await
}
