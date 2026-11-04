use mere_view::{ArchiveRequest, GraphModel, NodeEntry, NodeState};

use super::*;

fn catalog_model() -> MereViewModel {
    MereViewModel {
        title: "Knot graph".into(),
        hide_graph: true,
        graph: GraphModel {
            nodes: vec![NodeEntry {
                key: "doc".into(),
                label: "Doc".into(),
                state: NodeState::Available,
            }],
            relations: Vec::new(),
        },
        ..Default::default()
    }
}

fn pending_plan() -> PendingMergeV1 {
    PendingMergeV1 {
        schema: mere_overmap::wire::PENDING_MERGE_SCHEMA.into(),
        source: pandect::SessionId::from_uuid(uuid::Uuid::from_u128(7)),
        source_name: "Branch".into(),
        target: pandect::SessionId::from_uuid(uuid::Uuid::from_u128(8)),
        target_name: "Main".into(),
        conflicts: vec![pandect::MergeConflict {
            part: pandect::Part::Title(uuid::Uuid::from_u128(9)),
            base: Some(serde_json::json!("Base")),
            source: Some(serde_json::json!("Source")),
            target: Some(serde_json::json!("Target")),
            add_as_new: false,
        }],
        carried: Vec::new(),
        dropped: Vec::new(),
    }
}

fn apply_choices(
    plan: &PendingMergeV1,
    choices: &BTreeMap<String, MergeOption>,
) -> mere_overmap::wire::MergeResolutionV1 {
    use sceno::{Footprint, ProjectedItem, Representation, Scene, SourceRef, Transform2};
    let mut scene = Scene::new();
    let source = scene.intern_source(SourceRef::new(
        mere_overmap::wire::MERGE_SOURCE,
        plan.source.as_uuid().to_string(),
    ));
    scene.items.push(ProjectedItem {
        source,
        space: Scene::WORLD,
        transform: Transform2::translation(0.0, 0.0),
        footprint: Footprint::Circle { radius: 8.0 },
        representation: Representation::Card,
        layer: 0,
        visible: true,
        hit: None,
        channels: vec![],
    });
    let sessions = ProjectionSnapshot {
        version: chirograph::ProtocolVersion::V1,
        session: chirograph::ProjectionSession(mere_overmap::wire::MERE_SESSIONS.into()),
        scene: chirograph::SceneSnapshot::from_dense(
            scenotime::SceneEpoch(1),
            scenotime::Revision(1),
            scene,
        )
        .unwrap(),
        presentation: Default::default(),
        cache_policy: Default::default(),
    };
    let invocation = route::merge_intent(
        &MereViewRequest::Merge(MergeRequest::Apply),
        &sessions,
        plan,
        choices,
    )
    .expect("every conflict has a choice, so Apply is available");
    serde_json::from_slice(&invocation.payload).unwrap()
}

#[test]
fn replacement_for_dropped_choice_survives_unchanged_refresh_and_apply() {
    let mut plan = pending_plan();
    let part = plan.conflicts[0].part.clone();
    let key = route::conflict_key(&part);
    plan.dropped.push(pandect::DroppedChoice {
        part: part.clone(),
        choice: pandect::MergeChoice::Source,
        reason: "The conflict changed".into(),
    });
    let mut choices = BTreeMap::from([(key.clone(), MergeOption::Source)]);
    reconcile_merge_choices(&mut choices, None, Some(&plan));
    assert!(choices.is_empty(), "the new plan drops the old choice");
    // The same local update as MergeRequest::Choose, followed by its worker refresh.
    choices.insert(key.clone(), MergeOption::Target);
    reconcile_merge_choices(&mut choices, Some(&plan), Some(&plan.clone()));
    assert_eq!(choices.get(&key), Some(&MergeOption::Target));
    let applied = apply_choices(&plan, &choices);
    assert_eq!(applied.choices[0].part, part);
    assert_eq!(applied.choices[0].choice, pandect::MergeChoice::Target);
}

