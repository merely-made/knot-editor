// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Serve a disposable resident for Knot's mere overmap acceptance receipt.
//!
//! This is a receipt fixture, not product code. It uses PUBLIC TEST KEYS ONLY,
//! in a new root marked `PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET`, and never
//! opens an installed vault or a default endpoint:
//! - a Knot persona store seeded the way `resident_v1_fixture` seeds one, so
//!   `knot --mere-root <root>/shared --mere-persona <uuid>` can attach;
//! - the `divination` mere ensured through the reservoir route, edited by
//!   "turnstone" and given extra named sessions, the way `resident_mere`'s
//!   tests seed one;
//! - the app broker served at `<endpoint>` with a Keeper whose wallet root is
//!   the fixture's shared root, so custody releases Knot's epoch keys.
//!
//! Usage: `knot_overmap_live_host <endpoint> <new-absolute-root>`. The root
//! also holds two linked documents, `essay.djot` and `field_notes.djot`, for
//! Knot's catalog. It writes `<root>/ready.json` once it serves, and stops on
//! a line on stdin. Knot's receipt `docs/receipts/2026-10-15_mere_overmap_live/`
//! has the commands.

use std::{path::PathBuf, sync::Arc, time::Duration};

use castellan::{
    authority::PersonaeHost,
    custody::{IdentityVault, InMemoryStorage, Profile},
};
use chirograph::{IntentInvocation, IntentResult, ProjectionSession};
use djinn::door::app_broker::{AppEndpointCatalog, serve_app_broker};
use djinn::{
    embedded_reservoir::reservoir_applications,
    keeper::Keeper,
    resident_mere::{MereRoutes, mere_route_id},
    resident_reservoir::{
        EnsureMereV1, RESERVOIR_ENSURE_MERE_INTENT, RESIDENT_RESERVOIR_ROUTE, ResidentReservoir,
    },
};
use graphshell_endpoint::door::{
    app_admission::{AllowedAppRoutes, AppRouteGrants},
    identity::VaultProtectionView,
    tasks::ResidentTasks,
};
use graphshell_endpoint::{IntentSink, ProjectionCatalog, ProjectionSource};
use graphshell_host::{
    endpoint_catalog::ResidentEndpointCatalog, lifecycle::AdmittedEndpointContext,
};
use knot_editor::{PersonalVaultKeys, StartupUnlockedPersonalVault, VaultDocument};
use mere_overmap::wire::{
    APPLY_EDITS_INTENT, ApplyEditsV1, MINT_SESSION_INTENT, SESSION_ITEM_SOURCE, SessionsActionV1,
};
use personae::{Ed25519Keypair, PersonaId, ProfileId};
use sceno::InstanceId;
use uuid::Uuid;

const MARKER: &str = "PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET";
const MARKER_CONTENTS: &[u8] =
    b"Synthetic acceptance authority only; all keys are public test constants.\n";
const PUBLIC_SEED: [u8; 32] = [0x4b; 32];
const DOMAIN: &str = "divination";
const DEVICE: &str = "knot-overmap-live";

fn persona() -> PersonaId {
    PersonaId::from_uuid(Uuid::from_u128(0x6b6e_6f74_2026_1015))
}

fn context(app: &str) -> AdmittedEndpointContext {
    AdmittedEndpointContext::new(ProjectionSession("v1:knot-overmap-live".into()), [0x2b; 32])
        .with_application(app)
}

fn node(n: u128) -> pandect::CapturedDelta {
    pandect::CapturedDelta::ReplayAddNodeWithIdIfMissing {
        id: Uuid::from_u128(n).to_string(),
        url: format!("https://{n}.test/"),
        position: [0.0, 0.0],
    }
}

