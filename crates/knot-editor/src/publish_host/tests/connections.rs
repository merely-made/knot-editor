// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
        let (mut stream, noise) =
            secure_initiator_with_facts(reader().master_keypair(), outer.stream, &publish_alpn())
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
        let (mut stream, noise) =
            secure_initiator_with_facts(reader().master_keypair(), outer.stream, &publish_alpn())
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
