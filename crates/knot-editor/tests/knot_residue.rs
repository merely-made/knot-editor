// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What Knot leaves of its signing seed (vault lock rulings 48 and 49):
//! castellan's residue tracker, included by path, in one process.
//!
//! Moved from Mere's `mere-resident` (cycle breaks P2 remainder, ruling 2),
//! where it ran under the resident's Knot lane. The runs are Knot's own: the
//! source and session are what the supervised `knot_sync_host` child holds
//! and offers on the `knot` route, so this is the residue run of the
//! child-served lane. The tracker and its verdict module are copies of Mere's
//! test helpers (`tests/residue`, `tests/residue_verdict`).
//!
//! Each run is armed around one spawned task (so its frames are heap
//! allocations the tracker sees, as in the resident) that builds what it
//! uses and drops it again. The paths that open no network (`author`, a
//! resident source with a session, capture retention) fail on any copy of
//! the seed, freed uncleared or still live. The sync host's runs (open,
//! close, drop) are judged as mere-transport's `seed_residue.rs` judges the
//! transport: a block, freed or live, whose size is in every host run and in
//! no baseline run fails. A host block in only some runs, or of a size a
//! baseline also shows, is listed as unjudged rather than failed: size and
//! presence cannot attribute it (Q052). The baselines are iroh alone,
//! p2panda-net alone, and the sync host's own bind and join with the seed
//! held by this test (ruling 59); their blocks are reported, never failed on (the upstream
//! candidates ledger, items 10 and 11; ruling 60 for the overlay host's).
//! A block exactly the size of a run's own future is that run's outer frame,
//! whose size changes with nesting, so it is compared by presence.
//!
//! First, a held source must show the seed live: the instrument sees Knot's
//! copy.
//!
//! No libtest harness: the allocator is process-wide.

#[path = "residue/mod.rs"]
mod residue;
mod residue_verdict;

#[allow(unused_imports)]
use knot_editor::{KnotFileStoreExt as _, KnotMemoryStoreExt as _};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use knot_editor::{
    KnotCaptureGrant, KnotFileRevisionV1, KnotResidentSource, KnotSyncEvent, KnotSyncFileStore,
    KnotSyncHost, KnotSyncHostConfig, KnotVault, KnotWriteGrant, VaultDocument,
};
use muniment::{JsonCodec, MemoryBackend};
use p2panda_core::SigningKey;
use p2panda_net::address_book::AddressBookStoreHandle;
use p2panda_net::gossip::Gossip;
use p2panda_net::{AddressBook, Endpoint};
use residue::*;
use residue_verdict::{Run, own_blocks, unjudged};
use murm_sync::MunimentAddressBook;

const RUNS: usize = 5;
const SPACE: [u8; 32] = [0x91; 32];
const VAULT_KEY: [u8; 32] = [0xa1; 32];

type Seed = &'static [u8; 32];

/// How the seed reaches Knot: lent (ruling 49). The control against a Knot
/// that takes it by value returns `*seed` here, and nothing else changes.
fn lend(seed: Seed) -> &'static [u8; 32] {
    seed
}

fn measure<F>(rt: &tokio::runtime::Runtime, name: &str, mut work: impl FnMut(usize) -> F) -> Vec<Run>
where
    F: Future<Output = ()> + Send + 'static,
{
    (0..RUNS).map(|n| measure_once(rt, name, work(n))).collect()
}

fn measure_once<F>(rt: &tokio::runtime::Runtime, name: &str, future: F) -> Run
where
    F: Future<Output = ()> + Send + 'static,
{
    let frame = std::mem::size_of::<F>();
    arm();
    rt.block_on(async { tokio::spawn(future).await.unwrap() });
    // Let the runtime's workers finish what the run handed them.
    rt.block_on(async { tokio::time::sleep(std::time::Duration::from_millis(300)).await });
    let (hits, overflow) = disarm();
    assert!(!overflow, "{name}: the allocation table overflowed");
    let sizes = |live: bool| {
        let mut sizes: Vec<usize> = hits
            .iter()
            .filter(|h| h.live == live)
            .map(|h| h.size)
            .collect();
        sizes.sort_unstable();
        sizes
    };
    let (freed, live) = (sizes(false), sizes(true));
    println!(
        "knot residue {name}: live {live:?}, freed uncleared {} {freed:?} (outer frame {frame})",
        freed.len()
    );
    Run { freed, live, frame }
}

// ─── Knot ─────────────────────────────────────────────────────────────────

/// A vault and an operation store holding one authored note, made before
/// arming; each run takes its own.
fn fixture(root: &Path, seed: Seed) -> (KnotVault, KnotSyncFileStore) {
    let writer = *SigningKey::from_bytes(seed).verifying_key().as_bytes();
    std::fs::create_dir_all(root).unwrap();
    let vault = KnotVault::open(root.join("vault"), VAULT_KEY).unwrap();
    let store = KnotSyncFileStore::open(root.join("sync.redb"), SPACE, [writer]).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime
        .block_on(store.author(lend(seed), &vault, &note("setup")))
        .unwrap();
    (vault, store)
}

