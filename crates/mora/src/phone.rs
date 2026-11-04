// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Phones: the language-neutral atoms every other module reads.
//!
//! A language set supplies [`Phone`] values carrying a broad [`Class`]; the
//! syllable, weight, meter, and sonance layers above are written against the
//! class alone, so a new language costs a symbol table and nothing else.

/// Broad phonological class.
///
/// Only the sonority ordering is load-bearing: syllabification reads it to find
/// legal onsets, and weight reads whether a phone is a vowel. Finer distinctions
/// (place, voicing) belong to a language's own symbol ids, not here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Vowel,
    Glide,
    Liquid,
    Nasal,
    Fricative,
    Affricate,
    Stop,
}

impl Class {
    /// Position on the sonority hierarchy; higher is more sonorous.
    pub const fn sonority(self) -> u8 {
        match self {
            Class::Vowel => 6,
            Class::Glide => 5,
            Class::Liquid => 4,
            Class::Nasal => 3,
            Class::Fricative => 2,
            Class::Affricate => 1,
            Class::Stop => 0,
        }
    }

    pub const fn is_vowel(self) -> bool {
        matches!(self, Class::Vowel)
    }
}

/// Lexical stress on a syllable's nucleus.
///
/// `None` means the language does not mark stress, which is different from
/// [`Stress::Unstressed`]: a mora-timed or quantitative reading wants the
/// former, an English lexicon supplies the latter.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Stress {
    #[default]
    None,
    Unstressed,
    Secondary,
    Primary,
}

impl Stress {
    pub const fn is_prominent(self) -> bool {
        matches!(self, Stress::Primary | Stress::Secondary)
    }
}

/// One phone in a pronunciation.
///
/// `id` is opaque and belongs to whichever symbol table produced it; comparing
/// phones across two different tables is meaningless, so a consumer keeps one
/// table per language.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Phone {
    pub id: u16,
    pub class: Class,
    /// Long or diphthongal: intrinsically bimoraic under most weight rules.
    pub long: bool,
    pub stress: Stress,
}

impl Phone {
    pub const fn new(id: u16, class: Class) -> Self {
        Self { id, class, long: false, stress: Stress::None }
    }

    pub const fn with_long(self, long: bool) -> Self {
        Self { long, ..self }
    }

    pub const fn with_stress(self, stress: Stress) -> Self {
        Self { stress, ..self }
    }

    pub const fn is_vowel(self) -> bool {
        self.class.is_vowel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonority_is_ordered_vowel_down_to_stop() {
        assert!(Class::Vowel.sonority() > Class::Glide.sonority());
        assert!(Class::Glide.sonority() > Class::Liquid.sonority());
        assert!(Class::Liquid.sonority() > Class::Nasal.sonority());
        assert!(Class::Nasal.sonority() > Class::Fricative.sonority());
        assert!(Class::Fricative.sonority() > Class::Stop.sonority());
    }

    #[test]
    fn unmarked_stress_is_not_prominent() {
        assert!(!Stress::None.is_prominent());
        assert!(!Stress::Unstressed.is_prominent());
        assert!(Stress::Secondary.is_prominent());
        assert!(Stress::Primary.is_prominent());
    }
}
