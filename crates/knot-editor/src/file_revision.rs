// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bounded preparation of immutable files-in-place capture revisions.

use std::io;

use knot_file_catalog::read_file_bounded;

use crate::{DirectorySource, KnotFileRevisionV1};

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
        let body = read_file_bounded(&path, max_bytes)?;
        let revision = knot_file_catalog::KnotFileRevisionV1 {
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
