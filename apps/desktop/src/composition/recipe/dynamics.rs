// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Session-only playback for the shared relationship-projection dynamics
//! adapter. The retained recipe supplies the slot and occurrence roles; this
//! module owns neither authored state nor source acquisition.

use super::*;
use cambium::{button, el, span};
use cambium_genet_winit_host::AppCtx;
use knot_composition::retention::ProjectionRecipeMaterial;
use pictograph::canvas::projection_dynamics::ProjectionDynamics;
use scenograph::DynamicsSlot;
use scenomise::projection::{CompiledRelationshipProjection, ProjectionCompiler};
use std::collections::BTreeMap;

/// An explicitly authored shared schedule, not a private rest-policy bypass.
pub(super) fn bounded_spring_slot() -> Result<DynamicsSlot, String> {
    use seiche::spec::{DynamicsSpec, Node, Stage, Stop};
    let spec = DynamicsSpec::new(Node::Schedule {
        stages: vec![Stage {
            node: Node::preset("spring.rapier"),
            stop: Stop::Frames(120),
            capture: None,
        }],
        weight: 1.0,
        overlays: Vec::new(),
    });
    let json = serde_json::to_string(&spec).map_err(|error| error.to_string())?;
    DynamicsSlot::from_json(scenograph::DYNAMICS_SLOT_VERSION, &json)
}

/// The compiled scene's original bounds are the fixed physics/display plane.
/// Dynamic output is always inverted against these bounds, never against the
/// controller's per-tick bounds (which track the moving bodies).
#[derive(Clone, Copy, Debug, PartialEq)]
struct FixedPlane {
    origin: [f32; 2],
    size: [f32; 2],
}

impl FixedPlane {
    fn from_scene(scene: &sceno::Scene) -> Result<Self, String> {
        let bounds = scene.bounds;
        let plane = Self {
            origin: [bounds.origin.x, bounds.origin.y],
            size: [bounds.size.w, bounds.size.h],
        };
        if plane
            .origin
            .iter()
            .chain(plane.size.iter())
            .any(|value| !value.is_finite())
            || plane.size.iter().any(|value| *value <= f32::EPSILON)
        {
            return Err("The compiled scene has no finite placement plane.".into());
        }
        Ok(plane)
    }

    fn scene_point(self, normalized: [f32; 2]) -> Result<[f32; 2], String> {
        if normalized
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err("An authored occurrence position is outside normalized bounds.".into());
        }
        Ok([
            self.origin[0] + normalized[0] * self.size[0],
            self.origin[1] + normalized[1] * self.size[1],
        ])
    }

    fn normalized(self, point: [f32; 2]) -> Result<[f32; 2], String> {
        if point.iter().any(|value| !value.is_finite()) {
            return Err("The dynamics law produced a non-finite occurrence position.".into());
        }
        // GraphCanvas uses normalized positions. Values that leave the fixed
        // plane pin to its boundary rather than changing the scale every tick.
        Ok([
            ((point[0] - self.origin[0]) / self.size[0]).clamp(0.0, 1.0),
            ((point[1] - self.origin[1]) / self.size[1]).clamp(0.0, 1.0),
        ])
    }
}

/// Runtime state is deliberately absent from recipe serialization and undo.
/// Its pose map overrides authored/default positions only for this session.
#[derive(Default)]
pub(in crate::composition) struct RuntimeState {
    controller: Option<ProjectionDynamics>,
    running: bool,
    positions: BTreeMap<String, [f32; 2]>,
    plane: Option<FixedPlane>,
    occurrence_instances: BTreeMap<String, sceno::InstanceId>,
    refusal: Option<String>,
    ticks: u64,
}

impl RuntimeState {
    pub(in crate::composition) fn running(&self) -> bool {
        self.running
    }

    /// Session-only positions, keyed by occurrence rather than source span.
    pub(in crate::composition) fn positions(&self) -> &BTreeMap<String, [f32; 2]> {
        &self.positions
    }

