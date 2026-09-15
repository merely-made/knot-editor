// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! W3C Web Annotation target serialization for Fleece text evidence.
//!
//! Knot owns the source identity. Fleece supplies only selectors over its
//! documented text stream, so this module deliberately accepts the source URI
//! from the caller and emits its quote and position descriptions as siblings.

use fleece::TextAnchor;
use serde::Serialize;

use crate::relations::KnotRelationEndpointV1;

/// A W3C Web Annotation `SpecificResource` target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpecificResource {
    #[serde(rename = "type")]
    resource_type: &'static str,
    pub source: String,
    pub selector: Vec<SpecificResourceSelector>,
}

/// The two alternative selector descriptions carried by a Fleece anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum SpecificResourceSelector {
    TextQuoteSelector {
        exact: String,
        prefix: String,
        suffix: String,
    },
    TextPositionSelector {
        start: u64,
        end: u64,
    },
}

impl SpecificResource {
    /// Serialize one Fleece anchor as sibling Web Annotation selectors.
    ///
    /// This is intentionally not an Annotation: Knot's caller still owns a
    /// body, motivation, persistence, and the source resource identity.
    pub fn from_fleece_anchor(source: impl Into<String>, anchor: &TextAnchor) -> Self {
        Self {
            resource_type: "SpecificResource",
            source: source.into(),
            selector: vec![
                SpecificResourceSelector::TextQuoteSelector {
                    exact: anchor.quote.exact.clone(),
                    prefix: anchor.quote.prefix.clone(),
                    suffix: anchor.quote.suffix.clone(),
                },
                SpecificResourceSelector::TextPositionSelector {
                    start: anchor.position.start,
                    end: anchor.position.end,
                },
            ],
        }
    }

