// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The limits a Lua reading runs under: the row, note and source limits the
//! Rhai lane sets, and what stands where its array and map ceilings stood.
//! The Rhai files hold no direct tests of these (they are enforced and
//! receipted only through the budget error), so these are new for both lanes
//! in kind and Lua-only in form.
//!
//! Each limit has a control: the same script one step inside the limit runs.

use knot_readings::backend::ReadingBackend;
use knot_readings::didge::DidgeReadingBackend;
use knot_readings::{ReadingBudget, ReadingError, ReadingInput, ReadingResult, ReadingScript};

fn field_notes() -> ReadingInput {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/field_notes.djot"
    ))
    .unwrap();
    ReadingInput::from_text("memory:limits", "djot", text)
}

fn run(source: &str, budget: ReadingBudget) -> Result<ReadingResult, ReadingError> {
    DidgeReadingBackend.run(
        &ReadingScript::new("limits", source),
        &field_notes(),
        budget,
    )
}

fn budget_message(source: &str, budget: ReadingBudget) -> String {
    match run(source, budget) {
        Err(ReadingError::Budget { message, .. }) => message,
        other => panic!("expected a budget error for {source}, got {other:?}"),
    }
}

fn roomy() -> ReadingBudget {
    ReadingBudget {
        max_ops: 100_000_000,
        ..ReadingBudget::default()
    }
}

/// The source limit is in bytes and is checked before anything runs.
#[test]
fn the_source_limit_is_bytes_and_checked_first() {
    let budget = ReadingBudget {
        max_source_bytes: 16,
        ..ReadingBudget::default()
    };
    let exact = "return row('x')  "; // 17 bytes
    let at_limit = "return row('x') "; // 16 bytes
    assert_eq!((exact.len(), at_limit.len()), (17, 16));
    assert!(run(at_limit, budget).is_ok());
    let message = budget_message(exact, budget);
    assert!(message.contains("17 bytes"), "{message}");
    // Bytes, not characters: five two-byte characters are ten bytes.
    let multibyte = format!("return row('{}')", "é".repeat(2));
    assert!(multibyte.len() > multibyte.chars().count());
    let tight = ReadingBudget {
        max_source_bytes: multibyte.chars().count(),
        ..ReadingBudget::default()
    };
    assert!(budget_message(&multibyte, tight).contains("bytes"));
    // Refused before it is read: not even a syntax error comes first.
    assert!(budget_message("@@@@@@@@@@@@@@@@@@@@", budget).contains("20 bytes"));
}

#[test]
fn the_row_limit_holds() {
    let budget = ReadingBudget {
        max_rows: 3,
        ..ReadingBudget::default()
    };
    let rows = |count: u32| {
        format!("local out = {{}} for i = 1, {count} do out[i] = row('r' .. i) end return out")
    };
    assert_eq!(run(&rows(3), budget).unwrap().rows.len(), 3);
    let message = budget_message(&rows(4), budget);
    assert!(
        message.contains("4 rows") && message.contains("row limit"),
        "{message}"
    );
}

/// Where Rhai's array ceiling (four items a row) stood: a list longer than that
/// is refused. Notes do not count as rows, so the control is a list at the
/// ceiling made of notes.
#[test]
fn the_list_ceiling_holds() {
    let budget = ReadingBudget {
        max_rows: 3,
        ..ReadingBudget::default()
    };
    let notes = |count: u32| {
        format!("local out = {{}} for i = 1, {count} do out[i] = note('n') end return out")
    };
    assert_eq!(run(&notes(12), budget).unwrap().notes.len(), 12);
    let message = budget_message(&notes(13), budget);
    assert!(
        message.contains("13 items") && message.contains("12 item"),
        "{message}"
    );
}

/// A note's text may be at most `max_note_bytes`. A script cannot build a longer
/// string (the string limit is the same number), and a longer string the host
/// supplied is still refused when it comes back as a note.
#[test]
fn the_note_limit_holds() {
    let budget = ReadingBudget {
        max_note_bytes: 64,
        ..ReadingBudget::default()
    };
    let doc_len = field_notes().text.len();
    assert!(doc_len > 64);
    assert!(run("return note(('x'):rep(64))", budget).is_ok());
    let built = budget_message("return note(('x'):rep(65))", budget);
    assert!(built.contains("64 byte"), "{built}");
    let supplied = budget_message("return note(document().text)", budget);
    assert!(supplied.contains(&format!("{doc_len} bytes")), "{supplied}");
    // The limit is on the text, so a row label is not a note.
    assert!(run("return row(document().text)", budget).is_ok());
}

