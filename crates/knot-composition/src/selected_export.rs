// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit export of one copied text item. The caller selects the item;
//! private author notes, surrounding documents and collections stay local.

use crate::{CollectionError, CollectionItem, ItemKind};

pub const SCHEMA: &str = "knot.selected-content/v1";

pub fn export_selected_content(
    item: &CollectionItem,
) -> Result<serde_json::Value, CollectionError> {
    item.validate()?;
    if item.kind == ItemKind::ProjectionRecipe {
        return Err(CollectionError::Invalid(
            "projection recipes require a typed export adapter".into(),
        ));
    }
    Ok(serde_json::json!({
        "schema": SCHEMA, "id": item.id, "kind": item.kind,
        "label": item.label, "text": item.text,
        "document_source": item.document_source, "pack_sources": item.pack_sources,
        "tags": item.tags,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentAnchor;

    #[test]
    fn selected_text_preserves_native_anchor_but_omits_private_material() {
        let source = "private prefix; café; private suffix";
        let start = source.find("café").unwrap();
        let item = CollectionItem::new(ItemKind::Passage, "Coffee", "café")
            .with_document_source(
                DocumentAnchor::capture(
                    "mere://knot/document/coffee",
                    source,
                    start..start + "café".len(),
                )
                .unwrap(),
            )
            .with_author_notes("private author's notes")
            .with_tags(["coffee"]);
        let export = export_selected_content(&item).unwrap();
        let json = serde_json::to_string(&export).unwrap();
        assert!(!json.contains("private"));
        assert!(!json.contains("author_notes"));
        assert_eq!(export["text"], "café");
        assert_eq!(export["document_source"]["byte_span"]["end"], start + 5);
        assert_eq!(export["tags"][0], "coffee");
        assert_eq!(
            export["document_source"]["source_hash"],
            blake3::hash(source.as_bytes()).to_hex().to_string()
        );
    }
}
