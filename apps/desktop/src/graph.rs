// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's host adapter for Cambium's shared mere view.
//!
//! Knot owns the catalog, the bounded reading and every resulting action. The
//! component owns presentation only; this module does not draw a graph.

use cambium_genet_winit_host::HostWake;
use knot_document::{DocumentFormat, KnotDocumentSession};
use knot_file_catalog::{KnotFileCatalogAvailability, KnotFileCatalogSnapshot, KnotFileRevisionV1};
use knot_readings::ReadingInput;
use mere_view::{
    GraphModel, HostAction, MereViewModel, MereViewState, NodeEntry, NodeState, Provenance,
    RelationEntry, StatusNote, ViewStatus,
};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(crate) const LEAF_KEY: u64 = 0x6b6e_6f74_6772_6170;
pub(crate) const REGION_ID: &str = "knot-graph";

#[derive(Clone, Debug)]
pub(crate) struct OpenNode {
    pub key: u64,
    pub catalog_id: Option<String>,
    pub label: String,
    pub dirty: bool,
}

#[derive(Clone, Debug)]
struct CatalogReading {
    graph: GraphModel,
    paths: HashMap<String, PathBuf>,
    label: String,
    errors: Vec<String>,
}

pub(crate) struct GraphState {
    pub view: MereViewState,
    reading: Option<CatalogReading>,
    receiver: Option<Receiver<CatalogReading>>,
    wake: Option<HostWake>,
    pub layout: String,
    pub size: (u32, u32),
    pub notice: Option<String>,
    configured: bool,
    source_key: Option<String>,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            view: MereViewState::default(),
            reading: None,
            receiver: None,
            wake: None,
            layout: mere_view::DEFAULT_LAYOUT.to_owned(),
            size: (900, 560),
            notice: None,
            configured: false,
            source_key: None,
        }
    }
}

impl GraphState {
    pub fn set_wake(&mut self, wake: HostWake) {
        self.wake = Some(wake);
    }

    pub fn invalidate_catalog(&mut self) {
        self.source_key = None;
    }