#[test]
fn changed_carried_choice_survives_unchanged_refresh_and_apply() {
    let mut plan = pending_plan();
    let part = plan.conflicts[0].part.clone();
    let key = route::conflict_key(&part);
    plan.carried.push(mere_overmap::wire::MergeChoiceV1 {
        part,
        choice: pandect::MergeChoice::Source,
    });
    let mut choices = BTreeMap::new();
    reconcile_merge_choices(&mut choices, None, Some(&plan));
    assert_eq!(choices.get(&key), Some(&MergeOption::Source));
    choices.insert(key.clone(), MergeOption::Target);
    reconcile_merge_choices(&mut choices, Some(&plan), Some(&plan.clone()));
    assert_eq!(choices.get(&key), Some(&MergeOption::Target));
    assert_eq!(
        apply_choices(&plan, &choices).choices[0].choice,
        pandect::MergeChoice::Target
    );

    // Full material equality defines a plan, including names and carry/drop details.
    let mut changed = plan.clone();
    changed.target_name = "Renamed main".into();
    reconcile_merge_choices(&mut choices, Some(&plan), Some(&changed));
    assert_eq!(
        choices.get(&key),
        Some(&MergeOption::Source),
        "a changed plan adopts once"
    );
    choices.insert(key.clone(), MergeOption::Target);
    reconcile_merge_choices(&mut choices, Some(&changed), Some(&changed));
    assert_eq!(choices.get(&key), Some(&MergeOption::Target));
}

#[test]
fn refresh_prunes_vanished_conflicts_and_clears_changed_or_absent_merge() {
    let plan = pending_plan();
    let key = route::conflict_key(&plan.conflicts[0].part);
    let mut choices = BTreeMap::from([
        (key.clone(), MergeOption::Target),
        ("vanished".into(), MergeOption::Source),
    ]);
    reconcile_merge_choices(&mut choices, Some(&plan), Some(&plan));
    assert_eq!(
        choices,
        BTreeMap::from([(key.clone(), MergeOption::Target)])
    );
    let mut another = plan.clone();
    another.source = pandect::SessionId::from_uuid(uuid::Uuid::from_u128(10));
    reconcile_merge_choices(&mut choices, Some(&plan), Some(&another));
    assert!(choices.is_empty());
    choices.insert(key, MergeOption::Target);
    reconcile_merge_choices(&mut choices, Some(&another), None);
    assert!(choices.is_empty());
}

/// Exercise the real client boundary: a failed descriptor refresh must
/// invalidate every cached action before another request can reach the wire.
#[cfg(unix)]
#[tokio::test]
async fn failed_refresh_purges_cached_actions_and_merge_choices() {
    use chirograph::{ProjectionSession, ProtocolVersion, SceneSnapshot};
    use ostiary::app_admission::AppHello;
    use ostiary::frame::{read_native_message_async, write_native_message_async};
    use ostiary::wire::BrowserChallenge;
    type AppMessage = ostiary::wire::AppMessage<chirograph::CarrierRequest, ()>;
    type AppHostMessage =
        ostiary::wire::AppHostMessage<chirograph::CarrierResponse, chirograph::CarrierNotice, ()>;
    use mere_overmap::wire::{MERE_SESSIONS, PENDING_MERGE_SCHEMA};
    use sceno::Scene;
    use scenotime::{Revision, SceneEpoch};

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let root = tempfile::Builder::new()
            .prefix("knot-refresh-")
            .tempdir_in("/tmp")
            .unwrap();
        let endpoint = root.path().join("door.sock");
        let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _: AppHello = read_native_message_async(&mut stream)
                .await
                .unwrap()
                .unwrap();
            write_native_message_async(
                &mut stream,
                &AppHostMessage::Challenge {
                    challenge: BrowserChallenge::fresh(),
                },
            )
            .await
            .unwrap();
            let connect: AppMessage = read_native_message_async(&mut stream)
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(connect, AppMessage::Connect { .. }));
            write_native_message_async(
                &mut stream,
                &AppHostMessage::Connected {
                    app: AppId::new(MERE_OVERLAY_APP),
                    session: "refresh-failure-test".into(),
                },
            )
            .await
            .unwrap();
            let request: AppMessage = read_native_message_async(&mut stream)
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(request, AppMessage::Request { request }
                if matches!(request.body, CarrierRequestBody::Open(_))));
            write_native_message_async(
                &mut stream,
                &AppHostMessage::Failure {
                    message: "refresh refused".into(),
                },
            )
            .await
            .unwrap();
            let next: Option<AppMessage> = read_native_message_async(&mut stream).await.unwrap();
            assert!(next.is_none(), "no stale action may reach the resident");
        });
        let client =
            AppBrokerClient::open_at(endpoint.to_str().unwrap(), AppId::new(MERE_OVERLAY_APP))
                .await
                .unwrap();
        let sessions = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: ProjectionSession(MERE_SESSIONS.into()),
            scene: SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), Scene::new()).unwrap(),
            presentation: Default::default(),
            cache_policy: Default::default(),
        };
        let id = pandect::SessionId::from_uuid(uuid::Uuid::from_u128(7));
        let mut overlay = MereOverlay {
            client,
            sessions: Some(sessions.clone()),
            archive: Some(sessions),
            merge: Some(PendingMergeV1 {
                schema: PENDING_MERGE_SCHEMA.into(),
                source: id,
                source_name: "Branch".into(),
                target: id,
                target_name: "Main".into(),
                conflicts: Vec::new(),
                carried: Vec::new(),
                dropped: Vec::new(),
            }),
            merge_choices: BTreeMap::from([("old conflict".into(), MergeOption::Source)]),
        };
        assert!(overlay.read().await.is_err());
        assert!(overlay.sessions.is_none());
        assert!(overlay.archive.is_none());
        assert!(overlay.merge.is_none());
        assert!(overlay.merge_choices.is_empty());
        for request in [
            MereViewRequest::Mint,
            MereViewRequest::Session(id, mere_view::SessionStep::Switch),
            MereViewRequest::Archive(ArchiveRequest::Save(id)),
            MereViewRequest::Archive(ArchiveRequest::Open("old archive".into())),
            MereViewRequest::Merge(MergeRequest::Choose(
                "old conflict".into(),
                MergeOption::Source,
            )),
            MereViewRequest::Merge(MergeRequest::Apply),
            MereViewRequest::Merge(MergeRequest::Cancel),
        ] {
            assert_eq!(
                overlay.take(&request).await.unwrap().as_deref(),
                Some("Read the resident's mere before acting on it.")
            );
        }
        drop(overlay);
        server.await.unwrap();
    })
    .await
    .expect("failed-refresh regression timed out");
}

