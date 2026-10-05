// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::fs;

use knot_composition::{
    AnchorError, ByteSpan, CollectionError, CollectionItem, CollectionStore, DocumentAnchor,
    ItemKind, MAX_STORE_BYTES, MAX_TEXT_BYTES, PackSource,
};

fn word(text: &str) -> CollectionItem {
    CollectionItem::new(ItemKind::Word, text, text)
}

#[test]
fn old_collection_records_do_not_require_a_recipe_payload() {
    let original = word("estuary");
    let json = serde_json::to_string(&original).unwrap();
    assert!(!json.contains("projection_recipe"));
    let decoded: CollectionItem = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, original);
    decoded.validate().unwrap();
    assert!(
        CollectionItem::new(ItemKind::ProjectionRecipe, "recipe", "readable summary")
            .validate()
            .is_err()
    );
}

#[test]
fn absent_open_is_read_only_and_collect_creates_a_reopenable_store() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("personal/collections.json");
    let mut store = CollectionStore::open(&path).unwrap();
    assert!(store.items().is_empty());
    assert!(!path.parent().unwrap().exists());
    let item = word("estuary");
    store.collect(item.clone()).unwrap();
    assert_eq!(store.items(), std::slice::from_ref(&item));
    assert_eq!(CollectionStore::open(&path).unwrap().items(), &[item]);
}

#[test]
fn all_item_kinds_and_author_material_survive_reopen_without_pack_access() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let document = "The estuary held β reeds.\n";
    let anchor = DocumentAnchor::capture("knot:document:field-notes", document, 4..11).unwrap();
    let mut store = CollectionStore::open(&path).unwrap();
    for (index, kind) in [
        ItemKind::Word,
        ItemKind::Passage,
        ItemKind::Sense,
        ItemKind::Pronunciation,
        ItemKind::Note,
    ]
    .into_iter()
    .enumerate()
    {
        let item = CollectionItem::new(kind, "  Author's label β  ", "  copied é\r\ntext  ")
            .in_collection("River notebook")
            .with_pack_source(PackSource::new(
                "mora:test",
                "edition-2026.09",
                "estuary#sense-2",
            ))
            .with_document_source(anchor.clone())
            .with_author_notes("Keep my reading—not a dictionary replacement.")
            .with_tags(["water", "β"])
            .with_order(50 - index as i64);
        store.collect(item).unwrap();
    }
    let reopened = CollectionStore::open(&path).unwrap();
    assert_eq!(reopened.items(), store.items());
    assert_eq!(reopened.items()[0].text, "  copied é\r\ntext  ");
    assert_eq!(reopened.items()[0].label, "  Author's label β  ");
    assert_eq!(
        reopened.items()[0].pack_sources[0].pack_version,
        "edition-2026.09"
    );
    assert_eq!(
        reopened.items()[0]
            .document_source
            .as_ref()
            .unwrap()
            .exact_quote,
        "estuary"
    );
    assert_eq!(reopened.items()[0].order, 50);
    assert_eq!(reopened.items()[4].order, 46);
}

#[test]
fn copied_quote_is_not_normalized_or_reinterpreted() {
    let source = "Cafe\u{301} ≠ Café";
    let anchor = DocumentAnchor::capture("scratch:original", source, 0..6).unwrap();
    assert_eq!(anchor.exact_quote, "Cafe\u{301}");
    assert_eq!(anchor.validate_source(source), Ok(()));
    assert_eq!(
        anchor.validate_source("Café ≠ Café"),
        Err(AnchorError::StaleSource)
    );
}

#[test]
fn stale_source_is_not_implicitly_relocated_even_if_quote_still_exists() {
    let source = "The tide withdrew.";
    let anchor = DocumentAnchor::capture("document:one", source, 4..8).unwrap();
    assert_eq!(anchor.validate_source(source), Ok(()));
    // The same quote still occurs at the same byte range, but the captured source changed.
    assert_eq!(
        anchor.validate_source("The tide returned."),
        Err(AnchorError::StaleSource)
    );
    assert_eq!(
        anchor.validate_source("Later: The tide withdrew."),
        Err(AnchorError::StaleSource)
    );
    assert_eq!(anchor.exact_quote, "tide");
    assert_eq!(anchor.byte_span, ByteSpan { start: 4, end: 8 });
}

