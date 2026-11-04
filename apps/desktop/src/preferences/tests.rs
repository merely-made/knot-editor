// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use tempfile::tempdir;

/// The serde shape of `KnotEmbeddingPreference::bert_cpu` with pinned
/// weights, as knot-editor writes it (kebab-case, `digest` omitted when
/// unpinned).
const EMBEDDING: &str = r#"{"provider":"bert-cpu","weights":{"path":"C:/models/x","digest":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}}"#;

fn changed() -> Appearance {
    Appearance {
        dark: true,
        theme_choice: None,
        highlight: false,
        font_size: 19,
        source_face: crate::appearance::SourceFace::SystemMonospace,
        measure: crate::appearance::Measure::Full,
        follow_hard_wrap: false,
        relaxed: false,
    }
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn legacy_width_migrates_without_losing_newer_preferences() {
    use crate::appearance::{Measure, SourceFace};
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    for (legacy, explicit, expected) in [
        (true, None, Measure::Full),
        (false, None, Measure::Medium),
        (true, Some("narrow"), Measure::Narrow),
    ] {
        let mut appearance = serde_json::json!({
            "wide": legacy,
            "later-face": { "name": "Future face", "weight": 450 },
        });
        if let Some(measure) = explicit {
            appearance["measure"] = Value::String(measure.into());
        }
        let future = appearance["later-face"].clone();
        std::fs::write(
            &path,
            serde_json::json!({ "appearance": appearance }).to_string(),
        )
        .unwrap();
        let mut store = PreferencesStore::open(path.clone());
        assert_eq!(store.unreadable(), None);
        assert_eq!(store.preferences().appearance.known.measure, expected);
        let mut changed = store.preferences().appearance.known.clone();
        changed.source_face = SourceFace::SystemMonospace;
        changed.follow_hard_wrap = false;
        store.save_appearance(&changed).unwrap();
        let written = json(&path);
        assert!(written["appearance"].get("wide").is_none());
        assert_eq!(written["appearance"]["later-face"], future);
        assert_eq!(
            DesktopPreferences::load(&path).unwrap().appearance.known,
            changed
        );
    }
}

#[test]
fn appearance_round_trips_through_the_file() {
    let root = tempdir().unwrap();
    let path = root.path().join("Knot").join(PREFERENCES_FILE);
    let mut store = PreferencesStore::open(path.clone());
    assert_eq!(store.save_appearance(&changed()), Ok(true));
    assert!(!temporary_path(&path).exists(), "temp file left behind");
    let reopened = PreferencesStore::open(path.clone());
    assert_eq!(reopened.unreadable(), None);
    assert_eq!(reopened.preferences().appearance.known, changed());
    assert_eq!(reopened.preferences().version, PREFERENCES_VERSION);
    assert_eq!(reopened.written_by_newer(), None);

    // A file missing fields still loads; the rest are defaults.
    std::fs::write(&path, r#"{"appearance":{"dark":true}}"#).unwrap();
    let partial = DesktopPreferences::load(&path).unwrap();
    assert_eq!(
        partial.appearance,
        StoredAppearance {
            known: Appearance {
                dark: true,
                ..Appearance::default()
            },
            other: Map::new(),
        }
    );
    assert_eq!(partial.version, PREFERENCES_VERSION);
}

#[test]
fn graph_layout_is_a_persistent_surface_preference() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    let mut store = PreferencesStore::open(path.clone());
    assert_eq!(store.save_graph_layout("grid.default"), Ok(true));
    assert_eq!(
        DesktopPreferences::load(&path).unwrap().graph_layout,
        "grid.default"
    );
    assert_eq!(store.save_graph_layout("grid.default"), Ok(false));
}

#[test]
fn collapse_defaults_migrate_and_threshold_changes_keep_unknown_fields() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    std::fs::write(&path, r#"{"version":3,"later":{"flag":true},"collapse":{"reading-min-width":310,"later-rule":{"priority":2}}}"#).unwrap();
    let mut store = PreferencesStore::open(path.clone());
    assert_eq!(store.preferences().collapse.menu_below, 1150);
    let mut collapse = store.preferences().collapse.clone();
    collapse.menu_below = 760;
    collapse.drawer_below = 480;
    assert_eq!(store.save_collapse(&collapse), Ok(true));
    assert_eq!(store.save_collapse(&collapse), Ok(false));
    store.save_appearance(&changed()).unwrap();
    let written = json(&path);
    assert_eq!(written["version"], 3);
    assert_eq!(written["later"]["flag"], true);
    assert_eq!(written["collapse"]["later-rule"]["priority"], 2);
    let reopened = DesktopPreferences::load(&path).unwrap();
    assert_eq!(reopened.collapse, collapse);
    let known_only = CollapsePreferences {
        menu_below: 800,
        ..CollapsePreferences::default()
    };
    store.save_collapse(&known_only).unwrap();
    assert_eq!(json(&path)["collapse"]["later-rule"]["priority"], 2);
    std::fs::write(&path, "{}").unwrap();
    assert_eq!(
        DesktopPreferences::load(&path).unwrap().collapse,
        CollapsePreferences::default()
    );
}

#[test]
fn recovery_retention_is_bounded_and_persists_without_changing_other_preferences() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    let mut store = PreferencesStore::open(path.clone());
    store.save_appearance(&changed()).unwrap();
    assert_eq!(store.save_recovery_days(7), Ok(true));
    assert_eq!(store.save_recovery_days(7), Ok(false));
    assert!(store.save_recovery_days(365).is_err());
    let reopened = DesktopPreferences::load(&path).unwrap();
    assert_eq!(reopened.recovery_days, 7);
    assert_eq!(reopened.appearance.known, changed());
}

