// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Syllabification: one algorithm, parameterized per language.
//!
//! Nuclei are the vowels; the consonants between two nuclei are divided by
//! onset maximization constrained by the sonority hierarchy. What varies
//! between languages is [`SyllableRule`], not the procedure.

use crate::phone::{Class, Phone};

/// A syllable, addressed by index into the pronunciation it came from.
///
/// Onset is `start..nucleus`, the nucleus is the single phone at `nucleus`, and
/// the coda is `nucleus + 1..end`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Syllable {
    pub start: usize,
    pub nucleus: usize,
    pub end: usize,
}

impl Syllable {
    pub const fn onset(&self) -> core::ops::Range<usize> {
        self.start..self.nucleus
    }

    pub const fn coda(&self) -> core::ops::Range<usize> {
        self.nucleus + 1..self.end
    }

    pub const fn onset_len(&self) -> usize {
        self.nucleus - self.start
    }

    pub const fn coda_len(&self) -> usize {
        self.end - self.nucleus - 1
    }

    /// True when the syllable ends in a consonant.
    pub const fn is_closed(&self) -> bool {
        self.coda_len() > 0
    }
}

/// The per-language parameters of syllabification.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SyllableRule {
    /// Longest consonant cluster allowed in an onset.
    pub max_onset: usize,
    /// Permit a sibilant fricative before a stop in an onset, which violates
    /// the sonority rise. English `str-`, `sp-`, `sk-` need this; a language
    /// without such clusters should leave it off.
    pub sibilant_stop_onset: bool,
}

impl SyllableRule {
    pub const ENGLISH: Self = Self { max_onset: 3, sibilant_stop_onset: true };
    /// Strict rising sonority, no sibilant licence: the conservative default
    /// for a language whose clusters have not been described yet.
    pub const STRICT: Self = Self { max_onset: 2, sibilant_stop_onset: false };
    /// One consonant per onset, which is the safe reading for languages with
    /// simple syllable structure.
    pub const SIMPLE: Self = Self { max_onset: 1, sibilant_stop_onset: false };
}

impl Default for SyllableRule {
    fn default() -> Self {
        Self::STRICT
    }
}

/// Is `cluster` a legal onset under `rule`?
///
/// Legal means sonority rises strictly toward the nucleus, with the optional
/// sibilant-plus-stop licence applied to the first phone only.
pub fn is_legal_onset(cluster: &[Phone], rule: SyllableRule) -> bool {
    if cluster.len() > rule.max_onset {
        return false;
    }
    if cluster.len() < 2 {
        return true;
    }

    let mut from = 0;
    if rule.sibilant_stop_onset
        && cluster[0].class == Class::Fricative
        && cluster[1].class.sonority() <= Class::Fricative.sonority()
    {
        // `s` + stop: skip the offending step, then require a rise from there.
        from = 1;
    }

    cluster[from..]
        .windows(2)
        .all(|w| w[0].class.sonority() < w[1].class.sonority())
}

/// Divide `phones` into syllables, writing them into `out`.
///
/// Returns the number of syllables written, or `None` if `out` is too short.
/// A pronunciation with no vowel yields zero syllables.
pub fn syllabify_into(
    phones: &[Phone],
    rule: SyllableRule,
    out: &mut [Syllable],
) -> Option<usize> {
    let mut count = 0;

    // Pass 1: every vowel is a nucleus.
    for (i, p) in phones.iter().enumerate() {
        if p.is_vowel() {
            if count >= out.len() {
                return None;
            }
            out[count] = Syllable { start: i, nucleus: i, end: i + 1 };
            count += 1;
        }
    }
    if count == 0 {
        return Some(0);
    }

    // Pass 2: everything before the first nucleus is its onset; everything
    // after the last is its coda.
    out[0].start = 0;
    out[count - 1].end = phones.len();

    // Pass 3: divide each intervening cluster by onset maximization.
    for i in 0..count.saturating_sub(1) {
        let left = out[i].nucleus;
        let right = out[i + 1].nucleus;
        let cluster = &phones[left + 1..right];

        // Take the longest legal onset the following syllable will accept; the
        // remainder closes the preceding one.
        let mut onset_len = 0;
        for take in (1..=cluster.len().min(rule.max_onset)).rev() {
            if is_legal_onset(&cluster[cluster.len() - take..], rule) {
                onset_len = take;
                break;
            }
        }

        let split = right - onset_len;
        out[i].end = split;
        out[i + 1].start = split;
    }

    Some(count)
}

