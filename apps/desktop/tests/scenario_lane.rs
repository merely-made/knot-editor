// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot on Mesquite's scenario lane, headless: the lane clicks the desktop's
//! Appearance control by role and label and asserts the snapshot moved. The
//! headed form of the same run is `scenarios/lane_smoke.scn`.

use cambium_genet_winit_host::{Harness, Init, WindowCommands};
use knot_desktop::{
    desktop_sheet, host_hooks,
    scenario::KnotLane,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use knot_editor::{
    KnotAssertedTime, KnotCorePredicateV1, KnotPredicateRefV1, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotSyncEvent, KnotSyncFileStore, KnotVault, VaultDocument,
    capture_endpoint_context,
};
use knot_file_catalog::KnotFileCatalog;
use mesquite::LaneConfig;
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;

type Logic = fn(&DesktopState) -> DesktopView;

const SCENARIO: &str = "assert snap format == Djot
assert snap appearance_open == false
assert snap document_count == 1
assert snap reading_count == 0
assert snap graph_open == false
click role:button Appearance
settle 1
assert snap appearance_open == true
";

const STEP_9_SCENARIOS: &[&str] = &[
    "focused_writing.scn",
    "source_beside_preview.scn",
    "research_with_references.scn",
    "last_tab_graph.scn",
    "two_sites.scn",
    "context_navigation.scn",
    "context_scroll.scn",
    "context_scroll_reopen.scn",
];

fn scenario_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(name)
}

#[test]
fn composition_lane_refuses_retention_without_authority_and_preserves_source() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/composition/selection.djot");
    let before = std::fs::read(&fixture).unwrap();
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture.clone()),
    );
    state.set_preferences_path(Some(root.path().join("preferences.json")));
    let receipt =
        run_without_native_capture_at_height(state, "composition.scn", &root, 900.0, false);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
    assert!(!root.path().join("composition/collection.json").exists());
    assert_eq!(std::fs::read(fixture).unwrap(), before);
}

