// SPDX-License-Identifier: MPL-2.0
//! Native acceptance instrument, NEVER a personal-wallet initializer.
//! Uses public deterministic test keys in a marked disposable fixture. Ordinary
//! Knot launch never calls this instrument or acquires its synthetic authority.
use knot_capture::KnotPersonaDisplayV1;
use knot_composition::{
    CollectionItem, DocumentAnchor, ItemKind, retention::CompositionRetainPort,
};
use knot_desktop::{DesktopLaunch, run_desktop_with_targets};
use knot_document::KnotDocumentSession;
use knot_editor::{
    CompositionGrant, KnotResidentCompositionPort, KnotResidentSource, KnotSyncFileStore, KnotVault,
};
use personae::{IdentityProvider, InMemoryProvider};
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mode = args
        .next()
        .ok_or("expected --initialize-test-fixture or --reopen-test-fixture")?;
    let root = PathBuf::from(
        args.next()
            .ok_or("expected absolute disposable fixture root")?,
    );
    if !root.is_absolute() || args.next().is_some() {
        return Err("expected one absolute fixture root".into());
    }
    let marker = root.join("PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET");
    if mode == "--initialize-test-fixture" {
        if root.exists() && std::fs::read_dir(&root)?.next().is_some() {
            return Err("initialization requires an empty disposable fixture directory".into());
        }
        std::fs::create_dir_all(&root)?;
        std::fs::write(
            &marker,
            "Synthetic acceptance authority only; all keys are public test constants.\n",
        )?;
        std::fs::write(
            root.join("essay.djot"),
            "# Night light\n\nNight light by the river.\n",
        )?;
    } else if mode == "--reopen-test-fixture" {
        if !marker.is_file() || !root.join("sync.redb").is_file() || !root.join("vault").is_dir() {
            return Err("reopen requires a previously initialized marked test fixture".into());
        }
    } else {
        return Err("unrecognized fixture mode".into());
    }
    let identity = InMemoryProvider::from_seed([0x71; 32]);
    let seed = identity.master_keypair().to_seed();
    let writer = identity.master_public_key().to_bytes();
    let vault = KnotVault::open(root.join("vault"), [0x72; 32])?;
    let store = KnotSyncFileStore::open(root.join("sync.redb"), [0x73; 32], [writer])?;
    let resident = KnotResidentSource::from_synced_vault(vault, store, seed)?;
    let port = KnotResidentCompositionPort::new(
        KnotPersonaDisplayV1 {
            stable_id: "persona:public-test-fixture".into(),
            label: "Synthetic acceptance mere".into(),
        },
        resident.composition_retention(CompositionGrant::new(8192, 20))?,
    )?;
    let source = root.join("essay.djot");
    let first = KnotDocumentSession::open(&source)?;
    if mode == "--initialize-test-fixture" {
        let snapshot = first.snapshot();
        let mut item = CollectionItem::new(ItemKind::Note, "Night light passage", "Night light");
        item.id = "collection-native-fixture-v1".into();
        item.document_source = Some(DocumentAnchor::capture(
            snapshot.source.address,
            &snapshot.text,
            2..13,
        )?);
        port.retain(port.target(), item)?;
    }
    run_desktop_with_targets(
        DesktopLaunch {
            first,
            first_path: Some(source),
            behind: Vec::new(),
            sites: Vec::new(),
            failures: Vec::new(),
            recovery_path: None,
        },
        None,
        8192,
        Some(root.join("readings")),
        Some(root.join("preferences.json")),
        Vec::new(),
        vec![Arc::new(port)],
        None,
    )
    .map_err(Into::into)
}
