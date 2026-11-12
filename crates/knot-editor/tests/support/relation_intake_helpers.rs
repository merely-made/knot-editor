// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// Helpers of relation_intake.rs, included into it textually (a mechanical split
// to stay under the line ceiling): the constants, fixtures and builders.

const SPACE: [u8; 32] = [0x51; 32];
const VAULT_KEY: [u8; 32] = [0x52; 32];
const SUBJECT_SURFACE: Uuid = Uuid::from_u128(0x5101);
const OBJECT_SURFACE: Uuid = Uuid::from_u128(0x5102);
const ASSERTED_AT: u64 = 1_700_000_000_123;
const WITHDRAWN_AT: u64 = 1_700_000_100_456;

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

struct SignedProjection {
    directory: TempDir,
    verified: KnotRelationProjection,
    stale_held: KnotRelationProjection,
    projection: knot_editor::KnotDocumentProjection,
    subject: KnotRelationEndpointV1,
    object: KnotRelationEndpointV1,
    signer_iri: String,
}

async fn signed_projection() -> SignedProjection {
    signed_projection_with_first_withdrawal(KnotAssertedTime::At(WITHDRAWN_AT)).await
}

async fn signed_projection_with_first_withdrawal(
    first_withdrawal_time: KnotAssertedTime,
) -> SignedProjection {
    let directory = tempfile::tempdir().unwrap();
    let provider = InMemoryProvider::from_seed([0x61; 32]);
    let seed = provider.master_keypair().to_seed();
    let signer = provider.master_public_key().to_bytes();
    let signer_iri = format!("urn:knot:author:{}", hex(&signer));
    let sync_path = directory.path().join("knot-sync.redb");
    let vault = KnotVault::open(directory.path().join("vault"), VAULT_KEY).unwrap();
    let store = KnotSyncFileStore::open(&sync_path, SPACE, [signer]).unwrap();

    let subject_body = "# Essay\nThe claim is recorded.\n";
    let object_body = "# Source\nThe supporting source.\n";
    let subject_document = document("essay", subject_body);
    let object_document = document("source", object_body);
    let subject_head = store
        .author_at(
            &seed,
            &vault,
            &KnotSyncEvent::Put(subject_document),
            KnotAssertedTime::At(ASSERTED_AT),
        )
        .await
        .unwrap();
    let object_head = store
        .author_at(
            &seed,
            &vault,
            &KnotSyncEvent::Put(object_document),
            KnotAssertedTime::None,
        )
        .await
        .unwrap();
    let subject = endpoint(
        "essay",
        *subject_head.hash.as_bytes(),
        subject_body,
        "The claim is recorded.",
    );
    let object = endpoint(
        "source",
        *object_head.hash.as_bytes(),
        object_body,
        "The supporting source.",
    );

    let retracted = store
        .author_at(
            &seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                subject: subject.clone(),
                object: object.clone(),
                evidence: Some("urn:evidence:shared".into()),
                qualification: Some("earlier qualification".into()),
            },
            KnotAssertedTime::At(ASSERTED_AT),
        )
        .await
        .unwrap();
    let held = store
        .author_at(
            &seed,
            &vault,
            &KnotSyncEvent::AssertRelation {
                predicate: KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports),
                subject: subject.clone(),
                object: object.clone(),
                evidence: Some("urn:evidence:shared".into()),
                qualification: Some("later qualification".into()),
            },
            KnotAssertedTime::None,
        )
        .await
        .unwrap();
    let stale_held = KnotRelationProjection::from_store(&store, &vault)
        .await
        .unwrap();
    store
        .author_at(
            &seed,
            &vault,
            &KnotSyncEvent::RetractRelation {
                assertion: *retracted.hash.as_bytes(),
            },
            first_withdrawal_time,
        )
        .await
        .unwrap();
    let later_withdrawal_time = match first_withdrawal_time {
        KnotAssertedTime::At(_) | KnotAssertedTime::Now => KnotAssertedTime::None,
        KnotAssertedTime::None => KnotAssertedTime::At(WITHDRAWN_AT),
    };
    store
        .author_at(
            &seed,
            &vault,
            &KnotSyncEvent::RetractRelation {
                assertion: *retracted.hash.as_bytes(),
            },
            later_withdrawal_time,
        )
        .await
        .unwrap();

    store.save_checkpoint(&vault).await.unwrap();
    drop(store);
    drop(vault);
    let vault = KnotVault::open(directory.path().join("vault"), VAULT_KEY).unwrap();
    let reopened = KnotSyncFileStore::open(&sync_path, SPACE, [signer]).unwrap();
    let verified = KnotRelationProjection::from_store(&reopened, &vault)
        .await
        .unwrap();
    let projection = verified.as_projection().clone();
    assert_eq!(projection.relations.len(), 2);
    let expected_first_withdrawal = match first_withdrawal_time {
        KnotAssertedTime::At(value) => Some(value),
        KnotAssertedTime::Now => {
            relation_first_withdrawal_time(&projection, *retracted.hash.as_bytes())
        },
        KnotAssertedTime::None => None,
    };
    assert!(projection.relations.iter().any(|relation| {
        relation.id == *retracted.hash.as_bytes()
            && relation.asserted_at_ms == Some(ASSERTED_AT)
            && relation
                .retractions
                .first()
                .and_then(|value| value.withdrawn_at_ms)
                == expected_first_withdrawal
    }));
    assert!(projection.relations.iter().any(|relation| {
        relation.id == *held.hash.as_bytes()
            && relation.asserted_at_ms.is_none()
            && relation.retractions.is_empty()
    }));

    SignedProjection {
        directory,
        verified,
        stale_held,
        projection,
        subject,
        object,
        signer_iri,
    }
}

