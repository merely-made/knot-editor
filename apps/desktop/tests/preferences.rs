// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Windowless receipts for the desktop preferences file, driven through the
//! real Appearance buttons: a missing file, a malformed one, and a relaunch.

use cambium_genet_winit_host::{Harness, Init, WindowCommands};
use genet_scripted_dom::{NodeId, ScriptedDom};
use knot_desktop::{
    appearance::{Appearance, appearance_css},
    host_hooks,
    preferences::{DesktopPreferences, PREFERENCES_FILE},
    workspace::{DESKTOP_CSS, DesktopState, DesktopView, desktop_view},
};
use knot_document::{KNOT_DOCUMENT_CSS, KnotDocumentSession};
use layout_dom_api::LayoutDom;
use std::path::Path;
use taproot::Selector;
use tempfile::tempdir;

type DesktopHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

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

/// One launch of the desktop against a preferences file, as `lib.rs` wires it.
fn launch(preferences: &Path) -> DesktopHarness {
    let mut state = DesktopState::with_path(
        KnotDocumentSession::scratch("scratch:untitled", "# Draft\n"),
        WindowCommands::new(),
        None,
    );
    state.set_preferences_path(Some(preferences.to_path_buf()));
    let mut harness = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: format!("{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}{}", appearance_css()),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    harness.layout_at(1100.0, 800.0);
    harness
}

fn click(harness: &mut DesktopHarness, label: &str) {
    assert!(
        harness.click_on(&Selector::role("button").containing(label)),
        "no {label:?} button"
    );
    harness.after_dispatch();
}

#[test]
fn a_missing_preferences_file_launches_with_defaults_and_writes_nothing() {
    let root = tempdir().unwrap();
    let settings = root.path().join("Knot");
    let path = settings.join(PREFERENCES_FILE);
    let mut harness = launch(&path);
    assert_eq!(harness.state().appearance, Appearance::default());
    assert_eq!(harness.state().message, None);
    click(&mut harness, "Appearance");
    // Already light: not a change, so still nothing on disk.
    click(&mut harness, "Light");
    assert!(!path.exists());
    assert!(!settings.exists());
    // Positive control: a real change does write, so the absence above is not
    // an instrument that can never see a file.
    click(&mut harness, "Dark");
    assert!(DesktopPreferences::load(&path).unwrap().appearance.dark);
}

#[test]
fn a_malformed_preferences_file_uses_defaults_says_why_and_is_not_overwritten() {
    let root = tempdir().unwrap();
    let path = root.path().join(PREFERENCES_FILE);
    let malformed = "{ \"appearance\": { \"dark\": yes } }\n";
    std::fs::write(&path, malformed).unwrap();
    let mut harness = launch(&path);
    assert_eq!(harness.state().appearance, Appearance::default());
    let message = class_text(&harness, "knot-workspace-message").unwrap();
    assert!(message.contains("could not be read"), "{message}");
    assert!(message.contains(PREFERENCES_FILE), "{message}");

    click(&mut harness, "Appearance");
    click(&mut harness, "Dark");
    assert!(harness.state().appearance.dark, "the session still changes");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), malformed);
    let refused = harness.state().message.clone().unwrap();
    assert!(refused.starts_with("Appearance not saved:"), "{refused}");
    let note = class_text(&harness, "knot-preferences-error").unwrap();
    assert!(note.contains("could not be read"), "{note}");

    // Only the explicit reset overwrites, and saving resumes after it.
    click(&mut harness, "Reset preferences file");
    assert!(
        harness
            .state()
            .preferences()
            .unwrap()
            .unreadable()
            .is_none()
    );
    assert!(DesktopPreferences::load(&path).unwrap().appearance.dark);
    click(&mut harness, "Wide");
    let saved = DesktopPreferences::load(&path).unwrap().appearance;
    assert!(saved.dark && saved.wide);
}

#[test]
fn an_appearance_change_persists_across_a_relaunch() {
    let root = tempdir().unwrap();
    let path = root.path().join("Knot").join(PREFERENCES_FILE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let embedding = serde_json::json!({
        "provider": "bert-wgpu",
        "weights": { "path": "C:/models/bge-micro-v2" }
    });
    std::fs::write(
        &path,
        serde_json::json!({ "version": 1, "embedding": embedding }).to_string(),
    )
    .unwrap();

    let mut first = launch(&path);
    click(&mut first, "Appearance");
    for label in ["Dark", "Highlight", "Larger", "Larger", "Wide", "Compact"] {
        click(&mut first, label);
    }
    let chosen = first.state().appearance.clone();
    assert_ne!(chosen, Appearance::default());
    drop(first);

    let relaunched = launch(&path);
    assert_eq!(relaunched.state().appearance, chosen);
    assert_eq!(relaunched.state().appearance.font_size, 18);
    assert!(class_text(&relaunched, "knot-theme-dark").is_some());
    assert_eq!(
        relaunched
            .state()
            .preferences()
            .unwrap()
            .preferences()
            .embedding,
        Some(embedding)
    );
}
