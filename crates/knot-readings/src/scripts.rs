// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Listing `.rhai` readings in a directory the app owns.
//!
//! Read-only and non-recursive: no symlink following, no writes, and no
//! traversal past the named directory. A missing directory is an empty list,
//! not an error — the host may simply have no readings yet. Every skipped entry
//! is reported with its reason rather than silently dropped.

use crate::ReadingScript;
use std::path::Path;

/// List readings in `root`, sorted by file name. Returns the scripts it
/// accepted and a note for every entry it refused.
pub fn load_dir(
    root: &Path,
    max_scripts: usize,
    max_bytes: usize,
) -> (Vec<ReadingScript>, Vec<String>) {
    let mut notes = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return (Vec::new(), notes);
    };
    let mut candidates = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            notes.push("a directory entry could not be read".to_owned());
            continue;
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.extension().and_then(|value| value.to_str()) != Some("rhai") {
            continue;
        }
        // `symlink_metadata` does not follow links, so a link out of the
        // readings directory is refused rather than quietly followed.
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() => {
                notes.push(format!("{name}: not a regular file"));
                continue;
            },
            Ok(metadata) if metadata.len() > max_bytes as u64 => {
                notes.push(format!("{name}: {} bytes, over the limit", metadata.len()));
                continue;
            },
            Ok(_) => {},
            Err(error) => {
                notes.push(format!("{name}: {error}"));
                continue;
            },
        }
        candidates.push((name, path));
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    let mut scripts = Vec::new();
    for (name, path) in candidates {
        if scripts.len() >= max_scripts {
            notes.push(format!("{name}: beyond the {max_scripts} script limit"));
            continue;
        }
        match std::fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(source) => scripts.push(ReadingScript::new(name, source)),
                Err(_) => notes.push(format!("{name}: not UTF-8")),
            },
            Err(error) => notes.push(format!("{name}: {error}")),
        }
    }
    (scripts, notes)
}
