// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Meter: feet, lines, and scansion.
//!
//! Accentual meter (English iambic pentameter) and quantitative meter (Latin
//! dactylic hexameter) are the same matcher reading a different feature of the
//! same syllables. [`Mode`] chooses which, and that is the whole difference.

use crate::phone::Phone;
use crate::syllable::Syllable;
use crate::weight::{Weight, WeightRule, weight};

/// One position in a metrical pattern.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Beat {
    Strong,
    Weak,
}

/// Which property of a syllable carries the beat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Stress carries it: the Germanic and English reading.
    Accentual,
    /// Weight carries it: the Greek and Latin reading, and the reason this
    /// crate counts morae at all.
    Quantitative,
}

/// Read the beat of every syllable under `mode`.
pub fn beats_into(
    phones: &[Phone],
    syllables: &[Syllable],
    mode: Mode,
    rule: WeightRule,
    out: &mut [Beat],
) -> Option<usize> {
    if out.len() < syllables.len() {
        return None;
    }
    for (i, s) in syllables.iter().enumerate() {
        out[i] = match mode {
            Mode::Accentual => {
                let stressed =
                    phones.get(s.nucleus).map(|p| p.stress.is_prominent()).unwrap_or(false);
                if stressed { Beat::Strong } else { Beat::Weak }
            }
            Mode::Quantitative => {
                if weight(phones, s, rule) >= Weight::Heavy { Beat::Strong } else { Beat::Weak }
            }
        };
    }
    Some(syllables.len())
}

/// Read the beat of every syllable under `mode`.
#[cfg(feature = "alloc")]
pub fn beats(
    phones: &[Phone],
    syllables: &[Syllable],
    mode: Mode,
    rule: WeightRule,
) -> alloc::vec::Vec<Beat> {
    let mut out = alloc::vec![Beat::Weak; syllables.len()];
    beats_into(phones, syllables, mode, rule, &mut out);
    out
}

/// A metrical foot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Foot {
    Iamb,
    Trochee,
    Spondee,
    Pyrrhic,
    Dactyl,
    Anapest,
    Amphibrach,
    Cretic,
}

impl Foot {
    /// The beats this foot expects.
    pub const fn pattern(self) -> &'static [Beat] {
        use Beat::{Strong as S, Weak as W};
        match self {
            Foot::Iamb => &[W, S],
            Foot::Trochee => &[S, W],
            Foot::Spondee => &[S, S],
            Foot::Pyrrhic => &[W, W],
            Foot::Dactyl => &[S, W, W],
            Foot::Anapest => &[W, W, S],
            Foot::Amphibrach => &[W, S, W],
            Foot::Cretic => &[S, W, S],
        }
    }

    /// Syllable positions in this foot.
    pub const fn positions(self) -> usize {
        self.pattern().len()
    }

    /// The feet worth trying when nothing is known about a line.
    pub const COMMON: [Foot; 4] = [Foot::Iamb, Foot::Trochee, Foot::Dactyl, Foot::Anapest];
}

/// A line's metrical shape: a foot repeated a number of times.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Meter {
    pub foot: Foot,
    pub feet: usize,
}

impl Meter {
    pub const fn new(foot: Foot, feet: usize) -> Self {
        Self { foot, feet }
    }

    /// Beats in a full line of this meter.
    pub const fn positions(&self) -> usize {
        self.foot.positions() * self.feet
    }

    /// The expected beat at `position`, wrapping through the foot.
    pub const fn beat_at(&self, position: usize) -> Beat {
        self.foot.pattern()[position % self.foot.positions()]
    }
}

/// The result of matching beats against a meter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scansion {
    pub meter: Meter,
    /// Positions that matched the expected pattern.
    pub matches: usize,
    /// Positions read, which is the shorter of the line and the meter.
    pub compared: usize,
    /// Syllables past the end of the meter, or missing from it. Positive means
    /// the line ran long, negative means it fell short (a catalectic line ends
    /// one short by design).
    pub overrun: isize,
}

impl Scansion {
    /// Share of compared positions that matched, from 0.0 to 1.0.
    pub fn fit(&self) -> f32 {
        if self.compared == 0 { 0.0 } else { self.matches as f32 / self.compared as f32 }
    }

    /// A line matching its meter at every position, with no syllables spare.
    pub fn is_regular(&self) -> bool {
        self.matches == self.compared && self.overrun == 0 && self.compared > 0
    }
}

