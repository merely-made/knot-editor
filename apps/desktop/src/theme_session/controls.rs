// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn appearance_controls(state: &DesktopState) -> DesktopView {
    let resolved = state.theme_session.resolve(&state.appearance);
    let mut options = vec![default_theme()];
    if let Some(workshop) = &state.theme_session.workshop {
        options.extend(
            workshop
                .registry()
                .list()
                .into_iter()
                .filter(|t| t.source == ThemeSource::User && t.id != DEFAULT_THEME)
                .cloned(),
        );
    }
    let buttons: Vec<DesktopView> = options
        .into_iter()
        .map(|theme| {
            let selected = theme.id == resolved.theme.id;
            let id = theme.id.clone();
            Box::new(
                button(theme.name, move |state: &mut DesktopState, _| {
                    let choice = state.theme_session.choice(&id, None);
                    state.select_theme_choice(choice);
                })
                .attr("data-theme-id", theme.id)
                .attr("aria-pressed", selected.to_string()),
            ) as DesktopView
        })
        .collect();
    let mut modes = vec![Mode::Light, Mode::Dark, Mode::HcLight, Mode::HcDark];
    for key in resolved.theme.mode_sheets.keys() {
        if let Some(mode @ Mode::Custom(_)) = Mode::from_key(key) {
            modes.push(mode);
        }
    }
    let mode_buttons: Vec<DesktopView> = modes
        .into_iter()
        .map(|mode| {
            let active = mode == resolved.mode;
            let id = resolved.theme.id.clone();
            let key = mode.as_key();
            Box::new(
                button(
                    format!("Mode: {}", mode.label()),
                    move |state: &mut DesktopState, _| {
                        let choice = state.theme_session.choice(&id, Some(mode.clone()));
                        state.select_theme_choice(choice);
                    },
                )
                .attr("data-theme-mode", key)
                .attr("aria-pressed", active.to_string()),
            ) as DesktopView
        })
        .collect();
    let mut rows: Vec<DesktopView> = vec![
        Box::new(el("div", buttons).attr("class", "knot-theme-options")),
        Box::new(el("div", mode_buttons).attr("class", "knot-theme-options")),
    ];
    rows.push(Box::new(lens(
        |editor: &mut tabard_workshop::fonts::FontRoleEditor| {
            tabard_workshop::fonts::font_role_view(editor)
        },
        |state: &mut DesktopState| &mut state.theme_session.font_editor,
    )));
    let fonts = appearance_choice(&state.appearance).fonts;
    for notice in crate::font_roles::diagnostics(&fonts, &state.theme_session.available_fonts)
        .into_iter()
        .chain(
            resolved
                .theme
                .typography
                .missing_families(&state.theme_session.available_fonts),
        )
    {
        rows.push(Box::new(el("p", span(notice)).attr("role", "status")));
    }
    rows.push(Box::new(el("p", span("Bundled: IBM Plex Mono and Source Serif 4. Choose… lists the families available here; availability does not guarantee every glyph."))));
    for notice in [
        resolved.notice.as_ref(),
        state.theme_session.notice.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        rows.push(Box::new(
            el("p", span(notice.clone()))
                .attr("role", "status")
                .attr("class", "knot-theme-notice"),
        ));
    }
    if state.theme_session.workshop.is_some() {
        rows.push(Box::new(
            button("Edit themes…", |state: &mut DesktopState, _| {
                let paths = protected_export_paths(state);
                if let Err(error) = state.theme_session.begin_edit(&state.appearance) {
                    state.theme_session.notice = Some(error);
                } else if let Some(workshop) = &mut state.theme_session.workshop {
                    workshop.set_protected_export_paths(paths);
                }
            })
            .attr("data-action", "edit-themes"),
        ));
    }
    if state.theme_session.library_path.is_some() {
        rows.push(Box::new(
            button("Reload theme library", |state: &mut DesktopState, _| {
                if state
                    .theme_session
                    .workshop
                    .as_ref()
                    .is_some_and(WorkshopState::has_changes)
                {
                    state.theme_session.notice = Some(
                        "Save or discard the current editor changes before reloading the library."
                            .into(),
                    );
                } else if let Some(path) = state.theme_session.library_path.clone() {
                    state.theme_session.load(path);
                    state.theme_session.request_stylesheet_update();
                }
            })
            .attr("data-action", "reload-theme-library"),
        ));
    }
    Box::new(el("div", rows).attr("aria-label", "Tabard appearance"))
}