/// The string limit is the note limit, and every way of building a long string
/// meets it as a budget error, one byte past the limit and not before.
#[test]
fn the_string_limit_is_the_note_limit() {
    let budget = ReadingBudget::default();
    let limit = budget.max_note_bytes;
    let at = |build: &str| format!("local s = {build} return row(tostring(#s))");
    for build in [
        format!("('x'):rep({limit})"),
        format!("('x'):rep({}) .. 'x'", limit - 1),
        format!("table.concat({{('x'):rep({}), 'x'}})", limit - 1),
        format!("string.format('%s', ('x'):rep({limit}))"),
    ] {
        let result = run(&at(&build), budget).unwrap();
        assert_eq!(result.rows[0].label, limit.to_string(), "{build}");
    }
    for build in [
        format!("('x'):rep({})", limit + 1),
        format!("('x'):rep({limit}) .. 'x'"),
        format!("table.concat({{('x'):rep({limit}), 'x'}})"),
        format!("string.format('%s%s', ('x'):rep({limit}), 'x')"),
        "(function() local s = 'x' for i = 1, 40 do s = s .. s end return s end)()".to_owned(),
    ] {
        let message = budget_message(&at(&build), budget);
        assert!(
            message.contains(&format!("{limit} byte")),
            "{build}: {message}"
        );
    }
    // A script's own error that says "too long" is a runtime error, not a budget.
    let error = run("error('too long')", budget).unwrap_err();
    assert!(matches!(error, ReadingError::Runtime { .. }), "{error:?}");
    // The limit follows the budget given, not a constant.
    let small = ReadingBudget {
        max_note_bytes: 8,
        ..budget
    };
    assert!(run("return row(('x'):rep(8))", small).is_ok());
    assert!(matches!(
        run("return row(('x'):rep(9))", small),
        Err(ReadingError::Budget { .. })
    ));
}

/// What stood behind Rhai's array and map ceilings was memory. Fuel bounds work
/// and the string limit bounds one string, but a table can still grow: the heap
/// is read between fuel steps and past `ReadingBudget::max_heap_bytes` the run is cut off.
#[test]
fn the_heap_ceiling_cuts_off_growth_that_fuel_would_allow() {
    let strings = "local t = {} for i = 1, 100000 do t[i] = ('x'):rep(60000) .. i end";
    let message = budget_message(strings, roomy());
    assert!(message.contains("memory"), "{message}");
    assert!(message.contains(&ReadingBudget::default().max_heap_bytes.to_string()), "{message}");
    let tables = "local t = {} for i = 1, 5000000 do t[i] = {i, i, i} end";
    assert!(budget_message(tables, roomy()).contains("memory"));

    // The ceiling is the budget's field: halve it and a smaller growth is cut.
    let half = ReadingBudget {
        max_heap_bytes: 1024 * 1024,
        ..roomy()
    };
    let grow = "local t = {} for i = 1, 100 do t[i] = ('x'):rep(60000) .. i end";
    assert!(budget_message(grow, half).contains("1048576"));
    assert!(run(&format!("{grow} return row('done')"), roomy()).is_ok());

    // Controls: garbage that is collected does not count, and honest work in
    // the same loops, a little smaller, runs.
    let garbage = "for i = 1, 20000 do local s = ('x'):rep(1000) .. i end return row('done')";
    assert_eq!(run(garbage, roomy()).unwrap().rows[0].label, "done");
    let small =
        "local t = {} for i = 1, 100 do t[i] = ('x'):rep(60000) .. i end return row('done')";
    assert_eq!(run(small, roomy()).unwrap().rows[0].label, "done");
    // A reading that returns the most rows the default budget allows is within
    // the heap.
    let many =
        "local out = {} for i = 1, 2000 do out[i] = row('row ' .. i, span(0, 1)) end return out";
    assert_eq!(
        run(many, ReadingBudget::default()).unwrap().rows.len(),
        2000
    );
}

/// Fuel is the VM's: a loop is cut off at the budget, `ops_used` stays within it,
/// and a budget of one still refuses a script that needs more.
#[test]
fn fuel_is_the_operation_budget() {
    let tight = ReadingBudget {
        max_ops: 50,
        ..ReadingBudget::default()
    };
    let work = "local n = 0 for i = 1, 100000 do n = n + i end return row('x')";
    let message = budget_message(work, tight);
    assert!(message.contains("50"), "{message}");
    let enough = ReadingBudget {
        max_ops: 10_000_000,
        ..ReadingBudget::default()
    };
    let used = run(work, enough).unwrap().provenance.ops_used;
    assert!((100_000..=10_000_000).contains(&used), "{used}");
    // The same script is refused just under what it used, and runs at it.
    let under = ReadingBudget {
        max_ops: used / 2,
        ..ReadingBudget::default()
    };
    assert!(matches!(run(work, under), Err(ReadingError::Budget { .. })));
}
