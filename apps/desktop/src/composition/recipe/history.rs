// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Session-only undo for authored relationship recipes and scene presentation.
//! The retained copied dataset and its source anchors stay with the current
//! material, rather than being duplicated into each history entry.

use edit_history::History;
use knot_composition::retention::{ProjectionRecipeMaterial, RecipeScenePresentation};
use scenograph::relationship::RelationshipSnapshot;

/// A bounded authoring history. The 64-entry cap keeps session memory
/// predictable while allowing a useful sequence of recipe and scene edits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecipeHistory {
    history: History<RecipeAuthored>,
    dataset: Option<scenomise::projection::RelationshipDataset>,
    anchors: Option<Vec<knot_composition::retention::RecipeSourceAnchor>>,
}

/// The fields that an undo step can restore. Copied input data and source
/// anchors are intentionally not part of an entry.
#[derive(Clone, Debug, PartialEq)]
struct RecipeAuthored {
    snapshot: RelationshipSnapshot,
    presentation: Option<RecipeScenePresentation>,
}

impl RecipeAuthored {
    fn from_material(material: &ProjectionRecipeMaterial) -> Self {
        Self {
            snapshot: material.snapshot.clone(),
            presentation: material.presentation.clone(),
        }
    }

    fn differs_authored(&self, other: &Self) -> bool {
        self.snapshot.recipe != other.snapshot.recipe || self.presentation != other.presentation
    }

    fn restore_into(self, mut current: ProjectionRecipeMaterial) -> ProjectionRecipeMaterial {
        // Occurrence and relationship selection are live scene state. Restore
        // only the recipe definition from the stored snapshot. A selected
        // relationship must still be disclosed by the restored category, as
        // in `relationship_category`; an occurrence selection remains valid
        // across category changes.
        current.snapshot.recipe = self.snapshot.recipe;
        if current
            .snapshot
            .selected_relationship
            .as_ref()
            .is_some_and(|id| {
                !current.dataset.relationships.iter().any(|relation| {
                    &relation.id == id
                        && current
                            .snapshot
                            .recipe
                            .relationship_kind
                            .as_ref()
                            .is_none_or(|kind| kind == &relation.kind)
                })
            })
        {
            current.snapshot.selected_relationship = None;
        }
        current.presentation = self.presentation;
        current
    }
}

impl Default for RecipeHistory {
    fn default() -> Self {
        Self {
            history: History::new().with_cap(64),
            dataset: None,
            anchors: None,
        }
    }
}

impl RecipeHistory {
    /// Start a fresh document history. Retained material is initially clean;
    /// unretained session material has no saved position and reads dirty.
    pub(crate) fn reset(&mut self, material: &ProjectionRecipeMaterial, retained: bool) {
        self.history.clear();
        if retained {
            self.history.mark_saved();
        } else {
            self.history.forget_saved();
        }
        self.dataset = Some(material.dataset.clone());
        self.anchors = Some(material.anchors.clone());
    }

    /// Record the pre-edit authored state as one discrete step. Selection-only
    /// changes are not authoring edits. A source rebind must reset the history
    /// first; mismatched source material is ignored defensively.
    pub(crate) fn record(
        &mut self,
        before: &ProjectionRecipeMaterial,
        after: &ProjectionRecipeMaterial,
    ) {
        if before.dataset != after.dataset || before.anchors != after.anchors {
            return;
        }
        if self.dataset.as_ref() != Some(&before.dataset)
            || self.anchors.as_ref() != Some(&before.anchors)
        {
            return;
        }

        let previous = RecipeAuthored::from_material(before);
        let next = RecipeAuthored::from_material(after);
        if previous.differs_authored(&next) {
            // No coalescing key is used. The supplied zero is not a clock; the
            // history has no time window and every call is a distinct step.
            self.history.record(previous, None, 0);
        }
    }

    /// Undo into the current source material, validating before consuming a
    /// history step. On refusal, both the current material and stacks survive.
    pub(crate) fn undo(
        &mut self,
        current: &ProjectionRecipeMaterial,
    ) -> Result<Option<ProjectionRecipeMaterial>, String> {
        self.step(current, Step::Undo)
    }

    /// Redo into the current source material, validating before consuming a
    /// history step. On refusal, both the current material and stacks survive.
    pub(crate) fn redo(
        &mut self,
        current: &ProjectionRecipeMaterial,
    ) -> Result<Option<ProjectionRecipeMaterial>, String> {
        self.step(current, Step::Redo)
    }

    fn step(
        &mut self,
        current: &ProjectionRecipeMaterial,
        step: Step,
    ) -> Result<Option<ProjectionRecipeMaterial>, String> {
        self.ensure_source(current)?;
        let mut candidate_history = self.history.clone();
        let restored = match step {
            Step::Undo => candidate_history.undo(RecipeAuthored::from_material(current)),
            Step::Redo => candidate_history.redo(RecipeAuthored::from_material(current)),
        };
        let Some(restored) = restored else {
            return Ok(None);
        };
        let candidate = restored.restore_into(current.clone());
        candidate.validate().map_err(|error| error.to_string())?;
        self.history = candidate_history;
        Ok(Some(candidate))
    }

    fn ensure_source(&self, current: &ProjectionRecipeMaterial) -> Result<(), String> {
        if self.dataset.as_ref() != Some(&current.dataset)
            || self.anchors.as_ref() != Some(&current.anchors)
        {
            return Err("Recipe history belongs to a different source material; reset it before undo or redo.".into());
        }
        Ok(())
    }