#[test]
fn optional_hash_checks_exact_original_bytes_and_never_searches() {
    let mut anchor = DocumentAnchor::capture("document:one", "The tide withdrew.", 4..8).unwrap();
    anchor.source_hash = None;
    assert_eq!(anchor.validate_source("The tide returned."), Ok(()));
    assert_eq!(
        anchor.validate_source("The reed tide."),
        Err(AnchorError::QuoteMismatch)
    );
    assert_eq!(
        anchor.validate_source("The"),
        Err(AnchorError::InvalidRange)
    );
}

#[test]
fn unicode_ranges_are_bytes_not_characters_and_cannot_split_codepoints() {
    let source = "α🌊é tide";
    let anchor = DocumentAnchor::capture("document:unicode", source, 2..8).unwrap();
    assert_eq!(anchor.exact_quote, "🌊é");
    assert_eq!(anchor.byte_span, ByteSpan { start: 2, end: 8 });
    assert_eq!(anchor.validate_source(source), Ok(()));
    for (start, end) in [(1, 2), (2, 4), (8, 100), (4, 2), (2, 2)] {
        assert_eq!(
            DocumentAnchor::capture("document:unicode", source, start..end),
            Err(AnchorError::InvalidRange)
        );
    }
    let mut split_anchor = anchor;
    split_anchor.source_hash = None;
    split_anchor.byte_span = ByteSpan { start: 1, end: 7 };
    assert_eq!(
        split_anchor.validate_source(source),
        Err(AnchorError::InvalidRange)
    );
}

#[test]
fn malformed_or_unknown_store_is_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    for bytes in [
        br#"{this is the author's malformed data"#.as_slice(),
        br#"{"version":1,"items":[],"future_personal_material":"retain"}"#.as_slice(),
        br#"{"version":999,"items":[]}"#.as_slice(),
        br#"{"version":1}"#.as_slice(),
        b"".as_slice(),
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(CollectionStore::open(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn unknown_item_fields_and_duplicate_ids_are_not_silently_discarded() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let item = word("river");
    let mut value = serde_json::json!({"version": 1, "items": [item.clone()]});
    value["items"][0]["unknown_source"] = serde_json::json!("personal source");
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        CollectionStore::open(&path),
        Err(CollectionError::Malformed(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let duplicate =
        serde_json::to_vec(&serde_json::json!({"version": 1, "items": [item.clone(), item]}))
            .unwrap();
    fs::write(&path, &duplicate).unwrap();
    assert!(matches!(
        CollectionStore::open(&path),
        Err(CollectionError::DuplicateId(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), duplicate);
}

#[test]
fn refused_item_or_duplicate_preserves_previous_file_and_memory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let mut store = CollectionStore::open(&path).unwrap();
    let original = word("river");
    store.collect(original.clone()).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        store.collect(original.clone()),
        Err(CollectionError::DuplicateId(_))
    ));
    let mut invalid = word("changed");
    invalid.text.clear();
    assert!(matches!(
        store.collect(invalid),
        Err(CollectionError::Invalid(_))
    ));
    let too_large = CollectionItem::new(ItemKind::Passage, "big", "x".repeat(MAX_TEXT_BYTES + 1));
    assert!(matches!(
        store.collect(too_large),
        Err(CollectionError::Invalid(_))
    ));
    assert_eq!(store.items(), &[original]);
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn malformed_external_replacement_is_preserved_and_mutation_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let mut store = CollectionStore::open(&path).unwrap();
    let original = word("river");
    store.collect(original.clone()).unwrap();
    let external = b"author's externally changed malformed file";
    fs::write(&path, external).unwrap();
    assert!(matches!(
        store.collect(word("tide")),
        Err(CollectionError::ConcurrentChange)
    ));
    assert!(matches!(
        store.remove(&original.id),
        Err(CollectionError::ConcurrentChange)
    ));
    assert_eq!(store.items(), &[original]);
    assert_eq!(fs::read(&path).unwrap(), external);
}

#[test]
fn two_handles_cannot_lose_each_others_material() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let mut first = CollectionStore::open(&path).unwrap();
    let mut stale = CollectionStore::open(&path).unwrap();
    first.collect(word("river")).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        stale.collect(word("tide")),
        Err(CollectionError::ConcurrentChange)
    ));
    assert!(stale.items().is_empty());
    assert_eq!(fs::read(&path).unwrap(), before);
    let mut refreshed = CollectionStore::open(&path).unwrap();
    refreshed.collect(word("tide")).unwrap();
    assert_eq!(CollectionStore::open(&path).unwrap().items().len(), 2);
}

