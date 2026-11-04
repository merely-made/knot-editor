// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Sonance: sound kinship between two pronunciations.
//!
//! Named for the stem the phenomena share: as-sonance, con-sonance,
//! dis-sonance, re-sonance. Every relation here is a comparison of phone
//! sequences against a shared symbol table, so any language that supplies
//! phones gets all of them at once.
//!
//! This is the canonical public API for Sonance. Import it as `mora::sonance`;
//! the standalone `sonance` repository is an archive pointer only.

use crate::phone::{Phone, Stress};
use crate::syllable::Syllable;

/// Where a rhyme begins: the nucleus of the last syllable carrying primary
/// stress, falling back to the last syllable.
///
/// This is the rhyme's anchor in English and in most accentual traditions:
/// everything from here to the end of the word must agree for a perfect rhyme.
pub fn rhyme_start(phones: &[Phone], syllables: &[Syllable]) -> Option<usize> {
    if syllables.is_empty() {
        return None;
    }
    let anchor = syllables
        .iter()
        .rposition(|s| {
            phones.get(s.nucleus).map(|p| p.stress == Stress::Primary).unwrap_or(false)
        })
        .unwrap_or(syllables.len() - 1);
    Some(syllables[anchor].nucleus)
}

/// The rhyme tail: nucleus onward from [`rhyme_start`].
pub fn rhyme_tail<'a>(phones: &'a [Phone], syllables: &[Syllable]) -> &'a [Phone] {
    match rhyme_start(phones, syllables) {
        Some(i) => &phones[i..],
        None => &[],
    }
}

fn ids_equal(a: &[Phone], b: &[Phone]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.id == y.id)
}

/// A perfect rhyme: identical from the stressed nucleus to the end, with
/// different consonants before it.
///
/// The differing-onset requirement is what separates a rhyme from a repetition;
/// without it every word rhymes with itself.
pub fn is_perfect_rhyme(
    a: (&[Phone], &[Syllable]),
    b: (&[Phone], &[Syllable]),
) -> bool {
    let (ta, tb) = (rhyme_tail(a.0, a.1), rhyme_tail(b.0, b.1));
    if ta.is_empty() || !ids_equal(ta, tb) {
        return false;
    }

    let onset = |p: &[Phone], s: &[Syllable]| -> Option<u16> {
        let i = rhyme_start(p, s)?;
        if i == 0 { None } else { Some(p[i - 1].id) }
    };
    onset(a.0, a.1) != onset(b.0, b.1)
}

/// A slant rhyme: the tails agree on their consonants but not their vowels, or
/// on their vowels but not their consonants.
///
/// Deliberately a boolean over two named cases rather than a tuned score;
/// a consumer that wants a threshold can build one on [`agreement`].
pub fn is_slant_rhyme(
    a: (&[Phone], &[Syllable]),
    b: (&[Phone], &[Syllable]),
) -> bool {
    let (ta, tb) = (rhyme_tail(a.0, a.1), rhyme_tail(b.0, b.1));
    if ta.is_empty() || tb.is_empty() || ids_equal(ta, tb) {
        return false;
    }

    let vowels = |t: &[Phone]| -> Option<u16> { t.iter().find(|p| p.is_vowel()).map(|p| p.id) };
    let consonants_agree = {
        let ca = ta.iter().filter(|p| !p.is_vowel());
        let cb = tb.iter().filter(|p| !p.is_vowel());
        ca.clone().count() > 0
            && ca.clone().count() == cb.clone().count()
            && ca.zip(cb).all(|(x, y)| x.id == y.id)
    };

    let vowels_agree = vowels(ta).is_some() && vowels(ta) == vowels(tb);
    consonants_agree != vowels_agree
}

/// Assonance: the stressed nuclei agree.
pub fn is_assonant(
    a: (&[Phone], &[Syllable]),
    b: (&[Phone], &[Syllable]),
) -> bool {
    match (rhyme_start(a.0, a.1), rhyme_start(b.0, b.1)) {
        (Some(i), Some(j)) => a.0[i].id == b.0[j].id,
        _ => false,
    }
}

/// Consonance: the consonant sequences agree, whatever the vowels do.
pub fn is_consonant(a: &[Phone], b: &[Phone]) -> bool {
    let ca = a.iter().filter(|p| !p.is_vowel());
    let cb = b.iter().filter(|p| !p.is_vowel());
    ca.clone().count() > 0
        && ca.clone().count() == cb.clone().count()
        && ca.zip(cb).all(|(x, y)| x.id == y.id)
}

