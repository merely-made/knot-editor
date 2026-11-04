// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! English: the ARPAbet phone set, and parsing for CMUdict-shaped input.
//!
//! This is the only English-specific module in the crate, and it is a table.
//! Everything above it (syllables, weight, meter, sonance) is language-neutral,
//! so a second language costs another table and nothing more.
//!
//! No lexicon ships here. Pronunciations are data, often large and separately
//! licensed; this module parses the format CMUdict publishes and leaves the
//! storage to the consumer.

use crate::phone::{Class, Phone, Stress};
use crate::syllable::SyllableRule;
use crate::weight::WeightRule;

/// Syllabification parameters for English.
pub const SYLLABLE_RULE: SyllableRule = SyllableRule::ENGLISH;

/// English stress is weight-sensitive, so the classical reading applies.
pub const WEIGHT_RULE: WeightRule = WeightRule::CLASSICAL;

/// The ARPAbet phone set: symbol, class, and whether the phone is long or
/// diphthongal (and so intrinsically bimoraic).
///
/// A phone's `id` is its index in this table.
pub const ARPABET: [(&str, Class, bool); 39] = [
    ("AA", Class::Vowel, true),
    ("AE", Class::Vowel, false),
    ("AH", Class::Vowel, false),
    ("AO", Class::Vowel, true),
    ("AW", Class::Vowel, true),
    ("AY", Class::Vowel, true),
    ("B", Class::Stop, false),
    ("CH", Class::Affricate, false),
    ("D", Class::Stop, false),
    ("DH", Class::Fricative, false),
    ("EH", Class::Vowel, false),
    ("ER", Class::Vowel, true),
    ("EY", Class::Vowel, true),
    ("F", Class::Fricative, false),
    ("G", Class::Stop, false),
    ("HH", Class::Fricative, false),
    ("IH", Class::Vowel, false),
    ("IY", Class::Vowel, true),
    ("JH", Class::Affricate, false),
    ("K", Class::Stop, false),
    ("L", Class::Liquid, false),
    ("M", Class::Nasal, false),
    ("N", Class::Nasal, false),
    ("NG", Class::Nasal, false),
    ("OW", Class::Vowel, true),
    ("OY", Class::Vowel, true),
    ("P", Class::Stop, false),
    ("R", Class::Liquid, false),
    ("S", Class::Fricative, false),
    ("SH", Class::Fricative, false),
    ("T", Class::Stop, false),
    ("TH", Class::Fricative, false),
    ("UH", Class::Vowel, false),
    ("UW", Class::Vowel, true),
    ("V", Class::Fricative, false),
    ("W", Class::Glide, false),
    ("Y", Class::Glide, false),
    ("Z", Class::Fricative, false),
    ("ZH", Class::Fricative, false),
];

/// The symbol for a phone id, if the id is in the table.
pub fn symbol(id: u16) -> Option<&'static str> {
    ARPABET.get(id as usize).map(|e| e.0)
}

/// Parse one ARPAbet symbol, with or without a trailing stress digit.
///
/// `"AE1"` is a primary-stressed vowel, `"AE0"` unstressed, `"T"` a consonant.
pub fn phone(symbol: &str) -> Option<Phone> {
    let (base, stress) = match symbol.as_bytes().last() {
        Some(b'0') => (&symbol[..symbol.len() - 1], Stress::Unstressed),
        Some(b'1') => (&symbol[..symbol.len() - 1], Stress::Primary),
        Some(b'2') => (&symbol[..symbol.len() - 1], Stress::Secondary),
        _ => (symbol, Stress::None),
    };

    let id = ARPABET.iter().position(|e| e.0.eq_ignore_ascii_case(base))?;
    let (_, class, long) = ARPABET[id];
    Some(Phone::new(id as u16, class).with_long(long).with_stress(stress))
}

/// Parse a whitespace-separated pronunciation into `out`.
///
/// Returns the number of phones written; `None` if a symbol is unrecognized or
/// `out` is too short.
pub fn pronounce_into(pronunciation: &str, out: &mut [Phone]) -> Option<usize> {
    let mut n = 0;
    for symbol in pronunciation.split_whitespace() {
        if n >= out.len() {
            return None;
        }
        out[n] = phone(symbol)?;
        n += 1;
    }
    Some(n)
}

/// Parse a whitespace-separated pronunciation, e.g. `"K AE1 T"`.
#[cfg(feature = "alloc")]
pub fn pronounce(pronunciation: &str) -> Option<alloc::vec::Vec<Phone>> {
    pronunciation.split_whitespace().map(phone).collect()
}

