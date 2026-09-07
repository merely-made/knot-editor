// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use cambium::{
    AnyView, GenetCtx, GenetElement, Keyed, TextInput, button, el, lens, span, text_field_typed,
};
use cambium_genet_winit_host::{
    AppCtx, CloseDisposition, CloseRequest, FocusedTextSlot, Key, KeyPress, Runner, WindowCommands,
};
use knot_document::{
    KnotDiskComparisonV1, KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentRefusalV1,
    KnotDocumentSession, KnotDocumentSurfaceState, KnotOutlineSnapshotV1, knot_document_view,
};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::path::PathBuf;

const SCRATCH_ADDRESS: &str = "scratch:untitled";

pub type DesktopView = Box<dyn AnyView<DesktopState, (), GenetCtx, GenetElement>>;
pub type DesktopRunner = Runner<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingAction {
    Close,
    New,
    Open(PathBuf),
    Reload,
}

/// State owned by the standalone application around the reusable document surface.
///
/// The path field is an explicit capability supplied by this desktop host. It is
/// intentionally not part of `knot.document.v1`, whose source admission remains
/// the embedding host's responsibility.
pub struct DesktopState {
    pub document: KnotDocumentSurfaceState,
    pub path: TextInput,
    pub message: Option<String>,
    comparison: Option<KnotDiskComparisonV1>,
    comparison_error: Option<String>,
    outline_visible: bool,
    outline_snapshot: Option<KnotOutlineSnapshotV1>,
    outline_error: Option<String>,
    focus_source_requested: bool,
    window: WindowCommands,
    pending: Option<PendingAction>,
    discard_close: bool,
}

impl DesktopState {
    pub fn new(session: KnotDocumentSession, window: WindowCommands) -> Self {
        Self::with_path(session, window, None)
    }

    pub fn with_path(
        session: KnotDocumentSession,
        window: WindowCommands,
        initial_path: Option<PathBuf>,
    ) -> Self {
        let path = initial_path.map_or_else(TextInput::default, |path| {
            TextInput::new(path.to_string_lossy())
        });
        Self {
            document: KnotDocumentSurfaceState::new(session),
            path,
            message: None,
            comparison: None,
            comparison_error: None,
            outline_visible: false,
            outline_snapshot: None,
            outline_error: None,
            focus_source_requested: false,
            window,
            pending: None,
            discard_close: false,
        }
    }

    fn dirty(&self) -> bool {
        self.document.snapshot().dirty
    }

    fn path_value(&self) -> Result<PathBuf, String> {
        let value = self.path.text().trim();
        if value.is_empty() {
            Err("Enter a file path first.".to_owned())
        } else {
            Ok(PathBuf::from(value))
        }
    }

    fn clear_comparison(&mut self) {
        self.comparison = None;
        self.comparison_error = None;
    }

    fn clear_outline(&mut self) {
        self.outline_snapshot = None;
        self.outline_error = None;
    }

    fn sync_outline_snapshot(&mut self) {
        if !self.outline_visible {
            return;
        }
        let current = self.document.snapshot();
        let stale = self.outline_snapshot.as_ref().is_none_or(|snapshot| {
            snapshot.address != current.source.address || snapshot.source_text != current.text
        });
        if stale {
            self.outline_snapshot = Some(self.document.session().outline_snapshot());
        }
    }

    fn toggle_outline(&mut self) {
        self.outline_visible = !self.outline_visible;
        if self.outline_visible {
            self.outline_snapshot = Some(self.document.session().outline_snapshot());
            self.outline_error = None;
        } else {
            self.clear_outline();
        }
    }

    fn select_outline_item(&mut self, index: usize) {
        let Some(snapshot) = self.outline_snapshot.as_ref() else {
            self.outline_error = Some("Outline is not ready; show it again.".to_owned());
            return;
        };
        let result = self
            .document
            .session_mut()
            .select_outline_item(snapshot, index);
        match result {
            Ok(()) => {
                self.outline_error = None;
                self.focus_source_requested = true;
            },
            Err(error) => {
                self.outline_error = Some(error.clone());
                self.message = Some(format!("Outline selection failed: {error}"));
            },
        }
    }

    fn compare_disk(&mut self) {
        match self.document.session().compare_disk() {
            Ok(comparison) => {
                self.comparison = Some(comparison);
                self.comparison_error = None;
            },
            Err(error) => {
                self.comparison = None;
                self.comparison_error = Some(error);
            },
        }
    }

    fn hide_comparison(&mut self) {
        self.clear_comparison();
    }

