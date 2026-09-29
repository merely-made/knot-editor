// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Private, app-local crash recovery records for the standalone desktop.
//!
//! Recovery identities are independent of runtime document keys. Each item
//! is an individually checksummed JSON file, replaced through a synced sibling
//! file and atomic rename. A per-session advisory lock identifies live app
//! sessions; record contents alone never grant authority to claim a session.

use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const RECOVERY_VERSION: u32 = 1;
const STORE_DIR: &str = "items";
const SESSION_DIR: &str = "sessions";
const MUTATION_LOCK: &str = ".mutation.lock";
const MAX_POLICY_RECORDS: usize = MAX_SCAN_ENTRIES;
const HARD_MAX_ITEM_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SCAN_ENTRIES: usize = 2_048;
const MAX_SCAN_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ISSUES: usize = 64;
static MUTATION_MUTEX: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecoveryId(Uuid);

impl RecoveryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RecoveryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RecoveryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaretSelection {
    pub anchor: usize,
    pub head: usize,
    /// `true` is upstream affinity; `false` is downstream affinity.
    pub anchor_upstream: bool,
    /// `true` is upstream affinity; `false` is downstream affinity.
    pub head_upstream: bool,
}

/// The local retention limits. The hard per-item ceiling remains 64 MiB even
/// if a caller configures a larger value, so a corrupt file cannot trigger an
/// unbounded allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRetention {
    pub max_records: usize,
    pub max_total_bytes: u64,
    pub max_item_bytes: u64,
    /// Expire only records belonging to sessions that are no longer live.
    pub max_age_days: Option<u32>,
}

