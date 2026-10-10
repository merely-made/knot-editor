// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! App-owned selection and embedding of the shared Tabard workshop. Document
//! state and writing preferences remain Knot's authority.
use crate::{
    appearance::Appearance,
    workspace::{DesktopState, DesktopView},
};
use cambium::{button, el, lens, span};
use cambium_genet_winit_host::{
    AppCtx, CloseDisposition, CloseRequest, SceneProducer, choose_save_path,
};
use cambium_rootstock::{ProducerRole, ProducerSemantics};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use tabard::theme::{
    choice::ThemeChoice,
    registry::{Mode, ThemeSource},
    seed::default_mode_for_def,
};
use tabard::{Theme, ThemePresentation, resolve_theme_choice};
use tabard_workshop::{
    GRAPH_LEAF_KEY, PreviewScene, READER_LEAF_KEY, ReaderSpecimen, STYLESHEET_LEAF_KEY,
    StylesheetSpecimen, WorkshopState, workshop_stylesheet, workshop_view_with_captions,
};

pub const DEFAULT_THEME: &str = "knot:default";
#[derive(Default)]
pub struct ThemeSession {
    pub workshop: Option<WorkshopState>,
    pub workshop_open: bool,
    pub close_app: bool,
    pub notice: Option<String>,
    library_path: Option<PathBuf>,
    applied: Option<(ThemeChoice, Presentation)>,
    stylesheet_update: bool,
}
#[derive(Clone)]
pub struct Presentation {
    pub theme: Theme,
    pub mode: Mode,
    pub presentation: ThemePresentation,
    pub notice: Option<String>,
}
fn default_theme() -> Theme {
    let mut theme = Theme::new(DEFAULT_THEME, "Knot", crate::appearance::seeds(false));
    theme.source = ThemeSource::BuiltIn;
    theme
}
impl ThemeSession {
    pub(crate) fn request_stylesheet_update(&mut self) {
        self.stylesheet_update = true;
    }
    pub fn load(&mut self, path: PathBuf) {
        self.workshop_open = false;
        self.close_app = false;
        self.applied = None;
        self.library_path = Some(path.clone());
        match WorkshopState::load(&path) {
            Ok(workshop) => {
                self.notice = workshop.registry().theme_def(DEFAULT_THEME).map(|_| "A saved theme uses Knot's reserved default identity. The artifact was preserved; choose New copy in Tabard and save that copy before applying it to Knot.".into());
                self.workshop = Some(workshop);
            },
            Err(error) => {
                self.workshop = None;
                self.notice = Some(format!(
                    "Could not open theme library {}: {error}. Repair the library and choose Reload theme library; its contents were preserved.",
                    path.display()
                ));
            },
        }
    }
    pub fn resolve(&self, appearance: &Appearance) -> Presentation {
        let choice = appearance_choice(appearance);
        if let Some((applied, presentation)) = &self.applied {
            if applied == &choice {
                return presentation.clone();
            }
        }
        if choice.theme_id != DEFAULT_THEME {
            if let Some(workshop) = &self.workshop {
                if workshop.registry().theme_def(&choice.theme_id).is_some() {
                    if let Ok(resolved) = resolve_theme_choice(workshop.registry(), &choice) {
                        let mut notice = (!resolved.diagnostics.is_empty()).then(|| {
                            resolved
                                .diagnostics
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(" ")
                        });
                        if matches!(
                            &resolved.presentation,
                            ThemePresentation::AuthoredStylesheet(_)
                        ) {
                            let graph_notice = "Authored CSS controls Knot's appearance. Graph nodes retain Knot's default palette because a stylesheet has no typed graph colors.";
                            notice = Some(match notice {
                                Some(previous) => format!("{previous} {graph_notice}"),
                                None => graph_notice.into(),
                            });
                        }
                        return Presentation {
                            theme: resolved.theme,
                            mode: resolved
                                .resolved
                                .theme_mode
                                .expect("resolved mode is explicit"),
                            presentation: resolved.presentation,
                            notice,
                        };
                    }
                }
            }
        }
        let theme = default_theme();
        let mut notices = Vec::new();
        if choice.theme_id != DEFAULT_THEME {
            notices.push(format!(
                "Theme {} is unavailable; showing Knot's default. Your saved choice is retained.",
                choice.theme_id
            ));
        }
        if let Some(mode @ Mode::Custom(_)) = &choice.theme_mode {
            notices.push(format!(
                "Mode {} is unavailable; showing Light. Your requested mode is retained.",
                mode.label()
            ));
        }
        let mode = choice
            .theme_mode
            .filter(|mode| !matches!(mode, Mode::Custom(_)))
            .unwrap_or(Mode::Light);
        Presentation {
            presentation: theme
                .presentation_for_mode(&mode)
                .expect("Knot's built-in seeds are valid"),
            theme,
            mode,
            notice: (!notices.is_empty()).then(|| notices.join(" ")),
        }
    }
    pub fn begin_edit(&mut self, appearance: &Appearance) -> Result<(), String> {
        let resolved = self.resolve(appearance);
        // A library save updates authored definitions; explicit application is
        // the point that changes this window's presentation of an existing ID.
        self.applied = Some((appearance_choice(appearance), resolved.clone()));
        let workshop = self
            .workshop
            .as_mut()
            .ok_or("Theme authoring is unavailable until the library can be read.")?;
        // Reopening a pending session keeps the exact draft and text fields.
        if !workshop.has_changes() {
            workshop.edit_definition(&resolved.theme, Some(resolved.mode))?;
        }
        workshop.cancel_close();
        self.close_app = false;
        self.workshop_open = true;
        self.request_stylesheet_update();
        self.notice = None;
        Ok(())
    }
    pub fn record_applied_choice(&mut self, appearance: &Appearance) {
        self.applied = None;
        let presentation = self.resolve(appearance);
        self.applied = Some((appearance_choice(appearance), presentation));
        self.request_stylesheet_update();
    }
    pub fn choice(&self, id: &str, mode: Option<Mode>) -> Result<ThemeChoice, String> {
        let theme = if id == DEFAULT_THEME {
            default_theme()
        } else {
            self.workshop
                .as_ref()
                .and_then(|w| w.registry().theme_def(id))
                .cloned()
                .ok_or("The selected theme is unavailable.")?
        };
        let mode = mode.unwrap_or_else(|| default_mode_for_def(&theme));
        theme
            .presentation_for_mode(&mode)
            .map_err(|error| error.to_string())?;
        Ok(ThemeChoice::new(theme.id, Some(mode)))
    }
}

