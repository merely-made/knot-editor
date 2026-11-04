// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A prosody engine, built on the mora.
//!
//! `mora` turns pronunciations into the units verse is measured in: syllables,
//! their weight in morae, the metrical shape of a line, and the sound kinship
//! between words. It reads phones and knows nothing about spelling.
//!
//! # The layering, and why it generalizes
//!
//! Languages are usually sorted into stress-timed (English), syllable-timed
//! (Spanish), and mora-timed (Japanese) types. The mora sits underneath all
//! three: syllable weight is counted in morae, weight-sensitive stress rules
//! read that count, and quantitative meter is moraic outright. So only the
//! bottom layer here is language-specific:
//!
//! | layer | module | per-language cost |
//! |---|---|---|
//! | phones | [`phone`] | a symbol table |
//! | syllables | [`syllable`] | a [`syllable::SyllableRule`] |
//! | weight | [`weight`] | a [`weight::WeightRule`] |
//! | meter | [`meter`] | nothing |
//! | sound kinship | [`sonance`] | nothing |
//!
//! English ships as [`english`]: a table and two constants. A second language
//! costs the same.
//!
//! No pronunciation lexicon is bundled. Pronunciations are large, separately
//! licensed data; [`english::cmudict_entry`] parses the format CMUdict
//! publishes and leaves storage to the consumer.
//!
//! # Example
//!
//! ```
//! use mora::english::{SYLLABLE_RULE, WEIGHT_RULE, pronounce};
//! use mora::meter::{Foot, Mode, beats, scan_best};
//! use mora::sonance::is_perfect_rhyme;
//! use mora::syllable::syllabify;
//!
//! let cat = pronounce("K AE1 T").unwrap();
//! let hat = pronounce("HH AE1 T").unwrap();
//! let cat_syllables = syllabify(&cat, SYLLABLE_RULE);
//! let hat_syllables = syllabify(&hat, SYLLABLE_RULE);
//!
//! assert!(is_perfect_rhyme((&cat, &cat_syllables), (&hat, &hat_syllables)));
//!
//! // "the CAT is HERE" scans as two iambs.
//! let line = pronounce("DH AH0 K AE1 T IH0 Z HH IY1 R").unwrap();
//! let syllables = syllabify(&line, SYLLABLE_RULE);
//! let line_beats = beats(&line, &syllables, Mode::Accentual, WEIGHT_RULE);
//! let scansion = scan_best(&line_beats, &Foot::COMMON).unwrap();
//! assert_eq!(scansion.meter.foot, Foot::Iamb);
//! assert_eq!(scansion.meter.feet, 2);
//! ```
//!
//! # Boundaries
//!
//! # Canonical sound-kinship API
//!
//! Sound kinship is part of this crate: use [`sonance`] as
//! `mora::sonance`. The separately named `sonance` repository is an archived
//! pointer, not a second implementation or dependency surface.
//!
//! # Boundaries
//!
//! `mora` measures sound. It is not a phonemizer (bring pronunciations), not a
//! semantic analyzer, and not a rhyming dictionary: it answers whether two
//! given pronunciations rhyme, not which words in a language do.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod meter;
pub mod phone;
/// Sound kinship between pronunciations. This is the canonical Sonance API.
pub mod sonance;
pub mod syllable;
pub mod weight;

#[cfg(feature = "english")]
pub mod english;

pub use phone::{Class, Phone, Stress};
pub use syllable::{Syllable, SyllableRule};
pub use weight::{Weight, WeightRule};
