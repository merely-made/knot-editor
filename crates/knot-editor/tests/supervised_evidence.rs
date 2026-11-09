// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A peer fetches Knot's evidence from a supervised child through the
//! resident's blob door, and revocation cuts it off (cycle breaks P2 step 3).
//!
//! The resident (custody, the blob door, the app door) runs in this process
//! as Mere's own `blob_door` tests run it. The Knot sync host is a real child
//! process started by the resident's generic [`AppSupervisor`]: it holds no
//! blob store, takes its keys from custody, keeps its evidence in the
//! resident's custody and serves peers' iroh-blobs requests through a door
//! session. The peer is a second Knot host in this process, paired with the
//! child.

#![cfg(feature = "door-custody")]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use castellan::authority::PersonaeHost;
use castellan::custody::{IdentityStorage, IdentityVault, InMemoryStorage, Profile};
use chirograph::{KnotClipArtifactRoleV1, KnotClipArtifactV1};
use djinn::door::app_admission::{AllowedAppRoutes, AppRouteGrants};
use dramatis::view::VaultProtectionView;
use dramatis_resident::keeper::Keeper;
use graphshell_host::endpoint_catalog::ResidentEndpointCatalog;
use knot_editor::{
    KnotAuthoritySource, KnotClipEvidenceRef, KnotEvidenceFetchStatus, KnotSettings,
    KnotSpaceAuthoritySnapshot, KnotSyncFileStore, KnotSyncHost, KnotSyncHostConfig,
    KnotSyncHostError, KnotSyncSettings, PersonalVaultKeys,
    StartupUnlockedPersonalVault, VaultDocument, knot_settings_path,
};
use mere_resident::door::app_broker::{AppEndpointCatalog, serve_app_broker};
use mere_resident::resident_blob_door::BlobDoor;
use mere_resident::resident_blobs::ResidentBlobCustody;
use mere_resident::settings::{ResidentContentSettings, SupervisedAppSettings};
use mere_resident::supervisor::AppSupervisor;
use ostiary::declaration::AppGrant;
use personae::{Ed25519Keypair, PersonaId, ProfileId};
use transport::{BlobLease, BlobScope};

const DEVICE: &str = "supervised-evidence";

fn persona() -> PersonaId {
    PersonaId::from_uuid(uuid::Uuid::from_u128(0x6b6e_6f74_2026_1108))
}

/// What the owner granted Knot at the resident: custody for its declared
/// epochs and the blob custody route. The declaration test below holds the
/// real declaration to this.
fn owner_grant() -> Vec<AppGrant> {
    serde_json::from_value(serde_json::json!([{
        "app": "knot-editor",
        "routes": [{"exact": "custody"}, {"exact": "blob-custody-v1"}],
        "custody": {
            "operations": ["status", "roster", "watch_lock", "release_epoch_keys"],
            "epoch": [{"prefix": "mere.knot."}]
        }
    }]))
    .unwrap()
}

fn resident_host() -> Arc<PersonaeHost<InMemoryStorage>> {
    let profile = Profile::new(
        ProfileId("default".into()),
        "Default",
        Ed25519Keypair::from_seed([0x5e; 32]),
    );
    let storage = InMemoryStorage::new();
    storage.save_profile(&profile).unwrap();
    Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(storage, profile),
        None,
        VaultProtectionView::Ephemeral,
    ))
}

fn artifact(uri: &str, bytes: &[u8]) -> KnotClipArtifactV1 {
    KnotClipArtifactV1 {
        role: KnotClipArtifactRoleV1::SourceResponse,
        media_type: "text/plain".into(),
        canonical_uri: uri.into(),
        bytes: bytes.to_vec(),
    }
}

fn reference_to(artifact: &KnotClipArtifactV1) -> KnotClipEvidenceRef {
    KnotClipEvidenceRef::portable(artifact)
}

