# Collapse: non-destructive workspace presentation

Status: Combined integration and shared text paint repair accepted after automated and native qualification.
Date: 2026-09-29
Base: Knot `8bc98c7`, following Typography acceptance.
Deciders: Existing design-pass section 7 rulings; implementation follows them.

## Context

Three simultaneously visible stacks crowd the source below its writer measure.
The source owns document bytes, selection, preedit, undo and write authority;
layout must not acquire those responsibilities or close a reading to gain room.
The saved split ratio must come back when the viewport becomes wide enough.
The reflow target is 320 CSS pixels, including the same logical width under zoom.

## Decision

Mere Workbench gains additive, transient stack presentation separate from the
canonical tile tree. Shared Frisket renders rails, inline openings and bounded
drawers through `overlay_surface`. Knot computes the source measure, fold order
and temporary shares from the live post-zoom host geometry. Navigator gives way
first; readings narrow to their configurable minimum (280px by default), then
fold. Below the configurable drawer threshold (500px by default), a rail opens
over the source. Canonical tabs, active selection and split shares remain intact.

The command menus fold below the configurable 700px threshold. Severity-first
status overflow keeps at least one fact slot and exposes other facts through
`+N`; refusals outrank warnings and quiet facts, with stable ties. Its renderer
and interaction are shared, while Knot owns the width threshold.

## Options considered and trade-offs

1. Transient presentation over the canonical tree: preserves ownership and
   restores ratios; adds a small controlled contract and an independent policy.
2. Rewriting the tree on resize: simple initial rendering but couples visibility
   to document/tab lifecycle and makes reliable restoration harder. Rejected.
3. CSS-only hiding at fixed widths: cheap, but cannot preserve a font-dependent
   source measure or assure that hidden controls leave keyboard navigation.
   Rejected in favor of semantic rails and actual host geometry.

## Consequences

- Shared mechanics have no Knot threshold, document or source-font authority.
- The source may still wrap when the viewport cannot physically fit its measure.
- A closed rail does not mount hidden panel controls. Opening a drawer must be
  reachable by keyboard and dismissible with source focus restored.
- Thresholds are local preferences; unknown fields must survive read/write.
- No runtime font downloads, save-authority changes or credential changes.

## Test plan and action items

- [x] Pure policy: 320/499/500/699/700/900/1100/1280/1600px, source sizes
      12/16/24, hard-wrap override and Full; Navigator-first, reading minimum,
      restoration of canonical shares, bounded threshold values.
- [x] Shared controls: rails/drawers, active-tab retention, Close/Escape/outside
      dismissal, hidden controls absent, bounded drawer geometry and focus.
- [x] Shared status: severity ordering, refusal kept, overflow details/actions,
      Escape focus return, displaced open detail during resize, 320px geometry.
- [x] Knot host: actual frame/zoom hook, painted source measure when feasible,
      no horizontal chrome overflow, narrow menu/panel reachability.
- [x] Data safety: source bytes, selection/preedit, dirty/baseline/undo and disk
      content unchanged across resize and panel open/close.
- [x] Preferences: missing/legacy/unknown/malformed data and store round-trip.
- [x] Full workspace, proportionate lint, exact binary and native captures.

## Receipt

### Combined main integration, 2026-09-30

The integration merges Knot main `3dfb70b` into the accepted Collapse branch,
preserving catalog diagnostics alongside Commands, typography and collapse.
Mere `c670795851546000aea4b1ef66fd929d458d0a41` preserves both dependency
lines and repairs shared text paint ordering, using Genet
`b1eb3af197111ecde34421bbe1bad2e6024ca564`.

The merge review found that a retained Graph tab was being reported as visible
even when inactive or hidden by Collapse. The capture adapter now follows the
active tile and controlled rail/drawer state. Hidden Graph captures keep the
catalog owner's facts but omit the presentation operation/cause. The bounded
capture fields and scenario snapshot expose `graph_visible`. Two new tests
cover tab activation, closed/open presentation, canonical-tree preservation,
and accepted-fact suppression. Existing Save diagnostics remain scoped to the
explicit Save command; other save flows do not acquire a fabricated causal chain.

Verification before the shared paint repair:

- Locked workspace suite: 490 passed, zero failed, one existing diagnostic
  timing ignore. This completed before the final capture-adapter correction.
