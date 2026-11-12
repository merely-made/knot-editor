// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The typed face of a collection item's projection recipe.
//!
//! `knot-sync` holds the item with its recipe as exact JSON text; this crate
//! owns the recipe's types and the scene crates that judge it.

use knot_sync::{CollectionError, CollectionItem, RecipeJson, RecipeValidator};

use crate::retention::ProjectionRecipeMaterial;

/// The scenomise-backed judge of a projection recipe: the recipe must read as
/// [`ProjectionRecipeMaterial`] and pass its own validation. Hand it to every
/// Knot sync store that retains composition material.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScenomiseRecipes;

impl RecipeValidator for ScenomiseRecipes {
    fn validate(&self, recipe: &RecipeJson) -> Result<(), CollectionError> {
        let material: ProjectionRecipeMaterial =
            serde_json::from_str(recipe.get()).map_err(CollectionError::Malformed)?;
        material.validate()
    }
}

/// Typed access to an item's projection recipe, and validation with the real judge.
pub trait CollectionItemExt: Sized {
    /// Carry `material` as this item's recipe (and make it a recipe item).
    #[must_use]
    fn with_projection_recipe(self, material: ProjectionRecipeMaterial) -> Self;

    /// The recipe as typed material, if the item carries one.
    ///
    /// # Errors
    /// Fails when the carried text is not recipe material.
    fn recipe_material(&self) -> Result<Option<ProjectionRecipeMaterial>, CollectionError>;

    /// Validate the item, recipe included.
    ///
    /// # Errors
    /// Fails as [`CollectionItem::validate_with`] does.
    fn validate(&self) -> Result<(), CollectionError>;
}

impl CollectionItemExt for CollectionItem {
    fn with_projection_recipe(self, material: ProjectionRecipeMaterial) -> Self {
        let text = serde_json::to_string(&material).expect("recipe material serializes");
        let recipe = RecipeJson::from_text(text).expect("serialized material is JSON");
        self.with_projection_recipe_json(recipe)
    }

    fn recipe_material(&self) -> Result<Option<ProjectionRecipeMaterial>, CollectionError> {
        self.projection_recipe
            .as_ref()
            .map(|recipe| serde_json::from_str(recipe.get()).map_err(CollectionError::Malformed))
            .transpose()
    }

    fn validate(&self) -> Result<(), CollectionError> {
        self.validate_with(&ScenomiseRecipes)
    }
}

/// The item as the pretty-printed collection file has always written it: the
/// recipe laid out as typed material at the item's depth, so the file's bytes
/// do not depend on how the recipe text was first produced.
pub(crate) fn for_store_file(item: &CollectionItem) -> Result<CollectionItem, CollectionError> {
    let mut stored = item.clone();
    if let Some(material) = item.recipe_material()? {
        let pretty = serde_json::to_string_pretty(&material).map_err(CollectionError::Malformed)?;
        let depth = pretty.replace('\n', "\n      ");
        stored.projection_recipe =
            Some(RecipeJson::from_text(depth).map_err(CollectionError::Malformed)?);
    }
    Ok(stored)
}