fn retitle(n: u128, title: &str) -> pandect::CapturedDelta {
    pandect::CapturedDelta::ReplaySetNodeTitleById {
        node_id: Uuid::from_u128(n).to_string(),
        title: title.to_string(),
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_ansi(false)
        .init();
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        2,
        "usage: knot_overmap_live_host <endpoint> <new-absolute-root>"
    );
    let endpoint = args[0].clone();
    let root = PathBuf::from(&args[1]);
    assert!(root.is_absolute(), "root must be absolute");
    assert!(!root.exists(), "fixture root must be new");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(MARKER), MARKER_CONTENTS).unwrap();
    std::fs::write(
        root.join("essay.djot"),
        "# Night light\n\nNight light by the river.\n",
    )
    .unwrap();
    // Knot catalogues a document when it opens it, never by scanning the
    // root, so the receipt names both documents on Knot's command line.
    std::fs::write(
        root.join("field_notes.djot"),
        "# Field notes\n\nThe tide withdrew; see [Night light](essay.djot).\n",
    )
    .unwrap();
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let persona = persona();

    // Wallet with the public seed, then Knot's store, as resident_v1_fixture.
    castellan::custody::wallet::save_identity_seed(&shared, PUBLIC_SEED).unwrap();
    let seed = castellan::custody::wallet::ensure_wallet_state(&shared, persona, DEVICE).unwrap();
    assert_eq!(seed, PUBLIC_SEED, "fixture wallet uses the public seed");
    let released = djinn::custody::epoch_keys(
        &djinn::custody::ReleasePolicy::default(),
        &shared,
        persona,
        Some(DEVICE),
        &PersonalVaultKeys::requests(),
    )
    .unwrap();
    let keys = PersonalVaultKeys::from_released(&released).unwrap();
    let authority = StartupUnlockedPersonalVault::open(&shared, persona, keys, []).unwrap();
    authority
        .author_document(VaultDocument {
            id: "overmap-note".into(),
            title: "Overmap live note".into(),
            body: b"# Overmap live note\n".to_vec(),
            media_type: "text/djot".into(),
        })
        .unwrap();
    drop(authority);

    // Reservoir, routes and grants, as c1-receipt-host composes them.
    let reservoir = ResidentReservoir::open(&shared, Some(persona))
        .await
        .unwrap();
    let apps = reservoir_applications();
    let catalog = AppEndpointCatalog::new(ResidentEndpointCatalog::new());
    let grants = AppRouteGrants::new(AllowedAppRoutes::new(
        apps.iter()
            .cloned()
            .map(|app| (app, ResidentReservoir::route())),
    ));
    djinn::custody::grant(&grants).unwrap();
    let routes = MereRoutes::new(
        shared.clone(),
        persona,
        catalog.clone(),
        grants.clone(),
        apps,
    )
    .with_divination_authority();
    routes.load_access(&reservoir).await.unwrap();
    catalog
        .update(|catalog| reservoir.register(catalog, Some(routes.clone())))
        .await
        .unwrap();

    // Ensure the mere through the reservoir route, as the tests' fixture does.
    {
        let mut door = catalog
            .update(|catalog| catalog.open(RESIDENT_RESERVOIR_ROUTE, &context("turnstone")))
            .await
            .unwrap();
        let request = door.describe().projections.remove(0).request;
        let listed = door.snapshot(request).unwrap();
        let ensure = IntentInvocation {
            session: listed.session.clone(),
            target: InstanceId(0),
            observed_epoch: listed.scene.epoch,
            observed_revision: listed.scene.revision,
            intent: RESERVOIR_ENSURE_MERE_INTENT.into(),
            payload: serde_json::to_vec(&EnsureMereV1::new(DOMAIN)).unwrap(),
        };
        assert_eq!(door.invoke(ensure).unwrap(), IntentResult::Accepted);
    }

    // Turnstone attaches (minting the first session), mints two more named
    // sessions, then edits the first so it is the one changed last.
    {
        let route = mere_route_id(DOMAIN);
        let mut writer = catalog
            .update(|catalog| catalog.open(&route, &context("turnstone")))
            .await
            .unwrap();
        let sessions_request = writer.describe().projections[0].request.clone();
        let graph_request = writer.describe().projections[1].request.clone();
        for name in ["Evening reading", "Draft spread"] {
            let listed = writer.snapshot(sessions_request.clone()).unwrap();
            let mint = IntentInvocation {
                session: listed.session.clone(),
                target: InstanceId(0),
                observed_epoch: listed.scene.epoch,
                observed_revision: listed.scene.revision,
                intent: MINT_SESSION_INTENT.into(),
                payload: serde_json::to_vec(&SessionsActionV1::mint(Some(name.into()))).unwrap(),
            };
            assert_eq!(
                writer.invoke(mint).unwrap(),
                IntentResult::Accepted,
                "mint {name}"
            );
        }
        let graph = writer.snapshot(graph_request).unwrap();
        let item = graph
            .scene
            .active_items_in_order()
            .into_iter()
            .find_map(|(instance, item)| {
                graph.scene.tables.sources[item.source.0 as usize]
                    .as_ref()
                    .filter(|source| source.adapter == SESSION_ITEM_SOURCE)
                    .map(|_| instance)
            })
            .expect("graph shows the session item");
        let apply = IntentInvocation {
            session: graph.session.clone(),
            target: item,
            observed_epoch: graph.scene.epoch,
            observed_revision: graph.scene.revision,
            intent: APPLY_EDITS_INTENT.into(),
            payload: serde_json::to_vec(&ApplyEditsV1::new(vec![
                node(1),
                node(2),
                retitle(1, "Night light"),
                retitle(2, "River card"),
            ]))
            .unwrap(),
        };
        assert_eq!(writer.invoke(apply).unwrap(), IntentResult::Accepted);
    }

    // Serve the door.
    let host = Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(
            InMemoryStorage::new(),
            Profile::new(
                ProfileId("knot-overmap-live".into()),
                "Knot overmap live public fixture",
                Ed25519Keypair::from_seed(PUBLIC_SEED),
            ),
        ),
        None,
        VaultProtectionView::Ephemeral,
    ));
    let keeper = Arc::new(Keeper::new(host).with_wallet_root(Some(shared.clone())));
    let tasks = ResidentTasks::new();
    let (scope, door, address, served_grants) = (
        tasks.clone(),
        catalog.clone(),
        endpoint.clone(),
        grants.clone(),
    );
    let mut server = tokio::spawn(async move {
        scope
            .scope(serve_app_broker(
                &address,
                keeper,
                served_grants,
                600_000,
                None,
                door,
            ))
            .await
    });
    let mut ready = false;
    for _ in 0..200 {
        if graphshell_endpoint::door::local_endpoint::connect_local(&endpoint)
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        assert!(!server.is_finished(), "fixture resident failed to bind");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(ready, "fixture resident did not bind");

    // Self-check through the same door Knot uses, as `knot-editor`.
    {
        let mut overlay = knot_editor::mere_overlay::MereOverlay::open_at(&endpoint, DOMAIN)
            .await
            .expect("knot-editor admitted on the mere route");
        let reading = overlay.read().await.expect("reading");
        for session in &reading.sessions {
            println!(
                "FIXTURE_SESSION id={:?} name={:?} attached={}",
                session.id, session.name, session.attached
            );
        }
        println!(
            "FIXTURE_READING title={:?} sessions={} can_mint={} archive={:?}",
            reading.title,
            reading.sessions.len(),
            reading.can_mint,
            reading.archive
        );
    }
    std::fs::write(
        root.join("ready.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "knot.overmap-live-fixture/v1",
            "pid": std::process::id(),
            "persona": persona.as_uuid().to_string(),
            "endpoint": endpoint,
            "mere_root": shared,
            "domain": DOMAIN,
            "public_test_keys": true,
        }))
        .unwrap(),
    )
    .unwrap();
    println!(
        "FIXTURE_READY pid={} persona={} mere_root={}",
        std::process::id(),
        persona.as_uuid(),
        shared.display()
    );
    let stop = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    });
    tokio::select! {
        result = &mut server => { println!("FIXTURE_SERVER_ENDED {:?}", result.map(|r| r.map_err(|e| e.to_string()))); },
        _ = stop => { server.abort(); let _ = server.await; },
    }
    tasks.cancel_and_join().await;
    println!("FIXTURE_STOPPED pid={}", std::process::id());
}
