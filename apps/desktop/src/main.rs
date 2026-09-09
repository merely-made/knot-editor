// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Thin standalone host for the reusable Knot document surface.
use knot_desktop::{run_desktop_with_targets, workspace};

use knot_document::KnotDocumentSession;
use knot_file_catalog::KnotFileCatalog;
use std::ffi::OsString;
use std::path::PathBuf;
use workspace::DEFAULT_CAPTURE_MAX_BYTES;
const SCRATCH_ADDRESS: &str = "scratch:untitled";
#[derive(Debug, PartialEq, Eq)]
enum DocumentSelection {
    Scratch,
    File(PathBuf),
}
#[derive(Debug, PartialEq, Eq)]
struct CatalogOptions {
    root: PathBuf,
    path: PathBuf,
}
#[derive(Debug, PartialEq, Eq)]
struct LaunchOptions {
    document: DocumentSelection,
    catalog: Option<CatalogOptions>,
    capture_max_bytes: usize,
}
fn select_document<I: IntoIterator<Item = OsString>>(args: I) -> Result<LaunchOptions, String> {
    let mut args = args.into_iter();
    let _ = args.next();
    let mut path = None;
    let mut root = None;
    let mut catalog_path = None;
    let mut capture_max_bytes = None;
    let mut positional_only = false;
    while let Some(arg) = args.next() {
        if !positional_only && arg == "--" {
            positional_only = true;
        } else if !positional_only && arg == "--capture-max-bytes" {
            if capture_max_bytes.is_some() {
                return Err("--capture-max-bytes was supplied more than once".into());
            }
            let value = args
                .next()
                .ok_or("--capture-max-bytes requires a byte count")?;
            let value = value
                .to_str()
                .filter(|value| {
                    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                })
                .ok_or("--capture-max-bytes requires a non-negative integer byte count")?;
            capture_max_bytes = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "--capture-max-bytes exceeds the supported byte count")?,
            );
        } else if !positional_only && (arg == "--catalog-root" || arg == "--catalog") {
            let slot = if arg == "--catalog-root" {
                &mut root
            } else {
                &mut catalog_path
            };
            if slot.is_some() {
                return Err(format!(
                    "{} was supplied more than once",
                    arg.to_string_lossy()
                ));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{} requires a path", arg.to_string_lossy()))?;
            if value.is_empty()
                || value == "--catalog-root"
                || value == "--catalog"
                || value == "--capture-max-bytes"
                || value == "--"
            {
                return Err(format!("{} requires a path", arg.to_string_lossy()));
            }
            *slot = Some(PathBuf::from(value));
        } else if !positional_only && arg.to_string_lossy().starts_with('-') {
            return Err(format!(
                "unknown option {}; use -- before a document path starting with -",
                arg.to_string_lossy()
            ));
        } else if path.replace(PathBuf::from(arg)).is_some() {
            return Err("expected zero or one document path".into());
        }
    }
    let catalog = match (root, catalog_path) {
        (None, None) => None,
        (Some(root), Some(path)) => Some(CatalogOptions { root, path }),
        _ => return Err("--catalog-root and --catalog must be supplied together".into()),
    };
    if capture_max_bytes.is_some() && catalog.is_none() {
        return Err("--capture-max-bytes requires --catalog-root and --catalog".into());
    }
    Ok(LaunchOptions {
        document: path
            .map(DocumentSelection::File)
            .unwrap_or(DocumentSelection::Scratch),
        catalog,
        capture_max_bytes: capture_max_bytes.unwrap_or(DEFAULT_CAPTURE_MAX_BYTES),
    })
}
fn open_selection(selection: DocumentSelection) -> Result<KnotDocumentSession, String> {
    match selection {
        DocumentSelection::Scratch => Ok(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "")),
        DocumentSelection::File(path) => KnotDocumentSession::open(path),
    }
}
fn main() {
    let options = select_document(std::env::args_os()).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    let catalog = options
        .catalog
        .map(|options| KnotFileCatalog::open(options.root, options.path))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("knot: catalog could not be opened: {error}");
            std::process::exit(1)
        });
    let initial_path = match &options.document {
        DocumentSelection::Scratch => None,
        DocumentSelection::File(path) => Some(path.clone()),
    };
    let session = open_selection(options.document).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    if let Err(error) = run_desktop_with_targets(
        session,
        initial_path,
        catalog,
        options.capture_max_bytes,
        vec![],
    ) {
        eprintln!("knot: host failed: {error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::Init;
    use cambium_genet_winit_host::{CloseRequest, Harness, KeyPress, Modifiers, NamedKey};
    use genet_probe::Selector;
    use knot_desktop::host_hooks;
    use knot_document::KNOT_DOCUMENT_CSS;
    use tempfile::tempdir;
    use workspace::{DESKTOP_CSS, DesktopState, DesktopView, desktop_view};
    fn launch(args: &[&str]) -> Result<LaunchOptions, String> {
        select_document(args.iter().map(OsString::from))
    }
    #[test]
    fn launch_options_keep_catalog_explicit_and_preserve_document_paths() {
        assert_eq!(
            launch(&["knot"]).unwrap(),
            LaunchOptions {
                document: DocumentSelection::Scratch,
                catalog: None,
                capture_max_bytes: DEFAULT_CAPTURE_MAX_BYTES,
            }
        );
        assert_eq!(
            launch(&["knot", "my essay.djot"]).unwrap().document,
            DocumentSelection::File(PathBuf::from("my essay.djot"))
        );
        let options = launch(&[
            "knot",
            "--catalog-root",
            "notes",
            "--catalog",
            "metadata/catalog.redb",
            "--",
            "--essay.djot",
        ])
        .unwrap();
        assert_eq!(
            options.document,
            DocumentSelection::File(PathBuf::from("--essay.djot"))
        );
        assert_eq!(
            options.catalog,
            Some(CatalogOptions {
                root: PathBuf::from("notes"),
                path: PathBuf::from("metadata/catalog.redb")
            })
        );
    }
    #[test]
    fn launch_options_refuse_partial_duplicate_or_ambiguous_configuration() {
        for args in [
            vec!["knot", "--catalog"],
            vec!["knot", "--catalog", "metadata/catalog.redb"],
            vec!["knot", "--catalog-root", "notes"],
            vec![
                "knot",
                "--catalog-root",
                "--catalog",
                "metadata/catalog.redb",
            ],
            vec!["knot", "--catalog-root", "notes", "--catalog-root", "other"],
            vec!["knot", "--unknown"],
            vec!["knot", "one.djot", "two.djot"],
        ] {
            assert!(launch(&args).is_err(), "unexpectedly accepted {args:?}");
        }
    }
    #[test]
    fn capture_byte_limit_is_explicit_bounded_and_requires_a_catalog() {
        let base = [
            "knot",
            "--catalog-root",
            "notes",
            "--catalog",
            "metadata/catalog.redb",
        ];
        assert_eq!(
            launch(&base).unwrap().capture_max_bytes,
            DEFAULT_CAPTURE_MAX_BYTES
        );
        for (value, expected) in [("0", 0), ("4096", 4096)] {
            let args = base
                .into_iter()
                .chain(["--capture-max-bytes", value])
                .collect::<Vec<_>>();
            assert_eq!(launch(&args).unwrap().capture_max_bytes, expected);
        }
        for value in ["", "-1", "+1", "1.5", " 10", "184467440737095516160"] {
            let args = base
                .into_iter()
                .chain(["--capture-max-bytes", value])
                .collect::<Vec<_>>();
            assert!(launch(&args).is_err(), "accepted invalid count {value:?}");
        }
        let repeated = base
            .into_iter()
            .chain(["--capture-max-bytes", "1", "--capture-max-bytes", "2"])
            .collect::<Vec<_>>();
        assert!(launch(&repeated).is_err());
        let missing = base
            .into_iter()
            .chain(["--capture-max-bytes"])
            .collect::<Vec<_>>();
        assert!(launch(&missing).is_err());
        assert!(launch(&["knot", "--capture-max-bytes", "1"]).is_err());
        assert!(launch(&["knot", "--catalog", "--capture-max-bytes", "1"]).is_err());
        assert_eq!(
            launch(&["knot", "--", "--capture-max-bytes"])
                .unwrap()
                .document,
            DocumentSelection::File(PathBuf::from("--capture-max-bytes"))
        );
    }
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