/// Split one CMUdict line into its word and its pronunciation.
///
/// Comment lines (`;;;`) and blanks yield `None`. A variant marker is stripped,
/// so `"READ(2)  R EH1 D"` gives `("READ", "R EH1 D")`.
pub fn cmudict_entry(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_end();
    if line.is_empty() || line.starts_with(";;;") {
        return None;
    }

    let split = line.find("  ").or_else(|| line.find(' '))?;
    let (word, rest) = line.split_at(split);
    let pronunciation = rest.trim_start();
    if word.is_empty() || pronunciation.is_empty() {
        return None;
    }

    let word = match word.find('(') {
        Some(i) if word.ends_with(')') => &word[..i],
        _ => word,
    };
    Some((word, pronunciation))
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use crate::meter::{Foot, Mode, beats, scan_best};
    use crate::sonance::is_perfect_rhyme;
    use crate::syllable::syllabify;
    use crate::weight::{Weight, total_morae, weight};

    #[test]
    fn stress_digits_parse_and_the_table_round_trips() {
        let p = phone("AE1").unwrap();
        assert_eq!(p.stress, Stress::Primary);
        assert_eq!(p.class, Class::Vowel);
        assert_eq!(symbol(p.id), Some("AE"));
        assert!(!p.long);

        assert_eq!(phone("IY0").unwrap().stress, Stress::Unstressed);
        assert!(phone("IY0").unwrap().long, "IY is a long vowel");
        assert_eq!(phone("T").unwrap().stress, Stress::None);
        assert!(phone("QQ").is_none());
    }

    #[test]
    fn cmudict_lines_split_and_variants_are_stripped() {
        assert_eq!(cmudict_entry("CAT  K AE1 T"), Some(("CAT", "K AE1 T")));
        assert_eq!(cmudict_entry("READ(2)  R EH1 D"), Some(("READ", "R EH1 D")));
        assert_eq!(cmudict_entry(";;; comment"), None);
        assert_eq!(cmudict_entry(""), None);
    }

    #[test]
    fn syllabifies_a_real_word() {
        // "poetry": P OW1 AH0 T R IY0
        let w = pronounce("P OW1 AH0 T R IY0").unwrap();
        let s = syllabify(&w, SYLLABLE_RULE);
        assert_eq!(s.len(), 3, "three vowels, three syllables");
        // "tr" is a legal rising onset, so it opens the last syllable.
        assert_eq!(s[2].onset_len(), 2);
        assert_eq!(s[1].coda_len(), 0);
    }

    #[test]
    fn weight_reads_a_long_vowel_and_a_closed_syllable() {
        // "beat": B IY1 T — long nucleus and a coda, so superheavy.
        let w = pronounce("B IY1 T").unwrap();
        let s = syllabify(&w, SYLLABLE_RULE);
        assert_eq!(weight(&w, &s[0], WEIGHT_RULE), Weight::Superheavy);

        // "bit": B IH1 T — short nucleus, closed: heavy.
        let w2 = pronounce("B IH1 T").unwrap();
        let s2 = syllabify(&w2, SYLLABLE_RULE);
        assert_eq!(weight(&w2, &s2[0], WEIGHT_RULE), Weight::Heavy);
    }

    #[test]
    fn haiku_counts_morae_not_syllables() {
        // "Tokyo": T OW1 K IY0 OW0 — three syllables in English reading, but
        // the long vowels carry more morae, which is the counting difference
        // that makes English-syllable haiku come out short.
        let w = pronounce("T OW1 K IY0 OW0").unwrap();
        let s = syllabify(&w, SYLLABLE_RULE);
        assert_eq!(s.len(), 3);
        assert!(total_morae(&w, &s, WeightRule::MORAIC) > s.len() as u32);
    }

    #[test]
    fn rhyme_holds_across_real_pronunciations() {
        let cat = pronounce("K AE1 T").unwrap();
        let hat = pronounce("HH AE1 T").unwrap();
        let dog = pronounce("D AO1 G").unwrap();
        let (sc, sh, sd) = (
            syllabify(&cat, SYLLABLE_RULE),
            syllabify(&hat, SYLLABLE_RULE),
            syllabify(&dog, SYLLABLE_RULE),
        );
        assert!(is_perfect_rhyme((&cat, &sc), (&hat, &sh)));
        assert!(!is_perfect_rhyme((&cat, &sc), (&dog, &sd)));
    }

    #[test]
    fn an_iambic_phrase_scans_as_iambic() {
        // "the CAT is HERE": weak-strong, weak-strong.
        let phrase = pronounce("DH AH0 K AE1 T IH0 Z HH IY1 R").unwrap();
        let s = syllabify(&phrase, SYLLABLE_RULE);
        let b = beats(&phrase, &s, Mode::Accentual, WEIGHT_RULE);
        let scansion = scan_best(&b, &Foot::COMMON).unwrap();
        assert_eq!(scansion.meter.foot, Foot::Iamb);
        assert!(scansion.is_regular(), "beats were {b:?}");
    }

    #[test]
    fn the_into_form_needs_no_allocator() {
        let mut buf = [Phone::new(0, Class::Stop); 8];
        let n = pronounce_into("K AE1 T", &mut buf).unwrap();
        assert_eq!(n, 3);
        assert!(pronounce_into("K AE1 T", &mut buf[..2]).is_none());
    }
}