    fn request(&mut self, action: PendingAction) {
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::Close => self.window.close(),
            PendingAction::New => {
                self.document = KnotDocumentSurfaceState::new(KnotDocumentSession::scratch(
                    SCRATCH_ADDRESS,
                    "",
                ));
                self.path = TextInput::default();
                self.clear_comparison();
                self.clear_outline();
                self.sync_outline_snapshot();
                self.message = Some("New untitled Djot document.".to_owned());
            },
            PendingAction::Open(path) => match KnotDocumentSession::open(&path) {
                Ok(session) => {
                    self.path = TextInput::new(path.to_string_lossy().into_owned());
                    self.document = KnotDocumentSurfaceState::new(session);
                    self.clear_comparison();
                    self.clear_outline();
                    self.sync_outline_snapshot();
                    self.message = Some(format!("Opened {}.", path.display()));
                },
                Err(error) => self.message = Some(format!("Open failed: {error}")),
            },
            PendingAction::Reload => match self.document.apply(KnotDocumentIntentV1::Reload) {
                Ok(_) => {
                    self.clear_comparison();
                    self.clear_outline();
                    self.sync_outline_snapshot();
                    self.message = Some("Reloaded from disk.".to_owned());
                },
                Err(error) => self.message = Some(intent_error_label(error)),
            },
        }
    }

    fn new_document(&mut self) {
        self.request(PendingAction::New);
    }

    fn open_document(&mut self) {
        match self.path_value() {
            Ok(path) => self.request(PendingAction::Open(path)),
            Err(error) => self.message = Some(error),
        }
    }

    fn save(&mut self) {
        match self.document.apply(KnotDocumentIntentV1::Save) {
            Ok(_) => self.message = Some("Saved.".to_owned()),
            Err(error) => self.message = Some(intent_error_label(error)),
        }
    }

    fn save_as(&mut self) -> bool {
        let path = match self.path_value() {
            Ok(path) => path,
            Err(error) => {
                self.message = Some(error);
                return false;
            },
        };
        match self
            .document
            .apply(KnotDocumentIntentV1::SaveAs(path.clone()))
        {
            Ok(_) => {
                self.clear_comparison();
                self.clear_outline();
                self.sync_outline_snapshot();
                self.message = Some(format!("Saved as {}.", path.display()));
                true
            },
            Err(error) => {
                self.message = Some(intent_error_label(error));
                false
            },
        }
    }

    fn reload(&mut self) {
        self.request(PendingAction::Reload);
    }

    fn confirm_save(&mut self) {
        let Some(action) = self.pending.take() else {
            return;
        };
        match action {
            PendingAction::Close => {
                if self.document.snapshot().write_posture
                    == knot_document::KnotDocumentWritePostureV1::Scratch
                {
                    if self.save_as() {
                        self.window.close();
                    } else {
                        self.pending = Some(PendingAction::Close);
                    }
                } else if let Err(error) = self.document.apply(KnotDocumentIntentV1::Save) {
                    self.message = Some(intent_error_label(error));
                    self.pending = Some(PendingAction::Close);
                } else {
                    self.window.close();
                }
            },
            other => {
                let saved = if self.document.snapshot().write_posture
                    == knot_document::KnotDocumentWritePostureV1::Scratch
                {
                    self.save_as()
                } else {
                    match self.document.apply(KnotDocumentIntentV1::Save) {
                        Ok(_) => true,
                        Err(error) => {
                            self.message = Some(intent_error_label(error));
                            false
                        },
                    }
                };
                if saved {
                    self.perform(other);
                } else {
                    self.pending = Some(other);
                }
            },
        }
    }

    fn confirm_discard(&mut self) {
        let Some(action) = self.pending.take() else {
            return;
        };
        if matches!(action, PendingAction::Close) {
            self.discard_close = true;
            self.window.close();
        } else {
            self.perform(action);
        }
    }

    fn cancel_pending(&mut self) {
        self.pending = None;
    }

    fn close_request(&mut self, request: CloseRequest) -> CloseDisposition {
        if self.discard_close {
            self.discard_close = false;
            return CloseDisposition::Exit;
        }
        if self.dirty() {
            self.pending = Some(PendingAction::Close);
            return CloseDisposition::KeepVisible;
        }
        if matches!(request, CloseRequest::Native | CloseRequest::Command) {
            CloseDisposition::Exit
        } else {
            CloseDisposition::KeepVisible
        }
    }
}

fn intent_error_label(error: KnotDocumentIntentErrorV1) -> String {
    match error {
        KnotDocumentIntentErrorV1::Refused(KnotDocumentRefusalV1::ExternalChange) => {
            "Save refused: file changed on disk. Compare, Reload, or choose a new Save As path."
                .to_owned()
        },
        KnotDocumentIntentErrorV1::Refused(refusal) => format!("Action refused: {refusal:?}"),
        KnotDocumentIntentErrorV1::SaveFailed(failure) => {
            format!("Save failed: {}", failure.message)
        },
    }
}

