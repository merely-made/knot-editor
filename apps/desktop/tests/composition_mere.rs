#![cfg(feature = "mere-retention")]
// SPDX-License-Identifier: MPL-2.0

//! Windowless end-to-end receipt: the desktop collects only into a host-issued
//! Knot mere capability, whose sealed signed operation survives a reopen.

use std::{sync::Arc, time::Duration};

use cambium_genet_winit_host::{Harness, Init, WindowCommands};
use genet_scripted_dom::{NodeId, ScriptedDom};
use knot_capture::KnotPersonaDisplayV1;
use knot_composition::retention::CompositionRetainPort;
use knot_desktop::{
    desktop_sheet, host_hooks,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use knot_editor::{
    CompositionGrant, KnotResidentCompositionPort, KnotResidentSource, KnotSyncFileStore, KnotVault,
};
use layout_dom_api::LayoutDom;
use personae::{IdentityProvider, InMemoryProvider};
use taproot::Selector;

type DesktopHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

fn text_content(dom: &ScriptedDom, node: NodeId) -> String {
    let own = dom.text(node).unwrap_or_default();
    let children = dom
        .dom_children(node)
        .map(|child| text_content(dom, child))
        .collect::<String>();
    format!("{own}{children}")
}

fn panel_text(host: &DesktopHarness) -> String {
    let dom = host.runner().dom();
    let dom = dom.borrow();
    text_content(&dom, dom.document())
}

fn wait_for_confirmation(host: &mut DesktopHarness, count: usize) {
    let expected = format!("Confirmed {count} retained items");
    for _ in 0..1_000 {
        if panel_text(host).contains(&expected) {
            // The worker's confirmation can enter the retained DOM from the
            // post-dispatch hook after the wake's layout. Deliver the next
            // redraw before resolving controls newly inserted by that result.
            host.relayout();
            return;
        }
        host.process_wake();
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "desktop collection worker did not confirm {count} items: {}",
        panel_text(host)
    );
}

#[test]
fn desktop_collection_retains_in_sealed_signed_mere_and_reopens_without_sidecar() {
    let root = tempfile::tempdir().unwrap();
    let document_path = root.path().join("essay.djot");
    let source = "a café by the river";
    std::fs::write(&document_path, source).unwrap();

    let identity = InMemoryProvider::from_seed([0x71; 32]);
    let seed = identity.master_keypair().to_seed();
    let writer = identity.master_public_key().to_bytes();
    let vault_path = root.path().join("vault");
    let store_path = root.path().join("sync.redb");
    let vault = KnotVault::open(&vault_path, [0x72; 32]).unwrap();
    let store = KnotSyncFileStore::open(&store_path, [0x73; 32], [writer]).unwrap();
    let read_vault = KnotVault::open(&vault_path, [0x72; 32]).unwrap();
    pollster::block_on(store.save_checkpoint(&read_vault)).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();
    let port = KnotResidentCompositionPort::new(
        KnotPersonaDisplayV1 {
            stable_id: "persona:test-mere".into(),
            label: "Test mere".into(),
        },
        resident
            .composition_retention(CompositionGrant::new(8192, 20))
            .unwrap(),
    )
    .unwrap();
    let target = port.target().clone();
    let target_port: Arc<dyn CompositionRetainPort> = Arc::new(port);

    let session = KnotDocumentSession::open(&document_path).unwrap();
    let mut state =
        DesktopState::with_path(session, WindowCommands::new(), Some(document_path.clone()));
    let snapshot = state.document().snapshot();
    state
        .document_mut()
        .session_mut()
        .select_source_span(&snapshot.source.address, &snapshot.text, 2, 7)
        .unwrap();
    let prefs = root.path().join("settings").join("preferences.json");
    state.set_preferences_path(Some(prefs));
    let mut host = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: desktop_sheet(),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    let wake = host.wake();
    host.update(|state| state.set_composition_targets(vec![target_port], wake));
    host.layout_at(1100.0, 800.0);

    assert!(host.click_on(&Selector::role("button").containing("Readings")));
    host.after_dispatch();
    assert!(host.click_on(&Selector::role("button").containing("Collection")));
    host.after_dispatch();
    assert!(host.click_on(&Selector::role("button").containing("Test mere")));
    wait_for_confirmation(&mut host, 0);
    assert!(host.click_on(&Selector::role("button").containing("Collect selected passage")));
    wait_for_confirmation(&mut host, 1);

    let list = resident
        .composition_retention(CompositionGrant::new(8192, 20))
        .unwrap();
    let read_port = KnotResidentCompositionPort::new(
        KnotPersonaDisplayV1 {
            stable_id: "persona:test-mere".into(),
            label: "Test mere".into(),
        },
        list,
    )
    .unwrap();
    let items = read_port.list(&target).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item.text, "café");
    assert_eq!(
        items[0].item.document_source.as_ref().unwrap().exact_quote,
        "café"
    );
    let operation = items[0].receipt.operation;
    assert_eq!(
        pollster::block_on(store.tail_receipt()).unwrap().operations,
        vec![operation]
    );
    assert!(host.click_on(&Selector::role("button").containing("Archive collection item")));
    wait_for_confirmation(&mut host, 1);
    assert!(!panel_text(&host).contains("Restore collection item"));
    assert!(host.click_on(&Selector::role("button").containing("Include archived items")));
    host.after_dispatch();
    host.relayout();
    assert!(panel_text(&host).contains("Restore collection item"));
    assert!(host.click_on(&Selector::role("button").containing("Restore collection item")));
    wait_for_confirmation(&mut host, 1);
    assert!(!read_port.list(&target).unwrap()[0].organization.archived);
    assert!(host.click_on(&Selector::role("button").containing("Archive collection item")));
    wait_for_confirmation(&mut host, 1);
    let archived_revision = read_port.list(&target).unwrap()[0].organization_revision;
    assert_ne!(archived_revision, operation);
    assert_eq!(std::fs::read_to_string(&document_path).unwrap(), source);
    assert!(
        !root
            .path()
            .join("settings/composition/collection.json")
            .exists()
    );

    drop(host);
    drop(read_port);
    drop(resident);
    drop(read_vault);
    drop(store);
    let bytes = std::fs::read(&store_path).unwrap();
    assert!(
        !bytes
            .windows("café".len())
            .any(|slice| slice == "café".as_bytes())
    );

    let reopened_vault = KnotVault::open(&vault_path, [0x72; 32]).unwrap();
    let reopened_store = KnotSyncFileStore::open(&store_path, [0x73; 32], [writer]).unwrap();
    let reopened =
        KnotResidentSource::from_synced_vault(reopened_vault, reopened_store, seed).unwrap();
    let reopened_port = KnotResidentCompositionPort::new(
        KnotPersonaDisplayV1 {
            stable_id: "persona:test-mere".into(),
            label: "Test mere".into(),
        },
        reopened
            .composition_retention(CompositionGrant::new(8192, 20))
            .unwrap(),
    )
    .unwrap();
    let after = reopened_port.list(reopened_port.target()).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].item, items[0].item);
    assert_eq!(after[0].receipt.operation, operation);
    assert!(after[0].organization.archived);
    assert_eq!(after[0].organization_revision, archived_revision);
}

