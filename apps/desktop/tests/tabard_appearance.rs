// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use cambium_genet_winit_host::{CloseRequest, Harness, Init, WindowCommands};
use knot_desktop::{
    appearance::{Appearance, Measure, SourceFace},
    host_hooks,
    preferences::DesktopPreferences,
    theme_session::{self, ThemeSession},
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use tabard::{
    Theme, ThemePresentation,
    library::ThemeLibraryStore,
    theme::{
        choice::ThemeChoice,
        registry::{Harmony, Mode},
    },
};
use taproot::Selector;
use tempfile::tempdir;

type Host = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

#[test]
fn native_appearance_scenarios_use_the_shared_parseable_lane() {
    for name in [
        "tabard_authoring.scn",
        "tabard_reopen.scn",
        "tabard_authored_roles.scn",
        "tabard_authored_reopen.scn",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios")
            .join(name);
        let source = std::fs::read_to_string(&path).unwrap();
        taproot::Scenario::parse(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

fn launch(library: &std::path::Path, prefs: &std::path::Path) -> Host {
    let mut state = DesktopState::with_path(
        KnotDocumentSession::scratch("scratch:untitled", "# Writing stays here\n"),
        WindowCommands::new(),
        None,
    );
    state.set_preferences_path(Some(prefs.to_path_buf()));
    state.theme_session.load(library.to_path_buf());
    let sheet = theme_session::stylesheet(&state);
    let mut host = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet,
            fonts: knot_desktop::fonts::bundled_fonts(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    host.layout_at(1280.0, 960.0);
    host
}

#[test]
fn a_loaded_library_does_not_replace_embedding_styles_on_unrelated_input() {
    let dir = tempdir().unwrap();
    let mut state = DesktopState::with_path(
        KnotDocumentSession::scratch("scratch:untitled", "Writing with an embedding rule"),
        WindowCommands::new(),
        None,
    );
    state.theme_session.load(dir.path().join("themes.json"));
    assert!(state.theme_session.workshop.is_some());
    let sheet = format!(
        "{} .knot-workspace .knot-writing-area {{ min-height:555px; }}",
        theme_session::stylesheet(&state)
    );
    let mut host = Host::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet,
            fonts: Vec::new(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    host.layout_at(1280.0, 960.0);
    appearance(&mut host);
    let node =
        host.with_dom(|dom| taproot::matching(dom, &Selector::class("knot-writing-area"))[0]);
    assert_eq!(
        host.computed_value(node, "min-height").as_deref(),
        Some("555px")
    );
    assert_eq!(
        host.state().document().snapshot().text,
        "Writing with an embedding rule"
    );
    assert!(!host.state().document().snapshot().dirty);
}

fn action(host: &mut Host, key: &str) {
    assert!(
        host.click_on(&Selector::role("button").with_attr("data-action", key)),
        "mounted action {key}"
    );
    host.after_dispatch();
    host.relayout();
}
fn appearance(host: &mut Host) {
    assert!(host.click_on(&Selector::role("button").containing("Appearance")));
    host.after_dispatch();
    host.relayout();
}
fn test_seeds() -> tinct::Seeds {
    tabard::theme::registry::ThemeRegistry::default().list()[0].seeds
}

fn saved_theme(path: &std::path::Path, sheets: bool) -> Theme {
    let mut theme = Theme::new("user:knot-test", "Writing theme", test_seeds());
    theme.harmony = Harmony::Locked {
        secondary_deg: 30.0,
        tertiary_deg: -30.0,
    };
    if sheets {
        theme.mode_sheets.insert(
            "dark".into(),
            vec![".knot-writing-area { background: #123456; color: #ffffff; }".into()],
        );
    }
    ThemeLibraryStore::load(path)
        .unwrap()
        .save(&[theme.clone()])
        .unwrap();
    theme
}

#[test]
fn saved_choice_relaunch_preserves_every_writing_preference_and_document() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let theme = saved_theme(&library, false);
    let mut original = DesktopPreferences::default();
    original.appearance.known = Appearance {
        font_size: 21,
        source_face: SourceFace::SystemMonospace,
        measure: Measure::Wide,
        follow_hard_wrap: false,
        relaxed: false,
        highlight: false,
        ..Appearance::default()
    };
    original.save(&prefs).unwrap();
    let mut host = launch(&library, &prefs);
    let writing = host.state().document().snapshot().text;
    appearance(&mut host);
    assert!(host.click_on(&Selector::role("button").with_attr("data-theme-id", &theme.id)));
    host.after_dispatch();
    host.relayout();
    assert!(host.click_on(&Selector::role("button").with_attr("data-theme-mode", "hc_dark")));
    host.after_dispatch();
    let mut expected = original.appearance.known.clone();
    expected.dark = true;
    expected.theme_choice = Some(ThemeChoice::new(&theme.id, Some(Mode::HcDark)));
    assert_eq!(host.state().appearance, expected);
    assert_eq!(host.state().document().snapshot().text, writing);
    let restored = launch(&library, &prefs);
    assert_eq!(restored.state().appearance, expected);
    assert_eq!(
        restored.state().theme_session.resolve(&expected).mode,
        Mode::HcDark
    );
    assert!(!restored.state().theme_session.workshop_open);
}

#[test]
fn editor_save_is_separate_from_application_and_apply_requires_registered_savepoint() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let mut host = launch(&library, &prefs);
    let original = host.state().appearance.clone();
    let writing = host.state().document().snapshot().text;
    appearance(&mut host);
    action(&mut host, "edit-themes");
    assert!(host.state().theme_session.workshop_open);
    action(&mut host, "apply-to-knot");
    assert_eq!(host.state().appearance, original);
    assert!(!prefs.exists());
    action(&mut host, "save");
    assert!(library.exists());
    assert!(!prefs.exists());
    assert_eq!(host.state().appearance, original);
    action(&mut host, "apply-to-knot");
    assert!(prefs.exists());
    assert_eq!(host.state().document().snapshot().text, writing);
    assert_eq!(
        host.state().appearance.theme_choice.as_ref().unwrap(),
        &host
            .state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .saved_choice()
            .unwrap()
    );
}

#[test]
fn corrupt_preferences_block_theme_activation_and_preserve_both_files() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let theme = saved_theme(&library, false);
    std::fs::write(&prefs, "corrupt preferences").unwrap();
    let before = std::fs::read(&library).unwrap();
    let mut host = launch(&library, &prefs);
    let original = host.state().appearance.clone();
    appearance(&mut host);
    assert!(host.click_on(&Selector::role("button").with_attr("data-theme-id", &theme.id)));
    host.after_dispatch();
    assert_eq!(host.state().appearance, original);
    assert!(
        host.state()
            .theme_session
            .notice
            .as_ref()
            .unwrap()
            .contains("not changed")
    );
    assert_eq!(
        std::fs::read_to_string(prefs).unwrap(),
        "corrupt preferences"
    );
    assert_eq!(std::fs::read(library).unwrap(), before);
}

#[test]
fn corrupt_library_disables_editor_retains_request_and_can_be_reloaded() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    std::fs::write(&library, "corrupt library").unwrap();
    let mut settings = DesktopPreferences::default();
    settings.appearance.known.theme_choice =
        Some(ThemeChoice::new("user:recover", Some(Mode::Dark)));
    settings.save(&prefs).unwrap();
    let mut host = launch(&library, &prefs);
    appearance(&mut host);
    assert!(host.state().theme_session.workshop.is_none());
    assert_eq!(
        host.state().appearance.theme_choice,
        settings.appearance.known.theme_choice
    );
    assert!(!host.click_on(&Selector::role("button").with_attr("data-action", "edit-themes")));
    assert_eq!(
        std::fs::read_to_string(&library).unwrap(),
        "corrupt library"
    );
    std::fs::remove_file(&library).unwrap();
    action(&mut host, "reload-theme-library");
    assert!(host.state().theme_session.workshop.is_some());
}

#[test]
fn exact_modes_use_product_roles_and_authored_css_is_last_in_cascade() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let theme = saved_theme(&library, false);
    let mut host = launch(&library, &prefs);
    for mode in [Mode::Light, Mode::Dark, Mode::HcLight, Mode::HcDark] {
        host.update(|state| {
            state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(mode.clone()))))
        });
        let resolved = host.state().theme_session.resolve(&host.state().appearance);
        let ThemePresentation::Derived(p) = resolved.presentation else {
            panic!("derived exact mode")
        };
        assert_eq!(p.mode, mode);
        assert_eq!(p, theme.palette_for_mode(&mode).unwrap());
        let selected = knot_desktop::appearance::selected_palette_css(&p);
        assert!(theme_session::stylesheet(host.state()).ends_with(&selected));
        let surface = format!(
            "rgb({}, {}, {})",
            p.palette.surface.r, p.palette.surface.g, p.palette.surface.b
        );
        assert!(selected.contains(&surface));
        assert!(selected.contains("syntax-"));
    }
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let theme = saved_theme(&library, true);
    let mut host = launch(&library, &prefs);
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(Mode::Dark))))
    });
    let sheet = theme_session::stylesheet(host.state());
    assert!(sheet.ends_with(&theme.mode_sheets["dark"][0]));
    let resolved = host.state().theme_session.resolve(&host.state().appearance);
    assert!(matches!(
        resolved.presentation,
        ThemePresentation::AuthoredStylesheet(_)
    ));
    assert!(resolved.notice.unwrap().contains("Graph nodes"));
}

