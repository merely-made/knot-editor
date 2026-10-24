// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Exercise Knot's actual platform projections through the host. Window verbs
//! are recorded here; this is not a Windows/Linux window-manager receipt.

use cambium_genet_winit_host::{
    AppRegion, Harness, Init, KeyPress, Modifiers, NamedKey, WindowCommand, WindowCommands,
    WindowFrame, platform_window_frame,
};
use knot_desktop::{
    collapse::CollapsePreferences, desktop_sheet, host_hooks,
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

fn harness_with_menu_threshold(width: f32, menu_below: u16) -> DesktopHarness {
    let mut h = harness_at(CommandChrome::ClientTitlebar, width, 800.0);
    h.update(|state| {
        let mut preferences = CollapsePreferences::default();
        preferences.menu_below = menu_below;
        state.set_collapse_preferences(preferences);
    });
    h.layout_at(width, 800.0);
    h.prepare_frame();
    h.layout_at(width, 800.0);
    h
}

fn button(label: &str) -> Selector {
    Selector::role("button").with_attr("aria-label", label)
}

#[test]
fn default_command_chrome_matches_the_platform_frame() {
    let expected = match platform_window_frame() {
        WindowFrame::Host => CommandChrome::PlainRow,
        WindowFrame::App => CommandChrome::ClientTitlebar,
    };
    assert_eq!(CommandChrome::default(), expected);
}

fn node(h: &DesktopHarness, selector: &Selector) -> genet_scripted_dom::NodeId {
    let projection = h.a11y_projection();
    h.with_dom(|dom| taproot::matching_with_projection(dom, selector, &projection).into_iter().next())
        .expect("matching DOM node")
}

fn nodes(h: &DesktopHarness, selector: &Selector) -> Vec<genet_scripted_dom::NodeId> {
    let projection = h.a11y_projection();
    h.with_dom(|dom| taproot::matching_with_projection(dom, selector, &projection))
}

fn titlebar_action(h: &DesktopHarness, label: &str) -> Option<genet_scripted_dom::NodeId> {
    let selector = Selector::role("button").containing(label);
    nodes(h, &selector).into_iter().find(|node| {
        h.painted_rect(*node)
            .is_some_and(|rect| rect.1 >= 0.0 && rect.1 + rect.3 <= 38.0)
    })
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
    if cfg!(target_os = "macos") {
        let h = harness(CommandChrome::ClientTitlebar, 1400.0);
        for label in ["Minimize window", "Maximize window", "Close window"] {
            assert!(h.resolve(&button(label)).is_none(), "duplicate {label}");
        }
        return;
    }
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
fn client_caption_buttons_remain_fully_inside_the_window_at_supported_widths() {
    if cfg!(target_os = "macos") {
        return;
    }
    for width in [540.0, 700.0, 1000.0] {
        let h = harness_at(CommandChrome::ClientTitlebar, width, 800.0);
        for label in ["Minimize window", "Maximize window", "Close window"] {
            let rect = h
                .painted_rect(node(&h, &button(label)))
                .unwrap_or_else(|| panic!("{label} has no layout at width {width}"));
            assert!(rect.0 >= 0.0, "{label} starts outside at width {width}: {rect:?}");
            assert!(
                rect.0 + rect.2 <= width,
                "{label} is clipped at width {width}: {rect:?}"
            );
        }
    }
}

#[test]
fn full_menu_actions_end_before_caption_controls_at_user_selected_widths() {
    if cfg!(target_os = "macos") {
        return;
    }
    // At the default threshold, equality selects the full menu. The second
    // case models a user who prefers the full menu at a narrower window.
    for (width, menu_below) in [(1150.0, 1150), (1000.0, 900)] {
        let mut h = harness_with_menu_threshold(width, menu_below);
        assert!(h.resolve(&Selector::role("menubar")).is_some());
        let preview = titlebar_action(&h, "Show following Preview")
            .expect("titlebar has the following-Preview toggle");
        let preview_rect = h.painted_rect(preview).expect("Preview action rect");
        h.click_at(preview_rect.0 + preview_rect.2 / 2.0, preview_rect.1 + preview_rect.3 / 2.0);
        h.layout_at(width, 800.0);
        h.prepare_frame();
        h.layout_at(width, 800.0);
        assert!(titlebar_action(&h, "Hide following Preview").is_some());
        let captions = h
            .painted_rect(node(&h, &Selector::class("title-bar-captions")))
            .expect("caption group has layout");
        let actions_node = node(&h, &Selector::class("knot-titlebar-actions"));
        let actions = h.painted_rect(actions_node).expect("actions have layout");
        let menu = h.painted_rect(node(&h, &Selector::role("menubar"))).expect("menubar rect");
        assert!(
            actions.0 + actions.2 <= captions.0 + 0.5,
            "action region overlaps captions at width {width}: actions={actions:?}, captions={captions:?}"
        );

        let mut content_right = menu.0 + menu.2;
        let mut rightmost_label = "menubar";
        for label in ["Save", "Outline", "Hide following Preview", "Readings"] {
            if let Some(action) = titlebar_action(&h, label) {
                let rect = h.painted_rect(action).unwrap_or_else(|| panic!("{label} has no layout"));
                assert!(
                    rect.0 + rect.2 <= captions.0 + 0.5,
                    "{label} overlaps captions at width {width}: {rect:?}, captions={captions:?}"
                );
                if rect.0 + rect.2 > content_right {
                    content_right = rect.0 + rect.2;
                    rightmost_label = label;
                }
            }
        }
        assert!(
            menu.0 + menu.2 <= captions.0 + 0.5,
            "menu overlaps captions at width {width}: menu={menu:?}, captions={captions:?}"
        );
        let navigation = h
            .painted_rect(node(&h, &Selector::class("knot-context-navigation")))
            .expect("context trail rect");
        assert!(content_right <= captions.0 + 0.5,
            "full toolbar overlaps captions at width {width}: rightmost={rightmost_label}, content_right={content_right}, menu={menu:?}, navigation={navigation:?}, actions={actions:?}, captions={captions:?}");
        for label in ["Minimize window", "Maximize window", "Close window"] {
            let rect = h
                .painted_rect(node(&h, &button(label)))
                .expect("caption button rect");
            assert!(
                rect.0 >= 0.0 && rect.0 + rect.2 <= width,
                "{label} is clipped at width {width}: {rect:?}"
            );
        }
    }
}

#[test]
fn title_is_draggable_but_menu_and_document_are_not() {
    let h = harness(CommandChrome::ClientTitlebar, 1400.0);
    assert_eq!(
        h.resolve(&Selector::class("knot-titlebar-mark")).is_some(),
        platform_window_frame() == WindowFrame::App,
        "the Knot mark belongs only in an app-owned titlebar"
    );
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

#[test]
fn hest_window_invocation_uses_knots_actual_host_queue_without_claiming_completion() {
    let h = harness(CommandChrome::ClientTitlebar, 1400.0);
    let commands = h.commands();
    let invocation = WindowCommand::Resize(960.5, 720.25).invocation();
    assert_eq!(invocation.id, "window:resize");
    assert_eq!(commands.invoke(&invocation), Ok(()));
    // Admission to the queue gives no terminal Done; this harness does not
    // stand in for a window manager accepting the resize.
    assert_eq!(commands.drain(), vec![WindowCommand::Resize(960.5, 720.25)]);
    let mut malformed = WindowCommand::Close.invocation();
    malformed.schema_id.push_str("-other");
    assert!(matches!(commands.invoke(&malformed), Err(cambium::Outcome::Refused { .. })));
    assert!(commands.is_empty());
}