#[test]
fn every_step_9_scenario_uses_the_shared_parseable_lane() {
    for name in STEP_9_SCENARIOS {
        let path = scenario_path(name);
        let source = std::fs::read_to_string(&path).unwrap();
        taproot::Scenario::parse(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

#[test]
fn opened_link_swatch_reads_checkpointed_signed_knot_history() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/field_notes.djot");
    let author = InMemoryProvider::from_seed([0xa1; 32]);
    let seed = author.master_keypair().to_seed();
    let writer = author.master_public_key().to_bytes();
    let database = root.path().join("knot-relations.redb");
    let vault_path = root.path().join("relation-vault");
    let vault = KnotVault::open(&vault_path, [0xa2; 32]).unwrap();
    let store = KnotSyncFileStore::open(&database, [0xa3; 32], [writer]).unwrap();
    let essay_text = "# Signed essay\nA passage supports the source.\n";
    let source_text = "# Signed source\nA retained source passage.\n";
    let essay_put = pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::Put(VaultDocument {
            id: "essay".into(),
            title: "Signed essay".into(),
            body: essay_text.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }),
        KnotAssertedTime::At(1_600_000_000_000),
    ))
    .unwrap();
    let source_put = pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::Put(VaultDocument {
            id: "source".into(),
            title: "Signed source".into(),
            body: source_text.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }),
        KnotAssertedTime::At(1_600_000_000_100),
    ))
    .unwrap();
    let endpoint = |id: &str, head: [u8; 32], text: &str, quote: &str| {
        let start = text.find(quote).unwrap();
        let end = start + quote.len();
        let (prefix, suffix) = capture_endpoint_context(text, start, end);
        KnotRelationEndpointV1 {
            document_id: id.into(),
            document_head: head,
            quote: quote.into(),
            position: Some(KnotRelationPositionV1 {
                start: start as u64,
                end: end as u64,
            }),
            prefix,
            suffix,
        }
    };
    let essay = endpoint("essay", *essay_put.hash.as_bytes(), essay_text, "A passage");
    let source = endpoint(
        "source",
        *source_put.hash.as_bytes(),
        source_text,
        "retained source passage",
    );
    pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
            subject: essay.clone(),
            object: source.clone(),
            evidence: Some("urn:knot:test:evidence:signed".into()),
            qualification: Some("The highlighted passage supports this reading.".into()),
        },
        KnotAssertedTime::At(1_650_000_000_000),
    ))
    .unwrap();
    let withdrawn = pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Questions),
            subject: essay.clone(),
            object: source.clone(),
            evidence: None,
            qualification: Some("A second, later withdrawn reading.".into()),
        },
        KnotAssertedTime::At(1_660_000_000_000),
    ))
    .unwrap();
    pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::RetractRelation {
            assertion: *withdrawn.hash.as_bytes(),
        },
        KnotAssertedTime::At(1_670_000_000_000),
    ))
    .unwrap();
    pollster::block_on(store.author_at(
        &seed,
        &vault,
        &KnotSyncEvent::AssertRelation {
            predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Cites),
            subject: source,
            object: essay,
            evidence: None,
            qualification: Some("Reverse direction and unknown first time.".into()),
        },
        KnotAssertedTime::None,
    ))
    .unwrap();
    pollster::block_on(store.save_checkpoint(&vault)).unwrap();
    drop(store);
    drop(vault);

    let vault = KnotVault::open(&vault_path, [0xa2; 32]).unwrap();
    let reopened = KnotSyncFileStore::open(&database, [0xa3; 32], [writer]).unwrap();
    let projection = pollster::block_on(reopened.projection(&vault)).unwrap();
    assert_eq!(projection.relations.len(), 3);
    let with_retraction = projection
        .relations
        .iter()
        .find(|relation| !relation.retractions.is_empty())
        .unwrap();
    assert_eq!(with_retraction.asserted_at_ms, Some(1_660_000_000_000));
    assert_eq!(
        with_retraction.retractions[0].withdrawn_at_ms,
        Some(1_670_000_000_000)
    );
    let view = knot_desktop::knot_relations::KnotRelationView::from_projection(
        &projection,
        reopened.space_id(),
    )
    .unwrap();
    assert_eq!(view.held.links.len(), 1);
    assert_eq!(
        view.held.links[0]
            .buckets
            .iter()
            .map(|bucket| bucket.assertions.len())
            .sum::<usize>(),
        2
    );
    assert_eq!(
        view.history.links[0]
            .buckets
            .iter()
            .map(|bucket| bucket.assertions.len())
            .sum::<usize>(),
        3
    );
    let opened_link = &view.history.links[0];
    assert_eq!(opened_link.key.stratum, scenograph::link::LinkStratum::Surface);
    assert_eq!(opened_link.buckets.len(), 2, "opposite directions stay distinct");
    let assertions = opened_link
        .buckets
        .iter()
        .flat_map(|bucket| bucket.assertions.iter())
        .collect::<Vec<_>>();
    assert_eq!(assertions.iter().map(|item| item.id.as_str()).collect::<std::collections::BTreeSet<_>>().len(), 3);
    assert!(assertions.iter().all(|item| item.id.starts_with("urn:knot:assertion:")));
    let signed_support = assertions
        .iter()
        .find(|item| item.basis.note.as_deref() == Some("The highlighted passage supports this reading."))
        .unwrap();
    assert_eq!(signed_support.first_asserted_ms, Some(1_650_000_000_000));
    assert_eq!(signed_support.asserted_at_ms, Some(1_650_000_000_000));
    assert_eq!(
        signed_support.basis.evidence,
        vec!["urn:knot:test:evidence:signed".to_owned()]
    );
    assert!(signed_support.asserter.as_deref().unwrap().starts_with("urn:knot:author:"));
    let held_undated = assertions
        .iter()
        .find(|item| item.basis.note.as_deref() == Some("Reverse direction and unknown first time."))
        .unwrap();
    assert_eq!(held_undated.first_asserted_ms, None);
    assert_eq!(held_undated.asserted_at_ms, None);
    assert!(held_undated.withdrawn.is_none());
    let withdrawn_view = assertions
        .iter()
        .find(|item| item.basis.note.as_deref() == Some("A second, later withdrawn reading."))
        .unwrap();
    assert_eq!(withdrawn_view.first_asserted_ms, Some(1_660_000_000_000));
    assert_eq!(
        withdrawn_view.withdrawn.as_ref().and_then(|item| item.at_ms),
        Some(1_670_000_000_000)
    );
    assert!(withdrawn_view
        .withdrawn
        .as_ref()
        .and_then(|item| item.by.as_deref())
        .unwrap()
        .starts_with("urn:knot:author:"));
    let scenograph::link::LinkSource::Available { binding, revision } = &view.history.source else {
        panic!("the signed personal source binding is retained")
    };
    assert_eq!(binding.authority, "knot");
    assert_eq!(binding.domain, "personal");
    assert!(binding.resource.starts_with("urn:knot:space:"));
    assert!(matches!(revision, scenograph::link::LinkSourceRevision::Unavailable { .. }));
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture),
    );
    state.set_knot_relation_view(Some(view));
    let receipt = run_without_native_capture_at_size(
        state,
        "opened_link_knot.scn",
        &root,
        1100.0,
        900.0,
        1.0,
        false,
    );
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
}

