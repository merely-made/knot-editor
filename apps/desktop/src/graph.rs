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
    RelationEntry, SessionPlacement, StatusNote, ViewStatus,
};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(crate) mod diagnostics;
pub(crate) mod folds;
use apparatus_core::{Cursor, OperationId, RecordRef};
use diagnostics::{Diagnostics, Installed, Observation, Phase, ResultKind};

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
    label: String,
    errors: Vec<String>,
}

struct CatalogRequest {
    catalog: KnotFileCatalogSnapshot,
    max_bytes: usize,
    generation: u64,
    request: Option<RecordRef>,
}

struct CatalogCompletion {
    reading: CatalogReading,
    generation: u64,
    request: Option<RecordRef>,
    outcome: Option<RecordRef>,
}

#[cfg(test)]
#[derive(Clone, Default)]
struct WorkerControl {
    state: std::sync::Arc<(std::sync::Mutex<(usize, usize)>, std::sync::Condvar)>,
}

#[cfg(test)]
impl WorkerControl {
    fn started(&self) {
        let (lock, changed) = &*self.state;
        let mut state = lock.lock().unwrap();
        state.0 += 1;
        changed.notify_all();
        while state.1 == 0 {
            state = changed.wait(state).unwrap();
        }
        state.1 -= 1;
    }

    fn wait_for_starts(&self, expected: usize) {
        let (lock, changed) = &*self.state;
        let state = lock.lock().unwrap();
        let (state, timeout) = changed
            .wait_timeout_while(state, std::time::Duration::from_secs(2), |state| {
                state.0 < expected
            })
            .unwrap();
        assert!(!timeout.timed_out(), "worker {expected} did not start");
        assert_eq!(state.0, expected);
    }

    fn release(&self) {
        let (lock, changed) = &*self.state;
        let mut state = lock.lock().unwrap();
        state.1 += 1;
        changed.notify_all();
    }

    fn starts(&self) -> usize {
        self.state.0.lock().unwrap().0
    }
}

pub(crate) struct GraphState {
    pub view: MereViewState,
    pub folds: folds::CatalogFolds,
    reading: Option<CatalogReading>,
    receiver: Option<Receiver<CatalogCompletion>>,
    queued: Option<CatalogRequest>,
    wake: Option<HostWake>,
    pub layout: String,
    pub size: (u32, u32),
    pub notice: Option<String>,
    configured: bool,
    requested_signature: Option<String>,
    desired_generation: u64,
    generation_exhausted: bool,
    installed: Option<Installed>,
    diagnostics: Option<Diagnostics>,
    receipt_cursor: std::cell::RefCell<Option<Cursor>>,
    running: Option<(u64, Option<RecordRef>)>,
    invalidated: bool,
    /// The resident mere's overmap layer: its worker, and its last reading.
    #[cfg(feature = "mere-retention")]
    overlay: Option<crate::mere_overlay::OverlayLink>,
    #[cfg(feature = "mere-retention")]
    overlay_reading: Option<knot_editor::mere_overlay::MereOverlayReading>,
    #[cfg(test)]
    worker_control: Option<WorkerControl>,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            view: MereViewState::default(),
            folds: Default::default(),
            reading: None,
            receiver: None,
            queued: None,
            wake: None,
            layout: mere_view::DEFAULT_LAYOUT.to_owned(),
            size: (900, 560),
            notice: None,
            configured: false,
            requested_signature: None,
            desired_generation: 0,
            generation_exhausted: false,
            installed: None,
            diagnostics: Diagnostics::from_env(),
            receipt_cursor: Default::default(),
            running: None,
            invalidated: false,
            #[cfg(feature = "mere-retention")]
            overlay: None,
            #[cfg(feature = "mere-retention")]
            overlay_reading: None,
            #[cfg(test)]
            worker_control: None,
        }
    }
}

fn record(
    diagnostics: Option<&Diagnostics>,
    payload: Observation,
    cause: Option<RecordRef>,
) -> Option<RecordRef> {
    let operation = if matches!(
        payload.phase,
        Phase::InterestInvalidated | Phase::GenerationExhausted
    ) {
        None
    } else {
        payload
            .generation
            .map(|generation| OperationId(format!("catalog:{generation}")))
    };
    diagnostics.and_then(|diagnostics| diagnostics.record(payload, operation, cause))
}

fn read_catalog(catalog: KnotFileCatalogSnapshot, max_bytes: usize) -> CatalogReading {
    let records = catalog.records();
    let by_path: HashMap<PathBuf, String> = records
        .iter()
        .map(|record| (record.relative_path.clone(), record.id.clone()))
        .collect();
    let mut graph = GraphModel::default();
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
    let target = percent_encoding::percent_decode_str(target)
        .decode_utf8()
        .ok()?;
    let raw = if let Some(rooted) = target.strip_prefix('/') {
        PathBuf::from(rooted)
    } else {
        source
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(target.as_ref())
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
mod tests;

/// The view's notice for a session or archive request with no resident mere.
fn declined_without_resident(request: &mere_view::MereViewRequest) -> String {
    match request {
        mere_view::MereViewRequest::Archive(_) => {
            "Archives come from a resident mere; open Knot with Djinn's mere to use them."
        },
        mere_view::MereViewRequest::Merge(_) => {
            "Merges come from a resident mere; open Knot with Djinn's mere to use them."
        },
        _ => "Sessions come from a resident mere; open Knot with Djinn's mere to use them.",
    }
    .to_owned()
}

mod operations;
