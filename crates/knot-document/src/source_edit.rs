// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Host adapter: proposals do not own a buffer or grant write authority.
use crate::{KnotDocumentIntentV1, KnotDocumentSession, KnotDocumentWritePostureV1};
use cambium::{CaretSelection, TextCommand};
use mere_source_edit::{EditReceipt, Proposal, SourceSnapshot, SourceStamp, TextEdit};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub(crate) struct SourceVersion {
    instance: u64,
    revision: u64,
}
impl SourceVersion {
    pub(crate) fn new() -> Self {
        Self {
            instance: NEXT_INSTANCE
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .expect("document instance space exhausted"),
            revision: 0,
        }
    }
    pub(crate) fn advance(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("source revision exhausted");
    }
}

impl KnotDocumentSession {
    /// A session-local source version, never a durable identity or write grant.
    /// Mutable input escape hatches conservatively advance it before lending
    /// the input, including selection-only borrows. Undo/redo cannot revive it.
    pub fn source_stamp(&self) -> SourceStamp {
        SourceStamp {
            document: self.editor.address().to_owned(),
            instance: self.source_version.instance,
            revision: self.source_version.revision,
        }
    }

    /// Capture an immutable source witness, not another editor buffer.
    pub fn source_snapshot(&self) -> SourceSnapshot {
        SourceSnapshot::capture(self.source_stamp(), self.editor.source())
    }

    /// Read the captured selection only while its source and host selection
    /// still match. Shared identity/revision/byte checks remain in one place.
    pub fn selected_source<'a>(
        &self,
        base: &'a SourceSnapshot,
        selection: CaretSelection,
    ) -> Result<&'a str, String> {
        base.check(&self.source_stamp(), self.editor.source())
            .map_err(|error| error.to_string())?;
        let range = self.checked_selection_range(selection)?;
        Ok(&base.source()[range])
    }

    /// Preview a replacement against the original captured source, not a fresh
    /// base silently captured after a form was opened. No mutation or I/O.
    pub fn propose_selection_edit(
        &self,
        base: &SourceSnapshot,
        selection: CaretSelection,
        text: String,
    ) -> Result<Proposal, String> {
        self.selected_source(base, selection)?;
        let range = selection_range(selection);
        base.propose(vec![TextEdit {
            start: range.start,
            end: range.end,
            text,
        }])
        .map_err(|error| error.to_string())
    }

    /// Apply one captured-selection replacement through the existing input and
    /// undo journal. The receipt is returned only after application; no I/O.
    /// Multi-range batches need a host undo transaction and are refused here.
    pub fn apply_source_proposal(
        &mut self,
        proposal: &Proposal,
        selection: CaretSelection,
    ) -> Result<EditReceipt, String> {
        let range = self.checked_selection_range(selection)?;
        let [edit] = proposal.edits() else {
            return Err("this editor accepts one selection replacement".into());
        };
        if edit.start != range.start || edit.end != range.end {
            return Err("proposal range does not match the captured selection".into());
        }
        let application = proposal
            .application(&self.source_stamp(), self.editor.source())
            .map_err(|error| error.to_string())?;
        // Break typing coalescence on both sides, preserving the exact selection
        // and affinities in the ordinary one-step undo snapshot.
        self.editor.apply(TextCommand::SetSelection(selection));
        self.apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
            edit.text.clone(),
        )))
        .map_err(|error| format!("proposal edit refused: {error:?}"))?;
        let after = self.editor.selection();
        self.editor.apply(TextCommand::SetSelection(after));
        debug_assert_eq!(self.editor.source(), application.text);
        self.source_version.revision = application.receipt.result.revision;
        Ok(application.receipt)
    }

    fn checked_selection_range(
        &self,
        selection: CaretSelection,
    ) -> Result<std::ops::Range<usize>, String> {
        self.check_proposal_authority()?;
        if self.editor.selection() != selection {
            return Err("source selection changed".into());
        }
        let range = selection_range(selection);
        self.editor
            .source()
            .get(range.clone())
            .ok_or("the captured selection is invalid")?;
        Ok(range)
    }

    fn check_proposal_authority(&self) -> Result<(), String> {
        if self.snapshot().write_posture == KnotDocumentWritePostureV1::ReadOnly
            || self.editor.is_file_read_only()
        {
            return Err("this document is read-only".into());
        }
        if self.input().composition().is_some() {
            return Err("finish the current text composition before applying a proposal".into());
        }
        Ok(())
    }
}

