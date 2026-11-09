// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot's editor behind Graphshell's block pane (cycle breaks P2 remainder,
//! ruling 1). Graphshell ships a small default buffer and names no editor of
//! its own choosing; a host that composes a pane with Knot passes it
//! [`KnotBlockEditor::factory`], and that pane's documents edit through
//! [`KnotEditor`] (another pane in the same process may use another editor). The document tests moved here from Graphshell's
//! `block_editing.rs` and run over this editor (Graphshell keeps the same
//! tests over its default buffer).

use cambium::{CaretSelection, TextCommand, TextInput};
use graphshell::block_editing::{BlockEditor, EditOutcome, EditorFactory};
use knot_document::retained::KnotEditor;

/// [`KnotEditor`] as a block pane's buffer.
pub struct KnotBlockEditor(KnotEditor);

impl KnotBlockEditor {
    pub fn new(address: &str, source: &str) -> Self {
        Self(KnotEditor::scratch(address, source))
    }

    /// The factory a host passes to a block pane (`Pane::load_with`) or to
    /// `Document::open` so its documents edit through Knot's editor.
    pub fn factory() -> impl EditorFactory + Clone {
        KnotEditors
    }

    /// The retained editor, for readouts the pane does not draw (outline,
    /// highlights, folds).
    pub fn editor(&self) -> &KnotEditor {
        &self.0
    }
}

impl BlockEditor for KnotBlockEditor {
    fn source(&self) -> &str {
        self.0.source()
    }

    fn selection(&self) -> CaretSelection {
        self.0.selection()
    }

    fn apply(&mut self, command: TextCommand) -> EditOutcome {
        let outcome = self.0.apply(command);
        EditOutcome {
            state_changed: outcome.state_changed,
            source_changed: outcome.source_changed,
        }
    }

    fn input(&self) -> &TextInput {
        self.0.input()
    }

    fn input_mut(&mut self) -> &mut TextInput {
        self.0.input_mut()
    }

    fn is_dirty(&self) -> bool {
        self.0.is_dirty()
    }

    fn accept_saved_source(&mut self, source: &str) {
        self.0.accept_saved_source(source);
    }
}

/// What [`KnotBlockEditor::factory`] returns.
#[derive(Clone, Copy, Debug, Default)]
struct KnotEditors;

