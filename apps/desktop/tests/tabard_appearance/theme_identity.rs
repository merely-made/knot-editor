// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
