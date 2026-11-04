// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::*;
use std::collections::{BTreeMap, BTreeSet};

/// Conservative defaults; callers may lower or deliberately raise limits.
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_pack_bytes: usize,
    pub max_installed_bytes: usize,
    pub max_registry_bytes: usize,
    pub max_sources: usize,
    pub max_entries: usize,
    pub max_senses: usize,
    pub max_relations: usize,
    pub max_string_bytes: usize,
    pub max_lemma_bytes: usize,
    pub max_query_sources: usize,
    pub max_lookup_results: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pack_bytes: 32 * 1024 * 1024,
            max_installed_bytes: 128 * 1024 * 1024,
            max_registry_bytes: 1024 * 1024,
            max_sources: 128,
            max_entries: 100_000,
            max_senses: 200_000,
            max_relations: 500_000,
            max_string_bytes: 16 * 1024,
            max_lemma_bytes: 256,
            max_query_sources: 16,
            max_lookup_results: 256,
        }
    }
}

/// Struct-order canonical JSON, preserving entry and list order. No whitespace,
/// sorting, Unicode normalization, or floating point is involved in this schema.
pub fn digest_entries(entries: &[LexicalEntry]) -> Result<String> {
    Ok(blake3::hash(&serde_json::to_vec(entries)?)
        .to_hex()
        .to_string())
}

#[derive(Debug)]
pub struct ValidatedPack {
    pack: NormalizedPack,
    by_id: BTreeMap<String, usize>,
    by_lemma: BTreeMap<String, Vec<usize>>,
}
impl ValidatedPack {
    pub fn from_json(bytes: &[u8], expected_digest: Option<&str>, limits: &Limits) -> Result<Self> {
        if bytes.len() > limits.max_pack_bytes {
            return Err(ReferenceError::Limit("pack bytes"));
        }
        std::str::from_utf8(bytes)
            .map_err(|_| ReferenceError::Invalid("pack is not UTF-8".into()))?;
        let pack: NormalizedPack = serde_json::from_slice(bytes)?;
        if pack.schema_version != SCHEMA_VERSION {
            return Err(ReferenceError::Invalid("unsupported pack schema".into()));
        }
        validate_manifest(&pack.manifest, limits)?;
        let digest =
            pack.manifest.digest.as_deref().ok_or_else(|| {
                ReferenceError::Invalid("installed pack requires a digest".into())
            })?;
        if let Some(expected) = expected_digest {
            validate_digest(expected)?;
            if expected != digest {
                return Err(ReferenceError::DigestMismatch);
            }
        }
        if pack.entries.len() > limits.max_entries {
            return Err(ReferenceError::Limit("entries"));
        }
        let mut by_id = BTreeMap::new();
        let mut by_lemma: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut senses = 0usize;
        let mut relations = 0usize;
        for (index, entry) in pack.entries.iter().enumerate() {
            validate_reference(&entry.id)?;
            if entry.id.source_key() != pack.manifest.source_key() {
                return Err(ReferenceError::Invalid(
                    "entry source/version differs from manifest".into(),
                ));
            }
            if by_id.insert(entry.id.entry.clone(), index).is_some() {
                return Err(ReferenceError::Invalid("duplicate entry id".into()));
            }
            text(&entry.lemma, limits.max_lemma_bytes, false)?;
            language(&entry.language)?;
            if pack.manifest.language != "mul" && pack.manifest.language != entry.language {
                return Err(ReferenceError::Invalid(
                    "entry language differs from source".into(),
                ));
            }
            by_lemma.entry(entry.lemma.clone()).or_default().push(index);
            senses = senses
                .checked_add(entry.senses.len())
                .ok_or(ReferenceError::Limit("senses"))?;
            relations = relations
                .checked_add(entry.relations.len())
                .ok_or(ReferenceError::Limit("relations"))?;
            let mut sense_ids = BTreeSet::new();
            for sense in &entry.senses {
                identifier(&sense.id)?;
                if let Some(concept) = &sense.concept_id {
                    identifier(concept)?;
                }
                if !sense_ids.insert(&sense.id) {
                    return Err(ReferenceError::Invalid(
                        "duplicate sense id in entry".into(),
                    ));
                }
                text(&sense.definition, limits.max_string_bytes, false)?;
                require_feature(&pack.manifest, ReferenceFeature::Definitions)?;
                for example in &sense.examples {
                    text(example, limits.max_string_bytes, false)?;
                    require_feature(&pack.manifest, ReferenceFeature::Examples)?;
                }
                relations = relations
                    .checked_add(sense.relations.len())
                    .ok_or(ReferenceError::Limit("relations"))?;
            }
            for pronunciation in &entry.pronunciations {
                text(&pronunciation.notation, 128, false)?;
                text(&pronunciation.value, limits.max_string_bytes, false)?;
                if let Some(variant) = &pronunciation.variant {
                    text(variant, limits.max_string_bytes, false)?;
                }
                require_feature(&pack.manifest, ReferenceFeature::Pronunciations)?;
            }
            if senses > limits.max_senses {
                return Err(ReferenceError::Limit("senses"));
            }
            if relations > limits.max_relations {
                return Err(ReferenceError::Limit("relations"));
            }
        }
        for entry in &pack.entries {
            for relation in entry
                .relations
                .iter()
                .chain(entry.senses.iter().flat_map(|sense| &sense.relations))
            {
                require_feature(&pack.manifest, ReferenceFeature::Relations)?;
                validate_reference(&relation.target)?;
                // V1 deliberately forbids dangling/external relations. A future
                // format can model unresolved references explicitly instead.
                if relation.target.source_key() != pack.manifest.source_key()
                    || !by_id.contains_key(&relation.target.entry)
                {
                    return Err(ReferenceError::Invalid(
                        "relation target is outside this pack or missing".into(),
                    ));
                }
                if let Some(sense) = &relation.target_sense {
                    identifier(sense)?;
                    let target = &pack.entries[by_id[&relation.target.entry]];
                    if !target.senses.iter().any(|candidate| &candidate.id == sense) {
                        return Err(ReferenceError::Invalid(
                            "relation target sense is missing from target entry".into(),
                        ));
                    }
                }
            }
            for list in
                std::iter::once(&entry.relations).chain(entry.senses.iter().map(|s| &s.relations))
            {
                let mut unique = BTreeSet::new();
                for relation in list {
                    if !unique.insert(serde_json::to_string(relation)?) {
                        return Err(ReferenceError::Invalid("duplicate relation".into()));
                    }
                }
            }
        }
        if digest_entries(&pack.entries)? != digest {
            return Err(ReferenceError::DigestMismatch);
        }
        Ok(Self {
            pack,
            by_id,
            by_lemma,
        })
    }
    pub fn manifest(&self) -> &SourceManifest {
        &self.pack.manifest
    }
    pub fn entries(&self) -> &[LexicalEntry] {
        &self.pack.entries
    }
    pub fn entry(&self, id: &ReferenceId) -> Option<&LexicalEntry> {
        if id.source_key() != self.pack.manifest.source_key() {
            return None;
        }
        self.by_id
            .get(&id.entry)
            .map(|index| &self.pack.entries[*index])
    }
    pub(crate) fn lookup(
        &self,
        lemma: &str,
        lang: Option<&str>,
        limit: usize,
    ) -> Vec<LexicalEntry> {
        let mut result = Vec::new();
        if let Some(indexes) = self.by_lemma.get(lemma) {
            for index in indexes {
                if lang.is_some_and(|value| value != self.pack.entries[*index].language) {
                    continue;
                }
                if result.len() == limit {
                    return result;
                }
                result.push(self.pack.entries[*index].clone());
            }
        }
        result
    }
}