    /// Export one relation endpoint as a Web Annotation `SpecificResource`.
    ///
    /// Knot's UTF-8 byte offsets are the internal truth; the exported position
    /// selector counts characters, as the Web Annotation model requires, against
    /// `source_text` -- the same head the offsets were captured at. Returns
    /// `None` when the endpoint has no position or its offsets are not
    /// character boundaries of that text, so a mismatched head exports nothing
    /// rather than a wrong anchor.
    pub fn from_relation_endpoint(
        source: impl Into<String>,
        endpoint: &KnotRelationEndpointV1,
        source_text: &str,
    ) -> Option<Self> {
        let position = endpoint.position?;
        let start = usize::try_from(position.start).ok()?;
        let end = usize::try_from(position.end).ok()?;
        if !source_text.is_char_boundary(start) || !source_text.is_char_boundary(end) {
            return None;
        }
        let character_start = source_text.get(..start)?.chars().count() as u64;
        let character_end = source_text.get(..end)?.chars().count() as u64;
        Some(Self {
            resource_type: "SpecificResource",
            source: source.into(),
            selector: vec![
                SpecificResourceSelector::TextQuoteSelector {
                    exact: endpoint.quote.clone(),
                    prefix: endpoint.prefix.clone(),
                    suffix: endpoint.suffix.clone(),
                },
                SpecificResourceSelector::TextPositionSelector {
                    start: character_start,
                    end: character_end,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use fleece::{TextAnchor, TextPositionSelector, TextQuoteSelector};

    use super::SpecificResource;
    use crate::relations::{
        KnotRelationEndpointV1, KnotRelationPositionV1, capture_endpoint_context,
    };

    const DOCUMENT: &str = include_str!("../tests/fixtures/fleece_specific_resource.txt");

    fn code_point_offset(document: &str, byte_offset: usize) -> u64 {
        document[..byte_offset].chars().count() as u64
    }

    fn resolve_position(document: &str, start: u64, end: u64) -> String {
        document
            .chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect()
    }

    fn resolve_quote(document: &str, quote: &TextQuoteSelector) -> Vec<(u64, u64)> {
        document
            .match_indices(&quote.exact)
            .filter(|(byte_offset, _)| {
                document[..*byte_offset].ends_with(&quote.prefix)
                    && document[*byte_offset + quote.exact.len()..].starts_with(&quote.suffix)
            })
            .map(|(byte_offset, _)| {
                let start = code_point_offset(document, byte_offset);
                (start, start + quote.exact.chars().count() as u64)
            })
            .collect()
    }

    #[test]
    fn serializes_and_independently_resolves_sibling_fleece_selectors() {
        let document = DOCUMENT.trim_end();
        let exact = "Repeat this sentence.";
        let byte_start = document.match_indices(exact).nth(1).unwrap().0;
        let start = code_point_offset(document, byte_start);
        let anchor = TextAnchor {
            position: TextPositionSelector {
                start,
                end: start + exact.chars().count() as u64,
            },
            quote: TextQuoteSelector {
                exact: exact.to_string(),
                prefix: "Middle. ".to_string(),
                suffix: " End.".to_string(),
            },
        };

        let target = SpecificResource::from_fleece_anchor("https://example.test/article", &anchor);
        let serialized = serde_json::to_value(&target).unwrap();
        assert_eq!(serialized["type"], "SpecificResource");
        assert_eq!(serialized["source"], "https://example.test/article");
        assert_eq!(serialized["selector"].as_array().unwrap().len(), 2);
        assert!(serialized.get("refinedBy").is_none());

        assert_eq!(
            resolve_position(document, anchor.position.start, anchor.position.end),
            anchor.quote.exact
        );
        assert_eq!(
            resolve_quote(document, &anchor.quote),
            vec![(anchor.position.start, anchor.position.end)]
        );
    }

    #[test]
    fn a_relation_endpoint_exports_character_offsets_and_re_anchors_its_quote() {
        // Multibyte source: byte offsets and character offsets must differ.
        let source = "αβγ Repeat this sentence. Middle. Repeat this sentence. End.";
        let exact = "Repeat this sentence.";
        let byte_start = source.match_indices(exact).nth(1).unwrap().0;
        let byte_end = byte_start + exact.len();
        let (prefix, suffix) = capture_endpoint_context(source, byte_start, byte_end);
        let endpoint = KnotRelationEndpointV1 {
            document_id: "essay".into(),
            document_head: [0x21; 32],
            quote: exact.into(),
            position: Some(KnotRelationPositionV1 {
                start: byte_start as u64,
                end: byte_end as u64,
            }),
            prefix,
            suffix,
        };

        let target =
            SpecificResource::from_relation_endpoint("urn:knot:doc:essay", &endpoint, source)
                .unwrap();
        let serialized = serde_json::to_value(&target).unwrap();
        assert_eq!(serialized["type"], "SpecificResource");
        assert_eq!(serialized["source"], "urn:knot:doc:essay");
        let position = &serialized["selector"][1];
        let start = position["start"].as_u64().unwrap();
        let end = position["end"].as_u64().unwrap();
        assert_ne!(start, byte_start as u64, "characters, not bytes");
        assert_eq!(resolve_position(source, start, end), exact);

        let quote = fleece::TextQuoteSelector {
            exact: exact.to_string(),
            prefix: endpoint.prefix.clone(),
            suffix: endpoint.suffix.clone(),
        };
        assert_eq!(
            resolve_quote(source, &quote),
            vec![(start, end)],
            "the captured context disambiguates the repeated quote"
        );

        let no_position = KnotRelationEndpointV1 {
            quote: String::new(),
            position: None,
            prefix: String::new(),
            suffix: String::new(),
            ..endpoint.clone()
        };
        assert!(
            SpecificResource::from_relation_endpoint("urn:knot:doc:essay", &no_position, source)
                .is_none()
        );
        let off_boundary = KnotRelationEndpointV1 {
            position: Some(KnotRelationPositionV1 { start: 1, end: 3 }),
            ..endpoint
        };
        assert!(
            SpecificResource::from_relation_endpoint("urn:knot:doc:essay", &off_boundary, source)
                .is_none()
        );
    }
}
