// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's half of the host's scenario lane. `KNOT_SCENARIO` names a taproot
//! scenario; `KNOT_CAPTURE_DIR` and `KNOT_RECEIPT` say where its captures and
//! receipt go. The lane drives the desktop from inside, by role and label, and
//! with no scenario named the desktop runs as usual.

use cambium_genet_winit_host::{AppCtx, LaneApp, ProbeSnapshot};

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

impl LaneApp<DesktopState, DesktopLogic, DesktopView> for KnotLane {
    fn sheet(&self) -> &str {
        &self.sheet
    }

    fn snapshot(&self, ctx: &AppCtx<'_, DesktopState, DesktopLogic, DesktopView>) -> ProbeSnapshot {
        ctx.runner.state().scenario_snapshot()
    }
}
