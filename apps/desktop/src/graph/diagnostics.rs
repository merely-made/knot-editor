// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Bounded redacted observations. Catalog and document authority stay in Knot.
use apparatus::{
    Admission, Batch, Cursor, ObservationMetadata, ObservationStore, OperationId, RecordRef,
    RetentionLimits, SourceId, StoreStats,
};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    SaveDispatched,
    FileWriteReturned,
    CatalogRequested,
    QueuedSuperseded,
    InterestInvalidated,
    GenerationExhausted,
    WorkerStarted,
    WorkerCompleted,
    WorkerSpawnFailed,
    WorkerDisconnected,
    Accepted,
    Discarded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResultKind {
    Success,
    WriteFailed,
    ReadErrors,
    Superseded,
    InterestUnavailable,
    IdentityExhausted,
    Unconfirmed,
}

/// Fixed enums and scalars. Never retain paths, labels, hashes, source or errors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Observation {
    pub phase: Phase,
    pub generation: Option<u64>,
    pub result: Option<ResultKind>,
    pub nodes: Option<usize>,
    pub relations: Option<usize>,
    pub read_errors: Option<usize>,
}
impl Observation {
    pub fn new(phase: Phase, generation: Option<u64>, result: Option<ResultKind>) -> Self {
        Self {
            phase,
            generation,
            result,
            nodes: None,
            relations: None,
            read_errors: None,
        }
    }
}

