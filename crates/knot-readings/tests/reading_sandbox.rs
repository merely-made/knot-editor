// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Sandbox receipts. A reading gets the reading surface and nothing else: no
//! file, no fetch, no module import, no `eval`, no reach into the host's input.
//!
//! `Engine::new()` installs a `FileModuleResolver` on native targets, so the
//! import receipt below is the one that would actually have read a file off
//! disk had this lane not replaced the resolver.

use knot_document::KnotDocumentSession;
use knot_readings::{
    READING_SURFACE, ReadingBudget, ReadingError, ReadingInput, ReadingScript, run,
};

fn input() -> ReadingInput {
    ReadingInput::from_session(&KnotDocumentSession::read_only(
        "memory:sandbox",
        "# Sandbox\n\nOne [link](/target){rel=cites} here.\n",
    ))
    .unwrap()
}

fn evaluate(source: &str) -> Result<knot_readings::ReadingResult, ReadingError> {
    run(
        &ReadingScript::new("sandbox", source),
        &input(),
        ReadingBudget::default(),
    )
}

fn must_not_resolve(call: &str) {
    match evaluate(call) {
        Err(ReadingError::Runtime { message }) => assert!(
            message.to_lowercase().contains("function not found"),
            "{call} failed for the wrong reason: {message}"
        ),
        Err(ReadingError::Compile { message, .. }) => {
            assert!(!message.is_empty(), "{call} must not compile into anything")
        },
        other => panic!("{call} was not refused: {other:?}"),
    }
}

/// (11) The allowlist receipt: every name on the reading surface resolves, and
/// nothing resembling a host effect does.
///
/// Rhai's own `gen_fn_signatures` would enumerate the registered set directly,
/// but it is gated behind rhai's `metadata` feature, which script-rhai does not
/// enable and this lane must not turn on (it would pin rhai here and unify the
/// feature across every consumer of the shared engine). So the receipt is
/// behavioural: the surface is callable, and the effect vocabulary is not.
#[test]
fn the_reading_surface_is_the_whole_surface() {
    let calls = [
        ("document", r#"row(document().address)"#),
        ("outline", r#"row("" + outline().len())"#),
        ("sections", r#"row("" + sections().len())"#),
        ("folds", r#"row("" + folds().len())"#),
        ("links", r#"row("" + links().len())"#),
        ("relations", r#"row("" + relations().len())"#),
        ("span", r#"row("s", span(0, 1))"#),
        ("row", r#"row("r")"#),
        ("note", r#"note("n", "djot")"#),
        ("text_in", r#"row(document().text_in(span(0, 1)))"#),
    ];
    assert_eq!(calls.len(), READING_SURFACE.len());
    for (name, call) in calls {
        assert!(
            READING_SURFACE.contains(&name),
            "{name} is callable but not named on the surface"
        );
        assert!(
            evaluate(call).is_ok(),
            "{name} is on the surface but did not resolve"
        );
    }
    for name in READING_SURFACE {
        assert!(
            calls.iter().any(|(called, _)| *called == name),
            "{name} is named on the surface but unexercised"
        );
    }
}

/// (12) Nothing file-shaped exists to call.
#[test]
fn no_file_binding_exists() {
    for call in [
        r#"open_file("/etc/passwd")"#,
        r#"File("/etc/passwd")"#,
        r#"read_file("/etc/passwd")"#,
        r#"write_file("out.txt", "x")"#,
        r#"remove_file("out.txt")"#,
    ] {
        must_not_resolve(call);
    }
}

/// (13) `import` cannot reach a script that really is on disk. The plan named
/// a sibling in the process's cwd; an absolute path is the same receipt without
/// mutating a process-wide cwd that the other tests in this binary share, and
/// rhai's default `FileModuleResolver` resolves an absolute path just as
/// readily as a relative one.
#[test]
fn import_cannot_reach_a_script_on_disk() {
    let directory = tempfile::tempdir().unwrap();
    let sibling = directory.path().join("sibling.rhai");
    std::fs::write(&sibling, "fn borrowed() { 42 }").unwrap();
    assert!(sibling.exists(), "the sibling script really is on disk");
    let stem = sibling.with_extension("");
    let stem = stem.to_string_lossy().replace('\\', "/");
    for source in [
        format!(r#"import "{stem}" as m; row("" + m::borrowed())"#),
        r#"import "sibling" as m; row("" + m::borrowed())"#.to_owned(),
    ] {
        let outcome = evaluate(&source);
        assert!(
            matches!(
                outcome,
                Err(ReadingError::Compile { .. }) | Err(ReadingError::Runtime { .. })
            ),
            "import was not refused: {outcome:?}"
        );
    }
}

/// (13b) `eval` is disabled, so a script cannot assemble its own program.
#[test]
fn eval_is_disabled() {
    let outcome = evaluate(r#"eval("row(\"smuggled\")")"#);
    assert!(
        matches!(
            outcome,
            Err(ReadingError::Compile { .. }) | Err(ReadingError::Runtime { .. })
        ),
        "eval was not refused: {outcome:?}"
    );
}

/// (14) Nothing network-shaped exists to call.
#[test]
fn no_fetch_binding_exists() {
    for call in [
        r#"http_get("https://example.test/")"#,
        r#"fetch("https://example.test/")"#,
        r#"get("https://example.test/")"#,
        r#"request("https://example.test/")"#,
    ] {
        must_not_resolve(call);
    }
}

/// (15) `print` and `debug` reach nothing: they are silenced on the shared base
/// engine, and neither can put anything into a reading. (Stdout itself is not
/// observable from in here; what is asserted is that the result is untouched.)
#[test]
fn print_and_debug_reach_nothing() {
    let result = evaluate(r#"print("noise"); debug("noise"); row("only this")"#).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].label, "only this");
    assert!(result.notes.is_empty());
}

/// (16) A script gets copies. Nothing it does reaches the host's input, and the
/// readers have no setters to write through.
#[test]
fn a_script_cannot_mutate_the_host_input() {
    let before = input();
    let mutated = evaluate(
        r#"let t = document().text;
           t += " appended";
           row(document().text.len() + "|" + t.len())"#,
    )
    .unwrap();
    let parts = mutated.rows[0].label.clone();
    let (seen, local) = parts.split_once('|').unwrap();
    assert_ne!(seen, local, "the script edited only its own copy");
    assert_eq!(
        seen.parse::<usize>().unwrap(),
        before.text.chars().count(),
        "the document the script re-read is the host's, unchanged"
    );
    for write in [
        r#"let h = outline(); h[0].label = "rewritten"; row(h[0].label)"#,
        r#"let d = document(); d.text = "rewritten"; row(d.text)"#,
        r#"let l = links(); l[0].target = "elsewhere"; row(l[0].target)"#,
    ] {
        assert!(
            evaluate(write).is_err(),
            "a reader value accepted a write: {write}"
        );
    }
    assert_eq!(input(), before);
}
