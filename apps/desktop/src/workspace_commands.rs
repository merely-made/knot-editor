// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's one command catalog. Menus and the palette project this tree; every
//! activation resolves the id against a fresh tree before it touches state.

use super::{DesktopState, PathCommand, PendingAction};
use crate::documents::ReadingKind;
use cambium::{CommandItem, TextCommand};
use cambium_genet_winit_host::{Key, KeyPress};
use knot_document::{KnotDocumentIntentV1, KnotDocumentWritePostureV1};

pub(super) const PALETTE: &str = "command.palette";
pub(super) const NEW: &str = "file.new";
pub(super) const OPEN: &str = "file.open";
pub(super) const SAVE: &str = "file.save";
pub(super) const SAVE_AS: &str = "file.save-as";
pub(super) const RELOAD: &str = "file.reload";
pub(super) const CLOSE: &str = "file.close";
pub(super) const UNDO: &str = "edit.undo";
pub(super) const REDO: &str = "edit.redo";
pub(super) const SELECT_ALL: &str = "edit.select-all";
pub(super) const NAVIGATOR: &str = "view.navigator";
pub(super) const GRAPH: &str = "view.graph";
pub(super) const OUTLINE: &str = "view.outline";
pub(super) const PREVIEW: &str = "view.preview";
pub(super) const FOLDS: &str = "view.folds";
pub(super) const READINGS: &str = "view.readings";
pub(super) const CHANGES: &str = "view.changes";
pub(super) const LINKS: &str = "view.links";
pub(super) const LINK_DOCUMENT: &str = "document.link";
pub(super) const APPEARANCE: &str = "view.appearance";
pub(super) const SUBMIT: &str = "document.submit";
pub(super) const COMPARE: &str = "document.compare";
pub(super) const SITE_OPEN: &str = "site.open";
const SITE_PAGE_PREFIX: &str = "site.page:";
const SITE_METADATA_PREFIX: &str = "site.metadata:";
pub(super) const SITE_PUBLISH: &str = "site.publish";
pub(super) const SITE_STOP: &str = "site.stop";
pub(super) const SITE_CLOSE: &str = "site.close";

#[derive(Clone, Copy)]
struct Shortcut {
    key: char,
    shift: bool,
}

impl Shortcut {
    const fn new(key: char, shift: bool) -> Self {
        Self { key, shift }
    }

    fn label(self) -> String {
        let key = self.key.to_ascii_uppercase();
        if cfg!(target_os = "macos") {
            format!("⌘{}{key}", if self.shift { "⇧" } else { "" })
        } else {
            format!("Ctrl+{}{key}", if self.shift { "Shift+" } else { "" })
        }
    }

    fn matches(self, press: &KeyPress) -> bool {
        press.modifiers.is_command_chord()
            && !press.modifiers.alt
            && press.modifiers.shift == self.shift
            && matches!(&press.key, Key::Character(key) if key.eq_ignore_ascii_case(&self.key.to_string()))
    }
}

fn shortcut(id: &str) -> Option<Shortcut> {
    Some(match id {
        PALETTE => Shortcut::new('p', true),
        NEW => Shortcut::new('n', false),
        OPEN => Shortcut::new('o', false),
        SAVE => Shortcut::new('s', false),
        SAVE_AS => Shortcut::new('s', true),
        _ => return None,
    })
}

fn item(id: &'static str, label: impl Into<String>, reason: Option<&'static str>) -> CommandItem {
    let mut item = CommandItem::new(label).with_id(id);
    if let Some(shortcut) = shortcut(id) {
        item = item.with_shortcut(shortcut.label());
    }
    if let Some(reason) = reason {
        item = item.disabled_because(reason);
    }
    item
}

