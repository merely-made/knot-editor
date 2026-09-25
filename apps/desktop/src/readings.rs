// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Readings tile: run a sandboxed rhai reading over its document and show
//! its rows as selectable source ranges.
//!
//! A reading is derived state, so the tile labels every one of them with the
//! source, script and cost it came from, and says so when the source has moved
//! underneath it. A stale reading is still worth looking at, so it stays and
//! says it is stale, and its rows refuse to select.

use cambium::{Keyed, button, el, span};
use knot_readings::{ReadingError, ReadingNoteV1, ReadingResult};

use crate::documents::DocKey;
use crate::workspace::{DesktopState, DesktopView};

pub const CSS: &str = concat!(
    ".knot-readings { min-width:0; box-sizing:border-box; display:flex; flex-direction:column; ",
    "gap:8px; overflow:auto; }",
    ".knot-readings-header { display:flex; flex-wrap:wrap; align-items:baseline; gap:8px; }",
    ".knot-readings-scripts { display:flex; flex-wrap:wrap; gap:6px; }",
    ".knot-readings-rows { display:flex; flex-direction:column; gap:2px; }",
    ".knot-readings-row { display:block; width:100%; text-align:left; }",
    ".knot-readings-provenance { opacity:.8; }",
    ".knot-readings-stale { color:darkorange; }",
    ".knot-readings-error { color:crimson; }",
    ".knot-readings-note { margin:0; white-space:pre-wrap; overflow-wrap:anywhere; ",
    "user-select:text; }",
);

