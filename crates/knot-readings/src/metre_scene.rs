// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Read-only export of an existing, complete lexical sound reading. The
//! template is supplied by the caller; dictionary stress is a candidate,
//! never an assertion about a writer's intended performance.
use crate::sound::{self, SoundLayers, SoundReading};
use mora::{
    english::{SYLLABLE_RULE, WEIGHT_RULE},
    meter::{Beat, Mode, beats},
    sonance::rhyme_tail,
    syllable::syllabify,
};
use mora_cmudict::Cmudict;
use sceno::SourceRef;
use scenomise::backdrops::{
    BACKDROP_SCENE_VERSION, MetreGrid, SceneFacts, SceneInput, Syllable, VerseLine,
};
use std::collections::BTreeMap;

pub struct MetreTemplate {
    pub feet: u16,
    pub positions_per_foot: u16,
    /// One expected rhyme group per nonempty source line, or empty for none.
    pub rhyme_groups: Vec<Option<String>>,
}

pub fn metre_scene(
    text: &str,
    reading: &SoundReading,
    source: SourceRef,
    template: &MetreTemplate,
) -> Result<SceneInput, String> {
    let choices = reading
        .tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            (!t.defaulted)
                .then_some(t.selected.map(|v| (i, v)))
                .flatten()
        })
        .collect::<BTreeMap<_, _>>();
    let current = sound::analyze(
        text,
        reading.selection.clone(),
        &choices,
        &SoundLayers::default(),
    )?;
    if current.tokens != reading.tokens
        || !reading.unresolved.is_empty()
        || reading.tokens.iter().any(|t| t.selected.is_none())
    {
        return Err("Metre export needs a current reading with every pronunciation resolved; unknown stress is not weak stress".into());
    }
    let dictionary = Cmudict::embedded();
    let mut lines = Vec::new();
    let mut offset = reading.selection.start;
    for raw in text[reading.selection.clone()].split_inclusive(['\n', '\r']) {
        let end = offset + raw.trim_end_matches(['\n', '\r']).len();
        let tokens: Vec<_> = reading
            .tokens
            .iter()
            .filter(|t| t.start >= offset && t.end <= end)
            .collect();
        if !tokens.is_empty() {
            let mut syllables = Vec::new();
            let mut tail = None;
            for token in tokens {
                let word = token.word.replace('’', "'");
                let phones = dictionary
                    .pronunciations(&word)
                    .and_then(|v| v.get(token.selected?))
                    .ok_or("Pronunciation changed; rerun the sound reading")?;
                let parts = syllabify(phones, SYLLABLE_RULE);
                let stress = beats(phones, &parts, Mode::Accentual, WEIGHT_RULE);
                for (index, beat) in stress.into_iter().enumerate() {
                    syllables.push(Syllable {
                        text: if index == 0 {
                            token.word.clone()
                        } else {
                            format!("·{}", index + 1)
                        },
                        stressed: beat == Beat::Strong,
                        // The CMUdict pronunciation supplies the stress judgment.
                        stress_known: true,
                    });
                }
                let phones = rhyme_tail(phones, &parts);
                tail = (!phones.is_empty()).then(|| {
                    phones
                        .iter()
                        .map(|p| p.id.to_string())
                        .collect::<Vec<_>>()
                        .join("-")
                });
            }
            let key = format!("bytes-{offset}-{end}");
            lines.push(VerseLine {
                key,
                source: SourceRef {
                    adapter: "knot.document-range".into(),
                    id: format!("{}#bytes={offset}..{end}", source.id),
                },
                text: text[offset..end].into(),
                syllables,
                alignment: None,
                rhyme_group: None,
                rhyme_tail: tail,
            });
        }
        offset += raw.len();
    }
    if lines.is_empty() {
        return Err("No resolved source lines to export".into());
    }
    if !template.rhyme_groups.is_empty() && template.rhyme_groups.len() != lines.len() {
        return Err("Expected rhyme groups must name each nonempty source line".into());
    }
    for (line, group) in lines.iter_mut().zip(&template.rhyme_groups) {
        line.rhyme_group = group.clone();
    }
    let facts = SceneFacts::MetreGrid(MetreGrid {
        feet: template.feet,
        positions_per_foot: template.positions_per_foot,
        lines,
    });
    let pronunciations = reading
        .tokens
        .iter()
        .map(|token| {
            (
                token.start,
                token.end,
                token.selected,
                token.defaulted,
                token.selected.and_then(|index| token.variants.get(index)),
            )
        })
        .collect::<Vec<_>>();
    let revision = blake3::hash(
        &serde_json::to_vec(&(text, &facts, pronunciations)).map_err(|e| e.to_string())?,
    )
    .to_hex()
    .to_string();
    let input = SceneInput {
        version: BACKDROP_SCENE_VERSION,
        source,
        revision,
        title: "CMUdict candidate stress · later syllables numbered · not intended performance"
            .into(),
        facts,
    };
    input.compile().map_err(|e| e.to_string())?;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> SourceRef {
        SourceRef {
            adapter: "knot.document".into(),
            id: "file:///poem.djot".into(),
        }
    }
    #[test]
    fn selected_pronunciation_and_source_ranges_survive_portable_round_trip() {
        let text = "the night\nthe light";
        let reading = sound::analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        let before = reading.clone();
        let template = MetreTemplate {
            feet: 1,
            positions_per_foot: 2,
            rhyme_groups: vec![Some("A".into()), Some("A".into())],
        };
        let input = metre_scene(text, &reading, source(), &template).unwrap();
        assert_eq!(reading, before);
        let compiled = input.compile().unwrap();
        assert_eq!(
            compiled.scene.relations[0].kind.as_deref(),
            Some("derived.rhyme")
        );
        assert!(
            compiled
                .scene
                .sources
                .iter()
                .any(|s| s.id.ends_with("#bytes=10..19"))
        );
        assert_eq!(
            input,
            SceneInput::parse(&serde_json::to_string(&input).unwrap()).unwrap()
        );
    }
    #[test]
    fn unresolved_and_stale_readings_are_refused_without_source_edits() {
        for text in ["qzxvplugh", "the night"] {
            let reading = sound::analyze(
                text,
                0..text.len(),
                &BTreeMap::new(),
                &SoundLayers::default(),
            )
            .unwrap();
            let changed = if text == "the night" {
                "the light"
            } else {
                text
            };
            assert!(
                metre_scene(
                    changed,
                    &reading,
                    source(),
                    &MetreTemplate {
                        feet: 1,
                        positions_per_foot: 3,
                        rhyme_groups: vec![]
                    }
                )
                .is_err()
            );
        }
    }
    #[test]
    fn a_selected_pronunciation_changes_the_exported_candidate_revision() {
        let text = "read";
        let a = sound::analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        assert!(a.tokens[0].variants.len() > 1);
        let b = sound::analyze(
            text,
            0..text.len(),
            &[(0, 1)].into(),
            &SoundLayers::default(),
        )
        .unwrap();
        let template = MetreTemplate {
            feet: 1,
            positions_per_foot: 2,
            rhyme_groups: vec![],
        };
        assert_ne!(
            metre_scene(text, &a, source(), &template).unwrap().revision,
            metre_scene(text, &b, source(), &template).unwrap().revision
        );
        let explicit_first = sound::analyze(
            text,
            0..text.len(),
            &[(0, 0)].into(),
            &SoundLayers::default(),
        )
        .unwrap();
        let default_recipe = metre_scene(text, &a, source(), &template).unwrap();
        let explicit_recipe = metre_scene(text, &explicit_first, source(), &template).unwrap();
        assert_eq!(default_recipe.facts, explicit_recipe.facts);
        assert_ne!(default_recipe.revision, explicit_recipe.revision);
    }
    #[test]
    fn original_carriage_return_ranges_are_kept_in_the_reading() {
        let text = "the night\r\nthe light";
        let reading = sound::analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        let input = metre_scene(
            text,
            &reading,
            source(),
            &MetreTemplate {
                feet: 1,
                positions_per_foot: 2,
                rhyme_groups: vec![Some("A".into()), Some("A".into())],
            },
        )
        .unwrap();
        let SceneFacts::MetreGrid(grid) = input.facts else {
            unreachable!()
        };
        assert_eq!(grid.lines.len(), 2);
        assert!(grid.lines[1].source.id.ends_with("#bytes=11..20"));
    }
}
