// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Session-only playback for the shared relationship-projection dynamics
//! adapter. The retained recipe supplies the slot and occurrence roles.
//! Keep placement delegates authored edits to the recipe's history.
//! This module owns no source acquisition.

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaybackStatus {
    Ready,
    Running,
    Paused,
    Finished(seiche::SettleEnd),
    Refused,
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

    fn status(&self) -> PlaybackStatus {
        if self.refusal.is_some() {
            PlaybackStatus::Refused
        } else if self.running {
            PlaybackStatus::Running
        } else if let Some(end) = self.controller.as_ref().and_then(ProjectionDynamics::end) {
            PlaybackStatus::Finished(end)
        } else if self.controller.is_some() {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Ready
        }
    }

    pub(in crate::composition) fn status_key(&self) -> &'static str {
        match self.status() {
            PlaybackStatus::Ready => "ready",
            PlaybackStatus::Running => "running",
            PlaybackStatus::Paused => "paused",
            PlaybackStatus::Finished(seiche::SettleEnd::Rested) => "rested",
            PlaybackStatus::Finished(seiche::SettleEnd::LawFinished) => "finished",
            PlaybackStatus::Finished(seiche::SettleEnd::StepLimit) => "step-limit",
            PlaybackStatus::Refused => "refused",
        }
    }

    fn status_label(&self, enabled: bool) -> String {
        match self.status() {
            PlaybackStatus::Ready if !enabled => {
                "Choose a shared dynamics recipe to enable playback.".into()
            },
            PlaybackStatus::Ready => "Motion ready · playback is session-only".into(),
            PlaybackStatus::Running => "Motion running · fixed-step session preview".into(),
            PlaybackStatus::Paused => format!("Motion paused · {} fixed ticks", self.ticks),
            PlaybackStatus::Finished(seiche::SettleEnd::Rested) => {
                format!("Motion at rest · {} fixed ticks", self.ticks)
            },
            PlaybackStatus::Finished(seiche::SettleEnd::LawFinished) => {
                format!("Motion finished · {} fixed ticks", self.ticks)
            },
            PlaybackStatus::Finished(seiche::SettleEnd::StepLimit) => {
                format!("Step limit reached · {} fixed ticks", self.ticks)
            },
            PlaybackStatus::Refused => "Motion refused · preview stopped".into(),
        }
    }

    fn can_resume(&self) -> bool {
        matches!(
            self.status(),
            PlaybackStatus::Running | PlaybackStatus::Paused
        )
    }

    fn can_step(&self) -> bool {
        matches!(
            self.status(),
            PlaybackStatus::Ready | PlaybackStatus::Paused
        )
    }

    fn refuse(&mut self, error: String) {
        self.running = false;
        self.refusal = Some(error);
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
        if self.can_resume() {
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
        if !self.can_step() {
            // Finished and refused previews preserve their last valid pose.
            // Only explicit Play or Restart binds a fresh controller.
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
                self.refuse(error);
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

/// Explicitly promote a paused or finished pose through the recipe's history.
pub(super) fn keep_placement(state: &mut DesktopState) {
    state.composition.recipe.motion_preview_reveal = true;
    let recipe = &state.composition.recipe;
    if recipe.dynamics.running() {
        state.composition.notice = Some("Pause scene motion before keeping placement.".into());
        return;
    }
    if recipe.dynamics.positions().is_empty() {
        state.composition.notice = Some("Preview scene motion before keeping placement.".into());
        return;
    }
    let Some(mut candidate) = recipe.material.clone() else {
        return;
    };
    match candidate.capture_scene_positions(recipe.dynamics.positions()) {
        Ok(changed) => {
            if changed {
                super::install_edit(state, candidate);
            }
            state.composition.recipe.dynamics.reset();
            state.composition.notice = Some(
                if changed {
                    "Placement kept as one recipe edit; retain deliberately to save this version."
                } else {
                    "This placement is already authored."
                }
                .into(),
            );
        },
        Err(error) => state.composition.notice = Some(format!("Keep placement refused: {error}")),
    }
}

/// Session playback controls and the explicit authored-placement action.
pub(super) fn controls(state: &DesktopState) -> DesktopView {
    let running = state.composition.recipe.dynamics.running();
    let enabled = enabled(state);
    let card = state
        .composition
        .recipe
        .measured
        .as_ref()
        .map(|measured| measured.card);
    let (can_step, status_label, refusal) = {
        let runtime = &state.composition.recipe.dynamics;
        (
            runtime.can_step(),
            runtime.status_label(enabled),
            runtime.refusal().map(str::to_owned),
        )
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
                button("Play", move |state: &mut DesktopState, _| {
                    let Some(card) = card else {
                        return;
                    };
                    if state.composition.recipe.dynamics.running() {
                        return;
                    }
                    state.composition.recipe.motion_preview_reveal = true;
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
                        state.composition.recipe.dynamics.refuse(error.clone());
                        state.composition.notice = Some(error);
                    }
                })
                .attr("aria-label", start_label)
                .attr("class", "knot-recipe-dynamics-primary")
                .attr(
                    "aria-disabled",
                    (!enabled || running || card.is_none()).to_string(),
                ),
                button("Pause", |state: &mut DesktopState, _| {
                    state.composition.recipe.motion_preview_reveal = true;
                    state.composition.recipe.dynamics.pause();
                })
                .attr("aria-label", "Pause scene motion")
                .attr("class", "knot-recipe-dynamics-primary")
                .attr("aria-disabled", (!running).to_string()),
                button("Keep placement", |state: &mut DesktopState, _| {
                    keep_placement(state);
                })
                .attr("class", "knot-recipe-dynamics-keep")
                .attr(
                    "aria-disabled",
                    (running || state.composition.recipe.dynamics.positions().is_empty())
                        .to_string(),
                ),
                button("Step once", move |state: &mut DesktopState, _| {
                    let Some(card) = card else {
                        return;
                    };
                    if !state.composition.recipe.dynamics.can_step() {
                        return;
                    }
                    state.composition.recipe.motion_preview_reveal = true;
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
                        state.composition.recipe.dynamics.refuse(error.clone());
                        state.composition.notice = Some(error);
                    }
                })
                .attr("aria-label", "Step scene motion once")
                .attr(
                    "aria-disabled",
                    (!enabled || !can_step || card.is_none()).to_string(),
                ),
                button("Restart", move |state: &mut DesktopState, _| {
                    let Some(card) = card else {
                        return;
                    };
                    state.composition.recipe.motion_preview_reveal = true;
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
                        state.composition.recipe.dynamics.refuse(error.clone());
                        state.composition.notice = Some(error);
                    }
                })
                .attr("aria-label", "Restart scene motion")
                .attr("aria-disabled", (!enabled || card.is_none()).to_string()),
                span(status_label)
                    .attr("id", "knot-recipe-motion-status")
                    .attr("role", "status")
                    .attr("aria-live", "polite"),
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
mod tests;
