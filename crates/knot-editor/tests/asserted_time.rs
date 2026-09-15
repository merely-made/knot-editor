// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Author-asserted wall clock: signed into the header, informative only.

use knot_editor::{
    KnotAssertedTime, KnotCorePredicateV1, KnotPredicateRefV1, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotSyncEvent, KnotSyncFileStore, KnotVault, VaultDocument,
    capture_endpoint_context,
};
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;

const SPACE: [u8; 32] = [0x41; 32];
const VAULT_KEY: [u8; 32] = [0x42; 32];
const TRANSCRIBED: u64 = 1_600_000_000_000;

fn document(id: &str, body: &str) -> VaultDocument {
    VaultDocument {
        id: id.to_owned(),
        title: id.to_owned(),
        body: body.as_bytes().to_vec(),
        media_type: "text/vnd.knot".to_owned(),
    }
}

fn endpoint(id: &str, head: [u8; 32], source: &str, quote: &str) -> KnotRelationEndpointV1 {
    let start = source.find(quote).expect("fixture quote is present");
    let end = start + quote.len();
    let (prefix, suffix) = capture_endpoint_context(source, start, end);
    KnotRelationEndpointV1 {
        document_id: id.to_owned(),
        document_head: head,
        quote: quote.to_owned(),
        position: Some(KnotRelationPositionV1 {
            start: start as u64,
            end: end as u64,
        }),
        prefix,
        suffix,
    }
}

#[tokio::test]
async fn an_asserted_time_rides_the_signed_header_into_the_projection() {
    let temp = tempdir().unwrap();
    let alice = InMemoryProvider::from_seed([0x43; 32]);
    let seed = alice.master_keypair().to_seed();
    let store = KnotSyncFileStore::open(
        temp.path().join("time.redb"),
        SPACE,
        [alice.master_public_key().to_bytes()],
    )
    .unwrap();
    let vault = KnotVault::open(temp.path().join("vault"), VAULT_KEY).unwrap();

    let body = "# Essay\nα holds throughout.\n";
    let put = store
        .author_at(
            seed,
            &vault,
            &KnotSyncEvent::Put(document("essay", body)),
            KnotAssertedTime::At(TRANSCRIBED),
        )
        .await
        .unwrap();
    assert_eq!(put.header.extensions.asserted_at_ms, Some(TRANSCRIBED));

    let captured = endpoint("essay", *put.hash.as_bytes(), body, "α holds");
    let assertion = store
        .author_at(
            seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                subject: captured.clone(),
                object: captured,
                evidence: None,
                qualification: None,
            },
            KnotAssertedTime::At(TRANSCRIBED),
        )
        .await
        .unwrap();
    assert_eq!(
        assertion.header.extensions.asserted_at_ms,
        Some(TRANSCRIBED)
    );

    let projection = store.projection(&vault).await.unwrap();
    assert_eq!(projection.relations.len(), 1);
    assert_eq!(projection.relations[0].asserted_at_ms, Some(TRANSCRIBED));
    let checkpoint = store.save_checkpoint(&vault).await.unwrap();
    assert_eq!(
        checkpoint.snapshot.unwrap().relations[0].asserted_at_ms,
        Some(TRANSCRIBED)
    );
}

#[tokio::test]
async fn the_asserted_time_is_inside_the_operation_hash_and_absent_when_unasserted() {
    let temp = tempdir().unwrap();
    let alice = InMemoryProvider::from_seed([0x44; 32]);
    let seed = alice.master_keypair().to_seed();
    let writers = [alice.master_public_key().to_bytes()];
    let vault = KnotVault::open(temp.path().join("vault"), VAULT_KEY).unwrap();
    let document = document("essay", "# Essay\n");

    // Two stores so both operations are the author's first: same seq_num,
    // same backlink, same body, differing only in asserted time.
    let timed = KnotSyncFileStore::open(temp.path().join("timed.redb"), SPACE, writers).unwrap();
    let untimed =
        KnotSyncFileStore::open(temp.path().join("untimed.redb"), SPACE, writers).unwrap();
    let with_time = timed
        .author_at(
            seed,
            &vault,
            &KnotSyncEvent::Put(document.clone()),
            KnotAssertedTime::At(TRANSCRIBED),
        )
        .await
        .unwrap();
    let without_time = untimed
        .author_at(
            seed,
            &vault,
            &KnotSyncEvent::Put(document),
            KnotAssertedTime::None,
        )
        .await
        .unwrap();
    assert_ne!(
        with_time.hash, without_time.hash,
        "asserted time is inside the signed header bytes"
    );
    assert_eq!(without_time.header.extensions.asserted_at_ms, None);
    let encoded = serde_json::to_value(&without_time.header.extensions).unwrap();
    assert!(
        !encoded.as_object().unwrap().contains_key("asserted_at_ms"),
        "an unasserted time is absent from the encoded header, not null"
    );

    // `Now` reads this machine's clock; it never invents a time.
    let now = KnotSyncFileStore::open(temp.path().join("now.redb"), SPACE, writers).unwrap();
    let current = now
        .author(seed, &vault, &KnotSyncEvent::Delete { id: "essay".into() })
        .await
        .unwrap();
    let asserted = current.header.extensions.asserted_at_ms.unwrap();
    assert!(asserted > 1_700_000_000_000, "{asserted}");
    assert!(asserted < 4_000_000_000_000, "{asserted}");
}
