// SPDX-License-Identifier: MPL-2.0
//! Explicit offline WN-LMF subset conversion. No DTD/entity fetching occurs.
//!
//! This is deliberately a lexical subset, not an archival WN-LMF round trip.
//! Exact selected lemmas and one-hop lexical neighbors are included. Unsupported
//! relation labels are counted, never relabeled as synonyms or generic related
//! words. Source sense IDs and the complete imported sense-to-synset map remain
//! available in the conversion report.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Write};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use reference_data::{
    LexicalEntry, LexicalRelation, LexicalSense, LicenseInfo, Limits, NormalizedPack, PartOfSpeech,
    Pronunciation, ReferenceFeature, ReferenceId, RelationKind, SCHEMA_VERSION, SourceManifest,
    ValidatedPack, digest_entries,
};
use serde::Serialize;

pub const MAX_XML_BYTES: usize = 256 * 1024 * 1024;
/// Conservative retained-record/string/index accounting, not an RSS guarantee.
pub const MAX_ESTIMATED_BYTES: usize = 512 * 1024 * 1024;
const MAX_DIAGNOSTIC_KEYS: usize = 1024;
const MAX_XML_ENTRIES: usize = 300_000;
const MAX_XML_SYNSETS: usize = 200_000;
const MAX_XML_SENSES: usize = 500_000;
const MAX_XML_RELATIONS: usize = 2_000_000;
const MAX_OUTPUT_ENTRIES: usize = 4096;
const MAX_STRING_BYTES: usize = 16 * 1024;
const OEWN_NOTICES: &str = include_str!("../assets/oewn-notices.txt");

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub source: String,
    pub version: String,
    /// Exact, case-sensitive lemmas; no normalization or wildcard expansion.
    pub lemmas: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub source: String,
    pub version: String,
    pub requested_lemmas: Vec<String>,
    pub input_entries: usize,
    pub output_entries: usize,
    pub excluded_entries: usize,
    /// Unsupported kinds count original input edges. Boundary exclusions count
    /// lexical projections omitted while constructing imported senses.
    pub excluded_relations: BTreeMap<String, usize>,
    pub ignored_elements: BTreeMap<String, usize>,
    pub ignored_attributes: BTreeMap<String, usize>,
    /// Exact original identities for every sense retained in the output pack.
    pub sense_synsets: BTreeMap<String, String>,
}

#[derive(Debug)]
pub struct ImportedWordnet {
    pub pack: NormalizedPack,
    pub report: ImportReport,
}

#[derive(Debug)]
pub struct ImportError(pub String);

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ImportError {}

fn invalid(message: impl Into<String>) -> ImportError {
    ImportError(message.into())
}

#[derive(Default)]
struct RawEntry {
    id: String,
    lemma: String,
    pos: String,
    senses: Vec<RawSense>,
    pronunciations: Vec<Pronunciation>,
}
#[derive(Default)]
struct RawSense {
    id: String,
    synset: String,
    relations: Vec<RawRelation>,
    examples: Vec<String>,
}
#[derive(Default)]
struct RawSynset {
    id: String,
    definitions: Vec<String>,
    examples: Vec<String>,
    relations: Vec<RawRelation>,
}
struct RawRelation {
    kind: String,
    target: String,
}
#[derive(Default)]
struct RawWordnet {
    metadata: BTreeMap<String, String>,
    entries: Vec<RawEntry>,
    synsets: BTreeMap<String, RawSynset>,
    ignored_elements: BTreeMap<String, usize>,
    ignored_attributes: BTreeMap<String, usize>,
    retained_bytes: usize,
}

struct MemoryBudget {
    used: usize,
    limit: usize,
}

impl MemoryBudget {
    fn charge(&mut self, bytes: usize) -> Result<(), ImportError> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| invalid("retained-memory budget exceeded"))?;
        if self.used > self.limit {
            return Err(invalid("estimated retained-memory budget exceeded"));
        }
        Ok(())
    }
}

