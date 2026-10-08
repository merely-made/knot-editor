// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's compact, editable presentation of a compiled relationship recipe.
//! Scene truth stays in the copied recipe; this module stores only visibility,
//! emphasis and camera preferences in Knot's retained composition material.

use super::*;
use cambium::{
    GraphCanvasEdge, GraphCanvasEvent, GraphCanvasNode, GraphCanvasRelation, GraphCanvasSubgraph,
    GraphCanvasSwatch, button, el, graph_canvas, span,
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
    SelectOccurrence(String),
    SelectRelationship(String),
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

fn swatch(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
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
        let position = (
            normalized(x, scene.bounds.origin.x, scene.bounds.size.w),
            normalized(y, scene.bounds.origin.y, scene.bounds.size.h),
        );
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
        .with_label("Static relationship scene")
        .with_expand(false)
        .with_node_labels(true);
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
            presentation.version = if presentation.hidden_background_categories.is_empty() {
                1
            } else {
                2
            };
        },
        SceneAction::ShowAllBackgroundCategories => {
            let presentation = candidate
                .presentation
                .get_or_insert_with(RecipeScenePresentation::default);
            presentation.hidden_background_categories.clear();
            presentation.version = 1;
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
        // This overview is a static compiled projection. GraphCanvas exposes
        // drag events, but Knot does not turn them into placement or physics.
        GraphCanvasEvent::Drag(_) | GraphCanvasEvent::Expand => return Ok(false),
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
    let Some(material) = state.composition.recipe.material.as_mut() else {
        return;
    };
    state.composition.notice = Some(match apply_action(material, action) {
        Ok(_) => "Scene presentation updated; retain deliberately to save this version.".into(),
        Err(error) => error,
    });
}

fn dispatch_canvas(state: &mut DesktopState, event: GraphCanvasEvent<String>) {
    let Some(material) = state.composition.recipe.material.as_mut() else {
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
    let result = compiled
        .map_err(|error| format!("{error:?}"))
        .and_then(|compiled| apply_canvas_event(material, &compiled, event));
    if let Err(error) = result {
        state.composition.notice = Some(error);
    }
}

pub(super) fn view(
    material: &ProjectionRecipeMaterial,
    compiled: &CompiledRelationshipProjection,
    camera_controls_expanded: bool,
    background_controls_expanded: bool,
) -> DesktopView {
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
                span("Static overview; nodes do not move. Pan the canvas or use camera controls."),
                span(format!(
                    "Scene strata · {foreground_count} foreground · {background_count} background occurrences · pan {:.2}, {:.2} · zoom {:.2}",
                    presentation.pan_x, presentation.pan_y, presentation.zoom
                ))
                .attr("class", "knot-recipe-scene-status"),
                span(format!("Background categories: {visible_categories} enabled / {} compiled; {shown_occurrences} occurrences shown", categories.len()))
                    .attr("class", "knot-recipe-scene-category-status"),
                span("Background category filters change presentation only; foreground occurrences stay visible."),
                el(
                    "div",
                    (
                        span("Foreground · emphasized occurrence").attr("class", "knot-recipe-scene-key-foreground"),
                        span("Background · contextual occurrence").attr("class", "knot-recipe-scene-key-background"),
                        span(format!("Selected occurrence role: {selected_role}")),
                    ),
                )
                .attr("class", "knot-recipe-scene-key"),
                graph_canvas(
                    &swatch(material, compiled),
                    |state: &mut DesktopState, event| dispatch_canvas(state, event),
                ),
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

pub(super) fn refresh_leaf(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
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
                Some(swatch(material, &compiled).paint_leaf(color_for_layer))
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
    fn static_drag_event_does_not_change_retained_material() {
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
