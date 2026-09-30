//! Host-issued authority for retained composition material. Calls are blocking:
//! execute on a worker, never an input/render or store executor thread.
use crate::CollectionItem;
use knot_capture::{KnotRetainError, KnotRetainTargetV1};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionReceipt {
    pub target: KnotRetainTargetV1,
    pub item_id: String,
    pub operation: [u8; 32],
    pub already_retained: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