fn provenance(references: &[KnotClipEvidenceRef]) -> Vec<u8> {
    let block = serde_json::json!({
        "schema": "knot.clip.insert/v2",
        "evidence": references,
    });
    format!(
        "# Supervised evidence\n\n```knot.clip.provenance\n{}\n```\n",
        serde_json::to_string(&block).unwrap()
    )
    .into_bytes()
}

/// The held declaration, against the grant the resident was given.
#[test]
fn the_declaration_asks_for_what_the_owner_granted() {
    let declaration = knot_editor::declaration::app_declaration();
    assert!(declaration.declares_route(&ostiary::app_admission::AppRouteId::new("blob-custody-v1").unwrap()));
    let grant = owner_grant().remove(0);
    for request in PersonalVaultKeys::requests() {
        assert!(
            request.context.starts_with("mere.knot."),
            "{} is outside the granted epoch prefix",
            request.context
        );
    }
    assert_eq!(grant.app.as_str(), knot_editor::KNOT_APP_ID);
    // Mere's resident tests stand a vault application in for Knot with these
    // literals (`declared_applications.rs`); this holds them to the real one.
    let contexts: Vec<_> = PersonalVaultKeys::requests()
        .into_iter()
        .map(|request| (request.context, request.device_bound, request.public_only))
        .collect();
    assert_eq!(
        contexts,
        [
            ("mere.knot.persona-vault.root.v1".to_string(), false, false),
            ("mere.knot.persona-vault.writer.v1".to_string(), true, false),
            ("mere.knot.persona-vault.writer.v1".to_string(), false, true),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_peer_fetches_evidence_from_the_supervised_child_and_revocation_cuts_it_off() {
    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("shared");
    std::fs::create_dir_all(&data_root).unwrap();
    castellan::custody::wallet::ensure_wallet_state(&data_root, persona(), DEVICE).unwrap();

    // Keys as the resident's custody releases them, the same derivation the
    // child asks for through the door.
    let released = dramatis_resident::custody::epoch_keys(
        &dramatis_resident::custody::ReleasePolicy::none()
            .with_declared_epochs(&knot_editor::declaration::app_declaration().custody.epoch),
        &data_root,
        persona(),
        Some(DEVICE),
        &PersonalVaultKeys::requests(),
    )
    .unwrap();
    let child_keys = PersonalVaultKeys::from_released(&released).unwrap();
    let child_writer = child_keys.writer();

    // The peer: a second device, paired with the child both ways.
    let peer_seed = [0x77u8; 32];
    let peer_writer = *p2panda_core::SigningKey::from_bytes(&peer_seed)
        .verifying_key()
        .as_bytes();
    let mut sync = KnotSyncSettings::default();
    assert!(sync.pair(peer_writer));
    let settings_file = knot_settings_path(&data_root, persona());
    KnotSettings { sync: Some(sync) }.save(&settings_file).unwrap();

    // The child's vault: one document naming two evidence artifacts.
    let first = artifact("https://example.test/first", b"evidence served before revocation");
    let second = artifact("https://example.test/second", b"evidence asked for after revocation");
    let (first_ref, second_ref) = (reference_to(&first), reference_to(&second));
    let space = {
        let vault = StartupUnlockedPersonalVault::open(&data_root, persona(), child_keys, [peer_writer]).unwrap();
        vault
            .author_document(VaultDocument {
                id: "supervised".into(),
                title: "Supervised".into(),
                body: provenance(&[first_ref.clone(), second_ref.clone()]),
                media_type: "text/vnd.djot".into(),
            })
            .unwrap();
        vault.store().space_id()
    };

    // The resident: custody holds the evidence under Knot's lease, the door
    // serves the route, the owner's grant names it.
    let custody = ResidentBlobCustody::open(
        &temp.path().join("resident"),
        &ResidentContentSettings {
            gc_interval_seconds: 60,
            ..ResidentContentSettings::default()
        },
    )
    .await
    .unwrap();
    let scope = BlobScope::new(space);
    for artifact in [&first, &second] {
        let digest = blake3::hash(&artifact.bytes);
        let lease = BlobLease::new(scope, knot_editor::EVIDENCE_LEASE, digest.as_bytes()).unwrap();
        custody
            .blobs()
            .put_bytes_leased(artifact.bytes.clone(), &lease)
            .await
            .unwrap();
    }
    let door = BlobDoor::new(custody.clone());
    let mut catalog = ResidentEndpointCatalog::new();
    let route = door.register(&mut catalog).unwrap();
    let grants = AppRouteGrants::new(AllowedAppRoutes::none());
    grants.update(|routes| routes.serve(route));
    mere_resident::custody::grant(&grants).unwrap();
    mere_resident::applications::install(
        &grants,
        mere_resident::applications::validated(owner_grant()).unwrap(),
    );
    let endpoint = std::env::temp_dir()
        .join(format!("knot-supervised-{}.sock", uuid::Uuid::new_v4()))
        .display()
        .to_string();
    let keeper = Arc::new(Keeper::new(resident_host()).with_wallet_root(Some(data_root.clone())));
    let serving = endpoint.clone();
    let server = tokio::spawn(async move {
        let _ = serve_app_broker(&serving, keeper, grants, 60_000, None, AppEndpointCatalog::new(catalog)).await;
    });

    // The supervised child: the real sync host binary, in door-custody mode.
    let ticket_file = temp.path().join("ticket");
    let mut supervisor = AppSupervisor::new(
        SupervisedAppSettings {
            app: knot_editor::KNOT_APP_ID.into(),
            command: env!("CARGO_BIN_EXE_knot_sync_host").into(),
            args: vec![
                "{data_root}".into(),
                persona().as_uuid().to_string(),
                "--label".into(),
                DEVICE.into(),
                "--door-custody".into(),
                "--ticket-file".into(),
                ticket_file.display().to_string(),
            ],
        },
        &data_root,
    );
    // The door answers once it is bound; the child retries nothing, so wait.
    wait_for(|| Path::new(&endpoint).exists(), "the door never bound").await;
    assert!(supervisor.start(&endpoint).unwrap());
    wait_for(|| ticket_file.exists(), "the supervised child never listened").await;
    let ticket = std::fs::read_to_string(&ticket_file).unwrap();

    // The peer dials the child by its ticket and asks for the first artifact.
    let peer_store = KnotSyncFileStore::open(
        temp.path().join("peer.redb"),
        space,
        [child_writer, peer_writer],
    )
    .unwrap();
    let peer_blobs = Arc::new(
        transport::BlobStore::open(temp.path().join("peer-evidence"))
            .await
            .unwrap(),
    );
    let peer = KnotSyncHost::open_with_evidence(
        &peer_store,
        &peer_seed,
        KnotSyncHostConfig {
            authority: KnotSpaceAuthoritySnapshot::new(
                KnotAuthoritySource::PersonalPairing,
                [child_writer],
                [child_writer],
                [child_writer],
                [(child_writer, ticket)],
            ),
            relay_urls: vec![],
        },
        Arc::clone(&peer_blobs),
        4096,
    )
    .await
    .unwrap();

    let fetched = retry_fetch(&peer, &first_ref, child_writer).await;
    assert_eq!(fetched, KnotEvidenceFetchStatus::Fetched, "the child serves through the door");
    assert_eq!(peer.read_evidence(&first_ref).await.unwrap(), first.bytes);

    // Revocation: the door cuts the child's sessions and withdraws serving.
    door.revoke(knot_editor::KNOT_APP_ID).await.unwrap();
    let refused = peer.fetch_evidence(&second_ref, child_writer).await;
    assert!(
        matches!(refused, Err(KnotSyncHostError::EvidenceBlob(_))),
        "revocation cuts the child off: {refused:?}"
    );
    // Custody is kept: the bytes are still the resident's.
    assert!(custody
        .blobs()
        .leased_hashes(scope)
        .await
        .unwrap()
        .contains(&second_ref.blob_hash().unwrap()));

    // Lock: the supervisor ends the child.
    assert!(supervisor.stop().await.unwrap());
    peer.close().await.unwrap();
    server.abort();
    let _ = server.await;
    drop(door);
    drop(custody);
}

async fn wait_for(mut ready: impl FnMut() -> bool, message: &str) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{message}"));
}

/// The child may still be restoring its serving authority when the peer
/// first asks; ask again until it answers or the wait runs out.
async fn retry_fetch(
    peer: &KnotSyncHost,
    reference: &KnotClipEvidenceRef,
    from: [u8; 32],
) -> KnotEvidenceFetchStatus {
    let mut last = None;
    for _ in 0..120 {
        match peer.fetch_evidence(reference, from).await {
            Ok(receipt) => return receipt.status,
            Err(error) => last = Some(error),
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("the peer never fetched the evidence: {last:?}");
}

/// The door-backed variant of `KnotContentRetentionPort` writes evidence into
/// the resident's custody under Knot's lease and retains it for serving,
/// where the store-backed one writes it in this process.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_door_backed_retention_port_retains_into_the_residents_custody() {
    use graphshell_door::blob_door::BlobDoorClient;
    use knot_editor::{DoorEvidence, KnotContentRetentionPort};

    let temp = tempfile::tempdir().unwrap();
    let custody = ResidentBlobCustody::open(
        &temp.path().join("resident"),
        &ResidentContentSettings {
            gc_interval_seconds: 60,
            ..ResidentContentSettings::default()
        },
    )
    .await
    .unwrap();
    let door = BlobDoor::new(custody.clone());
    let mut catalog = ResidentEndpointCatalog::new();
    let route = door.register(&mut catalog).unwrap();
    let grants = AppRouteGrants::new(AllowedAppRoutes::none());
    grants.update(|routes| routes.serve(route));
    mere_resident::applications::install(
        &grants,
        mere_resident::applications::validated(owner_grant()).unwrap(),
    );
    let endpoint = std::env::temp_dir()
        .join(format!("knot-retention-{}.sock", uuid::Uuid::new_v4()))
        .display()
        .to_string();
    let keeper = Arc::new(Keeper::new(resident_host()));
    let serving = endpoint.clone();
    let server = tokio::spawn(async move {
        let _ = serve_app_broker(&serving, keeper, grants, 60_000, None, AppEndpointCatalog::new(catalog)).await;
    });
    wait_for(|| Path::new(&endpoint).exists(), "the door never bound").await;

    let declaration = knot_editor::declaration::app_declaration();
    let open = || {
        BlobDoorClient::open_declared_at(&endpoint, ostiary::app_admission::AppId::new(knot_editor::KNOT_APP_ID), declaration.clone())
    };
    let scope = BlobScope::new([0x42; 32]);
    let evidence = Arc::new(DoorEvidence::open(open().await.unwrap(), open().await.unwrap(), scope).await.unwrap());
    let port = KnotContentRetentionPort::open_door(Arc::clone(&evidence), 64).unwrap();
    assert!(port.blob_store().is_none(), "a door-backed port holds no store");

    let kept = artifact("https://example.test/retained", b"kept in the resident");
    let reference = port.retain_async(&kept).await.unwrap();
    assert_eq!(reference, reference_to(&kept));
    let hash = reference.blob_hash().unwrap();
    assert_eq!(
        custody.blobs().get_bytes(hash).await.unwrap().as_ref(),
        kept.bytes.as_slice(),
        "the bytes are in the resident's store"
    );
    assert!(
        custody.blobs().leased_hashes(scope).await.unwrap().contains(&hash),
        "under Knot's lease in the Knot scope"
    );
    // Retained for serving: a reader the owner names may then be answered.
    let reader = [0x09; 32];
    assert!(evidence.replace_readers_for_test(&[reader]).await);
    assert!(custody.authorizer().allows(scope, &reader, hash));

    // The limit is the port's.
    let too_big = artifact("https://example.test/big", &[0u8; 65]);
    assert!(port.retain_async(&too_big).await.unwrap_err().contains("limit"));

    drop(port);
    server.abort();
    let _ = server.await;
}