struct Collector {
    store: ObservationStore<Observation>,
    started: Instant,
    failure: Option<&'static str>,
}
#[derive(Clone)]
pub(crate) struct Diagnostics {
    collector: Arc<Mutex<Collector>>,
}
impl Diagnostics {
    pub fn new(limits: RetentionLimits) -> Self {
        Self {
            collector: Arc::new(Mutex::new(Collector {
                store: ObservationStore::new(
                    format!("knot-catalog-{}", uuid::Uuid::new_v4()).into(),
                    SourceId::from("knot.catalog"),
                    limits,
                ),
                started: Instant::now(),
                failure: None,
            })),
        }
    }
    pub fn from_env() -> Option<Self> {
        if std::env::var("KNOT_DIAGNOSTICS").ok().as_deref() != Some("1") {
            return None;
        }
        fn setting(name: &str, default: usize, maximum: usize) -> Result<usize, &'static str> {
            std::env::var(name).ok().map_or(Ok(default), |value| {
                value
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value <= maximum)
                    .ok_or("invalid catalog diagnostic retention setting")
            })
        }
        let configured = (|| {
            Ok::<_, &'static str>(RetentionLimits {
                max_records: setting("KNOT_DIAGNOSTIC_RECORDS", 256, 4096)?,
                max_bytes: setting("KNOT_DIAGNOSTIC_BYTES", 262_144, 4_194_304)?,
                max_age: Duration::from_secs(
                    setting("KNOT_DIAGNOSTIC_AGE_SECS", 300, 86_400)? as u64
                ),
            })
        })();
        let diagnostics = Self::new(configured.unwrap_or(RetentionLimits {
            max_records: 0,
            max_bytes: 0,
            max_age: Duration::ZERO,
        }));
        diagnostics.collector.lock().expect("new collector").failure = configured.err();
        Some(diagnostics)
    }
    pub fn record(
        &self,
        payload: Observation,
        operation: Option<OperationId>,
        cause: Option<RecordRef>,
    ) -> Option<RecordRef> {
        // Fixed payload construction precedes admission; encoded length is truthful.
        let bytes = serde_json::to_vec(&payload).ok()?.len();
        let mut collector = self.collector.lock().ok()?;
        if collector.failure.is_some() {
            return None;
        }
        let now = collector.started.elapsed();
        let metadata = ObservationMetadata {
            operation,
            cause,
            ..Default::default()
        };
        match collector.store.record(payload, bytes, metadata, now) {
            Ok(Admission::Retained(reference) | Admission::Rejected { reference, .. }) => {
                Some(reference)
            },
            Err(_) => {
                collector.failure = Some("catalog diagnostic admission failed");
                None
            },
        }
    }
    pub fn cursor(&self) -> Result<Cursor, String> {
        let collector = self
            .collector
            .lock()
            .map_err(|_| "catalog diagnostic store unavailable")?;
        if let Some(failure) = collector.failure {
            return Err(failure.into());
        }
        Ok(collector.store.cursor())
    }
    pub fn read(&self, cursor: &mut Cursor) -> Result<Batch<serde_json::Value>, String> {
        let mut collector = self
            .collector
            .lock()
            .map_err(|_| "catalog diagnostic store unavailable")?;
        if let Some(failure) = collector.failure {
            return Err(failure.into());
        }
        let now = collector.started.elapsed();
        let batch = collector
            .store
            .read(cursor, now, 128)
            .map_err(|_| "catalog diagnostic read failed")?;
        Ok(batch
            .map_payload(|payload| serde_json::to_value(payload).expect("fixed scalar payload")))
    }
    /// Admission/loss context only, not a receipt reader or a frame cause.
    pub fn stats(&self) -> Result<StoreStats, String> {
        let collector = self
            .collector
            .try_lock()
            .map_err(|_| "catalog diagnostic store unavailable at seal")?;
        if let Some(failure) = collector.failure {
            return Err(failure.into());
        }
        Ok(collector.store.stats())
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Installed {
    pub generation: u64,
    pub request: Option<RecordRef>,
    pub outcome: Option<RecordRef>,
    pub acceptance: Option<RecordRef>,
}

#[derive(Serialize)]
pub(crate) struct CaptureFacts<'a> {
    pub configured: bool,
    pub busy: bool,
    pub queued: bool,
    pub desired_generation: u64,
    pub generation_exhausted: bool,
    pub installed: Option<&'a Installed>,
    pub catalog_nodes: usize,
    pub catalog_relations: usize,
    pub read_errors: usize,
    pub failure_present: bool,
    pub diagnostic_admission: Option<StoreStats>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retention_loss_is_visible_to_independent_readers_and_zero_still_counts() {
        for capacity in [0, 1] {
            let diagnostics = Diagnostics::new(RetentionLimits {
                max_records: capacity,
                max_bytes: 4096,
                max_age: Duration::from_secs(60),
            });
            let mut receipt = diagnostics.cursor().unwrap();
            let mut inspector = diagnostics.cursor().unwrap();
            for generation in 1..=3 {
                diagnostics.record(
                    Observation::new(Phase::CatalogRequested, Some(generation), None),
                    None,
                    None,
                );
            }
            let first = diagnostics.read(&mut inspector).unwrap();
            let second = diagnostics.read(&mut receipt).unwrap();
            assert_eq!(first, second);
            assert_eq!(first.records.len(), capacity);
            assert_eq!(first.gaps.len(), 1);
            assert_eq!(first.stats.next_sequence, 4);
            if capacity == 0 {
                assert_eq!(first.stats.loss.rejected_disabled, 3);
            } else {
                assert_eq!(first.stats.loss.evicted, 2);
            }
        }
    }
    #[test]
    fn fixed_payload_and_seal_stats_do_not_consume_a_reader() {
        let diagnostics = Diagnostics::new(RetentionLimits {
            max_records: 4,
            max_bytes: 4096,
            max_age: Duration::from_secs(60),
        });
        let mut cursor = diagnostics.cursor().unwrap();
        let reference = diagnostics
            .record(
                Observation::new(
                    Phase::WorkerCompleted,
                    Some(2),
                    Some(ResultKind::ReadErrors),
                ),
                Some("catalog:2".into()),
                None,
            )
            .unwrap();
        assert_eq!(diagnostics.stats().unwrap().next_sequence, 2);
        assert_eq!(cursor.next_sequence(), 1);
        let batch = diagnostics.read(&mut cursor).unwrap();
        assert_eq!(batch.records[0].envelope.reference, reference);
        let encoded = serde_json::to_string(&batch.records[0].payload).unwrap();
        assert!(encoded.len() < 256);
        assert!(!encoded.contains("path"));
        assert!(!encoded.contains("hash"));
    }
}
