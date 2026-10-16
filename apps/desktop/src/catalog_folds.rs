// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's catalog fold commands and preference boundary.
use super::*;
impl DesktopState {
    pub(super) fn catalog_fold_scope(&self) -> Option<String> {
        self.catalog
            .as_ref()
            .map(|catalog| format!("knot:catalog:{}", catalog.root().display()))
    }
    pub(super) fn carry_catalog_fold_request(
        &mut self,
        request: &mere_view::MereViewRequest,
    ) -> bool {
        let key = match request {
            mere_view::MereViewRequest::Activate(key) => {
                let model = self.graph.model(&self.graph_open_nodes());
                let fold = self.graph.view.folds.iter().position(|f| {
                    f.members
                        .iter()
                        .all(|member| model.graph.nodes.iter().any(|n| &n.key == member))
                        && (mere_view::fold_node_key(&f.id) == *key
                            || f.stand_in_member() == Some(key.as_str()))
                });
                if let Some(index) = fold {
                    let previous = self.graph.view.folds.clone();
                    self.graph.view.folds.remove(index);
                    self.graph.folds.record(previous);
                    self.save_catalog_folds();
                    return true;
                }
                if !self.graph.folds.selecting {
                    return false;
                }
                let model = self.graph.model(&self.graph_open_nodes());
                if model.graph.nodes.iter().any(|n| n.key == *key) {
                    if !self.graph.folds.selected.remove(key) {
                        self.graph.folds.selected.insert(key.clone());
                    }
                }
                return true;
            },
            mere_view::MereViewRequest::Host(key) if key.starts_with("fold-") => key.as_str(),
            _ => return false,
        };
        if key == "fold-select" {
            self.graph.folds.selecting = !self.graph.folds.selecting;
            return true;
        }
        let Some(scope) = self.catalog_fold_scope() else {
            self.graph.folds.feedback =
                Some("Configure a file catalog to retain catalog folds.".into());
            return true;
        };
        let previous = self.graph.view.folds.clone();
        match key {
            "fold-create" => {
                let model = self.graph.model(&self.graph_open_nodes());
                match self.graph.folds.selection(&scope, &model.graph, &previous) {
                    Ok(record) => {
                        self.graph.view.folds.push(record);
                        self.graph.folds.record(previous);
                    },
                    Err(why) => {
                        self.graph.folds.feedback = Some(why);
                        return true;
                    },
                }
            },
            "fold-expand" => {
                if previous.is_empty() {
                    return true;
                }
                self.graph.view.folds.clear();
                self.graph.folds.record(previous);
            },
            "fold-undo" => {
                let Some(old) = self.graph.folds.undo.pop() else {
                    return true;
                };
                self.graph.folds.redo.push(previous);
                self.graph.view.folds = old;
            },
            "fold-redo" => {
                let Some(next) = self.graph.folds.redo.pop() else {
                    return true;
                };
                self.graph.folds.undo.push(previous);
                self.graph.view.folds = next;
            },
            _ => return false,
        }
        self.save_catalog_folds();
        true
    }
    pub(crate) fn catalog_fold_facts(&self) -> Vec<(&'static str, String)> {
        let open = self.graph_open_nodes();
        let model = self.graph.model(&open);
        let swatch = mere_view::MereView {
            model: &model,
            state: &self.graph.view,
            leaf_key: crate::graph::LEAF_KEY,
            width: self.graph.size.0,
            height: self.graph.size.1,
        }
        .swatch();
        let boundary = swatch
            .relations
            .iter()
            .filter(|r| r.id.starts_with("curation:bundle:"))
            .collect::<Vec<_>>();
        let cells = boundary
            .iter()
            .filter_map(|r| r.label.split_whitespace().next()?.parse::<usize>().ok())
            .sum::<usize>();
        let mut places = model
            .graph
            .nodes
            .iter()
            .filter_map(|n| self.graph.view.position(&n.key).map(|p| (n.key.clone(), p)))
            .collect::<Vec<_>>();
        places.sort_by(|a, b| a.0.cmp(&b.0));
        let fold_json = serde_json::to_vec(&self.graph.view.folds).unwrap();
        vec![
            ("catalog_folds", self.graph.view.folds.len().to_string()),
            ("catalog_truth_digest", self.graph.fold_source_digest()),
            (
                "catalog_visible_nodes",
                swatch.graph.nodes.len().to_string(),
            ),
            ("catalog_fold_routes", boundary.len().to_string()),
            ("catalog_fold_cells", cells.to_string()),
            (
                "catalog_fold_selection",
                self.graph.folds.selected.len().to_string(),
            ),
            (
                "catalog_fold_digest",
                blake3::hash(&fold_json).to_hex().to_string(),
            ),
            (
                "catalog_positions_digest",
                blake3::hash(&serde_json::to_vec(&places).unwrap())
                    .to_hex()
                    .to_string(),
            ),
        ]
    }

    fn save_catalog_folds(&mut self) {
        let Some(scope) = self.catalog_fold_scope() else {
            return;
        };
        if let Some(store) = self.preferences.as_mut() {
            if let Err(why) = store.save_graph_folds(&scope, &self.graph.view.folds) {
                self.graph.folds.feedback = Some(format!(
                    "Catalog folds are visible but were not saved: {why}"
                ));
                return;
            }
        }
        self.graph.folds.feedback = Some(format!(
            "{} catalog folds. Source documents and relations are unchanged.",
            self.graph.view.folds.len()
        ));
    }
}
