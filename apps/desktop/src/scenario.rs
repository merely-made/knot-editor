// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's product hooks for Mesquite's scenario lane. `KNOT_SCENARIO` names a taproot
//! scenario; `KNOT_CAPTURE_DIR` and `KNOT_RECEIPT` say where its captures and
//! receipt go. The lane drives the desktop from inside, by role and label, and
//! with no scenario named the desktop runs as usual.

use cambium_genet_winit_host::AppCtx;
use genet_scripted_dom::NodeId;
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use taproot::ProbeSnapshot;

use crate::workspace::{DesktopState, DesktopView};

pub(crate) type DesktopLogic = fn(&DesktopState) -> DesktopView;

pub struct KnotLane {
    sheet: String,
    capture_run: Option<String>,
    metre_pointer: Option<(f32, f32)>,
}

impl KnotLane {
    pub fn new(sheet: String) -> Self {
        Self {
            sheet,
            metre_pointer: None,
            capture_run: std::env::var_os("KNOT_CAPTURE_CORRELATION")
                .filter(|value| value == "1")
                .map(|_| uuid::Uuid::new_v4().to_string()),
        }
    }
}

fn visible_context_text_ranges(
    ctx: &AppCtx<'_, DesktopState, DesktopLogic, DesktopView>,
) -> (Option<(usize, usize)>, Option<(usize, usize)>) {
    fn collect<D: LayoutDom<NodeId = NodeId>>(
        dom: &D,
        node: NodeId,
        sources: &mut Vec<NodeId>,
        previews: &mut Vec<NodeId>,
    ) {
        let namespace = Namespace::from("");
        if dom.attribute(node, &namespace, &LocalName::from("aria-label")) == Some("Document text")
        {
            sources.push(node);
        }
        if dom.has_class(node, "knot-document-preview")
            && dom
                .attribute(
                    node,
                    &namespace,
                    &LocalName::from("data-knot-preview-document"),
                )
                .is_some()
        {
            previews.push(node);
        }
        for child in dom.dom_children(node) {
            collect(dom, child, sources, previews);
        }
    }

    fn range(
        ctx: &AppCtx<'_, DesktopState, DesktopLogic, DesktopView>,
        node: NodeId,
    ) -> Option<(usize, usize)> {
        let (x, y, width, height) = ctx.visible_rect(node)?;
        let x_offsets = [8.0, width * 0.2, width * 0.5, width * 0.8];
        let y_offsets = [1.0, 10.0, height * 0.5, (height - 2.0).max(1.0)];
        let mut positions = Vec::new();
        for y_offset in y_offsets.into_iter().filter(|offset| *offset < height) {
            for x_offset in x_offsets.into_iter().filter(|offset| *offset < width) {
                if let Some(byte) = ctx.text_position_at(node, (x + x_offset, y + y_offset)) {
                    positions.push(byte);
                }
            }
        }
        Some((*positions.iter().min()?, *positions.iter().max()?))
    }

    let (sources, previews) = {
        let dom = ctx.runner.dom();
        let dom = dom.borrow();
        let mut sources = Vec::new();
        let mut previews = Vec::new();
        collect(&*dom, dom.document(), &mut sources, &mut previews);
        (sources, previews)
    };
    let source = sources
        .into_iter()
        .find(|node| ctx.visible_rect(*node).is_some());
    let preview = previews
        .into_iter()
        .find(|node| ctx.visible_rect(*node).is_some());
    (
        source.and_then(|node| range(ctx, node)),
        preview.and_then(|node| range(ctx, node)),
    )
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

fn first_share(state: &DesktopState) -> String {
    state
        .docs
        .workspace()
        .tiled()
        .fractions_at(&workbench::TilePath(vec![]))
        .and_then(|fractions| fractions.first().copied())
        .map_or_else(|| "none".into(), |share| format!("{share:.2}"))
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
        ("forme_editing", state.docs.forme_editing().to_string()),
        ("forme_first_share", first_share(state)),
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
    fields.extend(
        state
            .catalog_fold_facts()
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value)),
    );
    fields.extend(state.composition.motion_scenario_fields());
    fields.extend(crate::composition::metre::scenario_fields(state));
    fields
}

#[cfg(test)]
mod tests;

mod product;
