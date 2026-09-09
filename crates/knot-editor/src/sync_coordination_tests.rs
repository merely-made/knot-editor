// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Deterministic tests for the serialized mutation boundary in [`super`].

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;
use tokio::sync::Barrier;

use super::*;

const SPACE: [u8; 32] = [0x91; 32];
const VAULT_KEY: [u8; 32] = [0x92; 32];

fn revision(body: &str) -> KnotFileRevisionV1 {
    KnotFileRevisionV1 {
        document_id: "knot:document:coordination".into(),
        title: "Coordination file".into(),
        media_type: "text/plain".into(),
        body: body.as_bytes().to_vec(),
    }
}

fn document(id: &str, body: &str) -> VaultDocument {
    VaultDocument {
        id: id.into(),
        title: id.into(),
        body: body.as_bytes().to_vec(),
        media_type: "text/vnd.knot".into(),
    }
}

fn poll_once<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    let mut context = Context::from_waker(Waker::noop());
    future.poll(&mut context)
}

#[tokio::test]
async fn same_writer_authors_are_serial_and_have_distinct_operations() {
    let roots = tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([0x93; 32]);
    let writer = identity.master_public_key().to_bytes();
    let seed = identity.master_keypair().to_seed();
    let vault =
        std::sync::Arc::new(KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap());
    let store = KnotSyncStore::in_memory(SPACE, [writer]);
    let barrier = std::sync::Arc::new(Barrier::new(3));

    let first_store = store.clone();
    let first_vault = vault.clone();
    let first_barrier = barrier.clone();
    let first = tokio::spawn(async move {
        first_barrier.wait().await;
        first_store
            .author(
                seed,
                &first_vault,
                &KnotSyncEvent::Put(document("first", "one")),
            )
            .await
            .unwrap()
    });
    let second_store = store.clone();
    let second_vault = vault.clone();
    let second_barrier = barrier.clone();
    let second = tokio::spawn(async move {
        second_barrier.wait().await;
        second_store
            .author(
                seed,
                &second_vault,
                &KnotSyncEvent::Put(document("second", "two")),
            )
            .await
            .unwrap()
    });
    barrier.wait().await;
    let (first, second) = tokio::join!(first, second);
    let mut operations = [first.unwrap(), second.unwrap()];
    operations.sort_by_key(|operation| operation.header.seq_num);
    let first = &operations[0];
    let second = &operations[1];
    assert_ne!(first.hash, second.hash);
    assert_eq!(first.header.seq_num, 0);
    assert_eq!(second.header.seq_num, 1);
    assert!(
        second
            .header
            .extensions
            .parents
            .contains(first.hash.as_bytes())
    );
}

#[tokio::test]
async fn concurrent_retained_captures_reuse_one_operation() {
    let roots = tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([0x94; 32]);
    let writer = identity.master_public_key().to_bytes();
    let seed = identity.master_keypair().to_seed();
    let vault =
        std::sync::Arc::new(KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap());
    let store = KnotSyncStore::in_memory(SPACE, [writer]);
    let capture = revision("same bytes");
    let left = {
        let store = store.clone();
        let vault = vault.clone();
        let capture = capture.clone();
        tokio::spawn(async move {
            store
                .retain_file_revision_with_cipher(seed, KnotSyncCipher::Personal(&vault), &capture)
                .await
                .unwrap()
        })
    };
    let right = {
        let store = store.clone();
        let vault = vault.clone();
        let capture = capture.clone();
        tokio::spawn(async move {
            store
                .retain_file_revision_with_cipher(seed, KnotSyncCipher::Personal(&vault), &capture)
                .await
                .unwrap()
        })
    };
    let (left, right) = tokio::join!(left, right);
    let (left_hash, left_already_retained) = left.unwrap();
    let (right_hash, right_already_retained) = right.unwrap();
    assert_eq!(left_hash, right_hash);
    assert_ne!(left_already_retained, right_already_retained);
}