#[test]
fn native_editor_close_returns_to_document_confirmation_and_cancel_is_reusable() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let mut host = launch(&library, &prefs);
    assert!(host.click_on(&Selector::role("textbox").with_attr("aria-label", "Document text")));
    host.key_injected("Unsaved document edit");
    host.after_dispatch();
    assert!(host.state().document().snapshot().dirty);
    appearance(&mut host);
    action(&mut host, "edit-themes");
    host.request_close(CloseRequest::Native);
    host.relayout();
    assert!(!host.close_requested());
    action(&mut host, "cancel-close");
    assert!(!host.state().theme_session.close_app);
    action(&mut host, "back-to-knot");
    action(&mut host, "discard-close");
    assert!(!host.close_requested());
    assert!(!host.state().theme_session.workshop_open);
    action(&mut host, "edit-themes");
    host.request_close(CloseRequest::Native);
    host.relayout();
    action(&mut host, "discard-close");
    assert!(!host.state().theme_session.workshop_open);
    assert!(
        !host.close_requested(),
        "unsaved writing still needs its own confirmation"
    );
    assert!(
        host.state()
            .document()
            .snapshot()
            .text
            .contains("Writing stays here")
    );
}

#[test]
fn legacy_dark_and_wide_migrate_without_an_authored_choice() {
    let appearance: Appearance =
        serde_json::from_str(r#"{"dark":true,"wide":true,"font-size":19}"#).unwrap();
    assert_eq!(appearance.measure, Measure::Full);
    assert!(appearance.theme_choice.is_none());
    assert_eq!(
        ThemeSession::default().resolve(&appearance).mode,
        Mode::Dark
    );
}

#[test]
fn native_typing_edits_theme_fields_and_conflict_keeps_the_previous_savepoint() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let theme = saved_theme(&library, false);
    let mut host = launch(&library, &prefs);
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(Mode::Light))))
    });
    let original = host.state().appearance.clone();
    appearance(&mut host);
    action(&mut host, "edit-themes");
    assert!(host.click_on(&Selector::role("textbox").with_attr("data-field", "name")));
    host.set_modifiers(cambium_genet_winit_host::Modifiers {
        ctrl: true,
        ..Default::default()
    });
    host.key_char("a");
    host.set_modifiers(Default::default());
    host.key_injected("Changed through native text routing");
    host.after_dispatch();
    assert_eq!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .draft_theme()
            .name,
        "Changed through native text routing"
    );
    assert!(
        host.state()
            .document()
            .snapshot()
            .text
            .contains("Writing stays here")
    );
    let changed = b"external edit, retained";
    std::fs::write(&library, changed).unwrap();
    action(&mut host, "save");
    assert!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .has_changes()
    );
    assert_eq!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .registry()
            .theme_def(&theme.id),
        Some(&theme)
    );
    action(&mut host, "apply-to-knot");
    assert_eq!(host.state().appearance, original);
    assert_eq!(std::fs::read(&library).unwrap(), changed);
}

