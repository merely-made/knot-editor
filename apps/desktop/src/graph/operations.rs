// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

    pub(super) fn record(
        &self,
        payload: Observation,
        cause: Option<RecordRef>,
    ) -> Option<RecordRef> {
        record(self.diagnostics.as_ref(), payload, cause)
    }

    pub(super) fn advance_generation(&mut self, cause: Option<RecordRef>) -> bool {
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

    pub(super) fn discard_queued(&mut self, reason: ResultKind) {
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

    pub(super) fn spawn(&mut self, request: CatalogRequest) {
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
        self.receiver.is_some() || self.overlay_busy()
    }

    pub fn drain(&mut self) -> bool {
        let catalog = self.drain_catalog();
        self.drain_overlay() || catalog
    }

    /// Read the resident mere on `mere/<domain>` as the overmap layer.
    #[cfg(feature = "mere-retention")]
    pub(crate) fn attach_overlay(&mut self, link: crate::mere_overlay::OverlayLink) {
        self.overlay = Some(link);
        self.overlay_reading = None;
    }

    /// Send a session or archive request to the overmap layer, or decline it
    /// with the view's notice when no resident mere is attached.
    pub fn overlay_request(&mut self, request: mere_view::MereViewRequest) {
        #[cfg(feature = "mere-retention")]
        if let Some(link) = self.overlay.as_mut() {
            if let Err(reason) = link.request(request) {
                self.overlay = None;
                self.overlay_reading = None;
                self.notice = Some(reason);
            }
            return;
        }
        self.notice = Some(declined_without_resident(&request));
    }

    pub(super) fn overlay_busy(&self) -> bool {
        #[cfg(feature = "mere-retention")]
        if let Some(link) = &self.overlay {
            return link.busy();
        }
        false
    }

    pub(super) fn drain_overlay(&mut self) -> bool {
        #[cfg(feature = "mere-retention")]
        if let Some(link) = self.overlay.as_mut() {
            use crate::mere_overlay::OverlayUpdate;
            let updates = link.drain();
            let changed = !updates.is_empty();
            for update in updates {
                match update {
                    OverlayUpdate::Reading(reading) => self.overlay_reading = Some(reading),
                    OverlayUpdate::Declined(reason) => self.notice = Some(reason),
                    OverlayUpdate::Failed(reason) => {
                        self.overlay = None;
                        self.overlay_reading = None;
                        self.notice = Some(format!("The resident mere is unavailable: {reason}"));
                        break;
                    },
                }
            }
            return changed;
        }
        false
    }

    pub(super) fn apply_overlay(&self, model: &mut MereViewModel) {
        #[cfg(feature = "mere-retention")]
        knot_editor::mere_overlay::overlay(model, self.overlay_reading.as_ref());
        #[cfg(not(feature = "mere-retention"))]
        {
            model.hide_graph = false;
            model.sessions.clear();
            model.can_mint = false;
            model.archive = None;
        }
    }

    pub(super) fn drain_catalog(&mut self) -> bool {
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
    ) -> Result<Option<apparatus_core::Batch<serde_json::Value>>, String> {
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
        self.diagnostics = Some(Diagnostics::new(apparatus_core::RetentionLimits {
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
        let mut model = MereViewModel {
            title: "Knot graph".to_owned(),
            status,
            sessions: Vec::new(),
            can_mint: false,
            hide_graph: false,
            archive: None,
            // A pending merge is the resident mere's; the overlay sets it.
            merge: None,
            // A resident mere's sessions show as the overmap tier on the
            // catalog's own canvas, one level up from its documents, never as
            // the catalog's own sessions (Mark, 2026-10-15).
            placement: SessionPlacement::Tier,
            // Mere's default: three codicils on a session before "+N".
            max_archive_nodes: None,
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
            notice: self
                .notice
                .clone()
                .or_else(|| self.folds.feedback.clone())
                .or_else(|| {
                    if self.configured {
                        catalog_notice
                    } else {
                        Some("No catalog is configured; showing open documents.".to_owned())
                    }
                }),
        };
        self.apply_overlay(&mut model);
        if self.configured {
            model.actions.extend(self.folds.actions());
        }
        if self.folds.selecting {
            for node in &mut model.graph.nodes {
                if self.folds.selected.contains(&node.key) {
                    node.label.push_str(", selected for fold");
                }
            }
            model.notice = Some(format!(
                "{} catalog documents selected. Activate documents to toggle selection, then Fold selected.",
                self.folds.selected.len()
            ));
        }
        model
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
