// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::{
    DocumentFormat, KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentSession,
    KnotDocumentSnapshotV1, KnotDocumentSourceKindV1, KnotDocumentWritePostureV1,
};
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, RunnerSurfaceSession, TextInput,
    button, div, el, lens, span, textarea_typed,
};
#[cfg(feature = "highlight")]
use cambium::{Highlight, highlighted_textarea};
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape,
};

pub const KNOT_DOCUMENT_CSS: &str = ".knot-document { display: flex; flex-direction: column; gap: 8px; } .knot-document-status { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; } .knot-document-status-item { white-space: nowrap; } .knot-document-save { margin-left: auto; } .knot-document-body { min-height: 240px; white-space: pre-wrap; } .knot-document-read-only { cursor: default; user-select: text; }";
pub struct KnotDocumentSurfaceState {
    session: KnotDocumentSession,
}
impl KnotDocumentSurfaceState {
    pub fn new(session: KnotDocumentSession) -> Self {
        Self { session }
    }
    pub fn session(&self) -> &KnotDocumentSession {
        &self.session
    }
    pub fn session_mut(&mut self) -> &mut KnotDocumentSession {
        &mut self.session
    }
    fn input_mut_for_editable_view(&mut self) -> &mut TextInput {
        self.session.input_mut_for_editable_view()
    }
    pub fn snapshot(&self) -> KnotDocumentSnapshotV1 {
        self.session.snapshot()
    }
    pub fn apply(
        &mut self,
        intent: KnotDocumentIntentV1,
    ) -> Result<KnotDocumentSnapshotV1, KnotDocumentIntentErrorV1> {
        self.session.apply(intent)
    }
}
pub type KnotDocumentView = Box<dyn AnyView<KnotDocumentSurfaceState, (), GenetCtx, GenetElement>>;
pub fn knot_document_view(state: &KnotDocumentSurfaceState) -> KnotDocumentView {
    knot_document_view_with_highlighting(state, false)
}

/// Who draws the document's status: the surface, above its text, or the host
/// around it (the desktop's status bar).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KnotDocumentStatus {
    #[default]
    Embedded,
    /// No status row and no Save control; the host shows those facts.
    HostOwned,
}

/// Builds the document surface with optional Djot/Knot source highlighting.
///
/// The editable branch always lenses the session's one [`TextInput`]. When the
/// `highlight` feature is unavailable, or the source is not Djot/Knot, `enabled`
/// retains the ordinary textarea projection.
pub fn knot_document_view_with_highlighting(
    state: &KnotDocumentSurfaceState,
    enabled: bool,
) -> KnotDocumentView {
    knot_document_view_with_status(state, enabled, KnotDocumentStatus::Embedded)
}

/// [`knot_document_view_with_highlighting`], with the status row and its Save
/// control drawn only when `status` is [`KnotDocumentStatus::Embedded`].
pub fn knot_document_view_with_status(
    state: &KnotDocumentSurfaceState,
    enabled: bool,
    status: KnotDocumentStatus,
) -> KnotDocumentView {
    let snapshot = state.snapshot();
    let read_only = snapshot.write_posture == KnotDocumentWritePostureV1::ReadOnly;
    let highlight_note = uses_note_highlighting(enabled, snapshot.format);
    let save_affordance: Box<dyn AnyView<KnotDocumentSurfaceState, (), GenetCtx, GenetElement>> =
        if read_only {
            Box::new(
                span("Save disabled: read-only")
                    .attr("class", "knot-document-status-item")
                    .attr("aria-live", "polite"),
            )
        } else {
            Box::new(
                button("Save", |state: &mut KnotDocumentSurfaceState, _| {
                    let _ = state.apply(KnotDocumentIntentV1::Save);
                })
                .attr("class", "knot-document-save"),
            )
        };
    let status_row = div((
        span(format!("Source: {}", snapshot.display_label))
            .attr("class", "knot-document-status-item"),
        span(format!(
            "Format: {}",
            knot_document_format_label(snapshot.format)
        ))
        .attr("class", "knot-document-status-item"),
        span(if snapshot.dirty { "Dirty" } else { "Clean" })
            .attr("class", "knot-document-status-item"),
        span(format!(
            "Posture: {}",
            knot_document_posture_label(snapshot.write_posture)
        ))
        .attr("class", "knot-document-status-item"),
        span(save_outcome_label(&snapshot)).attr("class", "knot-document-status-item"),
        save_affordance,
    ))
    .attr("class", "knot-document-status");
    let body: Box<dyn AnyView<KnotDocumentSurfaceState, (), GenetCtx, GenetElement>> = if read_only
    {
        Box::new(
            div(snapshot.text)
                .attr("class", "knot-document-body knot-document-read-only")
                .attr("role", "document")
                .attr("aria-label", "Read-only document text")
                .attr("aria-readonly", "true"),
        )
    } else {
        Box::new(
            el(
                "div",
                lens(
                    move |input: &mut TextInput| editable_document_textarea(input, highlight_note),
                    |state: &mut KnotDocumentSurfaceState| state.input_mut_for_editable_view(),
                ),
            )
            .attr("class", "knot-document-body")
            .attr("role", "textbox")
            .attr("aria-label", "Document text"),
        )
    };
    let section = match status {
        KnotDocumentStatus::Embedded => el("section", (Some(status_row), body)),
        KnotDocumentStatus::HostOwned => el("section", (None, body)),
    };
    Box::new(
        section
            .attr("class", "knot-document")
            .attr("data-surface", "knot.document.v1"),
    )
}

