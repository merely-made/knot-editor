// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The focused document's status chips: format, save state and posture, each
//! opening the facts behind it (slice 1 step 5c). The desktop draws these in
//! its status bar, so the document surface no longer draws its own row.

use cambium::{DetailRow, DetailSection, StatusChip, StatusSeverity, TabMark};
use knot_document::{
    KnotDocumentSaveOutcomeV1, KnotDocumentSnapshotV1, KnotDocumentWritePostureV1,
    knot_document_format_label, knot_document_posture_label, knot_document_refusal_label,
    knot_document_save_outcome_label,
};

pub(crate) const FORMAT: &str = "format";
pub(crate) const SAVE: &str = "save";
pub(crate) const POSTURE: &str = "posture";

/// One document's chips, in bar order. Quiet while inert; a refusal raises
/// the save chip to refused, a failed save to warning.
pub(crate) fn chips(snapshot: &KnotDocumentSnapshotV1) -> Vec<StatusChip> {
    let (save, severity) = if snapshot.refusal.is_some() {
        ("Save refused", StatusSeverity::Refused)
    } else if snapshot.last_save_outcome == Some(KnotDocumentSaveOutcomeV1::Failed) {
        ("Save failed", StatusSeverity::Warning)
    } else if snapshot.dirty {
        ("Unsaved changes", StatusSeverity::Quiet)
    } else if snapshot.write_posture == KnotDocumentWritePostureV1::Scratch {
        ("Not saved", StatusSeverity::Quiet)
    } else {
        ("Saved", StatusSeverity::Quiet)
    };
    vec![
        StatusChip::new(FORMAT, knot_document_format_label(snapshot.format)),
        StatusChip::new(SAVE, save).with_severity(severity),
        StatusChip::new(
            POSTURE,
            capitalized(knot_document_posture_label(snapshot.write_posture)),
        ),
    ]
}

/// The facts behind one chip, for its popover; `None` for a key no chip has.
pub(crate) fn sections(key: &str, snapshot: &KnotDocumentSnapshotV1) -> Option<Vec<DetailSection>> {
    let (title, rows) = match key {
        FORMAT => (
            "Document",
            vec![
                DetailRow::new("Format", knot_document_format_label(snapshot.format)),
                DetailRow::new("Source", snapshot.display_label.clone()),
            ],
        ),
        SAVE => {
            let mut rows = vec![
                DetailRow::new("Unsaved changes", if snapshot.dirty { "Yes" } else { "No" }),
                DetailRow::new(
                    "Last save",
                    knot_document_save_outcome_label(snapshot.last_save_outcome),
                ),
            ];
            if let Some(refusal) = snapshot.refusal {
                rows.push(DetailRow::new(
                    "Refused",
                    knot_document_refusal_label(refusal),
                ));
            }
            if let Some(failure) = &snapshot.last_save_failure {
                rows.push(DetailRow::new("Failed", failure.message.clone()));
            }
            ("Save", rows)
        },
        POSTURE => (
            "Posture",
            vec![
                DetailRow::new(
                    "Posture",
                    knot_document_posture_label(snapshot.write_posture),
                ),
                DetailRow::new("Saving", posture_sentence(snapshot.write_posture)),
            ],
        ),
        _ => return None,
    };
    Some(vec![DetailSection::new(title, rows)])
}

/// A document tab's mark: a refusal needs attention before unsaved changes.
pub(crate) fn tab_mark(snapshot: &KnotDocumentSnapshotV1) -> Option<TabMark> {
    if snapshot.refusal.is_some() {
        Some(TabMark::Attention)
    } else if snapshot.dirty {
        Some(TabMark::Modified)
    } else {
        None
    }
}

fn posture_sentence(posture: KnotDocumentWritePostureV1) -> &'static str {
    match posture {
        KnotDocumentWritePostureV1::FileTarget => "Save writes the document's file.",
        KnotDocumentWritePostureV1::Scratch => "No file yet. Save As chooses one.",
        KnotDocumentWritePostureV1::ReadOnly => "Opened read-only. Edits and saves are refused.",
    }
}

