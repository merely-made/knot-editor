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
    presentation.version = 3;
    invalid.presentation = Some(presentation);
    assert!(invalid.validate().is_err());

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
