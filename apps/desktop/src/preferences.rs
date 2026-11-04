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
use crate::collapse::CollapsePreferences;
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
    pub collapse: CollapsePreferences,
    /// Cartography strategy used by the shared mere view.
    pub graph_layout: String,
    /// View-local folds keyed by the canonical catalog root, never document truth.
    pub graph_folds: std::collections::BTreeMap<String, Vec<curation::FoldRecord>>,
    /// Maximum age of private local recovery copies in this standalone app.
    pub recovery_days: u16,
    /// A knot-editor `KnotEmbeddingPreference`, uninterpreted here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Value>,
    /// Local contextual navigation state. Its payload is versioned by the
    /// shared scope history model and remains separate from document content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_navigation: Option<Value>,
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
            collapse: CollapsePreferences::default(),
            graph_layout: mere_view::DEFAULT_LAYOUT.to_owned(),
            graph_folds: Default::default(),
            recovery_days: 30,
            embedding: None,
            context_navigation: None,
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
        preferences.recovery_days = preferences.recovery_days.clamp(1, 90);
        preferences.collapse.normalize();
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

    pub fn context_navigation<T: serde::de::DeserializeOwned>(&self) -> Result<Option<T>, String> {
        self.saved
            .context_navigation
            .as_ref()
            .map(|value| {
                serde_json::from_value(value.clone()).map_err(|error| {
                    format!(
                        "{}: invalid context navigation: {error}",
                        self.path.display()
                    )
                })
            })
            .transpose()
    }

    pub fn save_context_navigation<T: Serialize>(
        &mut self,
        navigation: &T,
    ) -> Result<bool, String> {
        let value = serde_json::to_value(navigation).map_err(|error| error.to_string())?;
        if self.saved.context_navigation.as_ref() == Some(&value) {
            return Ok(false);
        }
        if let Some(why) = &self.unreadable {
            return Err(format!(
                "the preferences file could not be read ({why}); reset it in Appearance to save again"
            ));
        }
        let mut next = self.saved.clone();
        next.context_navigation = Some(value);
        next.save(&self.path)?;
        self.saved = next;
        Ok(true)
    }

    /// The workspace layout is host-local presentation, separate from source.
    pub fn workspace_forme(&self) -> Option<Value> {
        self.saved.other.get("workspace_forme").cloned()
    }

    pub fn save_workspace_forme(
        &mut self,
        baseline: Option<&Value>,
        value: &Value,
    ) -> Result<(), String> {
        if let Some(why) = &self.unreadable {
            return Err(format!("Preferences could not be read: {why}"));
        }
        // Preserve unrelated changes made since this store was opened, and
        // refuse an overwritten layout instead of acknowledging a stale draft.
        let mut next = DesktopPreferences::load(&self.path)?;
        if next.other.get("workspace_forme") != baseline {
            self.saved = next;
            return Err(
                "The saved workspace changed elsewhere; discard this draft and unlock again".into(),
            );
        }
        next.other.insert("workspace_forme".into(), value.clone());
        next.save(&self.path)?;
        self.saved = next;
        Ok(())
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

    pub fn save_graph_folds(
        &mut self,
        scope: &str,
        folds: &[curation::FoldRecord],
    ) -> Result<(), String> {
        if let Some(why) = &self.unreadable {
            return Err(format!("preferences are unreadable: {why}"));
        }
        let mut next = self.saved.clone();
        next.graph_folds.insert(scope.to_owned(), folds.to_vec());
        next.save(&self.path)?;
        self.saved = next;
        Ok(())
    }

    pub fn save_graph_layout(&mut self, layout: &str) -> Result<bool, String> {
        if self.saved.graph_layout == layout {
            return Ok(false);
        }
        if let Some(why) = &self.unreadable {
            return Err(format!(
                "the preferences file could not be read ({why}); reset it in Appearance to save again"
            ));
        }
        let mut next = self.saved.clone();
        next.graph_layout = layout.to_owned();
        next.save(&self.path)?;
        self.saved = next;
        Ok(true)
    }

    pub fn save_collapse(&mut self, collapse: &CollapsePreferences) -> Result<bool, String> {
        let mut collapse = collapse.clone();
        for (key, value) in &self.saved.collapse.other {
            collapse
                .other
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
        collapse.normalize();
        if self.saved.collapse == collapse {
            return Ok(false);
        }
        if let Some(why) = &self.unreadable {
            return Err(format!(
                "the preferences file could not be read ({why}); reset it in Appearance to save again"
            ));
        }
        let mut next = self.saved.clone();
        next.collapse = collapse;
        next.save(&self.path)?;
        self.saved = next;
        Ok(true)
    }

    pub fn save_recovery_days(&mut self, days: u16) -> Result<bool, String> {
        if !matches!(days, 7 | 30 | 90) {
            return Err("recovery retention must be 7, 30, or 90 days".into());
        }
        if self.saved.recovery_days == days {
            return Ok(false);
        }
        if let Some(why) = &self.unreadable {
            return Err(format!(
                "the preferences file could not be read ({why}); reset it in Appearance to save again"
            ));
        }
        let mut next = self.saved.clone();
        next.recovery_days = days;
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
mod tests;

#[cfg(test)]
mod catalog_fold_tests;