impl Default for RecoveryRetention {
    fn default() -> Self {
        Self {
            max_records: 20,
            max_total_bytes: 100 * 1024 * 1024,
            max_item_bytes: 8 * 1024 * 1024,
            max_age_days: Some(30),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub id: RecoveryId,
    pub source_text: String,
    pub format: String,
    pub original_path: Option<PathBuf>,
    pub original_address: Option<String>,
    pub selection: CaretSelection,
    pub active_hint: bool,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    /// Opaque owner token assigned by `RecoveryStore::upsert`.
    pub session_id: String,
}

impl RecoveryRecord {
    pub fn new(id: RecoveryId, source_text: String, format: impl Into<String>) -> Self {
        Self {
            id,
            source_text,
            format: format.into(),
            original_path: None,
            original_address: None,
            selection: CaretSelection::default(),
            active_hint: false,
            created_at_ms: 0,
            updated_at_ms: 0,
            session_id: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryListing {
    pub records: Vec<RecoveryRecord>,
    pub issues: Vec<RecoveryIssue>,
    /// False means a hard directory or byte budget stopped inspection.
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryIssue {
    pub path: PathBuf,
    pub kind: RecoveryIssueKind,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryIssueKind {
    Corrupt,
    ChecksumMismatch,
    UnsupportedVersion,
    Oversized,
    UnsafePath,
    Unreadable,
    Retention,
}

#[derive(Debug)]
pub enum RecoveryError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Serialization(String),
    ItemTooLarge {
        actual: u64,
        limit: u64,
    },
    Capacity {
        detail: String,
    },
    OwnedByLiveSession,
    Busy,
    InvalidRetention,
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Serialization(detail) => write!(f, "recovery serialization failed: {detail}"),
            Self::ItemTooLarge { actual, limit } => {
                write!(f, "recovery item is {actual} bytes; limit is {limit}")
            },
            Self::Capacity { detail } => write!(f, "recovery retention limit: {detail}"),
            Self::OwnedByLiveSession => write!(f, "recovery item belongs to another live session"),
            Self::Busy => write!(f, "recovery store is busy; retry shortly"),
            Self::InvalidRetention => write!(f, "recovery retention limits are invalid"),
        }
    }
}

impl std::error::Error for RecoveryError {}

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    checksum: String,
    record: RecoveryRecord,
}

/// One app window's handle to the app-local recovery directory.
pub struct RecoveryStore {
    items: PathBuf,
    sessions: PathBuf,
    session_id: String,
    session_lock_path: PathBuf,
    _session_lock: File,
    retention: RecoveryRetention,
    startup_issue: Mutex<Option<RecoveryIssue>>,
}

struct MutationLock {
    _file: File,
    _process_guard: MutexGuard<'static, ()>,
}

impl Drop for MutationLock {
    fn drop(&mut self) {}
}

impl RecoveryStore {
    pub fn open(
        root: impl Into<PathBuf>,
        retention: RecoveryRetention,
    ) -> Result<Self, RecoveryError> {
        validate_retention(&retention)?;
        let root = root.into();
        ensure_private_directory(&root)?;
        let items = root.join(STORE_DIR);
        let sessions = root.join(SESSION_DIR);
        ensure_private_directory(&items)?;
        ensure_private_directory(&sessions)?;
        let init_lock = mutation_lock_at(&root)?;
        cleanup_stale_sessions(&sessions)?;

        let (session_id, session_lock_path, session_lock) = loop {
            let id = Uuid::new_v4().to_string();
            let path = sessions.join(format!("{id}.lock"));
            match private_create_options()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    set_private_file(&file).map_err(|source| RecoveryError::Io {
                        path: path.clone(),
                        source,
                    })?;
                    file.write_all(id.as_bytes())
                        .and_then(|()| file.sync_all())
                        .map_err(|source| RecoveryError::Io {
                            path: path.clone(),
                            source,
                        })?;
                    file.try_lock().map_err(|error| RecoveryError::Io {
                        path: path.clone(),
                        source: try_lock_error(error),
                    })?;
                    break (id, path, file);
                },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(RecoveryError::Io { path, source }),
            }
        };

        let store = Self {
            items,
            sessions,
            session_id,
            session_lock_path,
            _session_lock: session_lock,
            retention,
            startup_issue: Mutex::new(None),
        };
        drop(init_lock);
        if let Err(error) = store.prune_to_policy()
            && !has_storage_issue(&store.list())
        {
            *store
                .startup_issue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(issue(
                &store.items,
                RecoveryIssueKind::Retention,
                error.to_string(),
            ));
        }
        Ok(store)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// A session is live only while a process holds the advisory lock on its
    /// marker. The serialized session id is not itself proof of liveness.
    pub fn is_session_live(&self, session_id: &str) -> bool {
        if session_id == self.session_id {
            return true;
        }
        let Some(path) = valid_session_path(&self.sessions, session_id) else {
            return true;
        };
        match open_regular_file(&path, false) {
            Ok(file) => match file.try_lock() {
                Ok(()) => {
                    let _ = file.unlock();
                    false
                },
                Err(std::fs::TryLockError::WouldBlock) => true,
                Err(std::fs::TryLockError::Error(_)) => true,
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => true,
        }
    }

    pub fn list(&self) -> RecoveryListing {
        let mut listing = RecoveryListing {
            records: Vec::new(),
            issues: Vec::new(),
            complete: true,
        };
        if let Some(issue) = self
            .startup_issue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        {
            add_issue(&mut listing, issue);
        }
        let entries = match fs::read_dir(&self.items) {
            Ok(entries) => entries,
            Err(error) => {
                add_issue(
                    &mut listing,
                    issue(
                        &self.items,
                        RecoveryIssueKind::Unreadable,
                        error.to_string(),
                    ),
                );
                listing.complete = false;
                return listing;
            },
        };
        let mut scanned_entries = 0usize;
        let mut scanned_bytes = 0u64;
        for entry in entries {
            scanned_entries += 1;
            if scanned_entries > MAX_SCAN_ENTRIES {
                listing.complete = false;
                add_issue(
                    &mut listing,
                    issue(
                        &self.items,
                        RecoveryIssueKind::Oversized,
                        "record directory entry limit exceeded",
                    ),
                );
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    add_issue(
                        &mut listing,
                        issue(
                            &self.items,
                            RecoveryIssueKind::Unreadable,
                            error.to_string(),
                        ),
                    );
                    continue;
                },
            };
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let entry_size = match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_file() => meta.len(),
                Ok(_) => {
                    add_issue(
                        &mut listing,
                        issue(
                            &path,
                            RecoveryIssueKind::UnsafePath,
                            "entry is not a regular file",
                        ),
                    );
                    continue;
                },
                Err(error) => {
                    add_issue(
                        &mut listing,
                        issue(&path, RecoveryIssueKind::Unreadable, error.to_string()),
                    );
                    continue;
                },
            };
            if entry_size > MAX_SCAN_BYTES.saturating_sub(scanned_bytes) {
                listing.complete = false;
                add_issue(
                    &mut listing,
                    issue(
                        &path,
                        RecoveryIssueKind::Oversized,
                        "aggregate record read budget exceeded",
                    ),
                );
                break;
            }
            scanned_bytes = scanned_bytes.saturating_add(entry_size);
            match read_record(&path) {
                Ok(record) => {
                    if item_path(&self.items, record.id) != path {
                        add_issue(
                            &mut listing,
                            issue(
                                &path,
                                RecoveryIssueKind::Corrupt,
                                "record identity does not match filename",
                            ),
                        );
                    } else {
                        listing.records.push(record);
                    }
                },
                Err(problem) => add_issue(&mut listing, problem),
            }
        }
        listing
            .records
            .sort_by_key(|record| (record.updated_at_ms, record.id.to_string()));
        listing
    }

    /// Insert or atomically replace exactly the record named by `record.id`.
    /// Records owned by another live session cannot be overwritten.
    pub fn upsert(&self, mut record: RecoveryRecord) -> Result<(), RecoveryError> {
        let _mutation = self.mutation_lock()?;
        let configured_limit = self.retention.max_item_bytes.min(HARD_MAX_ITEM_BYTES);
        record.session_id = self.session_id.clone();
        record.updated_at_ms = now_ms();

        let target = item_path(&self.items, record.id);
        if let Some(existing) = self.read_existing(&target)? {
            if existing.session_id != self.session_id && self.is_session_live(&existing.session_id)
            {
                return Err(RecoveryError::OwnedByLiveSession);
            }
            record.created_at_ms = existing.created_at_ms;
        } else if record.created_at_ms == 0 {
            record.created_at_ms = record.updated_at_ms;
        }

        let bytes = encode_record(&record)?;
        if bytes.len() as u64 > configured_limit {
            return Err(RecoveryError::ItemTooLarge {
                actual: bytes.len() as u64,
                limit: configured_limit,
            });
        }
        let listing = self.list();
        if !listing.complete || has_storage_issue(&listing) {
            return Err(RecoveryError::Capacity {
                detail:
                    "store contains unreadable or uninspected entries; refusing retention changes"
                        .into(),
            });
        }
        let mut valid = listing.records;
        let mut retained: Vec<_> = valid
            .drain(..)
            .filter(|existing| existing.id != record.id)
            .collect();
        let mut victims = Vec::<RecoveryId>::new();
        let now = now_ms();
        if let Some(days) = self.retention.max_age_days {
            let cutoff = now.saturating_sub(u64::from(days).saturating_mul(86_400_000));
            retained.retain(|old| {
                if old.updated_at_ms < cutoff && !self.is_session_live(&old.session_id) {
                    victims.push(old.id);
                    false
                } else {
                    true
                }
            });
        }

        let count_limit = self.retention.max_records;
        let total_limit = self.retention.max_total_bytes;
        let mut size = retained
            .iter()
            .map(|item| encoded_len(item).unwrap_or(u64::MAX))
            .sum::<u64>();
        // Oldest dead-session records are evicted first. Live sessions remain
        // protected, even when that means refusing this new snapshot.
        retained.sort_by_key(|item| item.updated_at_ms);
        while retained.len() + 1 > count_limit
            || size.saturating_add(bytes.len() as u64) > total_limit
        {
            let Some(index) = retained
                .iter()
                .position(|item| !self.is_session_live(&item.session_id))
            else {
                return Err(RecoveryError::Capacity {
                    detail: "all remaining records belong to live sessions".into(),
                });
            };
            let old = retained.remove(index);
            size = size.saturating_sub(encoded_len(&old).unwrap_or(u64::MAX));
            victims.push(old.id);
        }
        if self.retention.max_records == 0 || bytes.len() as u64 > total_limit {
            return Err(RecoveryError::Capacity {
                detail: "configured limits cannot hold this record".into(),
            });
        }

        atomic_replace(&target, &bytes)?;
        for id in victims {
            let path = item_path(&self.items, id);
            if path != target {
                remove_regular_file(&path).map_err(|source| RecoveryError::Io {
                    path: path.clone(),
                    source,
                })?;
            }
        }
        sync_directory(&self.items)?;
        self.clear_startup_issue();
        Ok(())
    }

    pub fn remove(&self, id: RecoveryId) -> Result<bool, RecoveryError> {
        let _mutation = self.mutation_lock()?;
        let path = item_path(&self.items, id);
        if let Some(existing) = self.read_existing(&path)? {
            if existing.session_id != self.session_id && self.is_session_live(&existing.session_id)
            {
                return Err(RecoveryError::OwnedByLiveSession);
            }
        } else {
            return Ok(false);
        }
        remove_regular_file(&path).map_err(|source| RecoveryError::Io { path, source })?;
        sync_directory(&self.items)?;
        Ok(true)
    }

    /// Claim a crash candidate before restoring it. The shared mutation lock
    /// makes this transfer exclusive across windows and processes.
    pub fn claim(&self, id: RecoveryId) -> Result<Option<RecoveryRecord>, RecoveryError> {
        let _mutation = self.mutation_lock()?;
        let path = item_path(&self.items, id);
        let Some(mut record) = self.read_existing(&path)? else {
            return Ok(None);
        };
        if record.id != id {
            return Err(RecoveryError::Serialization(
                "record identity does not match requested id".into(),
            ));
        }
        if record.session_id != self.session_id && self.is_session_live(&record.session_id) {
            return Err(RecoveryError::OwnedByLiveSession);
        }
        record.session_id = self.session_id.clone();
        record.updated_at_ms = now_ms();
        atomic_replace(&path, &encode_record(&record)?)?;
        Ok(Some(record))
    }

    pub fn set_retention(&mut self, retention: RecoveryRetention) -> Result<(), RecoveryError> {
        validate_retention(&retention)?;
        let previous = self.retention.clone();
        self.retention = retention;
        if let Err(error) = self.prune_to_policy() {
            self.retention = previous;
            return Err(error);
        }
        self.clear_startup_issue();
        Ok(())
    }

    fn prune_to_policy(&self) -> Result<(), RecoveryError> {
        let _mutation = self.mutation_lock()?;
        let listing = self.list();
        if !listing.complete || has_storage_issue(&listing) {
            return Err(RecoveryError::Capacity {
                detail:
                    "store contains unreadable or uninspected entries; refusing retention changes"
                        .into(),
            });
        }
        let mut records = listing.records;
        let cutoff = self
            .retention
            .max_age_days
            .map(|days| now_ms().saturating_sub(u64::from(days).saturating_mul(86_400_000)));
        let mut victims = Vec::new();
        records.retain(|record| {
            if cutoff.is_some_and(|limit| record.updated_at_ms < limit)
                && !self.is_session_live(&record.session_id)
            {
                victims.push(record.id);
                false
            } else {
                true
            }
        });
        let mut size = records
            .iter()
            .map(|record| encoded_len(record).unwrap_or(u64::MAX))
            .sum::<u64>();
        records.sort_by_key(|record| record.updated_at_ms);
        while records.len() > self.retention.max_records || size > self.retention.max_total_bytes {
            let Some(index) = records
                .iter()
                .position(|record| !self.is_session_live(&record.session_id))
            else {
                return Err(RecoveryError::Capacity {
                    detail: "retention limits are exceeded by live records".into(),
                });
            };
            let record = records.remove(index);
            size = size.saturating_sub(encoded_len(&record).unwrap_or(u64::MAX));
            victims.push(record.id);
        }
        let did_remove = !victims.is_empty();
        for id in victims {
            let path = item_path(&self.items, id);
            remove_regular_file(&path).map_err(|source| RecoveryError::Io { path, source })?;
        }
        if did_remove {
            sync_directory(&self.items)?;
        }
        Ok(())
    }

    fn read_existing(&self, path: &Path) -> Result<Option<RecoveryRecord>, RecoveryError> {
        match read_record(path) {
            Ok(record) => Ok(Some(record)),
            Err(problem) if problem.kind == RecoveryIssueKind::Unreadable && !path.exists() => {
                Ok(None)
            },
            Err(problem) => Err(RecoveryError::Io {
                path: problem.path,
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, problem.detail),
            }),
        }
    }

