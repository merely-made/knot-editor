// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Caller-selected durable identities for ordinary files under one local root.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use muniment::{Backend, RedbBackend};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

const CATALOG_KEY: &str = "knot/file-catalog/v1";
const CATALOG_VERSION: u16 = 1;
const DOCUMENT_ID_PREFIX: &str = "knot:document:";

/// Whether the recorded binding currently resolves to its original canonical file path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotFileCatalogAvailability {
    Available,
    Unavailable,
}

/// A bounded observation of a catalog-bound ordinary file.
///
/// This value is portable capture material. Preparing it never changes the
/// catalog, its source file, or another caller's index state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub struct KnotFileRevisionV1 {
    /// Stable document identity from the caller's file catalog.
    pub document_id: String,
    /// Deterministic title derived from the bound file's name.
    pub title: String,
    /// Deterministic media type derived from the bound file's extension.
    pub media_type: String,
    /// Captured source bytes.
    pub body: Vec<u8>,
}

impl KnotFileRevisionV1 {
    /// Validate fields accepted by the signed capture event format.
    pub fn validate(&self) -> Result<(), String> {
        if self.document_id.trim().is_empty() {
            return Err("file revision document id must not be empty".into());
        }
        if self.document_id != self.document_id.trim() {
            return Err("file revision document id must not have surrounding whitespace".into());
        }
        if self.document_id.contains('\0') {
            return Err("file revision document id must not contain NUL".into());
        }
        if self.title.contains('\0') {
            return Err("file revision title must not contain NUL".into());
        }
        if self.media_type.trim().is_empty() {
            return Err("file revision media type must not be empty".into());
        }
        if self.media_type != self.media_type.trim() {
            return Err("file revision media type must not have surrounding whitespace".into());
        }
        if self.media_type.contains('\0') {
            return Err("file revision media type must not contain NUL".into());
        }
        Ok(())
    }
}

/// One durable ordinary-file binding retained by a [`KnotFileCatalog`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotFileCatalogRecord {
    pub id: String,
    /// A normal-component path relative to the catalog's canonical root.
    pub relative_path: PathBuf,
    /// Calculated on each read; unavailable records remain part of catalog history.
    pub availability: KnotFileCatalogAvailability,
}

/// An owned, read-only view of the catalog's admitted bindings.
///
/// A host can move this value to a worker without moving the catalog's sole
/// mutable backend owner. Captures repeat the catalog's containment and file
/// checks at the moment each revision is read.
#[derive(Clone, Debug)]
pub struct KnotFileCatalogSnapshot {
    root: PathBuf,
    bindings: Vec<StoredBinding>,
}

impl KnotFileCatalogSnapshot {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn records(&self) -> Vec<KnotFileCatalogRecord> {
        self.bindings
            .iter()
            .map(|binding| record_for(&self.root, binding))
            .collect()
    }

    pub fn capture_file_revision(
        &self,
        id: &str,
        max_bytes: usize,
    ) -> io::Result<KnotFileRevisionV1> {
        let binding = self
            .bindings
            .iter()
            .find(|binding| binding.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file catalog id is unknown"))?;
        capture_binding(&self.root, binding, max_bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StoredBinding {
    id: String,
    relative_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StoredCatalog {
    version: u16,
    root: PathBuf,
    bindings: Vec<StoredBinding>,
}

/// A root-bound, locally configured catalog of caller-authorized file bindings.
///
/// It intentionally has no `Clone` implementation and exposes no backend handle:
/// one open Redb catalog is the mutating owner for its configured database path.
pub struct KnotFileCatalog {
    root: PathBuf,
    backend: RedbBackend,
    state: StoredCatalog,
}

fn record_for(root: &Path, binding: &StoredBinding) -> KnotFileCatalogRecord {
    KnotFileCatalogRecord {
        id: binding.id.clone(),
        relative_path: binding.relative_path.clone(),
        availability: availability(root, binding),
    }
}

fn availability(root: &Path, binding: &StoredBinding) -> KnotFileCatalogAvailability {
    let path = root.join(&binding.relative_path);
    let Ok(canonical) = fs::canonicalize(&path) else {
        return KnotFileCatalogAvailability::Unavailable;
    };
    if canonical != path || !canonical.starts_with(root) {
        return KnotFileCatalogAvailability::Unavailable;
    }
    match fs::metadata(canonical) {
        Ok(metadata) if metadata.is_file() => KnotFileCatalogAvailability::Available,
        _ => KnotFileCatalogAvailability::Unavailable,
    }
}

fn capture_binding(
    root: &Path,
    binding: &StoredBinding,
    max_bytes: usize,
) -> io::Result<KnotFileRevisionV1> {
    let path = root.join(&binding.relative_path);
    let canonical = fs::canonicalize(&path)?;
    if canonical != path || !canonical.starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "file catalog binding is unavailable",
        ));
    }
    if !fs::metadata(&canonical)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "file catalog binding is not a regular file",
        ));
    }
    let (title, media_type) = file_revision_metadata(&canonical);
    let revision = KnotFileRevisionV1 {
        document_id: binding.id.clone(),
        title,
        media_type,
        body: read_file_bounded(&canonical, max_bytes)?,
    };
    revision.validate().map_err(io::Error::other)?;
    Ok(revision)
}

