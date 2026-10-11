#[path = "support/recipe.rs"]
mod support;
use knot_composition::{CollectionItem, ItemKind};

#[test]
fn typed_recipe_roundtrips_and_rejects_invalid_associations_and_anchors() {
    let material = support::recipe_material();
    material.validate().unwrap();
    let item = CollectionItem::new(ItemKind::Note, "recipe", "Readable saved analysis")
        .with_projection_recipe(material.clone());
    item.validate().unwrap();
    let decoded: CollectionItem =
        serde_json::from_slice(&serde_json::to_vec(&item).unwrap()).unwrap();
    assert_eq!(decoded, item);
    let mut wrong_kind = item.clone();
    wrong_kind.kind = ItemKind::Note;
    assert!(wrong_kind.validate().is_err());
    let mut missing = item;
    missing.projection_recipe = None;
    assert!(missing.validate().is_err());
    let mut duplicate = material.clone();
    duplicate.anchors.push(duplicate.anchors[0].clone());
    assert!(duplicate.validate().is_err());
    let mut missing_node = material.clone();
    missing_node.anchors[0].node_id = "not-disclosed".into();
    assert!(missing_node.validate().is_err());
    let mut missing_hash = material.clone();
    missing_hash.anchors[0].source.source_hash = None;
    assert!(missing_hash.validate().is_err());
    let mut stale_binding = material.clone();
    stale_binding.dataset.dataset.revision = "changed".into();
    assert!(stale_binding.validate().is_err());
    let mut unknown_selection = material.clone();
    unknown_selection.snapshot.selected_occurrence = Some("missing".into());
    assert!(unknown_selection.validate().is_err());
    let mut oversized = material;
    oversized.snapshot.recipe.definition.provenance.note =
        "x".repeat(knot_composition::MAX_TEXT_BYTES);
    assert!(oversized.validate().is_err());
}

#[test]
fn scene_presentation_roundtrips_with_disclosed_occurrences() {
    use knot_composition::retention::RecipeScenePresentation;

    let mut material = support::recipe_material();
    material.presentation = Some(RecipeScenePresentation {
        overview_visible: true,
        background_visible: false,
        foreground_occurrences: ["night:0".to_owned(), "light:6".to_owned()]
            .into_iter()
            .collect(),
        pan_x: -1.25,
        pan_y: 2.5,
        zoom: 1.75,
        ..RecipeScenePresentation::default()
    });
    material.validate().unwrap();

    let bytes = serde_json::to_vec(&material).unwrap();
    let decoded: knot_composition::retention::ProjectionRecipeMaterial =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, material);
}

#[test]
fn presentation_v1_without_background_categories_remains_byte_identical() {
    use knot_composition::retention::RecipeScenePresentation;

    let bytes: &[u8] = br#"{"version":1,"overview_visible":false,"background_visible":true,"foreground_occurrences":[],"pan_x":0.0,"pan_y":0.0,"zoom":1.0}"#;
    let decoded: RecipeScenePresentation = serde_json::from_slice(bytes).unwrap();
    assert!(decoded.hidden_background_categories.is_empty());
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes.to_vec());
}

#[test]
fn presentation_v2_roundtrips_exact_disclosed_background_categories() {
    use knot_composition::retention::RecipeScenePresentation;

    let mut material = support::recipe_material();
    material.presentation = Some(RecipeScenePresentation {
        version: 2,
        hidden_background_categories: ["perfect_rhyme".to_owned()].into(),
        ..RecipeScenePresentation::default()
    });
    material.validate().unwrap();
    let bytes = serde_json::to_vec(&material).unwrap();
    let decoded: knot_composition::retention::ProjectionRecipeMaterial =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, material);
}

#[test]
fn presentation_v2_allows_an_empty_background_category_set() {
    use knot_composition::retention::RecipeScenePresentation;

    let mut material = support::recipe_material();
    material.presentation = Some(RecipeScenePresentation {
        version: 2,
        ..RecipeScenePresentation::default()
    });
    material.validate().unwrap();
}

#[test]
fn presentation_v1_refuses_nonempty_background_categories() {
    use knot_composition::retention::RecipeScenePresentation;

    let mut material = support::recipe_material();
    material.presentation = Some(RecipeScenePresentation {
        hidden_background_categories: ["perfect_rhyme".to_owned()].into(),
        ..RecipeScenePresentation::default()
    });
    assert!(material.validate().is_err());
}

