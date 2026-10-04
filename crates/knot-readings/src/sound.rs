// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bounded, read-only sound readings over a selected source range.
//! Pronunciations are suggestions from the bundled English lexicon; alternatives
//! and the default choice stay visible so the reading never claims source truth.

use std::collections::BTreeMap;
use std::ops::Range;

use mora::Phone;
use mora::english::{SYLLABLE_RULE, WEIGHT_RULE, symbol};
use mora::meter::{Foot, Mode, beats, scan_best};
use mora::sonance::{is_alliterative, is_assonant, is_perfect_rhyme, is_slant_rhyme};
use mora::syllable::syllabify;
use mora_cmudict::Cmudict;

pub const MAX_SELECTION_BYTES: usize = 16 * 1024;
pub const MAX_TOKENS: usize = 128;
pub const MAX_PAIRS: usize = 4_096;

/// Every comparison is opt-in. An empty layer set yields only pronunciation data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoundLayers {
    pub perfect_rhyme: bool,
    pub slant_rhyme: bool,
    pub assonance: bool,
    pub alliteration: bool,
    pub meter: bool,
}

impl SoundLayers {
    fn any_pairwise(self) -> bool {
        self.perfect_rhyme || self.slant_rhyme || self.assonance || self.alliteration
    }
}

/// A token's exact UTF-8 byte range in the input source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundToken {
    pub word: String,
    pub start: usize,
    pub end: usize,
    /// Every dictionary pronunciation, in its dictionary order, as ARPAbet.
    pub variants: Vec<String>,
    /// Index into `variants`; `None` means the word is unresolved.
    pub selected: Option<usize>,
    /// True when the caller did not choose and the first variant was used.
    pub defaulted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundKind {
    PerfectRhyme,
    SlantRhyme,
    Assonance,
    Alliteration,
}

impl SoundKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::PerfectRhyme => "Perfect rhyme",
            Self::SlantRhyme => "Slant rhyme",
            Self::Assonance => "Assonance",
            Self::Alliteration => "Alliteration",
        }
    }
}

/// Token indices refer to `SoundReading::tokens`, whose ranges identify source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundRelation {
    pub left: usize,
    pub right: usize,
    pub kind: SoundKind,
}

/// A whole-selection accentual reading, available only with complete coverage.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundMeter {
    pub label: String,
    pub fit: f32,
    pub regular: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundReading {
    pub selection: Range<usize>,
    pub tokens: Vec<SoundToken>,
    pub relations: Vec<SoundRelation>,
    pub meter: Option<SoundMeter>,
    /// Token indices with no dictionary pronunciation; no guess is substituted.
    pub unresolved: Vec<usize>,
}

