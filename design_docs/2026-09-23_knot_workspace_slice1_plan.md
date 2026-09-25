# Knot workspace slice 1: frame, tiles, status, sites and graph

**Date:** 2026-09-23
**Status:** Plan. Assessment done 2026-09-23; the decisions below were ruled
by Mark the same day. Implementation not started.
**Owner:** Knot Editor
**Carries:** slice 1 of [the design pass](2026-09-23_knot_design_pass.md#slice-order),
which includes the first workspace slice and the navigator slice of
[the workspace plan](2026-09-05_knot_application_workspace_plan.md).

## What slice 1 is

The standalone desktop becomes a Workbench frame of tiles. Documents open as
tabs in a centre stack; readings (Outline, Preview, Folded source, Readings,
Changes) are tiles in a right stack that follow the focused document or pin
to one; the Navigator and each open site are tiles in a left stack; a status
bar of authority chips replaces the blocks above the editor. When the last
document closes, a graph view of the catalog fills the centre. The title bar
stays OS-drawn; a provisional command row carries the commands until slice 2.

Ruled into slice 1 during the assessment, beyond the design pass summary:

- **Sites.** The site tile (design ruling 5) and per-document site state land
  here, so Scroll, Gemtext and Micron pages open as tabs like any document.
- **The empty centre.** Closing the last document shows a Mere graph view:
  catalog documents as nodes, links extracted from them as edges, laid out by
  a cartography strategy, opened from the graph.
- **Quitting with several unsaved documents** asks once, in one summary
  prompt.
- **The instrument comes first.** A generic self-drive scenario lane in the
  Cambium host, which Knot consumes, proves the slice headed.

Out of scope, each owned by a later slice: the client-drawn title bar, menus
and palette (2); faces, the measure and wrap detection (3); collapse rules,
rails and drawers (4); the font picker (5); macOS menus (6); local recovery
(after 1); document relations (after 2); workspace layout persistence. Tab
drags between stacks, user-made splits and floats are declined in this slice
(the tree is unchanged and the gesture is logged) and adopted later from the
same model.

## Findings from the assessment

These are the facts the plan is built on, checked against the tree at
knot-editor `c5a5946`, mere `250fd238` and genet `532f1fad`.

**One struct, one document.** `DesktopState` (`apps/desktop/src/workspace.rs`)
holds about fifty fields for exactly one document: the document surface,
catalog binding, prepared capture, comparison, outline, fold and preview
state, rhai reading results, the pending lifecycle action, retention, and the
whole site workspace. It splits three ways:

| Owner | Fields today |
| --- | --- |
| App | `appearance`, `preferences`, `window`, `message`, readings root and loaded scripts, retention targets and wake, catalog handle, capture limit |
| Document entry | `document`, `path`, catalog binding and id or error, prepared capture, comparison, outline, fold and preview snapshots, reading results and their source, retention busy, receipt and error, `pending`, `micron_folds` |
| Site entry | from `ScrollWorkspace`: folder, format, port, server, metadata fields and baseline, publication number, Titan and Spartan submission state |
| Page (in its document entry) | from `ScrollWorkspace`: page, Micron form, Micron submissions |
| Tile | which reading, follow or pin, selected script; view flags that are today global booleans |

**Focus assumes one document.** `focused_text` routes the host's text input by
walking up from the focused node, and treats any focused `textarea` as the
document. `after_dispatch` returns focus to the first `textarea` under
`.knot-document-body`. Both need the document's key in the DOM.

**Lifecycle semantics change.** `New` and `Open` replace the one document
today (`PendingAction::{New, Open}` with a dirty preflight). With tabs they
add or activate a tab and need no preflight. Several desktop tests (90 test
functions in `apps/desktop/src` and 9 in `apps/desktop/tests` at `c5a5946`)
assert replacement and are rewritten, not deleted.

**The frame fits, with two constraints.** `cambium::workspace_view` takes
`fill: Fn(&Tile) -> Slot`, which never sees host state, so each tile's view
reaches its data through a `lens` keyed by that tile's document, site or
reading key; a tile and its entry are removed in the same update.
`TileTree::apply(Closed)` removes a stack when its last tile closes, so
reopening a reading or Navigator re-inserts a stack with `split_beside`. A
drop outside the window returns a `TearOut` effect, which slice 1 declines.

**The document surface draws its own status.** `knot_document_view_with_highlighting`
renders "Source · Format · Clean · Posture · Save" and a Save button inside the
surface. Turnstone embeds the same surface through `knot_document_surface`,
pinned at knot-editor `44f0519`, so the embedded status stays the default and
the desktop gets an additive host-owned-status option.

**Readings stamp fixed ids.** `knot-document-preview`, `knot-document-folding`
and `knot-readings` would collide between two pinned tiles of one kind; ids
become per tile.

**Tabs cannot carry a mark.** Frisket builds each tab from `Tile.title` and an
optional accent tint. Dirty and refused marks (design ruling 2) need an
additive field on the Workbench `Tile` and a marker on Cambium's `TabItem`.

**The catalog is optional.** It exists only when the desktop is launched with
`--catalog-root` and `--catalog`. `KnotFileCatalog::records()` returns every
binding with `Available` or `Unavailable`, which is what the Navigator and
graph need.

**The graph has every part in the stack.** Graph-kernel's public write path
(`add_node`, `assert_relation`, through `apply_graph_delta`) builds a
rebuildable reading graph. `mere-cartography` projects it through
`LayoutStrategy` adapters (Grid, Radial, Spectral, SemanticEmbedding and the
macro-built family). Cambium's graph Swatch paints it as one leaf with native,
labelled hit targets. `knot_readings::links::extract` already pulls Djot links
with targets, rel and spans.

**The instruments exist.** The in-process `Harness` lays out and measures
(`layout_at`, `painted_rect`, `resolve`, the accessibility tree), so
behaviour and geometry are headless tests. For headed runs, the host exposes
`read_frame`, the `after_frame` hook and `HostPointer` routing, and its
`examples/smoke.rs` is the reference wiring of taproot's `Automatable` and
`Driveable`. Woodshed carries its own copy of that wiring.

## Decisions ruled 2026-09-23

| Question | Ruling |
| --- | --- |
| Sites in slice 1 | The site tile and per-document site state land in slice 1. |
| The empty centre | A Mere graph view takes the Start tile's responsibilities. |
| Quitting with several unsaved documents | One summary prompt. |
| Instrument | Build the self-drive lane first. |
| Where the lane lives | A generic scenario module in `cambium-genet-winit-host`; Knot is its first consumer and the smoke example moves onto it. Woodshed may migrate later. |
| Graph scope | Catalog documents and open documents as nodes; links extracted from available documents as edges; a cartography strategy lays it out, Spectral by default, with the strategy as a preference. |
| The mere view (later the same day) | One Cambium component, V2b of the reservoir plan, embedded by every application; no app-local mere view. Step 8 re-scoped accordingly. |

## Design calls this plan makes

Each has an obvious default; reject any of them in review.

- **Keys.** Runtime `DocKey` and `SiteKey` values; a Save As never changes
  them. Duplicate open is detected by catalog id when bound, otherwise by
  canonical path, and activates the existing tab.
- **Tile kinds** use `ContentSource::Open { kind, id }`: `knot.document`,
  `knot.reading` (outline, preview, folded, readings, changes; follow or a
  pinned `DocKey`), `knot.navigator`, `knot.site`, `knot.graph`.
- **Placement.** A new document opens in the stack holding the focused
  document. A reading opens in the stack of the most recently focused reading,
  otherwise in a new stack right of the documents. The Navigator and sites
  open in the left stack, otherwise in a new stack left of the documents. The
  graph opens in the document stack.
- **Closing.** A clean document tile closes at once. A dirty one prompts
  Save, Discard or Cancel, and its tile stays until the decision; Cancel,
  failure and custody refusal leave both tile and document. Closing the last
  document tile puts the graph tile in its place. Reading, Navigator and
  graph tiles close without a prompt; a site tile closes through the
  dirty-page preflight of its open pages.
- **Quitting.** One prompt lists every dirty document with Save all, Discard
  all, Review and Cancel. Save all saves file-backed documents; a scratch or
  refused document stays listed, since it needs Save As or a discard, and the
  window closes only when none remain dirty. Review activates the first listed
  document and dismisses the prompt. Cancel changes nothing.
- **Status bar** is a Cambium component: the message line (`aria-live`
  polite) and chips with a severity (quiet, warning, refused), each able to
  open an `overlay_surface` holding a `detail_panel`. Knot's chips: format,
  save state, posture, catalog, retention, and serving when a site page is
  focused.
- **Tab marks.** *Changed in step 2:* marks are a host-supplied lookup
  (`TileId` to an optional `TabMark`) passed to `frisket_with_marks` and
  `workspace_view_with_marks`, not a field on the Workbench `Tile`. A `Tile`
  field would have broken every struct literal that builds one (about fifteen
  files across mere, woodshed and isometry), and dirty or refused is live
  document state that must never be saved into a layout. `TabItem` carries the
  mark; the glyph is hidden from assistive technology and the tab announces
  "Unsaved changes" or "Needs attention" as its `aria-description`.
- **Changes tile.** The disk comparison and the saved-revision review move
  there from the blocks above the editor.
- **Command row** (provisional until slice 2): New, Open, Save, Save As,
  Reload, Compare, the reading toggles, Navigator, Graph, Site and Appearance.
  Open shows a single-line path field in a popover; the path textbox leaves
  the top of the window.
- **Launcher.** Several document paths open several tabs; a site folder opens
  its site tile and index page.
- **What Knot hands the mere view** (the component itself is Cambium's; see
  step 8). Unavailable documents are dimmed and labelled. Links are read on a
  worker within the capture limit, and the reading is labelled with the
  catalog revision it read. Without a catalog, the view shows open documents
  and says no catalog is configured. Activating a node opens or activates its
  document.

## Steps and done-conditions

Sequential. Each step ends green on its tests before the next begins.

**0. Baseline.** The desktop and crate test suites run at `c5a5946` and their
counts are recorded here; `knot.exe` builds.

**1. The scenario lane.** In mere: a scenario module in
`cambium-genet-winit-host` that runs a taproot `Scenario` from the
`after_frame` hook, delivers pointer input through `HostPointer`, captures with
`read_frame`, writes captures and a sentinel, and asks the application only
for a snapshot, its events, its `act` verbs and whether it is busy. The smoke
example moves onto it. In Knot: `KNOT_SCENARIO` and `KNOT_CAPTURE_DIR` wired
through it. *Done when* the smoke scenario passes on the generic module
unchanged; a Knot scenario launches, clicks a control by role and label,
asserts a snapshot field, captures a frame whose PNG shows the window, and
writes its sentinel; and Knot's ordinary launch and tests are unaffected
when the variables are unset.

**2. Cambium additions.** The status bar component and tab marks, with their
own tests in mere. *Done when* the component's roles, live region, severity
classes, chip activation, popover and Escape dismissal are tested; a `Tile`
round-trips through serde with and without a mark; and Frisket renders the
mark with an accessible description.

**Checkpoint A.** Mere is committed and pushed and Knot repinned to it, with
Mark's sign-off, before any Knot commit that depends on it. Knot never
commits a local path patch.

**3. The workspace.** `DocumentWorkspace`, the three stacks through
`cambium::workspace_view`, tabs, document keys in the DOM, focus routing and
focus return by key, the close and quit prompts, the launcher, the command
row and the Open popover. *Done when* (the workspace plan's first-slice
conditions, carried over) two file sessions and one scratch session coexist;
a duplicate open activates the existing tab; each entry keeps its exact
source, selection, undo, dirty state and derived readings across activation;
a dirty close keeps Save, Discard, Cancel and custody refusal; the quit
prompt behaves as specified; the editor's painted width matches its tile's
content width within one pixel at 1100 x 700 and at 640 x 700 (D1); the Open
popover shows a long path on one line (D6 absent there); and the rewritten
desktop suite passes.

**4. Reading tiles.** Outline, Preview, Folded source, Readings and Changes as
tiles with follow and pin. D2, D3 and D4 fixed. *Done when* a following tile
switches with the focused document, a pinned tile does not, and two pinned
Previews coexist without an id collision; selecting a row or heading returns
focus to the right document's source range; stale readings label themselves;
and the comparison and saved-revision tests pass in the Changes tile.

**5. Status and marks.** The status bar, chips and popovers; the retention and
catalog blocks and the surface's embedded status retired from the desktop.
*Done when* a test enumerates every authority fact the old blocks showed and
finds each in a chip or popover; a refusal raises its chip and posts its
sentence; tabs carry dirty and refused marks; and the retention tests pass
through the popover.

**6. Navigator.** *Done when* (the workspace plan's navigator condition)
available and unavailable catalog history is labelled truthfully without
changing source bytes; selecting an available record opens or activates its
document; and a launch without a catalog says so.

**7. Sites.** Per-site and per-page state, the site tile, native pages as tabs,
metadata as a tile, publication per site. D5 fixed. *Done when* two sites are
open at once, each with its own server and submission state; the Preview
tile follows the focused page in its format's presentation; Micron forms stay
with their page; the existing site and Micron tests pass; and paths display
without the verbatim prefix.

**8. Graph.** *Re-scoped 2026-09-23 by Mark's ruling on the mere view,
relayed by the Cleromancy session:* "3 by way of 2. cambium should be the
solution for all". The mere view is one Cambium component, built as V2b of
Mere's reservoir plan beside V2's session lifecycle; Graphshell presents it
and every application embeds it, so Knot builds no graph view of its own.
Step 8 becomes embedding V2b as the tile shown when the last document closes
and from the command row. Knot's requirements were sent to that session for
V2b's design: embedding behind a keyed lens at any tile size, host-owned
activation and lifecycle requests, a host action slot for New and Open,
host-supplied node states, distinct edge provenance, the layout strategy as a
preference, native labelled hit targets with stable keys, truthful empty and
error states, and host theming. Whether Knot shows a throwaway interim before
V2b lands, or step 8 waits for it, is Mark's call when step 8 comes up; the
catalog-and-links scope ruled earlier describes what Knot hands the component.

**9. Headed receipts.** Scenarios for focused writing, source beside preview,
research with references, the last-tab graph and a two-site session, with
captures compared against the design pass frames. *Done when* each scenario
passes and its frames are reviewed whole-frame, with differences from the
design frames either fixed or recorded.

## Risks

- **D1's cause is unconfirmed.** If the editor surface ignores its tile's
  width inside Frisket too, the fix lies in how the editor leaf is sized, in
  Cambium or Livery.
- **`scroll_site.rs` is 3,244 lines** of site, page and submission logic with
  global state; step 7 is the largest refactor in the slice and leans on its
  existing tests.
- **Two mere lines.** Knot repins to a new mere while Turnstone stays on
  `250fd238` until it repins; the lines diverge in between, as they have
  before.
- **Link reading cost** grows with the catalog; the worker and the capture
  limit bound it, and a large-catalog probe belongs in step 8.

## Progress

- **2026-09-23, step 0.** Baseline at `3ff250b` (Rust 1.97.1, mere `250fd238`,
  genet `532f1fad`), `cargo test --locked`, all green. Workspace: desktop
  library 85 passed and 1 ignored; launcher 4; desktop `preferences` 4 and
  `readings_panel` 4; knot-editor library 147, its binaries 4 and 4, and its
  integration suites 2, 3, 5, 4 (1 ignored), 1, 2, 2, 1, 1, 4, 3, 1, 2, 1;
  knot-file-catalog 8; knot-readings 12 and 7. Standalone: knot-document with
  all features 47 passed and 1 ignored; knot-site 4, 2, 1, 4, 3 and 3. The
  first full build after the morning's repin failed to link eight
  `knot-editor` test targets with "required to be available in rlib format";
  an immediate rerun linked every target without a clean, so it is recorded
  as a transient artifact race, not a defect.
- **2026-09-23, coordination.** Mere's working tree is shared with the
  Cleromancy session, whose pandect work is uncommitted there, and local
  `main` carries four other sessions' unpushed commits. The Cambium work
  therefore happens in the worktree `Code/worktrees/mere-knot-slice1` on
  branch `knot-slice1-cambium` from `250fd238`, Knot's pin, so patching the
  Cambium crates cannot mix two mere revisions. Moving that branch onto
  mere's `main` for a push is Checkpoint A's question. No shared mere-view
  component is assigned yet; Cleromancy's session is raising it with Mark,
  and suggests step 8 build over pandect's reservoir types (committed
  locally at mere `fe5adc1a`, unpushed and early).

- **2026-09-23, step 1.** In the mere worktree: `cambium-genet-winit-host`
  gains a `scenario` module (`ScenarioLane`, `LaneApp`, `LaneConfig`,
  `CaptureRecord`, and `ProbeSnapshot` re-exported so applications need no
  direct taproot dependency). Selectors resolve through the host's live layout,
  as `Harness::resolve` does; captures are PNG through `png 0.18`, already in
  mere's lock; steps hold while a capture is in flight, and a capture that never
  lands fails the receipt after 120 frames instead of hanging. The smoke
  example moved onto it, keeping its alpha receipt lines in the form
  `x11-shadow-receipt.ps1` matches; its captures are now PNG, which no script
  reads. Six headless lane tests pass; the unchanged `smoke.scn` passes headed.
  In Knot: `KNOT_SCENARIO`, `KNOT_CAPTURE_DIR` and `KNOT_RECEIPT` wire the lane,
  `DesktopState::scenario_snapshot` supplies the facts, `scenarios/lane_smoke.scn`
  clicks Appearance by role and label and captures two 2200 x 1400 frames
  headed, and `tests/scenario_lane.rs` runs the same scenario headless. The
  desktop suite is unchanged against the worktree (85 and 1 ignored, 4, 4, 4,
  plus the new lane test). Uncommitted until Checkpoint A: Knot builds against
  the worktree through a gitignored `.cargo/config.toml` that patches all 52
  mere packages together. The headed capture shows the editor box running past
  the window's right edge, the D1 symptom, now documented from inside the app.
- **2026-09-23, step 2.** Cambium gains `status_bar` (a labelled region, a
  polite `status` message, chips as buttons with `data-severity` and
  `aria-expanded`, a popover anchored above its chip by CSS over a fixed
  outside-click layer, Escape and outside click returning focus to the chip,
  and `STATUS_BAR_CSS`) with five tests, and tab marks as described under the
  design calls, with a test each in `tabs.rs` and `frisket.rs`. The popover is
  anchored in CSS because `overlay_surface` needs measured trigger and panel
  geometry, which a chip docked at the bottom of any application does not have
  before layout; Livery supports `position: fixed`. Cambium's 223 unit tests,
  Workbench's 19 and the host's suites pass. Two `ui_zoom` host tests fail
  identically on a pristine checkout of `250fd238` (a painted box of 132 x 42
  where the test expects 120 x 40), so they predate this work. Clippy is clean
  in every touched file; only touched files were formatted.
- **2026-09-23, Checkpoint A.** With Mark's sign-off the Cambium branch was
  rebased onto `origin/main` and pushed as three commits: `ad7f63e2` (the
  scenario lane), `2a93c836` (tab marks) and `9ad5990b` (the status bar).
  Knot moved all 35 mere pins to `9ad5990b` in `fec7c63`, which also carries
  the step 1 wiring; the gitignored patch config is gone. Turnstone stays on
  `250fd238` until it repins. The two `ui_zoom` failures remain in mere,
  unfixed.
- **2026-09-23, step 3a** (`75cf5cc`). `DocumentWorkspace` keeps open
  documents by a runtime `DocKey` that no Save As changes, maps each tile to
  what it shows, resolves a duplicate open by identity, and moves the focus
  when a tile closes. Eight unit tests, generic over the payload.
- **2026-09-23, step 3b-1** (`ce20159`). Everything the desktop derived from
  its one document moved into a per-document entry, behaviour unchanged. The
  suite passed at 93, and the feature-gated resident retention test, which
  the baseline had not run, passed too.
- **2026-09-23, step 3b-2.** Tabs. The frame is `workspace_view_with_marks`;
  its fill closure sees no host state, so each tile renders through an
  identity lens that hands the tile view the whole window state. A document
  tile carries `data-knot-document`, the text focus routes by that key, and
  focus return after an outline activation looks for the focused document's
  editor. New and Open add tabs, and opening a path already open activates
  its tab and says so. A clean tab closes at once; a dirty one asks Save,
  Discard or Cancel and keeps its tile until the decision, and a refused save
  keeps both. Quitting asks once, as specified above, and Save all leaves the
  focus where it was. Closing the last tab shows a one-line empty frame until
  step 8's graph. A dirty document's tab carries the unsaved mark. The tab
  bar follows the design pass stylesheet: a 30px bar, 13px chrome, the active
  tab on the surface joined to its content, inactive tabs transparent, and
  the close control shown on the active or hovered tab, with colours from
  the Tinct palette through `appearance_css`. The window still scrolls as a
  page: the frame takes its natural height, so the Micron viewport tests pass
  unchanged, and scrolling inside tiles arrives with the reading tiles in
  step 4.

  The desktop library passes 100 with 1 ignored; the other desktop suites
  pass as before, the resident retention test included. Seven tests are new:
  two files and a scratch keeping text, selection, undo, dirty state and
  marks across activation; typing after switching tabs; a duplicate open
  through another spelling of the path; clean and dirty tab close through
  Cancel, a refused save and Discard; save on close; the empty frame and
  New; and the quit summary. Rewritten for the new semantics: Open no longer
  prompts but adds a tab; the comparison and the reviewed revision stay with
  their own document; and the catalog-failure test saves a file outside the
  root, since Save all does not Save As a scratch. The suite now builds its
  sheet from `desktop_sheet()` instead of a partial copy.

  Headed, a throwaway scenario (launch with a file, New, back to the file,
  and the same in the dark theme with a scratch `APPDATA` so no real
  preference was written) passed with distinct frames, kept under
  `Code/testing/knot-editor/images/2026-09-23_tabs/`. The frames still show
  D1 and D6, which 3d and 3c own. Still to come in step 3: the multi-file
  launcher, planned with this step and following as its own commit; the
  command row and the Open popover (3c); and the D1 geometry test and fix
  (3d).

  Two things noticed on the way. Scratch tabs read `scratch:untitled`, the
  surface's own label, so two scratch tabs look alike. And `cargo fmt --all`
  rewrites 19 files in `knot-editor` and `knot-site`, since the tree predates
  the canonical rustfmt policy's sweep; the policy wants that sweep in its own
  commit on a quiet tree, so it was reverted here and only desktop files were
  formatted.
- **2026-09-24, a fix to 3b-2** (`52a4957`). The site panel's page, which
  picks the metadata shown and the config entry Save metadata writes, was
  synced only on open, so after a tab switch it named the previous page. It
  now follows every focus change, and unsaved metadata edits hold the focus:
  another tab, closing the focused tab, and the quit prompt's Review all
  refuse with Open's message. Its test fails without the sync.
- **2026-09-24, step 3b-3.** The launcher takes several paths and opens them
  as tabs in the order named. Mark ruled the three open questions: the first
  path stays in front; a path that fails is reported in the message line
  while the rest open, and only a launch where nothing opens stops with the
  error; and two site folders are refused ("one site folder at a time until
  sites get their own tiles") until step 7. A site folder named after a file
  holds the site panel and opens its index page behind. The failure line
  names each path once: document errors already carry it, site errors get it
  added. `run_desktop_with_targets` now takes a `DesktopLaunch`. Desktop
  library 102 with 1 ignored, launcher 7; a headed launch with two files and
  a missing path between them showed both tabs, the first in front, and the
  failure line.
- **2026-09-24, step 3c.** The command row and its path popovers. Open and
  Save As are Cambium popovers below their buttons, each holding a one-line
  path field and the command's own button; Enter runs it, and the field
  takes focus when the popover opens. Save As starts from the focused
  document's own path without the Windows verbatim prefix (D5), and Save on
  a scratch document opens it with "Choose where to save". Ctrl+O and
  Ctrl+Shift+S open them. The row gains Save and an accessible name and
  loses the standing Path field, and the panel takes the document surface's
  colours. D6 is absent in the popover: a long path stays on one line at
  1100 and at 640 wide.

  Fixes below Knot, each with a test that fails without it. In Cambium
  (mere `175b5829`, `d938c708`, `52d0a146`, `5be48683`): a field lost its
  selected text when it had no composition; the host's caret, selection and
  hit geometry was keyed by element, so a field had none; a single-line
  field neither stayed on one line nor followed its caret; and the popover
  itself, which the status bar's chips now use. In Genet, fixed there on
  Mark's rulings: an inline-block's text never reached the retained text
  frame, so inputs and textareas had no caret geometry (`3bf7b0276e6`);
  percentage insets resolved against the containing block's width, which
  put the panel off screen (`22ebde46ae3`); that first fix drew an
  inline-block's border around each of its lines, striking through the
  editor text (`99ba03e6406`); and positioned children counted toward a
  block's intrinsic width, so an open popover's anchor took the whole row
  (`16ca28adda3`). The last two showed only in the headed check, whose
  frames are under `Code/testing/knot-editor/images/2026-09-24_3c/`, the
  failing ones kept as `-before`. Mere repinned to each Genet head
  (`e8fa4b20`, `2fae9ef4`) and Knot followed (`a663b69`, `3bb1684`); the
  first repin moved Knot's graphshell carriers onto graphshell-endpoint's
  `stdio` and `local` features.

  The desktop library passes 105 with 1 ignored: new tests for Save As from
  a focused popover field, Save As prefilled from the document's path, a
  long path on one line, and a command row an open popover leaves in place;
  the Open, Save As and shortcut tests now go through the popovers.
  Everything else passes as at `3bb1684`.

  Noticed, not fixed here. Shift+End does not extend a text field's
  selection; the tests set it directly. With Save added, Readings wraps to
  a second line at 1100 wide. Genet measures a `pre-wrap` inline-block with
  its newlines collapsed, so its text can overrun its box.
  cambium-genet-winit-host's two ui_zoom identity tests fail because genet
  `8a44e944de1` gave buttons UA padding without `border-box`, and
  genet-render's accessibility bounds test is off by the same 12px; both
  went to the Genet roadmap session. genet-parley's test target does not
  build, and cambium-nematic's has the known `views.rs:513` break. Scratch
  tabs still read `scratch:untitled`.
- **2026-09-24, step 3d.** D1 had one cause behind both of its faces. Genet
  lays every inline-block out in a pre-pass against the viewport, before its
  real containing block is known, so the editor textarea's `width:100%`
  meant the window: 1100 wide in its 900 column, and under the outline when
  that was shown. The editor textarea is now `display:block`, as an editor
  surface is anyway, and Genet sizes a block against its column. Mark ruled
  to close D1 here and to take the Genet fix next, before step 4.

  A test holds the editor to its column within a pixel at 1100 x 700 and at
  640 x 700; before the change it painted 1100 in a 900 column. Desktop
  library 106 with 1 ignored. Headed, the editor ends at its column's edge
  and narrows beside the outline instead of running under it (frames under
  `Code/testing/knot-editor/images/2026-09-24_3d/`). With 3d, every step 3
  done-condition holds.

  Noticed: the caret still paints while a selection shows, which browsers
  do not do, and the outline's item buttons run into its panel's right
  border, which step 4's tile replaces.
- **2026-09-24, the Genet fix after 3d.** Genet's K7 note moved the
  atomic-inline pre-pass off the viewport. Its first slice measures each
  atom's contribution with percentages treated as CSS Sizing 3 section
  5.2.1 says, then lays each percentage-sensitive atom out again against
  its real containing block (`d2342bf7742`). The second shrinks to fit the
  atoms Buckram's shrink-to-fit skips, and wraps an atom's text at the
  width it gets (`e67fdad259d`, merged at `3e8d797fda5`). Mere repinned to
  each (`2b80f599`, `b0e29383`) and Knot followed (`3d14997`, `3f3b51d`),
  the second at 384 passed with 3 ignored across the workspace. The WPT
  receipts are under `Code/testing/genet/wpt-ledger/2026-09-24_k7_*`.
- **2026-09-24, step 4a.** Outline and Preview are tiles. A reading opens
  in a stack to the right of the documents: beside the focused document's
  tile the first time, then as a tab after the last reading used. Closing
  the last reading closes its stack. Each reading tile has a header naming
  the reading and its document, with a Pin button the name gives way
  before (D2); the preview's diagnostics sit on a line of their own. A
  following tile shows the focused document, a pinned one keeps its own,
  and closing a document closes the readings pinned to it. The preview's
  id carries its tile (`knot-document-preview-{tile}`), so two pinned
  previews coexist. An outline row or a preview heading focuses its own
  document before it selects there. Preview headings lose the button
  chrome (D3). The command row's Outline and Preview buttons show and hide
  the following tile. The document tile no longer holds the outline or
  the preview, and the preview-mode class and the two visibility flags are
  gone.

  Nine tests are new. In `documents.rs`: readings share a stack right of
  the documents; a pinned reading keeps its document as the focus moves;
  closing a document closes the readings pinned to it; and the last reading
  closes its stack while the next opens another. In `workspace.rs`: a
  following reading switches with the focus while a pinned one stays; two
  pinned previews keep their own ids; a pinned outline row selects in its
  own document; a long document name stops short of the Pin button at 640
  wide; and a short reading tile fills its stack. The three preview tests
  were rewritten for the tile. Desktop library: 115 passed, 1 ignored.

  Headed, a scenario captured the outline tile, the preview tile and a
  pinned preview still naming its file after New (frames under
  `Code/testing/knot-editor/images/2026-09-24_4a/`). Reviewing the frames
  found two defects. First, a short reading's surface stopped where its
  content did: the tile content was `flex:0 0 auto`, set when there was
  only one stack. It now grows into its stack (`flex:1 0 auto`), and the
  fill test fails without that. Second, the header's separator did not
  paint. Genet drops the text of any span blockified as a flex or grid
  item, a float or an absolutely positioned box when that span starts a
  stacking context through opacity or z-index. It took the text's inline
  owner from the computed `display` rather than the box tree. On Mark's
  ruling that is fixed in Genet and repinned. Until the repin, D2's
  separator is blank. Still to come in step 4: the Folded source tile with
  D4 and the Readings tile (4b), then the Changes tile (4c).
- **2026-09-25, the Genet fix under 4a.** Genet's paint now takes a
  text's inline owner from the box tree's used display (`ad20ad17a74`).
  A blockified span that starts a stacking context therefore paints its
  text. Its test covers flex, grid, floated and absolutely positioned
  spans with opacity and with z-index. WPT reftests over six directories
  did not change, and two local control pairs went from fail to pass. The
  receipt is under
  `Code/testing/genet/wpt-ledger/2026-09-24_blockified_stacking_text/`.
  Mere repinned to it at `aeeb5bc7`, which also carries tabard's fill, and
  Knot followed, passing 393 with 3 ignored across the workspace. Headed,
  both reading headers show their separator, and a short reading fills its
  tile. The frames are under
  `Code/testing/knot-editor/images/2026-09-25_4a_repin/`. D2 and D3 are
  fixed.
- **2026-09-25, step 4b rulings.** Mark ruled three questions about the
  Folded source tile. The fold gutter comes from Cambium, as a
  line-structured read-only view on `fold_projection` with a gutter cell
  for each visible line. That is the gutter of mere's virtualized editor
  plan (P2), without virtualization. When the source changes, the tile
  re-derives its folds, like the Outline. A collapsed fold stays collapsed
  while a fold of the same kind and opening line still exists. Edit source
  focuses the document's editor and leaves the tile open.
- **2026-09-25, step 4b-1.** Readings is a tile. It runs over its own
  document, following or pinned. Each Readings tile keeps its own script
  choice by name, which is dropped when the tile closes. A row focuses its
  document before it selects there. The body's id carries its tile
  (`knot-readings-{tile}`). The panel's Hide button is gone: the tab
  closes the tile, and the command row's Readings toggle shows and hides
  the following one. While a reading tile is open, its toggle on the
  command row names the tile's region through `aria-controls`. A stale
  reading still labels itself and refuses row selection. The four readings
  receipts pass through the tile.

  Two tests are new. One checks that a pinned Readings tile runs over its
  own document and that its row focuses that document. The other checks
  that each Readings tile keeps its own script and id, and that a closed
  tile's choice goes with it. The desktop library passes 117 with 1
  ignored. Headed, the frames show a run, a row selecting in the editor,
  and the pinned tile keeping its document after New. They are under
  `Code/testing/knot-editor/images/2026-09-25_4b1/`. The provenance line
  there is a faded flex item, and it paints only since the Genet fix under
  4a.

  Open: the 3b-2 entry said scrolling inside tiles would arrive with the
  reading tiles in step 4. The window still scrolls as one page, so a long
  reading scrolls the whole window.
- **2026-09-25, step 4b-2.** Cambium gains a line view of a fold
  projection (mere `0d341dee`). `FoldProjection::lines()` splits the
  visible source into lines. A marker stays on the line it starts on, each
  line keeps its source number, and CRLF counts as one break.
  `rows(gutter)` renders one keyed row per line, with a gutter cell the
  host fills, and `FOLD_ROWS_CSS` lays the gutter beside the line. It is
  the gutter of mere's virtualized editor plan (P2), without the
  virtualization.

  Its layout test found that Genet gave an empty line no height. An inline
  formatting context measured its height from its shaped items, and a line
  holding only a newline or a `<br>` has none, so pre-wrap "\n" measured 0
  and a leading or trailing blank line counted for nothing. On Mark's
  ruling that was fixed in Genet (`5621ca05768`): line boxes that end in a
  break now count, per CSS 2.1 section 9.4.2. Its WPT receipt flipped two
  reftests from fail to pass, with no regressions and a positive control;
  it is under `Code/testing/genet/wpt-ledger/2026-09-25_empty_line_height/`.
  The same fix affects preview code blocks that start or end with a blank
  line. Mere repinned (`c3a5681e`) and Knot followed (`20fa75c`), passing
  395 with 3 ignored.
- **2026-09-25, step 4b-3.** Folded source is a tile. Each visible source
  line is a row from Cambium's line view. A fold's control sits in the
  gutter of the line it starts on: ▾ collapses and ▸ expands. Each control
  is named for assistive technology ("Collapse Section · line 6 ·
  ## Morning transect") and carries `aria-expanded` (D4).

  A collapsed fold now hides everything from its opening line's break to
  its own last break. Its marker ends the heading's row, and the text after
  the fold starts a row of its own; before, the marker began the next line
  and the following heading ran on after it. The projection uses the
  editor's face, measure and surface colours.

  As Mark ruled, the tile follows its source. After an edit, a Reload or a
  Save As, the folds are derived again. A collapsed fold stays collapsed
  while a fold of the same kind and opening line still exists, counted
  among any namesakes; the others reopen. Edit source focuses the
  document's editor and leaves the tile open. Collapsed folds belong to the
  document, as Mark also ruled: a pinned tile and a following tile on the
  same document share them, and they survive the focus moving away and
  back. The fold-mode flag, its force-close on edit and the Collapse row
  list are gone. A fold action that carries a stale snapshot still refuses
  and says so.

  Two tests are new: a gutter toggle folds its section beside its heading,
  and a pinned Folded tile folds its own document. Three were rewritten for
  the tile: the tile leaves the source alone, the tile follows an edit and
  keeps its folds, and an outline selection leaves the tile open. The
  follow-an-edit test fails with the identity remap disabled. In
  `document_folding.rs`, the conceal test now expects the new marker
  position, and a new test carries collapsed folds across an edit by
  identity. The desktop library passes 120 with 1 ignored.

  Headed, the frames show the gutter beside the headings, a collapsed
  section ending its heading's row, and Collapse all folding the document
  to its title row. Blank lines keep their height there only since the
  Genet fix. The frames are under
  `Code/testing/knot-editor/images/2026-09-25_4b3/`. With 4b, D4 is fixed.
  Step 4 still has the Changes tile (4c), and the open question about
  scrolling inside tiles.

## Related material

- [Design pass](2026-09-23_knot_design_pass.md) and its frames
- [Application workspace plan](2026-09-05_knot_application_workspace_plan.md)
- `mere/crates/cambium/cambium/src/{workspace,frisket,tabs,graph_canvas,overlay_surface,detail_panel}.rs`
- `mere/crates/cambium/workbench/lib.rs`
- `mere/crates/cambium/cambium-genet-winit-host/examples/smoke.rs` (the lane's reference wiring)
- `mere/crates/cambium/cambium-rootstock/src/capture.rs` (`read_frame`)
- `mere/crates/canvas/cartography/src/{strategy,request,adapters/mod}.rs`
- `mere/crates/graph/graph-kernel/src/graph/apply.rs` (`add_node`, `assert_relation`)
- `genet/components/taproot/{lib,scenario}.rs`
- `crates/knot-readings/src/links.rs`