- After that correction, all three capture-adapter unit tests passed, including
  both new regressions; production desktop/readings library and binary Clippy
  passed with warnings denied, and the final locked binary build passed.
- `git diff --check` passed. Build and bundle SHA-256 agree:
  `773959b032b8c063b7266578cc6e4d28534e8bd18a3dbfe824849d526c35b982`.
- Fixture SHA-256 remains
  `3d2a2bbd8fb43be0bcfa6086af5abd6f1f0b2f40f4ff05bc4fb8e9c2b4d331be`.

Logs, isolated app, scenarios and receipt destinations are under
`/Users/markik/Code/testing/knot-editor/collapse-integrated-20260930`.
The first native launch was blocked by the locked Mac. After native access
returned, the 320px lane passed at 2527 frames with ten captures, including
active/background Graph metadata. The 400% zoom lane passed its assertions at
2452 frames with five captures, but visual review confirmed the existing
selection/caret overlay defect. These are preserved under
`receipts/320-before-paint-fix-773959` and
`receipts/1280zoom-before-paint-fix-773959`.

`MESQUITE_CAPTURE_PAINT=1` saved actual paint streams beside the baseline PNGs.
The zoomed reading-body capture contains 207 commands: the final DOM clip
closes at index 203, then selection rectangles 204/205 and caret 206 paint
with zero active clip, transform or layer scopes. The PNG visibly shows that
source selection above the drawer's Close and Preview controls. This confirms
the shared Rootstock emission order, not a Knot drawer-color defect. The
native and browser hosts share that path. The decoded command summary is
`baseline-zoom-paint.txt` beside the receipts.

The repair inserts selection then caret after field content, within its CSS
clip/scroll/transform/layer context and before later overlays, including
positive-z descendants. This preserves translucent selection over syntax
backgrounds rather than suppressing selection. Existing IME and caret geometry
APIs are unchanged. Genet has 38 passing relevant checks, including five new
slot-order regressions. Mere has 64 passing checks, including six production
text-paint regressions and a failing control for the previous global append.
The web-host wasm32 compile check passes; this is not browser runtime acceptance.
Knot's repaired locked workspace suite passes 492 tests, with zero failures and
one existing diagnostic timing ignore. Strict desktop/readings Clippy passes
with warnings denied. The locked native build passes; build and isolated bundle
SHA-256 agree at
`5f355e7daac67578c1d98eeef085ed1510c5976fbec290bbaecafe667e035740`.
The fixture digest remains unchanged. Native access was restored and the final
binary passed all five lanes, producing 26 nonblank captures:

| Logical viewport | Frames | Captures | Physical PNG size |
| --- | ---: | ---: | --- |
| 320x700 | 2527 | 10 | 640x1400 |
| 640x700 | 2458 | 5 | 1280x1400 |
| 1100x700 | 2426 | 3 | 2200x1400 |
| 1280x700 | 2426 | 3 | 2560x1400 |
| 320x175 (1280x700 at 400%) | 2452 | 5 | 2560x1400 |

Four lanes enable capture pairing; the normal 1280px lane keeps the default
unpaired path. The 320px lane verifies active/background Graph visibility and
restores the menu threshold to 700px. Fixture identity and clean-document
assertions pass; the disk digest was rechecked after the matrix.
All 26 images were visually reviewed using uniquely named copies, and all 26
paint streams decoded with balanced clip/transform/layer stacks. No new visual
blocker was found. This is native acceptance, not full WCAG/VoiceOver or browser
runtime acceptance.

The repaired zoom paint stream still has 207 commands, but selection commands
78/79 and caret 80 now occur with two active clips and one transform, before
the drawer paints. `fixed-zoom-paint.txt` records the decoded stream. Visual
review confirms clean Close Readings and Preview controls; after closing the
drawer, source selection and caret remain visible within the source viewport.

An interrupted launch had already completed the zoom run; a later retry hit
existing paint-sidecar files and produced a failed receipt. That directory is
preserved as `receipts/1280zoom-5f355e-interrupted-relaunch`. A clean final rerun
returned RESULT ok, and all five PNGs are byte-identical to those already
reviewed. Baseline and interrupted artifacts are not counted as final receipts.
Earlier native receipts below apply only to their named isolated binary.

### Accepted isolated candidate, 2026-09-30

