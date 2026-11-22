// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Lua reading surface beyond what the Rhai tests reach: every field of
//! every host type, span and constructor checks, and fuel through coroutines
//! and host readers. New for the Lua lane; each has a control in the same test.

use knot_document::KnotDocumentSession;
use knot_readings::backend::ReadingBackend;
use knot_readings::didge::DidgeReadingBackend;
use knot_readings::{ReadingBudget, ReadingError, ReadingInput, ReadingResult, ReadingScript};
use std::path::PathBuf;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn field_notes() -> String {
    std::fs::read_to_string(PathBuf::from(FIXTURES).join("field_notes.djot")).unwrap()
}

fn input_from(source: &str) -> ReadingInput {
    ReadingInput::from_session(&KnotDocumentSession::read_only(
        "memory:field-notes",
        source,
    ))
    .unwrap()
}

fn inline(source: &str) -> ReadingScript {
    ReadingScript::new("inline", source)
}

fn run(
    script: &ReadingScript,
    input: &ReadingInput,
    budget: ReadingBudget,
) -> Result<ReadingResult, ReadingError> {
    DidgeReadingBackend.run(script, input, budget)
}

fn notes(source: &str, input: &ReadingInput) -> Vec<String> {
    run(&inline(source), input, ReadingBudget::default())
        .unwrap()
        .notes
        .into_iter()
        .map(|note| note.text)
        .collect()
}

/// The fields of every host type that test (1) leaves unread.
#[test]
fn every_field_of_every_host_type_reads() {
    let input = input_from(&field_notes());
    let read = notes(
        r#"local h, s, f, l, d = outline()[1], sections()[1], folds()[1], links()[2], document()
           return {
             note(h.label .. "|" .. h.level .. "|" .. h.start .. "|" .. h["end"] .. "|" .. h.stop
                  .. "|" .. h.span.start .. "|" .. h.span["end"] .. "|" .. h.span.len),
             note(s.label .. "|" .. s.level .. "|" .. s.heading.start .. "|" .. s.body.start
                  .. "|" .. s.span.stop .. "|" .. s.span.len),
             note(f.kind .. "|" .. f.start .. "|" .. f["end"] .. "|" .. f.span.len),
             note(l.target .. "|" .. l.rel .. "|" .. l.text .. "|" .. tostring(l.has_span)
                  .. "|" .. l.span.start .. "|" .. l.span.len),
             note(d.selection_start .. "|" .. d.selection_end .. "|[" .. d.revision .. "]"),
           }"#,
        &input,
    );
    let heading = &input.outline[0];
    let fold = &input.folds[0];
    let link = &input.links[1];
    let (link_start, link_end) = link.span.unwrap();
    assert_eq!(
        read[0],
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            heading.label,
            heading.level,
            heading.start,
            heading.end,
            heading.end,
            heading.start,
            heading.end,
            heading.end - heading.start
        )
    );
    assert!(read[1].starts_with(&format!("{}|{}|", heading.label, heading.level)));
    assert!(read[2].contains(&format!("|{}|{}|", fold.start, fold.end)));
    assert_eq!(
        read[3],
        format!(
            "{}|{}|{}|true|{link_start}|{}",
            link.target,
            link.rel.clone().unwrap_or_default(),
            link.text,
            link_end - link_start
        )
    );
    // The selection is the session's (a caret); no revision reads as "".
    let (from, to) = input.selection.unwrap();
    assert_eq!(read[4], format!("{from}|{to}|[]"));
    let bare = ReadingInput::from_text("memory:bare", "djot", "x");
    assert_eq!(
        notes(
            r#"return note(document().selection_start .. "|" .. document().selection_end)"#,
            &bare
        ),
        vec!["-1|-1"],
        "no selection reads as -1, as in the Rhai lane"
    );
}

/// (4b) Fuel reaches into a coroutine and into the host's own work: neither is
/// a way around the budget. Not in the Rhai file; Lua has coroutines.
#[test]
fn fuel_reaches_coroutines_and_host_readers() {
    let input = input_from(&field_notes());
    let budget = ReadingBudget {
        max_ops: 20_000,
        ..ReadingBudget::default()
    };
    for source in [
        "local co = coroutine.create(function() while true do end end) coroutine.resume(co)",
        "while true do local _ = sections() end",
    ] {
        let error = run(&inline(source), &input, budget).unwrap_err();
        assert!(
            matches!(error, ReadingError::Budget { .. }),
            "{source}: {error:?}"
        );
    }
}

/// Spans: constructors check their range, byte offsets are 0-based and text
/// is copied out by byte range.
#[test]
fn spans_are_checked_byte_ranges() {
    let input = input_from("# Σ\n");
    let text = notes(
        r#"local d = document()
           return {
             note(d:text_in(span(0, 1))),
             note(d:text_in(span(2, 4))),
             note(d.text_in(d, span(0, #d.text))),
           }"#,
        &input,
    );
    assert_eq!(text, vec!["#", "Σ", "# Σ\n"]);
    for bad in [
        "span(4, 2)",
        "span(0, 99)",
        "span(3, 4)",
        "span(1.5, 2)",
        "span('a', 2)",
        "document():text_in(3)",
    ] {
        let error = run(&inline(bad), &input, ReadingBudget::default()).unwrap_err();
        assert!(
            matches!(error, ReadingError::Runtime { .. }),
            "{bad}: {error:?}"
        );
    }
    // Σ is bytes 2..4: a range that starts inside it is refused.
    assert!(
        run(
            &inline("row('r', span(2, 4))"),
            &input,
            ReadingBudget::default()
        )
        .is_ok()
    );
    assert!(
        run(
            &inline("row('r', span(3, 4))"),
            &input,
            ReadingBudget::default()
        )
        .is_err()
    );
}

/// Constructors take what they are given and refuse the rest.
#[test]
fn constructors_refuse_what_they_cannot_read() {
    let input = input_from("# A\n");
    let ok = run(
        &inline(r#"return {row("a"), row("b", span(0, 1)), row("c", nil), note("n", "gemtext")}"#),
        &input,
        ReadingBudget::default(),
    )
    .unwrap();
    assert_eq!(ok.rows[1].span, Some((0, 1)));
    assert_eq!(ok.rows[2].span, None);
    assert_eq!(ok.notes[0].format, "gemtext");
    for bad in [
        "row(5)",
        "row()",
        r#"row("a", 3)"#,
        r#"row("a", outline()[1])"#,
        "note(nil)",
        r#"note("a", "rtf")"#,
        r#"note("a", 3)"#,
        "document().nope",
        "outline()[1].nope",
        "span(0, 1).label",
        "row('a').label",
    ] {
        let error = run(&inline(bad), &input, ReadingBudget::default()).unwrap_err();
        assert!(
            matches!(error, ReadingError::Runtime { .. }),
            "{bad}: {error:?}"
        );
    }
}
