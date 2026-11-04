// SPDX-License-Identifier: MPL-2.0

use super::*;

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