/// The first eight hex digits of a hash, the panel's short form throughout.
fn short(hash: &[u8; 32]) -> String {
    hash.iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn provenance_line(result: &ReadingResult) -> String {
    let provenance = &result.provenance;
    format!(
        "{} · {} · {} · source {} · {} rows · {} ops · {} ms",
        provenance.script_name,
        short(&provenance.script_hash),
        provenance.source.address,
        short(&provenance.source.hash),
        result.rows.len(),
        provenance.ops_used,
        provenance.elapsed_micros / 1_000,
    )
}

pub(crate) fn error_line(error: &ReadingError) -> String {
    error.to_string()
}

/// Render a note through the engine its format names. `plain` stays a `<pre>`;
/// the note's address is its own, so a heading inside a note can never select a
/// range in the document.
fn note_view(note: &ReadingNoteV1) -> DesktopView {
    use inker::{Engine, EngineInput};
    let input = EngineInput::new("knot:reading-note", note.text.clone());
    let rendered = match note.format.as_str() {
        "djot" | "knot" => nematic::DjotKnotEngine::new().render(&input).ok(),
        "markdown" => nematic::MarkdownEngine::new().render(&input).ok(),
        "gemtext" => nematic::GemtextEngine::new()
            .render(&input.with_content_type("text/gemini"))
            .ok(),
        _ => None,
    };
    match rendered {
        Some(document) => Box::new(
            el("div", crate::document_preview::note_blocks(&document))
                .attr("class", "knot-readings-note"),
        ),
        None => Box::new(el("pre", note.text.clone()).attr("class", "knot-readings-note")),
    }
}

/// The Readings tile `tile`, reading `key`'s document.
pub(crate) fn view(state: &DesktopState, key: DocKey, tile: workbench::TileId) -> DesktopView {
    let Some(entry) = state.docs.doc(key) else {
        return Box::new(el("div", ()));
    };
    let chosen = state
        .reading_script(tile)
        .map(|script| script.name.as_str());
    let scripts = state
        .readings
        .iter()
        .enumerate()
        .map(|(index, script)| {
            let selected = chosen == Some(script.name.as_str());
            let label = format!("{} · {}", script.name, short(&script.hash));
            (
                index,
                button(label.clone(), move |state: &mut DesktopState, _| {
                    state.select_reading(tile, index);
                })
                .attr("aria-pressed", selected.to_string())
                .attr("aria-label", format!("Reading script: {label}"))
                .attr("data-reading-index", index.to_string()),
            )
        })
        .collect::<Vec<_>>();
    let scripts_view: DesktopView = if scripts.is_empty() {
        Box::new(span("No readings in this folder."))
    } else {
        Box::new(el("div", Keyed::new(scripts)).attr("class", "knot-readings-scripts"))
    };
    let run_reason = if chosen.is_none() {
        Some("Choose a reading to run.")
    } else {
        None
    };
    let run = button("Run", move |state: &mut DesktopState, _| {
        state.run_reading(tile)
    })
    .attr("aria-disabled", run_reason.is_some().to_string())
    .attr(
        "aria-label",
        run_reason.map_or("Run reading".to_owned(), |reason| {
            format!("Run reading — {reason}")
        }),
    );
    let stale = state.reading_is_stale(key);
    let provenance = entry
        .reading_result
        .as_ref()
        .map(|result| span(provenance_line(result)).attr("class", "knot-readings-provenance"));
    let stale_line = (stale && entry.reading_result.is_some()).then(|| {
        span("This reading is stale: the source moved since it ran. Run it again.")
            .attr("class", "knot-readings-stale")
    });
    let error = entry
        .reading_error
        .as_ref()
        .map(|error| span(error_line(error)).attr("class", "knot-readings-error"));
    let rows_view: DesktopView = match entry.reading_result.as_ref() {
        None => Box::new(span("No reading has run yet.")),
        Some(result) if result.rows.is_empty() => Box::new(span("This reading returned no rows.")),
        Some(result) => {
            let rows = result
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let selectable = row.span.is_some() && !stale;
                    let label = match (&row.span, stale) {
                        (None, _) => format!("{} · no source range", row.label),
                        (Some(_), true) => format!("{} · stale", row.label),
                        (Some(_), false) => row.label.clone(),
                    };
                    (
                        index,
                        button(label.clone(), move |state: &mut DesktopState, _| {
                            state.select_reading_row_in(key, index);
                        })
                        .attr("class", "knot-readings-row")
                        .attr("data-reading-row", index.to_string())
                        .attr("aria-disabled", (!selectable).to_string())
                        .attr("aria-label", format!("Reading row: {label}")),
                    )
                })
                .collect::<Vec<_>>();
            Box::new(el("div", Keyed::new(rows)).attr("class", "knot-readings-rows"))
        },
    };
    let notes = entry.reading_result.as_ref().map(|result| {
        el(
            "div",
            Keyed::new(
                result
                    .notes
                    .iter()
                    .enumerate()
                    .map(|(index, note)| (index, el("div", note_view(note))))
                    .collect::<Vec<_>>(),
            ),
        )
    });
    // Relations are supplied by a relation authority the desktop does not hold
    // yet, so say that rather than showing an honest-looking zero.
    let relations = span("No space attached: relations() is empty in this window.");
    Box::new(
        el(
            "section",
            (
                el(
                    "div",
                    (
                        span(state.readings_root_label()),
                        button("Refresh", |state: &mut DesktopState, _| {
                            state.refresh_readings();
                        })
                        .attr("aria-label", "Refresh readings"),
                    ),
                )
                .attr("class", "knot-readings-header"),
                (scripts_view, run, load_notes(state)),
                (provenance, stale_line, error),
                (rows_view, notes, relations),
            ),
        )
        .attr("class", "knot-readings")
        .attr("id", format!("knot-readings-{}", tile.0)),
    )
}

/// Every refused script says why; nothing is dropped silently.
fn load_notes(state: &DesktopState) -> DesktopView {
    if state.readings_load_notes.is_empty() {
        return Box::new(el("div", ()));
    }
    Box::new(
        span(format!("Skipped: {}", state.readings_load_notes.join("; ")))
            .attr("class", "knot-readings-error"),
    )
}
