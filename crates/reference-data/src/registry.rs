// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::validation::{
    identifier, language, text, validate_manifest, validate_reference, version_identifier,
};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSource {
    manifest: SourceManifest,
    enabled: bool,
    /// Hash of complete imported JSON bytes. Never a source ID or supplied path.
    pack: Option<String>,
}
impl StoredSource {
    fn status(&self) -> SourceStatus {
        SourceStatus {
            manifest: self.manifest.clone(),
            installed: self.pack.is_some(),
            enabled: self.enabled,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    schema_version: u32,
    sources: Vec<StoredSource>,
}

/// A local single-writer registry. The caller supplies a private, trusted data
/// directory. One open handle holds an OS advisory writer lock until drop or
/// process exit. The persistent lock inode is never unlinked or stolen.
///
/// Source IDs never become filesystem paths; immutable pack files are named by
/// hashes. Mutations validate first, sync temporary files, atomically publish a
/// new registry, then update memory. A failed publication can leave an inert,
/// unreferenced pack file but cannot enable or replace an invalid source.
#[derive(Debug)]
pub struct SourceRegistry {
    root: PathBuf,
    limits: Limits,
    sources: BTreeMap<SourceKey, StoredSource>,
    packs: BTreeMap<SourceKey, ValidatedPack>,
    installed_bytes: usize,
    commit_uncertain: bool,
    _lock: WriterLock,
}

#[derive(Debug)]
struct WriterLock {
    _file: File,
}

impl SourceRegistry {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_limits(root, Limits::default())
    }
    pub fn open_with_limits(root: impl AsRef<Path>, limits: Limits) -> Result<Self> {
        let root = root.as_ref();
        if root.exists() {
            check_directory(root)?;
        } else {
            fs::create_dir_all(root)?;
        }
        let root = root.canonicalize()?;
        let packs_dir = root.join("packs");
        match fs::symlink_metadata(&packs_dir) {
            Ok(_) => check_directory(&packs_dir)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&packs_dir)?
            },
            Err(error) => return Err(error.into()),
        }
        let lock_path = root.join("writer.lock");
        match fs::symlink_metadata(&lock_path) {
            Ok(_) => {
                regular_file(&lock_path)?;
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        lock_file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => ReferenceError::Busy,
            std::fs::TryLockError::Error(error) => ReferenceError::Io(error),
        })?;
        let lock = WriterLock { _file: lock_file };
        let mut registry = Self {
            root,
            limits,
            sources: BTreeMap::new(),
            packs: BTreeMap::new(),
            installed_bytes: 0,
            commit_uncertain: false,
            _lock: lock,
        };
        let path = registry.root.join("registry.json");
        let bytes = match read_bounded(&path, registry.limits.max_registry_bytes) {
            Ok(bytes) => bytes,
            Err(ReferenceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(registry);
            },
            Err(error) => return Err(error),
        };
        let saved: RegistryFile = serde_json::from_slice(&bytes)?;
        if saved.schema_version != SCHEMA_VERSION {
            return Err(ReferenceError::Invalid(
                "unsupported registry schema".into(),
            ));
        }
        if saved.sources.len() > registry.limits.max_sources {
            return Err(ReferenceError::Limit("sources"));
        }
        for source in saved.sources {
            validate_manifest(&source.manifest, &registry.limits)?;
            let key = source.manifest.source_key();
            if registry.sources.contains_key(&key) {
                return Err(ReferenceError::Invalid("duplicate registry source".into()));
            }
            if let Some(file_name) = &source.pack {
                validate_pack_name(file_name)?;
                let path = registry.root.join("packs").join(file_name);
                // Check total on-disk bounds before allocating each next pack.
                let size = regular_file(&path)?.len();
                let size =
                    usize::try_from(size).map_err(|_| ReferenceError::Limit("installed bytes"))?;
                registry.installed_bytes = registry
                    .installed_bytes
                    .checked_add(size)
                    .ok_or(ReferenceError::Limit("installed bytes"))?;
                if registry.installed_bytes > registry.limits.max_installed_bytes {
                    return Err(ReferenceError::Limit("installed bytes"));
                }
                let bytes = read_bounded(&path, registry.limits.max_pack_bytes)?;
                if pack_name(&bytes) != *file_name {
                    return Err(ReferenceError::DigestMismatch);
                }
                let pack = ValidatedPack::from_json(
                    &bytes,
                    source.manifest.digest.as_deref(),
                    &registry.limits,
                )?;
                if pack.manifest() != &source.manifest {
                    return Err(ReferenceError::Conflict);
                }
                registry.packs.insert(key.clone(), pack);
            } else if source.enabled {
                return Err(ReferenceError::Invalid(
                    "uninstalled source cannot be enabled".into(),
                ));
            }
            registry.sources.insert(key, source);
        }
        Ok(registry)
    }

    pub fn list(&self) -> Vec<SourceStatus> {
        self.sources.values().map(StoredSource::status).collect()
    }

    /// Advertise inert metadata, not an installation. Optional unknown digest is
    /// useful for upstream catalogs; no fictitious checksum is needed.
    pub fn advertise(&mut self, manifest: SourceManifest) -> Result<SourceStatus> {
        self.require_writable()?;
        validate_manifest(&manifest, &self.limits)?;
        let key = manifest.source_key();
        if let Some(existing) = self.sources.get(&key) {
            if existing.manifest == manifest {
                return Ok(existing.status());
            }
            return Err(ReferenceError::Conflict);
        }
        let source = StoredSource {
            manifest,
            enabled: false,
            pack: None,
        };
        let mut next = self.sources.clone();
        next.insert(key, source.clone());
        self.persist(&next)?;
        self.sources = next;
        Ok(source.status())
    }

    /// Validate and durably install one pack; first installation is disabled.
    /// `expected_digest` is an optional trusted entries-payload checksum obtained
    /// independently of these bytes. Reimport cannot replace an installed version.
    pub fn import_json(
        &mut self,
        bytes: &[u8],
        expected_digest: Option<&str>,
    ) -> Result<SourceStatus> {
        self.require_writable()?;
        let pack = ValidatedPack::from_json(bytes, expected_digest, &self.limits)?;
        let key = pack.manifest().source_key();
        if let Some(existing) = self.sources.get(&key) {
            if existing.pack.is_some() {
                if existing.manifest == *pack.manifest() {
                    return Ok(existing.status());
                }
                return Err(ReferenceError::Conflict);
            }
            let mut advertised = existing.manifest.clone();
            if advertised.digest.is_none() {
                advertised.digest = pack.manifest().digest.clone();
            }
            if advertised != *pack.manifest() {
                return Err(ReferenceError::Conflict);
            }
        }
        let total = self
            .installed_bytes
            .checked_add(bytes.len())
            .ok_or(ReferenceError::Limit("installed bytes"))?;
        if total > self.limits.max_installed_bytes {
            return Err(ReferenceError::Limit("installed bytes"));
        }
        let file_name = pack_name(bytes);
        let source = StoredSource {
            manifest: pack.manifest().clone(),
            enabled: false,
            pack: Some(file_name.clone()),
        };
        let mut next = self.sources.clone();
        next.insert(key.clone(), source.clone());
        // Bound/serialize metadata before touching storage.
        let registry_bytes = self.registry_bytes(&next)?;
        store_immutable(
            &self.root.join("packs"),
            &file_name,
            bytes,
            self.limits.max_pack_bytes,
        )?;
        self.publish(&registry_bytes)?;
        self.sources = next;
        self.packs.insert(key, pack);
        self.installed_bytes = total;
        Ok(source.status())
    }

    pub fn set_enabled(&mut self, id: &str, version: &str, enabled: bool) -> Result<()> {
        self.require_writable()?;
        identifier(id)?;
        version_identifier(version)?;
        let key = SourceKey {
            source: id.into(),
            version: version.into(),
        };
        let mut next = self.sources.clone();
        let source = next.get_mut(&key).ok_or(ReferenceError::NotInstalled)?;
        if source.pack.is_none() {
            return Err(ReferenceError::NotInstalled);
        }
        source.enabled = enabled;
        self.persist(&next)?;
        self.sources = next;
        Ok(())
    }

    pub fn lookup(&self, query: &LookupQuery, limit: usize) -> Result<Vec<LexicalEntry>> {
        text(&query.lemma, self.limits.max_lemma_bytes, false)?;
        if let Some(value) = &query.language {
            language(value)?;
        }
        if limit > self.limits.max_lookup_results {
            return Err(ReferenceError::Limit("lookup results"));
        }
        if query.sources.len() > self.limits.max_query_sources {
            return Err(ReferenceError::Limit("query sources"));
        }
        let mut unique = BTreeSet::new();
        let mut result = Vec::new();
        for key in &query.sources {
            identifier(&key.source)?;
            version_identifier(&key.version)?;
            if !unique.insert(key) {
                return Err(ReferenceError::Invalid("duplicate query source".into()));
            }
            if !self.sources.get(key).is_some_and(|source| source.enabled) {
                continue;
            }
            if let Some(pack) = self.packs.get(key) {
                result.extend(pack.lookup(
                    &query.lemma,
                    query.language.as_deref(),
                    limit - result.len(),
                ));
            }
        }
        Ok(result)
    }

    pub fn entry(&self, id: &ReferenceId) -> Result<Option<LexicalEntry>> {
        validate_reference(id)?;
        let key = id.source_key();
        if !self.sources.get(&key).is_some_and(|source| source.enabled) {
            return Ok(None);
        }
        Ok(self
            .packs
            .get(&key)
            .and_then(|pack| pack.entry(id))
            .cloned())
    }

    fn registry_bytes(&self, next: &BTreeMap<SourceKey, StoredSource>) -> Result<Vec<u8>> {
        if next.len() > self.limits.max_sources {
            return Err(ReferenceError::Limit("sources"));
        }
        let bytes = serde_json::to_vec(&RegistryFile {
            schema_version: SCHEMA_VERSION,
            sources: next.values().cloned().collect(),
        })?;
        if bytes.len() > self.limits.max_registry_bytes {
            return Err(ReferenceError::Limit("registry bytes"));
        }
        Ok(bytes)
    }
    fn persist(&mut self, next: &BTreeMap<SourceKey, StoredSource>) -> Result<()> {
        self.publish(&self.registry_bytes(next)?)
    }
    fn require_writable(&self) -> Result<()> {
        if self.commit_uncertain {
            return Err(ReferenceError::Invalid(
                "close and reopen after an uncertain commit".into(),
            ));
        }
        Ok(())
    }
    fn publish(&mut self, bytes: &[u8]) -> Result<()> {
        let result = atomic_replace(&self.root, "registry.json", bytes);
        if matches!(&result, Err(ReferenceError::CommitUncertain(_))) {
            self.commit_uncertain = true;
        }
        result
    }
}

