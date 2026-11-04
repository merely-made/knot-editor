// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use quinn::crypto::rustls::QuicClientConfig;
use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName, UnixTime};

fn source(operation: u8, body: &[u8]) -> KnotPublishedDocument {
    KnotPublishedDocument {
        publication: PublicationId::from_uuid(uuid::Uuid::from_u128(5)),
        media_type: "text/vnd.knot".into(),
        body: body.to_vec(),
        operation: [operation; 32],
        body_digest: *blake3::hash(body).as_bytes(),
    }
}

fn configured_adapter() -> MarkReadAdapter {
    let publication = PublicationId::from_uuid(uuid::Uuid::from_u128(5));
    let mut adapter = MarkReadAdapter::new(MarkReadAdapterLimits::default());
    adapter
        .configure_export(
            "/shares/field-notes.md",
            publication,
            MarkReadAccess::protected("reader-token"),
        )
        .unwrap();
    adapter
}

fn request(path: &str, metadata: &str) -> Vec<u8> {
    format!("FETCH {path}\n---\n{metadata}---\n").into_bytes()
}

#[test]
fn snapshots_are_commonmark_versions_with_a_sha256_chain() {
    let mut adapter = configured_adapter();
    let timestamp = MarkTimestamp::parse("2026-08-08T02:00:00Z").unwrap();
    assert_eq!(
        adapter
            .snapshot(
                "/shares/field-notes.md",
                &source(1, b"# Field notes\n\nFirst pass.\n"),
                timestamp.clone(),
            )
            .unwrap(),
        MarkSnapshotOutcome::Created(MarkVersionId(1))
    );
    assert_eq!(
        adapter
            .snapshot(
                "/shares/field-notes.md",
                &source(2, b"# Field notes\n\nSecond pass.\n"),
                MarkTimestamp::parse("2026-08-08T02:01:00Z").unwrap(),
            )
            .unwrap(),
        MarkSnapshotOutcome::Created(MarkVersionId(2))
    );
    let document = adapter.documents.get("/shares/field-notes.md").unwrap();
    let first = &document.versions[0];
    let second = &document.versions[1];
    assert!(
        std::str::from_utf8(&second.body)
            .unwrap()
            .contains("Second pass.")
    );
    assert!(
        std::str::from_utf8(second.stored())
            .unwrap()
            .contains(&format!("previous-hash: sha256-{}", first.etag()))
    );

    let response = adapter.respond(&request("/shares/field-notes.md", "auth: reader-token\n"));
    let wire = String::from_utf8(response.to_wire()).unwrap();
    assert!(wire.starts_with("---\nstatus: ok\n"));
    assert!(wire.contains("version: 2\n"));
    assert!(wire.contains("content-hash: sha256-"));
    assert!(wire.contains("Second pass."));
}

#[test]
fn conditional_and_denied_reads_do_not_reveal_an_export() {
    let mut adapter = configured_adapter();
    adapter
        .snapshot(
            "/shares/field-notes.md",
            &source(1, b"# Field notes\n\nOne.\n"),
            MarkTimestamp::parse("2026-08-08T02:00:00Z").unwrap(),
        )
        .unwrap();
    let current = adapter.documents["/shares/field-notes.md"]
        .versions
        .last()
        .unwrap();
    let conditional_request = request(
        "/shares/field-notes.md",
        &format!("auth: reader-token\nif-none-match: {}\n", current.etag()),
    );
    assert_eq!(
        adapter.respond(&conditional_request).status(),
        "not-modified"
    );
    assert_eq!(
        adapter
            .respond(&request("/shares/field-notes.md", "auth: wrong\n"))
            .status(),
        "not-found"
    );
    assert_eq!(
        adapter
            .respond(&request("/missing.md", "auth: wrong\n"))
            .status(),
        "not-found"
    );
}

#[test]
fn request_parser_rejects_bodies_and_path_traversal() {
    assert!(decode_mark_request(b"FETCH /safe.md\n").is_ok());
    assert!(decode_mark_request(b"FETCH /../safe.md\n").is_err());
    assert!(decode_mark_request(b"FETCH /safe.md\nbody").is_err());
    assert!(decode_mark_request(b"FETCH /safe.md\r\n").is_err());
}

#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _: &CertificateDer,
        _: &[CertificateDer],
        _: &ServerName,
        _: &[u8],
        _: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn insecure_mark_client() -> quinn::ClientConfig {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    tls.alpn_protocols = vec![MARK_ALPN.to_vec()];
    quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(tls).unwrap()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standard_quic_mark_alpn_serves_a_snapshot() {
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let certificate_der = CertificateDer::from(certificate.cert.der().to_vec());
    let private_key = PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der());
    let config = mark_server_config(vec![certificate_der], private_key.into()).unwrap();
    let mut adapter = configured_adapter();
    adapter
        .snapshot(
            "/shares/field-notes.md",
            &source(1, b"# Field notes\n\nOver QUIC.\n"),
            MarkTimestamp::parse("2026-08-08T02:00:00Z").unwrap(),
        )
        .unwrap();
    let expected_body = String::from_utf8(
        adapter.documents["/shares/field-notes.md"].versions[0]
            .body
            .clone(),
    )
    .unwrap();
    let host = Arc::new(
        MarkQuicHost::bind(
            "127.0.0.1:0".parse().unwrap(),
            config,
            Arc::new(RwLock::new(adapter)),
        )
        .unwrap(),
    );
    let address = host.local_addr().unwrap();
    let serving_host = Arc::clone(&host);
    let serving = tokio::spawn(async move { serving_host.serve_once().await });

    let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    client.set_default_client_config(insecure_mark_client());
    let connection = client.connect(address, "localhost").unwrap().await.unwrap();
    let (mut send, mut receive) = connection.open_bi().await.unwrap();
    send.write_all(&request("/shares/field-notes.md", "auth: reader-token\n"))
        .await
        .unwrap();
    send.finish().unwrap();
    let response = receive
        .read_to_end(MARK_MAX_DOCUMENT_BYTES + 1024)
        .await
        .unwrap();
    drop(send);
    drop(receive);
    drop(connection);
    serving.await.unwrap().unwrap();
    drop(host);
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("---\nstatus: ok\n"));
    assert!(response.ends_with(&expected_body));
    assert!(response.contains("Over QUIC."));
}
