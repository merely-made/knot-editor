# Knot design pass brief

**Date:** 2026-09-13
**Status:** Brief. Collects the material a design pass over the standalone
Knot desktop can work from, records what the current surface looks like, and
names the decisions the pass has to make. It does not decide them.
**Owner:** Knot Editor

## Why now

The next implementation sequence in
[the workspace plan](2026-09-05_knot_application_workspace_plan.md) is the
multi-document workspace: a Knot-owned `DocumentWorkspace`, one tab stack,
a catalog navigator, local recovery, then document relations. Each of those
adds chrome. The current desktop grew one control at a time as receipts
landed, and no pass has yet asked what the whole should look like. Doing that
before tabs and a navigator arrive is cheaper than after.

## Materials

Everything here existed before this brief except the screenshots and the
capture harness.

| Material | Where | What it gives the pass |
| --- | --- | --- |
| Intended layout | workspace plan, "Workspace layout" | Navigator left, writing area centre, one switchable right panel. "Deliberately quiet." |
| Preference inventory | workspace plan, "Settings and recovery boundaries" | Which appearance knobs are app preferences and which are not. |
| Appearance model | `apps/desktop/src/appearance.rs` | Tinct seeds, light/dark, highlight toggle, 12 to 24 px source type, writing width, line spacing. |
| Desktop stylesheet | `DESKTOP_CSS` in `apps/desktop/src/workspace.rs`, plus `document_folding::CSS` and `document_preview::CSS` | The whole current visual vocabulary. Flex columns, 1 px borders, 4 px radii, system-ui. |
| Theme boundary | `mere/design_docs/mere_docs/implementation_strategy/2026-07-05_theme_modes_plan.md` | App chrome, reader appearance, and author stylesheet are three targets. A reader preference never rewrites source. |
| Widget tiers | `genet/docs/2026-07-08_chisel_widget_catalog.md` | What is free CSS, what is a leaf, what needs the arrangement mechanism. Keeps proposals buildable. |
| Text editing ladder | `genet/docs/2026-07-25_text_editing_primitive_plan.md` and the Mere virtualized editor plan | The one `TextInput`, the read-only fold projection, and the editable-fold track that stays in Mere. |
| Command surfaces | `mere/design_docs/mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md` | Palette, context menu, direct gesture, shellbar: four surfaces fanning out from one action bus. Dated, but the vocabulary holds. |
| Host zoom rule | workspace plan, "Settings and recovery boundaries" | Zoom goes through the host. No independent CSS scale with different input coordinates. |
| Screenshots | `Code/testing/knot-editor/images/2026-09-13_*.png` | Five states of the current desktop at the app's default 1100 x 700 logical size. |
| Capture harness | `Code/testing/knot-editor/shoot.ps1`, `capture.ps1` | Launch, optional frame-relative clicks, PrintWindow capture, stop. Reproducible on any display. |
| Fixtures | `Code/testing/knot-editor/fixtures/field_notes.djot`, `fixtures/tidewatch/` | A Djot note with headings, lists, a quote, code, and links; a three-page Scroll site with metadata. |

There is no mockup, wireframe, or typography reference anywhere in the tree.
The pass produces the first ones.

## The current surface, observed

Five captures, taken 2026-09-13 from the `knot.exe` debug build at commit
`d460670`. Window 1100 x 700 logical, light theme, defaults.

1. `2026-09-13_djot_launch.png`: a Djot file at launch.
2. `2026-09-13_djot_outline_preview.png`: the same file with Outline and
   Preview shown.
3. `2026-09-13_djot_folds.png`: the same file in the read-only folded reading.
4. `2026-09-13_scratch_launch.png`: an untitled scratch document.
5. `2026-09-13_scroll_site_launch.png`: a Scroll site folder at launch.

What the frames show, in the order a first-time viewer meets it:

- **The document starts below the fold.** Toolbar, path field, a status line,
  and the retention block occupy the top half of the default window. The
  first source line appears at roughly 60 percent of the window height. In
  the scratch state the writing area is an empty box under an empty path
  field. The plan's "writing area in the middle" is not what the app shows.
- **The retention block is always present and almost always inert.** "Retain
  reviewed revision / No storage destinations available / No destination
  selected" is the largest bordered element on screen in every non-site
  state, even with no catalog, no persona, and no destination. It is the
  clearest case of an authority-truthful status rendered at feature weight.
