// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Reversible, host-owned catalog curation. No source or resident route writes.
use curation::{FoldId, FoldRecord};
use mere_view::{GraphModel, HostAction};
use std::collections::BTreeSet;

#[derive(Default)]
pub(crate) struct CatalogFolds {
    pub selecting: bool,
    pub feedback: Option<String>,
    pub selected: BTreeSet<String>,
    pub undo: Vec<Vec<FoldRecord>>,
    pub redo: Vec<Vec<FoldRecord>>,
}
impl CatalogFolds {
    pub fn actions(&self) -> Vec<HostAction> {
        [
            (
                "fold-select",
                if self.selecting {
                    "Stop selecting folds"
                } else {
                    "Select documents for fold"
                },
            ),
            ("fold-create", "Fold selected"),
            ("fold-expand", "Expand all folds"),
            ("fold-undo", "Undo fold"),
            ("fold-redo", "Redo fold"),
        ]
        .map(|(key, label)| HostAction::new(key, label))
        .into_iter()
        .collect()
    }
    pub fn selection(
        &self,
        scope: &str,
        model: &GraphModel,
        records: &[FoldRecord],
    ) -> Result<FoldRecord, String> {
        if self
            .selected
            .iter()
            .any(|key| !model.nodes.iter().any(|n| &n.key == key))
        {
            return Err("A selected catalog document is no longer present.".into());
        }
        let mut record = FoldRecord::selection(
            FoldId::new(uuid::Uuid::new_v4().to_string()),
            scope,
            self.selected.iter().cloned().collect::<Vec<_>>(),
        )
        .map_err(|e| e.to_string())?;
        if records.iter().any(|f| f.overlaps(&record)) {
            return Err("Expand an overlapping fold before folding those documents.".into());
        }
        let number = (1..)
            .find(|n| {
                !records
                    .iter()
                    .any(|f| f.label.as_deref() == Some(format!("Catalog fold {n}").as_str()))
            })
            .unwrap();
        record.label = Some(format!("Catalog fold {number}"));
        Ok(record)
    }
    pub fn record(&mut self, previous: Vec<FoldRecord>) {
        self.undo.push(previous);
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.selected.clear();
        self.selecting = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere_view::{NodeEntry, NodeState};
    #[test]
    fn selection_refuses_overlap_and_missing_members_without_changing_source() {
        let source = GraphModel {
            nodes: ["catalog:a", "catalog:b", "catalog:c"]
                .map(|key| NodeEntry {
                    key: key.into(),
                    label: key.into(),
                    state: NodeState::Available,
                })
                .to_vec(),
            relations: vec![],
        };
        let mut state = CatalogFolds::default();
        state
            .selected
            .extend(["catalog:a".into(), "catalog:b".into()]);
        let first = state.selection("one", &source, &[]).unwrap();
        assert_eq!(first.members, vec!["catalog:a", "catalog:b"]);
        assert!(state.selection("one", &source, &[first]).is_err());
        state.selected.insert("absent".into());
        assert!(state.selection("one", &source, &[]).is_err());
        assert_eq!(source.nodes.len(), 3);
    }
}

impl super::GraphState {
    pub(crate) fn fold_source_digest(&self) -> String {
        let mut hash = blake3::Hasher::new();
        if let Some(reading) = &self.reading {
            hash.update(reading.label.as_bytes());
            for node in &reading.graph.nodes {
                for value in [&node.key, &node.label] {
                    hash.update(&(value.len() as u64).to_le_bytes());
                    hash.update(value.as_bytes());
                }
            }
            for relation in &reading.graph.relations {
                for value in [&relation.key, &relation.from, &relation.to] {
                    hash.update(&(value.len() as u64).to_le_bytes());
                    hash.update(value.as_bytes());
                }
            }
        }
        hash.finalize().to_hex().to_string()
    }
}

#[cfg(test)]
mod feedback_tests {
    #[test]
    fn fold_feedback_neither_invents_nor_overwrites_a_catalog_failure() {
        let mut state = super::super::GraphState::default();
        state.folds.feedback = Some("Two catalog folds".into());
        assert_eq!(
            state.model(&[]).notice.as_deref(),
            Some("Two catalog folds")
        );
        assert!(!state.capture_facts().unwrap().failure_present);
        state.notice = Some("Catalog worker failed".into());
        assert_eq!(
            state.model(&[]).notice.as_deref(),
            Some("Catalog worker failed")
        );
        assert!(state.capture_facts().unwrap().failure_present);
    }
}
