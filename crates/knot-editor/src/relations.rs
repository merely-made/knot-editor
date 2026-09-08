// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Authored relation captures retained alongside Knot's causal document events.

use zeroize::Zeroize;

/// A selected source position captured as UTF-8 byte offsets.
///
/// It records the position at the observed document revision; it is not a
/// durable passage identity or an instruction to follow later text edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotRelationPositionV1 {
    pub start: u64,
    pub end: u64,
}

/// One document endpoint captured by an authored relation assertion.
///
/// `document_head` is the hash of an immutable, signed document-producing or
/// file-capture operation observed by the assertion. It never names a derived
/// merge head or an uncaptured reading of current disk bytes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotRelationEndpointV1 {
    pub document_id: String,
    pub document_head: [u8; 32],
    pub quote: String,
    pub position: Option<KnotRelationPositionV1>,
}

/// One author-only retraction retained on an assertion's history.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotRelationRetractionV1 {
    pub operation: [u8; 32],
    pub author: [u8; 32],
}

/// An attributable relation assertion projected from one signed operation.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotRelationAssertionV1 {
    /// The signed assertion operation hash, not caller-provided identity.
    pub id: [u8; 32],
    pub author: [u8; 32],
    pub operation: [u8; 32],
    /// The admitted Knot space that authenticated this assertion.
    pub scope: [u8; 32],
    pub predicate: String,
    pub subject: KnotRelationEndpointV1,
    pub object: KnotRelationEndpointV1,
    /// Opaque author-supplied reference, not a fetched or verified evidence record.
    pub evidence: Option<String>,
    pub qualification: Option<String>,
    /// Valid author-signed retractions. An empty list means the assertion is active.
    pub retractions: Vec<KnotRelationRetractionV1>,
}

/// An attributable assertion whose captured endpoints could not be verified.
///
/// It is retained for inspection and later repair, but is excluded from the
/// validated relation projection and access-filtered reads.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotUnverifiedRelationV1 {
    pub assertion: KnotRelationAssertionV1,
    pub reason: String,
}

/// A relation event retained in history but rejected from the active relation projection.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotRejectedRelationV1 {
    pub operation: [u8; 32],
    pub author: [u8; 32],
    pub reason: String,
}

pub(crate) fn validate_relation_payload(
    predicate: &str,
    subject: &KnotRelationEndpointV1,
    object: &KnotRelationEndpointV1,
) -> Result<(), String> {
    if predicate.is_empty() || predicate.trim() != predicate || predicate.contains('\0') {
        return Err("relation predicate must be a non-empty trimmed identifier".to_owned());
    }
    validate_endpoint(subject, "subject")?;
    validate_endpoint(object, "object")?;
    Ok(())
}

fn validate_endpoint(endpoint: &KnotRelationEndpointV1, role: &str) -> Result<(), String> {
    if endpoint.document_id.is_empty()
        || endpoint.document_id.trim() != endpoint.document_id
        || endpoint.document_id.contains('\0')
    {
        return Err(format!(
            "relation {role} document id must be non-empty and trimmed"
        ));
    }
    match (endpoint.quote.is_empty(), endpoint.position) {
        (true, None) => {},
        (false, Some(position)) if position.start <= position.end => {},
        (_, Some(position)) if position.start > position.end => {
            return Err(format!("relation {role} position ends before it starts"));
        },
        _ => {
            return Err(format!(
                "relation {role} must capture both a quote and UTF-8 byte position, or neither"
            ));
        },
    }
    Ok(())
}
