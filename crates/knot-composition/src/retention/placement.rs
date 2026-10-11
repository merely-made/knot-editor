//! A normalized placement record using the shared scene snapshot and trace.
//! Its canonical base depends on copied disclosure and anchors, never on card
//! measurement, arrangement, selection, camera, or reading emphasis.

use super::{ProjectionRecipeMaterial, RecipeScenePresentation};
use crate::CollectionError;
use sceno::{
    Footprint, InstanceId, ProjectedItem, Rect, Representation, Scene, Size2, Transform2, Vec2,
};
use scenotime::{Revision, SceneDiff, SceneEpoch, SceneOp, SceneSnapshot, SceneTrace, TraceStep};
use std::collections::{BTreeMap, BTreeSet};

const MAX_STEPS: usize = 256;
const MAX_LABEL_BYTES: usize = 128;

fn invalid(message: impl Into<String>) -> CollectionError {
    CollectionError::Invalid(message.into())
}

fn canonical(
    material: &ProjectionRecipeMaterial,
) -> Result<(SceneSnapshot, BTreeMap<String, InstanceId>), CollectionError> {
    let binding = serde_json::to_vec(&(&material.dataset, &material.anchors))
        .map_err(|error| invalid(error.to_string()))?;
    let digest = blake3::hash(&binding);
    let epoch = SceneEpoch(u64::from_le_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("eight digest bytes"),
    ));
    let mut scene = Scene::new();
    // The full digest makes exact base comparison protect the complete source
    // binding rather than relying only on the epoch's 64 bits.
    scene.spaces[0].name = Some(format!("knot.recipe.placement.v1:{}", digest.to_hex()));
    scene.bounds = Rect::new(Vec2::ZERO, Size2::new(1.0, 1.0));
    let mut occurrences: Vec<_> = material.dataset.dataset.occurrences.iter().collect();
    occurrences.sort_by(|a, b| a.occurrence_id.cmp(&b.occurrence_id));
    let mut mapping = BTreeMap::new();
    for occurrence in occurrences {
        let instance = InstanceId(scene.items.len() as u32);
        let source = scene.intern_source(occurrence.source.clone());
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Point,
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        mapping.insert(occurrence.occurrence_id.clone(), instance);
    }
    let base = SceneSnapshot::from_dense(epoch, Revision(0), scene)
        .map_err(|error| invalid(format!("invalid scene placement base: {error:?}")))?;
    Ok((base, mapping))
}

pub(super) fn validate(
    material: &ProjectionRecipeMaterial,
    trace: &SceneTrace,
) -> Result<(), CollectionError> {
    if trace.len() > MAX_STEPS {
        return Err(invalid("too many recipe scene placement steps"));
    }
    let (base, _) = canonical(material)?;
    if trace.base() != &base {
        return Err(invalid(
            "recipe scene placement belongs to different source material; reset scene presentation before rebinding",
        ));
    }
    for step in trace.steps() {
        if step.label.trim().is_empty()
            || step.label.len() > MAX_LABEL_BYTES
            || step.annotation.is_some()
        {
            return Err(invalid(
                "recipe scene placement step label or annotation is invalid",
            ));
        }
        let Some(diff) = &step.diff else {
            return Err(invalid(
                "recipe scene placement step requires a translation diff",
            ));
        };
        let [SceneOp::UpdateItem { index, value }] = diff.operations.as_slice() else {
            return Err(invalid(
                "recipe scene placement permits one occurrence translation per step",
            ));
        };
        let Some(original) = base.active_item(*index) else {
            return Err(invalid(
                "recipe scene placement occurrence is not disclosed",
            ));
        };
        let position = value.transform.translate;
        if !position.x.is_finite()
            || !position.y.is_finite()
            || !(0.0..=1.0).contains(&position.x)
            || !(0.0..=1.0).contains(&position.y)
        {
            return Err(invalid(
                "recipe scene placement must be finite and within normalized bounds",
            ));
        }
        let mut expected = original.clone();
        expected.transform.translate = position;
        if value != &expected {
            return Err(invalid(
                "recipe scene placement cannot alter source, appearance, strata, or geometry",
            ));
        }
    }
    Ok(())
}