    /// Mark a save receipt only when it describes the exact current material.
    /// This prevents a delayed receipt for an older state from clearing dirt.
    pub(crate) fn mark_retained(
        &mut self,
        current: &ProjectionRecipeMaterial,
        retained: &ProjectionRecipeMaterial,
    ) {
        if current == retained {
            self.history.mark_saved();
        }
    }

    pub(crate) fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub(crate) fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }
}

#[derive(Clone, Copy)]
enum Step {
    Undo,
    Redo,
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_readings::sound::SoundLayers;

    fn material() -> ProjectionRecipeMaterial {
        let (source, reading) = super::super::tests::fixture();
        super::super::material(
            &source,
            &reading,
            &SoundLayers {
                perfect_rhyme: true,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn edited(material: &ProjectionRecipeMaterial, suffix: &str) -> ProjectionRecipeMaterial {
        let mut result = material.clone();
        result.snapshot.recipe.definition.label.push_str(suffix);
        result
    }

    fn mixed_category_material() -> (ProjectionRecipeMaterial, String, String) {
        let mut material = material();
        let original_kind = material.dataset.relationships[0].kind.clone();
        let alternate_kind = "test.alternate".to_owned();
        let mut alternate = material.dataset.relationships[0].clone();
        alternate.id = "alternate-category-relation".into();
        alternate.kind = alternate_kind.clone();
        material.dataset.relationships.push(alternate);
        (material, original_kind, alternate_kind)
    }

    #[test]
    fn selection_only_changes_do_not_create_history_steps() {
        let before = material();
        let mut after = before.clone();
        after.snapshot.selected_occurrence = Some("token-6-11".into());
        after.snapshot.selected_relationship = Some("sound-relation-0".into());

        let mut history = RecipeHistory::default();
        history.reset(&before, true);
        history.record(&before, &after);

        assert!(!history.can_undo());
        assert!(!history.is_dirty());
    }

    #[test]
    fn undo_and_redo_restore_authored_state_but_keep_current_data_and_anchors() {
        let before = material();
        let after = edited(&before, " revised");
        let mut history = RecipeHistory::default();
        history.reset(&before, true);
        history.record(&before, &after);

        let mut current = after.clone();
        current.snapshot.selected_occurrence = Some("token-6-11".into());
        let undone = history.undo(&current).unwrap().unwrap();
        assert_eq!(undone.snapshot.recipe, before.snapshot.recipe);
        assert_eq!(
            undone.snapshot.selected_occurrence,
            current.snapshot.selected_occurrence
        );
        assert_eq!(undone.dataset, current.dataset);
        assert_eq!(undone.anchors, current.anchors);
        assert!(history.can_redo());

        let redone = history.redo(&undone).unwrap().unwrap();
        assert_eq!(redone.snapshot.recipe, after.snapshot.recipe);
        assert_eq!(redone.dataset, current.dataset);
        assert_eq!(redone.anchors, current.anchors);
    }

    #[test]
    fn category_undo_clears_only_a_relationship_selection_excluded_by_restored_category() {
        let (mut before, original_kind, alternate_kind) = mixed_category_material();
        super::super::relationship_category(&mut before, Some(original_kind)).unwrap();
        let mut after = before.clone();
        super::super::relationship_category(&mut after, Some(alternate_kind)).unwrap();
        after.snapshot.selected_occurrence = Some("token-6-11".into());
        after.snapshot.selected_relationship = Some("alternate-category-relation".into());

        let mut history = RecipeHistory::default();
        history.reset(&before, true);
        history.record(&before, &after);

        let undone = history.undo(&after).unwrap().unwrap();
        assert_eq!(
            undone.snapshot.recipe.relationship_kind,
            before.snapshot.recipe.relationship_kind
        );
        assert_eq!(
            undone.snapshot.selected_occurrence,
            after.snapshot.selected_occurrence
        );
        assert!(undone.snapshot.selected_relationship.is_none());
        undone.validate().unwrap();
    }

    #[test]
    fn record_rejects_cross_source_changes_and_steps_reject_rebound_material() {
        let before = material();
        let after = edited(&before, " revised");
        let mut history = RecipeHistory::default();
        history.reset(&before, true);

        let mut rebound = after.clone();
        rebound.anchors.clear();
        history.record(&before, &rebound);
        assert!(!history.can_undo());
        assert!(history.undo(&rebound).is_err());
    }

    #[test]
    fn stale_retention_receipt_does_not_mark_newer_material_saved() {
        let before = material();
        let after = edited(&before, " revised");
        let mut history = RecipeHistory::default();
        history.reset(&before, false);
        history.record(&before, &after);
        assert!(history.is_dirty());

        history.mark_retained(&after, &before);
        assert!(history.is_dirty());

        history.mark_retained(&after, &after);
        assert!(!history.is_dirty());
    }

    #[test]
    fn invalid_undo_candidate_does_not_advance_history() {
        let before = material();
        let after = edited(&before, " revised");
        let mut history = RecipeHistory::default();
        history.reset(&before, true);
        history.record(&before, &after);

        // Keep the same input identity but make the current data invalid, so
        // the restored candidate fails validation as well.
        let mut invalid = after;
        invalid.dataset.dataset.occurrences.clear();
        history.reset(&invalid, true);
        history.record(&invalid, &edited(&invalid, " again"));
        assert!(history.undo(&invalid).is_err());
        assert!(history.can_undo());
    }
}