/// Convert locally supplied UTF-8 WN-LMF bytes. The XML Lexicon identity must
/// match the caller's explicit source/version; multiple lexicons/extensions are
/// refused rather than flattened together.
pub fn import_wordnet(xml: &[u8], options: &ImportOptions) -> Result<ImportedWordnet, ImportError> {
    import_wordnet_with_budget(xml, options, MAX_ESTIMATED_BYTES)
}

/// As [`import_wordnet`], with a deliberately lower estimated-memory budget.
/// Inputs, records, retained strings and output copies are accounted before
/// growth. The ceiling cannot be raised beyond the bounded offline-tool default.
pub fn import_wordnet_with_budget(
    xml: &[u8],
    options: &ImportOptions,
    max_estimated_bytes: usize,
) -> Result<ImportedWordnet, ImportError> {
    if max_estimated_bytes > MAX_ESTIMATED_BYTES {
        return Err(invalid("memory budget may only lower the 512 MiB ceiling"));
    }
    if xml.len() > MAX_XML_BYTES {
        return Err(invalid("WN-LMF input exceeds 256 MiB"));
    }
    std::str::from_utf8(xml).map_err(|_| invalid("WN-LMF input must be UTF-8"))?;
    if options.source.is_empty()
        || options.source.len() > 128
        || options.version.is_empty()
        || options.version.len() > 128
    {
        return Err(invalid("explicit source and version must be 1..128 bytes"));
    }
    if options.lemmas.is_empty() || options.lemmas.len() > 64 {
        return Err(invalid("explicitly select between 1 and 64 lemmas"));
    }
    for lemma in &options.lemmas {
        if lemma.trim().is_empty() || lemma.len() > 256 {
            return Err(invalid(
                "selected lemma is empty or exceeds 256 UTF-8 bytes",
            ));
        }
    }
    let raw = parse(xml, max_estimated_bytes)?;
    let mut budget = MemoryBudget {
        used: raw.retained_bytes,
        limit: max_estimated_bytes,
    };
    if required(&raw.metadata, "id")? != options.source
        || required(&raw.metadata, "version")? != options.version
    {
        return Err(invalid(
            "explicit source/version differs from the XML Lexicon identity",
        ));
    }
    let language = required(&raw.metadata, "language")?.to_owned();
    let license_url = required(&raw.metadata, "license")?;
    let license = if options.source == "oewn" {
        if !matches!(
            license_url.trim_end_matches('/'),
            "https://creativecommons.org/licenses/by/4.0"
                | "http://creativecommons.org/licenses/by/4.0"
        ) {
            return Err(invalid(
                "OEWN declares an unreviewed license; refusing to apply the bundled paired notices",
            ));
        }
        LicenseInfo {
            name: "Princeton WordNet License and CC BY 4.0 (Open English WordNet)".into(),
            text: Some(OEWN_NOTICES.into()),
            url: Some(license_url.into()),
        }
    } else {
        LicenseInfo {
            name: "License declared by source WN-LMF Lexicon".into(),
            text: None,
            url: Some(license_url.into()),
        }
    };
    let mut report = ImportReport {
        source: options.source.clone(),
        version: options.version.clone(),
        requested_lemmas: options.lemmas.clone(),
        input_entries: raw.entries.len(),
        output_entries: 0,
        excluded_entries: 0,
        excluded_relations: BTreeMap::new(),
        ignored_elements: raw.ignored_elements,
        ignored_attributes: raw.ignored_attributes,
        sense_synsets: BTreeMap::new(),
    };
    let mut sense_owner = BTreeMap::new();
    let mut members: BTreeMap<&str, Vec<(usize, &RawSense)>> = BTreeMap::new();
    let mut entry_ids = BTreeSet::new();
    let mut selected = BTreeSet::new();
    let mut found = BTreeSet::new();
    for (index, entry) in raw.entries.iter().enumerate() {
        if !entry_ids.insert(&entry.id) {
            return Err(invalid(format!("duplicate lexical entry ID {}", entry.id)));
        }
        if options.lemmas.contains(&entry.lemma) {
            selected.insert(index);
            found.insert(&entry.lemma);
        }
        for sense in &entry.senses {
            for relation in &sense.relations {
                if relation_kind(&relation.kind).is_none() {
                    count(
                        &mut report.excluded_relations,
                        &format!("unsupported:{}", relation.kind),
                    )?;
                }
            }
            if sense_owner
                .insert(sense.id.as_str(), (index, sense))
                .is_some()
            {
                return Err(invalid(format!("duplicate sense ID {}", sense.id)));
            }
            if !raw.synsets.contains_key(&sense.synset) {
                return Err(invalid(format!(
                    "sense {} references missing synset {}",
                    sense.id, sense.synset
                )));
            }
            members
                .entry(&sense.synset)
                .or_default()
                .push((index, sense));
        }
    }
    for synset in raw.synsets.values() {
        for relation in &synset.relations {
            if relation_kind(&relation.kind).is_none() {
                count(
                    &mut report.excluded_relations,
                    &format!("unsupported:{}", relation.kind),
                )?;
            }
        }
    }
    for lemma in &options.lemmas {
        if !found.contains(lemma) {
            return Err(invalid(format!(
                "selected lemma not found (exact case): {lemma}"
            )));
        }
    }
    // Include only one hop from the explicitly selected entries. Neighbors do
    // not recursively expand the entire WordNet graph.
    for index in selected.clone() {
        for sense in &raw.entries[index].senses {
            for (neighbor, _) in &members[sense.synset.as_str()] {
                selected.insert(*neighbor);
            }
            for relation in &sense.relations {
                if relation_kind(&relation.kind).is_some() {
                    let (neighbor, _) =
                        sense_owner.get(relation.target.as_str()).ok_or_else(|| {
                            invalid(format!("missing sense relation target {}", relation.target))
                        })?;
                    selected.insert(*neighbor);
                }
            }
            for relation in &raw.synsets[&sense.synset].relations {
                if relation_kind(&relation.kind).is_some() {
                    if !raw.synsets.contains_key(&relation.target) {
                        return Err(invalid(format!(
                            "missing synset relation target {}",
                            relation.target
                        )));
                    }
                    if let Some(neighbors) = members.get(relation.target.as_str()) {
                        selected.extend(neighbors.iter().map(|(index, _)| *index));
                    }
                }
            }
        }
        if selected.len() > MAX_OUTPUT_ENTRIES {
            return Err(invalid(
                "selected lemmas and one-hop neighbors exceed 4096 entries",
            ));
        }
    }
    let reference = |index: usize| ReferenceId {
        source: options.source.clone(),
        version: options.version.clone(),
        entry: raw.entries[index].id.clone(),
    };
    let mut entries = Vec::new();
    let mut output_relations = 0usize;
    let mut output_senses = 0usize;
    for index in &selected {
        let entry = &raw.entries[*index];
        budget.charge(512 + entry.id.len() + entry.lemma.len())?;
        let mut senses = Vec::new();
        for sense in &entry.senses {
            budget.charge(384 + sense.id.len() + sense.synset.len())?;
            output_senses += 1;
            if output_senses > Limits::default().max_senses {
                return Err(invalid(
                    "output sense count exceeds the reference pack limit",
                ));
            }
            let synset = &raw.synsets[&sense.synset];
            if synset.definitions.len() != 1 {
                return Err(invalid(format!(
                    "imported synset {} needs exactly one definition; refusing ambiguous flattening",
                    synset.id
                )));
            }
            let mut relations = Vec::new();
            let mut unique = BTreeSet::new();
            let mut add = |kind: RelationKind, target_index: usize, target_sense: &str| {
                let relation = LexicalRelation {
                    kind,
                    target: reference(target_index),
                    target_sense: Some(target_sense.to_owned()),
                };
                let key = serde_json::to_string(&relation).expect("relation strings serialize");
                if unique.insert(key) {
                    budget.charge(512 + raw.entries[target_index].id.len() + target_sense.len())?;
                    output_relations += 1;
                    if output_relations > Limits::default().max_relations {
                        return Err(invalid(
                            "output relation count exceeds the reference pack limit",
                        ));
                    }
                    relations.push(relation);
                }
                Ok(())
            };
            for (neighbor, target_sense) in &members[sense.synset.as_str()] {
                if target_sense.id != sense.id && selected.contains(neighbor) {
                    add(RelationKind::Synonym, *neighbor, &target_sense.id)?;
                }
            }
            for relation in &sense.relations {
                let Some(kind) = relation_kind(&relation.kind) else {
                    continue;
                };
                let (neighbor, target_sense) =
                    sense_owner.get(relation.target.as_str()).ok_or_else(|| {
                        invalid(format!("missing sense relation target {}", relation.target))
                    })?;
                if selected.contains(neighbor) {
                    add(kind, *neighbor, &target_sense.id)?;
                } else {
                    count(&mut report.excluded_relations, "outside_one_hop_subset")?;
                }
            }
            for relation in &synset.relations {
                let Some(kind) = relation_kind(&relation.kind) else {
                    continue;
                };
                if !raw.synsets.contains_key(&relation.target) {
                    return Err(invalid(format!(
                        "missing synset relation target {}",
                        relation.target
                    )));
                }
                if let Some(targets) = members.get(relation.target.as_str()) {
                    let mut included = false;
                    for (neighbor, target_sense) in targets {
                        if selected.contains(neighbor) {
                            add(kind.clone(), *neighbor, &target_sense.id)?;
                            included = true;
                        }
                    }
                    if !included {
                        count(&mut report.excluded_relations, "outside_one_hop_subset")?;
                    }
                } else {
                    count(
                        &mut report.excluded_relations,
                        "synset_without_lexical_members",
                    )?;
                }
            }
            report
                .sense_synsets
                .insert(sense.id.clone(), sense.synset.clone());
            budget.charge(
                synset.definitions[0].len()
                    + sense
                        .examples
                        .iter()
                        .chain(&synset.examples)
                        .map(String::len)
                        .sum::<usize>(),
            )?;
            let mut examples = sense.examples.clone();
            examples.extend(synset.examples.clone());
            senses.push(LexicalSense {
                id: sense.id.clone(),
                concept_id: Some(sense.synset.clone()),
                definition: synset.definitions[0].clone(),
                examples,
                relations,
            });
        }
        entries.push(LexicalEntry {
            id: reference(*index),
            lemma: entry.lemma.clone(),
            language: language.clone(),
            part_of_speech: Some(part_of_speech(&entry.pos)?),
            senses,
            pronunciations: entry.pronunciations.clone(),
            relations: Vec::new(),
        });
    }
    report.output_entries = entries.len();
    report.excluded_entries = report.input_entries - report.output_entries;
    let pack = NormalizedPack {
        schema_version: SCHEMA_VERSION,
        manifest: SourceManifest {
            id: options.source.clone(),
            version: options.version.clone(),
            label: format!(
                "{} (explicit lemma subset)",
                required(&raw.metadata, "label")?
            ),
            language,
            features: vec![
                ReferenceFeature::Definitions,
                ReferenceFeature::Examples,
                ReferenceFeature::Relations,
                ReferenceFeature::Pronunciations,
            ],
            license,
            attribution: if options.source == "oewn" {
                format!(
                    "Princeton University WordNet; The Open English WordNet Team, edition {}. Explicit lemma subset converted by Knot; see license notices and exclusion report.{}",
                    options.version,
                    raw.metadata
                        .get("citation")
                        .map(|value| format!(" Source citation: {value}"))
                        .unwrap_or_default()
                )
            } else {
                raw.metadata
                    .get("citation")
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("{} {}", raw.metadata["label"], options.version))
            },
            upstream: raw.metadata.get("url").cloned(),
            digest: Some(digest_entries(&entries).map_err(|error| invalid(error.to_string()))?),
        },
        entries,
    };
    let mut json = BoundedJson(Vec::new());
    serde_json::to_writer(&mut json, &pack).map_err(|error| invalid(error.to_string()))?;
    // Account for the serialized payload and independent validation copy.
    budget.charge(json.0.len().saturating_mul(2))?;
    ValidatedPack::from_json(&json.0, None, &Limits::default())
        .map_err(|error| invalid(error.to_string()))?;
    Ok(ImportedWordnet { pack, report })
}

