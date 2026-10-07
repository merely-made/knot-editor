// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! One bounded writer for document recovery. Source copies leave the UI thread
//! through a bounded channel; the writer coalesces edits before disk IO.

use crate::recovery::{
    RecoveryId, RecoveryIssue, RecoveryRecord, RecoveryRetention, RecoveryStore,
};
use cambium_genet_winit_host::HostWake;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};

enum Command {
    Upsert(RecoveryRecord),
    Remove(RecoveryId, SyncSender<Result<(), String>>),
    Flush(SyncSender<Result<(), String>>),
    Retention(RecoveryRetention, SyncSender<Result<(), String>>),
    Claim(
        RecoveryId,
        SyncSender<Result<Option<RecoveryRecord>, String>>,
    ),
}

pub(crate) struct RecoveryRuntime {
    pub records: Vec<RecoveryRecord>,
    pub issues: Vec<RecoveryIssue>,
    pub session_id: String,
    sender: Option<SyncSender<Command>>,
    #[cfg(test)]
    writer_handle: Option<std::thread::JoinHandle<()>>,
    results: Receiver<(Option<RecoveryId>, String)>,
    deferred: BTreeMap<RecoveryId, RecoveryRecord>,
    local_error: Option<String>,
}

impl RecoveryRuntime {
    pub fn start(
        root: PathBuf,
        retention: RecoveryRetention,
        wake: HostWake,
    ) -> Result<Self, String> {
        let store = RecoveryStore::open(root, retention).map_err(|error| error.to_string())?;
        let listing = store.list();
        let complete = listing.complete;
        let records = listing
            .records
            .into_iter()
            .filter(|record| !store.is_session_live(&record.session_id))
            .collect();
        let session_id = store.session_id().to_owned();
        let (sender, receiver) = mpsc::sync_channel(8);
        let (result_sender, results) = mpsc::channel();
        let writer_handle = std::thread::Builder::new()
            .name("knot-recovery-writer".into())
            .spawn(move || writer(store, receiver, result_sender, wake))
            .map_err(|error| format!("could not start recovery writer: {error}"))?;
        #[cfg(not(test))]
        let _ = writer_handle;
        Ok(Self {
            records,
            issues: listing.issues,
            session_id,
            sender: Some(sender),
            #[cfg(test)]
            writer_handle: Some(writer_handle),
            results,
            deferred: BTreeMap::new(),
            local_error: (!complete).then(|| "Recovery inspection stopped at its safety limit; some copies may not be listed.".into()),
        })
    }

    pub fn offer(&mut self, record: RecoveryRecord) -> bool {
        let id = record.id;
        match self
            .sender
            .as_ref()
            .expect("active recovery writer")
            .try_send(Command::Upsert(record))
        {
            Ok(()) => {
                self.deferred.remove(&id);
                true
            },
            Err(TrySendError::Full(Command::Upsert(record))) => {
                let total: usize = self
                    .deferred
                    .values()
                    .map(|item| item.source_text.len())
                    .sum();
                if (self.deferred.len() < 20 || self.deferred.contains_key(&id))
                    && total
                        .saturating_sub(
                            self.deferred
                                .get(&id)
                                .map_or(0, |old| old.source_text.len()),
                        )
                        .saturating_add(record.source_text.len())
                        <= 100 * 1024 * 1024
                {
                    self.deferred.insert(id, record);
                    true
                } else {
                    self.local_error = Some(
                        "Document recovery queue is full; the latest edit was not copied yet."
                            .into(),
                    );
                    false
                }
            },
            Err(TrySendError::Disconnected(_)) => {
                self.deferred.remove(&id);
                self.local_error =
                    Some("Document recovery writer stopped; edits are not being copied.".into());
                false
            },
            Err(_) => unreachable!(),
        }
    }

    pub fn retry(&mut self) {
        let pending = std::mem::take(&mut self.deferred);
        for (_, record) in pending {
            self.offer(record);
        }
    }

