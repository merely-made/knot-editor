// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Changes tile: how a document's buffer stands against its file on
//! disk, and, with a catalog, the saved revision a reviewer would retain.
//! Both are snapshots taken on request; each says when it has gone stale
//! and offers to read disk again.

use cambium::{button, el, span};

use crate::documents::DocKey;
use crate::workspace::{DesktopState, DesktopView, DocumentEntry};

/// The Changes tile `tile`, reading `key`'s document.
pub(crate) fn view(state: &DesktopState, key: DocKey, tile: workbench::TileId) -> DesktopView {
    let Some(entry) = state.docs.doc(key) else {
        return Box::new(el("div", ()));
    };
    Box::new(
        el("div", (comparison(entry, key), review(state, entry, key)))
            .attr("class", "knot-changes")
            .attr("id", format!("knot-changes-{}", tile.0)),
    )
}

fn refresh_comparison(key: DocKey) -> DesktopView {
    Box::new(button(
        "Refresh comparison",
        move |state: &mut DesktopState, _| {
            state.compare_disk_for(key);
        },
    ))
}

fn hide_comparison(key: DocKey) -> DesktopView {
    Box::new(button(
        "Hide comparison",
        move |state: &mut DesktopState, _| {
            state.hide_comparison_for(key);
        },
    ))
}

/// The disk comparison: its error, its two versions, or an offer to take one.
fn comparison(entry: &DocumentEntry, key: DocKey) -> DesktopView {
    if let Some(error) = &entry.comparison_error {
        return Box::new(
            el(
                "section",
                (
                    span("Comparison unavailable"),
                    span(error.clone()).attr("class", "knot-comparison-error"),
                    span("Disk text could not be read; refresh to try again."),
                    refresh_comparison(key),
                    hide_comparison(key),
                ),
            )
            .attr("class", "knot-comparison knot-comparison-error-panel")
            .attr("role", "region")
            .attr("aria-label", "Disk comparison error"),
        );
    }
    let Some(comparison) = &entry.comparison else {
        return Box::new(
            el(
                "section",
                (
                    span("Not compared with disk."),
                    button("Compare with disk", move |state: &mut DesktopState, _| {
                        state.compare_disk_for(key);
                    }),
                ),
            )
            .attr("class", "knot-comparison knot-comparison-empty"),
        );
    };
    let snapshot = entry.document.snapshot();
    let stale =
        comparison.buffer_text != snapshot.text || comparison.address != snapshot.source.address;
    let status = if stale {
        "Snapshot: stale because the source or address changed since comparison"
    } else {
        "Buffer snapshot matches current source"
    };
    let disk_status = if comparison.disk_changed_since_baseline {
        "Disk changed since the saved baseline at comparison: yes"
    } else {
        "Disk changed since the saved baseline at comparison: no"
    };
    Box::new(
        el(
            "section",
            (
                el("header", (span("Disk comparison"), span(status)))
                    .attr("class", "knot-comparison-header"),
                span(format!("Compared address: {}", comparison.address)),
                span(disk_status),
                span("Disk text was read when compared; refresh to read it again."),
                refresh_comparison(key),
                hide_comparison(key),
                el(
                    "div",
                    (
                        el(
                            "section",
                            (
                                span("Buffer at comparison"),
                                el("pre", comparison.buffer_text.clone()),
                            ),
                        )
                        .attr("class", "knot-comparison-version")
                        .attr("aria-label", "Buffer source at comparison"),
                        el(
                            "section",
                            (
                                span("Disk at comparison"),
                                el("pre", comparison.disk_text.clone()),
                            ),
                        )
                        .attr("class", "knot-comparison-version")
                        .attr("aria-label", "Disk source at comparison"),
                    ),
                )
                .attr("class", "knot-comparison-versions"),
            ),
        )
        .attr("class", "knot-comparison")
        .attr("role", "region")
        .attr("aria-label", "Disk comparison"),
    )
}

/// The saved revision review, when this window has a catalog.
fn review(state: &DesktopState, entry: &DocumentEntry, key: DocKey) -> DesktopView {
    if state.catalog.is_none() {
        return Box::new(el("div", ()));
    }
    let header = || {
        el(
            "header",
            (
                span("Saved revision review"),
                span(format!("Limit: {} bytes", state.capture_limit)),
            ),
        )
        .attr("class", "knot-review-header")
    };
    let refresh = move || {
        button(
            "Refresh saved revision",
            move |state: &mut DesktopState, _| {
                state.prepare_capture_for(key);
            },
        )
    };
    let discard = move || {
        button(
            "Discard saved revision",
            move |state: &mut DesktopState, _| {
                state.discard_prepared_capture_for(key);
            },
        )
    };
    if let Some(error) = &entry.prepared_capture_error {
        return Box::new(
            el(
                "section",
                (
                    header(),
                    span(format!("Review unavailable: {error}")).attr("class", "knot-review-error"),
                    entry.catalog_id.as_ref().map(|_| refresh()),
                    discard(),
                ),
            )
            .attr("class", "knot-review knot-review-error-panel")
            .attr("role", "region")
            .attr("aria-label", "Saved revision review error"),
        );
    }
    if let Some(revision) = &entry.prepared_capture {
        let source_path = entry.prepared_capture_source_path.as_ref().map_or_else(
            || "(source path unavailable)".to_owned(),
            |path| path.display().to_string(),
        );
        let differs = entry.document.snapshot().text.as_bytes() != revision.body.as_slice();
        let status = if differs {
            "Editor differs from prepared revision; refresh to read disk again"
        } else {
            "Prepared snapshot; refresh to read disk again"
        };
        let body = String::from_utf8(revision.body.clone()).expect("validated review body");
        return Box::new(
            el(
                "section",
                (
                    header(),
                    span(format!("Document ID: {}", revision.document_id)),
                    span(format!("Title: {}", revision.title)),
                    span(format!("Media type: {}", revision.media_type)),
                    span(format!("Source path: {source_path}")),
                    span(format!("Prepared bytes: {}", revision.body.len())),
                    span("Unsaved changes are excluded."),
                    span("Reviewing does not store or share bytes. Retain uses the selected destination."),
                    span(status).attr("class", "knot-review-status"),
                    refresh(),
                    discard(),
                    el("pre", body).attr("class", "knot-review-source"),
                ),
            )
            .attr("class", "knot-review")
            .attr("role", "region")
            .attr("aria-label", "Saved revision review"),
        );
    }
    let Some(id) = &entry.catalog_id else {
        return Box::new(el("div", ()));
    };
    Box::new(
        el(
            "section",
            (
                header(),
                button(
                    "Review saved revision",
                    move |state: &mut DesktopState, _| {
                        state.prepare_capture_for(key);
                    },
                ),
            ),
        )
        .attr("class", "knot-review")
        .attr("aria-label", format!("Saved revision review for {id}")),
    )
}