    pub(in crate::composition) fn ticks(&self) -> u64 {
        self.ticks
    }

    pub(super) fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    fn can_resume(&self) -> bool {
        self.controller
            .as_ref()
            .is_some_and(ProjectionDynamics::is_running)
    }

    /// Discard the controller and all transient poses; called when the recipe
    /// is replaced, edited, reopened, or history moves to another snapshot.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    fn pause(&mut self) {
        self.running = false;
    }

    fn start(
        &mut self,
        material: &ProjectionRecipeMaterial,
        card: sceno::Size2,
    ) -> Result<(), String> {
        if self.running {
            return Ok(());
        }
        if self
            .controller
            .as_ref()
            .is_some_and(ProjectionDynamics::is_running)
        {
            self.running = true;
            self.refusal = None;
            return Ok(());
        }
        self.reset();
        self.bind(material, card)?;
        self.running = true;
        self.refusal = None;
        Ok(())
    }

    fn step_once(
        &mut self,
        material: &ProjectionRecipeMaterial,
        card: sceno::Size2,
    ) -> Result<(), String> {
        if self.running {
            return Ok(());
        }
        if self
            .controller
            .as_ref()
            .is_some_and(|controller| !controller.is_running())
        {
            // A completed run stays at its final pose. Only an explicit
            // Start or Restart re-seeds a finished controller from authored positions.
            return Ok(());
        }
        if self.controller.is_none() {
            self.bind(material, card)?;
        }
        let _ = self.tick_once()?;
        self.running = false;
        Ok(())
    }

    fn restart(
        &mut self,
        material: &ProjectionRecipeMaterial,
        card: sceno::Size2,
    ) -> Result<(), String> {
        self.reset();
        self.bind(material, card)?;
        self.running = true;
        Ok(())
    }

    fn bind(
        &mut self,
        material: &ProjectionRecipeMaterial,
        card: sceno::Size2,
    ) -> Result<(), String> {
        let Some(slot) = material.snapshot.recipe.definition.dynamics.as_ref() else {
            return Err("Choose a shared dynamics recipe before starting motion.".into());
        };
        let compiled = ProjectionCompiler::new(ItemSizes { card })
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .map_err(|issues| format!("The relationship recipe cannot be realized: {issues:?}"))?;
        self.bind_compiled(material, compiled, slot)
    }

    fn bind_compiled(
        &mut self,
        material: &ProjectionRecipeMaterial,
        mut compiled: CompiledRelationshipProjection,
        slot: &DynamicsSlot,
    ) -> Result<(), String> {
        let plane = FixedPlane::from_scene(&compiled.projection.scene)?;
        let authored = material
            .scene_positions()
            .map_err(|error| error.to_string())?;
        apply_authored_positions(&mut compiled, plane, &authored)?;

        let roles = material
            .presentation
            .as_ref()
            .map(|presentation| &presentation.motion_roles)
            .cloned()
            .unwrap_or_default();
        let definition = material
            .snapshot
            .recipe
            .definition
            .bind(&material.snapshot.source_name, None)
            .map_err(|error| error.to_string())?;
        let controller = ProjectionDynamics::new_relationship(&definition, &compiled, slot, &roles)
            .map_err(|error| error.to_string())?;
        let occurrence_instances = compiled.projection.instance_by_occurrence.clone();
        let positions = positions_from_scene(controller.scene(), plane, &occurrence_instances)?;
        if positions.len() != occurrence_instances.len() {
            return Err("The dynamics adapter returned an incomplete occurrence scene.".into());
        }
        self.ticks = controller.ticks();
        self.controller = Some(controller);
        self.positions = positions;
        self.plane = Some(plane);
        self.occurrence_instances = occurrence_instances;
        self.refusal = None;
        Ok(())
    }

    fn tick_once(&mut self) -> Result<bool, String> {
        let Some(controller) = self.controller.as_mut() else {
            self.running = false;
            return Ok(false);
        };
        let running = controller.tick().map_err(|error| error.to_string())?;
        let plane = self.plane.ok_or_else(|| {
            "The active dynamics session has no fixed placement plane.".to_owned()
        })?;
        let next = positions_from_scene(controller.scene(), plane, &self.occurrence_instances)?;
        if next.len() != self.occurrence_instances.len() {
            return Err("The dynamics adapter returned an incomplete occurrence scene.".into());
        }
        self.positions = next;
        self.ticks = controller.ticks();
        self.running = running;
        Ok(running)
    }

    fn advance_frame(&mut self, presented: bool) -> bool {
        if !presented {
            // Hidden surfaces freeze in place and require an explicit Start to
            // resume; reopening a drawer cannot silently restart motion.
            self.pause();
            return false;
        }
        if !self.running {
            return false;
        }
        match self.tick_once() {
            Ok(running) => running,
            Err(error) => {
                self.running = false;
                self.refusal = Some(error);
                false
            },
        }
    }
}

