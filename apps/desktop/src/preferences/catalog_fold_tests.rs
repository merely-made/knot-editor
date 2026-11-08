// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
#[test]
fn catalog_folds_round_trip_per_root_without_dropping_other_preferences() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("preferences.json");
    let mut store = PreferencesStore::open(path.clone());
    store
        .saved
        .other
        .insert("future".into(), serde_json::json!({"keep":true}));
    let record = incipit::curation::FoldRecord::selection(
        incipit::curation::FoldId::new("opaque"),
        "catalog:a",
        vec!["a".to_string(), "b".to_string()],
    )
    .unwrap();
    store
        .save_graph_folds("catalog:a", &[record.clone()])
        .unwrap();
    store.save_graph_folds("catalog:b", &[]).unwrap();
    let reloaded = PreferencesStore::open(path);
    assert_eq!(
        reloaded.preferences().graph_folds["catalog:a"],
        vec![record]
    );
    assert!(reloaded.preferences().graph_folds["catalog:b"].is_empty());
    assert_eq!(reloaded.preferences().other["future"]["keep"], true);
}
#[test]
fn malformed_fold_preferences_are_preserved_and_cannot_be_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("preferences.json");
    let bad = r#"{"graph_folds":{"catalog:a":[{"version":99}]}}"#;
    std::fs::write(&path, bad).unwrap();
    let mut store = PreferencesStore::open(path.clone());
    assert!(store.unreadable().is_some());
    assert!(store.save_graph_folds("catalog:a", &[]).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), bad);
}