#[test]
fn sidecar_lock_refuses_overlapping_writes_without_changing_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let mut store = CollectionStore::open(&path).unwrap();
    store.collect(word("river")).unwrap();
    let before = fs::read(&path).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.path().join("collections.json.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(matches!(
        store.collect(word("tide")),
        Err(CollectionError::ConcurrentChange)
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(store.items().len(), 1);
    drop(lock);
    store.collect(word("tide")).unwrap();
}

#[test]
fn remove_is_durable_and_absent_remove_does_not_create_a_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let mut store = CollectionStore::open(&path).unwrap();
    assert!(!store.remove("absent").unwrap());
    assert!(!path.exists());
    let first = word("river");
    let second = word("tide");
    store.collect(first.clone()).unwrap();
    store.collect(second.clone()).unwrap();
    assert!(store.remove(&first.id).unwrap());
    assert_eq!(CollectionStore::open(&path).unwrap().items(), &[second]);
    let before = fs::read(&path).unwrap();
    assert!(!store.remove(&first.id).unwrap());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn oversized_existing_store_is_refused_without_modification() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let oversized = fs::File::create(&path).unwrap();
    oversized.set_len(MAX_STORE_BYTES as u64 + 1).unwrap();
    assert!(matches!(
        CollectionStore::open(&path),
        Err(CollectionError::TooLarge)
    ));
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        MAX_STORE_BYTES as u64 + 1
    );
}

#[test]
fn invalid_anchor_shape_or_hash_cannot_be_persisted() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    let mut store = CollectionStore::open(&path).unwrap();
    let mut anchor = DocumentAnchor::capture("document:one", "river", 0..5).unwrap();
    anchor.source_hash = Some("not a source digest".into());
    assert!(matches!(
        store.collect(word("river").with_document_source(anchor.clone())),
        Err(CollectionError::Anchor(AnchorError::InvalidHash))
    ));
    anchor.source_hash = None;
    anchor.byte_span.end = 6;
    assert!(matches!(
        store.collect(word("river").with_document_source(anchor)),
        Err(CollectionError::Anchor(AnchorError::InvalidRange))
    ));
    assert!(store.items().is_empty());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn symlink_store_is_refused_and_target_is_untouched() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("personal-original.json");
    let link = root.path().join("collections.json");
    fs::write(&target, b"personal bytes").unwrap();
    symlink(&target, &link).unwrap();
    assert!(matches!(
        CollectionStore::open(&link),
        Err(CollectionError::Invalid(_))
    ));
    assert_eq!(fs::read(&target).unwrap(), b"personal bytes");
}

#[cfg(unix)]
#[test]
fn saved_personal_data_is_private_and_temporary_files_are_cleaned() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("collections.json");
    CollectionStore::open(&path)
        .unwrap()
        .collect(word("river"))
        .unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let names: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2); // personal JSON and stable lock, no temporary copies
}
