// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Explicit links are source edits. The form holds the document and selection
//! it was opened for; neither a tab switch nor Save As can redirect its edit.

use crate::document_links;
use crate::documents::{DocIdentity, DocKey};
use crate::workspace::{DesktopState, DesktopView};
use cambium::{CaretSelection, Keyed, TextCommand, button, el, span};
use knot_document::{KnotDocumentIntentV1, KnotDocumentWritePostureV1};
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct Target {
    key: DocKey,
    identity: DocIdentity,
    path: PathBuf,
    label: String,
}

pub(crate) struct LinkForm {
    source: DocKey,
    identity: DocIdentity,
    path: PathBuf,
    address: String,
    text: String,
    selection: CaretSelection,
    targets: Vec<Target>,
    selected: Option<usize>,
    relationship: Option<&'static str>,
    error: Option<String>,
}

fn readable_file(path: &Path) -> bool {
    path.is_file() && std::fs::File::open(path).is_ok()
}

pub(crate) fn unavailable(state: &DesktopState) -> Option<&'static str> {
    let Some(key) = state.focused_key() else {
        return Some("No document is open.");
    };
    let session = state.surface_for(key).session();
    let snapshot = session.snapshot();
    if snapshot.write_posture == KnotDocumentWritePostureV1::ReadOnly {
        return Some("This document is read-only.");
    }
    if !document_links::supported(snapshot.format) {
        return Some("Link insertion is available for Djot or legacy Knot documents.");
    }
    let Some(path) = session.source_path() else {
        return Some("Save As first to give this document a local file path.");
    };
    if session.input().composition().is_some() {
        return Some("Finish the current text composition before creating a link.");
    }
    if !readable_file(path) {
        return Some("The source file is missing or unreadable.");
    }
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Some("The source file is read-only.");
    }
    None
}

pub(crate) fn begin(state: &mut DesktopState) {
    if let Some(reason) = unavailable(state) {
        state.message = Some(reason.into());
        return;
    }
    let source = state.focused_key().expect("link source checked");
    let session = state.surface_for(source).session();
    let snapshot = session.snapshot();
    let targets = state
        .docs
        .docs()
        .filter_map(|(key, entry)| {
            if key == source {
                return None;
            }
            let session = entry.document.session();
            let path = session.source_path()?;
            readable_file(path).then(|| Target {
                key,
                identity: state.docs.identity(key).expect("open identity").clone(),
                path: path.to_path_buf(),
                label: session.snapshot().display_label,
            })
        })
        .collect();
    state.link_form = Some(LinkForm {
        source,
        identity: state.docs.identity(source).expect("open identity").clone(),
        path: session.source_path().expect("saved source").to_path_buf(),
        address: snapshot.source.address,
        text: session.input().text().to_owned(),
        selection: snapshot.selection,
        targets,
        selected: None,
        relationship: None,
        error: None,
    });
    state.link_form_focus_requested = true;
}

pub(crate) fn dismiss(state: &mut DesktopState) {
    if let Some(form) = state.link_form.take()
        && state.docs.doc(form.source).is_some()
    {
        state.focus_document(form.source);
        state.entry_mut_for(form.source).focus_source_requested = true;
    }
    state.link_form_focus_requested = false;
}

