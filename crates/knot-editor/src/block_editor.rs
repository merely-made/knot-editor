// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot's editor behind a block pane (cycle breaks P2 remainder, ruling 1; engine
//! split D73). Cambium owns the [`BlockEditor`] trait, messages only; a host that
//! composes a pane with Knot passes it [`KnotBlockEditor::factory`], and that
//! pane's documents edit through [`KnotEditor`] (another pane in the same process
//! may use another editor). The pane owns its own text widget and mirrors this
//! editor through `source` and `selection`. The document tests that ran here over
//! Graphshell's `Document` live with the pane in Mere, over its default buffer.

use cambium::{BlockEditor, CaretSelection, EditOutcome, EditorFactory, TextCommand};
use knot_document::retained::KnotEditor;

/// [`KnotEditor`] as a block pane's buffer.
pub struct KnotBlockEditor(KnotEditor);

impl KnotBlockEditor {
    pub fn new(address: &str, source: &str) -> Self {
        Self(KnotEditor::scratch(address, source))
    }

    /// The factory a host passes to a block pane (`Pane::load_with`) so its
    /// documents edit through Knot's editor.
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
    use cambium::{CaretAffinity, CaretPosition};

    fn open(id: &str, source: &str) -> Box<dyn BlockEditor> {
        KnotBlockEditor::factory().open(id, source)
    }

    fn select_bytes(editor: &mut dyn BlockEditor, start: usize, end: usize) {
        editor.apply(TextCommand::SetSelection(CaretSelection {
            anchor: CaretPosition { byte: start, affinity: CaretAffinity::Downstream },
            focus: CaretPosition { byte: end, affinity: CaretAffinity::Upstream },
        }));
    }

    #[test]
    fn the_factory_opens_a_clean_editor_over_the_source() {
        let editor = open("memory:test", "# One α
");
        assert_eq!(editor.source(), "# One α
");
        assert!(!editor.is_dirty());
    }

    #[test]
    fn edits_undo_and_redo_through_the_messages_alone() {
        let source = "outside α

::: card
inside 👩‍🚀
:::
";
        let mut editor = open("memory:test", source);
        let outcome = editor.apply(TextCommand::Insert("!".into()));
        assert!(outcome.state_changed && outcome.source_changed);
        let edited = editor.source().to_owned();
        assert_ne!(edited, source);
        assert!(editor.is_dirty());

        // A replacement over a selection is one undo step.
        let start = edited.find("inside").unwrap();
        let end = start + "inside".len();
        select_bytes(editor.as_mut(), start, end);
        assert_eq!(editor.selection().anchor.byte, start);
        editor.apply(TextCommand::Insert("within".into()));
        let replaced = editor.source().to_owned();
        assert!(replaced.contains("within 👩‍🚀"));

        assert!(editor.apply(TextCommand::Undo).source_changed);
        assert_eq!(editor.source(), edited);
        editor.apply(TextCommand::Undo);
        assert_eq!(editor.source(), source);
        assert!(!editor.is_dirty());
        editor.apply(TextCommand::Redo);
        editor.apply(TextCommand::Redo);
        assert_eq!(editor.source(), replaced);
    }

    #[test]
    fn a_selection_change_is_state_not_source_and_makes_no_undo_entry() {
        let mut editor = open("memory:test", "one α
two 漢
");
        let outcome = editor.apply(TextCommand::SelectAll);
        assert!(!outcome.source_changed);
        assert!(!editor.apply(TextCommand::Undo).state_changed);
        assert_eq!(editor.source(), "one α
two 漢
");
    }

    #[test]
    fn accepting_a_saved_source_cleans_it_and_later_edits_stay_dirty() {
        let mut editor = open("memory:test", "a");
        editor.apply(TextCommand::Insert("b".into()));
        let saved = editor.source().to_owned();
        assert!(editor.is_dirty());
        editor.accept_saved_source(&saved);
        assert!(!editor.is_dirty());
        editor.apply(TextCommand::Insert("c".into()));
        assert!(editor.is_dirty());
    }

    #[test]
    fn knot_and_other_editors_coexist_in_one_process() {
        let mut knot = KnotBlockEditor::new("memory:direct", "# One
");
        knot.apply(TextCommand::Insert("
## Two
".into()));
        // Knot's editor keeps the retained readout the pane does not draw.
        assert_eq!(knot.editor().outline().len(), 2);
        // Opening one set nothing process-wide.
        assert!(!open("memory:after", "x").is_dirty());
    }
}
