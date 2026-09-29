# Knot workspace slice 1: frame, tiles, status, sites and graph

**Date:** 2026-09-23
**Status:** In progress. Steps 7d and 7e are implemented; the two-site headed
scenario is ready for its native frame receipt. The step 7c scenario-scroll
follow-up and the Taproot/Mesquite runner unification are recorded below.
Assessment and initial rulings were made 2026-09-23.
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
With those blocks gone, the frame fills the window below the command row and
each tile scrolls on its own; Mark moved this here from step 4 on
2026-09-25. *Done when* a test enumerates every authority fact the old blocks
showed and finds each in a chip or popover; a refusal raises its chip and
posts its sentence; tabs carry dirty and refused marks; the retention tests
pass through the popover; and a tile taller than its stack scrolls inside it
while the tiles beside it stay put.

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
- **2026-09-25, step 4c.** Changes is a tile. It holds the disk comparison,
  which used to sit under the editor, and, when the window has a catalog,
  the saved-revision review, which used to sit above the frame. Both act on
  the tile's own document, following or pinned. A tile with nothing
  compared offers Compare with disk. Compare on the command row compares
  the focused document and brings forward a Changes tile showing it,
  opening one if none does, so its result stays in view as before. Show
  changes toggles the following tile, like the other readings. The
  comparison panel is now a column, so its lines no longer run together,
  and its items keep their size inside its scrolling box.

  The comparison tests pass unchanged through the tile. The saved-revision
  tests and the resident retention test open the tile first. Three tests
  are new: Compare shows its result in a Changes tile and never in the
  document's own tile; a pinned Changes tile compares its own document; and
  a pinned Preview's heading selects in its own document. The desktop
  library passes 123 with 1 ignored. Headed, the frames show a comparison
  in the tile and the tile after Hide comparison. They are under
  `Code/testing/knot-editor/images/2026-09-25_4c/`.

  The headed check found two engine gaps. Both are worked around in Knot's
  CSS and noted here, not fixed:
  - Genet places a tall inline-block about half a leading too high, so it
    overlaps the line above. A 40px atom in 20px lines lands 2px up; a
    button aligned on its own text overlaps more.
  - In a column that overflows its max-height, a flex item shrinks below its
    content height, where `min-height:auto` should stop it.

  Step 4's done-conditions hold:
  - following and pinned tiles, and two pinned previews with their own ids
    (4a);
  - rows and headings focusing their own document's source range: outline
    rows, Readings rows and preview headings;
  - stale readings labelling themselves: the Readings tile, the comparison
    and the review;
  - the comparison and saved-revision tests passing in the Changes tile.

  D2, D3 and D4 are fixed. Scrolling inside tiles moved to step 5 on Mark's
  ruling of 2026-09-25, since the frame can fill the window once the blocks
  above it are gone.
- **2026-09-25, step 5a.** The host keeps a moved caret in view vertically
  (mere `6a206411`). Before, it followed a caret only across a single-line
  field, so typing at the end of a long document left the caret below the
  window. Now it scrolls one plane just far enough to show the caret's
  line: the field itself when it scrolls, else the nearest scrolling
  ancestor with room, else the window. Three host tests cover a field's
  pane, the window, and a caret that has not moved, which leaves the
  reader's scroll alone. Knot repinned (`78d014c`) and passed 401 with 3
  ignored.
