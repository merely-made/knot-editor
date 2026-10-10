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
                let graph_visible = graph_is_presented(state);
                Ok(mesquite::CaptureProjection {
                    product: serde_json::json!({
                        "schema": "knot.desktop-presentation/v2",
                        "fields": fields,
                        "graph_catalog": graph_capture_projection(&graph, graph_visible),
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
        let (arrangement, columns, retained) =
            ctx.runner.state().composition.recipe_scenario_fields();
        ctx.runner
            .state()
            .scenario_snapshot()
            .with_field(
                "source_focused",
                crate::workspace::source_is_focused(ctx.runner).to_string(),
            )
            .with_field(
                "graph_visible",
                graph_is_presented(ctx.runner.state()).to_string(),
            )
            .with_field("recipe_arrangement", arrangement)
            .with_field("recipe_columns", columns)
            .with_field("recipe_edits_retained", retained.to_string())
    }

    fn busy(&self, ctx: &mesquite::Ctx<'_, Self>, capture_pending: bool) -> Option<bool> {
        let state = ctx.runner.state();
        Some(capture_pending || state.background_busy())
    }
}

/// A retained tab is not a presented graph: match Workbench's active content
/// and controlled rail/drawer state, including its visible float policy.
fn graph_is_presented(state: &DesktopState) -> bool {
    fn in_tree(
        tree: &workbench::TileTree,
        presentation: &workbench::WorkbenchPresentation,
        graph: workbench::TileId,
    ) -> bool {
        match tree {
            workbench::TileTree::Stack(stack) => {
                stack
                    .tabs
                    .get(stack.active)
                    .is_some_and(|tile| tile.id == graph)
                    && !presentation
                        .stack(stack)
                        .and_then(|entry| entry.collapsed.as_ref())
                        .is_some_and(|collapsed| !collapsed.open)
            },
            workbench::TileTree::Split { children, .. } => children
                .iter()
                .any(|branch| in_tree(&branch.tree, presentation, graph)),
        }
    }
    let Some(graph) = state.docs.graph() else {
        return false;
    };
    let workspace = state.docs.workspace();
    in_tree(workspace.tiled(), &state.presentation, graph)
        || workspace
            .visible_floating(false)
            .iter()
            .any(|floating| floating.tile.id == graph)
}

fn graph_capture_projection(
    graph: &crate::graph::diagnostics::CaptureFacts<'_>,
    visible: bool,
) -> serde_json::Value {
    let observed = graph
        .installed
        .filter(|installed| visible && installed.acceptance.is_some());
    serde_json::json!({
        "visible": visible,
        "facts": graph,
        "operation": observed.map(|installed| format!("catalog:{}", installed.generation)),
        "cause": observed.and_then(|installed| installed.acceptance.as_ref()),
    })
}

/// Fixed categories and numeric/boolean facts only. Unlike the general scenario
/// snapshot, this never clones document text, paths, labels, messages or errors.
fn capture_fields(state: &DesktopState) -> std::collections::BTreeMap<String, String> {
    [
        ("document_count", state.docs.len().to_string()),
        ("reading_count", state.docs.readings().count().to_string()),
        ("graph_visible", graph_is_presented(state).to_string()),
        (
            "source_bytes",
            state.document().session().input().text().len().to_string(),
        ),
        ("theme_dark", state.appearance.dark.to_string()),
        ("font_size", state.appearance.font_size.to_string()),
        (
            "measure",
            match state.appearance.measure {
                crate::appearance::Measure::Narrow => "narrow",
                crate::appearance::Measure::Medium => "medium",
                crate::appearance::Measure::Wide => "wide",
                crate::appearance::Measure::Full => "full",
            }
            .to_owned(),
        ),
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
    fn graph_capture_requires_active_content_and_an_open_collapsed_stack() {
        use workbench::{CollapsedStack, DrawerGeometry, StackPresentation};
        let mut state = DesktopState::new(
            knot_document::KnotDocumentSession::scratch("scratch:graph-capture", "# Source"),
            cambium_genet_winit_host::WindowCommands::new(),
        );
        let document = state.docs.tile_of(state.docs.focused().unwrap()).unwrap();
        let graph = state.docs.open_graph("Graph");
        let canonical = state.docs.workspace().tiled().clone();
        assert!(graph_is_presented(&state), "active Graph is presented");
        state.docs.activate(document);
        assert!(
            state.docs.graph().is_some(),
            "the background Graph remains retained"
        );
        assert!(
            !graph_is_presented(&state),
            "a background Graph is not presented"
        );
        state.docs.activate(graph);
        state.presentation.stacks.push(StackPresentation {
            anchor: graph,
            min_width: 0.0,
            collapsed: Some(CollapsedStack {
                label: "Readings".into(),
                rail_width: 28.0,
                open: false,
                drawer: None,
            }),
        });
        assert!(
            !graph_is_presented(&state),
            "a closed rail hides its active Graph"
        );
        state.presentation.stacks[0]
            .collapsed
            .as_mut()
            .unwrap()
            .drawer = Some(DrawerGeometry {
            trigger: (0.0, 0.0, 28.0, 0.0),
            panel_size: (280.0, 600.0),
            bounds: (0.0, 0.0, 320.0, 600.0),
        });
        assert!(
            !graph_is_presented(&state),
            "a closed drawer hides its active Graph"
        );
        state.presentation.stacks[0]
            .collapsed
            .as_mut()
            .unwrap()
            .open = true;
        assert!(
            graph_is_presented(&state),
            "an open drawer presents its active Graph"
        );
        state.docs.activate(document);
        assert!(
            !graph_is_presented(&state),
            "opening a drawer does not present an inactive Graph tab"
        );
        state.docs.activate(graph);
        assert_eq!(
            state.docs.workspace().tiled(),
            &canonical,
            "presentation did not mutate the canonical tree"
        );
    }

    #[test]
    fn hidden_graph_keeps_owner_facts_but_suppresses_the_presentation_cause() {
        use crate::graph::diagnostics::{CaptureFacts, Installed};
        let accepted = apparatus::RecordRef {
            run: "capture-test".into(),
            source: "knot.catalog".into(),
            sequence: 6,
        };
        let installed = Installed {
            generation: 2,
            request: None,
            outcome: None,
            acceptance: Some(accepted.clone()),
        };
        let facts = CaptureFacts {
            configured: true,
            busy: false,
            queued: false,
            desired_generation: 2,
            generation_exhausted: false,
            installed: Some(&installed),
            catalog_nodes: 3,
            catalog_relations: 2,
            read_errors: 0,
            failure_present: false,
            diagnostic_admission: None,
        };
        let visible = graph_capture_projection(&facts, true);
        assert_eq!(visible["operation"], "catalog:2");
        assert_eq!(visible["cause"], serde_json::to_value(&accepted).unwrap());
        let hidden = graph_capture_projection(&facts, false);
        assert_eq!(hidden["visible"], false);
        assert!(hidden["operation"].is_null());
        assert!(hidden["cause"].is_null());
        assert_eq!(
            hidden["facts"], visible["facts"],
            "owner facts remain observable without a presentation claim"
        );
    }
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
        assert_eq!(fields["measure"], "medium");
        assert!(!fields.contains_key("wide"));
        assert!(!serde_json::to_string(&fields).unwrap().contains("secret"));
        state.appearance.dark = true;
        state.appearance.measure = crate::appearance::Measure::Full;
        assert_eq!(capture_fields(&state)["theme_dark"], "true");
        assert_eq!(capture_fields(&state)["measure"], "full");
        assert_eq!(fields["measure"], "medium");
        assert_eq!(
            fields["theme_dark"], "false",
            "the earlier reading stays frozen"
        );
    }
}