fn note(id: &str) -> KnotSyncEvent {
    KnotSyncEvent::Put(VaultDocument {
        id: id.into(),
        title: "Note".into(),
        body: b"# Residue\n".to_vec(),
        media_type: "text/djot".into(),
    })
}

fn revision() -> KnotFileRevisionV1 {
    KnotFileRevisionV1 {
        document_id: "file:essay".into(),
        title: "Essay".into(),
        media_type: "text/x-djot".into(),
        body: b"reviewed source\n".to_vec(),
    }
}

/// Sign one more note.
async fn author(vault: KnotVault, store: KnotSyncFileStore, seed: Seed) {
    store
        .author(lend(seed), &vault, &note("authored"))
        .await
        .unwrap();
}

/// A resident source and a session over it, as the supervised child holds
/// and offers them on the `knot` route.
async fn source_and_session(vault: KnotVault, store: KnotSyncFileStore, seed: Seed) {
    tokio::task::spawn_blocking(move || {
        let source = KnotResidentSource::from_synced_vault(vault, store, lend(seed)).unwrap();
        let session = source.session(Some(KnotWriteGrant::new(4096)));
        drop(session);
        drop(source);
    })
    .await
    .unwrap();
}

/// Capture retention, a blocking API, on a worker as its contract asks.
async fn capture(vault: KnotVault, store: KnotSyncFileStore, seed: Seed) {
    tokio::task::spawn_blocking(move || {
        let source = KnotResidentSource::from_synced_vault(vault, store, lend(seed)).unwrap();
        let port = source
            .capture_retention(KnotCaptureGrant::new([revision().document_id], 4096))
            .unwrap();
        let prepared = port.prepare(revision()).unwrap();
        assert!(!port.retain(&prepared).unwrap().already_retained);
        drop(port);
        drop(source);
    })
    .await
    .unwrap();
}

/// The sync host opened and closed the way the resident's lane does.
async fn sync_host(store: KnotSyncFileStore, seed: Seed) {
    let host = KnotSyncHost::open(&store, lend(seed), KnotSyncHostConfig::default())
        .await
        .unwrap();
    host.close().await.unwrap();
    drop(store);
}

// ─── Baselines, as mere-transport's `seed_residue.rs` builds them ────────

/// A connection hook that does nothing, standing in for the transport's.
#[derive(Clone, Debug)]
struct NoHook;

impl iroh::endpoint::EndpointHooks for NoHook {}

/// p2panda-net's endpoint spawn, built and boxed in a plain function.
#[allow(clippy::type_complexity)]
fn spawn_endpoint(
    address_book: AddressBook,
    seed: Seed,
) -> Pin<Box<impl Future<Output = Result<Endpoint, impl std::fmt::Debug>>>> {
    Box::pin(
        Endpoint::builder(address_book)
            .signing_key(SigningKey::from_bytes(seed))
            .hooks(NoHook)
            .spawn(),
    )
}

/// Two async layers, as the transport's `bind` and `bind_inner`, with gossip
/// as Knot's host binds it.
async fn bind_outer(address_book: AddressBook, seed: Seed) -> (Box<Endpoint>, Box<Gossip>) {
    bind_like(address_book, seed).await
}

async fn bind_like(address_book: AddressBook, seed: Seed) -> (Box<Endpoint>, Box<Gossip>) {
    let endpoint = Box::new(spawn_endpoint(address_book.clone(), seed).await.unwrap());
    let spawn = Box::pin(Gossip::builder(address_book, (*endpoint).clone()).spawn());
    (endpoint, Box::new(spawn.await.unwrap()))
}

async fn p2panda_net(seed: Seed) {
    let store = AddressBookStoreHandle::new(MunimentAddressBook::<MemoryBackend, JsonCodec>::new(
        MemoryBackend::new(),
    ));
    let address_book = AddressBook::builder().store(store).spawn().await.unwrap();
    let (endpoint, overlay) = bind_outer(address_book.clone(), seed).await;
    let iroh = endpoint.endpoint().await.unwrap();
    iroh.close().await;
    drop((endpoint, address_book, overlay));
}

/// Mere's overlay host alone, no Knot, bound as Knot's sync host binds it;
/// `mdns` false turns the policy's default mDNS off (ruling 60).
async fn overlay_alone(seed: Seed, mdns: bool) {
    let host = bind_overlay(seed, SPACE, mdns).await;
    host.close().await.unwrap();
    drop(host);
}

/// The sync host's bind and join with the seed held here, not by Knot
/// (ruling 59): the store's join never sees the seed.
async fn joined_alone(store: KnotSyncFileStore, seed: Seed) {
    let host = bind_overlay(seed, store.space_id(), true).await;
    let (endpoint, gossip) = host.transport().sync_parts().unwrap();
    let joined = store.join(endpoint, gossip).await.unwrap();
    joined.leave_and_wait().await.unwrap();
    host.close().await.unwrap();
    drop((host, store));
}

