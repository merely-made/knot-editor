// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Authored metre over document blocks, independent of dictionary completeness.
//! Attribute names are prototype serialization details; domain facts and the
//! normal document history own authorship, not an analysis result or a view.
use sceno::SourceRef;
use scenomise::backdrops::{
    BACKDROP_SCENE_VERSION, MetreGrid, SceneFacts, SceneInput, Syllable, VerseLine,
};
use scenomise::{span_edit::SpanEditIntent, template::Span};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const AUTHORSHIP: &str = "metre-authorship";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredLine {
    pub text: String,
    pub units: Vec<String>,
    pub stress: Vec<Option<bool>>,
    pub alignment: Option<Vec<Option<Span>>>,
}

pub fn authorship(attributes: &BTreeMap<String, String>) -> Result<Vec<AuthoredLine>, String> {
    let Some(encoded) = attributes.get(AUTHORSHIP) else {
        return Ok(Vec::new());
    };
    let value: Value =
        serde_json::from_str(encoded).map_err(|e| format!("Unreadable authored metre: {e}"))?;
    value
        .as_array()
        .ok_or("Authored metre must contain lines")?
        .iter()
        .map(|line| {
            let text = line["text"]
                .as_str()
                .ok_or("Authored line needs text")?
                .to_string();
            let units = line["units"]
                .as_array()
                .ok_or("Authored line needs units")?
                .iter()
                .map(|unit| {
                    unit.as_str()
                        .map(String::from)
                        .ok_or("Unit must be text".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let stress = line["stress"]
                .as_array()
                .ok_or("Authored line needs scansion")?
                .iter()
                .map(|v| {
                    if v.is_null() {
                        Ok(None)
                    } else {
                        v.as_bool()
                            .map(Some)
                            .ok_or("Invalid authored stress".to_string())
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            if units.len() != stress.len() || units.len() > 512 {
                return Err("Units and scansion must agree".into());
            }
            let alignment = if line["alignment"].is_null() {
                None
            } else {
                Some(
                    serde_json::from_value::<Vec<Option<Span>>>(line["alignment"].clone())
                        .map_err(|e| e.to_string())?,
                )
            };
            if alignment.as_ref().is_some_and(|a| a.len() != units.len()) {
                return Err("Alignment and units must agree".into());
            }
            Ok(AuthoredLine {
                text,
                units,
                stress,
                alignment,
            })
        })
        .collect()
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

pub fn set_line(
    attributes: &mut BTreeMap<String, String>,
    text: &str,
    units: &str,
    scan: &str,
) -> Result<(), String> {
    let units: Vec<String> = units
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if compact(&units.concat()) != compact(text) {
        return Err("Authored units must cover this line's text; use | to divide it".into());
    }
    let stress: Vec<Option<bool>> = scan
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '/' => Ok(Some(true)),
            'u' | 'U' => Ok(Some(false)),
            '?' => Ok(None),
            _ => Err("Use / for strong, u for weak, and ? for unknown".to_string()),
        })
        .collect::<Result<_, _>>()?;
    if units.len() != stress.len() {
        return Err("Give one scansion mark per authored unit".into());
    }
    let mut lines = authorship(attributes)?;
    let alignment = lines
        .iter()
        .find(|line| line.text == text && line.units == units)
        .and_then(|line| line.alignment.clone());
    lines.retain(|line| line.text != text);
    lines.push(AuthoredLine {
        text: text.into(),
        units,
        stress,
        alignment,
    });
    attributes.insert(
        AUTHORSHIP.into(),
        serde_json::to_string(
            &lines
                .iter()
                .map(|l| json!({"text":l.text,"units":l.units,"stress":l.stress,"alignment":l.alignment}))
                .collect::<Vec<_>>(),
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(())
}

/// Exact unique text matches survive moves or unrelated edits. Ambiguous or
/// changed lines retain their old authored records for review, without guessing.
pub fn matched<'a>(
    lines: &'a [AuthoredLine],
    current: &[&str],
    text: &str,
) -> Option<&'a AuthoredLine> {
    if current.iter().filter(|line| **line == text).count() != 1 {
        return None;
    }
    let mut matches = lines.iter().filter(|line| line.text == text);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

pub fn scene(
    address: &str,
    id: &str,
    body: &str,
    attrs: &BTreeMap<String, String>,
) -> Result<(SceneInput, Vec<String>), String> {
    let authored = authorship(attrs)?;
    let current: Vec<_> = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let feet = attrs
        .get("feet")
        .map(|s| s.parse::<u16>().map_err(|_| "Invalid feet"))
        .transpose()?
        .unwrap_or(0);
    let positions = attrs
        .get("positions-per-foot")
        .map(|s| s.parse::<u16>().map_err(|_| "Invalid metrical unit"))
        .transpose()?
        .unwrap_or(2);
    let mut notes = Vec::new();
    let rhyme: Vec<_> = attrs
        .get("rhyme")
        .map(|s| s.split_whitespace().collect())
        .unwrap_or_default();
    if !rhyme.is_empty() && rhyme.len() != current.len() {
        notes.push(
            "Expected rhyme groups do not yet match the current line count; review the form".into(),
        );
    }

    let lines = current
        .iter()
        .enumerate()
        .map(|(row, text)| {
            let authored = matched(&authored, &current, text);
            if authored.is_none() {
                notes.push(format!(
                    "Line {}: units/scansion unresolved; manual editing is available",
                    row + 1
                ));
            }
            VerseLine {
                key: format!("{id}/line/{row}"),
                source: SourceRef {
                    adapter: "knot.authored-metre-line".into(),
                    id: format!("{address}#{id}/line/{row}"),
                },
                text: (*text).into(),
                syllables: authored
                    .map(|line| {
                        line.units
                            .iter()
                            .zip(&line.stress)
                            .map(|(text, stress)| Syllable {
                                text: text.clone(),
                                stressed: stress.unwrap_or(false),
                                stress_known: stress.is_some(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                alignment: authored.and_then(|line| line.alignment.clone()),
                rhyme_group: rhyme.get(row).map(|s| (*s).to_string()),
                rhyme_tail: None,
            }
        })
        .collect();
    let stale = authored
        .iter()
        .filter(|line| !current.contains(&line.text.as_str()))
        .count();
    if stale > 0 {
        notes.push(format!(
            "{stale} authored line(s) need review after text changes; their records are preserved"
        ));
    }
    let input = SceneInput {
        version: BACKDROP_SCENE_VERSION,
        title: attrs
            .get("form")
            .cloned()
            .unwrap_or_else(|| "Free composition".into()),
        source: SourceRef {
            adapter: "knot.authored-metre".into(),
            id: format!("{address}#{id}"),
        },
        revision: blake3::hash(&serde_json::to_vec(&(body, attrs)).map_err(|e| e.to_string())?)
            .to_hex()
            .to_string(),
        facts: SceneFacts::MetreGrid(MetreGrid {
            feet,
            positions_per_foot: positions,
            lines,
        }),
    };
    input.validate().map_err(|e| e.to_string())?;
    notes.extend(
        input
            .compile()
            .map_err(|e| e.to_string())?
            .diagnostics
            .into_iter()
            .map(|mut note| {
                for row in (0..current.len()).rev() {
                    note = note.replace(&format!("{id}/line/{row}"), &format!("Line {}", row + 1));
                }
                note
            }),
    );
    if let Some(pattern) = attrs.get("expected-stress") {
        let expected: Vec<Option<bool>> = pattern
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| match c {
                '/' => Ok(Some(true)),
                'u' | 'U' => Ok(Some(false)),
                '?' => Ok(None),
                _ => Err("Expected stress uses u, / or ?".to_string()),
            })
            .collect::<Result<_, _>>()?;
        if !expected.is_empty() {
            for (row, text) in current.iter().enumerate() {
                if let Some(line) = matched(&authored, &current, text) {
                    if line.stress.iter().enumerate().any(|(i, stress)| {
                        stress.is_some_and(|s| expected[i % expected.len()].is_some_and(|e| s != e))
                    }) {
                        notes.push(format!("Line {}: authored scansion differs from the expected form; authorship is preserved", row + 1));
                    }
                }
            }
        }
    }
    Ok((input, notes))
}

/// Accept an existing shared span proposal against current authored block facts.
/// The source text and all scansion/form data stay intact; only alignment changes.
pub fn apply_span(
    address: &str,
    id: &str,
    body: &str,
    attrs: &mut BTreeMap<String, String>,
    intent: &SpanEditIntent,
) -> Result<(), String> {
    let (input, _) = scene(address, id, body, attrs)?;
    let SceneFacts::MetreGrid(after) = intent.proposed_facts(&input)? else {
        unreachable!()
    };
    let row = after
        .lines
        .iter()
        .find(|line| {
            (0..line.syllables.len())
                .any(|i| intent.occurrence == format!("{}/syllable/{i}", line.key))
        })
        .ok_or("Authored line missing")?;
    let mut lines = authorship(attrs)?;
    if lines.iter().filter(|line| line.text == row.text).count() != 1 {
        return Err("Authored line match is ambiguous".into());
    }
    let line = lines.iter_mut().find(|line| line.text == row.text).unwrap();
    line.alignment = row.alignment.clone();
    attrs.insert(AUTHORSHIP.into(), serde_json::to_string(&lines.iter().map(|l|
        json!({"text":l.text,"units":l.units,"stress":l.stress,"alignment":l.alignment})).collect::<Vec<_>>()).map_err(|e|e.to_string())?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_dictionary_or_form_is_required_and_unknown_is_not_weak() {
        let mut attrs = BTreeMap::new();
        let (input, notes) = scene("memory:test", "verse", "quux light\n", &attrs).unwrap();
        let SceneFacts::MetreGrid(grid) = input.facts else {
            panic!()
        };
        assert_eq!(grid.feet, 0);
        assert!(grid.lines[0].syllables.is_empty());
        assert_eq!(notes.len(), 1);
        set_line(&mut attrs, "quux light", "quux | light", "? /").unwrap();
        let (input, _) = scene("memory:test", "verse", "quux light\n", &attrs).unwrap();
        let SceneFacts::MetreGrid(grid) = &input.facts else {
            panic!()
        };
        assert!(!grid.lines[0].syllables[0].stress_known);
        assert!(grid.lines[0].syllables[1].stressed);
        input.compile().unwrap();
    }
    #[test]
    fn shared_span_edit_preserves_scansion_and_form_and_survives_reopening() {
        use scenomise::span_edit::SpanEdit;
        let mut attrs = BTreeMap::from([
            ("feet".into(), "3".into()),
            ("form".into(), "Custom".into()),
        ]);
        set_line(&mut attrs, "quux light", "quux | light", "? /").unwrap();
        let (input, _) = scene("memory:test", "verse", "quux light\n", &attrs).unwrap();
        let intent =
            SpanEditIntent::propose(&input, "verse/line/0/syllable/1", SpanEdit::Move, 1.0, 1)
                .unwrap();
        apply_span("memory:test", "verse", "quux light\n", &mut attrs, &intent).unwrap();
        let restored: BTreeMap<String, String> =
            serde_json::from_str(&serde_json::to_string(&attrs).unwrap()).unwrap();
        let line = authorship(&restored).unwrap().remove(0);
        assert_eq!(line.stress, vec![None, Some(true)]);
        assert_eq!(line.alignment.as_ref().unwrap()[1].unwrap().start, 2);
        assert_eq!(restored["form"], "Custom");
        let before = attrs.clone();
        assert!(
            apply_span(
                "memory:test",
                "verse",
                "quux changed\n",
                &mut attrs,
                &intent
            )
            .is_err()
        );
        assert_eq!(attrs, before);
    }
    #[test]
    fn clear_matches_survive_moves_and_changed_authorship_stays_for_review() {
        let mut attrs = BTreeMap::new();
        set_line(&mut attrs, "the night", "the | night", "u /").unwrap();
        set_line(&mut attrs, "the light", "the | light", "u /").unwrap();
        let (input, notes) = scene(
            "memory:test",
            "verse",
            "new line\nthe light\nthe night\n",
            &attrs,
        )
        .unwrap();
        let SceneFacts::MetreGrid(grid) = input.facts else {
            panic!()
        };
        assert_eq!(grid.lines[2].syllables.len(), 2);
        assert_eq!(notes.len(), 1);
        let before = attrs.clone();
        let (_, notes) = scene("memory:test", "verse", "the day\nthe light\n", &attrs).unwrap();
        assert!(notes.iter().any(|s| s.contains("preserved")));
        assert_eq!(attrs, before);
        let (input, _) = scene("memory:test", "verse", "the light\nthe light\n", &attrs).unwrap();
        let SceneFacts::MetreGrid(grid) = input.facts else {
            panic!()
        };
        assert!(grid.lines.iter().all(|line| line.syllables.is_empty()));
    }
}