#[test]
fn relationship_recipe_lane_edits_explains_and_refuses_unauthorized_retention() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/composition/selection.djot");
    let before = std::fs::read(&fixture).unwrap();
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture.clone()),
    );
    state.set_preferences_path(Some(root.path().join("preferences.json")));
    let receipt = run_without_native_capture_at_height(
        state,
        "relationship_recipe.scn",
        &root,
        1100.0,
        false,
    );
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
    assert!(!root.path().join("composition/collection.json").exists());
    assert_eq!(std::fs::read(fixture).unwrap(), before);
}

fn run_without_native_capture(state: DesktopState, name: &str, temp: &tempfile::TempDir) -> String {
    run_without_native_capture_at_height(state, name, temp, 700.0, false)
}

fn run_without_native_capture_at_height(
    state: DesktopState,
    name: &str,
    temp: &tempfile::TempDir,
    height: f32,
    check_toolbar: bool,
) -> String {
    run_without_native_capture_at_size(state, name, temp, 1100.0, height, 1.0, check_toolbar)
}

#[test]
fn compact_relationship_recipe_selects_the_offscreen_occurrence() {
    for (width, zoom) in [(420.0, 1.0), (1280.0, 4.0)] {
        let root = tempdir().unwrap();
        let fixture = scenario_path("fixtures/composition/selection.djot");
        let before = std::fs::read(&fixture).unwrap();
        let mut state = DesktopState::with_path(
            KnotDocumentSession::open(&fixture).unwrap(),
            WindowCommands::new(),
            Some(fixture.clone()),
        );
        state.set_preferences_path(Some(root.path().join("preferences.json")));
        let receipt = run_without_native_capture_at_size(
            state,
            "relationship_recipe_compact.scn",
            &root,
            width,
            900.0,
            zoom,
            false,
        );
        assert!(
            receipt.starts_with("RESULT ok"),
            "width {width}, zoom {zoom}: {receipt}"
        );
        assert_eq!(std::fs::read(&fixture).unwrap(), before);
    }
}

#[test]
fn compact_font_role_picker_is_reachable_through_the_menu_at_420px() {
    let root = tempdir().unwrap();
    let mut state = DesktopState::with_path(
        KnotDocumentSession::scratch("scratch:font-role-compact", "# Compact font roles\n"),
        WindowCommands::new(),
        None,
    );
    state.set_preferences_path(Some(root.path().join("preferences.json")));
    state
        .theme_session
        .load(scenario_path("fixtures/font_role_library.json"));

    let sheet = knot_desktop::theme_session::stylesheet(&state);
    let receipt = run_without_native_capture_at_size_with_chrome_and_sheet(
        state,
        "font_roles_compact.scn",
        &root,
        420.0,
        1000.0,
        1.0,
        false,
        knot_desktop::workspace::CommandChrome::ClientTitlebar,
        sheet,
    );
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
}

#[test]
fn relationship_scene_controls_preserve_source_at_wide_compact_and_high_zoom() {
    for (name, width, zoom) in [
        ("relationship_scene.scn", 1100.0, 1.0),
        ("relationship_scene_compact.scn", 420.0, 1.0),
        ("relationship_scene_compact.scn", 1280.0, 4.0),
    ] {
        let root = tempdir().unwrap();
        let fixture = scenario_path("fixtures/composition/selection.djot");
        let before = std::fs::read(&fixture).unwrap();
        let mut state = DesktopState::with_path(
            KnotDocumentSession::open(&fixture).unwrap(),
            WindowCommands::new(),
            Some(fixture.clone()),
        );
        state.set_preferences_path(Some(root.path().join("preferences.json")));
        let receipt =
            run_without_native_capture_at_size(state, name, &root, width, 900.0, zoom, false);
        assert!(
            receipt.starts_with("RESULT ok"),
            "{name}, {width}, {zoom}: {receipt}"
        );
        assert_eq!(std::fs::read(&fixture).unwrap(), before);
    }
}