/// Return whether the current retained recipe has an opt-in shared slot.
pub(super) fn enabled(state: &DesktopState) -> bool {
    state
        .composition
        .recipe
        .material
        .as_ref()
        .is_some_and(|material| material.snapshot.recipe.definition.dynamics.is_some())
}

/// Recipe controls. Durable preset and role edits are supplied by `scene`;
/// these actions only control the session-local shared runner.
pub(super) fn controls(state: &DesktopState) -> DesktopView {
    let running = state.composition.recipe.dynamics.running();
    let enabled = enabled(state);
    let card = state
        .composition
        .recipe
        .measured
        .as_ref()
        .map_or(VALIDATION_CARD, |measured| measured.card);
    let (ticks, refusal) = {
        let runtime = &state.composition.recipe.dynamics;
        (runtime.ticks(), runtime.refusal().map(str::to_owned))
    };
    let start_label = if state.composition.recipe.dynamics.can_resume() {
        "Resume scene motion"
    } else {
        "Start scene motion"
    };
    Box::new(
        el(
            "div",
            (
                button(start_label, move |state: &mut DesktopState, _| {
                    if state.composition.recipe.dynamics.running() {
                        return;
                    }
                    state.composition.recipe.scene_drag = None;
                    let result = state
                        .composition
                        .recipe
                        .material
                        .as_ref()
                        .ok_or_else(|| "No relationship recipe is open.".to_owned())
                        .and_then(|material| {
                            state.composition.recipe.dynamics.start(material, card)
                        });
                    if let Err(error) = result {
                        state.composition.recipe.dynamics.refusal = Some(error.clone());
                        state.composition.notice = Some(error);
                    }
                })
                .attr("aria-disabled", (!enabled || running).to_string()),
                button("Pause scene motion", |state: &mut DesktopState, _| {
                    state.composition.recipe.dynamics.pause();
                })
                .attr("aria-disabled", (!running).to_string()),
                button(
                    "Step scene motion once",
                    move |state: &mut DesktopState, _| {
                        if state.composition.recipe.dynamics.running() {
                            return;
                        }
                        state.composition.recipe.scene_drag = None;
                        let result = state
                            .composition
                            .recipe
                            .material
                            .as_ref()
                            .ok_or_else(|| "No relationship recipe is open.".to_owned())
                            .and_then(|material| {
                                state.composition.recipe.dynamics.step_once(material, card)
                            });
                        if let Err(error) = result {
                            state.composition.recipe.dynamics.running = false;
                            state.composition.recipe.dynamics.refusal = Some(error.clone());
                            state.composition.notice = Some(error);
                        }
                    },
                )
                .attr("aria-disabled", (!enabled || running).to_string()),
                button(
                    "Restart scene motion",
                    move |state: &mut DesktopState, _| {
                        state.composition.recipe.scene_drag = None;
                        let result = state
                            .composition
                            .recipe
                            .material
                            .as_ref()
                            .ok_or_else(|| "No relationship recipe is open.".to_owned())
                            .and_then(|material| {
                                state.composition.recipe.dynamics.restart(material, card)
                            });
                        if let Err(error) = result {
                            state.composition.recipe.dynamics.running = false;
                            state.composition.recipe.dynamics.refusal = Some(error.clone());
                            state.composition.notice = Some(error);
                        }
                    },
                )
                .attr("aria-disabled", (!enabled).to_string()),
                span(if running {
                    "Shared dynamics running · one fixed tick per presented frame".to_owned()
                } else if state.composition.recipe.dynamics.controller.is_some() {
                    format!("Shared dynamics paused · fixed ticks {ticks}")
                } else if enabled {
                    "Shared dynamics ready · playback is session-only".to_owned()
                } else {
                    "Choose a shared dynamics recipe to enable playback.".to_owned()
                }),
                refusal.map(|message| span(message).attr("class", "knot-recipe-dynamics-refusal")),
            ),
        )
        .attr("class", "knot-recipe-dynamics-controls"),
    )
}

