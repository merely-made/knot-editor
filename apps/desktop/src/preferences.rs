// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! App preferences the standalone desktop keeps across launches, in one local
//! file it owns. The file is never synced, and it is not `KnotSettings`: that
//! is the sync file, and it excludes surface preferences.
//!
//! The embedding section is a knot-editor `KnotEmbeddingPreference`. This app
//! does not link knot-editor, so the section is carried as opaque JSON and
//! written back as loaded; the host that builds search reads it. Keys written
//! by a newer Knot, at the top level or inside the appearance section, survive
//! a save by this one. A file whose version is newer than this build writes
//! loads what this build knows, says so, and keeps saving.
//!
//! A missing file is unconfigured: defaults, and nothing is written until a
//! preference changes. An unreadable or malformed file is an error, and saves
//! refuse until the writer resets it, so a typo is never replaced by defaults.

use crate::appearance::Appearance;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const PREFERENCES_FILE: &str = "preferences.json";
pub const PREFERENCES_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPreferences {
    /// Kept as loaded, so a file a newer Knot wrote is not relabelled older.
    pub version: u32,
    pub appearance: StoredAppearance,
    /// A knot-editor `KnotEmbeddingPreference`, uninterpreted here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Value>,
    /// Top-level keys this build does not know.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// The appearance section as stored: the settings this build knows, plus any
/// keys a newer Knot wrote there, kept so a save does not drop them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredAppearance {
    #[serde(flatten)]
    pub known: Appearance,
    /// Appearance keys this build does not know.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            version: PREFERENCES_VERSION,
            appearance: StoredAppearance::default(),
            embedding: None,
            other: Map::new(),
        }
    }
}

impl DesktopPreferences {
    /// Missing file: defaults. Anything else that fails names the path and why.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            },
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        let mut preferences: Self =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        let appearance = &mut preferences.appearance.known;
        appearance.font_size = appearance
            .font_size
            .clamp(Appearance::MIN_FONT_SIZE, Appearance::MAX_FONT_SIZE);
        Ok(preferences)
    }

    /// Write a sibling temp file, then rename it over the target, so a crash
    /// mid-write leaves the previous file rather than a truncated one.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let fail = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| fail(&error))?;
        }
        let mut text = serde_json::to_string_pretty(self).map_err(|error| fail(&error))?;
        text.push('\n');
        let temporary = temporary_path(path);
        let written = std::fs::File::create(&temporary).and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
        if let Err(error) = written.and_then(|()| std::fs::rename(&temporary, path)) {
            let _ = std::fs::remove_file(&temporary);
            return Err(fail(&error));
        }
        Ok(())
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// The preferences file as this window holds it.
#[derive(Debug)]
pub struct PreferencesStore {
    path: PathBuf,
    /// What the file holds: as loaded, or as last written.
    saved: DesktopPreferences,
    /// Why the file could not be read. Saves refuse while this is set.
    unreadable: Option<String>,
}

impl PreferencesStore {
    pub fn open(path: PathBuf) -> Self {
        let (saved, unreadable) = match DesktopPreferences::load(&path) {
            Ok(saved) => (saved, None),
            Err(why) => (DesktopPreferences::default(), Some(why)),
        };
        Self {
            path,
            saved,
            unreadable,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn preferences(&self) -> &DesktopPreferences {
        &self.saved
    }

    pub fn unreadable(&self) -> Option<&str> {
        self.unreadable.as_deref()
    }

    /// One line for the writer when the file was written by a newer Knot,
    /// naming both versions. Nothing is refused: known settings loaded, the
    /// rest are carried, and saves go ahead with the file's version kept.
    pub fn written_by_newer(&self) -> Option<String> {
        let found = self.saved.version;
        (found > PREFERENCES_VERSION).then(|| {
            format!(
                "Preferences were written by a newer Knot (file version {found}, this Knot writes \
                 version {PREFERENCES_VERSION}); settings this Knot does not know are kept."
            )
        })
    }

    /// Write `appearance` when it differs from the file, keeping every other
    /// section, and appearance keys this build does not know, as loaded.
    /// `Ok(false)` means there was nothing to write.
    pub fn save_appearance(&mut self, appearance: &Appearance) -> Result<bool, String> {
        if self.saved.appearance.known == *appearance {
            return Ok(false);
        }
        if let Some(why) = &self.unreadable {
            return Err(format!(
                "the preferences file could not be read ({why}); reset it in Appearance to save again"
            ));
        }
        let mut next = self.saved.clone();
        next.appearance.known = appearance.clone();
        next.save(&self.path)?;
        self.saved = next;
        Ok(true)
    }

    /// The writer's explicit reset: replace the file with `appearance` and
    /// defaults for everything else. The only path that overwrites a file
    /// that could not be read.
    pub fn reset(&mut self, appearance: &Appearance) -> Result<(), String> {
        let mut next = DesktopPreferences::default();
        next.appearance.known = appearance.clone();
        next.save(&self.path)?;
        self.saved = next;
        self.unreadable = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// The serde shape of `KnotEmbeddingPreference::bert_cpu` with pinned
    /// weights, as knot-editor writes it (kebab-case, `digest` omitted when
    /// unpinned).
    const EMBEDDING: &str = r#"{"provider":"bert-cpu","weights":{"path":"C:/models/x","digest":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}}"#;

    fn changed() -> Appearance {
        Appearance {
            dark: true,
            highlight: false,
            font_size: 19,
            wide: true,
            relaxed: false,
        }
    }

    fn json(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
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
}
