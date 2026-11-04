// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use knot_file_catalog::KnotFileCatalog;
use tempfile::tempdir;

#[test]
fn catalog_reading_is_bounded_and_keeps_unavailable_nodes() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.djot"), "[B](b.djot)").unwrap();
    std::fs::write(root.join("b.djot"), "# B").unwrap();
    let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
    let a = catalog.bind("a.djot").unwrap();
    let b = catalog.bind("b.djot").unwrap();
    std::fs::remove_file(root.join("b.djot")).unwrap();
    let reading = read_catalog(catalog.snapshot(), 1024);
    assert_eq!(reading.graph.nodes.len(), 2);
    assert!(
        reading
            .graph
            .nodes
            .iter()
            .any(|node| node.key == b && node.state == NodeState::Unavailable)
    );
    assert!(
        reading
            .graph
            .relations
            .iter()
            .any(|edge| edge.from == a && edge.to == b)
    );
    assert_eq!(reading.label.len(), 12);

    let limited = read_catalog(catalog.snapshot(), 2);
    assert_eq!(limited.errors.len(), 1);
    assert!(limited.graph.relations.is_empty());
}

#[test]
fn open_nodes_merge_by_catalog_id_and_no_catalog_stays_truthful() {
    let mut state = GraphState::default();
    let open = [OpenNode {
        key: 7,
        catalog_id: None,
        label: "Draft".into(),
        dirty: true,
    }];
    let model = state.model(&open);
    assert_eq!(model.graph.nodes[0].key, "open:7");
    assert_eq!(model.graph.nodes[0].state, NodeState::Dirty);
    assert_eq!(model.status, ViewStatus::Ready);
    assert!(model.notice.as_deref().unwrap().contains("No catalog"));
    state.relayout(&open);
    assert!(state.view.position("open:7").is_some());
}

#[test]
fn a_resident_meres_sessions_show_as_the_overmap_tier_above_the_catalog() {
    let state = GraphState::default();
    let model = state.model(&[]);
    assert_eq!(model.placement, SessionPlacement::Tier);
    assert!(!model.hide_graph, "the catalog stays the graph");
    assert!(
        model.sessions.is_empty() && model.archive.is_none(),
        "without a resident there is no layer"
    );
}

#[test]
fn local_targets_cannot_escape_the_catalog() {
    assert_eq!(
        resolve_local(Path::new("notes/a.djot"), "../b.djot"),
        Some(PathBuf::from("b.djot"))
    );
    assert_eq!(resolve_local(Path::new("a.djot"), "../outside"), None);
    assert_eq!(
        resolve_local(Path::new("notes/a.djot"), "b%20note.djot#section"),
        Some(PathBuf::from("notes/b note.djot"))
    );
    assert_eq!(
        resolve_local(Path::new("a.djot"), "https://example.test"),
        None
    );
}

