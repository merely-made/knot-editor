// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot's overmap layer against a live mere's route (cycle breaks P2
//! remainder, ruling 3). Ported from Mere's `resident_mere` tests, which drove
//! `MereOverlay` against the module's private test `Fixture`; that fixture is
//! now public behind `mere-resident`'s `test-fixtures` feature, so Knot is a
//! consumer of the published crate.

#![cfg(feature = "mere-overlay")]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chirograph::{IntentInvocation, IntentResult, ProjectionSnapshot};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use graphshell_host::endpoint_catalog::ResidentEndpointSession;
use sceno::InstanceId;
use mere_overmap::route as mere_route;
use mere_overmap::wire::{
    self as session_item, ATTACH_SESSION_INTENT, ApplyEditsV1, BRANCH_SESSION_INTENT,
    SESSIONS_SOURCE, SessionsActionV1,
};
use mere_resident::resident_mere::fixture::{Fixture, fixture};
use mere_view::MereViewModel;
use pandect::SessionId;
use uuid::Uuid;

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

fn projection(session: &mut ResidentEndpointSession, index: usize) -> ProjectionSnapshot {
    let request = session.describe().projections[index].request.clone();
    session.snapshot(request).unwrap()
}

fn graph(session: &mut ResidentEndpointSession) -> ProjectionSnapshot {
    projection(session, 1)
}

fn session_item_of(snapshot: &ProjectionSnapshot) -> InstanceId {
    snapshot
        .scene
        .active_items_in_order()
        .into_iter()
        .find_map(|(instance, item)| {
            snapshot.scene.tables.sources[item.source.0 as usize]
                .as_ref()
                .filter(|source| source.adapter == session_item::SESSION_ITEM_SOURCE)
                .map(|_| instance)
        })
        .expect("the resident graph shows the session item")
}

fn apply(snapshot: &ProjectionSnapshot, edits: Vec<pandect::CapturedDelta>) -> IntentInvocation {
    IntentInvocation {
        session: snapshot.session.clone(),
        target: session_item_of(snapshot),
        observed_epoch: snapshot.scene.epoch,
        observed_revision: snapshot.scene.revision,
        intent: session_item::APPLY_EDITS_INTENT.into(),
        payload: serde_json::to_vec(&ApplyEditsV1::new(edits)).unwrap(),
    }
}

/// The sessions a list shows, oldest first.
fn session_ids(snapshot: &ProjectionSnapshot) -> Vec<SessionId> {
    snapshot
        .scene
        .active_items_in_order()
        .into_iter()
        .filter_map(|(_, item)| {
            snapshot.scene.tables.sources[item.source.0 as usize]
                .as_ref()
                .filter(|source| source.adapter == SESSIONS_SOURCE)
                .and_then(|source| Uuid::parse_str(&source.id).ok())
                .map(SessionId)
        })
        .collect()
}

/// A lifecycle step taken from `snapshot`. The payload names the session, so
/// the target is only a placeholder.
fn sessions_step(
    snapshot: &ProjectionSnapshot,
    intent: &str,
    payload: SessionsActionV1,
) -> IntentInvocation {
    IntentInvocation {
        session: snapshot.session.clone(),
        target: InstanceId(0),
        observed_epoch: snapshot.scene.epoch,
        observed_revision: snapshot.scene.revision,
        intent: intent.into(),
        payload: serde_json::to_vec(&payload).unwrap(),
    }
}

/// The route as the adapter reads it: both projections, the cards they name,
/// and the model they fill.
struct RouteRead {
    model: MereViewModel,
}

fn read_route(session: &mut ResidentEndpointSession) -> RouteRead {
    let (sessions, graph) =
        mere_route::requests(&session.describe()).expect("a mere's route offers both");
    let sessions = session.snapshot(sessions).unwrap();
    let graph = session.snapshot(graph).unwrap();
    let cards: HashMap<_, _> = mere_route::card_requests(&sessions)
        .into_iter()
        .map(|request| (request.resource, session.resource(request).unwrap().bytes))
        .collect();
    let mut model = MereViewModel::default();
    mere_route::fill(&mut model, &sessions, &graph, &cards);
    RouteRead { model }
}

fn label_of(model: &MereViewModel, n: u128) -> Option<String> {
    let key = Uuid::from_u128(n).to_string();
    model
        .graph
        .nodes
        .iter()
        .find(|node| node.key == key)
        .map(|node| node.label.clone())
}