pub(super) fn groups(state: &DesktopState) -> Vec<CommandItem> {
    let doc = state.focused_key();
    let snapshot = doc.map(|_| state.document().snapshot());
    let no_doc = doc.is_none().then_some("No document is open.");
    let read_only = snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.write_posture == KnotDocumentWritePostureV1::ReadOnly)
        .then_some("This document is read-only.");
    let writable = no_doc.or(read_only);
    let recovery_original =
        state.entry().recovery_candidate && state.entry().recovery_origin.is_some();
    let saved_target = writable.or_else(|| {
        snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.write_posture == KnotDocumentWritePostureV1::Scratch)
            .then_some("Save As first to choose a file.")
    });
    let reload_target = if recovery_original {
        writable
    } else {
        saved_target
    };
    let comparable = no_doc.or_else(|| {
        snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.write_posture == KnotDocumentWritePostureV1::Scratch && !recovery_original
            })
            .then_some("This document has no disk source to compare.")
    });
    let close_busy = doc.and_then(|key| {
        state.retention_busy.as_ref().and_then(|busy| {
            (state
                .docs
                .doc(key)
                .and_then(|entry| entry.catalog_id.as_ref())
                == Some(&busy.document_id))
            .then_some("Retention is in progress for this document.")
        })
    });
    let site = state.site_page().map(|page| page.site);
    let no_site = site.is_none().then_some("Focus a page in an open site.");
    let site_unsaved = site.and_then(|site| {
        (!state.site_dirty_pages(site).is_empty() || !state.site_dirty_drafts(site).is_empty())
            .then_some("Save source and metadata before publishing locally.")
    });
    let serving = site
        .and_then(|site| state.scroll.site(site))
        .is_some_and(|site| site.server.is_some());
    let pages: Vec<String> = site
        .and_then(|site| state.scroll.site(site))
        .map(|entry| {
            entry
                .site
                .config
                .pages
                .iter()
                .map(|page| page.path.clone())
                .collect()
        })
        .unwrap_or_default();
    let site_id = site.map(|site| site.0).unwrap_or_default();
    let page_commands = pages
        .iter()
        .map(|name| CommandItem::new(name).with_id(format!("{SITE_PAGE_PREFIX}{site_id}:{name}")));
    let metadata_commands = pages.iter().map(|name| {
        CommandItem::new(name).with_id(format!("{SITE_METADATA_PREFIX}{site_id}:{name}"))
    });
    let format = snapshot.as_ref().map(|snapshot| snapshot.format);
    vec![
        CommandItem::new("File")
            .with_id("group.file")
            .with_children([
                item(NEW, "New", None),
                item(OPEN, "Open path", None),
                item(SAVE, "Save", saved_target),
                item(SAVE_AS, "Save As", writable),
                item(RELOAD, "Reload", reload_target),
                item(CLOSE, "Close document", no_doc.or(close_busy)),
            ]),
        CommandItem::new("Edit")
            .with_id("group.edit")
            .with_children([
                item(UNDO, "Undo", writable),
                item(REDO, "Redo", writable),
                item(SELECT_ALL, "Select All", writable),
            ]),
        CommandItem::new("View")
            .with_id("group.view")
            .with_children([
                item(PALETTE, "Command palette", None),
                item(NAVIGATOR, "Navigator", None),
                item(GRAPH, "Graph", None),
                item(OUTLINE, "Outline", no_doc),
                item(
                    PREVIEW,
                    "Preview",
                    no_doc.or_else(|| {
                        (!format.is_some_and(crate::document_preview::supported))
                            .then_some("Preview is unavailable for this format.")
                    }),
                ),
                item(
                    FOLDS,
                    "Folds",
                    no_doc.or_else(|| {
                        (!format.is_some_and(crate::document_folding::supported))
                            .then_some("Folds are unavailable for this format.")
                    }),
                ),
                item(READINGS, "Readings", no_doc),
                item(CHANGES, "Changes", no_doc),
                item(LINKS, "Links", no_doc),
                item(APPEARANCE, "Appearance", None),
            ]),
        CommandItem::new("Document")
            .with_id("group.document")
            .with_children([
                item(COMPARE, "Compare with disk", comparable),
                item(
                    LINK_DOCUMENT,
                    "Link to document",
                    crate::link_workflow::unavailable(state),
                ),
                item(SUBMIT, "Upload / submit", no_doc),
            ]),
        CommandItem::new("Site")
            .with_id("group.site")
            .with_children([
                item(SITE_OPEN, "Open or create site", None),
                if let Some(reason) = no_site {
                    CommandItem::new("Pages")
                        .with_id("site.pages")
                        .with_children(page_commands)
                        .disabled_because(reason)
                } else {
                    CommandItem::new("Pages")
                        .with_id("site.pages")
                        .with_children(page_commands)
                },
                if let Some(reason) = no_site {
                    CommandItem::new("Metadata")
                        .with_id("site.metadata")
                        .with_children(metadata_commands)
                        .disabled_because(reason)
                } else {
                    CommandItem::new("Metadata")
                        .with_id("site.metadata")
                        .with_children(metadata_commands)
                },
                item(SITE_PUBLISH, "Publish locally", no_site.or(site_unsaved)),
                item(
                    SITE_STOP,
                    "Stop serving",
                    no_site.or_else(|| (!serving).then_some("This site is not serving.")),
                ),
                item(SITE_CLOSE, "Close site", no_site),
            ]),
    ]
}

pub(super) fn palette_items(groups: &[CommandItem]) -> Vec<CommandItem> {
    fn leaves(parent: &CommandItem, prefix: &str, output: &mut Vec<CommandItem>) {
        for child in &parent.children {
            if child.id == PALETTE {
                continue;
            }
            let label = format!("{prefix} › {}", child.label);
            if child.children.is_empty() {
                let mut item = child.clone();
                item.label = label;
                output.push(item);
            } else {
                leaves(child, &label, output);
            }
        }
    }
    let mut output = Vec::new();
    for group in groups {
        leaves(group, &group.label, &mut output);
    }
    output
}

