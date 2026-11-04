// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Explicit links are source edits. The form holds the document and selection
//! it was opened for; neither a tab switch nor Save As can redirect its edit.

use crate::document_links;
use crate::documents::{DocIdentity, DocKey};
use crate::workspace::{DesktopState, DesktopView};
#[cfg(test)]
use cambium::TextCommand;
use cambium::{CaretSelection, Keyed, button, el, span};
#[cfg(test)]
use knot_document::KnotDocumentIntentV1;
use knot_document::KnotDocumentWritePostureV1;
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
    base: knot_document::SourceSnapshot,
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
        base: session.source_snapshot(),
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

fn insertion(state: &DesktopState, form: &LinkForm) -> Result<knot_document::Proposal, String> {
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
    let selected = session
        .selected_source(&form.base, form.selection)
        .map_err(|error| {
            format!("The source changed or its selection moved: {error}. Open the form again.")
        })?;
    let markup = document_links::markup(
        &form.path,
        &target.path,
        selected,
        &target.label,
        form.relationship,
    )?;
    session.propose_selection_edit(&form.base, form.selection, markup)
}

pub(crate) fn insert(state: &mut DesktopState) {
    let Some(form) = state.link_form.as_ref() else {
        return;
    };
    let result = insertion(state, form);
    let source = form.source;
    let selection = form.selection;
    let result = result.and_then(|proposal| {
        let document = state.surface_mut_for(source);
        document
            .session_mut()
            .apply_source_proposal(&proposal, selection)
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
mod tests;
