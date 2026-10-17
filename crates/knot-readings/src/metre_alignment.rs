// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Authored metrical positions over a copied, resolved lexical reading.
//! These edits never rewrite document bytes, stress, pronunciations or rhyme.
use scenomise::{
    backdrops::{SceneFacts, SceneInput},
    template::Span,
};

pub use scenomise::span_edit::{SpanEdit as Edit, SpanEditIntent as Proposal};
#[derive(Clone)]
pub struct MetreDraft {
    pub scene: SceneInput,
    origin: String,
    undo: Vec<SceneInput>,
    redo: Vec<SceneInput>,
}
impl MetreDraft {
    pub fn new(scene: SceneInput) -> Result<Self, String> {
        scene.validate().map_err(|e| e.to_string())?;
        if !matches!(scene.facts, SceneFacts::MetreGrid(_)) {
            return Err("A metre reading is required".into());
        }
        Ok(Self {
            origin: scene.revision.clone(),
            scene,
            undo: vec![],
            redo: vec![],
        })
    }
    fn indices(&self, key: &str) -> Option<(usize, usize)> {
        let SceneFacts::MetreGrid(grid) = &self.scene.facts else {
            return None;
        };
        grid.lines.iter().enumerate().find_map(|(row, line)| {
            (0..line.syllables.len())
                .find(|i| key == format!("{}/syllable/{i}", line.key))
                .map(|i| (row, i))
        })
    }
    pub fn span(&self, key: &str) -> Option<Span> {
        let (row, index) = self.indices(key)?;
        let SceneFacts::MetreGrid(grid) = &self.scene.facts else {
            return None;
        };
        grid.placement(row, index)
    }
    pub fn extent(&self) -> u32 {
        let SceneFacts::MetreGrid(grid) = &self.scene.facts else {
            unreachable!()
        };
        grid.extent()
    }
    pub fn propose(&self, key: &str, edit: Edit, delta: f32) -> Result<Proposal, String> {
        Proposal::propose(&self.scene, key, edit, delta, 1)
    }
    pub fn preview(&self, proposal: &Proposal) -> Result<SceneInput, String> {
        let facts = proposal.proposed_facts(&self.scene)?;
        if proposal.before == proposal.after {
            return Ok(self.scene.clone());
        }
        let mut next = self.scene.clone();
        next.facts = facts;
        next.revision = blake3::hash(
            &serde_json::to_vec(&(&self.origin, &next.facts)).map_err(|e| e.to_string())?,
        )
        .to_hex()
        .to_string();
        next.validate().map_err(|e| e.to_string())?;
        Ok(next)
    }
    pub fn commit(&mut self, proposal: &Proposal) -> Result<(), String> {
        let next = self.preview(proposal)?;
        if proposal.before != proposal.after {
            self.undo.push(self.scene.clone());
            if self.undo.len() > 32 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.scene = next;
        }
        Ok(())
    }
    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.scene, previous));
        }
    }
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.scene, next));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        metre_scene::{MetreTemplate, metre_scene},
        sound::{self, SoundLayers},
    };
    use std::collections::BTreeMap;
    fn fixture() -> MetreDraft {
        let text = "the night\nthe light";
        let reading = sound::analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        MetreDraft::new(
            metre_scene(
                text,
                &reading,
                sceno::SourceRef {
                    adapter: "knot.document".into(),
                    id: "file:///verse.djot".into(),
                },
                &MetreTemplate {
                    feet: 3,
                    positions_per_foot: 2,
                    rhyme_groups: vec![Some("A".into()), Some("A".into())],
                },
            )
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn positional_edits_preserve_source_stress_and_rhyme_and_one_step_history() {
        let mut d = fixture();
        let before = d.scene.clone();
        let key = "bytes-0-9/syllable/1";
        for (edit, delta, expected) in [
            (
                Edit::Move,
                1.0,
                Span {
                    start: 2,
                    length: 1,
                },
            ),
            (
                Edit::End,
                2.0,
                Span {
                    start: 2,
                    length: 3,
                },
            ),
            (
                Edit::Start,
                -1.0,
                Span {
                    start: 1,
                    length: 4,
                },
            ),
        ] {
            let p = d.propose(key, edit, delta).unwrap();
            assert_eq!(p.after, expected);
            let preview = d.preview(&p).unwrap();
            assert_ne!(preview, d.scene);
            d.commit(&p).unwrap();
            assert_eq!(d.span(key), Some(expected));
        }
        let SceneFacts::MetreGrid(a) = &before.facts else {
            unreachable!()
        };
        let SceneFacts::MetreGrid(b) = &d.scene.facts else {
            unreachable!()
        };
        for (a, b) in a.lines.iter().zip(&b.lines) {
            assert_eq!(a.syllables, b.syllables);
            assert_eq!(a.source, b.source);
            assert_eq!(a.rhyme_tail, b.rhyme_tail);
            assert_eq!(a.rhyme_group, b.rhyme_group);
        }
        assert_eq!(before.source, d.scene.source);
        d.undo();
        assert_eq!(
            d.span(key),
            Some(Span {
                start: 2,
                length: 3
            })
        );
        d.redo();
        assert_eq!(
            d.span(key),
            Some(Span {
                start: 1,
                length: 4
            })
        );
        let json = serde_json::to_string(&d.scene).unwrap();
        assert_eq!(SceneInput::parse(&json).unwrap(), d.scene);
    }
    #[test]
    fn stale_forged_and_invalid_proposals_are_atomic_refusals() {
        let mut d = fixture();
        let key = "bytes-0-9/syllable/1";
        let p = d.propose(key, Edit::Move, 1.0).unwrap();
        for cause in 0..5 {
            let mut forged = p.clone();
            match cause {
                0 => forged.revision.push('x'),
                1 => forged.source.id.push('x'),
                2 => forged.occurrence = "missing".into(),
                3 => forged.after.length = 0,
                _ => forged.after.start = u32::MAX,
            };
            let old = d.scene.clone();
            assert!(d.commit(&forged).is_err());
            assert_eq!(d.scene, old);
        }
        d.commit(&p).unwrap();
        let old = d.scene.clone();
        assert!(d.commit(&p).is_err());
        assert_eq!(d.scene, old);
        assert!(d.propose(key, Edit::Move, f32::NAN).is_err());
    }
}
