// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A retained composition item whose projection recipe the scenomise-backed
//! validator refuses is refused at admission, with the same error, however the
//! sync store is built.

use knot_composition::{CollectionItem, ItemKind};
use knot_editor::{KnotSyncError, KnotSyncEvent, KnotSyncStore, KnotVault};
use personae::{IdentityProvider, InMemoryProvider};

#[path = "../../knot-composition/tests/support/recipe.rs"]
mod recipe_support;

const SPACE: [u8; 32] = [0x61; 32];

fn recipe_item() -> CollectionItem {
    CollectionItem::new(ItemKind::ProjectionRecipe, "Recipe", "Summary")
        .with_projection_recipe(recipe_support::recipe_material())
}

async fn refusal(item_json: String) -> String {
    let root = tempfile::tempdir().unwrap();
    let alice = InMemoryProvider::from_seed([0x62; 32]);
    let seed = alice.master_keypair().to_seed();
    let store = KnotSyncStore::in_memory(SPACE, [alice.master_public_key().to_bytes()]);
    let vault = KnotVault::open(root.path().join("vault"), [0x63; 32]).unwrap();
    match store
        .author(&seed, &vault, &KnotSyncEvent::RetainCompositionV1 { item_json })
        .await
    {
        Err(KnotSyncError::Payload(message)) => message,
        other => panic!("expected a payload refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_recipe_that_is_a_valid_item_is_admitted() {
    let root = tempfile::tempdir().unwrap();
    let alice = InMemoryProvider::from_seed([0x62; 32]);
    let seed = alice.master_keypair().to_seed();
    let store = KnotSyncStore::in_memory(SPACE, [alice.master_public_key().to_bytes()]);
    let vault = KnotVault::open(root.path().join("vault"), [0x63; 32]).unwrap();
    let item_json = serde_json::to_string(&recipe_item()).unwrap();
    store
        .author(&seed, &vault, &KnotSyncEvent::RetainCompositionV1 { item_json })
        .await
        .unwrap();
}

#[tokio::test]
async fn the_recipe_shape_check_still_refuses_at_admission() {
    let mut value = serde_json::to_value(recipe_item()).unwrap();
    let mut presentation =
        serde_json::to_value(knot_composition::retention::RecipeScenePresentation::default())
            .unwrap();
    presentation["version"] = 9.into();
    value["projection_recipe"]["presentation"] = presentation;
    let message = refusal(value.to_string()).await;
    println!("SHAPE: {message}");
    assert_eq!(
        message,
        "invalid collection material: unsupported recipe scene presentation version"
    );
}

#[tokio::test]
async fn the_recipe_one_mebibyte_bound_still_refuses_at_admission() {
    let mut value = serde_json::to_value(recipe_item()).unwrap();
    value["projection_recipe"]["snapshot"]["recipe"]["definition"]["provenance"]["source_revision"] =
        "x".repeat(knot_composition::MAX_TEXT_BYTES + 1).into();
    let message = refusal(value.to_string()).await;
    println!("BOUND: {message}");
    assert_eq!(
        message,
        "invalid collection material: projection recipe material exceeds one MiB"
    );
}
