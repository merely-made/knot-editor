// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit host-granted saved-site history. A checkpoint is separate from
//! saving source, serving a site, and retaining a composition in a persona.

use crate::{
    documents::SiteKey,
    workspace::{DesktopState, DesktopView},
};
use cambium::button;
use cambium_genet_winit_host::HostWake;
use knot_site::history::{Checkpoint, SiteHistory, SiteRevision};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// Embeddings supply their own backend, including sealed storage when needed.
pub type SiteHistoryStore = SiteHistory<Box<dyn muniment::Backend + Send + Sync>>;

#[derive(Default)]
pub(crate) struct SiteHistoryState {
    store: Option<Arc<SiteHistoryStore>>,
    receiver: Option<Receiver<Result<SiteRevision, String>>>,
    pub(crate) wake: Option<HostWake>,
}

impl SiteHistoryState {
    pub(crate) fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub(crate) fn drain(&mut self) -> Option<String> {
        let outcome = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                Err("checkpoint worker stopped before confirmation".into())
            },
        };
        self.receiver = None;
        Some(match outcome {
            Ok(revision) => format!(
                "Saved-site checkpoint {} recorded. Working files unchanged.",
                revision.sequence + 1
            ),
            Err(error) => format!("Site checkpoint failed: {error}"),
        })
    }
}

impl DesktopState {
    /// Attach optional history authority. A busy store cannot be switched until
    /// its completion is observed, so a confirmed write is not lost from status.
    pub fn set_site_history(&mut self, store: Option<SiteHistoryStore>) {
        if self.scroll.history.busy() {
            self.message =
                Some("Wait for the site checkpoint before changing history storage.".into());
            return;
        }
        self.scroll.history.store = store.map(Arc::new);
    }

    pub(crate) fn site_checkpoint_disabled(&self, site: SiteKey) -> Option<&'static str> {
        if self.scroll.site(site).is_none() {
            return Some("Open a site first.");
        }
        if self.scroll.history.store.is_none() {
            return Some("Site history storage has not been attached.");
        }
        if self.scroll.history.busy() {
            return Some("A site checkpoint is in progress.");
        }
        if !self.site_dirty_pages(site).is_empty() || !self.site_dirty_drafts(site).is_empty() {
            return Some("Save source and metadata before checkpointing the site.");
        }
        None
    }

    pub(crate) fn checkpoint_site(&mut self, site: SiteKey) {
        if let Some(reason) = self.site_checkpoint_disabled(site) {
            self.message = Some(reason.into());
            return;
        }
        let capture = (|| {
            let entry = self.scroll.site(site).ok_or("Open a site first.")?;
            let snapshot = entry.site.publication()?.to_snapshot_v1()?;
            let recorded_at_unix_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis()
                .try_into()
                .map_err(|_| "Checkpoint clock overflow")?;
            Ok::<_, String>((
                entry.site.history_id(),
                snapshot,
                Checkpoint {
                    recorded_at_unix_ms,
                    label: String::new(),
                },
            ))
        })();
        let (id, snapshot, checkpoint) = match capture {
            Ok(capture) => capture,
            Err(error) => {
                self.message = Some(format!("Site checkpoint failed: {error}"));
                return;
            },
        };
        let store = self
            .scroll
            .history
            .store
            .as_ref()
            .expect("checked history store")
            .clone();
        let wake = self.scroll.history.wake.clone();
        let (sender, receiver) = mpsc::channel();
        self.scroll.history.receiver = Some(receiver);
        self.message = Some("Recording saved-site checkpoint…".into());
        std::thread::spawn(move || {
            let outcome = tokio::runtime::Builder::new_current_thread()
                .build()
                .map_err(|e| e.to_string())
                .and_then(|runtime| {
                    runtime
                        .block_on(store.record(id, &snapshot, checkpoint))
                        .map_err(|e| e.to_string())
                });
            let _ = sender.send(outcome);
            if let Some(wake) = wake {
                wake.wake();
            }
        });
    }
}