pub(super) fn id_at_path(groups: &[CommandItem], path: &[usize]) -> Option<String> {
    let mut items = groups;
    let mut selected = None;
    for index in path {
        let item = items.get(*index)?;
        selected = Some(item.id.clone());
        items = &item.children;
    }
    selected
}

fn find<'a>(groups: &'a [CommandItem], id: &str) -> Option<&'a CommandItem> {
    for item in groups {
        if item.id == id {
            return Some(item);
        }
        if let Some(found) = find(&item.children, id) {
            return Some(found);
        }
    }
    None
}

pub(super) fn disabled_reason(state: &DesktopState, id: &str) -> Option<String> {
    find(&groups(state), id).and_then(|item| item.disabled_reason.clone())
}

pub(super) fn shortcut_id(press: &KeyPress) -> Option<&'static str> {
    [PALETTE, NEW, OPEN, SAVE, SAVE_AS]
        .into_iter()
        .find(|id| shortcut(id).is_some_and(|shortcut| shortcut.matches(press)))
}

impl DesktopState {
    /// A command's final authority check, shared by menu, palette, toolbar and
    /// keyboard. A stale visual row cannot perform an action after focus moves.
    pub(super) fn activate_command(&mut self, id: &str) {
        if id == PALETTE {
            self.open_command_palette();
            return;
        }
        let catalog = groups(self);
        let Some(command) = find(&catalog, id) else {
            self.message = Some(format!("Unknown command: {id}."));
            return;
        };
        if command.disabled {
            self.message = command
                .disabled_reason
                .clone()
                .or_else(|| Some("Command unavailable.".into()));
            return;
        }
        match id {
            NEW => self.new_document(),
            OPEN => self.show_path_popover(PathCommand::Open),
            SAVE => self.save(),
            SAVE_AS => self.show_path_popover(PathCommand::SaveAs),
            RELOAD => self.reload(),
            CLOSE => {
                if let Some(key) = self.focused_key() {
                    self.request(PendingAction::CloseDocument(key));
                }
            },
            UNDO => self.edit_command("Undo", TextCommand::Undo),
            REDO => self.edit_command("Redo", TextCommand::Redo),
            SELECT_ALL => self.edit_command("Select All", TextCommand::SelectAll),
            NAVIGATOR => self.toggle_navigator(),
            GRAPH => self.toggle_graph(),
            OUTLINE => self.toggle_reading(ReadingKind::Outline),
            PREVIEW => self.toggle_reading(ReadingKind::Preview),
            FOLDS => self.toggle_reading(ReadingKind::Folded),
            READINGS => self.toggle_reading(ReadingKind::Readings),
            CHANGES => self.toggle_reading(ReadingKind::Changes),
            LINKS => self.toggle_reading(ReadingKind::Links),
            LINK_DOCUMENT => crate::link_workflow::begin(self),
            APPEARANCE => self.toggle_appearance(),
            COMPARE => self.compare_disk(),
            SUBMIT => self.toggle_reading(ReadingKind::Submit),
            SITE_OPEN => self.scroll.popover.open = true,
            id if id.starts_with(SITE_PAGE_PREFIX) || id.starts_with(SITE_METADATA_PREFIX) => {
                let Some(site) = self.site_page().map(|page| page.site) else {
                    return;
                };
                if let Some(name) = id.strip_prefix(&format!("{SITE_PAGE_PREFIX}{}:", site.0)) {
                    self.open_site_page(site, name);
                } else if let Some(name) =
                    id.strip_prefix(&format!("{SITE_METADATA_PREFIX}{}:", site.0))
                {
                    self.open_metadata(site, name);
                }
            },
            SITE_PUBLISH | SITE_STOP | SITE_CLOSE => {
                let Some(site) = self.site_page().map(|page| page.site) else {
                    return;
                };
                match id {
                    SITE_PUBLISH => self.publish_site(site),
                    SITE_STOP => {
                        self.scroll.stop_serving(site);
                        self.message = Some("Stopped serving locally.".into());
                    },
                    SITE_CLOSE => self.request_close_site(site),
                    _ => unreachable!(),
                }
            },
            _ => self.message = Some(format!("Unknown command: {id}.")),
        }
    }

    fn edit_command(&mut self, label: &str, command: TextCommand) {
        match self
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(command))
        {
            Ok(_) => self.message = Some(format!("{label}.")),
            Err(error) => self.message = Some(super::intent_error_label(label, error)),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