pub(crate) fn identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().any(|b| b.is_ascii_alphanumeric())
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
    {
        return Err(ReferenceError::Invalid(
            "identifier must be 1..128 ASCII letters, digits, '_', '-' or '.', with an alphanumeric character".into(),
        ));
    }
    Ok(())
}
pub(crate) fn validate_reference(id: &ReferenceId) -> Result<()> {
    identifier(&id.source)?;
    version_identifier(&id.version)?;
    identifier(&id.entry)
}
pub(crate) fn version_identifier(value: &str) -> Result<()> {
    if value.len() > 128 {
        return Err(ReferenceError::Invalid(
            "version identifier is too long".into(),
        ));
    }
    for segment in value.split('.') {
        identifier(segment)?;
    }
    Ok(())
}
pub(crate) fn language(value: &str) -> Result<()> {
    if value.len() > 64
        || value
            .split('-')
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        return Err(ReferenceError::Invalid("invalid language tag".into()));
    }
    Ok(())
}
pub(crate) fn text(value: &str, max: usize, allow_empty: bool) -> Result<()> {
    if value.len() > max {
        return Err(ReferenceError::Limit("text bytes"));
    }
    if (!allow_empty && value.trim().is_empty())
        || value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ReferenceError::Invalid(
            "empty text or control character".into(),
        ));
    }
    Ok(())
}
pub(crate) fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ReferenceError::Invalid(
            "digest must be lowercase BLAKE3 hex".into(),
        ));
    }
    Ok(())
}
fn url(value: &str, limits: &Limits) -> Result<()> {
    text(value, limits.max_string_bytes, false)?;
    if !(value.starts_with("https://") || value.starts_with("http://"))
        || value.chars().any(char::is_whitespace)
    {
        return Err(ReferenceError::Invalid(
            "metadata URL must be HTTP(S) without whitespace".into(),
        ));
    }
    Ok(())
}
pub(crate) fn validate_manifest(manifest: &SourceManifest, limits: &Limits) -> Result<()> {
    identifier(&manifest.id)?;
    version_identifier(&manifest.version)?;
    language(&manifest.language)?;
    text(&manifest.label, limits.max_string_bytes, false)?;
    text(&manifest.license.name, limits.max_string_bytes, false)?;
    text(&manifest.attribution, limits.max_string_bytes, false)?;
    if let Some(value) = &manifest.license.text {
        text(value, limits.max_string_bytes, false)?;
    }
    if let Some(value) = &manifest.license.url {
        url(value, limits)?;
    }
    if let Some(value) = &manifest.upstream {
        url(value, limits)?;
    }
    if let Some(value) = &manifest.digest {
        validate_digest(value)?;
    }
    let mut features = BTreeSet::new();
    for feature in &manifest.features {
        if !features.insert(serde_json::to_string(feature)?) {
            return Err(ReferenceError::Invalid("duplicate feature".into()));
        }
    }
    Ok(())
}
fn require_feature(manifest: &SourceManifest, feature: ReferenceFeature) -> Result<()> {
    if !manifest.features.contains(&feature) {
        return Err(ReferenceError::Invalid(
            "entry data is not declared by source features".into(),
        ));
    }
    Ok(())
}
