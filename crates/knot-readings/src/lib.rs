// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Lua *reading* lane: a script authors a reading over typed Knot
//! snapshots and returns rows, ranges and notes.
//!
//! It runs on didgeridoo through `script-didge`'s sandbox, the same one as
//! inker's `lua eval` fences, and differs from that lane only by its binding
//! set: read-only constructors and readers over a [`ReadingInput`] the host
//! filled. A reading proposes; it never asserts. Nothing here writes, fetches,
//! or opens a file, and the sandbox receipts in `tests/reading_sandbox.rs` are
//! the standing proof.
//!
//! A reading is derived state. Its [`ReadingProvenanceV1`] labels the source it
//! ran against, the script and its hash, and what the run cost; a source edit
//! makes it stale and rerunning rederives it.

pub mod backend;
mod input;
pub mod links;
pub mod lua;
mod lua_bindings;
mod lua_host;
pub mod sound;
#[cfg(feature = "portable-scenes")]
pub mod metre_scene;
#[cfg(feature = "portable-scenes")]
pub mod metre_alignment;
mod scripts;

pub use lua_bindings::READING_SURFACE;
pub use input::{
    ReadingEndpointV1, ReadingInput, ReadingLinkV1, ReadingRelationV1, ReadingSourceIdV1,
};
pub use scripts::load_dir;

/// Note formats a reading may declare, matching inker's rendering set.
pub const NOTE_FORMATS: [&str; 5] = ["plain", "djot", "knot", "markdown", "gemtext"];

/// One named reading script and the blake3 hash of the exact source that ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingScript {
    pub name: String,
    pub source: String,
    pub hash: [u8; 32],
}

impl ReadingScript {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        let source = source.into();
        Self {
            name: name.into(),
            hash: blake3::hash(source.as_bytes()).into(),
            source,
        }
    }
    pub fn hash_hex(&self) -> String {
        hex32(&self.hash)
    }
}

pub(crate) fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Every bound a reading runs under. `max_ops` of `0` is refused: a reading
/// always runs on a budget. `max_heap_bytes` bounds the script's heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadingBudget {
    pub max_ops: u64,
    pub max_source_bytes: usize,
    pub max_rows: usize,
    pub max_note_bytes: usize,
    pub max_heap_bytes: usize,
}

impl Default for ReadingBudget {
    fn default() -> Self {
        Self {
            max_ops: 200_000,
            max_source_bytes: 64 * 1024,
            max_rows: 2_000,
            max_note_bytes: 64 * 1024,
            max_heap_bytes: 16 * 1024 * 1024,
        }
    }
}

/// One reading row: a label, optionally a source range the panel can select,
/// optionally another document the row points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingRowV1 {
    pub label: String,
    pub span: Option<(usize, usize)>,
    pub document: Option<String>,
}

/// Free text a reading produced, in one of [`NOTE_FORMATS`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingNoteV1 {
    pub format: String,
    pub text: String,
}

/// The labels that make a reading inspectable derived state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingProvenanceV1 {
    pub source: ReadingSourceIdV1,
    pub script_name: String,
    pub script_hash: [u8; 32],
    pub ops_used: u64,
    pub elapsed_micros: u64,
    pub budget: ReadingBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingResult {
    pub rows: Vec<ReadingRowV1>,
    pub notes: Vec<ReadingNoteV1>,
    pub provenance: ReadingProvenanceV1,
}

/// What a script returns: a row or a note. Scripts build these with `row(..)`
/// and `note(..)` and return one item or a list of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingItem(pub(crate) ReadingItemKind);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReadingItemKind {
    Row(ReadingRowV1),
    Note(ReadingNoteV1),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadingError {
    Compile {
        message: String,
        line: Option<u16>,
        column: Option<u16>,
    },
    Runtime {
        message: String,
    },
    Budget {
        message: String,
        budget: ReadingBudget,
    },
    /// The script ran but its final expression was not a reading.
    Shape {
        message: String,
    },
}

impl std::fmt::Display for ReadingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile {
                message,
                line: Some(line),
                ..
            } => write!(f, "Compile error (line {line}): {message}"),
            Self::Compile { message, .. } => write!(f, "Compile error: {message}"),
            Self::Runtime { message } => write!(f, "Runtime error: {message}"),
            Self::Budget { message, .. } => write!(f, "Budget: {message}"),
            Self::Shape { message } => write!(f, "Result: {message}"),
        }
    }
}

impl std::error::Error for ReadingError {}

/// Split a reading's items into rows and notes and hold them to the row and
/// note limits.
pub(crate) fn split_items(
    items: Vec<ReadingItem>,
    budget: ReadingBudget,
) -> Result<(Vec<ReadingRowV1>, Vec<ReadingNoteV1>), ReadingError> {
    let mut rows = Vec::new();
    let mut notes = Vec::new();
    for item in items {
        match item.0 {
            ReadingItemKind::Row(row) => rows.push(row),
            ReadingItemKind::Note(note) => notes.push(note),
        }
    }
    if rows.len() > budget.max_rows {
        return Err(ReadingError::Budget {
            message: format!(
                "the reading returned {} rows, over the {} row limit",
                rows.len(),
                budget.max_rows
            ),
            budget,
        });
    }
    if let Some(over) = notes
        .iter()
        .find(|note| note.text.len() > budget.max_note_bytes)
    {
        return Err(ReadingError::Budget {
            message: format!(
                "a note is {} bytes, over the {} byte limit",
                over.text.len(),
                budget.max_note_bytes
            ),
            budget,
        });
    }
    Ok((rows, notes))
}
