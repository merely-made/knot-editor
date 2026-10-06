//! Host-issued authority for retained composition material. Calls are blocking:
//! execute on a worker, never an input/render or store executor thread.
use crate::{CollectionError, CollectionItem, DocumentAnchor};
use knot_capture::{KnotRetainError, KnotRetainTargetV1};
use serde::{Deserialize, Serialize};

/// The card validation compiles with. Never drawn: a recipe's accept or refuse
/// does not depend on its card size, and the desktop draws with the card it
/// measures from its own font (Mere burn plan 13.46, "Named 164x68 constant").
pub const VALIDATION_CARD: sceno::Size2 = sceno::Size2 { w: 164.0, h: 68.0 };

/// The compiler that decides whether a recipe is valid, without drawing it.
pub fn validation_compiler() -> &'static scenomise::projection::ProjectionCompiler {
    static COMPILER: std::sync::OnceLock<scenomise::projection::ProjectionCompiler> =
        std::sync::OnceLock::new();
    COMPILER.get_or_init(|| {
        scenomise::projection::ProjectionCompiler::new(scenomise::projection::ItemSizes {
            card: VALIDATION_CARD,
        })
    })
}

/// A copied, inspectable projection recipe and its disclosed analysis input.
/// No runtime witness, execution handle, or source-acquisition authority is saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionRecipeMaterial {
    pub snapshot: scenograph::relationship::RelationshipSnapshot,
    pub dataset: scenomise::projection::RelationshipDataset,
    pub anchors: Vec<RecipeSourceAnchor>,
}

/// An occurrence-specific source quotation, not a label-based lookup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeSourceAnchor {
    pub node_id: String,
    pub source: DocumentAnchor,
}

impl ProjectionRecipeMaterial {
    pub fn validate(&self) -> Result<(), CollectionError> {
        if self.anchors.len() > 1024 {
            return Err(CollectionError::Invalid(
                "too many recipe source anchors".into(),
            ));
        }
        let bytes = serde_json::to_vec(self)
            .map_err(|error| CollectionError::Invalid(error.to_string()))?;
        if bytes.len() > crate::MAX_TEXT_BYTES {
            return Err(CollectionError::Invalid(
                "projection recipe material exceeds one MiB".into(),
            ));
        }
        validation_compiler()
            .compile_relationship_snapshot(&self.snapshot, &self.dataset)
            .map_err(|issues| {
                CollectionError::Invalid(format!("invalid projection recipe: {issues:?}"))
            })?;
        let mut seen = std::collections::HashSet::new();
        for anchor in &self.anchors {
            if !seen.insert(&anchor.node_id)
                || !self
                    .dataset
                    .dataset
                    .occurrences
                    .iter()
                    .any(|occurrence| occurrence.occurrence_id == anchor.node_id)
            {
                return Err(CollectionError::Invalid(
                    "recipe source anchor is duplicate or not disclosed".into(),
                ));
            }
            anchor
                .source
                .validate_shape()
                .map_err(CollectionError::Anchor)?;
            if anchor.source.source_hash.is_none() {
                return Err(CollectionError::Invalid(
                    "recipe source anchor requires a full source hash".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionReceipt {
    pub target: KnotRetainTargetV1,
    pub item_id: String,
    pub operation: [u8; 32],
    pub already_retained: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetainedCompositionItem {
    pub item: CollectionItem,
    /// Historical operation signer; may be another admitted device.
    pub author: [u8; 32],
    pub receipt: CompositionReceipt,
}

/// An explicitly supplied capability, not a request to open or choose a persona.
pub trait CompositionRetainPort: Send + Sync {
    fn target(&self) -> &KnotRetainTargetV1;
    fn retain(
        &self,
        expected_target: &KnotRetainTargetV1,
        item: CollectionItem,
    ) -> Result<CompositionReceipt, KnotRetainError>;
    fn list(
        &self,
        expected_target: &KnotRetainTargetV1,
    ) -> Result<Vec<RetainedCompositionItem>, KnotRetainError>;
}