#[test]
fn a_missing_file_yields_defaults_and_writes_nothing() {
    let root = tempdir().unwrap();
    let parent = root.path().join("Knot");
    let path = parent.join(PREFERENCES_FILE);
    let mut store = PreferencesStore::open(path.clone());
    assert_eq!(store.unreadable(), None);
    assert_eq!(store.preferences(), &DesktopPreferences::default());
    assert_eq!(store.save_appearance(&Appearance::default()), Ok(false));
    assert!(!path.exists());
    assert!(!parent.exists());
}

#[test]
fn a_malformed_file_yields_defaults_and_refuses_to_save_until_reset() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    let malformed = "{\"appearance\": {\"dark\": tru";
    std::fs::write(&path, malformed).unwrap();
    let mut store = PreferencesStore::open(path.clone());
    assert!(
        store
            .unreadable()
            .is_some_and(|why| why.contains("preferences.json"))
    );
    assert_eq!(store.preferences(), &DesktopPreferences::default());
    let refused = store.save_appearance(&changed()).unwrap_err();
    assert!(refused.contains("could not be read"), "{refused}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), malformed);
    store.reset(&changed()).unwrap();
    assert_eq!(store.unreadable(), None);
    assert_eq!(
        DesktopPreferences::load(&path).unwrap().appearance.known,
        changed()
    );
}

#[test]
fn an_opaque_embedding_section_survives_load_and_save() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    let embedding: Value = serde_json::from_str(EMBEDDING).unwrap();
    let file = serde_json::json!({ "version": 1, "embedding": embedding });
    std::fs::write(&path, serde_json::to_string_pretty(&file).unwrap()).unwrap();
    let mut store = PreferencesStore::open(path.clone());
    assert_eq!(store.preferences().embedding.as_ref(), Some(&embedding));
    assert_eq!(store.save_appearance(&changed()), Ok(true));
    let written = json(&path);
    assert_eq!(written["embedding"], embedding);
    assert_eq!(
        DesktopPreferences::load(&path).unwrap().embedding,
        Some(embedding)
    );
}

#[test]
fn an_unknown_top_level_key_survives_a_save() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    let future = serde_json::json!({ "pane": "left", "recent": [1, 2.5, null] });
    let file = serde_json::json!({ "version": 2, "later-knot": future });
    std::fs::write(&path, file.to_string()).unwrap();
    let mut store = PreferencesStore::open(path.clone());
    assert_eq!(store.unreadable(), None);
    let note = store.written_by_newer().expect("version 2 is newer than 1");
    assert!(
        note.contains("version 2") && note.contains("version 1"),
        "{note}"
    );
    assert_eq!(store.save_appearance(&changed()), Ok(true));
    let written = json(&path);
    assert_eq!(written["later-knot"], future);
    assert_eq!(written["version"], 2);
    assert_eq!(written["appearance"]["font-size"], 19);
}

#[test]
fn an_unknown_key_inside_appearance_survives_an_appearance_change() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    let accent = serde_json::json!({ "hue": 212, "names": ["slate", null] });
    let file = serde_json::json!({
        "version": 1,
        "appearance": { "dark": false, "font-size": 15, "accent": accent },
    });
    std::fs::write(&path, file.to_string()).unwrap();
    let mut store = PreferencesStore::open(path.clone());
    assert_eq!(store.unreadable(), None);
    assert_eq!(store.preferences().appearance.known.font_size, 15);
    assert_eq!(store.preferences().appearance.other["accent"], accent);

    assert_eq!(store.save_appearance(&changed()), Ok(true));
    let written = json(&path);
    assert_eq!(written["appearance"]["accent"], accent);
    assert_eq!(written["appearance"]["dark"], true);
    assert_eq!(written["appearance"]["font-size"], 19);
    let reloaded = DesktopPreferences::load(&path).unwrap();
    assert_eq!(reloaded.appearance.known, changed());
    assert_eq!(reloaded.appearance.other.len(), 1);
    assert!(
        reloaded.other.is_empty(),
        "a nested key must not leak to the top level: {:?}",
        reloaded.other
    );
}