    fn clear_startup_issue(&self) {
        *self
            .startup_issue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    fn mutation_lock(&self) -> Result<MutationLock, RecoveryError> {
        mutation_lock_at(self.items.parent().unwrap_or(&self.items))
    }
}

impl Drop for RecoveryStore {
    fn drop(&mut self) {
        let _ = self._session_lock.unlock();
        let _ = remove_regular_file(&self.session_lock_path);
    }
}

fn validate_retention(policy: &RecoveryRetention) -> Result<(), RecoveryError> {
    if policy.max_records > MAX_POLICY_RECORDS
        || policy.max_item_bytes == 0
        || policy.max_item_bytes > HARD_MAX_ITEM_BYTES
        || policy.max_total_bytes == 0
        || policy.max_total_bytes > MAX_SCAN_BYTES
    {
        return Err(RecoveryError::InvalidRetention);
    }
    Ok(())
}

fn encode_record(record: &RecoveryRecord) -> Result<Vec<u8>, RecoveryError> {
    let payload = serde_json::to_vec(record)
        .map_err(|error| RecoveryError::Serialization(error.to_string()))?;
    let envelope = Envelope {
        version: RECOVERY_VERSION,
        checksum: blake3::hash(&payload).to_hex().to_string(),
        record: record.clone(),
    };
    let mut bytes = serde_json::to_vec(&envelope)
        .map_err(|error| RecoveryError::Serialization(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn encoded_len(record: &RecoveryRecord) -> Result<u64, RecoveryError> {
    Ok(encode_record(record)?.len() as u64)
}

fn read_record(path: &Path) -> Result<RecoveryRecord, RecoveryIssue> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| issue(path, RecoveryIssueKind::Unreadable, error.to_string()))?;
    if !metadata.file_type().is_file() {
        return Err(issue(
            path,
            RecoveryIssueKind::UnsafePath,
            "record is not a regular file",
        ));
    }
    if metadata.len() > HARD_MAX_ITEM_BYTES {
        return Err(issue(
            path,
            RecoveryIssueKind::Oversized,
            format!("record is {} bytes", metadata.len()),
        ));
    }
    let file = open_regular_file(path, false)
        .map_err(|error| issue(path, RecoveryIssueKind::UnsafePath, error.to_string()))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(HARD_MAX_ITEM_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| issue(path, RecoveryIssueKind::Unreadable, error.to_string()))?;
    if bytes.len() as u64 > HARD_MAX_ITEM_BYTES {
        return Err(issue(
            path,
            RecoveryIssueKind::Oversized,
            "record exceeded bounded read limit",
        ));
    }
    let envelope: Envelope = serde_json::from_slice(&bytes)
        .map_err(|error| issue(path, RecoveryIssueKind::Corrupt, error.to_string()))?;
    if envelope.version > RECOVERY_VERSION {
        return Err(issue(
            path,
            RecoveryIssueKind::UnsupportedVersion,
            format!(
                "version {} is newer than {}",
                envelope.version, RECOVERY_VERSION
            ),
        ));
    }
    if envelope.version != RECOVERY_VERSION {
        return Err(issue(
            path,
            RecoveryIssueKind::UnsupportedVersion,
            format!("unsupported version {}", envelope.version),
        ));
    }
    let payload = serde_json::to_vec(&envelope.record)
        .map_err(|error| issue(path, RecoveryIssueKind::Corrupt, error.to_string()))?;
    let actual = blake3::hash(&payload).to_hex().to_string();
    if envelope.checksum != actual {
        return Err(issue(
            path,
            RecoveryIssueKind::ChecksumMismatch,
            "checksum does not match record",
        ));
    }
    Ok(envelope.record)
}

fn item_path(items: &Path, id: RecoveryId) -> PathBuf {
    items.join(format!("{id}.json"))
}

fn valid_session_path(sessions: &Path, session_id: &str) -> Option<PathBuf> {
    let parsed = Uuid::parse_str(session_id).ok()?;
    let path = sessions.join(format!("{parsed}.lock"));
    Some(path)
}

fn cleanup_stale_sessions(sessions: &Path) -> Result<(), RecoveryError> {
    let entries = fs::read_dir(sessions).map_err(|source| RecoveryError::Io {
        path: sessions.to_path_buf(),
        source,
    })?;
    let mut count = 0usize;
    let mut changed = false;
    for entry in entries {
        count += 1;
        if count > MAX_SCAN_ENTRIES {
            return Err(RecoveryError::Capacity {
                detail: "session marker directory limit exceeded".into(),
            });
        }
        let entry = entry.map_err(|source| RecoveryError::Io {
            path: sessions.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if path.extension().is_none_or(|extension| extension != "lock")
            || Uuid::parse_str(stem).is_err()
        {
            continue;
        }
        if let Ok(file) = open_regular_file(&path, false) {
            match file.try_lock() {
                Ok(()) => {
                    let _ = file.unlock();
                    remove_regular_file(&path).map_err(|source| RecoveryError::Io {
                        path: path.clone(),
                        source,
                    })?;
                    changed = true;
                },
                Err(std::fs::TryLockError::WouldBlock) => (),
                Err(std::fs::TryLockError::Error(_)) => (),
            }
        }
    }
    if changed {
        sync_directory(sessions)?;
    }
    Ok(())
}

fn mutation_lock_at(root: &Path) -> Result<MutationLock, RecoveryError> {
    let mut process_guard = None;
    for _ in 0..200 {
        match MUTATION_MUTEX.try_lock() {
            Ok(guard) => {
                process_guard = Some(guard);
                break;
            },
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(10))
            },
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                process_guard = Some(poisoned.into_inner());
                break;
            },
        }
    }
    let process_guard = process_guard.ok_or(RecoveryError::Busy)?;
    let path = root.join(MUTATION_LOCK);
    let file = match open_regular_file(&path, true) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match private_create_options()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    open_regular_file(&path, true).map_err(|source| RecoveryError::Io {
                        path: path.clone(),
                        source,
                    })?
                },
                Err(source) => return Err(RecoveryError::Io { path, source }),
            }
        },
        Err(source) => return Err(RecoveryError::Io { path, source }),
    };
    for _ in 0..200 {
        match file.try_lock() {
            Ok(()) => {
                return Ok(MutationLock {
                    _file: file,
                    _process_guard: process_guard,
                });
            },
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
            Err(std::fs::TryLockError::Error(source)) => {
                return Err(RecoveryError::Io { path, source });
            },
        }
    }
    Err(RecoveryError::Busy)
}

