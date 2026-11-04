// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use scenotime::{Revision, SceneEpoch, SceneSnapshot};

const POEM: &str = "Morning gathers light\nBranches answer night\n\nFootsteps cross the hill\nEvening settles still\n";
const LYRIC: &str =
    "Raise your open hand\nWe will take a stand\n\nCarry home the song\nLet the road run long\n";

#[test]
fn poem_and_lyric_are_two_deterministic_rosette_receipts() {
    let lexicon = CmudictPronunciations;
    for (id, text) in [("poem", POEM), ("lyric", LYRIC)] {
        let source = SourceRef::new("knot.fixture", id);
        let projection = project_rosette(source.clone(), text, &lexicon, Default::default());
        let repeated = project_rosette(source, text, &lexicon, Default::default());

        assert_eq!(projection, repeated);
        assert_eq!(
            projection.scene.items.len(),
            6,
            "four lines plus two stanzas"
        );
        assert!(projection.scene.relations.len() >= 2);
        assert!(
            projection
                .scene
                .relations
                .iter()
                .all(|relation| relation.kind.as_deref() == Some(PERFECT_RHYME))
        );
        assert!(projection.coverage.total_tokens > 0);
        assert!(projection.coverage.resolved_tokens > 0);
        assert!(projection.coverage.unresolved.len() < projection.coverage.total_tokens);
        assert_eq!(projection.meter.len(), 4);
        assert!(projection.meter.iter().all(|line| !line.beats.is_empty()));

        let first = serde_json::to_vec(&projection.scene).unwrap();
        let second = serde_json::to_vec(&repeated.scene).unwrap();
        assert_eq!(first, second);
        SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), projection.scene).unwrap();
    }
}

#[test]
fn unknown_words_are_reported_with_source_spans() {
    let projection = project_rosette(
        SourceRef::new("knot.fixture", "unknown"),
        "Known flibbertigibbet\n",
        &CmudictPronunciations,
        Default::default(),
    );
    let unresolved = projection
        .coverage
        .unresolved
        .iter()
        .find(|token| token.token == "flibbertigibbet")
        .unwrap();
    assert_eq!(unresolved.byte_start, 6);
    assert_eq!(unresolved.byte_end, 21);
    assert!(projection.scene.sources[0].id.contains("line:bytes=0..21"));
}

#[test]
fn slant_rhyme_and_meter_are_derived_without_touching_source_truth() {
    let projection = project_rosette(
        SourceRef::new("knot.fixture", "slant"),
        "A cat\nA cut\n",
        &CmudictPronunciations,
        Default::default(),
    );
    assert!(projection.scene.relations.iter().any(|relation| {
        relation.kind.as_deref() == Some(SLANT_RHYME) && relation.weight == Some(0.6)
    }));
    assert_eq!(projection.meter.len(), 2);
    assert!(
        projection
            .meter
            .iter()
            .all(|line| line.resolved_tokens == 2)
    );
    assert_eq!(projection.interiors[0].byte_start, 0);
    assert_eq!(projection.interiors[0].byte_end, 5);
}