/// Advance at most one shared fixed tick. The host calls this through
/// `update_deferred`; it does not rebuild the semantic tree or hit targets.
pub(super) fn frame(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
    presented: bool,
) -> bool {
    let was_running = ctx.runner.state().composition.recipe.dynamics.running();
    let demand = ctx
        .runner
        .update_deferred(|state| state.composition.recipe.dynamics.advance_frame(presented));
    if was_running && !demand {
        // A paused, settled or refused run re-enables authored canvas events.
        // Reconcile their semantic hit targets to the frozen leaf pose once;
        // the steady animation path remains repaint-only.
        ctx.runner.update(|_| {});
    }
    demand
}

fn apply_authored_positions(
    compiled: &mut CompiledRelationshipProjection,
    plane: FixedPlane,
    authored: &BTreeMap<String, [f32; 2]>,
) -> Result<(), String> {
    for (occurrence, normalized) in authored {
        let instance = compiled
            .projection
            .instance_by_occurrence
            .get(occurrence)
            .ok_or_else(|| "An authored position names an undisclosed occurrence.".to_owned())?;
        let item = compiled
            .projection
            .scene
            .items
            .get_mut(instance.0 as usize)
            .ok_or_else(|| "An authored occurrence has no compiled scene item.".to_owned())?;
        let center = plane.scene_point(*normalized)?;
        let footprint = item
            .footprint
            .bounds()
            .ok_or_else(|| "An occurrence has no measurable physics footprint.".to_owned())?;
        item.transform.translate.x = center[0] - footprint.origin.x - footprint.size.w * 0.5;
        item.transform.translate.y = center[1] - footprint.origin.y - footprint.size.h * 0.5;
    }
    Ok(())
}