#[test]
fn export_cancellation_and_protected_application_files_never_create_a_replacement_prompt() {
    use std::{cell::RefCell, rc::Rc};
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let document = dir.path().join("writing.djot");
    std::fs::write(&document, "# Keep this writing\n").unwrap();
    let destination = Rc::new(RefCell::new(None));
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&document).unwrap(),
        WindowCommands::new(),
        Some(document.clone()),
    );
    state.set_preferences_path(Some(prefs.clone()));
    state.theme_session.load(library.clone());
    state.theme_session.begin_edit(&state.appearance).unwrap();
    let sheet = theme_session::stylesheet(&state);
    let mut hooks = host_hooks();
    let selection = destination.clone();
    hooks.after_dispatch = Box::new(move |ctx| {
        theme_session::after_dispatch_with_exporter(ctx, |_| selection.borrow().clone())
    });
    let mut host = Host::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet,
            fonts: Vec::new(),
            images: Vec::new(),
        },
        hooks,
    );
    host.layout_at(1280.0, 960.0);
    action(&mut host, "export");
    assert!(!prefs.exists());
    assert!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .status()
            .contains("cancelled")
    );
    *destination.borrow_mut() = Some(prefs.clone());
    action(&mut host, "export");
    assert!(!prefs.exists());
    assert!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .replacement_path()
            .is_none()
    );
    assert!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .status()
            .contains("protected")
    );
    *destination.borrow_mut() = Some(dir.path().join("missing/../preferences.json"));
    action(&mut host, "export");
    assert!(!prefs.exists());
    assert!(!dir.path().join("missing").exists());
    *destination.borrow_mut() = Some(document.clone());
    action(&mut host, "export");
    assert_eq!(
        std::fs::read_to_string(&document).unwrap(),
        "# Keep this writing\n"
    );
    assert!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .replacement_path()
            .is_none()
    );
    #[cfg(unix)]
    {
        let alias = dir.path().join("writing-alias.djot");
        std::os::unix::fs::symlink(&document, &alias).unwrap();
        *destination.borrow_mut() = Some(alias);
        action(&mut host, "export");
        assert_eq!(
            std::fs::read_to_string(&document).unwrap(),
            "# Keep this writing\n"
        );
        assert!(
            host.state()
                .theme_session
                .workshop
                .as_ref()
                .unwrap()
                .replacement_path()
                .is_none()
        );
    }
    let output = dir.path().join("export.json");
    *destination.borrow_mut() = Some(output.clone());
    action(&mut host, "export");
    assert!(output.exists());
    assert!(!library.exists());
    assert!(!prefs.exists());
}

