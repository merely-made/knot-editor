// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! R1 receipt: a first-party Turnstone client opens Knot through the resident
//! application door, after both local admission and an app-to-route grant.

use std::sync::Arc;
use std::time::Duration;

use graphshell::identity::{
    AgentListenerView, CarryView, IdentitySurfaceSnapshot, VaultLockView, VaultProtectionView,
    VaultView,
};
use graphshell::native::app_admission::{AllowedAppRoutes, AppId, AppRouteGrants, AppRouteId};
use graphshell::native::app_broker::{AppEndpointCatalog, serve_app_broker};
use graphshell::native::app_client::AppBrokerClient;
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::local_session::DoorIdentity;
use graphshell::native::resident_identity::ResidentIdentity;
use personae::{IdentityError, InMemoryProvider, RetainedKeys};

#[cfg(not(windows))]
struct SocketPath(std::path::PathBuf);

#[cfg(not(windows))]
impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A resident identity with fixed door keys and no custody route: the route
/// under test is Knot's, and only djinn links the real keeper (dramatis D4).
struct FixtureResident(InMemoryProvider);

impl DoorIdentity for FixtureResident {
    fn door_keys(&self) -> Result<Arc<RetainedKeys>, IdentityError> {
        self.0.door_keys()
    }
}

impl ResidentIdentity for FixtureResident {
    fn snapshot(&self) -> std::io::Result<IdentitySurfaceSnapshot> {
        Ok(IdentitySurfaceSnapshot {
            vault: VaultView {
                protection: VaultProtectionView::Ephemeral,
                lock: VaultLockView::Unlocked,
                agent: AgentListenerView::StandaloneRetained,
            },
            profiles: Vec::new(),
            ssh_keys: Vec::new(),
            carry: CarryView::default(),
            pending_signing: Vec::new(),
            signing_history: Vec::new(),
        })
    }

    fn apply_intent(&self, _intent: &str, _payload: &[u8]) -> Result<(), String> {
        Err("the fixture resident takes no identity intents".into())
    }
}

fn resident_host() -> Arc<FixtureResident> {
    Arc::new(FixtureResident(InMemoryProvider::from_seed([0xA1; 32])))
}

#[tokio::test(flavor = "multi_thread")]
async fn turnstone_opens_the_in_memory_knot_route() {
    let mut catalog = ResidentEndpointCatalog::new();
    catalog
        .register("knot", "Knot fixture", |_| {
            Ok(knot_editor::KnotEndpoint::fixture())
        })
        .unwrap();
    let route = ResidentEndpointRoute::new("knot", Duration::from_millis(10)).unwrap();
    let grants = AppRouteGrants::new(AllowedAppRoutes::new([(AppId::new("turnstone"), route)]));

    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\graphshell-knot-route-{}", uuid::Uuid::new_v4());
    #[cfg(not(windows))]
    let socket_path = SocketPath(
        std::path::Path::new("/tmp").join(format!("knot-route-{}.sock", uuid::Uuid::new_v4())),
    );
    #[cfg(not(windows))]
    let endpoint = socket_path.0.display().to_string();

    let server_endpoint = endpoint.clone();
    let server = tokio::spawn(async move {
        let _ = serve_app_broker(
            &server_endpoint,
            resident_host(),
            grants,
            60_000,
            None,
            AppEndpointCatalog::new(catalog),
        )
        .await;
    });

    let mut client = None;
    let mut last_error = String::new();
    for _ in 0..50 {
        match AppBrokerClient::open_route_at(
            &endpoint,
            AppId::new("turnstone"),
            AppRouteId::new("knot").unwrap(),
        )
        .await
        {
            Ok(open) => {
                client = Some(open);
                break;
            }
            Err(error) => {
                last_error = error.to_string();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    let mut client =
        client.unwrap_or_else(|| panic!("the Knot route never opened, last: {last_error}"));
    let opened = client.open_session().await.unwrap();
    let request = opened.descriptor.projections[0].request.clone();
    let snapshot = client.snapshot(request).await.unwrap();
    assert_eq!(
        snapshot.scene.active_item_count(),
        3,
        "the selected endpoint is Knot's deterministic fixture",
    );
    client.close().await.unwrap();
    server.abort();
    let _ = server.await;
}
