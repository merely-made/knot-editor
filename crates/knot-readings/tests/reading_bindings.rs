// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Receipts for the reading surface: every binding reads the snapshot it was
//! given, results carry valid spans and provenance, and every failure mode
//! arrives as a labelled error rather than a hang.

use knot_document::KnotDocumentSession;
use knot_readings::{
    ReadingBudget, ReadingEndpointV1, ReadingError, ReadingInput, ReadingRelationV1, ReadingScript,
    run,
};
use std::path::{Path, PathBuf};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixtures() -> PathBuf {
    PathBuf::from(FIXTURES)
}

fn field_notes() -> String {
    std::fs::read_to_string(fixtures().join("field_notes.djot")).unwrap()
}

fn input_from(source: &str) -> ReadingInput {
    ReadingInput::from_session(&KnotDocumentSession::read_only(
        "memory:field-notes",
        source,
    ))
    .unwrap()
}

fn fixture_script(name: &str) -> ReadingScript {
    let path = fixtures().join(name);
    ReadingScript::new(name, std::fs::read_to_string(path).unwrap())
}

fn inline(source: &str) -> ReadingScript {
    ReadingScript::new("inline", source)
}

fn notes(source: &str, input: &ReadingInput) -> Vec<String> {
    run(&inline(source), input, ReadingBudget::default())
        .unwrap()
        .notes
        .into_iter()
        .map(|note| note.text)
        .collect()
}

/// (1) Every reader hands back exactly what the host put in the input.
#[test]
fn every_binding_reads_the_snapshot_it_was_given() {
    let input = input_from(&field_notes());
    assert!(!input.outline.is_empty());
    assert!(!input.folds.is_empty());
    assert!(!input.links.is_empty());
    let counted = notes(
        r#"[
            note("outline=" + outline().len()),
            note("sections=" + sections().len()),
            note("folds=" + folds().len()),
            note("links=" + links().len()),
            note("relations=" + relations().len()),
            note("address=" + document().address),
            note("format=" + document().format),
            note("text=" + document().text.len()),
        ]"#,
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
            format!("text={}", input.text.chars().count()),
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
            r#"let r = relations()[0];
               [note(r.predicate + "|" + r.active + "|" + r.subject_start + "|" + r.object_start
                     + "|" + r.subject_document + "|" + r.object_document + "|" + r.id)]"#,
            &with_relations,
        ),
        vec![format!(
            "cites|true|10|-1|memory:field-notes|memory:other|{}",
            "11".repeat(32)
        )]
    );
}