#[test]
fn empty_writing_field_keeps_native_geometry_and_typing_after_semantic_input_adoption() {
    let state = DesktopState::with_path(
        KnotDocumentSession::scratch("scratch:empty", ""),
        WindowCommands::new(),
        None,
    );
    let sheet = theme_session::stylesheet(&state);
    let mut host = Host::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet,
            fonts: knot_desktop::fonts::bundled_fonts(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    host.layout_at(1280.0, 960.0);
    assert!(host.click_on(&Selector::role("textbox").with_attr("aria-label", "Document text")));
    host.key_injected("New writing");
    host.after_dispatch();
    assert_eq!(host.state().document().snapshot().text, "New writing");
    assert!(host.state().document().snapshot().dirty);
}

fn rendered_color(host: &Host, class: &str, property: &str, expected: tinct::Srgb) {
    let node = host
        .with_dom(|dom| {
            taproot::matching(dom, &Selector::class(class))
                .first()
                .copied()
        })
        .expect("mounted colored product region");
    let actual = host
        .computed_value(node, property)
        .expect("resolved product paint color")
        .to_ascii_lowercase()
        .replace(' ', "");
    let hex = tinct::color_to_hex(expected).to_ascii_lowercase();
    let rgb = format!("rgb({},{},{})", expected.r, expected.g, expected.b);
    let rgba = format!("rgba({},{},{},1)", expected.r, expected.g, expected.b);
    assert!(
        actual == hex || actual == rgb || actual == rgba,
        "{class} {property}: {actual}; expected {hex}"
    );
}

#[test]
fn authored_tabard_role_properties_change_live_product_paints_and_reopen_exactly() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let mut theme = Theme::new("user:roles", "Authored role sheet", test_seeds());
    theme.mode_sheets.insert("dark".into(), vec![":root { --tabard-color-bg: #142536; --tabard-color-surface: #123456; --tabard-color-text: #f1f2f3; --tabard-color-primary: #abcdef; --tabard-syntax-heading: #fedcba; }".into()]);
    ThemeLibraryStore::load(&library)
        .unwrap()
        .save(&[theme.clone()])
        .unwrap();
    let mut host = launch(&library, &prefs);
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(Mode::Dark))))
    });
    host.after_dispatch();
    host.relayout();
    rendered_color(
        &host,
        "knot-workspace",
        "background-color",
        tinct::Srgb::rgb(20, 37, 54),
    );
    rendered_color(
        &host,
        "knot-writing-area",
        "background-color",
        tinct::Srgb::rgb(18, 52, 86),
    );
    rendered_color(
        &host,
        "knot-writing-area",
        "color",
        tinct::Srgb::rgb(241, 242, 243),
    );
    let restored = launch(&library, &prefs);
    assert_eq!(
        restored.state().appearance.theme_choice,
        host.state().appearance.theme_choice
    );
    rendered_color(
        &restored,
        "knot-writing-area",
        "background-color",
        tinct::Srgb::rgb(18, 52, 86),
    );
    assert!(matches!(
        restored
            .state()
            .theme_session
            .resolve(&restored.state().appearance)
            .presentation,
        ThemePresentation::AuthoredStylesheet(_)
    ));
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(Mode::Light))))
    });
    host.after_dispatch();
    host.relayout();
    let derived = theme.palette_for_mode(&Mode::Light).unwrap();
    rendered_color(
        &host,
        "knot-writing-area",
        "background-color",
        derived.palette.surface,
    );
    assert!(!theme_session::stylesheet(host.state()).contains("--tabard-color-bg: #142536"));
}

