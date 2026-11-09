// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot's projection through the door, served by the supervised child (cycle
//! breaks P2 remainder, ruling 2). Ported from Mere's deleted
//! `knot_checkpoint_receiving`, which drove the resident's own Knot lane:
//! here the resident hosts only the door, the real `knot_sync_host` runs as
//! the supervised child, repairs the legacy checkpoint when it opens the
//! store, and offers the `knot` route as a projection provider. A viewer
//! (Turnstone's stand-in) reads and saves through that route.

#![cfg(all(unix, feature = "door-custody"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use castellan::authority::PersonaeHost;
use castellan::custody::{IdentityStorage, IdentityVault, InMemoryStorage, Profile};
use chirograph::{
    CarrierRequestBody, CarrierResponseBody, EditableTextV1, IntentInvocation, IntentResult,
    PresentationCodec, SaveTextV1,
};
use djinn::door::app_admission::{AllowedAppRoutes, AppRouteGrants};
use dramatis::view::VaultProtectionView;
use dramatis_resident::keeper::Keeper;
use graphshell_door::projection::AppBrokerClient;
use graphshell_host::endpoint_catalog::ResidentEndpointCatalog;
use knot_editor::{
    KnotSettings, KnotSyncSettings, KnotVault, PersonalVaultKeys, StartupUnlockedPersonalVault,
    VaultDocument, knot_settings_path, persona_vault_root,
};
use mere_resident::door::app_broker::{AppEndpointCatalog, serve_app_broker};
use mere_resident::door::projection_offers::ProviderPermission;
use mere_resident::resident_blob_door::BlobDoor;
use mere_resident::resident_blobs::ResidentBlobCustody;
use mere_resident::settings::{ResidentContentSettings, SupervisedAppSettings};
use mere_resident::supervisor::AppSupervisor;
use muniment::Backend;
use ostiary::app_admission::{AppId, AppRouteId};
use ostiary::declaration::{AppDeclaration, AppGrant, NamePattern};
use p2panda_core::Hash;
use personae::{Ed25519Keypair, PersonaId, ProfileId};
use zeroize::{Zeroize, Zeroizing};

const DEVICE: &str = "checkpoint-receiving-fixture";
const DOCUMENT: &str = "checkpoint-private-note";
const TITLE: &str = "Djinn private checkpoint title 717b330a";
const LEGACY_BODY: &str = "# Private legacy source e046cc49\n";
const CURRENT_BODY: &str = "# Current signed source b3b9b465\n";
const CHILD_BODY: &str = "# Saved through the supervised child's route a65bb21d\n";
const VIEWER: &str = "turnstone";
const ROUTE: &str = "knot";

fn persona() -> PersonaId {
    PersonaId::from_uuid(uuid::Uuid::from_u128(0xc022_2026_1022))
}

