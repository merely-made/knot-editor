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
pub(super) const FORME_UNLOCK: &str = "workspace.unlock";
pub(super) const FORME_APPLY: &str = "workspace.apply";
pub(super) const FORME_DISCARD: &str = "workspace.discard";
pub(super) const FORME_MORE: &str = "workspace.first-more";
pub(super) const FORME_LESS: &str = "workspace.first-less";
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
pub(super) const SITE_CHECKPOINT: &str = "site.checkpoint";
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
                item(
                    FORME_UNLOCK,
                    "Unlock workspace layout",
                    state
                        .docs
                        .forme_editing()
                        .then_some("A workspace draft is already open."),
                ),
                item(
                    FORME_APPLY,
                    "Apply workspace layout",
                    (!state.docs.forme_editing())
                        .then_some("Unlock the workspace layout first.")
                        .or_else(|| {
                            state
                                .preferences
                                .is_none()
                                .then_some("No local preferences store is configured.")
                        }),
                ),
                item(
                    FORME_DISCARD,
                    "Discard workspace layout",
                    (!state.docs.forme_editing()).then_some("No workspace draft is open."),
                ),
                item(
                    FORME_MORE,
                    "Give first pane more space",
                    (!state.docs.forme_resizable())
                        .then_some("Unlock a workspace with at least two panes first."),
                ),
                item(
                    FORME_LESS,
                    "Give first pane less space",
                    (!state.docs.forme_resizable())
                        .then_some("Unlock a workspace with at least two panes first."),
                ),
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
                    SITE_CHECKPOINT,
                    "Checkpoint saved site",
                    no_site.or_else(|| site.and_then(|site| state.site_checkpoint_disabled(site))),
                ),
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
            FORME_UNLOCK => {
                let saved = self
                    .preferences
                    .as_ref()
                    .and_then(crate::preferences::PreferencesStore::workspace_forme);
                self.message = Some(match self.docs.unlock_forme(saved) {
                    Ok(()) => "Workspace layout unlocked. Give the first pane more or less space, then Apply or Discard workspace layout.".into(),
                    Err(error) => error,
                });
            },
            FORME_MORE | FORME_LESS => {
                self.message = Some(match self.docs.resize_forme_first(id == FORME_MORE) {
                    Ok(()) => {
                        "Workspace layout preview updated. Apply or Discard the draft.".into()
                    },
                    Err(error) => error,
                });
            },
            FORME_DISCARD => {
                self.message = Some(match self.docs.discard_forme() {
                    Ok(()) => "Workspace layout draft discarded.".into(),
                    Err(error) => error,
                });
            },
            FORME_APPLY => {
                let store = &mut self.preferences;
                self.message = Some(
                    match self.docs.apply_forme(|baseline, value| {
                        store
                            .as_mut()
                            .ok_or("No local preferences store is configured")?
                            .save_workspace_forme(baseline, value)
                    }) {
                        Ok(()) => "Workspace layout applied and saved locally.".into(),
                        Err(error) => error,
                    },
                );
            },
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
            SITE_PUBLISH | SITE_CHECKPOINT | SITE_STOP | SITE_CLOSE => {
                let Some(site) = self.site_page().map(|page| page.site) else {
                    return;
                };
                match id {
                    SITE_PUBLISH => self.publish_site(site),
                    SITE_CHECKPOINT => self.checkpoint_site(site),
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
#[path = "workspace_commands/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "workspace_commands/forme_command_tests.rs"]
mod forme_command_tests;
