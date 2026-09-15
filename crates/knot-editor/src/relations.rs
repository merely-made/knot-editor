// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Authored relation captures retained alongside Knot's causal document events.

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

use crate::settings::hex32;

/// Base IRI of Mere's canonical relation vocabulary. Knot consumes that table
/// rather than owning it; this is a local const because knot-editor depends on
/// neither graph-kernel nor linked-data.
pub const KNOT_REL_VOCAB: &str = "https://mere.computer/ns/rel#";

/// The writer-facing subset of Mere's relation vocabulary.
///
/// Identity is the Mere IRI, behaviour the matching sub-kind, and the standard
/// alignment (CiTO and friends) comes from Mere's table, never from Knot.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    Zeroize,
)]
#[serde(rename_all = "kebab-case")]
pub enum KnotCorePredicateV1 {
    Cites,
    Quotes,
    Supports,
    Contradicts,
    Questions,
    Elaborates,
    ExampleOf,
    Summarizes,
}

macro_rules! core_predicate_iri {
    ($slug:literal) => {
        concat!("https://mere.computer/ns/rel#", $slug)
    };
}

impl KnotCorePredicateV1 {
    pub const ALL: [Self; 8] = [
        Self::Cites,
        Self::Quotes,
        Self::Supports,
        Self::Contradicts,
        Self::Questions,
        Self::Elaborates,
        Self::ExampleOf,
        Self::Summarizes,
    ];

    /// The wire spelling, which is also the Mere sub-kind slug.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Cites => "cites",
            Self::Quotes => "quotes",
            Self::Supports => "supports",
            Self::Contradicts => "contradicts",
            Self::Questions => "questions",
            Self::Elaborates => "elaborates",
            Self::ExampleOf => "example-of",
            Self::Summarizes => "summarizes",
        }
    }

    pub fn iri(self) -> &'static str {
        match self {
            Self::Cites => core_predicate_iri!("cites"),
            Self::Quotes => core_predicate_iri!("quotes"),
            Self::Supports => core_predicate_iri!("supports"),
            Self::Contradicts => core_predicate_iri!("contradicts"),
            Self::Questions => core_predicate_iri!("questions"),
            Self::Elaborates => core_predicate_iri!("elaborates"),
            Self::ExampleOf => core_predicate_iri!("example-of"),
            Self::Summarizes => core_predicate_iri!("summarizes"),
        }
    }

    /// The Mere sub-kind a core predicate lowers to.
    pub fn sub_kind(self) -> &'static str {
        self.slug()
    }

    pub fn default_label(self) -> &'static str {
        match self {
            Self::Cites => "cites",
            Self::Quotes => "quotes",
            Self::Supports => "supports",
            Self::Contradicts => "contradicts",
            Self::Questions => "questions",
            Self::Elaborates => "elaborates",
            Self::ExampleOf => "example of",
            Self::Summarizes => "summarizes",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|core| core.slug() == slug)
    }

    pub fn from_iri(iri: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|core| core.iri() == iri)
    }
}

/// The identity an assertion stores for its predicate.
#[derive(Clone, Debug, PartialEq, Eq, Zeroize)]
pub enum KnotPredicateRefV1 {
    Core(KnotCorePredicateV1),
    /// A `KnotPredicateDefinitionV1` operation hash. Any id in a supersession
    /// chain is accepted; display and IRI resolve through the chain root.
    Defined([u8; 32]),
    /// Only produced by decoding an older or foreign payload. Never authored;
    /// always refused by [`validate_relation_payload`].
    Unrecognized(String),
}

impl Serialize for KnotPredicateRefV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // A bare slug, so a body sealed before this change reserializes
            // byte-identically and an old peer still reads a core assertion.
            Self::Core(core) => serializer.serialize_str(core.slug()),
            Self::Defined(id) => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("defined", &hex32(id))?;
                map.end()
            },
            Self::Unrecognized(label) => serializer.serialize_str(label),
        }
    }
}

fn parse_lowercase_hex32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