- **Ten equal buttons in one row.** Site, New, Open, Save As, Reload,
  Compare, Show Preview, Show folds, Appearance, Show Outline. File
  lifecycle, readings, and preferences share one strip with no grouping and
  no hierarchy. Save sits separately beside the status line. The Scroll state
  adds a second strip of ten more (formats, Create, Open, Close, Metadata,
  Toggle preview, Upload, page buttons, port, Publish, Stop).
- **The outline overlaps instead of sitting beside.** With Outline shown, the
  panel is drawn over the status column and the left edge of the source
  editor, and the status facts wrap into a narrow column behind it. The
  stylesheet says `flex:0 0 280px` inside a flex row, so this is a layout
  defect to confirm and fix, not a design choice. The done-condition for A2
  did not include a headed frame, which is how it survived.
- **Preview's header runs two facts together.** "Source: file:///…/field_notes.djotPreview diagnostics: none" has no separator between the address and the diagnostics.
- **Two type systems.** Chrome and preview are system-ui sans; the source is
  the platform monospace at the same size. The preview heading "Field notes
  on the estuary" is set at roughly three times body size inside a bordered
  band, which is louder than anything else on screen.
- **The Scroll preview is a reading, not a page.** It renders a heading, a
  paragraph, and link buttons in the chrome style. That is honest to the
  three-target rule but leaves no visual difference between "this is what a
  Scroll reader would show" and "this is a form".
- **Status is a sentence, not a state.** "Source: field_notes.djot Format:
  Djot Clean Posture: file target Save: not attempted" reads as a log line.
  The facts are the right ones.
- **The folded reading is a list, not a source view.** Rows read "Section ·
  line 6 · ## Morning transect" with a Collapse button each. It is truthful
  and read-only by design; it does not yet look like the same document.

None of this is a surprise given how the desktop was built. It is the
baseline the pass measures against.

## The intended shape

From the workspace plan, unchanged by this brief:

- A document navigator on the left. Open documents, recent files or vault,
  search results. Unavailable, locked, and denied states are labelled, never
  guessed.
- The writing area in the middle. Title, source and state, the source
  editor, inline diagnostics, a status line for save, sync, and lock.
- One optional right-hand panel that switches among Outline, Preview,
  References, Lenses, Changes, and Share. Not six columns.
- Tabs represent open document sessions. A document can move between
  stacks or to a floating window through the shared workspace tree.
- Source is authoritative. Every panel is a reading that selects source
  ranges. When a reading cannot be produced, the writing area stays usable
  and the panel says why.

## Planned features and directions the pass must leave room for

These are scoped or ruled in other plans. The pass designs their places, not
their behaviour.

**Next sequence (workspace plan, "Scoped next sequence").**

- `DocumentWorkspace` with runtime document keys; duplicate Open activates
  the existing tab.
- One `workbench::Workspace` tab stack via `cambium::workspace_view`.
  New, Open, activate, close, with dirty close preflighted through
  Save/Discard/Cancel and custody refusal. Splits, floats, and tab dragging
  are later adoptions of the same model.
- A catalog navigator reading `KnotFileCatalog::records()` with available
  and unavailable history shown explicitly. Rebinding unavailable history is
  a user action.
- Local recovery: application-scoped, versioned, checksummed, bounded and
  configurable retention. Restoration produces a candidate, never an
  overwrite.
- Native Scroll, Gemtext, and Micron tabs wait until `ScrollWorkspace` state
  is per document.

**Readings and lenses (A2, A3).**

- Preview with source ranges for every block, so any rendered element can
  return to its source.
- References panel: declared clip references versus bytes actually
  available, with custody states (absent, denied, locked, stale, failed)
  said plainly.
- Lenses panel for Rosette: rhyme, meter, and coverage as optional readings
  with per-workspace geometry preferences. A compact list first; the graph
  projection stays optional.
- Editable visual folds arrive from Mere's coordinate-map track, replacing
  the read-only folded reading only after headed and accessibility
  receipts.

**Changes, drafts, sharing (A4).**

- Changes panel: current head, pending operations, automatic merges,
  concurrent versions, explicit resolve. Conflicts appear in the working
  document, not as detached records.
- Titled drafts are a design gate, not a label on a dirty buffer.
- Share panel only for a publishable document with an admitted authority.
  Names revision, scope, recipient, expiry, pinned head, endpoint
  availability, revocation outcome. Unavailable is actionable status, never
  fake success.

**Connected writing (G1 to G3).**

- Links, backlinks, passage targets; optional named predicates. The ordinary
  link gesture works before any relation form opens.
- Saved questions with coordinated presentations: table, network, reading
  sequence. A layout-only drag never changes a semantic relation.
- Explainable graph operations.

**Small-web authoring (the two 2026-09-11 plans).**

