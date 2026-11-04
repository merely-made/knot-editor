// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse(xml: &[u8], max_estimated_bytes: usize) -> Result<RawWordnet, ImportError> {
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