fn insertion(state: &DesktopState, form: &LinkForm) -> Result<String, String> {
    let entry = state
        .docs
        .doc(form.source)
        .ok_or("The source document was closed. Open the form again.")?;
    let session = entry.document.session();
    let snapshot = session.snapshot();
    if snapshot.write_posture != KnotDocumentWritePostureV1::FileTarget
        || !document_links::supported(snapshot.format)
        || session.source_path() != Some(form.path.as_path())
        || state.docs.identity(form.source) != Some(&form.identity)
        || snapshot.source.address != form.address
        || session.input().text() != form.text
        || snapshot.selection != form.selection
        || session.input().composition().is_some()
    {
        return Err("The source changed, its selection moved, or its write authority moved. Open the form again.".into());
    }
    if !readable_file(&form.path)
        || std::fs::metadata(&form.path).is_ok_and(|metadata| metadata.permissions().readonly())
    {
        return Err("The source file is missing, unreadable or read-only.".into());
    }
    let target = form
        .selected
        .and_then(|index| form.targets.get(index))
        .ok_or("Choose an open saved document.")?;
    let current = state
        .docs
        .doc(target.key)
        .ok_or("The target document was closed. Open the form again.")?;
    if current.document.session().source_path() != Some(target.path.as_path())
        || state.docs.identity(target.key) != Some(&target.identity)
        || !readable_file(&target.path)
    {
        return Err("The target was moved, saved elsewhere, or is missing or unreadable. Open the form again.".into());
    }
    let start = form.selection.anchor.byte.min(form.selection.focus.byte);
    let end = form.selection.anchor.byte.max(form.selection.focus.byte);
    let selected = form
        .text
        .get(start..end)
        .ok_or("The captured selection is invalid.")?;
    document_links::markup(
        &form.path,
        &target.path,
        selected,
        &target.label,
        form.relationship,
    )
}

pub(crate) fn insert(state: &mut DesktopState) {
    let Some(form) = state.link_form.as_ref() else {
        return;
    };
    let result = insertion(state, form);
    let source = form.source;
    let result = result.and_then(|markup| {
        let document = state.surface_mut_for(source);
        document
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(markup)))
            .map_err(|error| format!("Link insertion refused: {error:?}"))?;
        Ok(())
    });
    match result {
        Ok(()) => {
            dismiss(state);
            state.message = Some("Link inserted. Save to persist this source edit.".into());
        },
        Err(error) => {
            state.message = Some(error.clone());
            if let Some(form) = state.link_form.as_mut() {
                form.error = Some(error);
            }
        },
    }
}

pub(crate) fn form_view(state: &DesktopState) -> DesktopView {
    let Some(form) = &state.link_form else {
        return Box::new(el("div", ()));
    };
    let choices = form
        .targets
        .iter()
        .enumerate()
        .map(|(index, target)| {
            (
                index,
                button(
                    format!("{} · {}", target.label, target.path.display()),
                    move |state: &mut DesktopState, _| {
                        if let Some(form) = state.link_form.as_mut() {
                            form.selected = Some(index);
                        }
                    },
                )
                .attr("aria-pressed", (form.selected == Some(index)).to_string())
                .attr("data-link-target", index.to_string()),
            )
        })
        .collect::<Vec<_>>();
    let relationships = std::iter::once(("", "Ordinary link"))
        .chain(
            document_links::RELATIONSHIPS
                .iter()
                .map(|relationship| (relationship.slug, relationship.label)),
        )
        .enumerate()
        .map(|(index, (slug, label))| {
            (
                index,
                button(label, move |state: &mut DesktopState, _| {
                    if let Some(form) = state.link_form.as_mut() {
                        form.relationship = (!slug.is_empty()).then_some(slug);
                    }
                })
                .attr(
                    "aria-pressed",
                    (form.relationship.unwrap_or_default() == slug).to_string(),
                ),
            )
        })
        .collect::<Vec<_>>();
    Box::new(el("aside", (
        span(format!("Link from {}", form.path.display())),
        span("Choose another open saved document. Read-only targets are allowed."),
        if choices.is_empty() { Box::new(span("No other saved readable document is open. Open a target, then reopen this form.")) as DesktopView }
        else { Box::new(el("div", Keyed::new(choices))) as DesktopView },
        el("div", Keyed::new(relationships)),
        form.error.as_ref().map(|error| span(error.clone()).attr("role", "status")),
        button("Insert link", |state: &mut DesktopState, _| insert(state))
            .attr("aria-disabled", form.selected.is_none().to_string()).attr("id", "knot-link-insert"),
        button("Cancel", |state: &mut DesktopState, _| dismiss(state)).attr("id", "knot-link-cancel"),
    )).attr("id", "knot-link-form").attr("class", "knot-confirm")
        .attr("role", "dialog").attr("aria-label", "Link to document"))
}

