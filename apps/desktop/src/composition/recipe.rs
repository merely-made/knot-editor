// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot supplies source facts and exact document actions. Shared Scenograph and
//! Scenomise own the authoring contract and executable projection compiler.

use super::SelectionSnapshot;
use crate::workspace::{DesktopState, DesktopView};
use cambium::{Keyed, button, el, span};
use knot_composition::DocumentAnchor;
use knot_composition::retention::{ProjectionRecipeMaterial, RecipeSourceAnchor};
use knot_composition::{CollectionItem, ItemKind};
use knot_readings::sound::SoundReading;
use scenograph::relationship::{
    RecipeEdit, RelationshipRecipeDraft, RelationshipSnapshot, relationship_recipe,
};
use scenograph::{ProjectionInputBinding, PublicSourceRevision, RevisionEvidence, SourceBinding};
use scenomise::projection::{
    DisclosedRelationship, ProjectionDataset, ProjectionFieldType, ProjectionOccurrence,
    ProjectionValue, RelationshipDataset, RelationshipProvenance, compile_relationship_snapshot,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct RecipeState {
    pub(super) material: Option<ProjectionRecipeMaterial>,
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
    compile_relationship_snapshot(&material.snapshot, &material.dataset)
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
            compile_relationship_snapshot(&material.snapshot, &material.dataset)
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
    match compile_relationship_snapshot(&material.snapshot, &material.dataset) {
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
            match compile_relationship_snapshot(&candidate, &material.dataset) {
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
    let compiled = match compile_relationship_snapshot(&material.snapshot, &material.dataset) {
        Ok(compiled) => compiled,
        Err(issues) => return Box::new(span(format!("Recipe cannot be realized: {issues:?}"))),
    };
    let projection = &compiled.projection;
    let occurrence_buttons: Vec<(String, DesktopView)> = projection
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
        .collect();
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
                el("div", Keyed::new(occurrence_buttons)).attr(
                    "style",
                    format!(
                        "position:relative;height:{}px;width:{}px;",
                        projection.scene.bounds.size.h.max(90.0),
                        projection.scene.bounds.size.w.max(180.0)
                    ),
                ),
            )
            .attr("style", "overflow:auto;max-width:100%;max-height:450px;"),
            el("div", Keyed::new(relations)),
            explanation.map(span),
            selected_anchor.map(|anchor| {
                span(format!(
                    "Selected occurrence: {} · {} · bytes {}–{}",
                    anchor.exact_quote,
                    anchor.document_address,
                    anchor.byte_span.start,
                    anchor.byte_span.end
                ))
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
        let compiled =
            compile_relationship_snapshot(&material.snapshot, &material.dataset).unwrap();
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
        assert!(compile_relationship_snapshot(&material.snapshot, &material.dataset).is_err());
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
        let compiled =
            compile_relationship_snapshot(&reopened.snapshot, &reopened.dataset).unwrap();
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
        compile_relationship_snapshot(&knot_snapshot, &knot.dataset).unwrap();

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
        let compiled = compile_relationship_snapshot(&snapshot, &music).unwrap();
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
        assert!(compile_relationship_snapshot(&snapshot, &unsupported).is_err());
        assert_eq!(serde_json::to_vec(&knot).unwrap(), knot_before);
        assert_eq!(serde_json::to_vec(&music).unwrap(), music_before);
    }
}