    pub fn start(&mut self, catalog: Option<KnotFileCatalogSnapshot>, max_bytes: usize) {
        self.configured = catalog.is_some();
        self.notice = None;
        let Some(catalog) = catalog else {
            self.receiver = None;
            self.reading = None;
            self.source_key = None;
            return;
        };
        if self.wake.is_none() {
            self.source_key = None;
            return;
        }
        let source_key = catalog
            .records()
            .iter()
            .fold(blake3::Hasher::new(), |mut digest, record| {
                digest.update(record.id.as_bytes());
                digest.update(record.relative_path.to_string_lossy().as_bytes());
                digest.update(format!("{:?}", record.availability).as_bytes());
                digest
            })
            .finalize()
            .to_hex()
            .to_string();
        if self.source_key.as_deref() == Some(&source_key)
            && (self.reading.is_some() || self.receiver.is_some())
        {
            return;
        }
        self.source_key = Some(source_key);
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.reading = None;
        let wake = self.wake.clone();
        let spawned = std::thread::Builder::new()
            .name("knot-catalog-graph".to_owned())
            .spawn(move || {
                let reading = read_catalog(catalog, max_bytes);
                let _ = sender.send(reading);
                if let Some(wake) = wake {
                    wake.wake();
                }
            });
        if spawned.is_err() {
            self.receiver = None;
            self.notice = Some("The catalog graph worker could not start.".to_owned());
        }
    }

    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn drain(&mut self) -> bool {
        let Some(receiver) = self.receiver.as_ref() else {
            return false;
        };
        match receiver.try_recv() {
            Ok(reading) => {
                self.reading = Some(reading);
                self.receiver = None;
                true
            },
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                self.notice =
                    Some("The catalog graph worker stopped before returning a reading.".to_owned());
                true
            },
        }
    }

    pub fn model(&self, open: &[OpenNode]) -> MereViewModel {
        let mut graph = self
            .reading
            .as_ref()
            .map_or_else(GraphModel::default, |reading| reading.graph.clone());
        let mut positions: HashMap<String, usize> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.key.clone(), index))
            .collect();
        for document in open {
            let key = document
                .catalog_id
                .clone()
                .unwrap_or_else(|| format!("open:{}", document.key));
            let state = if document.dirty {
                NodeState::Dirty
            } else {
                NodeState::Open
            };
            if let Some(&index) = positions.get(&key) {
                graph.nodes[index].state = state;
                graph.nodes[index].label = document.label.clone();
            } else {
                positions.insert(key.clone(), graph.nodes.len());
                graph.nodes.push(NodeEntry {
                    key,
                    label: document.label.clone(),
                    state,
                });
            }
        }
        let status = if self.receiver.is_some() {
            ViewStatus::Building(StatusNote {
                message: "Reading catalog links within the configured capture limit.".to_owned(),
                action: None,
            })
        } else if self.configured && self.reading.is_none() {
            ViewStatus::Unavailable(StatusNote {
                message: self
                    .notice
                    .clone()
                    .unwrap_or_else(|| "The catalog graph is unavailable.".to_owned()),
                action: None,
            })
        } else if graph.nodes.is_empty() {
            ViewStatus::Empty(StatusNote {
                message: if self.configured {
                    "The catalog holds no documents yet.".to_owned()
                } else {
                    "No catalog is configured and no document is open.".to_owned()
                },
                action: Some(HostAction::new("open", "Open")),
            })
        } else {
            ViewStatus::Ready
        };
        let catalog_notice = self.reading.as_ref().map(|reading| {
            let errors = if reading.errors.is_empty() {
                String::new()
            } else {
                format!(" {} document(s) could not be read.", reading.errors.len())
            };
            format!("Catalog reading {}.{errors}", reading.label)
        });
        MereViewModel {
            title: "Knot graph".to_owned(),
            status,
            sessions: Vec::new(),
            can_mint: false,
            graph,
            layout: self.layout.clone(),
            layouts: vec![
                ("spectral.default".to_owned(), "Spectral".to_owned()),
                ("grid.default".to_owned(), "Grid".to_owned()),
                ("phyllotaxis.default".to_owned(), "Spiral".to_owned()),
            ],
            actions: vec![
                HostAction::new("new", "New"),
                HostAction::new("open", "Open"),
            ],
            notice: self.notice.clone().or_else(|| {
                if self.configured {
                    catalog_notice
                } else {
                    Some("No catalog is configured; showing open documents.".to_owned())
                }
            }),
        }
    }

    pub fn path(&self, key: &str) -> Option<PathBuf> {
        self.reading.as_ref()?.paths.get(key).cloned()
    }

    pub fn set_size(&mut self, size: (u32, u32), open: &[OpenNode]) -> bool {
        if self.size == size {
            return false;
        }
        self.size = size;
        self.relayout(open);
        true
    }

    pub fn relayout(&mut self, open: &[OpenNode]) {
        let model = self.model(open);
        self.view.lay_out(&model, self.size.0, self.size.1);
    }
}