struct BoundedJson(Vec<u8>);

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > Limits::default().max_pack_bytes {
            return Err(io::Error::other(
                "output exceeds the 32 MiB reference pack limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn relation_kind(value: &str) -> Option<RelationKind> {
    match value {
        "synonym" => Some(RelationKind::Synonym),
        "antonym" => Some(RelationKind::Antonym),
        "hypernym" => Some(RelationKind::Hypernym),
        "hyponym" => Some(RelationKind::Hyponym),
        "meronym" => Some(RelationKind::Meronym),
        "holonym" => Some(RelationKind::Holonym),
        "derivation" => Some(RelationKind::Derivation),
        // Do not collapse instance/member/substance/domain/similar/also etc.
        // into a broader relation whose exact original meaning would be lost.
        _ => None,
    }
}

fn part_of_speech(value: &str) -> Result<PartOfSpeech, ImportError> {
    match value {
        "n" => Ok(PartOfSpeech::Noun),
        "v" => Ok(PartOfSpeech::Verb),
        "a" | "s" => Ok(PartOfSpeech::Adjective),
        "r" => Ok(PartOfSpeech::Adverb),
        "t" | "c" | "p" | "x" | "u" | "z" => Ok(PartOfSpeech::Other),
        _ => Err(invalid(format!("unsupported part of speech: {value}"))),
    }
}

fn required<'a>(
    attributes: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, ImportError> {
    attributes
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .map(String::as_str)
        .ok_or_else(|| invalid(format!("missing required WN-LMF attribute {name}")))
}

fn count(counts: &mut BTreeMap<String, usize>, key: &str) -> Result<(), ImportError> {
    if !counts.contains_key(key) && counts.len() >= MAX_DIAGNOSTIC_KEYS {
        return Err(invalid("distinct diagnostic names exceed 1024"));
    }
    *counts.entry(key.to_owned()).or_default() += 1;
    Ok(())
}

fn attributes(
    event: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
) -> Result<BTreeMap<String, String>, ImportError> {
    let mut values = BTreeMap::new();
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| invalid(error.to_string()))?;
        let name = std::str::from_utf8(attribute.key.as_ref())
            .map_err(|_| invalid("attribute name is not UTF-8"))?
            .to_owned();
        if name.len() > 128 {
            return Err(invalid("XML attribute name exceeds 128 bytes"));
        }
        let value = attribute
            .decode_and_unescape_value(reader.decoder())
            .map_err(|error| invalid(error.to_string()))?
            .into_owned();
        if matches!(name.as_str(), "id" | "synset" | "target") && value.len() > 128 {
            return Err(invalid("WN-LMF identity/reference exceeds 128 bytes"));
        }
        if value.len() > MAX_STRING_BYTES || values.len() >= 64 {
            return Err(invalid("WN-LMF attribute size/count limit exceeded"));
        }
        if values.insert(name, value).is_some() {
            return Err(invalid("duplicate XML attribute"));
        }
    }
    Ok(values)
}