fn uses_note_highlighting(enabled: bool, format: DocumentFormat) -> bool {
    enabled && matches!(format, DocumentFormat::Djot | DocumentFormat::Knot)
}

#[cfg(feature = "highlight")]
fn editable_document_textarea(input: &TextInput, highlight_note: bool) -> cambium::TextField {
    if highlight_note {
        highlighted_textarea(input, Highlight::Note)
    } else {
        textarea_typed(input)
    }
}

#[cfg(not(feature = "highlight"))]
fn editable_document_textarea(_input: &TextInput, _highlight_note: bool) -> cambium::TextField {
    textarea_typed(_input)
}

pub fn knot_document_descriptor() -> SurfaceDescriptor {
    SurfaceDescriptor {
        provider_id: ProviderId::from("knot"),
        surface_id: SurfaceId::from("knot.document.v1"),
        label: "Knot document".to_owned(),
        accepted_source: SurfaceSourceShape::One(SourceKindId::from("knot.document.v1")),
    }
}
pub fn knot_document_surface(
    dom: DomHandle,
    state: KnotDocumentSurfaceState,
) -> Box<dyn cambium::RetainedSurfaceSession> {
    let runner = GenetAppRunner::new(dom, knot_document_view, state);
    Box::new(RunnerSurfaceSession::new(
        knot_document_descriptor(),
        runner,
        |state: &KnotDocumentSurfaceState| match state.snapshot().source.kind {
            KnotDocumentSourceKindV1::File | KnotDocumentSourceKindV1::Scratch => {
                SurfaceAvailability::Available
            },
        },
        |_state, _viewport| {},
        |_action: ()| Vec::new(),
    ))
}
/// The name a document's format is shown by.
pub fn knot_document_format_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Djot => "Djot",
        DocumentFormat::Scroll => "Scrolltext",
        DocumentFormat::Gemtext => "Gemtext",
        DocumentFormat::Micron => "Micron",
        DocumentFormat::Knot => "legacy .knot",
        DocumentFormat::Markdown => "Markdown",
        DocumentFormat::Json => "JSON",
    }
}
/// The name a write posture is shown by.
pub fn knot_document_posture_label(posture: KnotDocumentWritePostureV1) -> &'static str {
    match posture {
        KnotDocumentWritePostureV1::FileTarget => "file target",
        KnotDocumentWritePostureV1::Scratch => "scratch",
        KnotDocumentWritePostureV1::ReadOnly => "read-only",
    }
}

/// What the last save did, in a word or two.
pub fn knot_document_save_outcome_label(
    outcome: Option<crate::KnotDocumentSaveOutcomeV1>,
) -> &'static str {
    match outcome {
        None => "not attempted",
        Some(crate::KnotDocumentSaveOutcomeV1::Written) => "written",
        Some(crate::KnotDocumentSaveOutcomeV1::Unchanged) => "unchanged",
        Some(crate::KnotDocumentSaveOutcomeV1::Refused) => "refused",
        Some(crate::KnotDocumentSaveOutcomeV1::Failed) => "failed",
    }
}