/// Djinn's application door over `fixture`'s catalog, admitting Knot's
/// overlay as `knot-editor`, with a keeper over a fixed fixture profile.
/// Returns the door's address, its task scope and the server task.
async fn knot_door(
    fixture: &Fixture,
) -> (
    String,
    armillary::tasks::ResidentTasks,
    tokio::task::JoinHandle<Result<(), mere_resident::door::app_broker::AppBrokerError>>,
) {
    use mere_resident::door::app_broker::serve_app_broker;
    use armillary::tasks::ResidentTasks;
    use castellan::{
        authority::PersonaeHost,
        custody::{IdentityVault, InMemoryStorage, Profile},
    };
    use personae::{Ed25519Keypair, ProfileId};
    let host = Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(
            InMemoryStorage::new(),
            Profile::new(
                ProfileId("knot-overlay".into()),
                "Knot overlay fixture",
                Ed25519Keypair::from_seed([0x4b; 32]),
            ),
        ),
        None,
        dramatis::view::VaultProtectionView::Ephemeral,
    ));
    #[cfg(not(windows))]
    let address = format!("/tmp/knov-{}.sock", Uuid::new_v4());
    #[cfg(windows)]
    let address = format!(r"\\.\pipe\knov-{}", Uuid::new_v4());
    let tasks = ResidentTasks::new();
    let (scope, door, grants, socket) = (
        tasks.clone(),
        fixture.catalog.clone(),
        fixture.grants.clone(),
        address.clone(),
    );
    let server = tokio::spawn(async move {
        scope
            .scope(serve_app_broker(
                &socket,
                Arc::new(dramatis_resident::keeper::Keeper::new(host)),
                grants,
                60_000,
                None,
                door,
            ))
            .await
    });
    for _ in 0..100 {
        if ostiary::local_endpoint::connect_local(&address)
            .await
            .is_ok()
        {
            break;
        }
        assert!(!server.is_finished());
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    (address, tasks, server)
}

/// Knot's overmap layer (knot-editor `mere_overlay`) saves the attached
/// session to the archive and opens it again, through the first-party
/// door as `knot-editor`, and reads the codicil back from the route.
#[tokio::test(flavor = "multi_thread")]
async fn knot_overlay_saves_and_opens_an_archive_through_the_door() {
    use knot_editor::mere_overlay::MereOverlay;
    use mere_view::{ArchiveRequest, MereViewRequest};
    let fixture = fixture(35, "divination").await;
    let mut writer = fixture.attach("turnstone").await;
    let initial = graph(&mut writer);
    assert_eq!(
        writer.invoke(apply(&initial, vec![node(1)])).unwrap(),
        IntentResult::Accepted
    );
    let (address, tasks, server) = knot_door(&fixture).await;

    let mut overlay = MereOverlay::open_at(&address, "divination").await.unwrap();
    let before = overlay.read().await.unwrap();
    let attached = before
        .sessions
        .iter()
        .find(|session| session.attached)
        .expect("the route marks the attached session")
        .id;
    let archive = before.archive.expect("the route advertises its archive");
    assert!(archive.entries.is_empty());
    assert!(archive.save_sessions.contains(&attached));

    let saved = overlay
        .take(&MereViewRequest::Archive(ArchiveRequest::Save(attached)))
        .await
        .unwrap();
    assert_eq!(saved, None, "the route accepts Knot's save");
    let after = overlay.read().await.unwrap();
    let entries = after.archive.expect("the archive stays advertised").entries;
    assert_eq!(entries.len(), 1, "the saved codicil is read back");
    assert!(entries[0].can_open);
    assert_eq!(
        entries[0].session,
        Some(attached),
        "the codicil names the session it was saved from"
    );

    let opened = overlay
        .take(&MereViewRequest::Archive(ArchiveRequest::Open(
            entries[0].key.clone(),
        )))
        .await
        .unwrap();
    assert_eq!(opened, None, "the route accepts Knot's open");
    let names: Vec<String> = overlay
        .read()
        .await
        .unwrap()
        .sessions
        .into_iter()
        .map(|session| session.name)
        .collect();
    let source = before
        .sessions
        .iter()
        .find(|session| session.id == attached)
        .unwrap()
        .name
        .clone();
    assert_eq!(source, "Session");
    let thaw = format!(
        "Session (from {})",
        pandect::graph_codicil::codicil_short_id(&entries[0].key)
    );
    assert!(
        names.contains(&thaw),
        "the opened session is named apart from its source: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|name| **name == source).count(),
        1,
        "only the source keeps its name: {names:?}"
    );
    assert!(
        overlay
            .take(&MereViewRequest::Archive(ArchiveRequest::Compose(vec![
                "unknown".into(),
                "unknown".into(),
            ])))
            .await
            .unwrap()
            .is_some(),
        "an unadvertised compose is declined before it reaches the route"
    );
    assert!(
        tasks.close_and_join().await.is_clean(),
        "task shutdown failed"
    );
    server.abort();
}

