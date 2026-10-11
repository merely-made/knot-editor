# Explicit placement from motion, October 11, 2026

Mark chose **Preview; keep placement explicitly**. The production consumer
review at `9790938` remains the preceding qualified source. This continuation
adds **Keep placement** to that runtime without changing dependency pins or
shared force policy.

Keep placement requires a paused or finished preview. It copies the displayed
normalized pose into the existing source-bound placement trace, then discards
the controller and its ticks. The host records the complete candidate as one
recipe-history edit: one Undo restores all earlier placement; Redo restores the
kept pose. Retain still deliberately saves an immutable signed/encrypted version.
Document source, copied disclosure, roles and the canonical dynamics slot are
unchanged.

The captured pose must cover exactly the copied occurrences and use finite
normalized coordinates. Hidden background occurrences remain part of this
complete scene pose. Individual pointer edits keep their visibility check.
The existing 256-step trace bound applies atomically; each changed occurrence
uses an ordinary translation step, while host undo records one whole edit.
An exhausted trace refuses the complete capture and leaves material unchanged.
Repeated capture of an identical authored pose creates no new edit.

## Automated checks

- [Portable recipe tests](portable-windows.log): **21 passed, zero failed**;
  includes exact pose roundtrip, preserved roles/source/disclosure, hidden
  occurrences, no-op recapture, and atomic missing/foreign/nonfinite/capacity
  refusal.
- [Focused dynamics](focused-windows.log): **14 passed, zero failed**, including
  one-edit undo/redo and refusal to keep running or absent motion. The final
  workspace run also covers the added finished-controller case.
- [Native build](build-windows.log): locked application and public-fixture
  acceptance example built; [final identity build](identity-build-windows.log)
  includes the explicit immutable-version selector and marked-fixture inspection.
- [Full workspace](workspace-windows.log): locked, all targets, no fail-fast,
  `-j 2`, four test threads; **662 passed, zero failed, six platform/existing ignored across 44
  harnesses**. This final run includes the version selector and fixture inspection.
  The earlier complete Keep-runtime run is preserved in
  [keep-workspace-windows.log](keep-workspace-windows.log), with the same counts.

The first focused compile had a test-initialization typo, corrected without a
runtime change; its diagnostic is preserved in `initial-focused-windows.log`.
The initial workspace build exhausted available memory through default parallel
compilation. Only this task's Cargo job was interrupted, and its log is retained
as `interrupted-workspace-windows.log`. The completed retry uses two build jobs
and the same stable `C:\t\cargo-targets\knot-editor` target. Existing supplier
golden failure and warnings remain as recorded in the preceding consumer review.

The first complete final-source run with default test concurrency recorded
**661 passed, one failed, six ignored**. Its unchanged
`continuous_edits_flush_before_typing_stops` recovery test found the final
`edit 26`, violating its expectation of an earlier autosave. That timer-based
test passed in isolation with one thread; no recovery source was changed.
[parallel-workspace-windows.log](parallel-workspace-windows.log) and
[recovery-isolated-windows.log](recovery-isolated-windows.log) preserve both
results. The final complete repeat uses four test threads with every test still
enabled; it is not a filtered replacement for the workspace gate.

## Native evidence

The five passing native processes use the marked public fixture at
`C:\Users\mark_\Code\testing\knot-editor\relationship-dynamics\native-windows\kept-fixture`.
The seed and wide/compact reopen scenarios repeat the prior motion assertions
on this runtime before the explicit keep/undo/redo/retain and fresh-reopen pair.
Actual pixels, binary/PNG/source hashes and presentation fields are recorded
in the [motion matrix](motion-native-matrix.json) and
[Keep matrix](native-matrix.json). PNGs, pre-launch binary identities and process
logs remain in that evidence directory. All **21 captures** were visually reviewed.

| Process | Captures | Actual pixels | Presented frames | Binary phase |
| --- | ---: | --- | ---: | --- |
| Wide seed | 7 | 2200 × 1504 | 292 | Keep runtime |
| Wide fresh reopen | 2 | 2200 × 1504 | 176 | Keep runtime |
| Compact fresh reopen | 6 | 840 × 1504 | 215 | Keep runtime |
| Keep / Undo / Redo / Retain | 5 | 2200 × 1504 | 242 | Keep runtime |
| Exact kept-version fresh reopen | 1 | 2200 × 1504 | 170 | Identity selection |

The requested logical height of 900 was constrained by the Windows display;
pixel sizes above come from PNG headers and matching presentation metadata.
Each passing receipt explicitly says `RESULT ok`, with zero unpresented redraws.
The unchanged document remains 41 bytes, SHA-256
`8155c926a5f084c721cdeb968c909b45b74bb4b76f24666b38b9a51d008b0a08`.

Two executable phases are explicit. The first four processes use SHA-256
`05fc2881b13423ce631229c5bd8a77faa8274c2ecbef5840d92a89d304b2f8aa`,
with the source delta from `9790938` recorded in [keep-runtime.patch](keep-runtime.patch).
The final reopen uses SHA-256
`c4c28a202909108716c9b81d102e3e636a9c0e8697cfd218803e5f3931d74a6b`,
after adding only the retained item's `data-item-id` and the public-fixture
inspection mode; the complete final delta is [qualified-source.patch](qualified-source.patch).
The physics and Keep action are unchanged between these phases. Verifiers read
each process's pre-launch binary identity rather than relabeling earlier runs
with whichever executable occupies the target path now.

The earlier `keep-placement` run exercised Keep, Undo, Redo and retention but
failed on a newly added `source_bytes` scenario assertion. That field exists in
the capture product projection, not the scenario snapshot interface. The final
scenario corrects that instrumentation; the verifier checks the field in every
capture and hashes the actual source file. The failed receipt is preserved.
Process exit alone is not a pass; each final receipt must say `RESULT ok`.

The first generic fresh-reopen selection opened the older immutable recipe,
which shares the new version's button caption. Its failing receipt is preserved
as [initial-version-reopen.done](initial-version-reopen.done). The kept version
was present; the final scenario selects its exact item ID, without changing the
application's choice or deleting either version. The read-only
`collection_acceptance --inspect-test-fixture <absolute-marked-fixture>` command
reports recipe IDs and placement counts, refusing an unmarked fixture.
[The unmarked inspection control](unmarked-inspection-refusal.log) exits 1 before
opening any owner or launching a window.
[fixture-inspection.json](fixture-inspection.json) supplies the sole kept version
for [exact-version-reopen.scn](exact-version-reopen.scn); the checked-in
`relationship_keep_placement_reopen.scn` remains a substitution template.

The read-only [Keep verifier](verify-native.py) compares the paused runtime
digest with the kept authored digest, then compares undo, redo, retained and
freshly reopened state. It also checks exact source bytes, preserved slot/roles,
zero retained runtime, PNG metadata and capture/presentation pairing.

Visual limits remain explicit. The wide Windows menu stacks vertically, and the
Readings panel can overlay or crop source content. Its long controls push the
scene partly or wholly below the visible panel, especially at compact width and
in paused/kept captures. Exact-version reopen shows both kept bodies and their
edge after selection reveals that portion of the panel. These are passing
interaction/state receipts with known layout defects, not full visual acceptance.

Firefox remains blocked by the Windows Computer Use helper's missing URL-policy
support. Independent physics review and coordinator-owned adoption are still
open; F199's broader T2/G4b2 pause remains in force. This is a bounded Knot action,
not a general scene adapter, private-custody native acceptance or visual release
qualification.
