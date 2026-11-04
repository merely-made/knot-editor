// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::time::Duration;

use chirograph::{KnotClipArtifactRoleV1, KnotClipArtifactV1, PortableContentRefV1};
use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
use gemot::moot::{MOOT_ACT_ACTION, MOOT_DELEGATION_DOMAIN, MootAuthority, MootDelegations};
use insigne::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
    SignedDelegationCertificate, SignedDelegationRevocation,
};
use personae::delegation::Issue;
use personae::{IdentityProvider, InMemoryProvider};
use servitor::cap::Cap;
use servitor::cap_path;
use tempfile::tempdir;

use crate::{BlobClipEvidenceStore, KnotSyncEvent, KnotVault};

const SPACE: [u8; 32] = [0x51; 32];
const VAULT_KEY: [u8; 32] = [0x52; 32];
const MOOT: [u8; 32] = [0x53; 32];
const ROOT_GRANT: [u8; 32] = [0x54; 32];

/// A real Ed25519 public key: a peer id is a curve point, so an array of
/// repeated bytes will not parse as one.
fn writer(seed: u8) -> [u8; 32] {
    personae::Ed25519Keypair::from_seed([seed; 32])
        .public_key()
        .to_bytes()
}

fn peer(seed: u8, reachable: bool, connected: bool) -> KnownPeer {
    KnownPeer {
        peer: PeerID::from_bytes(&writer(seed)).expect("a valid peer key"),
        reachable,
        bootstrap: false,
        connected,
        on_overlay: true,
    }
}

#[test]
fn only_connected_peers_have_their_addresses_written_back() {
    let peers = [
        // Known and addressed, but nothing is flowing: its address may be
        // the stale one a good hint would replace.
        peer(1, true, false),
        // Actually talking: this address is true right now.
        peer(2, true, true),
        // Named by discovery, no address at all.
        peer(3, false, false),
    ];

    assert_eq!(
        writers_to_refresh(&peers),
        vec![writer(2)],
        "a reachable-but-silent peer must not overwrite a working hint"
    );
}

#[test]
fn nothing_is_written_back_when_no_device_is_talking() {
    let peers = [peer(1, true, false), peer(2, true, false)];
    assert!(
        writers_to_refresh(&peers).is_empty(),
        "an address book full of unreachable devices records no routes"
    );
}

