// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Signed Knot relation projection into durable Mere assertion history.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use chartulary::resource_id;
#[path = "support/knot_relation_intake.rs"]
mod knot_relation_intake;
use knot_relation_intake::{KnotEndpointBinding, KnotRelationProjection, import_knot_relations};
use kernel::geometry::PortablePoint;
use kernel::graph::{
    Author, Graph, ResourceNode, SemanticStatement,
    apply::apply_graph_delta,
    apply::{GraphDelta, add_node},
};
use kernel::persistence::PersistedResourceRecord;
use kernel::types::{AssertionBasis, AssertionWithdrawal, GraphScope};
use knot_editor::{
    KnotAssertedTime, KnotCorePredicateV1, KnotPredicateRefV1, KnotRelationEndpointV1,
    KnotRelationPositionV1, KnotSyncEvent, KnotSyncFileStore, KnotVault, VaultDocument,
    capture_endpoint_context,
};
use muniment::DirectoryBackend;
use pandect::{GraphSession, MereSessions};
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::TempDir;
use uuid::Uuid;

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

#[tokio::test]
async fn signed_projection_imports_exact_ids_and_survives_fresh_process_reopen() {
    let source = signed_projection().await;
    let mere_root = source.directory.path().join("mere-store");
    let (graph, subject_resource, object_resource) = graph_with_endpoints();
    let backend = DirectoryBackend::open(mere_root.clone()).unwrap();
    let sessions = MereSessions::new(backend.clone());
    let mut session = sessions.begin_recorded(Author::user(), Some(graph));
    session.flush(SystemTime::UNIX_EPOCH).await.unwrap();
    let admitted = bindings(&source.projection, OBJECT_SURFACE, object_resource);
    let report = import_knot_relations(
        &mut session,
        &source.verified,
        SPACE,
        &admitted,
        Author::person("urn:mere:local-recorder"),
        SystemTime::UNIX_EPOCH,
    )
    .await
    .unwrap();
    assert_eq!(report.imported, 2);
    assert_eq!(report.already_present, 0);
    assert_eq!(report.omitted_hidden_or_unmapped, 0);

    let mut expected_ids = Vec::new();
    for relation in &source.projection.relations {
        let id = format!("urn:knot:assertion:{}", hex(&relation.operation));
        let (owner, statement) = session.graph().find_semantic_statement(&id).unwrap();
        assert_eq!(
            session.graph().find_held_semantic_statement(&id).is_some(),
            relation.retractions.is_empty()
        );
        assert_eq!(
            statement.provenance_iri.as_deref(),
            Some(source.signer_iri.as_str())
        );
        assert_eq!(
            statement.graph_scope,
            GraphScope::Custom(format!("urn:knot:space:{}", hex(&SPACE)))
        );
        assert_eq!(statement.first_asserted_ms, relation.asserted_at_ms);
        assert_eq!(statement.asserted_at_ms, relation.asserted_at_ms);
        assert_eq!(statement.basis.note, relation.qualification);
        assert_eq!(
            statement.basis.evidence,
            relation.evidence.iter().cloned().collect::<Vec<_>>()
        );
        if let Some(retraction) = relation.retractions.first() {
            assert_eq!(
                statement.withdrawn.as_ref().and_then(|value| value.at_ms),
                retraction.withdrawn_at_ms
            );
            assert_eq!(
                statement
                    .withdrawn
                    .as_ref()
                    .and_then(|value| value.by.as_deref()),
                Some(source.signer_iri.as_str())
            );
        }
        assert!(matches!(owner, kernel::graph::RelationKey::Resource(_)));
        expected_ids.push(id);
    }
    assert_eq!(expected_ids.len(), 2);
    assert_ne!(expected_ids[0], expected_ids[1]);
    let first = session
        .graph()
        .find_semantic_statement(&expected_ids[0])
        .unwrap()
        .1;
    let second = session
        .graph()
        .find_semantic_statement(&expected_ids[1])
        .unwrap()
        .1;
    assert!(first.basis.same_reason_as(&second.basis));
    let after_first_import = serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap();
    let after_first_import_storage = directory_bytes(&mere_root);
    let after_first_import_cursor = session.journal().live_cursor();
    let repeated = import_knot_relations(
        &mut session,
        &source.verified,
        SPACE,
        &admitted,
        Author::person("urn:mere:local-recorder"),
        SystemTime::UNIX_EPOCH,
    )
    .await
    .unwrap();
    assert_eq!(repeated.imported, 0);
    assert_eq!(repeated.already_present, 2);
    assert_eq!(
        serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap(),
        after_first_import
    );
    assert_eq!(session.journal().live_cursor(), after_first_import_cursor);
    assert_eq!(directory_bytes(&mere_root), after_first_import_storage);

    let stale_held = import_knot_relations(
        &mut session,
        &source.stale_held,
        SPACE,
        &admitted,
        Author::person("urn:mere:local-recorder"),
        SystemTime::UNIX_EPOCH,
    )
    .await
    .unwrap();
    assert_eq!(stale_held.imported, 0);
    assert_eq!(stale_held.already_present, 2);
    let withdrawn_id = source
        .projection
        .relations
        .iter()
        .find(|relation| !relation.retractions.is_empty())
        .map(|relation| format!("urn:knot:assertion:{}", hex(&relation.operation)))
        .unwrap();
    assert_eq!(
        session
            .graph()
            .find_semantic_statement(&withdrawn_id)
            .unwrap()
            .1
            .withdrawn
            .as_ref()
            .and_then(|withdrawn| withdrawn.at_ms),
        Some(WITHDRAWN_AT)
    );
    assert_eq!(
        serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap(),
        after_first_import
    );
    assert_eq!(session.journal().live_cursor(), after_first_import_cursor);
    assert_eq!(directory_bytes(&mere_root), after_first_import_storage);
    assert_eq!(
        session
            .graph()
            .shown_resource_id(session.graph().get_node_key_by_id(SUBJECT_SURFACE).unwrap()),
        Some(subject_resource)
    );
    assert_eq!(
        session
            .graph()
            .shown_resource_id(session.graph().get_node_key_by_id(OBJECT_SURFACE).unwrap()),
        Some(object_resource)
    );
    session.flush(SystemTime::UNIX_EPOCH).await.unwrap();

    let session_id = session.id().as_uuid().to_string();
    let expected_file = source.directory.path().join("expected-graph.json");
    std::fs::write(
        &expected_file,
        serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap(),
    )
    .unwrap();
    drop(session);
    drop(sessions);
    drop(backend);

    let status = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("fresh_process_reopen_helper")
        .arg("--ignored")
        .arg("--nocapture")
        .env("MERE_KNOT_REOPEN_ROOT", &mere_root)
        .env("MERE_KNOT_REOPEN_SESSION", session_id)
        .env("MERE_KNOT_REOPEN_EXPECTED", &expected_file)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "fresh process reopen helper failed: {status}"
    );
    let _ = source.subject;
    let _ = source.object;
}

