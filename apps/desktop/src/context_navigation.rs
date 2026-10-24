// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Application scope topology derived from the live source outline.
//!
//! Persisted places use source addresses and site roots. Runtime document and
//! tile keys are deliberately not part of their identity: they are rebound
//! only against sessions and sites the current launch actually opened.

use cambium::{CaretAffinity, CaretPosition, CaretSelection};
use knot_document::{KnotDocumentSnapshotV1, KnotOutlineItemV1};
use serde::{Deserialize, Serialize};
use crate::documents::{DocKey, SiteKey};
use workbench::TileId;

/// A runtime admission. It is deliberately omitted from disk: a durable
/// address is rebound only during startup against documents/sites already
/// admitted by the host. Closing and reopening within one window gets a new
/// key, so old visits cannot silently retarget it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContextBinding {
    Document(DocKey),
    Site(SiteKey),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ContextPlace {
    Document {
        address: String,
        /// A persisted open-session identity, distinct from the source
        /// address. Reopened copies in one window must not merge old routes.
        #[serde(default)]
        admission_id: String,
        #[serde(skip)]
        binding: Option<ContextBinding>,
    },
    Heading {
        address: String,
        #[serde(default)]
        admission_id: String,
        start: usize,
        end: usize,
        label: String,
        source_digest: [u8; 32],
        #[serde(skip)]
        binding: Option<ContextBinding>,
    },
    Site {
        root: String,
        #[serde(default)]
        admission_id: String,
        #[serde(skip)]
        binding: Option<ContextBinding>,
    },
}

