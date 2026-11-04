// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::time::Duration;

use crate::{
    KNOT_PUBLISH_DOMAIN, KNOT_PUBLISH_READ_ACTION, KnotPublishRead, KnotSyncEvent, PublishRequest,
    decode_response, encode_request, publication_path, publish_alpn, publish_policy,
};
use insigne::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
    SignedDelegationCertificate, SignedDelegationRevocation,
};
use notochord::{
    DenyReason, NetworkId, ProfileRef, RequestedAction, SessionHello, TrafficClass, TrustedRoot,
    initiate_session, read_frame, write_frame,
};
use personae::delegation::Issue;
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;
use transport::memory::MemoryTransport;
use transport::noise::{secure_initiator_with_facts, secure_responder_with_facts};
use transport::p2panda_transport::P2pandaTransport;
use transport::{
    AcceptedSession, Alpn, IngressContext, IngressInterfaceId, OutboundSession, PeerID, Transport,
    TransportError,
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

    async fn accept(&self, alpn: Alpn) -> Result<AcceptedSession<Self::Stream>, TransportError> {
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
        let (mut stream, noise) =
            secure_initiator_with_facts(reader().master_keypair(), outer.stream, &publish_alpn())
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
    assert!(
        matches!(
            outcome,
            KnotPublishServeOutcome::Refused(PublishRefusal::NotAdmitted(_))
        ),
        "unexpected setup outcome: {outcome:?}"
    );
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
async fn stalled_secure_setup_releases_the_host_lease_before_source_disclosure() {
    let mut fixture = host_and_publication().await;
    fixture.host = fixture.host.with_secure_setup(
        1,
        SetupLimits {
            timeout: std::time::Duration::from_millis(30),
            ..SetupLimits::default()
        },
    );
    let holder_peer = PeerID::from_public_key(holder().master_public_key());
    let reader_peer = PeerID::from_public_key(reader().master_public_key());
    let (server, client) = MemoryTransport::pair(holder_peer, reader_peer);
    let _stalled = client.connect(holder_peer, publish_alpn()).await.unwrap();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        fixture.host.accept_and_serve(&server),
    )
    .await
    .unwrap();
    assert!(matches!(
        outcome,
        Err(KnotPublishHostError::Carrier(PublishCarrierError::Setup(
            transport::security::SetupError::Deadline { .. }
        )))
    ));
    assert_eq!(fixture.host.setup_stats().pending, 0);
    assert_eq!(fixture.host.setup_stats().timed_out, 1);
    assert_eq!(fixture.host.active_sessions(), 0);
}

#[tokio::test]
async fn the_shared_setup_lease_covers_admission_and_is_released_on_cancellation() {
    let mut fixture = host_and_publication().await;
    fixture.host = fixture.host.with_secure_setup(1, SetupLimits::default());
    let holder_peer = PeerID::from_public_key(holder().master_public_key());
    let reader_peer = PeerID::from_public_key(reader().master_public_key());
    let (server, client) = MemoryTransport::pair(holder_peer, reader_peer);
    let host = fixture.host.clone();
    let server = Arc::new(server);
    let first_server = Arc::clone(&server);
    let first = tokio::spawn(async move { host.accept_and_serve(&*first_server).await });
    let outer = client.connect(holder_peer, publish_alpn()).await.unwrap();
    let (mut secured, _) =
        secure_initiator_with_facts(reader().master_keypair(), outer, &publish_alpn())
            .await
            .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while fixture.host.setup_stats().secured == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.host.setup_stats().pending, 1);
    assert_eq!(fixture.host.setup_stats().secured, 1);
    let _second = client.connect(holder_peer, publish_alpn()).await.unwrap();
    assert!(matches!(
        fixture.host.accept_and_serve(&*server).await,
        Err(KnotPublishHostError::Carrier(PublishCarrierError::Setup(
            transport::security::SetupError::Capacity
        )))
    ));
    first.abort();
    let _ = first.await;
    assert_eq!(fixture.host.setup_stats().pending, 0);
    assert_eq!(fixture.host.setup_stats().cancelled, 1);
    assert_eq!(fixture.host.setup_stats().capacity_refused, 1);
    assert_eq!(fixture.host.active_sessions(), 0);
    assert!(read_frame(&mut secured, 1024).await.is_err());
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

#[path = "tests/connections.rs"]
mod connections;
