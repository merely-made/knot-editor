// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Bounded startup policy for marked native acceptance scenarios.
//!
//! This only relaxes Mesquite's initial presentation wait for the disposable
//! public-key fixture. Ordinary launches and unmarked scenario sources retain
//! Mesquite's normal ten-second timeout.

use std::{ffi::OsStr, path::Path, time::Duration};

pub(crate) const INITIAL_GRACE_ENV: &str = "KNOT_TEST_INITIAL_PRESENTATION_GRACE_SECS";
pub(crate) const NORMAL_PRESENTATION_TIMEOUT_SECS: u64 = 10;
const MAX_INITIAL_GRACE_SECS: u64 = 60;
const FIXTURE_MARKER: &str = "PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET";
const FIXTURE_MARKER_CONTENTS: &[u8] =
    b"Synthetic acceptance authority only; all keys are public test constants.\n";

/// Return the configured grace only for a marked synthetic fixture launched
/// under a Mesquite scenario lane. `read_setting` is deliberately lazy: for
/// ordinary launches or unmarked sources, the environment value is never read.
pub(crate) fn initial_presentation_grace(
    scenario_lane: bool,
    first_path: Option<&Path>,
    read_setting: impl FnOnce() -> Option<std::ffi::OsString>,
) -> Option<u64> {
    if !scenario_lane || !has_fixture_marker(first_path) {
        return None;
    }
    Some(parse_grace_seconds(
        read_setting().as_deref().and_then(OsStr::to_str),
    ))
}

fn has_fixture_marker(first_path: Option<&Path>) -> bool {
    let Some(marker) = first_path
        .and_then(Path::parent)
        .map(|parent| parent.join(FIXTURE_MARKER))
    else {
        return false;
    };
    std::fs::read(marker).is_ok_and(|contents| contents == FIXTURE_MARKER_CONTENTS)
}

fn parse_grace_seconds(value: Option<&str>) -> u64 {
    value
        .and_then(|text| text.parse::<u64>().ok())
        .filter(|seconds| *seconds >= NORMAL_PRESENTATION_TIMEOUT_SECS)
        .unwrap_or(NORMAL_PRESENTATION_TIMEOUT_SECS)
        .min(MAX_INITIAL_GRACE_SECS)
}

/// One-shot timeout restoration. A failed/absent presentation before the first
/// success keeps the initial allowance; afterward no later failure can re-arm
/// it.
pub(crate) struct PresentationGrace {
    active: bool,
    restored: bool,
}

impl PresentationGrace {
    pub(crate) fn new(initial_grace_secs: Option<u64>) -> Self {
        Self {
            active: initial_grace_secs.is_some(),
            restored: false,
        }
    }

    /// Called before `Lane::after_frame`, so the successful first frame is
    /// processed using the normal timeout and the existing lane state.
    pub(crate) fn take_restore_timeout(&mut self, presented: bool) -> Option<Duration> {
        if !self.active || self.restored || !presented {
            return None;
        }
        self.restored = true;
        Some(Duration::from_secs(NORMAL_PRESENTATION_TIMEOUT_SECS))
    }

    /// Run lane restoration at the call site before it admits the same
    /// after-frame presentation.
    pub(crate) fn before_lane_after_frame(
        &mut self,
        presented: bool,
        restore: impl FnOnce(Duration),
    ) {
        if let Some(timeout) = self.take_restore_timeout(presented) {
            restore(timeout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, ffi::OsString};
    use tempfile::tempdir;

    fn marked_source() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join(FIXTURE_MARKER), FIXTURE_MARKER_CONTENTS).unwrap();
        let source = root.path().join("essay.djot");
        (root, source)
    }

    #[test]
    fn grace_defaults_invalid_empty_and_caps_at_sixty() {
        assert_eq!(parse_grace_seconds(None), 10);
        assert_eq!(parse_grace_seconds(Some("")), 10);
        assert_eq!(parse_grace_seconds(Some("not-a-number")), 10);
        assert_eq!(parse_grace_seconds(Some("9")), 10);
        assert_eq!(parse_grace_seconds(Some("10")), 10);
        assert_eq!(parse_grace_seconds(Some("60")), 60);
        assert_eq!(parse_grace_seconds(Some("600")), 60);
    }

    #[test]
    fn only_a_marked_synthetic_scenario_reads_the_setting() {
        let (_root, source) = marked_source();
        let ordinary_root = tempdir().unwrap();
        let ordinary_source = ordinary_root.path().join("essay.djot");
        // The source's ordinary filename is not authority; its parent marker
        // is the exact opt-in boundary.
        let calls = Cell::new(0);
        let read = || {
            calls.set(calls.get() + 1);
            Some(OsString::from("60"))
        };
        assert_eq!(
            initial_presentation_grace(true, Some(&source), read),
            Some(60)
        );
        assert_eq!(calls.get(), 1);

        let calls = Cell::new(0);
        assert_eq!(
            initial_presentation_grace(true, Some(&ordinary_source), || {
                calls.set(calls.get() + 1);
                Some(OsString::from("60"))
            }),
            None
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(
            initial_presentation_grace(false, Some(&source), || {
                calls.set(calls.get() + 1);
                Some(OsString::from("60"))
            }),
            None
        );
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn marker_must_have_the_exact_public_test_contents() {
        let root = tempdir().unwrap();
        let source = root.path().join("essay.djot");
        let marker = root.path().join(FIXTURE_MARKER);
        for contents in [
            b"Synthetic acceptance authority only; all keys are public test constants.".as_slice(),
            b"not a fixture\n".as_slice(),
        ] {
            std::fs::write(&marker, contents).unwrap();
            assert_eq!(
                initial_presentation_grace(true, Some(&source), || { Some(OsString::from("60")) }),
                None
            );
        }
        std::fs::write(&marker, FIXTURE_MARKER_CONTENTS).unwrap();
        assert_eq!(
            initial_presentation_grace(true, Some(&source), || None),
            Some(10)
        );
    }

    #[test]
    fn first_failure_retains_grace_and_first_success_restores_once() {
        let mut grace = PresentationGrace::new(Some(60));
        assert_eq!(grace.take_restore_timeout(false), None);
        assert_eq!(grace.take_restore_timeout(false), None);
        assert_eq!(
            grace.take_restore_timeout(true),
            Some(Duration::from_secs(10))
        );
        for presented in [false, false, true, false] {
            assert_eq!(grace.take_restore_timeout(presented), None);
        }
    }

    #[test]
    fn restoration_callback_runs_before_lane_after_frame_and_only_once() {
        let mut grace = PresentationGrace::new(Some(60));
        let mut order = Vec::new();
        grace.before_lane_after_frame(false, |_| order.push("restore"));
        order.push("lane");
        assert_eq!(order, ["lane"]);

        order.clear();
        grace.before_lane_after_frame(true, |timeout| {
            assert_eq!(timeout, Duration::from_secs(10));
            order.push("restore");
        });
        order.push("lane");
        assert_eq!(order, ["restore", "lane"]);

        order.clear();
        grace.before_lane_after_frame(false, |_| order.push("restore"));
        order.push("lane");
        assert_eq!(order, ["lane"]);
    }

    #[test]
    fn unmarked_and_default_lanes_never_enter_restoration_state() {
        let mut grace = PresentationGrace::new(None);
        assert_eq!(grace.take_restore_timeout(true), None);
        assert_eq!(grace.take_restore_timeout(false), None);
    }
}
