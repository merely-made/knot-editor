// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Filesystem and copy guarantees, independent of the store's cryptographic
//! checkpoint/history qualification. Fixtures called sealed below are opaque
//! prepared bytes; these tests do not establish a cipher's confidentiality.

#[allow(dead_code)]
#[path = "../src/checkpoint_migration.rs"]
mod checkpoint_migration;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use checkpoint_migration::{
    ClosedStore, MigrationDecision, MigrationEntries, MigrationError, MigrationFaults,
    MigrationScope, MigrationStep, open_or_rewrite, open_or_rewrite_with_faults,
};
use muniment::{Backend, RedbBackend};

const CHECKPOINT: &str = "knot-sync/checkpoint/space-a";
const KEYRING: &str = "knot-sync/commons-keyring/space-a";
const MARKER: &str = "knot-sync/sealed-storage/space-a";
const SEALED: &[u8] = b"opaque-prepared-checkpoint-ciphertext-v1";
const SEALED_KEYS: &[u8] = b"opaque-local-custody-keyring-ciphertext-v1";
const SEALED_MARKER: &[u8] = b"opaque-authenticated-format-marker-v1";
const OPAQUE: &[u8] = b"\0\x01opaque-signed-header-and-encrypted-operation\xff";

fn scope() -> MigrationScope {
    MigrationScope {
        managed_prefixes: [
            "knot-sync/checkpoint/",
            "knot-sync/commons-keyring/",
            "knot-sync/sealed-storage/",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        managed_keys: [CHECKPOINT, KEYRING, MARKER]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    }
}

fn rewrite() -> MigrationDecision {
    MigrationDecision::Rewrite {
        replacements: BTreeMap::from([
            (CHECKPOINT.to_owned(), Some(SEALED.to_vec())),
            (KEYRING.to_owned(), Some(SEALED_KEYS.to_vec())),
            (MARKER.to_owned(), Some(SEALED_MARKER.to_vec())),
        ]),
    }
}

fn failure(result: Result<ClosedStore, MigrationError>) -> MigrationError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("migration unexpectedly succeeded"),
    }
}

fn create_source(path: &Path) -> MigrationEntries {
    let entries = BTreeMap::from([
        (
            CHECKPOINT.to_owned(),
            b"{\"snapshot\":\"legacy plaintext fixture\"}".to_vec(),
        ),
        (KEYRING.to_owned(), b"legacy group secret fixture".to_vec()),
        ("op/signed-fixture".to_owned(), OPAQUE.to_vec()),
        ("log/retained-frontier".to_owned(), vec![0, 9, 8, 7, 0xff]),
    ]);
    pollster::block_on(async {
        let backend = RedbBackend::open(path).unwrap();
        for (key, value) in &entries {
            backend.put(key, value).await.unwrap();
        }
    });
    entries
}

fn read_source(path: &Path) -> MigrationEntries {
    pollster::block_on(async {
        let backend = RedbBackend::open(path).unwrap();
        let mut entries = BTreeMap::new();
        for key in backend.list("").await.unwrap() {
            let value = backend.get(&key).await.unwrap().unwrap();
            entries.insert(key, value);
        }
        entries
    })
}

fn stage_files(parent: &Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".knot-checkpoint-")
        })
        .collect()
}

