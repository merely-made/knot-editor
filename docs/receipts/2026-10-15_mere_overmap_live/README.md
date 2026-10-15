# Knot mere overmap, live — October 15, 2026

Status: the first native foreground run of Knot's mere overmap layer against a
live Djinn resident passed end to end on macOS. This receipt keeps that run's
evidence and the commands that reproduce it. It qualifies the layer's data
path and requests, not its final rendering: the layer still showed as
mere-view's side column (see "What the run found").

Knot `a27911a` (acceptance binary SHA-256
`5f5dc7e46e5b4b32183dbec0f34b832c760dbd2d445c9d53c6bd5e0b0a1afa62`). The
fixture resident is Mere's Djinn example `knot_overmap_live_host`, committed in
Mere `59b5bfeda` from the scratch copy that ran (Mere `90ce8f12d`, binary
SHA-256 `11ca9a98d136344f761dc3257200420f36a8baa980fd7acf6d3eb2646ce57b58`).
Only its header comment changed on commit.

## Fixture

`knot_overmap_live_host <endpoint> <new-root>` creates a new root marked
`PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET` (public test keys only, never an
installed vault or the default endpoint), seeds a Knot persona store, ensures
the `divination` mere with three sessions ("Session", attached, plus
"Evening reading" and "Draft spread"), serves the app broker at `<endpoint>`,
checks the route as `knot-editor` itself, writes `<root>/ready.json`, and stops
on a line on stdin.

## Commands

[`run_overmap_live.sh`](run_overmap_live.sh) is the whole procedure:

```sh
cargo build --locked -p knot-desktop --bin knot                       # Knot
cargo build --locked -p djinn --example knot_overmap_live_host       # Mere
TMPDIR=/tmp docs/receipts/2026-10-15_mere_overmap_live/run_overmap_live.sh \
  <knot> <knot_overmap_live_host> <knot-checkout> <mere-checkout> <new-out> \
  essay.djot field_notes.djot
```

It starts the fixture on a fresh `/tmp/kov-*.sock`, then runs three Knot
processes through Mere's `scripts/run_macos_scenario.py` (LaunchServices,
`--prefix KNOT`), each with `KNOT_TEST_INITIAL_PRESENTATION_GRACE_SECS=60`,
`TMPDIR=/tmp` and its own `APPDATA` profile:

1. [`overmap_live.scn`](../../../scenarios/overmap_live.scn) with
   `GRAPHSHELL_APP_ENDPOINT=<endpoint>` and
   `--catalog-root <root> --catalog <out>/live-catalog.redb
   --mere-root <root>/shared --mere-persona 00000000-0000-0000-6b6e-6f7420261015
   --mere-domain divination <root>/essay.djot <root>/field_notes.djot`;
2. [`overmap_live_reopen.scn`](../../../scenarios/overmap_live_reopen.scn), a
   fresh process with the same profile, catalog and arguments;
3. after the fixture stops,
   [`overmap_standalone.scn`](../../../scenarios/overmap_standalone.scn) with no
   `--mere-*` options, a new profile and catalog.

The run kept here was driven by hand in the same shape, with the catalog root
in a separate marked `docs/` folder and only `essay.djot` named. The reopen scenario then clicked the
codicil by its full accessible name (`Open Codicil blake3:f612a as session`);
a codicil's id includes its save time, so the committed scenario selects the
Open control by `@data-step=open-archive` instead.

## Results

| Process | Presentations | Captures | Result |
| --- | ---: | ---: | --- |
| Live seed | 24 | 3 | ok |
| Live fresh-process reopen | 15 | 2 | ok |
| Standalone, no resident | 10 | 1 | ok |

Receipts, process records and native logs are under [runs](runs); the
fixture's own log shows custody releasing Knot's epoch keys once per Knot
process. One earlier reopen attempt failed by selector (it named the button by
its visible text, which its aria-label overrides); it is kept as
[a failure](runs/live-native-reopen-selector-miss), not as evidence.

Full-frame captures, 2200×1400, under [native](native):

- [01](native/01-knot-overmap-djinn-sessions.png): Djinn's three sessions
  beside the catalog graph, the attached one "Shown here".
- [02](native/02-knot-overmap-archive-controls.png): Save to archive on each
  session, Compose on the empty archive.
- [03](native/03-knot-overmap-after-archive-save-codicil.png): after Save, the
  codicil `blake3:f612a` read back from the route with Open and Select.
- [04](native/04-knot-fresh-process-codicil-read-back.png): a fresh Knot
  process reads the same codicil back.
- [05](native/05-knot-after-open-codicil-as-session.png): after Open as
  session, a fourth session, now "Shown here" — and also named "Session",
  the same as its source (follow-up 1 below).
- [06](native/06-knot-standalone-no-djinn-no-layer.png): standalone, no
  resident: the catalog graph and no layer.

## What the run found

Three follow-ups, each ruled on and handled separately:

1. The opened session was named "Session", the same as its source. Fixed in
   Mere `aaf8798fc`: a thaw is named after its source and codicil, such as
   "Session (from blake3:f612a)".
2. The catalog root held `essay.djot` and `field_notes.djot`, but the graph
   showed only `essay.djot`. Not a bug: Knot's file catalog binds a document
   when Knot opens it and never scans its root, and the run opened only
   `essay.djot` (the catalog recorded one binding). The fixture now writes a
   second, linked document (Mere `efc8467ba`) and the receipt names both.
