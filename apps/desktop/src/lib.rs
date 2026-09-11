// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Desktop workspace with destinations supplied by an existing resident owner.
//! The host binds persona display identities to granted retention capabilities
//! before launch. This entrypoint never opens a vault or creates an identity.
pub mod appearance;
pub mod scroll_site;
pub mod workspace;

use cambium_genet_winit_host::{HostHooks, HostOptions, Init, inert_hooks, run};
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
pub fn run_desktop_with_targets(
    session: KnotDocumentSession,
    initial_path: Option<PathBuf>,
    catalog: Option<KnotFileCatalog>,
    capture_max_bytes: usize,
    targets: Vec<Arc<dyn KnotRetainPort>>,
) -> Result<(), String> {
    run(
        HostOptions {
            title: "Knot".into(),
            initial_logical_size: (1100.0, 700.0),
            ..HostOptions::default()
        },
        move |_, commands, wake| {
            let mut state =
                DesktopState::with_catalog(session, commands.clone(), initial_path, catalog);
            state.set_capture_limit(capture_max_bytes);
            state.set_retention_targets(targets, wake.clone());
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: format!(
                    "{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}{}{}",
                    appearance::appearance_css(),
                    scroll_site::CSS
                ),
            }
        },
        host_hooks(),
    )
    .map_err(|error| error.to_string())
}