pub fn desktop_view(state: &DesktopState) -> DesktopView {
    let outline_panel: DesktopView = if !state.outline_visible {
        Box::new(el("div", ()))
    } else if let Some(snapshot) = &state.outline_snapshot {
        let rows = snapshot
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let label = item.label.clone();
                let level = item.level;
                let key = item.start;
                let accessible_label = format!("Heading level {level}: {label}");
                (
                    key,
                    button(label, move |state: &mut DesktopState, _| {
                        state.select_outline_item(index);
                    })
                    .attr("class", "knot-outline-row")
                    .attr("data-outline-index", index.to_string())
                    .attr("data-outline-level", level.to_string())
                    .attr("aria-label", accessible_label),
                )
            })
            .collect::<Vec<_>>();
        let rows_view = if rows.is_empty() {
            Box::new(span("No headings in this document.")) as DesktopView
        } else {
            Box::new(el("div", Keyed::new(rows)).attr("class", "knot-outline-rows")) as DesktopView
        };
        let error = state.outline_error.as_ref().map(|error| {
            span(format!("Outline error: {error}")).attr("class", "knot-outline-error")
        });
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Outline"),
                            button("Hide Outline", |state: &mut DesktopState, _| {
                                state.toggle_outline();
                            }),
                        ),
                    )
                    .attr("class", "knot-outline-header"),
                    error,
                    rows_view,
                ),
            )
            .attr("class", "knot-outline")
            .attr("role", "region")
            .attr("aria-label", "Document outline"),
        )
    } else {
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Outline"),
                            button("Hide Outline", |state: &mut DesktopState, _| {
                                state.toggle_outline();
                            }),
                        ),
                    )
                    .attr("class", "knot-outline-header"),
                    span("Outline is updating; try again shortly."),
                ),
            )
            .attr("class", "knot-outline")
            .attr("role", "region")
            .attr("aria-label", "Document outline"),
        )
    };
    let comparison_panel: DesktopView = if let Some(error) = &state.comparison_error {
        Box::new(
            el(
                "section",
                (
                    span("Comparison unavailable"),
                    span(error.clone()).attr("class", "knot-comparison-error"),
                    span("Disk text could not be read; refresh to try again."),
                    button("Refresh comparison", |state: &mut DesktopState, _| {
                        state.compare_disk();
                    }),
                    button("Hide comparison", |state: &mut DesktopState, _| {
                        state.hide_comparison();
                    }),
                ),
            )
            .attr("class", "knot-comparison knot-comparison-error-panel")
            .attr("role", "region")
            .attr("aria-label", "Disk comparison error"),
        )
    } else if let Some(comparison) = &state.comparison {
        let snapshot = state.document.snapshot();
        let stale = comparison.buffer_text != snapshot.text
            || comparison.address != snapshot.source.address;
        let status = if stale {
            "Snapshot: stale because the source or address changed since comparison"
        } else {
            "Buffer snapshot matches current source"
        };
        let disk_status = if comparison.disk_changed_since_baseline {
            "Disk changed since the saved baseline at comparison: yes"
        } else {
            "Disk changed since the saved baseline at comparison: no"
        };
        Box::new(
            el(
                "section",
                (
                    el("header", (span("Disk comparison"), span(status)))
                        .attr("class", "knot-comparison-header"),
                    span(format!("Compared address: {}", comparison.address)),
                    span(disk_status),
                    span("Disk text was read when compared; refresh to read it again."),
                    button("Refresh comparison", |state: &mut DesktopState, _| {
                        state.compare_disk();
                    }),
                    button("Hide comparison", |state: &mut DesktopState, _| {
                        state.hide_comparison();
                    }),
                    el(
                        "div",
                        (
                            el(
                                "section",
                                (
                                    span("Buffer at comparison"),
                                    el("pre", comparison.buffer_text.clone()),
                                ),
                            )
                            .attr("class", "knot-comparison-version")
                            .attr("aria-label", "Buffer source at comparison"),
                            el(
                                "section",
                                (
                                    span("Disk at comparison"),
                                    el("pre", comparison.disk_text.clone()),
                                ),
                            )
                            .attr("class", "knot-comparison-version")
                            .attr("aria-label", "Disk source at comparison"),
                        ),
                    )
                    .attr("class", "knot-comparison-versions"),
                ),
            )
            .attr("class", "knot-comparison")
            .attr("role", "region")
            .attr("aria-label", "Disk comparison"),
        )
    } else {
        Box::new(el("div", ()))
    };
    let document: DesktopView = Box::new(lens(
        |state: &mut KnotDocumentSurfaceState| knot_document_view(state),
        |state: &mut DesktopState| &mut state.document,
    ));
    let prompt: DesktopView = match state.pending.as_ref() {
        Some(action) => {
            let title = match action {
                PendingAction::Close => "This document has unsaved changes.",
                PendingAction::New => "New document will replace unsaved changes.",
                PendingAction::Open(_) => "Opening will replace unsaved changes.",
                PendingAction::Reload => "Reload will replace unsaved changes with disk text.",
            };
            Box::new(
                el(
                    "aside",
                    (
                        span(title).attr("id", "knot-confirm-message"),
                        button("Save", |state: &mut DesktopState, _| state.confirm_save())
                            .attr("id", "knot-confirm-save"),
                        button("Discard", |state: &mut DesktopState, _| {
                            state.confirm_discard()
                        }),
                        button("Cancel", |state: &mut DesktopState, _| {
                            state.cancel_pending()
                        }),
                    ),
                )
                .attr("class", "knot-confirm")
                .attr("role", "dialog")
                .attr("aria-label", "Unsaved changes"),
            )
        },
        None => Box::new(el("div", ())),
    };
    let message: DesktopView = Box::new(
        span(state.message.clone().unwrap_or_else(|| "Ready.".to_owned()))
            .attr("class", "knot-workspace-message")
            .attr("aria-live", "polite"),
    );
    Box::new(
        el(
            "main",
            (
                el(
                    "nav",
                    (
                        button("New", |state: &mut DesktopState, _| state.new_document()),
                        button("Open", |state: &mut DesktopState, _| state.open_document()),
                        button("Save As", |state: &mut DesktopState, _| {
                            state.save_as();
                        }),
                        button("Reload", |state: &mut DesktopState, _| state.reload()),
                        button("Compare", |state: &mut DesktopState, _| {
                            state.compare_disk();
                        }),
                        button(
                            if state.outline_visible {
                                "Hide Outline"
                            } else {
                                "Show Outline"
                            },
                            |state: &mut DesktopState, _| state.toggle_outline(),
                        ),
                        el(
                            "label",
                            (
                                "Path",
                                lens(
                                    |input: &mut TextInput| text_field_typed(input),
                                    |state: &mut DesktopState| &mut state.path,
                                ),
                            ),
                        )
                        .attr("id", "knot-path-field")
                        .attr("class", "knot-path-field"),
                    ),
                )
                .attr("class", "knot-workspace-toolbar"),
                message,
                el("div", (document, outline_panel)).attr("class", "knot-writing-area"),
                comparison_panel,
                prompt,
            ),
        )
        .attr("class", "knot-workspace"),
    )
}