fn parse(xml: &[u8], max_estimated_bytes: usize) -> Result<RawWordnet, ImportError> {
    let mut budget = MemoryBudget {
        used: 0,
        limit: max_estimated_bytes,
    };
    // Include supplied input and room for bounded diagnostic map/tree overhead.
    budget.charge(xml.len() + 1024 * 1024)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().check_end_names = true;
    let mut raw = RawWordnet::default();
    let mut stack = Vec::<String>::new();
    let mut entry: Option<RawEntry> = None;
    let mut sense: Option<RawSense> = None;
    let mut synset: Option<RawSynset> = None;
    let mut text = String::new();
    let mut pronunciation_attributes = BTreeMap::new();
    let mut lexicons = 0;
    let mut roots = 0;
    let mut senses = 0;
    let mut relations = 0;
    let mut ignored_depth = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| invalid(format!("invalid WN-LMF XML: {error}")))?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(event) | Event::Empty(event) => {
                let name = std::str::from_utf8(event.name().as_ref())
                    .map_err(|_| invalid("element name is not UTF-8"))?
                    .to_owned();
                if name.len() > 128 {
                    return Err(invalid("XML element name exceeds 128 bytes"));
                }
                let attrs = attributes(&event, &reader)?;
                budget.charge(
                    attrs
                        .iter()
                        .map(|(key, value)| key.len() + value.len())
                        .sum(),
                )?;
                let parent = stack.last().map(String::as_str);
                if stack.len() >= 32 {
                    return Err(invalid("XML nesting exceeds 32 levels"));
                }
                if ignored_depth.is_none() {
                    let consumed: &[&str] = match name.as_str() {
                        "LexicalResource" if parent.is_none() => {
                            roots += 1;
                            if roots != 1 {
                                return Err(invalid("multiple XML roots"));
                            }
                            &["xmlns:dc"]
                        },
                        "Lexicon" if parent == Some("LexicalResource") => {
                            lexicons += 1;
                            if lexicons != 1 {
                                return Err(invalid(
                                    "multiple lexicons are unsupported; choose one local source",
                                ));
                            }
                            raw.metadata = attrs.clone();
                            &[
                                "id", "version", "language", "label", "license", "citation", "url",
                            ]
                        },
                        "LexicalEntry" if parent == Some("Lexicon") => {
                            budget.charge(512)?;
                            entry = Some(RawEntry {
                                id: required(&attrs, "id")?.into(),
                                ..Default::default()
                            });
                            &["id"]
                        },
                        "Lemma" if parent == Some("LexicalEntry") => {
                            let current = entry.as_mut().expect("entry parent");
                            if !current.lemma.is_empty() {
                                return Err(invalid("multiple Lemma elements"));
                            }
                            current.lemma = required(&attrs, "writtenForm")?.into();
                            current.pos = required(&attrs, "partOfSpeech")?.into();
                            &["writtenForm", "partOfSpeech"]
                        },
                        "Sense" if parent == Some("LexicalEntry") => {
                            budget.charge(384)?;
                            sense = Some(RawSense {
                                id: required(&attrs, "id")?.into(),
                                synset: required(&attrs, "synset")?.into(),
                                ..Default::default()
                            });
                            senses += 1;
                            if senses > MAX_XML_SENSES {
                                return Err(invalid("XML sense limit exceeded"));
                            }
                            &["id", "synset"]
                        },
                        "Synset" if parent == Some("Lexicon") => {
                            budget.charge(512)?;
                            synset = Some(RawSynset {
                                id: required(&attrs, "id")?.into(),
                                ..Default::default()
                            });
                            &["id"]
                        },
                        "SenseRelation" if parent == Some("Sense") => {
                            budget.charge(160)?;
                            sense
                                .as_mut()
                                .expect("sense parent")
                                .relations
                                .push(RawRelation {
                                    kind: required(&attrs, "relType")?.into(),
                                    target: required(&attrs, "target")?.into(),
                                });
                            relations += 1;
                            &["relType", "target"]
                        },
                        "SynsetRelation" if parent == Some("Synset") => {
                            budget.charge(160)?;
                            synset
                                .as_mut()
                                .expect("synset parent")
                                .relations
                                .push(RawRelation {
                                    kind: required(&attrs, "relType")?.into(),
                                    target: required(&attrs, "target")?.into(),
                                });
                            relations += 1;
                            &["relType", "target"]
                        },
                        "Definition" if parent == Some("Synset") => {
                            text.clear();
                            &[]
                        },
                        "Example" if matches!(parent, Some("Sense" | "Synset")) => {
                            text.clear();
                            &[]
                        },
                        "Pronunciation" if parent == Some("Lemma") => {
                            text.clear();
                            pronunciation_attributes = attrs.clone();
                            &["notation", "variety"]
                        },
                        "LexiconExtension"
                        | "ExternalLexicalEntry"
                        | "ExternalSynset"
                        | "ExternalSense" => {
                            return Err(invalid(
                                "external/extended lexicons are unsupported; references must be local",
                            ));
                        },
                        "LexicalResource" | "Lexicon" | "LexicalEntry" | "Lemma" | "Sense"
                        | "Synset" | "SenseRelation" | "SynsetRelation" | "Definition"
                        | "Example" | "Pronunciation" => {
                            return Err(invalid(format!(
                                "invalid WN-LMF element placement: {name}"
                            )));
                        },
                        _ => {
                            if matches!(
                                parent,
                                None | Some(
                                    "LexicalResource" | "Definition" | "Example" | "Pronunciation"
                                )
                            ) {
                                return Err(invalid(format!(
                                    "unsupported important WN-LMF element: {name}"
                                )));
                            }
                            count(&mut raw.ignored_elements, &name)?;
                            ignored_depth = Some(stack.len());
                            &[]
                        },
                    };
                    for key in attrs.keys() {
                        if !consumed.contains(&key.as_str()) {
                            count(&mut raw.ignored_attributes, &format!("{name}:{key}"))?;
                        }
                    }
                }
                if relations > MAX_XML_RELATIONS {
                    return Err(invalid("XML relation limit exceeded"));
                }
                stack.push(name.clone());
                if empty {
                    finish(
                        &name,
                        &mut raw,
                        &mut entry,
                        &mut sense,
                        &mut synset,
                        &mut text,
                        &pronunciation_attributes,
                        ignored_depth.is_some(),
                    )?;
                    stack.pop();
                    if ignored_depth == Some(stack.len()) {
                        ignored_depth = None;
                    }
                }
            },
            Event::End(event) => {
                let name = std::str::from_utf8(event.name().as_ref())
                    .map_err(|_| invalid("element name is not UTF-8"))?
                    .to_owned();
                if stack.pop().as_deref() != Some(name.as_str()) {
                    return Err(invalid("mismatched XML end element"));
                }
                finish(
                    &name,
                    &mut raw,
                    &mut entry,
                    &mut sense,
                    &mut synset,
                    &mut text,
                    &pronunciation_attributes,
                    ignored_depth.is_some(),
                )?;
                if ignored_depth == Some(stack.len()) {
                    ignored_depth = None;
                }
            },
            Event::Text(value) => {
                let value = value.decode().map_err(|error| invalid(error.to_string()))?;
                budget.charge(value.len())?;
                append_text(&mut text, &stack, ignored_depth, &value)?;
            },
            Event::CData(value) => {
                let value = value.decode().map_err(|error| invalid(error.to_string()))?;
                budget.charge(value.len())?;
                append_text(&mut text, &stack, ignored_depth, &value)?;
            },
            Event::GeneralRef(value) => {
                let entity = format!(
                    "&{};",
                    value.decode().map_err(|error| invalid(error.to_string()))?
                );
                let resolved = quick_xml::escape::unescape(&entity)
                    .map_err(|error| invalid(format!("unsupported XML entity: {error}")))?;
                budget.charge(resolved.len())?;
                append_text(&mut text, &stack, ignored_depth, &resolved)?;
            },
            Event::DocType(value) => {
                if value.as_ref().contains(&b'[') {
                    return Err(invalid(
                        "internal DTD/entities are unsupported; no external entities are fetched",
                    ));
                }
            },
            Event::Decl(value) => {
                if let Some(encoding) = value.encoding() {
                    let encoding = encoding.map_err(|error| invalid(error.to_string()))?;
                    if !encoding.as_ref().eq_ignore_ascii_case(b"UTF-8") {
                        return Err(invalid("only UTF-8 XML is supported"));
                    }
                }
            },
            Event::Comment(_) => {},
            Event::PI(_) => return Err(invalid("XML processing instructions are unsupported")),
            Event::Eof => break,
        }
    }
    if roots != 1 || lexicons != 1 || !stack.is_empty() {
        return Err(invalid("incomplete WN-LMF LexicalResource/Lexicon"));
    }
    raw.retained_bytes = budget.used;
    Ok(raw)
}

