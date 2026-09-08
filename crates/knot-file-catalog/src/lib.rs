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

impl KnotFileCatalog {
    /// Open a catalog explicitly configured outside `root`.
    ///
    /// The catalog persists the canonical root and refuses a later open against
    /// another root. Relocating a catalog is an explicit future operation.
    pub fn open(root: impl AsRef<Path>, catalog_path: impl AsRef<Path>) -> Result<Self, String> {
        let root = canonical_root(root.as_ref())?;
        let catalog_path = canonical_catalog_path(catalog_path.as_ref())?;
        if catalog_path.starts_with(&root) {
            return Err("file catalog path must be outside its configured root".into());
        }
        let backend = RedbBackend::open(&catalog_path).map_err(|error| error.to_string())?;
        let state = match pollster::block_on(backend.get(CATALOG_KEY))
            .map_err(|error| error.to_string())?
        {
            Some(bytes) => {
                let state: StoredCatalog = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("invalid file catalog: {error}"))?;
                validate_state(&state, &root)?;
                state
            },
            None => {
                let state = StoredCatalog {
                    version: CATALOG_VERSION,
                    root: root.clone(),
                    bindings: Vec::new(),
                };
                persist_state(&backend, &state)?;
                state
            },
        };
        Ok(Self {
            root,
            backend,
            state,
        })
    }

    /// Canonical root that bounds every catalog binding.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Bind one existing regular file, returning an exact-path idempotent ID.
    ///
    /// Canonical symlink aliases resolve to the same binding. A distinct hardlink
    /// path intentionally receives a new caller-owned document ID.
    pub fn bind(&mut self, path: impl AsRef<Path>) -> Result<String, String> {
        let relative_path = self.canonical_file_relative(path.as_ref())?;
        if let Some(binding) = self
            .state
            .bindings
            .iter()
            .find(|binding| binding.relative_path == relative_path)
        {
            return Ok(binding.id.clone());
        }
        let mut next = self.state.clone();
        let id = format!("{DOCUMENT_ID_PREFIX}{}", Uuid::new_v4());
        next.bindings.push(StoredBinding {
            id: id.clone(),
            relative_path,
        });
        next.bindings.sort_by(|left, right| left.id.cmp(&right.id));
        self.persist(next)?;
        Ok(id)
    }

    /// Move one binding after an external file move has made its old path absent.
    ///
    /// This changes catalog metadata only. It never moves, deletes, or writes a
    /// source file, and it refuses an already-bound target.
    pub fn rebind(&mut self, id: &str, new_path: impl AsRef<Path>) -> Result<(), String> {
        let current = self
            .state
            .bindings
            .iter()
            .find(|binding| binding.id == id)
            .ok_or_else(|| format!("file catalog id is unknown: {id}"))?;
        let old_path = self.root.join(&current.relative_path);
        match fs::symlink_metadata(&old_path) {
            Ok(_) => {
                return Err(format!(
                    "cannot rebind {id}: old binding still exists at {}",
                    old_path.display()
                ));
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => {
                return Err(format!(
                    "could not inspect old binding {}: {error}",
                    old_path.display()
                ));
            },
        }
        let relative_path = self.canonical_file_relative(new_path.as_ref())?;
        if self
            .state
            .bindings
            .iter()
            .any(|binding| binding.relative_path == relative_path)
        {
            return Err(format!(
                "catalog target is already bound: {}",
                self.root.join(&relative_path).display()
            ));
        }
        let mut next = self.state.clone();
        next.bindings
            .iter_mut()
            .find(|binding| binding.id == id)
            .expect("validated catalog binding remains present")
            .relative_path = relative_path;
        self.persist(next)
    }

    /// Find a binding by path, including an unavailable historical path.
    pub fn lookup(&self, path: impl AsRef<Path>) -> Result<Option<KnotFileCatalogRecord>, String> {
        let relative_path = self.lookup_relative(path.as_ref())?;
        Ok(self
            .state
            .bindings
            .iter()
            .find(|binding| binding.relative_path == relative_path)
            .map(|binding| self.record_for(binding)))
    }

    /// Find one binding by its opaque durable ID, including unavailable history.
    pub fn record(&self, id: &str) -> Option<KnotFileCatalogRecord> {
        self.state
            .bindings
            .iter()
            .find(|binding| binding.id == id)
            .map(|binding| self.record_for(binding))
    }

    /// All catalog bindings, including unavailable historical paths, ordered by ID.
    pub fn records(&self) -> Vec<KnotFileCatalogRecord> {
        self.state
            .bindings
            .iter()
            .map(|binding| self.record_for(binding))
            .collect()
    }

    /// Capture one available, exact canonical binding into a bounded revision.
    ///
    /// The binding must still resolve directly to its stored canonical path
    /// below this catalog's root. A redirected symlink, missing path, or
    /// non-file is rejected; ordinary replacement at the same bound path is
    /// allowed. The reader consumes at most `max_bytes + 1` bytes and does
    /// not preallocate from that caller-controlled limit.
    /// These checks precede the read; they do not exclude concurrent filesystem
    /// changes between checking the path and opening or reading the file.
    pub fn capture_file_revision(
        &self,
        id: &str,
        max_bytes: usize,
    ) -> io::Result<KnotFileRevisionV1> {
        let binding = self
            .state
            .bindings
            .iter()
            .find(|binding| binding.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file catalog id is unknown"))?;
        let path = self.root.join(&binding.relative_path);
        let canonical = fs::canonicalize(&path)?;
        if canonical != path || !canonical.starts_with(&self.root) {
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

    fn persist(&mut self, next: StoredCatalog) -> Result<(), String> {
        persist_state(&self.backend, &next)?;
        self.state = next;
        Ok(())
    }

    fn canonical_file_relative(&self, path: &Path) -> Result<PathBuf, String> {
        let requested = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let canonical = fs::canonicalize(&requested)
            .map_err(|error| format!("could not resolve {}: {error}", requested.display()))?;
        if !canonical.starts_with(&self.root) {
            return Err(format!(
                "file binding resolves outside catalog root: {}",
                canonical.display()
            ));
        }
        if !fs::metadata(&canonical)
            .map_err(|error| format!("could not inspect {}: {error}", canonical.display()))?
            .is_file()
        {
            return Err(format!(
                "catalog target is not a regular file: {}",
                canonical.display()
            ));
        }
        relative_from_canonical(&self.root, &canonical)
    }

    fn lookup_relative(&self, path: &Path) -> Result<PathBuf, String> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        if absolute.exists() {
            return self.canonical_file_relative(&absolute);
        }
        let canonical = canonicalize_missing_path(&absolute)?;
        let relative = canonical.strip_prefix(&self.root).map_err(|_| {
            format!(
                "unavailable path is outside catalog root: {}",
                canonical.display()
            )
        })?;
        validate_relative_path(relative)?;
        Ok(relative.to_path_buf())
    }

    fn record_for(&self, binding: &StoredBinding) -> KnotFileCatalogRecord {
        KnotFileCatalogRecord {
            id: binding.id.clone(),
            relative_path: binding.relative_path.clone(),
            availability: self.availability(binding),
        }
    }

    fn availability(&self, binding: &StoredBinding) -> KnotFileCatalogAvailability {
        let path = self.root.join(&binding.relative_path);
        let Ok(canonical) = fs::canonicalize(&path) else {
            return KnotFileCatalogAvailability::Unavailable;
        };
        if canonical != path || !canonical.starts_with(&self.root) {
            return KnotFileCatalogAvailability::Unavailable;
        }
        match fs::metadata(canonical) {
            Ok(metadata) if metadata.is_file() => KnotFileCatalogAvailability::Available,
            _ => KnotFileCatalogAvailability::Unavailable,
        }
    }
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
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let metadata = temp.path().join("metadata");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&metadata).unwrap();
        (temp, root, metadata.join("catalog.redb"))
    }

    #[test]
    fn bind_is_path_idempotent_persistent_and_never_writes_source_bytes() {
        let (_temp, root, catalog_path) = setup();
        let path = root.join("essay.djot");
        fs::write(&path, "# Essay\n").unwrap();
        let original = fs::read(&path).unwrap();
        let id = {
            let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
            let first = catalog.bind("essay.djot").unwrap();
            assert_eq!(catalog.bind(&path).unwrap(), first);
            assert_eq!(catalog.lookup("./essay.djot").unwrap().unwrap().id, first);
            assert!(first.starts_with(DOCUMENT_ID_PREFIX));
            first
        };
        let catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        assert_eq!(catalog.lookup(&path).unwrap().unwrap().id, id);
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn capture_uses_only_a_registered_available_binding_without_mutating_catalog() {
        let (_temp, root, catalog_path) = setup();
        let path = root.join("note.txt");
        fs::write(&path, [0, 159, 255]).unwrap();
        let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        let id = catalog.bind(&path).unwrap();
        let records = catalog.records();

        let revision = catalog.capture_file_revision(&id, 3).unwrap();
        assert_eq!(revision.document_id, id);
        assert_eq!(revision.title, "note");
        assert_eq!(revision.media_type, "text/plain");
        assert_eq!(revision.body, [0, 159, 255]);
        assert_eq!(catalog.records(), records);
        assert_eq!(fs::read(&path).unwrap(), [0, 159, 255]);
        assert_eq!(
            catalog
                .capture_file_revision("knot:document:missing", 3)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn capture_enforces_byte_bounds_without_a_limit_sized_allocation() {
        let (_temp, root, catalog_path) = setup();
        let path = root.join("note.txt");
        fs::write(&path, b"abc").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        let id = catalog.bind(&path).unwrap();
        assert_eq!(
            catalog.capture_file_revision(&id, 2).unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(
            catalog.capture_file_revision(&id, 0).unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
        fs::write(&path, []).unwrap();
        assert!(
            catalog
                .capture_file_revision(&id, 0)
                .unwrap()
                .body
                .is_empty()
        );
        fs::write(&path, b"small").unwrap();
        assert_eq!(
            catalog.capture_file_revision(&id, usize::MAX).unwrap().body,
            b"small"
        );
    }

    #[cfg(unix)]
    #[test]
    fn capture_accepts_canonical_aliases_but_refuses_a_replaced_symlink_binding() {
        use std::os::unix::fs::symlink;

        let (_temp, root, catalog_path) = setup();
        let path = root.join("note.txt");
        let alias = root.join("alias.txt");
        let outside = root.parent().unwrap().join("outside.txt");
        fs::write(&path, b"bound").unwrap();
        symlink("note.txt", &alias).unwrap();
        let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        let id = catalog.bind(&alias).unwrap();
        assert_eq!(catalog.bind(&path).unwrap(), id);
        assert_eq!(
            catalog.capture_file_revision(&id, 16).unwrap().body,
            b"bound"
        );

        fs::remove_file(&path).unwrap();
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, &path).unwrap();
        assert_eq!(
            catalog.capture_file_revision(&id, 16).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn unavailable_history_can_rebind_only_after_the_old_path_is_absent() {
        let (_temp, root, catalog_path) = setup();
        let old = root.join("old.djot");
        let new = root.join("new.djot");
        fs::write(&old, "old").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        let id = catalog.bind(&old).unwrap();
        fs::write(&new, "new").unwrap();
        assert!(catalog.rebind(&id, &new).is_err());
        assert_eq!(
            catalog.record(&id).unwrap().relative_path,
            PathBuf::from("old.djot")
        );
        fs::remove_file(&old).unwrap();
        assert_eq!(
            catalog.lookup(&old).unwrap().unwrap().availability,
            KnotFileCatalogAvailability::Unavailable
        );
        catalog.rebind(&id, &new).unwrap();
        assert_eq!(
            catalog.record(&id).unwrap().relative_path,
            PathBuf::from("new.djot")
        );
    }

    #[test]
    fn unavailable_lookup_normalizes_an_absolute_path_through_a_missing_parent() {
        let (_temp, root, catalog_path) = setup();
        let parent = root.join("moved");
        let path = parent.join("essay.djot");
        fs::create_dir(&parent).unwrap();
        fs::write(&path, "essay").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        let id = catalog.bind(&path).unwrap();
        fs::remove_dir_all(&parent).unwrap();

        let record = catalog.lookup(&path).unwrap().unwrap();
        assert_eq!(record.id, id);
        assert_eq!(
            record.availability,
            KnotFileCatalogAvailability::Unavailable
        );
    }

    #[test]
    fn copies_get_new_ids_and_catalog_path_and_root_are_bound_explicitly() {
        let (_temp, root, catalog_path) = setup();
        let first = root.join("first.djot");
        let copy = root.join("copy.djot");
        fs::write(&first, "same bytes").unwrap();
        fs::write(&copy, "same bytes").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        let first_id = catalog.bind(&first).unwrap();
        assert_ne!(catalog.bind(&copy).unwrap(), first_id);
        assert!(KnotFileCatalog::open(&root, root.join("catalog.redb")).is_err());
        let other_root = root.parent().unwrap().join("other");
        fs::create_dir(&other_root).unwrap();
        assert!(KnotFileCatalog::open(&other_root, &catalog_path).is_err());
    }

    #[test]
    fn malformed_persisted_bindings_are_refused() {
        let (_temp, root, catalog_path) = setup();
        let backend = RedbBackend::open(&catalog_path).unwrap();
        let invalid = StoredCatalog {
            version: CATALOG_VERSION,
            root: fs::canonicalize(&root).unwrap(),
            bindings: vec![StoredBinding {
                id: "knot:document:not-a-uuid".into(),
                relative_path: PathBuf::from("../escape.djot"),
            }],
        };
        persist_state(&backend, &invalid).unwrap();
        drop(backend);
        assert!(KnotFileCatalog::open(&root, &catalog_path).is_err());
    }

    #[test]
    fn persisted_duplicate_ids_and_paths_and_second_owner_are_refused() {
        let (_temp, root, catalog_path) = setup();
        let catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
        assert!(KnotFileCatalog::open(&root, &catalog_path).is_err());
        drop(catalog);

        let backend = RedbBackend::open(&catalog_path).unwrap();
        let id = format!("{DOCUMENT_ID_PREFIX}{}", Uuid::new_v4());
        let duplicate = StoredCatalog {
            version: CATALOG_VERSION,
            root: fs::canonicalize(&root).unwrap(),
            bindings: vec![
                StoredBinding {
                    id: id.clone(),
                    relative_path: PathBuf::from("one.djot"),
                },
                StoredBinding {
                    id,
                    relative_path: PathBuf::from("two.djot"),
                },
            ],
        };
        persist_state(&backend, &duplicate).unwrap();
        drop(backend);
        assert!(KnotFileCatalog::open(&root, &catalog_path).is_err());
    }
}