Final frozen binary SHA-256:
`9feb2c3fdf5a03844ee833bafd03daa8b0c4039cb5ae83b02a2bc4e3a0fa1cbf`.
The compiled binary and isolated app bundle agree. Fixture SHA-256 remains
`3d2a2bbd8fb43be0bcfa6086af5abd6f1f0b2f40f4ff05bc4fb8e9c2b4d331be`.

- Locked workspace: 482 passed, zero failed, one ignored diagnostic timing test.
- Strict desktop/readings library and binary Clippy: passed, warnings denied.
- Final app lane: four Collapse integrations and 15 focus checks passed.
- 1280x700 at 400% zoom: native RESULT ok, 2452 frames, five distinct nonblank
  captures at 2560x1400. Inspected source, compact menu, initial drawer,
  preview-heading reachability and restored source. Site/Menu and status remain
  visible after close. The source-focus path anchors outer workspace scrolling
  without resetting nested source/reading scrolling or selection.

After unlock, all ordinary-width lanes were rerun against this same frozen
binary. Every lane returned native RESULT ok with zero blank captures, and all
24 captures were visually inspected using fresh, uniquely named review copies.

| Logical viewport | Frames | Captures | Physical PNG size |
| --- | ---: | ---: | --- |
| 320x700 | 2500 | 8 | 640x1400 |
| 640x700 | 2458 | 5 | 1280x1400 |
| 1100x700 | 2426 | 3 | 2200x1400 |
| 1280x700 | 2426 | 3 | 2560x1400 |
| 320x175 (1280x700 at 400%) | 2452 | 5 | 2560x1400 |

The 320px lane covers compact menus, reading and Navigator drawers, Escape
focus return, status overflow, Appearance, and a saved threshold adjustment
restored to 700px. The 640px lane covers manual inline reading opening and
closing. At 1100px the fixture's writer measure requires a reading rail; at
1280px source and preview fit side by side with Navigator folded first.
The fixture digest above was rechecked after the complete matrix.

This acceptance is for the isolated `codex/knot-collapse` branch based on
`8bc98c7`, with Mere pinned to `20021477cf59d61ace1c8c75666f3836a5f41fcf`.
Remote main advanced concurrently to
`3dfb70b01e79dadfcbd1e615ded43b34f01802da`, adding catalog diagnostics and
overlapping workspace and Mere/Genet dependency changes. This candidate is
accepted for isolated branch publication, not merged into main. Integrating
the newer main requires reconciling those changes and verifying the combined
revision; these receipts must not be attributed to that untested combination.

Historical issue in this isolated candidate (repaired in the combined build
above): focused source selection/caret paint is emitted above
DOM overlays and can spill over the drawer or tabbar. This capture observes it;
the Collapse scrolling repair does not fix it. No full WCAG or VoiceOver
acceptance is claimed. Status refusal priority is verified in shared tests,
not by a native refused-state capture. Only one threshold control's native
save/restore was exercised; the broader persistence matrix is automated.

Image review must use unique filenames when replacing candidates: reusing a
capture name displayed stale pixels during review. The uniquely named
`receipts/1280zoom/final-9feb-restored-review.png` confirms restored chrome.
All earlier failed and superseded receipts below remain preserved.

### Implementation and failure chronology

Mere status tests: eight component tests and four retained-host tests pass
(two new collapse geometry checks plus existing squeeze/switch checks).
The current complete Cambium component suite passes 244 tests. These are
implementation-stage receipts; they are not combined Knot/native acceptance.

Independent Knot retained-host checks passed the resize/font matrix, canonical
share restoration, drawer bounds, source snapshot/preedit/disk preservation and
1280px at 400% zoom producing a 320px logical viewport. Full and system-monospace
policy fallbacks and preservation of unknown preference fields also passed.

Native artifacts are isolated under
`/Users/markik/Code/testing/knot-editor/collapse-20260929`. The original 320px run
failed at 2456 frames despite five captures: text-only menu selectors selected
ancestor items, so the commands did not execute. The next run at 2469 frames
opened the reading drawer but failed exact-editor focus restoration after
scenario Escape. Both failed receipts are preserved, not counted as acceptance.
Inspection also found horizontal clipped rail text, off-viewport compact-menu
flyouts and transparent overflow headings. Regression-backed repairs must be
captured together before native acceptance.

A later full-suite rerun exposed a test-only real-loopback transport lifetime
race: the reader's owned endpoint could shut down before the holder finished
its response. The test now borrows that endpoint through both joined futures;
eight focused repetitions passed. Production networking is unchanged.