3. The layer rendered as mere-view's side column, not as an overmap. See
   [the rendering note](../../../design_docs/2026-10-15_knot_overmap_layer_rendering.md).

## The overmap layer, before and after

Both runs use the committed fixture at Mere `392e0ecc0` (binary SHA-256
`a38df088d955059f592b23c8ad77175846c04f7033e387d0d969d4c0cc40b0c9`), so the
catalog holds both documents and an opened codicil's session is named apart,
and the same script and scenarios:

```sh
TMPDIR=/tmp docs/receipts/2026-10-15_mere_overmap_live/run_overmap_live.sh \
  <knot> <knot_overmap_live_host> <knot-checkout> <mere-checkout> <new-out> \
  essay.djot field_notes.djot
```

- [before](before): Knot `a27911a` (`5f5dc7e4…`), the side column.
- [after](after): Knot with Mere `392e0ecc0`'s overmap placement
  (`e428327bd7af2beaa905ad4aa383f9aaa8bc83c36ac3776016b217c82eb98f75`):
  sessions as container nodes in a band above the catalog, the saved codicil
  on the session it came from, the Archive shelf with Compose, New session
  last, and the catalog's two documents and their link below.

| Run | Seed | Reopen | Standalone |
| --- | --- | --- | --- |
| before | ok, 19 presentations, 3 captures | ok, 17, 2 | ok, 10, 1 |
| after | ok, 19, 3 | ok, 16, 2 | ok, 10, 1 |

Each run directory keeps its PNGs (2200×1400), `scenario.done`,
`process.json`, native log and the fixture's log. The first after-run of
Knot `8fdc1f7` (not kept) showed two defects, both fixed before the run kept
here: "Open as session" wrapped inside its archive chip (Mere `392e0ecc0`),
and Knot's workspace button style drew each catalog relation cell as a grey
bar (Knot now clears it, as it already did for node targets; the before
captures still show the bar).

Three earlier attempts on this machine stalled at zero presented frames while
a system dialog held the foreground; they are not evidence. Every process the
script started exited; none remained afterwards.

## The overmap as an in-graph tier, before and after (October 15, later)

Mark ruled the overmap into the catalog's own canvas (see the rendering note's
addendum). Same script, same fixture source, macOS foreground, `TMPDIR=/tmp`;
the fixture host was built at Mere `2222917f3` (SHA-256
`be08873765f8b7ad0f01d454076a255c27193e410919bfd03842a9757a920b80`) for both.

- [tier-before](tier-before): Knot `601f99b` (`a530e326…`), the band of
  framed cards above the catalog.
- [tier-after](tier-after): Knot on Mere `2222917f3` (`f8da7223…`): sessions
  and the saved codicil as nodes in a tier on the catalog's canvas, the shown
  session marked; the session's card opened in place, growing the tier with
  the catalog laid out again below; the codicil's popover with Open as
  session, Select and Compose; New session and Compose as tier controls; after
  Open, the thaw "Session (from blake3:…)" shown here, and after its card's
  Fork, a fifth session named "(fork of Session (from blake3:…))".

| Run | Seed | Reopen | Standalone |
| --- | --- | --- | --- |
| tier-before | ok, 3 captures | ok, 2 | ok, 1 |
| tier-after | ok, 3 | ok, 3 | ok, 1 |

The first after-run (Knot on Mere `86effe47f`, not kept) failed its new fork
step: Open as session left the codicil's popover pinned, so the click on the
new session only dismissed it. Mere `2222917f3` closes the popover on Open,
stops a node with an open card from also drawing its label past the card's
edge, and stops the hidden side column rendering duplicate controls. Earlier
attempts while the screen was locked presented no frames and are not
evidence. Every process the script started exited.

## Re-capture on Knot b19eebf (October 15, later still)

Knot `b19eebf` on Mere `0c03dff3d` (P5 and merge-in, with Mere `dfbf6f97c`'s
centred button text and middle-cut labels); binary SHA-256 `77cb75e4…`,
fixture host built at Mere `0c03dff3d` (`0f73d9dc…`). Same script, macOS
foreground, `TMPDIR=/tmp`. [tier-after-b19eebf](tier-after-b19eebf) holds:

- `overmap-live-card-fork-branch.png` (new): the shown session's card offers
  Branch beside Fork, with Trash and Save to archive; button text is centred.
- the existing seed, reopen and standalone captures; after the thaw's Fork the
  fork's label is cut in the middle ("Session (fork of … blake3:…))"), so the
  reopen scenario now asserts `Session (fork of`.

Every step and assertion of all three scenarios passed in this run (seed 3
captures, reopen 3, standalone 1). The seed process is marked `RESULT fail`
only because the presentation watchdog tripped after its last step, as it
closed (25 frames presented, then 10 s without one); reopen and standalone, in
the same run, are `RESULT ok`. Around it the lane was unstable: other attempts
stalled at zero presented frames with the screen unlocked, and are not kept.

A pending-merge card was not captured: it shows only for a merge with
conflicts, and the fixture resident cannot make a conflicting edit on a branch
(it seeds three sessions and an empty archive, and edits nothing).
