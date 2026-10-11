// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's compact, editable presentation of a compiled relationship recipe.
//! Scene truth stays in the copied recipe. Authored occurrence placements are
//! portable scene traces; visibility, emphasis and camera remain host preferences.

use super::*;
use cambium::{
    GraphCanvasEdge, GraphCanvasEvent, GraphCanvasNode, GraphCanvasRelation, GraphCanvasSubgraph,
    GraphCanvasSwatch, PointerPhase, button, el, graph_canvas, span,
};
use cambium_genet_winit_host::AppCtx;
use knot_composition::retention::RecipeScenePresentation;
use sprigging::{ColorF, GraphViewport};
use std::collections::{BTreeMap, BTreeSet};

const SCENE_LEAF_KEY: u64 = 0x4b4e_4f54_5343_454e;
// The compact drawer leaves about 232 logical pixels after its padding.
// Keep the declared viewport inside it: CSS-clamping a wider shared canvas
// would crop its node targets while camera projection still used that width.
const SCENE_WIDTH: u32 = 220;
const SCENE_HEIGHT: u32 = 200;
const CAMERA_STEP: f32 = 0.1;
const PLACEMENT_STEP: f32 = 0.05;

pub(crate) struct SceneDrag {
    id: String,
    before: ProjectionRecipeMaterial,
    preview: Option<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SceneLayer {
    Background,
    Foreground,
}

#[derive(Clone, Debug, PartialEq)]
enum SceneAction {
    Overview(bool),
    Background(bool),
    BackgroundCategory { kind: String, visible: bool },
    ShowAllBackgroundCategories,
    PromoteSelected,
    DemoteSelected,
    Pan(f32, f32),
    Zoom(f32),
    Fit,
    Reset,
    MoveOccurrence { id: String, position: (f32, f32) },
    SelectOccurrence(String),
    SelectRelationship(String),
    MotionPreset(Option<String>),
    MotionBoundedSpring,
    MotionRole(seiche::Role),
    ClearMotionRoles,
}

fn color_for_layer(layer: &SceneLayer) -> ColorF {
    match layer {
        SceneLayer::Background => ColorF {
            r: 0.28,
            g: 0.42,
            b: 0.58,
            a: 0.9,
        },
        SceneLayer::Foreground => ColorF {
            r: 0.86,
            g: 0.48,
            b: 0.2,
            a: 1.0,
        },
    }
}

fn normalized(value: f32, origin: f32, extent: f32) -> f32 {
    if !value.is_finite() || !origin.is_finite() || !extent.is_finite() || extent <= f32::EPSILON {
        return 0.5;
    }
    ((value - origin) / extent).clamp(0.0, 1.0)
}

#[cfg(test)]
fn swatch(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
) -> GraphCanvasSwatch<String, SceneLayer> {
    swatch_with_preview(
        material,
        compiled,
        &material.scene_positions().unwrap(),
        None,
    )
}

fn swatch_with_preview(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
    authored: &BTreeMap<String, [f32; 2]>,
    drag: Option<&SceneDrag>,
) -> GraphCanvasSwatch<String, SceneLayer> {
    let presentation = material
        .presentation
        .clone()
        .unwrap_or_else(RecipeScenePresentation::default);
    let scene = &compiled.projection.scene;
    let visible_occurrences = visible_occurrence_ids(material, compiled);
    let mut positions = BTreeMap::new();
    let mut nodes = Vec::with_capacity(material.dataset.dataset.occurrences.len());

    // Occurrence identity, not source identity, keys every node. Two readings
    // of the same document span therefore remain two distinct scene items.
    for occurrence in &material.dataset.dataset.occurrences {
        let Some(instance) = compiled
            .projection
            .instance_by_occurrence
            .get(&occurrence.occurrence_id)
        else {
            continue;
        };
        let Some(item) = scene.items.get(instance.0 as usize) else {
            continue;
        };
        let foot = item.footprint.bounds().unwrap_or_default();
        let x = item.transform.translate.x + foot.origin.x + foot.size.w / 2.0;
        let y = item.transform.translate.y + foot.origin.y + foot.size.h / 2.0;
        let default_position = (
            normalized(x, scene.bounds.origin.x, scene.bounds.size.w),
            normalized(y, scene.bounds.origin.y, scene.bounds.size.h),
        );
        let position = drag
            .filter(|drag| drag.before == *material && drag.id == occurrence.occurrence_id)
            .and_then(|drag| drag.preview)
            .or_else(|| {
                authored
                    .get(&occurrence.occurrence_id)
                    .map(|p| (p[0], p[1]))
            })
            .unwrap_or(default_position);
        positions.insert(occurrence.occurrence_id.clone(), position);
        if !visible_occurrences.contains(&occurrence.occurrence_id) {
            continue;
        }
        nodes.push(GraphCanvasNode {
            id: occurrence.occurrence_id.clone(),
            kind: if presentation
                .foreground_occurrences
                .contains(&occurrence.occurrence_id)
            {
                SceneLayer::Foreground
            } else {
                SceneLayer::Background
            },
            position,
            label: format!(
                "{} · {}",
                compiled
                    .projection
                    .labels
                    .get(instance)
                    .cloned()
                    .unwrap_or_else(|| occurrence.occurrence_id.clone()),
                occurrence.occurrence_id
            ),
            key: Some(occurrence.occurrence_id.clone()),
        });
    }

    let relations = compiled
        .relationships
        .iter()
        .filter_map(|relation| {
            let disclosure = &relation.disclosure;
            let from = positions.get(&disclosure.from_occurrence)?;
            let to = positions.get(&disclosure.to_occurrence)?;
            let foreground_relation = presentation
                .foreground_occurrences
                .contains(&disclosure.from_occurrence)
                && presentation
                    .foreground_occurrences
                    .contains(&disclosure.to_occurrence);
            let visible = foreground_relation
                || (presentation.background_visible
                    && !presentation
                        .hidden_background_categories
                        .contains(&disclosure.kind)
                    && visible_occurrences.contains(&disclosure.from_occurrence)
                    && visible_occurrences.contains(&disclosure.to_occurrence));
            visible.then(|| GraphCanvasRelation {
                id: disclosure.id.clone(),
                from: disclosure.from_occurrence.clone(),
                to: disclosure.to_occurrence.clone(),
                kind: disclosure.kind.clone(),
                label: disclosure.label.clone(),
                route: vec![*from, *to],
                visible,
                emphasized: material.snapshot.selected_relationship.as_ref()
                    == Some(&disclosure.id),
            })
        })
        .collect();

    let graph = GraphCanvasSubgraph {
        nodes,
        edges: Vec::<GraphCanvasEdge<String>>::new(),
    };
    let mut swatch = GraphCanvasSwatch::new(SCENE_LEAF_KEY, graph)
        .with_relations(relations)
        .with_size(SCENE_WIDTH, SCENE_HEIGHT)
        .with_label("Authored relationship scene")
        .with_expand(false)
        .with_node_labels(true)
        .with_deferred_drag_rebuild(true);
    swatch.selected = material.snapshot.selected_occurrence.clone();
    swatch.viewport = GraphViewport {
        pan: (presentation.pan_x, presentation.pan_y),
        zoom: presentation.zoom,
    };
    swatch
}

fn visible_occurrence_ids(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
) -> BTreeSet<String> {
    let presentation = material.presentation.as_ref().cloned().unwrap_or_default();
    let mut related_background = BTreeSet::new();
    if !presentation.hidden_background_categories.is_empty() {
        for relation in &compiled.relationships {
            let disclosure = &relation.disclosure;
            if !presentation
                .hidden_background_categories
                .contains(&disclosure.kind)
            {
                related_background.insert(disclosure.from_occurrence.clone());
                related_background.insert(disclosure.to_occurrence.clone());
            }
        }
    }
    material
        .dataset
        .dataset
        .occurrences
        .iter()
        .filter_map(|occurrence| {
            let id = &occurrence.occurrence_id;
            (presentation.foreground_occurrences.contains(id)
                || (presentation.background_visible
                    && (presentation.hidden_background_categories.is_empty()
                        || related_background.contains(id))))
            .then(|| id.clone())
        })
        .collect()
}

fn apply_action(
    material: &mut ProjectionRecipeMaterial,
    action: SceneAction,
) -> Result<bool, String> {
    let mut candidate = material.clone();
    match action {
        SceneAction::SelectOccurrence(id) => {
            candidate.snapshot.selected_occurrence = Some(id);
            candidate.snapshot.selected_relationship = None;
        },
        SceneAction::SelectRelationship(id) => {
            let relation = candidate
                .dataset
                .relationships
                .iter()
                .find(|relation| {
                    relation.id == id
                        && candidate
                            .snapshot
                            .recipe
                            .relationship_kind
                            .as_ref()
                            .is_none_or(|kind| kind == &relation.kind)
                })
                .ok_or_else(|| "The selected relationship is no longer disclosed.".to_owned())?;
            candidate.snapshot.selected_relationship = Some(id);
            candidate.snapshot.selected_occurrence = Some(relation.from_occurrence.clone());
        },
        SceneAction::Overview(visible) => {
            let first_enable = candidate.presentation.is_none();
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            if visible && first_enable && presentation.foreground_occurrences.is_empty() {
                let initial = candidate
                    .snapshot
                    .selected_occurrence
                    .as_ref()
                    .filter(|id| {
                        candidate
                            .dataset
                            .dataset
                            .occurrences
                            .iter()
                            .any(|occurrence| &occurrence.occurrence_id == *id)
                    })
                    .cloned()
                    .or_else(|| {
                        candidate
                            .dataset
                            .dataset
                            .occurrences
                            .first()
                            .map(|occurrence| occurrence.occurrence_id.clone())
                    });
                if let Some(initial) = initial {
                    presentation.foreground_occurrences.insert(initial);
                }
            }
            presentation.overview_visible = visible;
        },
        SceneAction::Background(visible) => {
            candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default)
                .background_visible = visible;
        },
        SceneAction::BackgroundCategory { kind, visible } => {
            if !candidate
                .dataset
                .relationships
                .iter()
                .any(|relation| relation.kind == kind)
            {
                return Err("The background category is not disclosed by this recipe.".into());
            }
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            if visible {
                presentation.hidden_background_categories.remove(&kind);
            } else {
                presentation.hidden_background_categories.insert(kind);
            }
            presentation.refresh_version();
        },
        SceneAction::ShowAllBackgroundCategories => {
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            presentation.hidden_background_categories.clear();
            presentation.refresh_version();
        },
        SceneAction::PromoteSelected => {
            let selected = candidate
                .snapshot
                .selected_occurrence
                .clone()
                .ok_or_else(|| {
                    "Select an occurrence before changing its scene layer.".to_owned()
                })?;
            candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default)
                .foreground_occurrences
                .insert(selected);
        },
        SceneAction::DemoteSelected => {
            let selected = candidate
                .snapshot
                .selected_occurrence
                .clone()
                .ok_or_else(|| {
                    "Select an occurrence before changing its scene layer.".to_owned()
                })?;
            candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default)
                .foreground_occurrences
                .remove(&selected);
        },
        SceneAction::Pan(dx, dy) => {
            if !dx.is_finite() || !dy.is_finite() {
                return Err("Scene pan values must be finite.".into());
            }
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            presentation.pan_x =
                (f64::from(presentation.pan_x) + f64::from(dx)).clamp(-4.0, 4.0) as f32;
            presentation.pan_y =
                (f64::from(presentation.pan_y) + f64::from(dy)).clamp(-4.0, 4.0) as f32;
        },
        SceneAction::Zoom(factor) => {
            if !factor.is_finite() || factor <= 0.0 {
                return Err("Scene zoom factor must be finite and positive.".into());
            }
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            presentation.zoom =
                (f64::from(presentation.zoom) * f64::from(factor)).clamp(0.25, 4.0) as f32;
        },
        SceneAction::Fit => {
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            presentation.pan_x = 0.0;
            presentation.pan_y = 0.0;
            presentation.zoom = 1.0;
        },
        SceneAction::Reset => candidate.presentation = None,
        SceneAction::MotionPreset(id) => {
            candidate.snapshot.recipe.definition.dynamics = id
                .as_deref()
                .map(pictograph::canvas::dynamics_recipe::preset_slot)
                .transpose()?;
        },
        SceneAction::MotionRole(role) => {
            let id = candidate
                .snapshot
                .selected_occurrence
                .clone()
                .ok_or_else(|| "Select an occurrence before assigning a motion role.".to_owned())?;
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            presentation.motion_roles.insert(id, role);
            presentation.refresh_version();
        },
        SceneAction::MotionBoundedSpring => {
            candidate.snapshot.recipe.definition.dynamics =
                Some(super::dynamics::bounded_spring_slot()?);
        },
        SceneAction::ClearMotionRoles => {
            if let Some(presentation) = &mut candidate.presentation {
                presentation.motion_roles.clear();
                presentation.refresh_version();
            }
        },
        SceneAction::MoveOccurrence { id, position } => {
            candidate
                .move_scene_occurrence(&id, [position.0, position.1])
                .map_err(|error| format!("Scene placement refused: {error}"))?;
        },
    }

    if candidate == *material {
        return Ok(false);
    }
    candidate
        .validate()
        .map_err(|error| format!("Scene presentation refused: {error}"))?;
    *material = candidate;
    Ok(true)
}

