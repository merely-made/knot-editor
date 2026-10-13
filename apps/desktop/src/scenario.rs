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
                let mut fields = capture_fields(state);
                let (painted, visible) = crate::composition::motion_viewport_geometry(ctx);
                fields.insert("scene_motion_canvas_painted".into(), serde_json::json!(painted).to_string());
                fields.insert("scene_motion_canvas_visible".into(), serde_json::json!(visible).to_string());
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
        let (positions, steps, position) = ctx.runner.state().composition.scene_scenario_fields();
        let source = ctx.runner.state().document().session().input().text();
        let mut snapshot = ctx
            .runner
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
            .with_field("scene_authored_positions", positions.to_string())
            .with_field("scene_placement_steps", steps.to_string())
            .with_field(
                "scene_positions_blake3",
                ctx.runner.state().composition.scene_positions_digest(),
            )
            .with_field("source_bytes", source.len().to_string())
            .with_field(
                "source_blake3",
                blake3::hash(source.as_bytes()).to_hex().to_string(),
            )
            .with_field("scene_selected_position", position);
        for (key, value) in ctx.runner.state().composition.motion_scenario_fields() {
            snapshot = snapshot.with_field(key, value);
        }
        snapshot
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

/// Bounded presentation facts plus role requests/drafts and opaque content
/// digests. Unlike the general snapshot, this never copies source text or labels.
fn capture_fields(state: &DesktopState) -> std::collections::BTreeMap<String, String> {
    let (positions, steps, _) = state.composition.scene_scenario_fields();
    let source = state.document().session().input().text();
    let choice = crate::theme_session::appearance_choice(&state.appearance);
    let font_roles_json =
        serde_json::to_string(&choice.fonts).expect("font role preferences serialize");
    let font_roles_draft: std::collections::BTreeMap<_, _> = tabard::typography::FontRole::ALL
        .into_iter()
        .map(|role| {
            (
                role.key(),
                state
                    .theme_session
                    .font_editor
                    .field(role)
                    .text()
                    .to_owned(),
            )
        })
        .collect();
    let font_roles_draft_json =
        serde_json::to_string(&font_roles_draft).expect("font role draft serializes");
    let font_roles_diagnostics =
        crate::font_roles::diagnostics(&choice.fonts, &state.theme_session.available_fonts)
            .join(" ");
    let theme_mode = choice.theme_mode.as_ref().map_or_else(
        || {
            if state.appearance.dark {
                "dark"
            } else {
                "light"
            }
            .to_owned()
        },
        |mode| mode.as_key(),
    );
    let source_face = match state.appearance.source_face {
        crate::appearance::SourceFace::IbmPlexMono => "ibm-plex-mono",
        crate::appearance::SourceFace::SystemMonospace => "system-monospace",
    };
    let mut fields: std::collections::BTreeMap<String, String> = [
        ("scene_authored_positions", positions.to_string()),
        ("scene_placement_steps", steps.to_string()),
        (
            "scene_positions_blake3",
            state.composition.scene_positions_digest(),
        ),
        ("document_count", state.docs.len().to_string()),
        ("reading_count", state.docs.readings().count().to_string()),
        ("graph_visible", graph_is_presented(state).to_string()),
        ("source_bytes", source.len().to_string()),
        (
            "source_blake3",
            blake3::hash(source.as_bytes()).to_hex().to_string(),
        ),
        ("theme_dark", state.appearance.dark.to_string()),
        ("theme_id", choice.theme_id),
        ("theme_mode", theme_mode),
        ("font_size", state.appearance.font_size.to_string()),
        ("source_face", source_face.to_owned()),
        ("requested_fonts", font_roles_json.clone()),
        ("font_roles_draft_fields", font_roles_draft_json),
        ("font_roles_diagnostics", font_roles_diagnostics),
        (
            "requested_fonts_blake3",
            blake3::hash(font_roles_json.as_bytes())
                .to_hex()
                .to_string(),
        ),
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
    .collect();
    fields.extend(state.composition.motion_scenario_fields());
    fields
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
        assert_eq!(fields["theme_id"], crate::theme_session::DEFAULT_THEME);
        assert_eq!(fields["theme_mode"], "light");
        assert_eq!(fields["source_face"], "ibm-plex-mono");
        assert_eq!(fields["requested_fonts"], "{}");
        let draft: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&fields["font_roles_draft_fields"]).unwrap();
        assert_eq!(draft["reading"], "");
        assert_eq!(fields["font_roles_diagnostics"], "");
        assert_eq!(
            fields["requested_fonts_blake3"],
            blake3::hash(b"{}").to_hex().to_string()
        );
        assert_eq!(fields["measure"], "medium");
        assert!(!fields.contains_key("wide"));
        assert!(!serde_json::to_string(&fields).unwrap().contains("secret"));
        let mut choice =
            tabard::theme::choice::ThemeChoice::new(crate::theme_session::DEFAULT_THEME, None);
        choice.fonts = tabard::typography::FontPreferences(std::collections::BTreeMap::from([(
            tabard::typography::FontRole::Reading,
            vec![tabard::typography::FontFamily::new("IBM Plex Mono").unwrap()],
        )]));
        state.appearance.theme_choice = Some(choice);
        let fields = capture_fields(&state);
        assert_eq!(
            fields["requested_fonts"],
            r#"{"reading":["IBM Plex Mono"]}"#
        );
        assert_eq!(
            fields["requested_fonts_blake3"],
            blake3::hash(fields["requested_fonts"].as_bytes())
                .to_hex()
                .to_string()
        );
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
