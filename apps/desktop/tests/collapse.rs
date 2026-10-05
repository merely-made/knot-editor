// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Independent retained-host receipts for the automatic give-way policy.

use cambium_genet_winit_host::{Harness, Init, KeyPress, NamedKey, WindowCommands};
use knot_desktop::{
    desktop_sheet,
    fonts::bundled_fonts,
    host_hooks,
    workspace::{CommandChrome, DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use layout_dom_api::LayoutDom;
use taproot::Selector;
use tempfile::tempdir;

type Host = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

fn settle(host: &mut Host, width: f32, height: f32) {
    host.layout_at(width, height);
    for _ in 0..4 {
        host.prepare_frame();
        host.relayout();
    }
}

fn launch(session: KnotDocumentSession) -> Host {
    let mut state = DesktopState::with_path(session, WindowCommands::new(), None);
    // These receipts activate the plain toolbar's reading buttons. Select
    // that supported chrome explicitly instead of inheriting the OS default.
    state.set_command_chrome(CommandChrome::PlainRow);
    let mut host = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: desktop_sheet(),
            fonts: bundled_fonts(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    settle(&mut host, 3000.0, 800.0);
    host
}

fn click(host: &mut Host, label: &str, width: f32) {
    assert!(
        host.click_on(&Selector::role("button").containing(label)),
        "missing {label}"
    );
    host.after_dispatch();
    settle(host, width, 800.0);
}

fn source() -> String {
    format!(
        "# Field\n\n{}\n{}\n{}\nEnd of paragraph.\n",
        "a".repeat(76),
        "b".repeat(76),
        "c".repeat(76)
    )
}

#[test]
fn resize_and_font_matrix_preserves_source_and_canonical_tree_and_restores_ratios() {
    let mut host = launch(KnotDocumentSession::scratch("scratch:field", &source()));
    click(&mut host, "Show Preview", 3000.0);
    click(&mut host, "Navigator", 3000.0);
    let canonical = host.state().docs.workspace().tiled().clone();
    let before = host.state().document().snapshot();
    for size in [12, 16, 24] {
        host.update(|state| state.appearance.font_size = size);
        for width in [
            320.0, 499.0, 500.0, 640.0, 699.0, 700.0, 900.0, 1100.0, 1280.0, 1600.0,
        ] {
            settle(&mut host, width, 800.0);
            assert_eq!(
                host.state().document().snapshot(),
                before,
                "{width}px/{size}px changed document"
            );
            assert_eq!(
                host.state().docs.workspace().tiled(),
                &canonical,
                "{width}px/{size}px changed tree"
            );
            let writing =
                host.with_dom(|dom| dom.all_with_class(dom.document(), "knot-writing-area"));
            assert_eq!(writing.len(), 1);
            let (x, _, w, _) = host.painted_rect(writing[0]).unwrap();
            assert!(
                x >= -0.5 && x + w <= width + 0.5,
                "writing outside viewport {width}: {x}/{w}"
            );
            // If there is room for source plus both rails and frame insets,
            // the measured writing area must fit 77ch plus its own34px inset.
            let text = 77.0 * f32::from(size) * 0.6;
            if width >= text + 58.0 + 56.0 + 40.0 + 2.0 {
                assert!(
                    w + 1.0 >= text + 34.0,
                    "lost source measure at {width}px/{size}px: {w}px < {}",
                    text + 34.0
                );
            }
            if width < 500.0 {
                assert!(
                    host.state()
                        .presentation
                        .stacks
                        .iter()
                        .filter_map(|s| s.collapsed.as_ref())
                        .all(|s| s.drawer.is_some())
                );
            }
        }
        // At 24px, the canonical source share needs more than3000px to fit
        // 77ch. A genuinely roomy viewport must restore it without squeezing.
        settle(&mut host, 4000.0, 800.0);
        assert!(
            host.state().presentation.splits.is_empty(),
            "wide resize should restore canonical fractions"
        );
        assert!(
            host.state()
                .presentation
                .stacks
                .iter()
                .all(|s| s.collapsed.is_none())
        );
    }
}

#[test]
fn narrow_menu_and_drawer_escape_preserve_preedit_selection_disk_and_focus() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("field.djot");
    let source = source();
    std::fs::write(&path, &source).unwrap();
    let mut host = launch(KnotDocumentSession::open(&path).unwrap());
    click(&mut host, "Show Preview", 3000.0);
    host.update(|state| {
        state
            .document_mut()
            .session_mut()
            .input_mut()
            .unwrap()
            .set_preedit("仮入力")
    });
    let before = host.state().document().snapshot();
    let disk = host.state().document().session().compare_disk().unwrap();
    let tree = host.state().docs.workspace().tiled().clone();
    settle(&mut host, 320.0, 800.0);
    assert_eq!(
        host.with_dom(
            |dom| taproot::matching(dom, &Selector::role("menuitem").containing("Menu")).len()
        ),
        1
    );
    let rails = host.with_dom(|dom| dom.all_with_class(dom.document(), "frisket-rail"));
    assert_eq!(rails.len(), 1);
    click(&mut host, "Readings", 320.0);
    let panels = host.with_dom(|dom| dom.all_with_class(dom.document(), "overlay-surface-panel"));
    assert_eq!(panels.len(), 1);
    let (x, _, w, _) = host.painted_rect(panels[0]).unwrap();
    assert!(
        x >= -0.5 && x + w <= 320.5,
        "drawer outside viewport: {x}/{w}"
    );
    host.press_key(&KeyPress::named(NamedKey::Escape));
    host.after_dispatch();
    settle(&mut host, 320.0, 800.0);
    assert!(host.with_dom(|dom| {
        dom.all_with_class(dom.document(), "overlay-surface-panel")
            .is_empty()
    }));
    assert_eq!(host.state().document().snapshot(), before);
    assert_eq!(
        host.state().document().session().input().preedit(),
        "仮入力"
    );
    assert_eq!(
        host.state().document().session().compare_disk().unwrap(),
        disk
    );
    assert_eq!(host.state().docs.workspace().tiled(), &tree);
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    let focused = host.runner().focus().expect("source focus restored");
    assert!(
        host.with_dom(|dom| taproot::matching(dom, &Selector::role("textbox")).contains(&focused)),
        "focus should return to document input"
    );
}

#[test]
fn four_times_zoom_uses_the_same_logical_reflow_policy() {
    let mut host = launch(KnotDocumentSession::scratch("scratch:zoom", &source()));
    click(&mut host, "Show Preview", 3000.0);
    host.set_ui_zoom(4.0);
    settle(&mut host, 1280.0, 700.0);
    assert!((host.logical_size().0 - 320.0).abs() < 0.5);
    assert!((host.logical_size().1 - 175.0).abs() < 0.5);
    let status = host.with_dom(|dom| dom.all_with_class(dom.document(), "status-bar"));
    assert_eq!(status.len(), 1);
    let (x, y, w, h) = host.painted_rect(status[0]).unwrap();
    assert!(
        x >= -0.5 && x + w <= 320.5 && y >= -0.5 && y + h <= 175.5,
        "status outside short zoomed viewport: ({x}, {y}, {w}, {h})"
    );
    let editors = host.with_dom(|dom| taproot::matching(dom, &Selector::role("textbox")));
    assert!(
        editors.iter().any(|node| host
            .visible_rect(*node)
            .is_some_and(|(_, y, w, h)| y >= 0.0 && y + h <= 175.5 && w > 100.0 && h >= 24.0)),
        "a readable source line must remain visible at 400% zoom"
    );
    assert!(
        host.state()
            .presentation
            .stacks
            .iter()
            .any(|s| s.collapsed.as_ref().is_some_and(|s| s.drawer.is_some()))
    );
}

#[test]
fn compact_menu_navigation_stays_bounded_and_activates_the_original_command() {
    let mut host = launch(KnotDocumentSession::scratch("scratch:menu", &source()));
    for width in [320.0, 640.0] {
        settle(&mut host, width, 700.0);
        for key in ["menu", "group.view"] {
            assert!(host.click_on(&Selector::role("menuitem").with_attr("data-key", key)));
            host.after_dispatch();
            settle(&mut host, width, 700.0);
        }
        let menus =
            host.with_dom(|dom| dom.all_with_class(dom.document(), "command-menu-bar-menu"));
        assert_eq!(menus.len(), 1, "compact navigation should show one level");
        for menu in menus {
            let (x, y, w, h) = host.painted_rect(menu).unwrap();
            assert!(
                x >= -0.5 && x + w <= width + 0.5 && y >= -0.5 && y + h <= 700.5,
                "menu outside {width}px viewport: ({x}, {y}, {w}, {h})"
            );
        }
        assert!(host.click_on(&Selector::role("menuitem").with_attr("data-key", "view.preview")));
        host.after_dispatch();
        settle(&mut host, width, 700.0);
        assert!(host.with_dom(|dom| {
            dom.all_with_class(dom.document(), "command-menu-bar-menu")
                .is_empty()
        }));
    }
}