#[test]
fn all_four_derived_modes_reach_live_product_paints_and_fresh_relaunch() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let theme = saved_theme(&library, false);
    let mut host = launch(&library, &prefs);
    for mode in [Mode::Light, Mode::Dark, Mode::HcLight, Mode::HcDark] {
        host.update(|state| {
            state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(mode.clone()))))
        });
        host.after_dispatch();
        host.relayout();
        let exact = theme.palette_for_mode(&mode).unwrap();
        rendered_color(
            &host,
            "knot-workspace",
            "background-color",
            exact.palette.bg,
        );
        rendered_color(
            &host,
            "knot-writing-area",
            "background-color",
            exact.palette.surface,
        );
        rendered_color(&host, "knot-writing-area", "color", exact.palette.text);
        let restored = launch(&library, &prefs);
        assert_eq!(
            restored
                .state()
                .theme_session
                .resolve(&restored.state().appearance)
                .mode,
            mode
        );
        rendered_color(
            &restored,
            "knot-writing-area",
            "background-color",
            exact.palette.surface,
        );
    }
}

#[test]
fn legacy_light_dark_cannot_clear_a_saved_choice_when_preference_save_fails() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let theme = saved_theme(&library, false);
    let mut host = launch(&library, &prefs);
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&theme.id, Some(Mode::Dark))))
    });
    host.after_dispatch();
    host.relayout();
    let original = host.state().appearance.clone();
    std::fs::remove_file(&prefs).unwrap();
    std::fs::create_dir(&prefs).unwrap();
    appearance(&mut host);
    assert!(host.click_on(&Selector::role("button").containing("Light")));
    host.after_dispatch();
    assert_eq!(host.state().appearance, original);
    assert!(
        host.state()
            .message
            .as_ref()
            .unwrap()
            .starts_with("Appearance not saved:")
    );
    assert!(prefs.is_dir());
}

