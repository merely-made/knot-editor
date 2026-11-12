// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A store written by the code before the carve opens, folds and continues
//! unchanged. `fixtures/legacy_store.redb` was written by the unmodified
//! knot-editor: three documents, a delete, a recipe item, a note item and a
//! checkpoint, in a personal space.

mod common;

use knot_sync::{
    KnotDomain, KnotEncryptionProfile, KnotSyncCipher, KnotSyncEvent, KnotSyncStore, VaultDocument,
};
use muniment::RedbBackend;
use personae::{IdentityProvider, InMemoryProvider};

use common::{KeySealer, accept_recipes};

const SPACE: [u8; 32] = [0x51; 32];
const VAULT_KEY: [u8; 32] = [0x52; 32];

fn copy_of_fixture(into: &std::path::Path) -> std::path::PathBuf {
    let copy = into.join("legacy.redb");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy_store.redb"),
        &copy,
    )
    .unwrap();
    copy
}

fn open(copy: &std::path::Path) -> KnotSyncStore<RedbBackend> {
    let writer = InMemoryProvider::from_seed([0x53; 32])
        .master_public_key()
        .to_bytes();
    KnotSyncStore::from_backend(
        RedbBackend::open(copy).unwrap(),
        SPACE,
        [writer],
        KnotEncryptionProfile::PersonalVaultV1,
        KnotDomain::new(accept_recipes()),
        false,
    )
}

#[tokio::test]
async fn a_store_written_before_the_carve_folds_to_the_same_documents() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&copy_of_fixture(directory.path()));
    let vault = KeySealer::new(VAULT_KEY);
    let projection = store.projection(&vault).await.unwrap();
    let ids: Vec<_> = projection.documents.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["essay", "note"]);
    assert_eq!(projection.documents[0].body, b"# Essay\nfirst\n");
    assert!(projection.pending.is_empty());
    let checkpoint = store.load_checkpoint(&vault).await.unwrap();
    assert_eq!(checkpoint.unwrap().space_id, SPACE);
}

#[tokio::test]
async fn retained_composition_reads_back_with_its_recipe_text_byte_for_byte() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(&copy_of_fixture(directory.path()));
    let vault = KeySealer::new(VAULT_KEY);
    let items = store
        .composition_with_cipher(KnotSyncCipher::Personal(&vault), 100, 16 * 1024 * 1024)
        .await
        .unwrap();
    let lengths: Vec<_> = items
        .iter()
        .map(|(item, _, _)| (item.id.clone(), serde_json::to_string(item).unwrap().len()))
        .collect();
    // The lengths the unmodified code wrote them at.
    assert_eq!(
        lengths,
        [
            ("fixture-note".to_string(), 158),
            ("fixture-recipe".to_string(), 2889)
        ]
    );
}

#[tokio::test]
async fn the_old_store_keeps_accepting_writes_from_its_writer() {
    let directory = tempfile::tempdir().unwrap();
    let copy = copy_of_fixture(directory.path());
    let seed = InMemoryProvider::from_seed([0x53; 32])
        .master_keypair()
        .to_seed();
    let vault = KeySealer::new(VAULT_KEY);
    {
        let store = open(&copy);
        store
            .author(
                &seed,
                &vault,
                &KnotSyncEvent::Put(VaultDocument {
                    id: "later".into(),
                    title: "later".into(),
                    body: b"after the carve".to_vec(),
                    media_type: "text/vnd.knot".into(),
                }),
            )
            .await
            .unwrap();
    }
    let store = open(&copy);
    let ids: Vec<_> = store
        .projection(&vault)
        .await
        .unwrap()
        .documents
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, ["essay", "later", "note"]);
}