impl ContextPlace {
    pub(crate) fn document_address(&self) -> Option<&str> {
        match self {
            Self::Document { address, .. } | Self::Heading { address, .. } => Some(address),
            Self::Site { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ContextView {
    #[default]
    Source,
    Preview,
    Outline,
    Site,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CaretAffinityBookmark {
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CaretPositionBookmark {
    pub(crate) byte: usize,
    pub(crate) affinity: CaretAffinityBookmark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CaretSelectionBookmark {
    pub(crate) anchor: CaretPositionBookmark,
    pub(crate) focus: CaretPositionBookmark,
}

impl From<CaretPosition> for CaretPositionBookmark {
    fn from(value: CaretPosition) -> Self {
        Self {
            byte: value.byte,
            affinity: match value.affinity {
                CaretAffinity::Upstream => CaretAffinityBookmark::Upstream,
                CaretAffinity::Downstream => CaretAffinityBookmark::Downstream,
            },
        }
    }
}

impl From<CaretPositionBookmark> for CaretPosition {
    fn from(value: CaretPositionBookmark) -> Self {
        Self {
            byte: value.byte,
            affinity: match value.affinity {
                CaretAffinityBookmark::Upstream => CaretAffinity::Upstream,
                CaretAffinityBookmark::Downstream => CaretAffinity::Downstream,
            },
        }
    }
}

impl From<CaretSelection> for CaretSelectionBookmark {
    fn from(value: CaretSelection) -> Self {
        Self {
            anchor: value.anchor.into(),
            focus: value.focus.into(),
        }
    }
}

impl From<CaretSelectionBookmark> for CaretSelection {
    fn from(value: CaretSelectionBookmark) -> Self {
        Self {
            anchor: value.anchor.into(),
            focus: value.focus.into(),
        }
    }
}

/// A rendered-text byte carries its own preview-text digest and the enclosing
/// context anchor also carries the source digest. Heading and block fallbacks
/// are never interpreted against a different source revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ContextScrollAnchor {
    SourceByte { byte: usize },
    PreviewTextByte { byte: usize, rendered_digest: [u8; 32] },
    PreviewBlock { index: usize },
    PreviewHeading { start: usize, end: usize },
}

#[derive(Clone, Debug)]
pub(crate) struct PendingContextScroll {
    pub(crate) document: DocKey,
    pub(crate) tile: TileId,
    pub(crate) source_digest: [u8; 32],
    pub(crate) anchor: ContextScrollAnchor,
    pub(crate) attempts: u8,
}

/// A source-bound editor bookmark. Offsets are applied only while the exact
/// address and source bytes remain live in the same open document session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ContextAnchor {
    pub(crate) document_address: String,
    pub(crate) source_digest: [u8; 32],
    pub(crate) selection: CaretSelectionBookmark,
    #[serde(default)]
    pub(crate) view: ContextView,
    #[serde(default = "source_is_present")]
    pub(crate) source_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) preview_present: Option<bool>,
    #[serde(default)]
    pub(crate) source_scroll: Option<ContextScrollAnchor>,
    #[serde(default)]
    pub(crate) preview_scroll: Option<ContextScrollAnchor>,
    /// The previous one-slot wire value. It is read for migration and omitted
    /// on the next save so the two independent view anchors become canonical.
    #[serde(default, rename = "scroll", skip_serializing)]
    legacy_scroll: Option<ContextScrollAnchor>,
    /// A same-process optimization only. Persistent history resolves its peer
    /// view by kind and the live open document address.
    #[serde(skip)]
    pub(crate) active_tile: Option<TileId>,
}

impl ContextAnchor {
    pub(crate) fn from_snapshot(
        snapshot: &KnotDocumentSnapshotV1,
        view: ContextView,
        active_tile: Option<TileId>,
        source_present: bool,
        preview_present: bool,
    ) -> Self {
        Self {
            document_address: snapshot.source.address.clone(),
            source_digest: *blake3::hash(snapshot.text.as_bytes()).as_bytes(),
            selection: snapshot.selection.into(),
            view,
            source_present,
            preview_present: Some(preview_present),
            source_scroll: Some(ContextScrollAnchor::SourceByte {
                byte: snapshot.selection.focus.byte,
            }),
            preview_scroll: None,
            legacy_scroll: None,
            active_tile,
        }
    }

    /// Move the previous single scroll value into its view-specific slot.
    /// Old Preview visits were active visits, so their pane was present; this
    /// inference does not reopen a Preview from a scroll value in a new-format
    /// visit whose explicit `preview_present` bit is false.
    pub(crate) fn migrate_legacy_scroll(&mut self) {
        let mut legacy_preview = false;
        if let Some(scroll) = self.legacy_scroll.take() {
            match scroll {
                scroll @ ContextScrollAnchor::SourceByte { .. } => {
                    self.source_scroll.get_or_insert(scroll);
                },
                scroll @ (ContextScrollAnchor::PreviewTextByte { .. }
                | ContextScrollAnchor::PreviewBlock { .. }
                | ContextScrollAnchor::PreviewHeading { .. }) => {
                    self.preview_scroll.get_or_insert(scroll);
                    legacy_preview = true;
                },
            }
        }
        if self.preview_present.is_none() {
            self.preview_present = Some(legacy_preview || self.view == ContextView::Preview);
        }
        self.source_present = true;
    }

    pub(crate) fn preview_is_present(&self) -> bool {
        self.preview_present.unwrap_or(self.view == ContextView::Preview)
    }

    pub(crate) fn active_scroll(&self) -> Option<&ContextScrollAnchor> {
        match self.view {
            ContextView::Preview => self.preview_scroll.as_ref(),
            _ => self.source_scroll.as_ref(),
        }
    }

    pub(crate) fn matches(&self, snapshot: &KnotDocumentSnapshotV1) -> bool {
        self.document_address == snapshot.source.address
            && self.source_digest == *blake3::hash(snapshot.text.as_bytes()).as_bytes()
    }
}

fn source_is_present() -> bool {
    true
}

/// Return each heading's nearest preceding shallower heading, if one exists.
pub(crate) fn heading_parents(items: &[KnotOutlineItemV1]) -> Vec<Option<usize>> {
    let mut parents = Vec::with_capacity(items.len());
    let mut ancestors: Vec<usize> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        while ancestors
            .last()
            .is_some_and(|ancestor| items[*ancestor].level >= item.level)
        {
            ancestors.pop();
        }
        parents.push(ancestors.last().copied());
        ancestors.push(index);
    }
    parents
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, level: u8) -> KnotOutlineItemV1 {
        KnotOutlineItemV1 {
            label: label.to_owned(),
            level,
            start: 0,
            end: 1,
        }
    }

    #[test]
    fn nested_outline_sections_keep_the_nearest_containing_parent() {
        let outline = [
            item("Root", 1),
            item("Branch", 2),
            item("Deep", 4),
            item("Deep sibling", 4),
            item("Branch sibling", 2),
            item("Next root", 1),
        ];

        assert_eq!(
            heading_parents(&outline),
            [None, Some(0), Some(1), Some(1), Some(0), None]
        );
    }

    #[test]
    fn an_up_route_targets_the_containing_section_not_the_previous_outline_row() {
        let outline = [
            item("Root", 1),
            item("First child", 2),
            item("Second child", 2),
            item("Nested", 3),
        ];
        let parents = heading_parents(&outline);

        assert_eq!(parents[2], Some(0));
        assert_eq!(parents[3], Some(2));
    }

    #[test]
    fn bookmark_serializes_stable_address_and_selection_without_runtime_tile_key() {
        let session = knot_document::KnotDocumentSession::read_only_with_format(
            "file:///notes.knot",
            "# Original\n",
            knot_document::DocumentFormat::Knot,
        );
        let anchor = ContextAnchor::from_snapshot(
            &session.snapshot(),
            ContextView::Preview,
            Some(TileId(7)),
            true,
            true,
        );
        let wire = serde_json::to_string(&anchor).unwrap();
        assert!(wire.contains("file:///notes.knot"));
        assert!(!wire.contains("active_tile"));
        let restored: ContextAnchor = serde_json::from_str(&wire).unwrap();
        assert_eq!(restored.view, ContextView::Preview);
        assert_eq!(restored.active_tile, None);
        assert!(restored.matches(&session.snapshot()));
        assert!(restored.source_present);
        assert!(restored.preview_is_present());
    }

    #[test]
    fn legacy_single_scroll_migrates_to_its_pane_and_closed_preview_stays_closed() {
        let legacy = serde_json::json!({
            "document_address": "file:///notes.knot",
            "source_digest": vec![0; 32],
            "selection": {
                "anchor": { "byte": 4, "affinity": "Downstream" },
                "focus": { "byte": 4, "affinity": "Downstream" }
            },
            "view": "Source",
            "scroll": { "SourceByte": { "byte": 88 } }
        });
        let mut source: ContextAnchor = serde_json::from_value(legacy.clone()).unwrap();
        source.migrate_legacy_scroll();
        assert_eq!(source.source_scroll, Some(ContextScrollAnchor::SourceByte { byte: 88 }));
        assert!(!source.preview_is_present());
        let saved = serde_json::to_value(&source).unwrap();
        assert!(saved.get("scroll").is_none());

        let mut preview: ContextAnchor = serde_json::from_value(serde_json::json!({
            "document_address": "file:///notes.knot",
            "source_digest": vec![0; 32],
            "selection": {
                "anchor": { "byte": 4, "affinity": "Downstream" },
                "focus": { "byte": 4, "affinity": "Downstream" }
            },
            "view": "Preview",
            "source_present": true,
            "preview_present": false,
            "preview_scroll": { "PreviewTextByte": { "byte": 500, "rendered_digest": vec![0; 32] } }
        })).unwrap();
        preview.migrate_legacy_scroll();
        assert!(!preview.preview_is_present(), "new-format false presence is authoritative");
        assert!(preview.preview_scroll.is_some());
    }
}