#[test]
fn overlapping_refreshes_run_one_worker_and_coalesce_one_latest_request() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.djot"), "# A").unwrap();
    let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
    catalog.bind("a.djot").unwrap();
    let snapshot = catalog.snapshot();
    let control = WorkerControl::default();
    let mut state = GraphState {
        worker_control: Some(control.clone()),
        ..GraphState::default()
    };
    state.enable_test_diagnostics();

    state.start(Some(snapshot.clone()), 64);
    control.wait_for_starts(1);
    for limit in [32, 16, 8] {
        state.invalidate_catalog();
        state.start(Some(snapshot.clone()), limit);
    }
    assert_eq!(
        control.starts(),
        1,
        "queued refreshes spawned overlapping workers"
    );

    control.release();
    for _ in 0..200 {
        if state.drain() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    control.wait_for_starts(2);
    assert_eq!(control.starts(), 2, "more than one queued refresh survived");
    control.release();
    for _ in 0..200 {
        if state.drain() && !state.busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!state.busy());
    assert!(state.reading.is_some());
    assert_eq!(control.starts(), 2);
    let batch = state.diagnostic_attachment().unwrap().unwrap();
    let phase_generations = |phase: &str| {
        batch
            .records
            .iter()
            .filter(|record| record.payload["phase"] == phase)
            .map(|record| record.payload["generation"].as_u64().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(phase_generations("catalog_requested"), [1, 2, 3, 4]);
    assert_eq!(phase_generations("worker_started"), [1, 4]);
    assert_eq!(phase_generations("queued_superseded"), [2, 3]);
    assert_eq!(phase_generations("discarded"), [1]);
    assert_eq!(phase_generations("accepted"), [4]);
    let installed = state.installed.as_ref().unwrap();
    assert_eq!(installed.generation, 4);
    let accepted = batch
        .records
        .iter()
        .find(|record| record.payload["phase"] == "accepted")
        .unwrap();
    assert_eq!(
        installed.acceptance.as_ref(),
        Some(&accepted.envelope.reference)
    );
    assert_eq!(accepted.envelope.metadata.cause, installed.outcome);
}

fn wait_for_drain(state: &mut GraphState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !state.drain() {
        assert!(
            std::time::Instant::now() < deadline,
            "real catalog worker did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn worker_execution_is_not_installed_early_and_invalidated_interest_discards_it() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("files");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("private-source.djot"), "private text").unwrap();
    let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
    catalog.bind("private-source.djot").unwrap();
    let control = WorkerControl::default();
    let mut state = GraphState {
        worker_control: Some(control.clone()),
        ..Default::default()
    };
    state.enable_test_diagnostics();
    state.start(Some(catalog.snapshot()), 1024);
    control.wait_for_starts(1);
    assert!(
        state.capture_facts().unwrap().installed.is_none(),
        "worker dispatch was installed before its outcome"
    );
    control.release();
    let diagnostics = state.diagnostics.clone().unwrap();
    let mut cursor = diagnostics.cursor().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if diagnostics
            .read(&mut cursor)
            .unwrap()
            .records
            .iter()
            .any(|record| record.payload["phase"] == "worker_completed")
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        state.capture_facts().unwrap().installed.is_none(),
        "worker completion was treated as host acceptance"
    );
    state.start(None, 1024);
    wait_for_drain(&mut state);
    assert!(
        state.reading.is_none(),
        "invalidated worker result was installed"
    );
    assert!(state.capture_facts().unwrap().installed.is_none());
    let batch = state.diagnostic_attachment().unwrap().unwrap();
    assert!(
        batch
            .records
            .iter()
            .any(|record| record.payload["phase"] == "discarded"
                && record.payload["result"] == "interest_unavailable")
    );
    assert!(
        !batch
            .records
            .iter()
            .any(|record| record.payload["phase"] == "accepted")
    );
    assert!(!serde_json::to_string(&batch).unwrap().contains("private"));
}

#[test]
fn exhausted_generation_never_aliases_a_request_or_installs_the_pending_result() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("files");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.djot"), "# A").unwrap();
    let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
    catalog.bind("a.djot").unwrap();
    let control = WorkerControl::default();
    let mut state = GraphState {
        desired_generation: u64::MAX - 1,
        worker_control: Some(control.clone()),
        ..Default::default()
    };
    state.enable_test_diagnostics();
    state.start(Some(catalog.snapshot()), 1024);
    control.wait_for_starts(1);
    state.invalidate_catalog();
    state.start(Some(catalog.snapshot()), 1024);
    assert!(state.generation_exhausted);
    assert_eq!(state.desired_generation, u64::MAX);
    assert!(state.queued.is_none());
    control.release();
    wait_for_drain(&mut state);
    assert!(state.reading.is_none());
    assert!(state.installed.is_none());
    assert_eq!(control.starts(), 1);
    let batch = state.diagnostic_attachment().unwrap().unwrap();
    assert_eq!(
        batch
            .records
            .iter()
            .filter(|record| record.payload["phase"] == "catalog_requested")
            .count(),
        1
    );
    assert!(
        batch
            .records
            .iter()
            .any(|record| record.payload["phase"] == "generation_exhausted")
    );
    assert!(
        !batch
            .records
            .iter()
            .any(|record| record.payload["phase"] == "accepted")
    );
}

#[test]
fn read_errors_are_an_explicit_real_worker_outcome_and_the_projection_is_frozen() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("files");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("private-source.djot"),
        "private text beyond the budget",
    )
    .unwrap();
    let mut catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
    catalog.bind("private-source.djot").unwrap();
    let control = WorkerControl::default();
    let mut state = GraphState {
        worker_control: Some(control.clone()),
        ..Default::default()
    };
    state.enable_test_diagnostics();
    state.start(Some(catalog.snapshot()), 1);
    control.wait_for_starts(1);
    control.release();
    wait_for_drain(&mut state);
    let frozen = serde_json::to_value(state.capture_facts().unwrap()).unwrap();
    assert_eq!(frozen["read_errors"], 1);
    assert_eq!(frozen["catalog_nodes"], 1);
    assert_eq!(frozen["catalog_relations"], 0);
    let batch = state.diagnostic_attachment().unwrap().unwrap();
    assert!(
        batch
            .records
            .iter()
            .any(|record| record.payload["phase"] == "worker_completed"
                && record.payload["result"] == "read_errors")
    );
    assert!(
        batch
            .records
            .iter()
            .any(|record| record.payload["phase"] == "accepted"
                && record.payload["result"] == "read_errors")
    );
    assert!(!serde_json::to_string(&frozen).unwrap().contains("private"));
    state.start(None, 1);
    assert!(state.capture_facts().unwrap().installed.is_none());
    assert!(frozen["installed"].is_object());
}
