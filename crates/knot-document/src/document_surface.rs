// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::{DocumentFormat, KnotEditor, KnotEditorSaveError, SaveOutcome};
use cambium::{CaretAffinity, CaretPosition};
use cambium::{CaretSelection, TextCommand, TextInput};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentSourceKindV1 {
    File,
    Scratch,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSourceV1 {
    pub kind: KnotDocumentSourceKindV1,
    pub address: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentWritePostureV1 {
    FileTarget,
    Scratch,
    ReadOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentSaveOutcomeV1 {
    Written,
    Unchanged,
    Refused,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotDocumentRefusalV1 {
    ScratchHasNoSaveTarget,
    ReadOnly,
    ExternalChange,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSaveFailureV1 {
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotDocumentIntentErrorV1 {
    Refused(KnotDocumentRefusalV1),
    SaveFailed(KnotDocumentSaveFailureV1),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentSnapshotV1 {
    pub source: KnotDocumentSourceV1,
    pub display_label: String,
    pub format: DocumentFormat,
    pub text: String,
    pub selection: CaretSelection,
    pub dirty: bool,
    pub write_posture: KnotDocumentWritePostureV1,
    pub last_save_outcome: Option<KnotDocumentSaveOutcomeV1>,
    pub refusal: Option<KnotDocumentRefusalV1>,
    pub last_save_failure: Option<KnotDocumentSaveFailureV1>,
}
/// A read-only point-in-time observation of a file-backed session's source.
/// It does not authorize overwriting the observed file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDiskComparisonV1 {
    pub address: String,
    pub buffer_text: String,
    pub disk_text: String,
    pub disk_changed_since_baseline: bool,
}
/// A source-bound outline projection from the committed editor text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotOutlineSnapshotV1 {
    pub address: String,
    pub source_text: String,
    pub items: Vec<KnotOutlineItemV1>,
}
/// One heading's UTF-8 byte span, including its source heading syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotOutlineItemV1 {
    pub label: String,
    pub level: u8,
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotDocumentIntentV1 {
    Edit(TextCommand),
    Save,
    SaveAs(PathBuf),
    /// The caller must obtain any discard confirmation before dispatching this.
    Reload,
}

/// One selected document over the retained Knot editor. There is no second buffer.
pub struct KnotDocumentSession {
    editor: KnotEditor,
    write_posture: KnotDocumentWritePostureV1,
    last_save_outcome: Option<KnotDocumentSaveOutcomeV1>,
    refusal: Option<KnotDocumentRefusalV1>,
    last_save_failure: Option<KnotDocumentSaveFailureV1>,
}
impl KnotDocumentSession {
    pub fn open(path: impl Into<std::path::PathBuf>) -> Result<Self, String> {
        Self::open_with_posture(path, KnotDocumentWritePostureV1::FileTarget)
    }

    /// Opens a local document for inspection while retaining its file identity.
    ///
    /// The session refuses both edit and save intents. A host uses this when it
    /// has deliberately admitted a file without delegating write authority.
    pub fn open_read_only(path: impl Into<std::path::PathBuf>) -> Result<Self, String> {
        Self::open_with_posture(path, KnotDocumentWritePostureV1::ReadOnly)
    }

    fn open_with_posture(
        path: impl Into<std::path::PathBuf>,
        write_posture: KnotDocumentWritePostureV1,
    ) -> Result<Self, String> {
        let editor = KnotEditor::open(path)?;
        let write_posture = if write_posture == KnotDocumentWritePostureV1::FileTarget
            && editor.is_file_read_only()
        {
            KnotDocumentWritePostureV1::ReadOnly
        } else {
            write_posture
        };
        Ok(Self {
            editor,
            write_posture,
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        })
    }
    pub fn scratch(address: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            editor: KnotEditor::scratch(address, source),
            write_posture: KnotDocumentWritePostureV1::Scratch,
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        }
    }

    /// Builds an intentionally immutable in-memory document projection.
    pub fn read_only(address: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            editor: KnotEditor::scratch(address, source),
            write_posture: KnotDocumentWritePostureV1::ReadOnly,
            last_save_outcome: None,
            refusal: None,
            last_save_failure: None,
        }
    }
    pub fn input(&self) -> &TextInput {
        self.editor.input()
    }
    /// The local file selected for this session, if it has one.
    pub fn source_path(&self) -> Option<&Path> {
        self.editor.path()
    }
    /// Borrows the input only when this session delegated text-write authority.
    ///
    /// Hosts should route document mutations through [`Self::apply`]. This
    /// guarded escape hatch remains for compatible editable text hosts.
    pub fn input_mut(&mut self) -> Result<&mut TextInput, KnotDocumentRefusalV1> {
        if self.write_posture == KnotDocumentWritePostureV1::ReadOnly {
            self.refuse_read_only_edit();
            return Err(KnotDocumentRefusalV1::ReadOnly);
        }
        Ok(self.editor.input_mut())
    }

    /// The editable view has already selected its writable branch. Kept crate
    /// private so a product cannot bypass [`Self::input_mut`] for a read-only
    /// session.
    pub(crate) fn input_mut_for_editable_view(&mut self) -> &mut TextInput {
        debug_assert_ne!(self.write_posture, KnotDocumentWritePostureV1::ReadOnly);
        self.editor.input_mut()
    }
    pub fn snapshot(&self) -> KnotDocumentSnapshotV1 {
        let path = self.editor.path();
        let file = path.is_some();
        KnotDocumentSnapshotV1 {
            source: KnotDocumentSourceV1 {
                kind: if file {
                    KnotDocumentSourceKindV1::File
                } else {
                    KnotDocumentSourceKindV1::Scratch
                },
                address: self.editor.address().to_owned(),
            },
            display_label: path
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| self.editor.address().to_owned()),
            format: self.editor.format(),
            text: self.editor.source().to_owned(),
            selection: self.editor.selection(),
            dirty: self.editor.is_dirty(),
            write_posture: self.write_posture,
            last_save_outcome: self.last_save_outcome,
            refusal: self.refusal,
            last_save_failure: self.last_save_failure.clone(),
        }
    }
    /// Observe the current file without changing this session's source,
    /// baseline, selection, undo history, dirty state, or save refusal.
    ///
    /// The returned external text is a point-in-time snapshot and does not
    /// authorize overwriting the observed file.
    pub fn compare_disk(&self) -> Result<KnotDiskComparisonV1, String> {
        let (disk_text, disk_changed_since_baseline) = self.editor.compare_disk()?;
        Ok(KnotDiskComparisonV1 {
            address: self.editor.address().to_owned(),
            buffer_text: self.editor.source().to_owned(),
            disk_text,
            disk_changed_since_baseline,
        })
    }
    /// Returns a point-in-time outline of committed source without updating session state.
    pub fn outline_snapshot(&self) -> KnotOutlineSnapshotV1 {
        KnotOutlineSnapshotV1 {
            address: self.editor.address().to_owned(),
            source_text: self.editor.source().to_owned(),
            items: self.outline_items(),
        }
    }
    /// Selects an item only when its source-bound snapshot still matches this session.
    pub fn select_outline_item(
        &mut self,
        snapshot: &KnotOutlineSnapshotV1,
        index: usize,
    ) -> Result<(), String> {
        let source = self.editor.source();
        if snapshot.address != self.editor.address() || snapshot.source_text != source {
            return Err("outline snapshot is stale for this document source".to_owned());
        }
        if snapshot.items != self.outline_items() {
            return Err("outline snapshot items do not match this document source".to_owned());
        }
        let item = snapshot
            .items
            .get(index)
            .ok_or_else(|| format!("outline item index {index} is out of range"))?;
        if item.start > item.end
            || item.end > source.len()
            || !source.is_char_boundary(item.start)
            || !source.is_char_boundary(item.end)
        {
            return Err("outline item range is not a valid source span".to_owned());
        }
        self.editor.apply_layout_selection(CaretSelection {
            anchor: CaretPosition {
                byte: item.start,
                affinity: CaretAffinity::Downstream,
            },
            focus: CaretPosition {
                byte: item.end,
                affinity: CaretAffinity::Upstream,
            },
        });
        Ok(())
    }
    fn outline_items(&self) -> Vec<KnotOutlineItemV1> {
        self.editor.outline_items()
    }
    pub fn apply(
        &mut self,
        intent: KnotDocumentIntentV1,
    ) -> Result<KnotDocumentSnapshotV1, KnotDocumentIntentErrorV1> {
        match intent {
            KnotDocumentIntentV1::Edit(command) => {
                if self.write_posture == KnotDocumentWritePostureV1::ReadOnly {
                    self.refuse_read_only_edit();
                    return Err(KnotDocumentIntentErrorV1::Refused(
                        KnotDocumentRefusalV1::ReadOnly,
                    ));
                }
                self.editor.apply(command);
                self.refusal = None;
            },
            KnotDocumentIntentV1::Save => self.save()?,
            KnotDocumentIntentV1::SaveAs(path) => self.save_as(path)?,
            KnotDocumentIntentV1::Reload => self.reload()?,
        };
        Ok(self.snapshot())
    }
    fn save(&mut self) -> Result<(), KnotDocumentIntentErrorV1> {
        if self.write_posture == KnotDocumentWritePostureV1::ReadOnly
            || self.editor.is_file_read_only()
        {
            return self.refuse_read_only_write();
        }
        if self.editor.path().is_none() {
            self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Refused);
            self.refusal = Some(KnotDocumentRefusalV1::ScratchHasNoSaveTarget);
            self.last_save_failure = None;
            return Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ScratchHasNoSaveTarget,
            ));
        }
        match self.editor.save_guarded() {
            Ok(SaveOutcome::Written) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Written);
                self.refusal = None;
                self.last_save_failure = None;
                Ok(())
            },
            Ok(SaveOutcome::Unchanged) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Unchanged);
                self.refusal = None;
                self.last_save_failure = None;
                Ok(())
            },
            Err(KnotEditorSaveError::ExternalChange) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Refused);
                self.refusal = Some(KnotDocumentRefusalV1::ExternalChange);
                self.last_save_failure = None;
                Err(KnotDocumentIntentErrorV1::Refused(
                    KnotDocumentRefusalV1::ExternalChange,
                ))
            },
            Err(KnotEditorSaveError::Failed(message)) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Failed);
                self.refusal = None;
                let failure = KnotDocumentSaveFailureV1 { message };
                self.last_save_failure = Some(failure.clone());
                Err(KnotDocumentIntentErrorV1::SaveFailed(failure))
            },
        }
    }

    fn save_as(&mut self, path: PathBuf) -> Result<(), KnotDocumentIntentErrorV1> {
        if self.write_posture == KnotDocumentWritePostureV1::ReadOnly {
            return self.refuse_read_only_write();
        }
        match self.editor.save_as(path) {
            Ok(outcome) => {
                self.write_posture = KnotDocumentWritePostureV1::FileTarget;
                self.record_save_outcome(outcome);
                Ok(())
            },
            Err(KnotEditorSaveError::ExternalChange) => {
                self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Refused);
                self.refusal = Some(KnotDocumentRefusalV1::ExternalChange);
                self.last_save_failure = None;
                Err(KnotDocumentIntentErrorV1::Refused(
                    KnotDocumentRefusalV1::ExternalChange,
                ))
            },
            Err(KnotEditorSaveError::Failed(message)) => self.record_save_failure(message),
        }
    }

    fn reload(&mut self) -> Result<(), KnotDocumentIntentErrorV1> {
        if self.write_posture == KnotDocumentWritePostureV1::ReadOnly {
            return self.refuse_read_only_write();
        }
        if self.editor.path().is_none() {
            self.refusal = Some(KnotDocumentRefusalV1::ScratchHasNoSaveTarget);
            self.last_save_failure = None;
            return Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ScratchHasNoSaveTarget,
            ));
        }
        match self.editor.reload() {
            Ok(()) => {
                self.last_save_outcome = None;
                self.refusal = None;
                self.last_save_failure = None;
                Ok(())
            },
            Err(message) => self.record_save_failure(message),
        }
    }

    fn record_save_outcome(&mut self, outcome: SaveOutcome) {
        self.last_save_outcome = Some(match outcome {
            SaveOutcome::Written => KnotDocumentSaveOutcomeV1::Written,
            SaveOutcome::Unchanged => KnotDocumentSaveOutcomeV1::Unchanged,
        });
        self.refusal = None;
        self.last_save_failure = None;
    }

    fn record_save_failure(&mut self, message: String) -> Result<(), KnotDocumentIntentErrorV1> {
        self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Failed);
        self.refusal = None;
        let failure = KnotDocumentSaveFailureV1 { message };
        self.last_save_failure = Some(failure.clone());
        Err(KnotDocumentIntentErrorV1::SaveFailed(failure))
    }

    fn refuse_read_only_write(&mut self) -> Result<(), KnotDocumentIntentErrorV1> {
        self.last_save_outcome = Some(KnotDocumentSaveOutcomeV1::Refused);
        self.refusal = Some(KnotDocumentRefusalV1::ReadOnly);
        self.last_save_failure = None;
        Err(KnotDocumentIntentErrorV1::Refused(
            KnotDocumentRefusalV1::ReadOnly,
        ))
    }

    fn refuse_read_only_edit(&mut self) {
        self.refusal = Some(KnotDocumentRefusalV1::ReadOnly);
        self.last_save_failure = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn file_session_round_trips_edit_save_drop_and_reopen() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        assert_eq!(session.snapshot().format, DocumentFormat::Djot);
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "body\n".into(),
            )))
            .unwrap();
        let saved = session.apply(KnotDocumentIntentV1::Save).unwrap();
        assert!(!saved.dirty);
        drop(session);
        assert_eq!(
            KnotDocumentSession::open(&path).unwrap().snapshot().text,
            "# Field\nbody\n"
        );
    }
    #[test]
    fn scratch_save_is_an_explicit_typed_refusal() {
        let mut session = KnotDocumentSession::scratch("memory:field", "");
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ScratchHasNoSaveTarget
            ))
        ));
    }

    #[test]
    fn compare_disk_is_unicode_exact_and_does_not_mutate_a_refusal_or_undo() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "α\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "local\n".into(),
            )))
            .unwrap();
        std::fs::write(&path, "外部\n").unwrap();
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ExternalChange
            ))
        ));
        let before = session.snapshot();

        let comparison = session.compare_disk().unwrap();
        assert_eq!(comparison.address, before.source.address);
        assert_eq!(comparison.buffer_text, "α\nlocal\n");
        assert_eq!(comparison.disk_text, "外部\n");
        assert!(comparison.disk_changed_since_baseline);
        assert_eq!(session.snapshot(), before);

        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(session.snapshot().text, "α\n");
    }

    #[test]
    fn compare_disk_reads_each_external_edit_freshly() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "initial\n").unwrap();
        let session = KnotDocumentSession::open(&path).unwrap();

        std::fs::write(&path, "first\n").unwrap();
        assert_eq!(session.compare_disk().unwrap().disk_text, "first\n");
        std::fs::write(&path, "second\n").unwrap();
        assert_eq!(session.compare_disk().unwrap().disk_text, "second\n");
    }

    #[test]
    fn compare_disk_reports_an_identity_only_replacement() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        let replacement = temp.path().join("replacement.djot");
        std::fs::write(&path, "same\n").unwrap();
        let session = KnotDocumentSession::open(&path).unwrap();
        std::fs::write(&replacement, "same\n").unwrap();
        std::fs::rename(&replacement, &path).unwrap();

        let comparison = session.compare_disk().unwrap();
        assert_eq!(comparison.buffer_text, "same\n");
        assert_eq!(comparison.disk_text, "same\n");
        assert!(comparison.disk_changed_since_baseline);
    }

    #[test]
    fn compare_disk_refuses_scratch_and_reports_missing_or_invalid_files_without_mutation() {
        assert!(
            KnotDocumentSession::scratch("memory:field", "draft")
                .compare_disk()
                .is_err()
        );

        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "valid\n").unwrap();
        let session = KnotDocumentSession::open(&path).unwrap();
        let before = session.snapshot();
        std::fs::remove_file(&path).unwrap();
        assert!(session.compare_disk().is_err());
        assert_eq!(session.snapshot(), before);

        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(session.compare_disk().is_err());
        assert_eq!(session.snapshot(), before);
    }

    #[test]
    fn outline_snapshot_uses_committed_unicode_source_and_selects_the_heading_span() {
        let mut session = KnotDocumentSession::scratch("memory:field", "# α\n\n## 二\n");
        let snapshot = session.outline_snapshot();
        assert_eq!(
            snapshot
                .items
                .iter()
                .map(|item| (item.label.as_str(), item.level))
                .collect::<Vec<_>>(),
            vec![("α", 1), ("二", 2)]
        );
        let second = &snapshot.items[1];
        assert_eq!(
            snapshot.source_text.get(second.start..second.end),
            Some("## 二\n")
        );

        session.select_outline_item(&snapshot, 1).unwrap();
        let item = &snapshot.items[1];
        assert_eq!(session.snapshot().selection.anchor.byte, item.start);
        assert_eq!(session.snapshot().selection.focus.byte, item.end);

        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::SetComposition {
                text: "仮".into(),
                selection: Some((0, 0)),
            }))
            .unwrap();
        assert_eq!(session.outline_snapshot(), snapshot);
    }

    #[test]
    fn outline_selection_rejects_stale_or_forged_ranges_without_changing_session_state() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# α\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        let stale = session.outline_snapshot();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "local\n".into(),
            )))
            .unwrap();
        let current = session.outline_snapshot();
        std::fs::write(&path, "external\n").unwrap();
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ExternalChange
            ))
        ));
        let before = session.snapshot();

        assert!(session.select_outline_item(&stale, 0).is_err());
        assert_eq!(session.snapshot(), before);
        assert!(session.select_outline_item(&current, 1).is_err());
        assert_eq!(session.snapshot(), before);

        let mut forged = current.clone();
        forged.items[0].end -= 1;
        assert!(session.select_outline_item(&forged, 0).is_err());
        assert_eq!(session.snapshot(), before);

        session.select_outline_item(&current, 0).unwrap();
        let after_selection = session.snapshot();
        assert_eq!(after_selection.text, before.text);
        assert_eq!(after_selection.dirty, before.dirty);
        assert_eq!(after_selection.refusal, before.refusal);
        assert_eq!(after_selection.last_save_outcome, before.last_save_outcome);

        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(session.snapshot().text, "# α\n");
    }

    #[test]
    fn read_only_session_allows_view_local_outline_selection() {
        let mut session = KnotDocumentSession::read_only("memory:field", "# Heading\n");
        let snapshot = session.outline_snapshot();
        let before = session.snapshot();

        session.select_outline_item(&snapshot, 0).unwrap();
        let after = session.snapshot();
        assert_eq!(after.text, before.text);
        assert_eq!(after.write_posture, KnotDocumentWritePostureV1::ReadOnly);
        assert_eq!(after.refusal, before.refusal);
        assert_ne!(after.selection, before.selection);
    }

    #[test]
    #[ignore = "manual A2 outline performance and parser-limit receipt"]
    fn outline_snapshot_large_unicode_and_atom_fixture_receipt() {
        use std::time::Instant;

        let mut source = String::new();
        for index in 0..2_000 {
            source.push_str(&format!("## α section {index}\n\n"));
            source.push_str("Unicode prose: 二 trees, élan, and a longer retained source line.\n");
            source.push_str("- first observation remains attached to this section\n");
            source.push_str("- second observation keeps the corpus representative\n\n");
            source.push_str(&format!("### 二 detail {index}\n\n"));
            source.push_str(
                "More prose makes source-range traversal exercise a substantial buffer.\n\n",
            );
        }
        let session = KnotDocumentSession::scratch("memory:large-outline", source);
        let started = Instant::now();
        let snapshot = session.outline_snapshot();
        println!(
            "outline_large source_bytes={} headings={} elapsed_us={}",
            snapshot.source_text.len(),
            snapshot.items.len(),
            started.elapsed().as_micros(),
        );
        assert_eq!(snapshot.items.len(), 4_000);
        assert_eq!(
            snapshot
                .source_text
                .get(snapshot.items[0].start..snapshot.items[0].end),
            Some("## α section 0\n")
        );
        assert_eq!(
            snapshot
                .source_text
                .get(snapshot.items[1].start..snapshot.items[1].end),
            Some("### 二 detail 0\n")
        );
        for item in &snapshot.items {
            assert!(snapshot.source_text.is_char_boundary(item.start));
            assert!(snapshot.source_text.is_char_boundary(item.end));
            let span = snapshot
                .source_text
                .get(item.start..item.end)
                .expect("outline span is UTF-8 bounded");
            assert!(span.starts_with('#'));
            assert!(span.ends_with('\n'));
            assert!(matches!(item.level, 2 | 3));
        }

        let fixture = KnotDocumentSession::scratch(
            "memory:outline-fixture",
            "## α plain\n\n## *emphasis* `code` $x^2$\n\n## soft\\\nwrapped\n\n## :symbol: “smart” … —\n\n## footnote[^one]\n\n[^one]: note\n\n## *unterminated\n",
        );
        let fixture_snapshot = fixture.outline_snapshot();
        for item in &fixture_snapshot.items {
            let span = fixture_snapshot
                .source_text
                .get(item.start..item.end)
                .expect("fixture span is UTF-8 bounded");
            println!(
                "outline_fixture label={:?} level={} range={}..{} span={:?}",
                item.label, item.level, item.start, item.end, span,
            );
        }
        assert!(fixture_snapshot.items.iter().all(|item| {
            fixture_snapshot
                .source_text
                .get(item.start..item.end)
                .is_some()
        }));
    }

    #[test]
    fn scratch_save_as_creates_a_new_djot_target_and_retains_undo_history() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        let mut session = KnotDocumentSession::scratch("memory:field", "# Field\n");
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "body\n".into(),
            )))
            .unwrap();

        let saved = session
            .apply(KnotDocumentIntentV1::SaveAs(path.clone()))
            .unwrap();
        assert_eq!(saved.source.kind, KnotDocumentSourceKindV1::File);
        assert_eq!(saved.write_posture, KnotDocumentWritePostureV1::FileTarget);
        assert!(!saved.dirty);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Field\nbody\n");

        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(session.snapshot().text, "# Field\n");
        assert!(session.snapshot().dirty);
    }

    #[test]
    fn long_valid_filename_supports_save_as_then_atomic_save() {
        let temp = tempdir().unwrap();
        let path = temp.path().join(format!("{}.djot", "a".repeat(230)));
        let mut session = KnotDocumentSession::scratch("memory:field", "one");

        session
            .apply(KnotDocumentIntentV1::SaveAs(path.clone()))
            .unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                " two".into(),
            )))
            .unwrap();
        session.apply(KnotDocumentIntentV1::Save).unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "one two");
    }

    #[test]
    fn save_as_does_not_clobber_an_existing_target_or_convert_scratch() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "external\n").unwrap();
        let mut session = KnotDocumentSession::scratch("memory:field", "local\n");

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::SaveAs(path.clone())),
            Err(KnotDocumentIntentErrorV1::SaveFailed(_))
        ));
        assert_eq!(
            session.snapshot().source.kind,
            KnotDocumentSourceKindV1::Scratch
        );
        assert_eq!(session.snapshot().text, "local\n");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "external\n");
    }

    #[test]
    fn external_edit_or_deletion_refuses_save_until_explicit_reload() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "local\n".into(),
            )))
            .unwrap();
        std::fs::write(&path, "external\n").unwrap();

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ExternalChange
            ))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external\n");
        assert!(session.snapshot().dirty);
        assert_eq!(
            session.snapshot().refusal,
            Some(KnotDocumentRefusalV1::ExternalChange)
        );

        let reloaded = session.apply(KnotDocumentIntentV1::Reload).unwrap();
        assert_eq!(reloaded.text, "external\n");
        assert!(!reloaded.dirty);
        assert_eq!(reloaded.refusal, None);

        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "local again\n".into(),
            )))
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ExternalChange
            ))
        ));
    }

    #[test]
    fn same_path_save_as_keeps_the_external_change_guard() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "local\n".into(),
            )))
            .unwrap();
        std::fs::write(&path, "external\n").unwrap();

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::SaveAs(path.clone())),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ExternalChange
            ))
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "external\n");
    }

    #[test]
    fn byte_identical_replacement_is_still_an_external_change() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        let replacement = temp.path().join("replacement.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "local\n".into(),
            )))
            .unwrap();
        std::fs::write(&replacement, "# Field\n").unwrap();
        std::fs::rename(&replacement, &path).unwrap();

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ExternalChange
            ))
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# Field\n");
    }

    #[test]
    fn read_only_file_refuses_edit_and_save_without_mutating_document_state() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut session = KnotDocumentSession::open_read_only(&path).unwrap();
        let before = session.snapshot();
        let comparison = session.compare_disk().unwrap();
        assert_eq!(comparison.disk_text, "# Field\n");
        assert!(!comparison.disk_changed_since_baseline);
        assert_eq!(session.snapshot(), before);

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "body\n".into()
            ))),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly
            ))
        ));
        let after_edit = session.snapshot();
        assert_eq!(after_edit.text, before.text);
        assert_eq!(after_edit.selection, before.selection);
        assert!(!after_edit.dirty);
        assert_eq!(
            after_edit.write_posture,
            KnotDocumentWritePostureV1::ReadOnly
        );
        assert_eq!(after_edit.refusal, Some(KnotDocumentRefusalV1::ReadOnly));

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly
            ))
        ));
        let after_save = session.snapshot();
        assert_eq!(after_save.text, before.text);
        assert!(!after_save.dirty);
        assert_eq!(
            after_save.last_save_outcome,
            Some(KnotDocumentSaveOutcomeV1::Refused)
        );
        assert!(matches!(
            session.input_mut(),
            Err(KnotDocumentRefusalV1::ReadOnly)
        ));
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::SaveAs(temp.path().join("copy.djot"))),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly
            ))
        ));
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Reload),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly
            ))
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# Field\n");
    }

    #[test]
    fn read_only_file_mode_opens_without_atomic_replace_authority() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let original = std::fs::metadata(&path).unwrap().permissions();
        let mut read_only = original.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&path, read_only).unwrap();

        let session = KnotDocumentSession::open(&path).unwrap();
        assert_eq!(
            session.snapshot().write_posture,
            KnotDocumentWritePostureV1::ReadOnly
        );

        std::fs::set_permissions(path, original).unwrap();
    }

    #[test]
    fn file_becoming_read_only_after_open_refuses_save_but_allows_save_as() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let original = std::fs::metadata(&path).unwrap().permissions();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        session.input_mut().unwrap().insert_str("draft");
        let mut read_only = original.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&path, read_only).unwrap();

        assert!(matches!(
            session.apply(KnotDocumentIntentV1::Save),
            Err(KnotDocumentIntentErrorV1::Refused(
                KnotDocumentRefusalV1::ReadOnly
            ))
        ));
        assert_eq!(
            session.snapshot().write_posture,
            KnotDocumentWritePostureV1::FileTarget
        );
        assert!(matches!(
            session.apply(KnotDocumentIntentV1::SaveAs(path.clone())),
            Err(KnotDocumentIntentErrorV1::SaveFailed(_))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Field\n");
        let draft = session.snapshot().text;
        let saved_elsewhere = temp.path().join("copy.djot");
        session
            .apply(KnotDocumentIntentV1::SaveAs(saved_elsewhere.clone()))
            .unwrap();
        assert_eq!(std::fs::read_to_string(saved_elsewhere).unwrap(), draft);

        std::fs::set_permissions(path, original).unwrap();
    }

    #[test]
    fn read_only_scratch_is_an_explicit_immutable_projection() {
        let session = KnotDocumentSession::read_only("memory:field", "# Field\n");
        let snapshot = session.snapshot();
        assert_eq!(snapshot.source.kind, KnotDocumentSourceKindV1::Scratch);
        assert_eq!(snapshot.write_posture, KnotDocumentWritePostureV1::ReadOnly);
        assert!(!snapshot.dirty);
    }
}
