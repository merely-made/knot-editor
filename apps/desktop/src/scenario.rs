// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's product hooks for Mesquite's scenario lane. `KNOT_SCENARIO` names a taproot
//! scenario; `KNOT_CAPTURE_DIR` and `KNOT_RECEIPT` say where its captures and
//! receipt go. The lane drives the desktop from inside, by role and label, and
//! with no scenario named the desktop runs as usual.

use cambium_genet_winit_host::AppCtx;
use taproot::ProbeSnapshot;

use crate::workspace::{DesktopState, DesktopView};

pub(crate) type DesktopLogic = fn(&DesktopState) -> DesktopView;

pub struct KnotLane {
    sheet: String,
    capture_run: Option<String>,
}

impl KnotLane {
    pub fn new(sheet: String) -> Self {
        Self {
            sheet,
            capture_run: std::env::var_os("KNOT_CAPTURE_CORRELATION")
                .filter(|value| value == "1")
                .map(|_| uuid::Uuid::new_v4().to_string()),
        }
    }
}

impl mesquite::Product for KnotLane {
    type State = DesktopState;
    type Logic = DesktopLogic;
    type View = DesktopView;
    const KIND: &'static str = "knot";
    const SURFACE: &'static str = "app";
    const LOG_PREFIX: &'static str = "knot";
    fn sheet(&self) -> &str {
        &self.sheet
    }

    fn capture_observer(&self) -> Option<mesquite::CaptureObserver<Self>> {
        Some(mesquite::CaptureObserver {
            run: self.capture_run.clone()?,
            observe: Box::new(|ctx, _frame| {
                let state = ctx.runner.state();
                let fields = capture_fields(state);
                let graph = state.graph_capture_facts()?;
                let graph_visible = state.docs.graph().is_some();
                let observed_graph = graph
                    .installed
                    .filter(|installed| graph_visible && installed.acceptance.is_some());
                Ok(mesquite::CaptureProjection {
                    product: serde_json::json!({
                        "schema": "knot.desktop-presentation/v1",
                        "fields": fields,
                        "graph_catalog": {
                            "visible": graph_visible,
                            "facts": graph,
                            "operation": observed_graph
                                .map(|installed| format!("catalog:{}", installed.generation)),
                            "cause": observed_graph
                                .and_then(|installed| installed.acceptance.as_ref()),
                        },
                        "operation": null,
                        "cause": null,
                        "semantic_revision": null,
                    }),
                    fields,
                    viewport: None,
                })
            }),
        })
    }

    fn diagnostic_attachment(
        &mut self,
        ctx: &mut mesquite::Ctx<'_, Self>,
    ) -> Result<Option<mesquite::DiagnosticBatch>, String> {
        ctx.runner.state().graph_diagnostic_attachment()
    }

    fn snapshot(
        &self,
        ctx: &AppCtx<'_, DesktopState, DesktopLogic, DesktopView>,
        _captures: usize,
        _: f32,
    ) -> ProbeSnapshot {
        ctx.runner.state().scenario_snapshot().with_field(
            "source_focused",
            crate::workspace::source_is_focused(ctx.runner).to_string(),
        )
    }

    fn busy(&self, ctx: &mesquite::Ctx<'_, Self>, capture_pending: bool) -> Option<bool> {
        let state = ctx.runner.state();
        Some(capture_pending || state.background_busy())
    }
}

/// Fixed categories and numeric/boolean facts only. Unlike the general scenario
/// snapshot, this never clones document text, paths, labels, messages or errors.
fn capture_fields(state: &DesktopState) -> std::collections::BTreeMap<String, String> {
    [
        ("document_count", state.docs.len().to_string()),
        ("reading_count", state.docs.readings().count().to_string()),
        (
            "source_bytes",
            state.document().session().input().text().len().to_string(),
        ),
        ("theme_dark", state.appearance.dark.to_string()),
        ("font_size", state.appearance.font_size.to_string()),
        ("wide", state.appearance.wide.to_string()),
        ("relaxed", state.appearance.relaxed.to_string()),
        ("background_busy", state.background_busy().to_string()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_projection_reads_current_bounded_facts_without_document_material() {
        let mut state = DesktopState::new(
            knot_document::KnotDocumentSession::scratch("secret-address", "secret source"),
            cambium_genet_winit_host::WindowCommands::new(),
        );
        state.message = Some("secret error".into());
        let fields = capture_fields(&state);
        assert_eq!(fields["source_bytes"], "13");
        assert_eq!(fields["theme_dark"], "false");
        assert!(!serde_json::to_string(&fields).unwrap().contains("secret"));
        state.appearance.dark = true;
        assert_eq!(capture_fields(&state)["theme_dark"], "true");
        assert_eq!(
            fields["theme_dark"], "false",
            "the earlier reading stays frozen"
        );
    }
}