#[test]
fn legacy_recipe_without_scene_presentation_remains_valid_and_omitted() {
    let material = support::recipe_material();
    assert!(material.presentation.is_none());
    material.validate().unwrap();

    let mut encoded = serde_json::to_value(&material).unwrap();
    let object = encoded.as_object_mut().unwrap();
    assert!(!object.contains_key("presentation"));

    // This is the historical shape: no presentation member at all.
    let decoded: knot_composition::retention::ProjectionRecipeMaterial =
        serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, material);
}

#[test]
fn scene_presentation_rejects_unknown_fields_versions_and_invalid_view_values() {
    use knot_composition::retention::RecipeScenePresentation;

    let base = support::recipe_material();
    let mut unknown_field = serde_json::to_value(&base).unwrap();
    unknown_field["presentation"] =
        serde_json::to_value(RecipeScenePresentation::default()).unwrap();
    unknown_field["presentation"]["provider"] = serde_json::json!("remote");
    assert!(
        serde_json::from_value::<knot_composition::retention::ProjectionRecipeMaterial>(
            unknown_field
        )
        .is_err()
    );

    let mut invalid = base.clone();
    let mut presentation = RecipeScenePresentation::default();
    presentation.version = 5;
    invalid.presentation = Some(presentation);
    assert!(invalid.validate().is_err());

    let mut invalid = base.clone();
    invalid.presentation = Some(RecipeScenePresentation {
        version: 3,
        ..Default::default()
    });
    assert!(
        invalid.validate().is_err(),
        "v3 requires its placement record"
    );

    let mut invalid = base.clone();
    let mut presentation = RecipeScenePresentation::default();
    presentation
        .foreground_occurrences
        .insert("not-disclosed".into());
    invalid.presentation = Some(presentation);
    assert!(invalid.validate().is_err());

    let mut invalid = base.clone();
    let mut presentation = RecipeScenePresentation::default();
    presentation.foreground_occurrences.insert(String::new());
    invalid.presentation = Some(presentation);
    assert!(invalid.validate().is_err());

    for (pan_x, pan_y, zoom) in [
        (f32::NAN, 0.0, 1.0),
        (0.0, f32::INFINITY, 1.0),
        (4.01, 0.0, 1.0),
        (0.0, -4.01, 1.0),
        (0.0, 0.0, f32::NAN),
        (0.0, 0.0, 0.24),
        (0.0, 0.0, 4.01),
    ] {
        let mut invalid = base.clone();
        invalid.presentation = Some(RecipeScenePresentation {
            pan_x,
            pan_y,
            zoom,
            ..RecipeScenePresentation::default()
        });
        assert!(
            invalid.validate().is_err(),
            "accepted {pan_x}, {pan_y}, {zoom}"
        );
    }

    let mut invalid = base;
    invalid.presentation = Some(RecipeScenePresentation {
        foreground_occurrences: (0..257)
            .map(|index| format!("occurrence-{index}"))
            .collect(),
        ..RecipeScenePresentation::default()
    });
    assert!(invalid.validate().is_err());
}

fn movable_scene() -> knot_composition::retention::ProjectionRecipeMaterial {
    use knot_composition::retention::RecipeScenePresentation;
    let mut material = support::recipe_material();
    material.presentation = Some(RecipeScenePresentation {
        overview_visible: true,
        ..RecipeScenePresentation::default()
    });
    material
}

#[test]
fn authored_positions_roundtrip_in_shared_trace_and_keep_zero_as_an_override() {
    let mut material = movable_scene();
    assert!(material.scene_positions().unwrap().is_empty());
    assert!(
        material
            .move_scene_occurrence("night:0", [0.25, 0.75])
            .unwrap()
    );
    assert!(
        material
            .move_scene_occurrence("night:0", [0.0, 0.0])
            .unwrap()
    );
    assert!(
        !material
            .move_scene_occurrence("night:0", [0.0, 0.0])
            .unwrap()
    );
    let positions = material.scene_positions().unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions["night:0"], [0.0, 0.0]);
    let presentation = material.presentation.as_ref().unwrap();
    assert_eq!(presentation.version, 3);
    assert_eq!(presentation.placement_trace.as_ref().unwrap().len(), 2);
    assert!(
        !serde_json::to_string(presentation)
            .unwrap()
            .contains("motion_roles")
    );
    let read: knot_composition::retention::ProjectionRecipeMaterial =
        serde_json::from_slice(&serde_json::to_vec(&material).unwrap()).unwrap();
    assert_eq!(read, material);
    assert_eq!(read.scene_positions().unwrap(), positions);
}