fn appearance_choice(appearance: &Appearance) -> ThemeChoice {
    appearance.theme_choice.clone().unwrap_or_else(|| {
        ThemeChoice::new(
            DEFAULT_THEME,
            Some(Mode::from_flags(appearance.dark, false)),
        )
    })
}

pub fn load_library(state: &mut DesktopState) {
    let path = std::env::var_os("KNOT_THEME_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            directories::BaseDirs::new()
                .map(|dirs| dirs.data_local_dir().join("mere/tabard/themes.json"))
        });
    if let Some(path) = path {
        state.theme_session.load(path);
    } else {
        state.theme_session.notice =
            Some("No local data directory is available for theme authoring.".into());
    }
}

const UI_CSS: &str = ".knot-client-titlebar > .cambium-title-bar { width:100%;--titlebar-padding:0px;--titlebar-min-height:32px; } .knot-titlebar-actions { display:flex;align-items:center;gap:8px; } .knot-titlebar-mark { font-weight:700;border:1px solid currentColor;border-radius:4px;padding:3px 6px; } .knot-theme-options { display:flex;flex-wrap:wrap;gap:8px;margin:8px 0; } .knot-theme-notice { padding:8px; }";
const EDITOR_CSS: &str = ".desktop-frame { display:flex;flex-direction:column;height:100%;min-height:0;overflow:hidden; } .knot-workshop { display:flex;flex:1;flex-direction:column;min-height:0; } .knot-workshop > .tabard-workshop { flex:1;min-height:0; } .knot-workshop-controls { display:flex;align-items:center;gap:12px;padding:10px 18px;background:#ffffff;color:#253247; } .knot-workshop-controls button { padding:8px;border:1px solid #53637a;border-radius:4px; } .knot-theme-notice { background:#fff4d8;color:#493d25; }";
pub fn stylesheet(state: &DesktopState) -> String {
    if state.theme_session.workshop_open {
        return format!(
            "{}{}{}",
            workshop_stylesheet(),
            cambium::TITLE_BAR_CSS,
            EDITOR_CSS
        );
    }
    let resolved = state.theme_session.resolve(&state.appearance);
    let selected = match resolved.presentation {
        ThemePresentation::Derived(mode) => crate::appearance::selected_palette_css(&mode),
        ThemePresentation::AuthoredStylesheet(rules) => {
            let fallback = default_theme()
                .palette_for_mode(&Mode::from_flags(
                    resolved.mode.dark(),
                    resolved.mode.high_contrast(),
                ))
                .expect("canonical Knot fallback");
            format!(
                "{}\n{}",
                crate::appearance::authored_role_css(&fallback),
                rules.join("\n")
            )
        },
    };
    format!("{}{}{}", crate::desktop_sheet(), UI_CSS, selected)
}

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

