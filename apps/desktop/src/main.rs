// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Thin standalone host for the reusable Knot document surface.
mod workspace;

use cambium_genet_winit_host::{HostHooks, HostOptions, Init, inert_hooks, run};
use knot_document::{KNOT_DOCUMENT_CSS, KnotDocumentSession};
use std::ffi::OsString;
use std::path::PathBuf;
use workspace::{DESKTOP_CSS, DesktopState, DesktopView, desktop_view};
const SCRATCH_ADDRESS: &str = "scratch:untitled";
#[derive(Debug, PartialEq, Eq)]
enum DocumentSelection {
    Scratch,
    File(PathBuf),
}
fn select_document<I: IntoIterator<Item = OsString>>(args: I) -> Result<DocumentSelection, String> {
    let mut args = args.into_iter();
    let _ = args.next();
    let path = args.next();
    if args.next().is_some() {
        return Err("expected zero or one document path".into());
    }
    Ok(path
        .map(|path| DocumentSelection::File(PathBuf::from(path)))
        .unwrap_or(DocumentSelection::Scratch))
}
fn open_selection(selection: DocumentSelection) -> Result<KnotDocumentSession, String> {
    match selection {
        DocumentSelection::Scratch => Ok(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "")),
        DocumentSelection::File(path) => KnotDocumentSession::open(path),
    }
}
fn host_hooks() -> HostHooks<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
    let mut hooks = inert_hooks();
    hooks.close_request = Box::new(|ctx, request| workspace::close_request(ctx.runner, request));
    hooks.focused_text = Box::new(workspace::focused_text);
    hooks.key_intercept = Box::new(workspace::key_intercept);
    hooks
}
fn run_standalone(
    session: KnotDocumentSession,
    initial_path: Option<PathBuf>,
) -> Result<(), String> {
    run(
        HostOptions {
            title: "Knot".into(),
            initial_logical_size: (1100.0, 700.0),
            ..HostOptions::default()
        },
        move |_, commands, _| Init {
            state: DesktopState::with_path(session, commands.clone(), initial_path),
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: format!("{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}"),
        },
        host_hooks(),
    )
    .map_err(|error| error.to_string())
}
fn main() {
    let selection = select_document(std::env::args_os()).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    let initial_path = match &selection {
        DocumentSelection::Scratch => None,
        DocumentSelection::File(path) => Some(path.clone()),
    };
    let session = open_selection(selection).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    if let Err(error) = run_standalone(session, initial_path) {
        eprintln!("knot: host failed: {error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::{CloseRequest, Harness, KeyPress, Modifiers, NamedKey};
    use genet_probe::Selector;
    use tempfile::tempdir;
    #[test]
    fn app_authored_open_edit_save_close_reopen_receipt() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("receipt.djot");
        std::fs::write(&path, "# Receipt\n").unwrap();
        let init = Init {
            state: DesktopState::with_path(
                KnotDocumentSession::open(&path).unwrap(),
                cambium_genet_winit_host::WindowCommands::new(),
                Some(path.clone()),
            ),
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: format!("{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}"),
        };
        let mut harness = Harness::with_hooks(init, host_hooks());
        harness.layout_at(900.0, 640.0);
        assert!(harness.click_on(&Selector::role("textbox").containing("Receipt")));
        assert!(harness.focus().is_some());
        harness.key_injected("Body");
        harness.press_key(&KeyPress::named(NamedKey::Enter));
        assert!(harness.state().document.snapshot().dirty);
        harness.set_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        harness.key_char("s");
        assert!(!harness.state().document.snapshot().dirty);
        harness.request_close(CloseRequest::Native);
        assert!(harness.close_requested());
        drop(harness);
        assert!(
            KnotDocumentSession::open(&path)
                .unwrap()
                .snapshot()
                .text
                .contains("Body")
        );
    }
}