#[test]
fn scene_placement_refusal_preserves_exact_material() {
    let material = movable_scene();
    for (id, position) in [
        ("missing", [0.5, 0.5]),
        ("night:0", [f32::NAN, 0.5]),
        ("night:0", [0.5, f32::INFINITY]),
        ("night:0", [-0.1, 0.5]),
        ("night:0", [0.5, 1.1]),
    ] {
        let mut candidate = material.clone();
        assert!(candidate.move_scene_occurrence(id, position).is_err());
        assert_eq!(candidate, material);
    }
    let mut hidden = material.clone();
    hidden.presentation.as_mut().unwrap().background_visible = false;
    let before = hidden.clone();
    assert!(hidden.move_scene_occurrence("night:0", [0.5, 0.5]).is_err());
    assert_eq!(hidden, before);
    hidden
        .presentation
        .as_mut()
        .unwrap()
        .foreground_occurrences
        .insert("night:0".into());
    assert!(hidden.move_scene_occurrence("night:0", [0.5, 0.5]).unwrap());
    let mut filtered = material;
    let presentation = filtered.presentation.as_mut().unwrap();
    presentation.version = 2;
    presentation
        .hidden_background_categories
        .insert("perfect_rhyme".into());
    let before = filtered.clone();
    assert!(
        filtered
            .move_scene_occurrence("night:0", [0.5, 0.5])
            .is_err()
    );
    assert_eq!(filtered, before);
}

#[test]
fn authored_positions_survive_recipe_edits_but_refuse_rebound_source_anchors() {
    let mut material = movable_scene();
    material
        .move_scene_occurrence("night:0", [0.2, 0.8])
        .unwrap();
    let positions = material.scene_positions().unwrap();
    material.snapshot.recipe.definition.label = "Changed label".into();
    material.snapshot.recipe.definition.arrangement.spacing += 1;
    material.snapshot.selected_occurrence = Some("light:6".into());
    material.presentation.as_mut().unwrap().zoom = 2.0;
    assert_eq!(material.scene_positions().unwrap(), positions);
    material.anchors[0].source =
        knot_composition::DocumentAnchor::capture("document:poem", "Night light!", 0..5).unwrap();
    assert!(material.validate().is_err());
    material.presentation = None;
    material.validate().unwrap();
}

#[test]
fn placement_trace_refuses_shared_scene_edits_outside_occurrence_translation() {
    use scenotime::{Revision, SceneDiff, SceneOp, TraceStep};
    let mut material = movable_scene();
    material
        .move_scene_occurrence("night:0", [0.2, 0.8])
        .unwrap();
    let trace = material
        .presentation
        .as_ref()
        .unwrap()
        .placement_trace
        .as_ref()
        .unwrap()
        .clone();
    let head = trace.head();
    let illegal = trace
        .appended(TraceStep::diff(
            "Raise",
            SceneDiff {
                epoch: head.epoch,
                base: head.revision,
                revision: Revision(head.revision.0 + 1),
                operations: vec![SceneOp::SetItemLayer {
                    index: sceno::InstanceId(0),
                    layer: 1,
                }],
            },
        ))
        .unwrap();
    material.presentation.as_mut().unwrap().placement_trace = Some(illegal);
    assert!(material.validate().is_err());
    let mut value = head.active_item(sceno::InstanceId(0)).unwrap().clone();
    value.transform.rotate = 0.2;
    let rotated = trace
        .appended(TraceStep::diff(
            "Rotate",
            SceneDiff {
                epoch: head.epoch,
                base: head.revision,
                revision: Revision(head.revision.0 + 1),
                operations: vec![SceneOp::UpdateItem {
                    index: sceno::InstanceId(0),
                    value,
                }],
            },
        ))
        .unwrap();
    material.presentation.as_mut().unwrap().placement_trace = Some(rotated);
    assert!(material.validate().is_err());
}

#[test]
fn authored_occurrences_keep_distinct_positions_when_they_share_a_source() {
    let mut material = movable_scene();
    material.dataset.dataset.occurrences[1].source =
        material.dataset.dataset.occurrences[0].source.clone();
    material
        .move_scene_occurrence("night:0", [0.2, 0.8])
        .unwrap();
    material
        .move_scene_occurrence("light:6", [0.7, 0.1])
        .unwrap();
    let positions = material.scene_positions().unwrap();
    assert_eq!(positions["night:0"], [0.2, 0.8]);
    assert_eq!(positions["light:6"], [0.7, 0.1]);
    let base = material
        .presentation
        .as_ref()
        .unwrap()
        .placement_trace
        .as_ref()
        .unwrap()
        .base();
    assert_eq!(base.tables.sources.len(), 1);
    assert_eq!(base.tables.items.len(), 2);
}