fn positions(
    material: &ProjectionRecipeMaterial,
    trace: &SceneTrace,
) -> Result<BTreeMap<String, [f32; 2]>, CollectionError> {
    let (_, mapping) = canonical(material)?;
    let touched: BTreeSet<_> = trace
        .steps()
        .iter()
        .filter_map(|step| match step.diff.as_ref()?.operations.first()? {
            SceneOp::UpdateItem { index, .. } => Some(index.0),
            _ => None,
        })
        .collect();
    let head = trace.head();
    Ok(mapping
        .into_iter()
        .filter_map(|(id, index)| {
            if !touched.contains(&index.0) {
                return None;
            }
            let position = head.active_item(index)?.transform.translate;
            Some((id, [position.x, position.y]))
        })
        .collect())
}

impl ProjectionRecipeMaterial {
    /// Return only authored overrides. Untouched occurrences continue using
    /// the current shared compiler's measured placement.
    pub fn scene_positions(&self) -> Result<BTreeMap<String, [f32; 2]>, CollectionError> {
        self.validate()?;
        match self
            .presentation
            .as_ref()
            .and_then(|p| p.placement_trace.as_ref())
        {
            Some(trace) => positions(self, trace),
            None => Ok(BTreeMap::new()),
        }
    }

    /// Commit one placement atomically. Visibility is checked against current
    /// presentation and disclosed recipe relationships; hidden identities
    /// cannot acquire authoring authority through a stale event.
    pub fn move_scene_occurrence(
        &mut self,
        id: &str,
        position: [f32; 2],
    ) -> Result<bool, CollectionError> {
        self.validate()?;
        if position
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(invalid(
                "recipe scene placement must be finite and within normalized bounds",
            ));
        }
        let (base, mapping) = canonical(self)?;
        let index = *mapping
            .get(id)
            .ok_or_else(|| invalid("recipe scene placement occurrence is not disclosed"))?;
        let presentation = self.presentation.as_ref().cloned().unwrap_or_default();
        let related = self.dataset.relationships.iter().any(|relation| {
            self.snapshot
                .recipe
                .relationship_kind
                .as_ref()
                .is_none_or(|kind| kind == &relation.kind)
                && !presentation
                    .hidden_background_categories
                    .contains(&relation.kind)
                && (relation.from_occurrence == id || relation.to_occurrence == id)
        });
        let visible = presentation.overview_visible
            && (presentation.foreground_occurrences.contains(id)
                || (presentation.background_visible
                    && (presentation.hidden_background_categories.is_empty() || related)));
        if !visible {
            return Err(invalid("recipe scene placement occurrence is hidden"));
        }
        let trace = match presentation.placement_trace.as_ref() {
            Some(trace) => trace.clone(),
            None => SceneTrace::new(base)
                .map_err(|error| invalid(format!("invalid scene placement trace: {error:?}")))?,
        };
        if positions(self, &trace)?.get(id) == Some(&position) {
            return Ok(false);
        }
        if trace.len() >= MAX_STEPS {
            return Err(invalid("too many recipe scene placement steps"));
        }
        let head = trace.head();
        let mut value = head
            .active_item(index)
            .expect("canonical occurrence")
            .clone();
        value.transform.translate = Vec2::new(position[0], position[1]);
        let revision = head
            .revision
            .0
            .checked_add(1)
            .ok_or_else(|| invalid("scene placement revision overflow"))?;
        let next = trace
            .appended(TraceStep::diff(
                "Move occurrence",
                SceneDiff {
                    epoch: head.epoch,
                    base: head.revision,
                    revision: Revision(revision),
                    operations: vec![SceneOp::UpdateItem { index, value }],
                },
            ))
            .map_err(|error| invalid(format!("scene placement refused: {error:?}")))?;
        let mut candidate = self.clone();
        let presentation = candidate
            .presentation
            .get_or_insert_with(RecipeScenePresentation::default);
        presentation.placement_trace = Some(next);
        presentation.refresh_version();
        candidate.validate()?;
        *self = candidate;
        Ok(true)
    }
}