fn save_outcome_label(snapshot: &KnotDocumentSnapshotV1) -> String {
    let outcome = format!(
        "Save: {}",
        knot_document_save_outcome_label(snapshot.last_save_outcome)
    );
    if let Some(refusal) = snapshot.refusal {
        format!("{outcome}: {}", knot_document_refusal_label(refusal))
    } else if let Some(failure) = &snapshot.last_save_failure {
        format!("{outcome} ({})", failure.message)
    } else {
        outcome
    }
}

/// Why an action was refused, in the words a host's message uses too.
pub fn knot_document_refusal_label(refusal: crate::KnotDocumentRefusalV1) -> &'static str {
    match refusal {
        crate::KnotDocumentRefusalV1::ScratchHasNoSaveTarget => "a new document has no file yet",
        crate::KnotDocumentRefusalV1::ReadOnly => "this document is read-only",
        crate::KnotDocumentRefusalV1::ExternalChange => "the file changed on disk",
    }
}
#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    #[cfg(feature = "highlight")]
    use cambium::{Key, KeyEvent, Modifiers};
    use genet_scripted_dom::ScriptedDom;
    use layout_dom_api::LayoutDom;

    use super::*;

    fn contains_element(dom: &ScriptedDom, node: genet_scripted_dom::NodeId, name: &str) -> bool {
        dom.element_name(node)
            .is_some_and(|qualified| qualified.local.as_ref() == name)
            || dom
                .dom_children(node)
                .any(|child| contains_element(dom, child, name))
    }

    #[cfg(feature = "highlight")]
    fn first_element(
        dom: &ScriptedDom,
        node: genet_scripted_dom::NodeId,
        name: &str,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom
            .element_name(node)
            .is_some_and(|qualified| qualified.local.as_ref() == name)
        {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| first_element(dom, child, name))
    }

    #[cfg(feature = "highlight")]
    fn descendant_text(dom: &ScriptedDom, node: genet_scripted_dom::NodeId) -> String {
        dom.text(node).unwrap_or_default().to_owned()
            + &dom
                .dom_children(node)
                .map(|child| descendant_text(dom, child))
                .collect::<String>()
    }
    #[test]
    fn state_uses_the_session_input_as_the_component_buffer() {
        let mut state =
            KnotDocumentSurfaceState::new(KnotDocumentSession::scratch("memory:test", "hello"));
        let first = state.session().input() as *const TextInput;
        let second = state.input_mut_for_editable_view() as *mut TextInput;
        assert_eq!(first, second.cast_const());
    }

    #[test]
    fn note_highlighting_is_limited_to_djot_and_legacy_knot() {
        assert!(uses_note_highlighting(true, DocumentFormat::Djot));
        assert!(uses_note_highlighting(true, DocumentFormat::Knot));
        assert!(!uses_note_highlighting(true, DocumentFormat::Markdown));
        assert!(!uses_note_highlighting(true, DocumentFormat::Json));
        assert!(!uses_note_highlighting(false, DocumentFormat::Djot));
    }

    #[test]
    fn read_only_posture_has_explicit_visible_labels() {
        let snapshot = KnotDocumentSession::read_only("memory:test", "hello").snapshot();
        assert_eq!(
            knot_document_posture_label(snapshot.write_posture),
            "read-only"
        );
        assert_eq!(
            save_outcome_label(&KnotDocumentSnapshotV1 {
                refusal: Some(crate::KnotDocumentRefusalV1::ReadOnly),
                last_save_outcome: Some(crate::KnotDocumentSaveOutcomeV1::Refused),
                ..snapshot
            }),
            "Save: refused: this document is read-only"
        );
    }

    #[test]
    fn read_only_view_has_no_editable_textbox_or_save_button() {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = KnotDocumentSurfaceState::new(KnotDocumentSession::read_only(
            "memory:field",
            "# Field\n",
        ));
        let runner = GenetAppRunner::new(dom.clone(), knot_document_view, state);
        let rendered = dom.borrow();
        let body = rendered
            .all_with_class(rendered.document(), "knot-document-read-only")
            .into_iter()
            .next()
            .expect("read-only body");
        assert_eq!(
            rendered
                .element_name(body)
                .map(|name| name.local.to_string()),
            Some("div".to_owned())
        );
        assert!(
            !contains_element(&rendered, runner.root(), "textarea"),
            "a read-only document must not retain an editable text control"
        );
        assert!(
            rendered
                .all_with_class(rendered.document(), "knot-document-save")
                .is_empty(),
            "a read-only document must not render a save button"
        );
    }

    /// The embedded status row and Save control are the default; a host that
    /// draws its own status gets the text alone.
    #[test]
    fn host_owned_status_leaves_the_row_and_save_to_the_host() {
        for (status, shown) in [
            (KnotDocumentStatus::Embedded, true),
            (KnotDocumentStatus::HostOwned, false),
        ] {
            let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
            let state = KnotDocumentSurfaceState::new(KnotDocumentSession::scratch(
                "memory:status",
                "# Status\n",
            ));
            let runner = GenetAppRunner::new(
                dom.clone(),
                move |state: &KnotDocumentSurfaceState| {
                    knot_document_view_with_status(state, false, status)
                },
                state,
            );
            let rendered = dom.borrow();
            let count = |class: &str| rendered.all_with_class(rendered.document(), class).len();
            assert_eq!(
                count("knot-document-status") == 1,
                shown,
                "{status:?}: status row"
            );
            assert_eq!(count("knot-document-save") == 1, shown, "{status:?}: Save");
            assert!(
                contains_element(&rendered, runner.root(), "textarea"),
                "{status:?}: the text stays editable"
            );
        }
    }

    #[cfg(feature = "highlight")]
    #[test]
    fn highlighted_read_only_view_remains_a_plain_immutable_projection() {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = KnotDocumentSurfaceState::new(KnotDocumentSession::read_only(
            "memory:highlighted-read-only",
            "# Field\n",
        ));
        let runner = GenetAppRunner::new(
            dom.clone(),
            |state| knot_document_view_with_highlighting(state, true),
            state,
        );
        let rendered = dom.borrow();
        assert!(!contains_element(&rendered, runner.root(), "textarea"));
        assert!(
            rendered
                .all_with_class(rendered.document(), "knot-document-save")
                .is_empty()
        );
        assert!(
            rendered
                .all_with_class(rendered.document(), "syntax-heading")
                .is_empty()
        );
    }

    #[cfg(feature = "highlight")]
    #[test]
    fn highlighted_djot_renders_syntax_spans_and_keeps_the_session_buffer_for_unicode_undo() {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = KnotDocumentSurfaceState::new(KnotDocumentSession::scratch(
            "memory:highlighted-field",
            "# caf\u{e9}\n",
        ));
        let mut runner = GenetAppRunner::new(
            dom.clone(),
            |state| knot_document_view_with_highlighting(state, true),
            state,
        );

        let rendered = dom.borrow();
        let heading = rendered
            .all_with_class(rendered.document(), "syntax-heading")
            .into_iter()
            .next()
            .expect("highlighted Djot heading");
        assert!(!descendant_text(&rendered, heading).is_empty());
        let textarea = first_element(&rendered, runner.root(), "textarea")
            .expect("highlighted editable textarea");
        assert_eq!(descendant_text(&rendered, textarea), "# caf\u{e9}\n");
        drop(rendered);

        let buffer = runner.state().session().input() as *const TextInput;
        runner.set_focus(Some(textarea));
        runner.dispatch_key(KeyEvent::new(Key::Character(
            "\u{1f469}\u{200d}\u{1f680}".into(),
        )));
        assert_eq!(
            runner.state().snapshot().text,
            "# caf\u{e9}\n\u{1f469}\u{200d}\u{1f680}"
        );
        runner.dispatch_key(KeyEvent::with_mods(
            Key::Character("z".into()),
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ));
        assert_eq!(runner.state().snapshot().text, "# caf\u{e9}\n");
        assert_eq!(buffer, runner.state().session().input() as *const TextInput);
    }

    #[cfg(feature = "highlight")]
    #[test]
    fn disabled_highlighting_keeps_djot_as_plain_textarea() {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = KnotDocumentSurfaceState::new(KnotDocumentSession::scratch(
            "memory:plain-field",
            "# Heading\n",
        ));
        let _runner = GenetAppRunner::new(
            dom.clone(),
            |state| knot_document_view_with_highlighting(state, false),
            state,
        );
        let rendered = dom.borrow();
        assert!(
            rendered
                .all_with_class(rendered.document(), "syntax-heading")
                .is_empty()
        );
    }
}
