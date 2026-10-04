// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot on Mesquite's scenario lane, headless: the lane clicks the desktop's
//! Appearance control by role and label and asserts the snapshot moved. The
//! headed form of the same run is `scenarios/lane_smoke.scn`.

use cambium_genet_winit_host::{Harness, Init, WindowCommands};
use knot_desktop::{
    desktop_sheet, host_hooks,
    scenario::KnotLane,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use knot_file_catalog::KnotFileCatalog;
use mesquite::LaneConfig;
use tempfile::tempdir;

type Logic = fn(&DesktopState) -> DesktopView;

const SCENARIO: &str = "assert snap format == Djot
assert snap appearance_open == false
assert snap document_count == 1
assert snap reading_count == 0
assert snap graph_open == false
click role:button Appearance
settle 1
assert snap appearance_open == true
";

const STEP_9_SCENARIOS: &[&str] = &[
    "focused_writing.scn",
    "source_beside_preview.scn",
    "research_with_references.scn",
    "last_tab_graph.scn",
    "two_sites.scn",
];

fn scenario_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(name)
}

#[test]
fn composition_lane_refuses_retention_without_authority_and_preserves_source() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/composition/selection.djot");
    let before = std::fs::read(&fixture).unwrap();
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture.clone()),
    );
    state.set_preferences_path(Some(root.path().join("preferences.json")));
    let receipt =
        run_without_native_capture_at_height(state, "composition.scn", &root, 900.0, false);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
    assert!(!root.path().join("composition/collection.json").exists());
    assert_eq!(std::fs::read(fixture).unwrap(), before);
}

#[test]
fn every_step_9_scenario_uses_the_shared_parseable_lane() {
    for name in STEP_9_SCENARIOS {
        let path = scenario_path(name);
        let source = std::fs::read_to_string(&path).unwrap();
        taproot::Scenario::parse(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

fn run_without_native_capture(state: DesktopState, name: &str, temp: &tempfile::TempDir) -> String {
    run_without_native_capture_at_height(state, name, temp, 700.0, false)
}

fn run_without_native_capture_at_height(
    mut state: DesktopState,
    name: &str,
    temp: &tempfile::TempDir,
    height: f32,
    check_toolbar: bool,
) -> String {
    state.set_command_chrome(knot_desktop::workspace::CommandChrome::PlainRow);
    let source = std::fs::read_to_string(scenario_path(name)).unwrap();
    let source = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("capture "))
        .collect::<Vec<_>>()
        .join("\n");
    let scenario = temp.path().join(name);
    std::fs::write(&scenario, source).unwrap();
    let config = LaneConfig {
        scenario,
        capture_dir: None,
        receipt: Some(temp.path().join(format!("{name}.done"))),
    };
    let receipt = config.receipt_path().unwrap();
    let mut lane = mesquite::Lane::from_config(
        config,
        KnotLane::new(desktop_sheet()),
        cambium_genet_winit_host::read_file,
    )
    .unwrap();
    let mut hooks = host_hooks();
    hooks.after_frame = Box::new(move |ctx| lane.after_frame(ctx));
    let mut h = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as Logic,
            sheet: desktop_sheet(),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        hooks,
    );
    let wake = h.wake();
    h.update(|state| state.set_retention_targets(Vec::new(), wake));
    let toolbar = taproot::Selector::class("knot-workspace-toolbar");
    let mut toolbar_before = None;
    for _ in 0..1_000 {
        h.layout_at(1100.0, height);
        if check_toolbar && toolbar_before.is_none() {
            toolbar_before = h.resolve(&toolbar);
        }
        h.after_frame();
        h.drain_pointer();
        h.after_dispatch();
        h.process_wake();
        if h.close_requested() {
            break;
        }
    }
    let text = std::fs::read_to_string(&receipt).expect("the lane wrote its receipt");
    assert!(h.close_requested(), "{name} did not finish: {text}");
    if check_toolbar {
        let before = toolbar_before.expect("the toolbar starts laid out");
        let after = h.resolve(&toolbar).expect("the toolbar remains laid out");
        assert_eq!(
            after.1, before.1,
            "focusing the clipped source must not scroll the command toolbar"
        );
    }
    text
}

#[test]
fn revealing_a_clipped_focused_textbox_keeps_the_command_toolbar_stationary() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/field_notes.djot");
    let state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture),
    );
    let receipt =
        run_without_native_capture_at_height(state, "focused_writing.scn", &root, 420.0, true);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
}

#[test]
fn every_step_9_scenario_passes_through_the_real_desktop_controls() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/field_notes.djot");
    let readings = scenario_path("fixtures/readings");

    for name in ["focused_writing.scn", "source_beside_preview.scn"] {
        let state = DesktopState::with_path(
            KnotDocumentSession::open(&fixture).unwrap(),
            WindowCommands::new(),
            Some(fixture.clone()),
        );
        let receipt = run_without_native_capture(state, name, &root);
        assert!(receipt.starts_with("RESULT ok"), "{name}: {receipt}");
    }

    let mut research = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture.clone()),
    );
    research.set_readings_root(Some(readings));
    let receipt = run_without_native_capture(research, "research_with_references.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");

    let catalog = KnotFileCatalog::open(
        scenario_path("fixtures"),
        root.path().join("step-9-catalog.redb"),
    )
    .unwrap();
    let graph = DesktopState::with_catalog(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture),
        Some(catalog),
    );
    let receipt = run_without_native_capture(graph, "last_tab_graph.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");

    let first = scenario_path("fixtures/site-one");
    let second = scenario_path("fixtures/site-two");
    let mut sites = DesktopState::with_catalog(
        KnotDocumentSession::open(first.join("index.scroll")).unwrap(),
        WindowCommands::new(),
        Some(first),
        None,
    );
    sites.attach_site(&second);
    sites.open_behind(
        vec![KnotDocumentSession::open(second.join("index.scroll")).unwrap()],
        &[],
    );
    let receipt = run_without_native_capture(sites, "two_sites.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
}

#[test]
fn the_lane_drives_the_desktop_by_role_and_label() {
    let root = tempdir().unwrap();
    let document = root.path().join("field_notes.djot");
    std::fs::write(&document, "# Field notes\n\nOpening.\n").unwrap();
    let scenario = root.path().join("appearance.scn");
    std::fs::write(&scenario, SCENARIO).unwrap();
    let config = LaneConfig {
        scenario,
        capture_dir: Some(root.path().to_path_buf()),
        receipt: None,
    };
    let receipt = config.receipt_path().unwrap();
    let mut lane = mesquite::Lane::from_config(
        config,
        KnotLane::new(desktop_sheet()),
        cambium_genet_winit_host::read_file,
    )
    .unwrap();
    let mut hooks = host_hooks();
    hooks.after_frame = Box::new(move |ctx| lane.after_frame(ctx));
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&document).unwrap(),
        WindowCommands::new(),
        Some(document.clone()),
    );
    state.set_command_chrome(knot_desktop::workspace::CommandChrome::PlainRow);
    let mut h = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as Logic,
            sheet: desktop_sheet(),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        hooks,
    );
    for _ in 0..200 {
        h.layout_at(1100.0, 700.0);
        h.after_frame();
        h.drain_pointer();
        h.after_dispatch();
        if h.close_requested() {
            break;
        }
    }
    let text = std::fs::read_to_string(&receipt).expect("the lane wrote its receipt");
    assert!(text.starts_with("RESULT ok"), "{text}");
    assert!(
        h.close_requested(),
        "a finished lane asks the host to close"
    );
}