impl<'de> Deserialize<'de> for KnotPredicateRefV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RefVisitor;

        impl<'de> Visitor<'de> for RefVisitor {
            type Value = KnotPredicateRefV1;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a core predicate slug or IRI, or a defined-hash map")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                // An unknown label decodes rather than failing the whole event;
                // validation then refuses it into the rejected set.
                Ok(KnotCorePredicateV1::from_slug(value)
                    .or_else(|| KnotCorePredicateV1::from_iri(value))
                    .map_or_else(
                        || KnotPredicateRefV1::Unrecognized(value.to_owned()),
                        KnotPredicateRefV1::Core,
                    ))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let Some(key) = map.next_key::<String>()? else {
                    return Err(de::Error::custom("empty relation predicate map"));
                };
                if key != "defined" {
                    return Err(de::Error::custom(format!(
                        "unknown relation predicate key '{key}'"
                    )));
                }
                let value: String = map.next_value()?;
                let id = parse_lowercase_hex32(&value).ok_or_else(|| {
                    de::Error::custom("defined predicate is not 64 lowercase hex characters")
                })?;
                if map.next_key::<String>()?.is_some() {
                    return Err(de::Error::custom("relation predicate map has extra keys"));
                }
                Ok(KnotPredicateRefV1::Defined(id))
            }
        }

        deserializer.deserialize_any(RefVisitor)
    }
}

/// How much source context each endpoint keeps on either side of its quote.
pub const KNOT_RELATION_CONTEXT_CHARS: usize = 32;
/// The serialized ceiling for one captured context field, in UTF-8 bytes.
pub const KNOT_RELATION_CONTEXT_BYTES: usize = 4 * KNOT_RELATION_CONTEXT_CHARS;