fn ancestor_has_id<D: LayoutDom>(dom: &D, focused: D::NodeId, id: &str) -> bool {
    let namespace = Namespace::from("");
    let local = LocalName::from("id");
    let mut node = Some(focused);
    while let Some(current) = node {
        if dom.attribute(current, &namespace, &local) == Some(id) {
            return true;
        }
        node = dom.parent(current);
    }
    false
}

fn ancestor_has_class<D: LayoutDom>(dom: &D, focused: D::NodeId, class: &str) -> bool {
    let mut node = Some(focused);
    while let Some(current) = node {
        if dom.has_class(current, class) {
            return true;
        }
        node = dom.parent(current);
    }
    false
}

pub fn focused_text(runner: &DesktopRunner) -> Option<FocusedTextSlot<DesktopState>> {
    let focused = runner.focus()?;
    let dom = runner.dom();
    let dom_ref = dom.borrow();
    let name = LayoutDom::element_name(&*dom_ref, focused)?;
    let is_text_control = name.local.as_ref() == "textarea" || name.local.as_ref() == "input";
    let is_document_textarea = name.local.as_ref() == "textarea";
    if !is_text_control {
        return None;
    }
    let path = ancestor_has_id(&*dom_ref, focused, "knot-path-field");
    drop(dom_ref);
    if path {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|state| &state.path),
            get_mut: Box::new(|state| &mut state.path),
        });
    }
    if is_document_textarea {
        if runner.state().document.snapshot().write_posture
            == knot_document::KnotDocumentWritePostureV1::ReadOnly
        {
            return None;
        }
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|state| state.document.session().input()),
            get_mut: Box::new(|state| {
                state
                    .document
                    .session_mut()
                    .input_mut()
                    .expect("editable document focus")
            }),
        });
    }
    None
}

