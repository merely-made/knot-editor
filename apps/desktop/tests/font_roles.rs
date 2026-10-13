// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use cambium::{TextInput, el, span};
use cambium_genet_winit_host::{Harness, Init, Key, KeyPress, NamedKey, WindowCommands};
use knot_desktop::{
    appearance::{Appearance, SourceFace},
    font_roles, host_hooks,
    preferences::DesktopPreferences,
    theme_session,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use std::{collections::BTreeMap, path::Path};
use tabard::{
    Theme,
    library::ThemeLibraryStore,
    theme::{choice::ThemeChoice, registry::Mode},
    theme_styles::ThemeFontStyle,
    typography::{FontFamily, FontPreferences, FontRole},
};
use taproot::Selector;
use tempfile::tempdir;

type Host = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;
fn prefs(role: FontRole, families: &[&str]) -> FontPreferences {
    FontPreferences(BTreeMap::from([(
        role,
        families
            .iter()
            .map(|name| FontFamily::new(*name).unwrap())
            .collect(),
    )]))
}
fn launch(library: &Path, preference: &Path, logic: fn(&DesktopState) -> DesktopView) -> Host {
    let mut state = DesktopState::with_path(
        KnotDocumentSession::scratch("scratch:font-test", "# Writer authority\n\n00000000\n"),
        WindowCommands::new(),
        None,
    );
    state.set_preferences_path(Some(preference.to_path_buf()));
    state.theme_session.load(library.to_path_buf());
    let sheet = theme_session::stylesheet(&state);
    let mut host = Harness::with_hooks(
        Init {
            state,
            logic,
            sheet,
            fonts: knot_desktop::fonts::bundled_fonts(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    host.layout_at(1280.0, 1300.0);
    let quick = Selector::role("button").containing("Appearance");
    if host.resolve(&quick).is_some() {
        assert!(host.click_on(&quick));
        host.after_dispatch();
        host.relayout();
    } else if host
        .resolve(&Selector::role("menuitem").with_attr("data-key", "group.view"))
        .is_some()
    {
        assert!(host.click_on(&Selector::role("menuitem").with_attr("data-key", "group.view")));
        host.after_dispatch();
        host.relayout();
        assert!(
            host.click_on(&Selector::role("menuitem").with_attr("data-key", "view.appearance"))
        );
        host.after_dispatch();
        host.relayout();
    }
    host
}
fn field(role: &str) -> Selector {
    Selector::role("textbox").with_attr("data-font-role", role)
}
fn focus(host: &mut Host, selector: &Selector) {
    let node = host
        .with_dom(|dom| taproot::matching(dom, selector).first().copied())
        .expect("mounted control");
    for _ in 0..160 {
        if host.focus() == Some(node) {
            return;
        }
        host.tab(true);
    }
    panic!("control was unreachable by keyboard: {selector:?}");
}
fn enter(host: &mut Host, selector: &Selector) {
    focus(host, selector);
    host.press_key(&KeyPress::new(Key::Named(NamedKey::Enter)));
    host.after_dispatch();
    host.relayout();
}
fn apply(host: &mut Host) {
    enter(
        host,
        &Selector::role("button").with_attr("data-action", "apply-font-roles"),
    );
}
fn requested(host: &Host) -> FontPreferences {
    host.state()
        .appearance
        .theme_choice
        .as_ref()
        .map(|choice| choice.fonts.clone())
        .unwrap_or_default()
}

#[test]
fn mounted_typing_picker_keyboard_apply_and_reopen_preserve_writer_authority() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let preference = dir.path().join("preferences.json");
    let mut host = launch(&library, &preference, desktop_view);
    assert!(
        host.state()
            .theme_session
            .available_fonts
            .iter()
            .any(|family| family == "IBM Plex Mono")
    );
    assert!(
        host.state()
            .theme_session
            .available_fonts
            .iter()
            .any(|family| family == "Source Serif 4")
    );
    let writer = host.state().document().snapshot();
    focus(&mut host, &field("reading"));
    host.key_injected("IBM Plex Mono");
    assert_eq!(
        host.state()
            .theme_session
            .font_editor
            .field(FontRole::Reading)
            .text(),
        "IBM Plex Mono"
    );
    assert!(requested(&host).is_empty());
    assert!(!preference.exists());
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-font-choose", "heading"),
    );
    focus(
        &mut host,
        &Selector::role("textbox").with_attr("data-font-search", "true"),
    );
    host.key_injected("Source Serif 4");
    host.after_dispatch();
    host.relayout();
    assert!(host.with_dom(|dom| {
        taproot::matching(
            dom,
            &Selector::role("button").with_attr("data-font-family", "IBM Plex Mono"),
        )
        .is_empty()
    }));
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-font-family", "Source Serif 4"),
    );
    assert!(requested(&host).is_empty());
    assert!(!preference.exists());
    apply(&mut host);
    assert_eq!(
        requested(&host).0[&FontRole::Reading][0].name(),
        "IBM Plex Mono"
    );
    assert_eq!(
        requested(&host).0[&FontRole::Heading][0].name(),
        "Source Serif 4"
    );
    assert_eq!(host.state().appearance.source_face, SourceFace::IbmPlexMono);
    assert_eq!(host.state().document().snapshot(), writer);
    let mut reopened = launch(&library, &preference, desktop_view);
    assert_eq!(requested(&reopened), requested(&host));
    assert_eq!(
        reopened
            .state()
            .theme_session
            .font_editor
            .field(FontRole::Reading)
            .text(),
        "IBM Plex Mono"
    );
    let saved = std::fs::read(&preference).unwrap();
    enter(
        &mut reopened,
        &Selector::role("button").with_attr("data-font-default", "reading"),
    );
    assert_eq!(requested(&reopened), requested(&host));
    assert_eq!(
        std::fs::read(&preference).unwrap(),
        saved,
        "Default stages until explicit Apply"
    );
    apply(&mut reopened);
    assert!(!requested(&reopened).0.contains_key(&FontRole::Reading));
    assert_eq!(
        requested(&reopened).0[&FontRole::Heading][0].name(),
        "Source Serif 4"
    );
}

#[test]
fn shared_slots_keep_preedit_out_of_saved_intent_and_route_commit() {
    let dir = tempdir().unwrap();
    let mut host = launch(
        &dir.path().join("themes.json"),
        &dir.path().join("preferences.json"),
        desktop_view,
    );
    focus(&mut host, &field("reading"));
    let slot = knot_desktop::workspace::focused_text(host.runner()).expect("shared font text slot");
    host.update(|state| (slot.get_mut)(state).set_preedit("仮入力"));
    host.after_dispatch();
    assert_eq!(
        host.state()
            .theme_session
            .font_editor
            .field(FontRole::Reading)
            .text(),
        ""
    );
    assert!(requested(&host).is_empty());
    host.update(|state| (slot.get_mut)(state).clear_preedit());
    host.key_injected("Source Serif 4");
    apply(&mut host);
    assert_eq!(
        requested(&host),
        prefs(FontRole::Reading, &["Source Serif 4"])
    );
}

fn theme(library: &Path, authored: bool) -> Theme {
    let mut theme = Theme::new(
        "user:font-test",
        "Font test",
        tabard::theme::registry::ThemeRegistry::default().list()[0].seeds,
    );
    theme
        .typography
        .set(
            FontRole::Reading,
            ThemeFontStyle {
                family: Some("IBM Plex Mono".into()),
                size_px: Some(36.0),
                line_height: Some(2.5),
            },
        )
        .unwrap();
    theme
        .typography
        .set(
            FontRole::Heading,
            ThemeFontStyle {
                family: Some("Source Serif 4".into()),
                size_px: Some(42.0),
                line_height: Some(1.0),
            },
        )
        .unwrap();
    if authored {
        theme.mode_sheets.insert(
            "dark".into(),
            vec![".knot-document-preview { font-family:'IBM Plex Mono'; }".into()],
        );
    }
    ThemeLibraryStore::load(library)
        .unwrap()
        .save(&[theme.clone()])
        .unwrap();
    theme
}

#[test]
fn ordered_requests_survive_theme_mode_workshop_and_legacy_light_dark_controls() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let preference = dir.path().join("preferences.json");
    let theme = theme(&library, false);
    let ordered = prefs(
        FontRole::Reading,
        &["Absent Knot Face", "Source Serif 4", "serif"],
    );
    let mut host = launch(&library, &preference, desktop_view);
    host.update(|state| state.apply_font_preferences(ordered.clone()));
    host.after_dispatch();
    host.relayout();
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-theme-id", &theme.id),
    );
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-theme-mode", "dark"),
    );
    assert_eq!(requested(&host), ordered);
    host.update(|state| state.theme_session.begin_edit(&state.appearance).unwrap());
    host.after_dispatch();
    host.relayout();
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-action", "apply-to-knot"),
    );
    assert_eq!(requested(&host), ordered);
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-action", "back-to-knot"),
    );
    enter(&mut host, &Selector::role("button").containing("Light"));
    assert_eq!(requested(&host), ordered);
    assert_eq!(
        host.state()
            .appearance
            .theme_choice
            .as_ref()
            .unwrap()
            .theme_id,
        theme_session::DEFAULT_THEME
    );
    apply(&mut host);
    assert_eq!(requested(&host), ordered);
    let diagnostics =
        font_roles::diagnostics(&ordered, &host.state().theme_session.available_fonts).join(" ");
    assert!(diagnostics.contains("Absent Knot Face"));
    assert!(diagnostics.contains("Saved requests are retained"));
    assert_eq!(
        requested(&launch(&library, &preference, desktop_view)),
        ordered
    );
}