fn ensure_private_directory(path: &Path) -> Result<(), RecoveryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => (),
        Ok(_) => {
            return Err(RecoveryError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "recovery directory is not a real directory",
                ),
            });
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|source| RecoveryError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            let metadata = fs::symlink_metadata(path).map_err(|source| RecoveryError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            if !metadata.file_type().is_dir() {
                return Err(RecoveryError::Io {
                    path: path.to_path_buf(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "recovery directory is not a real directory",
                    ),
                });
            }
        },
        Err(source) => {
            return Err(RecoveryError::Io {
                path: path.to_path_buf(),
                source,
            });
        },
    }
    set_private_directory(path).map_err(|source| RecoveryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn open_regular_file(path: &Path, write: bool) -> std::io::Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let file = OpenOptions::new().read(true).write(write).open(path)?;
    let opened = file.metadata()?;
    if !same_file(&metadata, &opened) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "file changed while opening",
        ));
    }
    Ok(file)
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), RecoveryError> {
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        Uuid::new_v4()
    ));
    let mut file = private_create_options()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| RecoveryError::Io {
            path: temporary.clone(),
            source,
        })?;
    set_private_file(&file).map_err(|source| RecoveryError::Io {
        path: temporary.clone(),
        source,
    })?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(RecoveryError::Io {
            path: temporary,
            source,
        });
    }
    // Rename replaces only the directory entry, never follows a target symlink.
    if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.file_type().is_file()) {
        let _ = fs::remove_file(&temporary);
        return Err(RecoveryError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "refusing to replace non-file recovery entry",
            ),
        });
    }
    fs::rename(&temporary, path).map_err(|source| {
        let _ = fs::remove_file(&temporary);
        RecoveryError::Io {
            path: path.to_path_buf(),
            source,
        }
    })?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn remove_regular_file(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => fs::remove_file(path),
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "refusing to remove non-file recovery entry",
        )),
        Err(error) => Err(error),
    }
}

