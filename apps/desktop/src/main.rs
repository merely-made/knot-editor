// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Thin standalone host for the reusable Knot document surface.
use knot_desktop::{DesktopLaunch, run_desktop_with_targets, workspace};

use knot_document::KnotDocumentSession;
use knot_file_catalog::KnotFileCatalog;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use workspace::DEFAULT_CAPTURE_MAX_BYTES;
const SCRATCH_ADDRESS: &str = "scratch:untitled";
#[derive(Debug, PartialEq, Eq)]
struct CatalogOptions {
    root: PathBuf,
    path: PathBuf,
}
#[derive(Debug, PartialEq, Eq)]
struct LaunchOptions {
    /// Document paths in the order named; none opens a scratch document.
    documents: Vec<PathBuf>,
    catalog: Option<CatalogOptions>,
    capture_max_bytes: usize,
}
fn select_document<I: IntoIterator<Item = OsString>>(args: I) -> Result<LaunchOptions, String> {
    let mut args = args.into_iter();
    let _ = args.next();
    let mut documents = Vec::new();
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
        } else {
            documents.push(PathBuf::from(arg));
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
        documents,
        catalog,
        capture_max_bytes: capture_max_bytes.unwrap_or(DEFAULT_CAPTURE_MAX_BYTES),
    })
}
/// Open the named documents in order, the first in front. A site folder
/// opens its index page, and one folder is the limit until sites get their
/// own tiles. A path that fails is reported unless none opens.
fn open_documents(paths: Vec<PathBuf>) -> Result<DesktopLaunch, String> {
    if paths.iter().filter(|path| path.is_dir()).count() > 1 {
        return Err("one site folder at a time until sites get their own tiles".into());
    }
    let mut opened = Vec::new();
    let mut failures = Vec::new();
    for path in paths {
        match open_path(&path) {
            Ok(session) => opened.push((session, path)),
            Err(error) => {
                // Name the path unless the error already does.
                let shown = path.display().to_string();
                failures.push(if error.contains(&shown) {
                    error
                } else {
                    format!("{shown}: {error}")
                });
            },
        }
    }
    if opened.is_empty() && !failures.is_empty() {
        return Err(failures.join("; "));
    }
    let site = opened
        .iter()
        .skip(1)
        .find(|(_, path)| path.is_dir())
        .map(|(_, path)| path.clone());
    let mut opened = opened.into_iter();
    let (first, first_path) = match opened.next() {
        Some((session, path)) => (session, Some(path)),
        None => (KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""), None),
    };
    Ok(DesktopLaunch {
        first,
        first_path,
        behind: opened.map(|(session, _)| session).collect(),
        site,
        failures,
    })
}
fn open_path(path: &Path) -> Result<KnotDocumentSession, String> {
    if path.is_dir() {
        let site = knot_site::Site::open(path)?;
        KnotDocumentSession::open(site.page_path(site.config.format.index_file())?)
    } else {
        KnotDocumentSession::open(path)
    }
}
fn main() {
    let options = select_document(std::env::args_os()).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    let settings_root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("Knot");
    let titan_submission_error =
        knot_site::submission::initialize_submission_trust(&settings_root.join("titan-trust.json"))
            .err();
    let catalog = options
        .catalog
        .map(|options| KnotFileCatalog::open(options.root, options.path))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("knot: catalog could not be opened: {error}");
            std::process::exit(1)
        });
    let launch = open_documents(options.documents).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    if let Err(error) = run_desktop_with_targets(
        launch,
        catalog,
        options.capture_max_bytes,
        Some(settings_root.join("readings")),
        Some(settings_root.join(knot_desktop::preferences::PREFERENCES_FILE)),
        vec![],
        titan_submission_error,
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
    use knot_desktop::host_hooks;
    use knot_document::KNOT_DOCUMENT_CSS;
    use taproot::Selector;
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
                documents: Vec::new(),
                catalog: None,
                capture_max_bytes: DEFAULT_CAPTURE_MAX_BYTES,
            }
        );
        assert_eq!(
            launch(&["knot", "my essay.djot"]).unwrap().documents,
            [PathBuf::from("my essay.djot")]
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
        assert_eq!(options.documents, [PathBuf::from("--essay.djot")]);
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
                .documents,
            [PathBuf::from("--capture-max-bytes")]
        );
    }
    #[test]
    fn several_document_paths_keep_their_order() {
        assert_eq!(
            launch(&["knot", "a.djot", "b.gmi", "--", "-c.djot"])
                .unwrap()
                .documents,
            [
                PathBuf::from("a.djot"),
                PathBuf::from("b.gmi"),
                PathBuf::from("-c.djot")
            ]
        );
    }
    #[test]
    fn launch_opens_every_path_it_can_and_reports_the_rest() {
        let temp = tempdir().unwrap();
        let first = temp.path().join("first.djot");
        let second = temp.path().join("second.djot");
        std::fs::write(&first, "first\n").unwrap();
        std::fs::write(&second, "second\n").unwrap();
        let missing = temp.path().join("missing.djot");
        let opened = open_documents(vec![missing.clone(), first.clone(), second.clone()]).unwrap();
        assert_eq!(opened.first.snapshot().text, "first\n");
        assert_eq!(opened.first_path.as_deref(), Some(first.as_path()));
        assert_eq!(opened.behind.len(), 1);
        assert_eq!(opened.behind[0].snapshot().text, "second\n");
        assert_eq!(opened.failures.len(), 1);
        assert!(opened.failures[0].contains("missing.djot"));

        let error = open_documents(vec![missing]).err().unwrap();
        assert!(error.contains("missing.djot"), "{error}");

        let scratch = open_documents(Vec::new()).unwrap();
        assert_eq!(scratch.first.snapshot().source.address, SCRATCH_ADDRESS);
        assert!(scratch.first_path.is_none() && scratch.behind.is_empty());
    }
    #[test]
    fn launch_takes_one_site_folder_until_sites_get_tiles() {
        let temp = tempdir().unwrap();
        let one = temp.path().join("one");
        let two = temp.path().join("two");
        knot_site::Site::create_for(&one, knot_site::SiteFormat::Scroll).unwrap();
        knot_site::Site::create_for(&two, knot_site::SiteFormat::Scroll).unwrap();
        let error = open_documents(vec![one.clone(), two]).err().unwrap();
        assert_eq!(
            error,
            "one site folder at a time until sites get their own tiles"
        );

        let loose = temp.path().join("loose.djot");
        std::fs::write(&loose, "loose\n").unwrap();
        let opened = open_documents(vec![loose, one.clone()]).unwrap();
        assert_eq!(opened.site.as_deref(), Some(one.as_path()));
        assert_eq!(opened.behind.len(), 1);
        assert_eq!(opened.behind[0].snapshot().display_label, "index.scroll");
        let front = open_documents(vec![one.clone()]).unwrap();
        assert_eq!(front.first_path.as_deref(), Some(one.as_path()));
        assert!(
            front.site.is_none(),
            "a leading folder opens through first_path"
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
            sheet: format!(
                "{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}{}{}{}",
                knot_desktop::appearance::appearance_css(),
                knot_desktop::document_preview::CSS,
                knot_desktop::readings::CSS
            ),
            fonts: Vec::new(),
            images: Vec::new(),
        };
        let mut harness = Harness::with_hooks(init, host_hooks());
        harness.layout_at(900.0, 640.0);
        // Highlighted text is nested in spans. Target the field's stable
        // accessible name rather than the probe's direct-child text matcher.
        assert!(harness.click_on(&Selector::role("textbox").containing("Document text")));
        assert!(harness.focus().is_some());
        harness.key_injected("Body");
        harness.press_key(&KeyPress::named(NamedKey::Enter));
        assert!(harness.state().document().snapshot().dirty);
        harness.set_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        harness.key_char("s");
        assert!(!harness.state().document().snapshot().dirty);
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
