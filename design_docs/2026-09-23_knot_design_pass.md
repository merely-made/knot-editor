# Knot design pass

**Date:** 2026-09-23
**Status:** Rulings recorded 2026-09-23. Mark took each question in turn with
options, numbers and a recommendation in front of him; every ruling below is
his. Frames produced. The slice order at the end was ruled the same day.
Implementation not started.
**Owner:** Knot Editor
**Answers:** [design pass brief](2026-09-13_knot_design_pass_brief.md)

## What this settles

The brief named seven layout questions to answer before the multi-document
workspace adds tabs and a navigator. This records the answer to each, the
case made for it, and the alternatives that were on the table, so a later
configurability pass does not have to re-derive them. Drawing the frames
surfaced three further questions (the measure against hard-wrapped files,
what decides collapse, and how the reading stack gives way); those rulings
are recorded with the question they refine.

Three assumptions were stated during the walk-through and not objected to.
They are recorded as assumptions, not rulings, and are marked where they
apply.

## Rulings at a glance

| # | Question | Ruling |
| --- | --- | --- |
| 1 | Command surface | One command list feeds menus, palette, context menus and a short toolbar. Menus and the short toolbar share one row in a client-drawn title bar on Windows and Linux. macOS gets a native global menu. |
| 2 | Authority status | A bottom status bar: the message line, then one chip per authority. A chip opens a popover with detail and actions. A refusal raises its chip. One bar for the focused document; other documents carry a mark on their tab. |
| 3 | Panels | Readings are Workbench tiles, one right-hand stack by default. A reading follows the focused document and can be pinned per tile. The Navigator is a tile in a left stack. |
| 4 | Two readings | Typed by kind. Projections of the source match the source; renderings use reader appearance; list readings wear chrome type. |
| 5 | Site mode | Each open site gets a site tile beside the Navigator. Pages are document tabs; commands live in the Site menu; serving state is a chip. |
| 6 | Typography | Bundled IBM Plex Mono (system monospace offered); Source Serif 4 for preview; a picker over bundled, installed and downloadable faces; measure in characters, 72ch default, following a hard-wrapped file's wrap column; chrome fixed at 13px. |
| 7 | Narrow sizes | Automatic, configurable collapse. Side stacks give way to keep the source measure: the Navigator folds, the reading stack narrows to its minimum, then folds. Menus and drawers keep width thresholds. Chips collapse most-severe-first. |

## 1. Command surface

**Ruling.** Every command is one `CommandItem` in one list, rendered by
Cambium's `command_surface` as the palette, pickers, context menus and the
menus. Its id, shortcut, disabled reason and children travel with it, so a
menu entry shows the same shortcut the palette does.

On Windows and Linux the title bar is client-drawn, using the host
decorations seam that genet's
`docs/2026-08-10_window_decorations_brief.md` landed (W0 to W4, headed
receipts on Windows and both Macs): the bar declares `--app-region: drag` and
its controls `no-drag`. It carries, in one row: the menus (File, Edit, View,
Document, Site), the document title in the drag region, Save, the reading
toggles, and the caption buttons.

On macOS the menus go to the native global menu bar, populated from the same
command list. The window keeps the real traffic lights (the host reserves
their rect and reports the remainder), the title and the short toolbar. The
"drawn menu now, native later" option was offered and not chosen, so the
macOS title bar lands with the native-menu seam rather than with an interim
drawn menu.

**Why.** Eleven equal buttons (Readings joined the ten in the brief) cannot
absorb six panel kinds, search and a site strip. Menus hold every command with
its shortcut, which teaches the keyboard route the plan's keyboard-only
acceptance asks for, and the palette comes free from the same list. Moving
the menu into the title bar takes the chrome from roughly four rows today
(buttons, path, status, retention) to one.

**Alternatives kept.** A grouped toolbar with separators (cheapest; wraps
past about seventeen buttons). Palette plus a minimal bar (the quietest; a
"show menu bar" preference over this ruling gives it). A drawn in-window menu
on macOS.