pub fn workshop_screen(state: &DesktopState) -> DesktopView {
    let mut rows: Vec<DesktopView> = vec![Box::new(el("div", (
        button("Back to Knot", |state: &mut DesktopState, _| { state.theme_session.close_app = false; if let Some(workshop) = &mut state.theme_session.workshop { if workshop.request_close() { workshop.cancel_close(); state.theme_session.workshop_open = false; state.theme_session.request_stylesheet_update(); } } }).attr("data-action", "back-to-knot"),
        button("Apply to Knot", |state: &mut DesktopState, _| { let choice = state.theme_session.workshop.as_ref().ok_or("Theme authoring is unavailable.".into()).and_then(|workshop| {
            if workshop.draft_theme().id == DEFAULT_THEME { return Err("This saved artifact uses Knot's reserved default identity. Choose New copy and save it before applying.".into()); }
            workshop.saved_choice()
        }); state.select_theme_choice(choice); }).attr("data-action", "apply-to-knot"),
        span("Save your theme, then apply it to Knot. Editing leaves your document appearance selected."),
    )).attr("class", "knot-workshop-controls"))];
    if let Some(notice) = &state.theme_session.notice {
        rows.push(Box::new(
            el("p", span(notice.clone()))
                .attr("role", "status")
                .attr("class", "knot-theme-notice"),
        ));
    }
    let commands = state.window_commands();
    rows.push(Box::new(lens(
        move |workshop: &mut WorkshopState| {
            workshop_view_with_captions(
                workshop,
                cambium_genet_winit_host::platform_caption_controls(&commands, &caption_labels()),
            )
        },
        |state: &mut DesktopState| {
            state
                .theme_session
                .workshop
                .as_mut()
                .expect("mounted editor has a library")
        },
    )));
    Box::new(
        el("div", rows)
            .attr("class", "knot-workshop")
            .attr("data-surface", "knot.appearance-workshop.v1"),
    )
}

type Ctx<'a> = AppCtx<'a, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;
pub fn after_dispatch(ctx: &mut Ctx<'_>) {
    after_dispatch_with_exporter(ctx, |artifact| {
        choose_save_path(
            "Export theme",
            &artifact.suggested_name,
            &[artifact.format.extension()],
        )
    });
}
pub fn after_dispatch_with_exporter(
    ctx: &mut Ctx<'_>,
    mut destination: impl FnMut(&tabard_workshop::ExportArtifact) -> Option<PathBuf>,
) {
    let mut export = None;
    let mut close_app = false;
    if ctx.runner.state().theme_session.workshop_open {
        let paths = protected_export_paths(ctx.runner.state());
        ctx.runner.update(|state| {
            let session = &mut state.theme_session;
            if let Some(workshop) = &mut session.workshop {
                workshop.set_protected_export_paths(paths);
                workshop.sync_controls();
                export = workshop.take_export();
                if workshop.exit_requested() {
                    if workshop.has_changes() {
                        workshop.discard();
                    }
                    workshop.cancel_close();
                    session.workshop_open = false;
                    session.request_stylesheet_update();
                    close_app = std::mem::take(&mut session.close_app);
                } else if !workshop.close_requested() {
                    session.close_app = false;
                }
            }
        });
    }
    if let Some(artifact) = export {
        let path = destination(&artifact);
        ctx.runner.update(|state| {
            state
                .theme_session
                .workshop
                .as_mut()
                .expect("export belongs to mounted editor")
                .complete_export(artifact, path)
        });
    }
    if close_app
        && crate::workspace::close_request(ctx.runner, CloseRequest::Native)
            == CloseDisposition::Exit
    {
        *ctx.close = true;
    }
    if !ctx.runner.state().theme_session.workshop_open {
        crate::workspace::after_dispatch(ctx);
    }
}
// Only actual editor/application appearance transitions replace the sheet.
// Unrelated input retains the host's scroll state and embedding stylesheet.
pub(crate) fn update_stylesheet(ctx: &mut Ctx<'_>) {
    if !ctx.runner.state().theme_session.stylesheet_update {
        return;
    }
    ctx.runner
        .update(|state| state.theme_session.stylesheet_update = false);
    *ctx.set_sheet = Some(stylesheet(ctx.runner.state()));
}

