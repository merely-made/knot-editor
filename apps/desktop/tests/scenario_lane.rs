// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot on the host's scenario lane, headless: the lane clicks the desktop's
//! Appearance control by role and label and asserts the snapshot moved. The
//! headed form of the same run is `scenarios/lane_smoke.scn`.

use cambium_genet_winit_host::{Harness, Init, LaneConfig, ScenarioLane, WindowCommands};
use knot_desktop::{
    desktop_sheet, host_hooks,
    scenario::KnotLane,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use tempfile::tempdir;

type Logic = fn(&DesktopState) -> DesktopView;

const SCENARIO: &str = "assert snap format == Djot
assert snap appearance_open == false
click role:button Appearance
settle 1
assert snap appearance_open == true
";

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
    let mut lane = ScenarioLane::new(config, KnotLane::new(desktop_sheet())).unwrap();
    let mut hooks = host_hooks();
    hooks.after_frame = Box::new(move |ctx| lane.drive(ctx));
    let state = DesktopState::with_path(
        KnotDocumentSession::open(&document).unwrap(),
        WindowCommands::new(),
        Some(document.clone()),
    );
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
