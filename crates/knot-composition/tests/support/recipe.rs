pub fn recipe_material() -> knot_composition::retention::ProjectionRecipeMaterial {
    use knot_composition::{
        DocumentAnchor,
        retention::{ProjectionRecipeMaterial, RecipeSourceAnchor},
    };
    use scenograph::{
        ProjectionInputBinding, PublicSourceRevision, RevisionEvidence, SourceBinding,
    };
    use scenomise::projection::{
        DisclosedRelationship, ProjectionDataset, ProjectionFieldType, ProjectionOccurrence,
        ProjectionValue, RelationshipDataset, RelationshipProvenance, RelationshipSnapshot,
        relationship_recipe,
    };
    let source = SourceBinding {
        authority: "knot:test".into(),
        domain: "sound".into(),
        resource: "poem".into(),
    };
    let revision = PublicSourceRevision::new("analysis:night-light-v1");
    let occurrences = [("night:0", "Night", 0.0), ("light:6", "light", 1.0)]
        .into_iter()
        .map(|(id, label, order)| ProjectionOccurrence {
            occurrence_id: id.into(),
            source: serde_json::from_value(serde_json::json!({"adapter":"knot.document", "id":id}))
                .unwrap(),
            values: [
                ("occurrence_id".into(), ProjectionValue::Text(id.into())),
                ("label".into(), ProjectionValue::Text(label.into())),
                ("order".into(), ProjectionValue::Number(order)),
            ]
            .into(),
        })
        .collect();
    let dataset = RelationshipDataset {
        dataset: ProjectionDataset {
            source: source.clone(),
            revision: revision.clone(),
            fields: [
                ("occurrence_id".into(), ProjectionFieldType::Text),
                ("label".into(), ProjectionFieldType::Text),
                ("order".into(), ProjectionFieldType::Number),
            ]
            .into(),
            occurrences,
        },
        facets: [
            "explained_relationships".into(),
            "authored_order".into(),
            "occurrence_labels".into(),
        ]
        .into(),
        relationships: vec![DisclosedRelationship {
            id: "rhyme:night-light".into(),
            from_occurrence: "night:0".into(),
            to_occurrence: "light:6".into(),
            kind: "perfect_rhyme".into(),
            label: "Perfect rhyme".into(),
            explanation: "Retained test analysis: matching stressed vowel and coda".into(),
            provenance: RelationshipProvenance {
                source: source.clone(),
                source_revision: revision.clone(),
                method: "test.rhyme".into(),
                method_version: 1,
                provider: "synthetic fixture".into(),
                evidence: vec![
                    serde_json::from_value(
                        serde_json::json!({"adapter":"knot.document", "id":"night:0"}),
                    )
                    .unwrap(),
                ],
            },
        }],
    };
    let snapshot = RelationshipSnapshot {
        recipe: relationship_recipe(
            "recipe:poem",
            "A private rhyme recipe",
            "Knot test",
            "recipe:revision:1",
            [(
                "sound".into(),
                ProjectionInputBinding {
                    source,
                    expects_generation: Some(revision),
                    revision_evidence: RevisionEvidence::PublicGeneration,
                },
            )]
            .into(),
        ),
        source_name: "sound".into(),
        selected_occurrence: Some("night:0".into()),
        selected_relationship: Some("rhyme:night-light".into()),
    };
    ProjectionRecipeMaterial {
        snapshot,
        dataset,
        anchors: vec![
            RecipeSourceAnchor {
                node_id: "night:0".into(),
                source: DocumentAnchor::capture("document:poem", "Night light", 0..5).unwrap(),
            },
            RecipeSourceAnchor {
                node_id: "light:6".into(),
                source: DocumentAnchor::capture("document:poem", "Night light", 6..11).unwrap(),
            },
        ],
    }
}
