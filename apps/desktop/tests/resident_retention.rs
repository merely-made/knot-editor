#![cfg(feature = "resident-retention-tests")]
// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Windowless receipt for the real desktop review and resident-retain route.

use std::{sync::Arc, time::Duration};

use cambium_genet_winit_host::{Harness, Init, WindowCommands};
use genet_probe::Selector;
use genet_scripted_dom::{NodeId, ScriptedDom};
use knot_capture::{KnotPersonaDisplayV1, KnotRetainPort};
use knot_desktop::{
    host_hooks,
    workspace::{DESKTOP_CSS, DesktopState, DesktopView, desktop_view},
};
use knot_document::{KNOT_DOCUMENT_CSS, KnotDocumentSession};
use knot_editor::{
    KnotCaptureGrant, KnotResidentRetainPort, KnotResidentSource, KnotSyncFileStore, KnotVault,
};
use knot_file_catalog::KnotFileCatalog;
use layout_dom_api::LayoutDom;
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;

fn class_node(dom: &ScriptedDom, node: NodeId, class: &str) -> Option<NodeId> {
    if dom.has_class(node, class) {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| class_node(dom, child, class))
}

fn text_content(dom: &ScriptedDom, node: NodeId) -> String {
    let own = dom.text(node).unwrap_or_default();
    let children = dom
        .dom_children(node)
        .map(|child| text_content(dom, child))
        .collect::<String>();
    format!("{own}{children}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn desktop_reviews_selects_a_host_target_and_retains_exact_saved_source_on_a_worker() {
    let root = tempdir().unwrap();
    let notes = root.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let path = notes.join("reviewed.djot");
    let source = b"# Reviewed\n\nExact saved source.\n";
    std::fs::write(&path, source).unwrap();

    let identity = InMemoryProvider::from_seed([0x61; 32]);
    let seed = identity.master_keypair().to_seed();
    let writer = identity.master_public_key().to_bytes();
    let vault_root = root.path().join("vault");
    let vault = KnotVault::open(&vault_root, [0x62; 32]).unwrap();
    let store_path = root.path().join("sync.redb");
    let store = KnotSyncFileStore::open(&store_path, [0x63; 32], [writer]).unwrap();
    pollster::block_on(store.save_checkpoint(&vault)).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();

    let mut catalog = KnotFileCatalog::open(&notes, root.path().join("catalog.redb")).unwrap();
    let document_id = catalog.bind(&path).unwrap();
    let capture = resident
        .capture_retention(KnotCaptureGrant::new([document_id.clone()], 4096))
        .unwrap();
    let target: Arc<dyn KnotRetainPort> = Arc::new(
        KnotResidentRetainPort::new(
            KnotPersonaDisplayV1 {
                stable_id: "persona:desktop-receipt".into(),
                label: "Desktop receipt".into(),
            },
            capture,
        )
        .unwrap(),
    );

    let session = KnotDocumentSession::open(&path).unwrap();
    let state = DesktopState::with_catalog(
        session,
        WindowCommands::new(),
        Some(path.clone()),
        Some(catalog),
    );
    let mut harness = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: format!("{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}"),
        },
        host_hooks(),
    );
    let wake = harness.wake();
    harness.update(|state| state.set_retention_targets(vec![target], wake));
    harness.layout_at(1000.0, 760.0);

    assert!(harness.click_on(&Selector::role("button").containing("Review saved revision")));
    harness.after_dispatch();
    let later_disk = b"# Later disk edit\n";
    std::fs::write(&path, later_disk).unwrap();
    harness.update(|state| {
        state
            .document
            .session_mut()
            .input_mut()
            .unwrap()
            .insert_str("unsaved buffer edit");
    });
    assert!(harness.click_on(&Selector::role("button").containing("Desktop receipt")));
    harness.after_dispatch();
    assert!(harness.click_on(&Selector::role("button").containing("Retain reviewed revision")));

    let mut woke = false;
    for _ in 0..1_000 {
        if harness.process_wake() {
            woke = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        woke,
        "resident retention worker did not wake the desktop host"
    );

    let tail = pollster::block_on(store.tail_receipt()).unwrap();
    assert_eq!(tail.operations.len(), 1);
    let receipt_text = {
        let dom = harness.runner().dom();
        let dom = dom.borrow();
        let receipt = class_node(&dom, dom.document(), "knot-retention-receipt")
            .expect("completed worker receipt");
        text_content(&dom, receipt)
    };
    assert!(receipt_text.contains(&document_id));
    assert!(receipt_text.contains(&hex(&tail.operations[0])));
    let read_vault = KnotVault::open(&vault_root, [0x62; 32]).unwrap();
    let retained =
        pollster::block_on(store.file_revision(&read_vault, &document_id, tail.operations[0]))
            .unwrap();
    assert_eq!(retained.unwrap().body, source);
    assert_eq!(std::fs::read(&path).unwrap(), later_disk);
    assert!(harness.state().document.snapshot().dirty);
}
