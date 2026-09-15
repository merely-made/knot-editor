// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Writer-defined relation predicates: identity minted once, label mutable.
//!
//! A predicate has three faces kept apart. The label is what the writer sees
//! and may change; the identity is the IRI an assertion stores and export
//! names; the behaviour is the Mere sub-kind it lowers to, if any.

use zeroize::Zeroize;

use crate::did_key::did_key;
use crate::relations::{KnotCorePredicateV1, KnotPredicateRefV1};
use crate::settings::hex32;

/// URN namespace for a predicate minted under a Knot signing key.
pub const KNOT_PREDICATE_IRI_PREFIX: &str = "urn:knot:rel:";

/// Mere sub-kind slugs Knot recognizes as lowering targets.
///
/// A local mirror of the graph kernel's recognized vocabulary: knot-editor
/// depends on neither graph-kernel nor linked-data.
const MERE_SUB_KINDS: [&str; 17] = [
    "hyperlink",
    "user-grouped",
    "agent-derived",
    "cites",
    "quotes",
    "summarizes",
    "elaborates",
    "example-of",
    "supports",
    "contradicts",
    "questions",
    "same-entity-as",
    "duplicate-of",
    "canonical-mirror-of",
    "depends-on",
    "blocks",
    "next-step",
];

/// What a superseding definition says about the predicate it replaces.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
#[serde(rename_all = "kebab-case")]
pub enum KnotPredicateReplacementV1 {
    /// No successor: the predicate refuses new assertions and keeps old ones.
    Retired,
    /// A successor a writer should assert with instead.
    Predicate(KnotPredicateRefV1),
}

/// One signed definition of a writer's own predicate.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotPredicateDefinitionV1 {
    /// This definition's operation hash, not caller-provided identity.
    pub id: [u8; 32],
    pub author: [u8; 32],
    pub operation: [u8; 32],
    pub scope: [u8; 32],
    /// The first definition in the chain: the minted identity.
    pub root: [u8; 32],
    /// Minted once at the root and carried unchanged through every rename.
    pub iri: String,
    pub slug: String,
    pub label: String,
    pub description: Option<String>,
    /// A core Mere IRI or any standard IRI; exports as `rdfs:subPropertyOf`.
    pub subproperty_of: Option<String>,
    /// A Mere sub-kind slug this predicate lowers to.
    pub lowers_to: Option<String>,
    pub supersedes: Option<[u8; 32]>,
    pub replaced_by: Option<KnotPredicateReplacementV1>,
    /// Author-asserted wall clock from the signed header. Informative only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_at_ms: Option<u64>,
}

/// A definition retained in history but refused from the active catalog.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Zeroize)]
pub struct KnotRejectedPredicateV1 {
    pub operation: [u8; 32],
    pub author: [u8; 32],
    pub reason: String,
}

/// Every predicate definition folded from one space, in causal order.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KnotPredicateCatalogV1 {
    pub definitions: Vec<KnotPredicateDefinitionV1>,
    pub rejected: Vec<KnotRejectedPredicateV1>,
}

impl KnotPredicateCatalogV1 {
    pub fn definition(&self, id: [u8; 32]) -> Option<&KnotPredicateDefinitionV1> {
        self.definitions
            .iter()
            .find(|definition| definition.id == id)
    }

    pub fn root_of(&self, id: [u8; 32]) -> Option<[u8; 32]> {
        self.definition(id).map(|definition| definition.root)
    }

    /// The latest definition in `id`'s chain: where the current label lives.
    pub fn current(&self, id: [u8; 32]) -> Option<&KnotPredicateDefinitionV1> {
        let root = self.root_of(id)?;
        self.definitions
            .iter()
            .rfind(|definition| definition.root == root)
    }

    /// Whether any definition in `id`'s chain declares a replacement.
    pub fn is_retired(&self, id: [u8; 32]) -> bool {
        self.root_of(id).is_some_and(|root| {
            self.definitions
                .iter()
                .any(|definition| definition.root == root && definition.replaced_by.is_some())
        })
    }

    /// The label to display for an assertion, which may have been signed under
    /// an older one.
    pub fn label_of(&self, predicate: &KnotPredicateRefV1) -> Option<&str> {
        match predicate {
            KnotPredicateRefV1::Core(core) => Some(core.default_label()),
            KnotPredicateRefV1::Defined(id) => self
                .current(*id)
                .map(|definition| definition.label.as_str()),
            KnotPredicateRefV1::Unrecognized(_) => None,
        }
    }

    pub fn iri_of(&self, predicate: &KnotPredicateRefV1) -> Option<&str> {
        match predicate {
            KnotPredicateRefV1::Core(core) => Some(core.iri()),
            KnotPredicateRefV1::Defined(id) => {
                self.current(*id).map(|definition| definition.iri.as_str())
            },
            KnotPredicateRefV1::Unrecognized(_) => None,
        }
    }

    pub fn lowers_to(&self, predicate: &KnotPredicateRefV1) -> Option<&str> {
        match predicate {
            KnotPredicateRefV1::Core(core) => Some(core.sub_kind()),
            KnotPredicateRefV1::Defined(id) => self
                .current(*id)
                .and_then(|definition| definition.lowers_to.as_deref()),
            KnotPredicateRefV1::Unrecognized(_) => None,
        }
    }
}