/// Follow a link belonging to an exact source snapshot. The host's ordinary
/// open path operation retains duplicate-open custody and all admission checks.
pub(crate) fn follow_link(
    state: &mut DesktopState,
    key: DocKey,
    address: &str,
    source_text: &str,
    target: &str,
) {
    let result = (|| {
        let entry = state
            .docs
            .doc(key)
            .ok_or("The link's source document was closed.")?;
        let session = entry.document.session();
        if session.snapshot().source.address != address || session.input().text() != source_text {
            return Err("This link reading is stale. Read the current source again.".to_owned());
        }
        let path = session
            .source_path()
            .ok_or("Save the source first to resolve a local link.")?;
        let resolved = document_links::resolve_local(path, target).ok_or(
            "Only relative local document links can be opened here; no network request was made.",
        )?;
        if !readable_file(&resolved) {
            return Err(format!(
                "Link target is missing or unreadable: {}",
                resolved.display()
            ));
        }
        Ok(resolved)
    })();
    match result {
        Ok(path) => {
            state.open_path(path);
        },
        Err(error) => state.message = Some(error),
    }
}

fn select_link(
    state: &mut DesktopState,
    key: DocKey,
    address: &str,
    text: &str,
    span: Option<(usize, usize)>,
) {
    let result = state
        .docs
        .doc_mut(key)
        .ok_or("The link's source document was closed.".into())
        .and_then(|entry| {
            let (start, end) = span.ok_or("This link has no exact source range.".to_owned())?;
            entry
                .document
                .session_mut()
                .select_source_span(address, text, start, end)
        });
    match result {
        Ok(()) => {
            state.focus_document(key);
            state.entry_mut_for(key).focus_source_requested = true;
        },
        Err(error) => state.message = Some(error),
    }
}