fn check_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(ReferenceError::Invalid(
            "registry path is not a real directory".into(),
        ));
    }
    Ok(())
}
fn regular_file(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(ReferenceError::Invalid(
            "registry data is not a regular file".into(),
        ));
    }
    Ok(meta)
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    if regular_file(path)?.len() > max as u64 {
        return Err(ReferenceError::Limit("file bytes"));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take((max as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(ReferenceError::Limit("file bytes"));
    }
    Ok(bytes)
}
fn pack_name(bytes: &[u8]) -> String {
    format!("{}.json", blake3::hash(bytes).to_hex())
}
fn validate_pack_name(value: &str) -> Result<()> {
    let digest = value
        .strip_suffix(".json")
        .ok_or_else(|| ReferenceError::Invalid("invalid pack filename".into()))?;
    crate::validation::validate_digest(digest)
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct TemporaryFile {
    path: PathBuf,
}
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
fn write_temporary(dir: &Path, bytes: &[u8]) -> Result<TemporaryFile> {
    check_directory(dir)?;
    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".pending-{}-{sequence}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let temp = TemporaryFile { path };
                file.write_all(bytes)?;
                file.sync_all()?;
                return Ok(temp);
            },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(ReferenceError::Busy)
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn store_immutable(dir: &Path, name: &str, bytes: &[u8], max: usize) -> Result<()> {
    validate_pack_name(name)?;
    let target = dir.join(name);
    let temp = write_temporary(dir, bytes)?;
    // Hard-link is an atomic create-new publication, unlike replacing rename.
    // Existing immutable data is verified and is never overwritten.
    match fs::hard_link(&temp.path, &target) {
        Ok(()) => sync_directory(dir),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_bounded(&target, max)? != bytes {
                return Err(ReferenceError::Conflict);
            }
            Ok(())
        },
        Err(error) => Err(error.into()),
    }
}
fn atomic_replace(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let target = dir.join(name);
    match fs::symlink_metadata(&target) {
        Ok(_) => {
            regular_file(&target)?;
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let temp = write_temporary(dir, bytes)?;
    fs::rename(&temp.path, target)?;
    sync_directory(dir).map_err(|error| match error {
        ReferenceError::Io(error) => ReferenceError::CommitUncertain(error),
        other => other,
    })
}
