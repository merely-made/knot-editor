// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! G1 receipt for explicit, durable files-in-place document bindings.

use std::fs;
use std::path::Path;

use chartulary::{AcceptAll, FacetId};
use knot_editor::{DirectorySource, FILE_DOCUMENT_FACET, IgnorePolicy, KnotFileCatalog};
use serde_json::json;
use tempfile::tempdir;

fn document_id_at(source: &DirectorySource, path: &Path) -> String {
    let path = fs::canonicalize(path).unwrap();
    source
        .documents()
        .find(|document| document.path == path)
        .map(|document| document.id.clone())
        .unwrap_or_else(|| panic!("indexed document at {}", path.display()))
}

fn catalog_for(root: &Path, catalog_root: &Path) -> KnotFileCatalog {
    KnotFileCatalog::open(root, catalog_root.join("file-catalog.redb")).unwrap()
}

#[test]
fn opt_in_catalog_preserves_replacements_and_restart_but_copies_get_new_ids() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("documents");
    let catalog_root = temp.path().join("catalog");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&catalog_root).unwrap();
    let path = root.join("essay.djot");
    fs::write(&path, "original").unwrap();

    let catalog = catalog_for(&root, &catalog_root);
    let mut source = DirectorySource::with_catalog(&root, IgnorePolicy::none(), catalog).unwrap();
    assert!(source.has_catalog());
    let original_id = document_id_at(&source, &path);
    assert!(original_id.starts_with("knot:document:"));
    source
        .facets_mut()
        .set(
            original_id.clone(),
            FacetId::new("knot.test-pin"),
            json!({"pinned": true}),
            &AcceptAll,
        )
        .unwrap();

    let mut session = knot_editor::KnotDocumentSession::open(&path).unwrap();
    session.input_mut().unwrap().insert_str(" saved");
    session
        .apply(knot_editor::KnotDocumentIntentV1::Save)
        .unwrap();
    assert!(source.refresh().unwrap());
    assert_eq!(document_id_at(&source, &path), original_id);
    drop(session);

    let replacement = root.join("essay.tmp");
    fs::write(&replacement, "replacement").unwrap();
    fs::remove_file(&path).unwrap();
    fs::rename(&replacement, &path).unwrap();
    assert!(source.refresh().unwrap());
    assert_eq!(document_id_at(&source, &path), original_id);
    assert_eq!(
        source
            .facets()
            .get(&original_id, &FacetId::new("knot.test-pin")),
        Some(&json!({"pinned": true}))
    );

    drop(source);
    let catalog = catalog_for(&root, &catalog_root);
    let mut restarted =
        DirectorySource::with_catalog(&root, IgnorePolicy::none(), catalog).unwrap();
    assert_eq!(document_id_at(&restarted, &path), original_id);
    assert!(
        restarted
            .facets()
            .get(&original_id, &FacetId::new(FILE_DOCUMENT_FACET))
            .is_some()
    );

    let copied = root.join("essay-copy.md");
    fs::copy(&path, &copied).unwrap();
    assert!(restarted.refresh().unwrap());
    let copied_id = document_id_at(&restarted, &copied);
    assert!(copied_id.starts_with("knot:document:"));
    assert_ne!(copied_id, original_id);

    let hardlink = root.join("essay-hardlink.md");
    fs::hard_link(&path, &hardlink).unwrap();
    assert!(restarted.refresh().unwrap());
    let hardlink_id = document_id_at(&restarted, &hardlink);
    assert_ne!(hardlink_id, original_id);
    assert_ne!(hardlink_id, copied_id);
    let revision = restarted.revision();
    assert!(!restarted.refresh().unwrap());
    assert_eq!(restarted.revision(), revision);
}

#[test]
fn catalog_rebind_is_explicit_and_cross_root_catalogs_are_rejected() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("documents");
    let catalog_root = temp.path().join("catalog");
    let other_root = temp.path().join("other");
    let other_catalog_root = temp.path().join("other-catalog");
    for directory in [&root, &catalog_root, &other_root, &other_catalog_root] {
        fs::create_dir(directory).unwrap();
    }
    let moved_from = root.join("moved.md");
    let moved_to = root.join("renamed.md");
    fs::write(&moved_from, "move me").unwrap();

    let catalog = catalog_for(&root, &catalog_root);
    let mut source = DirectorySource::with_catalog(&root, IgnorePolicy::none(), catalog).unwrap();
    let id = document_id_at(&source, &moved_from);
    fs::rename(&moved_from, &moved_to).unwrap();
    source
        .rebind_catalog_document(&id, &moved_to)
        .expect("explicit move rebind");
    assert_eq!(document_id_at(&source, &moved_to), id);
    assert!(
        source
            .documents()
            .all(|document| document.path != moved_from)
    );

    let foreign_catalog = catalog_for(&other_root, &other_catalog_root);
    let error = match DirectorySource::with_catalog(&root, IgnorePolicy::none(), foreign_catalog) {
        Ok(_) => panic!("a catalog from another authority root must be refused"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[cfg(any(unix, windows))]
#[test]
#[cfg_attr(windows, ignore = "requires Windows symbolic-link creation privilege")]
fn catalog_symlink_aliases_are_canonicalized_and_do_not_churn_refresh() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("documents");
    let catalog_root = temp.path().join("catalog");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&catalog_root).unwrap();
    let target = root.join("target.md");
    let alias = root.join("alias.md");
    fs::write(&target, "one source").unwrap();
    create_file_symlink(&target, &alias).unwrap_or_else(|error| {
        panic!("symlink alias receipt requires filesystem support: {error}")
    });

    let mut catalog = catalog_for(&root, &catalog_root);
    let target_id = catalog.bind(&target).unwrap();
    assert_eq!(catalog.bind(&alias).unwrap(), target_id);
    assert_eq!(catalog.lookup("alias.md").unwrap().unwrap().id, target_id);
    let mut source = DirectorySource::with_catalog(&root, IgnorePolicy::none(), catalog).unwrap();
    assert_eq!(source.documents().count(), 1);
    assert_eq!(
        document_id_at(&source, &target),
        document_id_at(&source, &alias)
    );
    let revision = source.revision();
    assert!(!source.refresh().unwrap());
    assert_eq!(source.revision(), revision);
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, alias: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, alias)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, alias: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, alias)
}

#[test]
fn default_source_keeps_discovery_identity_behavior() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("plain.md");
    fs::write(&path, "plain").unwrap();
    let source = DirectorySource::open(temp.path()).unwrap();
    assert!(
        source
            .documents()
            .next()
            .unwrap()
            .id
            .starts_with("knot:file:")
    );
    assert!(!source.has_catalog());
}

#[test]
fn default_source_hardlink_refresh_keeps_legacy_no_change_result() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("original.md");
    let alias = temp.path().join("alias.md");
    fs::write(&path, "shared bytes").unwrap();
    fs::hard_link(&path, &alias).unwrap();

    let mut source = DirectorySource::open(temp.path()).unwrap();
    let revision = source.revision();
    assert!(!source.refresh().unwrap());
    assert_eq!(source.revision(), revision);
}