#[cfg(unix)]
#[test]
fn fresh_file_preserves_opaque_keys_and_excludes_old_checkpoint_pages() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    let original = create_source(&path);
    let old_only = "checkpoint-only-in-obsolete-pages-6a09af9e".repeat(256);
    pollster::block_on(async {
        let backend = RedbBackend::open(&path).unwrap();
        backend.put(CHECKPOINT, old_only.as_bytes()).await.unwrap();
        backend
            .put(CHECKPOINT, &original[CHECKPOINT])
            .await
            .unwrap();
    });
    assert!(
        fs::read(&path)
            .unwrap()
            .windows(64)
            .any(|part| part == &old_only.as_bytes()[..64])
    );
    let original_inode = fs::metadata(&path).unwrap().ino();
    let opened = pollster::block_on(open_or_rewrite(&path, &scope(), |entries| async move {
        assert_eq!(entries, original);
        Ok(rewrite())
    }))
    .unwrap();
    assert!(opened.receipt.rewritten);
    assert!(opened.receipt.directory_synced);
    assert_eq!(opened.receipt.source_key_count, 4);
    assert_eq!(opened.receipt.destination_key_count, 5);
    assert_eq!(opened.receipt.preserved_key_count, 2);
    assert_ne!(
        opened.receipt.source_logical_hash,
        opened.receipt.destination_logical_hash
    );
    assert_ne!(fs::metadata(&path).unwrap().ino(), original_inode);
    assert_eq!(
        pollster::block_on(opened.backend.get("op/signed-fixture"))
            .unwrap()
            .unwrap(),
        OPAQUE
    );
    assert!(
        RedbBackend::open(&path).is_err(),
        "returned backend still owns replacement lock"
    );
    drop(opened);
    let data = fs::read(&path).unwrap();
    assert!(
        !data
            .windows(64)
            .any(|part| part == &old_only.as_bytes()[..64])
    );
    assert!(
        !data
            .windows(b"legacy plaintext fixture".len())
            .any(|part| part == b"legacy plaintext fixture")
    );
    let reopened = read_source(&path);
    assert_eq!(reopened[CHECKPOINT], SEALED);
    assert_eq!(reopened[KEYRING], SEALED_KEYS);
    assert_eq!(reopened[MARKER], SEALED_MARKER);
    assert_eq!(reopened["op/signed-fixture"], OPAQUE);
    assert!(stage_files(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn rewrite_removes_obsolete_pages_even_when_current_checkpoint_was_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    create_source(&path);
    let old_only = "deleted-checkpoint-residue-84c2b034".repeat(256);
    pollster::block_on(async {
        let backend = RedbBackend::open(&path).unwrap();
        backend.put(CHECKPOINT, old_only.as_bytes()).await.unwrap();
        backend.delete(CHECKPOINT).await.unwrap();
    });
    assert!(
        fs::read(&path)
            .unwrap()
            .windows(64)
            .any(|part| part == &old_only.as_bytes()[..64])
    );
    let opened = pollster::block_on(open_or_rewrite(&path, &scope(), |entries| async move {
        assert!(!entries.contains_key(CHECKPOINT));
        Ok(rewrite())
    }))
    .unwrap();
    assert!(opened.receipt.rewritten);
    drop(opened);
    assert!(
        !fs::read(&path)
            .unwrap()
            .windows(64)
            .any(|part| part == &old_only.as_bytes()[..64])
    );
}

#[test]
fn authenticated_current_decision_keeps_original_backend_and_no_stage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    let original = create_source(&path);
    let opened = pollster::block_on(open_or_rewrite(&path, &scope(), |entries| async move {
        assert_eq!(entries, original);
        // This fixture isolates copying. Production authenticates protected
        // ciphertext and its marker before returning this decision.
        Ok(MigrationDecision::Current)
    }))
    .unwrap();
    assert!(!opened.receipt.rewritten);
    assert!(!opened.receipt.directory_synced);
    assert_eq!(
        opened.receipt.source_logical_hash,
        opened.receipt.destination_logical_hash
    );
    assert!(RedbBackend::open(&path).is_err());
    drop(opened);
    assert!(stage_files(dir.path()).is_empty());
}

#[test]
fn absent_source_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.redb");
    let error = failure(pollster::block_on(open_or_rewrite(
        &path,
        &scope(),
        |_| async { panic!("preparation must not run for absent source") },
    )));
    assert!(!error.replacement_committed());
    assert!(!path.exists());
    assert!(stage_files(dir.path()).is_empty());
}

#[test]
fn existing_live_handle_or_clone_refuses_migration_before_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    let original = create_source(&path);
    let backend = RedbBackend::open(&path).unwrap();
    let clone = backend.clone();
    drop(backend);
    let error = failure(pollster::block_on(open_or_rewrite(
        &path,
        &scope(),
        |_| async { panic!("preparation must not run with a live source clone") },
    )));
    assert!(matches!(error, MigrationError::Store(_)));
    assert_eq!(
        pollster::block_on(clone.get(CHECKPOINT)).unwrap().unwrap(),
        original[CHECKPOINT]
    );
    drop(clone);
    assert_eq!(read_source(&path), original);
    assert!(stage_files(dir.path()).is_empty());
}