/// Read one caller-admitted file without allocating from the requested maximum.
///
/// This helper does not check catalog membership, root containment, or file
/// type. The returned bytes have length at most `max_bytes`; an additional byte
/// is read solely to distinguish an exact limit from an oversized file.
pub fn read_file_bounded(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut body = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        if body.len() == max_bytes {
            if file.read(&mut chunk[..1])? != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "file exceeds the capture byte limit",
                ));
            }
            return Ok(body);
        }
        let remaining = max_bytes - body.len();
        let read_limit = remaining.min(chunk.len());
        let read = file.read(&mut chunk[..read_limit])?;
        if read == 0 {
            return Ok(body);
        }
        body.extend_from_slice(&chunk[..read]);
    }
}

/// Title and media type used by catalog captures and directory discovery.
pub fn file_revision_metadata(path: &Path) -> (String, String) {
    let title = path
        .file_stem()
        .or_else(|| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let media_type = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        .map(media_type_for_extension)
        .unwrap_or("application/octet-stream")
        .to_string();
    (title, media_type)
}

/// Media type used for a lowercase filename extension.
pub fn media_type_for_extension(extension: &str) -> &'static str {
    match extension {
        "knot" => "text/vnd.knot",
        "djot" => "text/djot",
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn persist_state(backend: &RedbBackend, state: &StoredCatalog) -> Result<(), String> {
    let bytes = serde_json::to_vec(state)
        .map_err(|error| format!("could not encode file catalog: {error}"))?;
    pollster::block_on(backend.put(CATALOG_KEY, &bytes)).map_err(|error| error.to_string())
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    let root = fs::canonicalize(root)
        .map_err(|error| format!("could not resolve catalog root {}: {error}", root.display()))?;
    if !root.is_dir() {
        return Err(format!(
            "catalog root is not a directory: {}",
            root.display()
        ));
    }
    Ok(root)
}

fn canonical_catalog_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("could not resolve current directory: {error}"))?
            .join(path)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| format!("catalog path has no parent: {}", absolute.display()))?;
    let parent = fs::canonicalize(parent).map_err(|error| {
        format!(
            "could not resolve catalog parent {}: {error}",
            parent.display()
        )
    })?;
    if !parent.is_dir() {
        return Err(format!(
            "catalog parent is not a directory: {}",
            parent.display()
        ));
    }
    let name = absolute
        .file_name()
        .ok_or_else(|| format!("catalog path has no file name: {}", absolute.display()))?;
    let candidate = parent.join(name);
    match fs::symlink_metadata(&candidate) {
        Ok(_) => fs::canonicalize(&candidate).map_err(|error| {
            format!(
                "could not resolve catalog path {}: {error}",
                candidate.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(candidate),
        Err(error) => Err(format!(
            "could not inspect catalog path {}: {error}",
            candidate.display()
        )),
    }
}

fn relative_from_canonical(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "canonical file path is outside catalog root: {}",
            path.display()
        )
    })?;
    validate_relative_path(relative)?;
    Ok(relative.to_path_buf())
}

fn canonicalize_missing_path(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => {
                let mut canonical = fs::canonicalize(&ancestor).map_err(|error| {
                    format!(
                        "could not resolve unavailable path ancestor {}: {error}",
                        ancestor.display()
                    )
                })?;
                for component in missing.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = ancestor.file_name().ok_or_else(|| {
                    format!(
                        "unavailable path has no existing ancestor: {}",
                        path.display()
                    )
                })?;
                if !matches!(
                    Path::new(component).components().next(),
                    Some(Component::Normal(_))
                ) {
                    return Err(format!(
                        "unavailable path has a non-normal missing component: {}",
                        path.display()
                    ));
                }
                missing.push(component.to_os_string());
                if !ancestor.pop() {
                    return Err(format!(
                        "unavailable path has no existing ancestor: {}",
                        path.display()
                    ));
                }
            },
            Err(error) => {
                return Err(format!(
                    "could not inspect unavailable path {}: {error}",
                    ancestor.display()
                ));
            },
        }
    }
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("catalog binding path is empty".into());
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!(
                "catalog binding path must contain only normal relative components: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn validate_state(state: &StoredCatalog, root: &Path) -> Result<(), String> {
    if state.version != CATALOG_VERSION {
        return Err(format!(
            "unsupported file catalog version {}",
            state.version
        ));
    }
    if state.root != root {
        return Err(format!(
            "file catalog is bound to {} rather than {}",
            state.root.display(),
            root.display()
        ));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for binding in &state.bindings {
        validate_document_id(&binding.id)?;
        validate_relative_path(&binding.relative_path)?;
        if !ids.insert(binding.id.clone()) {
            return Err(format!("file catalog repeats document id {}", binding.id));
        }
        if !paths.insert(binding.relative_path.clone()) {
            return Err(format!(
                "file catalog repeats path {}",
                binding.relative_path.display()
            ));
        }
    }
    Ok(())
}

fn validate_document_id(id: &str) -> Result<(), String> {
    let raw = id
        .strip_prefix(DOCUMENT_ID_PREFIX)
        .ok_or_else(|| format!("file catalog id has wrong prefix: {id}"))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|error| format!("invalid catalog UUID {id}: {error}"))?;
    if uuid.get_version_num() != 4 {
        return Err(format!("catalog UUID is not v4: {id}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

mod operations;