fn apply_canvas_event(
    material: &mut ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
    event: GraphCanvasEvent<String>,
) -> Result<bool, String> {
    let action = match event {
        GraphCanvasEvent::Activate(id) => {
            if !swatch_node_visible(material, compiled, &id) {
                return Ok(false);
            }
            SceneAction::SelectOccurrence(id)
        },
        GraphCanvasEvent::RelationActivate(id) => {
            if !swatch_relation_visible(material, compiled, &id) {
                return Ok(false);
            }
            SceneAction::SelectRelationship(id)
        },
        GraphCanvasEvent::Pan { delta } => SceneAction::Pan(delta.0, delta.1),
        GraphCanvasEvent::Zoom { factor } => SceneAction::Zoom(factor),
        GraphCanvasEvent::Drag(drag) => {
            if drag.phase != PointerPhase::Up || !swatch_node_visible(material, compiled, &drag.id)
            {
                return Ok(false);
            }
            SceneAction::MoveOccurrence {
                id: drag.id,
                position: drag.position,
            }
        },
        GraphCanvasEvent::Expand => return Ok(false),
    };
    apply_action(material, action)
}

fn swatch_node_visible(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
    id: &str,
) -> bool {
    visible_occurrence_ids(material, compiled).contains(id)
}

fn swatch_relation_visible(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
    id: &str,
) -> bool {
    let Some(disclosure) = compiled
        .relationships
        .iter()
        .map(|relation| &relation.disclosure)
        .find(|relation| relation.id == id)
    else {
        return false;
    };
    let presentation = material.presentation.as_ref().cloned().unwrap_or_default();
    let foreground_relation = presentation
        .foreground_occurrences
        .contains(&disclosure.from_occurrence)
        && presentation
            .foreground_occurrences
            .contains(&disclosure.to_occurrence);
    foreground_relation
        || (presentation.background_visible
            && !presentation
                .hidden_background_categories
                .contains(&disclosure.kind)
            && swatch_node_visible(material, compiled, &disclosure.from_occurrence)
            && swatch_node_visible(material, compiled, &disclosure.to_occurrence))
}

