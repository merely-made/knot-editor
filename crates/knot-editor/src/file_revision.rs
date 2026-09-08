// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bounded preparation of immutable files-in-place capture revisions.

use std::fs::File;
use std::io::{self, Read};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{DirectorySource, VaultDocument};

/// File bytes captured from an indexed directory document.
///
/// This is a prepared value for a signed `CaptureFileRevision` event. Once
/// included in that event it represents the captured contents, rather than
/// the current contents of a vault document. Preparing one never changes the
/// directory, catalog, or editor state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub struct KnotFileRevisionV1 {
    /// Stable document identity from the caller's file catalog.
    pub document_id: String,
    /// Title from the indexed file container.
    pub title: String,
    /// Media type from the indexed file container.
    pub media_type: String,
    /// Captured source bytes.
    pub body: Vec<u8>,
}

impl KnotFileRevisionV1 {
    pub(crate) fn validate(&self) -> Result<(), String> {
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

    pub(crate) fn as_vault_document(&self) -> VaultDocument {
        VaultDocument {
            id: self.document_id.clone(),
            title: self.title.clone(),
            body: self.body.clone(),
            media_type: self.media_type.clone(),
        }
    }
}

impl DirectorySource {
    /// Capture an indexed catalog file into a bounded in-memory revision.
    ///
    /// The source must have been opened with a persistent file catalog. The
    /// indexed container supplies metadata; this method does not inspect an
    /// editor buffer or mutate either the catalog or the directory. The path
    /// is checked before opening, but a filesystem replacement between that
    /// check and the read remains the ordinary file TOCTOU limitation.
    pub fn capture_file_revision(
        &self,
        document_id: &str,
        max_bytes: usize,
    ) -> io::Result<KnotFileRevisionV1> {
        if !self.has_catalog() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file revision capture requires a catalog-backed directory source",
            ));
        }

        let document = self
            .document(document_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "document is not indexed"))?;
        let path = self.readable_document_path(document_id)?;
        let mut file = File::open(path)?;
        let mut body = Vec::new();
        let mut chunk = [0_u8; 8192];

        loop {
            if body.len() == max_bytes {
                let read = file.read(&mut chunk[..1])?;
                if read != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::FileTooLarge,
                        "file exceeds the capture byte limit",
                    ));
                }
                break;
            }

            let remaining = max_bytes - body.len();
            let read_limit = remaining.min(chunk.len());
            let read = file.read(&mut chunk[..read_limit])?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }

        let revision = KnotFileRevisionV1 {
            document_id: document.id.clone(),
            title: document.container.title.clone(),
            media_type: document
                .container
                .media_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into()),
            body,
        };
        revision.validate().map_err(io::Error::other)?;
        Ok(revision)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::IgnorePolicy;
    use knot_file_catalog::KnotFileCatalog;
    use tempfile::tempdir;

    use super::*;

    fn catalog_source(bytes: &[u8], name: &str) -> (tempfile::TempDir, DirectorySource, String) {
        let workspace = tempdir().unwrap();
        let root = workspace.path().join("files");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(name), bytes).unwrap();
        let catalog = KnotFileCatalog::open(&root, workspace.path().join("catalog.redb")).unwrap();
        let source =
            DirectorySource::with_catalog(&root, IgnorePolicy::default(), catalog).unwrap();
        let id = source.documents().next().unwrap().id.clone();
        (workspace, source, id)
    }

    #[test]
    fn capture_requires_catalog() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("note.txt"), b"bytes").unwrap();
        let source = DirectorySource::open(root.path()).unwrap();
        assert_eq!(
            source
                .capture_file_revision("missing", 10)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn capture_preserves_arbitrary_bytes_and_indexed_metadata() {
        let (_root, source, id) = catalog_source(&[0, 159, 255], "note.txt");
        let revision = source.capture_file_revision(&id, 3).unwrap();
        assert_eq!(revision.document_id, id);
        assert_eq!(revision.title, "note");
        assert_eq!(revision.media_type, "text/plain");
        assert_eq!(revision.body, [0, 159, 255]);
    }

    #[test]
    fn capture_denies_unknown_missing_and_oversize_files() {
        let (root, source, id) = catalog_source(b"abc", "note.txt");
        assert_eq!(
            source
                .capture_file_revision("knot:file:missing", 10)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        fs::remove_file(root.path().join("files").join("note.txt")).unwrap();
        assert_eq!(
            source.capture_file_revision(&id, 10).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );

        let (_root, source, id) = catalog_source(b"abc", "oversize.txt");
        assert_eq!(
            source.capture_file_revision(&id, 2).unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
    }

    #[test]
    fn capture_enforces_zero_bound_and_handles_usize_max_without_giant_allocation() {
        let (_root, source, id) = catalog_source(b"abc", "note.txt");
        assert_eq!(
            source.capture_file_revision(&id, 0).unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );

        let (_root, source, id) = catalog_source(b"", "empty.txt");
        let revision = source
            .capture_file_revision(&id, 0)
            .expect("empty file fits a zero-byte limit");
        assert!(revision.body.is_empty());

        let (_root, source, id) = catalog_source(b"small", "small.txt");
        let revision = source
            .capture_file_revision(&id, usize::MAX)
            .expect("small file fits the largest limit");
        assert_eq!(revision.body, b"small");
    }
}
