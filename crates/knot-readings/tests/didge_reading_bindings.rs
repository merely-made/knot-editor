// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Lua reading lane's counterpart of `reading_bindings.rs`: every binding
//! reads the snapshot it was given, results carry valid spans and provenance,
//! and every failure arrives as a labelled error rather than a hang.
//!
//! Differences from the Rhai file, all by ruling (plan D4, D6, D7, D8):
//! a reading `return`s its value; lists are 1-based but byte offsets stay
//! 0-based; text joins with `..`; `#s` is bytes and `char_count(s)` is
//! characters; `end` is a Lua keyword, so a range's end is `["end"]` or `stop`.

use knot_document::KnotDocumentSession;
use knot_readings::backend::{BudgetSupport, ReadingBackend};
use knot_readings::didge::DidgeReadingBackend;
use knot_readings::{
    ReadingBudget, ReadingEndpointV1, ReadingError, ReadingInput, ReadingRelationV1, ReadingResult,
    ReadingScript,
};
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

fn fixture_script(name: &str) -> ReadingScript {
    let path = PathBuf::from(FIXTURES).join(name);
    ReadingScript::new(name, std::fs::read_to_string(path).unwrap())
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

#[test]
fn the_backend_names_itself_and_counts_fuel() {
    let backend: Box<dyn ReadingBackend> = Box::new(DidgeReadingBackend);
    let descriptor = backend.descriptor();
    assert_eq!(descriptor.id, "knot.didge.reading");
    assert_eq!(descriptor.budget_support, BudgetSupport::NativeOperations);
}

/// (1) Every reader hands back exactly what the host put in the input.
#[test]
fn every_binding_reads_the_snapshot_it_was_given() {
    let input = input_from(&field_notes());
    assert!(!input.outline.is_empty());
    assert!(!input.folds.is_empty());
    assert!(!input.links.is_empty());
    assert!(
        input.text.len() > input.text.chars().count(),
        "the fixture has multibyte text, so bytes and characters differ"
    );
    let counted = notes(
        r#"return {
            note("outline=" .. #outline()),
            note("sections=" .. #sections()),
            note("folds=" .. #folds()),
            note("links=" .. #links()),
            note("relations=" .. #relations()),
            note("address=" .. document().address),
            note("format=" .. document().format),
            note("text_bytes=" .. #document().text),
            note("text_chars=" .. char_count(document().text)),
        }"#,
        &input,
    );
    assert_eq!(
        counted,
        vec![
            format!("outline={}", input.outline.len()),
            format!("sections={}", input.outline.len()),
            format!("folds={}", input.folds.len()),
            format!("links={}", input.links.len()),
            "relations=0".to_owned(),
            "address=memory:field-notes".to_owned(),
            "format=djot".to_owned(),
            // D8: `#` is bytes, as spans are; the Rhai test's characters are
            // `char_count`.
            format!("text_bytes={}", input.text.len()),
            format!("text_chars={}", input.text.chars().count()),
        ]
    );

    let with_relations = input.clone().with_relations(vec![ReadingRelationV1 {
        id: [0x11; 32],
        author: [0x22; 32],
        predicate: "cites".to_owned(),
        subject: ReadingEndpointV1 {
            document_id: "memory:field-notes".to_owned(),
            quote: "Gibson".to_owned(),
            position: Some((10, 20)),
        },
        object: ReadingEndpointV1 {
            document_id: "memory:other".to_owned(),
            quote: String::new(),
            position: None,
        },
        active: true,
    }]);
    assert_eq!(
        notes(
            r#"local r = relations()[1]
               return {note(r.predicate .. "|" .. tostring(r.active) .. "|" .. r.subject_start
                     .. "|" .. r.object_start .. "|" .. r.subject_document .. "|"
                     .. r.object_document .. "|" .. r.id .. "|" .. r.subject_end
                     .. "|" .. r.object_end .. "|" .. r.author)}"#,
            &with_relations,
        ),
        vec![format!(
            "cites|true|10|-1|memory:field-notes|memory:other|{}|20|-1|{}",
            "11".repeat(32),
            "22".repeat(32)
        )]
    );
}

/// (2) The headings-without-a-citation reading returns exactly the uncited
/// sections, in document order, with the outline items' own spans.
#[test]
fn headings_without_a_citation_returns_the_uncited_sections() {
    let source = field_notes();
    let input = input_from(&source);
    let result = run(
        &fixture_script("headings_without_a_citation.lua"),
        &input,
        ReadingBudget::default(),
    )
    .unwrap();
    let labels = result
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    let expected = input
        .outline
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            let end = input.outline[index + 1..]
                .iter()
                .find(|later| later.level <= item.level)
                .map_or(source.len(), |later| later.start);
            !input.links.iter().any(|link| {
                link.rel.as_deref() == Some("cites")
                    && link
                        .span
                        .is_some_and(|(start, _)| start >= item.end && start < end)
            })
        })
        .map(|(_, item)| item.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, expected);
    assert!(
        labels.len() < input.outline.len() && !labels.is_empty(),
        "the fixture has both cited and uncited sections"
    );
    for row in &result.rows {
        let item = input
            .outline
            .iter()
            .find(|item| item.label == row.label)
            .unwrap();
        assert_eq!(row.span, Some((item.start, item.end)));
    }
}

/// (3) Rows carry char-boundary spans of the source they ran against, and an
/// undeclared note is plain.
#[test]
fn rows_carry_valid_spans_and_a_plain_trailing_note() {
    let source = field_notes();
    let input = input_from(&source);
    let result = run(
        &fixture_script("rows_with_spans.lua"),
        &input,
        ReadingBudget::default(),
    )
    .unwrap();
    assert_eq!(result.rows.len(), input.folds.len());
    for row in &result.rows {
        let (start, end) = row.span.unwrap();
        assert!(start <= end && end <= source.len());
        assert!(source.is_char_boundary(start) && source.is_char_boundary(end));
    }
    assert_eq!(result.notes.len(), 1);
    assert_eq!(result.notes[0].format, "plain");
    assert_eq!(
        result.notes[0].text,
        format!("folds: {}", input.folds.len())
    );
}

/// (4) A runaway is a budget receipt, promptly, and the lane still works after.
#[test]
fn a_runaway_is_a_budget_receipt_not_a_hang() {
    let input = input_from(&field_notes());
    let budget = ReadingBudget {
        max_ops: 10_000,
        ..ReadingBudget::default()
    };
    let started = std::time::Instant::now();
    let error = run(&fixture_script("runaway.lua"), &input, budget).unwrap_err();
    match &error {
        ReadingError::Budget { message, .. } => assert!(message.contains("10000"), "{message}"),
        other => panic!("expected a budget error, got {other:?}"),
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let after = run(&inline(r#"row("still here")"#), &input, budget).unwrap();
    assert_eq!(after.rows[0].label, "still here");
}

/// (5) A budget of 0 is refused: the lane never runs unbounded. The control is
/// the same script under a stated budget, which runs.
#[test]
fn a_zero_operation_budget_is_refused_not_unlimited() {
    let input = input_from(&field_notes());
    let zero = ReadingBudget {
        max_ops: 0,
        ..ReadingBudget::default()
    };
    for script in [fixture_script("runaway.lua"), inline(r#"row("ok")"#)] {
        match run(&script, &input, zero).unwrap_err() {
            ReadingError::Budget { message, .. } => assert!(message.contains('0'), "{message}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
    let one_thousand = ReadingBudget {
        max_ops: 1_000,
        ..ReadingBudget::default()
    };
    assert!(run(&inline(r#"row("ok")"#), &input, one_thousand).is_ok());
}

/// (6) Compile and runtime failures are separable in the panel, and a compile
/// error names its line.
#[test]
fn compile_and_runtime_errors_are_separable() {
    let input = input_from(&field_notes());
    let compile = run(
        &inline("this is not valid lua @@@"),
        &input,
        ReadingBudget::default(),
    )
    .unwrap_err();
    match compile {
        ReadingError::Compile { line, .. } => assert_eq!(line, Some(1)),
        other => panic!("expected a compile error, got {other:?}"),
    }
    // The line is the author's, though the source is first tried as a value.
    let third = run(
        &inline("local a = 1\nlocal b = 2\nlocal c = = 3\nreturn row('x')"),
        &input,
        ReadingBudget::default(),
    )
    .unwrap_err();
    match third {
        ReadingError::Compile { line, .. } => assert_eq!(line, Some(3)),
        other => panic!("expected a compile error, got {other:?}"),
    }
    for source in [
        r#"span(-1, 4)"#,
        "local x = nil\nreturn x + 1",
        "error('boom')",
    ] {
        let runtime = run(&inline(source), &input, ReadingBudget::default()).unwrap_err();
        assert!(
            matches!(runtime, ReadingError::Runtime { .. }),
            "{source}: expected a runtime error, got {runtime:?}"
        );
    }
}

/// (7) A script whose result is not a reading is a shape error. An ambiguous
/// table is refused, never guessed (D14).
#[test]
fn a_non_reading_result_is_a_shape_error() {
    let input = input_from(&field_notes());
    for source in [
        "42",
        r#""a string""#,
        "{1, 2, 3}",
        "nil",
        "outline()[1]",
        "{row('a'), x = row('b')}",
        "{[1] = row('a'), [3] = row('b')}",
        "{[0] = row('a'), row('b')}",
        "{x = row('a')}",
    ] {
        let error = run(&inline(source), &input, ReadingBudget::default()).unwrap_err();
        assert!(
            matches!(error, ReadingError::Shape { .. }),
            "expected a shape error for {source}, got {error:?}"
        );
    }
    // Controls: a list, an empty list, and a single item are readings.
    let list = run(
        &inline("{row('a'), note('b')}"),
        &input,
        ReadingBudget::default(),
    )
    .unwrap();
    assert_eq!((list.rows.len(), list.notes.len()), (1, 1));
    let empty = run(&inline("{}"), &input, ReadingBudget::default()).unwrap();
    assert!(empty.rows.is_empty() && empty.notes.is_empty());
    assert!(run(&inline("note('b')"), &input, ReadingBudget::default()).is_ok());
}

/// (8) Every reading is labelled, and an edited source is a different source.
#[test]
fn provenance_labels_every_reading_and_tracks_the_source() {
    let source = field_notes();
    let input = input_from(&source);
    let script = fixture_script("rows_with_spans.lua");
    let first = run(&script, &input, ReadingBudget::default()).unwrap();
    assert_eq!(first.provenance.script_name, "rows_with_spans.lua");
    assert_eq!(first.provenance.script_hash, script.hash);
    assert_eq!(first.provenance.source, input.source);
    assert_eq!(first.provenance.budget, ReadingBudget::default());
    assert!(first.provenance.ops_used > 0);

    let edited = input_from(&format!("{source}\n## Added\n"));
    let second = run(&script, &edited, ReadingBudget::default()).unwrap();
    assert_ne!(second.provenance.source.hash, first.provenance.source.hash);
    assert_eq!(second.provenance.script_hash, first.provenance.script_hash);

    // `ops_used` is fuel: more work reads as more, and never more than allowed.
    let small = run(&inline("row('x')"), &input, ReadingBudget::default()).unwrap();
    let big = run(
        &inline("local n = 0 for i = 1, 1000 do n = n + i end return row('x')"),
        &input,
        ReadingBudget::default(),
    )
    .unwrap();
    assert!(big.provenance.ops_used > small.provenance.ops_used + 1_000);
    assert!(big.provenance.ops_used <= ReadingBudget::default().max_ops);
}

/// (9) Inline links carry spans, and predicate links carry their rel, in
/// document order, through the Lua reader.
#[test]
fn links_carry_spans_and_rel_in_document_order() {
    let source = field_notes();
    let input = input_from(&source);
    let read = notes(
        r#"local out = {}
           for _, l in ipairs(links()) do
             out[#out + 1] = note(l.target .. "|" .. l.rel .. "|" .. tostring(l.has_span)
               .. "|" .. l.span.start .. "|" .. l.span["end"])
           end
           return out"#,
        &input,
    );
    let expected = input
        .links
        .iter()
        .map(|link| {
            let (start, end) = link.span.unwrap();
            format!(
                "{}|{}|true|{start}|{end}",
                link.target,
                link.rel.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(read, expected);
    let targets = read
        .iter()
        .map(|line| line.split('|').next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        vec![
            "/index.djot",
            "https://example.test/gibson",
            "/notes.djot",
            "https://example.test/latour",
        ],
        "the fenced code block is not scanned for links"
    );
}

/// (9b) A paragraph link's `{rel=...}` predicate survives the knot preview and
/// reaches a Lua script with its rel; a plain link reads as an empty rel.
#[test]
fn paragraph_link_predicates_reach_a_lua_reading() {
    let source = field_notes();
    assert!(source.contains(r#"{rel="cites"}"#));
    let input = input_from(&source);
    let cited = notes(
        r#"local out = {}
           for _, l in ipairs(links()) do
             if l.rel == "cites" then out[#out + 1] = note(l.target) end
           end
           return out"#,
        &input,
    );
    assert_eq!(
        cited,
        vec!["https://example.test/gibson", "https://example.test/latour"]
    );
    let unplaced = notes(
        r#"local n = 0
           for _, l in ipairs(links()) do if not l.has_span then n = n + 1 end end
           return note("unplaced=" .. n)"#,
        &input,
    );
    assert_eq!(unplaced, vec!["unplaced=0"]);
}

/// (10) A section runs to the next heading at or above its own level.
#[test]
fn sections_nest_by_level() {
    let source = field_notes();
    let input = input_from(&source);
    let rows = run(
        &inline(
            r#"local out = {}
               for _, s in ipairs(sections()) do
                 out[#out + 1] = row(s.label .. "|" .. s.level, s.span)
               end
               return out"#,
        ),
        &input,
        ReadingBudget::default(),
    )
    .unwrap()
    .rows;
    let span_of = |label: &str| {
        rows.iter()
            .find(|row| row.label.starts_with(&format!("{label}|")))
            .unwrap()
            .span
            .unwrap()
    };
    let top = span_of("Field notes");
    assert_eq!(
        top,
        (0, source.len()),
        "an only level-1 section is the document"
    );
    let quiet = span_of("Quiet section");
    let deep = span_of("Deep note");
    assert!(
        quiet.0 < deep.0 && deep.1 <= quiet.1,
        "a level-3 section nests inside its level-2 parent: {quiet:?} vs {deep:?}"
    );
    let greek = span_of("Σχόλια");
    assert_eq!(greek.1, source.len());
    assert!(source.is_char_boundary(greek.0));
}

/// (20, Lua half) `load_dir` lists `.rhai` readings only and that extension is
/// not this phase's to change (plan D22), so the listing test has no Lua
/// translation. What does carry over is that the Lua fixtures are not listed.
#[test]
fn lua_fixtures_are_not_listed_as_rhai_readings() {
    let (scripts, _) = knot_readings::load_dir(&PathBuf::from(FIXTURES), 64, 65_536);
    assert!(scripts.iter().all(|script| script.name.ends_with(".rhai")));
    assert_eq!(scripts.len(), 3);
}
