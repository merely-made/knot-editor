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

mod importer;

pub use importer::import_wordnet_with_budget;

mod parser;

use parser::parse;
