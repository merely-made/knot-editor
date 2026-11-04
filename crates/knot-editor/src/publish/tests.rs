// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::KnotSyncEvent;
use insigne::delegation::{CapabilityScope, DelegationCertificate, DelegationParent};
use notochord::{AdmittedPrincipal, RequestedAction, TrafficClass};
use personae::delegation::Issue;
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;

const NETWORK: NetworkId = NetworkId([0x91; 32]);
const ROOT: [u8; 32] = [0x92; 32];
const NOW_MS: u64 = 50;
const EXPIRY_MS: u64 = 100;
const SPACE: [u8; 32] = [0x93; 32];
const VAULT_KEY: [u8; 32] = [0x94; 32];

fn owner() -> InMemoryProvider {
    InMemoryProvider::from_seed([0x95; 32])
}

fn reader() -> InMemoryProvider {
    InMemoryProvider::from_seed([0x96; 32])
}

fn doc(id: &str, body: &str) -> VaultDocument {
    VaultDocument {
        id: id.into(),
        title: id.into(),
        body: body.as_bytes().to_vec(),
        media_type: "text/vnd.knot".into(),
    }
}

fn authority(publication: PublicationId) -> RetainedAuthority {
    let certificate = SignedDelegationCertificate::issue(
        &owner(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT),
            owner().master_public_key().to_bytes(),
            reader().master_public_key().to_bytes(),
            CapabilityScope {
                domain: KNOT_PUBLISH_DOMAIN.into(),
                resource: NETWORK.0.to_vec(),
                path_prefix: publication_path(publication),
                actions: [KNOT_PUBLISH_READ_ACTION.to_string()].into_iter().collect(),
            },
            5,
            10,
            Some(EXPIRY_MS),
            1,
            [0x97; 32],
        ),
    )
    .unwrap();
    RetainedAuthority::new(
        AdmittedPrincipal {
            subject: reader().master_public_key().to_bytes(),
            class: TrafficClass::Interactive,
            session_id: [0x98; 32],
            action: RequestedAction {
                domain: KNOT_PUBLISH_DOMAIN.into(),
                path: KNOT_PUBLISH_SERVICE.into(),
                action: KNOT_PUBLISH_READ_ACTION.into(),
            },
        },
        vec![certificate],
    )
}

#[test]
fn owner_controls_issue_and_revoke_one_recipient_share() {
    let mut catalog = KnotPublishCatalog::default();
    let selected = catalog.publish("field-notes");
    let ticket = catalog
        .issue_share(
            &owner(),
            KnotShareRecipient {
                publication: selected,
                publisher: [0x77; 32],
                reader: reader().master_public_key().to_bytes(),
                network: NETWORK,
                endpoint_ticket: "endpoint-ticket".into(),
                root_authority: ROOT,
                issued_at_ms: NOW_MS,
                expires_at_ms: Some(EXPIRY_MS),
                pinned_head: Some([0x9a; 32]),
            },
        )
        .unwrap();
    assert_eq!(ticket.publisher, [0x77; 32]);
    assert_eq!(
        ticket.delegations.last().unwrap().certificate.issuer,
        owner().master_public_key().to_bytes(),
        "the Personae issuer and carrier publisher may be different keys"
    );
    assert_eq!(ticket.publication, selected);
    assert_eq!(ticket.pinned_head, Some([0x9a; 32]));
    let certificate = ticket.delegations.last().unwrap();
    certificate.check().expect("the share's certificate checks");
    assert_eq!(
        certificate.certificate.scope.path_prefix,
        publication_path(selected)
    );
    assert_eq!(
        certificate.certificate.scope.actions,
        [KNOT_PUBLISH_READ_ACTION.to_string()].into_iter().collect()
    );

    let revocation = revoke_share(&owner(), &ticket, NOW_MS + 1).unwrap();
    revocation.check().expect("the owner's revocation checks");
    assert_eq!(
        revocation.revocation.certificate,
        certificate.certificate.id()
    );

    let unselected = PublicationId::from_uuid(Uuid::from_u128(99));
    assert!(matches!(
        catalog.issue_share(
            &owner(),
            KnotShareRecipient {
                publication: unselected,
                publisher: owner().master_public_key().to_bytes(),
                reader: reader().master_public_key().to_bytes(),
                network: NETWORK,
                endpoint_ticket: "unreachable".into(),
                root_authority: ROOT,
                issued_at_ms: NOW_MS,
                expires_at_ms: Some(EXPIRY_MS),
                pinned_head: None,
            },
        ),
        Err(KnotShareControlError::UnknownPublication)
    ));
}

