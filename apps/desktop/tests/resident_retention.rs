#![cfg(feature = "resident-retention-tests")]
// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Windowless receipt for the real desktop review and resident-retain route.

use std::{sync::Arc, time::Duration};

use cambium_genet_winit_host::{Harness, Init, WindowCommands};
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
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use personae::{IdentityProvider, InMemoryProvider};
use taproot::Selector;
use tempfile::tempdir;

fn text_content(dom: &genet_scripted_dom::ScriptedDom, node: genet_scripted_dom::NodeId) -> String {
    let own = dom.text(node).unwrap_or_default();
    let children = dom
        .dom_children(node)
        .map(|child| text_content(dom, child))
        .collect::<String>();
    format!("{own}{children}")
}

fn class_node(dom: &ScriptedDom, node: NodeId, class: &str) -> Option<NodeId> {
    if dom.has_class(node, class) {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| class_node(dom, child, class))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn wait_for_worker(
    harness: &mut Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
    for _ in 0..1_000 {
        if harness.process_wake() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("resident retention worker did not wake the desktop host");
}

fn class_text(
    harness: &Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
    class: &str,
) -> String {
    let dom = harness.runner().dom();
    let dom = dom.borrow();
    let node = class_node(&dom, dom.document(), class)
        .unwrap_or_else(|| panic!("missing retained UI node with class {class}"));
    text_content(&dom, node)
}

/// Open the retention chip's popover, where retention's destinations and
/// actions live (slice 1 step 5d), by clicking the chip, and check it opened.
fn open_retention(
    harness: &mut Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
    fn chip(dom: &ScriptedDom, node: NodeId) -> Option<NodeId> {
        let key = dom.attribute(
            node,
            &Namespace::from(""),
            &LocalName::from("data-status-key"),
        );
        if key.as_deref() == Some("retention") {
            return Some(node);
        }
        dom.dom_children(node).find_map(|child| chip(dom, child))
    }
    let (x, y) = {
        let dom = harness.runner().dom();
        let dom = dom.borrow();
        let node = chip(&dom, dom.document()).expect("the retention chip");
        let (x, y, width, height) = harness.painted_rect(node).expect("the chip paints");
        (x + width / 2.0, y + height / 2.0)
    };
    harness.click_at(x, y);
    harness.relayout();
    let dom = harness.runner().dom();
    let dom = dom.borrow();
    assert!(
        class_node(&dom, dom.document(), "knot-retention").is_some(),
        "the retention popover opened"
    );
}

#[test]
fn desktop_retention_tracks_owner_grant_revocation_and_explicit_regrant() {
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
    let catalog_path = root.path().join("catalog.redb");
    let store = KnotSyncFileStore::open(&store_path, [0x63; 32], [writer]).unwrap();
    pollster::block_on(store.save_checkpoint(&vault)).unwrap();
    let resident = KnotResidentSource::from_synced_vault(vault, store.clone(), seed).unwrap();

    let mut catalog = KnotFileCatalog::open(&notes, &catalog_path).unwrap();
    let document_id = catalog.bind(&path).unwrap();
    let reviewed = catalog.capture_file_revision(&document_id, 4096).unwrap();
    let capture = resident
        .capture_retention(KnotCaptureGrant::new([document_id.clone()], 4096))
        .unwrap();
    let target: Arc<dyn KnotRetainPort> = Arc::new(
        KnotResidentRetainPort::new(
            KnotPersonaDisplayV1 {
                stable_id: "persona:desktop-receipt".into(),
                label: "Desktop receipt".into(),
            },
            capture.clone(),
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
            fonts: Vec::new(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    let wake = harness.wake();
    harness.update(|state| state.set_retention_targets(vec![target], wake));
    harness.layout_at(1000.0, 760.0);

    assert!(harness.click_on(&Selector::role("button").containing("Show changes")));
    assert!(harness.click_on(&Selector::role("button").containing("Review saved revision")));
    harness.after_dispatch();
    let later_disk = b"# Later disk edit\n";
    std::fs::write(&path, later_disk).unwrap();
    harness.update(|state| {
        state
            .document_mut()
            .session_mut()
            .input_mut()
            .unwrap()
            .insert_str("unsaved buffer edit");
    });
    open_retention(&mut harness);
    assert!(harness.click_on(&Selector::role("button").containing("Desktop receipt")));
    harness.after_dispatch();
    assert!(harness.click_on(&Selector::role("button").containing("Retain reviewed revision")));
    wait_for_worker(&mut harness);

    let tail = pollster::block_on(store.tail_receipt()).unwrap();
    assert_eq!(tail.operations.len(), 1);
    let receipt_text = class_text(&harness, "knot-retention-receipt");
    assert!(receipt_text.contains(&document_id));
    assert!(receipt_text.contains(&hex(&tail.operations[0])));
    let read_vault = KnotVault::open(&vault_root, [0x62; 32]).unwrap();
    let retained =
        pollster::block_on(store.file_revision(&read_vault, &document_id, tail.operations[0]))
            .unwrap();
    assert_eq!(retained.unwrap().body, source);

    capture.revoke();
    assert!(harness.click_on(&Selector::role("button").containing("Retain reviewed revision")));
    wait_for_worker(&mut harness);
    assert!(
        class_text(&harness, "knot-retention-error").contains("capture grant has been revoked")
    );
    assert_eq!(
        pollster::block_on(store.tail_receipt()).unwrap().operations,
        tail.operations
    );

    let renewed_capture = resident
        .capture_retention(KnotCaptureGrant::new([document_id.clone()], 4096))
        .unwrap();
    let renewed: Arc<dyn KnotRetainPort> = Arc::new(
        KnotResidentRetainPort::new(
            KnotPersonaDisplayV1 {
                stable_id: "persona:desktop-receipt".into(),
                label: "Desktop receipt renewed".into(),
            },
            renewed_capture,
        )
        .unwrap(),
    );
    let wake = harness.wake();
    harness.update(|state| state.set_retention_targets(vec![renewed], wake));
    // The popover is still open, so a missing Retain is really missing.
    assert!(!class_text(&harness, "knot-retention").is_empty());
    assert!(
        !harness.click_on(&Selector::role("button").containing("Retain reviewed revision")),
        "a newly injected owner target must require a new explicit selection"
    );
    assert!(harness.click_on(&Selector::role("button").containing("Desktop receipt renewed")));
    assert!(harness.click_on(&Selector::role("button").containing("Retain reviewed revision")));
    wait_for_worker(&mut harness);

    let retry_tail = pollster::block_on(store.tail_receipt()).unwrap();
    assert_eq!(retry_tail.operations, tail.operations);
    let retry_text = class_text(&harness, "knot-retention-receipt");
    assert!(retry_text.contains("Desktop receipt renewed"));
    assert!(retry_text.contains("already retained"));
    assert_eq!(std::fs::read(&path).unwrap(), later_disk);
    assert!(harness.state().document().snapshot().dirty);

    drop(harness);
    drop(capture);
    drop(resident);
    drop(read_vault);
    drop(store);

    let reopened_vault = KnotVault::open(&vault_root, [0x62; 32]).unwrap();
    let reopened_store = KnotSyncFileStore::open(&store_path, [0x63; 32], [writer]).unwrap();
    let reopened =
        KnotResidentSource::from_synced_vault(reopened_vault, reopened_store.clone(), seed)
            .unwrap();
    let retry = reopened
        .capture_retention(KnotCaptureGrant::new([document_id.clone()], 4096))
        .unwrap();
    let receipt = retry.retain(&retry.prepare(reviewed).unwrap()).unwrap();
    assert!(receipt.already_retained);
    assert_eq!(receipt.operation, tail.operations[0]);
    assert_eq!(
        pollster::block_on(reopened_store.tail_receipt())
            .unwrap()
            .operations,
        tail.operations
    );
}
