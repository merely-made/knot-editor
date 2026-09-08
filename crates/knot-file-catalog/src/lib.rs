// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Caller-selected durable identities for ordinary files under one local root.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use muniment::{Backend, RedbBackend};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const CATALOG_KEY: &str = "knot/file-catalog/v1";
const CATALOG_VERSION: u16 = 1;
const DOCUMENT_ID_PREFIX: &str = "knot:document:";

/// Whether the recorded binding currently resolves to its original canonical file path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotFileCatalogAvailability {
    Available,
    Unavailable,
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
