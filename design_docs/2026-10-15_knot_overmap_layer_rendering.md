# Rendering the mere overmap as a layer

Status: implemented and natively qualified on macOS (see Checks). Superseded
in rendering by the in-graph tier (Addendum, 2026-10-15 later); the band
stays a mere-view option. Follows
[the overmap layer contract](2026-10-15_knot_mere_overmap_layer.md).

## Problem

Mark's ruling is "catalog, route as overmap": Knot's catalog graph stays the
graph, and Djinn's `mere/<domain>` route shows as an overmap layer above it,
with sessions as container nodes and archives on them. Knot `a27911a` wired
the data but rendered it through mere-view's standard side panel: a column of
buttons beside the catalog graph, with every codicil in a list at its foot.
Nothing in that column reads as a node, a container or a layer, and nothing
says which session an archive was saved from.

## Proposal

**Above, not around.** The overmap is one level up from the catalog
(Turnstone's `overmap.rs` projects sessions the same way, as container nodes
one level up). It renders as a band across the top of the graph's area. The
catalog graph keeps the full width and is laid out in the area below the band.
The catalog is never drawn inside a session's container: the contract says the
route's graph is never shown and the catalog wins, and framing the catalog in
the attached session would claim it is that session's content. The attached
session says "Shown here" and has a heavier frame.

**Sessions are container nodes.** Each session is a framed node in the band:
its name, its state in words ("Shown here", "In the trash", the host's
detail), its steps, and its archives. Containers wrap into rows at the band's
width; the band's height follows the content, up to 60% of the area, and
scrolls beyond that.

**Archives sit on the session they were saved from.** A codicil saved from one
of the route's sessions is a chip inside that session's container. A codicil
the owner does not attribute to exactly one live session (composed from
several, from a trashed or forgotten session, or from an owner that does not
say) sits on an "Archive" shelf, a container at the end of the band. The shelf
also holds Compose, because a composition spans sessions.

**Interactions live on the nodes.**

| Node | Interactions |
| --- | --- |
| Session container | Switch to, Fork, Trash or Restore, Save to archive |
| Archive chip (on a session or the shelf) | Open as session, Select (for composition) |
| Archive shelf | Compose selected archives |
| "New session" container, last in the band | Mint |
| Catalog node (canvas, below) | Activate, unchanged |

Labels, `data-*` tokens and accessible names are the side panel's, so the
requests, notices and scenario selectors do not change. The notice moves below
the band, over the top of the catalog.

## Where the change lives

mere-view (Mere), small and generic, so Turnstone or any host whose graph is
not a session's own can use it:

- `MereViewModel::placement: SessionPlacement` — `Beside` (default, today's
  column) or `Overmap` (the band above the graph). Existing hosts are
  unchanged.
- `ArchiveEntry::session: Option<SessionId>` — the session an archive was
  saved from, when its owner says. `None` sits on the shelf.
- The view lays the graph out in the area below the band and sizes the band
  from the model, so the canvas, its hit targets and the layout agree.

mere-overmap `route::fill_archive` reads the codicil card's `Session` value
into `ArchiveEntry::session`. Djinn's archive projection adds that value: the
session id of each session in the codicil, read once per codicil (codicils are
immutable, so the reading is cached by id).

Knot sets `placement: Overmap` on its catalog model and repins Mere. Nothing
else in Knot changes: requests still split by owner as the contract says.

Pictograph is untouched. Lineage edges between containers (fork parent, the
codicil a thaw was opened from) are a follow-up: the route does not yet carry
lineage, and drawing them through the canvas would use Cambium's existing
node footprints (`GraphCanvasSwatch::with_node_footprint`), not a new API.

## Open for Mark

None blocks this slice. Options worth a ruling later:

1. Containers *around* the catalog instead of above it: the attached session
   frames the whole catalog graph. Rejected here for the reason above.
2. Archive chips as their own canvas nodes with "saved from" edges, instead of
   chips inside the container.
3. Lineage edges between session containers, as Turnstone's overmap draws
   them.

## Checks (2026-10-15)

