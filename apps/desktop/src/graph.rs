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

pub(crate) mod diagnostics;
use apparatus::{Cursor, OperationId, RecordRef};
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
    #[cfg(test)]
    worker_control: Option<WorkerControl>,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            view: MereViewState::default(),
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
            #[cfg(test)]
            worker_control: None,
        }
    }
}

impl GraphState {
    pub fn set_wake(&mut self, wake: HostWake) {
        self.wake = Some(wake);
    }

    pub fn invalidate_catalog(&mut self) {
        self.invalidated = true;
    }

    #[cfg(test)]
    pub fn start(&mut self, catalog: Option<KnotFileCatalogSnapshot>, max_bytes: usize) {
        self.start_with_cause(catalog, max_bytes, None);
    }

    pub(crate) fn save_dispatched(&self) -> Option<RecordRef> {
        self.record(Observation::new(Phase::SaveDispatched, None, None), None)
    }

    pub(crate) fn file_write_returned(
        &self,
        cause: Option<RecordRef>,
        success: bool,
    ) -> Option<RecordRef> {
        self.record(
            Observation::new(
                Phase::FileWriteReturned,
                None,
                Some(if success {
                    ResultKind::Success
                } else {
                    ResultKind::WriteFailed
                }),
            ),
            cause,
        )
    }

    fn record(&self, payload: Observation, cause: Option<RecordRef>) -> Option<RecordRef> {
        record(self.diagnostics.as_ref(), payload, cause)
    }

    fn advance_generation(&mut self, cause: Option<RecordRef>) -> bool {
        if !self.generation_exhausted
            && let Some(next) = self.desired_generation.checked_add(1)
        {
            self.desired_generation = next;
            return true;
        }
        self.generation_exhausted = true;
        self.discard_queued(ResultKind::IdentityExhausted);
        self.reading = None;
        self.installed = None;
        self.requested_signature = None;
        self.notice = Some("The catalog graph request identity is exhausted.".to_owned());
        self.record(
            Observation::new(
                Phase::GenerationExhausted,
                None,
                Some(ResultKind::IdentityExhausted),
            ),
            cause,
        );
        false
    }

    fn discard_queued(&mut self, reason: ResultKind) {
        if let Some(request) = self.queued.take() {
            self.record(
                Observation::new(
                    Phase::QueuedSuperseded,
                    Some(request.generation),
                    Some(reason),
                ),
                request.request,
            );
        }
    }

    pub(crate) fn start_with_cause(
        &mut self,
        catalog: Option<KnotFileCatalogSnapshot>,
        max_bytes: usize,
        cause: Option<RecordRef>,
    ) {
        self.configured = catalog.is_some();
        self.notice = None;
        let Some(catalog) = catalog else {
            if !self.advance_generation(cause.clone()) {
                return;
            }
            self.record(
                Observation::new(
                    Phase::InterestInvalidated,
                    Some(self.desired_generation),
                    Some(ResultKind::InterestUnavailable),
                ),
                cause,
            );
            self.reading = None;
            self.installed = None;
            self.discard_queued(ResultKind::InterestUnavailable);
            self.requested_signature = None;
            self.invalidated = false;
            return;
        };
        if self.generation_exhausted {
            self.advance_generation(cause);
            return;
        }
        #[cfg(not(test))]
        let can_start = self.wake.is_some();
        #[cfg(test)]
        let can_start = self.wake.is_some() || self.worker_control.is_some();
        if !can_start {
            self.requested_signature = None;
            return;
        }
        let mut digest =
            catalog
                .records()
                .iter()
                .fold(blake3::Hasher::new(), |mut digest, record| {
                    digest.update(record.id.as_bytes());
                    digest.update(record.relative_path.to_string_lossy().as_bytes());
                    digest.update(format!("{:?}", record.availability).as_bytes());
                    digest
                });
        digest.update(&max_bytes.to_le_bytes());
        let signature = digest.finalize().to_hex().to_string();
        let forced = std::mem::take(&mut self.invalidated);
        if !forced
            && self.requested_signature.as_deref() == Some(&signature)
            && (self.reading.is_some() || self.receiver.is_some() || self.queued.is_some())
        {
            return;
        }
        if !self.advance_generation(cause.clone()) {
            return;
        }
        self.requested_signature = Some(signature.clone());
        let observed = self.record(
            Observation::new(Phase::CatalogRequested, Some(self.desired_generation), None),
            cause,
        );
        let request = CatalogRequest {
            catalog,
            max_bytes,
            generation: self.desired_generation,
            request: observed,
        };
        if self.receiver.is_some() {
            self.discard_queued(ResultKind::Superseded);
            self.queued = Some(request);
            return;
        }
        self.spawn(request);
    }