#[test]
fn placement_trace_refuses_nonfinite_items_annotations_and_excess_steps() {
    use scenotime::{Revision, SceneDiff, SceneOp, SceneTrace, TraceStep};
    let mut material = movable_scene();
    material
        .move_scene_occurrence("night:0", [0.2, 0.8])
        .unwrap();
    let trace = material
        .presentation
        .as_ref()
        .unwrap()
        .placement_trace
        .as_ref()
        .unwrap()
        .clone();
    let original = trace.steps()[0].clone();
    let mut nonfinite = original.clone();
    let SceneOp::UpdateItem { value, .. } = &mut nonfinite.diff.as_mut().unwrap().operations[0]
    else {
        panic!("move operation")
    };
    value.transform.translate.x = f32::NAN;
    // Scenotime validates the structural scene contract; the host owns the
    // stronger normalized-number boundary.
    material.presentation.as_mut().unwrap().placement_trace =
        Some(SceneTrace::from_steps(trace.base().clone(), vec![nonfinite]).unwrap());
    assert!(material.validate().is_err());
    let annotated = original
        .clone()
        .with_annotation(serde_json::json!({"source":"other"}));
    material.presentation.as_mut().unwrap().placement_trace =
        Some(SceneTrace::from_steps(trace.base().clone(), vec![annotated]).unwrap());
    assert!(material.validate().is_err());
    let mut steps = Vec::new();
    for position in 0..257 {
        let mut step = original.clone();
        let diff = step.diff.as_mut().unwrap();
        *diff = SceneDiff {
            epoch: trace.base().epoch,
            base: Revision(position),
            revision: Revision(position + 1),
            operations: diff.operations.clone(),
        };
        steps.push(step);
    }
    material.presentation.as_mut().unwrap().placement_trace =
        Some(SceneTrace::from_steps(trace.base().clone(), steps).unwrap());
    assert!(material.validate().is_err());
    material.presentation.as_mut().unwrap().placement_trace =
        Some(trace.appended(TraceStep::mark("Select")).unwrap());
    assert!(material.validate().is_err());
}

#[test]
fn presentation_v2_bytes_stay_identical_without_placement_trace() {
    use knot_composition::retention::RecipeScenePresentation;
    let bytes: &[u8] = br#"{"version":2,"overview_visible":true,"background_visible":true,"hidden_background_categories":["perfect_rhyme"],"foreground_occurrences":[],"pan_x":0.0,"pan_y":0.0,"zoom":1.0}"#;
    let read: RecipeScenePresentation = serde_json::from_slice(bytes).unwrap();
    assert!(read.placement_trace.is_none());
    assert_eq!(serde_json::to_vec(&read).unwrap(), bytes);
}

#[test]
fn motion_roles_roundtrip_by_disclosed_occurrence_and_keep_repeated_sources_distinct() {
    use knot_composition::retention::RecipeScenePresentation;

    let mut material = support::recipe_material();
    material.dataset.dataset.occurrences[1].source =
        material.dataset.dataset.occurrences[0].source.clone();
    let copied_snapshot = material.snapshot.clone();
    let copied_dataset = material.dataset.clone();
    let copied_anchors = material.anchors.clone();
    let mut presentation = RecipeScenePresentation::default();
    presentation
        .motion_roles
        .insert("night:0".into(), seiche::Role::Anchored);
    presentation
        .motion_roles
        .insert("light:6".into(), seiche::Role::Pinned);
    presentation.refresh_version();
    assert_eq!(presentation.version, 4);
    material.presentation = Some(presentation);

    material.validate().unwrap();
    assert_eq!(material.snapshot, copied_snapshot);
    assert_eq!(material.dataset, copied_dataset);
    assert_eq!(material.anchors, copied_anchors);
    assert_eq!(
        material.presentation.as_ref().unwrap().motion_roles["night:0"],
        seiche::Role::Anchored
    );
    assert_eq!(
        material.presentation.as_ref().unwrap().motion_roles["light:6"],
        seiche::Role::Pinned
    );

    let encoded = serde_json::to_vec(&material).unwrap();
    let decoded: knot_composition::retention::ProjectionRecipeMaterial =
        serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, material);

    let mut with_trace = decoded;
    with_trace.presentation.as_mut().unwrap().overview_visible = true;
    with_trace
        .move_scene_occurrence("night:0", [0.2, 0.8])
        .unwrap();
    let presentation = with_trace.presentation.as_ref().unwrap();
    assert_eq!(presentation.version, 4);
    assert!(presentation.placement_trace.is_some());

    let mut downgraded = with_trace.clone();
    downgraded.presentation.as_mut().unwrap().version = 3;
    assert!(downgraded.validate().is_err());
}

