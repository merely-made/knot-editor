# Workspace slice 1 native review

**Date:** 2026-09-29
**Result:** Steps 8 and 9 complete under the plan's fixed-or-recorded visual
difference condition. Five native scenarios passed; six complete frames were
reviewed. This is not a release-readiness assessment.

## Evidence and provenance

The native-tested executable is based on `8c03804` plus the Graph-specific
clip fix integrated as `8820e20` (original agent commit `f046d15`). Its exact
SHA-256 is
`c4cc080bc662d298068017b8299a8ba3956ccd569d52ff48bc109b90339ca2aa`.
Only comments and regression tests were added between that executable build
and the fix commit; the product behavior is the same. No dependency pins moved.

The complete local artifact bundle, including the executable, launcher source,
delayed scenarios, logs, PNGs, hashes and diagnostic history, is retained at:

`/Users/markik/Code/testing/knot-editor/step9-graph-candidate-d-20260929-c4cc080/`

Its `REVIEW.md` provides the full manifest and native launch procedure. Each
receipt below is relative to that directory. Captures are 2200 x 1400 physical
pixels for a 1100 x 700 content surface; OS window chrome is not part of them.
The launcher adds a 300-frame foreground grace before the checked-in scenario
actions. It uses isolated settings and a test catalog, not the user's catalog.

| Scenario | Native result | Frame digest |
| --- | --- | --- |
| Focused writing | `RESULT ok`, 1 nonblank frame | `6b4ab2ddd79ead40` |
| Source beside Preview | `RESULT ok`, 1 nonblank frame | `82b77ee2aca4a742` |
| Research with references | `RESULT ok`, 1 nonblank frame | `47fa6c5e4b48ed7b` |
| Last-tab graph | `RESULT ok`, 1 nonblank frame | `1dfd6f3773b90feb` |
| Two independently serving sites | `RESULT ok`, 2 distinct nonblank frames | `62ed1192d54597d1`, `d4c966e89acd3461` |

Each lane has `receipts/<hyphenated-scenario-name>/scenario.done` and its PNGs.
Two-site PNGs are `two-sites-serving.png` and `two-sites-serving-second.png`.
All other PNG names match the scenario names in their directories.

The original reference PNGs named by the
[design pass](2026-09-23_knot_design_pass.md) were unavailable. Review used its
annotated criteria, not an invented pixel comparison.

## Native failure found and fixed

The earlier capture attempts returned zero frames because surface acquisition
reported `Occluded`. Native application access resolved that testing barrier.
The graph then exposed a different failure: the accessibility tree existed,
but the complete client surface was black. Its repeatable blank digest was
`a5543af33cd56725`.

Disabling the custom graph paint leaf did not change that black frame. Removing
only the shared Mere root clip, or that clip plus Knot's Graph wrapper clip,
also failed. Removing those clips plus the Workbench Graph content clip
produced a visible native frame. This is consistent with the pinned Mere
component's documented renderer limitation around a self-clipping graph and
positioned notice, rather than proof of a new defect inside the leaf itself.

The final fix reserves one stable tile identity for the singleton Graph and
uses its existing `data-tile` attribute to scope the content-slot exception.
Ordinary document, reading, metadata and site slots retain their scrolling and
padding. Graph close/reopen, ordinary ID allocation, actual DOM binding and
CSS scope have regression coverage. The shared Mere component still owns the
graph rendering. Failed diagnostic artifacts remain alongside the successful
bundle for comparison; they were not substituted for passing receipts.

## Whole-frame findings and recorded follow-ups

- **Focused writing:** the source, caret, clean state and status bar are
  visible. The first toolbar row is partly displaced above the capture edge
  after the source click. Scenario scroll-into-view of the tall textarea is a
  plausible cause, not yet a proven diagnosis. Record this for the scenario
  scroll/toolbar follow-up; expected source viewport clipping is separate.
- **Source beside Preview:** source and Preview fit their own columns;
  headings, diagnostics and status are readable. Preview still uses sans-serif
  text and boxed link controls rather than the proposed serif/plain-link
  treatment. Its lower clipping is the independent tile viewport.
- **Research:** Outline and Readings share the right tab stack. The reading
  displays all three actual emitted reference rows and a truthful no-space
  notice; long metadata wraps without overlap. The vertically split dedicated
  References panel and index-progress chip remain later A3/E1 work.
- **Last-tab graph:** the shared Graph, host actions, layouts, catalog-reading
  notice, toolbar and status are visible. An empty session column consumes
  left-hand space, and the default square node control obscures the start of
  the `field_notes` label. These are recorded shared-view presentation
  follow-ups, not fixed by the compositor workaround.
- **Two sites:** both site and source tabs are visible. Both servers remain
  active while selecting each site panel. Site-one uses port 5699; site-two
  displays its actual ephemeral endpoint (59860 in this run), with configured
  port `0`. Source focus and the serving status chip correctly remain on
  site-one when only the site panel changes. The saved-snapshot footer and
  Close site control are cramped and remain a spacing follow-up.

The provisional two-row command toolbar, full stacks instead of folded rails,
and OS title bar are current slice boundaries. Later title/menu, typography,
collapse and custody work is not claimed as implemented here.

## Automated verification

- `cargo test --workspace --locked`: passed on integrated main, including the
  updated two-site scenario; full log at `/tmp/knot-step9-final-workspace-test.log`.
- Desktop library: 169 passed, 0 failed, 1 ignored.
- Shared scenario lane: 3 passed, including all five checked-in step 9 flows.
- Strict desktop-library clippy: passed.
- Native receipts: five passing scenarios, six nonblank reviewed frames.

All-target strict clippy and repository-wide formatting still encounter
pre-existing Rust 1.97 lint/format drift in unrelated tests/files. Those were
not silently rewritten as part of this Graph fix. Windowless tests guard
behavior and scoping; only the native receipts establish that the compositor
actually paints these frames.
