// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The desktop's read-only source folding reading.
//!
//! Folding is a projection over the committed source in a
//! [`KnotFoldSnapshotV1`]. It deliberately does not use a second text input:
//! the ordinary source textarea remains the only editable buffer and the user
//! must explicitly return to it before editing.

use cambium::{Keyed, button, el, fold_projection, span};
use knot_document::{DocumentFormat, KnotDocumentSession, KnotFoldKindV1, KnotFoldSnapshotV1};
use std::collections::BTreeSet;

use crate::workspace::{DesktopState, DesktopView};

pub const CSS: &str = concat!(
    ".knot-folding { min-width:0; width:100%; box-sizing:border-box; ",
    "display:flex; flex-direction:column; gap:8px; padding:16px; overflow:auto; }",
    ".knot-folding-header { display:flex; flex-wrap:wrap; align-items:baseline; gap:8px; }",
    ".knot-folding-actions { display:flex; flex-wrap:wrap; gap:6px; }",
    ".knot-folding-controls { display:flex; flex-direction:column; gap:4px; }",
    ".knot-folding-row { display:flex; align-items:center; justify-content:space-between; gap:8px; }",
    ".knot-folding-row span { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }",
    ".knot-folded-source { margin:0; min-height:240px; white-space:pre-wrap; overflow:auto; ",
    "overflow-wrap:anywhere; user-select:text; }",
    ".knot-folding-error { color:crimson; }",
    ".knot-folded-source .fold-marker { color:inherit; opacity:.7; font-weight:600; }",
    "@media (max-width:700px) { .knot-folding { width:100%; flex-basis:auto; } }",
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NormalizedFold {
    pub source_index: usize,
    pub kind: KnotFoldKindV1,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn supported(format: DocumentFormat) -> bool {
    matches!(format, DocumentFormat::Djot | DocumentFormat::Knot)
}

pub(crate) fn snapshot_matches(
    session: &KnotDocumentSession,
    snapshot: &KnotFoldSnapshotV1,
) -> bool {
    &session.fold_snapshot() == snapshot
}

/// Validate fold ranges against the source and drop crossing ranges. Nested
/// ranges remain available as rows, while projection rendering gives an outer
/// collapsed range precedence over its contained ranges.
pub(crate) fn normalized_folds(snapshot: &KnotFoldSnapshotV1) -> Vec<NormalizedFold> {
    let source = snapshot.source_text.as_str();
    let mut candidates = snapshot
        .items
        .iter()
        .enumerate()
        .filter_map(|(source_index, item)| {
            (item.start < item.end
                && item.end <= source.len()
                && source.is_char_boundary(item.start)
                && source.is_char_boundary(item.end)
                && source[item.start..item.end].contains('\n'))
            .then_some(NormalizedFold {
                source_index,
                kind: item.kind,
                start: item.start,
                end: item.end,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|fold| (fold.start, std::cmp::Reverse(fold.end)));

    let mut accepted = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let duplicate = accepted.iter().any(|fold: &NormalizedFold| {
            fold.start == candidate.start && fold.end == candidate.end
        });
        let crossing = accepted
            .iter()
            .any(|fold: &NormalizedFold| candidate.start < fold.end && candidate.end > fold.end);
        if !duplicate && !crossing {
            accepted.push(candidate);
        }
    }
    accepted
}

pub(crate) fn fold_label(snapshot: &KnotFoldSnapshotV1, fold: NormalizedFold) -> String {
    let line = snapshot.source_text[..fold.start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let opener = snapshot.source_text[fold.start..fold.end]
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    let mut chars = opener.chars();
    let mut preview = chars.by_ref().take(48).collect::<String>();
    if chars.next().is_some() {
        preview.push('…');
    }
    let kind = match fold.kind {
        KnotFoldKindV1::Section => "Section",
        KnotFoldKindV1::List => "List",
        KnotFoldKindV1::Blockquote => "Blockquote",
        KnotFoldKindV1::CodeBlock => "Code block",
        KnotFoldKindV1::Div => "Div",
    };
    if preview.is_empty() {
        format!("{kind} · line {line}")
    } else {
        format!("{kind} · line {line} · {preview}")
    }
}

/// Build conceal ranges for a read-only folded reading. The first source line
/// of each collapsed container remains visible by beginning concealment after
/// its opening newline.
pub(crate) fn conceal_ranges(
    snapshot: &KnotFoldSnapshotV1,
    collapsed: &BTreeSet<usize>,
) -> Vec<std::ops::Range<usize>> {
    let mut outermost = Vec::new();
    for fold in normalized_folds(snapshot)
        .into_iter()
        .filter(|fold| collapsed.contains(&fold.source_index))
    {
        if outermost
            .iter()
            .any(|outer: &NormalizedFold| fold.start >= outer.start && fold.end <= outer.end)
        {
            continue;
        }
        outermost.push(fold);
    }
    outermost
        .iter()
        .filter_map(|fold| {
            let conceal_start = snapshot.source_text[fold.start..fold.end]
                .find('\n')
                .map(|offset| fold.start + offset + 1)?;
            (conceal_start < fold.end).then_some(conceal_start..fold.end)
        })
        .collect()
}

pub(crate) fn collapse_all_indices(snapshot: &KnotFoldSnapshotV1) -> BTreeSet<usize> {
    normalized_folds(snapshot)
        .into_iter()
        .map(|fold| fold.source_index)
        .collect()
}

pub(crate) fn view(state: &DesktopState) -> DesktopView {
    let snapshot = state
        .fold_snapshot
        .as_ref()
        .filter(|snapshot| snapshot_matches(state.document.session(), snapshot));
    let Some(snapshot) = snapshot else {
        return Box::new(
            el(
                "section",
                (
                    el("h2", "Folded source"),
                    span("Folds are updating; try again shortly."),
                ),
            )
            .attr("class", "knot-folding"),
        );
    };
    let folds = normalized_folds(snapshot);
    let conceal_ranges = conceal_ranges(snapshot, &state.collapsed_folds);
    let styles = if state.appearance.highlight {
        cambium::note_styles(snapshot.source_text.as_str())
    } else {
        Vec::new()
    };
    let source_children = match fold_projection(
        snapshot.source_text.as_str(),
        snapshot.source_text.len(),
        &conceal_ranges,
        &styles,
    ) {
        Ok(projection) => projection.field_children::<DesktopState, ()>(),
        Err(error) => {
            return Box::new(
                el(
                    "section",
                    (
                        el("h2", "Folded source"),
                        el("span", format!("Folded source unavailable: {error:?}")),
                    ),
                )
                .attr("class", "knot-folding"),
            );
        },
    };
    let controls = folds
        .iter()
        .map(|fold| {
            let source_index = fold.source_index;
            let collapsed = state.collapsed_folds.contains(&source_index);
            let label = fold_label(snapshot, *fold);
            let action = if collapsed { "Expand" } else { "Collapse" };
            let fold_snapshot = snapshot.clone();
            (
                source_index,
                el(
                    "div",
                    (
                        span(label.clone()),
                        button(action, move |state: &mut DesktopState, _| {
                            state.toggle_fold(fold_snapshot.clone(), source_index);
                        })
                        .attr("aria-label", format!("{action} {label}"))
                        .attr("data-fold-index", source_index.to_string()),
                    ),
                )
                .attr("class", "knot-folding-row"),
            )
        })
        .collect::<Vec<_>>();
    let controls_view: DesktopView = if controls.is_empty() {
        Box::new(span("No foldable containers in this source."))
    } else {
        Box::new(el("div", Keyed::new(controls)).attr("class", "knot-folding-controls"))
    };
    let error = state
        .fold_error
        .as_ref()
        .map(|error| span(format!("Fold error: {error}")).attr("class", "knot-folding-error"));
    Box::new(
        el(
            "section",
            (
                el(
                    "header",
                    (
                        el("h2", "Folded source"),
                        span(format!("Source: {}", snapshot.address)),
                    ),
                )
                .attr("class", "knot-folding-header"),
                el(
                    "div",
                    (
                        button("Edit source", |state: &mut DesktopState, _| {
                            state.edit_source();
                        }),
                        button("Collapse all", {
                            let fold_snapshot = snapshot.clone();
                            move |state: &mut DesktopState, _| {
                                state.collapse_all_folds(fold_snapshot.clone());
                            }
                        })
                        .attr("aria-label", "Collapse all folds"),
                        button("Expand all", {
                            let fold_snapshot = snapshot.clone();
                            move |state: &mut DesktopState, _| {
                                state.expand_all_folds(fold_snapshot.clone());
                            }
                        })
                        .attr("aria-label", "Expand all folds"),
                    ),
                )
                .attr("class", "knot-folding-actions"),
                error,
                controls_view,
                el("pre", source_children)
                    .attr("class", "knot-folded-source")
                    .attr("role", "document")
                    .attr("aria-label", "Read-only folded source")
                    .attr("aria-readonly", "true"),
            ),
        )
        .attr("class", "knot-folding")
        .attr("id", "knot-document-folding")
        .attr("role", "region")
        .attr("aria-label", "Folded source"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_document::KnotFoldItemV1;

    fn snapshot(source: &str, items: Vec<KnotFoldItemV1>) -> KnotFoldSnapshotV1 {
        KnotFoldSnapshotV1 {
            address: "memory:test".to_owned(),
            source_text: source.to_owned(),
            items,
        }
    }

    fn item(kind: KnotFoldKindV1, start: usize, end: usize) -> KnotFoldItemV1 {
        KnotFoldItemV1 { kind, start, end }
    }

    #[test]
    fn conceal_ranges_keep_unicode_opening_lines_and_nested_outer_precedence() {
        let source = "# α\n\n## 二\nbody\n\nend\n";
        let folds = snapshot(
            source,
            vec![
                item(KnotFoldKindV1::Section, 0, source.len()),
                item(KnotFoldKindV1::Section, 5, 16),
            ],
        );
        let mut collapsed = BTreeSet::new();
        collapsed.insert(0);
        collapsed.insert(1);
        assert_eq!(conceal_ranges(&folds, &collapsed), vec![5..source.len()]);
        assert_eq!(&source[..5], "# α\n");
        assert_eq!(
            fold_label(&folds, normalized_folds(&folds)[0]),
            "Section · line 1 · # α"
        );
    }

    #[test]
    fn crossing_ranges_are_dropped_before_projection() {
        let source = "first\nsecond\nthird\n";
        let folds = snapshot(
            source,
            vec![
                item(KnotFoldKindV1::Section, 0, 13),
                item(KnotFoldKindV1::List, 6, source.len()),
            ],
        );
        assert_eq!(normalized_folds(&folds).len(), 1);
        assert_eq!(normalized_folds(&folds)[0].source_index, 0);
    }
}
