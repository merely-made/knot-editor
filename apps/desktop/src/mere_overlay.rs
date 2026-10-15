//! The resident mere's overmap layer, read on a worker.
//!
//! Knot's graph is its catalog; when the launch names a resident mere's
//! domain, this worker reads Djinn's `mere/<domain>` route as `knot-editor`
//! (knot-editor `mere_overlay`) and hands the graph state the route's sessions
//! and archive. Session and archive requests come back here; graph and node
//! requests never do. The UI thread never waits on the door.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use cambium_genet_winit_host::HostWake;
use knot_editor::mere_overlay::{MereOverlay, MereOverlayReading};
use mere_view::MereViewRequest;

/// What the worker reports.
pub(crate) enum OverlayUpdate {
    /// A fresh reading of the route's sessions and archive.
    Reading(MereOverlayReading),
    /// The route declined a request, with its reason for the view's notice.
    Declined(String),
    /// The route could not be reached or read; the layer is withdrawn.
    Failed(String),
}

/// The UI side of the overmap worker.
pub(crate) struct OverlayLink {
    requests: Sender<MereViewRequest>,
    updates: Receiver<OverlayUpdate>,
    outstanding: usize,
}

impl OverlayLink {
    /// Start the worker on `mere/<domain>` at the configured first-party
    /// endpoint. It reads once on open, and again after every request.
    pub(crate) fn spawn(domain: String, wake: Option<HostWake>) -> Self {
        Self::spawn_at(None, domain, wake)
    }

    pub(crate) fn spawn_at(
        endpoint: Option<String>,
        domain: String,
        wake: Option<HostWake>,
    ) -> Self {
        let (requests, request_receiver) = mpsc::channel::<MereViewRequest>();
        let (update_sender, updates) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("knot-mere-overlay".into())
            .spawn(move || {
                let send = |update: OverlayUpdate| {
                    let delivered = update_sender.send(update).is_ok();
                    if let Some(wake) = &wake {
                        wake.wake();
                    }
                    delivered
                };
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        send(OverlayUpdate::Failed(error.to_string()));
                        return;
                    },
                };
                runtime.block_on(async move {
                    let opened = match &endpoint {
                        Some(endpoint) => MereOverlay::open_at(endpoint, &domain).await,
                        None => MereOverlay::open(&domain).await,
                    };
                    let mut overlay = match opened {
                        Ok(overlay) => overlay,
                        Err(reason) => {
                            send(OverlayUpdate::Failed(reason));
                            return;
                        },
                    };
                    let reading = overlay.read().await;
                    if !send(match reading {
                        Ok(reading) => OverlayUpdate::Reading(reading),
                        Err(reason) => OverlayUpdate::Failed(reason),
                    }) {
                        return;
                    }
                    while let Ok(request) = request_receiver.recv() {
                        match overlay.take(&request).await {
                            Ok(Some(reason)) => {
                                send(OverlayUpdate::Declined(reason));
                            },
                            Ok(None) => {},
                            Err(reason) => {
                                send(OverlayUpdate::Failed(reason));
                                return;
                            },
                        }
                        let reading = match overlay.read().await {
                            Ok(reading) => OverlayUpdate::Reading(reading),
                            Err(reason) => OverlayUpdate::Failed(reason),
                        };
                        let failed = matches!(reading, OverlayUpdate::Failed(_));
                        if !send(reading) || failed {
                            return;
                        }
                    }
                });
            });
        let outstanding = usize::from(spawned.is_ok());
        let link = Self {
            requests,
            updates,
            outstanding,
        };
        if let Err(error) = spawned {
            // No worker: report it the way a failed open is reported.
            let (sender, updates) = mpsc::channel();
            let _ = sender.send(OverlayUpdate::Failed(error.to_string()));
            return Self {
                updates,
                outstanding: 1,
                ..link
            };
        }
        link
    }

    /// Send one session or archive request to the route.
    pub(crate) fn request(&mut self, request: MereViewRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "The resident mere is no longer connected.".to_owned())?;
        self.outstanding += 1;
        Ok(())
    }

    /// A request or the opening read is still on its way.
    pub(crate) fn busy(&self) -> bool {
        self.outstanding > 0
    }

    /// Everything the worker has reported since the last drain.
    pub(crate) fn drain(&mut self) -> Vec<OverlayUpdate> {
        let mut drained = Vec::new();
        loop {
            match self.updates.try_recv() {
                Ok(update) => {
                    if matches!(update, OverlayUpdate::Reading(_) | OverlayUpdate::Failed(_)) {
                        self.outstanding = self.outstanding.saturating_sub(1);
                    }
                    if matches!(update, OverlayUpdate::Failed(_)) {
                        self.outstanding = 0;
                    }
                    drained.push(update);
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.outstanding = 0;
                    break;
                },
            }
        }
        drained
    }
}
