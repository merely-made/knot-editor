// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Lua reading lane: the same reading over the same typed snapshot, run on
//! didgeridoo, the piccolo fork, behind [`ReadingBackend`].
//!
//! The sandbox is `script_didge::sandbox`: `base`, `coroutine`, `math`,
//! `string` and `table`, with no `io`, no `load`, no `require` and no `print`,
//! and a string limit set from [`ReadingBudget::max_note_bytes`]. The reading
//! surface is added to it (see [`crate::lua_bindings`]).
//!
//! A reading is a chunk that `return`s one item or a list of them. As in a
//! `lua eval` fence it is tried as a value first (`row("x")`) and as written
//! otherwise, and a compile error reports the line of the source as written.
//!
//! Bounds: `max_ops` is VM fuel; the string limit is the note limit; a list may
//! hold at most four times `max_rows` items and the state's heap may not pass
//! [`ReadingBudget::max_heap_bytes`].

use crate::backend::{BudgetSupport, ReadingBackend, ReadingBackendDescriptor};
use crate::lua_bindings::{self, item_of, kind_of};
use crate::{
    ReadingBudget, ReadingError, ReadingInput, ReadingItem, ReadingProvenanceV1, ReadingResult,
    ReadingScript, split_items,
};
use piccolo::meta_ops::MetaOperatorError;
use piccolo::thread::VMError;
use piccolo::{
    Closure, CompilerError, Context, DriveError, DriveLimits, Error, Executor, Lua,
    StashedExecutor, Table, Value,
};
use std::sync::Arc;
use std::time::Instant;

/// A reading's list may hold this many items for each row the budget allows.
const ITEMS_PER_ROW: usize = 4;

/// The name a reading's chunk carries in error messages.
const CHUNK_NAME: &str = "lua reading";

/// Put before the source to read it as a value, on the same line so error line
/// numbers stay the author's.
const RETURN_PREFIX: &str = "return ";

/// The error piccolo's string builders raise past the string limit.
const STRING_TOO_LARGE: &[u8] = b"resulting string too large";

/// Lua readings behind the typed dispatch seam.
#[derive(Default)]
pub struct LuaReadingBackend;

impl ReadingBackend for LuaReadingBackend {
    fn descriptor(&self) -> ReadingBackendDescriptor {
        ReadingBackendDescriptor {
            id: "knot.didge.reading",
            version: env!("CARGO_PKG_VERSION"),
            budget_support: BudgetSupport::NativeOperations,
        }
    }

    fn run(
        &mut self,
        script: &ReadingScript,
        input: &ReadingInput,
        budget: ReadingBudget,
    ) -> Result<ReadingResult, ReadingError> {
        run(script, input, budget)
    }
}

fn over_budget(message: String, budget: ReadingBudget) -> ReadingError {
    ReadingError::Budget { message, budget }
}

/// Run one Lua reading against one snapshot, bounded.
///
/// # Errors
///
/// A budget of 0 or an oversize source is refused before anything runs; then
/// compile, runtime, budget (fuel, heap, string or list limits) and result
/// shape errors.
pub fn run(
    script: &ReadingScript,
    input: &ReadingInput,
    budget: ReadingBudget,
) -> Result<ReadingResult, ReadingError> {
    if budget.max_ops == 0 {
        return Err(over_budget(
            "an operation budget of 0 is refused: a reading runs under a stated limit".to_owned(),
            budget,
        ));
    }
    if script.source.len() > budget.max_source_bytes {
        return Err(over_budget(
            format!(
                "script source is {} bytes, over the {} byte limit",
                script.source.len(),
                budget.max_source_bytes
            ),
            budget,
        ));
    }
    let input = Arc::new(input.clone());
    let started = Instant::now();
    let mut lua = script_didge::sandbox(budget.max_note_bytes);
    let executor = lua.enter(|ctx| start(ctx, &script.source, &input))?;
    let ops_used = drive(&mut lua, &executor, budget)?;
    let (rows, notes) = lua.enter(|ctx| finish(ctx, &executor, budget))?;
    Ok(ReadingResult {
        rows,
        notes,
        provenance: ReadingProvenanceV1 {
            source: input.source.clone(),
            script_name: script.name.clone(),
            script_hash: script.hash,
            ops_used,
            elapsed_micros: started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            budget,
        },
    })
}

fn compile_error(error: &CompilerError) -> ReadingError {
    let (message, line) = match error {
        CompilerError::Parsing(parse) => (parse.kind.to_string(), parse.line_number.0),
        CompilerError::Compilation(compile) => (compile.kind.to_string(), compile.line_number.0),
    };
    ReadingError::Compile {
        message,
        // The compiler counts lines from 0; a reading's line is the author's.
        line: Some(u16::try_from(line.saturating_add(1)).unwrap_or(u16::MAX)),
        column: None,
    }
}