/// Match `beats` against one meter.
pub fn scan(beats: &[Beat], meter: Meter) -> Scansion {
    let compared = beats.len().min(meter.positions());
    let matches = beats[..compared]
        .iter()
        .enumerate()
        .filter(|(i, b)| **b == meter.beat_at(*i))
        .count();

    Scansion {
        meter,
        matches,
        compared,
        overrun: beats.len() as isize - meter.positions() as isize,
    }
}

/// Try every whole-line meter that `feet` can form over `beats` and keep the
/// best fit.
///
/// Substitution is expected rather than punished: a line of iambic pentameter
/// with a trochaic first foot still scans as iambic pentameter, and shows up
/// here as a high but imperfect fit.
pub fn scan_best(beats: &[Beat], feet: &[Foot]) -> Option<Scansion> {
    let mut best: Option<Scansion> = None;

    for &foot in feet {
        let n = beats.len().div_ceil(foot.positions()).max(1);
        // Consider the line rounded both down and up to whole feet, so a
        // catalectic line still finds its meter.
        for feet_count in [beats.len() / foot.positions(), n] {
            if feet_count == 0 {
                continue;
            }
            let s = scan(beats, Meter::new(foot, feet_count));
            let better = match best {
                None => true,
                Some(b) => {
                    (s.fit(), -s.overrun.abs()) > (b.fit(), -b.overrun.abs())
                }
            };
            if better {
                best = Some(s);
            }
        }
    }

    best
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use crate::phone::{Class, Stress};
    use crate::syllable::{SyllableRule, syllabify};

    use Beat::{Strong as S, Weak as W};

    #[test]
    fn iambic_pentameter_scans_regular() {
        let line = [W, S, W, S, W, S, W, S, W, S];
        let s = scan(&line, Meter::new(Foot::Iamb, 5));
        assert!(s.is_regular());
        assert_eq!(s.fit(), 1.0);
    }

    #[test]
    fn a_trochaic_first_foot_is_a_substitution_not_a_different_meter() {
        // The classic inversion: strong-weak, then iambic to the end.
        let line = [S, W, W, S, W, S, W, S, W, S];
        let best = scan_best(&line, &Foot::COMMON).unwrap();
        assert_eq!(best.meter.foot, Foot::Iamb);
        assert_eq!(best.meter.feet, 5);
        assert!(best.fit() > 0.7, "fit was {}", best.fit());
        assert!(!best.is_regular());
    }

    #[test]
    fn dactylic_hexameter_scans_regular() {
        let mut line = [W; 18];
        for i in (0..18).step_by(3) {
            line[i] = S;
        }
        let s = scan(&line, Meter::new(Foot::Dactyl, 6));
        assert!(s.is_regular());
    }

    #[test]
    fn a_catalectic_line_reports_its_missing_syllable() {
        // Trochaic tetrameter catalectic: seven syllables, not eight.
        let line = [S, W, S, W, S, W, S];
        let s = scan(&line, Meter::new(Foot::Trochee, 4));
        assert_eq!(s.overrun, -1);
        assert_eq!(s.matches, s.compared);
    }

    #[test]
    fn the_two_modes_read_the_same_syllables_differently() {
        // Two syllables: the first light but stressed, the second heavy but
        // unstressed. Accentual and quantitative readings must disagree.
        let w = [
            Phone::new(1, Class::Stop),
            Phone::new(2, Class::Vowel).with_stress(Stress::Primary),
            Phone::new(3, Class::Stop),
            Phone::new(4, Class::Vowel).with_long(true).with_stress(Stress::Unstressed),
        ];
        let s = syllabify(&w, SyllableRule::ENGLISH);
        assert_eq!(s.len(), 2);

        let accentual = beats(&w, &s, Mode::Accentual, WeightRule::CLASSICAL);
        let quantitative = beats(&w, &s, Mode::Quantitative, WeightRule::CLASSICAL);
        assert_eq!(accentual, [S, W]);
        assert_eq!(quantitative, [W, S]);
    }

    #[test]
    fn scanning_an_empty_line_yields_nothing() {
        assert!(scan_best(&[], &Foot::COMMON).is_none() || scan_best(&[], &Foot::COMMON).unwrap().compared == 0);
    }
}