#[tokio::test]
async fn mutation_gate_makes_accept_author_and_retain_pending_until_release() {
    let roots = tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([0x95; 32]);
    let writer = identity.master_public_key().to_bytes();
    let seed = identity.master_keypair().to_seed();
    let vault =
        std::sync::Arc::new(KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap());
    let store = KnotSyncStore::in_memory(SPACE, [writer]);
    let ingress_source = KnotSyncStore::in_memory(SPACE, [writer]);
    let ingress = ingress_source
        .author(seed, &vault, &KnotSyncEvent::Put(document("ingress", "in")))
        .await
        .unwrap();
    let _guard = store.mutation_gate.lock().await;
    let mut accept: Pin<Box<dyn Future<Output = _>>> = Box::pin(store.accept(&ingress));
    let author_event = KnotSyncEvent::Put(document("authored", "after"));
    let mut author: Pin<Box<dyn Future<Output = _>>> =
        Box::pin(store.author(seed, &vault, &author_event));
    let capture = revision("retained");
    let mut retain: Pin<Box<dyn Future<Output = _>>> = Box::pin(
        store.retain_file_revision_with_cipher(seed, KnotSyncCipher::Personal(&vault), &capture),
    );
    let mut checkpoint: Pin<Box<dyn Future<Output = _>>> = Box::pin(store.save_checkpoint(&vault));
    assert!(matches!(poll_once(accept.as_mut()), Poll::Pending));
    assert!(matches!(poll_once(author.as_mut()), Poll::Pending));
    assert!(matches!(poll_once(retain.as_mut()), Poll::Pending));
    assert!(matches!(poll_once(checkpoint.as_mut()), Poll::Pending));
    drop(_guard);
    let (accepted, authored, retained, checkpoint) =
        tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(accept, author, retain, checkpoint)
        })
        .await
        .expect("coordination futures made no progress after gate release");
    assert!(accepted.unwrap());
    let authored = authored.unwrap();
    assert_eq!(authored.header.seq_num, 1);
    assert!(
        authored
            .header
            .extensions
            .parents
            .contains(ingress.hash.as_bytes())
    );
    let (capture_operation, already_retained) = retained.unwrap();
    assert!(!already_retained);
    let checkpoint = checkpoint.unwrap();
    assert_eq!(checkpoint.heads.len(), 1);
    assert_eq!(checkpoint.heads[0].seq_num, 2);
    assert_eq!(checkpoint.heads[0].operation, capture_operation);
}

#[tokio::test]
async fn queued_retention_observes_admission_after_gate_release() {
    let roots = tempdir().unwrap();
    let identity = InMemoryProvider::from_seed([0x96; 32]);
    let writer = identity.master_public_key().to_bytes();
    let seed = identity.master_keypair().to_seed();
    let vault =
        std::sync::Arc::new(KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap());
    let store = KnotSyncStore::in_memory(SPACE, [writer]);
    let capture = revision("denied");
    let guard = store.mutation_gate.lock().await;
    let mut retain: Pin<Box<dyn Future<Output = _>>> = Box::pin(
        store.retain_file_revision_with_cipher(seed, KnotSyncCipher::Personal(&vault), &capture),
    );
    assert!(matches!(poll_once(retain.as_mut()), Poll::Pending));
    assert!(store.deny_writer(&writer));
    drop(guard);
    let result = tokio::time::timeout(Duration::from_secs(30), retain)
        .await
        .expect("queued retention did not finish after gate release");
    assert!(result.is_err());
    assert!(store.admitted_writers().is_empty());
    assert!(store.load_operations().await.unwrap().is_empty());

    // Cancelling a queued request must release its place without authoring.
    assert!(store.admit_writer(writer));
    let guard = store.mutation_gate.lock().await;
    let mut cancelled = Box::pin(store.retain_file_revision_with_cipher(
        seed,
        KnotSyncCipher::Personal(&vault),
        &capture,
    ));
    assert!(matches!(poll_once(cancelled.as_mut()), Poll::Pending));
    drop(cancelled);
    drop(guard);
    assert!(store.load_operations().await.unwrap().is_empty());
    let (_, already_present) = tokio::time::timeout(
        Duration::from_secs(30),
        store.retain_file_revision_with_cipher(seed, KnotSyncCipher::Personal(&vault), &capture),
    )
    .await
    .expect("cancelled waiter blocked a later capture")
    .unwrap();
    assert!(!already_present);
}
