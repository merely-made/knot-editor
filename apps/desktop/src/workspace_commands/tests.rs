// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::workspace::DocumentEntry;
use cambium::CaretSelection;
use cambium_genet_winit_host::{Modifiers, WindowCommands};
use knot_document::{DocumentFormat, KnotDocumentSession};
use knot_site::{Site, SiteFormat};
use std::collections::BTreeSet;

fn state(session: KnotDocumentSession) -> DesktopState {
    DesktopState::new(session, WindowCommands::new())
}

fn command<'a>(groups: &'a [CommandItem], id: &str) -> &'a CommandItem {
    find(groups, id).unwrap_or_else(|| panic!("missing command {id}"))
}

fn assert_unique_ids(items: &[CommandItem], ids: &mut BTreeSet<String>) {
    for item in items {
        assert!(
            ids.insert(item.id.clone()),
            "duplicate command id {}",
            item.id
        );
        assert_unique_ids(&item.children, ids);
    }
}

fn chord(key: &str, shift: bool) -> KeyPress {
    KeyPress::character(key).with_modifiers(Modifiers {
        ctrl: !cfg!(target_os = "macos"),
        meta: cfg!(target_os = "macos"),
        shift,
        ..Modifiers::NONE
    })
}

#[test]
fn catalog_ids_are_unique_and_shortcut_labels_match_key_dispatch() {
    let state = state(KnotDocumentSession::scratch("scratch:commands", "text"));
    let groups = groups(&state);
    assert_unique_ids(&groups, &mut BTreeSet::new());

    for (id, label, key, shift) in [
        (
            PALETTE,
            if cfg!(target_os = "macos") {
                "⌘⇧P"
            } else {
                "Ctrl+Shift+P"
            },
            "p",
            true,
        ),
        (
            NEW,
            if cfg!(target_os = "macos") {
                "⌘N"
            } else {
                "Ctrl+N"
            },
            "n",
            false,
        ),
        (
            OPEN,
            if cfg!(target_os = "macos") {
                "⌘O"
            } else {
                "Ctrl+O"
            },
            "o",
            false,
        ),
        (
            SAVE,
            if cfg!(target_os = "macos") {
                "⌘S"
            } else {
                "Ctrl+S"
            },
            "s",
            false,
        ),
        (
            SAVE_AS,
            if cfg!(target_os = "macos") {
                "⌘⇧S"
            } else {
                "Ctrl+Shift+S"
            },
            "s",
            true,
        ),
    ] {
        assert_eq!(command(&groups, id).shortcut.as_deref(), Some(label));
        assert_eq!(shortcut_id(&chord(key, shift)), Some(id));
        assert_ne!(shortcut_id(&chord(key, !shift)), Some(id));
    }
}

#[test]
fn empty_and_read_only_catalogs_keep_safe_availability() {
    let mut empty = state(KnotDocumentSession::scratch("scratch:empty", ""));
    let key = empty.focused_key().expect("initial scratch document");
    empty.close_document(key);
    let empty_groups = groups(&empty);
    assert!(empty.focused_key().is_none());
    assert!(!command(&empty_groups, NEW).disabled);
    assert!(!command(&empty_groups, OPEN).disabled);
    assert!(command(&empty_groups, SAVE).disabled);
    assert!(command(&empty_groups, COMPARE).disabled);

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("read-only.djot");
    std::fs::write(&path, "# Read only\n").unwrap();
    let read_only = state(KnotDocumentSession::open_read_only(&path).unwrap());
    let read_only_groups = groups(&read_only);
    assert!(!command(&read_only_groups, COMPARE).disabled);
    assert!(command(&read_only_groups, SAVE).disabled);
    assert!(command(&read_only_groups, UNDO).disabled);
    assert!(command(&read_only_groups, REDO).disabled);
    assert!(command(&read_only_groups, SELECT_ALL).disabled);
}

fn recovery_state(original: Option<std::path::PathBuf>) -> DesktopState {
    let mut state = state(KnotDocumentSession::scratch("scratch:seed", ""));
    let seed = state.focused_key().unwrap();
    state.close_document(seed);
    let session = KnotDocumentSession::recovery_candidate(
        "recovery:test",
        "recovered text\n".to_owned(),
        DocumentFormat::Djot,
        CaretSelection::default(),
    )
    .unwrap();
    let mut entry = DocumentEntry::new(session);
    entry.recovery_candidate = true;
    entry.recovery_origin = original;
    state.open_entry(entry);
    state
}

#[test]
fn recovery_candidate_compare_and_reload_need_an_original() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("original.djot");
    std::fs::write(&original, "original\n").unwrap();

    let with_original = recovery_state(Some(original));
    let with_original_groups = groups(&with_original);
    assert!(!command(&with_original_groups, COMPARE).disabled);
    assert!(!command(&with_original_groups, RELOAD).disabled);

    let scratch = recovery_state(None);
    let scratch_groups = groups(&scratch);
    assert!(command(&scratch_groups, COMPARE).disabled);
    assert!(command(&scratch_groups, RELOAD).disabled);
}

#[test]
fn stale_site_page_and_metadata_ids_cannot_target_another_focused_site() {
    let temp = tempfile::tempdir().unwrap();
    let first_root = temp.path().join("first");
    let second_root = temp.path().join("second");
    Site::create_for(&first_root, SiteFormat::Scroll).unwrap();
    Site::create_for(&second_root, SiteFormat::Scroll).unwrap();

    let mut state = state(KnotDocumentSession::scratch("scratch:sites", ""));
    let first_index = state.attach_site(&first_root).unwrap();
    state.open_path(first_index);
    let first_doc = state.focused_key().unwrap();
    let first_site = state.site_page().unwrap().site;

    let second_index = state.attach_site(&second_root).unwrap();
    state.open_path(second_index);
    let second_site = state.site_page().unwrap().site;
    assert_ne!(first_site, second_site);

    let stale_ids = [
        format!("{SITE_PAGE_PREFIX}{}:about.scroll", second_site.0),
        format!("{SITE_METADATA_PREFIX}{}:about.scroll", second_site.0),
    ];
    for id in &stale_ids {
        assert!(
            find(&groups(&state), id).is_some(),
            "expected current id {id}"
        );
    }

    state.docs.focus(first_doc);
    state.after_focus_change();
    for id in &stale_ids {
        state.activate_command(id);
        assert_eq!(state.focused_key(), Some(first_doc));
        assert!(
            state
                .message
                .as_deref()
                .unwrap()
                .contains("Unknown command")
        );
    }
}