/// Analyze at most 16 KiB and 128 tokens from the selected source text.
///
/// `choices` maps zero-based token indices to a dictionary variant index. The
/// first variant is a disclosed default. No script, document, network or file
/// authority is involved; the result owns all its labels and source ranges.
pub fn analyze(
    text: &str,
    selection: Range<usize>,
    choices: &BTreeMap<usize, usize>,
    layers: &SoundLayers,
) -> Result<SoundReading, String> {
    if selection.start > selection.end
        || selection.end > text.len()
        || !text.is_char_boundary(selection.start)
        || !text.is_char_boundary(selection.end)
    {
        return Err("selection is not a valid UTF-8 source range".into());
    }
    if selection.len() > MAX_SELECTION_BYTES {
        return Err(format!("selection exceeds {MAX_SELECTION_BYTES} bytes"));
    }
    let selected_text = &text[selection.clone()];
    let ranges = tokenize(selected_text, selection.start);
    if ranges.len() > MAX_TOKENS {
        return Err(format!("selection exceeds {MAX_TOKENS} word tokens"));
    }
    if choices.keys().any(|&index| index >= ranges.len()) {
        return Err("pronunciation choice names a token outside the selection".into());
    }
    let pair_count = ranges.len().saturating_mul(ranges.len().saturating_sub(1)) / 2;
    if layers.any_pairwise() && pair_count > MAX_PAIRS {
        return Err(format!("sound comparison exceeds {MAX_PAIRS} token pairs"));
    }

    let dictionary = Cmudict::embedded();
    let mut tokens = Vec::with_capacity(ranges.len());
    let mut selected_phones: Vec<Option<Vec<Phone>>> = Vec::with_capacity(ranges.len());
    let mut unresolved = Vec::new();
    for (index, (start, end)) in ranges.into_iter().enumerate() {
        let word = &text[start..end];
        let normalized = word.replace('’', "'");
        let pronunciations = dictionary.pronunciations(&normalized).unwrap_or(&[]);
        let chosen = choices.get(&index).copied();
        if let Some(choice) = chosen
            && choice >= pronunciations.len()
        {
            return Err(format!(
                "pronunciation choice {choice} is invalid for token {index}"
            ));
        }
        let selected = (!pronunciations.is_empty()).then_some(chosen.unwrap_or(0));
        if selected.is_none() {
            unresolved.push(index);
        }
        tokens.push(SoundToken {
            word: word.to_owned(),
            start,
            end,
            variants: pronunciations
                .iter()
                .map(|phones| phone_label(phones))
                .collect(),
            selected,
            defaulted: selected.is_some() && chosen.is_none(),
        });
        selected_phones.push(selected.map(|choice| pronunciations[choice].clone()));
    }

    let mut relations = Vec::new();
    if layers.any_pairwise() {
        for left in 0..tokens.len() {
            let Some(a) = selected_phones[left].as_deref() else {
                continue;
            };
            let a_syllables = syllabify(a, SYLLABLE_RULE);
            for (right, maybe_b) in selected_phones.iter().enumerate().skip(left + 1) {
                let Some(b) = maybe_b.as_deref() else {
                    continue;
                };
                let b_syllables = syllabify(b, SYLLABLE_RULE);
                let a = (a, a_syllables.as_slice());
                let b = (b, b_syllables.as_slice());
                let mut add = |enabled, matches, kind| {
                    if enabled && matches {
                        relations.push(SoundRelation { left, right, kind });
                    }
                };
                add(
                    layers.perfect_rhyme,
                    is_perfect_rhyme(a, b),
                    SoundKind::PerfectRhyme,
                );
                add(
                    layers.slant_rhyme,
                    is_slant_rhyme(a, b),
                    SoundKind::SlantRhyme,
                );
                add(layers.assonance, is_assonant(a, b), SoundKind::Assonance);
                add(
                    layers.alliteration,
                    is_alliterative(a, b),
                    SoundKind::Alliteration,
                );
            }
        }
    }

    // A full meter over missing words or multiple lines would overstate the
    // evidence. Leave it absent and report the unresolved token indices above.
    let meter = if layers.meter
        && unresolved.is_empty()
        && !selected_text.contains('\n')
        && !selected_text.contains('\r')
    {
        let mut line_beats = Vec::new();
        for phones in selected_phones.iter().flatten() {
            let syllables = syllabify(phones, SYLLABLE_RULE);
            line_beats.extend(beats(phones, &syllables, Mode::Accentual, WEIGHT_RULE));
        }
        scan_best(&line_beats, &Foot::COMMON).map(|scan| SoundMeter {
            label: format!("{} × {}", foot_label(scan.meter.foot), scan.meter.feet),
            fit: scan.fit(),
            regular: scan.is_regular(),
        })
    } else {
        None
    };

    Ok(SoundReading {
        selection,
        tokens,
        relations,
        meter,
        unresolved,
    })
}

fn tokenize(text: &str, offset: usize) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        let word = character.is_alphabetic() || matches!(character, '\'' | '’');
        match (start, word) {
            (None, true) => start = Some(index),
            (Some(begin), false) => {
                if text[begin..index].chars().any(char::is_alphabetic) {
                    ranges.push((offset + begin, offset + index));
                }
                start = None;
            },
            _ => {},
        }
    }
    if let Some(begin) = start
        && text[begin..].chars().any(char::is_alphabetic)
    {
        ranges.push((offset + begin, offset + text.len()));
    }
    ranges
}