- **2026-09-25, step 5b.** The frame fills the window, and each tile
  scrolls on its own. The workspace is now the window's height: the
  command row, the frame, and a status bar docked along the bottom. The
  frame takes the rest of the height, at least 240px. As Mark ruled ("grow,
  tile scrolls"), the editor grows with its text and its tile scrolls. The
  window no longer scrolls as one page.

  The message line is now the status bar's message, from Cambium's
  `status_bar`, with role status and a polite live region. It has no chips
  yet; 5c adds them. A long message is now cut to one line with an
  ellipsis, where it used to wrap. The live region still reads it whole.

  The scrollers nested inside tiles are gone. The preview, the Readings
  body and the outline no longer scroll on their own, and the comparison
  and review panels no longer cap their height, so a tile has one
  scrolling plane.

  Tile scrolling found two gaps in Cambium, both fixed there:
  - The harness's `click_on` clicked an element's full centre even where a
    pane clipped it. It now scrolls the element into view and clicks the
    part that shows (mere `e8f83f89`).
  - A scroll request moved one plane, so a control in a nested scroller
    that ran below the window could not be reached: the saved-revision
    review's Refresh button, under 4c's capped panel. On Mark's ruling, the
    nearest plane moves by the request's alignment. Each plane outside it,
    out to the window, then moves only as far as the element needs (mere
    `9a2e4eeb`).

  The two Genet gaps that 4c worked around are fixed in Genet. As Mark
  ruled, both are folded into this round:
  - A padded control keeps its content minimum in a flex column
    (`b666512513c`).
  - Line boxes are built per CSS 2.1 section 10.8 (`6afb472a0c6`).

  A button still aligns on its bottom edge until Genet's slice B. On
  `6afb472a0c6` that opened a 4px gap under a popover's trigger, so on
  Mark's ruling Cambium's popover anchor is now a flex container (mere
  `6562b7c2`). Mere repinned to the new Genet (`83289655`), and Knot
  followed to mere `9a2e4eeb` (`4b67c97`). That mere also brought the
  reservoir's first-party door, whose `serve_app_broker` now takes
  `AppRouteGrants`, so the resident route test wraps its grants.

  The new Genet puts parsed pages without a doctype in quirks mode. Knot
  parses no HTML, so it is not affected: a probe of the live document
  reported no-quirks, and in the same run a parsed page without a doctype
  reported quirks.

  The `flex-shrink:0` rule on the comparison's and review's items is gone,
  as Mark ruled. With the panels uncapped, nothing presses on those
  columns, and Genet now gives a padded control its content minimum
  anyway. A probe put the rule back inline on all 19 items in the two
  panels. None moved, and none overlapped the one above. An injected
  height did move the first item, so the probe could see a change. The
  columns stay.

  Tests:
  - The Micron in-page tests measure the preview tile's scroll and check
    that the window stays put.
  - The preferences receipts read the status bar's message.
  - One test is new: a tile taller than its stack scrolls inside it while
    the tile beside it and the window stay put. It fails on 4c's layout,
    where the window scrolls instead.

  The desktop library passes 124 with 1 ignored, and
  `cargo test --locked` passes 402 with 3 ignored. Headed, the
  frames show the frame over the status bar, a Preview tile beside the
  document, and a comparison in the Changes tile. They are under
  `Code/testing/knot-editor/images/2026-09-25_5b/`.

  Of step 5's done-conditions, one now holds: a tile taller than its stack
  scrolls inside it while the tiles beside it stay put. The chips and
  popovers, the refusal chip, the tab marks and the facts enumeration
  remain for 5c and 5d.

  Open: caret follow still moves one plane. On 2026-09-25 Mark ruled that
  scroll requests and caret follow will share one walk outward, measuring
  a container by its scrollport. It lands in mere after 5b and reaches
  Knot at its next repin. Knot does not need it for 5b: its window no
  longer scrolls, so a tile is the only plane.

  Update: the shared walk landed in mere `2ea1a6e7`, and Genet's slice B
  (atom baselines, `18e41e44c36`) in mere `e3d7061d`. Knot repinned to
  both (`0c98775`). Its headed frames matched 5b's, with nothing moved.
- **2026-09-25, step 5c.** The status bar carries the focused document's
  chips: format, save state and posture. Each opens its facts in a
  popover, a Cambium detail panel; the save chip's popover also has a Save
  button.

  The desktop's document surface no longer draws its own status row.
  knot-document gains `KnotDocumentStatus::HostOwned`, an addition, so
  Turnstone's embedded surface keeps its row. knot-document also exports
  its format, posture, save-outcome and refusal labels, and the chips use
  the same words.

  The chips:
  - Format: the format's name, "Djot".
  - Save: "Saved", "Unsaved changes", "Not saved" (a scratch document with
    no file), "Save refused" or "Save failed".
  - Posture: "File target", "Scratch" or "Read-only".

  The chips are quiet while inert. A refusal raises the save chip to
  refused, and a failed save raises it to warning.

  A refusal now posts a sentence that names the refused action, such as
  "Save refused: this document is read-only." or "Reload refused: a new
  document has no file yet. Use Save As." Before, any refusal but a change
  on disk posted Debug text such as "Action refused: ReadOnly". The message
  line stays quiet; the chip carries the weight.

  A refused document's tab carries the attention mark, which wins over the
  unsaved mark. The mark clears when knot-document clears the refusal: on
  a successful save, or on an allowed edit.

  Tests:
  - A facts test takes seven states and finds everything the old row
    showed in a chip or its popover: the source, the format, clean or
    dirty, the posture, and the last save with its refusal or failure.
    With the Source row dropped, it fails.
  - A refused save, where the file changed on disk, raises its chip,
    posts its sentence, marks its tab and shows the refusal in the
    popover.
  - knot-document checks that the embedded row stays the default.
  - One comparison test clicked the surface's own Save button. It now
    uses the command row's.

  The desktop library passes 127 with 1 ignored, and `cargo test --locked`
  passes 406 with 3 ignored. Headed, the frames show the chips, the save
  and format popovers, and a new document's refused save: the raised chip,
  the sentence and the tab's attention mark. They are under
  `Code/testing/knot-editor/images/2026-09-25_5c/`.

  Noted, not changed:
  - With a popover open, a click on another chip first only closes the
    open one, through the popover's click-outside layer, so switching takes
    two clicks.
  - The popover's refusal row keeps knot-document's wording ("scratch
    document has no file target"), while the message line says "a new
    document has no file yet".

  Update, 2026-09-26, on Mark's ruling: the wording now has one source.
  knot-document's refusal labels are plain words: "a new document has no
  file yet", "this document is read-only", "the file changed on disk". The
  desktop's message line is built from them, so the popover and the
  message line say the same thing. Turnstone's embedded row takes the new
  words at its next repin.

  Update, 2026-09-26: switching chips takes one click. On Mark's ruling,
  Cambium raises the status bar's chips above an open popover's
  click-outside layer (mere `365807a4`). Knot repinned to mere `149b8053`,
  which also carries Genet `1b62fd0b218`: a text field whose text is its
  children keeps its label as its accessible name. A desktop test switches
  chips in one click and closes the popover with a click in the document.

  Two more of step 5's done-conditions now hold: a refusal raises its chip
  and posts its sentence, and tabs carry dirty and refused marks. 5d
  remains: the catalog and retention chips that retire the blocks, the
  facts test over them, the retention tests through the popover, and the
  serving chip.
- **2026-09-26, step 5d.** The catalog and retention blocks are gone from
  above the frame. Their chips sit in the status bar, and their popovers
  hold what the blocks held, so the document now starts right under the
  command row.
  - **Catalog chip**, when a catalog is configured: "Catalogued", "Not
    catalogued" (an unsaved document), or "Catalog error" at warning
    weight. Its popover shows the document ID, or why there is none with
    Retry catalog.
  - **Retention chip**, where the block showed: "Not retained",
    "Retaining…", "Retained", or "Retention failed" at warning weight. Its
    popover holds:
    - the destinations;
    - the chosen destination's persona, stable ID, space, writer and
      encryption;
    - Retain reviewed revision, when it is available;
    - the busy, error and receipt lines.
  - **Serving chip**, when the open site is serving or the focused
    document is one of its pages. "Serving" shows the address and
    publication with Stop serving. "Not serving" says why, with Publish
    locally.

  Tests:
  - A facts test finds every line the retention and catalog blocks showed
    in their popovers. With the receipt line reworded, it fails.
  - A pure test checks the three chips' labels and severities.
  - A serving test publishes and stops through the chip's popover.
  - The retention and catalog tests open their chip first, through a
    helper that clicks the chip and checks it opened. So the tests that
    look for a missing Retain button do it with the popover open.
  - The resident retention test drives the whole flow through the popover:
    choosing a destination, retaining, being refused after revocation, and
    retaining again after a regrant.
  - One catalog test now closes the popover with Escape before clicking
    New, because a click outside a popover only dismisses it.

  The desktop library passes 131 with 1 ignored, and `cargo test --locked`
  passes 410 with 3 ignored. Headed, the frames show the frame under the
  command row, the catalog popover with the document ID, and the retention
  popover's empty state. They are under
  `Code/testing/knot-editor/images/2026-09-26_5d/`.

  Step 5's done-conditions all hold:
  - a test enumerates every authority fact the old blocks showed and finds
    each in a chip or popover (5c for the surface's row, 5d for the
    catalog and retention blocks);
  - a refusal raises its chip and posts its sentence;
  - tabs carry dirty and refused marks;
  - the retention tests pass through the popover;
  - a tile taller than its stack scrolls inside it while the tiles beside
    it stay put (5b).

  Noted:
  - The Site panel stays above the frame until the site tile (step 7).
  - knot-editor's `two_place_members_edit_one_held_document_through_projection`
    failed in two of three full runs under load and passed every time
    alone. It waits on a projected session over local endpoints, a timing
    flake outside the desktop.
- **2026-09-26, step 6.** The Navigator is a tile. The command row's
  Navigator toggle opens it in a new stack left of the documents and closes
  it. On Mark's ruling it opens at a quarter of the width; the divider
  moves it.

  It lists the catalog's records by path, each labelled Available or
  Unavailable. Listing reads the catalog and each file's metadata, never a
  document's bytes.
  - An available record opens its document, or activates the tab already
    showing it.
  - An unavailable record stays in the history, dimmed and labelled, and
    does nothing. Rebinding it is a later, explicit action, never inferred
    from a name.
  - Without a catalog, the Navigator says so and how to launch with one.

  The catalog's root is canonical, so the "Opened …" message now shows the
  path without Windows' verbatim prefix.

  Tests:
  - The Navigator opens once, left of the documents, at a quarter share.
    Without the share, the test fails.
  - The history test labels an available record and a deleted one, and
    checks that no file's bytes changed and nothing recreated the deleted
    one.
  - Selecting a record opens it, and selecting it again activates it
    without a second tab. The "Opened" message has no verbatim prefix;
    without the fix, that check fails.
  - A launch without a catalog says so.

  The desktop library passes 135 with 1 ignored, and `cargo test --locked`
  passes 414 with 3 ignored. Headed, the frames show the Navigator with one
  record available and one unavailable, and the available one opened
  beside a scratch document. They are under
  `Code/testing/knot-editor/images/2026-09-26_step6/`.

  Step 6's done-conditions hold:
  - available and unavailable catalog history is labelled truthfully,
    without changing source bytes;
  - selecting an available record opens or activates its document;
  - a launch without a catalog says so.

  Noted:
  - In a flex row aligned on baselines, Genet takes a button item's
    baseline from near its bottom, so "Available" sits about 8px below the
    button's text. This has been reported to the Genet roadmap.
  - A long message squeezes the status bar's chips and wraps their labels.
    On Mark's ruling, Cambium gets the fix after step 6.

  Update, 2026-09-26: the status chips keep their size beside a long
  message. On Mark's ruling the fix is Cambium's (mere `87b81fb8`): the
  chip row no longer shrinks and a chip's label stays on one line, so the
  message is what gets cut. A Cambium host test lays the chips out beside a
  short message and a long one, and fails on the old sheet. Knot repinned to
  mere `37b0bced`, which also brings the reservoir's one-tree phases 1 and
  2 (Cambium's file chooser among them, which Knot does not use); Genet
  stays `1b62fd0b218`. Headed, step 6's Navigator run shows the five chips
  whole beside the cut "Opened …" message. The frame is under
  `Code/testing/knot-editor/images/2026-09-26_chip_squeeze/`.

  Genet does not paint `text-overflow: ellipsis`, so the message is clipped
  mid-glyph, without the ellipsis the sheet asks for. On Mark's ruling this
  has been reported to the Genet roadmap. Cambium's doc now says the
  ellipsis waits on Genet (mere `518ca1dc`), and a note on `87b81fb8`
  corrects that commit's message.
- **2026-09-26, step 7 plan.** Assessed at knot-editor `e812260`. The
  desktop holds one site for the whole window. `DesktopState.scroll`
  carries:
  - the `Site`, its page, server and port;
  - the metadata fields;
  - the Titan and Spartan composer, its review and its worker;
  - the Micron form and its request.

  Only the Micron preview's folds moved into `DocumentEntry` in step 3.
  - The site panel sits above the frame.
  - A native page's preview is not the Preview tile. It is an aside inside
    the document tile, and it picks the Spartan or the Gemtext engine by the
    window's one site.
  - The site commands check only the focused document's dirty state, and
    unsaved metadata holds the focus.
  - The launcher refuses a second site folder.
  - `Site` has no name, so a site is labelled by its folder.
  - `display_path` strips `\\?\C:` but not `\\?\UNC\`.

  Mark's rulings:
  - **Metadata is per page.** "Metadata · ‹page›" opens from the page's
    row in the site tile, as a tile in the document stack.
    - Its draft lives with the page, and its tab carries the dirty mark.
    - Closing it over unsaved edits asks Save, Discard or Cancel.
    - The rule that unsaved metadata holds the focus retires.

    This replaces the metadata fields the findings table gave the site
    entry.
  - **Closing a site tile closes its pages.** Each dirty page asks Save,
    Discard or Cancel, and Cancel keeps everything. Then the site's server
    stops and its submission state goes.
  - **Upload and submit live with the document they act on**, in a Submit
    tile. Mark first ruled them into the site tile, then revised that the
    same day on a finding. Titan upload sends the focused document's saved
    bytes to a typed target and never needs a site, so today any saved
    document can be uploaded.
    - The Submit tile sits in the reading stack and follows the focused
      document, as Changes does.
    - It opens from a site tile's Upload / submit, from the command row,
      or when a Spartan prompt link is picked.
    - Each document keeps its own composer, review and reply. The Micron
      form and its reply stay with their page too.

  Sub-steps, each ending green. On Mark's ruling the two tiles come before
  the site tile, whose buttons open them:
  - **7a.** Per-site and per-document state. A `SiteKey` to `SiteEntry` map
    holds the site, server, port and publication count. These move into
    the document's `DocumentEntry`:
    - the page and its metadata draft;
    - the Micron form and request;
    - the Titan and Spartan composer, review and reply.

    Nothing else changes visibly: each document now simply keeps its own
    composer and Micron form. Existing tests are rewritten against the new
    state, not deleted.
  - **7b.** The Metadata tile (per page) and the Submit tile (for any
    document). They take the metadata form and the composer out of the
    panel.
  - **7c.** The site tile, in the left stack beside the Navigator, shows:
    - the name and format;
    - the pages, which open or activate as tabs;
    - Metadata, Upload / submit, Publish locally, Stop serving and the
      serving state;
    - Close.

    The command row's Site button becomes a popover with the folder, the
    format, Create and Open. The panel above the frame retires.
  - **7d.** The Preview tile presents a native page in its format's
    presentation, with follow and pin as for Djot, and the Spartan choice
    read from the page's own site. The aside retires. The Micron form moves
    with the preview, and its state stays with its page.
  - **7e.** Two sites at once:
    - The launcher takes several site folders.
    - The serving chip reads "‹folder› · serving :port" for the focused
      page's site.
    - A site whose format's default port another open site already holds
      starts at port 0, a free port chosen when it publishes.
    - A test finds no verbatim prefix in any site text, with
      `display_path` handling the UNC form.
    - Headed frames show two sites serving.
- **2026-09-26, step 7a.** Site state is split by owner.
  - The window's `ScrollWorkspace` now holds:
    - a `SiteKey` to `SiteEntry` map (each site with its port, server and
      publication count);
    - the panel's one current site;
    - the fields for opening or creating the next site;
    - the panel's view flags.
  - Each `DocumentEntry` holds a `DocumentSite`:
    - the site page it is, with its metadata draft;
    - its Micron form, request and reply;
    - its Titan or Spartan composer, review and reply.

  A document binds to its page when it opens, after Save As, and when a
  site opens or closes. It relies on knot-document opening and saving
  through the canonical path, as `Site::page_path` does.

  What changes visibly:
  - Each document keeps its own composer and Micron form. Their replies no
    longer share one line, so a Titan reply no longer shows in a Micron
    form.
  - A Micron reply lands on the page it was sent from, even after the focus
    moves. Before, a focus change made it look stale and it was dropped.
  - A Gemtext file outside a Spartan site keeps Gemtext's presentation
    while that site is open. Before, the window's one site decided it.
  - Sends are one at a time per document, not per window.
  - Entering or closing a site no longer closes a Micron form, which
    belongs to its document.

  The panel still shows one site at a time, and the launcher still refuses
  a second site folder. Both limits lift in 7c and 7e.

  Tests: the existing site and Micron tests are rewritten against the new
  state; none was deleted. Four are new:
  - a composer stays with its document across tab switches;
  - a Micron reply lands on its page after the focus moves;
  - a Gemtext file outside a Spartan site keeps its presentation, while the
    site's own page presents the prompt;
  - a page opened through another spelling of its path is its site's page.

  Controls:
  - On the pre-7a sources, the first and third fail.
  - The second fails when the drain judges replies against the focused
    document, as before.
  - The fourth passes on the old sources too. The session already opened
    the canonical path, so this test pins that behaviour rather than
    proving a change.

  The desktop library passes 139 with 1 ignored, and `cargo test --locked`
  passes 418 with 3 ignored. In the first full run, knot-editor's embedding
  digest test stopped using CPU for ten minutes while peer builds loaded
  the machine. Once stopped and rerun alone, knot-editor's library passed
  147 in 13 seconds.

  Headed, launched on a copy of the Tidewatch fixture:
  - The panel is as before.
  - Its metadata shows `index.scroll`'s values.
  - Opening `about.scroll` from the page list adds its tab and switches the
    metadata to its own.

  The frames are under `Code/testing/knot-editor/images/2026-09-26_7a/`.
  They also show the site folder field painting a long path past its box,
  behind the format buttons. That is D6's family, in the panel that
  retires in 7c.
- **2026-09-26, repin.** Knot moved to mere `0418391f` and genet `0cf4f30`
  together (`d265f03`).
  - Genet's render boot now forwards netrender's optional features.
  - Insigne phase B replaces `verify()` with `check()`. Five knot-editor
    call sites in tests and the example adopted it; the library is
    unchanged.

  The Cleromancy session had relayed an ask for Knot's genet rows to move
  first. Tried alone, that broke knot-desktop's build (22 lib errors, 505 in
  tests) with two genet copies in the graph, so on Mark's ruling mere went
  first. Mere's djinn then repinned to `d265f03` (mere `a464dc2a`), which
  dropped its two old-genet copies. The headed frame is under
  `Code/testing/knot-editor/images/2026-09-26_repin_0418/`.
- **2026-09-26, step 7b.** The Metadata and Submit tiles.
  - **The Metadata tile.** On Mark's ruling, a page's metadata draft lives
    with its site, keyed by page, not with the page's document. `SiteKey`
    moved beside `DocKey`. `TileRole::Metadata { site, page }` opens in the
    document stack as "Metadata · ‹page›", from the panel's Metadata button
    for the focused page. Its draft starts from the manifest's saved values
    and goes when the tile closes.
    - Its tab carries the unsaved mark while its draft differs from the
      manifest.
    - Closing it over unsaved edits asks Save, Discard or Cancel.
    - The quit prompt lists it beside dirty documents, and Save all writes
      it. A list holding both kinds says "N tabs have unsaved changes".
    - It outlives its page's source tab. Entering or closing a site closes
      that site's metadata tiles.
    - Publishing refuses while any draft of the site is dirty, and Titan
      preparation refuses while its page's draft is.
    - The rule that unsaved metadata holds the focus retired, with its
      eight guards.
  - **The Submit tile.** `ReadingKind::Submit`, titled "Upload / submit",
    follows the focused document or pins to one. It opens from the command
    row's toggle, or when a Spartan prompt link is picked. Its actions and
    fields act on the tile's own document. The fields carry classes, and the
    tile carries its document's key, so both text routes and lenses reach
    that document whichever one has the focus.
  - **The panel** keeps create, open and close, the page list, Metadata,
    the preview toggle and publishing. Its metadata form, composer and
    Upload / submit button retired.

  Tests: the metadata and composer tests are rewritten against the tiles.
  The focus test now checks three things: the metadata tab's mark, that the
  focus moves on with the draft kept, and that closing asks, with Discard
  leaving the manifest untouched. Three tests are new:
  - the quit prompt names unsaved metadata, and Save all writes it;
  - a pinned Submit tile takes typing, and a click's caret, for its own
    document while another has the focus;
  - a metadata tile outlives its page's source tab.

  Controls: each test fails with its behaviour broken:
  - the quit prompt ignoring metadata;
  - closing a document closing the metadata tiles;
  - a dirty tile closing without asking;
  - publish ignoring drafts;
  - for the pinned tile, both the text route and the field's lens sent to
    the focused document.

  Typing reaches a field through its lens. The text route places a click's
  caret and gates IME, which is why the pinned test needs both controls.

  Found on the way: the composer tests' harness lacked Cambium's frame
  sheet. Without it, a split's second stack collapsed to its tab bar and
  clipped the tile out of hit-testing. The harness now uses the desktop
  sheet.

  The desktop library passes 142 with 1 ignored, and `cargo test --locked`
  passes 421 with 3 ignored. Headed on a copy of the Tidewatch fixture:
  - the panel's Metadata button opens "Metadata · index.scroll" beside its
    page's tab, filled from the manifest;
  - the command row's Upload / submit opens the Submit tile in a stack to
    the right, following index.scroll.

  The frames are under `Code/testing/knot-editor/images/2026-09-26_7b/`.
  The panel still costs the frame its height until 7c retires it.
- **2026-09-27, step 7c.** The site tile and the Site popover. The panel
  above the frame is gone.
  - **The Site popover.** The command row's Site button opens a popover
    with the folder, the format, Create and Open. One site is open at a
    time until 7e. Opening another closes the open one first, and is
    refused while it holds unsaved pages or metadata.
  - **The site tile** (`TileRole::Site`) opens in the left stack, titled
    by its folder's name. It joins the Navigator's stack, or takes a new
    stack left of the documents at the Navigator's share; the Navigator
    now joins a site tile's stack the same way. The tile shows:
    - the format and the folder, without the verbatim prefix;
    - a row per page, whose button opens or activates the page's tab and
      whose Metadata opens its metadata tile;
    - Upload / submit and Toggle preview, which stays until 7d retires the
      aside;
    - the port field, Publish locally, Stop serving and the serving line;
    - Close site.

    The tile carries its site's key, so its port field takes text for that
    site. A launch on a site folder opens its tile.
  - **Closing a site** by its ×, or by Close site, asks once, on Mark's
    ruling. One prompt lists the site's unsaved pages and metadata, with
    Save all, Discard all, Review and Cancel; Cancel keeps everything. A
    site with nothing unsaved closes at once. Closing closes its page tabs,
    its metadata tiles and its tile, and stops its server. Other documents
    stay.

  Tests: five are new. Each fails with its behaviour broken:
  - the popover creates a site whose tile opens left of the documents, and
    whose rows open a page and its metadata;
  - closing a site asks once for its unsaved page and draft, Cancel keeps
    all, and Save all writes both and closes it;
  - a clean site closes its pages and stops its server without asking;
  - another site replaces the open one only once it is saved;
  - the tile's port field takes typing for its site.

  Rewritten against the retired panel:
  - The serving-chip test presses the popover's own buttons, since the
    tile offers the same labels.
  - The focus-rule test Tabs to the Site popover's folder field.
  - An outline test reaches that field through the popover.
  - The Micron in-page-link harness now uses the desktop sheet, since a
    site tile splits its frame.

  The desktop library passes 147 with 1 ignored, and `cargo test --locked`
  passes 426 with 3 ignored. Headed, on a copy of the Tidewatch fixture,
  the launch shows the site tile beside `index.scroll` and a row opens its
  metadata. The frames are under
  `Code/testing/knot-editor/images/2026-09-27_7c/`.

  Found: the tile is taller than the frame at 1100 × 700, since its folder
  path wraps in a quarter-width column, so Close site sits below the fold.
  A person can scroll the tile to it; the harness shows a wheel bringing it
  into view. The scenario lane cannot, though. It clicks a control at its
  painted place without scrolling it into view, unlike `Harness::click_on`,
  so the receipt's Close site click reached nothing and its third frame
  repeats the second.

- **2026-09-27, step 7c scenario-scroll follow-up.** Knot pins Mere
  `ba44756c` and Genet `34626a6c`. Every manifest naming the prior revisions
  was swept, including the desktop's direct dependency and the standalone
  `knot-document` workspace.

  Taproot supplies an optional mutable click-delivery hook. Mesquite owns one
  pending-click implementation used by both its lane and the older Cambium
  winit scenario lane. Rootstock supplies the visible rectangle and scroll
  request. A clipped target scrolls first; the next frame clicks the visible
  portion and holds scenario execution until pointer dispatch. A target that
  remains invisible fails the receipt. Mesquite's product coordinate mapping
  still applies.

  The Far-button regression failed before the fix (`count` stayed `0`) and
  passes afterward without a settle before its assertion. Shared checks pass:
  Taproot 21; scenarios 13; harness click-scroll 3; scroll requests 10;
  Mesquite units 17. Knot's `cargo test --offline --workspace` passes 377 with
  2 ignored, including the desktop library's 147 with 1 ignored.
  The standalone `knot-document` all-features suite passes 48 with 1 ignored.
  The `resident-retention-tests` feature's integration test also passes, giving
  the same aggregate **426 passed, 3 ignored** as the handoff. The aggregate
  includes these three commands; a default workspace run alone is not 426.

  Native acceptance reran `images/2026-09-27_7c/site.scn` on Tidewatch at
  1100 × 700 logical pixels. The scenario now asserts `message ~ Closed site`
  immediately after its Close site click. The receipt is `RESULT ok`, with
  three nonblank 2200 × 1400 captures. The third frame was inspected: the site,
  page and metadata tiles are closed, and the status says "Closed site
  tidewatch." The original images and receipt remain in `before-scroll-fix/`;
  the new receipt is `scroll-fix-scenario.done`, beside `scroll-fix.md` and
  the build/test logs. The Genet hook and the Mere/Knot follow-up commits are now published.
  The original approval gate is resolved.

- **2026-09-27, Taproot/Mesquite unification.** Knot now implements
  `mesquite::Product` and constructs `mesquite::Lane::from_config` directly.
  Mere `8fce5365` removes the duplicate runner from the native host and moves
  its configuration, text receipt, scripted file chooser and capture hooks
  into Mesquite's existing lifecycle. Taproot remains pinned at Genet
  `34626a6c`. All Mere manifest revisions were swept, including standalone
  `knot-document`. The host remains responsible for native file reading and
  input/capture delivery.

  `KNOT_SCENARIO`, `KNOT_CAPTURE_DIR`, `KNOT_RECEIPT`, scenario commands and
  named PNG/text receipt conventions remain supported. The native smoke
  example and Mere View harness use the same Product trait and lane. Shared
  validation passes 15 scenario tests, 17 Mesquite unit tests and 16 host
  unit tests. Native smoke and Mere View theme/resize runs each pass with
  three distinct nonblank frames; smoke also passes without saving PNGs.
  Knot's workspace passes 377 tests with 2 ignored; the standalone document
  suite passes 48 with 1 ignored; retention passes 1, preserving the aggregate
  **426 passed, 3 ignored**. Commands and full logs use the same stable
  `C:\t\cargo-targets\knot-editor` as the prior receipt.
  The unchanged site scenario also passes through Mesquite: three nonblank
  2200 x 1400 frames, with the third visually inspected to confirm the site,
  page and metadata tiles are closed and the status reads "Closed site
  tidewatch." Receipt: `images/2026-09-27_7c/unification/knot/scenario.done`;
  shared/native evidence and logs are alongside it and in the parent directory.
  The broader products' custom runners remain outside this bounded migration.
- **2026-09-29, step 7d.** Native pages now use the standard Preview reading
  tile. Scroll, Gemtext and Micron presentations follow the focused document
  and can be pinned exactly like Djot; a Gemtext page chooses Spartan only from
  the site that owns that page. The editor-side preview aside and the site
  tile's provisional Toggle preview command are gone.

  Micron forms render inside the Preview tile and continue to live on their
  page's `DocumentEntry`. Every form, fold, link and submission action is keyed
  to the document the tile presents, so a pinned preview remains attached to
  its page after focus moves. In-page jump scrolling is likewise scoped to the
  keyed preview instead of whichever document happens to have focus. A
  collapsible heading with an interactive link exposes the fold marker as its
  own button, avoiding nested buttons.

  Existing native-preview and Micron tests were moved to opening the Preview
  reading rather than relying on the retired aside. New receipts pin a Spartan
  page while focus moves to a loose Gemtext file, and pin a Micron page while
  its form stays with that page. The desktop library passes 148 tests with 1
  ignored. Step 7e remains the next site sub-step.

- **2026-09-29, step 7e.** Several sites now coexist in one window. The
  `SiteKey` map is the authority; opening another site no longer closes or
  preflights its siblings, and reopening the same canonical folder activates
  its existing entry. Each site keeps its own tile, metadata drafts, local
  server and publication count. Page-owned Submit and Micron state remains on
  its `DocumentEntry`, preserving step 7d's keyed Preview behavior when a
  preview is pinned while another site's page has focus.

  The launcher accepts several site folders and attaches all of them before
  opening their index sessions behind the first named target. When a site's
  format default is already claimed by another open site, the later site's
  port field starts at `0`; publication then uses the actual free loopback
  port reported by `LocalServer`. The serving chip is scoped through the
  focused page's `SiteKey`, reads “‹folder› · serving :‹port›” (or names that
  folder as not serving), and its Publish and Stop actions affect only that
  site. Drive and UNC verbatim path prefixes are removed before paths reach
  launcher failures, fields, site tiles or site-name fallbacks.

  Tests cover an unsaved first site surviving a second open; colliding
  defaults; two simultaneously serving sites on distinct free ports; separate
  composers and publication counts; focus switching the qualified chip; its
  Stop action leaving the other server alive; multiple launch folders; and
  drive plus UNC display paths. `site_count` and `serving_count` are scenario
  snapshot fields. `scenarios/two_sites.scn` publishes both launch fixtures,
  asserts both counts and captures `two-sites-serving` for the native receipt.

  Validation on macOS: knot-desktop passed 166 tests with 1 ignored across its
  unit, binary and integration targets; the full workspace passed; standalone
  knot-document all-features passed 46 with 1 ignored; and the resident
  retention feature receipt passed. `cargo clippy -p knot-desktop --lib --
  -D warnings` is clean. The all-target clippy run still reports pre-existing
  test-only lints outside this step. The headed two-site scenario is authored
  but its native PNG has not yet been captured or reviewed.

- **2026-09-29, step 8.** Knot now embeds Mere's V2b `mere-view` component at
  the existing pinned Mere revision `8fce5365`; it has no app-local graph
  drawing and neither Mere nor Genet moved. The singleton Graph tile opens
  from the command row, and the same shared component fills the centre when
  the last document closes. Native labelled node targets carry stable catalog
  or open-document keys back to Knot for activation, while New and Open remain
  host actions.

  A bounded background catalog reading captures saved bytes without moving the
  catalog owner, parses links through each document format's normal reading
  projection and hands V2b available, unavailable, open and dirty nodes plus
  extracted-provenance edges. Open documents merge with their catalog nodes.
  Building, empty and unavailable states are explicit, and the ready notice
  names a deterministic 12-character catalog-reading digest plus any read
  failures. An absent catalog still shows open documents and says what is
  absent. Spectral, Grid and Spiral requests route through the host; the chosen
  cartography strategy persists with the desktop preferences.

  Knot registers the shared graph paint leaf from the measured tile geometry,
  removes it when the graph is not present, supplies light/dark Tinct-derived
  node colors and includes Cambium's graph swatches plus `mere-view` CSS. The
  existing Mesquite scenario `after_frame` hook remains composed beside the
  graph frame hook. Focused coverage exercises snapshot capture, the bounded
  worker, unavailable history, open/catalog merging, last-document and
  singleton behavior, host-owned actions, activation and layout persistence.
  `knot-file-catalog` passes 10 tests; `knot-desktop --lib` passes 158 with 1
  ignored, and the default full workspace passes. Strict clippy is clean for
  the desktop library, file catalog and standalone engine-enabled
  `knot-document`. Step 9's headed graph receipt and whole-frame comparison
  remain outstanding.

  Review corrections keep catalog authority live at activation: Knot looks up
  the current record and requires current `Available` status, so a removed
  file or a binding replaced by a symlink cannot be opened from a stale graph.
  Percent-encoded local link paths are decoded before containment resolution.
  The last document is now replaced by the singleton Graph tab inside its
  existing workspace stack rather than bypassing the workspace renderer;
  Navigator, site and reading branches survive both the close and a later New.
  Every launch document is catalog-bound before the graph merges open state.

  All successful document-write paths share one graph invalidation seam,
  including Save As, dirty-close, Save all and site-close saves. The graph
  worker is single-flight: while one bounded read runs, refreshes replace one
  coalesced queued generation rather than spawning overlapping catalog scans,
  and a superseded completion is never installed. Regression coverage includes
  replaced-symlink activation, surviving workspace roles, a two-file launch
  without duplicate `open:` nodes, Save-plus-close and same-path Save As link
  freshness, and deterministic worker overlap/coalescing. Cargo.lock now keeps
  only the new direct dependencies and the shared view's solver-required
  package entries; unrelated resolver churn was removed. After these review
  corrections, `knot-desktop --lib` passes 166 tests with 1 ignored, the full
  default workspace passes, and the same strict clippy and locked-check lanes
  remain clean.

  The final correction resolves catalog identity before launcher insertion,
  so repeated copies of the same catalog-backed path reuse one session, tab
  and open graph node. If Graph is then explicitly closed between a surviving
  Navigator/site stack and reading stack, New reconstructs a distinct centre
  document stack instead of joining either side. The write-freshness receipt
  now drives a dirty tab through the close confirmation's Save path before it
  checks the refreshed catalog digest and link edge.

  A surviving metadata tile remains a centre-stack anchor after its source and
  Graph close, so New joins that metadata as a tab without creating a second
  centre or disturbing the Navigator/site and reading branches.

- **2026-09-29, step 9.** Five checked-in scenarios now cover focused writing,
  source beside Preview, research over authored references, the last-tab
  shared Mere graph, and two independently serving sites. They use the same
  Mesquite/Taproot lane as every earlier receipt. Scenario snapshots now name
  document and reading counts plus the open Preview, Outline, Readings,
  Navigator and Graph roles, so each scenario proves its arrangement before
  capture. The last-tab scenario closes the real document tab, waits for the
  bounded catalog worker and requires the shared view's “Catalog reading”
  state. Checked-in Djot, Rhai and two-site fixtures make the runs repeatable.

  All five action and assertion streams pass through the real desktop controls
  in the windowless harness; only their `capture` lines are omitted there
  because that harness has no native swapchain. The dedicated References
  custody panel and index-progress chip in the design research frame belong to
  later A3/E1 work and do not exist in this slice. The truthful current
  research receipt therefore shows Outline and a bounded authored-link Reading
  as tabs in the one right-hand stack, not the design frame's vertically split
  Outline/References pair. Automatic folded rails and the native/client-drawn
  menu work described by the design pass are likewise later Cambium/host work;
  the focused and Preview receipts exercise the slice's current toolbar and
  full stacks rather than pretending those seams landed here.

  Native capture was attempted on this macOS host both as the debug executable
  and through a LaunchServices application wrapper intended to foreground the
  same executable. In both cases
  the window and accessibility tree existed, but every surface acquisition
  reported `Occluded`; the lane exhausted its 120-frame capture patience and
  wrote `RESULT fail`, zero captures, and no PNG. The first exact receipt is
  retained at `/tmp/knot-step9.nJt4Xd/focused/scenario.done`; the wrapper
  receipt is `/tmp/knot-step9.nJt4Xd/focused-app/scenario.done`, and the
  120-second foreground probe wrote the same zero-frame result at
  `/tmp/knot-step9.nJt4Xd/root-foreground-probe/scenario.done`. Because
  no PNG exists, no whole-frame visual review is claimed. The original design
  PNGs referenced by the design pass are also absent from this checkout and
  from the current `Code` tree, so its annotated criteria in
  `2026-09-23_knot_design_pass.md` are the available comparison authority.
  Native whole-frame review, including the still-missing 7e two-site PNG,
  remains an environmental receipt blocker rather than a scenario omission.

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