#[test]
#[ignore = "invoked by the parent test in a fresh OS process"]
fn fresh_process_reopen_helper() {
    let root = std::env::var_os("MERE_KNOT_REOPEN_ROOT").unwrap();
    let session = std::env::var("MERE_KNOT_REOPEN_SESSION").unwrap();
    let expected = std::env::var_os("MERE_KNOT_REOPEN_EXPECTED").unwrap();
    let id = uuid::Uuid::parse_str(&session).unwrap();
    let id = incipit::SessionId::from_uuid(id);
    let backend = DirectoryBackend::open(PathBuf::from(root)).unwrap();
    let reopened = pollster::block_on(GraphSession::open(backend, id)).unwrap();
    let actual = serde_json::to_vec(&reopened.graph().to_snapshot_at(0)).unwrap();
    assert_eq!(actual, std::fs::read(expected).unwrap());
}

#[tokio::test]
async fn missing_exact_head_omits_but_stale_shown_resource_refuses_without_changes() {
    let source = signed_projection().await;
    let (graph, _, object_resource) = graph_with_endpoints();
    let (mut session, _) = durable_session(source.directory.path(), graph);
    let mut admitted = bindings(&source.projection, OBJECT_SURFACE, object_resource);
    let wrong_head = [0xEE; 32];
    admitted.remove(&(
        source.projection.relations[0].subject.document_id.clone(),
        source.projection.relations[0].subject.document_head,
    ));
    admitted.insert(
        (
            source.projection.relations[0].subject.document_id.clone(),
            wrong_head,
        ),
        KnotEndpointBinding {
            surface_id: SUBJECT_SURFACE,
            shown_resource_id: ResourceNode::new("https://mere.test/essay").id(),
        },
    );
    let report = import_knot_relations(
        &mut session,
        &source.verified,
        SPACE,
        &admitted,
        Author::user(),
        SystemTime::UNIX_EPOCH,
    )
    .await
    .unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(report.omitted_hidden_or_unmapped, 2);

    let before_scope_refusal = serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap();
    let scope_cursor = session.journal().live_cursor();
    assert!(
        import_knot_relations(
            &mut session,
            &source.verified,
            [0x99; 32],
            &bindings(&source.projection, OBJECT_SURFACE, object_resource),
            Author::user(),
            SystemTime::UNIX_EPOCH,
        )
        .await
        .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap(),
        before_scope_refusal
    );
    assert_eq!(session.journal().live_cursor(), scope_cursor);

    let mut stale = bindings(&source.projection, OBJECT_SURFACE, object_resource);
    stale
        .values_mut()
        .find(|binding| binding.surface_id == SUBJECT_SURFACE)
        .unwrap()
        .shown_resource_id = resource_id("https://mere.test/retargeted");
    let before = serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap();
    let cursor = session.journal().live_cursor();
    assert!(
        import_knot_relations(
            &mut session,
            &source.verified,
            SPACE,
            &stale,
            Author::user(),
            SystemTime::UNIX_EPOCH,
        )
        .await
        .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap(),
        before
    );
    assert_eq!(session.journal().live_cursor(), cursor);
}