    fn spawn(&mut self, request: CatalogRequest) {
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.reading = None;
        self.installed = None;
        self.running = Some((request.generation, request.request.clone()));
        let failed_request = request.request.clone();
        let failed_generation = request.generation;
        let diagnostics = self.diagnostics.clone();
        let wake = self.wake.clone();
        #[cfg(test)]
        let control = self.worker_control.clone();
        let spawned = std::thread::Builder::new()
            .name("knot-catalog-graph".to_owned())
            .spawn(move || {
                let started = record(
                    diagnostics.as_ref(),
                    Observation::new(Phase::WorkerStarted, Some(request.generation), None),
                    request.request.clone(),
                );
                #[cfg(test)]
                if let Some(control) = control {
                    control.started();
                }
                let reading = read_catalog(request.catalog, request.max_bytes);
                let mut payload = Observation::new(
                    Phase::WorkerCompleted,
                    Some(request.generation),
                    Some(if reading.errors.is_empty() {
                        ResultKind::Success
                    } else {
                        ResultKind::ReadErrors
                    }),
                );
                payload.nodes = Some(reading.graph.nodes.len());
                payload.relations = Some(reading.graph.relations.len());
                payload.read_errors = Some(reading.errors.len());
                let outcome = record(diagnostics.as_ref(), payload, started);
                let _ = sender.send(CatalogCompletion {
                    reading,
                    generation: request.generation,
                    request: request.request,
                    outcome,
                });
                if let Some(wake) = wake {
                    wake.wake();
                }
            });
        if spawned.is_err() {
            self.receiver = None;
            self.running = None;
            self.record(
                Observation::new(
                    Phase::WorkerSpawnFailed,
                    Some(failed_generation),
                    Some(ResultKind::Unconfirmed),
                ),
                failed_request,
            );
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
            Ok(completion) => {
                self.receiver = None;
                self.running = None;
                if let Some(request) = self.queued.take() {
                    self.record(
                        Observation::new(
                            Phase::Discarded,
                            Some(completion.generation),
                            Some(ResultKind::Superseded),
                        ),
                        completion.outcome,
                    );
                    self.spawn(request);
                } else if self.configured
                    && !self.generation_exhausted
                    && completion.generation == self.desired_generation
                {
                    let acceptance = self.record(
                        Observation::new(
                            Phase::Accepted,
                            Some(completion.generation),
                            Some(if completion.reading.errors.is_empty() {
                                ResultKind::Success
                            } else {
                                ResultKind::ReadErrors
                            }),
                        ),
                        completion.outcome.clone(),
                    );
                    self.installed = Some(Installed {
                        generation: completion.generation,
                        request: completion.request,
                        outcome: completion.outcome,
                        acceptance,
                    });
                    self.reading = Some(completion.reading);
                } else {
                    self.record(
                        Observation::new(
                            Phase::Discarded,
                            Some(completion.generation),
                            Some(if self.generation_exhausted {
                                ResultKind::IdentityExhausted
                            } else if !self.configured {
                                ResultKind::InterestUnavailable
                            } else {
                                ResultKind::Superseded
                            }),
                        ),
                        completion.outcome,
                    );
                }
                true
            },
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                if let Some((generation, request)) = self.running.take() {
                    self.record(
                        Observation::new(
                            Phase::WorkerDisconnected,
                            Some(generation),
                            Some(ResultKind::Unconfirmed),
                        ),
                        request,
                    );
                }
                if let Some(request) = self.queued.take() {
                    self.spawn(request);
                } else {
                    self.notice = Some(
                        "The catalog graph worker stopped before returning a reading.".to_owned(),
                    );
                }
                true
            },
        }
    }

    /// Constant-size owner facts; does not clone the graph, labels or read errors.
    pub(crate) fn capture_facts(&self) -> Result<diagnostics::CaptureFacts<'_>, String> {
        Ok(diagnostics::CaptureFacts {
            configured: self.configured,
            busy: self.busy(),
            queued: self.queued.is_some(),
            desired_generation: self.desired_generation,
            generation_exhausted: self.generation_exhausted,
            installed: self.installed.as_ref(),
            catalog_nodes: self
                .reading
                .as_ref()
                .map_or(0, |reading| reading.graph.nodes.len()),
            catalog_relations: self
                .reading
                .as_ref()
                .map_or(0, |reading| reading.graph.relations.len()),
            read_errors: self
                .reading
                .as_ref()
                .map_or(0, |reading| reading.errors.len()),
            failure_present: self.notice.is_some(),
            diagnostic_admission: self
                .diagnostics
                .as_ref()
                .map(Diagnostics::stats)
                .transpose()?,
        })
    }

    pub(crate) fn diagnostic_attachment(
        &self,
    ) -> Result<Option<apparatus::Batch<serde_json::Value>>, String> {
        let Some(diagnostics) = &self.diagnostics else {
            return Ok(None);
        };
        let mut cursor = self
            .receipt_cursor
            .try_borrow_mut()
            .map_err(|_| "catalog diagnostic receipt reader unavailable")?;
        if cursor.is_none() {
            *cursor = Some(diagnostics.cursor()?);
        }
        diagnostics
            .read(cursor.as_mut().expect("reader initialized"))
            .map(Some)
    }

    #[cfg(test)]
    pub(crate) fn enable_test_diagnostics(&mut self) {
        self.diagnostics = Some(Diagnostics::new(apparatus::RetentionLimits {
            max_records: 256,
            max_bytes: 262_144,
            max_age: std::time::Duration::from_secs(60),
        }));
        self.receipt_cursor = Default::default();
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
        let status = if self.configured && self.receiver.is_some() {
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
            resolve_local(Path::new("notes/a.djot"), "b%20note.djot#section"),
            Some(PathBuf::from("notes/b note.djot"))
        );
        assert_eq!(
            resolve_local(Path::new("a.djot"), "https://example.test"),
            None
        );
    }

    #[test]
    fn overlapping_refreshes_run_one_worker_and_coalesce_one_latest_request() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.djot"), "# A").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
        catalog.bind("a.djot").unwrap();
        let snapshot = catalog.snapshot();
        let control = WorkerControl::default();
        let mut state = GraphState {
            worker_control: Some(control.clone()),
            ..GraphState::default()
        };
        state.enable_test_diagnostics();

        state.start(Some(snapshot.clone()), 64);
        control.wait_for_starts(1);
        for limit in [32, 16, 8] {
            state.invalidate_catalog();
            state.start(Some(snapshot.clone()), limit);
        }
        assert_eq!(
            control.starts(),
            1,
            "queued refreshes spawned overlapping workers"
        );

        control.release();
        for _ in 0..200 {
            if state.drain() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        control.wait_for_starts(2);
        assert_eq!(control.starts(), 2, "more than one queued refresh survived");
        control.release();
        for _ in 0..200 {
            if state.drain() && !state.busy() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!state.busy());
        assert!(state.reading.is_some());
        assert_eq!(control.starts(), 2);
        let batch = state.diagnostic_attachment().unwrap().unwrap();
        let phase_generations = |phase: &str| {
            batch
                .records
                .iter()
                .filter(|record| record.payload["phase"] == phase)
                .map(|record| record.payload["generation"].as_u64().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(phase_generations("catalog_requested"), [1, 2, 3, 4]);
        assert_eq!(phase_generations("worker_started"), [1, 4]);
        assert_eq!(phase_generations("queued_superseded"), [2, 3]);
        assert_eq!(phase_generations("discarded"), [1]);
        assert_eq!(phase_generations("accepted"), [4]);
        let installed = state.installed.as_ref().unwrap();
        assert_eq!(installed.generation, 4);
        let accepted = batch
            .records
            .iter()
            .find(|record| record.payload["phase"] == "accepted")
            .unwrap();
        assert_eq!(
            installed.acceptance.as_ref(),
            Some(&accepted.envelope.reference)
        );
        assert_eq!(accepted.envelope.metadata.cause, installed.outcome);
    }

    fn wait_for_drain(state: &mut GraphState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !state.drain() {
            assert!(
                std::time::Instant::now() < deadline,
                "real catalog worker did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[test]
    fn worker_execution_is_not_installed_early_and_invalidated_interest_discards_it() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("files");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("private-source.djot"), "private text").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
        catalog.bind("private-source.djot").unwrap();
        let control = WorkerControl::default();
        let mut state = GraphState {
            worker_control: Some(control.clone()),
            ..Default::default()
        };
        state.enable_test_diagnostics();
        state.start(Some(catalog.snapshot()), 1024);
        control.wait_for_starts(1);
        assert!(
            state.capture_facts().unwrap().installed.is_none(),
            "worker dispatch was installed before its outcome"
        );
        control.release();
        let diagnostics = state.diagnostics.clone().unwrap();
        let mut cursor = diagnostics.cursor().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if diagnostics
                .read(&mut cursor)
                .unwrap()
                .records
                .iter()
                .any(|record| record.payload["phase"] == "worker_completed")
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            state.capture_facts().unwrap().installed.is_none(),
            "worker completion was treated as host acceptance"
        );
        state.start(None, 1024);
        wait_for_drain(&mut state);
        assert!(
            state.reading.is_none(),
            "invalidated worker result was installed"
        );
        assert!(state.capture_facts().unwrap().installed.is_none());
        let batch = state.diagnostic_attachment().unwrap().unwrap();
        assert!(
            batch
                .records
                .iter()
                .any(|record| record.payload["phase"] == "discarded"
                    && record.payload["result"] == "interest_unavailable")
        );
        assert!(
            !batch
                .records
                .iter()
                .any(|record| record.payload["phase"] == "accepted")
        );
        assert!(!serde_json::to_string(&batch).unwrap().contains("private"));
    }

    #[test]
    fn exhausted_generation_never_aliases_a_request_or_installs_the_pending_result() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("files");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.djot"), "# A").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
        catalog.bind("a.djot").unwrap();
        let control = WorkerControl::default();
        let mut state = GraphState {
            desired_generation: u64::MAX - 1,
            worker_control: Some(control.clone()),
            ..Default::default()
        };
        state.enable_test_diagnostics();
        state.start(Some(catalog.snapshot()), 1024);
        control.wait_for_starts(1);
        state.invalidate_catalog();
        state.start(Some(catalog.snapshot()), 1024);
        assert!(state.generation_exhausted);
        assert_eq!(state.desired_generation, u64::MAX);
        assert!(state.queued.is_none());
        control.release();
        wait_for_drain(&mut state);
        assert!(state.reading.is_none());
        assert!(state.installed.is_none());
        assert_eq!(control.starts(), 1);
        let batch = state.diagnostic_attachment().unwrap().unwrap();
        assert_eq!(
            batch
                .records
                .iter()
                .filter(|record| record.payload["phase"] == "catalog_requested")
                .count(),
            1
        );
        assert!(
            batch
                .records
                .iter()
                .any(|record| record.payload["phase"] == "generation_exhausted")
        );
        assert!(
            !batch
                .records
                .iter()
                .any(|record| record.payload["phase"] == "accepted")
        );
    }

    #[test]
    fn read_errors_are_an_explicit_real_worker_outcome_and_the_projection_is_frozen() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("files");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("private-source.djot"),
            "private text beyond the budget",
        )
        .unwrap();
        let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
        catalog.bind("private-source.djot").unwrap();
        let control = WorkerControl::default();
        let mut state = GraphState {
            worker_control: Some(control.clone()),
            ..Default::default()
        };
        state.enable_test_diagnostics();
        state.start(Some(catalog.snapshot()), 1);
        control.wait_for_starts(1);
        control.release();
        wait_for_drain(&mut state);
        let frozen = serde_json::to_value(state.capture_facts().unwrap()).unwrap();
        assert_eq!(frozen["read_errors"], 1);
        assert_eq!(frozen["catalog_nodes"], 1);
        assert_eq!(frozen["catalog_relations"], 0);
        let batch = state.diagnostic_attachment().unwrap().unwrap();
        assert!(
            batch
                .records
                .iter()
                .any(|record| record.payload["phase"] == "worker_completed"
                    && record.payload["result"] == "read_errors")
        );
        assert!(
            batch
                .records
                .iter()
                .any(|record| record.payload["phase"] == "accepted"
                    && record.payload["result"] == "read_errors")
        );
        assert!(!serde_json::to_string(&frozen).unwrap().contains("private"));
        state.start(None, 1);
        assert!(state.capture_facts().unwrap().installed.is_none());
        assert!(frozen["installed"].is_object());
    }
}