pub(crate) fn reading_view(state: &DesktopState, key: DocKey) -> DesktopView {
    let Some(entry) = state.docs.doc(key) else {
        return Box::new(el("div", ()));
    };
    let session = entry.document.session();
    let mut rows = Vec::<(usize, DesktopView)>::new();
    let address = session.snapshot().source.address;
    let text = session.input().text().to_owned();
    match document_links::parsed_links(session) {
        Err(error) => rows.push((0, Box::new(span(format!("Links unavailable: {error}"))))),
        Ok(links) => {
            if links.is_empty() {
                rows.push((0, Box::new(span("No outgoing links."))));
            }
            for link in links {
                let rel = link.rel.as_deref().map_or("ordinary link", |iri| {
                    document_links::RELATIONSHIPS
                        .iter()
                        .find(|relationship| relationship.iri == iri)
                        .map_or(iri, |relationship| relationship.label)
                });
                let display_target =
                    percent_encoding::percent_decode_str(&link.target).decode_utf8_lossy();
                let status = session
                    .source_path()
                    .and_then(|path| document_links::resolve_local(path, &link.target))
                    .map(|path| {
                        if readable_file(&path) {
                            "local document"
                        } else {
                            "missing or unreadable target"
                        }
                    })
                    .unwrap_or("not a relative local document");
                let (select_address, select_text) = (address.clone(), text.clone());
                let (follow_address, follow_text) = (address.clone(), text.clone());
                let target = link.target.clone();
                rows.push((
                    rows.len(),
                    Box::new(
                        el(
                            "div",
                            (
                                el(
                                    "div",
                                    (
                                        span(link.text.clone()).attr("class", "knot-link-label"),
                                        span(format!("→ {display_target}"))
                                            .attr("class", "knot-link-target"),
                                        span(format!("{rel} · {status}"))
                                            .attr("class", "knot-link-kind"),
                                    ),
                                )
                                .attr("class", "knot-link-description")
                                .attr(
                                    "aria-label",
                                    format!(
                                        "{} → {} · {} · {status}",
                                        link.text,
                                        link.target,
                                        link.rel.as_deref().unwrap_or("ordinary link")
                                    ),
                                ),
                                el(
                                    "div",
                                    (
                                        button(
                                            "Select source",
                                            move |state: &mut DesktopState, _| {
                                                select_link(
                                                    state,
                                                    key,
                                                    &select_address,
                                                    &select_text,
                                                    link.span,
                                                )
                                            },
                                        )
                                        .attr("aria-disabled", link.span.is_none().to_string()),
                                        button(
                                            "Open target",
                                            move |state: &mut DesktopState, _| {
                                                follow_link(
                                                    state,
                                                    key,
                                                    &follow_address,
                                                    &follow_text,
                                                    &target,
                                                )
                                            },
                                        ),
                                    ),
                                )
                                .attr("class", "knot-link-actions"),
                            ),
                        )
                        .attr("class", "knot-link-row"),
                    ),
                ));
            }
        },
    }
    let mut backlinks = Vec::<(usize, DesktopView)>::new();
    let mut excluded = 0usize;
    if let Some(current_path) = session.source_path() {
        for (other_key, other) in state.docs.docs() {
            let other_session = other.document.session();
            let Some(other_path) = other_session.source_path() else {
                excluded += 1;
                continue;
            };
            let Ok(links) = document_links::parsed_links(other_session) else {
                excluded += 1;
                continue;
            };
            for link in links {
                let matches = document_links::resolve_local(other_path, &link.target)
                    .and_then(|path| std::fs::canonicalize(path).ok())
                    .as_deref()
                    == Some(current_path);
                if !matches {
                    continue;
                }
                let address = other_session.snapshot().source.address;
                let text = other_session.input().text().to_owned();
                backlinks.push((
                    backlinks.len(),
                    Box::new(button(
                        format!(
                            "{}{} · {}",
                            other_session.snapshot().display_label,
                            if other_session.snapshot().dirty {
                                " · unsaved buffer"
                            } else {
                                ""
                            },
                            link.text
                        ),
                        move |state: &mut DesktopState, _| {
                            select_link(state, other_key, &address, &text, link.span)
                        },
                    )),
                ));
            }
        }
    }
    if backlinks.is_empty() {
        backlinks.push((
            0,
            Box::new(span("No backlinks in currently open documents.")),
        ));
    }
    Box::new(el("div", (
        span(if session.snapshot().dirty { "Outgoing links · derived from the current unsaved buffer" } else { "Outgoing links · derived from the current source" }),
        el("div", Keyed::new(rows)),
        span("Backlinks · currently open saved documents only; this is not a workspace-wide index."),
        span(format!("{excluded} open documents excluded because they have no saved path or their links could not be read.")),
        el("div", Keyed::new(backlinks)),
    )).attr("class", "knot-readings"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::WindowCommands;
    use knot_document::KnotDocumentSession;

    fn fixture(source: &str) -> (tempfile::TempDir, DesktopState, DocKey, DocKey) {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("source.djot");
        let b = dir.path().join("target.djot");
        std::fs::write(&a, source).unwrap();
        std::fs::write(&b, "# Target\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&a).unwrap(),
            WindowCommands::new(),
        );
        let source = state.focused_key().unwrap();
        state.open_path(b);
        let target = state.focused_key().unwrap();
        state.focus_document(source);
        (dir, state, source, target)
    }

    fn select(state: &mut DesktopState, key: DocKey, start: usize, end: usize) {
        let session = state.surface_for(key).session();
        let address = session.snapshot().source.address;
        let text = session.input().text().to_owned();
        state
            .surface_mut_for(key)
            .session_mut()
            .select_source_span(&address, &text, start, end)
            .unwrap();
    }

    fn arm(state: &mut DesktopState) {
        begin(state);
        state.link_form.as_mut().unwrap().selected = Some(0);
    }

    #[test]
    fn unicode_selection_link_is_one_undo_step_and_only_save_persists() {
        let (_dir, mut state, source, target) = fixture("α 日本語 tail\n");
        let path = state
            .surface_for(source)
            .session()
            .source_path()
            .unwrap()
            .to_owned();
        select(&mut state, source, "α ".len(), "α 日本語".len());
        arm(&mut state);
        state.focus_document(target);
        insert(&mut state);
        assert!(state.link_form.is_none());
        assert_eq!(state.focused_key(), Some(source));
        let inserted = state
            .surface_for(source)
            .session()
            .input()
            .text()
            .to_owned();
        assert_eq!(inserted, "α [日本語](./target.djot) tail\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "α 日本語 tail\n");
        state
            .surface_mut_for(source)
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(
            state.surface_for(source).session().input().text(),
            "α 日本語 tail\n"
        );
        state
            .surface_mut_for(source)
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Redo))
            .unwrap();
        state
            .surface_mut_for(source)
            .apply(KnotDocumentIntentV1::Save)
            .unwrap();
        let reopened = KnotDocumentSession::open(path).unwrap();
        assert_eq!(reopened.input().text(), inserted);
        assert_eq!(
            document_links::parsed_links(&reopened).unwrap()[0].target,
            "./target.djot"
        );
    }

    #[test]
    fn dirty_source_allowed_but_changed_source_refuses_stale_form() {
        let (_dir, mut state, source, _) = fixture("draft");
        state
            .surface_mut_for(source)
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "dirty ".into(),
            )))
            .unwrap();
        assert_eq!(unavailable(&state), None);
        arm(&mut state);
        state
            .surface_mut_for(source)
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "later".into(),
            )))
            .unwrap();
        let before = state
            .surface_for(source)
            .session()
            .input()
            .text()
            .to_owned();
        insert(&mut state);
        assert_eq!(state.surface_for(source).session().input().text(), before);
        assert!(
            state
                .link_form
                .as_ref()
                .unwrap()
                .error
                .as_deref()
                .unwrap()
                .contains("source changed")
        );
    }

    #[test]
    fn moved_selection_or_pending_composition_refuses_stale_form() {
        for composition in [false, true] {
            let (_dir, mut state, source, _) = fixture("source");
            select(&mut state, source, 0, 2);
            arm(&mut state);
            if composition {
                state
                    .surface_mut_for(source)
                    .apply(KnotDocumentIntentV1::Edit(TextCommand::SetComposition {
                        text: "未".into(),
                        selection: Some((0, 3)),
                    }))
                    .unwrap();
            } else {
                select(&mut state, source, 2, 4);
            }
            insert(&mut state);
            assert_eq!(state.surface_for(source).session().input().text(), "source");
            assert!(state.link_form.as_ref().unwrap().error.is_some());
        }
    }

    #[test]
    fn target_close_save_as_and_missing_file_refuse_without_edit() {
        for operation in 0..3 {
            let (dir, mut state, source, target) = fixture("source");
            arm(&mut state);
            match operation {
                0 => {
                    let tile = state.docs.tile_of(target).unwrap();
                    state.docs.close(tile);
                },
                1 => {
                    state
                        .surface_mut_for(target)
                        .apply(KnotDocumentIntentV1::SaveAs(dir.path().join("moved.djot")))
                        .unwrap();
                },
                _ => {
                    std::fs::remove_file(dir.path().join("target.djot")).unwrap();
                },
            }
            insert(&mut state);
            assert_eq!(state.surface_for(source).session().input().text(), "source");
            assert!(state.link_form.as_ref().unwrap().error.is_some());
        }
    }

    #[test]
    fn source_save_as_or_closed_source_refuses_without_redirecting() {
        for close in [false, true] {
            let (dir, mut state, source, target) = fixture("source");
            arm(&mut state);
            if close {
                let tile = state.docs.tile_of(source).unwrap();
                state.docs.close(tile);
            } else {
                state
                    .surface_mut_for(source)
                    .apply(KnotDocumentIntentV1::SaveAs(
                        dir.path().join("elsewhere.djot"),
                    ))
                    .unwrap();
            }
            state.focus_document(target);
            insert(&mut state);
            assert_eq!(
                state.surface_for(target).session().input().text(),
                "# Target\n"
            );
            assert!(state.link_form.as_ref().unwrap().error.is_some());
        }
    }

    #[test]
    fn unsaved_and_read_only_sources_refuse_read_only_target_is_allowed() {
        let mut scratch = DesktopState::new(
            KnotDocumentSession::scratch("scratch:test", "draft"),
            WindowCommands::new(),
        );
        begin(&mut scratch);
        assert!(scratch.link_form.is_none());
        assert!(unavailable(&scratch).unwrap().contains("Save As"));
        let (dir, mut state, source, target) = fixture("source");
        state.docs.doc_mut(target).unwrap().document = knot_document::KnotDocumentSurfaceState::new(
            KnotDocumentSession::open_read_only(dir.path().join("target.djot")).unwrap(),
        );
        arm(&mut state);
        insert(&mut state);
        assert!(
            state
                .surface_for(source)
                .session()
                .input()
                .text()
                .contains("[target\\.djot](./target.djot)")
        );
        let mut readonly = DesktopState::new(
            KnotDocumentSession::open_read_only(dir.path().join("source.djot")).unwrap(),
            WindowCommands::new(),
        );
        begin(&mut readonly);
        assert!(readonly.link_form.is_none());
        assert!(unavailable(&readonly).unwrap().contains("read-only"));
    }

    #[test]
    fn current_file_permissions_are_checked_again_before_insert() {
        let (dir, mut state, source, _) = fixture("source");
        arm(&mut state);
        let path = dir.path().join("source.djot");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(path, permissions).unwrap();
        insert(&mut state);
        assert_eq!(state.surface_for(source).session().input().text(), "source");
        assert!(
            state
                .link_form
                .as_ref()
                .unwrap()
                .error
                .as_deref()
                .unwrap()
                .contains("read-only")
        );
    }

    #[test]
    fn follows_exact_owner_source_and_reuses_open_target_key() {
        let (_dir, mut state, source, target) = fixture("[Target](target.djot)\n");
        let session = state.surface_for(source).session();
        let address = session.snapshot().source.address;
        let text = session.input().text().to_owned();
        follow_link(&mut state, source, &address, &text, "target.djot");
        assert_eq!(state.focused_key(), Some(target));
        assert_eq!(state.docs.len(), 2);
        follow_link(&mut state, source, &address, "stale", "target.djot");
        assert!(state.message.as_deref().unwrap().contains("stale"));
        follow_link(&mut state, source, &address, &text, "https://example.org/");
        assert!(state.message.as_deref().unwrap().contains("no network"));
    }

    #[test]
    fn named_relationship_round_trips_and_link_span_is_source_bound() {
        let (_dir, mut state, source, _) = fixture("日本語");
        select(&mut state, source, 0, "日本語".len());
        arm(&mut state);
        let relationship = &document_links::RELATIONSHIPS[0];
        state.link_form.as_mut().unwrap().relationship = Some(relationship.slug);
        insert(&mut state);
        let session = state.surface_for(source).session();
        let links = document_links::parsed_links(session).unwrap();
        assert_eq!(links[0].rel.as_deref(), Some(relationship.iri));
        let address = session.snapshot().source.address;
        let text = session.input().text().to_owned();
        select_link(&mut state, source, &address, &text, links[0].span);
        let span = links[0].span.unwrap();
        assert_eq!(
            state.surface_for(source).snapshot().selection.focus.byte,
            span.1
        );
        state
            .surface_mut_for(source)
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "changed".into(),
            )))
            .unwrap();
        let before = state.surface_for(source).snapshot().selection;
        select_link(&mut state, source, &address, &text, Some(span));
        assert_eq!(state.surface_for(source).snapshot().selection, before);
        assert!(state.message.as_deref().unwrap().contains("stale"));
    }
}