#[tokio::test]
async fn selected_current_and_retained_versions_are_the_only_source_reads() {
    let directory = tempdir().unwrap();
    let vault = KnotVault::open(directory.path(), VAULT_KEY).unwrap();
    let writer = owner().master_public_key().to_bytes();
    let store = KnotSyncStore::in_memory(SPACE, [writer]);
    let first = store
        .author(
            &owner().master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("selected", "first")),
        )
        .await
        .unwrap();
    let second = store
        .author(
            &owner().master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("selected", "second")),
        )
        .await
        .unwrap();
    let unrelated = store
        .author(
            &owner().master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("unrelated", "hidden")),
        )
        .await
        .unwrap();

    let mut catalog = KnotPublishCatalog::default();
    let selected = catalog.publish("selected");
    let other = catalog.publish("unrelated");
    let authority = authority(selected);
    assert_eq!(
        catalog.list(&authority, &RevocationLedger::default(), NOW_MS),
        vec![selected],
        "a one-publication grant does not enumerate a neighbour"
    );

    let current = catalog
        .get_current(
            &store,
            &vault,
            &authority,
            &RevocationLedger::default(),
            NOW_MS,
            selected,
        )
        .await
        .unwrap();
    let KnotPublishRead::Document(current) = current else {
        panic!("the selected current document must be available")
    };
    assert_eq!(current.body, b"second");
    assert_eq!(current.operation, *second.hash.as_bytes());
    assert!(current.body_digest_matches());

    let retained = catalog
        .get_version(
            &store,
            &vault,
            &authority,
            &RevocationLedger::default(),
            NOW_MS,
            selected,
            *first.hash.as_bytes(),
        )
        .await
        .unwrap();
    let KnotPublishRead::Document(retained) = retained else {
        panic!("the exact retained Put must be available")
    };
    assert_eq!(retained.body, b"first");
    assert_eq!(retained.operation, *first.hash.as_bytes());

    for outcome in [
        catalog
            .get_current(
                &store,
                &vault,
                &authority,
                &RevocationLedger::default(),
                NOW_MS,
                other,
            )
            .await
            .unwrap(),
        catalog
            .get_version(
                &store,
                &vault,
                &authority,
                &RevocationLedger::default(),
                NOW_MS,
                selected,
                *unrelated.hash.as_bytes(),
            )
            .await
            .unwrap(),
    ] {
        assert_eq!(outcome, KnotPublishRead::NotAvailable);
    }
}

#[tokio::test]
async fn deletes_conflicts_and_pending_history_are_not_available() {
    let directory = tempdir().unwrap();
    let vault = KnotVault::open(directory.path(), VAULT_KEY).unwrap();
    let alice = owner();
    let bob = InMemoryProvider::from_seed([0x99; 32]);
    let writers = [
        alice.master_public_key().to_bytes(),
        bob.master_public_key().to_bytes(),
    ];

    let deleted = KnotSyncStore::in_memory(SPACE, writers);
    deleted
        .author(
            &alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("deleted", "before delete")),
        )
        .await
        .unwrap();
    deleted
        .author(
            &alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Delete {
                id: "deleted".into(),
            },
        )
        .await
        .unwrap();

    let conflict_left = KnotSyncStore::in_memory(SPACE, writers);
    let conflict_right = KnotSyncStore::in_memory(SPACE, writers);
    let left = conflict_left
        .author(
            &alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("conflict", "alice")),
        )
        .await
        .unwrap();
    let right = conflict_right
        .author(
            &bob.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("conflict", "bob")),
        )
        .await
        .unwrap();
    conflict_left.accept(&right).await.unwrap();

    let parent = KnotSyncStore::in_memory(SPACE, writers);
    let child = KnotSyncStore::in_memory(SPACE, writers);
    let pending = KnotSyncStore::in_memory(SPACE, writers);
    let base = parent
        .author(
            &alice.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("pending", "base")),
        )
        .await
        .unwrap();
    child.accept(&base).await.unwrap();
    let child_operation = child
        .author(
            &bob.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("pending", "child")),
        )
        .await
        .unwrap();
    pending.accept(&child_operation).await.unwrap();

    for (store, source) in [
        (&deleted, "deleted"),
        (&conflict_left, "conflict"),
        (&pending, "pending"),
    ] {
        let mut catalog = KnotPublishCatalog::default();
        let publication = catalog.publish(source);
        let outcome = catalog
            .get_current(
                store,
                &vault,
                &authority(publication),
                &RevocationLedger::default(),
                NOW_MS,
                publication,
            )
            .await
            .unwrap();
        assert_eq!(outcome, KnotPublishRead::NotAvailable);
    }
    assert_ne!(*left.hash.as_bytes(), *right.hash.as_bytes());
}

#[tokio::test]
async fn a_ticket_checks_its_pinned_causal_head_and_exact_bytes() {
    let directory = tempdir().unwrap();
    let vault = KnotVault::open(directory.path(), VAULT_KEY).unwrap();
    let writer = owner().master_public_key().to_bytes();
    let store = KnotSyncStore::in_memory(SPACE, [writer]);
    let operation = store
        .author(
            &owner().master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(doc("selected", "source")),
        )
        .await
        .unwrap();
    let mut catalog = KnotPublishCatalog::default();
    let publication = catalog.publish("selected");
    let current = catalog
        .get_current(
            &store,
            &vault,
            &authority(publication),
            &RevocationLedger::default(),
            NOW_MS,
            publication,
        )
        .await
        .unwrap();
    let KnotPublishRead::Document(document) = current else {
        panic!("selected source must be available")
    };
    let ticket = KnotShareTicket::new(
        writer,
        "endpoint-ticket",
        NETWORK,
        publication,
        Vec::new(),
        Some(*operation.hash.as_bytes()),
    );
    assert!(ticket.accepts(&document));

    let wrong_head = KnotShareTicket::new(
        writer,
        "endpoint-ticket",
        NETWORK,
        publication,
        Vec::new(),
        Some([0; 32]),
    );
    assert!(!wrong_head.accepts(&document));
    let mut tampered = document;
    tampered.body.push(b'!');
    assert!(!ticket.accepts(&tampered));
}
