// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
#![allow(dead_code)]

//! Explicit, revision-bound intake of Knot's verified relation projection.
//!
//! Knot remains the authority for signed operations and their full causal
//! history. This adapter records those operation identities in a Mere session
//! only when the caller has admitted both exact document heads to existing
//! surface/resource pairs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use kernel::graph::{
    CapturedDelta, Graph, SemanticStatement, apply::GraphDelta, sub_kind_from_iri,
    with_isolated_capture,
};
use kernel::types::{AssertionBasis, AssertionWithdrawal, GraphScope};
use knot_editor::{KnotDocumentProjection, KnotRelationAssertionV1, KnotSyncFileStore, KnotVault};
use pandect::graph_session::LegacyTransactionBackend;
use pandect::{Author, GraphSession};
use uuid::Uuid;

/// A caller-admitted existing surface and the resource it is expected to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnotEndpointBinding {
    pub surface_id: Uuid,
    pub shown_resource_id: Uuid,
}

/// Intake outcomes. Omitted counts relation records, not endpoints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KnotRelationIntakeReport {
    pub imported: usize,
    pub already_present: usize,
    pub omitted_hidden_or_unmapped: usize,
}

/// A refused projection intake. No session state is changed on this error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotRelationIntakeError(String);

impl std::fmt::Display for KnotRelationIntakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KnotRelationIntakeError {}

fn refuse(message: impl Into<String>) -> KnotRelationIntakeError {
    KnotRelationIntakeError(message.into())
}

/// A Knot-authenticated relation projection.
///
/// The field is private and this type has no raw-projection constructor. It
/// can only be created by folding Knot's verified store through
/// [`KnotRelationProjection::from_store`].
#[derive(Clone, Debug)]
pub struct KnotRelationProjection {
    projection: KnotDocumentProjection,
}

impl KnotRelationProjection {
    /// Fold the store with Knot's normal signature, causal, and capture
    /// validation before exposing its read-only projection.
    pub async fn from_store(
        store: &KnotSyncFileStore,
        vault: &KnotVault,
    ) -> Result<Self, KnotRelationIntakeError> {
        let projection = store
            .projection(vault)
            .await
            .map_err(|error| refuse(format!("Knot relation projection refused: {error}")))?;
        Ok(Self { projection })
    }

    /// Read the authenticated projection without granting mutation access.
    pub fn as_projection(&self) -> &KnotDocumentProjection {
        &self.projection
    }
}

fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn operation_iri(operation: &[u8; 32]) -> String {
    format!("urn:knot:assertion:{}", hex32(operation))
}

fn signer_iri(author: &[u8; 32]) -> String {
    format!("urn:knot:author:{}", hex32(author))
}

#[derive(Clone)]
struct PreparedRelation {
    id: String,
    from: kernel::graph::NodeKey,
    to: kernel::graph::NodeKey,
    statement: SemanticStatement,
}

fn same_immutable_source(left: &SemanticStatement, right: &SemanticStatement) -> bool {
    left.statement_id == right.statement_id
        && left.predicate == right.predicate
        && left.recognized_sub_kind == right.recognized_sub_kind
        && left.graph_scope == right.graph_scope
        && left.provenance_iri == right.provenance_iri
        && left.basis == right.basis
        && left.first_asserted_ms == right.first_asserted_ms
        && left.asserted_at_ms == right.asserted_at_ms
        && (left.withdrawn.is_none()
            || right.withdrawn.is_none()
            || left.withdrawn == right.withdrawn)
}

