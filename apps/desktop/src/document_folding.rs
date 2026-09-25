// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The desktop's read-only folded source reading, as a tile.
//!
//! Folding is a projection over the committed source in a
//! [`KnotFoldSnapshotV1`]. It deliberately does not use a second text input:
//! the ordinary source textarea remains the only editable buffer. Each
//! visible source line is a row, and a fold's control sits in the gutter
//! beside the line it starts on.

use cambium::{
    FieldChild, FoldProjectionLine, FoldProjectionSegment, button, el, fold_projection, span,
};
use knot_document::{DocumentFormat, KnotDocumentSession, KnotFoldKindV1, KnotFoldSnapshotV1};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::documents::DocKey;
use crate::workspace::{DesktopState, DesktopView};

pub const CSS: &str = concat!(
    ".knot-folding { min-width:0; box-sizing:border-box; display:flex; flex-direction:column; gap:8px; }",
    ".knot-folding-actions { display:flex; flex-wrap:wrap; gap:6px; }",
    ".knot-folding-error { color:crimson; }",
    ".knot-folded-source { box-sizing:border-box; padding:8px 0; border:1px solid; ",
    "font-family:monospace; user-select:text; }",
    ".knot-folded-source .fold-gutter { width:1.75em; text-align:center; }",
    ".knot-folded-source .fold-marker { color:inherit; opacity:.7; font-weight:600; }",
    ".knot-folded-source .knot-fold-toggle { padding:0; border:none; background:transparent; ",
    "color:inherit; font:inherit; line-height:inherit; cursor:pointer; }",
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
/// ranges remain available, while projection rendering gives an outer
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

fn kind_name(kind: KnotFoldKindV1) -> &'static str {
    match kind {
        KnotFoldKindV1::Section => "Section",
        KnotFoldKindV1::List => "List",
        KnotFoldKindV1::Blockquote => "Blockquote",
        KnotFoldKindV1::CodeBlock => "Code block",
        KnotFoldKindV1::Div => "Div",
    }
}

/// The first line of a fold, trimmed.
fn opener(snapshot: &KnotFoldSnapshotV1, fold: NormalizedFold) -> &str {
    snapshot.source_text[fold.start..fold.end]
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
}

pub(crate) fn fold_label(snapshot: &KnotFoldSnapshotV1, fold: NormalizedFold) -> String {
    let line = snapshot.source_text[..fold.start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let mut chars = opener(snapshot, fold).chars();
    let mut preview = chars.by_ref().take(48).collect::<String>();
    if chars.next().is_some() {
        preview.push('…');
    }
    let kind = kind_name(fold.kind);
    if preview.is_empty() {
        format!("{kind} · line {line}")
    } else {
        format!("{kind} · line {line} · {preview}")
    }
}

/// Build conceal ranges for a read-only folded reading. A collapsed fold
/// hides everything from its opening line's break to its own last break,
/// so its marker sits at the end of the opening line and the text after
/// the fold starts a line of its own.
pub(crate) fn conceal_ranges(
    snapshot: &KnotFoldSnapshotV1,
    collapsed: &BTreeSet<usize>,
) -> Vec<std::ops::Range<usize>> {
    let source = snapshot.source_text.as_str();
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
            let opening_break = fold.start + source[fold.start..fold.end].find('\n')?;
            let end = if source[..fold.end].ends_with('\n') {
                fold.end - 1
            } else {
                fold.end
            };
            (opening_break < end).then_some(opening_break..end)
        })
        .collect()
}

pub(crate) fn collapse_all_indices(snapshot: &KnotFoldSnapshotV1) -> BTreeSet<usize> {
    normalized_folds(snapshot)
        .into_iter()
        .map(|fold| fold.source_index)
        .collect()
}

