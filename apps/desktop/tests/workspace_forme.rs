// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The receiving host's real layout commands and shared session, without a window.
use cambium_genet_winit_host::{Harness, Init, WindowCommands};
use knot_desktop::{
    appearance::Appearance,
    desktop_sheet,
    fonts::bundled_fonts,
    host_hooks,
    preferences::PreferencesStore,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use taproot::Selector;
type Logic = fn(&DesktopState) -> DesktopView;
type Host = Harness<DesktopState, Logic, DesktopView>;
fn frame(host: &mut Host) {
    host.layout_at(1800., 800.);
    host.prepare_frame();
    host.layout_at(1800., 800.);
}
fn command(host: &mut Host, id: &str) {
    assert!(host.click_on(&Selector::role("menuitem").with_attr("data-key", "group.view")));
    frame(host);
    assert!(
        host.click_on(&Selector::role("menuitem").with_attr("data-key", id)),
        "{id}"
    );
    frame(host);
}
#[test]
fn menu_unlock_resize_discard_and_apply_use_the_shared_session() {
    let temp = tempfile::tempdir().unwrap();
    let prefs = temp.path().join("preferences.json");
    let mut state = DesktopState::new(
        KnotDocumentSession::scratch("scratch:forme", "# Writer\n\nSource stays here.\n"),
        WindowCommands::new(),
    );
    state.set_preferences_path(Some(prefs.clone()));
    state.set_command_chrome(knot_desktop::workspace::CommandChrome::ClientTitlebar);
    let mut host = Harness::new(desktop_sheet(), state, desktop_view as Logic);
    frame(&mut host);
    command(&mut host, "view.preview");
    let original = host.state().docs.workspace().tiled().clone();
    let divider_before = host.resolve(&Selector::role("separator")).unwrap().0;
    let text = host.state().document().snapshot().text;
    command(&mut host, "workspace.unlock");
    assert!(host.state().docs.forme_editing());
    command(&mut host, "workspace.first-more");
    assert!(host.resolve(&Selector::role("separator")).unwrap().0 > divider_before + 10.);
    assert_ne!(host.state().docs.workspace().tiled(), &original);
    command(&mut host, "workspace.discard");
    assert!((host.resolve(&Selector::role("separator")).unwrap().0 - divider_before).abs() < 1.);
    assert_eq!(host.state().docs.workspace().tiled(), &original);
    command(&mut host, "workspace.unlock");
    command(&mut host, "workspace.first-less");
    let preview = host.state().docs.workspace().tiled().clone();
    assert_ne!(preview, original);
    command(&mut host, "workspace.apply");
    assert!(!host.state().docs.forme_editing());
    assert_eq!(host.state().docs.workspace().tiled(), &preview);
    assert!(PreferencesStore::open(prefs).workspace_forme().is_some());
    assert_eq!(host.state().document().snapshot().text, text);
}

fn responsive_frame(host: &mut Host, width: f32) {
    host.layout_at(width, 800.0);
    for _ in 0..4 {
        host.prepare_frame();
        host.relayout();
    }
}

fn responsive_command(host: &mut Host, id: &str) {
    let width = 3000.0;
    for key in ["group.view", id] {
        assert!(host.click_on(&Selector::role("menuitem").with_attr("data-key", key)));
        host.after_dispatch();
        responsive_frame(host, width);
    }
}

#[test]
fn saved_forme_survives_compact_presentation_and_recovers_its_rendered_wide_divider() {
    let temp = tempfile::tempdir().unwrap();
    let prefs = temp.path().join("preferences.json");
    let mut state = DesktopState::new(
        KnotDocumentSession::scratch(
            "scratch:responsive-forme",
            "# Writer\n\nSource stays here.\n",
        ),
        WindowCommands::new(),
    );
    state.set_preferences_path(Some(prefs.clone()));
    state.set_command_chrome(knot_desktop::workspace::CommandChrome::ClientTitlebar);
    assert_eq!(state.appearance, Appearance::default());
    let mut host = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as Logic,
            sheet: desktop_sheet(),
            fonts: bundled_fonts(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    responsive_frame(&mut host, 3000.0);
    responsive_command(&mut host, "view.preview");
    let original_divider = host.resolve(&Selector::role("separator")).unwrap().0;
    responsive_command(&mut host, "workspace.unlock");
    responsive_command(&mut host, "workspace.first-less");
    responsive_command(&mut host, "workspace.apply");
    assert!(!host.state().docs.forme_editing());
    let canonical = host.state().docs.workspace().tiled().clone();
    let share = canonical
        .fractions_at(&workbench::TilePath(vec![]))
        .unwrap()[0];
    assert!((share - 0.45).abs() < 0.0001);
    assert!(host.state().presentation.splits.is_empty());
    let saved_divider = host.resolve(&Selector::role("separator")).unwrap().0;
    assert!(saved_divider < original_divider - 50.0);
    let saved_record = PreferencesStore::open(prefs.clone())
        .workspace_forme()
        .unwrap();
    let source = host.state().document().snapshot();

    responsive_frame(&mut host, 320.0);
    assert!(!host.state().presentation.splits.is_empty());
    assert!(host.state().presentation.stacks.iter().any(|stack| {
        stack
            .collapsed
            .as_ref()
            .is_some_and(|collapsed| collapsed.drawer.is_some())
    }));
    let projection = host.a11y_projection();
    let editors = host.with_dom(|dom| {
        taproot::matching_with_projection(
            dom,
            &Selector::role("textbox").with_attr("aria-label", "Document text"),
            &projection,
        )
    });
    assert!(
        editors.iter().any(|node| {
            host.visible_rect(*node)
                .is_some_and(|(x, y, width, height)| {
                    x >= -0.5
                        && x + width <= 320.5
                        && y >= -0.5
                        && y + height <= 800.5
                        && width > 100.0
                        && height >= 24.0
                })
        }),
        "compact presentation must retain a visible source editor"
    );
    assert_eq!(host.state().docs.workspace().tiled(), &canonical);
    assert_eq!(host.state().document().snapshot(), source);
    assert_eq!(
        PreferencesStore::open(prefs.clone()).workspace_forme(),
        Some(saved_record.clone())
    );

    responsive_frame(&mut host, 3000.0);
    assert!(host.state().presentation.splits.is_empty());
    assert!(
        host.state()
            .presentation
            .stacks
            .iter()
            .all(|stack| stack.collapsed.is_none())
    );
    let recovered_divider = host.resolve(&Selector::role("separator")).unwrap().0;
    assert!((recovered_divider - saved_divider).abs() < 1.0);
    assert_eq!(host.state().docs.workspace().tiled(), &canonical);
    assert_eq!(host.state().document().snapshot(), source);
    assert_eq!(host.state().appearance, Appearance::default());
    assert_eq!(
        PreferencesStore::open(prefs).workspace_forme(),
        Some(saved_record)
    );
}