#[test]
fn communal_blob_and_document_permissions_come_from_gemot_paths() {
    let founder = InMemoryProvider::from_seed([61; 32]);
    let document_writer = InMemoryProvider::from_seed([62; 32]);
    let evidence_reader = InMemoryProvider::from_seed([63; 32]);
    let outsider = InMemoryProvider::from_seed([64; 32]);
    let evidence_source = InMemoryProvider::from_seed([65; 32]);
    let space_scope = Cap::scope(&format!("knot/{}", crate::hex32(&SPACE))).unwrap();
    let document_scope = Cap::scope(&format!("knot/{}/document", crate::hex32(&SPACE))).unwrap();
    let evidence_read_scope =
        Cap::scope(&format!("knot/{}/evidence/read", crate::hex32(&SPACE))).unwrap();
    let evidence_source_scope =
        Cap::scope(&format!("knot/{}/evidence/source", crate::hex32(&SPACE))).unwrap();
    let mut rules = ConstitutionRules::founder_only(founder.master_public_key().to_bytes());
    rules.grant(CapabilityGrant {
        id: ROOT_GRANT,
        subject: founder.master_public_key().to_bytes(),
        path_prefix: cap_path(&space_scope),
        not_before_ms: 10,
        expires_at_ms: Some(1_000),
        delegation_depth: 2,
    });
    let issue = |subject: &InMemoryProvider, capability: &Cap, nonce: u8| {
        SignedDelegationCertificate::issue(
            &founder,
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_GRANT),
                founder.master_public_key().to_bytes(),
                subject.master_public_key().to_bytes(),
                CapabilityScope {
                    domain: MOOT_DELEGATION_DOMAIN.into(),
                    resource: MOOT.to_vec(),
                    path_prefix: cap_path(capability),
                    actions: [MOOT_ACT_ACTION.to_string()].into_iter().collect(),
                },
                15,
                20,
                Some(900),
                0,
                [nonce; 32],
            ),
        )
        .unwrap()
    };
    let mut delegations = MootDelegations::new();
    delegations
        .accept_certificate(MOOT, &rules, issue(&document_writer, &document_scope, 1))
        .unwrap();
    delegations
        .accept_certificate(
            MOOT,
            &rules,
            issue(&evidence_reader, &evidence_read_scope, 2),
        )
        .unwrap();
    delegations
        .accept_certificate(
            MOOT,
            &rules,
            issue(&evidence_source, &evidence_source_scope, 3),
        )
        .unwrap();
    let authority = MootAuthority {
        delegations: &delegations,
        rules: &rules,
        moot_id: MOOT,
        now_ms: 50,
    };
    let materialized = KnotSpaceAuthoritySnapshot::from_gemot_authority(
        &authority,
        SPACE,
        [
            outsider.master_public_key().to_bytes(),
            evidence_reader.master_public_key().to_bytes(),
            document_writer.master_public_key().to_bytes(),
            evidence_source.master_public_key().to_bytes(),
        ],
        [],
    )
    .unwrap();

    assert_eq!(
        materialized.writers().collect::<Vec<_>>(),
        vec![document_writer.master_public_key().to_bytes()]
    );
    assert!(
        materialized
            .evidence_readers()
            .any(|peer| peer == evidence_reader.master_public_key().to_bytes())
    );
    assert!(
        !materialized
            .evidence_readers()
            .any(|peer| peer == document_writer.master_public_key().to_bytes())
    );
    assert!(
        !materialized
            .writers()
            .any(|peer| peer == evidence_reader.master_public_key().to_bytes())
    );
    assert!(
        materialized
            .evidence_sources()
            .any(|peer| peer == evidence_source.master_public_key().to_bytes())
    );
    assert!(
        !materialized
            .evidence_sources()
            .any(|peer| peer == document_writer.master_public_key().to_bytes())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_gemot_grant_and_revocation_update_every_live_consumer() {
    let roots = tempdir().unwrap();
    let founder = InMemoryProvider::from_seed([66; 32]);
    let peer = InMemoryProvider::from_seed([67; 32]);
    let resident = InMemoryProvider::from_seed([68; 32]);
    let peer_id = peer.master_public_key().to_bytes();
    let space_scope = Cap::scope(&format!("knot/{}", crate::hex32(&SPACE))).unwrap();
    let document_scope = Cap::scope(&format!("knot/{}/document", crate::hex32(&SPACE))).unwrap();
    let evidence_read_scope =
        Cap::scope(&format!("knot/{}/evidence/read", crate::hex32(&SPACE))).unwrap();
    let evidence_source_scope =
        Cap::scope(&format!("knot/{}/evidence/source", crate::hex32(&SPACE))).unwrap();
    let mut rules = ConstitutionRules::founder_only(founder.master_public_key().to_bytes());
    rules.grant(CapabilityGrant {
        id: ROOT_GRANT,
        subject: founder.master_public_key().to_bytes(),
        path_prefix: cap_path(&space_scope),
        not_before_ms: 10,
        expires_at_ms: Some(1_000),
        delegation_depth: 2,
    });
    let issue = |capability: &Cap, nonce: u8| {
        SignedDelegationCertificate::issue(
            &founder,
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_GRANT),
                founder.master_public_key().to_bytes(),
                peer_id,
                CapabilityScope {
                    domain: MOOT_DELEGATION_DOMAIN.into(),
                    resource: MOOT.to_vec(),
                    path_prefix: cap_path(capability),
                    actions: [MOOT_ACT_ACTION.to_string()].into_iter().collect(),
                },
                15,
                20,
                Some(900),
                0,
                [nonce; 32],
            ),
        )
        .unwrap()
    };
    let certificates = [
        issue(&document_scope, 4),
        issue(&evidence_read_scope, 5),
        issue(&evidence_source_scope, 6),
    ];
    let mut delegations = MootDelegations::new();
    let empty = KnotSpaceAuthoritySnapshot::from_gemot_authority(
        &MootAuthority {
            delegations: &delegations,
            rules: &rules,
            moot_id: MOOT,
            now_ms: 50,
        },
        SPACE,
        [peer_id],
        [],
    )
    .unwrap();
    let store =
        KnotSyncFileStore::open_commons(roots.path().join("communal-authority.redb"), SPACE, [])
            .unwrap();
    let blobs = Arc::new(BlobStore::new());
    let bytes = b"communal evidence remains under owner custody";
    let portable = PortableContentRefV1::of(bytes);
    let reference: KnotClipEvidenceRef = serde_json::from_value(serde_json::json!({
        "content": portable,
        "media_type": "text/plain",
        "canonical_uri": "https://example.test/communal-evidence",
        "role": KnotClipArtifactRoleV1::SourceResponse,
    }))
    .unwrap();
    assert_eq!(
        blobs.put_bytes(bytes.to_vec()).await.unwrap(),
        reference.blob_hash().unwrap()
    );
    let mut host = KnotSyncHost::open_with_communal_evidence(
        &store,
        &resident.master_keypair().to_seed(),
        KnotSyncHostConfig {
            authority: empty,
            relay_urls: vec![],
        },
        Arc::clone(&blobs),
        4096,
    )
    .await
    .unwrap();
    assert!(host.retain_evidence_custody(&reference).unwrap());
    let scope = BlobScope::new(SPACE);
    let readers = host.evidence_authorizer().unwrap();
    let empty_revision = host.authority_revision();
    assert!(store.admitted_writers().is_empty());
    assert!(!readers.allows(scope, &peer_id, reference.blob_hash().unwrap()));
    assert!(matches!(
        host.fetch_evidence(&reference, peer_id).await,
        Err(KnotSyncHostError::EvidenceUnauthorized)
    ));

    for certificate in certificates.iter().cloned() {
        delegations
            .accept_certificate(MOOT, &rules, certificate)
            .unwrap();
    }
    let granted = KnotSpaceAuthoritySnapshot::from_gemot_authority(
        &MootAuthority {
            delegations: &delegations,
            rules: &rules,
            moot_id: MOOT,
            now_ms: 50,
        },
        SPACE,
        [peer_id],
        [],
    )
    .unwrap();
    assert!(host.apply_authority(granted).await.unwrap());
    let granted_revision = host.authority_revision();
    assert_ne!(granted_revision, empty_revision);
    assert_eq!(store.admitted_writers(), vec![peer_id]);
    assert!(readers.allows(scope, &peer_id, reference.blob_hash().unwrap()));
    assert_eq!(
        host.fetch_evidence(&reference, peer_id)
            .await
            .unwrap()
            .status,
        KnotEvidenceFetchStatus::AlreadyPresent
    );

    for (certificate, nonce) in certificates.iter().zip(7_u8..) {
        let revocation = DelegationRevocation::new(
            certificate.certificate.id(),
            founder.master_public_key().to_bytes(),
            certificate.certificate.scope.clone(),
            60,
            [nonce; 32],
        );
        delegations
            .accept_revocation(SignedDelegationRevocation::issue(&founder, revocation).unwrap())
            .unwrap();
    }
    let revoked = KnotSpaceAuthoritySnapshot::from_gemot_authority(
        &MootAuthority {
            delegations: &delegations,
            rules: &rules,
            moot_id: MOOT,
            now_ms: 70,
        },
        SPACE,
        [peer_id],
        [],
    )
    .unwrap();
    assert!(host.apply_authority(revoked).await.unwrap());
    assert_ne!(host.authority_revision(), granted_revision);
    assert_eq!(host.authority_revision(), empty_revision);
    assert!(store.admitted_writers().is_empty());
    assert!(!readers.allows(scope, &peer_id, reference.blob_hash().unwrap()));
    assert!(matches!(
        host.fetch_evidence(&reference, peer_id).await,
        Err(KnotSyncHostError::EvidenceUnauthorized)
    ));
    assert_eq!(host.read_evidence(&reference).await.unwrap(), bytes);

    host.close().await.unwrap();
    blobs.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paired_peers_replicate_djot_then_fetch_and_reopen_verified_evidence() {
    let roots = tempdir().unwrap();
    let alice = InMemoryProvider::from_seed([71; 32]);
    let bob = InMemoryProvider::from_seed([72; 32]);
    let alice_writer = alice.master_public_key().to_bytes();
    let bob_writer = bob.master_public_key().to_bytes();
    let writers = [alice_writer, bob_writer];
    let alice_store =
        KnotSyncFileStore::open(roots.path().join("alice.redb"), SPACE, writers).unwrap();
    let bob_store = KnotSyncFileStore::open(roots.path().join("bob.redb"), SPACE, writers).unwrap();
    let alice_vault = KnotVault::open(roots.path().join("alice-vault"), VAULT_KEY).unwrap();
    let bob_vault = KnotVault::open(roots.path().join("bob-vault"), VAULT_KEY).unwrap();

    let artifact = KnotClipArtifactV1 {
        role: KnotClipArtifactRoleV1::SourceResponse,
        media_type: "text/html".into(),
        canonical_uri: "https://example.test/source".into(),
        bytes: b"<main>source bytes travel separately</main>".to_vec(),
    };
    let alice_evidence_root = roots.path().join("alice-evidence");
    let bob_evidence_root = roots.path().join("bob-evidence");
    let alice_evidence = BlobClipEvidenceStore::open_async(&alice_evidence_root, 4096)
        .await
        .unwrap();
    let bob_evidence = BlobClipEvidenceStore::open_async(&bob_evidence_root, 4096)
        .await
        .unwrap();
    let reference = alice_evidence.retain_async(&artifact).await.unwrap();
    let provenance = serde_json::json!({
        "schema": "knot.clip.insert/v2",
        "evidence": [reference.clone()]
    });
    let source = format!(
        "# Replicated note\n\nThe authored body is ordinary Djot.\n\n```knot.clip.provenance\n{}\n```\n",
        serde_json::to_string(&provenance).unwrap()
    )
    .into_bytes();
    assert!(
        source
            .windows(artifact.bytes.len())
            .all(|window| window != artifact.bytes.as_slice())
    );
    let authored = VaultDocument {
        id: "portable-clip".into(),
        title: "Portable clip".into(),
        body: source,
        media_type: "text/vnd.djot".into(),
    };
    alice_store
        .author(
            &alice.master_keypair().to_seed(),
            &alice_vault,
            &KnotSyncEvent::Put(authored.clone()),
        )
        .await
        .unwrap();

    let alice_blobs = alice_evidence.resident_blob_store().unwrap();
    let bob_blobs = bob_evidence.resident_blob_store().unwrap();
    let mut alice_host = KnotSyncHost::open_with_evidence(
        &alice_store,
        &alice.master_keypair().to_seed(),
        KnotSyncHostConfig {
            authority: KnotSpaceAuthoritySnapshot::new(
                KnotAuthoritySource::PersonalPairing,
                [bob_writer],
                [bob_writer],
                [bob_writer],
                [],
            ),
            relay_urls: vec![],
        },
        Arc::clone(&alice_blobs),
        4096,
    )
    .await
    .unwrap();
    assert!(alice_host.retain_evidence_custody(&reference).unwrap());
    let alice_ticket = alice_host.ticket().await.unwrap();
    let bob_host = KnotSyncHost::open_with_evidence(
        &bob_store,
        &bob.master_keypair().to_seed(),
        KnotSyncHostConfig {
            authority: KnotSpaceAuthoritySnapshot::new(
                KnotAuthoritySource::PersonalPairing,
                [alice_writer],
                [alice_writer],
                [alice_writer],
                [(alice_writer, alice_ticket)],
            ),
            relay_urls: vec![],
        },
        Arc::clone(&bob_blobs),
        4096,
    )
    .await
    .unwrap();

    let replicated = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(document) = bob_store
                .projection(&bob_vault)
                .await
                .unwrap()
                .documents
                .into_iter()
                .find(|document| document.id == authored.id)
            {
                break document;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("paired Knot peers did not replicate the Djot operation");
    assert_eq!(replicated, authored);
    assert!(
        replicated
            .body
            .windows(reference.content_uri.len())
            .any(|window| window == reference.content_uri.as_bytes())
    );
    assert!(
        replicated
            .body
            .windows(artifact.bytes.len())
            .all(|window| window != artifact.bytes.as_slice())
    );

    let receipts = bob_host
        .fetch_document_evidence(&replicated, alice_writer)
        .await
        .unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].status, KnotEvidenceFetchStatus::Fetched);
    assert_eq!(
        bob_host.read_evidence(&reference).await.unwrap(),
        artifact.bytes
    );
    let second = bob_host
        .fetch_evidence(&reference, alice_writer)
        .await
        .unwrap();
    assert_eq!(second.status, KnotEvidenceFetchStatus::AlreadyPresent);
    let mut false_size = reference.clone();
    false_size.byte_size += 1;
    assert!(matches!(
        bob_host.read_evidence(&false_size).await,
        Err(KnotSyncHostError::EvidenceReference(_))
    ));

    let post_unpair_artifact = KnotClipArtifactV1 {
        role: KnotClipArtifactRoleV1::SourceResponse,
        media_type: "text/plain".into(),
        canonical_uri: "https://example.test/after-unpair".into(),
        bytes: b"fresh bytes retained after peer revocation".to_vec(),
    };
    let post_unpair_reference = alice_evidence
        .retain_async(&post_unpair_artifact)
        .await
        .unwrap();
    assert!(
        alice_host
            .retain_evidence_custody(&post_unpair_reference)
            .unwrap()
    );
    assert!(
        alice_host
            .apply_authority(KnotSpaceAuthoritySnapshot::default())
            .await
            .unwrap()
    );
    assert!(!alice_host.evidence_authorizer().unwrap().allows(
        BlobScope::new(SPACE),
        &bob_writer,
        post_unpair_reference.blob_hash().unwrap()
    ));
    assert!(matches!(
        bob_host
            .fetch_evidence(&post_unpair_reference, alice_writer)
            .await,
        Err(KnotSyncHostError::EvidenceBlob(_))
    ));
    assert_eq!(
        alice_host
            .read_evidence(&post_unpair_reference)
            .await
            .unwrap(),
        post_unpair_artifact.bytes
    );

    alice_host.close().await.unwrap();
    bob_host.close().await.unwrap();
    drop(alice_evidence);
    drop(bob_evidence);
    alice_blobs.shutdown().await.unwrap();
    bob_blobs.shutdown().await.unwrap();
    drop(alice_blobs);
    drop(bob_blobs);

    let reopened = BlobStore::open(&bob_evidence_root).await.unwrap();
    let offline = reopened
        .get_bytes(reference.blob_hash().unwrap())
        .await
        .unwrap();
    reference.verify_bytes(&offline).unwrap();
    assert_eq!(offline.as_ref(), artifact.bytes.as_slice());
    reopened.shutdown().await.unwrap();
}
