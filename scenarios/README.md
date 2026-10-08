# Knot headed workspace receipts

These scenarios use Knot's existing Mesquite/Taproot lane. They do not have a
second runner. Each scenario asserts the workspace arrangement before asking
the native host for a whole-frame PNG.

Build the desktop once:

```sh
cargo build -p knot-desktop --bin knot
```

Use a fresh output directory for each scenario. `focused_writing.scn` and
`source_beside_preview.scn` launch with
`scenarios/fixtures/field_notes.djot`. `research_with_references.scn` also
expects `references.rhai` under the launch settings root's `Knot/readings`
directory; the checked-in source is
`scenarios/fixtures/readings/references.rhai`.

`last_tab_graph.scn` launches the same document with an explicit catalog:

```sh
KNOT_SCENARIO=scenarios/last_tab_graph.scn \
KNOT_CAPTURE_DIR=/tmp/knot-step9-graph \
KNOT_RECEIPT=/tmp/knot-step9-graph/scenario.done \
target/debug/knot \
  --catalog-root scenarios/fixtures \
  --catalog /tmp/knot-step9-graph/catalog.redb \
  scenarios/fixtures/field_notes.djot
```

That receipt closes the only document and asserts the shared Mere graph's
catalog-reading state before capture. It does not substitute a Knot-local
graph.

The two-site receipt launches both checked-in site folders:

```sh
KNOT_SCENARIO=scenarios/two_sites.scn \
KNOT_CAPTURE_DIR=/tmp/knot-step9-two-sites \
KNOT_RECEIPT=/tmp/knot-step9-two-sites/scenario.done \
target/debug/knot \
  scenarios/fixtures/site-one \
  scenarios/fixtures/site-two
```

It selects each site tile, publishes that site's saved snapshot, asserts two
independent serving owners, and captures the resulting whole frame.

The integration test in `apps/desktop/tests/scenario_lane.rs` runs all five
scenario action/assertion streams against the real retained desktop controls.
It removes only `capture` lines because the windowless harness has no native
swapchain. The headed invocation remains the authority for PNG receipts.

The focused-writing receipt clicks the source textbox and asserts its
host-owned keyboard focus without dirtying the document. Preview receipts
assert a successfully rendered Preview and its heading count rather than
matching source text on the global surface. Research asserts both the reading
result row count and exact labels emitted by `references.rhai`. Open-reading
snapshot flags count pinned and following tiles alike, and `wait` includes
graph, publication, retention, and pending capture work.

`relationship_recipe_compact.scn` launches
`scenarios/fixtures/composition/selection.djot` with isolated settings. It uses
the compact View menu, binds actual opt-in sound results, edits spacing, selects
the second spatial occurrence, explains the rhyme, checks the original-source
action, and refuses stale rebind and retention without authority. Paragraph
clicks deliberately reveal non-interactive explanation/anchor/notice text for
capture; they do not invoke a mutation. The headless regression runs at 420×900
and at 1280×900 with 400% UI zoom. Native receipts still require foreground
capture review at both settings; matching hidden text is not visual acceptance.

`collection_organization.scn` and `collection_organization_reopen.scn` run with
the `collection_acceptance` desktop example, not ordinary personal-wallet
startup. Initialize with `--initialize-test-fixture /absolute/empty/test/root`,
then use `--reopen-test-fixture` on that same marked disposable root in a new
process. The instrument uses public test keys and must never hold personal data.
The scenarios exercise the visible details editor, archive/restore, encrypted
fresh-process reopen and original-source navigation. Worker/store tests cover
edited metadata, stale revision and authority refusal; the scenario's unchanged
details save alone does not prove typing into every field.
The `_compact` and `_compact_reopen` variants use the compact View menu at 420px;
the wide scenarios' direct Readings toolbar button is not a compact selector.

`relationship_scene.scn` and `relationship_scene_compact.scn` launch the same
selection fixture and exercise the opt-in static shared canvas, explicit
foreground/background roles, camera controls, and unchanged original-source
navigation. The windowless regression runs wide, 420px compact, and 400% zoom
streams. These scenarios do not attach retention authority; encrypted save and
fresh-process reopen require the marked `collection_acceptance` fixture and a
separate native receipt. Static graph drag is deliberately ignored; it is not
an advertised move or pin operation.

The `_retained` and `_retained_compact` scene variants use the marked
`collection_acceptance` fixture, promote light and demote Night, exclude the
perfect-rhyme background category while leaving the master background enabled,
author pan 0.10/0.00 and zoom 1.25, and explicitly retain version 2 presentation. Their
wide and compact `_reopen` counterparts assert those values in a separate
process before restoring all background categories and using Fit. Category controls
reopen collapsed; the scenarios expand them, assert the retained exclusion and
capture restored categories before collapsing them again. Canvas clicks are
interactive: a click over a relationship can select its explanation. They are
not equivalent to the non-interactive paragraph reveal clicks described above.

Camera refinement: Fit and Zoom remain available in a wrapping row. Pan buttons
are created only after `Show scene camera controls` and removed after `Hide
scene camera controls`. This disclosure is session-only, not saved presentation.
Retained scenarios capture the expanded controls and collapse them after authoring
pan; fresh-process reopen asserts the collapsed disclosure alongside the restored
camera. Foreground/background have both a canvas-matching color key and a textual
selected-role label. Reset remains available even when the overview is hidden.

Editorial continuation: retained scene scenarios first reveal declaration-driven
arrangement controls, reject columns 0 without replacing the recipe, apply columns
2, undo and redo, then retain the accepted options with the scene. Reopen asserts
columns 2 and a confirmed retained marker, removes the override through Use default,
and undoes to the retained position. Both widths capture expanded option controls.
Scene and arrangement disclosures reopen collapsed; session undo does not persist.
The October 8 committed-runtime native qualification covers five captures per
save/reopen process at each width (20 final captures); the composition retention
plan records the local receipts and independent layout-performance publication hold.