fn dispatch_control(state: &mut DesktopState, action: SceneAction) {
    if state.composition.recipe.dynamics.running()
        && matches!(&action, SceneAction::MotionPreset(_) | SceneAction::MotionBoundedSpring | SceneAction::MotionRole(_) | SceneAction::ClearMotionRoles)
    {
        state.composition.notice = Some("Pause scene motion before changing its recipe or occurrence roles.".into());
        return;
    }
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return;
    };
    let mut candidate = material.clone();
    state.composition.notice = Some(match apply_action(&mut candidate, action) {
        Ok(true) => {
            super::install_edit(state, candidate);
            "Scene presentation updated; retain deliberately to save this version.".into()
        },
        Ok(false) => "Scene presentation unchanged.".into(),
        Err(error) => error,
    });
}

fn dispatch_canvas(state: &mut DesktopState, event: GraphCanvasEvent<String>) {
    if state.composition.recipe.dynamics.running() {
        state.composition.notice =
            Some("Pause scene motion before selecting or placing occurrences.".into());
        return;
    }
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return;
    };
    let card = state
        .composition
        .recipe
        .measured
        .as_ref()
        .map_or(VALIDATION_CARD, |measured| measured.card);
    let compiled = ProjectionCompiler::new(ItemSizes { card })
        .compile_relationship_snapshot(&material.snapshot, &material.dataset);
    let mut candidate = material.clone();
    if let GraphCanvasEvent::Drag(drag) = event {
        let result = compiled
            .map_err(|error| format!("{error:?}"))
            .and_then(|compiled| handle_drag(&mut state.composition.recipe, &compiled, drag));
        match result {
            Ok(Some(candidate)) => super::install_edit(state, candidate),
            Ok(None) => {},
            Err(error) => {
                state.composition.recipe.scene_drag = None;
                state.composition.notice = Some(error);
            },
        }
        return;
    }
    state.composition.recipe.scene_drag = None;
    let result = compiled
        .map_err(|error| format!("{error:?}"))
        .and_then(|compiled| apply_canvas_event(&mut candidate, &compiled, event));
    match result {
        Ok(true) => super::install_edit(state, candidate),
        Ok(false) => {},
        Err(error) => state.composition.notice = Some(error),
    }
}

fn handle_drag(
    recipe: &mut RecipeState,
    compiled: &CompiledRelationshipProjection,
    drag: cambium::GraphCanvasNodeDrag<String>,
) -> Result<Option<ProjectionRecipeMaterial>, String> {
    let Some(material) = recipe.material.as_ref() else {
        return Ok(None);
    };
    if !drag.position.0.is_finite()
        || !drag.position.1.is_finite()
        || !(0.0..=1.0).contains(&drag.position.0)
        || !(0.0..=1.0).contains(&drag.position.1)
    {
        recipe.scene_drag = None;
        return Err("Scene placement requires finite normalized coordinates.".into());
    }
    if !material
        .presentation
        .as_ref()
        .is_some_and(|p| p.overview_visible)
        || !swatch_node_visible(material, compiled, &drag.id)
    {
        recipe.scene_drag = None;
        return Ok(None);
    }
    match drag.phase {
        PointerPhase::Down => {
            recipe.scene_drag = Some(SceneDrag {
                id: drag.id,
                before: material.clone(),
                preview: None,
            });
            Ok(None)
        },
        PointerPhase::Move => {
            if let Some(pending) = recipe
                .scene_drag
                .as_mut()
                .filter(|pending| pending.id == drag.id && pending.before == *material)
            {
                pending.preview = Some(drag.position);
            } else {
                recipe.scene_drag = None;
            }
            Ok(None)
        },
        PointerPhase::Up => {
            let Some(pending) = recipe
                .scene_drag
                .take()
                .filter(|pending| pending.id == drag.id && pending.before == *material)
            else {
                return Ok(None);
            };
            // A click or abandoned secondary press is not a placement edit.
            if pending.preview.is_none() {
                return Ok(None);
            }
            let mut candidate = material.clone();
            if apply_canvas_event(&mut candidate, compiled, GraphCanvasEvent::Drag(drag))? {
                Ok(Some(candidate))
            } else {
                Ok(None)
            }
        },
    }
}