async fn bind_overlay(seed: Seed, space: [u8; 32], mdns: bool) -> transport::P2pandaOverlayHost {
    let default = transport::P2pandaHostPolicy::default();
    let policy = transport::P2pandaHostPolicy {
        mdns: if mdns { default.mdns.clone() } else { None },
        ..default
    };
    transport::P2pandaOverlayHost::bind(
        transport::P2pandaTransport::builder_from_seed_ref(seed).gossip(),
        transport::sync_overlay_topic(space),
        &policy,
    )
    .await
    .unwrap()
}

async fn iroh_alone(seed: Seed) {
    let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .secret_key(iroh::SecretKey::from_bytes(seed))
        .bind()
        .await
        .unwrap();
    endpoint.close().await;
}

fn main() {
    let seed: [u8; 32] = canary_bytes(0);
    positive_control(&seed);
    clear_canaries();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    // Borrowed by every run; allocated before arming, so never scanned.
    let seed: Seed = Box::leak(Box::new(seed));
    let root = |name: &str, n: usize| -> PathBuf { dir.path().join(format!("{name}-{n}")) };
    plant(0, "Knot signing seed", seed, true);

    // The instrument's control for Knot's allocations: a held source.
    let (vault, store) = fixture(&root("control", 0), seed);
    arm();
    let held = KnotResidentSource::from_synced_vault(vault, store, lend(seed)).unwrap();
    let (hits, overflow) = disarm();
    let live = hits.iter().filter(|h| h.live).count();
    println!("knot control: a held resident source keeps the seed in {live} live block(s)");
    assert!(!overflow && live > 0, "the instrument must see Knot's live seed");
    drop(held);

    let mut failures = Vec::new();
    let mut strict = |name: &str, run: Run| {
        if !run.freed.is_empty() || !run.live.is_empty() {
            failures.push(format!(
                "{name}: live {:?}, freed uncleared {:?}",
                run.live, run.freed
            ));
        }
    };
    let (vault, store) = fixture(&root("author", 0), seed);
    strict("author", measure_once(&rt, "author", author(vault, store, seed)));
    let (vault, store) = fixture(&root("session", 0), seed);
    strict(
        "source and session",
        measure_once(&rt, "source and session", source_and_session(vault, store, seed)),
    );
    let (vault, store) = fixture(&root("capture", 0), seed);
    strict(
        "capture retention",
        measure_once(&rt, "capture retention", capture(vault, store, seed)),
    );

    let iroh = measure(&rt, "iroh endpoint alone", |_| iroh_alone(seed));
    let net = measure(&rt, "p2panda-net alone, gossip", |_| p2panda_net(seed));
    let overlay = measure(&rt, "overlay host alone, mDNS on", |_| overlay_alone(seed, true));
    let quiet = measure(&rt, "overlay host alone, mDNS off", |_| overlay_alone(seed, false));
    let stores = |kind: &str| {
        (0..RUNS)
            .map(|n| fixture(&root(kind, n), seed).1)
            .collect::<Vec<_>>()
            .into_iter()
    };
    let mut joins = stores("join");
    let joined = measure(&rt, "overlay host and Knot's join, seed held here", |_| {
        joined_alone(joins.next().unwrap(), seed)
    });
    let mut hosts = stores("host");
    let host = measure(&rt, "sync host", |_| sync_host(hosts.next().unwrap(), seed));
    clear_canaries();
    drop(rt);

    // Reported, never failed on here: the overlay host's own blocks belong to
    // mere-transport's `seed_residue.rs` (ruling 60).
    println!(
        "knot residue overlay host, mDNS on, beyond iroh and p2panda-net {:?}",
        own_blocks(&overlay, &[&iroh, &net])
    );
    println!(
        "knot residue overlay host, mDNS off, beyond iroh and p2panda-net {:?}",
        own_blocks(&quiet, &[&iroh, &net])
    );
    println!(
        "knot residue join, beyond iroh, p2panda-net and the overlay host {:?}",
        own_blocks(&joined, &[&iroh, &net, &overlay])
    );

    // Ruling 59: the sync host against the same bind and join with the seed
    // held by this test, so what is left is Knot's handling of the seed.
    let baselines: [&[Run]; 3] = [&iroh, &net, &joined];
    let own = own_blocks(&host, &baselines);
    println!("knot residue sync host: blocks (live, size) beyond the join baseline {own:?}");
    if !own.is_empty() {
        failures.push(format!("sync host: Knot's own blocks {own:?}"));
    }
    let unjudged = unjudged(&host, &baselines);
    for block in &unjudged {
        println!("knot residue sync host: unjudged {block}");
    }
    if !failures.is_empty() {
        println!("knot residue: FAILED {failures:?}");
        std::process::exit(1);
    }
    println!(
        "knot residue: no seed-bearing block (live, size) is in all {RUNS} sync-host runs and \
         absent from every baseline run; {} host block(s) unjudged, listed above",
        unjudged.len()
    );
}