fn sync_directory(path: &Path) -> Result<(), RecoveryError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| RecoveryError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    #[cfg(not(unix))]
    // Directory handles are not portably syncable here; file data was synced,
    // and privacy depends on the parent directory's inherited ACLs.
    let _ = path;
    Ok(())
}

fn issue(path: &Path, kind: RecoveryIssueKind, detail: impl Into<String>) -> RecoveryIssue {
    RecoveryIssue {
        path: path.to_path_buf(),
        kind,
        detail: detail.into(),
    }
}

fn add_issue(listing: &mut RecoveryListing, new_issue: RecoveryIssue) {
    if listing.issues.len() < MAX_ISSUES {
        listing.issues.push(new_issue);
    } else {
        listing.complete = false;
        if new_issue.kind == RecoveryIssueKind::Oversized && MAX_ISSUES > 0 {
            listing.issues[MAX_ISSUES - 1] = new_issue;
        }
    }
}

fn has_storage_issue(listing: &RecoveryListing) -> bool {
    listing
        .issues
        .iter()
        .any(|issue| issue.kind != RecoveryIssueKind::Retention)
}

fn private_create_options() -> OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options.mode(0o600);
        options
    }
    #[cfg(not(unix))]
    OpenOptions::new()
}

fn try_lock_error(error: std::fs::TryLockError) -> std::io::Error {
    match error {
        std::fs::TryLockError::WouldBlock => {
            std::io::Error::new(std::io::ErrorKind::WouldBlock, "lock is already held")
        },
        std::fs::TryLockError::Error(error) => error,
    }
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino() && left.file_type().is_file()
}

