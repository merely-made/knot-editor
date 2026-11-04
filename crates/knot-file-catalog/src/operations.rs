// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

    /// Copy the catalog authority needed for bounded background reads.
    pub fn snapshot(&self) -> KnotFileCatalogSnapshot {
        KnotFileCatalogSnapshot {
            root: self.root.clone(),
            bindings: self.state.bindings.clone(),
        }
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
        capture_binding(&self.root, binding, max_bytes)
    }

    pub(super) fn persist(&mut self, next: StoredCatalog) -> Result<(), String> {
        persist_state(&self.backend, &next)?;
        self.state = next;
        Ok(())
    }

    pub(super) fn canonical_file_relative(&self, path: &Path) -> Result<PathBuf, String> {
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

    pub(super) fn lookup_relative(&self, path: &Path) -> Result<PathBuf, String> {
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

    pub(super) fn record_for(&self, binding: &StoredBinding) -> KnotFileCatalogRecord {
        record_for(&self.root, binding)
    }
}