#[test]
fn motion_roles_reject_unknown_ids_invalid_versions_and_empty_v4() {
    use knot_composition::retention::RecipeScenePresentation;

    let base = support::recipe_material();
    for occurrence_id in ["undisclosed", ""] {
        let mut invalid = base.clone();
        let mut presentation = RecipeScenePresentation::default();
        presentation
            .motion_roles
            .insert(occurrence_id.into(), seiche::Role::Pinned);
        presentation.refresh_version();
        invalid.presentation = Some(presentation);
        assert!(invalid.validate().is_err(), "accepted {occurrence_id:?}");
    }

    let mut oversized = base.clone();
    let mut presentation = RecipeScenePresentation::default();
    presentation.motion_roles = (0..257)
        .map(|index| (format!("occurrence-{index}"), seiche::Role::Seeded))
        .collect();
    presentation.refresh_version();
    oversized.presentation = Some(presentation);
    assert!(oversized.validate().is_err());

    let mut empty_v4 = base.clone();
    empty_v4.presentation = Some(RecipeScenePresentation {
        version: 4,
        ..RecipeScenePresentation::default()
    });
    assert!(empty_v4.validate().is_err());

    let mut downgraded = base;
    let mut presentation = RecipeScenePresentation::default();
    presentation
        .motion_roles
        .insert("night:0".into(), seiche::Role::Pinned);
    presentation.version = 2;
    downgraded.presentation = Some(presentation);
    assert!(downgraded.validate().is_err());
}

#[test]
fn presentation_rejects_invalid_background_categories() {
    use knot_composition::retention::RecipeScenePresentation;

    let base = support::recipe_material();
    for categories in [["undisclosed".to_owned()].into(), [String::new()].into()] {
        let mut material = base.clone();
        material.presentation = Some(RecipeScenePresentation {
            version: 2,
            hidden_background_categories: categories,
            ..RecipeScenePresentation::default()
        });
        assert!(material.validate().is_err());
    }

    let mut oversized = base;
    oversized.presentation = Some(RecipeScenePresentation {
        version: 2,
        hidden_background_categories: (0..257).map(|index| format!("category-{index}")).collect(),
        ..RecipeScenePresentation::default()
    });
    let error = oversized.validate().unwrap_err().to_string();
    assert!(error.contains("too many recipe scene background categories"));
}

/// Validation compiles with a nominal card (Mere burn plan 13.46): a recipe's
/// accept or refuse, and every issue, must not depend on the card size.
#[test]
fn validation_outcome_does_not_depend_on_card_size() {
    use knot_composition::retention::{ProjectionRecipeMaterial, VALIDATION_CARD};
    use sceno::Size2;
    use scenomise::projection::{
        ItemSizes, ProjectionCompiler, RelationshipCompileLimits, SCATTER_ARRANGEMENT_ID,
    };
    let outcome = |card: Size2, material: &ProjectionRecipeMaterial, limits| {
        ProjectionCompiler::new(ItemSizes { card })
            .compile_relationship_snapshot_with_limits(
                &material.snapshot,
                &material.dataset,
                limits,
            )
            .map(|compiled| {
                (
                    compiled.projection.scene.items.len(),
                    compiled.relationships.len(),
                )
            })
            .map_err(|issues| {
                issues
                    .into_iter()
                    .map(|issue| (issue.field, issue.message))
                    .collect::<Vec<_>>()
            })
    };
    let valid = support::recipe_material();
    let mut scatter = valid.clone();
    scatter.snapshot.recipe.definition.arrangement.kind = SCATTER_ARRANGEMENT_ID.into();
    let mut dangling = valid.clone();
    dangling.dataset.relationships[0].to_occurrence = "not-disclosed".into();
    let mut no_facets = valid.clone();
    no_facets.dataset.facets.clear();
    let default = RelationshipCompileLimits::default();
    let tight = RelationshipCompileLimits {
        max_occurrences: 1,
        ..RelationshipCompileLimits::default()
    };
    let cases = [
        (&valid, &default),
        (&scatter, &default),
        (&dangling, &default),
        (&no_facets, &default),
        (&valid, &tight),
    ];
    for (index, (material, limits)) in cases.into_iter().enumerate() {
        let reference = outcome(VALIDATION_CARD, material, limits);
        assert_eq!(reference.is_ok(), index == 0, "case {index}: {reference:?}");
        for card in [
            Size2::new(1.0, 1.0),
            Size2::new(125.0, 31.0),
            Size2::new(343.0, 31.0),
            Size2::new(1.0e6, 1.0e6),
        ] {
            assert_eq!(
                outcome(card, material, limits),
                reference,
                "case {index} at {card:?}"
            );
        }
    }
}
