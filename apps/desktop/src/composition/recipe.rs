// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot supplies source facts and exact document actions. Shared Scenograph and
//! Scenomise own the authoring contract and executable projection compiler.

use super::SelectionSnapshot;
use crate::workspace::{DesktopState, DesktopView};
use cambium::{Keyed, button, el, span};
use cambium_genet_winit_host::AppCtx;
use knot_composition::DocumentAnchor;
use knot_composition::retention::{
    ProjectionRecipeMaterial, RecipeSourceAnchor, VALIDATION_CARD, validation_compiler,
};
use knot_composition::{CollectionItem, ItemKind};
use knot_readings::sound::SoundReading;
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use sceno::Size2;
use scenograph::relationship::{
    RecipeEdit, RelationshipRecipeDraft, RelationshipSnapshot, relationship_recipe,
};
use scenograph::{ProjectionInputBinding, PublicSourceRevision, RevisionEvidence, SourceBinding};
use scenomise::projection::{
    CompiledProjection, DisclosedRelationship, ItemSizes, ProjectionCompiler, ProjectionDataset,
    ProjectionFieldType, ProjectionOccurrence, ProjectionValue, RelationshipDataset,
    RelationshipProvenance,
};
use std::collections::{BTreeMap, BTreeSet};

/// What an occurrence button adds around its label: Knot's declared
/// `padding:6px 10px; border:1px solid` (`appearance.rs`) in the UA's
/// border-box, so a card is its label plus this.
const CARD_FRAME: Size2 = Size2 {
    w: 2.0 * (10.0 + 1.0),
    h: 2.0 * (6.0 + 1.0),
};
/// Layout rects are whole pixels and a label's advance is not: one more pixel
/// keeps the widest label on one line (Mere burn plan 13.46).
const CARD_ROUNDING: f32 = 1.0;
/// The probe attribute: its value is the drawn label, shown only through CSS
/// generated content so no DOM text or role exists for a scenario to match.
const MEASURE_ATTR: &str = "data-knot-recipe-measure";

/// The card measured for one drawn label set.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MeasuredCard {
    pub(super) labels: Vec<String>,
    pub(super) card: Size2,
}

/// Frames a probe may go without a usable rect before the hook stops asking
/// for more (Mere burn plan 13.46, "Stop after a few frames").
pub(super) const MEASURE_ATTEMPTS: u32 = 3;

/// Consecutive frames whose probes gave no usable rect, for one label set in
/// one window size. At [`MEASURE_ATTEMPTS`] the hook has given up on them.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FailedMeasure {
    pub(super) labels: Vec<String>,
    pub(super) surface: (f32, f32),
    pub(super) frames: u32,
}

#[derive(Default)]
pub(super) struct RecipeState {
    pub(super) material: Option<ProjectionRecipeMaterial>,
    /// The card for the label set last measured; drawing waits for it.
    pub(super) measured: Option<MeasuredCard>,
    /// How many measurements this session took.
    pub(super) measurements: u32,
    /// The current run of failed measurements, if any.
    pub(super) failed: Option<FailedMeasure>,
}

/// A card for the widest probe: its rounded-up label, the rounding margin and
/// the button frame, by one measured line and the frame.
fn card_for(width: f32, height: f32) -> Size2 {
    Size2::new(
        width.ceil() + CARD_ROUNDING + CARD_FRAME.w,
        height.ceil() + CARD_FRAME.h,
    )
}

/// What each occurrence button shows, in instance order: the label set a
/// measurement belongs to.
fn drawn_labels(projection: &CompiledProjection) -> Vec<String> {
    projection
        .instance_by_occurrence
        .iter()
        .map(|(id, instance)| {
            let label = projection
                .labels
                .get(instance)
                .cloned()
                .unwrap_or_else(|| id.clone());
            format!("{label} · {id}")
        })
        .collect()
}

/// One hidden, role-less probe: the drawn label at the pressed weight, laid out
/// on one line through `::after { content: attr(...) }` (`readings::CSS`).
fn probe(label: &str) -> DesktopView {
    Box::new(
        el::<_, DesktopState, ()>("span", ())
            .attr("class", "knot-recipe-measure")
            .attr("aria-hidden", "true")
            .attr(MEASURE_ATTR, label.to_owned()),
    )
}

/// The frame hook's half of measurement. Reads the probes the previous layout
/// placed, stores the card for their label set, and so removes them. Returns
/// whether probes are still waiting for a layout: after [`MEASURE_ATTEMPTS`]
/// frames without a usable rect it stops, leaving the scene hidden until the
/// label set or the window size changes.
pub(crate) fn measure_cards(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) -> bool {
    if ctx.runner.state().composition.recipe.material.is_none() {
        return false;
    }
    fn probes<D: LayoutDom>(dom: &D, node: D::NodeId, out: &mut Vec<(D::NodeId, String)>) {
        if let Some(label) =
            dom.attribute(node, &Namespace::from(""), &LocalName::from(MEASURE_ATTR))
        {
            out.push((node, label.to_owned()));
        }
        for child in dom.dom_children(node) {
            probes(dom, child, out);
        }
    }
    let found = {
        let dom = ctx.runner.dom();
        let dom = dom.borrow();
        let mut out = Vec::new();
        probes(&*dom, ctx.runner.root(), &mut out);
        out
    };
    if found.is_empty() {
        return false;
    }
    let labels: Vec<String> = found.iter().map(|(_, label)| label.clone()).collect();
    // The window's own size, not the zoomed layout size: a zoom is no resize.
    let surface = (
        ctx.logical_size.0 * ctx.ui_zoom,
        ctx.logical_size.1 * ctx.ui_zoom,
    );
    let earlier = ctx
        .runner
        .state()
        .composition
        .recipe
        .failed
        .as_ref()
        .filter(|failed| failed.labels == labels && failed.surface == surface)
        .map_or(0, |failed| failed.frames);
    if earlier >= MEASURE_ATTEMPTS {
        return false;
    }
    // Only a finite, positive rect is a measurement. Anything else keeps the
    // scene hidden and counts as a failed frame, so the compiler is never
    // handed a size from a missing or degenerate probe.
    let mut widest = (0.0f32, 0.0f32);
    for (node, _) in &found {
        let usable = |width: f32, height: f32| {
            width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
        };
        let Some((_, _, width, height)) = ctx
            .painted_rect(*node)
            .filter(|&(_, _, width, height)| usable(width, height))
        else {
            let frames = earlier + 1;
            if frames == MEASURE_ATTEMPTS {
                eprintln!(
                    "knot: recipe cards not measured after {frames} frames; the scene stays hidden until its labels or the window size change"
                );
            }
            ctx.runner.update(|state| {
                state.composition.recipe.failed = Some(FailedMeasure {
                    labels,
                    surface,
                    frames,
                });
            });
            return frames < MEASURE_ATTEMPTS;
        };
        widest = (widest.0.max(width), widest.1.max(height));
    }
    let measured = MeasuredCard {
        labels,
        card: card_for(widest.0, widest.1),
    };
    ctx.runner.update(|state| {
        state.composition.recipe.measured = Some(measured);
        state.composition.recipe.measurements += 1;
        state.composition.recipe.failed = None;
    });
    false
}