fn move_selected(state: &mut DesktopState, dx: f32, dy: f32) {
    if state.composition.recipe.dynamics.running() {
        state.composition.notice = Some("Pause scene motion before authoring placement.".into());
        return;
    }
    state.composition.recipe.scene_drag = None;
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return;
    };
    let Some(id) = material.snapshot.selected_occurrence.clone() else {
        state.composition.notice = Some("Select a visible occurrence before moving it.".into());
        return;
    };
    let card = state
        .composition
        .recipe
        .measured
        .as_ref()
        .map_or(VALIDATION_CARD, |measured| measured.card);
    let Ok(compiled) = ProjectionCompiler::new(ItemSizes { card })
        .compile_relationship_snapshot(&material.snapshot, &material.dataset)
    else {
        return;
    };
    let Some(node) = swatch_with_preview(
        material,
        &compiled,
        &state.composition.recipe.scene_positions,
        None,
    )
    .graph
    .nodes
    .into_iter()
    .find(|node| node.id == id) else {
        state.composition.notice =
            Some("The selected occurrence is hidden; reveal it before moving it.".into());
        return;
    };
    dispatch_control(
        state,
        SceneAction::MoveOccurrence {
            id,
            position: (
                (node.position.0 + dx).clamp(0.0, 1.0),
                (node.position.1 + dy).clamp(0.0, 1.0),
            ),
        },
    );
}

pub(super) fn motion_authoring(material: &ProjectionRecipeMaterial) -> DesktopView {
    let role = material
        .snapshot
        .selected_occurrence
        .as_ref()
        .and_then(|id| material.presentation.as_ref()?.motion_roles.get(id))
        .copied()
        .unwrap_or_default();
    let roles: Vec<(String, DesktopView)> = seiche::Role::ALL
        .into_iter()
        .map(|value| {
            (
                value.id().to_owned(),
                Box::new(
                    button(
                        format!("Set selected motion role: {}", value.label()),
                        move |state: &mut DesktopState, _| {
                            dispatch_control(state, SceneAction::MotionRole(value))
                        },
                    )
                    .attr("aria-pressed", (role == value).to_string()),
                ) as DesktopView,
            )
        })
        .collect();
    Box::new(el("div", (
        span("Motion recipe · opt in; playback and transient poses are never retained."),
        button("Use shared spring dynamics", |state: &mut DesktopState, _| {
            dispatch_control(state, SceneAction::MotionPreset(Some("spring.rapier".into())))
        }),
        button("Use shared charge dynamics", |state: &mut DesktopState, _| {
            dispatch_control(state, SceneAction::MotionPreset(Some("charge.barnes-hut".into())))
        }),
        button("Use shared spring for 120 fixed frames", |state: &mut DesktopState, _| {
            dispatch_control(state, SceneAction::MotionBoundedSpring)
        }),
        button("Disable scene dynamics", |state: &mut DesktopState, _| {
            dispatch_control(state, SceneAction::MotionPreset(None))
        }),
        span("Seeded starts here; Anchored returns here; Pinned stays here. These roles do not change foreground emphasis."),
        Keyed::new(roles),
        button("Clear scene motion roles", |state: &mut DesktopState, _| {
            dispatch_control(state, SceneAction::ClearMotionRoles)
        }),
        span(format!("Motion slot: {} · occurrence role overrides: {}",
            if material.snapshot.recipe.definition.dynamics.is_some() { "authored" } else { "none" },
            material.presentation.as_ref().map_or(0, |p| p.motion_roles.len()))),
        span("Background filters change paint, not physics participation. Every compiled occurrence and disclosed relationship participates."),
    )).attr("class", "knot-recipe-motion-controls"))
}