#[cfg(not(unix))]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.file_type().is_file()
        && left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(file: &File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file(_file: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(temp: &tempfile::TempDir) -> RecoveryStore {
        RecoveryStore::open(
            temp.path().canonicalize().unwrap(),
            RecoveryRetention::default(),
        )
        .unwrap()
    }

    #[test]
    fn upsert_round_trips_text_metadata_and_selection() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        let id = RecoveryId::new();
        let mut record = RecoveryRecord::new(id, "unsaved".into(), "djot");
        record.original_path = Some(PathBuf::from("notes/one.djot"));
        record.original_address = Some("file:///notes/one.djot".into());
        record.selection = CaretSelection {
            anchor: 1,
            head: 4,
            anchor_upstream: true,
            head_upstream: false,
        };
        record.active_hint = true;
        store.upsert(record.clone()).unwrap();
        let loaded = store.list();
        assert!(loaded.issues.is_empty());
        assert_eq!(loaded.records.len(), 1);
        let actual = &loaded.records[0];
        assert_eq!(actual.id, id);
        assert_eq!(actual.source_text, "unsaved");
        assert_eq!(
            actual.selection,
            CaretSelection {
                anchor: 1,
                head: 4,
                anchor_upstream: true,
                head_upstream: false
            }
        );
        assert_eq!(actual.session_id, store.session_id());
        assert!(actual.created_at_ms > 0 && actual.updated_at_ms > 0);
    }

    #[test]
    fn corrupt_newer_and_oversized_records_are_reported_and_preserved() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        let bad = store.items.join("bad.json");
        fs::write(&bad, b"not json").unwrap();
        let oversized = store.items.join("large.json");
        fs::write(&oversized, vec![b'x'; HARD_MAX_ITEM_BYTES as usize + 1]).unwrap();
        let future = store.items.join("future.json");
        fs::write(&future, br#"{"version":99,"checksum":"","record":{}}"#).unwrap();
        let listing = store.list();
        assert_eq!(listing.issues.len(), 3);
        assert!(
            listing
                .issues
                .iter()
                .any(|issue| issue.kind == RecoveryIssueKind::Oversized)
        );
        assert!(bad.exists() && oversized.exists() && future.exists());
    }

    #[test]
    fn interrupted_temporary_files_are_not_listed_or_removed() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        let partial = store.items.join(".interrupted.tmp");
        fs::write(&partial, b"partial").unwrap();
        assert!(store.list().records.is_empty());
        assert!(store.list().issues.is_empty());
        assert!(partial.exists());
    }

    #[test]
    fn another_live_session_cannot_be_pruned_or_removed() {
        let temp = tempfile::tempdir().unwrap();
        let one = store(&temp);
        let two = store(&temp);
        let id = RecoveryId::new();
        one.upsert(RecoveryRecord::new(id, "draft".into(), "djot"))
            .unwrap();
        assert!(two.is_session_live(one.session_id()));
        assert!(matches!(
            two.remove(id),
            Err(RecoveryError::OwnedByLiveSession)
        ));
        assert!(one.list().records.iter().any(|record| record.id == id));
    }

    #[test]
    fn symlinked_record_and_directory_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = temp.path().join("outside.json");
            fs::write(&outside, b"safe").unwrap();
            let link = store.items.join("link.json");
            symlink(&outside, &link).unwrap();
            assert!(
                store
                    .list()
                    .issues
                    .iter()
                    .any(|issue| issue.kind == RecoveryIssueKind::UnsafePath)
            );
            assert_eq!(fs::read(&outside).unwrap(), b"safe");
            let unsafe_root = temp.path().join("linked");
            symlink(temp.path(), &unsafe_root).unwrap();
            assert!(RecoveryStore::open(unsafe_root, RecoveryRetention::default()).is_err());
        }
    }

    #[test]
    fn retention_prunes_only_dead_session_records() {
        let temp = tempfile::tempdir().unwrap();
        let active = store(&temp);
        let constrained = RecoveryStore::open(
            temp.path(),
            RecoveryRetention {
                max_records: 1,
                max_total_bytes: 10_000,
                max_item_bytes: 8_000,
                max_age_days: None,
            },
        )
        .unwrap();
        let first = RecoveryId::new();
        active
            .upsert(RecoveryRecord::new(first, "live".into(), "txt"))
            .unwrap();
        assert!(matches!(
            constrained.upsert(RecoveryRecord::new(
                RecoveryId::new(),
                "blocked".into(),
                "txt"
            )),
            Err(RecoveryError::Capacity { .. })
        ));
        drop(active);
        let second = RecoveryId::new();
        constrained
            .upsert(RecoveryRecord::new(second, "new".into(), "txt"))
            .unwrap();
        let ids: Vec<_> = constrained
            .list()
            .records
            .iter()
            .map(|record| record.id)
            .collect();
        assert_eq!(ids, vec![second]);
    }

    #[test]
    fn concurrent_claims_transfer_a_candidate_to_only_one_live_session() {
        let temp = tempfile::tempdir().unwrap();
        let first = store(&temp);
        let prior_id = RecoveryId::new();
        first
            .upsert(RecoveryRecord::new(prior_id, "candidate".into(), "djot"))
            .unwrap();
        let prior_session = first.session_id().to_owned();
        drop(first);

        let left = store(&temp);
        let right = store(&temp);
        assert!(!left.is_session_live(&prior_session));
        let id = prior_id;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let left_barrier = barrier.clone();
        let right_barrier = barrier.clone();
        let left_thread = std::thread::spawn(move || {
            left_barrier.wait();
            let result = left
                .claim(id)
                .map(|record| (record.is_some(), left.session_id().to_owned()));
            left_barrier.wait();
            result
        });
        let right_thread = std::thread::spawn(move || {
            right_barrier.wait();
            let result = right
                .claim(id)
                .map(|record| (record.is_some(), right.session_id().to_owned()));
            right_barrier.wait();
            result
        });
        barrier.wait();
        barrier.wait();
        let left_result = left_thread.join().unwrap();
        let right_result = right_thread.join().unwrap();
        assert_ne!(
            left_result.is_ok(),
            right_result.is_ok(),
            "left={left_result:?} right={right_result:?}"
        );
        assert_eq!(
            left_result.as_ref().is_ok_and(|(claimed, _)| *claimed),
            left_result.is_ok()
        );
        assert_eq!(
            right_result.as_ref().is_ok_and(|(claimed, _)| *claimed),
            right_result.is_ok()
        );
    }

    #[test]
    fn retention_expires_dead_records_at_open_and_preserves_live_records() {
        let temp = tempfile::tempdir().unwrap();
        let old_id = RecoveryId::new();
        let old_record = {
            let prior = store(&temp);
            prior
                .upsert(RecoveryRecord::new(old_id, "old draft".into(), "txt"))
                .unwrap();
            prior.list().records.into_iter().next().unwrap()
        };
        let mut expired = old_record;
        expired.updated_at_ms = now_ms().saturating_sub(8 * 86_400_000);
        atomic_replace(
            &item_path(&temp.path().canonicalize().unwrap().join(STORE_DIR), old_id),
            &encode_record(&expired).unwrap(),
        )
        .unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut store = RecoveryStore::open(
            &root,
            RecoveryRetention {
                max_records: 20,
                max_total_bytes: 100_000,
                max_item_bytes: 50_000,
                max_age_days: Some(30),
            },
        )
        .unwrap();
        assert!(
            store
                .list()
                .records
                .iter()
                .any(|record| record.id == old_id)
        );
        store
            .set_retention(RecoveryRetention {
                max_records: 20,
                max_total_bytes: 100_000,
                max_item_bytes: 50_000,
                max_age_days: Some(7),
            })
            .unwrap();
        assert!(store.list().records.is_empty());

        let live_id = RecoveryId::new();
        let live_record = RecoveryRecord::new(live_id, "active".into(), "txt");
        store.upsert(live_record.clone()).unwrap();
        let mut live_record = store
            .list()
            .records
            .into_iter()
            .find(|record| record.id == live_id)
            .unwrap();
        live_record.updated_at_ms = now_ms().saturating_sub(8 * 86_400_000);
        atomic_replace(
            &item_path(&store.items, live_id),
            &encode_record(&live_record).unwrap(),
        )
        .unwrap();
        store
            .set_retention(RecoveryRetention {
                max_records: 20,
                max_total_bytes: 100_000,
                max_item_bytes: 50_000,
                max_age_days: Some(7),
            })
            .unwrap();
        assert!(
            store
                .list()
                .records
                .iter()
                .any(|record| record.id == live_id)
        );
    }

    #[test]
    fn scan_stops_at_entry_budget_and_upsert_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        for index in 0..=MAX_SCAN_ENTRIES {
            fs::write(store.items.join(format!("extra-{index}.json")), b"{}").unwrap();
        }
        let listing = store.list();
        assert!(!listing.complete);
        assert!(
            listing
                .issues
                .iter()
                .any(|issue| issue.kind == RecoveryIssueKind::Oversized)
        );
        assert!(matches!(
            store.upsert(RecoveryRecord::new(RecoveryId::new(), "x".into(), "txt")),
            Err(RecoveryError::Capacity { .. })
        ));
    }

    #[test]
    fn child_process_crash_preserves_record_and_leaves_claimable_session() {
        let temp = tempfile::tempdir().unwrap();
        let id = RecoveryId::new();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("recovery::tests::crash_child_writer")
            .arg("--nocapture")
            .env("KNOT_RECOVERY_CRASH_TEST_ROOT", temp.path())
            .env("KNOT_RECOVERY_CRASH_TEST_ID", id.to_string())
            .status()
            .unwrap();
        assert_eq!(output.code(), Some(41));
        let store = store(&temp);
        let listed = store.list();
        assert_eq!(listed.records.len(), 1);
        assert_eq!(listed.records[0].source_text, "crash-safe text");
        assert!(!store.is_session_live(&listed.records[0].session_id));
        assert_eq!(
            store.claim(id).unwrap().unwrap().source_text,
            "crash-safe text"
        );
    }

    #[test]
    fn crash_child_writer() {
        let (Ok(root), Ok(id)) = (
            std::env::var("KNOT_RECOVERY_CRASH_TEST_ROOT"),
            std::env::var("KNOT_RECOVERY_CRASH_TEST_ID"),
        ) else {
            return;
        };
        let id = RecoveryId(Uuid::parse_str(&id).unwrap());
        let store = RecoveryStore::open(root, RecoveryRetention::default()).unwrap();
        store
            .upsert(RecoveryRecord::new(id, "crash-safe text".into(), "txt"))
            .unwrap();
        std::process::exit(41);
    }

    #[test]
    fn interrupted_sibling_write_does_not_replace_existing_record() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp);
        let id = RecoveryId::new();
        store
            .upsert(RecoveryRecord::new(id, "last complete".into(), "txt"))
            .unwrap();
        fs::write(
            store.items.join(format!(".{id}.json.abandoned.tmp")),
            b"partial replacement",
        )
        .unwrap();
        let listing = store.list();
        assert_eq!(listing.records.len(), 1);
        assert_eq!(listing.records[0].source_text, "last complete");
    }
}