/// Alliteration: the first onsets agree.
///
/// A word beginning with its nucleus has no onset and alliterates with nothing,
/// which is the conservative reading; traditions that let vowels alliterate can
/// compare first phones directly.
pub fn is_alliterative(
    a: (&[Phone], &[Syllable]),
    b: (&[Phone], &[Syllable]),
) -> bool {
    let first = |p: &[Phone], s: &[Syllable]| -> Option<u16> {
        let syllable = s.first()?;
        if syllable.onset_len() == 0 { None } else { Some(p[syllable.start].id) }
    };
    match (first(a.0, a.1), first(b.0, b.1)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Share of phones agreeing from the end backward, from 0.0 to 1.0.
///
/// The raw measure the named relations are thresholds over; useful when a
/// consumer wants to rank near-rhymes rather than classify them.
pub fn agreement(a: &[Phone], b: &[Phone]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let agreeing = (0..n).take_while(|k| a[a.len() - 1 - k].id == b[b.len() - 1 - k].id).count();
    agreeing as f32 / a.len().max(b.len()) as f32
}

/// Every relation between two pronunciations, in one pass.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Sonance {
    pub perfect_rhyme: bool,
    pub slant_rhyme: bool,
    pub assonance: bool,
    pub consonance: bool,
    pub alliteration: bool,
    pub agreement: f32,
}

impl Sonance {
    /// True when the two share any named relation.
    pub fn any(&self) -> bool {
        self.perfect_rhyme
            || self.slant_rhyme
            || self.assonance
            || self.consonance
            || self.alliteration
    }
}

/// Compare two pronunciations across every relation.
pub fn compare(
    a: (&[Phone], &[Syllable]),
    b: (&[Phone], &[Syllable]),
) -> Sonance {
    Sonance {
        perfect_rhyme: is_perfect_rhyme(a, b),
        slant_rhyme: is_slant_rhyme(a, b),
        assonance: is_assonant(a, b),
        consonance: is_consonant(a.0, b.0),
        alliteration: is_alliterative(a, b),
        agreement: agreement(a.0, b.0),
    }
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use crate::phone::Class;
    use crate::syllable::{SyllableRule, syllabify};

    // A tiny symbol table: ids are arbitrary but shared across the tests.
    const K: u16 = 1;
    const AE: u16 = 2;
    const T: u16 = 3;
    const B: u16 = 4;
    const M: u16 = 5;
    const IY: u16 = 6;

    fn stop(id: u16) -> Phone {
        Phone::new(id, Class::Stop)
    }
    fn nasal(id: u16) -> Phone {
        Phone::new(id, Class::Nasal)
    }
    fn vowel(id: u16) -> Phone {
        Phone::new(id, Class::Vowel).with_stress(Stress::Primary)
    }

    #[cfg(feature = "alloc")]
    fn word(phones: &[Phone]) -> alloc::vec::Vec<Syllable> {
        syllabify(phones, SyllableRule::ENGLISH)
    }

    #[test]
    fn cat_and_bat_rhyme() {
        let cat = [stop(K), vowel(AE), stop(T)];
        let bat = [stop(B), vowel(AE), stop(T)];
        let (sc, sb) = (word(&cat), word(&bat));
        assert!(is_perfect_rhyme((&cat, &sc), (&bat, &sb)));
    }

    #[test]
    fn a_word_does_not_rhyme_with_itself() {
        let cat = [stop(K), vowel(AE), stop(T)];
        let s = word(&cat);
        assert!(!is_perfect_rhyme((&cat, &s), (&cat, &s)));
    }

    #[test]
    fn cat_and_cab_are_slant_not_perfect() {
        let cat = [stop(K), vowel(AE), stop(T)];
        let cab = [stop(K), vowel(AE), stop(B)];
        let (sc, sb) = (word(&cat), word(&cab));
        assert!(!is_perfect_rhyme((&cat, &sc), (&cab, &sb)));
        assert!(is_slant_rhyme((&cat, &sc), (&cab, &sb)), "same vowel, different coda");
        assert!(is_assonant((&cat, &sc), (&cab, &sb)));
    }

    #[test]
    fn cat_and_kit_alliterate_without_rhyming() {
        let cat = [stop(K), vowel(AE), stop(T)];
        let kit = [stop(K), vowel(IY), stop(T)];
        let (sc, sk) = (word(&cat), word(&kit));
        assert!(is_alliterative((&cat, &sc), (&kit, &sk)));
        assert!(!is_perfect_rhyme((&cat, &sc), (&kit, &sk)));
        assert!(is_consonant(&cat, &kit), "same consonant frame, different vowel");
        assert!(is_slant_rhyme((&cat, &sc), (&kit, &sk)));
    }

    #[test]
    fn different_words_share_nothing() {
        let cat = [stop(K), vowel(AE), stop(T)];
        let mee = [nasal(M), vowel(IY)];
        let (sc, sm) = (word(&cat), word(&mee));
        assert!(!compare((&cat, &sc), (&mee, &sm)).any());
    }

    #[test]
    fn agreement_rises_with_a_longer_shared_tail() {
        let cat = [stop(K), vowel(AE), stop(T)];
        let bat = [stop(B), vowel(AE), stop(T)];
        let kit = [stop(K), vowel(IY), stop(T)];
        assert!(agreement(&cat, &bat) > agreement(&cat, &kit));
    }

    #[test]
    fn an_empty_pronunciation_rhymes_with_nothing() {
        let cat = [stop(K), vowel(AE), stop(T)];
        let s = word(&cat);
        assert!(!is_perfect_rhyme((&cat, &s), (&[], &[])));
        assert_eq!(agreement(&cat, &[]), 0.0);
    }
}
