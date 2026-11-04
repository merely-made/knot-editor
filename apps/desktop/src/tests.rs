// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

#[test]
fn site_history_requires_an_explicit_single_directory() {
    assert!(
        select_document(["knot"].map(OsString::from))
            .unwrap()
            .site_history
            .is_none()
    );
    assert_eq!(
        select_document(["knot", "--site-history", "history", "site"].map(OsString::from))
            .unwrap()
            .site_history,
        Some(PathBuf::from("history"))
    );
    for args in [
        vec!["knot", "--site-history"],
        vec!["knot", "--site-history", "--mere-root"],
        vec![
            "knot",
            "--catalog-root",
            "--site-history",
            "history",
            "--catalog",
            "catalog",
        ],
        vec!["knot", "--site-history", "a", "--site-history", "b"],
        vec!["knot", "--site-history-revisions", "2"],
        vec![
            "knot",
            "--site-history",
            "a",
            "--site-history-revisions",
            "0",
        ],
        vec![
            "knot",
            "--site-history",
            "a",
            "--site-history-journal-bytes",
            "-1",
        ],
        vec![
            "knot",
            "--site-history",
            "a",
            "--site-history-revisions",
            "2",
            "--site-history-revisions",
            "3",
        ],
    ] {
        assert!(select_document(args.into_iter().map(OsString::from)).is_err());
    }
    let configured = select_document(
        [
            "knot",
            "--site-history",
            "history",
            "--site-history-revisions",
            "2",
            "--site-history-journal-bytes",
            "4096",
        ]
        .map(OsString::from),
    )
    .unwrap();
    assert_eq!(configured.site_history_revisions, Some(2));
    assert_eq!(configured.site_history_journal_bytes, Some(4096));
}
use super::*;
#[test]
#[cfg(feature = "mere-retention")]
#[ignore = "run mere/scripts/dr_c_receipts.py against the isolated receipt keeper"]
fn dr_c_knot_desktop_identity_stays_pending() {
    let mode = std::env::var("DR_C_RECEIPT_MODE").expect("receipt mode");
    assert!(mode == "absent" || mode == "locked");
    let root = tempfile::tempdir().unwrap();
    let persona = uuid::Uuid::new_v4();
    let store =
        knot_editor::persona_vault_root(root.path(), personae::PersonaId::from_uuid(persona))
            .join("knot/sync.redb");
    std::fs::create_dir_all(store.parent().unwrap()).unwrap();
    std::fs::write(&store, b"opaque existing operations").unwrap();
    let document = root.path().join("essay.djot");
    std::fs::write(&document, b"An already-public document.").unwrap();
    let attachment = attach_mere(Some(MereOptions {
        root: root.path().to_path_buf(),
        persona,
    }));
    assert!(
        matches!(attachment, Err(ref error) if error.contains("pending")),
        "desktop must attach no retention authority or fallback key"
    );
    assert_eq!(std::fs::read(store).unwrap(), b"opaque existing operations");
    assert_eq!(
        std::fs::read(document).unwrap(),
        b"An already-public document."
    );
    assert!(
        !knot_editor::knot_settings_path(root.path(), personae::PersonaId::from_uuid(persona))
            .exists()
    );
}
#[test]
fn mere_attachment_requires_explicit_paired_options() {
    let id = "00000000-0000-0000-0000-000000000042";
    assert!(select_document(["knot", "--mere-root", "data"].map(OsString::from)).is_err());
    assert!(select_document(["knot", "--mere-persona", id].map(OsString::from)).is_err());
    assert!(
        select_document(
            ["knot", "--mere-root", "data", "--mere-persona", "invalid"].map(OsString::from)
        )
        .is_err()
    );
    assert!(
        select_document(
            [
                "knot",
                "--mere-root",
                "data",
                "--mere-root",
                "other",
                "--mere-persona",
                id
            ]
            .map(OsString::from)
        )
        .is_err()
    );
    let parsed = select_document(
        [
            "knot",
            "--mere-root",
            "data",
            "--mere-persona",
            id,
            "essay.djot",
        ]
        .map(OsString::from),
    )
    .unwrap();
    assert_eq!(parsed.mere.unwrap().root, PathBuf::from("data"));
    assert_eq!(parsed.mere_domain, None);
    assert!(
        select_document(["knot", "--mere-domain", "divination"].map(OsString::from)).is_err(),
        "the overmap layer needs the resident mere it belongs to"
    );
    let overlaid = select_document(
        [
            "knot",
            "--mere-root",
            "data",
            "--mere-persona",
            id,
            "--mere-domain",
            "divination",
        ]
        .map(OsString::from),
    )
    .unwrap();
    assert_eq!(overlaid.mere_domain.as_deref(), Some("divination"));
    assert_eq!(parsed.documents, [PathBuf::from("essay.djot")]);
    let attachment = attach_mere(None).unwrap();
    assert!(attachment.composition_targets.is_empty());
    assert!(attachment.relation_view.is_none());
}
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
            mere: None,
            mere_domain: None,
            site_history: None,
            site_history_revisions: None,
            site_history_journal_bytes: None,
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
fn launch_takes_several_site_folders_in_order() {
    let temp = tempdir().unwrap();
    let one = temp.path().join("one");
    let two = temp.path().join("two");
    site_host::Site::create_for(&one, site_host::SiteFormat::Scroll).unwrap();
    site_host::Site::create_for(&two, site_host::SiteFormat::Scroll).unwrap();
    let from_sites = open_documents(vec![one.clone(), two.clone()]).unwrap();
    assert_eq!(from_sites.first_path.as_deref(), Some(one.as_path()));
    assert_eq!(from_sites.sites.as_slice(), std::slice::from_ref(&two));
    assert_eq!(from_sites.behind.len(), 1);

    let loose = temp.path().join("loose.djot");
    std::fs::write(&loose, "loose\n").unwrap();
    let opened = open_documents(vec![loose, one.clone(), two.clone()]).unwrap();
    assert_eq!(opened.sites, [one.clone(), two]);
    assert_eq!(opened.behind.len(), 2);
    assert_eq!(opened.behind[0].snapshot().display_label, "index.scroll");
    let front = open_documents(vec![one.clone()]).unwrap();
    assert_eq!(front.first_path.as_deref(), Some(one.as_path()));
    assert!(
        front.sites.is_empty(),
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
        fonts: knot_desktop::fonts::bundled_fonts(),
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