pub(super) fn view(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
    camera_controls_expanded: bool,
    background_controls_expanded: bool,
    placement_controls_expanded: bool,
    drag: Option<&SceneDrag>,
    authored: &BTreeMap<String, [f32; 2]>,
    transient: &BTreeMap<String, [f32; 2]>,
    motion_controls: DesktopView,
    motion_expanded: bool,
) -> DesktopView {
    let mut display = authored.clone();
    display.extend(
        transient
            .iter()
            .map(|(id, position)| (id.clone(), *position)),
    );
    let presentation = material
        .presentation
        .as_ref()
        .cloned()
        .unwrap_or_else(RecipeScenePresentation::default);
    let selected = material.snapshot.selected_occurrence.as_ref();
    let overview_visible = presentation.overview_visible;
    let background_visible = presentation.background_visible;
    let selected_is_foreground =
        selected.is_some_and(|id| presentation.foreground_occurrences.contains(id));
    let foreground_count = presentation.foreground_occurrences.len();
    let selected_role = selected.map_or("none selected", |id| {
        if presentation.foreground_occurrences.contains(id) {
            "foreground"
        } else {
            "background"
        }
    });
    let background_count = material
        .dataset
        .dataset
        .occurrences
        .len()
        .saturating_sub(foreground_count);
    let categories: BTreeSet<_> = compiled
        .relationships
        .iter()
        .map(|relation| relation.disclosure.kind.clone())
        .collect();
    let visible_categories = categories
        .iter()
        .filter(|kind| !presentation.hidden_background_categories.contains(*kind))
        .count();
    let shown_occurrences = visible_occurrence_ids(material, compiled).len();
    let category_buttons: Vec<(String, DesktopView)> = categories
        .iter()
        .map(|kind| {
            let visible = !presentation.hidden_background_categories.contains(kind);
            let action_kind = kind.clone();
            (
                kind.clone(),
                Box::new(
                    button(
                        if visible {
                            format!("Hide scene background category: {kind}")
                        } else {
                            format!("Show scene background category: {kind}")
                        },
                        move |state: &mut DesktopState, _| {
                            dispatch_control(
                                state,
                                SceneAction::BackgroundCategory {
                                    kind: action_kind.clone(),
                                    visible: !visible,
                                },
                            )
                        },
                    )
                    .attr("aria-pressed", visible.to_string()),
                ) as DesktopView,
            )
        })
        .collect();
    let category_controls: Option<DesktopView> = background_controls_expanded.then(|| {
        Box::new(el(
            "div",
            (
                Keyed::new(category_buttons),
                button(
                    "Show all scene background categories",
                    |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::ShowAllBackgroundCategories)
                    },
                ),
            ),
        )) as DesktopView
    });
    let pan_controls = camera_controls_expanded.then(|| {
        Box::new(
            el(
                "div",
                (
                    button("Pan scene left", |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Pan(-CAMERA_STEP, 0.0))
                    }),
                    button("Pan scene right", |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Pan(CAMERA_STEP, 0.0))
                    }),
                    button("Pan scene up", |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Pan(0.0, -CAMERA_STEP))
                    }),
                    button("Pan scene down", |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Pan(0.0, CAMERA_STEP))
                    }),
                ),
            )
            .attr("class", "knot-recipe-scene-pan-controls"),
        ) as DesktopView
    });
    let body: DesktopView = if presentation.overview_visible {
        Box::new(el(
            "div",
            (
                span("Authored overview; drag an occurrence or use placement controls. Positions are not motion or attention pins."),
                span(format!("Authored scene positions: {}", authored.len()))
                    .attr("class", "knot-recipe-scene-placement-status"),
                span(format!(
                    "Scene strata · {foreground_count} foreground · {background_count} background occurrences · pan {:.2}, {:.2} · zoom {:.2}",
                    presentation.pan_x, presentation.pan_y, presentation.zoom
                ))
                .attr("class", "knot-recipe-scene-status"),
                span(format!("Background categories: {visible_categories} enabled / {} compiled; {shown_occurrences} occurrences shown", categories.len()))
                    .attr("class", "knot-recipe-scene-category-status"),
                span("Background category filters change presentation only; foreground occurrences stay visible."),
                el("div", (button(
                    if motion_expanded { "Hide scene motion authoring controls" } else { "Show scene motion authoring controls" },
                    move |state: &mut DesktopState, _| {
                        state.composition.recipe.motion_controls_expanded = !motion_expanded;
                    },
                ).attr("aria-expanded", motion_expanded.to_string()),
                motion_expanded.then(|| motion_authoring(material)),
                motion_controls,)),
                el(
                    "div",
                    (
                        span("Foreground · emphasized occurrence").attr("class", "knot-recipe-scene-key-foreground"),
                        span("Background · contextual occurrence").attr("class", "knot-recipe-scene-key-background"),
                        span(format!("Selected occurrence role: {selected_role}")),
                    ),
                )
                .attr("class", "knot-recipe-scene-key"),
                el("div", graph_canvas(
                    &swatch_with_preview(material, compiled, &display, drag),
                    |state: &mut DesktopState, event| dispatch_canvas(state, event),
                )).attr("id", "knot-recipe-motion-viewport"),
                button(
                    if placement_controls_expanded { "Hide scene placement controls" } else { "Show scene placement controls" },
                    move |state: &mut DesktopState, _| {
                        state.composition.recipe.scene_drag = None;
                        state.composition.recipe.placement_controls_expanded = !placement_controls_expanded;
                    },
                ).attr("aria-expanded", placement_controls_expanded.to_string())
                 .attr("aria-controls", "knot-recipe-scene-placement-controls"),
                el("div", placement_controls_expanded.then(|| {
                    Box::new(el("div", (
                        button("Move selected occurrence left", |state: &mut DesktopState, _| move_selected(state, -PLACEMENT_STEP, 0.0)),
                        button("Move selected occurrence right", |state: &mut DesktopState, _| move_selected(state, PLACEMENT_STEP, 0.0)),
                        button("Move selected occurrence up", |state: &mut DesktopState, _| move_selected(state, 0.0, -PLACEMENT_STEP)),
                        button("Move selected occurrence down", |state: &mut DesktopState, _| move_selected(state, 0.0, PLACEMENT_STEP)),
                    ))) as DesktopView
                })).attr("id", "knot-recipe-scene-placement-controls"),
                el(
                    "div",
                    (
                        button(
                            if presentation.background_visible {
                                "Hide scene background"
                            } else {
                                "Show scene background"
                            },
                            move |state: &mut DesktopState, _| {
                                dispatch_control(
                                    state,
                                    SceneAction::Background(!background_visible),
                                )
                            },
                        )
                        .attr("aria-pressed", presentation.background_visible.to_string()),
                        button(
                            "Promote selected occurrence",
                            |state: &mut DesktopState, _| {
                                dispatch_control(state, SceneAction::PromoteSelected)
                            },
                        )
                        .attr("aria-pressed", selected_is_foreground.to_string()),
                        button(
                            "Demote selected occurrence",
                            |state: &mut DesktopState, _| {
                                dispatch_control(state, SceneAction::DemoteSelected)
                            },
                        )
                        .attr(
                            "aria-pressed",
                            (selected.is_some() && !selected_is_foreground).to_string(),
                        ),
                    ),
                ),
                button(
                    if background_controls_expanded {
                        "Hide scene category controls"
                    } else {
                        "Show scene category controls"
                    },
                    move |state: &mut DesktopState, _| {
                        state.composition.recipe.background_controls_expanded =
                            !background_controls_expanded;
                    },
                )
                .attr("aria-expanded", background_controls_expanded.to_string())
                .attr("aria-controls", "knot-recipe-scene-category-controls"),
                el("div", category_controls).attr("id", "knot-recipe-scene-category-controls"),
                el("div", (
                    button("Zoom scene out", |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Zoom(0.8))
                    }),
                    button("Zoom scene in", |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Zoom(1.25))
                    }),
                    button("Fit relationship scene", |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Fit)
                    }),
                ))
                .attr("class", "knot-recipe-scene-camera-row"),
                button(
                    if camera_controls_expanded {
                        "Hide scene camera controls"
                    } else {
                        "Show scene camera controls"
                    },
                    move |state: &mut DesktopState, _| {
                        state.composition.recipe.camera_controls_expanded =
                            !camera_controls_expanded;
                    },
                )
                .attr("aria-expanded", camera_controls_expanded.to_string())
                .attr("aria-controls", "knot-recipe-scene-camera-controls"),
                el("div", pan_controls).attr("id", "knot-recipe-scene-camera-controls"),
            ),
        ))
    } else {
        Box::new(span("The relationship scene is hidden."))
    };
    Box::new(
        el(
            "section",
            (
                button(
                    if presentation.overview_visible {
                        "Hide relationship scene"
                    } else {
                        "Show relationship scene"
                    },
                    move |state: &mut DesktopState, _| {
                        dispatch_control(state, SceneAction::Overview(!overview_visible))
                    },
                )
                .attr("aria-pressed", presentation.overview_visible.to_string()),
                button("Reset scene presentation", |state: &mut DesktopState, _| {
                    dispatch_control(state, SceneAction::Reset)
                }),
                body,
            ),
        )
        .attr("class", "knot-recipe-scene"),
    )
}