fn directory_bytes(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path.strip_prefix(root).unwrap().to_path_buf(), bytes)
        })
        .collect()
}

#[tokio::test]
async fn the_first_undated_retraction_remains_undated_after_a_later_timed_retraction_and_import() {
    let source = signed_projection_with_first_withdrawal(KnotAssertedTime::None).await;
    let (graph, _, object_resource) = graph_with_endpoints();
    let mere_root = source.directory.path().join("undated-withdrawal-store");
    let backend = DirectoryBackend::open(mere_root).unwrap();
    let sessions = MereSessions::new(backend);
    let mut session = sessions.begin_recorded(Author::user(), Some(graph));
    session.flush(SystemTime::UNIX_EPOCH).await.unwrap();
    let admitted = bindings(&source.projection, OBJECT_SURFACE, object_resource);
    import_knot_relations(
        &mut session,
        &source.verified,
        SPACE,
        &admitted,
        Author::user(),
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(99_999),
    )
    .await
    .unwrap();

    let withdrawn = source
        .projection
        .relations
        .iter()
        .find(|relation| !relation.retractions.is_empty())
        .unwrap();
    assert_eq!(withdrawn.retractions[0].withdrawn_at_ms, None);
    assert_eq!(withdrawn.retractions[1].withdrawn_at_ms, Some(WITHDRAWN_AT));
    let id = format!("urn:knot:assertion:{}", hex(&withdrawn.operation));
    let statement = session.graph().find_semantic_statement(&id).unwrap().1;
    assert_eq!(statement.withdrawn.as_ref().unwrap().at_ms, None);
    assert_eq!(
        statement.withdrawn.as_ref().unwrap().by.as_deref(),
        Some(source.signer_iri.as_str())
    );
}

