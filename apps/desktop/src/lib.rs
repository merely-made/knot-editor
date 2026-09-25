// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Desktop workspace with destinations supplied by an existing resident owner.
//! The host binds persona display identities to granted retention capabilities
//! before launch. This entrypoint never opens a vault or creates an identity.
pub mod appearance;
pub mod changes;
pub mod document_folding;
pub mod document_preview;
pub mod documents;
pub mod preferences;
pub mod readings;
pub mod scenario;
pub mod scroll_site;
pub mod workspace;

use cambium::{FOLD_ROWS_CSS, FRISKET_CSS, POPOVER_CSS, WORKSPACE_CSS};
use cambium_genet_winit_host::{
    HostHooks, HostOptions, Init, LaneConfig, ScenarioLane, inert_hooks, run,
};
use knot_capture::KnotRetainPort;
use knot_document::{KNOT_DOCUMENT_CSS, KnotDocumentSession};
use knot_file_catalog::KnotFileCatalog;
use std::{path::PathBuf, sync::Arc};
use workspace::{DESKTOP_CSS, DesktopState, DesktopView, desktop_view};

pub fn host_hooks() -> HostHooks<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
    let mut hooks = inert_hooks();
    hooks.after_dispatch = Box::new(workspace::after_dispatch);
    hooks.after_wake = Box::new(workspace::after_wake);
    hooks.close_request = Box::new(|ctx, request| workspace::close_request(ctx.runner, request));
    hooks.focused_text = Box::new(workspace::focused_text);
    hooks.key_intercept = Box::new(workspace::key_intercept);
    hooks
}

/// The desktop stylesheet, in cascade order: Cambium's frame first, so Knot's
/// rules dress it.
pub fn desktop_sheet() -> String {
    format!(
        "{FRISKET_CSS}{WORKSPACE_CSS}{POPOVER_CSS}{FOLD_ROWS_CSS}{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}{}{}{}{}{}",
        appearance::appearance_css(),
        document_folding::CSS,
        document_preview::CSS,
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
    /// A site folder named after the first path; its index page is among
    /// `behind`.
    pub site: Option<PathBuf>,
    /// One line for each path that would not open.
    pub failures: Vec<String>,
}

pub fn run_desktop_with_targets(
    launch: DesktopLaunch,
    catalog: Option<KnotFileCatalog>,
    capture_max_bytes: usize,
    readings_root: Option<PathBuf>,
    preferences_path: Option<PathBuf>,
    targets: Vec<Arc<dyn KnotRetainPort>>,
    titan_submission_error: Option<String>,
) -> Result<(), String> {
    let mut hooks = host_hooks();
    if let Some(config) = LaneConfig::from_env("KNOT") {
        let mut lane = ScenarioLane::new(config, scenario::KnotLane::new(desktop_sheet()))?;
        hooks.after_frame = Box::new(move |ctx| lane.drive(ctx));
    }
    run(
        HostOptions {
            title: "Knot".into(),
            initial_logical_size: (1100.0, 700.0),
            ..HostOptions::default()
        },
        move |_, commands, wake| {
            let DesktopLaunch {
                first,
                first_path,
                behind,
                site,
                failures,
            } = launch;
            let mut state =
                DesktopState::with_catalog(first, commands.clone(), first_path, catalog);
            if let Some(folder) = site {
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
            state.set_retention_targets(targets, wake.clone());
            state.open_behind(behind, &failures);
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            }
        },
        hooks,
    )
    .map_err(|error| error.to_string())
}
