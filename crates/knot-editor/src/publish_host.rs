// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Resident host for one private Knot publishing service.
//!
//! The host keeps publication selection, revocations, and live-session
//! accounting. It neither changes `KnotSyncHost` nor exposes its paired writer
//! transport: read publication has a different authority and lifetime.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::publish_carrier::accept_publish_session_with_setup;
use muniment::Backend;
use notochord::{
    AdmittedSession, AuthorityLapse, FrameError, LocalNetworkPolicy, RetainedAuthority,
    RevocationLedger, read_frame, write_frame,
};
use personae::Ed25519Keypair;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::RwLock;
use transport::Transport;
use transport::security::{SetupGate, SetupLimits, SetupStats};

use crate::{
    KnotPublishCandidate, KnotPublishCatalog, KnotPublishError, KnotShareControlError,
    KnotShareRecipient, KnotShareTicket, KnotSyncStore, KnotVault, PublicationId,
    PublishCarrierError, PublishRefusal, PublishRequest, PublishResponse, PublishWireError,
    PublishWireLimits, decode_request, encode_response,
};

/// Owner-configurable serving limits. The candidate codec applies its own hard
/// caps before any request or body allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnotPublishHostLimits {
    pub wire: PublishWireLimits,
    pub max_concurrent_sessions: u32,
}

impl Default for KnotPublishHostLimits {
    fn default() -> Self {
        Self {
            wire: PublishWireLimits::default(),
            max_concurrent_sessions: 8,
        }
    }
}

/// Terminal outcome for one accepted carrier stream. It intentionally carries
/// no source bytes: readers receive those only on the encrypted stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotPublishServeOutcome {
    Responded,
    Refused(PublishRefusal),
    Lapsed(AuthorityLapse),
}