pub fn key_intercept(runner: &mut DesktopRunner, press: &KeyPress) -> bool {
    if !press.modifiers.is_command_chord() {
        return false;
    }
    let Key::Character(key) = &press.key else {
        return false;
    };
    let key = key.to_ascii_lowercase();
    match (key.as_str(), press.modifiers.shift) {
        ("n", false) => runner.update(DesktopState::new_document),
        ("o", false) => runner.update(DesktopState::open_document),
        ("s", true) => runner.update(|state| {
            state.save_as();
        }),
        ("s", false) => runner.update(DesktopState::save),
        _ => return false,
    }
    true
}

pub fn close_request(runner: &mut DesktopRunner, request: CloseRequest) -> CloseDisposition {
    let mut disposition = CloseDisposition::KeepVisible;
    runner.update(|state| disposition = state.close_request(request));
    disposition
}

/// Complete an outline activation after the click has rebuilt the retained
/// tree. The click target itself is a button, so focus must be returned to the
/// existing document textarea before the next key or IME event is routed.
pub fn after_dispatch(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
    let state = ctx.runner.state();
    let focus_requested = state.focus_source_requested;
    let outline_needs_sync = if state.outline_visible {
        let current = state.document.snapshot();
        state.outline_snapshot.as_ref().is_none_or(|snapshot| {
            snapshot.address != current.source.address || snapshot.source_text != current.text
        })
    } else {
        false
    };
    if !focus_requested && !outline_needs_sync {
        return;
    }
    ctx.runner.update(|state| {
        if focus_requested {
            state.focus_source_requested = false;
        }
        if outline_needs_sync {
            state.sync_outline_snapshot();
        }
    });
    if !focus_requested {
        return;
    }
    let target = {
        let dom = ctx.runner.dom();
        let dom_ref = dom.borrow();
        ctx.runner.focusables().into_iter().find(|node| {
            LayoutDom::element_name(&*dom_ref, *node)
                .is_some_and(|name| name.local.as_ref() == "textarea")
                && ancestor_has_class(&*dom_ref, *node, "knot-document-body")
        })
    };
    if let Some(target) = target {
        ctx.runner.set_focus(Some(target));
    }
}

pub const DESKTOP_CSS: &str = concat!(
    ".knot-workspace { display:flex; flex-direction:column; gap:12px; padding:20px; }",
    ".knot-workspace-toolbar { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }",
    ".knot-path-field { display:flex; align-items:center; gap:6px; flex:1; }",
    ".knot-path-field input { min-width:280px; flex:1; }",
    ".knot-workspace-message { min-height:1.4em; }",
    ".knot-confirm { display:flex; align-items:center; gap:8px; padding:12px; border:1px solid; }",
    ".knot-confirm [id=knot-confirm-message] { margin-right:auto; }",
    ".knot-writing-area { display:flex; align-items:flex-start; gap:12px; }",
    ".knot-document { flex:1; min-width:0; }",
    ".knot-document-body textarea { width:100%; min-height:360px; line-height:1.5; box-sizing:border-box; }",
    ".knot-outline { flex:0 0 280px; width:280px; box-sizing:border-box; max-height:480px; overflow:auto; padding:12px; border:1px solid; }",
    ".knot-outline-header { display:flex; align-items:center; justify-content:space-between; gap:8px; }",
    ".knot-outline-rows { display:flex; flex-direction:column; gap:2px; margin-top:8px; }",
    ".knot-outline-row { display:block; width:100%; text-align:left; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }",
    ".knot-outline-row[data-outline-level=2] { padding-left:16px; }",
    ".knot-outline-row[data-outline-level=3] { padding-left:32px; }",
    ".knot-outline-row[data-outline-level=4] { padding-left:48px; }",
    ".knot-outline-row[data-outline-level=5] { padding-left:64px; }",
    ".knot-outline-row[data-outline-level=6] { padding-left:80px; }",
    ".knot-outline-error { color:crimson; }",
    ".knot-comparison { max-height:360px; overflow:auto; padding:12px; border:1px solid; }",
    ".knot-comparison-header { display:flex; flex-wrap:wrap; gap:8px; align-items:baseline; }",
    ".knot-comparison-versions { display:flex; flex-wrap:wrap; gap:12px; }",
    ".knot-comparison-version { flex:1 1 360px; min-width:0; }",
    ".knot-comparison-version pre { max-height:240px; overflow:auto; white-space:pre-wrap; overflow-wrap:anywhere; user-select:text; }",
    "@media (max-width:700px) { .knot-workspace { padding:12px; } .knot-path-field input { min-width:160px; } .knot-writing-area { flex-direction:column; align-items:stretch; } .knot-outline { flex-basis:auto; width:100%; max-height:240px; } }",
);