pub(crate) fn checkpoint_button(state: &DesktopState, site: SiteKey) -> DesktopView {
    let button = button(
        "Checkpoint saved site",
        move |state: &mut DesktopState, _| state.checkpoint_site(site),
    );
    match state.site_checkpoint_disabled(site) {
        Some(reason) => Box::new(button.attr("disabled", "").attr("title", reason)),
        None => Box::new(button),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::TextCommand;
    use cambium_genet_winit_host::WindowCommands;
    use knot_document::{KnotDocumentIntentV1, KnotDocumentSession};
    use knot_site::{Site, SiteFormat, history::HistoryLimits};

    fn wait(state: &mut DesktopState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.scroll.history.busy() {
            assert!(
                std::time::Instant::now() < deadline,
                "checkpoint did not finish"
            );
            state.drain_submissions();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn checkpoint_command_is_opt_in_refuses_dirty_source_and_never_starts_serving() {
        let temp = tempfile::tempdir().unwrap();
        let site = Site::create_for(&temp.path().join("site"), SiteFormat::Gemini).unwrap();
        let index = site.page_path("index.gmi").unwrap();
        let id = site.history_id();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&index).unwrap(),
            WindowCommands::new(),
        );
        state.attach_site(site.root());
        let key = state.scroll.current_key().unwrap();
        state.checkpoint_site(key);
        assert!(
            state
                .message
                .as_ref()
                .unwrap()
                .contains("not been attached")
        );
        let backend = muniment::MemoryBackend::new();
        let inspect = SiteHistory::new(backend.clone(), HistoryLimits::default());
        state.set_site_history(Some(SiteHistory::new(
            Box::new(backend),
            HistoryLimits {
                max_revisions: 2,
                ..HistoryLimits::default()
            },
        )));
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "unsaved".into(),
            )))
            .unwrap();
        state.checkpoint_site(key);
        assert!(state.message.as_ref().unwrap().contains("Save source"));
        assert!(!state.scroll.history.busy());
        assert!(pollster::block_on(inspect.list(id)).unwrap().is_empty());
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
        state.checkpoint_site(key);
        wait(&mut state);
        assert!(
            state
                .message
                .as_ref()
                .unwrap()
                .contains("checkpoint 1 recorded")
        );
        assert_eq!(pollster::block_on(inspect.list(id)).unwrap().len(), 1);
        assert!(state.scroll.current_site().unwrap().server.is_none());
        assert_eq!(
            std::fs::read(&index).unwrap(),
            site.publication().unwrap().to_snapshot_v1().unwrap().pages["/index.gmi"].source
        );
        let open_source = state.document().snapshot().text;
        std::fs::write(&index, "# Saved by hand\r\n").unwrap();
        state.checkpoint_site(key);
        wait(&mut state);
        assert!(
            state
                .message
                .as_ref()
                .unwrap()
                .contains("checkpoint 2 recorded")
        );
        assert_eq!(state.document().snapshot().text, open_source);
        assert_eq!(
            pollster::block_on(inspect.load(id, 1))
                .unwrap()
                .unwrap()
                .pages["/index.gmi"]
                .source,
            b"# Saved by hand\r\n"
        );
        state.checkpoint_site(key);
        wait(&mut state);
        assert!(
            state
                .message
                .as_ref()
                .unwrap()
                .contains("checkpoint failed")
        );
        assert_eq!(pollster::block_on(inspect.list(id)).unwrap().len(), 2);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "another draft".into(),
            )))
            .unwrap();
        let draft = state.document().snapshot();
        pollster::block_on(inspect.load(id, 0)).unwrap().unwrap();
        assert_eq!(state.document().snapshot().text, draft.text);
        assert_eq!(state.document().snapshot().dirty, draft.dirty);
    }
}