fn finish_draft(
    mut draft: RelationshipRecipeDraft,
) -> Result<scenograph::relationship::RelationshipRecipe, String> {
    // Content identity belongs to the authored recipe, not the document's
    // source revision. Exclude this self-referential field before hashing.
    let mut provenance = draft.recipe().definition.provenance.clone();
    provenance.source_revision = None;
    draft.apply(RecipeEdit::SetProvenance(provenance.clone()));
    let bytes = serde_json::to_vec(draft.recipe()).map_err(|error| error.to_string())?;
    provenance.source_revision = Some(PublicSourceRevision::new(
        blake3::hash(&bytes).to_hex().to_string(),
    ));
    draft.apply(RecipeEdit::SetProvenance(provenance));
    draft
        .to_recipe()
        .map_err(|issues| format!("Recipe draft refused: {issues:?}"))
}

fn material(
    source: &SelectionSnapshot,
    reading: &SoundReading,
    layers: &knot_readings::sound::SoundLayers,
) -> Result<ProjectionRecipeMaterial, String> {
    let facts = sound_facts(source, reading)?;
    if facts.relations.is_empty() {
        return Err("This recipe requires an actual sound relationship. Enable a sound layer and read a related selection first.".into());
    }
    let binding = SourceBinding {
        authority: "knot-editor".into(),
        domain: "sound-reading".into(),
        resource: format!("{}#bytes={}-{}", source.address, source.start, source.end),
    };
    let revision_bytes = serde_json::to_vec(&(
        "knot.sound-reading.v1",
        "Mora 0.1.0; CMUdict 59d6398f55297e59afb2ca3276380827524c0940fcbbfcd19022bb76fd55f719",
        source.anchor()?,
        &facts,
        (
            layers.perfect_rhyme,
            layers.slant_rhyme,
            layers.assonance,
            layers.alliteration,
            layers.meter,
        ),
    ))
    .map_err(|error| error.to_string())?;
    let revision = PublicSourceRevision::new(blake3::hash(&revision_bytes).to_hex().to_string());
    let source_ref = sceno::SourceRef::new("knot.document", source.address.clone());
    let dataset = RelationshipDataset {
        dataset: ProjectionDataset {
            source: binding.clone(), revision: revision.clone(),
            fields: BTreeMap::from([("occurrence_id".into(), ProjectionFieldType::Text), ("order".into(), ProjectionFieldType::Number), ("label".into(), ProjectionFieldType::Text)]),
            occurrences: facts.occurrences.iter().enumerate().map(|(index, occurrence)| ProjectionOccurrence {
                occurrence_id: occurrence.id.clone(), source: source_ref.clone(),
                values: BTreeMap::from([("occurrence_id".into(), ProjectionValue::Text(occurrence.id.clone())), ("order".into(), ProjectionValue::Number(index as f64)), ("label".into(), ProjectionValue::Text(occurrence.label.clone()))]),
            }).collect(),
        },
        facets: BTreeSet::from(["authored_order".into(), "occurrence_labels".into(), "explained_relationships".into()]),
        relationships: facts.relations.iter().enumerate().map(|(index, relation)| DisclosedRelationship {
            id: format!("sound-relation-{index}"), from_occurrence: relation.left.clone(), to_occurrence: relation.right.clone(),
            kind: relation.kind.clone(), label: relation.explanation.split(':').next().unwrap_or("Sound relation").into(), explanation: relation.explanation.clone(),
            provenance: RelationshipProvenance { source: binding.clone(), source_revision: revision.clone(), method: "Mora sound comparison".into(), method_version: 1, provider: "Mora 0.1.0; CMUdict SHA-256 59d6398f55297e59afb2ca3276380827524c0940fcbbfcd19022bb76fd55f719".into(), evidence: vec![source_ref.clone()] },
        }).collect(),
    };
    let recipe = relationship_recipe(
        "knot-sound-comparison",
        "Sound relationship comparison",
        "Knot",
        "1",
        BTreeMap::from([(
            "selection".into(),
            ProjectionInputBinding {
                source: binding,
                expects_generation: Some(revision),
                revision_evidence: RevisionEvidence::PublicGeneration,
            },
        )]),
    );
    let recipe = finish_draft(RelationshipRecipeDraft::new(recipe))?;
    let material = ProjectionRecipeMaterial {
        snapshot: RelationshipSnapshot {
            recipe,
            source_name: "selection".into(),
            selected_occurrence: None,
            selected_relationship: None,
        },
        dataset,
        anchors: facts
            .occurrences
            .into_iter()
            .map(|occurrence| RecipeSourceAnchor {
                node_id: occurrence.id,
                source: occurrence.anchor,
            })
            .collect(),
    };
    validation_compiler()
        .compile_relationship_snapshot(&material.snapshot, &material.dataset)
        .map_err(|errors| format!("Recipe refused: {errors:?}"))?;
    Ok(material)
}

