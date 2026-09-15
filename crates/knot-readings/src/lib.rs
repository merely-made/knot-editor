// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The rhai *reading* lane: a script authors a reading over typed Knot
//! snapshots and returns rows, ranges and notes.
//!
//! This is the second rhai lane beside inker's fence evaluator, and it differs
//! from that one only by its binding set: read-only constructors and readers
//! over a [`ReadingInput`] the host filled. A reading proposes; it never
//! asserts. Nothing here writes, fetches, or opens a file, and the sandbox
//! receipts in `tests/reading_sandbox.rs` are the standing proof.
//!
//! A reading is derived state. Its [`ReadingProvenanceV1`] labels the source it
//! ran against, the script and its hash, and what the run cost; a source edit
//! makes it stale and rerunning rederives it.

mod bindings;
mod input;
pub mod links;
mod scripts;

pub use bindings::READING_SURFACE;
pub use input::{
    ReadingEndpointV1, ReadingInput, ReadingLinkV1, ReadingRelationV1, ReadingSourceIdV1,
};
pub use scripts::load_dir;

use script_rhai::rhai::{Array, Dynamic, EvalAltResult};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

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

/// Every bound a reading runs under. `max_ops` of `0` is refused rather than
/// honoured: rhai reads zero as *unlimited*, which is the opposite of a budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadingBudget {
    pub max_ops: u64,
    pub max_source_bytes: usize,
    pub max_rows: usize,
    pub max_note_bytes: usize,
}

impl Default for ReadingBudget {
    fn default() -> Self {
        Self {
            max_ops: 200_000,
            max_source_bytes: 64 * 1024,
            max_rows: 2_000,
            max_note_bytes: 64 * 1024,
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
/// and `note(..)`; the final expression is one item or an array of them.
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

/// Run one reading script against one snapshot, bounded.
///
/// Budget exhaustion is a receipt, never a hang: rhai's own operation cap
/// aborts the run and it arrives here as [`ReadingError::Budget`].
pub fn run(
    script: &ReadingScript,
    input: &ReadingInput,
    budget: ReadingBudget,
) -> Result<ReadingResult, ReadingError> {
    if budget.max_ops == 0 {
        return Err(ReadingError::Budget {
            message: "an operation budget of 0 is unlimited in rhai and is refused".to_owned(),
            budget,
        });
    }
    if script.source.len() > budget.max_source_bytes {
        return Err(ReadingError::Budget {
            message: format!(
                "script source is {} bytes, over the {} byte limit",
                script.source.len(),
                budget.max_source_bytes
            ),
            budget,
        });
    }
    let input = Arc::new(input.clone());
    let ops = Arc::new(AtomicU64::new(0));
    let engine = bindings::engine_for(Arc::clone(&input), budget, Arc::clone(&ops));
    let started = Instant::now();
    let ast = engine.compile(&script.source).map_err(|error| {
        let position = error.position();
        ReadingError::Compile {
            message: error.err_type().to_string(),
            line: position.line().map(|line| line as u16),
            column: position.position().map(|column| column as u16),
        }
    })?;
    let value = engine
        .eval_ast::<Dynamic>(&ast)
        .map_err(|error| classify(&error, budget))?;
    let (rows, notes) = coerce_items(value, budget)?;
    Ok(ReadingResult {
        rows,
        notes,
        provenance: ReadingProvenanceV1 {
            source: input.source.clone(),
            script_name: script.name.clone(),
            script_hash: script.hash,
            ops_used: ops.load(Ordering::Relaxed),
            elapsed_micros: started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            budget,
        },
    })
}

/// Only the engine's own resource ceilings are a budget failure. Everything
/// else — an unresolved call included — is an ordinary runtime error, so a
/// script reaching for a file binding reads as "no such function", not "spent".
fn classify(error: &EvalAltResult, budget: ReadingBudget) -> ReadingError {
    match error {
        EvalAltResult::ErrorTooManyOperations(..)
        | EvalAltResult::ErrorDataTooLarge(..)
        | EvalAltResult::ErrorTooManyModules(..) => ReadingError::Budget {
            message: error.to_string(),
            budget,
        },
        other => ReadingError::Runtime {
            message: other.to_string(),
        },
    }
}

fn coerce_items(
    value: Dynamic,
    budget: ReadingBudget,
) -> Result<(Vec<ReadingRowV1>, Vec<ReadingNoteV1>), ReadingError> {
    let items = if let Some(item) = value.clone().try_cast::<ReadingItem>() {
        vec![item]
    } else if let Some(array) = value.clone().try_cast::<Array>() {
        array
            .into_iter()
            .map(|element| {
                let kind = element.type_name();
                element.try_cast::<ReadingItem>().ok_or_else(|| {
                    ReadingError::Shape {
                        message: format!(
                            "a reading is rows and notes; the array held a {kind}. Build items with row(..) or note(..)."
                        ),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        return Err(ReadingError::Shape {
            message: format!(
                "a reading is row(..)/note(..) or an array of them; the script returned a {}",
                value.type_name()
            ),
        });
    };
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