**What moves.** The four shortcuts in `key_intercept`
(`apps/desktop/src/workspace.rs`, Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+Shift+S) move
into the command list. The path textbox stays a test and fallback capability
until the host supplies a picker, as the workspace plan says, but it leaves
the toolbar: File > Open path opens a single-line field in a popover, and the
harness keeps its route to it.

**Stack work.** A menu bar component in Cambium (ARIA menubar: roving focus
across the top-level items, Alt or F10 to enter, overflow into one Menu
button at narrow widths), composed from `selection_bar` and `command_menu`.
A native-menu seam in `cambium-genet-winit-host` for macOS. Nothing in the
stack links a native menu crate today.

## 2. Authority status

**Ruling.** A status bar along the bottom of the window. At its left, the
message line (`aria-live` polite, as today). At its right, one chip per
authority: format, save state, posture, catalog, retention, and later sync,
lock, index progress and recovery. E1 already wrote index progress as a
status string, `index · building 148/214`; it becomes a chip.

A chip opens a popover (Cambium's `overlay_surface` holding a
`detail_panel`) with the facts and the actions: Retry catalog, Retain
reviewed revision, and so on. The same actions are Document-menu commands. A
refusal raises its chip to warning weight and posts its sentence to the
message line; at narrow widths it keeps the visible slot (section 7).

One bar shows the focused document. Other open documents carry a mark on
their tab: the dirty dot, and a refused-state mark.

**Why.** The document starts at the top of the window instead of at 60
percent of its height, and every authority keeps an always-visible chip, so
the brief's truthful-status rule holds: weight and place change, no refusal
is hidden.

**Alternatives kept.** A Document panel in the reading set (competes with
readings for the one stack and duplicates the planned Changes panel). Inline
blocks that collapse when inert (closest to today; still pushes the document
down whenever something needs attention). A status line per stack (more
truthful in a split, more chrome).

**What moves.** The retention block becomes the Retention chip and its
popover. `knot-catalog-status` becomes the Catalog chip. The
`knot-document-status` sentence becomes chips. *Assumption:* the saved
revision review and the disk comparison are tasks, not status, and move into
the Changes tile (A4).

**Stack work.** A status bar component in Cambium with chip severity and the
collapse order of section 7. Turnstone has status lines too, so it earns its
place in the stack.

## 3. Panels

**Ruling.** Readings are Workbench tiles. The default layout is three stacks:
the Navigator on the left, documents in the centre, one reading stack on the
right. Splitting, dragging and later floating come from the same frame
documents use. A reading follows the focused document by default; each
reading tile has a Pin that holds it to one document, and its tab says which
("Preview · follows", or the pinned document's name). The Navigator is a
tile in the left stack; View reopens it if closed.

**Why.** "One panel" becomes the default layout rather than a rule, which is
the configurability rule. Documents and readings share one frame and one set
of gestures, instead of a Knot-owned panel region beside the Workbench. The
A2 text already anticipated it: Preview goes "in the right panel or a
sibling tab". The contract supports it without change: a `workbench::Tile`
carries a `ContentSource` with an open tail, `DropTarget::Edge` creates a
split, and `DropTarget::Outside` requests a tearout.

**Alternatives kept.** One panel as a rule, with a switcher. One panel by
default that can stack a second reading (a second layout system beside the
Workbench). Readings that always follow, or are always pinned. A fixed
Navigator sidebar outside the tree.

**What each control becomes.** Outline, Preview, Readings, References,
Lenses, Changes and Share are reading tiles. *Assumption:* the folded reading
is a reading tile until editable folds land, then an editor toggle.
*Assumption:* Appearance leaves the panel set and becomes a preferences
popover under View. Reload, Compare and Save As are File and Document menu
commands.

**State.** Follow and pin are Knot state keyed by `TileId`, the first of the
three membranes `cambium::workspace` describes; they survive a tile moving
between stacks and windows.

## 4. Two readings of the same source

**Ruling.** Readings are typed by kind, following the theme plan's three
targets (`mere/design_docs/mere_docs/implementation_strategy/2026-07-05_theme_modes_plan.md`):

- **Projections of the source match the source.** The folded source, and
  later editable folds, use the source's face, size, measure and colours.
  Fold controls sit in a gutter beside the lines they fold, drawn with
  `fold_projection`'s `fold-marker`, instead of the list of Collapse rows
  that today pushes the projection below the fold.
- **Renderings use reader appearance.** Preview and site previews take
  Tabard reader themes through the theme plan's native reader adapter,
  following the writer's size and measure unless changed. A site preview
  uses its format's reader presentation and reads as a page; Micron's
  authored colours stay source facts.
- **List readings wear chrome type.** Outline, the Navigator, Readings
  results and References.

Preview headings lose the chrome button styling that made the bordered band
the brief observed (defect D3). They stay controls that return to their
source, with hover and focus states.

**Alternatives kept.** Every reading in the source's face and measure (reads
as one document; Preview stops showing what a reader sees). Every reading
with its own look (the folded reading stays a structural list).

