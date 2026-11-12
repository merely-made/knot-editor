// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The collection file keeps its bytes now that a projection recipe is carried
//! as text. `fixtures/recipe_collection.json` was written by the code before
//! the carve.

use knot_composition::{CollectionItem, CollectionItemExt, CollectionStore, ItemKind};

#[path = "support/recipe.rs"]
mod recipe_support;

const FIXTURE: &[u8] = include_bytes!("fixtures/recipe_collection.json");

fn recipe_item() -> CollectionItem {
    let mut item = CollectionItem::new(ItemKind::ProjectionRecipe, "Recipe", "Summary")
        .with_projection_recipe(recipe_support::recipe_material());
    item.id = "fixture-recipe".into();
    item
}

fn note_item() -> CollectionItem {
    let mut note = CollectionItem::new(ItemKind::Note, "Note", "A note");
    note.id = "fixture-note".into();
    note
}

#[test]
fn a_new_store_writes_the_bytes_the_old_code_wrote() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collection.json");
    let mut store = CollectionStore::open(&path).unwrap();
    store.collect(recipe_item()).unwrap();
    store.collect(note_item()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), FIXTURE);
}

#[test]
fn an_old_file_reopens_equal_and_rewrites_to_the_same_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collection.json");
    std::fs::write(&path, FIXTURE).unwrap();
    let mut store = CollectionStore::open(&path).unwrap();
    assert_eq!(store.items(), [recipe_item(), note_item()]);
    assert!(store.remove("fixture-note").unwrap());
    store.collect(note_item()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), FIXTURE);
}