fn read_catalog(catalog: KnotFileCatalogSnapshot, max_bytes: usize) -> CatalogReading {
    let root = catalog.root().to_path_buf();
    let records = catalog.records();
    let by_path: HashMap<PathBuf, String> = records
        .iter()
        .map(|record| (record.relative_path.clone(), record.id.clone()))
        .collect();
    let mut graph = GraphModel::default();
    let mut paths = HashMap::new();
    let mut errors = Vec::new();
    let mut digest = blake3::Hasher::new();
    for record in &records {
        let label = record
            .relative_path
            .file_stem()
            .or_else(|| record.relative_path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_owned();
        graph.nodes.push(NodeEntry {
            key: record.id.clone(),
            label,
            state: match record.availability {
                KnotFileCatalogAvailability::Available => NodeState::Available,
                KnotFileCatalogAvailability::Unavailable => NodeState::Unavailable,
            },
        });
        paths.insert(record.id.clone(), root.join(&record.relative_path));
        digest.update(record.id.as_bytes());
        digest.update(record.relative_path.to_string_lossy().as_bytes());
        if record.availability == KnotFileCatalogAvailability::Unavailable {
            digest.update(b"unavailable");
            continue;
        }
        match catalog.capture_file_revision(&record.id, max_bytes) {
            Ok(revision) => {
                digest.update(blake3::hash(&revision.body).as_bytes());
                match links(&revision, &record.relative_path) {
                    Ok(links) => {
                        for (place, target) in links.into_iter().enumerate() {
                            let Some(relative) = resolve_local(&record.relative_path, &target)
                            else {
                                continue;
                            };
                            let Some(to) = by_path.get(&relative) else {
                                continue;
                            };
                            graph.relations.push(RelationEntry {
                                key: format!("{}>{to}:extracted:{place}", record.id),
                                from: record.id.clone(),
                                to: to.clone(),
                                provenance: Provenance::Extracted,
                            });
                        }
                    },
                    Err(error) => {
                        errors.push(format!("{}: {error}", record.relative_path.display()))
                    },
                }
            },
            Err(error) => {
                digest.update(error.to_string().as_bytes());
                errors.push(format!("{}: {error}", record.relative_path.display()));
            },
        }
    }
    let label = digest.finalize().to_hex()[..12].to_owned();
    CatalogReading {
        graph,
        paths,
        label,
        errors,
    }
}

fn links(revision: &KnotFileRevisionV1, path: &Path) -> Result<Vec<String>, String> {
    let Some(format) = DocumentFormat::from_path(path) else {
        return Ok(Vec::new());
    };
    let source = String::from_utf8(revision.body.clone())
        .map_err(|error| format!("source is not UTF-8: {error}"))?;
    let address = format!("catalog:{}", revision.document_id);
    let session = KnotDocumentSession::read_only_with_format(address, source, format);
    Ok(ReadingInput::from_session(&session)?
        .links
        .into_iter()
        .map(|link| link.target)
        .collect())
}

fn resolve_local(source: &Path, target: &str) -> Option<PathBuf> {
    let target = target.split(['#', '?']).next()?.trim();
    if target.is_empty() || target.contains("://") || target.starts_with("mailto:") {
        return None;
    }
    let raw = if let Some(rooted) = target.strip_prefix('/') {
        PathBuf::from(rooted)
    } else {
        source
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(target)
    };
    let mut clean = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {},
            Component::ParentDir if clean.pop() => {},
            _ => return None,
        }
    }
    Some(clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_file_catalog::KnotFileCatalog;
    use tempfile::tempdir;

    #[test]
    fn catalog_reading_is_bounded_and_keeps_unavailable_nodes() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.djot"), "[B](b.djot)").unwrap();
        std::fs::write(root.join("b.djot"), "# B").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
        let a = catalog.bind("a.djot").unwrap();
        let b = catalog.bind("b.djot").unwrap();
        std::fs::remove_file(root.join("b.djot")).unwrap();
        let reading = read_catalog(catalog.snapshot(), 1024);
        assert_eq!(reading.graph.nodes.len(), 2);
        assert!(
            reading
                .graph
                .nodes
                .iter()
                .any(|node| node.key == b && node.state == NodeState::Unavailable)
        );
        assert!(
            reading
                .graph
                .relations
                .iter()
                .any(|edge| edge.from == a && edge.to == b)
        );
        assert_eq!(reading.label.len(), 12);

        let limited = read_catalog(catalog.snapshot(), 2);
        assert_eq!(limited.errors.len(), 1);
        assert!(limited.graph.relations.is_empty());
    }

    #[test]
    fn open_nodes_merge_by_catalog_id_and_no_catalog_stays_truthful() {
        let mut state = GraphState::default();
        let open = [OpenNode {
            key: 7,
            catalog_id: None,
            label: "Draft".into(),
            dirty: true,
        }];
        let model = state.model(&open);
        assert_eq!(model.graph.nodes[0].key, "open:7");
        assert_eq!(model.graph.nodes[0].state, NodeState::Dirty);
        assert_eq!(model.status, ViewStatus::Ready);
        assert!(model.notice.as_deref().unwrap().contains("No catalog"));
        state.relayout(&open);
        assert!(state.view.position("open:7").is_some());
    }

    #[test]
    fn local_targets_cannot_escape_the_catalog() {
        assert_eq!(
            resolve_local(Path::new("notes/a.djot"), "../b.djot"),
            Some(PathBuf::from("b.djot"))
        );
        assert_eq!(resolve_local(Path::new("a.djot"), "../outside"), None);
        assert_eq!(
            resolve_local(Path::new("a.djot"), "https://example.test"),
            None
        );
    }
}