fn selection_range(selection: CaretSelection) -> std::ops::Range<usize> {
    selection.anchor.byte.min(selection.focus.byte)..selection.anchor.byte.max(selection.focus.byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KnotDocumentIntentV1, KnotDocumentRefusalV1};
    use cambium::{CaretAffinity, CaretPosition};

    fn selection() -> CaretSelection {
        CaretSelection {
            anchor: CaretPosition {
                byte: 9,
                affinity: CaretAffinity::Upstream,
            },
            focus: CaretPosition {
                byte: 0,
                affinity: CaretAffinity::Downstream,
            },
        }
    }

    #[test]
    fn snapshot_selected_text_and_proposal_keep_the_original_base() {
        let mut session = KnotDocumentSession::scratch("memory:captured", "日本語 tail\n");
        session.editor.apply_layout_selection(selection());
        let base = session.source_snapshot();
        assert_eq!(
            session.selected_source(&base, selection()).unwrap(),
            "日本語"
        );
        let proposal = session
            .propose_selection_edit(&base, selection(), "link".into())
            .unwrap();
        assert_eq!(proposal.base(), base.stamp());
        assert_eq!(proposal.preview(), "link tail\n");
        assert_eq!(session.input().text(), base.source());
        for command in [TextCommand::Insert("later".into()), TextCommand::Undo] {
            session.apply(KnotDocumentIntentV1::Edit(command)).unwrap();
        }
        assert_eq!(session.input().text(), base.source());
        assert!(
            session
                .selected_source(&base, selection())
                .unwrap_err()
                .contains("stale")
        );
        assert!(
            session
                .propose_selection_edit(&base, selection(), "link".into())
                .unwrap_err()
                .contains("stale")
        );
        assert_eq!(session.input().text(), "日本語 tail\n");
    }

    #[test]
    fn snapshot_exact_source_check_refuses_forged_reused_stamp_without_mutation() {
        let mut session = KnotDocumentSession::scratch("memory:forged", "日本語 tail\n");
        session.editor.apply_layout_selection(selection());
        let base = SourceSnapshot::capture(session.source_stamp(), "別の文 tail\n");
        let before = session.snapshot();
        let stamp = session.source_stamp();
        assert!(
            session
                .selected_source(&base, selection())
                .unwrap_err()
                .contains("source changed")
        );
        assert!(
            session
                .propose_selection_edit(&base, selection(), "link".into())
                .unwrap_err()
                .contains("source changed")
        );
        assert_eq!(session.snapshot(), before);
        assert_eq!(session.source_stamp(), stamp);
    }

    #[test]
    fn preview_receipt_one_step_undo_selection_and_explicit_guarded_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.djot");
        let original = "日本語 tail\n";
        std::fs::write(&path, original).unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        session.editor.apply_layout_selection(selection());
        let stamp = session.source_stamp();
        let proposal = session
            .propose_selection_edit(
                &session.source_snapshot(),
                session.input().caret_selection(),
                "[日本語](./target.djot)".into(),
            )
            .unwrap();
        assert_eq!(proposal.preview(), "[日本語](./target.djot) tail\n");
        assert_eq!(session.source_stamp(), stamp);
        assert_eq!(session.input().text(), original);
        let receipt = session
            .apply_source_proposal(&proposal, selection())
            .unwrap();
        assert_eq!(receipt.base, stamp);
        assert_eq!(receipt.result, session.source_stamp());
        assert_eq!(receipt.ranges[0].base_end, 9);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(session.input().text(), original);
        assert_eq!(session.input().caret_selection(), selection());
        assert!(
            session
                .apply_source_proposal(&proposal, selection())
                .is_err()
        );
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Redo))
            .unwrap();
        let inserted = session.input().text().to_owned();
        std::fs::write(&path, "external author\n").unwrap();
        assert!(session.apply(KnotDocumentIntentV1::Save).is_err());
        assert_eq!(
            session.snapshot().refusal,
            Some(KnotDocumentRefusalV1::ExternalChange)
        );
        assert_eq!(session.input().text(), inserted);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external author\n");
    }

    #[test]
    fn proposals_refuse_wrong_instance_ime_selection_and_mutable_escape_aba() {
        let mut a = KnotDocumentSession::scratch("memory:same", "日本語 tail\n");
        let mut b = KnotDocumentSession::scratch("memory:same", "日本語 tail\n");
        a.editor.apply_layout_selection(selection());
        b.editor.apply_layout_selection(selection());
        let proposal = a
            .propose_selection_edit(
                &a.source_snapshot(),
                a.input().caret_selection(),
                "link".into(),
            )
            .unwrap();
        assert!(
            b.apply_source_proposal(&proposal, selection())
                .unwrap_err()
                .contains("instance")
        );
        a.apply(KnotDocumentIntentV1::Edit(TextCommand::SetComposition {
            text: "未".into(),
            selection: None,
        }))
        .unwrap();
        assert!(
            a.apply_source_proposal(&proposal, selection())
                .unwrap_err()
                .contains("composition")
        );
        assert_eq!(a.input().text(), "日本語 tail\n");
        a.apply(KnotDocumentIntentV1::Edit(TextCommand::CancelComposition))
            .unwrap();
        a.editor.apply_layout_selection(CaretSelection::default());
        assert!(
            a.apply_source_proposal(&proposal, selection())
                .unwrap_err()
                .contains("selection")
        );
        a.editor.apply_layout_selection(selection());
        {
            let input = a.input_mut().unwrap();
            input.apply(TextCommand::Insert("changed".into()));
            input.apply(TextCommand::Undo);
        }
        assert_eq!(a.input().text(), "日本語 tail\n");
        assert!(
            a.apply_source_proposal(&proposal, selection())
                .unwrap_err()
                .contains("stale")
        );
    }

    #[test]
    fn typing_and_following_typing_are_separate_from_proposal_undo() {
        let mut session = KnotDocumentSession::scratch("memory:typing", "");
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "before".into(),
            )))
            .unwrap();
        let selection = session.input().caret_selection();
        let proposal = session
            .propose_selection_edit(
                &session.source_snapshot(),
                session.input().caret_selection(),
                "[link](target.djot)".into(),
            )
            .unwrap();
        session.apply_source_proposal(&proposal, selection).unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "after".into(),
            )))
            .unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(session.input().text(), "before[link](target.djot)");
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(session.input().text(), "before");
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(session.input().text(), "");
    }

    #[test]
    fn selection_intents_do_not_advance_and_save_as_round_trip_invalidates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.djot");
        std::fs::write(&path, "source").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        let stamp = session.source_stamp();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
            .unwrap();
        assert_eq!(session.source_stamp(), stamp);
        let selection = session.input().caret_selection();
        let proposal = session
            .propose_selection_edit(
                &session.source_snapshot(),
                session.input().caret_selection(),
                "link".into(),
            )
            .unwrap();
        session
            .apply(KnotDocumentIntentV1::SaveAs(
                dir.path().join("elsewhere.djot"),
            ))
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        session.apply(KnotDocumentIntentV1::SaveAs(path)).unwrap();
        assert_eq!(session.source_stamp().document, stamp.document);
        assert!(
            session
                .apply_source_proposal(&proposal, selection)
                .unwrap_err()
                .contains("stale")
        );
    }

    #[test]
    fn successful_proposal_clears_edit_refusal_without_resetting_save_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.djot");
        std::fs::write(&path, "source").unwrap();
        let mut session = KnotDocumentSession::open(&path).unwrap();
        session
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                " draft".into(),
            )))
            .unwrap();
        std::fs::write(&path, "external").unwrap();
        assert!(session.apply(KnotDocumentIntentV1::Save).is_err());
        let before = session.snapshot();
        let selection = session.input().caret_selection();
        let proposal = session
            .propose_selection_edit(
                &session.source_snapshot(),
                session.input().caret_selection(),
                " link".into(),
            )
            .unwrap();
        session.apply_source_proposal(&proposal, selection).unwrap();
        assert_eq!(session.snapshot().refusal, None);
        assert_eq!(
            session.snapshot().last_save_outcome,
            before.last_save_outcome
        );
        assert_eq!(
            session.snapshot().last_save_failure,
            before.last_save_failure
        );
        assert!(session.apply(KnotDocumentIntentV1::Save).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
    }

    #[test]
    fn read_only_and_multi_range_refusal_leave_buffer_unchanged() {
        let mut session = KnotDocumentSession::read_only("memory:r", "source");
        assert!(
            session
                .propose_selection_edit(
                    &session.source_snapshot(),
                    session.input().caret_selection(),
                    "link".into()
                )
                .is_err()
        );
        let mut writable = KnotDocumentSession::scratch("memory:w", "source");
        let proposal = Proposal::prepare(
            writable.source_stamp(),
            "source",
            vec![
                TextEdit {
                    start: 0,
                    end: 1,
                    text: "a".into(),
                },
                TextEdit {
                    start: 2,
                    end: 3,
                    text: "b".into(),
                },
            ],
        )
        .unwrap();
        assert!(
            writable
                .apply_source_proposal(&proposal, writable.input().caret_selection())
                .is_err()
        );
        assert_eq!(writable.input().text(), "source");
        let proposal = Proposal::prepare(
            session.source_stamp(),
            "source",
            vec![TextEdit {
                start: 0,
                end: 6,
                text: "a".into(),
            }],
        )
        .unwrap();
        assert!(
            session
                .apply_source_proposal(&proposal, session.input().caret_selection())
                .is_err()
        );
        assert_eq!(session.input().text(), "source");
    }
}