fn relation_first_withdrawal_time(
    projection: &knot_editor::KnotDocumentProjection,
    id: [u8; 32],
) -> Option<u64> {
    projection
        .relations
        .iter()
        .find(|relation| relation.id == id)
        .and_then(|relation| relation.retractions.first())
        .and_then(|retraction| retraction.withdrawn_at_ms)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn graph_with_endpoints() -> (Graph, Uuid, Uuid) {
    let mut graph = Graph::default();
    let subject_url = "https://mere.test/essay";
    let object_url = "https://mere.test/source";
    let subject_key = add_node(
        &mut graph,
        Some(SUBJECT_SURFACE),
        subject_url.into(),
        PortablePoint::new(10.0, 20.0),
    );
    let object_key = add_node(
        &mut graph,
        Some(OBJECT_SURFACE),
        object_url.into(),
        PortablePoint::new(30.0, 40.0),
    );
    let subject_resource = ResourceNode::new(subject_url).id();
    let object_resource = ResourceNode::new(object_url).id();
    for (surface_id, resource_id, iri) in [
        (SUBJECT_SURFACE, subject_resource, subject_url),
        (OBJECT_SURFACE, object_resource, object_url),
    ] {
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetResourceRecordById {
                resource_id,
                record: Some(PersistedResourceRecord {
                    canonical_iri: iri.into(),
                    facets: Vec::new(),
                }),
            },
        );
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetShownResourceById {
                surface_id,
                resource_id: Some(resource_id),
            },
        );
    }
    assert_eq!(graph.shown_resource_id(subject_key), Some(subject_resource));
    assert_eq!(graph.shown_resource_id(object_key), Some(object_resource));
    (graph, subject_resource, object_resource)
}

fn bindings(
    projection: &knot_editor::KnotDocumentProjection,
    object_surface: Uuid,
    object_resource: Uuid,
) -> BTreeMap<(String, [u8; 32]), KnotEndpointBinding> {
    let relation = &projection.relations[0];
    BTreeMap::from([
        (
            (
                relation.subject.document_id.clone(),
                relation.subject.document_head,
            ),
            KnotEndpointBinding {
                surface_id: SUBJECT_SURFACE,
                shown_resource_id: ResourceNode::new("https://mere.test/essay").id(),
            },
        ),
        (
            (
                relation.object.document_id.clone(),
                relation.object.document_head,
            ),
            KnotEndpointBinding {
                surface_id: object_surface,
                shown_resource_id: object_resource,
            },
        ),
    ])
}

fn imported_statement(
    projection: &knot_editor::KnotDocumentProjection,
    relation: &knot_editor::KnotRelationAssertionV1,
    signer_iri: &str,
    qualification: Option<String>,
) -> SemanticStatement {
    let predicate = projection.predicate_iri(relation).unwrap().to_owned();
    let withdrawal = relation
        .retractions
        .first()
        .map(|retraction| AssertionWithdrawal {
            at_ms: retraction.withdrawn_at_ms,
            by: Some(signer_iri.to_owned()),
        });
    SemanticStatement {
        statement_id: format!("urn:knot:assertion:{}", hex(&relation.operation)),
        recognized_sub_kind: kernel::graph::sub_kind_from_iri(&predicate),
        predicate,
        label: qualification.clone(),
        basis: AssertionBasis {
            note: qualification,
            method: None,
            evidence: relation.evidence.iter().cloned().collect(),
        },
        graph_scope: GraphScope::Custom(format!("urn:knot:space:{}", hex(&SPACE))),
        provenance_iri: Some(signer_iri.to_owned()),
        first_asserted_ms: relation.asserted_at_ms,
        asserted_at_ms: relation.asserted_at_ms,
        withdrawn: withdrawal,
    }
}

fn durable_session(
    root: &Path,
    graph: Graph,
) -> (
    GraphSession<DirectoryBackend>,
    MereSessions<DirectoryBackend>,
) {
    let backend = DirectoryBackend::open(root.join("mere")).unwrap();
    let sessions = MereSessions::new(backend.clone());
    let mut session = sessions.begin_recorded(Author::user(), Some(graph));
    pollster::block_on(session.flush(SystemTime::UNIX_EPOCH)).unwrap();
    (session, sessions)
}
