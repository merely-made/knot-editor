# Local document recovery

Date: 2026-09-29
Status: Implementation, automated checks, and isolated native crash/restore
acceptance passed on 2026-09-29. Shared presentation follow-ups remain separate.

## Milestone

This is the local-recovery step between workspace slice 1 and commands, as
ordered by the [workspace plan](2026-09-05_knot_application_workspace_plan.md#scoped-next-sequence-multi-document-workspace)
and [design pass](2026-09-23_knot_design_pass.md#slice-order).

Recover eligible unsaved document text and exact selection after interruption
without ever automatically writing the original source file. Recovery is
private application state, not a catalog record, Workbench layout, causal
checkpoint, sync publication, or file autosave.

## Ownership and safety boundaries

- Storage owns versioned, checksummed, bounded per-item records and atomic
  replacement. Recovery identities are separate from runtime document keys.
- The standalone desktop owns eligibility, scheduling, the recovery affordance,
  fresh candidate sessions, and record cleanup after successful user decisions.
- Recovered file drafts start as unsaved candidates. Their original paths are
  provenance, not write authority. Save As uses the normal guarded write path.
- Sealed-vault and read-only projections are excluded from plaintext recovery.
  Optional embedding does not enable standalone recovery implicitly.
- Active-session records must not be collected or offered as abandoned drafts
  by another live window. Capacity exhaustion must be visible rather than
  silently evicting a live draft.
- Record cleanup must be ordered after pending writes so a late completion
  cannot resurrect a saved or discarded draft.
- Site metadata forms, upload composers, Micron form values, full undo history,
  workspace layout persistence, and commands slice 2 are outside this milestone.

## Acceptance checks

| Area | Required evidence |
| --- | --- |
| Store integrity | Version/checksum validation; bounded reads; atomic replacement; interrupted temporary write leaves previous record intact |
| Private local files | Restricted permissions where supported; no path traversal or symlink targets; serialized paths never used for recovery-file authority |
| Retention | Configurable bounded policy; live records protected; unknown/malformed records reported without silent replacement |
| Restart | Exact Unicode text and directional selection restored under a fresh runtime key; original target unchanged |
| Empty draft | Deleting all source text still restores as an unsaved candidate with close protection |
| Lifecycle | Save failure and Cancel preserve the record; successful Save or explicit Discard clears only the associated item |
| Multiple documents | One decision does not clear unrelated drafts; switching and duplicate Restore preserve the correct candidate |
| Concurrency | Other live sessions remain protected; stale records become eligible after lock release; delayed writes cannot recreate cleared records |
| Native interaction | Isolated test settings, real recovery controls, visible candidate/selection/provenance, nonblank capture |

Storage and lifecycle tests establish data semantics. Native captures establish
the user-facing affordance; one does not substitute for the other. Final test
commands, revision, artifact hashes, limitations and follow-ups will be appended
only after verification.

## Implemented policy and limitations

The standalone application opts into a `recovery` directory under its settings
root. Embedders do not opt in implicitly. Eligible editable file-backed and
ordinary scratch documents are copied by a bounded background writer after
650 ms of idle time, with a two-second flush deadline during continuous edits.
This is best-effort crash recovery, not a promise that the last keystroke is
durable. Successful writes synchronize file contents and atomically replace a
checksummed versioned record. Unix also synchronizes directory entries; the
portable non-Unix implementation does not claim that extra durability guarantee.

Default retention is 30 days, configurable to 7, 30, or 90 days in Appearance.
The store defaults to 20 records, 100 MiB aggregate, and 8 MiB per item. Active
sessions are protected rather than evicted to make room. Scan work is separately
bounded; incomplete scans and unreadable records produce visible issues and
prevent unsafe replacement. Unknown/newer records are preserved, not migrated
or silently discarded. Unix directories and files use 0700 and 0600 respectively;
non-Unix access control relies on inherited ACLs and has not been validated on
Windows in this receipt.

Restore claims an abandoned record under a live session lock and opens a fresh
unsaved candidate. The original path is comparison provenance only. Save As
uses the existing guarded document writer; failed Save and Cancel preserve the
recovery copy. Explicit Discard and successful Save remove only the matching
record through an ordered writer barrier. An empty recovered document still
requires a save/discard decision. Text and directional caret selection are
recovered, not undo history or layout.

## Automated verification and native acceptance

The candidate is based on `ff46cbe` plus the recovery working changes. Final
checks on 2026-09-29 passed:

- `cargo test -p knot-desktop --lib`: 192 passed, 0 failed, 1 existing ignored.
  This includes the store's 12 tests and runtime/integration custody tests.
- Desktop launcher tests: 7 passed; `scenario_lane`: 3 passed.
- `cargo clippy -p knot-desktop --lib -- -D warnings`: passed.
- `git diff --check`: passed.
- Desktop binary build: passed.
- `cargo test --workspace`: passed on the recovery worktree, including desktop,
  editor, catalog, readings, integration and doc-test targets. Existing unused
  import/variable warnings in `knot-editor` remain; no unrelated cleanup was made.
  The separately excluded `knot-document` package's own standalone unit suite
  was not run; its recovery constructor is exercised through desktop integration.

The store suite includes a subprocess exit that bypasses Drop, concurrent
exclusive claims with both sessions held alive, corrupted/newer/oversized
records, scan limits, private paths, symlink rejection, interrupted temporary
files, and startup/update retention. Desktop tests cover exact Unicode text
and directional selection, fresh runtime keys, originals remaining unchanged,
Save As cleanup, Compare and Reload, empty candidates, cancellation, failed
save, unrelated drafts surviving one discard, undo-to-clean cleanup, and
read-only/vault-style scratch exclusion.

The exact native candidate binary SHA-256 is
`db3ce10b81ab7f127b3671b1a61ff54768118eea9af1f3e24e3ef03d1c503af5`.
The prepared app is
`/Users/markik/Code/testing/knot-editor/recovery-native-20260929/KnotRecoveryTest.app`.
It uses isolated settings and a copied `original.djot`, whose pre-test SHA-256
is `e003cd459419854b629e3594173749d9fa23b7f97dac3bc3205cf4d2763a79d1`.
The initial native launch was blocked by the locked Mac. After the user
unlocked it, the parent launched and raised the isolated app through Computer
Use, edited the copied original, selected the final two bytes backward
(anchor 49, head 47, both downstream), and inspected its flushed record.
Only the verified test-app PID 46099 was terminated with SIGKILL.

Relaunch showed `Recovery · 1 copies` and the unchanged original. Restore opened
`Recovered original.djot` as a separate unsaved Scratch candidate. The newly
owned record retained the exact text and directional selection. Compare original
opened the Changes tile with the message that neither source was written.
Save As to the separate `recovered.djot` succeeded, switched the candidate to
Saved / File target, and returned Recovery to zero copies. Disk verification
confirmed exact equality with the pre-crash record's 49 source bytes, removal
of the matching recovery record, and the unchanged original SHA-256 above.
The recovered file SHA-256 is
`96049d931fd55706835ff19b4f38fa021a6c9d05804848585ac6cebd33f940bf`.
The clean test window was closed and its process exit verified.

The native clipboard paste timed out, direct typing omitted non-ASCII input,
and accessibility SetValue did not change the text. Therefore this native run
proves recovery of the text that actually reached the editor, not Unicode input
through Computer Use. Unicode exactness is established separately by desktop
integration tests. Nonblank whole-window screenshots were inspected in the
conversation after Restore and Save As; no durable PNG capture is claimed for
this manual run. `before-crash.json`, the original and recovered files, wrapper,
exact binary, and launch log remain in the isolated artifact directory.

## Parallel presentation follow-ups

The [slice 1 native review](2026-09-29_knot_workspace_native_review.md) records
three bounded follow-ups: Graph hit-target styling and unused session space,
focused-writing toolbar displacement, and cramped site footer controls. These
remain independently verified changes, not a typography or layout redesign.

Graph hit-target transparency and wrapping site-footer spacing are implemented
in `2d4c1bd` (integrated on main as `2f9a9fe`). The exact native candidate binary
SHA-256 was `7475e37b1086b9f018fb0ad0dea88545f70e972d86d43185be2c767530e034c8`.
The last-tab Graph scenario passed with one nonblank capture (digest
`6edaca38c426051a`); the two-sites scenario passed with two distinct nonblank
captures (`e60e236d88867dbe`, `b7e691ced4e7ae4b`). Whole-frame review confirmed
the complete graph label and footer spacing. Initial occluded failures are
preserved alongside the successful receipts; test-only foreground grace was
increased without changing the product binary or assertions.

The empty Graph Sessions column and focused-writing toolbar displacement are
not fixed by that commit. The shared Graph subtracts a fixed Sessions width
even for an empty model. Two Knot-only root-overflow experiments also failed
the toolbar regression: at a 420-pixel viewport the toolbar moved from y=56
to y=-91 after source focus. The failing experiments were reverted. Shared
component changes and a dependency repin require separate authorization and
verification.

Follow-up: that authorization and verification are now complete in the
[shared presentation receipt](2026-09-29_knot_shared_presentation_review.md).
Both shared fixes passed native captures as well as their automated regressions.

Local artifacts: `/Users/markik/Code/testing/knot-editor/recovery-visuals-20260929/knot-visual-followups-graph-footer/`.
This directory includes `REVIEW.md`, native receipts, PNGs, and the preserved
toolbar reproduction snippet and experiment log.