/// Install the surface, compile the source and start it.
fn start(
    ctx: Context<'_>,
    source: &str,
    input: &Arc<ReadingInput>,
) -> Result<StashedExecutor, ReadingError> {
    lua_bindings::install(ctx, input).map_err(|error| ReadingError::Runtime {
        message: error.to_string(),
    })?;
    let as_value = format!("{RETURN_PREFIX}{source}");
    let closure = match Closure::load(ctx, Some(CHUNK_NAME), as_value.as_bytes()) {
        Ok(closure) => closure,
        // The error reported is the one for the source as written.
        Err(_) => Closure::load(ctx, Some(CHUNK_NAME), source.as_bytes())
            .map_err(|error| compile_error(&error))?,
    };
    Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
}

/// Run the executor under the budget with didgeridoo's bounded run. Returns
/// the fuel spent: `ops_used`.
fn drive(
    lua: &mut Lua,
    executor: &StashedExecutor,
    budget: ReadingBudget,
) -> Result<u64, ReadingError> {
    let limits = DriveLimits {
        max_ops: Some(budget.max_ops),
        max_heap_bytes: budget.max_heap_bytes,
    };
    piccolo::drive(lua, executor, limits).map_err(|error| match error {
        DriveError::OutOfFuel { max_ops } => over_budget(
            format!("the reading ran past its budget of {max_ops} operations"),
            budget,
        ),
        DriveError::OverHeap { max_heap_bytes } => over_budget(
            format!("the reading used more than {max_heap_bytes} bytes of memory"),
            budget,
        ),
        DriveError::Runtime(message) => ReadingError::Runtime { message },
    })
}

/// Only a limit the sandbox set is a budget failure: the string limit, in
/// either of the two forms piccolo raises it. Everything else is a runtime error.
fn classify(error: &Error<'_>, budget: ReadingBudget) -> ReadingError {
    let string_limit = match error {
        Error::Runtime(carried) => {
            matches!(
                carried.downcast::<VMError>(),
                Some(VMError::OperatorError(MetaOperatorError::ConcatOverflow))
            ) || matches!(
                carried.downcast::<MetaOperatorError>(),
                Some(MetaOperatorError::ConcatOverflow)
            )
        },
        Error::Lua(carried) => {
            matches!(carried.0, Value::String(text) if text.as_bytes() == STRING_TOO_LARGE)
        },
    };
    if string_limit {
        over_budget(
            format!(
                "a string passed the {} byte limit ({error})",
                budget.max_note_bytes
            ),
            budget,
        )
    } else {
        ReadingError::Runtime {
            message: error.to_string(),
        }
    }
}

/// Read the finished chunk's value as items and split it into rows and notes.
fn finish(
    ctx: Context<'_>,
    executor: &StashedExecutor,
    budget: ReadingBudget,
) -> Result<(Vec<crate::ReadingRowV1>, Vec<crate::ReadingNoteV1>), ReadingError> {
    let result = ctx.fetch(executor).take_result::<Value>(ctx);
    // A main chunk that yields leaves the executor suspended, not finished.
    let value = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => return Err(classify(&error, budget)),
        Err(mode) => {
            return Err(ReadingError::Runtime {
                message: mode.to_string(),
            });
        },
    };
    split_items(items_of(value, budget)?, budget)
}

fn shape(message: String) -> ReadingError {
    ReadingError::Shape { message }
}

fn items_of(value: Value<'_>, budget: ReadingBudget) -> Result<Vec<ReadingItem>, ReadingError> {
    if let Some(item) = item_of(value) {
        return Ok(vec![ReadingItem(item)]);
    }
    let Value::Table(table) = value else {
        return Err(shape(format!(
            "a reading is row(..)/note(..) or a list of them; the script returned a {}",
            kind_of(value)
        )));
    };
    list_items(table, budget)
}

/// A table is a list when its keys are exactly `1..=n`; an empty table is an
/// empty list; any other table is ambiguous and refused, never guessed.
fn list_items(table: Table<'_>, budget: ReadingBudget) -> Result<Vec<ReadingItem>, ReadingError> {
    let cap = budget.max_rows.saturating_mul(ITEMS_PER_ROW);
    let pairs: Vec<_> = table.iter().collect();
    if pairs.len() > cap {
        return Err(over_budget(
            format!(
                "the reading built a list of {} items, over the {cap} item limit",
                pairs.len()
            ),
            budget,
        ));
    }
    let mut slots: Vec<Option<Value<'_>>> = vec![None; pairs.len()];
    for (key, value) in pairs {
        let slot = match key {
            Value::Integer(key) => key
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| slots.get_mut(index)),
            _ => None,
        };
        let Some(slot) = slot else {
            return Err(shape(
                "a list's keys must be exactly 1..n; this table's are not, so it is refused"
                    .to_owned(),
            ));
        };
        *slot = Some(value);
    }
    slots
        .into_iter()
        .flatten()
        .map(|value| {
            item_of(value).map(ReadingItem).ok_or_else(|| {
                shape(format!(
                    "a reading is rows and notes; the list held a {}. Build items with row(..) or note(..).",
                    kind_of(value)
                ))
            })
        })
        .collect()
}