fn capitalized(label: &str) -> String {
    let mut chars = label.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_document::{KnotDocumentRefusalV1, KnotDocumentSaveFailureV1, KnotDocumentSession};

    /// Every chip label and popover row a document's status shows.
    fn shown(snapshot: &KnotDocumentSnapshotV1) -> Vec<String> {
        let mut texts = Vec::new();
        for chip in chips(snapshot) {
            texts.push(chip.label.clone());
            for section in sections(&chip.key, snapshot).expect("each chip opens facts") {
                for row in section.rows {
                    texts.push(format!("{}: {}", row.key, row.value));
                }
            }
        }
        texts
    }

    /// The surface's own row showed source, format, clean or dirty, posture,
    /// and the last save with its refusal or failure: each is still shown.
    #[test]
    fn every_fact_the_surface_row_showed_is_in_a_chip_or_its_popover() {
        let base = KnotDocumentSession::scratch("scratch:untitled", "# Notes\n").snapshot();
        let file = KnotDocumentSnapshotV1 {
            display_label: "notes.djot".into(),
            write_posture: KnotDocumentWritePostureV1::FileTarget,
            ..base.clone()
        };
        let states = [
            ("clean scratch", base.clone()),
            ("clean file", file.clone()),
            (
                "dirty file",
                KnotDocumentSnapshotV1 {
                    dirty: true,
                    last_save_outcome: Some(KnotDocumentSaveOutcomeV1::Written),
                    ..file.clone()
                },
            ),
            (
                "changed on disk",
                KnotDocumentSnapshotV1 {
                    dirty: true,
                    last_save_outcome: Some(KnotDocumentSaveOutcomeV1::Refused),
                    refusal: Some(KnotDocumentRefusalV1::ExternalChange),
                    ..file.clone()
                },
            ),
            (
                "scratch save",
                KnotDocumentSnapshotV1 {
                    last_save_outcome: Some(KnotDocumentSaveOutcomeV1::Refused),
                    refusal: Some(KnotDocumentRefusalV1::ScratchHasNoSaveTarget),
                    ..base.clone()
                },
            ),
            (
                "read-only",
                KnotDocumentSnapshotV1 {
                    write_posture: KnotDocumentWritePostureV1::ReadOnly,
                    last_save_outcome: Some(KnotDocumentSaveOutcomeV1::Refused),
                    refusal: Some(KnotDocumentRefusalV1::ReadOnly),
                    ..file.clone()
                },
            ),
            (
                "failed",
                KnotDocumentSnapshotV1 {
                    dirty: true,
                    last_save_outcome: Some(KnotDocumentSaveOutcomeV1::Failed),
                    last_save_failure: Some(KnotDocumentSaveFailureV1 {
                        message: "disk full".into(),
                    }),
                    ..file
                },
            ),
        ];
        for (name, snapshot) in states {
            let texts = shown(&snapshot);
            let mut facts = vec![
                format!("Source: {}", snapshot.display_label),
                format!("Format: {}", knot_document_format_label(snapshot.format)),
                format!(
                    "Unsaved changes: {}",
                    if snapshot.dirty { "Yes" } else { "No" }
                ),
                format!(
                    "Posture: {}",
                    knot_document_posture_label(snapshot.write_posture)
                ),
                format!(
                    "Last save: {}",
                    knot_document_save_outcome_label(snapshot.last_save_outcome)
                ),
            ];
            if let Some(refusal) = snapshot.refusal {
                facts.push(format!("Refused: {}", knot_document_refusal_label(refusal)));
            }
            if let Some(failure) = &snapshot.last_save_failure {
                facts.push(format!("Failed: {}", failure.message));
            }
            for fact in facts {
                assert!(
                    texts.contains(&fact),
                    "{name}: {fact:?} is not shown in {texts:?}"
                );
            }
        }
    }

    #[test]
    fn a_refusal_raises_the_save_chip_and_marks_the_tab_before_unsaved_changes() {
        let base = KnotDocumentSession::scratch("scratch:untitled", "x").snapshot();
        let save = |snapshot: &KnotDocumentSnapshotV1| {
            chips(snapshot)
                .into_iter()
                .find(|chip| chip.key == SAVE)
                .map(|chip| (chip.label, chip.severity))
        };
        let refused = KnotDocumentSnapshotV1 {
            dirty: true,
            refusal: Some(KnotDocumentRefusalV1::ScratchHasNoSaveTarget),
            ..base.clone()
        };
        assert_eq!(
            save(&refused),
            Some(("Save refused".into(), StatusSeverity::Refused))
        );
        assert_eq!(tab_mark(&refused), Some(TabMark::Attention));
        let dirty = KnotDocumentSnapshotV1 {
            dirty: true,
            ..base.clone()
        };
        assert_eq!(
            save(&dirty),
            Some(("Unsaved changes".into(), StatusSeverity::Quiet))
        );
        assert_eq!(tab_mark(&dirty), Some(TabMark::Modified));
        assert_eq!(
            save(&base),
            Some(("Not saved".into(), StatusSeverity::Quiet))
        );
        assert_eq!(tab_mark(&base), None);
    }
}