/// Import the verified relations in a trusted Knot projection.
///
/// Bindings are keyed by the exact source `(document_id, document_head)` pair.
/// A missing binding omits that relation; a stale binding (missing surface,
/// missing resource, or changed shown resource) refuses the whole batch.
/// This function does not infer an endpoint, create graph nodes, or interpret
/// rejected/unverified Knot records. A missing binding omits that relation.
/// For a withdrawn assertion, only its first effective withdrawal is carried
/// into Mere; the later retraction operation IDs and clocks remain Knot-owned.
pub async fn import_knot_relations<B>(
    session: &mut GraphSession<B>,
    projection: &KnotRelationProjection,
    expected_space: [u8; 32],
    bindings: &BTreeMap<(String, [u8; 32]), KnotEndpointBinding>,
    author: Author,
    at: SystemTime,
) -> Result<KnotRelationIntakeReport, KnotRelationIntakeError>
where
    B: LegacyTransactionBackend + Clone,
{
    let projection = projection.as_projection();
    let graph = session.graph();
    let mut unique = BTreeMap::<[u8; 32], KnotRelationAssertionV1>::new();

    // Validate source identity and scope before filtering endpoints. A valid
    // projection must never smuggle another space through an omitted binding.
    for relation in &projection.relations {
        if relation.scope != expected_space {
            return Err(refuse(format!(
                "Knot relation {} belongs to a different space",
                hex32(&relation.id)
            )));
        }
        if relation.id != relation.operation {
            return Err(refuse(format!(
                "Knot relation {} has inconsistent operation identity",
                hex32(&relation.id)
            )));
        }
        if relation.subject.document_id.is_empty() || relation.object.document_id.is_empty() {
            return Err(refuse(format!(
                "Knot relation {} has an empty endpoint document id",
                hex32(&relation.id)
            )));
        }
        let predicate = projection.predicate_iri(relation).ok_or_else(|| {
            refuse(format!(
                "Knot relation {} has no resolved predicate IRI",
                hex32(&relation.id)
            ))
        })?;
        if predicate.trim().is_empty() {
            return Err(refuse(format!(
                "Knot relation {} has an empty predicate IRI",
                hex32(&relation.id)
            )));
        }
        let mut seen_retractions = BTreeMap::new();
        for retraction in &relation.retractions {
            if retraction.author != relation.author {
                return Err(refuse(format!(
                    "Knot relation {} has a retraction by a different author",
                    hex32(&relation.id)
                )));
            }
            if let Some(previous) = seen_retractions.insert(retraction.operation, retraction) {
                if previous != retraction {
                    return Err(refuse(format!(
                        "Knot relation {} has conflicting duplicate retraction identity",
                        hex32(&relation.id)
                    )));
                }
            }
        }
        if let Some(previous) = unique.insert(relation.id, relation.clone())
            && previous != *relation
        {
            return Err(refuse(format!(
                "Knot projection contains conflicting records for operation {}",
                hex32(&relation.id)
            )));
        }
    }

    let mut report = KnotRelationIntakeReport::default();
    let mut prepared = Vec::new();
    for relation in unique.values() {
        let source_key = (
            relation.subject.document_id.clone(),
            relation.subject.document_head,
        );
        let target_key = (
            relation.object.document_id.clone(),
            relation.object.document_head,
        );
        let (Some(source), Some(target)) = (bindings.get(&source_key), bindings.get(&target_key))
        else {
            report.omitted_hidden_or_unmapped += 1;
            continue;
        };

        let resolve = |binding: KnotEndpointBinding, role: &str| {
            let key = graph
                .get_node_key_by_id(binding.surface_id)
                .ok_or_else(|| {
                    refuse(format!(
                        "admitted Knot {role} surface {} is missing",
                        binding.surface_id
                    ))
                })?;
            let shown = graph.shown_resource_id(key).ok_or_else(|| {
                refuse(format!(
                    "admitted Knot {role} surface {} shows no resource",
                    binding.surface_id
                ))
            })?;
            if shown != binding.shown_resource_id || graph.resource(shown).is_none() {
                return Err(refuse(format!(
                    "admitted Knot {role} surface {} no longer shows expected resource {}",
                    binding.surface_id, binding.shown_resource_id
                )));
            }
            Ok::<_, KnotRelationIntakeError>(key)
        };
        let from = resolve(*source, "subject")?;
        let to = resolve(*target, "object")?;
        let predicate = projection
            .predicate_iri(relation)
            .expect("resolved while validating projection")
            .to_owned();
        let withdrawal = relation
            .retractions
            .first()
            .map(|retraction| AssertionWithdrawal {
                at_ms: retraction.withdrawn_at_ms,
                by: Some(signer_iri(&retraction.author)),
            });
        let statement = SemanticStatement {
            statement_id: operation_iri(&relation.operation),
            recognized_sub_kind: sub_kind_from_iri(&predicate),
            predicate,
            label: relation.qualification.clone(),
            basis: AssertionBasis {
                note: relation.qualification.clone(),
                method: None,
                evidence: relation.evidence.iter().cloned().collect::<Vec<_>>(),
            },
            graph_scope: GraphScope::Custom(format!("urn:knot:space:{}", hex32(&expected_space))),
            provenance_iri: Some(signer_iri(&relation.author)),
            first_asserted_ms: relation.asserted_at_ms,
            asserted_at_ms: relation.asserted_at_ms,
            withdrawn: withdrawal,
        };
        prepared.push(PreparedRelation {
            id: statement.statement_id.clone(),
            from,
            to,
            statement,
        });
    }

    // Preflight into a detached graph with an isolated recorder. Any conflict
    // or unsupported capture is discarded before the durable session writes.
    let captured = Arc::new(Mutex::new(Vec::<CapturedDelta>::new()));
    let sink = captured.clone();
    let mut candidate = graph.clone();
    candidate.set_recorder(Some(Arc::new(move |delta| {
        sink.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(delta.clone());
    })));
    let mut changed = BTreeSet::new();
    with_isolated_capture(|| -> Result<(), KnotRelationIntakeError> {
        for item in &prepared {
            let before = candidate
                .find_semantic_statement(&item.id)
                .map(|(_, statement)| statement.clone());
            if let Some(existing) = &before
                && !same_immutable_source(existing, &item.statement)
            {
                return Err(refuse(format!(
                    "Knot relation {} conflicts with an existing signed source record",
                    item.id
                )));
            }
            candidate
                .try_assert_persisted_semantic_statement(item.from, item.to, item.statement.clone())
                .map_err(|error| refuse(format!("Knot relation {} refused: {error}", item.id)))?;
            let after = candidate
                .find_semantic_statement(&item.id)
                .map(|(_, statement)| statement.clone());
            if before != after {
                changed.insert(item.id.clone());
            }
        }
        Ok(())
    })?;

    let captures = captured
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let mut deltas = Vec::<GraphDelta>::with_capacity(captures.len());
    for capture in captures {
        let delta = capture.replay().into_write().map_err(|why| {
            refuse(format!(
                "Knot intake produced a non-replayable graph capture: {why}"
            ))
        })?;
        deltas.push(delta);
    }
    if !deltas.is_empty() {
        session
            .transact_deltas(author, deltas, at)
            .await
            .map_err(|error| refuse(format!("Knot relation transaction refused: {error}")))?;
    }
    report.imported = changed.len();
    report.already_present = prepared.len().saturating_sub(report.imported);
    Ok(report)
}