/// Divide `phones` into syllables.
#[cfg(feature = "alloc")]
pub fn syllabify(phones: &[Phone], rule: SyllableRule) -> alloc::vec::Vec<Syllable> {
    let mut out = alloc::vec![Syllable { start: 0, nucleus: 0, end: 0 }; phones.len()];
    let n = syllabify_into(phones, rule, &mut out).unwrap_or(0);
    out.truncate(n);
    out
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use crate::phone::Class;

    fn c(id: u16, class: Class) -> Phone {
        Phone::new(id, class)
    }
    fn v(id: u16) -> Phone {
        Phone::new(id, Class::Vowel)
    }

    #[test]
    fn single_syllable_takes_everything() {
        // "cat": stop, vowel, stop
        let w = [c(1, Class::Stop), v(2), c(3, Class::Stop)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0], Syllable { start: 0, nucleus: 1, end: 3 });
        assert!(s[0].is_closed());
    }

    #[test]
    fn onset_maximization_moves_a_single_consonant_rightward() {
        // V C V: the consonant belongs to the second syllable.
        let w = [v(1), c(2, Class::Stop), v(3)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].coda_len(), 0);
        assert_eq!(s[1].onset_len(), 1);
    }

    #[test]
    fn falling_sonority_cluster_splits_across_the_boundary() {
        // "am.ber" — V, nasal, stop, V: sonority falls across the cluster, so
        // the nasal cannot join the onset and closes the first syllable.
        let w = [v(1), c(2, Class::Nasal), c(3, Class::Stop), v(4)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].coda_len(), 1, "nasal closes the first syllable");
        assert_eq!(s[1].onset_len(), 1, "stop opens the second");
    }

    #[test]
    fn a_rising_cluster_joins_the_onset_even_where_a_language_forbids_it() {
        // Stop + nasal rises in sonority, so the sonority principle admits it;
        // that English has no /kn/ onset is phonotactics, which belongs to a
        // language's own table rather than to this algorithm.
        let w = [v(1), c(2, Class::Stop), c(3, Class::Nasal), v(4)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(s[1].onset_len(), 2);
        assert_eq!(s[0].coda_len(), 0);
    }

    #[test]
    fn rising_sonority_cluster_stays_whole_in_the_onset() {
        // V, stop, liquid, V: sonority rises, so both join the onset.
        let w = [v(1), c(2, Class::Stop), c(3, Class::Liquid), v(4)];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(s[0].coda_len(), 0);
        assert_eq!(s[1].onset_len(), 2);
    }

    #[test]
    fn sibilant_licence_is_a_language_parameter() {
        let cluster = [c(1, Class::Fricative), c(2, Class::Stop), c(3, Class::Liquid)];
        assert!(is_legal_onset(&cluster, SyllableRule::ENGLISH));
        assert!(!is_legal_onset(&cluster, SyllableRule { max_onset: 3, sibilant_stop_onset: false }));
    }

    #[test]
    fn a_pronunciation_without_vowels_has_no_syllables() {
        let w = [c(1, Class::Stop), c(2, Class::Fricative)];
        assert!(syllabify(&w, SyllableRule::ENGLISH).is_empty());
    }

    #[test]
    fn into_form_reports_an_undersized_buffer() {
        let w = [v(1), c(2, Class::Stop), v(3)];
        let mut out = [Syllable { start: 0, nucleus: 0, end: 0 }; 1];
        assert!(syllabify_into(&w, SyllableRule::ENGLISH, &mut out).is_none());
    }
}