- Scroll, Gemini, Spartan, and Micron sites: page list, metadata form,
  native preview, local publication with explicit start and stop, reviewed
  submission. Micron request forms.
- Explicit limits to respect: no Scroll highlighting or outline yet, assets
  reserved, page add/remove is a later site-management slice.

**Preferences.**

- Cross-launch storage of appearance preferences is open.
- Persistent recent files are a later preference slice, scoped beside
  recovery so retention is visible.
- UI zoom is supplied to the host at startup and applied through it.

## Constraints the pass works inside

- **Tier 1 first.** Layout, borders, hover, focus, scrolling, and theming
  are CSS on the retained DOM. A proposal that needs a leaf widget has to
  say so and name the tier.
- **One text model.** The source editor is the one Cambium `TextInput`.
  Readings decorate or project it; nothing mirrors it.
- **Three theme targets.** App chrome, reader appearance, and author
  stylesheet are separate. Knot's palette comes from Tinct seeds; the
  contrast test in `appearance.rs` is the floor.
- **Truthful status.** Every authority state the app shows today (custody,
  catalog, retention, endpoint) has to remain visible somewhere. The pass
  may change weight, place, and grouping. It may not hide a refusal.
- **Host owns zoom and placement.** No independent scale.
- **Configurable over opinionated.** Where the plan lists a preference,
  expose it; do not pick one reading mode and hard-code it.
- **Plain vocabulary in the product.** Panel and control names are working
  words. The evocative names are a product-tier decision, not a chrome one.

## Questions the pass has to answer

Each has more than one defensible answer. They are Mark's.

1. **Toolbar or menu.** Ten equal buttons cannot absorb tabs, a navigator,
   and six panel kinds. Options: a grouped toolbar with separators; a menu
   bar plus a short toolbar; a command palette plus a minimal bar in the
   Mere four-surface style. Each changes what the first slice's tab strip
   sits under.
2. **Where authority status lives.** The retention block, catalog status,
   and save posture are all truthful and all heavy. Options: a single
   status bar with expandable detail; a "Document" panel in the right-panel
   set; keep inline but collapse when inert.
3. **Panel switching versus side-by-side.** The plan says one right panel.
   Outline plus Preview together is a common writing arrangement. Decide
   whether "one panel" is a default or a rule.
4. **Two readings of the same source.** Preview and folded source currently
   look unrelated to the editor and to each other. Decide whether readings
   share the source's type and measure (they are the same document) or
   deliberately differ (they are readings, not the document).
5. **Site mode.** The Scroll strip doubles the chrome. Options: a site
   navigator that replaces the document navigator while a site is open; a
   Site panel in the right-panel set; a separate site window.
6. **Typography.** Monospace source at 16 px with 1.7 line height, sans
   chrome at the same size, one display heading in preview. Decide the
   source face, the preview face, the measure (currently 900 px max), and
   whether chrome type is a step smaller than body type.
7. **Density at narrow sizes.** The plan asks for narrow and regular
   acceptance. Nothing narrower than the default has been captured.

## What the pass produces

- A dated design doc in this directory with the answers to the questions
  above and the reasoning, plus annotated frames or mockups for the three
  arrangements the plan names: focused writing, source beside preview,
  research with references.
- A stylesheet proposal small enough to review as a diff against
  `DESKTOP_CSS` and `appearance_css()`.
- A list of layout defects confirmed from the frames (the outline overlap,
  the preview header separator) filed against the workspace plan's
  progress section, so they are fixed in the slice that touches them.

*Done when* the three arrangements exist as frames the next session can
compare against captures from the same harness; every question above has a
recorded answer or an explicit "later, because"; and the multi-document
first slice can place its tab strip and navigator without inventing a new
layout.

## Capture notes

`shoot.ps1` starts `target/debug/knot.exe` with the given arguments, waits,
sizes the window to a fixed physical size, optionally clicks at
frame-relative coordinates, captures with PrintWindow and
`PW_RENDERFULLCONTENT`, crops to the DWM visible frame, and stops the
process. The window is found as the largest visible top-level window of the
process id, because the winit host's `MainWindowHandle` can be a stub.
PrintWindow works on this host on the first try, as it did on pelt; no
compositor grab was needed.

Clicks are synthetic pointer input and can land in another window if the
foreground changes between the foreground call and the click. The two
click captures were checked by eye and show the expected state. Knot has no
self-drive scenario lane yet; when one lands, the harness should switch to
it and the click step should go.

On this display the app's default 1100 x 700 logical size is 2200 x 1400
physical. A capture at another size is a different layout and should be
named for it.