/// (2) The headings-without-a-citation reading returns exactly the uncited
/// sections, in document order, with the outline items' own spans.
///
/// The expectation is recomputed here from the same input rather than written
/// down, because which links carry a `rel` is upstream's answer, not this
/// crate's — see `paragraph_link_predicates_survive_the_knot_preview`.
#[test]
fn headings_without_a_citation_returns_the_uncited_sections() {
    let source = field_notes();
    let input = input_from(&source);
    let result = run(
        &fixture_script("headings_without_a_citation.rhai"),
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
        &fixture_script("rows_with_spans.rhai"),
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
    let error = run(&fixture_script("runaway.rhai"), &input, budget).unwrap_err();
    assert!(
        matches!(error, ReadingError::Budget { .. }),
        "expected a budget error, got {error:?}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let after = run(&inline(r#"row("still here")"#), &input, budget).unwrap();
    assert_eq!(after.rows[0].label, "still here");
}

/// (5) Rhai reads a zero operation cap as unlimited, so the lane refuses it.
#[test]
fn a_zero_operation_budget_is_refused_not_unlimited() {
    let input = input_from(&field_notes());
    let budget = ReadingBudget {
        max_ops: 0,
        ..ReadingBudget::default()
    };
    let error = run(&fixture_script("runaway.rhai"), &input, budget).unwrap_err();
    match error {
        ReadingError::Budget { message, .. } => assert!(message.contains("unlimited")),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// (6) Compile and runtime failures are separable in the panel.
#[test]
fn compile_and_runtime_errors_are_separable() {
    let input = input_from(&field_notes());
    let compile = run(
        &inline("this is not valid rhai @@@"),
        &input,
        ReadingBudget::default(),
    )
    .unwrap_err();
    match compile {
        ReadingError::Compile { line, .. } => assert_eq!(line, Some(1)),
        other => panic!("expected a compile error, got {other:?}"),
    }
    let runtime = run(&inline(r#"span(-1, 4)"#), &input, ReadingBudget::default()).unwrap_err();
    assert!(
        matches!(runtime, ReadingError::Runtime { .. }),
        "expected a runtime error, got {runtime:?}"
    );
}

/// (7) A script whose final expression is not a reading is a shape error.
#[test]
fn a_non_reading_result_is_a_shape_error() {
    let input = input_from(&field_notes());
    for source in ["42", r#""a string""#, "[1, 2, 3]"] {
        let error = run(&inline(source), &input, ReadingBudget::default()).unwrap_err();
        assert!(
            matches!(error, ReadingError::Shape { .. }),
            "expected a shape error for {source}, got {error:?}"
        );
    }
}

/// (8) Every reading is labelled, and an edited source is a different source.
#[test]
fn provenance_labels_every_reading_and_tracks_the_source() {
    let source = field_notes();
    let input = input_from(&source);
    let script = fixture_script("rows_with_spans.rhai");
    let first = run(&script, &input, ReadingBudget::default()).unwrap();
    assert_eq!(first.provenance.script_name, "rows_with_spans.rhai");
    assert_eq!(first.provenance.script_hash, script.hash);
    assert_eq!(first.provenance.source, input.source);
    assert_eq!(first.provenance.budget, ReadingBudget::default());
    assert!(first.provenance.ops_used > 0);

    let edited = input_from(&format!("{source}\n## Added\n"));
    let second = run(&script, &edited, ReadingBudget::default()).unwrap();
    assert_ne!(second.provenance.source.hash, first.provenance.source.hash);
    assert_eq!(second.provenance.script_hash, first.provenance.script_hash);
}

/// (9) Inline links carry spans, and predicate links carry their rel, in
/// document order.
#[test]
fn links_carry_spans_and_rel_in_document_order() {
    let source = field_notes();
    let input = input_from(&source);
    let spanned = input
        .links
        .iter()
        .filter_map(|link| link.span.map(|span| (link, span)))
        .collect::<Vec<_>>();
    assert_eq!(spanned.len(), input.links.len(), "every link was placed");
    let targets = spanned
        .iter()
        .map(|(link, _)| link.target.as_str())
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
    let rels = spanned
        .iter()
        .map(|(link, _)| link.rel.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        rels,
        vec![None, Some("cites"), None, Some("cites")],
        "each rel-annotated link keeps its predicate, plain nav links have none"
    );
    for (link, (start, end)) in &spanned {
        assert!(source[*start..*end].starts_with(&format!("[{}]", link.text)));
        assert!(source[*start..*end].contains(&link.target));
    }
    // The reader hands the same rows to a script.
    assert_eq!(
        notes(
            r#"let out = [];
               for l in links() { out.push(note(l.target + "|" + l.rel + "|" + l.has_span)); }
               out"#,
            &input,
        )
        .len(),
        input.links.len()
    );
}

/// (9b) A paragraph link's `{rel=...}` predicate survives the knot preview,
/// the same as a heading link's.
///
/// nematic's knot expand pass (`rewrite_one_span` in `knot/expand.rs`) used to
/// rebuild every **paragraph** link with `predicate: None`, so
/// `inker::link_statements` reported nothing for a source that plainly carries
/// `{rel="cites"}` and the join in `links.rs` was fed nothing. Fixed upstream
/// in mere d075c6be; this test holds the fix in place.
#[test]
fn paragraph_link_predicates_survive_the_knot_preview() {
    let source = field_notes();
    assert!(source.contains(r#"{rel="cites"}"#));
    let input = input_from(&source);
    let cited = input
        .links
        .iter()
        .filter(|link| link.rel.as_deref() == Some("cites"))
        .map(|link| link.target.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        cited,
        vec!["https://example.test/gibson", "https://example.test/latour"],
        "every paragraph cites-annotated link reached the reader with its rel"
    );
    assert!(
        input.links.iter().all(|link| link.span.is_some()),
        "no link statement was left unplaced"
    );
}

/// (10) A section runs to the next heading at or above its own level.
#[test]
fn sections_nest_by_level() {
    let source = field_notes();
    let input = input_from(&source);
    let rows = run(
        &inline(
            r#"let out = [];
               for s in sections() { out.push(row(s.label + "|" + s.level, s.span)); }
               out"#,
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

/// (20, crate half) A missing readings directory is an empty list.
#[test]
fn load_dir_lists_sorted_scripts_and_refuses_nothing_silently() {
    let (missing, notes) = knot_readings::load_dir(Path::new("no/such/readings"), 64, 65_536);
    assert!(missing.is_empty() && notes.is_empty());

    let (scripts, notes) = knot_readings::load_dir(&fixtures(), 64, 65_536);
    assert_eq!(
        scripts.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec![
            "headings_without_a_citation.rhai",
            "rows_with_spans.rhai",
            "runaway.rhai"
        ],
        "sorted by name, and the .djot fixture is not a script"
    );
    assert!(notes.is_empty());

    let (capped, notes) = knot_readings::load_dir(&fixtures(), 1, 65_536);
    assert_eq!(capped.len(), 1);
    assert_eq!(notes.len(), 2, "every skipped script says why: {notes:?}");

    let (none, notes) = knot_readings::load_dir(&fixtures(), 64, 8);
    assert!(none.is_empty());
    assert_eq!(notes.len(), 3);
}