#[test]
fn saving_an_active_theme_identity_does_not_apply_the_new_definition_implicitly() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let original_theme = saved_theme(&library, false);
    let mut host = launch(&library, &prefs);
    host.update(|state| {
        state.select_theme_choice(Ok(ThemeChoice::new(&original_theme.id, Some(Mode::Light))))
    });
    host.after_dispatch();
    host.relayout();
    let original = original_theme.palette_for_mode(&Mode::Light).unwrap();
    appearance(&mut host);
    action(&mut host, "edit-themes");
    assert!(host.click_on(&Selector::role("textbox").with_attr("data-field", "seed-hex")));
    host.set_modifiers(cambium_genet_winit_host::Modifiers {
        ctrl: true,
        ..Default::default()
    });
    host.key_char("a");
    host.set_modifiers(Default::default());
    host.key_injected("#e42b65");
    host.after_dispatch();
    action(&mut host, "apply-hex");
    action(&mut host, "save");
    assert_ne!(
        host.state()
            .theme_session
            .workshop
            .as_ref()
            .unwrap()
            .registry()
            .theme_def(&original_theme.id)
            .unwrap()
            .seeds,
        original_theme.seeds
    );
    let ThemePresentation::Derived(current) = host
        .state()
        .theme_session
        .resolve(&host.state().appearance)
        .presentation
    else {
        panic!("derived app snapshot")
    };
    assert_eq!(current, original);
    action(&mut host, "back-to-knot");
    rendered_color(
        &host,
        "knot-writing-area",
        "background-color",
        original.palette.surface,
    );
    action(&mut host, "edit-themes");
    // Reopening this old applied definition creates a user copy, so first
    // discard that intake and choose the updated saved library definition.
    action(&mut host, "discard");
    assert!(
        host.click_on(&Selector::role("button").with_attr("data-theme-id", &original_theme.id))
    );
    host.after_dispatch();
    action(&mut host, "apply-to-knot");
    let ThemePresentation::Derived(applied) = host
        .state()
        .theme_session
        .resolve(&host.state().appearance)
        .presentation
    else {
        panic!("derived explicit application")
    };
    assert_ne!(applied, original);
}

#[test]
fn a_global_user_artifact_cannot_replace_knots_product_default_identity() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let prefs = dir.path().join("preferences.json");
    let artifact = Theme::new(
        theme_session::DEFAULT_THEME,
        "Reserved artifact",
        test_seeds(),
    );
    ThemeLibraryStore::load(&library)
        .unwrap()
        .save(&[artifact.clone()])
        .unwrap();
    let mut host = launch(&library, &prefs);
    assert!(
        host.state()
            .theme_session
            .notice
            .as_ref()
            .unwrap()
            .contains("reserved")
    );
    appearance(&mut host);
    action(&mut host, "edit-themes");
    action(&mut host, "discard");
    assert!(host.click_on(
        &Selector::role("button").with_attr("data-theme-id", theme_session::DEFAULT_THEME)
    ));
    host.after_dispatch();
    action(&mut host, "apply-to-knot");
    assert!(host.state().appearance.theme_choice.is_none());
    assert!(
        host.state()
            .theme_session
            .notice
            .as_ref()
            .unwrap()
            .contains("reserved")
    );
    action(&mut host, "new-copy");
    action(&mut host, "save");
    action(&mut host, "apply-to-knot");
    assert_ne!(
        host.state()
            .appearance
            .theme_choice
            .as_ref()
            .unwrap()
            .theme_id,
        theme_session::DEFAULT_THEME
    );
    assert_eq!(
        ThemeLibraryStore::load(&library)
            .unwrap()
            .themes()
            .iter()
            .find(|theme| theme.id == theme_session::DEFAULT_THEME),
        Some(&artifact)
    );
}