/// What the owner granted: Knot its custody, blob custody and the provider
/// route; the viewer nothing by grant (the provider permission admits it).
fn owner_grant() -> Vec<AppGrant> {
    serde_json::from_value(serde_json::json!([{
        "app": "knot-editor",
        "routes": [
            {"exact": "custody"},
            {"exact": "blob-custody-v1"},
            {"exact": "mere.projection-provider"}
        ],
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

fn keys(root: &Path) -> (PersonalVaultKeys, Zeroizing<[u8; 32]>) {
    let mut released = dramatis_resident::custody::epoch_keys(
        &dramatis_resident::custody::ReleasePolicy::none()
            .with_declared_epochs(&knot_editor::declaration::app_declaration().custody.epoch),
        root,
        persona(),
        Some(DEVICE),
        &PersonalVaultKeys::requests(),
    )
    .unwrap();
    let vault_key = Zeroizing::new(
        released
            .iter()
            .find(|key| key.request.context == knot_editor::VAULT_KEY_CONTEXT)
            .unwrap()
            .key,
    );
    let keys = PersonalVaultKeys::from_released(&released).unwrap();
    for release in &mut released {
        release.key.zeroize();
    }
    (keys, vault_key)
}

fn document(body: &str) -> VaultDocument {
    VaultDocument {
        id: DOCUMENT.into(),
        title: TITLE.into(),
        body: body.as_bytes().to_vec(),
        media_type: "text/djot".into(),
    }
}

fn database(root: &Path) -> PathBuf {
    persona_vault_root(root, persona()).join("knot/sync.redb")
}

fn assert_sealed(path: &Path) {
    let bytes = std::fs::read(path).unwrap();
    for secret in [TITLE, LEGACY_BODY, CURRENT_BODY, CHILD_BODY] {
        assert!(
            !bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes()),
            "plaintext source in active sync database"
        );
        let json_body = serde_json::to_vec(secret.as_bytes()).unwrap();
        assert!(
            !bytes
                .windows(json_body.len())
                .any(|window| window == json_body),
            "plaintext JSON byte array in active sync database"
        );
    }
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

async fn offered(catalog: &AppEndpointCatalog) -> bool {
    catalog
        .update(|catalog| catalog.offers().iter().any(|offer| offer.id == ROUTE))
        .await
}

async fn wait_offer(catalog: &AppEndpointCatalog, present: bool, message: &str) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while offered(catalog).await != present {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{message}"));
}

/// Open the route as the viewer and, optionally, perform the ordinary Save.
/// Returns the document's source as the route disclosed it.
async fn visit(endpoint: &str, replacement: Option<&str>) -> String {
    let mut client = AppBrokerClient::open_declared_route_at(
        endpoint,
        AppId::new(VIEWER),
        AppRouteId::new(ROUTE).unwrap(),
        AppDeclaration {
            routes: vec![NamePattern::exact(ROUTE)],
            ..AppDeclaration::default()
        },
    )
    .await
    .expect("the owner's permission admits the viewer to the offered route");
    let opened = client.open_session().await.unwrap();
    let request = opened.descriptor.projections[0].request.clone();
    let snapshot = client.snapshot(request).await.unwrap();
    for (instance, _) in snapshot.scene.active_items_in_order() {
        let Some(offers) = snapshot.presentation.offers_for(instance) else {
            continue;
        };
        let Some(offer) = offers
            .iter()
            .find(|offer| offer.codec == PresentationCodec::EditableTextV1)
        else {
            continue;
        };
        let bytes = client
            .resource(snapshot.session.clone(), offer.resource)
            .await
            .unwrap();
        let editable: EditableTextV1 = serde_json::from_slice(&bytes).unwrap();
        if !editable.address.ends_with(DOCUMENT) {
            continue;
        }
        if let Some(source) = replacement {
            let action = &offer.semantics.actions[0];
            let body = client
                .request_body(CarrierRequestBody::Intent(IntentInvocation {
                    session: snapshot.session.clone(),
                    target: instance,
                    observed_epoch: snapshot.scene.epoch,
                    observed_revision: snapshot.scene.revision,
                    intent: action.intent.0.clone(),
                    payload: serde_json::to_vec(&SaveTextV1 {
                        base_token: editable.base_token.clone(),
                        source: source.into(),
                    })
                    .unwrap(),
                }))
                .await
                .unwrap();
            assert_eq!(body, CarrierResponseBody::Intent(IntentResult::Accepted));
        }
        let _ = client.close().await;
        return editable.source;
    }
    panic!("the supervised child's route did not disclose the signed fixture document");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_supervised_child_repairs_a_legacy_checkpoint_and_serves_the_route() {
    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("shared");
    std::fs::create_dir_all(&data_root).unwrap();
    castellan::custody::wallet::ensure_wallet_state(&data_root, persona(), DEVICE).unwrap();
    // The child refuses to start without a sync section; nobody is paired.
    KnotSettings { sync: Some(KnotSyncSettings::default()) }
        .save(&knot_settings_path(&data_root, persona()))
        .unwrap();

    // The legacy state, as the old test built it: an earlier checkpoint shape
    // over a later signed source.
    let (released, vault_key) = keys(&data_root);
    let startup = StartupUnlockedPersonalVault::open(&data_root, persona(), released, []).unwrap();
    startup.author_document(document(LEGACY_BODY)).unwrap();
    let vault = KnotVault::open(persona_vault_root(&data_root, persona()), *vault_key).unwrap();
    let store = startup.store().clone();
    let previous = store.save_checkpoint(&vault).await.unwrap();
    let previous_digest = *blake3::hash(&serde_json::to_vec(&previous).unwrap()).as_bytes();
    startup.author_document(document(CURRENT_BODY)).unwrap();
    let projection = store.projection(&vault).await.unwrap();
    let current_head = projection.document_heads[DOCUMENT];
    let current = store
        .sync_store()
        .get_operation(&Hash::from(current_head))
        .await
        .unwrap()
        .unwrap();
    let current_seq = current.header.seq_num;
    let raw = store.sync_store();
    let suffix = knot_editor::hex32(&store.space_id());
    raw.backend()
        .put(
            &format!("knot-sync/checkpoint/{suffix}"),
            &serde_json::to_vec(&previous).unwrap(),
        )
        .await
        .unwrap();
    raw.backend()
        .delete(&format!("knot-sync/sealed-storage/{suffix}"))
        .await
        .unwrap();
    assert!(store.load_checkpoint(&vault).await.is_err());
    drop(raw);
    drop(store);
    drop(vault);
    drop(startup);
    let before = std::fs::read(database(&data_root)).unwrap();
    assert!(
        before
            .windows(TITLE.len())
            .any(|window| window == TITLE.as_bytes()),
        "legacy plaintext before-control"
    );

    // The resident: only the door, custody for the child, and the owner's
    // permission for the viewer to reach Knot's offered route.
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
    let mut resident_catalog = ResidentEndpointCatalog::new();
    let route = door.register(&mut resident_catalog).unwrap();
    let grants = AppRouteGrants::new(AllowedAppRoutes::none());
    grants.update(|routes| routes.serve(route));
    mere_resident::custody::grant(&grants).unwrap();
    mere_resident::applications::install(
        &grants,
        mere_resident::applications::validated(owner_grant()).unwrap(),
    );
    let catalog = AppEndpointCatalog::new(resident_catalog);
    catalog
        .set_provider_permissions(vec![ProviderPermission {
            provider: AppId::new(knot_editor::KNOT_APP_ID),
            route: AppRouteId::new(ROUTE).unwrap(),
            viewers: vec![AppId::new(VIEWER)],
            max_sessions: 4,
            timeout: Duration::from_secs(30),
        }])
        .await
        .expect("`knot` is no longer a reserved route: the resident composes no lane of its own");
    let endpoint = std::env::temp_dir()
        .join(format!("knot-route-{}.sock", uuid::Uuid::new_v4()))
        .display()
        .to_string();
    let keeper = Arc::new(Keeper::new(resident_host()).with_wallet_root(Some(data_root.clone())));
    let serving = endpoint.clone();
    let serving_catalog = catalog.clone();
    let server = tokio::spawn(async move {
        let _ = serve_app_broker(&serving, keeper, grants, 60_000, None, serving_catalog).await;
    });

    // The supervised child, as the resident's generic supervisor runs it.
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
            ],
        },
        &data_root,
    );
    wait_for(|| Path::new(&endpoint).exists(), "the door never bound").await;
    assert!(supervisor.start(&endpoint).unwrap());
    wait_offer(&catalog, true, "the supervised child never offered the knot route").await;

    // The child opened the legacy store, repaired it and serves the signed
    // source, not the stale checkpoint's.
    assert_eq!(visit(&endpoint, None).await, CURRENT_BODY);
    assert_sealed(&database(&data_root));

    // Lock: the supervisor ends the child, which releases the store, and the
    // offer is withdrawn with it.
    assert!(supervisor.stop().await.unwrap());
    wait_offer(&catalog, false, "the offer outlived the child").await;
    let (released, vault_key) = keys(&data_root);
    let authority =
        StartupUnlockedPersonalVault::open(&data_root, persona(), released, []).unwrap();
    assert!(
        !authority.checkpoint_repair().rewritten,
        "the child already completed storage replacement"
    );
    let vault = KnotVault::open(persona_vault_root(&data_root, persona()), *vault_key).unwrap();
    let repaired = authority
        .store()
        .load_checkpoint(&vault)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(
        *blake3::hash(&serde_json::to_vec(&repaired).unwrap()).as_bytes(),
        previous_digest
    );
    assert_eq!(
        repaired.snapshot.as_ref().unwrap().document_heads[DOCUMENT],
        current_head
    );
    assert_eq!(
        authority.store().projection(&vault).await.unwrap(),
        projection
    );
    drop(vault);
    drop(authority);

    // Unlock: a fresh child process continues the signed author, saving
    // through the route.
    assert!(supervisor.start(&endpoint).unwrap());
    wait_offer(&catalog, true, "the restarted child never offered the knot route").await;
    assert_eq!(visit(&endpoint, Some(CHILD_BODY)).await, CURRENT_BODY);
    assert!(supervisor.stop().await.unwrap());
    wait_offer(&catalog, false, "the offer outlived the restarted child").await;
    assert_sealed(&database(&data_root));
    let (released, vault_key) = keys(&data_root);
    let authority =
        StartupUnlockedPersonalVault::open(&data_root, persona(), released, []).unwrap();
    assert!(!authority.checkpoint_repair().rewritten);
    let vault = KnotVault::open(persona_vault_root(&data_root, persona()), *vault_key).unwrap();
    let projection = authority.store().projection(&vault).await.unwrap();
    assert_eq!(projection.documents[0].body, CHILD_BODY.as_bytes());
    let next_head = projection.document_heads[DOCUMENT];
    let next = authority
        .store()
        .sync_store()
        .get_operation(&Hash::from(next_head))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.header.seq_num, current_seq + 1);
    assert_eq!(
        next.header.backlink.as_ref().map(|hash| *hash.as_bytes()),
        Some(current_head)
    );

    server.abort();
    let _ = server.await;
    drop(door);
    drop(custody);
}