Repository-wide formatting check reports pre-existing drift in unrelated files;
only the new integration test was formatted, and diff whitespace checks pass.

Final repaired shared revision: Mere
`20021477cf59d61ace1c8c75666f3836a5f41fcf`, published only to
`codex/knot-collapse`. All Knot Mere references are pinned to that revision.
The final Cambium component suite passes 244 tests; the shared retained-host
matrix passes nine checks (four rails/drawers, one short compact menu, and four
status checks). These are retained-host checks, not OS captures.

The rebuilt native candidate's SHA-256 is
`b50b31fd148541130c3a841a6e96016df182a75f825fa761242fc1a44572e94c`.
Its isolated fixture SHA-256 remains
`3d2a2bbd8fb43be0bcfa6086af5abd6f1f0b2f40f4ff05bc4fb8e9c2b4d331be`.
On 2026-09-30 the native launcher was blocked by the locked Mac before any
repaired-candidate capture. Acceptance at 320/640/1100/1280 CSS pixels, including
the real 1280px/400% zoom capture, remains pending. Knot has not been committed,
fast-forwarded to main or pushed for this slice.

Earlier locked workspace suite: 481 passed, zero failed, one ignored. The four
independent Collapse integration checks are included. Strict desktop/readings
library and binary Clippy passes with warnings denied. The build, workspace and
Clippy logs are stored beside the isolated native candidate. These automated
results do not close the pending native visual gate above.

After unlock, candidate `b50b31fd...` passed native assertions at 320px
(2485 frames / seven captures), 640px with a manually opened inline reading
(2458 frames / five captures), and 1100px (2426 frames / three captures).
The captures were inspected. The 1280px lane's `zoom 4` command was rejected by
Mesquite's 200% receipt-command cap, so that failed receipt is preserved. A
separate launch using the normal `CAMBIUM_UI_ZOOM=4` host receipt aid reached
320x175 logical pixels and passed its assertions (2437 frames / four captures),
but visual review rejected it: the 240px workspace frame minimum and gaps around
absent controls pushed source/status out of the short viewport. The independent
zoom regression now tests 1280x700, not a tall 1280x2800 window, and requires
visible source and status. A repaired binary and full native matrix are needed.

The final short-height repair removes absent-control DOM slots, allows the
workspace frame to shrink below 240 CSS pixels, and reduces only vertical
chrome spacing in short viewports. Appearance remains scrollable and capped
at 20vh. Its regression exercises Appearance closed/open/closed, source
visibility, status bounds and reading-drawer bounds at actual 320x175 logical
geometry. The locked workspace suite now passes 482 tests, zero failed, one
ignored. Incidental repository-wide formatter changes were removed; the
unrelated baseline formatting drift remains outside this slice.

Intermediate repaired binary SHA-256:
`f6e36c2b66de3b8890d68bf1655bc1ce4a04dc3e2f7b1d2878e0b8cf2ef2e727`.
The 640px native run passes at 2458 frames with five inspected captures,
including a manually opened inline reading. The 1280x700/400% launch passes
at 2437 frames with four captures: source and status are now visible, but the
initial reading drawer shows only its header. Preview-body scrolling and
reachability still need an explicit receipt before accepting that lane.

The added preview-heading reachability regression passes. The final source
build after that test addition is SHA-256
`4239cc4ea130717327f2ebce84e580cb95942ec84370bad4facc70866dbc9da2`;
two consecutive locked builds agree. The final workspace suite remains
482 passed, zero failed, one ignored, and strict desktop/readings Clippy passes.
Native heading navigation reveals preview text but its subsequent Close and
Escape sequence leaves the drawer open. That failed receipt (2452 frames,
five captures) is retained. Publishing and the exact-candidate matrix remain
blocked on this interaction; no final Collapse acceptance is claimed.

Isolation found the Close selector matched the covered background rail, not
the actual header button. Targeting `.frisket-panel-close` passes the native
interaction. Visual review then caught a separate 12 CSS-pixel root viewport
scroll after preview-heading navigation: the toolbar left the viewport and a
blank strip appeared at the bottom. A retained-host regression reproduces it.
The app's fixed-height workspace must contain its overflow while source and
reading panels retain their own scrolling. Final acceptance awaits that fix.