Mere `7160353` (mere-view, mere-overmap, Djinn) and `392e0ecc0` (archive
chip controls stay on one line); Knot repins every Mere row from `19573b51`
to `392e0ecc0`.

- Mere: `cargo test --locked -p mere-view` (20 unit tests, two of them new
  for the overmap: containers, archive placement, requests, band wrapping),
  `-p mere-overmap` (codicil card parsing), `-p djinn --lib` (124, the Knot
  overlay round trip now asserts the saved codicil names its session),
  `-p pandect`: pass.
- Knot: `cargo check --workspace --locked --all-targets`;
  `cargo test --locked -p knot-editor --features mere-overlay`;
  `cargo test --locked -p knot-desktop --lib --bins --test scenario_lane
  --test font_roles --test typography_fonts` (331 lib tests, including the
  new placement assertion): pass.
- Native, macOS foreground: the receipt
  [2026-10-15_mere_overmap_live](../docs/receipts/2026-10-15_mere_overmap_live/README.md)
  holds before (Knot `a27911a`) and after runs on the same fixture: seed,
  fresh-process reopen and standalone all pass. The after captures show the
  band, the codicil on its session, the shelf, and the thaw named
  "Session (from blake3:…)".
- Knot also clears its workspace button style from the catalog's relation
  cells, as it already did for node targets: the fixture's linked documents
  showed each link as a grey bar.

## Addendum, 2026-10-15: the overmap as an in-graph tier

Mark ruled after the band landed: sessions and codicils become **nodes on the
graph canvas**, in an overmap tier one level up from the catalog, and each
node kind gets its own widget. His rulings on the follow-up options, the same
day:

- **Tier, not band.** `SessionPlacement::Tier` (Mere `ec5279784`,
  `89aa14a36`). The catalog's canvas keeps the whole graph area. A tier across
  its top holds a node per session, the shown one marked (emphasis, bold
  label, `aria-current`), and a node per codicil, joined to the session it was
  saved from by a dotted `saved-from` relation. The catalog is laid out below
  the tier.
- **A session node opens in place into its card**: Switch to, Fork, Trash or
  Restore, Save to archive, and its codicils as chips. The card is the node's
  footprint on the canvas (`GraphCanvasSwatch::with_node_footprint`), so the
  codicils' routes end at its border. One card is open at a time. The **tier
  grows** to hold it, and Knot lays the catalog out again below on every view
  event. Close or Escape closes it and returns focus to the node.
- **Codicils and archive chips use a hover-peek, click-pin popover**
  (Cambium's `detail_popover_panel` over `overlay_surface`): Open as session,
  Select for composition, Compose. Escape or an outside click dismisses it.
- **New session and Compose** are tier-level controls at the right end;
  Compose is in every archive popover too.
- **"+N"**: past three codicils on a session (or the shelf), they collapse
  into one "+N" node that opens the card, whose chips pin each popover.
- **Archive shelf**: codicils no live session holds hang off an "Archive"
  node by an `on-shelf` relation; it opens into a card with Compose.
- **Node kinds**: the paint leaf's palette takes `MereNodeKind`. Knot paints
  catalog documents by state as before, sessions in the palette's secondary,
  codicils, "+N" and the shelf in tertiary, and a trashed session dimmed.
  Cards and popover panels take the surface colours of Knot's other panels.
- **Naming** (Mere `a0ffffea2`): a fork names its parent, `Session (fork of
  Session)`, nesting for a fork of a fork; a fork of a thaw drops the doubled
  "from", `Session (fork of Session (from blake3:f612a))`; a picked-off part
  (Turnstone's tear-out) is `Session (picked from Evening reading)`. Branches,
  which merge back, wait on the merge-in design (Mere
  `design_docs/mere_docs/technical_architecture/2026-10-15_session_merge_in_note.md`).

The scenarios now open the shown session's card by its node
(`@aria-label=Session Session, shown here`) before Save, and open a codicil's
popover by its node (`@data-node-kind=archive`) before Open as session. The
step controls keep their aria-labels and `data-step` tokens. The reopen
scenario also forks the thaw and checks its name.
