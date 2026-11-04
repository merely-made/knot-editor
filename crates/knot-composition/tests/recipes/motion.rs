use super::*;

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
