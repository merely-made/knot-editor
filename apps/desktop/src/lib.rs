// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Desktop workspace with destinations supplied by an existing resident owner.
//! The host binds persona display identities to granted retention capabilities
//! before launch. This entrypoint never opens a vault or creates an identity.
pub mod appearance;
pub mod changes;
pub mod collapse;
pub mod composition;
pub mod document_folding;
mod document_links;
pub mod document_preview;
pub mod documents;
pub mod fonts;
pub mod font_roles;
pub mod graph;
pub mod knot_relations;
#[cfg(feature = "mere-retention")]
mod mere_overlay;
mod link_workflow;
pub mod navigator;
pub mod preferences;
pub mod readings;
pub mod recovery;
mod recovery_runtime;
pub mod scenario;
mod scenario_startup;
pub mod scroll_site;
pub mod status;
pub mod theme_session;
pub mod workspace;

use cambium::{
    COMMAND_MENU_BAR_CSS, FOLD_ROWS_CSS, FRISKET_CSS, GRAPH_CANVAS_SWATCH_CSS, POPOVER_CSS,
    STATUS_BAR_CSS, WORKSPACE_CSS,
};
use cambium_genet_winit_host::{HostHooks, HostOptions, Init, WindowFrame, inert_hooks, run};
use knot_capture::KnotRetainPort;
use knot_document::{KNOT_DOCUMENT_CSS, KnotDocumentSession};
use knot_file_catalog::KnotFileCatalog;
use mesquite::LaneConfig;
use std::{path::PathBuf, sync::Arc};
use workspace::{DESKTOP_CSS, DesktopState, DesktopView, desktop_view};

pub fn host_hooks() -> HostHooks<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
    let mut hooks: HostHooks<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> = inert_hooks();
    let mut previews = theme_session::PreviewBindings::default();
    // Rootstock clears the current redraw receipt before the pre-frame hook.
    // Observe its result after-frame instead, retaining only session-local
    // eligibility for the following attempt. A failed presentation freezes
    // motion on the next frame and never silently resumes it.
    let last_presented = std::rc::Rc::new(std::cell::Cell::new(false));
    let frame_presented = last_presented.clone();
    hooks.frame = Box::new(move |ctx| {
        if ctx.runner.state().theme_session.workshop_open
            || (ctx.window.is_some() && !frame_presented.get())
        {
            crate::composition::pause_hidden_motion(ctx);
        }
        theme_session::update_stylesheet(ctx);
        previews.frame(ctx)
    });
    hooks.after_frame = Box::new(move |ctx| {
        last_presented.set(ctx.presentation.is_some());
    });
    hooks.after_dispatch = Box::new(move |ctx| {
        theme_session::after_dispatch(ctx);
        theme_session::update_stylesheet(ctx);
        crate::composition::reveal_motion_preview(ctx);
    });
    hooks.after_wake = Box::new(workspace::after_wake);
    hooks.close_request = Box::new(|ctx, request| {
        let disposition = theme_session::close_request(ctx, request);
        theme_session::update_stylesheet(ctx);
        disposition
    });
    hooks.focused_text = Box::new(workspace::focused_text);
    hooks.key_intercept = Box::new(workspace::key_intercept);
    hooks
}

/// The desktop stylesheet, in cascade order: Cambium's frame first, so Knot's
/// rules dress it.
pub fn desktop_sheet() -> String {
    format!(
        "{}{FRISKET_CSS}{WORKSPACE_CSS}{POPOVER_CSS}{COMMAND_MENU_BAR_CSS}{STATUS_BAR_CSS}{FOLD_ROWS_CSS}{GRAPH_CANVAS_SWATCH_CSS}{}{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}{}{}{}{}{}{}",
        cambium::TITLE_BAR_CSS,
        mere_view::MERE_VIEW_CSS,
        appearance::appearance_css(),
        document_folding::CSS,
        document_preview::CSS,
        navigator::CSS,
        readings::CSS,
        scroll_site::CSS
    )
}

/// The documents a launch opens, in the order they were named.
pub struct DesktopLaunch {
    /// The document in front: the first path that opened, else a scratch.
    pub first: KnotDocumentSession,
    /// The path it was named by, which the path field shows. A site folder
    /// here also opens that site, whose index page `first` is.
    pub first_path: Option<PathBuf>,
    /// The documents named after it, opened behind it in order.
    pub behind: Vec<KnotDocumentSession>,
    /// Site folders named after the first path; their index pages are among
    /// `behind`. A leading site folder is attached through `first_path`.
    pub sites: Vec<PathBuf>,
    /// One line for each path that would not open.
    pub failures: Vec<String>,
    /// Private local recovery root. None keeps optional embeddings free of
    /// plaintext recovery writes until their host grants that policy.
    pub recovery_path: Option<PathBuf>,
    /// A resident mere's domain to read as the overmap layer above the
    /// catalog graph: Djinn's `mere/<domain>` route, as `knot-editor`. None
    /// leaves the layer empty.
    pub mere_overlay: Option<String>,
}