## 5. Site mode

**Ruling.** Each open site gets a site tile in the left stack, tabbed beside
the Navigator. It lists the pages, Metadata, the format, and the publication
settings and state. Pages open as ordinary document tabs; the Preview tile
follows the focused page in the site's reader presentation; site commands
live in the Site menu (create with a format choice, open, close, metadata,
publish locally, stop serving, upload or submit); serving state is a chip,
for example "Tidewatch · serving :5699". Closing the site tile closes the
site through the usual dirty-page preflight. The Metadata form opens as a
tile in the document stack, since it is authored content.

**Why.** One data source per reading: the Navigator reads the file catalog
(available and unavailable history, as the first-slice plan scopes it) and
the site tile reads the site folder. The separate site strip disappears.

**Prerequisite.** Per-document `ScrollWorkspace` state, already required by
the workspace plan before site tabs, which also allows several sites open at
once.

**Alternatives kept.** A site as a group inside the Navigator (mixes the
folder listing into catalog history). A Site tile in the reading stack
(competes with Preview while authoring a site). A separate site window
(needs multi-window first).

## 6. Typography

**Ruling.**

- **Source face.** IBM Plex Mono, bundled, is the default. System monospace
  is offered without bundling. (Recorded interpretation of "I'm ok with ibm
  and system monospace as defaults"; correct it here if meant otherwise.)
- **Preview face.** Source Serif 4, bundled, is the reader-appearance default.
- **Picker.** It lists bundled, installed and downloadable faces. Mark's
  point: nobody should have to pick for everyone when the faces share a
  licence and are small, and people with their own fonts should be able to
  use them.
  - *Installed* faces already render: `genet-livery`'s `TextSystem` builds
    its `FontContext` over fontique's system collection, so any installed
    family resolves by name. The picker needs a family-list accessor there.
  - *Bundled* faces use `cambium-rootstock`'s `HostFont { family, bytes }`
    seam, which registers them into every text system the host builds.
  - *Downloadable* faces come from a git source (the `google/fonts`
    repository's `ofl/` directories) with one button: fetch through
    `mere-fetch`, verify a digest, store under `%APPDATA%\Knot\fonts`,
    register. Registering after launch is new host work, since
    `HostOptions.fonts` is fixed at launch today. The catalog shows each
    family's licence, because `google/fonts` also carries `apache/` and
    `ufl/` families.
  - This is Knot's first network fetch. The E1 ruling that Knot acquires no
    fetcher concerned model weights and stands unchanged.
- **Measure.** In characters, so it scales with the font size preference:
  Narrow 60ch, Medium 72ch (the default), Wide 90ch, Full. `ch` resolves from
  real advances; `TextSystem` caches `ch_advances` per face.
- **Hard-wrapped files** (ruled after the frames showed it). When a file is
  hard-wrapped, the source measure follows its wrap column, so each written
  line stays one screen line. Soft-wrapped files, which include everything
  Knot writes, keep 72ch. A preference turns this off. Preview keeps the
  writer's 72ch. The fixture wraps at up to 76 columns: three of its 47 lines
  exceed 72, and at 72ch they wrap again.
  - Detection is a pure function over the source, testable alone in
    `knot-document`: a file is hard-wrapped when most paragraph lines are
    followed by a continuation line, and its wrap column is the longest such
    line.
  - The rendered source box carries one `ch` of slack, because a line exactly
    at the wrap column otherwise wraps from rounding (the frames caught a
    76-character line doing so at 76ch). The collapse rule in section 7
    compares against the wrap column itself.
- **Chrome.** Fixed at 13px: menus, tabs, the status bar and list readings,
  independent of the source size preference. Host UI zoom still scales
  everything.
- **Preview heading scale.** 1.6em, 1.25em, 1.05em, replacing a display
  heading at roughly three times body size.

**Alternatives kept.** JetBrains Mono or iA Writer Duo as the default source
face; Plex Serif as the preview face; pixel measures; chrome that follows the
body size.

## 7. Narrow sizes

**Ruling.** Collapse is automatic, and every threshold is a preference.

- **Side stacks keep the source measure** (ruled after the frames showed that
  three columns at the default window leave the source about 50 columns
  wide). When the writing column would fall below the source measure, the
  Navigator folds to a rail first. The reading stack then narrows, down to a
  minimum (default 280px), and folds only if the source still cannot keep its
  measure. The split ratio is kept and comes back when there is room.
- **Menus** fold into one Menu button below about 700px.
- **Drawers.** Below about 500px a side stack opens as a drawer over the
  document, on `overlay_surface`.
- **Status chips** collapse most-severe-first; a refusal always keeps the
  visible slot, and "+N" opens the rest.
- **Target.** Usable at 320 CSS px wide: the WCAG reflow width, and a 1280px
  window at 400% zoom, so small windows and high zoom get one answer.

**Numbers.** Plex Mono advances 9.6px at 16px. The fixture's 76ch source
needs 729.6px of text plus 56px of padding, 786px. At the default 1100px
window, with the Navigator folded to a 28px rail and two 1px dividers, the
reading stack gets 284px. At Mark's laptop work area, about 1280 logical
pixels wide, it gets 464px. At 640px both side stacks fold and the source
wraps below its measure, which is the case drawers exist for.

**Alternatives kept.** Fixed window-width thresholds for stacks. Folding the
reading stack without narrowing it first (source beside preview then needs
about 1190px). Never auto-folding a reading the writer opened.

**Stack work.** The Workbench needs collapsed stacks (rails), minimum widths
and a drawer mode; the give-way policy computes stack fractions from the
measure on each layout, on the host side.

## Frames

Rendered 2026-09-23 from HTML mockups in `Code/testing/knot-editor/mockups/`
(`knot-mockup.css` plus one page per arrangement) by `render.ps1`, which runs
headless Chrome at 1100 x 700 CSS px and device scale 2, giving 2200 x 1400,
the physical size the capture harness produces for the default window on
this display. The baseline captures are 2178 x 1389 because the harness crops
an OS-decorated window to its DWM frame; with a client-drawn title bar the
harness frame includes the title bar. Compare layout, not glyph pixels: the
mockups are Chrome's text rendering, not Livery's.

The material is the `fixtures/field_notes.djot` fixture. The research frame
extends its "Open questions" section with three citation links (two clips
and a shared note) to show custody states; the fixture has one link.

| Frame | File in `Code/testing/knot-editor/images/` |
| --- | --- |
| Focused writing | `2026-09-23_design_focused_writing.png`, `_annotated.png` |
| Source beside preview | `2026-09-23_design_source_beside_preview.png`, `_annotated.png` |
| Research with references | `2026-09-23_design_research_with_references.png`, `_annotated.png` |
| Source beside preview at 640px | `2026-09-23_design_source_beside_preview_640.png` |

Annotated markers, **focused writing**:

1. Menus in the title bar (1).
2. The document title in the drag region (1).
3. The short toolbar: Save, then the reading toggles (1).
4. Client-drawn caption buttons on Windows and Linux (1).
5. The Navigator folded to a rail (3, 7).
6. The document stack's own tab strip; the dirty mark on Untitled (2, 3).
7. The source measure at the file's 76-column wrap (6).
8. The message line (2).
9. Authority chips, quiet while inert (2).

**Source beside preview**:

1. The Navigator folded to keep the source measure (7).
2. The source at the file's wrap column (6).
3. The reading stack narrowed to 284px before folding (7).
4. The Preview tile follows the focused document; the close control closes
   the tile (3).
5. The reading header: document, edit and diagnostics, truncating before the
   Pin button (3, D2).
6. Reader appearance; headings return to source without button chrome (4,
   D3).
7. Chips (2).

**Research with references**:

1. The folded left stack lists both its tiles, the Navigator and the
   Tidewatch site tile (5).
2. Outline split above References in the reading column; each follows (3).
3. References: declared references with custody in plain words (A3).
4. States that block use, locked and denied, raised.
5. A refusal on the message line (2).
6. The chip's popover, with the facts and the Retry action (2).
7. Index progress as a chip (E1).

**640px**: the menus fold into Menu, both side stacks fold to rails, and the
status bar reads "Saved +3" (7).

## Stylesheet proposal

The palette moves into custom properties that `appearance_css()` emits once
per theme scope, and the rules read the properties. Today
`appearance_css()` repeats each coloured rule per theme with literal colours.
The focus rule is unchanged: one per theme in the primary token, so the
`each_theme_has_one_focus_rule_in_its_primary_token` test keeps holding.

```css
.knot-theme-light {           /* values from derive_palette(&seeds(false)) */
  --knot-bg: <p.bg>;
  --knot-surface: <p.surface>;
  --knot-control: <p.surface_2>;
  --knot-control-hover: <p.surface_hover>;
  --knot-text: <p.text>;
  --knot-text-dim: <p.text_dim>;
  --knot-accent: <p.primary>;
  --knot-danger: <p.danger>;
  --knot-warning-*: <new: derived from the tertiary seed>;
}
.knot-workspace {
  --knot-chrome-size: 13px;
  --knot-source-face: 'IBM Plex Mono', monospace;   /* the face preference */
  --knot-reader-face: 'Source Serif 4', serif;      /* reader appearance */
  --knot-measure: 72ch;                             /* the measure preference */
  --knot-source-measure: <wrap column or measure>;  /* per document */
}
```

Tinct's palette has no warning role today; one derived from the tertiary
seed is a Tinct addition, which is Cambium work.

Against `DESKTOP_CSS`:

| Current rules | Become |
| --- | --- |
| `.knot-workspace` padding and column gap | The three-row grid: title bar, frame, status bar |
| `.knot-workspace-toolbar`, `.knot-path-field` | The title bar; the path field moves into the Open path popover |
| `.knot-workspace-message` | `.knot-message` in the status bar |
| `.knot-catalog-status`, `.knot-catalog-error` | The Catalog chip and its popover |
| `.knot-retention*` | Retention popover content |
| `.knot-writing-area`, `.knot-outline*`, the `max-width:700px` block | Frisket stacks and tiles; outline rows keep their level indents; the collapse rules |
| `.knot-document-body textarea` min-height 360px | The tile fills its stack |
| `.knot-review*`, `.knot-comparison*` | Kept, inside the Changes tile (assumption) |
| `.knot-confirm` | Kept as the dialog |
| `.knot-appearance-*` | Kept, inside the preferences popover |

The complete rule set, using Frisket's class names (`frisket-stack`,
`frisket-tabbar`, `frisket-tab` with `active`, `frisket-close`,
`frisket-content`, `frisket-divider`) so it moves across without renaming, is
`Code/testing/knot-editor/mockups/knot-mockup.css`. Before adopting it,
verify these in Livery: `var()` substitution (custom properties already
cascade; the decorations seam reads `--app-region` off the cascade), `calc()`
with `ch`, and `writing-mode: vertical-rl` for rail labels (fallback: a short
horizontal label).

## Layout defects confirmed

Filed in the workspace plan's progress section so the slice that touches each
one fixes it.

- **D1. Outline and Readings drawn over the status column and editor.** Seen
  in the 2026-09-13 outline frame and the 2026-09-15 Readings frame. Not a
  plain CSS fault: the source wrapper collapses to the status column's width
  while the editor paints across the full row, which points at how the
  editor surface is sized, not at `.knot-outline { flex:0 0 280px }`. The
  hand-built region retires under ruling 3; the first slice needs a headed
  check that editor surfaces follow their tile's width.
- **D2. Preview header runs two facts together**
  ("…field_notes.djotPreview diagnostics: none"). The reading header uses
  separators and truncates before its Pin button.
- **D3. Preview headings wear chrome button styling.** They are `button`
  elements with `.knot-document-preview-heading { display:block; width:100% }`,
  which draws the bordered band.
- **D4. The folded reading's Collapse rows push the projection off-screen.**
  The `fold_projection` source sits below the row list; controls move to a
  gutter (ruling 4).
- **D5. The Windows verbatim path prefix is displayed.** `knot_site::Site::open`
  and `page_path` canonicalize (`crates/knot-site/src/lib.rs`), and
  `workspace.rs` puts that path in the path field as is, so the site frame
  shows `\\?\C:\…`. Identity checks keep the canonical form; display strips
  the prefix.
- **D6. Single-line fields wrap long values.** The path field and the site
  folder field are `text_field_typed` inputs, and in the site frame both
  break their value onto a second line; the site folder's second line spills
  outside its border. Cause not confirmed; it lies in Cambium's text field or
  Livery's input layout, not in Knot's stylesheet.

## Slice order

Ruled 2026-09-23. It also places the two items the workspace plan's "Scoped
next sequence" had after its navigator slice: local recovery and the
document-to-document relation workflow.

1. **Frame, tiles and status.** `DocumentWorkspace`, the three stacks through
   `cambium::workspace_view`, reading tiles with follow and pin, and the
   status bar with chips and popovers. This carries the plan's first
   workspace slice and its navigator slice (the Navigator is a tile), retires
   the toolbar-row blocks and D1 to D4. The title bar stays OS-drawn, and a
   provisional in-window command row carries the command list until slice 2.
   - **Then local recovery**, as the workspace plan scopes it. Several
     documents can be open from here on, and the status bar gives a recovery
     candidate its chip.
2. **Commands.** The command list, the palette, the Cambium menu bar and the
   client-drawn title bar on Windows and Linux.
   - **Then the document-to-document relation workflow.** Relation actions
     are commands, so they follow the command list.
3. **Typography.** The bundled faces through `HostFont`, the measure in
   characters, wrap-column detection and 13px chrome.
4. **Collapse.** Workbench rails, minimum widths and drawers; the give-way
   policy; chip collapse.
5. **Fonts.** The installed-family picker, then the one-button download with
   runtime registration.
6. **macOS menus.** The native-menu seam and the macOS title bar.

On macOS the OS title bar and slice 1's provisional command row stay through
slices 2 to 5; that row is a plain row of commands, not a drawn menu bar, so
ruling 1 holds. Ruled to stay rather than moving the macOS menus up.

## Later, because

- **Editable folds.** Mere's coordinate-map track; the folded reading stays
  read-only until its receipts pass.
- **Dark frames.** The baseline is light, so the frames are light; the tokens
  carry dark.
- **The site tile, palette, Changes, Share and Lenses frames.** Not drawn;
  their places are fixed above and their content is their own plans' work.
- **Drawer frames.** The 500px drawer behaviour is ruled but not drawn.

## Related material

- [Design pass brief](2026-09-13_knot_design_pass_brief.md)
- [Application workspace plan](2026-09-05_knot_application_workspace_plan.md)
- [Predicates, inference and scripted readings](2026-09-15_knot_predicates_inference_scripting_plan.md) (E1's index progress string)
- [Native small-web authoring](2026-09-11_small_web_authoring_plan.md)
- `genet/docs/2026-08-10_window_decorations_brief.md`
- `mere/design_docs/mere_docs/implementation_strategy/2026-07-05_theme_modes_plan.md`
- `mere/crates/cambium/cambium/src/{command_surface,frisket,workspace,overlay_surface,detail_panel,fold_projection}.rs`
- `mere/crates/cambium/workbench/lib.rs`
- `genet/components/genet-livery/src/text.rs` (`TextSystem`, `ch_advances`, font registration)
- `mere/crates/cambium/cambium-rootstock/src/host.rs` (`HostFont`)
