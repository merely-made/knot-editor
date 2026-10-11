// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

fn main() {
    use knot_composition::{CollectionItem, DocumentAnchor, ItemKind};
    let source = "private prefix; café; private suffix";
    let start = source.find("café").unwrap();
    let mut item = CollectionItem::new(ItemKind::Passage, "Coffee", "café")
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
    item.id = "knot-coffee".into();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &knot_composition::selected_export::export_selected_content(&item).unwrap()
        )
        .unwrap()
    );
}