/// Mint the permanent IRI for a predicate rooted at `root` under `author`.
///
/// The namespace is the signing key, because that is what signs the definition
/// and what a peer verifies. It is minted once and never changes; a rename is
/// a superseding definition carrying this same IRI.
pub fn mint_predicate_iri(author: &[u8; 32], root: [u8; 32]) -> String {
    let multibase = did_key(author)
        .strip_prefix("did:key:")
        .unwrap_or_default()
        .to_owned();
    format!("{KNOT_PREDICATE_IRI_PREFIX}{multibase}:{}", hex32(&root))
}

/// Whether `value` is an absolute IRI by scheme.
///
/// A scheme-and-colon check plus a NUL and whitespace refusal: `url` rejects a
/// bare `urn:` form, which is exactly the shape Knot mints.
fn is_absolute_iri(value: &str) -> bool {
    if value.contains('\0') || value.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    !rest.is_empty()
        && scheme.starts_with(|character: char| character.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "+-.".contains(character))
}

pub(crate) fn validate_predicate_definition_payload(
    slug: &str,
    label: &str,
    description: Option<&str>,
    subproperty_of: Option<&str>,
    lowers_to: Option<&str>,
) -> Result<(), String> {
    if slug.is_empty()
        || slug.trim() != slug
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!(
            "predicate slug '{slug}' must be a non-empty trimmed [a-z0-9-] identifier"
        ));
    }
    if KnotCorePredicateV1::from_slug(slug).is_some() {
        return Err(format!("predicate slug '{slug}' is a core predicate"));
    }
    if label.is_empty() || label.trim() != label || label.contains('\0') {
        return Err("predicate label must be non-empty and trimmed".to_owned());
    }
    if let Some(description) = description
        && (description.is_empty()
            || description.trim() != description
            || description.contains('\0'))
    {
        return Err("predicate description must be non-empty and trimmed".to_owned());
    }
    if let Some(subproperty_of) = subproperty_of
        && !is_absolute_iri(subproperty_of)
    {
        return Err(format!(
            "predicate subproperty_of '{subproperty_of}' is not an absolute IRI"
        ));
    }
    if let Some(lowers_to) = lowers_to
        && !MERE_SUB_KINDS.contains(&lowers_to)
    {
        return Err(format!(
            "predicate lowers_to '{lowers_to}' is not a Mere sub-kind Knot knows"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(id: u8, root: u8, supersedes: Option<[u8; 32]>) -> KnotPredicateDefinitionV1 {
        KnotPredicateDefinitionV1 {
            id: [id; 32],
            author: [0x01; 32],
            operation: [id; 32],
            scope: [0x02; 32],
            root: [root; 32],
            iri: mint_predicate_iri(&[0x01; 32], [root; 32]),
            slug: "corroborates".into(),
            label: if supersedes.is_some() {
                "corroborates strongly".into()
            } else {
                "corroborates".into()
            },
            description: None,
            subproperty_of: Some("https://sparontologies.net/ontologies/cito/supports".into()),
            lowers_to: Some("supports".into()),
            supersedes,
            replaced_by: None,
            asserted_at_ms: None,
        }
    }

    #[test]
    fn a_chain_keeps_one_iri_and_shows_its_current_label() {
        let mut catalog = KnotPredicateCatalogV1::default();
        catalog.definitions.push(definition(0x10, 0x10, None));
        catalog
            .definitions
            .push(definition(0x11, 0x10, Some([0x10; 32])));
        let old = KnotPredicateRefV1::Defined([0x10; 32]);
        assert_eq!(catalog.root_of([0x11; 32]), Some([0x10; 32]));
        assert_eq!(catalog.label_of(&old), Some("corroborates strongly"));
        assert_eq!(
            catalog.iri_of(&old),
            Some(mint_predicate_iri(&[0x01; 32], [0x10; 32]).as_str())
        );
        assert_eq!(catalog.lowers_to(&old), Some("supports"));
        assert!(!catalog.is_retired([0x10; 32]));

        let mut retirement = definition(0x12, 0x10, Some([0x11; 32]));
        retirement.replaced_by = Some(KnotPredicateReplacementV1::Retired);
        catalog.definitions.push(retirement);
        assert!(catalog.is_retired([0x10; 32]));
        assert_eq!(
            catalog.label_of(&KnotPredicateRefV1::Defined([0x99; 32])),
            None
        );
    }

    #[test]
    fn a_minted_iri_names_its_signing_key_and_its_root() {
        let iri = mint_predicate_iri(&[0x01; 32], [0x10; 32]);
        assert!(iri.starts_with("urn:knot:rel:z6Mk"), "{iri}");
        assert!(iri.ends_with(&hex32(&[0x10; 32])));
        assert!(is_absolute_iri(&iri));
        assert_ne!(iri, mint_predicate_iri(&[0x02; 32], [0x10; 32]));
    }

    #[test]
    fn definition_payload_validation_refuses_the_shapes_it_names() {
        assert!(
            validate_predicate_definition_payload(
                "corroborates",
                "corroborates",
                None,
                Some("urn:knot:rel:z6Mk:abc"),
                Some("supports")
            )
            .is_ok()
        );
        for (slug, label) in [("", "x"), (" x", "x"), ("Corroborates", "x"), ("x", "")] {
            assert!(
                validate_predicate_definition_payload(slug, label, None, None, None).is_err(),
                "{slug}/{label}"
            );
        }
        assert!(
            validate_predicate_definition_payload("supports", "supports", None, None, None)
                .is_err(),
            "a core slug cannot be redefined"
        );
        assert!(
            validate_predicate_definition_payload("x", "x", None, Some("not an iri"), None)
                .is_err()
        );
        assert!(
            validate_predicate_definition_payload("x", "x", None, None, Some("invented")).is_err()
        );
    }
}