pub(super) fn display_positions(recipe: &RecipeState) -> BTreeMap<String, [f32; 2]> {
    let mut positions = recipe.scene_positions.clone();
    positions.extend(
        recipe
            .dynamics
            .positions()
            .iter()
            .map(|(id, position)| (id.clone(), *position)),
    );
    positions
}

/// Clock only a mounted, painted, intersecting scene—not a retained tab.
pub(super) fn presented(
    ctx: &AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) -> bool {
    let state = ctx.runner.state();
    if state.theme_session.workshop_open
        || state.composition.section != 4
        || !state
            .composition
            .recipe
            .material
            .as_ref()
            .is_some_and(|m| m.presentation.as_ref().is_some_and(|p| p.overview_visible))
    {
        return false;
    }
    fn find<D: LayoutDom>(dom: &D, node: D::NodeId) -> Option<D::NodeId> {
        if dom.attribute(node, &Namespace::from(""), &LocalName::from("id"))
            == Some("knot-recipe-motion-viewport")
        {
            return Some(node);
        }
        dom.dom_children(node).find_map(|child| find(dom, child))
    }
    let node = {
        let dom_handle = ctx.runner.dom();
        let dom = dom_handle.borrow();
        find(&*dom, ctx.runner.root())
    };
    node.and_then(|node| ctx.visible_rect(node))
        .is_some_and(|(x, y, w, h)| {
            [x, y, w, h].iter().all(|v| v.is_finite())
                && w > 0.0
                && h > 0.0
                && x < ctx.logical_size.0
                && y < ctx.logical_size.1
                && x + w > 0.0
                && y + h > 0.0
        })
}

