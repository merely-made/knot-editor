# Forme workspace receiving qualification — 2026-10-29

Knot adopts Platen's `workspace-editor` session for explicit host-local layout
drafts. View and the command palette expose Unlock, first-pane more/less space,
Apply and Discard. Knot owns open documents, source edits, focus, tile bindings
and the preferences file. The shared session owns the draft and acknowledges a
commit only after the host's preferences write succeeds. Failed writes retain
the draft; changed tile bindings or an externally changed saved slot refuse.

This receipt qualifies that bounded Knot host flow on macOS. It preserves the
writing-workspace documentation commit `ba7a7f4` and the concurrent publication
admission work `d400d0c`, integrated through `bd48817`. Relative to that integrated
baseline, the lockfile adds only `platen` and `uxtree`, plus the desktop dependency
edges. Both use the existing Mere `b232631f3e5ef30d24b19b3e832ddf225177c381`
source. The incoming publication work's supplier updates are preserved.

## Review and source boundaries

Review found a focus mismatch: restoring a saved active tab could show document
A while Edit, Save and Close still targeted focused document B. Unlock now
activates the current focused document through the shared draft before exposing
the restored geometry. A two-document regression verifies the same visible and
focused target before and after Apply.

The adapter derives presentation member IDs from host-owned document/reading
bindings instead of runtime tile allocation. A matching fresh process may
allocate different tile numbers. A different document, changed pin, changed open
set or new scratch instance cannot inherit the earlier binding. The source is
never part of the Forme record.

A file-backed Unicode editing regression performs a real source replacement
during a layout draft, changes selection, discards layout, and compares the exact
document snapshot. Ordinary document Undo restores the original source,
selection and clean state. The file's bytes remain unchanged. Additional cases
cover failed persistence/retry, changed saved-slot refusal and matching fresh
host restoration. Source review approved the resulting ownership and
acknowledged-save paths.

## Automated gates

All commands use the committed lockfile and offline resolution:

| Gate | Result |
| --- | --- |
| `cargo test --locked --offline -p knot-desktop --lib` | 372 passed, 0 failed, 1 existing ignored timing probe |
| `cargo test --locked --offline -p knot-desktop --test workspace_forme` | 2 passed, 0 failed |
| `cargo check --locked --offline -p knot-desktop --all-targets` | passed |
| `cargo build --locked --offline -p knot-desktop --bin knot` | passed |

The ignored case is `workspace::tests::outline_long_document_probe`, an explicit
diagnostic timing receipt. The library total includes eight Forme model cases
and one Forme command-routing case; those are not extra tests added to the total.
These are desktop gates, not a full repository or excluded-workspace test claim.

The two menu harness tests use actual semantic menu actions. One checks raw
rendered divider movement and Discard. The other uses production `host_hooks`,
bundled fonts and default appearance: it saves a canonical 0.45 share at 3000px,
resizes to 320px, checks real responsive collapse and a visible source editor,
then returns to 3000px. The rendered divider recovers within one pixel; source,
canonical layout and saved slot remain identical throughout.

Exact commands, process exits and source hashes are retained in
[checks.json](checks.json), [source inputs](source-inputs.json),
[lock scope](lock-scope.json) and the [check logs](checks/desktop-lib.log).
The native [verification record](verification.json) identifies the executed
binary by SHA-256 and the source revision. The key native gate is repeated from
the committed implementation; a later receipt-only commit does not change that
runtime source.

## Native save, reopen and compact presentation

The native runner creates a copied public field-notes fixture and isolated
APPDATA/XDG/MERE_ROOT directories. It requires both a zero process exit and
`RESULT ok`; process exit alone is insufficient. Capture correlation records
the canonical Forme facts alongside each presented frame.

| Separate process | Logical viewport | Presented frames | Captures | Result |
| --- | --- | ---: | ---: | --- |
| Seed: draft 0.55, Discard 0.50, Apply 0.45 | 1100 × 800 | 40 | 3 | pass |
| Fresh reopen: Unlock saved 0.45, Discard live 0.50 | 1100 × 800 | 14 | 1 | pass |
| Default-appearance compact reopen | 420 × 800 | 14 | 1 | pass |

The first pair uses 12px source text, narrow measure and follow-hard-wrap off so
small authored proportions are visible. The compact run uses default 16px source
text, medium measure and follow-hard-wrap on. This distinguishes canonical
authorship from the responsive presentation that protects the writer's measure.

The seed explicitly pins Preview. Contextual navigation restores that same
pinned reading in the fresh process; it does not open a duplicate Preview.
Unlock loads saved geometry only for the matching document/reading bindings.
The separate process then discards its layout draft. It neither reapplies nor
rewrites the saved slot. The runner verifies the exact saved-slot digest before
and after both reopen checks and preserves the fixture's SHA-256
`55750ca64366fa50f7ea77d60c731b27ac6948a5b752cb4796fddf8dd99df34c`.

The five captures are reviewed separately from the model assertions:

- [Draft](native/seed/forme-draft.png): the source/Preview divider moves right.
- [Discarded](native/seed/forme-discarded.png): the divider returns to the middle.
- [Applied](native/seed/forme-applied.png): the divider moves left.
- [Fresh reopen](native/reopen/forme-reopened.png): the divider matches Applied,
  with the pinned Preview and document content retained.
- [Default compact](native/default-compact/forme-reopened.png): readable source,
  an accessible Menu and the collapsed Readings rail remain visible; the saved
  authored ratio is not forced onto the narrow viewport.

The wide PNGs are 2200 × 1600 pixels and the compact PNG is 840 × 1600. Source and
Preview remain legible and nonblank. Existing Preview link-button wrapping is
rough, and the compact status row clips the long Unlock feedback horizontally.
Those visual residuals are recorded; neither is used as proof of pointer input
or full compact-chrome acceptance. The production-hook test supplies the exact
rendered-rectangle recovery check, rather than inferring pixel geometry solely
from the scenario's canonical `forme_first_share` field.

## Reproduction and preserved failures

After the locked build above, choose a new output directory:

```sh
python3 docs/receipts/2026-10-29_forme_workspace/run_native.py \
  --binary target/debug/knot \
  --output /Users/markik/Code/testing/knot-editor/forme-new-run
```

The runner refuses an existing output directory. It executes
[`workspace_forme.scn`](../../../scenarios/workspace_forme.scn) and
[`workspace_forme_reopen.scn`](../../../scenarios/workspace_forme_reopen.scn),
records real exits and receipts, verifies binary/source stability and compares
the saved slot. It does not mutate a personal profile or vault.

Earlier failed attempts remain preserved under
`/Users/markik/Code/validation/knot-forme-20261029/`; their diagnoses and locations
are recorded in [prior attempts](prior-attempts.json). They include an unavailable
pointer producer, duplicated/context-restored reading bindings, runtime tile
allocation drift and follow-focus versus pinned Preview. They are not counted
as passing native gates. The generic process-exit-zero / `RESULT fail` case is
why the final runner checks both outcomes.

## Qualification limits

Frisket's receiving-cohort separator has no pointer handler; this slice uses
explicit menu/palette resize commands. Pointer dragging, new splits, tab drags,
floats, layout-undo controls, automatic source-session reopening, other desktop
platforms and human assistive-technology review remain separate work. Saved
layout loading requires an explicit Unlock with matching bindings. The compact
→ wide transition is exercised by production-hook receiving tests; the native
wide and compact receipts are separate processes. Publication is a separate
action; this work is committed locally without a push.