fn phone_label(phones: &[Phone]) -> String {
    phones
        .iter()
        .map(|phone| {
            let name = symbol(phone.id).unwrap_or("?");
            let stress = match phone.stress {
                mora::Stress::Unstressed if phone.is_vowel() => "0",
                mora::Stress::Primary => "1",
                mora::Stress::Secondary => "2",
                _ => "",
            };
            format!("{name}{stress}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn foot_label(foot: Foot) -> &'static str {
    match foot {
        Foot::Iamb => "Iamb",
        Foot::Trochee => "Trochee",
        Foot::Dactyl => "Dactyl",
        Foot::Anapest => "Anapest",
        _ => "Other foot",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_word_is_reported_and_never_yields_full_meter() {
        let layers = SoundLayers {
            meter: true,
            ..SoundLayers::default()
        };
        let text = "cat qzxqzx";
        let reading = analyze(text, 0..text.len(), &BTreeMap::new(), &layers).unwrap();
        assert_eq!(reading.unresolved, vec![1]);
        assert!(reading.tokens[1].variants.is_empty());
        assert_eq!(reading.tokens[1].selected, None);
        assert!(reading.meter.is_none());
    }

    #[test]
    fn alternative_pronunciations_are_exposed_and_choice_is_used() {
        let text = "read red";
        let default = analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        assert!(default.tokens[0].variants.len() > 1);
        assert_eq!(default.tokens[0].selected, Some(0));
        assert!(default.tokens[0].defaulted);
        let chosen = analyze(
            text,
            0..text.len(),
            &BTreeMap::from([(0, 1)]),
            &SoundLayers::default(),
        )
        .unwrap();
        assert_eq!(chosen.tokens[0].selected, Some(1));
        assert!(!chosen.tokens[0].defaulted);
        assert_ne!(chosen.tokens[0].variants[0], chosen.tokens[0].variants[1]);
        let assonance = SoundLayers {
            assonance: true,
            ..SoundLayers::default()
        };
        let default_sound = analyze(text, 0..text.len(), &BTreeMap::new(), &assonance).unwrap();
        let chosen_sound =
            analyze(text, 0..text.len(), &BTreeMap::from([(0, 1)]), &assonance).unwrap();
        assert_ne!(default_sound.relations, chosen_sound.relations);
        assert!(
            analyze(
                text,
                0..text.len(),
                &BTreeMap::from([(0, 99)]),
                &SoundLayers::default()
            )
            .is_err()
        );
    }

    #[test]
    fn punctuation_and_unicode_keep_exact_source_byte_ranges() {
        let text = "é—cat, bat!";
        let reading = analyze(
            text,
            5..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        assert_eq!(
            reading
                .tokens
                .iter()
                .map(|token| (token.word.as_str(), token.start, token.end))
                .collect::<Vec<_>>(),
            vec![("cat", 5, 8), ("bat", 10, 13)]
        );
        assert!(
            analyze(
                text,
                1..text.len(),
                &BTreeMap::new(),
                &SoundLayers::default()
            )
            .is_err()
        );
    }

    #[test]
    fn bounds_refuse_large_or_invalid_selections() {
        let large = "a".repeat(MAX_SELECTION_BYTES + 1);
        assert!(
            analyze(
                &large,
                0..large.len(),
                &BTreeMap::new(),
                &SoundLayers::default()
            )
            .is_err()
        );
        let many = "cat ".repeat(MAX_TOKENS + 1);
        assert!(
            analyze(
                &many,
                0..many.len(),
                &BTreeMap::new(),
                &SoundLayers::default()
            )
            .is_err()
        );
        let pairs = "cat ".repeat(92);
        let pair_layers = SoundLayers {
            assonance: true,
            ..SoundLayers::default()
        };
        assert!(analyze(&pairs, 0..pairs.len(), &BTreeMap::new(), &pair_layers).is_err());
        assert!(
            analyze(
                "cat",
                std::ops::Range { start: 3, end: 1 },
                &BTreeMap::new(),
                &SoundLayers::default()
            )
            .is_err()
        );
        assert!(
            analyze(
                "cat",
                0..3,
                &BTreeMap::from([(1, 0)]),
                &SoundLayers::default()
            )
            .is_err()
        );
    }

    #[test]
    fn relations_and_meter_require_explicit_layers() {
        let text = "cat bat";
        let plain = analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        assert!(plain.relations.is_empty() && plain.meter.is_none());
        let layers = SoundLayers {
            perfect_rhyme: true,
            meter: true,
            ..SoundLayers::default()
        };
        let reading = analyze(text, 0..text.len(), &BTreeMap::new(), &layers).unwrap();
        assert!(
            reading
                .relations
                .iter()
                .any(|relation| relation.kind == SoundKind::PerfectRhyme)
        );
        assert!(reading.meter.is_some());
        assert!(
            reading
                .relations
                .iter()
                .all(|relation| relation.left == 0 && relation.right == 1)
        );
    }
}