/// A fold as a reader knows it across edits: its kind, its opening line,
/// and how many folds before it share both.
type FoldIdentity = (&'static str, String, usize);

fn identities(snapshot: &KnotFoldSnapshotV1) -> Vec<(usize, FoldIdentity)> {
    let mut seen = HashMap::<(&'static str, String), usize>::new();
    normalized_folds(snapshot)
        .into_iter()
        .map(|fold| {
            let key = (kind_name(fold.kind), opener(snapshot, fold).to_owned());
            let occurrence = seen.entry(key.clone()).or_default();
            let identity = (key.0, key.1, *occurrence);
            *occurrence += 1;
            (fold.source_index, identity)
        })
        .collect()
}

/// Carry collapsed folds across a change of source: a fold stays collapsed
/// while a fold of the same kind and opening line still exists, and the
/// rest open.
pub(crate) fn remap_collapsed(
    old: &KnotFoldSnapshotV1,
    collapsed: &BTreeSet<usize>,
    new: &KnotFoldSnapshotV1,
) -> BTreeSet<usize> {
    let kept = identities(old)
        .into_iter()
        .filter(|(index, _)| collapsed.contains(index))
        .map(|(_, identity)| identity)
        .collect::<HashSet<_>>();
    identities(new)
        .into_iter()
        .filter(|(_, identity)| kept.contains(identity))
        .map(|(index, _)| index)
        .collect()
}

/// The Folded source tile `tile`, reading `key`'s document.
pub(crate) fn view(state: &DesktopState, key: DocKey, tile: workbench::TileId) -> DesktopView {
    let Some(entry) = state.docs.doc(key) else {
        return Box::new(el("div", ()));
    };
    let snapshot = entry
        .fold_snapshot
        .as_ref()
        .filter(|snapshot| snapshot_matches(entry.document.session(), snapshot));
    let Some(snapshot) = snapshot else {
        return Box::new(
            span("Folds are updating; try again shortly.").attr("class", "knot-folding"),
        );
    };
    let folds = normalized_folds(snapshot);
    let conceal_ranges = conceal_ranges(snapshot, &entry.collapsed_folds);
    let styles = if state.appearance.highlight {
        cambium::note_styles(snapshot.source_text.as_str())
    } else {
        Vec::new()
    };
    let projection = match fold_projection(
        snapshot.source_text.as_str(),
        snapshot.source_text.len(),
        &conceal_ranges,
        &styles,
    ) {
        Ok(projection) => projection,
        Err(error) => {
            return Box::new(
                span(format!("Folded source unavailable: {error:?}"))
                    .attr("class", "knot-folding knot-folding-error"),
            );
        },
    };
    // Every control carries the snapshot it acts on, to refuse a stale one;
    // they share one copy rather than each cloning the source.
    let shared = Arc::new(snapshot.clone());
    let rows = projection.rows(|line: &FoldProjectionLine| {
        folds
            .iter()
            .filter(|fold| starts_on(line, fold.start))
            .map(|fold| {
                let source_index = fold.source_index;
                let collapsed = entry.collapsed_folds.contains(&source_index);
                let label = fold_label(snapshot, *fold);
                let action = if collapsed { "Expand" } else { "Collapse" };
                let fold_snapshot = Arc::clone(&shared);
                Box::new(
                    button(
                        if collapsed { "▸" } else { "▾" },
                        move |state: &mut DesktopState, _| {
                            state.toggle_fold(key, (*fold_snapshot).clone(), source_index);
                        },
                    )
                    .attr("class", "knot-fold-toggle")
                    .attr("aria-label", format!("{action} {label}"))
                    .attr("aria-expanded", (!collapsed).to_string())
                    .attr("data-fold-index", source_index.to_string()),
                ) as FieldChild<DesktopState, ()>
            })
            .collect()
    });
    let error = entry
        .fold_error
        .as_ref()
        .map(|error| span(format!("Fold error: {error}")).attr("class", "knot-folding-error"));
    let actions = el(
        "div",
        (
            button("Edit source", move |state: &mut DesktopState, _| {
                state.edit_source(key);
            }),
            button("Collapse all", {
                let fold_snapshot = Arc::clone(&shared);
                move |state: &mut DesktopState, _| {
                    state.collapse_all_folds(key, (*fold_snapshot).clone());
                }
            })
            .attr("aria-label", "Collapse all folds"),
            button("Expand all", {
                let fold_snapshot = Arc::clone(&shared);
                move |state: &mut DesktopState, _| {
                    state.expand_all_folds(key, (*fold_snapshot).clone());
                }
            })
            .attr("aria-label", "Expand all folds"),
        ),
    )
    .attr("class", "knot-folding-actions");
    let no_folds = folds
        .is_empty()
        .then(|| span("No foldable containers in this source."));
    Box::new(
        el(
            "div",
            (
                actions,
                error,
                no_folds,
                el("div", rows)
                    .attr("class", "knot-folded-source")
                    .attr("style", state.appearance.writing_style())
                    .attr("role", "document")
                    .attr("aria-label", "Read-only folded source")
                    .attr("aria-readonly", "true"),
            ),
        )
        .attr("class", "knot-folding")
        .attr("id", format!("knot-document-folding-{}", tile.0)),
    )
}

/// Whether a fold starting at `start` begins in `line`'s visible source.
fn starts_on(line: &FoldProjectionLine, start: usize) -> bool {
    line.segments.iter().any(
        |segment| matches!(segment, FoldProjectionSegment::Source(range) if range.contains(&start)),
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
    fn a_collapsed_fold_ends_its_opening_line_and_the_outer_one_wins() {
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
        let opening_break = source.find('\n').unwrap();
        assert_eq!(
            conceal_ranges(&folds, &collapsed),
            vec![opening_break..source.len() - 1],
            "the opening line keeps its text and the fold keeps its last break"
        );
        assert_eq!(&source[..opening_break], "# α");
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

    #[test]
    fn collapsed_folds_follow_their_identity_across_an_edit() {
        let before = "# A\n\none\n\n# B\n\ntwo\n\n# B\n\nthree\n";
        let a = before.find("# B").unwrap();
        let b = before.rfind("# B").unwrap();
        let old = snapshot(
            before,
            vec![
                item(KnotFoldKindV1::Section, 0, a),
                item(KnotFoldKindV1::Section, a, b),
                item(KnotFoldKindV1::Section, b, before.len()),
            ],
        );
        // The second "# B" is collapsed; an edit inserts a section above.
        let after = format!("# New\n\nzero\n\n{before}");
        let shift = after.len() - before.len();
        let new = snapshot(
            &after,
            vec![
                item(KnotFoldKindV1::Section, 0, shift),
                item(KnotFoldKindV1::Section, shift, shift + a),
                item(KnotFoldKindV1::Section, shift + a, shift + b),
                item(KnotFoldKindV1::Section, shift + b, after.len()),
            ],
        );
        let collapsed = BTreeSet::from([2]);
        assert_eq!(
            remap_collapsed(&old, &collapsed, &new),
            BTreeSet::from([3]),
            "the same heading, counted among its namesakes, stays collapsed"
        );
        let renamed = snapshot(
            &after.replace("# B\n\nthree", "# C\n\nthree"),
            new.items.clone(),
        );
        assert!(
            remap_collapsed(&old, &collapsed, &renamed).is_empty(),
            "a fold whose opening line changed reopens"
        );
    }
}