#[test]
fn save_failure_preserves_visible_preferences_and_editor_baseline() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let preference = dir.path().join("preferences.json");
    let ordered = prefs(FontRole::Reading, &["Absent Knot Face", "serif"]);
    let mut stored = DesktopPreferences::default();
    let mut choice = ThemeChoice::new(theme_session::DEFAULT_THEME, Some(Mode::Light));
    choice.fonts = ordered.clone();
    stored.appearance.known.theme_choice = Some(choice);
    stored.save(&preference).unwrap();
    let mut host = launch(&library, &preference, desktop_view);
    let before = host.state().appearance.clone();
    std::fs::remove_file(&preference).unwrap();
    std::fs::create_dir(&preference).unwrap();
    host.update(|state| {
        *state.theme_session.font_editor.field_mut(FontRole::Reading) =
            TextInput::new("IBM Plex Mono")
    });
    apply(&mut host);
    assert_eq!(host.state().appearance, before);
    assert!(
        host.state()
            .theme_session
            .font_editor
            .notice
            .as_ref()
            .unwrap()
            .contains("not applied")
    );
    std::fs::remove_dir(&preference).unwrap();
    host.update(|state| {
        *state.theme_session.font_editor.field_mut(FontRole::Reading) =
            TextInput::new("Absent Knot Face")
    });
    apply(&mut host);
    assert_eq!(
        requested(&host),
        ordered,
        "failed save did not replace the ordered editor baseline"
    );
}