    /// A synchronous barrier is intentional for a successful Save or explicit
    /// Discard: no older queued snapshot can reappear after this returns.
    pub fn clear(&mut self, id: RecoveryId) -> Result<(), String> {
        self.deferred.remove(&id);
        let (reply, ack) = mpsc::sync_channel(1);
        self.send_bounded(Command::Remove(id, reply))?;
        ack.recv_timeout(Duration::from_secs(3))
            .map_err(|_| "recovery cleanup did not finish within three seconds".to_owned())??;
        self.records.retain(|record| record.id != id);
        Ok(())
    }

    pub fn drain_error(&mut self) -> (Option<String>, Vec<RecoveryId>) {
        let mut latest = self.local_error.take();
        let mut failed = Vec::new();
        while let Ok((id, error)) = self.results.try_recv() {
            if let Some(id) = id {
                failed.push(id);
            }
            latest = Some(error);
        }
        (latest, failed)
    }

    pub fn flush(&mut self) -> Result<(), String> {
        self.retry();
        if !self.deferred.is_empty() {
            return Err("recovery queue is still full".into());
        }
        let (reply, ack) = mpsc::sync_channel(1);
        self.send_bounded(Command::Flush(reply))?;
        ack.recv_timeout(Duration::from_secs(3))
            .map_err(|_| "recovery flush did not finish within three seconds".to_owned())?
    }

    pub fn set_retention_days(&mut self, days: u16) -> Result<(), String> {
        let (reply, ack) = mpsc::sync_channel(1);
        let policy = RecoveryRetention {
            max_age_days: Some(u32::from(days)),
            ..RecoveryRetention::default()
        };
        self.send_bounded(Command::Retention(policy, reply))?;
        ack.recv_timeout(Duration::from_secs(3)).map_err(|_| {
            "recovery retention update did not finish within three seconds".to_owned()
        })?
    }

    pub fn claim(&mut self, id: RecoveryId) -> Result<Option<RecoveryRecord>, String> {
        let (reply, ack) = mpsc::sync_channel(1);
        self.send_bounded(Command::Claim(id, reply))?;
        ack.recv_timeout(Duration::from_secs(3))
            .map_err(|_| "recovery claim did not finish within three seconds".to_owned())?
    }

    #[cfg(test)]
    pub fn shutdown(mut self) -> Result<(), String> {
        self.flush()?;
        self.sender.take();
        self.writer_handle
            .take()
            .expect("writer handle")
            .join()
            .map_err(|_| "recovery writer panicked".to_owned())?;
        Ok(())
    }

    fn send_bounded(&self, mut command: Command) -> Result<(), String> {
        let start = Instant::now();
        loop {
            match self
                .sender
                .as_ref()
                .expect("active recovery writer")
                .try_send(command)
            {
                Ok(()) => return Ok(()),
                Err(TrySendError::Disconnected(_)) => return Err("recovery writer stopped".into()),
                Err(TrySendError::Full(returned)) => {
                    if start.elapsed() >= Duration::from_secs(3) {
                        return Err(
                            "recovery writer did not accept the operation within three seconds"
                                .into(),
                        );
                    }
                    command = returned;
                    std::thread::sleep(Duration::from_millis(10));
                },
            }
        }
    }
}

fn writer(
    mut store: RecoveryStore,
    receiver: Receiver<Command>,
    results: mpsc::Sender<(Option<RecoveryId>, String)>,
    wake: HostWake,
) {
    let mut pending = BTreeMap::new();
    let mut first_pending: Option<Instant> = None;
    loop {
        match receiver.recv_timeout(Duration::from_millis(650)) {
            Ok(Command::Upsert(record)) => {
                if pending.len() >= 20 && !pending.contains_key(&record.id) {
                    let _ = results.send((
                        Some(record.id),
                        "Document recovery has reached its 20 document queue limit.".into(),
                    ));
                    wake.wake();
                    continue;
                }
                first_pending.get_or_insert_with(Instant::now);
                pending.insert(record.id, record);
                if first_pending.is_some_and(|since| since.elapsed() >= Duration::from_secs(2)) {
                    let _ = flush_pending(&mut store, &mut pending, &results, &wake);
                    first_pending = None;
                }
                wake.wake();
            },
            Ok(Command::Remove(id, reply)) => {
                pending.remove(&id);
                let result = store
                    .remove(id)
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                let _ = reply.send(result);
                wake.wake();
            },
            Ok(Command::Flush(reply)) => {
                let result = flush_pending(&mut store, &mut pending, &results, &wake);
                first_pending = None;
                let _ = reply.send(result);
            },
            Ok(Command::Retention(policy, reply)) => {
                let result = store
                    .set_retention(policy)
                    .map_err(|error| error.to_string());
                let _ = reply.send(result);
                wake.wake();
            },
            Ok(Command::Claim(id, reply)) => {
                let result = store.claim(id).map_err(|error| error.to_string());
                let _ = reply.send(result);
                wake.wake();
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = flush_pending(&mut store, &mut pending, &results, &wake);
                first_pending = None;
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = flush_pending(&mut store, &mut pending, &results, &wake);
                break;
            },
        }
    }
}

