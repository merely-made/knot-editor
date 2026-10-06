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
