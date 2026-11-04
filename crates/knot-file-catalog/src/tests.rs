// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
fn owned_snapshot_captures_on_a_worker_without_moving_the_catalog_owner() {
    let (_temp, root, catalog_path) = setup();
    let path = root.join("essay.djot");
    fs::write(&path, "worker bytes").unwrap();
    let mut catalog = KnotFileCatalog::open(&root, &catalog_path).unwrap();
    let id = catalog.bind(&path).unwrap();
    let snapshot = catalog.snapshot();
    let worker_id = id.clone();
    let revision =
        std::thread::spawn(move || snapshot.capture_file_revision(&worker_id, 32).unwrap())
            .join()
            .unwrap();
    assert_eq!(revision.body, b"worker bytes");
    assert_eq!(catalog.record(&id).unwrap().id, id);
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
