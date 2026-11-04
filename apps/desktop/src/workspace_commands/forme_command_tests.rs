// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium_genet_winit_host::WindowCommands;
use knot_document::KnotDocumentSession;

#[test]
fn forme_commands_route_the_shared_draft_and_acknowledged_local_save() {
    let temp = tempfile::tempdir().unwrap();
    let prefs = temp.path().join("preferences.json");
    let mut state = DesktopState::new(
        KnotDocumentSession::scratch("scratch:layout-test", "# Writer text"),
        WindowCommands::new(),
    );
    state.set_preferences_path(Some(prefs.clone()));
    let source_dirty = state.document().snapshot().dirty;
    state.activate_command(OUTLINE);
    state.activate_command(FORME_UNLOCK);
    assert!(state.docs.forme_editing());
    state.activate_command(FORME_DISCARD);
    assert!(!state.docs.forme_editing());
    state.activate_command(FORME_UNLOCK);
    assert!(state.docs.apply_layout(&workbench::WorkspaceEvent::Tile(
        workbench::TileEvent::DividerMoved {
            split: workbench::TilePath(vec![]),
            fractions: vec![0.6, 0.4],
        }
    )));
    state.activate_command(FORME_APPLY);
    assert!(!state.docs.forme_editing(), "{:?}", state.message);
    assert!(
        state
            .preferences
            .as_ref()
            .unwrap()
            .workspace_forme()
            .is_some()
    );
    assert_eq!(state.document().snapshot().dirty, source_dirty);
    assert!(
        state
            .message
            .as_ref()
            .unwrap()
            .contains("applied and saved")
    );
}