fn positions_from_scene(
    scene: &sceno::Scene,
    plane: FixedPlane,
    occurrence_instances: &BTreeMap<String, sceno::InstanceId>,
) -> Result<BTreeMap<String, [f32; 2]>, String> {
    let mut positions = BTreeMap::new();
    for (occurrence, instance) in occurrence_instances {
        let item = scene
            .items
            .get(instance.0 as usize)
            .ok_or_else(|| "A compiled occurrence has no dynamics scene item.".to_owned())?;
        let footprint = item
            .footprint
            .bounds()
            .ok_or_else(|| "A dynamics occurrence has no measurable footprint.".to_owned())?;
        let center = [
            item.transform.translate.x + footprint.origin.x + footprint.size.w * 0.5,
            item.transform.translate.y + footprint.origin.y + footprint.size.h * 0.5,
        ];
        positions.insert(occurrence.clone(), plane.normalized(center)?);
    }
    Ok(positions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenomise::projection::{ProjectionOccurrence, ProjectionValue};

    fn fixture_material() -> ProjectionRecipeMaterial {
        let (source, reading) = super::super::tests::fixture();
        super::super::material(
            &source,
            &reading,
            &knot_readings::sound::SoundLayers {
                perfect_rhyme: true,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn with_spring_slot() -> ProjectionRecipeMaterial {
        let mut material = fixture_material();
        material.snapshot.recipe.definition.dynamics =
            Some(pictograph::canvas::dynamics_recipe::preset_slot("spring.rapier").unwrap());
        material
    }

    #[test]
    fn fixed_plane_inversion_does_not_follow_moving_scene_bounds() {
        let plane = FixedPlane {
            origin: [-10.0, 20.0],
            size: [100.0, 80.0],
        };
        assert_eq!(plane.scene_point([0.25, 0.75]).unwrap(), [15.0, 80.0]);
        assert_eq!(plane.normalized([15.0, 80.0]).unwrap(), [0.25, 0.75]);
        assert_eq!(plane.normalized([-50.0, 200.0]).unwrap(), [0.0, 1.0]);
    }

    #[test]
    fn authored_positions_reject_nonfinite_or_out_of_plane_values() {
        let plane = FixedPlane {
            origin: [0.0, 0.0],
            size: [10.0, 10.0],
        };
        assert!(plane.scene_point([f32::NAN, 0.5]).is_err());
        assert!(plane.scene_point([1.1, 0.5]).is_err());
    }

    #[test]
    fn cold_runtime_does_not_advance_and_hidden_motion_stays_paused() {
        let mut runtime = RuntimeState::default();
        assert!(!runtime.running());
        assert_eq!(runtime.ticks(), 0);
        assert!(runtime.positions().is_empty());
        assert!(!runtime.advance_frame(true));

        runtime.running = true;
        runtime.positions.insert("occurrence-a".into(), [0.4, 0.6]);
        assert!(!runtime.advance_frame(false));
        assert!(!runtime.running());
        assert_eq!(runtime.positions()["occurrence-a"], [0.4, 0.6]);
        assert!(!runtime.advance_frame(true));
        assert_eq!(runtime.ticks(), 0);
    }

    #[test]
    fn frame_advances_one_fixed_tick_without_mutating_retained_material() {
        let material = with_spring_slot();
        let original = material.clone();
        let mut runtime = RuntimeState::default();
        runtime.start(&material, VALIDATION_CARD).unwrap();
        assert_eq!(runtime.ticks(), 0);

        assert!(runtime.advance_frame(true));
        assert_eq!(runtime.ticks(), 1);
        assert_eq!(material, original);

        runtime.pause();
        assert!(!runtime.advance_frame(true));
        assert_eq!(runtime.ticks(), 1);
    }

    #[test]
    fn single_step_advances_exactly_once_and_never_starts_continuous_playback() {
        let material = with_spring_slot();
        let mut runtime = RuntimeState::default();
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
        assert_eq!(runtime.ticks(), 1);
        assert!(!runtime.running());
        assert!(!runtime.advance_frame(true));
        assert_eq!(runtime.ticks(), 1);

        // A stale callback while already running is harmless: it neither
        // converts the playback to a one-step pause nor adds an extra tick.
        runtime.start(&material, VALIDATION_CARD).unwrap();
        assert!(runtime.running());
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
        assert!(runtime.running());
        assert_eq!(runtime.ticks(), 1);
    }

    #[test]
    fn hidden_frame_freezes_real_poses_and_reopening_does_not_resume() {
        let material = with_spring_slot();
        let mut runtime = RuntimeState::default();
        runtime.start(&material, VALIDATION_CARD).unwrap();
        assert!(runtime.advance_frame(true));
        let poses = runtime.positions().clone();
        let ticks = runtime.ticks();

        assert!(!runtime.advance_frame(false));
        assert!(!runtime.running());
        assert_eq!(runtime.positions(), &poses);
        assert!(!runtime.advance_frame(true));
        assert_eq!(runtime.ticks(), ticks);
        assert_eq!(runtime.positions(), &poses);
    }

    #[test]
    fn unknown_shared_channel_refuses_atomically_without_partial_controller() {
        let mut material = with_spring_slot();
        let mut spec: serde_json::Value = serde_json::from_str(
            &material
                .snapshot
                .recipe
                .definition
                .dynamics
                .as_ref()
                .unwrap()
                .spec,
        )
        .unwrap();
        spec["channels"] = serde_json::json!({"mass":"mass.pagerank"});
        material.snapshot.recipe.definition.dynamics =
            Some(DynamicsSlot::from_json(1, &spec.to_string()).unwrap());
        let original = material.clone();
        let mut runtime = RuntimeState::default();

        assert!(runtime.start(&material, VALIDATION_CARD).is_err());
        assert!(runtime.controller.is_none());
        assert!(runtime.positions().is_empty());
        assert_eq!(runtime.ticks(), 0);
        assert_eq!(material, original);
    }

    #[test]
    fn authored_pinned_center_is_preserved_while_another_occurrence_moves() {
        let mut material = with_spring_slot();
        let pinned = material.dataset.dataset.occurrences[0]
            .occurrence_id
            .clone();
        let moving = material.dataset.dataset.occurrences[2]
            .occurrence_id
            .clone();
        let target = [0.05, 0.2];
        let mut presentation = knot_composition::retention::RecipeScenePresentation {
            overview_visible: true,
            foreground_occurrences: BTreeMap::from([(pinned.clone(), ()), (moving.clone(), ())])
                .into_keys()
                .collect(),
            ..Default::default()
        };
        material.presentation = Some(presentation.clone());
        material.move_scene_occurrence(&pinned, target).unwrap();
        presentation = material.presentation.take().unwrap();
        presentation
            .motion_roles
            .insert(pinned.clone(), seiche::Role::Pinned);
        presentation.refresh_version();
        material.presentation = Some(presentation);

        let mut runtime = RuntimeState::default();
        runtime.start(&material, VALIDATION_CARD).unwrap();
        let initial = runtime.positions().clone();
        assert!((initial[&pinned][0] - target[0]).abs() < 1e-5);
        assert!((initial[&pinned][1] - target[1]).abs() < 1e-5);

        runtime.pause();
        for _ in 0..12 {
            runtime.step_once(&material, VALIDATION_CARD).unwrap();
        }
        assert_eq!(runtime.ticks(), 12);
        let final_positions = runtime.positions();
        assert!((final_positions[&pinned][0] - target[0]).abs() < 1e-5);
        assert!((final_positions[&pinned][1] - target[1]).abs() < 1e-5);
        assert_ne!(final_positions[&moving], initial[&moving]);
    }

    #[test]
    fn stepping_a_completed_crowded_pinned_run_preserves_its_final_pose() {
        let mut material = with_spring_slot();
        let pinned = material.dataset.dataset.occurrences[0]
            .occurrence_id
            .clone();
        let neighbor = material.dataset.dataset.occurrences[2]
            .occurrence_id
            .clone();
        let target = [0.72, 0.34];
        let mut presentation = knot_composition::retention::RecipeScenePresentation {
            overview_visible: true,
            foreground_occurrences: [pinned.clone(), neighbor.clone()].into(),
            ..Default::default()
        };
        material.presentation = Some(presentation.clone());
        material.move_scene_occurrence(&pinned, target).unwrap();
        presentation = material.presentation.take().unwrap();
        presentation
            .motion_roles
            .insert(pinned.clone(), seiche::Role::Pinned);
        presentation.refresh_version();
        material.presentation = Some(presentation);

        let mut runtime = RuntimeState::default();
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
        for _ in 0..128 {
            if runtime
                .controller
                .as_ref()
                .is_some_and(|controller| !controller.is_running())
            {
                break;
            }
            runtime.step_once(&material, VALIDATION_CARD).unwrap();
        }
        assert!(
            runtime
                .controller
                .as_ref()
                .is_some_and(|controller| !controller.is_running())
        );

        let final_ticks = runtime.ticks();
        let final_positions = runtime.positions().clone();
        for _ in 0..3 {
            runtime.step_once(&material, VALIDATION_CARD).unwrap();
            assert_eq!(runtime.ticks(), final_ticks);
            assert_eq!(runtime.positions(), &final_positions);
        }
    }

    #[test]
    fn bounded_preview_uses_the_shared_frame_schedule_and_preserves_its_slot() {
        let mut material = with_spring_slot();
        let slot = bounded_spring_slot().unwrap();
        material.snapshot.recipe.definition.dynamics = Some(slot.clone());
        let original = material.clone();
        let mut runtime = RuntimeState::default();
        for tick in 1..=120 {
            runtime.step_once(&material, VALIDATION_CARD).unwrap();
            assert_eq!(runtime.ticks(), tick);
            assert_eq!(runtime.can_resume(), tick < 120);
        }
        let positions = runtime.positions().clone();
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
        assert_eq!(runtime.ticks(), 120);
        assert_eq!(runtime.positions(), &positions);
        assert_eq!(material, original);
        assert_eq!(
            material.snapshot.recipe.definition.dynamics.as_ref(),
            Some(&slot)
        );
    }

    #[test]
    fn shared_slot_and_roles_roundtrip_as_durable_material_not_runtime_state() {
        let mut material = with_spring_slot();
        let occurrence = material.dataset.dataset.occurrences[0]
            .occurrence_id
            .clone();
        let presentation = material.presentation.get_or_insert_with(Default::default);
        presentation
            .motion_roles
            .insert(occurrence.clone(), seiche::Role::Pinned);
        presentation.refresh_version();
        let runtime = RuntimeState::default();
        let json = serde_json::to_vec(&material).unwrap();
        let decoded: ProjectionRecipeMaterial = serde_json::from_slice(&json).unwrap();

        assert_eq!(decoded, material);
        assert_eq!(
            decoded.presentation.as_ref().unwrap().motion_roles[&occurrence],
            seiche::Role::Pinned
        );
        assert!(runtime.positions().is_empty());
        assert_eq!(runtime.ticks(), 0);
    }

    #[test]
    fn maximum_occurrence_scene_is_deterministic_and_ticks_only_when_presented() {
        let mut material = with_spring_slot();
        let template = material.dataset.dataset.occurrences[0].clone();
        let anchor = material.anchors[0].clone();
        while material.dataset.dataset.occurrences.len() < 256 {
            let index = material.dataset.dataset.occurrences.len();
            let id = format!("fixture-occurrence-{index}");
            let mut occurrence: ProjectionOccurrence = template.clone();
            occurrence.occurrence_id = id.clone();
            occurrence
                .values
                .insert("occurrence_id".into(), ProjectionValue::Text(id.clone()));
            occurrence
                .values
                .insert("label".into(), ProjectionValue::Text(id.clone()));
            occurrence
                .values
                .insert("order".into(), ProjectionValue::Number(index as f64));
            material.dataset.dataset.occurrences.push(occurrence);
            let mut source_anchor = anchor.clone();
            source_anchor.node_id = id;
            material.anchors.push(source_anchor);
        }
        material.validate().unwrap();
        assert_eq!(material.dataset.dataset.occurrences.len(), 256);

        let mut first = RuntimeState::default();
        let mut second = RuntimeState::default();
        first.start(&material, VALIDATION_CARD).unwrap();
        second.start(&material, VALIDATION_CARD).unwrap();
        assert_eq!(first.positions().len(), 256);
        assert_eq!(first.positions(), second.positions());
        assert!(!first.advance_frame(false));
        assert_eq!(first.ticks(), 0);
        assert!(!first.advance_frame(true));
        first.start(&material, VALIDATION_CARD).unwrap();
        assert!(first.advance_frame(true));
        assert!(second.advance_frame(true));
        assert_eq!(first.ticks(), 1);
        assert_eq!(second.ticks(), 1);
        assert_eq!(first.positions(), second.positions());
    }
}