/// Capture up to [`KNOT_RELATION_CONTEXT_CHARS`] characters either side of
/// `start..end`, always on character boundaries.
///
/// A Web Annotation `TextQuoteSelector` re-anchors on these; they are context,
/// not identity, and truncate at the edges of the source.
pub fn capture_endpoint_context(source: &str, start: usize, end: usize) -> (String, String) {
    let prefix = source.get(..start).map_or_else(String::new, |before| {
        let skip = before
            .chars()
            .count()
            .saturating_sub(KNOT_RELATION_CONTEXT_CHARS);
        before.chars().skip(skip).collect()
    });
    let suffix = source.get(end..).map_or_else(String::new, |after| {
        after.chars().take(KNOT_RELATION_CONTEXT_CHARS).collect()
    });
    (prefix, suffix)
}

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
    /// Source context immediately before `position`, for quote re-anchoring.
    /// Appended after the original fields so an old capture reserializes exactly.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    /// Source context immediately after `position`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suffix: String,
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
    pub predicate: KnotPredicateRefV1,
    pub subject: KnotRelationEndpointV1,
    pub object: KnotRelationEndpointV1,
    /// Opaque author-supplied reference, not a fetched or verified evidence record.
    pub evidence: Option<String>,
    pub qualification: Option<String>,
    /// Valid author-signed retractions. An empty list means the assertion is active.
    pub retractions: Vec<KnotRelationRetractionV1>,
    /// Author-asserted wall clock lifted from the signed header. Informative
    /// only; it never orders a fold. Appended last so an old checkpoint
    /// reserializes exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_at_ms: Option<u64>,
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
    predicate: &KnotPredicateRefV1,
    subject: &KnotRelationEndpointV1,
    object: &KnotRelationEndpointV1,
) -> Result<(), String> {
    // Existence, causal observation and retirement of a defined predicate are
    // catalog questions, answered against the fold in sync.rs.
    if let KnotPredicateRefV1::Unrecognized(label) = predicate {
        return Err(format!(
            "relation predicate '{label}' is neither a core predicate nor a defined predicate"
        ));
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
    for (field, value) in [("prefix", &endpoint.prefix), ("suffix", &endpoint.suffix)] {
        if value.contains('\0') || value.len() > KNOT_RELATION_CONTEXT_BYTES {
            return Err(format!(
                "relation {role} {field} must be NUL-free and at most {KNOT_RELATION_CONTEXT_BYTES} bytes"
            ));
        }
    }
    match (endpoint.quote.is_empty(), endpoint.position) {
        (true, None) => {
            if !endpoint.prefix.is_empty() || !endpoint.suffix.is_empty() {
                return Err(format!(
                    "relation {role} captures context without a quote and position"
                ));
            }
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> KnotRelationEndpointV1 {
        KnotRelationEndpointV1 {
            document_id: "essay".into(),
            document_head: [0x11; 32],
            quote: "quote".into(),
            position: Some(KnotRelationPositionV1 { start: 0, end: 5 }),
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    #[test]
    fn a_core_predicate_is_a_bare_slug_on_the_wire_and_an_iri_resolves_to_it() {
        let core = KnotPredicateRefV1::Core(KnotCorePredicateV1::Supports);
        let encoded = serde_json::to_string(&core).unwrap();
        assert_eq!(encoded, "\"supports\"");
        assert_eq!(
            serde_json::from_str::<KnotPredicateRefV1>(&encoded).unwrap(),
            core
        );

        let from_iri: KnotPredicateRefV1 =
            serde_json::from_str("\"https://mere.computer/ns/rel#example-of\"").unwrap();
        assert_eq!(
            from_iri,
            KnotPredicateRefV1::Core(KnotCorePredicateV1::ExampleOf)
        );
        for core in KnotCorePredicateV1::ALL {
            assert_eq!(core.iri(), format!("{KNOT_REL_VOCAB}{}", core.slug()));
            assert_eq!(KnotCorePredicateV1::from_slug(core.slug()), Some(core));
            assert_eq!(KnotCorePredicateV1::from_iri(core.iri()), Some(core));
            assert_eq!(core.sub_kind(), core.slug());
        }

        let unknown: KnotPredicateRefV1 = serde_json::from_str("\"corroborates\"").unwrap();
        assert_eq!(
            unknown,
            KnotPredicateRefV1::Unrecognized("corroborates".into())
        );
        assert_eq!(serde_json::to_string(&unknown).unwrap(), "\"corroborates\"");

        let defined = KnotPredicateRefV1::Defined([0xab; 32]);
        let encoded = serde_json::to_string(&defined).unwrap();
        assert_eq!(encoded, format!("{{\"defined\":\"{}\"}}", "ab".repeat(32)));
        assert_eq!(
            serde_json::from_str::<KnotPredicateRefV1>(&encoded).unwrap(),
            defined
        );
        assert!(serde_json::from_str::<KnotPredicateRefV1>("{\"minted\":\"ab\"}").is_err());
        assert!(
            serde_json::from_str::<KnotPredicateRefV1>(&format!(
                "{{\"defined\":\"{}\"}}",
                "AB".repeat(32)
            ))
            .is_err(),
            "the hash spelling is lowercase hex only"
        );
    }

    #[test]
    fn an_unrecognized_label_is_refused_by_payload_validation() {
        let unknown = KnotPredicateRefV1::Unrecognized("corroborates".into());
        let error = validate_relation_payload(&unknown, &endpoint(), &endpoint()).unwrap_err();
        assert!(error.contains("corroborates"), "{error}");
        assert!(
            validate_relation_payload(
                &KnotPredicateRefV1::Core(KnotCorePredicateV1::Cites),
                &endpoint(),
                &endpoint()
            )
            .is_ok()
        );
        assert!(
            validate_relation_payload(
                &KnotPredicateRefV1::Defined([0x07; 32]),
                &endpoint(),
                &endpoint()
            )
            .is_ok(),
            "a defined reference is a catalog question, not a payload one"
        );
    }

    #[test]
    fn context_is_captured_on_character_boundaries_and_truncates_at_the_edges() {
        let source = "αβγδ quoted εζηθ";
        let start = source.find("quoted").unwrap();
        let end = start + "quoted".len();
        let (prefix, suffix) = capture_endpoint_context(source, start, end);
        assert_eq!(prefix, "αβγδ ");
        assert_eq!(suffix, " εζηθ");

        let long = "λ".repeat(100);
        let middle = format!("{long}quoted{long}");
        let start = long.len();
        let (prefix, suffix) = capture_endpoint_context(&middle, start, start + "quoted".len());
        assert_eq!(prefix.chars().count(), KNOT_RELATION_CONTEXT_CHARS);
        assert_eq!(suffix.chars().count(), KNOT_RELATION_CONTEXT_CHARS);
        assert!(prefix.len() <= KNOT_RELATION_CONTEXT_BYTES);
        assert!(middle[..start].ends_with(&prefix));
        assert!(middle[start + "quoted".len()..].starts_with(&suffix));

        // A byte offset off a character boundary yields no context, never a panic.
        assert_eq!(
            capture_endpoint_context(source, 1, 3),
            (String::new(), String::new())
        );
    }

    #[test]
    fn captured_context_is_bounded_and_needs_a_quote() {
        let mut oversized = endpoint();
        oversized.prefix = "x".repeat(KNOT_RELATION_CONTEXT_BYTES + 1);
        let core = KnotPredicateRefV1::Core(KnotCorePredicateV1::Cites);
        assert!(validate_relation_payload(&core, &oversized, &endpoint()).is_err());

        let bare = KnotRelationEndpointV1 {
            quote: String::new(),
            position: None,
            prefix: "orphan".into(),
            ..endpoint()
        };
        assert!(validate_relation_payload(&core, &bare, &endpoint()).is_err());
    }
}