pub fn close_request(ctx: &mut Ctx<'_>, request: CloseRequest) -> CloseDisposition {
    let mut allow = true;
    if ctx.runner.state().theme_session.workshop_open {
        ctx.runner.update(|state| {
            state.theme_session.close_app = true;
            if let Some(workshop) = &mut state.theme_session.workshop {
                allow = workshop.request_close();
                if allow {
                    workshop.cancel_close();
                    state.theme_session.workshop_open = false;
                    state.theme_session.close_app = false;
                    state.theme_session.request_stylesheet_update();
                }
            }
        });
    }
    if allow {
        crate::workspace::close_request(ctx.runner, request)
    } else {
        CloseDisposition::KeepVisible
    }
}

#[derive(Default)]
pub struct PreviewBindings {
    reader: Option<Rc<RefCell<SceneProducer<ReaderSpecimen>>>>,
    stylesheet: Option<Rc<RefCell<SceneProducer<StylesheetSpecimen>>>>,
}
impl PreviewBindings {
    pub fn frame(&mut self, ctx: &mut Ctx<'_>) -> bool {
        if !ctx.runner.state().theme_session.workshop_open {
            return crate::workspace::graph_frame(ctx);
        }
        let Some(state) = &ctx.runner.state().theme_session.workshop else {
            return false;
        };
        ctx.leaves
            .insert(GRAPH_LEAF_KEY, Box::new(state.graph_leaf()));
        let source = state.reader_preview();
        let reader = self.reader.get_or_insert_with(|| {
            Rc::new(RefCell::new(scene_producer(
                source.clone(),
                0x6b6e_6f74_7462_7264,
            )))
        });
        reader.borrow_mut().set_source(source);
        if !ctx.producers.contains(READER_LEAF_KEY) {
            ctx.producers
                .register(READER_LEAF_KEY, reader.clone(), &[])
                .expect("bounded Tabard reader key");
        }
        let source = state.stylesheet_preview();
        let stylesheet = self.stylesheet.get_or_insert_with(|| {
            Rc::new(RefCell::new(scene_producer(
                source.clone(),
                0x6b6e_6f74_7462_6373,
            )))
        });
        stylesheet.borrow_mut().set_source(source);
        if !ctx.producers.contains(STYLESHEET_LEAF_KEY) {
            ctx.producers
                .register(STYLESHEET_LEAF_KEY, stylesheet.clone(), &[])
                .expect("bounded Tabard CSS key");
        }
        false
    }
}
fn scene_producer<T: PreviewScene>(source: Rc<RefCell<T>>, key: u64) -> SceneProducer<T> {
    SceneProducer::new(source, key, T::frame, T::revision, |source| {
        Some(ProducerSemantics {
            role: Some(ProducerRole::Image),
            name: Some(source.accessible_name().into()),
            children: Vec::new(),
        })
    })
}

// Knot supplies its file authorities; Tabard owns identity checks for both
// initial exports and explicit replacement, including symlink/path aliases.
fn protected_export_paths(state: &DesktopState) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(store) = state.preferences() {
        paths.push(store.path().to_path_buf());
    }
    paths.extend(state.docs.docs().filter_map(|(_, entry)| {
        entry
            .document
            .session()
            .source_path()
            .map(std::path::Path::to_path_buf)
    }));
    paths
}

pub(crate) fn caption_labels() -> cambium_genet_winit_host::CaptionLabels {
    cambium_genet_winit_host::CaptionLabels {
        minimize: "Minimize window".into(),
        maximize: "Maximize window".into(),
        close: "Close window".into(),
    }
}