fn probe_view(state: &DesktopState) -> DesktopView {
    Box::new(
        el(
            "div",
            (
                el(
                    "div",
                    (
                        span("00000000")
                            .attr("class", "reader-probe")
                            .attr("style", "position:absolute;top:0px;left:0px;"),
                        el("h2", span("00000000").attr("class", "heading-probe"))
                            .attr("class", "heading-style-probe"),
                        el("code", span("00000000").attr("class", "code-probe")),
                    ),
                )
                .attr("class", "knot-document-preview")
                .attr("style", state.appearance.preview_style()),
                el(
                    "div",
                    span("00000000")
                        .attr("class", "writer-probe")
                        .attr("style", "position:absolute;top:100px;left:0px;"),
                )
                .attr("class", "knot-document-body")
                .attr("style", state.appearance.writing_style("probe")),
            ),
        )
        .attr("class", "knot-workspace"),
    )
}
fn width(host: &Host, class: &str) -> f32 {
    let node = host.with_dom(|dom| taproot::matching(dom, &Selector::class(class))[0]);
    host.painted_rect(node).unwrap().2
}
#[test]
fn mounted_geometry_and_authored_cascade_honor_family_only_preferences() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let preference = dir.path().join("preferences.json");
    let theme = theme(&library, true);
    let mut host = launch(&library, &preference, probe_view);
    let serif = width(&host, "reader-probe");
    let writer = width(&host, "writer-probe");
    let reader =
        host.with_dom(|dom| taproot::matching(dom, &Selector::class("knot-document-preview"))[0]);
    let heading =
        host.with_dom(|dom| taproot::matching(dom, &Selector::class("heading-style-probe"))[0]);
    let reader_metrics = (
        host.computed_value(reader, "font-size"),
        host.computed_value(reader, "line-height"),
    );
    let heading_metrics = (
        host.computed_value(heading, "font-size"),
        host.computed_value(heading, "line-height"),
    );
    host.update(|state| state.apply_font_preferences(prefs(FontRole::Reading, &["IBM Plex Mono"])));
    host.after_dispatch();
    host.relayout();
    let mono = width(&host, "reader-probe");
    assert!((mono - 76.8).abs() < 0.6);
    assert!((mono - serif).abs() > 0.5);
    assert_eq!(width(&host, "writer-probe"), writer);
    host.update(|state| {
        state.apply_font_preferences(prefs(FontRole::Reading, &["Source Serif 4"]))
    });
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(Mode::Light))))
    });
    host.after_dispatch();
    host.relayout();
    assert!(
        (width(&host, "reader-probe") - serif).abs() < 0.3,
        "application family overrides derived theme family"
    );
    assert_eq!(
        (
            host.computed_value(reader, "font-size"),
            host.computed_value(reader, "line-height")
        ),
        reader_metrics
    );
    assert_eq!(
        (
            host.computed_value(heading, "font-size"),
            host.computed_value(heading, "line-height")
        ),
        heading_metrics
    );
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(Mode::Dark))))
    });
    host.after_dispatch();
    host.relayout();
    assert!(
        (width(&host, "reader-probe") - mono).abs() < 0.3,
        "explicit authored selector has ordinary cascade authority"
    );
    assert_eq!(width(&host, "writer-probe"), writer);
    assert_eq!(
        requested(&host),
        prefs(FontRole::Reading, &["Source Serif 4"])
    );
    host.layout_at(640.0, 900.0);
    assert!((width(&host, "reader-probe") - mono).abs() < 0.3);
}