fn run_without_native_capture_at_size(
    state: DesktopState,
    name: &str,
    temp: &tempfile::TempDir,
    width: f32,
    height: f32,
    zoom: f32,
    check_toolbar: bool,
) -> String {
    run_without_native_capture_at_size_with_chrome(
        state,
        name,
        temp,
        width,
        height,
        zoom,
        check_toolbar,
        knot_desktop::workspace::CommandChrome::PlainRow,
    )
}

fn run_without_native_capture_at_size_with_chrome(
    state: DesktopState,
    name: &str,
    temp: &tempfile::TempDir,
    width: f32,
    height: f32,
    zoom: f32,
    check_toolbar: bool,
    chrome: knot_desktop::workspace::CommandChrome,
) -> String {
    run_without_native_capture_at_size_with_chrome_and_sheet(
        state,
        name,
        temp,
        width,
        height,
        zoom,
        check_toolbar,
        chrome,
        desktop_sheet(),
    )
}

fn run_without_native_capture_at_size_with_chrome_and_sheet(
    mut state: DesktopState,
    name: &str,
    temp: &tempfile::TempDir,
    width: f32,
    height: f32,
    zoom: f32,
    check_toolbar: bool,
    chrome: knot_desktop::workspace::CommandChrome,
    sheet: String,
) -> String {
    state.set_command_chrome(chrome);
    let source = std::fs::read_to_string(scenario_path(name)).unwrap();
    let source = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("capture "))
        .collect::<Vec<_>>()
        .join("\n");
    let scenario = temp.path().join(name);
    std::fs::write(&scenario, source).unwrap();
    let config = LaneConfig {
        scenario,
        capture_dir: None,
        receipt: Some(temp.path().join(format!("{name}.done"))),
    };
    let receipt = config.receipt_path().unwrap();
    let mut lane = mesquite::Lane::from_config(
        config,
        KnotLane::new(sheet.clone()),
        cambium_genet_winit_host::read_file,
    )
    .unwrap();
    let mut hooks = host_hooks();
    let mut app_after_frame = hooks.after_frame;
    hooks.after_frame = Box::new(move |ctx| {
        app_after_frame(ctx);
        lane.after_frame(ctx);
    });
    let mut h = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as Logic,
            sheet,
            fonts: if name.contains("compact") {
                knot_desktop::fonts::bundled_fonts()
            } else {
                Vec::new()
            },
            images: Vec::new(),
        },
        hooks,
    );
    h.set_ui_zoom(zoom);
    let wake = h.wake();
    h.update(|state| state.set_retention_targets(Vec::new(), wake));
    let toolbar = taproot::Selector::class("knot-workspace-toolbar");
    let mut toolbar_before = None;
    for _ in 0..1_000 {
        h.layout_at(width, height);
        // Recipe cards are measured in the frame hook, as in the native loop.
        if name.starts_with("relationship_") || name == "font_roles_compact.scn" {
            h.prepare_frame();
            h.relayout();
        }
        if check_toolbar && toolbar_before.is_none() {
            toolbar_before = h.resolve(&toolbar);
        }
        h.after_frame();
        h.drain_pointer();
        h.after_dispatch();
        h.process_wake();
        if h.close_requested() {
            break;
        }
    }
    let text = std::fs::read_to_string(&receipt).expect("the lane wrote its receipt");
    assert!(h.close_requested(), "{name} did not finish: {text}");
    if check_toolbar {
        let before = toolbar_before.expect("the toolbar starts laid out");
        let after = h.resolve(&toolbar).expect("the toolbar remains laid out");
        assert_eq!(
            after.1, before.1,
            "focusing the clipped source must not scroll the command toolbar"
        );
    }
    text
}

#[test]
fn revealing_a_clipped_focused_textbox_keeps_the_command_toolbar_stationary() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/field_notes.djot");
    let state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture),
    );
    let receipt =
        run_without_native_capture_at_height(state, "focused_writing.scn", &root, 420.0, true);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
}

