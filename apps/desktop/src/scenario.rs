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
}

impl KnotLane {
    pub fn new(sheet: String) -> Self {
        Self { sheet }
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