#[test]
fn edited_relationship_recipe_roundtrips_through_real_desktop_worker_and_mere() {
    let root = tempfile::tempdir().unwrap();
    let document = root.path().join("poem.djot");
    std::fs::write(&document, "night night light").unwrap();
    let identity = InMemoryProvider::from_seed([0x61; 32]);
    let seed = identity.master_keypair().to_seed();
    let writer = identity.master_public_key().to_bytes();
    let vault_path = root.path().join("vault");
    let database = root.path().join("sync.redb");
    let persona = KnotPersonaDisplayV1 {
        stable_id: "persona:recipe".into(),
        label: "Recipe mere".into(),
    };
    let make_host = |port: Arc<dyn CompositionRetainPort>| {
        let session = KnotDocumentSession::open(&document).unwrap();
        let mut state =
            DesktopState::with_path(session, WindowCommands::new(), Some(document.clone()));
        let snapshot = state.document().snapshot();
        state
            .document_mut()
            .session_mut()
            .select_source_span(&snapshot.source.address, &snapshot.text, 0, 17)
            .unwrap();
        state.set_preferences_path(Some(root.path().join("settings/preferences.json")));
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        let wake = host.wake();
        host.update(|state| state.set_composition_targets(vec![port], wake));
        host.layout_at(1100.0, 800.0);
        host
    };
    let open_resident = || {
        KnotResidentSource::from_synced_vault(
            KnotVault::open(&vault_path, [0x62; 32]).unwrap(),
            KnotSyncFileStore::open(&database, [0x63; 32], [writer]).unwrap(),
            seed,
        )
        .unwrap()
    };
    let click = |host: &mut DesktopHarness, label: &str| {
        let selector = Selector::role("button").containing(label);
        let matches = host.with_dom(|dom| taproot::matching(dom, &selector));
        let geometry: Vec<_> = matches
            .iter()
            .map(|node| (host.painted_rect(*node), host.visible_rect(*node)))
            .collect();
        assert!(
            host.click_on(&selector),
            "unclickable button {label}, geometry {geometry:?}: {}",
            panel_text(host)
        );
        host.after_dispatch();
    };
    let (saved, operation) = {
        let resident = open_resident();
        let port = Arc::new(
            KnotResidentCompositionPort::new(
                persona.clone(),
                resident
                    .composition_retention(CompositionGrant::new(65536, 20))
                    .unwrap(),
            )
            .unwrap(),
        );
        let mut host = make_host(port.clone());
        click(&mut host, "Readings");
        click(&mut host, "Collection");
        click(&mut host, "Recipe mere");
        wait_for_confirmation(&mut host, 0);
        click(&mut host, "Sound");
        click(&mut host, "Enable bundled CMUdict");
        click(&mut host, "Perfect rhyme");
        click(&mut host, "Read selected sounds");
        click(&mut host, "Use sound relationship recipe");
        click(&mut host, "Increase recipe spacing");
        click(&mut host, "night · token-6-11");
        assert!(panel_text(&host).contains("bytes 6–11"));
        click(&mut host, "Retain relationship recipe");
        wait_for_confirmation(&mut host, 1);
        let retained = port.list(port.target()).unwrap();
        let item = retained[0].item.clone();
        assert_eq!(item.kind, knot_composition::ItemKind::ProjectionRecipe);
        let material = item.projection_recipe.as_ref().unwrap();
        assert_eq!(material.snapshot.recipe.definition.arrangement.spacing, 24);
        assert_eq!(
            material.snapshot.selected_occurrence.as_deref(),
            Some("token-6-11")
        );
        assert_eq!(material.dataset.dataset.occurrences.len(), 3);
        (item, retained[0].receipt.operation)
    };
    let bytes = std::fs::read(&database).unwrap();
    assert!(
        !bytes
            .windows("token-6-11".len())
            .any(|window| window == b"token-6-11")
    );
    let resident = open_resident();
    let port = Arc::new(
        KnotResidentCompositionPort::new(
            persona,
            resident
                .composition_retention(CompositionGrant::new(65536, 20))
                .unwrap(),
        )
        .unwrap(),
    );
    let mut host = make_host(port.clone());
    click(&mut host, "Readings");
    click(&mut host, "Collection");
    click(&mut host, "Recipe mere");
    wait_for_confirmation(&mut host, 1);
    click(&mut host, "Open retained relationship recipe");
    assert!(panel_text(&host).contains("spacing 24"));
    assert!(panel_text(&host).contains("bytes 6–11"));
    let items = port.list(port.target()).unwrap();
    assert_eq!(items[0].item, saved);
    assert_eq!(items[0].receipt.operation, operation);
    assert_eq!(
        std::fs::read_to_string(&document).unwrap(),
        "night night light"
    );
    assert!(
        !root
            .path()
            .join("settings/composition/collection.json")
            .exists()
    );
}
