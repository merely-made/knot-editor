// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The sync host's residue verdict (vault lock ruling 59), apart from the
//! instrument so it can be tested alone.
//!
//! Judged: a seed-bearing block, `(live, size)`, present in every host run
//! and in no baseline run. Everything else the host runs saw is unjudged,
//! never dropped: size and presence cannot attribute it (Q052).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// One armed run: the sizes of blocks freed uncleared, and of blocks still
/// live at disarm, that held the seed.
pub struct Run {
    pub freed: Vec<usize>,
    pub live: Vec<usize>,
    /// The measured future's size: a block exactly this size is the run's
    /// own outer frame, whatever the nesting made it (ruling 59).
    pub frame: usize,
}

/// A run's outer frame, compared across runs by presence, not size.
pub const OUTER_FRAME: usize = usize::MAX;

/// Copies per `(live, size)` in one run, the outer frame folded to one key.
fn counts(run: &Run) -> BTreeMap<(bool, usize), usize> {
    let outer = |size: usize| if size == run.frame { OUTER_FRAME } else { size };
    let mut counts = BTreeMap::new();
    let blocks = run.freed.iter().map(|&size| (false, outer(size)));
    for key in blocks.chain(run.live.iter().map(|&size| (true, outer(size)))) {
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

/// Blocks, `(live, size)`, in every run and in no baseline run.
pub fn own_blocks(runs: &[Run], baseline: &[&[Run]]) -> BTreeSet<(bool, usize)> {
    let keys = |run: &Run| counts(run).into_keys().collect::<BTreeSet<_>>();
    let seen: BTreeSet<(bool, usize)> = baseline
        .iter()
        .flat_map(|runs| runs.iter())
        .flat_map(keys)
        .collect();
    let mut always = keys(&runs[0]);
    for run in &runs[1..] {
        always = always.intersection(&keys(run)).copied().collect();
    }
    always.difference(&seen).copied().collect()
}

/// Why a host block was seen but not judged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// In some host runs, not all, and in no baseline run.
    Intermittent,
    /// A baseline run shows a block of this size too; `host_copies` and
    /// `baseline_copies` are the most copies any one run of each held.
    BaselineSize {
        host_copies: usize,
        baseline_copies: usize,
    },
}

/// A host block the verdict does not judge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unjudged {
    pub live: bool,
    pub size: usize,
    /// How many host runs saw it.
    pub runs: usize,
    pub reason: Reason,
}

impl fmt::Display for Unjudged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = if self.live { "live" } else { "freed uncleared" };
        let size = if self.size == OUTER_FRAME {
            "outer frame".to_string()
        } else {
            self.size.to_string()
        };
        write!(f, "{kind} {size} in {} host run(s): ", self.runs)?;
        match &self.reason {
            Reason::Intermittent => write!(f, "intermittent, in no baseline"),
            Reason::BaselineSize {
                host_copies,
                baseline_copies,
            } => write!(
                f,
                "baseline size, up to {host_copies} host copies against {baseline_copies}"
            ),
        }
    }
}

/// Every host block `own_blocks` does not judge, with the reason.
pub fn unjudged(runs: &[Run], baseline: &[&[Run]]) -> Vec<Unjudged> {
    let own = own_blocks(runs, baseline);
    let most = |runs: &mut dyn Iterator<Item = &Run>| {
        let mut most: BTreeMap<(bool, usize), (usize, usize)> = BTreeMap::new();
        for run in runs {
            for (key, copies) in counts(run) {
                let entry = most.entry(key).or_insert((0, 0));
                entry.0 += 1;
                entry.1 = entry.1.max(copies);
            }
        }
        most
    };
    let base = most(&mut baseline.iter().flat_map(|runs| runs.iter()));
    most(&mut runs.iter())
        .into_iter()
        .filter(|(key, _)| !own.contains(key))
        .map(|((live, size), (seen_in, host_copies))| Unjudged {
            live,
            size,
            runs: seen_in,
            reason: match base.get(&(live, size)) {
                Some(&(_, baseline_copies)) => Reason::BaselineSize {
                    host_copies,
                    baseline_copies,
                },
                None => Reason::Intermittent,
            },
        })
        .collect()
}
