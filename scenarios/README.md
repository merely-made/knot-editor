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