#[test]
fn missing_theme_and_empty_legacy_fonts_retain_intent_without_serialization_churn() {
    assert!(
        !serde_json::to_string(&Appearance::default())
            .unwrap()
            .contains("theme-choice")
    );
    let legacy: ThemeChoice =
        serde_json::from_str(r#"{"theme_id":"knot:default","theme_mode":"light"}"#).unwrap();
    assert!(legacy.fonts.is_empty());
    assert!(!serde_json::to_string(&legacy).unwrap().contains("fonts"));
    let dir = tempdir().unwrap();
    let mut host = launch(
        &dir.path().join("themes.json"),
        &dir.path().join("preferences.json"),
        desktop_view,
    );
    host.update(|state| {
        state.appearance.theme_choice = Some(ThemeChoice::new(
            "missing:theme",
            Some(Mode::Custom("absent".into())),
        ))
    });
    host.update(|state| {
        state.apply_font_preferences(prefs(FontRole::Code, &["Absent Knot Face", "monospace"]))
    });
    host.after_dispatch();
    let choice = host.state().appearance.theme_choice.as_ref().unwrap();
    assert_eq!(choice.theme_id, "missing:theme");
    assert_eq!(choice.theme_mode, Some(Mode::Custom("absent".into())));
    assert!(
        host.state()
            .theme_session
            .resolve(&host.state().appearance)
            .notice
            .unwrap()
            .contains("retained")
    );
    assert_eq!(font_roles::defaults().len(), 5);
}

#[test]
fn fonts_only_apply_retains_the_frozen_same_identity_theme_until_theme_apply() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let preference = dir.path().join("preferences.json");
    let original = theme(&library, true);
    let mut host = launch(&library, &preference, desktop_view);
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&original.id, Some(Mode::Dark))))
    });
    host.after_dispatch();
    host.relayout();
    let frozen = host.state().theme_session.resolve(&host.state().appearance);
    host.update(|state| {
        state.theme_session.begin_edit(&state.appearance).unwrap();
        let workshop = state.theme_session.workshop.as_mut().unwrap();
        *workshop.text_field_mut("seed-hex").unwrap() = TextInput::new("#e42b65");
        assert!(workshop.apply_hex());
        workshop.set_font_role(FontRole::Heading);
        *workshop.text_field_mut("font-family").unwrap() = TextInput::new("IBM Plex Mono");
        assert!(workshop.apply_fonts());
        *workshop.text_field_mut("mode-sheet").unwrap() = TextInput::new(
            ".knot-document-preview { font-family:'Source Serif 4'; color:#123456; }",
        );
        workshop.apply_stylesheet();
        workshop.save();
        assert!(!workshop.has_changes());
    });
    host.after_dispatch();
    host.relayout();
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-action", "back-to-knot"),
    );
    let saved = host
        .state()
        .theme_session
        .workshop
        .as_ref()
        .unwrap()
        .registry()
        .theme_def(&original.id)
        .unwrap()
        .clone();
    assert_ne!(saved.seeds, frozen.theme.seeds);
    assert_ne!(saved.typography, frozen.theme.typography);
    let requested_fonts = prefs(FontRole::Reading, &["Source Serif 4"]);
    host.update(|state| state.apply_font_preferences(requested_fonts.clone()));
    host.after_dispatch();
    host.relayout();
    let retained = host.state().theme_session.resolve(&host.state().appearance);
    assert_eq!(retained.theme, frozen.theme);
    assert_eq!(retained.presentation, frozen.presentation);
    assert_eq!(requested(&host), requested_fonts);
    // Refused font persistence keeps both the current presentation and choice.
    std::fs::remove_file(&preference).unwrap();
    std::fs::create_dir(&preference).unwrap();
    host.update(|state| state.apply_font_preferences(prefs(FontRole::Label, &["IBM Plex Mono"])));
    host.after_dispatch();
    assert_eq!(
        host.state()
            .theme_session
            .resolve(&host.state().appearance)
            .presentation,
        frozen.presentation
    );
    assert_eq!(requested(&host), requested_fonts);
    std::fs::remove_dir(&preference).unwrap();
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&original.id, Some(Mode::Dark))))
    });
    host.after_dispatch();
    let applied = host.state().theme_session.resolve(&host.state().appearance);
    assert_eq!(applied.theme, saved);
    assert_ne!(applied.presentation, frozen.presentation);
    assert_eq!(requested(&host), requested_fonts);
}

#[test]
fn committed_font_role_library_loads_into_the_theme_workshop() {
    let library = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/fixtures/font_role_library.json");
    let store = ThemeLibraryStore::load(&library)
        .expect("the committed font-role fixture must use the supported library version");
    assert!(store
        .themes()
        .iter()
        .any(|theme| theme.id == "user:role-fixture"));

    let mut session = theme_session::ThemeSession::default();
    session.load(library);
    let workshop = session
        .workshop
        .as_ref()
        .expect("the committed fixture should open the theme workshop");
    assert!(workshop.registry().theme_def("user:role-fixture").is_some());
}