#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::{Harness, Init, Modifiers, inert_hooks};
    use genet_probe::Selector;
    use knot_document::KNOT_DOCUMENT_CSS;
    use tempfile::tempdir;

    fn harness(
        session: KnotDocumentSession,
    ) -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
        let mut host = Harness::with_hooks(
            Init {
                state: DesktopState::new(session, WindowCommands::new()),
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: format!("{DESKTOP_CSS}{KNOT_DOCUMENT_CSS}"),
            },
            {
                let mut hooks = inert_hooks();
                hooks.after_dispatch = Box::new(after_dispatch);
                hooks.close_request = Box::new(|ctx, request| close_request(ctx.runner, request));
                hooks.focused_text = Box::new(focused_text);
                hooks.key_intercept = Box::new(key_intercept);
                hooks
            },
        );
        let commands = host.commands();
        host.update(|state| state.window = commands.clone());
        host
    }

    fn input_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom
            .element_name(node)
            .is_some_and(|name| name.local.as_ref() == "input")
        {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| input_node(dom, child))
    }

    fn named_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        name: &str,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom
            .element_name(node)
            .is_some_and(|element| element.local.as_ref() == name)
        {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| named_node(dom, child, name))
    }

    fn class_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        class: &str,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom.has_class(node, class) {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| class_node(dom, child, class))
    }

    fn text_content(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
    ) -> String {
        let own = dom.text(node).unwrap_or_default();
        let children = dom
            .dom_children(node)
            .map(|child| text_content(dom, child))
            .collect::<String>();
        format!("{own}{children}")
    }

    fn count_class(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        class: &str,
    ) -> usize {
        usize::from(dom.has_class(node, class))
            + dom
                .dom_children(node)
                .map(|child| count_class(dom, child, class))
                .sum::<usize>()
    }

    fn request_native_close(
        host: &mut Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
    ) {
        host.request_close(CloseRequest::Native);
        // KeepVisible requests a native redraw. Supply that frame explicitly
        // before resolving newly created prompt controls in the windowless host.
        host.layout_at(900.0, 640.0);
    }

    #[test]
    fn path_control_and_save_as_are_real_controls() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("saved.djot");
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "# Draft\n"));
        host.layout_at(1000.0, 720.0);
        let path_input = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            input_node(&dom, dom.document()).expect("path input")
        };
        let (x, y, width, height) = host.painted_rect(path_input).expect("path input layout");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.key_injected(&path.to_string_lossy());
        assert!(host.click_on(&Selector::role("button").containing("Save As")));
        assert!(path.exists());
    }

    #[test]
    fn dirty_native_close_stays_open_until_discard_and_clean_close_exits() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        host.layout_at(900.0, 640.0);
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("edit");
        });
        request_native_close(&mut host);
        assert!(!host.close_requested());
        assert!(host.click_on(&Selector::role("button").containing("Discard")));
        assert!(host.close_requested());
    }

    #[test]
    fn dirty_native_close_save_writes_and_accepts_queued_close() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("close-save.djot");
        std::fs::write(&path, "# Original\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("draft");
        });
        host.layout_at(900.0, 640.0);
        request_native_close(&mut host);
        assert!(!host.close_requested());
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(host.close_requested());
        assert!(std::fs::read_to_string(&path).unwrap().contains("draft"));
        assert!(!host.state().document.snapshot().dirty);
    }

    #[test]
    fn canceled_dirty_close_preserves_the_document() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "draft"));
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edit");
        });
        host.layout_at(900.0, 640.0);
        request_native_close(&mut host);
        assert!(host.click_on(&Selector::role("button").containing("Cancel")));
        assert!(!host.close_requested());
        assert!(host.state().document.snapshot().text.contains("draft edit"));
        assert!(host.state().document.snapshot().dirty);
    }

    #[test]
    fn canceled_open_preserves_the_dirty_document() {
        let temp = tempdir().unwrap();
        let replacement = temp.path().join("replacement.djot");
        std::fs::write(&replacement, "replacement").unwrap();
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "draft"));
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edit");
            state.path = TextInput::new(replacement.to_string_lossy().into_owned());
        });
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Open")));
        assert!(host.click_on(&Selector::role("button").containing("Cancel")));
        assert_eq!(host.state().document.snapshot().text, "draft edit");
        assert!(host.state().document.snapshot().dirty);
    }

    #[test]
    fn outline_rows_select_unicode_source_and_return_focus_for_repeat_clicks() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# Café\n\n## Second heading\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        assert!(host.click_on(&Selector::role("button").containing("Café")));
        let snapshot = host.state().document.session().outline_snapshot();
        let item = snapshot
            .items
            .iter()
            .find(|item| item.label == "Café")
            .expect("unicode heading");
        assert_eq!(
            host.state().document.snapshot().selection.anchor.byte,
            item.start
        );
        assert_eq!(
            host.state().document.snapshot().selection.focus.byte,
            item.end
        );
        let textarea = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            named_node(&dom, dom.document(), "textarea").expect("document textarea")
        };
        assert_eq!(host.focus(), Some(textarea));

        let path_input = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            input_node(&dom, dom.document()).expect("path input")
        };
        let (x, y, width, height) = host.painted_rect(path_input).expect("path layout");
        host.click_at(x + width / 2.0, y + height / 2.0);
        assert_eq!(host.focus(), Some(path_input));
        assert!(host.click_on(&Selector::role("button").containing("Second heading")));
        assert_eq!(host.focus(), Some(textarea));
        host.key_injected("!");
        assert_eq!(host.state().path.text(), "");
        assert_eq!(host.state().document.snapshot().text, "# Café\n\n!");
    }

    #[test]
    fn outline_tracks_direct_committed_edits_and_excludes_ime_preedit() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "# First\n"));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("\n## Direct edit\n");
        });
        host.after_dispatch();
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let outline = class_node(&dom, dom.document(), "knot-outline").expect("outline panel");
        assert!(text_content(&dom, outline).contains("Direct edit"));
        drop(dom);

        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .set_preedit("仮入力");
        });
        host.after_dispatch();
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let outline = class_node(&dom, dom.document(), "knot-outline").expect("outline panel");
        assert!(!text_content(&dom, outline).contains("仮入力"));
    }

    #[test]
    fn stale_outline_activation_reports_refusal_without_changing_selection() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# First\n\n## Second\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("changed");
        });
        let before = host.state().document.snapshot();
        // Deliberately omit the normal after-dispatch refresh: this click uses
        // the old retained row and exercises the exact-source guard.
        assert!(host.click_on(&Selector::role("button").containing("First")));
        assert_eq!(host.state().document.snapshot(), before);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let workspace = class_node(&dom, dom.document(), "knot-workspace").expect("workspace");
        assert!(text_content(&dom, workspace).contains("Outline selection failed:"));
    }

    #[test]
    fn empty_outline_has_an_accessible_empty_state() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "plain text\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let outline = class_node(&dom, dom.document(), "knot-outline").expect("outline panel");
        assert!(text_content(&dom, outline).contains("No headings in this document."));
    }

    #[test]
    #[ignore = "diagnostic timing receipt; run with --ignored --nocapture"]
    fn outline_long_document_probe() {
        use std::time::Instant;

        let mut source = String::new();
        for index in 0..200 {
            source.push_str(&format!("# Heading {index}\n\n"));
            source
                .push_str(&"A long writing paragraph keeps the layout representative. ".repeat(10));
            source.push_str("\n\n");
        }
        let source_bytes = source.len();
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, source));
        host.layout_at(1100.0, 700.0);
        let show_started = Instant::now();
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        host.layout_at(1100.0, 700.0);
        let show_layout_us = show_started.elapsed().as_micros();
        let row_count = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            count_class(&dom, dom.document(), "knot-outline-row")
        };
        let edit_started = Instant::now();
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("\n## Appended heading\n");
        });
        host.after_dispatch();
        host.layout_at(1100.0, 700.0);
        let edit_layout_us = edit_started.elapsed().as_micros();
        println!(
            "outline_probe source_bytes={source_bytes} heading_rows={row_count} show_layout_us={show_layout_us} edit_layout_us={edit_layout_us}"
        );
        assert_eq!(row_count, 200);
    }

    #[test]
    fn command_shortcuts_reach_save_and_save_as() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shortcut.djot");
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        host.layout_at(900.0, 640.0);
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("edit");
            state.path = TextInput::new(path.to_string_lossy().into_owned());
        });
        host.set_modifiers(Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        });
        host.key_char("s");
        host.set_modifiers(Modifiers::NONE);
        assert!(path.exists());
        assert!(std::fs::read_to_string(&path).unwrap().contains("edit"));
        assert!(!host.state().document.snapshot().dirty);
        assert!(host.state().path.text().ends_with("shortcut.djot"));

        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" again");
        });
        host.set_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        host.key_char("s");
        host.set_modifiers(Modifiers::NONE);
        assert!(!host.state().document.snapshot().dirty);
        assert!(std::fs::read_to_string(&path).unwrap().contains("again"));
    }

    #[test]
    fn failed_save_keeps_native_close_prompt_open() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("changed.djot");
        std::fs::write(&path, "# Original\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("draft");
        });
        std::fs::write(&path, "# External\n").unwrap();
        request_native_close(&mut host);
        assert!(!host.close_requested());
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(!host.close_requested());
        assert!(
            host.state()
                .message
                .as_deref()
                .unwrap()
                .contains("changed on disk")
        );
    }

    #[test]
    fn compare_captures_disk_and_buffer_without_writing_or_mutating_source() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("compare.djot");
        std::fs::write(&path, "buffer\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edited");
        });
        let buffer_before = host.state().document.snapshot();
        std::fs::write(&path, "disk\n").unwrap();
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        let comparison = host
            .state()
            .comparison
            .as_ref()
            .expect("comparison snapshot");
        assert_eq!(comparison.buffer_text, buffer_before.text);
        assert_eq!(comparison.disk_text, "disk\n");
        assert!(comparison.disk_changed_since_baseline);
        assert_eq!(host.state().document.snapshot().text, buffer_before.text);
        assert_eq!(
            host.state().document.snapshot().selection,
            buffer_before.selection
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "disk\n");

        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" later");
        });
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let panel = class_node(&dom, dom.document(), "knot-comparison").expect("comparison panel");
        assert!(text_content(&dom, panel).contains("Snapshot: stale"));
        drop(dom);
        assert!(host.click_on(&Selector::role("button").containing("Refresh comparison")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let panel = class_node(&dom, dom.document(), "knot-comparison").expect("comparison panel");
        assert!(text_content(&dom, panel).contains("Buffer snapshot matches current source"));
    }

    #[test]
    fn comparison_refreshes_explicitly_and_clears_after_new() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("refresh.djot");
        std::fs::write(&path, "first\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        std::fs::write(&path, "second\n").unwrap();
        assert_eq!(
            host.state().comparison.as_ref().unwrap().disk_text,
            "first\n"
        );
        assert!(host.click_on(&Selector::role("button").containing("Refresh comparison")));
        assert_eq!(
            host.state().comparison.as_ref().unwrap().disk_text,
            "second\n"
        );
        assert!(host.click_on(&Selector::role("button").containing("New")));
        assert!(host.state().comparison.is_none());
        assert!(host.state().comparison_error.is_none());
    }

    #[test]
    fn missing_disk_replaces_the_old_comparison_with_an_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("missing.djot");
        std::fs::write(&path, "source\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        std::fs::remove_file(&path).unwrap();
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        assert!(host.state().comparison.is_none());
        assert!(
            host.state()
                .comparison_error
                .as_deref()
                .is_some_and(|error| error.contains("read"))
        );
    }

    #[test]
    fn comparison_renders_markup_as_plain_source_text() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("markup.djot");
        std::fs::write(&path, "<script>alert(1)</script>\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let pre = named_node(&dom, dom.document(), "pre").expect("comparison pre");
        assert!(text_content(&dom, pre).contains("<script>alert(1)</script>"));
        assert!(named_node(&dom, pre, "script").is_none());
    }

    #[test]
    fn compare_leaves_a_pending_dirty_close_untouched() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("pending-compare.djot");
        std::fs::write(&path, "source\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edited");
        });
        host.layout_at(900.0, 640.0);
        request_native_close(&mut host);
        let before = host.state().document.snapshot();
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        assert_eq!(host.state().pending, Some(PendingAction::Close));
        assert_eq!(host.state().document.snapshot().text, before.text);
        assert_eq!(host.state().document.snapshot().selection, before.selection);
    }

    #[test]
    fn inner_save_keeps_comparison_disk_text_historical() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("historical.djot");
        std::fs::write(&path, "original\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edited");
        });
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        assert!(host.click_on(&Selector::class("knot-document-save")));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n edited");
        assert!(!host.state().document.snapshot().dirty);
        assert_eq!(
            host.state().comparison.as_ref().unwrap().disk_text,
            "original\n"
        );
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let panel = class_node(&dom, dom.document(), "knot-comparison").expect("comparison panel");
        let rendered = text_content(&dom, panel);
        assert!(rendered.contains("Disk text was read when compared; refresh to read it again."));
        assert!(rendered.contains("Buffer snapshot matches current source"));
    }

    #[test]
    fn read_only_save_as_refusal_is_visible_and_does_not_write() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("read-only.djot");
        let mut host = harness(KnotDocumentSession::read_only(SCRATCH_ADDRESS, "read only"));
        host.update(|state| state.path = TextInput::new(path.to_string_lossy().into_owned()));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Save As")));
        assert!(!path.exists());
        assert_eq!(
            host.state().document.snapshot().write_posture,
            knot_document::KnotDocumentWritePostureV1::ReadOnly
        );
        assert!(host.state().message.as_deref().unwrap().contains("refused"));
    }
}
