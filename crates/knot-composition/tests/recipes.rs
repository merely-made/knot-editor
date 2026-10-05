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