fn append_text(
    text: &mut String,
    stack: &[String],
    ignored: Option<usize>,
    value: &str,
) -> Result<(), ImportError> {
    if ignored.is_some() {
        return Ok(());
    }
    if matches!(
        stack.last().map(String::as_str),
        Some("Definition" | "Example" | "Pronunciation")
    ) {
        if text.len() + value.len() > MAX_STRING_BYTES {
            return Err(invalid("XML text exceeds 16 KiB"));
        }
        text.push_str(value);
    } else if !value.trim().is_empty() {
        return Err(invalid("unexpected text outside lexical text elements"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn finish(
    name: &str,
    raw: &mut RawWordnet,
    entry: &mut Option<RawEntry>,
    sense: &mut Option<RawSense>,
    synset: &mut Option<RawSynset>,
    text: &mut String,
    pronunciation: &BTreeMap<String, String>,
    ignored: bool,
) -> Result<(), ImportError> {
    if ignored {
        return Ok(());
    }
    match name {
        "LexicalEntry" => {
            let value = entry.take().expect("entry opened");
            if value.lemma.is_empty() {
                return Err(invalid("lexical entry has no lemma"));
            }
            raw.entries.push(value);
            if raw.entries.len() > MAX_XML_ENTRIES {
                return Err(invalid("XML entry count exceeded"));
            }
        },
        "Sense" => entry
            .as_mut()
            .expect("entry parent")
            .senses
            .push(sense.take().expect("sense opened")),
        "Synset" => {
            let value = synset.take().expect("synset opened");
            if raw.synsets.insert(value.id.clone(), value).is_some() {
                return Err(invalid("duplicate synset ID"));
            }
            if raw.synsets.len() > MAX_XML_SYNSETS {
                return Err(invalid("XML synset count exceeded"));
            }
        },
        "Definition" => synset
            .as_mut()
            .expect("synset parent")
            .definitions
            .push(std::mem::take(text)),
        "Example" => {
            if let Some(sense) = sense {
                sense.examples.push(std::mem::take(text));
            } else {
                synset
                    .as_mut()
                    .expect("synset parent")
                    .examples
                    .push(std::mem::take(text));
            }
        },
        "Pronunciation" => {
            entry
                .as_mut()
                .expect("entry parent")
                .pronunciations
                .push(Pronunciation {
                    notation: pronunciation
                        .get("notation")
                        .cloned()
                        .unwrap_or_else(|| "unspecified".into()),
                    value: std::mem::take(text),
                    variant: pronunciation.get("variety").cloned(),
                })
        },
        _ => {},
    }
    Ok(())
}