pub(super) fn from_sound(state: &mut DesktopState, key: crate::documents::DocKey, rebind: bool) {
    let result = (|| {
        let source = state
            .composition
            .sound_selection
            .as_ref()
            .ok_or("Read selected sounds first.")?;
        if source.key != key || !source.current(state) {
            return Err("Sound selection changed; explicitly rerun the sound reading before binding a recipe.".to_owned());
        }
        let reading = state
            .composition
            .sound
            .as_ref()
            .ok_or("Read selected sounds first.")?;
        let mut material = material(source, reading, &state.composition.layers)?;
        if rebind {
            let previous = state
                .composition
                .recipe
                .material
                .as_ref()
                .ok_or("Open a recipe before rebinding.")?;
            let mut draft = RelationshipRecipeDraft::new(previous.snapshot.recipe.clone());
            draft.apply(RecipeEdit::BindInput {
                name: "selection".into(),
                binding: material.snapshot.recipe.definition.sources["selection"].clone(),
            });
            material.snapshot.recipe = finish_draft(draft)?;
            validation_compiler()
                .compile_relationship_snapshot(&material.snapshot, &material.dataset)
                .map_err(|issues| format!("Rebinding refused: {issues:?}"))?;
        }
        Ok(material)
    })();
    match result {
        Ok(material) => {
            state.composition.recipe.material = Some(material);
            state.composition.section = 4;
            state.composition.notice = Some("Shared relationship recipe bound to actual sound occurrences. Rebinding and retention are explicit.".into());
        },
        Err(error) => state.composition.notice = Some(error),
    }
}

pub(super) fn reopen(state: &mut DesktopState, material: ProjectionRecipeMaterial) {
    match validation_compiler().compile_relationship_snapshot(&material.snapshot, &material.dataset)
    {
        Ok(_) => {
            state.composition.recipe.material = Some(material);
            state.composition.section = 4;
            state.composition.notice = Some("Retained recipe reopened against its copied disclosure. Source actions still require the exact original source; rebind explicitly to use a new reading.".into());
        },
        Err(errors) => {
            state.composition.notice = Some(format!("Retained recipe refused: {errors:?}"))
        },
    }
}

fn edit(state: &mut DesktopState, spacing_delta: i32) {
    let Some(material) = state.composition.recipe.material.as_mut() else {
        return;
    };
    let mut draft = RelationshipRecipeDraft::new(material.snapshot.recipe.clone());
    let mut arrangement = material.snapshot.recipe.definition.arrangement.clone();
    arrangement.spacing = (arrangement.spacing as i64 + spacing_delta as i64).clamp(1, 256) as u32;
    draft.apply(RecipeEdit::SetArrangement(arrangement));
    match finish_draft(draft) {
        Ok(recipe) => {
            let mut candidate = material.snapshot.clone();
            candidate.recipe = recipe;
            match validation_compiler().compile_relationship_snapshot(&candidate, &material.dataset)
            {
                Ok(_) => {
                    material.snapshot = candidate;
                    state.composition.notice = Some(
                        "Shared recipe edit compiled; retain deliberately to save this version."
                            .into(),
                    );
                },
                Err(issues) => {
                    state.composition.notice = Some(format!("Arrangement refused: {issues:?}"))
                },
            }
        },
        Err(issues) => state.composition.notice = Some(format!("Recipe edit refused: {issues:?}")),
    }
}

/// An authored lens over copied disclosure, never an analysis/provider request.
/// Compile a candidate before replacing the current reading so a refused edit
/// cannot damage its selection, provenance or exact source actions.
fn relationship_category(
    material: &mut ProjectionRecipeMaterial,
    kind: Option<String>,
) -> Result<(), String> {
    if material.snapshot.recipe.relationship_kind == kind {
        return Ok(());
    }
    let mut draft = RelationshipRecipeDraft::new(material.snapshot.recipe.clone());
    draft.apply(RecipeEdit::SetRelationshipKind(kind));
    let mut candidate = material.snapshot.clone();
    candidate.recipe = finish_draft(draft)?;
    // Keep the explanation when it remains visible. Occurrence selection and
    // every source anchor remain unchanged, even when its edge is filtered out.
    if candidate.selected_relationship.as_ref().is_some_and(|id| {
        !material.dataset.relationships.iter().any(|relation| {
            &relation.id == id
                && candidate
                    .recipe
                    .relationship_kind
                    .as_ref()
                    .is_none_or(|kind| kind == &relation.kind)
        })
    }) {
        candidate.selected_relationship = None;
    }
    validation_compiler()
        .compile_relationship_snapshot(&candidate, &material.dataset)
        .map_err(|issues| format!("Relationship category refused: {issues:?}"))?;
    material.snapshot = candidate;
    Ok(())
}

fn select_category(state: &mut DesktopState, kind: Option<String>) {
    let Some(material) = state.composition.recipe.material.as_mut() else {
        return;
    };
    state.composition.notice = Some(match relationship_category(material, kind) {
        Ok(()) => "Relationship category compiled from copied disclosure; retain deliberately to save this version.".into(),
        Err(error) => error,
    });
}

