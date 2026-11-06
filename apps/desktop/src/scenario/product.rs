// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl mandrake::Product for KnotLane {
    type State = DesktopState;
    type Logic = DesktopLogic;
    type View = DesktopView;
    const KIND: &'static str = "knot";
    const SURFACE: &'static str = "app";
    const LOG_PREFIX: &'static str = "knot";
    fn sheet(&self) -> &str {
        &self.sheet
    }

    fn capture_observer(&self) -> Option<mandrake::CaptureObserver<Self>> {
        Some(mandrake::CaptureObserver {
            run: self.capture_run.clone()?,
            observe: Box::new(|ctx, _frame| {
                let state = ctx.runner.state();
                let mut fields = capture_fields(state);
                let (painted, visible) = crate::composition::motion_viewport_geometry(ctx);
                fields.insert(
                    "scene_motion_canvas_painted".into(),
                    serde_json::json!(painted).to_string(),
                );
                fields.insert(
                    "scene_motion_canvas_visible".into(),
                    serde_json::json!(visible).to_string(),
                );
                let graph = state.graph_capture_facts()?;
                let graph_visible = graph_is_presented(state);
                Ok(mandrake::CaptureProjection {
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
        ctx: &mut mandrake::Ctx<'_, Self>,
    ) -> Result<Option<mandrake::DiagnosticBatch>, String> {
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
        let resident = ctx
            .runner
            .state()
            .graph
            .model(&ctx.runner.state().graph_open_nodes());
        let source = ctx.runner.state().document().session().input().text();
        let (source_visible_bytes, preview_visible_bytes) = visible_context_text_ranges(ctx);
        let mut snapshot = ctx
            .runner
            .state()
            .scenario_snapshot()
            .with_field(
                "forme_editing",
                ctx.runner.state().docs.forme_editing().to_string(),
            )
            .with_field("forme_first_share", first_share(ctx.runner.state()))
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
                "context_source_live_byte_start",
                source_visible_bytes
                    .map_or_else(|| "unavailable".into(), |range| range.0.to_string()),
            )
            .with_field(
                "context_source_live_byte_end",
                source_visible_bytes
                    .map_or_else(|| "unavailable".into(), |range| range.1.to_string()),
            )
            .with_field(
                "context_preview_live_byte_start",
                preview_visible_bytes
                    .map_or_else(|| "unavailable".into(), |range| range.0.to_string()),
            )
            .with_field(
                "context_preview_live_byte_end",
                preview_visible_bytes
                    .map_or_else(|| "unavailable".into(), |range| range.1.to_string()),
            )
            .with_field(
                "source_blake3",
                blake3::hash(source.as_bytes()).to_hex().to_string(),
            )
            .with_field("scene_selected_position", position)
            .with_field(
                "resident_session_count",
                resident.sessions.len().to_string(),
            )
            .with_field(
                "resident_fork_count",
                resident
                    .sessions
                    .iter()
                    .filter(|session| session.name.starts_with("Session (fork of "))
                    .count()
                    .to_string(),
            );
        let (caption_group, controls_group) = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            (
                taproot::matching_classes(&dom, &taproot::Selector::class("title-bar-captions"))
                    .into_iter()
                    .next(),
                taproot::matching_classes(
                    &dom,
                    &taproot::Selector::class("window-caption-controls"),
                )
                .into_iter()
                .next(),
            )
        };
        let caption_rect = caption_group.and_then(|node| ctx.painted_rect(node));
        let controls_rect = controls_group.and_then(|node| ctx.painted_rect(node));
        let rect_text = |rect: Option<(f32, f32, f32, f32)>| {
            rect.map_or_else(
                || "unavailable".to_owned(),
                |(x, y, width, height)| format!("{x:.1},{y:.1},{width:.1},{height:.1}"),
            )
        };
        snapshot = snapshot
            .with_field(
                "chrome_host_logical_width",
                format!("{:.1}", ctx.logical_size.0),
            )
            .with_field(
                "chrome_host_logical_height",
                format!("{:.1}", ctx.logical_size.1),
            )
            .with_field("chrome_caption_group_rect", rect_text(caption_rect))
            .with_field("chrome_controls_group_rect", rect_text(controls_rect))
            .with_field(
                "chrome_caption_group_inside_viewport",
                caption_rect
                    .is_some_and(|(x, _, width, _)| {
                        x >= -0.5 && x + width <= ctx.logical_size.0 + 0.5
                    })
                    .to_string(),
            )
            .with_field(
                "chrome_controls_group_inside_viewport",
                controls_rect
                    .is_some_and(|(x, _, width, _)| {
                        x >= -0.5 && x + width <= ctx.logical_size.0 + 0.5
                    })
                    .to_string(),
            );
        for (key, value) in ctx.runner.state().catalog_fold_facts() {
            snapshot = snapshot.with_field(key, value);
        }
        for (key, value) in ctx.runner.state().composition.motion_scenario_fields() {
            snapshot = snapshot.with_field(key, value);
        }
        for (key, value) in crate::composition::metre::scenario_fields(ctx.runner.state()) {
            snapshot = snapshot.with_field(key, value);
        }
        snapshot
    }

    fn app_step(
        &mut self,
        ctx: &mut mandrake::Ctx<'_, Self>,
        _: mandrake::Checkpoints<'_>,
        line: &str,
    ) -> Result<(), String> {
        use cambium_rootstock::HostPointer;
        if line == "inspect-resident-fork" {
            let state = ctx.runner.state();
            let model = state.graph.model(&state.graph_open_nodes());
            let mut forks = model
                .sessions
                .iter()
                .filter(|session| session.name.starts_with("Session (fork of "));
            let fork = forks
                .next()
                .ok_or("Resident did not disclose the expected fork")?;
            if forks.next().is_some() {
                return Err("Resident disclosed more than one fixture fork".into());
            }
            let key = mere_view::session_node_key(fork.id);
            let report = serde_json::json!({"fork":fork.id.as_uuid().to_string(),"name":fork.name,"sessions":model.sessions.iter().map(|session|serde_json::json!({"id":session.id.as_uuid().to_string(),"name":session.name,"attached":session.attached,"trashed":session.trashed})).collect::<Vec<_>>()});
            let node = {
                let dom = ctx.runner.dom();
                let dom = dom.borrow();
                taproot::matching_classes(
                    &dom,
                    &taproot::Selector::class("graph-canvas-swatch-node")
                        .with_attr("data-key", key),
                )
                .into_iter()
                .next()
                .ok_or("Resident fork has no mounted node")?
            };
            let painted = ctx
                .painted_rect(node)
                .ok_or("Resident fork node is not painted")?;
            let visible = ctx
                .visible_rect(node)
                .ok_or("Resident fork node is not visible")?;
            if painted.2 <= 0.0
                || painted.3 <= 0.0
                || [
                    painted.0 - visible.0,
                    painted.1 - visible.1,
                    painted.2 - visible.2,
                    painted.3 - visible.3,
                ]
                .iter()
                .any(|delta| delta.abs() > 1.0)
            {
                return Err("Resident fork node is clipped".into());
            }
            // Inspect through the mounted shared card callback; never synthesize an owner action.
            ctx.runner.dispatch_click(
                node,
                cambium::PointerClick::at((painted.2 / 2.0, painted.3 / 2.0)),
            );
            let root = std::env::var_os("KNOT_CAPTURE_DIR").ok_or("No capture directory")?;
            std::fs::write(
                std::path::PathBuf::from(root).join("resident-fork.json"),
                serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            return Ok(());
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        if let ["checkpoint-metre", name] = parts.as_slice() {
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return Err("simple checkpoint name required".into());
            }
            let input = ctx
                .runner
                .state()
                .composition
                .metre
                .draft
                .as_ref()
                .ok_or("No metre draft")?;
            let root = std::env::var_os("KNOT_CAPTURE_DIR").ok_or("No capture directory")?;
            let root = std::path::PathBuf::from(root);
            std::fs::write(
                root.join(format!("{name}.scene.json")),
                serde_json::to_vec_pretty(&input.scene).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let nodes = {
                let dom = ctx.runner.dom();
                let dom = dom.borrow();
                taproot::matching_classes(&*dom, &taproot::Selector::class("knot-metre-scene"))
            };
            let geometry: Vec<_> = nodes.iter().map(|node| serde_json::json!({"painted":ctx.painted_rect(*node),"visible":ctx.visible_rect(*node)})).collect();
            std::fs::write(root.join(format!("{name}.geometry.json")),serde_json::to_vec_pretty(&serde_json::json!({"geometry":geometry,"producer":ctx.producers.contains(crate::composition::metre::KEY),"fields":crate::composition::metre::scenario_fields(ctx.runner.state())})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            return Ok(());
        }
        let node = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            taproot::matching_classes(&*dom, &taproot::Selector::class("knot-metre-scene"))
                .into_iter()
                .next()
                .ok_or("No metre scene leaf")?
        };
        let (x, y, w, h) = ctx.painted_rect(node).ok_or("Metre scene is not painted")?;
        let surfaces = ctx.runner.state().composition.metre.surfaces.borrow();
        let surface = surfaces.surface.as_ref().ok_or("No metre surface")?;
        match parts.as_slice() {
            ["press-metre-span", key, part] => {
                let r = surface
                    .occurrence_rect(key, w.round() as u32, h.round() as u32)
                    .ok_or("No placed metre occurrence")?;
                let local_x = match *part {
                    "start" => r.x + 1.0,
                    "end" => r.x + r.width - 1.0,
                    "body" => r.x + r.width / 2.0,
                    _ => return Err("body/start/end required".into()),
                };
                let point = (x + local_x, y + r.y + r.height / 2.0);
                self.metre_pointer = Some(point);
                ctx.pointer.push(HostPointer::Press(point.0, point.1));
            },
            [verb @ ("move-metre-span" | "release-metre-span"), delta] => {
                let delta: f32 = delta.parse().map_err(|_| "position delta required")?;
                if !delta.is_finite() {
                    return Err("finite delta required".into());
                }
                let origin = self.metre_pointer.ok_or("No pressed metre occurrence")?;
                let a = surface
                    .scene_point(0.0, 0.0, w.round() as u32, h.round() as u32)
                    .ok_or("Invalid viewport")?;
                let b = surface
                    .scene_point(1.0, 0.0, w.round() as u32, h.round() as u32)
                    .ok_or("Invalid viewport")?;
                let target = (origin.0 + delta * 47.0 / (b.x - a.x), origin.1);
                if *verb == "move-metre-span" {
                    ctx.pointer.push(HostPointer::Moved(target.0, target.1));
                } else {
                    ctx.pointer.push(HostPointer::Release(target.0, target.1));
                    self.metre_pointer = None;
                }
            },
            _ => return Err(format!("Unknown Knot step: {line}")),
        }
        Ok(())
    }

    fn busy(&self, ctx: &mandrake::Ctx<'_, Self>, capture_pending: bool) -> Option<bool> {
        let state = ctx.runner.state();
        Some(capture_pending || state.background_busy())
    }
}
