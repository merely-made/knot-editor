// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A projection recipe carried as opaque JSON, and the judge of it.
//!
//! The recipe's own types live with the scene crates. Here it is exact text:
//! what a writer signs is what a reader folds, byte for byte, so numbers and
//! key order are never re-rendered on the way through.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::composition::CollectionError;

/// The exact JSON text of a projection recipe.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipeJson(Box<RawValue>);

impl RecipeJson {
    /// Wrap text that is one JSON value.
    ///
    /// # Errors
    /// Fails when `text` is not a JSON value.
    pub fn from_text(text: impl Into<String>) -> Result<Self, serde_json::Error> {
        RawValue::from_string(text.into()).map(Self)
    }

    /// The recipe's JSON text, exactly as it was written.
    #[must_use]
    pub fn get(&self) -> &str {
        self.0.get()
    }
}

impl PartialEq for RecipeJson {
    /// Equal text, or the same JSON value when only the layout differs.
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
            || matches!(
                (
                    serde_json::from_str::<serde_json::Value>(self.get()),
                    serde_json::from_str::<serde_json::Value>(other.get()),
                ),
                (Ok(left), Ok(right)) if left == right
            )
    }
}

/// Judges a projection recipe when an item that carries one is validated.
///
/// The store is built with one; knot-sync supplies none, so a store cannot be
/// opened that forgets to check recipes.
pub trait RecipeValidator: Send + Sync {
    /// Accept the recipe or say why not.
    ///
    /// # Errors
    /// Returns the refusal that admission reports.
    fn validate(&self, recipe: &RecipeJson) -> Result<(), CollectionError>;
}