impl EditorFactory for KnotEditors {
    fn open(&self, id: &str, source: &str) -> Box<dyn BlockEditor> {
        Box::new(KnotBlockEditor::new(id, source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::{CaretAffinity, CaretPosition, CaretSelection, TextCommand};
    use graphshell::block_editing::{Change, Document, Preview};
    use illume::blocks;
    use std::collections::BTreeMap;

    /// A document over Knot's editor, as `KnotBlockEditor::factory` makes it.
    fn doc(id: &str, source: &str) -> Document {
        Document::open(id, source, &KnotBlockEditor::factory())
    }

    fn empty_changes() -> Vec<Change> {
        Vec::new()
    }

    fn attrs(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn select_bytes(document: &mut Document, start: usize, end: usize) {
        document
            .editor
            .apply(TextCommand::SetSelection(CaretSelection {
                anchor: CaretPosition {
                    byte: start,
                    affinity: CaretAffinity::Downstream,
                },
                focus: CaretPosition {
                    byte: end,
                    affinity: CaretAffinity::Upstream,
                },
            }));
    }

    #[test]
    fn text_edit_then_structural_edit_undoes_independently_and_redoes() {
        let source = "outside α\n\n{#outer owner=human}\n:::: outer\ninside 👩‍🚀\n\n{#inner keep=yes}\n::: child\nbody 漢\n:::\n::::\n";
        let mut document = doc("memory:test", source);
        document.editor.apply(TextCommand::Insert("!".into()));
        let edited_source = document.editor.source().to_owned();
        let block = document.blocks()[0].clone();
        let preview = Preview::new(&edited_source, block, empty_changes()).unwrap();
        document
            .apply(
                &preview,
                attrs(&[("id", "outer"), ("owner", "human"), ("status", "ready")]),
            )
            .unwrap();
        let structural_source = document.editor.source().to_owned();
        assert!(
            structural_source.contains("inside 👩‍🚀\n\n{#inner keep=yes}\n::: child\nbody 漢\n:::\n")
        );
        assert!(structural_source.contains("{id=\"outer\" owner=\"human\" status=\"ready\"}"));

        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), edited_source);
        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), edited_source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), structural_source);
    }

    #[test]
    fn stale_forged_and_invalid_targets_are_rejected() {
        let source = "::: card\nbody\n:::\n";
        let mut document = doc("memory:test", source);
        let block = document.blocks()[0].clone();
        let preview = Preview::new(source, block.clone(), empty_changes()).unwrap();
        document.editor.apply(TextCommand::Insert("!".into()));
        assert!(document.apply(&preview, attrs(&[("label", "x")])).is_err());

        let mut forged = block;
        forged.attributes.insert("forged".into(), "target".into());
        assert!(Preview::new(source, forged, empty_changes()).is_err());

        let mut fresh = doc("memory:test", source);
        let valid = Preview::new(source, fresh.blocks()[0].clone(), empty_changes()).unwrap();
        assert!(fresh.apply(&valid, attrs(&[("bad key", "x")])).is_err());
        assert!(
            fresh
                .apply(&valid, attrs(&[("label", "two\nlines")]))
                .is_err()
        );
    }

    #[test]
    fn no_declarations_do_not_rewrite_body_and_quote_prefix_is_kept() {
        let source = "> ::: card\n> body α\n> :::\n";
        let mut document = doc("memory:test", source);
        let preview = Preview::new(source, document.blocks()[0].clone(), empty_changes()).unwrap();
        document.apply(&preview, BTreeMap::new()).unwrap();
        assert_eq!(document.editor.source(), source);

        let preview = Preview::new(source, document.blocks()[0].clone(), empty_changes()).unwrap();
        document
            .apply(&preview, attrs(&[("label", "quoted")]))
            .unwrap();
        assert!(
            document
                .editor
                .source()
                .starts_with("> {label=\"quoted\"}\n> ::: card\n")
        );
        assert!(document.editor.source().contains("> body α\n> :::\n"));
    }

    #[test]
    fn unchanged_attributes_keep_noncanonical_source_bytes_without_undo_entry() {
        let source = "{#card  label=\"old\"}\n::: panel\nbody\n:::\n";
        let mut document = doc("memory:test", source);
        let preview = Preview::new(source, document.blocks()[0].clone(), empty_changes()).unwrap();
        let unchanged = preview.block.attributes.clone();
        document.apply(&preview, unchanged).unwrap();
        assert_eq!(document.editor.source(), source);
        assert!(!document.editor.apply(TextCommand::Undo).state_changed);
    }

    #[test]
    fn wraps_selected_unicode_lines_then_rule_edit_undoes_in_chronological_steps() {
        let source = "left α\nfirst 🧪\nsecond 漢\nright 👩‍🚀\n";
        let mut document = doc("memory:test", source);
        let start = source.find("first").unwrap();
        let end = source.find("right").unwrap();
        select_bytes(&mut document, start, end);

        document
            .wrap_selection(attrs(&[("id", "selection"), ("class", "verse")]))
            .unwrap();
        let wrapped_source = document.editor.source().to_owned();
        assert_eq!(
            wrapped_source,
            "left α\n\n{class=\"verse\" id=\"selection\"}\n:::\nfirst 🧪\nsecond 漢\n:::\n\nright 👩‍🚀\n"
        );
        let wrapped = document.blocks()[0].clone();
        assert_eq!(
            &wrapped_source[wrapped.body.clone()],
            "first 🧪\nsecond 漢\n"
        );

        let preview = Preview::new(&wrapped_source, wrapped, empty_changes()).unwrap();
        let mut rule_attributes = preview.block.attributes.clone();
        rule_attributes.insert("rule".into(), "authored".into());
        document.apply(&preview, rule_attributes).unwrap();
        let ruled_source = document.editor.source().to_owned();
        assert!(ruled_source.contains("rule=\"authored\""));

        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), wrapped_source);
        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), wrapped_source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), ruled_source);
    }

    #[test]
    fn wrapping_refuses_empty_selection_and_invalid_attrs_without_mutation() {
        let source = "one α\ntwo 漢\n";
        let mut document = doc("memory:test", source);
        let start = source.find("one").unwrap();
        select_bytes(&mut document, start, start);
        assert!(document.wrap_selection(BTreeMap::new()).is_err());
        assert_eq!(document.editor.source(), source);

        let end = source.len();
        select_bytes(&mut document, 0, end);
        let selected = document.editor.selection();
        assert!(
            document
                .wrap_selection(attrs(&[("bad key", "value")]))
                .is_err()
        );
        assert!(
            document
                .wrap_selection(attrs(&[("label", "line one\nline two")]))
                .is_err()
        );
        assert_eq!(document.editor.source(), source);
        assert_eq!(document.editor.selection(), selected);
        assert!(!document.editor.apply(TextCommand::Undo).state_changed);

        let mut blank = doc("memory:blank", "\n");
        select_bytes(&mut blank, 0, 1);
        assert!(blank.wrap_selection(BTreeMap::new()).is_err());
        assert_eq!(blank.editor.source(), "\n");

        let mut empty = doc("memory:empty", "");
        assert!(
            empty
                .wrap_selection(attrs(&[("label", "one\ntwo")]))
                .is_err()
        );
        assert_eq!(empty.editor.source(), "");
    }

    #[test]
    fn knot_and_default_documents_coexist_in_one_process() {
        use graphshell::block_editing::DefaultEditors;
        let knot = doc("memory:knot", "# One\n");
        let default = Document::open("memory:default", "# One\n", &DefaultEditors);
        assert_eq!(knot.editor.source(), default.editor.source());
        // Knot's editor is the one behind `knot`: it keeps the retained readout.
        let mut over = KnotBlockEditor::new("memory:direct", "# One\n");
        over.apply(TextCommand::Insert("\n## Two\n".into()));
        assert_eq!(over.editor().outline().len(), 2);
        // Opening a Knot document set nothing process-wide.
        assert!(!Document::new("memory:after", "x").editor.is_dirty());
    }
}