fn flush_pending(
    store: &mut RecoveryStore,
    pending: &mut BTreeMap<RecoveryId, RecoveryRecord>,
    results: &mpsc::Sender<(Option<RecoveryId>, String)>,
    wake: &HostWake,
) -> Result<(), String> {
    let mut last_error = None;
    for (_, record) in std::mem::take(pending) {
        let id = record.id;
        if let Err(error) = store.upsert(record) {
            let message = format!("Document recovery failed: {error}");
            let _ = results.send((Some(id), message.clone()));
            last_error = Some(message);
            wake.wake();
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool};
    use tempfile::tempdir;

    fn wake() -> HostWake {
        HostWake::new(Arc::new(AtomicBool::new(false)), Arc::new(|| {}))
    }

    fn record(id: RecoveryId, text: &str) -> RecoveryRecord {
        RecoveryRecord::new(id, text.into(), "Djot")
    }

    #[test]
    fn continuous_edits_flush_before_typing_stops() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("recovery");
        let mut runtime =
            RecoveryRuntime::start(root.clone(), RecoveryRetention::default(), wake()).unwrap();
        let id = RecoveryId::new();
        for index in 0..27 {
            assert!(runtime.offer(record(id, &format!("edit {index}"))));
            std::thread::sleep(Duration::from_millis(90));
        }
        let observer = RecoveryStore::open(root, RecoveryRetention::default()).unwrap();
        let listed = observer.list().records;
        assert_eq!(
            listed.len(),
            1,
            "the two-second deadline flushed during typing"
        );
        assert_ne!(listed[0].source_text, "edit 26");
        runtime.shutdown().unwrap();
    }

    #[test]
    fn queued_edits_cannot_resurrect_an_explicitly_cleared_id() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("recovery");
        let mut runtime =
            RecoveryRuntime::start(root.clone(), RecoveryRetention::default(), wake()).unwrap();
        let id = RecoveryId::new();
        for index in 0..60 {
            runtime.offer(record(id, &format!("edit {index}")));
        }
        runtime.clear(id).unwrap();
        runtime.shutdown().unwrap();
        let observer = RecoveryStore::open(root, RecoveryRetention::default()).unwrap();
        assert!(observer.list().records.is_empty());
    }

    #[test]
    fn failed_write_is_reported_and_later_edit_can_retry() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("recovery");
        let policy = RecoveryRetention {
            max_item_bytes: 1,
            ..RecoveryRetention::default()
        };
        let mut runtime = RecoveryRuntime::start(root.clone(), policy, wake()).unwrap();
        let id = RecoveryId::new();
        assert!(runtime.offer(record(id, "too large")));
        // The bounded FIFO barrier waits for the offered edit. The timer may
        // already have published its refusal before the barrier is processed.
        if let Err(error) = runtime.flush() {
            assert!(
                error.contains("failed"),
                "unexpected recovery barrier failure: {error}"
            );
        }
        let (error, failed) = runtime.drain_error();
        assert!(error.unwrap().contains("failed"));
        assert_eq!(failed, [id]);
        runtime.set_retention_days(30).unwrap();
        assert!(runtime.offer(record(id, "latest edit")));
        runtime.flush().unwrap();
        runtime.shutdown().unwrap();
        let observer = RecoveryStore::open(root, RecoveryRetention::default()).unwrap();
        assert_eq!(observer.list().records[0].source_text, "latest edit");
    }
}