pub(super) fn view(state: &DesktopState, key: crate::documents::DocKey) -> DesktopView {
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return Box::new(el(
            "section",
            (
                el("h4", "Relationship recipes"),
                span(
                    "Choose the shared relationship comparison recipe after an explicit sound reading.",
                ),
                button(
                    "Use sound relationship recipe",
                    move |state: &mut DesktopState, _| from_sound(state, key, false),
                ),
            ),
        ));
    };
    // Draw with the card measured for these labels. Until it exists the scene
    // holds only the probes: hidden for the one frame a measurement takes.
    let measured = state.composition.recipe.measured.as_ref();
    let card = measured.map_or(VALIDATION_CARD, |measured| measured.card);
    let compiled = match ProjectionCompiler::new(ItemSizes { card })
        .compile_relationship_snapshot(&material.snapshot, &material.dataset)
    {
        Ok(compiled) => compiled,
        Err(issues) => return Box::new(span(format!("Recipe cannot be realized: {issues:?}"))),
    };
    let projection = &compiled.projection;
    let categories: Vec<(String, DesktopView)> = material
        .dataset
        .relationships
        .iter()
        .map(|relation| relation.kind.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|kind| {
            let selected = material.snapshot.recipe.relationship_kind.as_ref() == Some(&kind);
            let target = kind.clone();
            (
                kind.clone(),
                Box::new(
                    button(
                        format!("Relationship category: {kind}"),
                        move |state: &mut DesktopState, _| {
                            select_category(state, Some(target.clone()))
                        },
                    )
                    .attr("aria-pressed", selected.to_string()),
                ) as DesktopView,
            )
        })
        .collect();
    let labels = drawn_labels(projection);
    let measuring = measured.is_none_or(|measured| measured.labels != labels);
    let probes: Vec<(String, DesktopView)> = if measuring {
        labels
            .iter()
            .map(|label| (label.clone(), probe(label)))
            .collect()
    } else {
        Vec::new()
    };
    let occurrence_buttons: Vec<(String, DesktopView)> = if measuring {
        Vec::new()
    } else {
        projection
            .instance_by_occurrence
            .iter()
            .map(|(id, instance)| {
                let item = &projection.scene.items[instance.0 as usize];
                let footprint = item.footprint.bounds().unwrap_or_default();
                let label = projection
                    .labels
                    .get(instance)
                    .cloned()
                    .unwrap_or_else(|| id.clone());
                let selected = material.snapshot.selected_occurrence.as_ref() == Some(id);
                let target = id.clone();
                (
                    id.clone(),
                    Box::new(
                        button(
                            format!("{label} · {id}"),
                            move |state: &mut DesktopState, _| {
                                if let Some(material) = state.composition.recipe.material.as_mut() {
                                    material.snapshot.selected_occurrence = Some(target.clone());
                                    material.snapshot.selected_relationship = None;
                                }
                            },
                        )
                        .attr("aria-pressed", selected.to_string())
                        .attr(
                            "style",
                            format!(
                                "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;",
                                item.transform.translate.x + footprint.origin.x
                                    - projection.scene.bounds.origin.x,
                                item.transform.translate.y + footprint.origin.y
                                    - projection.scene.bounds.origin.y,
                                footprint.size.w,
                                footprint.size.h
                            ),
                        ),
                    ) as DesktopView,
                )
            })
            .collect()
    };
    let relations: Vec<(String, DesktopView)> = compiled
        .relationships
        .iter()
        .map(|compiled_relation| {
            let relation = &compiled_relation.disclosure;
            let id = relation.id.clone();
            let left = relation.from_occurrence.clone();
            (
                id.clone(),
                Box::new(
                    button(
                        format!(
                            "Explain {}: {} ↔ {}",
                            relation.label, relation.from_occurrence, relation.to_occurrence
                        ),
                        move |state: &mut DesktopState, _| {
                            if let Some(material) = state.composition.recipe.material.as_mut() {
                                material.snapshot.selected_relationship = Some(id.clone());
                                material.snapshot.selected_occurrence = Some(left.clone());
                            }
                        },
                    )
                    .attr(
                        "aria-pressed",
                        (material.snapshot.selected_relationship.as_ref() == Some(&relation.id))
                            .to_string(),
                    ),
                ) as DesktopView,
            )
        })
        .collect();
    let selected_anchor = material
        .snapshot
        .selected_occurrence
        .as_ref()
        .and_then(|id| material.anchors.iter().find(|anchor| &anchor.node_id == id))
        .map(|anchor| anchor.source.clone());
    let explanation = material
        .snapshot
        .selected_relationship
        .as_ref()
        .and_then(|id| {
            compiled
                .relationships
                .iter()
                .map(|relation| &relation.disclosure)
                .find(|relation| &relation.id == id)
        })
        .map(|relation| relation.explanation.clone());
    let anchor_for_action = selected_anchor.clone();
    Box::new(el(
        "section",
        (
            el("h4", material.snapshot.recipe.definition.label.clone()),
            span(
                "This is a copied, source-bound reading, not an automatic analysis of the current selection. Source actions verify the original document; use Rebind explicitly after a new sound reading.",
            ),
            span(format!(
                "Shared Scenograph recipe · {} · spacing {} · {} distinct occurrences from {} source references",
                material.snapshot.recipe.definition.arrangement.kind,
                material.snapshot.recipe.definition.arrangement.spacing,
                material.dataset.dataset.occurrences.len(),
                material
                    .dataset
                    .dataset
                    .occurrences
                    .iter()
                    .map(|occurrence| &occurrence.source)
                    .collect::<std::collections::HashSet<_>>()
                    .len()
            )),
            el(
                "div",
                (
                    button("Increase recipe spacing", |state: &mut DesktopState, _| {
                        edit(state, 8)
                    }),
                    button("Decrease recipe spacing", |state: &mut DesktopState, _| {
                        edit(state, -8)
                    }),
                ),
            ),
            el(
                "div",
                (
                    span(
                        "Relationship categories filter this copied reading; they do not enable new analysis sources.",
                    ),
                    button(
                        "Show all disclosed relationship categories",
                        |state: &mut DesktopState, _| select_category(state, None),
                    )
                    .attr(
                        "aria-pressed",
                        material
                            .snapshot
                            .recipe
                            .relationship_kind
                            .is_none()
                            .to_string(),
                    ),
                    Keyed::new(categories),
                ),
            ),
            el(
                "div",
                el("div", (Keyed::new(occurrence_buttons), Keyed::new(probes))).attr(
                    "style",
                    format!(
                        "position:relative;height:{}px;width:{}px;",
                        // Exactly the cards' height (Mere burn plan 13.46, "Fit
                        // the scene's bounds"); a compiled recipe has two or more.
                        projection.scene.bounds.size.h,
                        projection.scene.bounds.size.w.max(180.0)
                    ),
                ),
            )
            .attr("style", "overflow:auto;max-width:100%;max-height:450px;"),
            el("div", Keyed::new(relations)),
            explanation.map(|text| span(text).attr("class", "knot-recipe-explanation")),
            selected_anchor.map(|anchor| {
                span(format!(
                    "Selected occurrence: {} · {} · bytes {}–{}",
                    anchor.exact_quote,
                    anchor.document_address,
                    anchor.byte_span.start,
                    anchor.byte_span.end
                ))
                .attr("class", "knot-recipe-source-anchor")
            }),
            button(
                "Open selected original source",
                move |state: &mut DesktopState, _| {
                    if let Some(anchor) = anchor_for_action.as_ref() {
                        super::return_to_source(state, anchor);
                    }
                },
            ),
            button(
                "Rebind recipe to current sound reading",
                move |state: &mut DesktopState, _| from_sound(state, key, true),
            ),
            button(
                "Retain relationship recipe",
                |state: &mut DesktopState, _| {
                    if let Some(material) = state.composition.recipe.material.clone() {
                        let item = CollectionItem::new(ItemKind::ProjectionRecipe, "Sound relationship recipe", "An explicitly retained relationship recipe with copied sound disclosure and exact source anchors.").with_projection_recipe(material);
                        super::collect(state, item);
                    }
                },
            ),
        ),
    ))
}