pub(super) fn refresh_leaf(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
    // The shared pointer protocol has no Cancel event. Losing capture (focus
    // loss, removed target, or an uncaptured secondary press) abandons preview.
    if ctx.runner.pointer_capture().is_none()
        && ctx.runner.state().composition.recipe.scene_drag.is_some()
    {
        ctx.runner
            .update(|state| state.composition.recipe.scene_drag = None);
    }
    let leaf = {
        let state = ctx.runner.state();
        state
            .composition
            .recipe
            .material
            .as_ref()
            .filter(|material| {
                material
                    .presentation
                    .as_ref()
                    .is_some_and(|presentation| presentation.overview_visible)
            })
            .and_then(|material| {
                let card = state
                    .composition
                    .recipe
                    .measured
                    .as_ref()
                    .map_or(VALIDATION_CARD, |measured| measured.card);
                let compiled = ProjectionCompiler::new(ItemSizes { card })
                    .compile_relationship_snapshot(&material.snapshot, &material.dataset)
                    .ok()?;
                Some(
                    swatch_with_preview(
                        material,
                        &compiled,
                        &display_positions(&state.composition.recipe),
                        state.composition.recipe.scene_drag.as_ref(),
                    )
                    .paint_leaf(color_for_layer),
                )
            })
    };
    if let Some(leaf) = leaf {
        ctx.leaves.insert(SCENE_LEAF_KEY, Box::new(leaf));
    } else {
        ctx.leaves.remove(&SCENE_LEAF_KEY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::{GraphCanvasNodeDrag, PointerPhase};
    use knot_readings::sound::SoundLayers;
    use std::collections::BTreeSet;

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

    fn mixed_material() -> ProjectionRecipeMaterial {
        let (source, _) = super::super::tests::fixture();
        let layers = SoundLayers {
            perfect_rhyme: true,
            assonance: true,
            ..Default::default()
        };
        let reading = knot_readings::sound::analyze(
            &source.text,
            0..source.text.len(),
            &std::collections::BTreeMap::new(),
            &layers,
        )
        .unwrap();
        super::super::material(&source, &reading, &layers).unwrap()
    }

    #[test]
    fn first_enable_seeds_foreground_without_changing_recipe_selection() {
        let mut material = material();
        material.snapshot.selected_occurrence = Some("token-6-11".into());
        let before_selection = material.snapshot.selected_occurrence.clone();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        let presentation = material.presentation.as_ref().unwrap();
        assert!(presentation.overview_visible);
        assert!(presentation.foreground_occurrences.contains("token-6-11"));
        assert_eq!(material.snapshot.selected_occurrence, before_selection);
        material.validate().unwrap();
    }

    #[test]
    fn reset_clears_presentation_without_changing_source_selection() {
        let mut material = material();
        material.snapshot.selected_occurrence = Some("token-6-11".into());
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        apply_action(&mut material, SceneAction::Pan(0.5, -0.25)).unwrap();
        apply_action(&mut material, SceneAction::Reset).unwrap();
        assert!(material.presentation.is_none());
        assert_eq!(
            material.snapshot.selected_occurrence.as_deref(),
            Some("token-6-11")
        );
        material.validate().unwrap();
    }

    #[test]
    fn camera_actions_stay_finite_and_within_persisted_bounds() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        apply_action(&mut material, SceneAction::Pan(100.0, -100.0)).unwrap();
        apply_action(&mut material, SceneAction::Zoom(100.0)).unwrap();
        let presentation = material.presentation.as_ref().unwrap();
        assert_eq!((presentation.pan_x, presentation.pan_y), (4.0, -4.0));
        assert_eq!(presentation.zoom, 4.0);
        assert!(apply_action(&mut material, SceneAction::Pan(f32::NAN, 0.0)).is_err());
        assert!(apply_action(&mut material, SceneAction::Zoom(f32::INFINITY)).is_err());
        apply_action(&mut material, SceneAction::Fit).unwrap();
        let presentation = material.presentation.as_ref().unwrap();
        assert_eq!(
            (presentation.pan_x, presentation.pan_y, presentation.zoom),
            (0.0, 0.0, 1.0)
        );
        material.validate().unwrap();
    }

    #[test]
    fn intermediate_drag_event_does_not_change_retained_material() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        let before = serde_json::to_vec(&material).unwrap();
        let event = GraphCanvasEvent::Drag(GraphCanvasNodeDrag {
            id: "token-6-11".into(),
            phase: PointerPhase::Move,
            position: (0.8, 0.7),
        });
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        assert!(!apply_canvas_event(&mut material, &compiled, event).unwrap());
        assert_eq!(serde_json::to_vec(&material).unwrap(), before);
    }

    #[test]
    fn drag_previews_then_commits_one_undoable_source_preserving_placement() {
        let mut before = material();
        apply_action(&mut before, SceneAction::Overview(true)).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&before.snapshot, &before.dataset)
            .unwrap();
        let id = "token-6-11".to_owned();
        let mut recipe = RecipeState::default();
        recipe.material = Some(before.clone());
        recipe.history.reset(&before, true);
        for (phase, position) in [
            (PointerPhase::Down, (0.2, 0.2)),
            (PointerPhase::Move, (0.4, 0.5)),
            (PointerPhase::Move, (0.8, 0.7)),
        ] {
            assert!(
                handle_drag(
                    &mut recipe,
                    &compiled,
                    GraphCanvasNodeDrag {
                        id: id.clone(),
                        phase,
                        position
                    }
                )
                .unwrap()
                .is_none()
            );
        }
        assert_eq!(recipe.material.as_ref(), Some(&before));
        assert!(!recipe.history.can_undo());
        let preview = swatch_with_preview(
            &before,
            &compiled,
            &before.scene_positions().unwrap(),
            recipe.scene_drag.as_ref(),
        );
        assert_eq!(
            preview
                .graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .position,
            (0.8, 0.7)
        );
        let after = handle_drag(
            &mut recipe,
            &compiled,
            GraphCanvasNodeDrag {
                id: id.clone(),
                phase: PointerPhase::Up,
                position: (0.8, 0.7),
            },
        )
        .unwrap()
        .unwrap();
        recipe.history.record(&before, &after);
        assert_eq!(after.dataset, before.dataset);
        assert_eq!(after.anchors, before.anchors);
        assert_eq!(after.snapshot, before.snapshot);
        assert_eq!(after.scene_positions().unwrap()[&id], [0.8, 0.7]);
        assert_eq!(
            after
                .presentation
                .as_ref()
                .unwrap()
                .placement_trace
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        let restored = recipe.history.undo(&after).unwrap().unwrap();
        assert_eq!(restored, before);
        assert!(!recipe.history.can_undo());
        let redone = recipe.history.redo(&restored).unwrap().unwrap();
        assert_eq!(redone, after);
        let graph = swatch(&after, &compiled);
        assert_eq!(
            graph
                .graph
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .position,
            (0.8, 0.7)
        );
        assert!(
            graph
                .relations
                .iter()
                .any(|relation| relation.route.contains(&(0.8, 0.7)))
        );
    }

    #[test]
    fn stale_hidden_invalid_or_click_only_gestures_cannot_author_placements() {
        let mut before = material();
        apply_action(&mut before, SceneAction::Overview(true)).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&before.snapshot, &before.dataset)
            .unwrap();
        let mut recipe = RecipeState::default();
        recipe.material = Some(before.clone());
        let event = |phase, position| GraphCanvasNodeDrag {
            id: "token-6-11".into(),
            phase,
            position,
        };
        assert!(
            handle_drag(
                &mut recipe,
                &compiled,
                event(PointerPhase::Down, (0.4, 0.4))
            )
            .unwrap()
            .is_none()
        );
        assert!(
            handle_drag(&mut recipe, &compiled, event(PointerPhase::Up, (0.4, 0.4)))
                .unwrap()
                .is_none()
        );
        handle_drag(
            &mut recipe,
            &compiled,
            event(PointerPhase::Down, (0.4, 0.4)),
        )
        .unwrap();
        handle_drag(
            &mut recipe,
            &compiled,
            event(PointerPhase::Move, (0.8, 0.7)),
        )
        .unwrap();
        apply_action(
            recipe.material.as_mut().unwrap(),
            SceneAction::Pan(0.1, 0.0),
        )
        .unwrap();
        assert!(
            handle_drag(&mut recipe, &compiled, event(PointerPhase::Up, (0.8, 0.7)))
                .unwrap()
                .is_none()
        );
        assert!(
            recipe
                .material
                .as_ref()
                .unwrap()
                .scene_positions()
                .unwrap()
                .is_empty()
        );
        assert!(
            handle_drag(
                &mut recipe,
                &compiled,
                event(PointerPhase::Down, (f32::NAN, 0.4))
            )
            .is_err()
        );
        assert!(recipe.scene_drag.is_none());
        apply_action(
            recipe.material.as_mut().unwrap(),
            SceneAction::Overview(false),
        )
        .unwrap();
        assert!(
            handle_drag(
                &mut recipe,
                &compiled,
                event(PointerPhase::Down, (0.4, 0.4))
            )
            .unwrap()
            .is_none()
        );
        assert!(recipe.scene_drag.is_none());
    }

    #[test]
    fn category_and_camera_edits_keep_trace_version_and_positions() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        material
            .move_scene_occurrence("token-6-11", [0.8, 0.7])
            .unwrap();
        let positions = material.scene_positions().unwrap();
        let kind = material.dataset.relationships[0].kind.clone();
        apply_action(
            &mut material,
            SceneAction::BackgroundCategory {
                kind,
                visible: false,
            },
        )
        .unwrap();
        assert_eq!(material.presentation.as_ref().unwrap().version, 3);
        apply_action(&mut material, SceneAction::ShowAllBackgroundCategories).unwrap();
        apply_action(&mut material, SceneAction::Fit).unwrap();
        assert_eq!(material.presentation.as_ref().unwrap().version, 3);
        assert_eq!(material.scene_positions().unwrap(), positions);
    }

    #[test]
    fn canvas_activation_updates_only_typed_recipe_selection() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        let relation = material.dataset.relationships[0].clone();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        apply_canvas_event(
            &mut material,
            &compiled,
            GraphCanvasEvent::RelationActivate(relation.id.clone()),
        )
        .unwrap();
        assert_eq!(
            material.snapshot.selected_relationship.as_deref(),
            Some(relation.id.as_str())
        );
        assert_eq!(
            material.snapshot.selected_occurrence.as_deref(),
            Some(relation.from_occurrence.as_str())
        );
        apply_canvas_event(
            &mut material,
            &compiled,
            GraphCanvasEvent::Activate(relation.to_occurrence.clone()),
        )
        .unwrap();
        assert_eq!(
            material.snapshot.selected_occurrence.as_deref(),
            Some(relation.to_occurrence.as_str())
        );
        assert!(material.snapshot.selected_relationship.is_none());
        assert!(material.presentation.as_ref().unwrap().overview_visible);
        material.validate().unwrap();
    }

    #[test]
    fn overview_preserves_distinct_occurrences_and_hides_background_strata() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        let graph = swatch(&material, &compiled);
        let ids: BTreeSet<_> = graph.graph.nodes.iter().map(|node| &node.id).collect();
        assert_eq!(
            graph.graph.nodes.len(),
            material.dataset.dataset.occurrences.len()
        );
        assert_eq!(ids.len(), graph.graph.nodes.len());
        assert_eq!(graph.relations.len(), compiled.relationships.len());
        let first_relation = &compiled.relationships[0].disclosure;
        let presentation = material.presentation.as_mut().unwrap();
        presentation
            .foreground_occurrences
            .insert(first_relation.from_occurrence.clone());
        presentation
            .foreground_occurrences
            .insert(first_relation.to_occurrence.clone());
        material.validate().unwrap();
        apply_action(&mut material, SceneAction::Background(false)).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        let graph = swatch(&material, &compiled);
        let foreground = &material
            .presentation
            .as_ref()
            .unwrap()
            .foreground_occurrences;
        assert_eq!(graph.graph.nodes.len(), foreground.len());
        assert!(
            graph
                .graph
                .nodes
                .iter()
                .all(|node| foreground.contains(&node.id))
        );
        assert!(graph.relations.iter().all(|relation| {
            relation.visible
                == (foreground.contains(&relation.from) && foreground.contains(&relation.to))
        }));
    }

    #[test]
    fn background_category_filter_is_presentation_only_and_keeps_foreground_visible() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        let kind = compiled.relationships[0].disclosure.kind.clone();
        let original_dataset = material.dataset.clone();
        let original_snapshot = material.snapshot.clone();
        material
            .presentation
            .as_mut()
            .unwrap()
            .foreground_occurrences
            .insert(compiled.relationships[0].disclosure.from_occurrence.clone());
        apply_action(
            &mut material,
            SceneAction::BackgroundCategory {
                kind: kind.clone(),
                visible: false,
            },
        )
        .unwrap();
        let presentation = material.presentation.as_ref().unwrap();
        assert_eq!(presentation.version, 2);
        assert!(presentation.hidden_background_categories.contains(&kind));
        assert_eq!(material.dataset, original_dataset);
        assert_eq!(material.snapshot, original_snapshot);

        let graph = swatch(&material, &compiled);
        assert!(
            graph
                .graph
                .nodes
                .iter()
                .any(|node| presentation.foreground_occurrences.contains(&node.id))
        );
        assert!(graph.relations.iter().all(|relation| relation.kind != kind));
        apply_action(&mut material, SceneAction::ShowAllBackgroundCategories).unwrap();
        let presentation = material.presentation.as_ref().unwrap();
        assert!(presentation.hidden_background_categories.is_empty());
        assert_eq!(presentation.version, 1);
    }

    #[test]
    fn category_controls_follow_the_global_recipe_category_filter() {
        let mut material = mixed_material();
        let categories: BTreeSet<_> = material
            .dataset
            .relationships
            .iter()
            .map(|relation| relation.kind.clone())
            .collect();
        assert!(categories.len() > 1);
        let selected = categories.iter().next().unwrap().clone();
        material.snapshot.recipe.relationship_kind = Some(selected.clone());
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        let compiled_categories: BTreeSet<_> = compiled
            .relationships
            .iter()
            .map(|relation| relation.disclosure.kind.clone())
            .collect();
        assert_eq!(compiled_categories, BTreeSet::from([selected]));
    }

    #[test]
    fn invalid_background_category_action_preserves_material() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        let before = material.clone();
        assert!(
            apply_action(
                &mut material,
                SceneAction::BackgroundCategory {
                    kind: "not-disclosed".into(),
                    visible: false,
                }
            )
            .is_err()
        );
        assert_eq!(material, before);
    }

    #[test]
    fn isolated_background_occurrences_keep_legacy_visibility_until_filtered() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        material
            .presentation
            .as_mut()
            .unwrap()
            .foreground_occurrences
            .clear();
        let mut compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        compiled.relationships.clear();
        let isolated = material.dataset.dataset.occurrences[0]
            .occurrence_id
            .clone();
        assert!(
            swatch(&material, &compiled)
                .graph
                .nodes
                .iter()
                .any(|node| node.id == isolated)
        );

        let kind = material.dataset.relationships[0].kind.clone();
        apply_action(
            &mut material,
            SceneAction::BackgroundCategory {
                kind,
                visible: false,
            },
        )
        .unwrap();
        assert!(
            !swatch(&material, &compiled)
                .graph
                .nodes
                .iter()
                .any(|node| node.id == isolated)
        );
    }

    #[test]
    fn foreground_edge_survives_category_and_master_background_hides() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        let disclosure = &compiled.relationships[0].disclosure;
        let relation_id = disclosure.id.clone();
        let category = disclosure.kind.clone();
        let from = disclosure.from_occurrence.clone();
        let to = disclosure.to_occurrence.clone();
        material
            .presentation
            .as_mut()
            .unwrap()
            .foreground_occurrences
            .extend([from, to]);
        apply_action(
            &mut material,
            SceneAction::BackgroundCategory {
                kind: category,
                visible: false,
            },
        )
        .unwrap();
        apply_action(&mut material, SceneAction::Background(false)).unwrap();
        let graph = swatch(&material, &compiled);
        assert!(
            graph
                .relations
                .iter()
                .any(|relation| relation.id == relation_id)
        );
    }

    #[test]
    fn hidden_canvas_targets_are_noops_but_fallback_occurrence_selection_remains_available() {
        let mut material = material();
        apply_action(&mut material, SceneAction::Overview(true)).unwrap();
        material
            .presentation
            .as_mut()
            .unwrap()
            .foreground_occurrences
            .clear();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        let relation = material.dataset.relationships[0].clone();
        apply_action(&mut material, SceneAction::Background(false)).unwrap();
        let before = material.clone();
        assert!(
            !apply_canvas_event(
                &mut material,
                &compiled,
                GraphCanvasEvent::Activate(relation.from_occurrence.clone()),
            )
            .unwrap()
        );
        assert_eq!(material, before);
        assert!(
            !apply_canvas_event(
                &mut material,
                &compiled,
                GraphCanvasEvent::RelationActivate(relation.id),
            )
            .unwrap()
        );
        assert_eq!(material, before);
        assert!(
            apply_action(
                &mut material,
                SceneAction::SelectOccurrence(relation.from_occurrence.clone()),
            )
            .unwrap()
        );
        assert_eq!(
            material.snapshot.selected_occurrence.as_deref(),
            Some(relation.from_occurrence.as_str())
        );
    }
}
