// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use graphshell::native::custody::ReleasedEpochKey;
use knot_editor::{PersonalVaultKeys, StartupUnlockedPersonalVault};
use p2panda_core::SigningKey;
use personae::PersonaId;
use tempfile::tempdir;

/// A release shaped like djinn's, from fixed bytes: the test has no djinn.
fn released(seed: u8) -> PersonalVaultKeys {
    let keys = PersonalVaultKeys::requests()
        .into_iter()
        .enumerate()
        .map(|(index, request)| {
            let bytes = [seed.wrapping_add(index as u8); 32];
            let key = match request.public_only {
                true => *SigningKey::from_bytes(&bytes).verifying_key().as_bytes(),
                false => bytes,
            };
            ReleasedEpochKey { request, key }
        })
        .collect::<Vec<_>>();
    PersonalVaultKeys::from_released(&keys).unwrap()
}

#[test]
fn second_owner_is_refused_while_pairing_facts_remain_available() {
    let root = tempdir().unwrap();
    let persona = PersonaId::new();

    let owner = StartupUnlockedPersonalVault::open(root.path(), persona, released(7), []).unwrap();
    let duplicate = match StartupUnlockedPersonalVault::open(root.path(), persona, released(7), [])
    {
        Ok(_) => panic!("a second persona owner must be refused promptly"),
        Err(error) => error,
    };
    assert!(
        duplicate.contains("another resident may already own this persona"),
        "{duplicate}"
    );
    assert_eq!(
        released(7).writer(),
        owner.writer(),
        "pairing facts must not reopen the resident-owned Knot stores",
    );

    drop(owner);
    drop(StartupUnlockedPersonalVault::open(root.path(), persona, released(7), []).unwrap());
}
