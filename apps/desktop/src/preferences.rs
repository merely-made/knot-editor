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
//! written back as loaded; the host that builds search reads it. Top-level
//! keys written by a newer Knot survive a save by this one.
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
    pub appearance: Appearance,
    /// A knot-editor `KnotEmbeddingPreference`, uninterpreted here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Value>,
    /// Top-level keys this build does not know.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            version: PREFERENCES_VERSION,
            appearance: Appearance::default(),
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
        let appearance = &mut preferences.appearance;
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

    /// Write `appearance` when it differs from the file, keeping every other
    /// section as loaded. `Ok(false)` means there was nothing to write.
    pub fn save_appearance(&mut self, appearance: &Appearance) -> Result<bool, String> {
        if self.saved.appearance == *appearance {
            return Ok(false);
        }
        if let Some(why) = &self.unreadable {
            return Err(format!(
                "the preferences file could not be read ({why}); reset it in Appearance to save again"
            ));
        }
        let next = DesktopPreferences {
            appearance: appearance.clone(),
            ..self.saved.clone()
        };
        next.save(&self.path)?;
        self.saved = next;
        Ok(true)
    }

    /// The writer's explicit reset: replace the file with `appearance` and
    /// defaults for everything else. The only path that overwrites a file
    /// that could not be read.
    pub fn reset(&mut self, appearance: &Appearance) -> Result<(), String> {
        let next = DesktopPreferences {
            appearance: appearance.clone(),
            ..DesktopPreferences::default()
        };
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
        assert_eq!(reopened.preferences().appearance, changed());
        assert_eq!(reopened.preferences().version, PREFERENCES_VERSION);

        // A file missing fields still loads; the rest are defaults.
        std::fs::write(&path, r#"{"appearance":{"dark":true}}"#).unwrap();
        let partial = DesktopPreferences::load(&path).unwrap();
        assert_eq!(
            partial.appearance,
            Appearance {
                dark: true,
                ..Appearance::default()
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
            DesktopPreferences::load(&path).unwrap().appearance,
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
        assert_eq!(store.save_appearance(&changed()), Ok(true));
        let written = json(&path);
        assert_eq!(written["later-knot"], future);
        assert_eq!(written["version"], 2);
        assert_eq!(written["appearance"]["font-size"], 19);
    }
}