#[derive(serde::Serialize)]
struct SoundOccurrence {
    id: String,
    label: String,
    pronunciation: String,
    anchor: DocumentAnchor,
}

#[derive(serde::Serialize)]
struct ExplainedSoundRelation {
    kind: String,
    left: String,
    right: String,
    explanation: String,
}

#[derive(serde::Serialize)]
struct SoundFacts {
    occurrences: Vec<SoundOccurrence>,
    relations: Vec<ExplainedSoundRelation>,
}

fn sound_facts(source: &SelectionSnapshot, reading: &SoundReading) -> Result<SoundFacts, String> {
    if reading.selection != (source.start..source.end) {
        return Err("Sound reading no longer matches its captured selection.".into());
    }
    let occurrences = reading
        .tokens
        .iter()
        .map(|token| {
            if token.start < source.start
                || token.end > source.end
                || source.text.get(token.start..token.end) != Some(token.word.as_str())
            {
                return Err("Sound occurrence does not match the captured source bytes.".to_owned());
            }
            Ok(SoundOccurrence {
                id: format!("token-{}-{}", token.start, token.end),
                label: token.word.clone(),
                pronunciation: token
                    .selected
                    .and_then(|index| token.variants.get(index))
                    .map(|value| {
                        format!(
                            "{}{}",
                            value,
                            if token.defaulted {
                                " (first pronunciation, not author-selected)"
                            } else {
                                " (author-selected pronunciation)"
                            }
                        )
                    })
                    .unwrap_or_else(|| "unresolved pronunciation".into()),
                anchor: DocumentAnchor::capture(
                    source.address.clone(),
                    &source.text,
                    token.start..token.end,
                )
                .map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let relations = reading.relations.iter().map(|relation| {
        let left = occurrences.get(relation.left).ok_or("Sound relation has an invalid left occurrence.")?;
        let right = occurrences.get(relation.right).ok_or("Sound relation has an invalid right occurrence.")?;
        Ok(ExplainedSoundRelation {
            kind: match relation.kind {
                knot_readings::sound::SoundKind::PerfectRhyme => "sound.perfect_rhyme",
                knot_readings::sound::SoundKind::SlantRhyme => "sound.slant_rhyme",
                knot_readings::sound::SoundKind::Assonance => "sound.assonance",
                knot_readings::sound::SoundKind::Alliteration => "sound.alliteration",
            }.into(),
            left: left.id.clone(), right: right.id.clone(),
            explanation: format!("{}: {} [{}] ↔ {} [{}]. Derived by Mora 0.1.0 from the explicitly enabled CMUdict pronunciation source; not a claim about intended performance.", relation.kind.label(), left.label, left.pronunciation, right.label, right.pronunciation),
        })
    }).collect::<Result<Vec<_>, String>>()?;
    Ok(SoundFacts {
        occurrences,
        relations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_readings::sound::{self, SoundLayers};
    use std::collections::BTreeMap;

    fn fixture() -> (SelectionSnapshot, SoundReading) {
        let text = "night night light";
        let source = SelectionSnapshot {
            key: crate::documents::DocKey(1),
            address: "file:///poem.djot".into(),
            text: text.into(),
            start: 0,
            end: text.len(),
        };
        let reading = sound::analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers {
                perfect_rhyme: true,
                ..Default::default()
            },
        )
        .unwrap();
        (source, reading)
    }

    #[test]
    fn category_edit_filters_disclosure_and_roundtrips_without_changing_sources() {
        let (source, _) = fixture();
        let layers = SoundLayers {
            perfect_rhyme: true,
            assonance: true,
            ..Default::default()
        };
        let reading = sound::analyze(
            &source.text,
            0..source.text.len(),
            &BTreeMap::new(),
            &layers,
        )
        .unwrap();
        let mut material = material(&source, &reading, &layers).unwrap();
        let original_dataset = material.dataset.clone();
        let original_anchors = material.anchors.clone();
        material.snapshot.selected_occurrence = Some("token-6-11".into());
        let selected_edge = material
            .dataset
            .relationships
            .iter()
            .find(|relation| relation.kind == "sound.perfect_rhyme")
            .unwrap()
            .id
            .clone();
        material.snapshot.selected_relationship = Some(selected_edge.clone());
        let original_recipe_revision = material
            .snapshot
            .recipe
            .definition
            .provenance
            .source_revision
            .clone();
        relationship_category(&mut material, Some("sound.perfect_rhyme".into())).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        assert!(!compiled.relationships.is_empty());
        assert!(
            compiled
                .relationships
                .iter()
                .all(|relation| relation.disclosure.kind == "sound.perfect_rhyme")
        );
        assert!(compiled.relationships.len() < original_dataset.relationships.len());
        assert_eq!(
            material.snapshot.selected_occurrence.as_deref(),
            Some("token-6-11")
        );
        assert_eq!(
            material.snapshot.selected_relationship.as_ref(),
            Some(&selected_edge)
        );
        assert_ne!(
            material
                .snapshot
                .recipe
                .definition
                .provenance
                .source_revision,
            original_recipe_revision
        );
        assert_eq!(material.dataset, original_dataset);
        assert_eq!(material.anchors, original_anchors);
        let restored: ProjectionRecipeMaterial =
            serde_json::from_slice(&serde_json::to_vec(&material).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_eq!(
            restored.snapshot.recipe.relationship_kind.as_deref(),
            Some("sound.perfect_rhyme")
        );
        relationship_category(&mut material, None).unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        assert_eq!(
            compiled.relationships.len(),
            original_dataset.relationships.len()
        );
        assert_eq!(
            material.snapshot.selected_relationship.as_ref(),
            Some(&selected_edge)
        );
        relationship_category(&mut material, Some("sound.assonance".into())).unwrap();
        assert!(material.snapshot.selected_relationship.is_none());
    }

    #[test]
    fn undisclosed_category_refuses_atomically() {
        let (source, reading) = fixture();
        let mut material = material(
            &source,
            &reading,
            &SoundLayers {
                perfect_rhyme: true,
                ..Default::default()
            },
        )
        .unwrap();
        material.snapshot.selected_relationship =
            Some(material.dataset.relationships[0].id.clone());
        let before = serde_json::to_vec(&material).unwrap();
        assert!(
            relationship_category(&mut material, Some("invented.relationship".into())).is_err()
        );
        assert_eq!(serde_json::to_vec(&material).unwrap(), before);
        relationship_category(&mut material, None).unwrap();
        assert_eq!(
            serde_json::to_vec(&material).unwrap(),
            before,
            "reselecting the current category keeps its explanation"
        );
    }

    #[test]
    fn repeated_words_keep_distinct_source_occurrences_and_explained_actual_edges() {
        let (source, reading) = fixture();
        let facts = sound_facts(&source, &reading).unwrap();
        assert_eq!(facts.occurrences.len(), 3);
        assert_eq!(facts.occurrences[0].label, facts.occurrences[1].label);
        assert_ne!(facts.occurrences[0].id, facts.occurrences[1].id);
        assert_ne!(
            facts.occurrences[0].anchor.byte_span,
            facts.occurrences[1].anchor.byte_span
        );
        assert_eq!(facts.relations.len(), reading.relations.len());
        assert!(
            facts
                .relations
                .iter()
                .all(|edge| edge.explanation.contains("Derived by Mora")
                    && edge.explanation.contains("CMUdict"))
        );
    }

    #[test]
    fn altered_occurrence_or_unbound_relation_is_refused() {
        let (source, mut reading) = fixture();
        reading.tokens[0].word = "invented".into();
        assert!(sound_facts(&source, &reading).is_err());
        let (source, mut reading) = fixture();
        reading.relations[0].left = 99;
        assert!(sound_facts(&source, &reading).is_err());
    }

    #[test]
    fn mere_shared_fields_do_not_invent_a_relationship() {
        let (source, _) = fixture();
        let layers = SoundLayers::default();
        let reading = sound::analyze(
            &source.text,
            source.start..source.end,
            &BTreeMap::new(),
            &layers,
        )
        .unwrap();
        assert!(
            material(&source, &reading, &layers)
                .unwrap_err()
                .contains("requires an actual sound relationship")
        );
    }

    #[test]
    fn shared_compiler_preserves_duplicate_source_identity_and_refuses_missing_semantics() {
        let (source, reading) = fixture();
        let mut material = material(
            &source,
            &reading,
            &SoundLayers {
                perfect_rhyme: true,
                ..Default::default()
            },
        )
        .unwrap();
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&material.snapshot, &material.dataset)
            .unwrap();
        assert_eq!(compiled.projection.instance_by_occurrence.len(), 3);
        assert_eq!(
            material.dataset.dataset.occurrences[0].source,
            material.dataset.dataset.occurrences[1].source
        );
        assert_ne!(
            material.dataset.dataset.occurrences[0].occurrence_id,
            material.dataset.dataset.occurrences[1].occurrence_id
        );
        material.dataset.facets.remove("explained_relationships");
        assert!(
            validation_compiler()
                .compile_relationship_snapshot(&material.snapshot, &material.dataset)
                .is_err()
        );
    }

    #[test]
    fn typed_recipe_roundtrip_keeps_exact_selection_and_spacing() {
        let (source, reading) = fixture();
        let mut material = material(
            &source,
            &reading,
            &SoundLayers {
                perfect_rhyme: true,
                ..Default::default()
            },
        )
        .unwrap();
        material.snapshot.selected_occurrence = Some("token-6-11".into());
        let mut draft = RelationshipRecipeDraft::new(material.snapshot.recipe.clone());
        let mut arrangement = draft.recipe().definition.arrangement.clone();
        arrangement.spacing = 40;
        draft.apply(RecipeEdit::SetArrangement(arrangement));
        material.snapshot.recipe = draft.to_recipe().unwrap();
        let item = CollectionItem::new(
            ItemKind::ProjectionRecipe,
            "Recipe",
            "Copied sound comparison",
        )
        .with_projection_recipe(material.clone());
        item.validate().unwrap();
        let reopened: CollectionItem =
            serde_json::from_slice(&serde_json::to_vec(&item).unwrap()).unwrap();
        reopened.validate().unwrap();
        let reopened = reopened.projection_recipe.unwrap();
        assert_eq!(reopened, material);
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&reopened.snapshot, &reopened.dataset)
            .unwrap();
        assert_eq!(
            compiled
                .projection
                .selected
                .and_then(|instance| compiled.projection.occurrence_by_instance.get(&instance))
                .map(String::as_str),
            Some("token-6-11")
        );
    }

    #[test]
    fn analysis_revision_tracks_layer_and_pronunciation_choices_not_only_document_bytes() {
        let (source, reading) = fixture();
        let layers = SoundLayers {
            perfect_rhyme: true,
            ..Default::default()
        };
        let first = material(&source, &reading, &layers).unwrap();
        let meter_layers = SoundLayers {
            meter: true,
            ..layers
        };
        let metered = sound::analyze(
            &source.text,
            source.start..source.end,
            &BTreeMap::new(),
            &meter_layers,
        )
        .unwrap();
        let second = material(&source, &metered, &meter_layers).unwrap();
        assert_ne!(
            first.dataset.dataset.revision,
            second.dataset.dataset.revision
        );
        assert_eq!(
            first.anchors[0].source.source_hash,
            second.anchors[0].source.source_hash
        );
        let chosen = sound::analyze(
            &source.text,
            source.start..source.end,
            &BTreeMap::from([(0, 0)]),
            &layers,
        )
        .unwrap();
        let third = material(&source, &chosen, &layers).unwrap();
        assert_ne!(
            first.dataset.dataset.revision,
            third.dataset.dataset.revision
        );
        assert_eq!(
            first.anchors[0].source.source_hash,
            third.anchors[0].source.source_hash
        );
    }

    #[test]
    fn actual_sound_recipe_rebinds_to_woodshed_export_without_borrowing_owner_actions() {
        // Generated by Woodshed's own keyed-pitch-set adapter at commit
        // c92e7c96779ef316f5d8244a59de7cbc84f42a0b. Keep a repository-local
        // fixture rather than reading another app's checkout at test time.
        let music: RelationshipDataset = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/woodshed_relationships.json"
        ))
        .unwrap();
        let (source, reading) = fixture();
        let knot = material(
            &source,
            &reading,
            &SoundLayers {
                perfect_rhyme: true,
                ..Default::default()
            },
        )
        .unwrap();
        let knot_before = serde_json::to_vec(&knot).unwrap();
        let music_before = serde_json::to_vec(&music).unwrap();

        let mut edited = RelationshipRecipeDraft::new(knot.snapshot.recipe.clone());
        edited.apply(RecipeEdit::SetLabel(
            "Cross-domain relationship study".into(),
        ));
        let mut arrangement = edited.recipe().definition.arrangement.clone();
        arrangement.spacing = 40;
        edited.apply(RecipeEdit::SetArrangement(arrangement));
        let edited_recipe = finish_draft(edited).unwrap();
        let mut knot_snapshot = knot.snapshot.clone();
        knot_snapshot.recipe = edited_recipe.clone();
        validation_compiler()
            .compile_relationship_snapshot(&knot_snapshot, &knot.dataset)
            .unwrap();

        // Retain the same authored recipe, roles and edits; only bind its
        // declared input to the music owner's disclosed source and revision.
        let mut rebound = RelationshipRecipeDraft::new(edited_recipe.clone());
        rebound.apply(RecipeEdit::BindInput {
            name: "selection".into(),
            binding: ProjectionInputBinding {
                source: music.dataset.source.clone(),
                expects_generation: Some(music.dataset.revision.clone()),
                revision_evidence: RevisionEvidence::PublicGeneration,
            },
        });
        let snapshot = RelationshipSnapshot {
            recipe: finish_draft(rebound).unwrap(),
            source_name: "selection".into(),
            selected_occurrence: Some("set:1:card:2".into()),
            selected_relationship: None,
        };
        let compiled = validation_compiler()
            .compile_relationship_snapshot(&snapshot, &music)
            .unwrap();
        assert_eq!(snapshot.recipe.definition.id, edited_recipe.definition.id);
        assert_eq!(
            snapshot.recipe.definition.label,
            "Cross-domain relationship study"
        );
        assert_eq!(
            snapshot.recipe.definition.arrangement,
            edited_recipe.definition.arrangement
        );
        assert_eq!(
            snapshot.recipe.definition.encoding,
            edited_recipe.definition.encoding
        );
        assert_eq!(snapshot.recipe.definition.arrangement.spacing, 40);
        assert_eq!(
            compiled
                .projection
                .instance_by_occurrence
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["set:1:card:1", "set:1:card:2", "set:1:card:3"]
        );
        assert_eq!(
            music.dataset.occurrences[0].source,
            music.dataset.occurrences[1].source
        );
        assert_ne!(
            compiled.projection.instance_by_occurrence["set:1:card:1"],
            compiled.projection.instance_by_occurrence["set:1:card:2"]
        );
        assert_eq!(
            compiled.projection.selected,
            Some(compiled.projection.instance_by_occurrence["set:1:card:2"])
        );
        assert_eq!(compiled.relationships.len(), 1);
        let relation = &compiled.relationships[0].disclosure;
        assert_eq!(relation.kind, "music.shared_pitch_classes");
        assert_eq!(relation.provenance.provider, "woodshed");
        assert_eq!(
            relation.provenance.method,
            "woodshed.keyed_pitch_set_intersection.v1"
        );
        assert!(relation.explanation.contains("share C, E"));
        assert_eq!(relation, &music.relationships[0]);

        // Domain actions are not portable recipe data: no Knot document
        // anchors are granted to Woodshed chord occurrences by rebinding.
        let music_material = ProjectionRecipeMaterial {
            snapshot: snapshot.clone(),
            dataset: music.clone(),
            anchors: Vec::new(),
        };
        music_material.validate().unwrap();
        assert!(music_material.anchors.is_empty());
        let mut wrong_actions = music_material.clone();
        wrong_actions.anchors = knot.anchors.clone();
        assert!(wrong_actions.validate().is_err());
        let mut unsupported = music.clone();
        unsupported.facets.remove("explained_relationships");
        assert!(
            validation_compiler()
                .compile_relationship_snapshot(&snapshot, &unsupported)
                .is_err()
        );
        assert_eq!(serde_json::to_vec(&knot).unwrap(), knot_before);
        assert_eq!(serde_json::to_vec(&music).unwrap(), music_before);
    }

    /// Control widths for `card_controls_view`: (label, card width).
    static CONTROLS: std::sync::Mutex<Vec<(&'static str, f32)>> = std::sync::Mutex::new(Vec::new());

    const CONTROL_LABELS: [&str; 4] = [
        "Night · token-2-7",
        "light · token-8-13",
        "the estuary · token-64-75",
        "A considerably longer occurrence label · token-120-161",
    ];

    fn card_controls_view(_: &DesktopState) -> DesktopView {
        let probes: Vec<(String, DesktopView)> = CONTROL_LABELS
            .iter()
            .map(|label| (label.to_string(), probe(label)))
            .collect();
        let controls: Vec<(String, DesktopView)> = CONTROLS
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(k, (label, width))| {
                (
                    k.to_string(),
                    Box::new(
                        el::<_, DesktopState, ()>("button", label.to_string())
                            .attr("data-control", k.to_string())
                            .attr("aria-pressed", "true")
                            .attr(
                                "style",
                                format!(
                                    "position:absolute;left:0;top:{}px;width:{width}px;",
                                    k * 90
                                ),
                            ),
                    ) as DesktopView,
                )
            })
            .collect();
        Box::new(
            el(
                "div",
                el(
                    "section",
                    el("div", (Keyed::new(probes), Keyed::new(controls)))
                        .attr("style", "position:relative;height:1600px;"),
                )
                .attr("class", "knot-composition"),
            )
            .attr("class", "knot-workspace knot-theme-light"),
        )
    }

    #[test]
    fn a_measured_card_holds_its_label_on_one_line_and_a_narrower_one_wraps() {
        use cambium_genet_winit_host::{Harness, Init, WindowCommands, inert_hooks};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.djot");
        std::fs::write(&path, "# Note\n").unwrap();
        let state = DesktopState::with_path(
            knot_document::KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
            Some(path.clone()),
        );
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: card_controls_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: crate::fonts::bundled_fonts(),
                images: Vec::new(),
            },
            inert_hooks(),
        );
        host.layout_at(1100.0, 900.0);
        let nodes =
            |host: &Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
             attr: &str| {
                host.with_dom(|dom| {
                    fn walk(
                        dom: &genet_scripted_dom::ScriptedDom,
                        node: genet_scripted_dom::NodeId,
                        attr: &LocalName,
                        out: &mut Vec<(genet_scripted_dom::NodeId, String)>,
                    ) {
                        if let Some(value) = dom.attribute(node, &Namespace::from(""), attr) {
                            out.push((node, value.to_owned()));
                        }
                        for child in dom.dom_children(node) {
                            walk(dom, child, attr, out);
                        }
                    }
                    let mut out = Vec::new();
                    walk(dom, dom.document(), &LocalName::from(attr), &mut out);
                    out
                })
            };
        let cards: Vec<(&'static str, Size2)> = nodes(&host, MEASURE_ATTR)
            .into_iter()
            .map(|(node, label)| {
                let (_, _, width, height) = host.painted_rect(node).expect("a probe is laid out");
                assert!(
                    width > 0.0 && height > 0.0,
                    "{label} measures through generated content"
                );
                let label = CONTROL_LABELS
                    .iter()
                    .find(|known| **known == label)
                    .unwrap();
                (*label, card_for(width, height))
            })
            .collect();
        assert_eq!(cards.len(), CONTROL_LABELS.len());
        // Each card at the measured width (+1px), at the rounded probe width
        // (exact) and a pixel under that (-1px), height left to the label.
        {
            let mut controls = CONTROLS.lock().unwrap();
            for (label, card) in &cards {
                controls.push((label, card.w));
                controls.push((label, card.w - CARD_ROUNDING));
                controls.push((label, card.w - CARD_ROUNDING - 1.0));
            }
        }
        host.update(|_| {});
        let drawn = nodes(&host, "data-control");
        let height = |k: usize| {
            let node = drawn
                .iter()
                .find(|(_, value)| *value == k.to_string())
                .unwrap()
                .0;
            host.painted_rect(node).unwrap().3
        };
        let mut exact_fits = 0;
        for (i, (label, card)) in cards.iter().enumerate() {
            assert_eq!(
                height(i * 3),
                card.h,
                "{label}: the measured card holds one line"
            );
            if height(i * 3 + 1) == card.h {
                exact_fits += 1;
            }
            assert!(
                height(i * 3 + 2) > card.h,
                "{label}: a pixel under the probe wraps"
            );
        }
        println!(
            "cards {cards:?}; the rounded probe width alone fits {exact_fits} of {}",
            cards.len()
        );
    }
}