#[test]
fn unknown_protected_space_refuses_before_preparation() {
    for prefix in ["checkpoint", "commons-keyring", "sealed-storage"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.redb");
        let mut original = create_source(&path);
        let unknown = format!("knot-sync/{prefix}/space-b");
        pollster::block_on(async {
            let backend = RedbBackend::open(&path).unwrap();
            backend.put(&unknown, b"other custody").await.unwrap();
        });
        original.insert(unknown, b"other custody".to_vec());
        let error = failure(pollster::block_on(open_or_rewrite(
            &path,
            &scope(),
            |_| async { panic!("preparation must not run for unknown protected space") },
        )));
        assert!(matches!(error, MigrationError::Rejected(_)));
        assert_eq!(read_source(&path), original);
        assert!(stage_files(dir.path()).is_empty());
    }
}

#[test]
fn preparation_refusal_preserves_logical_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    let original = create_source(&path);
    let error = failure(pollster::block_on(open_or_rewrite(
        &path,
        &scope(),
        |_| async { Err("signed history authentication refused".to_owned()) },
    )));
    assert!(matches!(error, MigrationError::Preparation(_)));
    assert!(!error.replacement_committed());
    assert_eq!(read_source(&path), original);
    assert!(stage_files(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn source_symlink_and_hard_link_are_refused() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    let original = create_source(&path);
    let alias = dir.path().join("alias.redb");
    symlink(&path, &alias).unwrap();
    let error = failure(pollster::block_on(open_or_rewrite(
        &alias,
        &scope(),
        |_| async { panic!("preparation must not run through symlink") },
    )));
    assert!(matches!(error, MigrationError::Rejected(_)));
    fs::remove_file(&alias).unwrap();
    fs::hard_link(&path, &alias).unwrap();
    let error = failure(pollster::block_on(open_or_rewrite(
        &path,
        &scope(),
        |_| async { panic!("preparation must not run for multiply linked source") },
    )));
    assert!(matches!(error, MigrationError::Rejected(_)));
    fs::remove_file(&alias).unwrap();
    assert_eq!(read_source(&path), original);
}

#[cfg(unix)]
#[test]
fn replacements_cannot_omit_protected_values_or_change_opaque_keys() {
    for replacements in [
        BTreeMap::from([(CHECKPOINT.to_owned(), Some(SEALED.to_vec()))]),
        BTreeMap::from([("op/signed-fixture".to_owned(), None)]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.redb");
        let original = create_source(&path);
        let error = failure(pollster::block_on(open_or_rewrite(
            &path,
            &scope(),
            |_| async move { Ok(MigrationDecision::Rewrite { replacements }) },
        )));
        assert!(matches!(error, MigrationError::Rejected(_)));
        assert_eq!(read_source(&path), original);
        assert!(stage_files(dir.path()).is_empty());
    }
}

#[cfg(unix)]
#[test]
fn faults_before_replace_preserve_logical_source_and_remove_only_stage() {
    for step in [
        MigrationStep::BeforeStage,
        MigrationStep::AfterCopyEntry(1),
        MigrationStep::AfterCopyEntry(3),
        MigrationStep::BeforeVerify,
        MigrationStep::BeforeReplace,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.redb");
        let original = create_source(&path);
        let protected_sibling = dir.path().join(".knot-checkpoint-unrelated.redb");
        fs::write(&protected_sibling, b"independent owner bytes").unwrap();
        let mut faults = MigrationFaults {
            fail_at: Some(step),
            ..Default::default()
        };
        let error = failure(pollster::block_on(open_or_rewrite_with_faults(
            &path,
            &scope(),
            |_| async { Ok(rewrite()) },
            &mut faults,
        )));
        assert!(!error.replacement_committed(), "{step:?}");
        assert_eq!(read_source(&path), original, "{step:?}");
        assert_eq!(
            fs::read(&protected_sibling).unwrap(),
            b"independent owner bytes"
        );
        assert_eq!(stage_files(dir.path()), vec![protected_sibling]);
    }
}

#[cfg(unix)]
#[test]
fn staged_copy_corruption_is_detected_before_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    let original = create_source(&path);
    let mut faults = MigrationFaults {
        corrupt_copy: Some(("op/signed-fixture".to_owned(), b"corrupted stage".to_vec())),
        ..Default::default()
    };
    let error = failure(pollster::block_on(open_or_rewrite_with_faults(
        &path,
        &scope(),
        |_| async { Ok(rewrite()) },
        &mut faults,
    )));
    assert!(matches!(error, MigrationError::Verification));
    assert!(!error.replacement_committed());
    assert_eq!(read_source(&path), original);
    assert!(stage_files(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn directory_sync_failure_reports_committed_replacement_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    create_source(&path);
    let mut faults = MigrationFaults {
        fail_at: Some(MigrationStep::AfterReplaceBeforeDirectorySync),
        ..Default::default()
    };
    let error = failure(pollster::block_on(open_or_rewrite_with_faults(
        &path,
        &scope(),
        |_| async { Ok(rewrite()) },
        &mut faults,
    )));
    assert!(error.replacement_committed());
    let reopened = read_source(&path);
    assert_eq!(reopened[CHECKPOINT], SEALED);
    assert_eq!(reopened[KEYRING], SEALED_KEYS);
    assert_eq!(reopened["op/signed-fixture"], OPAQUE);
    assert!(stage_files(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn preparation_source_path_replacement_is_detected_without_overwriting_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    create_source(&path);
    let moved = dir.path().join("owner-moved.redb");
    let error = failure(pollster::block_on(open_or_rewrite(
        &path,
        &scope(),
        |_| async {
            fs::rename(&path, &moved).unwrap();
            create_source(&path);
            Ok(rewrite())
        },
    )));
    assert!(matches!(error, MigrationError::Rejected(_)));
    assert_eq!(
        read_source(&path)[CHECKPOINT],
        b"{\"snapshot\":\"legacy plaintext fixture\"}"
    );
    assert_eq!(
        read_source(&moved)[CHECKPOINT],
        b"{\"snapshot\":\"legacy plaintext fixture\"}"
    );
    assert!(stage_files(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn replacement_reopens_in_independent_process() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    create_source(&path);
    let opened = pollster::block_on(open_or_rewrite(&path, &scope(), |_| async {
        Ok(rewrite())
    }))
    .unwrap();
    drop(opened);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("migration_child_process_reader")
        .arg("--nocapture")
        .env("KNOT_CHECKPOINT_MIGRATION_CHILD_PATH", &path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child stdout: {}\nchild stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("migration child verified replacement")
    );
}

#[test]
fn migration_child_process_reader() {
    let Some(path) = std::env::var_os("KNOT_CHECKPOINT_MIGRATION_CHILD_PATH") else {
        return;
    };
    let reopened = read_source(Path::new(&path));
    assert_eq!(
        reopened.keys().cloned().collect::<BTreeSet<_>>(),
        [
            CHECKPOINT,
            KEYRING,
            MARKER,
            "op/signed-fixture",
            "log/retained-frontier"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    assert_eq!(reopened[CHECKPOINT], SEALED);
    assert_eq!(reopened[KEYRING], SEALED_KEYS);
    assert_eq!(reopened[MARKER], SEALED_MARKER);
    assert_eq!(reopened["op/signed-fixture"], OPAQUE);
    println!("migration child verified replacement");
}

#[cfg(not(unix))]
#[test]
fn unsupported_legacy_rewrite_preserves_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.redb");
    let original = create_source(&path);
    let error = failure(pollster::block_on(open_or_rewrite(
        &path,
        &scope(),
        |_| async { Ok(rewrite()) },
    )));
    assert!(matches!(error, MigrationError::UnsupportedPlatform));
    assert_eq!(read_source(&path), original);
    assert!(stage_files(dir.path()).is_empty());
}
