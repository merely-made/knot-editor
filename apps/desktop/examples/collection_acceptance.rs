// SPDX-License-Identifier: MPL-2.0
//! Native acceptance instrument, NEVER a personal-wallet initializer.
//! Uses public deterministic test keys in a marked disposable fixture. Ordinary
//! Knot launch never calls this instrument or acquires its synthetic authority.
use knot_capture::KnotPersonaDisplayV1;
use knot_composition::{
    CollectionItem, DocumentAnchor, ItemKind, retention::CompositionRetainPort,
};
use knot_desktop::{DesktopLaunch, run_desktop_with_targets_and_relations};
use knot_document::KnotDocumentSession;
use knot_editor::{
    CompositionGrant, KnotAssertedTime, KnotCorePredicateV1, KnotPredicateRefV1,
    KnotRelationEndpointV1, KnotRelationPositionV1, KnotResidentCompositionPort,
    KnotResidentSource, KnotSyncEvent, KnotSyncFileStore, KnotVault, VaultDocument,
    capture_endpoint_context,
};
use personae::{IdentityProvider, InMemoryProvider};
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mode = args
        .next()
        .ok_or("expected a collection/opened-link fixture mode")?;
    let root = PathBuf::from(
        args.next()
            .ok_or("expected absolute disposable fixture root")?,
    );
    if !root.is_absolute() || args.next().is_some() {
        return Err("expected one absolute fixture root".into());
    }
    let marker = root.join("PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET");
    let initialize =
        mode == "--initialize-test-fixture" || mode == "--initialize-opened-link-fixture" || mode == "--initialize-metre-fixture";
    let opened_link_fixture =
        mode == "--initialize-opened-link-fixture" || mode == "--reopen-opened-link-fixture";
    if initialize {
        if root.exists() && std::fs::read_dir(&root)?.next().is_some() {
            return Err("initialization requires an empty disposable fixture directory".into());
        }
        std::fs::create_dir_all(&root)?;
        std::fs::write(
            &marker,
            "Synthetic acceptance authority only; all keys are public test constants.\n",
        )?;
        let essay = if mode == "--initialize-metre-fixture" {
            "the night\nthe light\n"
        } else if opened_link_fixture {
            "# Night light\nNight light by the river.\n"
        } else {
            "# Night light\n\nNight light by the river.\n"
        };
        std::fs::write(root.join("essay.djot"), essay)?;
    } else if mode == "--reopen-test-fixture"
        || mode == "--inspect-test-fixture"
        || mode == "--reopen-opened-link-fixture" || mode == "--reopen-metre-fixture"
    {
        if !marker.is_file()
            || !root.join("sync.redb").is_file()
            || !root.join("essay.djot").is_file()
        {
            return Err("reopen requires a previously initialized marked test fixture".into());
        }
    } else {
        return Err("unrecognized fixture mode".into());
    }
    let identity = InMemoryProvider::from_seed([0x71; 32]);
    let seed = identity.master_keypair().to_seed();
    let writer = identity.master_public_key().to_bytes();
    let mut vault = KnotVault::open(root.join("vault"), [0x72; 32])?;
    let mut store = KnotSyncFileStore::open(root.join("sync.redb"), [0x73; 32], [writer])?;
    if opened_link_fixture && mode == "--initialize-opened-link-fixture" {
        let essay_text = "# Night light\nNight light by the river.\n";
        let source_text = "# River notes\nThe river keeps the dusk.\n";
        let essay = put_document("essay", "Night light", essay_text);
        let source_document = put_document("source", "River notes", source_text);
        let essay_put = pollster::block_on(store.author_at(
            &seed,
            &vault,
            &KnotSyncEvent::Put(essay),
            KnotAssertedTime::At(1_600_000_000_000),
        ))?;
        let source_put = pollster::block_on(store.author_at(
            &seed,
            &vault,
            &KnotSyncEvent::Put(source_document),
            KnotAssertedTime::At(1_600_000_000_100),
        ))?;
        let essay_endpoint = relation_endpoint(
            "essay",
            *essay_put.hash.as_bytes(),
            essay_text,
            "Night light",
        );
        let source_endpoint = relation_endpoint(
            "source",
            *source_put.hash.as_bytes(),
            source_text,
            "river keeps the dusk",
        );
        pollster::block_on(store.author_at(
            &seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                subject: essay_endpoint.clone(),
                object: source_endpoint.clone(),
                evidence: Some("urn:knot:fixture:evidence:river-note".into()),
                qualification: Some("The highlighted passage supports this reading.".into()),
            },
            KnotAssertedTime::At(1_650_000_000_000),
        ))?;
        let withdrawn = pollster::block_on(store.author_at(
            &seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Questions),
                subject: essay_endpoint.clone(),
                object: source_endpoint.clone(),
                evidence: Some("urn:knot:fixture:evidence:night-question".into()),
                qualification: Some("A second, later withdrawn reading.".into()),
            },
            KnotAssertedTime::At(1_660_000_000_000),
        ))?;
        pollster::block_on(store.author_at(
            &seed,
            &vault,
            &KnotSyncEvent::RetractRelation {
                assertion: *withdrawn.hash.as_bytes(),
            },
            KnotAssertedTime::At(1_670_000_000_000),
        ))?;
        pollster::block_on(store.author_at(
            &seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Cites),
                subject: source_endpoint,
                object: essay_endpoint,
                evidence: None,
                qualification: Some("Reverse direction and unknown first time.".into()),
            },
            KnotAssertedTime::None,
        ))?;
        pollster::block_on(store.save_checkpoint(&vault))?;
        drop(store);
        drop(vault);
        vault = KnotVault::open(root.join("vault"), [0x72; 32])?;
        store = KnotSyncFileStore::open(root.join("sync.redb"), [0x73; 32], [writer])?;
        let reopened = pollster::block_on(store.projection(&vault))?;
        if reopened.relations.len() != 3
            || reopened
                .relations
                .iter()
                .filter(|relation| !relation.retractions.is_empty())
                .count()
                != 1
        {
            return Err("signed opened-link fixture did not survive checkpoint/reopen".into());
        }
    }
    let projection = pollster::block_on(store.projection(&vault))?;
    let relation_view = opened_link_fixture
        .then(|| {
            knot_desktop::knot_relations::KnotRelationView::from_projection(
                &projection,
                store.space_id(),
            )
        })
        .transpose()?;
    let resident = KnotResidentSource::from_synced_vault(vault, store, &seed)?;
    let port = KnotResidentCompositionPort::new(
        KnotPersonaDisplayV1 {
            stable_id: "persona:public-test-fixture".into(),
            label: "Synthetic acceptance mere".into(),
        },
        resident.composition_retention(CompositionGrant::new(8192, 20))?,
    )?;
    if mode == "--inspect-test-fixture" {
        // Read-only object identity for selecting one immutable test version.
        // This mode is guarded by the same public-fixture marker as reopen.
        let recipes: Vec<_> = port.list(port.target())?.into_iter().filter_map(|entry| {
            let material = entry.item.projection_recipe.as_ref()?;
            Some(serde_json::json!({
                "item_id": entry.item.id,
                "authored_positions": material.scene_positions().ok()?.len(),
                "placement_steps": material.presentation.as_ref()?.placement_trace.as_ref().map_or(0, |trace| trace.len()),
            }))
        }).collect();
        println!("{}", serde_json::to_string_pretty(&recipes)?);
        return Ok(());
    }
    let source = root.join("essay.djot");
    let mut first = KnotDocumentSession::open(&source)?;
    if mode == "--initialize-metre-fixture" {
        let snapshot = first.snapshot();
        first.select_source_span(&snapshot.source.address, &snapshot.text, 0, snapshot.text.len())?;
    }
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
    run_desktop_with_targets_and_relations(
        DesktopLaunch {
            first,
            first_path: Some(source),
            behind: Vec::new(),
            sites: Vec::new(),
            failures: Vec::new(),
            recovery_path: None,
            mere_overlay: None,
        },
        None,
        8192,
        Some(root.join("readings")),
        Some(root.join("preferences.json")),
        Vec::new(),
        vec![Arc::new(port)],
        None,
        relation_view,
    )
    .map_err(Into::into)
}

fn put_document(id: &str, title: &str, text: &str) -> VaultDocument {
    VaultDocument {
        id: id.into(),
        title: title.into(),
        body: text.as_bytes().to_vec(),
        media_type: "text/vnd.knot".into(),
    }
}

fn relation_endpoint(
    document_id: &str,
    document_head: [u8; 32],
    source: &str,
    quote: &str,
) -> KnotRelationEndpointV1 {
    let start = source.find(quote).expect("fixture quote is present");
    let end = start + quote.len();
    let (prefix, suffix) = capture_endpoint_context(source, start, end);
    KnotRelationEndpointV1 {
        document_id: document_id.into(),
        document_head,
        quote: quote.into(),
        position: Some(KnotRelationPositionV1 {
            start: start as u64,
            end: end as u64,
        }),
        prefix,
        suffix,
    }
}