pub fn run_desktop_with_targets(
    launch: DesktopLaunch,
    catalog: Option<KnotFileCatalog>,
    capture_max_bytes: usize,
    readings_root: Option<PathBuf>,
    preferences_path: Option<PathBuf>,
    targets: Vec<Arc<dyn KnotRetainPort>>,
    composition_targets: Vec<Arc<dyn knot_composition::retention::CompositionRetainPort>>,
    titan_submission_error: Option<String>,
) -> Result<(), String> {
    run_desktop_with_targets_and_relations(
        launch,
        catalog,
        capture_max_bytes,
        readings_root,
        preferences_path,
        targets,
        composition_targets,
        titan_submission_error,
        None,
    )
}

/// Desktop entrypoint that additionally receives an already authenticated,
/// read-only personal relation projection adapted for the opened-link view.
pub fn run_desktop_with_targets_and_relations(
    launch: DesktopLaunch,
    catalog: Option<KnotFileCatalog>,
    capture_max_bytes: usize,
    readings_root: Option<PathBuf>,
    preferences_path: Option<PathBuf>,
    targets: Vec<Arc<dyn KnotRetainPort>>,
    composition_targets: Vec<Arc<dyn knot_composition::retention::CompositionRetainPort>>,
    titan_submission_error: Option<String>,
    knot_relations: Option<knot_relations::KnotRelationView>,
) -> Result<(), String> {
    let mut hooks = host_hooks();
    let lane_config = LaneConfig::from_env("KNOT");
    let receipt = lane_config.is_some();
    if let Some(config) = lane_config {
        let initial_grace = scenario_startup::initial_presentation_grace(
            true,
            launch.first_path.as_deref(),
            || std::env::var_os(scenario_startup::INITIAL_GRACE_ENV),
        );
        if let Some(seconds) = initial_grace {
            eprintln!(
                "[knot] marked synthetic acceptance fixture: initial presentation grace {seconds}s; restores to 10s on first successful presentation"
            );
        }
        let initial_timeout = std::time::Duration::from_secs(
            initial_grace.unwrap_or(scenario_startup::NORMAL_PRESENTATION_TIMEOUT_SECS),
        );
        let mut lane = Some(
            mesquite::Lane::from_config(
                config,
                scenario::KnotLane::new(desktop_sheet()),
                cambium_genet_winit_host::read_file,
            )?
            .with_presentation_timeout(initial_timeout),
        );
        let mut grace = scenario_startup::PresentationGrace::new(initial_grace);
        let mut presentation_result = hooks.after_frame;
        hooks.after_frame = Box::new(move |ctx| {
            presentation_result(ctx);
            grace.before_lane_after_frame(ctx.presentation.is_some(), |timeout| {
                let current = lane.take().expect("acceptance lane is retained");
                lane = Some(current.with_presentation_timeout(timeout));
                eprintln!(
                    "[knot] first successful presentation observed; initial grace ended, normal 10s presentation timeout restored"
                );
            });
            lane.as_mut()
                .expect("acceptance lane is retained")
                .after_frame(ctx);
        });
    }
    run(
        HostOptions {
            title: "Knot".into(),
            window_frame: if cfg!(target_os = "macos") {
                WindowFrame::Host
            } else {
                WindowFrame::App
            },
            maximize_control_label: "Maximize window".into(),
            initial_logical_size: (1100.0, 700.0),
            // Explicit acceptance sizes use the host's normal window-size
            // seam. Without a scenario, ordinary startup is unchanged.
            size_env: receipt.then(|| ("KNOT_TEST_WIDTH".into(), "KNOT_TEST_HEIGHT".into())),
            ..HostOptions::default()
        },
        move |_, commands, wake| {
            let DesktopLaunch {
                first,
                first_path,
                behind,
                sites,
                failures,
                recovery_path,
                mere_overlay,
            } = launch;
            let mut state =
                DesktopState::with_catalog(first, commands.clone(), first_path, catalog);
            state.set_knot_relation_view(knot_relations);
            for folder in sites {
                state.attach_site(&folder);
            }
            state
                .scroll
                .set_titan_submission_error(titan_submission_error.clone());
            if let Some(error) = &titan_submission_error {
                state.message = Some(format!("Titan upload disabled: {error}"));
            }
            state.set_capture_limit(capture_max_bytes);
            state.set_readings_root(readings_root.clone());
            state.set_preferences_path(preferences_path.clone());
            theme_session::load_library(&mut state);
            state.set_recovery_path(recovery_path, wake.clone());
            if let Some(domain) = mere_overlay {
                state.attach_mere_overlay(domain, wake.clone());
            }
            state.set_retention_targets(targets, wake.clone());
            state.set_composition_targets(composition_targets, wake.clone());
            state.open_behind(behind, &failures);
            let sheet = theme_session::stylesheet(&state);
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet,
                fonts: fonts::bundled_fonts(),
                images: Vec::new(),
            }
        },
        hooks,
    )
    .map_err(|error| error.to_string())
}