#[test]
fn without_a_resident_the_layer_is_empty_and_the_catalog_graph_shows() {
    let mut model = catalog_model();
    model.can_mint = true;
    model.archive = Some(ArchiveModel::default());
    overlay(&mut model, None);
    assert!(model.sessions.is_empty());
    assert!(!model.can_mint);
    assert_eq!(model.archive, None);
    assert!(!model.hide_graph);
    assert_eq!(model.graph, catalog_model().graph);
    assert_eq!(model.title, "Knot graph");
}

#[test]
fn a_reading_adds_sessions_and_archive_and_keeps_the_catalog_graph() {
    let mut model = catalog_model();
    let reading = MereOverlayReading {
        title: "Divination".into(),
        sessions: Vec::new(),
        can_mint: true,
        archive: Some(ArchiveModel {
            can_compose: true,
            ..Default::default()
        }),
        merge: Some(MergeModel {
            source: pandect::SessionId::from_uuid(uuid::Uuid::from_u128(7)),
            source_name: "Branch".into(),
            target_name: "Main".into(),
            conflicts: Vec::new(),
            dropped: Vec::new(),
        }),
    };
    overlay(&mut model, Some(&reading));
    assert!(model.can_mint);
    assert_eq!(model.archive, reading.archive);
    assert_eq!(
        model.merge, reading.merge,
        "the pending merge shows as its card"
    );
    overlay(&mut model, None);
    assert_eq!(model.merge, None, "no resident, no merge");
    assert_eq!(model.graph, catalog_model().graph);
    assert_eq!(
        model.title, "Knot graph",
        "the catalog keeps the region's name"
    );
}

#[test]
fn only_session_archive_and_merge_requests_belong_to_the_layer() {
    assert!(is_overlay_request(&MereViewRequest::Mint));
    for step in [
        MergeRequest::Choose("part".into(), MergeOption::Source),
        MergeRequest::Apply,
        MergeRequest::Cancel,
    ] {
        assert!(
            is_overlay_request(&MereViewRequest::Merge(step)),
            "a merge step goes to the route"
        );
    }
    assert!(
        declined_without_resident(&MereViewRequest::Merge(MergeRequest::Apply)).contains("Merges")
    );
    assert!(is_overlay_request(&MereViewRequest::Archive(
        ArchiveRequest::Compose(Vec::new())
    )));
    assert!(!is_overlay_request(&MereViewRequest::Activate(
        "doc".into()
    )));
    assert!(!is_overlay_request(&MereViewRequest::Layout(
        "grid.default".into()
    )));
    assert!(!is_overlay_request(&MereViewRequest::Host("open".into())));
    assert!(
        declined_without_resident(&MereViewRequest::Archive(ArchiveRequest::Open("x".into())))
            .contains("Archives")
    );
}
