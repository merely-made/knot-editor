// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Weight: the mora, and what a language counts as one.
//!
//! This is the layer the crate is named for. Stress-timed, syllable-timed, and
//! mora-timed languages differ in what they *do* with weight, not in what
//! weight is, so one counter parameterized by [`WeightRule`] serves all three.

use crate::phone::Phone;
use crate::syllable::Syllable;

/// Traditional weight classes, derived from a mora count.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Weight {
    /// One mora: an open syllable with a short nucleus.
    Light,
    /// Two morae: a long nucleus, or a closed syllable where codas count.
    Heavy,
    /// Three or more.
    Superheavy,
}

impl Weight {
    pub const fn from_morae(morae: u8) -> Self {
        match morae {
            0 | 1 => Weight::Light,
            2 => Weight::Heavy,
            _ => Weight::Superheavy,
        }
    }
}

/// What a language counts as a mora.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WeightRule {
    /// Morae contributed by a long or diphthongal nucleus.
    pub long_nucleus_morae: u8,
    /// Whether a coda consonant contributes a mora. This single flag is most
    /// of the difference between the prosodic types.
    pub coda_moraic: bool,
    /// Cap on morae counted from the coda, so a heavy cluster does not run
    /// away.
    pub max_coda_morae: u8,
}

impl WeightRule {
    /// Weight-sensitive: long nuclei and closed syllables are both heavy.
    /// The reading English stress and Latin quantity both want.
    pub const CLASSICAL: Self =
        Self { long_nucleus_morae: 2, coda_moraic: true, max_coda_morae: 1 };
    /// Japanese: a coda consonant (moraic nasal, first half of a geminate) is
    /// its own mora, which is why the language is called mora-timed.
    pub const MORAIC: Self =
        Self { long_nucleus_morae: 2, coda_moraic: true, max_coda_morae: 2 };
    /// Weight-insensitive: every syllable counts one, the reading a
    /// syllable-timed language wants.
    pub const SYLLABIC: Self =
        Self { long_nucleus_morae: 1, coda_moraic: false, max_coda_morae: 0 };
}

impl Default for WeightRule {
    fn default() -> Self {
        Self::CLASSICAL
    }
}

/// Count the morae in one syllable.
pub fn morae(phones: &[Phone], syllable: &Syllable, rule: WeightRule) -> u8 {
    let nucleus = match phones.get(syllable.nucleus) {
        Some(p) => p,
        None => return 0,
    };

    let mut n = if nucleus.long { rule.long_nucleus_morae } else { 1 };

    if rule.coda_moraic {
        let coda = syllable.coda_len().min(rule.max_coda_morae as usize) as u8;
        n = n.saturating_add(coda);
    }

    n
}

/// Weight of one syllable.
pub fn weight(phones: &[Phone], syllable: &Syllable, rule: WeightRule) -> Weight {
    Weight::from_morae(morae(phones, syllable, rule))
}

/// Total morae across a pronunciation.
///
/// This is the count a haiku is written against: Japanese counts morae here,
/// not syllables, which is why English haiku written on syllables come out
/// short.
pub fn total_morae(phones: &[Phone], syllables: &[Syllable], rule: WeightRule) -> u32 {
    syllables.iter().map(|s| morae(phones, s, rule) as u32).sum()
}

/// The Latin stress rule, as the proof that weight sits beneath stress.
///
/// Stress falls on the penult when the penult is heavy, otherwise on the
/// antepenult; words of one or two syllables stress the first. Returns the
/// index of the stressed syllable, or `None` for an empty word.
///
/// Nothing about English is involved: the rule reads weight alone, which is
/// what makes weight the deeper layer.
pub fn penultimate_stress(
    phones: &[Phone],
    syllables: &[Syllable],
    rule: WeightRule,
) -> Option<usize> {
    match syllables.len() {
        0 => None,
        1 | 2 => Some(0),
        n => {
            let penult = n - 2;
            if weight(phones, &syllables[penult], rule) >= Weight::Heavy {
                Some(penult)
            } else {
                Some(n - 3)
            }
        }
    }
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use crate::phone::Class;
    use crate::syllable::{SyllableRule, syllabify};

    fn c(id: u16, class: Class) -> Phone {
        Phone::new(id, class)
    }
    fn v(id: u16) -> Phone {
        Phone::new(id, Class::Vowel)
    }
    fn long(id: u16) -> Phone {
        Phone::new(id, Class::Vowel).with_long(true)
    }

    #[test]
    fn open_short_syllable_is_light() {
        let w = [c(1, Class::Stop), v(2)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(weight(&w, &s[0], WeightRule::CLASSICAL), Weight::Light);
        assert_eq!(morae(&w, &s[0], WeightRule::CLASSICAL), 1);
    }

    #[test]
    fn a_long_nucleus_is_heavy_on_its_own() {
        let w = [c(1, Class::Stop), long(2)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(weight(&w, &s[0], WeightRule::CLASSICAL), Weight::Heavy);
    }

    #[test]
    fn a_closed_syllable_is_heavy_only_where_codas_count() {
        let w = [c(1, Class::Stop), v(2), c(3, Class::Nasal)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(weight(&w, &s[0], WeightRule::CLASSICAL), Weight::Heavy);
        assert_eq!(
            weight(&w, &s[0], WeightRule::SYLLABIC),
            Weight::Light,
            "a syllable-timed reading ignores the coda"
        );
    }

    #[test]
    fn latin_rule_prefers_a_heavy_penult() {
        // C V . C V C . C V  — penult closed, so heavy: stress the penult.
        let heavy_penult = [
            c(1, Class::Stop),
            v(2),
            c(3, Class::Stop),
            v(4),
            c(5, Class::Nasal),
            c(6, Class::Stop),
            v(7),
        ];
        let s = syllabify(&heavy_penult, SyllableRule::ENGLISH);
        assert_eq!(s.len(), 3);
        assert_eq!(penultimate_stress(&heavy_penult, &s, WeightRule::CLASSICAL), Some(1));
    }

    #[test]
    fn latin_rule_retreats_to_the_antepenult_when_the_penult_is_light() {
        // C V . C V . C V — every syllable open and short.
        let light_penult = [
            c(1, Class::Stop),
            v(2),
            c(3, Class::Stop),
            v(4),
            c(5, Class::Stop),
            v(6),
        ];
        let s = syllabify(&light_penult, SyllableRule::ENGLISH);
        assert_eq!(s.len(), 3);
        assert_eq!(penultimate_stress(&light_penult, &s, WeightRule::CLASSICAL), Some(0));
    }

    #[test]
    fn moraic_rule_counts_a_coda_nasal_as_its_own_mora() {
        let w = [c(1, Class::Nasal), v(2), c(3, Class::Nasal)];
        let s = syllabify(&w, SyllableRule::SIMPLE);
        assert_eq!(total_morae(&w, &s, WeightRule::MORAIC), 2);
        assert_eq!(total_morae(&w, &s, WeightRule::SYLLABIC), 1);
    }
}
