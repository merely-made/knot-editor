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

use muniment::Backend;
use notochord::{
    AdmittedSession, AuthorityLapse, FrameError, LocalNetworkPolicy, RetainedAuthority,
    RevocationLedger, read_frame, write_frame,
};
use personae::Ed25519Keypair;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::RwLock;
use transport::Transport;

use crate::{
    KnotPublishCandidate, KnotPublishCatalog, KnotPublishError, KnotShareControlError,
    KnotShareRecipient, KnotShareTicket, KnotSyncStore, KnotVault, PublicationId,
    PublishCarrierError, PublishRefusal, PublishRequest, PublishResponse, PublishWireError,
    PublishWireLimits, accept_publish_session, decode_request, encode_response,
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
            now_ms: Arc::new(system_now_ms),
        }
    }

    /// Replace the wall clock for deterministic carrier receipts.
    pub fn with_clock(mut self, now_ms: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.now_ms = Arc::new(now_ms);
        self
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
        let admitted = accept_publish_session(
            transport,
            &self.identity,
            &self.policy,
            &admission_ledger,
            now_ms,
            self.active_sessions(),
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
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::{
        KNOT_PUBLISH_DOMAIN, KNOT_PUBLISH_READ_ACTION, KnotPublishRead, KnotSyncEvent,
        PublishRequest, decode_response, encode_request, publication_path, publish_alpn,
        publish_policy,
    };
    use insigne::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
        SignedDelegationCertificate, SignedDelegationRevocation,
    };
    use notochord::{
        DenyReason, NetworkId, ProfileRef, RequestedAction, SessionHello, TrafficClass,
        TrustedRoot, initiate_session, read_frame, write_frame,
    };
    use personae::delegation::Issue;
    use personae::{IdentityProvider, InMemoryProvider};
    use tempfile::tempdir;
    use transport::memory::MemoryTransport;
    use transport::noise::{secure_initiator_with_facts, secure_responder_with_facts};
    use transport::p2panda_transport::P2pandaTransport;
    use transport::{
        AcceptedSession, Alpn, IngressContext, IngressInterfaceId, OutboundSession, PeerID,
        Transport, TransportError,
    };

    const NETWORK: NetworkId = NetworkId([0xa1; 32]);
    const ROOT: [u8; 32] = [0xa2; 32];
    const NOW_MS: u64 = 50;
    const EXPIRY_MS: u64 = 100;

    fn holder() -> InMemoryProvider {
        InMemoryProvider::from_seed([0xa3; 32])
    }

    fn reader() -> InMemoryProvider {
        InMemoryProvider::from_seed([0xa4; 32])
    }

    fn profile() -> ProfileRef {
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }
    }

    fn grant(publication: PublicationId) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &holder(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT),
                holder().master_public_key().to_bytes(),
                reader().master_public_key().to_bytes(),
                CapabilityScope {
                    domain: KNOT_PUBLISH_DOMAIN.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: publication_path(publication),
                    actions: [KNOT_PUBLISH_READ_ACTION.to_string()].into_iter().collect(),
                },
                0,
                0,
                Some(EXPIRY_MS),
                1,
                [0xa5; 32],
            ),
        )
        .unwrap()
    }

    struct HostFixture {
        _directory: tempfile::TempDir,
        host: KnotPublishHost<muniment::MemoryBackend>,
        publication: PublicationId,
    }

    async fn host_and_publication() -> HostFixture {
        let directory = tempdir().unwrap();
        let vault = Arc::new(KnotVault::open(directory.path(), [0xa6; 32]).unwrap());
        let writer = holder().master_public_key().to_bytes();
        let store = KnotSyncStore::in_memory(NETWORK.0, [writer]);
        store
            .author(
                &holder().master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(crate::VaultDocument {
                    id: "selected".into(),
                    title: "Selected".into(),
                    body: b"private source".to_vec(),
                    media_type: "text/vnd.knot".into(),
                }),
            )
            .await
            .unwrap();
        let mut catalog = KnotPublishCatalog::default();
        let publication = catalog.publish("selected");
        let policy = publish_policy(
            NETWORK,
            vec![TrustedRoot {
                authority: ROOT,
                issuer: holder().master_public_key().to_bytes(),
            }],
            vec![profile()],
            Some(2),
        );
        HostFixture {
            _directory: directory,
            host: KnotPublishHost::new(
                holder().master_keypair().clone(),
                policy,
                store,
                vault,
                catalog,
                KnotPublishHostLimits::default(),
            )
            .with_clock(|| NOW_MS),
            publication,
        }
    }

    /// Ordered in-process fixture for Reticulum's honest carrier facts. This
    /// models the absent initiator identity and shared link; it is not a live
    /// Reticulum or radio qualification.
    struct LinkCarrier {
        inner: MemoryTransport,
        link: [u8; 16],
    }

    impl Transport for LinkCarrier {
        type Stream = <MemoryTransport as Transport>::Stream;

        fn local_peer_id(&self) -> PeerID {
            self.inner.local_peer_id()
        }

        async fn connect(&self, peer: PeerID, alpn: Alpn) -> Result<Self::Stream, TransportError> {
            self.inner.connect(peer, alpn).await
        }

        async fn connect_session(
            &self,
            peer: PeerID,
            alpn: Alpn,
        ) -> Result<OutboundSession<Self::Stream>, TransportError> {
            Ok(OutboundSession {
                stream: self.connect(peer, alpn.clone()).await?,
                protocol: alpn,
                authenticated_initiator: None,
                shared_link: Some(self.link),
            })
        }

        async fn accept(
            &self,
            alpn: Alpn,
        ) -> Result<AcceptedSession<Self::Stream>, TransportError> {
            let mut accepted = self.inner.accept(alpn).await?;
            accepted.peer = None;
            accepted.ingress = IngressContext::reticulum(IngressInterfaceId(7), self.link);
            Ok(accepted)
        }
    }

    fn link_carriers(
        holder: PeerID,
        reader: PeerID,
        server_link: [u8; 16],
        client_link: [u8; 16],
    ) -> (LinkCarrier, LinkCarrier) {
        let (server, client) = MemoryTransport::pair(holder, reader);
        (
            LinkCarrier {
                inner: server,
                link: server_link,
            },
            LinkCarrier {
                inner: client,
                link: client_link,
            },
        )
    }

    fn share_ticket(fixture: &HostFixture) -> crate::KnotShareTicket {
        crate::KnotShareTicket::new(
            holder().master_public_key().to_bytes(),
            "in-process-carrier",
            NETWORK,
            fixture.publication,
            vec![grant(fixture.publication)],
            None,
        )
    }

    async fn fetch_once(
        host: &KnotPublishHost<muniment::MemoryBackend>,
        publication: PublicationId,
        certificate: SignedDelegationCertificate,
    ) -> (KnotPublishServeOutcome, Result<PublishResponse, String>) {
        let holder_peer = PeerID::from_bytes(&holder().master_public_key().to_bytes()).unwrap();
        let reader_peer = PeerID::from_bytes(&reader().master_public_key().to_bytes()).unwrap();
        let (server, client) = MemoryTransport::pair(holder_peer, reader_peer);
        let server_future = host.accept_and_serve(&server);
        let client_future = async move {
            let outer = client
                .connect_session(holder_peer, publish_alpn())
                .await
                .map_err(|error| error.to_string())?;
            let carrier_binding = outer.proof_binding();
            let (mut stream, noise) = secure_initiator_with_facts(
                reader().master_keypair(),
                outer.stream,
                &publish_alpn(),
            )
            .await
            .map_err(|error| error.to_string())?;
            if noise.peer().to_bytes() != holder_peer.to_bytes() {
                return Err("Noise peer differs from the carrier holder".into());
            }
            let hello = SessionHello::issue_secure(
                &reader(),
                NETWORK,
                profile(),
                RequestedAction {
                    domain: KNOT_PUBLISH_DOMAIN.into(),
                    path: publication_path(publication),
                    action: KNOT_PUBLISH_READ_ACTION.into(),
                },
                TrafficClass::Interactive,
                [0xa7; 32],
                &carrier_binding.with_secure_channel(noise.secure_channel_facts()),
                vec![certificate],
            )
            .map_err(|error| error.to_string())?;
            let reply = initiate_session(&mut stream, &hello, &Default::default())
                .await
                .map_err(|error| error.to_string())?;
            if !reply.is_accept() {
                return Err(format!("admission refused: {reply:?}"));
            }
            let limits = PublishWireLimits::default();
            let request = encode_request(&PublishRequest::GetCurrent { publication }, limits)
                .map_err(|error| error.to_string())?;
            write_frame(&mut stream, &request, limits.max_request_bytes)
                .await
                .map_err(|error| error.to_string())?;
            let response = read_frame(&mut stream, limits.max_response_bytes)
                .await
                .map_err(|error| error.to_string())?;
            decode_response(&response, limits).map_err(|error| error.to_string())
        };
        let (served, response) = tokio::join!(server_future, client_future);
        (served.unwrap(), response)
    }

    #[tokio::test]
    async fn memory_carrier_reaches_source_only_after_noise_and_notochord_then_revokes() {
        let fixture = host_and_publication().await;
        let certificate = grant(fixture.publication);
        let (outcome, response) =
            fetch_once(&fixture.host, fixture.publication, certificate.clone()).await;
        assert_eq!(outcome, KnotPublishServeOutcome::Responded);
        let read = response.unwrap().into_read().unwrap();
        let KnotPublishRead::Document(document) = read else {
            panic!("the selected source must reach its admitted reader")
        };
        assert_eq!(document.body, b"private source");
        assert!(document.body_digest_matches());

        let revocation = SignedDelegationRevocation::issue(
            &holder(),
            DelegationRevocation::new(
                certificate.certificate.id(),
                holder().master_public_key().to_bytes(),
                certificate.certificate.scope.clone(),
                NOW_MS,
                [0xa8; 32],
            ),
        )
        .unwrap();
        fixture
            .host
            .revocations()
            .write()
            .await
            .fold(revocation.check().expect("the holder's revocation checks"));
        let (outcome, response) = fetch_once(&fixture.host, fixture.publication, certificate).await;
        assert!(matches!(
            outcome,
            KnotPublishServeOutcome::Refused(PublishRefusal::NotAdmitted(_))
        ));
        assert!(
            response.is_err(),
            "revoked admission reaches no application response"
        );
    }

    #[tokio::test]
    async fn ticket_client_reads_only_its_named_publication() {
        let fixture = host_and_publication().await;
        let certificate = grant(fixture.publication);
        let holder_peer = PeerID::from_bytes(&holder().master_public_key().to_bytes()).unwrap();
        let reader_peer = PeerID::from_bytes(&reader().master_public_key().to_bytes()).unwrap();
        let (server, client) = MemoryTransport::pair(holder_peer, reader_peer);
        let ticket = crate::KnotShareTicket::new(
            holder_peer.to_bytes(),
            "memory-carrier",
            NETWORK,
            fixture.publication,
            vec![certificate],
            None,
        );

        let server_future = fixture.host.accept_and_serve(&server);
        let reader = reader();
        let client_future =
            crate::fetch_published_document(&client, reader.master_keypair(), profile(), &ticket);
        let (served, read) = tokio::join!(server_future, client_future);
        assert_eq!(served.unwrap(), KnotPublishServeOutcome::Responded);
        let KnotPublishRead::Document(document) = read.unwrap() else {
            panic!("a granted, selected publication must disclose its current source")
        };
        assert_eq!(document.publication, ticket.publication);
        assert_eq!(document.body, b"private source");
    }

    #[tokio::test]
    async fn ticket_client_binds_noise_identity_without_inventing_a_carrier_peer() {
        let fixture = host_and_publication().await;
        let ticket = share_ticket(&fixture);
        let holder_peer = PeerID::from_bytes(&ticket.publisher).unwrap();
        // The carrier-local routing identity is deliberately not the reader
        // application identity, and the carrier does not authenticate it.
        let routing_peer =
            PeerID::from_public_key(InMemoryProvider::from_seed([0xb1; 32]).master_public_key());
        let (server, client) = link_carriers(holder_peer, routing_peer, [0xb2; 16], [0xb2; 16]);
        assert_ne!(
            client.local_peer_id().to_bytes(),
            reader().master_public_key().to_bytes()
        );

        let reader = reader();
        let (served, read) = tokio::join!(
            fixture.host.accept_and_serve(&server),
            crate::fetch_published_document(&client, reader.master_keypair(), profile(), &ticket),
        );
        assert_eq!(served.unwrap(), KnotPublishServeOutcome::Responded);
        let KnotPublishRead::Document(document) = read.unwrap() else {
            panic!("the bound, granted Noise reader receives its publication")
        };
        assert_eq!(document.publication, ticket.publication);
        assert_eq!(document.body, b"private source");
    }

    #[tokio::test]
    async fn ticket_client_cannot_rebind_the_secure_proof_to_another_link() {
        let fixture = host_and_publication().await;
        let ticket = share_ticket(&fixture);
        let holder_peer = PeerID::from_bytes(&ticket.publisher).unwrap();
        let reader_peer = PeerID::from_public_key(reader().master_public_key());
        let (server, client) = link_carriers(holder_peer, reader_peer, [0xb3; 16], [0xb4; 16]);
        let reader = reader();
        let (served, read) = tokio::join!(
            fixture.host.accept_and_serve(&server),
            crate::fetch_published_document(&client, reader.master_keypair(), profile(), &ticket),
        );
        assert_eq!(
            served.unwrap(),
            KnotPublishServeOutcome::Refused(PublishRefusal::NotAdmitted(
                DenyReason::SessionProofInvalid,
            )),
        );
        assert!(matches!(read, Err(crate::KnotPublishClientError::Refused)));
    }

    #[tokio::test]
    async fn authenticated_carrier_and_noise_reader_mismatch_still_refuses() {
        let fixture = host_and_publication().await;
        let holder_peer = PeerID::from_public_key(holder().master_public_key());
        let different_carrier_peer =
            PeerID::from_public_key(InMemoryProvider::from_seed([0xb5; 32]).master_public_key());
        let (server, client) = MemoryTransport::pair(holder_peer, different_carrier_peer);
        let server_future = fixture.host.accept_and_serve(&server);
        let client_future = async {
            let outer = client.connect(holder_peer, publish_alpn()).await.unwrap();
            let (mut stream, _) =
                secure_initiator_with_facts(reader().master_keypair(), outer, &publish_alpn())
                    .await
                    .unwrap();
            assert!(read_frame(&mut stream, 1024).await.is_err());
        };
        let (served, ()) = tokio::join!(server_future, client_future);
        assert_eq!(
            served.unwrap(),
            KnotPublishServeOutcome::Refused(PublishRefusal::CarrierNoiseMismatch),
        );
    }

    #[tokio::test]
    async fn ticket_client_checks_the_noise_publisher_before_sending_its_hello() {
        let fixture = host_and_publication().await;
        let ticket = share_ticket(&fixture);
        let holder_peer = PeerID::from_bytes(&ticket.publisher).unwrap();
        let reader_peer = PeerID::from_public_key(reader().master_public_key());
        let (server, client) = MemoryTransport::pair(holder_peer, reader_peer);
        let server_future = async {
            let accepted = server.accept(publish_alpn()).await.unwrap();
            let impostor = InMemoryProvider::from_seed([0xb6; 32]);
            let (mut stream, _) =
                secure_responder_with_facts(impostor.master_keypair(), accepted.stream)
                    .await
                    .unwrap();
            assert!(
                read_frame(&mut stream, 1024).await.is_err(),
                "an unexpected publisher gets no admission hello or application request",
            );
        };
        let reader = reader();
        let ((), read) = tokio::join!(
            server_future,
            crate::fetch_published_document(&client, reader.master_keypair(), profile(), &ticket),
        );
        assert!(matches!(
            read,
            Err(crate::KnotPublishClientError::PublisherIdentity)
        ));
    }

    #[tokio::test]
    async fn a_prior_noise_session_hello_cannot_read_a_publication_after_reconnect() {
        let fixture = host_and_publication().await;
        let holder_peer = PeerID::from_public_key(holder().master_public_key());
        let reader_peer = PeerID::from_public_key(reader().master_public_key());
        let (server, client) = MemoryTransport::pair(holder_peer, reader_peer);
        let capture = async {
            let accepted = server.accept(publish_alpn()).await.unwrap();
            let (mut stream, _) =
                secure_responder_with_facts(holder().master_keypair(), accepted.stream)
                    .await
                    .unwrap();
            let bytes = read_frame(&mut stream, 64 * 1024).await.unwrap();
            SessionHello::decode(&bytes, &Default::default()).unwrap()
        };
        let mint = async {
            let outer = client
                .connect_session(holder_peer, publish_alpn())
                .await
                .unwrap();
            let binding = outer.proof_binding();
            let (mut stream, noise) = secure_initiator_with_facts(
                reader().master_keypair(),
                outer.stream,
                &publish_alpn(),
            )
            .await
            .unwrap();
            let hello = SessionHello::issue_secure(
                &reader(),
                NETWORK,
                profile(),
                RequestedAction {
                    domain: KNOT_PUBLISH_DOMAIN.into(),
                    path: publication_path(fixture.publication),
                    action: KNOT_PUBLISH_READ_ACTION.into(),
                },
                TrafficClass::Interactive,
                [0xb7; 32],
                &binding.with_secure_channel(noise.secure_channel_facts()),
                vec![grant(fixture.publication)],
            )
            .unwrap();
            let bytes = hello.encode(&Default::default()).unwrap();
            write_frame(&mut stream, &bytes, 64 * 1024).await.unwrap();
        };
        let (captured, ()) = tokio::join!(capture, mint);

        let (server, client) = MemoryTransport::pair(holder_peer, reader_peer);
        let replay = async {
            let outer = client.connect(holder_peer, publish_alpn()).await.unwrap();
            let (mut stream, _) =
                secure_initiator_with_facts(reader().master_keypair(), outer, &publish_alpn())
                    .await
                    .unwrap();
            let reply = initiate_session(&mut stream, &captured, &Default::default())
                .await
                .unwrap();
            assert!(!reply.is_accept());
        };
        let (served, ()) = tokio::join!(fixture.host.accept_and_serve(&server), replay);
        assert_eq!(
            served.unwrap(),
            KnotPublishServeOutcome::Refused(PublishRefusal::NotAdmitted(
                DenyReason::SessionProofInvalid,
            )),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn p2panda_loopback_uses_the_real_noise_and_notochord_path() {
        let fixture = host_and_publication().await;
        let server = P2pandaTransport::builder_from_seed(holder().master_keypair().to_seed())
            .alpns(vec![publish_alpn()])
            .bind()
            .await
            .expect("holder P2panda transport binds");
        let client = P2pandaTransport::builder_from_seed(reader().master_keypair().to_seed())
            .alpns(vec![publish_alpn()])
            .bind()
            .await
            .expect("reader P2panda transport binds");
        let holder_peer = server.local_peer_id();
        let reader_peer = client.local_peer_id();
        assert_eq!(
            holder_peer.to_bytes(),
            holder().master_public_key().to_bytes()
        );
        assert_eq!(
            reader_peer.to_bytes(),
            reader().master_public_key().to_bytes()
        );
        server
            .add_peer(client.endpoint_addr().await.expect("reader endpoint"))
            .await
            .expect("holder knows reader endpoint");
        client
            .add_peer(server.endpoint_addr().await.expect("holder endpoint"))
            .await
            .expect("reader knows holder endpoint");

        let publication = fixture.publication;
        let certificate = grant(publication);
        let server_future = fixture.host.accept_and_serve(&server);
        // Keep the client endpoint alive until both sides have completed. A
        // successful reader can otherwise drop it while the holder is still
        // flushing its response, making this real-transport test race shutdown.
        let client_future = async {
            let outer = client
                .connect_session(holder_peer, publish_alpn())
                .await
                .map_err(|error| error.to_string())?;
            let carrier_binding = outer.proof_binding();
            let (mut stream, noise) = secure_initiator_with_facts(
                reader().master_keypair(),
                outer.stream,
                &publish_alpn(),
            )
            .await
            .map_err(|error| error.to_string())?;
            if noise.peer().to_bytes() != holder_peer.to_bytes() {
                return Err("Noise peer differs from the P2panda holder".into());
            }
            let hello = SessionHello::issue_secure(
                &reader(),
                NETWORK,
                profile(),
                RequestedAction {
                    domain: KNOT_PUBLISH_DOMAIN.into(),
                    path: publication_path(publication),
                    action: KNOT_PUBLISH_READ_ACTION.into(),
                },
                TrafficClass::Interactive,
                [0xab; 32],
                &carrier_binding.with_secure_channel(noise.secure_channel_facts()),
                vec![certificate],
            )
            .map_err(|error| error.to_string())?;
            let reply = initiate_session(&mut stream, &hello, &Default::default())
                .await
                .map_err(|error| error.to_string())?;
            if !reply.is_accept() {
                return Err(format!("admission refused: {reply:?}"));
            }
            let limits = PublishWireLimits::default();
            let request = encode_request(&PublishRequest::GetCurrent { publication }, limits)
                .map_err(|error| error.to_string())?;
            write_frame(&mut stream, &request, limits.max_request_bytes)
                .await
                .map_err(|error| error.to_string())?;
            let response = read_frame(&mut stream, limits.max_response_bytes)
                .await
                .map_err(|error| error.to_string())?;
            decode_response(&response, limits).map_err(|error| error.to_string())
        };
        let (served, response) = tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(server_future, client_future)
        })
        .await
        .expect("P2panda loopback completed");
        assert_eq!(
            served.expect("holder server outcome"),
            KnotPublishServeOutcome::Responded
        );
        let KnotPublishRead::Document(document) = response
            .expect("reader response")
            .into_read()
            .expect("document response")
        else {
            panic!("the admitted P2panda reader must receive the selected source")
        };
        assert_eq!(document.body, b"private source");
        assert!(document.body_digest_matches());
    }

    #[tokio::test]
    async fn response_guard_linearizes_revocation_and_slots_release_on_drop() {
        let mut fixture = host_and_publication().await;
        fixture.host.limits.max_concurrent_sessions = 1;
        let first = fixture
            .host
            .reserve_slot()
            .expect("first reader reserves capacity");
        assert!(
            fixture.host.reserve_slot().is_none(),
            "a third-party reader is refused at capacity"
        );
        drop(first);
        assert!(
            fixture.host.reserve_slot().is_some(),
            "RAII releases capacity after a served task ends"
        );

        let certificate = grant(fixture.publication);
        let revocation = SignedDelegationRevocation::issue(
            &holder(),
            DelegationRevocation::new(
                certificate.certificate.id(),
                holder().master_public_key().to_bytes(),
                certificate.certificate.scope.clone(),
                NOW_MS,
                [0xa9; 32],
            ),
        )
        .unwrap();
        let ledger = fixture.host.revocations();
        let response_guard = ledger.read().await;
        let writer = {
            let ledger = Arc::clone(&ledger);
            tokio::spawn(async move {
                let mut ledger = ledger.write().await;
                ledger.fold(revocation.check().expect("the holder's revocation checks"));
            })
        };
        tokio::task::yield_now().await;
        assert!(
            !writer.is_finished(),
            "a revocation waits while the final bounded response guard is held"
        );
        drop(response_guard);
        writer.await.unwrap();
    }
}