/// Knot's overmap layer carries a merge to the route: it asks the branch
/// to merge into its source, reads the pending merge back as its card,
/// keeps a choice for the conflict, is told Apply waits for one, and
/// applies, through the first-party door as `knot-editor`.
#[tokio::test(flavor = "multi_thread")]
async fn knot_overlay_merges_a_branch_through_the_door() {
    use knot_editor::mere_overlay::MereOverlay;
    use mere_view::{MereViewRequest, MergeOption, MergeRequest, SessionStep};
    let fixture = fixture(37, "readings").await;
    let mut turnstone = fixture.attach("turnstone").await;
    let mut knot = fixture.attach("knot-editor").await;
    let start = graph(&mut turnstone);
    assert_eq!(
        turnstone
            .invoke(apply(&start, vec![node(1), retitle(1, "One")]))
            .unwrap(),
        IntentResult::Accepted
    );
    let listed = projection(&mut turnstone, 0);
    let parent = session_ids(&listed)[0];
    let branch = sessions_step(&listed, BRANCH_SESSION_INTENT, SessionsActionV1::on(parent));
    assert_eq!(turnstone.invoke(branch).unwrap(), IntentResult::Accepted);
    let listed = projection(&mut knot, 0);
    let branch = session_ids(&listed)[1];
    assert_eq!(
        knot.invoke(sessions_step(
            &listed,
            ATTACH_SESSION_INTENT,
            SessionsActionV1::on(branch)
        ))
        .unwrap(),
        IntentResult::Accepted
    );
    let in_branch = graph(&mut knot);
    assert_eq!(
        knot.invoke(apply(&in_branch, vec![retitle(1, "Branch")]))
            .unwrap(),
        IntentResult::Accepted
    );
    let in_parent = graph(&mut turnstone);
    assert_eq!(
        turnstone
            .invoke(apply(&in_parent, vec![retitle(1, "Parent")]))
            .unwrap(),
        IntentResult::Accepted
    );

    let (address, tasks, server) = knot_door(&fixture).await;
    let mut overlay = MereOverlay::open_at(&address, "readings").await.unwrap();
    let before = overlay.read().await.unwrap();
    assert_eq!(before.merge, None, "nothing is merging yet");
    assert_eq!(
        overlay
            .take(&MereViewRequest::Session(branch, SessionStep::Merge))
            .await
            .unwrap(),
        None,
        "the route takes Knot's merge into source"
    );
    let pending = overlay
        .read()
        .await
        .unwrap()
        .merge
        .expect("a pending merge");
    assert_eq!(pending.source, branch);
    assert_eq!(pending.conflicts.len(), 1, "the title conflicts");
    let key = pending.conflicts[0].key.clone();
    assert_eq!(pending.conflicts[0].chosen, None);

    assert!(
        overlay
            .take(&MereViewRequest::Merge(MergeRequest::Apply))
            .await
            .unwrap()
            .is_some(),
        "Apply waits for a choice on every conflict"
    );
    assert!(
        overlay
            .take(&MereViewRequest::Merge(MergeRequest::Choose(
                "unknown".into(),
                MergeOption::Source
            )))
            .await
            .unwrap()
            .is_some(),
        "a choice for a conflict that is not pending is declined"
    );
    assert_eq!(
        overlay
            .take(&MereViewRequest::Merge(MergeRequest::Choose(
                key.clone(),
                MergeOption::Source
            )))
            .await
            .unwrap(),
        None
    );
    let chosen = overlay.read().await.unwrap().merge.unwrap();
    assert_eq!(chosen.conflicts[0].chosen, Some(MergeOption::Source));
    assert!(chosen.resolved());

    assert_eq!(
        overlay
            .take(&MereViewRequest::Merge(MergeRequest::Apply))
            .await
            .unwrap(),
        None,
        "the route applies the merge"
    );
    assert_eq!(overlay.read().await.unwrap().merge, None);
    let read = read_route(&mut turnstone);
    assert_eq!(
        label_of(&read.model, 1).as_deref(),
        Some("Branch"),
        "the branch's title wins, as chosen"
    );
    assert!(
        tasks.close_and_join().await.is_clean(),
        "task shutdown failed"
    );
    server.abort();
}