#[tokio::test]
async fn conflicting_effective_withdrawal_refuses_without_changing_graph_or_storage() {
    let source = signed_projection().await;
    let relation = source
        .projection
        .relations
        .iter()
        .find(|relation| !relation.retractions.is_empty())
        .unwrap();
    let (mut graph, _, object_resource) = graph_with_endpoints();
    let from = graph.get_node_key_by_id(SUBJECT_SURFACE).unwrap();
    let to = graph.get_node_key_by_id(OBJECT_SURFACE).unwrap();
    let mut existing = imported_statement(
        &source.projection,
        relation,
        &source.signer_iri,
        relation.qualification.clone(),
    );
    existing.withdrawn.as_mut().unwrap().at_ms = Some(WITHDRAWN_AT + 1);
    graph
        .try_assert_persisted_semantic_statement(from, to, existing)
        .unwrap();

    let mere_root = source.directory.path().join("withdrawal-conflict-store");
    let backend = DirectoryBackend::open(mere_root.clone()).unwrap();
    let sessions = MereSessions::new(backend);
    let mut session = sessions.begin_recorded(Author::user(), Some(graph));
    session.flush(SystemTime::UNIX_EPOCH).await.unwrap();
    let before_graph = serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap();
    let before_storage = directory_bytes(&mere_root);
    let before_cursor = session.journal().live_cursor();
    let admitted = bindings(&source.projection, OBJECT_SURFACE, object_resource);

    assert!(
        import_knot_relations(
            &mut session,
            &source.verified,
            SPACE,
            &admitted,
            Author::user(),
            SystemTime::UNIX_EPOCH,
        )
        .await
        .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap(),
        before_graph
    );
    assert_eq!(session.journal().live_cursor(), before_cursor);
    assert_eq!(directory_bytes(&mere_root), before_storage);
}

#[tokio::test]
async fn collision_after_a_valid_candidate_import_keeps_graph_and_storage_unchanged() {
    let source = signed_projection().await;
    let mut relations = source.projection.relations.clone();
    relations.sort_by_key(|relation| relation.id);
    assert_eq!(relations.len(), 2);
    let collision_relation = relations.pop().unwrap();
    for mismatch in ["note", "time"] {
        let (mut graph, _, object_resource) = graph_with_endpoints();
        let from = graph.get_node_key_by_id(SUBJECT_SURFACE).unwrap();
        let to = graph.get_node_key_by_id(OBJECT_SURFACE).unwrap();
        let mut existing = imported_statement(
            &source.projection,
            &collision_relation,
            &source.signer_iri,
            collision_relation.qualification.clone(),
        );
        if mismatch == "note" {
            existing.label = Some("receiving graph has a different qualification".into());
            existing.basis.note = existing.label.clone();
        } else {
            existing.asserted_at_ms = Some(ASSERTED_AT + 1);
        }
        graph
            .try_assert_persisted_semantic_statement(from, to, existing)
            .unwrap();

        let mere_root = source
            .directory
            .path()
            .join(format!("collision-store-{mismatch}"));
        let backend = DirectoryBackend::open(mere_root.clone()).unwrap();
        let sessions = MereSessions::new(backend.clone());
        let mut session = sessions.begin_recorded(Author::user(), Some(graph));
        session.flush(SystemTime::UNIX_EPOCH).await.unwrap();
        let before_graph = serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap();
        let before_storage = directory_bytes(&mere_root);
        let before_cursor = session.journal().live_cursor();
        let admitted = bindings(&source.projection, OBJECT_SURFACE, object_resource);

        assert!(
            import_knot_relations(
                &mut session,
                &source.verified,
                SPACE,
                &admitted,
                Author::user(),
                SystemTime::UNIX_EPOCH,
            )
            .await
            .is_err(),
            "the same signed operation with changed {mismatch} must refuse"
        );
        assert_eq!(
            serde_json::to_vec(&session.graph().to_snapshot_at(0)).unwrap(),
            before_graph
        );
        assert_eq!(session.journal().live_cursor(), before_cursor);
        assert_eq!(directory_bytes(&mere_root), before_storage);
    }
}
