// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Engine split P4: a non-graph domain, Knot's operation store, opens through
//! the reservoir, writes a Knot operation, folds, reopens and reads back.
#![cfg(feature = "reservoir")]

mod common;

use knot_sync::{
    KnotEncryptionProfile, KnotSyncEvent, NOTES_DOMAIN, VaultDocument, open_through_reservoir,
};
use personae::{IdentityProvider, InMemoryProvider, PersonaId};
use uuid::Uuid;

use common::{KeySealer, accept_recipes};

const SPACE: [u8; 32] = [0x71; 32];

#[tokio::test(flavor = "multi_thread")]
async fn knots_store_opens_writes_folds_and_reopens_through_the_reservoir() {
    let root = tempfile::tempdir().unwrap();
    let persona = PersonaId::from_uuid(Uuid::from_u128(0x6b6e_6f74));
    let alice = InMemoryProvider::from_seed([0x72; 32]);
    let seed = alice.master_keypair().to_seed();
    let writer = alice.master_public_key().to_bytes();
    let vault = KeySealer::new([0x73; 32]);
    let open = || {
        let root = root.path().to_path_buf();
        tokio::task::spawn_blocking(move || {
            open_through_reservoir(
                &root,
                persona,
                1,
                SPACE,
                [writer],
                KnotEncryptionProfile::PersonalVaultV1,
                accept_recipes(),
            )
        })
    };

    let store = open().await.unwrap().unwrap();
    store
        .author(
            &seed,
            &vault,
            &KnotSyncEvent::Put(VaultDocument {
                id: "essay".into(),
                title: "Essay".into(),
                body: b"# through the reservoir\n".to_vec(),
                media_type: "text/vnd.knot".into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(store.projection(&vault).await.unwrap().documents.len(), 1);
    drop(store);

    let mere = catchment::MereId::derive(persona, &catchment::DomainId::new(NOTES_DOMAIN).unwrap());
    assert!(
        catchment::mere_dir(root.path(), persona, mere)
            .join("archive.redb")
            .is_file()
    );

    let reopened = open().await.unwrap().unwrap();
    let documents = reopened.projection(&vault).await.unwrap().documents;
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].body, b"# through the reservoir\n");
}
