// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Exercise Knot's actual platform projections through the host. Window verbs
//! are recorded here; this is not a Windows/Linux window-manager receipt.

use cambium_genet_winit_host::{
    AppRegion, Harness, Init, KeyPress, Modifiers, NamedKey, WindowCommand, WindowCommands,
};
use knot_desktop::{
    desktop_sheet, host_hooks,
    workspace::{CommandChrome, DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use taproot::Selector;

type Logic = fn(&DesktopState) -> DesktopView;
type DesktopHarness = Harness<DesktopState, Logic, DesktopView>;

fn state(commands: WindowCommands, chrome: CommandChrome) -> DesktopState {
    let mut state = DesktopState::with_path(
        KnotDocumentSession::scratch("scratch:commands", "# Commands\n\nOriginal text.\n"),
        commands,
        None,
    );
    state.set_command_chrome(chrome);
    state
}

fn harness(chrome: CommandChrome, width: f32) -> DesktopHarness {
    harness_at(chrome, width, 800.0)
}

fn harness_at(chrome: CommandChrome, width: f32, height: f32) -> DesktopHarness {
    let mut h = Harness::with_hooks(
        Init {
            state: state(WindowCommands::new(), chrome),
            logic: desktop_view as Logic,
            sheet: desktop_sheet(),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    // Wire the same queue the real host supplies at launch, rather than an
    // orphan queue that would make caption tests pass without enacting verbs.
    let commands = h.commands();
    h.update(|current| *current = state(commands, chrome));
    h.layout_at(width, height);
    h.prepare_frame();
    h.layout_at(width, height);
    h
}

fn button(label: &str) -> Selector {
    Selector::role("button").with_attr("aria-label", label)
}

fn node(h: &DesktopHarness, selector: &Selector) -> genet_scripted_dom::NodeId {
    h.with_dom(|dom| taproot::matching(dom, selector).into_iter().next())
        .expect("matching DOM node")
}

fn palette_shortcut() -> KeyPress {
    KeyPress::character("p").with_modifiers(Modifiers {
        meta: cfg!(target_os = "macos"),
        ctrl: !cfg!(target_os = "macos"),
        shift: true,
        ..Modifiers::NONE
    })
}

#[test]
fn plain_row_has_palette_but_no_client_caption_controls() {
    let h = harness(CommandChrome::PlainRow, 1100.0);
    assert!(
        h.resolve(&Selector::class("knot-client-titlebar-title"))
            .is_none()
    );
    for label in ["Minimize window", "Maximize window", "Close window"] {
        assert!(h.resolve(&button(label)).is_none(), "unexpected {label}");
    }
    assert!(
        h.resolve(&Selector::role("button").containing("Commands"))
            .is_some()
    );
}

#[test]
fn client_caption_controls_queue_verbs_without_dragging() {
    for (label, verb) in [
        ("Minimize window", WindowCommand::Minimize),
        ("Maximize window", WindowCommand::ToggleMaximize),
    ] {
        let mut h = harness(CommandChrome::ClientTitlebar, 1400.0);
        let (x, y) = h
            .resolve(&button(label))
            .expect("caption button is visible");
        assert_eq!(h.app_region_at(x, y), AppRegion::NoDrag);
        h.click_at(x, y);
        assert_eq!(h.performed(), &[verb]);
    }
}

#[test]
fn title_is_draggable_but_menu_and_document_are_not() {
    let h = harness(CommandChrome::ClientTitlebar, 1400.0);
    let title = h
        .resolve(&Selector::class("knot-client-titlebar-title"))
        .expect("title");
    assert_eq!(h.app_region_at(title.0, title.1), AppRegion::Drag);
    let menu = h.resolve(&Selector::role("menubar")).expect("menubar");
    assert_eq!(h.app_region_at(menu.0, menu.1), AppRegion::NoDrag);
    let source = h
        .resolve(&Selector::role("textbox").with_attr("aria-label", "Document text"))
        .expect("source");
    assert_eq!(h.app_region_at(source.0, source.1), AppRegion::NoDrag);
}

#[test]
fn dirty_caption_close_uses_the_existing_unsaved_guard() {
    let mut h = harness(CommandChrome::ClientTitlebar, 1400.0);
    assert!(h.click_on(&Selector::role("textbox").with_attr("aria-label", "Document text")));
    h.key_char("changed");
    assert!(h.state().document().snapshot().dirty);
    assert!(h.click_on(&button("Close window")));
    assert!(
        !h.close_requested(),
        "a caption click must not discard edits"
    );
    assert!(
        h.resolve(&Selector::role("dialog").with_attr("aria-label", "Unsaved changes"))
            .is_some()
    );
    assert!(h.click_on(&Selector::role("button").containing("Cancel")));
    assert!(!h.close_requested());
    assert!(h.state().document().snapshot().dirty);
}

#[test]
fn palette_keyboard_filter_escape_and_focus_return_do_not_edit_source() {
    let mut h = harness(CommandChrome::PlainRow, 1100.0);
    assert!(h.click_on(&Selector::role("textbox").with_attr("aria-label", "Document text")));
    let source_focus = h.focus().expect("source focused");
    let before = h.state().document().snapshot().text;
    h.press_key(&palette_shortcut());
    assert!(
        h.resolve(&Selector::role("dialog").with_attr("aria-label", "Commands"))
            .is_some()
    );
    h.key_char("Appearance");
    assert_eq!(h.state().document().snapshot().text, before);
    h.press_key(&KeyPress::named(NamedKey::Escape));
    assert!(
        h.resolve(&Selector::role("dialog").with_attr("aria-label", "Commands"))
            .is_none()
    );
    assert_eq!(h.focus(), Some(source_focus));
    assert_eq!(h.state().document().snapshot().text, before);
}

#[test]
fn palette_query_stays_authoritative_after_tab_and_enter_activates_the_filtered_command() {
    let mut h = harness(CommandChrome::PlainRow, 1100.0);
    assert!(h.click_on(&Selector::role("textbox").with_attr("aria-label", "Document text")));
    let before = h.state().document().snapshot().text;
    h.press_key(&palette_shortcut());
    h.key_char("Appearan");
    h.press_key(&KeyPress::named(NamedKey::Tab));
    h.key_char("ce");
    h.press_key(&KeyPress::named(NamedKey::Enter));
    assert!(
        h.resolve(&Selector::class("knot-command-palette"))
            .is_none()
    );
    assert!(
        h.resolve(&Selector::class("knot-appearance-panel"))
            .is_some()
    );
    assert_eq!(h.state().document().snapshot().text, before);
}

#[test]
fn palette_arrow_selection_reveals_long_list_without_scrolling_workspace() {
    let mut h = harness_at(CommandChrome::PlainRow, 640.0, 340.0);
    h.press_key(&palette_shortcut());
    let list = node(&h, &Selector::class("command-items"));
    assert_eq!(h.element_scroll(list).1, 0.0);
    let viewport_before = h.viewport_scroll();
    for _ in 0..12 {
        h.press_key(&KeyPress::named(NamedKey::ArrowDown));
    }
    let selected = node(
        &h,
        &Selector::class("command-item").with_attr("aria-selected", "true"),
    );
    let (_, row_y, _, row_height) = h.painted_rect(selected).expect("option rect");
    let (_, visible_y, _, visible_height) =
        h.visible_rect(selected).expect("active option is visible");
    assert!(
        h.element_scroll(list).1 > 0.0,
        "arrow navigation scrolls options"
    );
    assert_eq!(
        h.viewport_scroll(),
        viewport_before,
        "workspace stays fixed"
    );
    assert_eq!(row_y, visible_y);
    assert!(
        visible_height >= row_height - 1.0,
        "the whole active row is visible"
    );

    let (list_x, list_y, list_width, list_height) = h.painted_rect(list).expect("list rect");
    let (dialog_x, dialog_y, dialog_width, dialog_height) = h
        .painted_rect(node(&h, &Selector::class("knot-command-palette")))
        .expect("dialog rect");
    assert!(dialog_x >= 0.0 && dialog_x + dialog_width <= 640.0);
    assert!(dialog_y >= 0.0 && dialog_y + dialog_height <= 340.0);
    h.move_to(list_x + list_width / 2.0, list_y + list_height / 2.0);
    let before_wheel = h.element_scroll(list).1;
    h.wheel(0.0, 40.0);
    assert!(
        h.element_scroll(list).1 > before_wheel,
        "wheel scrolls palette options"
    );
    assert_eq!(
        h.viewport_scroll(),
        viewport_before,
        "wheel does not scroll workspace"
    );
    let after_wheel = h.element_scroll(list).1;
    h.press_key(&KeyPress::named(NamedKey::Tab));
    assert_eq!(
        h.element_scroll(list).1,
        after_wheel,
        "focus change does not snap the list back"
    );
}

#[test]
fn client_menu_collapses_and_reopens_safely_after_resize() {
    let mut h = harness(CommandChrome::ClientTitlebar, 1400.0);
    h.press_key(&KeyPress::named(NamedKey::F10));
    h.press_key(&KeyPress::named(NamedKey::ArrowDown));
    assert!(h.resolve(&Selector::role("menu")).is_some());
    h.layout_at(540.0, 800.0);
    h.prepare_frame();
    h.layout_at(540.0, 800.0);
    let compact = Selector::role("menuitem").with_attr("data-key", "menu");
    assert!(h.resolve(&compact).is_some(), "narrow chrome exposes Menu");
    for label in ["Minimize window", "Maximize window", "Close window"] {
        let (x, y) = h
            .resolve(&button(label))
            .expect("narrow caption remains laid out");
        assert!((0.0..540.0).contains(&x), "{label} is off-screen at x={x}");
        assert!(
            (0.0..80.0).contains(&y),
            "{label} left the title strip at y={y}"
        );
    }
    assert!(h.click_on(&compact));
    h.press_key(&KeyPress::named(NamedKey::Enter));
    assert!(
        h.resolve(&Selector::role("menuitem").with_attr("data-key", "file.new"))
            .is_some()
    );
    h.press_key(&KeyPress::named(NamedKey::Escape));
    h.press_key(&KeyPress::named(NamedKey::Escape));
    h.layout_at(1400.0, 800.0);
    h.prepare_frame();
    h.layout_at(1400.0, 800.0);
    assert!(
        h.resolve(&compact).is_none(),
        "wide chrome restores named groups"
    );
    h.press_key(&KeyPress::named(NamedKey::F10));
    h.press_key(&KeyPress::named(NamedKey::ArrowDown));
    assert!(
        h.resolve(&Selector::role("menuitem").with_attr("data-key", "file.new"))
            .is_some()
    );
}
