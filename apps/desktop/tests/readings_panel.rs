// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Windowless receipts for the Readings panel: list, run, select a source
//! range, go stale on an edit, and report a budget without hanging.

use cambium_genet_winit_host::{Harness, Init, WindowCommands};
use genet_scripted_dom::{NodeId, ScriptedDom};
use knot_desktop::{
    host_hooks,
    workspace::{DESKTOP_CSS, DesktopState, DesktopView, desktop_view},
};
use knot_document::{KNOT_DOCUMENT_CSS, KnotDocumentSession};
use layout_dom_api::LayoutDom;
use std::path::{Path, PathBuf};
use taproot::Selector;
use tempfile::{TempDir, tempdir};

const SOURCE: &str =
    "# Field notes\n\nOpening.\n\n## Citations\n\nA cited passage.\n\n## Σχόλια\n\nGreek notes.\n";

/// Every heading as a row carrying its own span, plus a rendered note.
const OUTLINE_ROWS: &str = r###"let out = [];
for h in outline() { out.push(row(h.label, h.span)); }
out.push(note("## Rendered note", "djot"));
out"###;

const RUNAWAY: &str = "let n = 0;\nloop { n += 1; }\n";

type DesktopHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

struct Fixture {
    _root: TempDir,
    document: PathBuf,
    harness: DesktopHarness,
}

fn text_content(dom: &ScriptedDom, node: NodeId) -> String {
    let own = dom.text(node).unwrap_or_default();
    let children = dom
        .dom_children(node)
        .map(|child| text_content(dom, child))
        .collect::<String>();
    format!("{own}{children}")
}

fn class_node(dom: &ScriptedDom, node: NodeId, class: &str) -> Option<NodeId> {
    if dom.has_class(node, class) {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| class_node(dom, child, class))
}

fn class_text(harness: &DesktopHarness, class: &str) -> Option<String> {
    let dom = harness.runner().dom();
    let dom = dom.borrow();
    class_node(&dom, dom.document(), class).map(|node| text_content(&dom, node))
}

fn panel_text(harness: &DesktopHarness) -> String {
    class_text(harness, "knot-readings").unwrap_or_default()
}

fn open(scripts: &[(&str, &str)], readings_root: Option<&Path>) -> Fixture {
    let root = tempdir().unwrap();
    let document = root.path().join("field_notes.djot");
    std::fs::write(&document, SOURCE).unwrap();
    let readings = root.path().join("readings");
    if !scripts.is_empty() {
        std::fs::create_dir(&readings).unwrap();
    }
    for (name, source) in scripts {
        std::fs::write(readings.join(name), source).unwrap();
    }
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&document).unwrap(),
        WindowCommands::new(),
        Some(document.clone()),
    );
    state.set_readings_root(Some(
        readings_root.map_or(readings, |explicit| explicit.to_path_buf()),
    ));
    let mut harness = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: format!(
                "{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}{}",
                knot_desktop::readings::CSS
            ),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    harness.layout_at(1100.0, 800.0);
    Fixture {
        _root: root,
        document,
        harness,
    }
}

fn show_panel(harness: &mut DesktopHarness) {
    assert!(
        harness.click_on(&Selector::role("button").with_attr("aria-controls", "knot-readings")),
        "the Readings toolbar toggle is missing"
    );
    harness.after_dispatch();
}

fn run_first_reading(harness: &mut DesktopHarness) {
    assert!(
        harness.click_on(&Selector::role("button").with_attr("data-reading-index", "0")),
        "no reading was listed"
    );
    harness.after_dispatch();
    assert!(
        harness.click_on(&Selector::role("button").containing("Run reading")),
        "Run is missing"
    );
    harness.after_dispatch();
}