/// Failure while running a host-owned carrier stream.
#[derive(Debug, thiserror::Error)]
pub enum KnotPublishHostError {
    #[error(transparent)]
    Carrier(#[from] PublishCarrierError),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("Knot publishing stream failed: {0}")]
    Stream(#[from] std::io::Error),
    #[error(transparent)]
    Wire(#[from] PublishWireError),
    #[error(transparent)]
    Publish(#[from] KnotPublishError),
}

/// A library host for an explicitly selected retained Knot catalog.
#[derive(Clone)]
pub struct KnotPublishHost<B> {
    identity: Ed25519Keypair,
    policy: LocalNetworkPolicy,
    store: KnotSyncStore<B>,
    vault: Arc<KnotVault>,
    catalog: Arc<RwLock<KnotPublishCatalog>>,
    revocations: Arc<RwLock<RevocationLedger>>,
    limits: KnotPublishHostLimits,
    active_sessions: Arc<AtomicU32>,
    setup_gate: SetupGate,
    setup_limits: SetupLimits,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
}

/// The independently retained read material a startup-unlocked Knot vault
/// hands to the publishing service. It keeps the carrier identity equal to the
/// signed writer/device identity while leaving the editor's mutable vault
/// handle with the authoring endpoint.
pub struct KnotPublishSource {
    identity: Ed25519Keypair,
    store: KnotSyncStore<muniment::RedbBackend>,
    vault: Arc<KnotVault>,
}

impl KnotPublishSource {
    pub(crate) fn from_unlocked(
        identity: Ed25519Keypair,
        store: KnotSyncStore<muniment::RedbBackend>,
        vault: Arc<KnotVault>,
    ) -> Self {
        Self {
            identity,
            store,
            vault,
        }
    }

    /// The device key that must own both the outer carrier and inner Noise
    /// identity for this host.
    pub fn transport_seed(&self) -> [u8; 32] {
        self.identity.to_seed()
    }

    /// The stable public carrier identity advertised in a share ticket.
    pub fn publisher(&self) -> [u8; 32] {
        self.identity.public_key().to_bytes()
    }

    /// Move the retained source material into one live publishing host.
    pub fn into_host(
        self,
        policy: LocalNetworkPolicy,
        catalog: KnotPublishCatalog,
        limits: KnotPublishHostLimits,
    ) -> KnotPublishHost<muniment::RedbBackend> {
        KnotPublishHost::new(
            self.identity,
            policy,
            self.store,
            self.vault,
            catalog,
            limits,
        )
    }
}

impl<B> KnotPublishHost<B>
where
    B: Backend + Clone,
{
    /// Build a host from holder-only state. The caller retains owner controls
    /// through [`Self::publish`], [`Self::unpublish`], and [`Self::revocations`].
    pub fn new(
        identity: Ed25519Keypair,
        policy: LocalNetworkPolicy,
        store: KnotSyncStore<B>,
        vault: Arc<KnotVault>,
        catalog: KnotPublishCatalog,
        limits: KnotPublishHostLimits,
    ) -> Self {
        Self {
            identity,
            policy,
            store,
            vault,
            catalog: Arc::new(RwLock::new(catalog)),
            revocations: Arc::new(RwLock::new(RevocationLedger::default())),
            limits,
            active_sessions: Arc::new(AtomicU32::new(0)),
            setup_gate: SetupGate::new(16),
            setup_limits: SetupLimits::default(),
            now_ms: Arc::new(system_now_ms),
        }
    }

    /// Replace the wall clock for deterministic carrier receipts.
    pub fn with_clock(mut self, now_ms: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.now_ms = Arc::new(now_ms);
        self
    }

    /// Configure setup before serving. Cloned hosts retain the same shared gate.
    pub fn with_secure_setup(mut self, max_pending: usize, limits: SetupLimits) -> Self {
        self.setup_gate = SetupGate::new(max_pending);
        self.setup_limits = limits;
        self
    }

    /// Counter-only setup diagnostics, separate from admitted serving slots.
    pub fn setup_stats(&self) -> SetupStats {
        self.setup_gate.stats()
    }

    /// Add one explicit owner selection to the served catalog.
    pub async fn publish(&self, source_document: impl Into<String>) -> PublicationId {
        self.catalog.write().await.publish(source_document)
    }

    /// Withdraw one selection. Holding the catalog read guard through response
    /// writing makes this linearize before or after a bounded response.
    pub async fn unpublish(&self, id: PublicationId) -> bool {
        self.catalog.write().await.unpublish(id).is_some()
    }

    /// Inspect owner-visible retained sources before selecting one.
    pub async fn candidates(&self) -> Result<Vec<KnotPublishCandidate>, KnotPublishError> {
        let catalog = self.catalog.read().await.clone();
        catalog.candidates(&self.store, &self.vault).await
    }

    /// Issue one reader-bound share through this host's current catalog.
    /// The host, not a product pane, supplies the transport identity encoded in
    /// the ticket so the reader cannot be directed to a different carrier.
    pub async fn issue_share<P: personae::IdentityProvider>(
        &self,
        issuer: &P,
        mut request: KnotShareRecipient,
    ) -> Result<KnotShareTicket, KnotShareControlError> {
        request.publisher = self.identity.public_key().to_bytes();
        self.catalog.read().await.issue_share(issuer, request)
    }

    /// The owner-maintained revocation ledger. Admission snapshots it; a
    /// response rereads it and holds the read guard through its final write.
    pub fn revocations(&self) -> Arc<RwLock<RevocationLedger>> {
        Arc::clone(&self.revocations)
    }

    /// Currently reserved admitted serving slots.
    pub fn active_sessions(&self) -> u32 {
        self.active_sessions.load(Ordering::Acquire)
    }

    /// Accept and serve one candidate one-request publishing stream.
    pub async fn accept_and_serve<T: Transport>(
        &self,
        transport: &T,
    ) -> Result<KnotPublishServeOutcome, KnotPublishHostError> {
        let now_ms = self.now();
        let admission_ledger = self.revocations.read().await.clone();
        let admitted = accept_publish_session_with_setup(
            transport,
            &self.identity,
            &self.policy,
            &admission_ledger,
            now_ms,
            self.active_sessions(),
            &self.setup_gate,
            self.setup_limits.clone(),
        )
        .await?;
        let mut session = match admitted {
            Ok(session) => session,
            Err(refusal) => return Ok(KnotPublishServeOutcome::Refused(refusal)),
        };

        let Some(_slot) = self.reserve_slot() else {
            let _ = session.session.stream.shutdown().await;
            return Ok(KnotPublishServeOutcome::Refused(
                PublishRefusal::CapacityExhausted,
            ));
        };
        let authority = RetainedAuthority::from_admitted(&session.session);
        // Keep the verified channel facts alongside the admitted authority
        // for the complete response/revocation lifetime.
        self.serve_admitted(&mut session.session, authority).await
    }

    async fn serve_admitted<S>(
        &self,
        session: &mut AdmittedSession<S>,
        authority: RetainedAuthority,
    ) -> Result<KnotPublishServeOutcome, KnotPublishHostError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let now_ms = self.now();
        let admission_ledger = self.revocations.read().await;
        let admission_lapse = authority.lapse(&admission_ledger, now_ms);
        drop(admission_ledger);
        if let Some(lapse) = admission_lapse {
            let _ = session.stream.shutdown().await;
            return Ok(KnotPublishServeOutcome::Lapsed(lapse));
        }

        let bytes = read_frame(
            &mut session.stream,
            self.limits.wire.clamped().max_request_bytes,
        )
        .await?;
        let request = match decode_request(&bytes, self.limits.wire) {
            Ok(request) => request,
            Err(error) => {
                let _ = session.stream.shutdown().await;
                return Err(error.into());
            },
        };
        let requested_publication = match request {
            PublishRequest::List => None,
            PublishRequest::GetCurrent { publication }
            | PublishRequest::GetVersion { publication, .. } => Some(publication),
        };

        // Materialize under snapshots, then take both final read guards before
        // writing. A revocation or unpublish that wins either writer lock
        // prevents this response; a response guard already held denotes the
        // bounded response that was in flight first.
        let initial_ledger = self.revocations.read().await.clone();
        let initial_catalog = self.catalog.read().await.clone();
        let selected_source = requested_publication.and_then(|publication| {
            initial_catalog
                .selection(publication)
                .map(|selection| selection.source_document.clone())
        });
        let response = match request {
            PublishRequest::List => PublishResponse::Catalog {
                publications: initial_catalog.list(&authority, &initial_ledger, now_ms),
            },
            PublishRequest::GetCurrent { publication } => initial_catalog
                .get_current(
                    &self.store,
                    &self.vault,
                    &authority,
                    &initial_ledger,
                    now_ms,
                    publication,
                )
                .await?
                .into(),
            PublishRequest::GetVersion {
                publication,
                operation,
            } => initial_catalog
                .get_version(
                    &self.store,
                    &self.vault,
                    &authority,
                    &initial_ledger,
                    now_ms,
                    publication,
                    operation,
                )
                .await?
                .into(),
        };

        let ledger = self.revocations.read().await;
        let catalog = self.catalog.read().await;
        let now_ms = self.now();
        if let Some(lapse) = authority.lapse(&ledger, now_ms) {
            let _ = session.stream.shutdown().await;
            return Ok(KnotPublishServeOutcome::Lapsed(lapse));
        }
        let response = match (requested_publication, response) {
            (None, _) => PublishResponse::Catalog {
                publications: catalog.list(&authority, &ledger, now_ms),
            },
            (Some(publication), response)
                if catalog.selection(publication).is_some_and(|selection| {
                    Some(&selection.source_document) == selected_source.as_ref()
                }) =>
            {
                response
            },
            (Some(_), _) => PublishResponse::NotAvailable,
        };
        let encoded = match encode_response(&response, self.limits.wire) {
            Ok(encoded) => encoded,
            // A response that would exceed the owner's ceilings never writes a
            // body. It joins other unavailable source states on the wire.
            Err(PublishWireError::ResponseLimit | PublishWireError::TooLarge) => {
                encode_response(&PublishResponse::NotAvailable, self.limits.wire)?
            },
            Err(error) => return Err(error.into()),
        };
        write_frame(
            &mut session.stream,
            &encoded,
            self.limits.wire.clamped().max_response_bytes,
        )
        .await?;
        session.stream.shutdown().await?;
        Ok(KnotPublishServeOutcome::Responded)
    }

    fn reserve_slot(&self) -> Option<LiveSessionSlot> {
        loop {
            let current = self.active_sessions.load(Ordering::Acquire);
            if current >= self.limits.max_concurrent_sessions {
                return None;
            }
            if self
                .active_sessions
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(LiveSessionSlot {
                    active_sessions: Arc::clone(&self.active_sessions),
                });
            }
        }
    }

    fn now(&self) -> u64 {
        (self.now_ms)()
    }
}

struct LiveSessionSlot {
    active_sessions: Arc<AtomicU32>,
}

impl Drop for LiveSessionSlot {
    fn drop(&mut self) {
        self.active_sessions.fetch_sub(1, Ordering::AcqRel);
    }
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
