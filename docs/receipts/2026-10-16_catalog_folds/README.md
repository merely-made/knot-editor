# Knot catalog fold acceptance — 2026-10-16

The current standalone desktop's catalog Swatch supports selection folds,
summary activation to expand, Expand all folds, and bounded session undo/redo.
Knot owns the selection and preferences. Native `FoldRecord`s persist under the
canonical catalog-root key in its ordinary local preferences, independent of
source files and the resident mere's session/archive layer. The shared Mere
view lays out the full source graph, then projects folds into the same Swatch
used by painting and native targets. Unfolding preserves member placement.

The checkout was fetched and advanced from `b19eebf` to current main `921b4e0`
before implementation. All 62 Mere source rows move coherently from `ee324e5`
to the tested shared addition `42c158bae`; other repository pins are unchanged.
The tested supplier requires source-link fields on synthetic rhyme and copied
recipe geometry, and its catalog has 13 arrangement families. Knot carries the
existing `b6a6222` rhyme adapter fix, initializes the recipe scene explicitly,
and derives picker labels from the shared catalog rather than a stale list.
Mere gained Mosaic and territory-rendering commits while this work was being
qualified. They are preserved in the supplier integration; Knot remains locked
to the exact `42c158bae` source tested by the receipts below.

## Headed result

Two separate LaunchServices processes used the same frozen binary and real
`run_desktop_with_targets` path over five catalog-bound Djot files and six
extracted links. Both Mesquite scenarios passed:

| Run | Presented frames | Captures | Result |
| --- | ---: | ---: | --- |
| Select, create two folds, undo and redo | 37 | 5 | pass |
| Fresh-process restore, summary activation, undo/redo, expand all | 25 | 3 | pass |

All eight captures are 2400 by 1700 and nonblank. Inspected frames show the
expanded source graph, both summaries, the restored summaries and the expanded
members. Every paired capture has the same source-reading fingerprint and the
same full member-position fingerprint. The saved two-fold record digest equals
the fresh process's restored digest. Five directed boundary source cells remain
represented as three endpoint/provenance bundles. The catalog retains five
source nodes and six source links, no read errors, one open document, and no
catalog failure notice. All five source files remain byte-equivalent.

The backend opens a table-initialization write transaction when opening Redb,
so database-file bytes change across reopen. The source-reading fingerprints
and source-file bytes are the truth checks; storage metadata is not classified
as graph truth. Preferences deliberately change as the second run expands and
saves all folds. See [verification](verification.json) and
[paired capture facts](capture-facts.json).

## Reproduction and source binding

Build `cargo build --locked -p knot-desktop --bin knot --example catalog_fold_acceptance`.
Launch the example through Mere's `scripts/run_macos_scenario.py`, prefix `KNOT`,
with `--initialize <absolute empty fixture directory>` and
[`catalog_folds.scn`](../../../scenarios/catalog_folds.scn). After it exits,
launch the same binary with `--reopen <same fixture directory>` and
[`catalog_folds_reopen.scn`](../../../scenarios/catalog_folds_reopen.scn).
The example creates a bound `source/` directory and keeps the database and
preferences outside it. It never opens a personal vault or retention target.

Use a new capture directory per run, `KNOT_CAPTURE_CORRELATION=1`, a 1200 by 850
logical window, isolated APPDATA/XDG/MERE_ROOT paths, and a 120-second process
bound. The marked fixture permits at most 60 seconds for its first presentation;
Knot restores its normal ten-second presentation timeout after the first frame.
Exact commands and environment are in [seed config](seed-config.json) and
[reopen config](reopen-config.json). [Inputs](inputs.json) bind the source,
lockfile, scenarios, launcher and frozen binary. [Checks](checks.json) record
validation and limits. Both owned native processes have exited.

This is a production catalog consumer proof on macOS. It does not qualify
Windows/Linux, renderer recovery, live screen-reader use or global fold keyboard
shortcuts. Descendant folding waits for an explicitly named source hierarchy;
ordinary extracted document links are not treated as parenthood.

## Validation

The final locked root-workspace suite passed 698 tests with four existing
ignores. Shared `mere-view` passed all 38 tests. New/focused-module rustfmt,
diff whitespace checks and Mere's port/web dependency boundaries pass. The
root suite covers all six workspace members; the excluded standalone
`knot-document` and `knot-site` workspaces were compiled as dependencies here.
The full workspace log and the three new persistence/selection tests plus the
owner-feedback regression remain in this receipt. The initial fixture refusal
and the superseded 11-family failure are recorded in `checks.json`; neither is
counted as final passing evidence.
