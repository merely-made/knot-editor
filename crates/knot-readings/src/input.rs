// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The typed snapshot a reading runs over. Everything a script can see is in
//! here, filled by the host before the engine exists; the bindings read this
//! and nothing else.

use knot_document::{KnotDocumentSession, KnotFoldItemV1, KnotOutlineItemV1};

/// Which source a reading ran against, and at what content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingSourceIdV1 {
    pub address: String,
    /// blake3 of the exact source text the reading saw.
    pub hash: [u8; 32],
    /// The document head this source belongs to, when the host knows one.
    pub revision: Option<[u8; 32]>,
}

/// One inline link with the byte span of its source syntax, when the scan
/// found it. `span` is never guessed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingLinkV1 {
    pub target: String,
    pub rel: Option<String>,
    pub text: String,
    pub span: Option<(usize, usize)>,
}

/// One visible assertion, as the host's relation store presents it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingRelationV1 {
    pub id: [u8; 32],
    pub author: [u8; 32],
    pub predicate: String,
    pub subject: ReadingEndpointV1,
    pub object: ReadingEndpointV1,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingEndpointV1 {
    pub document_id: String,
    pub quote: String,
    pub position: Option<(usize, usize)>,
}

/// The whole readable world of one reading run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingInput {
    pub source: ReadingSourceIdV1,
    pub format: String,
    pub text: String,
    pub selection: Option<(usize, usize)>,
    pub outline: Vec<KnotOutlineItemV1>,
    pub folds: Vec<KnotFoldItemV1>,
    pub links: Vec<ReadingLinkV1>,
    pub relations: Vec<ReadingRelationV1>,
}

impl ReadingInput {
    /// Take one point-in-time reading input from a live session. This reads the
    /// session's snapshots only; it changes nothing about the document.
    pub fn from_session(session: &KnotDocumentSession) -> Result<Self, String> {
        let snapshot = session.snapshot();
        let outline = session.outline_snapshot();
        let folds = session.fold_snapshot();
        let preview = session.preview_snapshot()?;
        let selection = {
            let selection = snapshot.selection;
            let (start, end) = (
                selection.anchor.byte.min(selection.focus.byte),
                selection.anchor.byte.max(selection.focus.byte),
            );
            (end <= snapshot.text.len()).then_some((start, end))
        };
        Ok(Self {
            source: ReadingSourceIdV1 {
                address: snapshot.source.address.clone(),
                hash: blake3::hash(snapshot.text.as_bytes()).into(),
                revision: None,
            },
            format: format!("{:?}", snapshot.format).to_lowercase(),
            links: crate::links::extract(&snapshot.text, &preview.document),
            text: snapshot.text,
            selection,
            outline: outline.items,
            folds: folds.items,
            relations: Vec::new(),
        })
    }

    /// Attach the assertions the caller's relation authority admitted. Access
    /// filtering happens before this call, never inside a binding.
    pub fn with_relations(mut self, relations: Vec<ReadingRelationV1>) -> Self {
        self.relations = relations;
        self
    }

    /// A bare input over loose text, for hosts with no session (and for tests).
    pub fn from_text(
        address: impl Into<String>,
        format: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        let text = text.into();
        Self {
            source: ReadingSourceIdV1 {
                address: address.into(),
                hash: blake3::hash(text.as_bytes()).into(),
                revision: None,
            },
            format: format.into(),
            text,
            selection: None,
            outline: Vec::new(),
            folds: Vec::new(),
            links: Vec::new(),
            relations: Vec::new(),
        }
    }
}