/// (17) The panel lists a reading, runs it, and its rows select source ranges,
/// returning focus to the one editable source.
#[test]
fn a_reading_lists_runs_and_selects_its_source_range() {
    let mut fixture = open(&[("outline_rows.rhai", OUTLINE_ROWS)], None);
    show_panel(&mut fixture.harness);
    assert!(panel_text(&fixture.harness).contains("outline_rows.rhai"));
    run_first_reading(&mut fixture.harness);

    let provenance = class_text(&fixture.harness, "knot-readings-provenance").unwrap();
    assert!(provenance.contains("outline_rows.rhai"), "{provenance}");
    assert!(provenance.contains("3 rows"), "{provenance}");
    assert!(provenance.contains("ops"), "{provenance}");
    // The declared-format note rendered through the preview block renderer.
    assert!(
        panel_text(&fixture.harness).contains("Rendered note"),
        "the note did not render"
    );

    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").with_attr("data-reading-row", "2")),
        "the third reading row is missing"
    );
    fixture.harness.after_dispatch();
    let selection = fixture.harness.state().document.snapshot().selection;
    let selected = &SOURCE[selection.anchor.byte.min(selection.focus.byte)
        ..selection.anchor.byte.max(selection.focus.byte)];
    assert!(
        selected.contains("Σχόλια"),
        "the row selected {selected:?} rather than its own heading"
    );
    assert!(
        fixture.harness.focus().is_some(),
        "focus did not return to the source"
    );
    assert!(class_text(&fixture.harness, "knot-readings-error").is_none());
}

/// (18) A source edit makes the reading stale: it stays visible and says so,
/// its rows refuse to select, and a rerun rederives against the new source.
#[test]
fn a_source_edit_makes_a_reading_stale_until_it_is_rerun() {
    let mut fixture = open(&[("outline_rows.rhai", OUTLINE_ROWS)], None);
    show_panel(&mut fixture.harness);
    run_first_reading(&mut fixture.harness);
    let before = class_text(&fixture.harness, "knot-readings-provenance").unwrap();

    fixture.harness.update(|state| {
        state
            .document
            .session_mut()
            .input_mut()
            .unwrap()
            .insert_str("edited ");
    });
    fixture.harness.after_dispatch();
    assert!(
        class_text(&fixture.harness, "knot-readings-stale").is_some(),
        "the reading did not report itself stale"
    );
    assert!(
        class_text(&fixture.harness, "knot-readings-provenance").is_some(),
        "a stale reading is still shown, not closed"
    );

    let selection_before = fixture.harness.state().document.snapshot().selection;
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").with_attr("data-reading-row", "2")),
        "the stale row is still listed"
    );
    fixture.harness.after_dispatch();
    assert_eq!(
        fixture.harness.state().document.snapshot().selection,
        selection_before,
        "a stale row moved the selection"
    );
    let error = class_text(&fixture.harness, "knot-readings-error").unwrap();
    assert!(error.contains("stale"), "{error}");

    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").containing("Run reading"))
    );
    fixture.harness.after_dispatch();
    let after = class_text(&fixture.harness, "knot-readings-provenance").unwrap();
    assert_ne!(
        before, after,
        "the rerun did not rederive a new source hash"
    );
    assert!(class_text(&fixture.harness, "knot-readings-stale").is_none());
}

/// (19) A runaway is a budget line in the panel, promptly, and it cannot touch
/// the source it read.
#[test]
fn a_runaway_reading_reports_a_budget_without_a_hang() {
    let mut fixture = open(&[("runaway.rhai", RUNAWAY)], None);
    show_panel(&mut fixture.harness);
    let started = std::time::Instant::now();
    run_first_reading(&mut fixture.harness);
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    let error = class_text(&fixture.harness, "knot-readings-error").unwrap();
    assert!(error.starts_with("Budget:"), "{error}");
    assert!(class_text(&fixture.harness, "knot-readings-provenance").is_none());
    assert_eq!(std::fs::read_to_string(&fixture.document).unwrap(), SOURCE);
    assert_eq!(fixture.harness.state().document.snapshot().text, SOURCE);
}

/// (20) A missing readings directory is an empty list, not an error.
#[test]
fn a_missing_readings_directory_is_an_empty_list() {
    let absent = tempdir().unwrap();
    let absent = absent.path().join("nowhere");
    let mut fixture = open(&[], Some(&absent));
    show_panel(&mut fixture.harness);
    let panel = panel_text(&fixture.harness);
    assert!(panel.contains("No readings in this folder."), "{panel}");
    assert!(panel.contains("No reading has run yet."), "{panel}");
    assert!(class_text(&fixture.harness, "knot-readings-error").is_none());
}