#[test]
fn every_step_9_scenario_passes_through_the_real_desktop_controls() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/field_notes.djot");
    let readings = scenario_path("fixtures/readings");

    for name in ["focused_writing.scn", "source_beside_preview.scn"] {
        let state = DesktopState::with_path(
            KnotDocumentSession::open(&fixture).unwrap(),
            WindowCommands::new(),
            Some(fixture.clone()),
        );
        let receipt = run_without_native_capture(state, name, &root);
        assert!(receipt.starts_with("RESULT ok"), "{name}: {receipt}");
    }

    let mut research = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture.clone()),
    );
    research.set_readings_root(Some(readings));
    let receipt = run_without_native_capture(research, "research_with_references.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");

    let catalog = KnotFileCatalog::open(
        scenario_path("fixtures"),
        root.path().join("step-9-catalog.redb"),
    )
    .unwrap();
    let graph = DesktopState::with_catalog(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture),
        Some(catalog),
    );
    let receipt = run_without_native_capture(graph, "last_tab_graph.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");

    let first = scenario_path("fixtures/site-one");
    let second = scenario_path("fixtures/site-two");
    let mut sites = DesktopState::with_catalog(
        KnotDocumentSession::open(first.join("index.scroll")).unwrap(),
        WindowCommands::new(),
        Some(first),
        None,
    );
    sites.attach_site(&second);
    sites.open_behind(
        vec![KnotDocumentSession::open(second.join("index.scroll")).unwrap()],
        &[],
    );
    let receipt = run_without_native_capture(sites, "two_sites.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
}

#[test]
fn contextual_navigation_scenario_uses_the_visible_shared_controls() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/context_navigation.djot");
    let original = std::fs::read(&fixture).unwrap();
    let state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture.clone()),
    );
    let receipt = run_without_native_capture(state, "context_navigation.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
}

#[test]
fn contextual_scroll_scenario_returns_to_source_and_preview_positions() {
    let root = tempdir().unwrap();
    let fixture = scenario_path("fixtures/context_scroll.djot");
    let original = std::fs::read(&fixture).unwrap();
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&fixture).unwrap(),
        WindowCommands::new(),
        Some(fixture.clone()),
    );
    state.set_preferences_path(Some(root.path().join("preferences.json")));
    let receipt = run_without_native_capture(state, "context_scroll.scn", &root);
    assert!(receipt.starts_with("RESULT ok"), "{receipt}");
    assert_eq!(std::fs::read(&fixture).unwrap(), original);
}

#[test]
fn the_lane_drives_the_desktop_by_role_and_label() {
    let root = tempdir().unwrap();
    let document = root.path().join("field_notes.djot");
    std::fs::write(&document, "# Field notes\n\nOpening.\n").unwrap();
    let scenario = root.path().join("appearance.scn");
    std::fs::write(&scenario, SCENARIO).unwrap();
    let config = LaneConfig {
        scenario,
        capture_dir: Some(root.path().to_path_buf()),
        receipt: None,
    };
    let receipt = config.receipt_path().unwrap();
    let mut lane = mesquite::Lane::from_config(
        config,
        KnotLane::new(desktop_sheet()),
        cambium_genet_winit_host::read_file,
    )
    .unwrap();
    let mut hooks = host_hooks();
    let mut app_after_frame = hooks.after_frame;
    hooks.after_frame = Box::new(move |ctx| {
        app_after_frame(ctx);
        lane.after_frame(ctx);
    });
    let mut state = DesktopState::with_path(
        KnotDocumentSession::open(&document).unwrap(),
        WindowCommands::new(),
        Some(document.clone()),
    );
    state.set_command_chrome(knot_desktop::workspace::CommandChrome::PlainRow);
    let mut h = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as Logic,
            sheet: desktop_sheet(),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        hooks,
    );
    for _ in 0..200 {
        h.layout_at(1100.0, 700.0);
        h.after_frame();
        h.drain_pointer();
        h.after_dispatch();
        if h.close_requested() {
            break;
        }
    }
    let text = std::fs::read_to_string(&receipt).expect("the lane wrote its receipt");
    assert!(text.starts_with("RESULT ok"), "{text}");
    assert!(
        h.close_requested(),
        "a finished lane asks the host to close"
    );
}
