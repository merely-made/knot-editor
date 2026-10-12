# Dynamics completion and refusal feedback, October 11, 2026

The preceding qualified source is `156626f`. Its controls labeled every stopped
controller "paused," including a finished schedule, and left Step available
after completion. Play could also resume a controller after a runtime refusal.
This continuation uses the existing `ProjectionDynamics::end` result to report
pause, rest, completion and refusal separately. The authored 120-frame spring
schedule finishes; reaching that schedule's end does not assert physical rest.

Step is semantically disabled and its callback guarded after completion or
refusal. A completed pose remains available for explicit Keep placement. Play
resumes an ordinary pause, but rebinds a completed or refused run from authored
positions. A failed tick preserves the last validated displayed pose and tick
count until an explicit action. The refusal's actual adapter diagnostic remains
visible beside the status. A polite status region and the capture observer's
`scene_motion_status` report this session state; neither enters serialization,
recipe history or retention.

Shared physics, source/disclosure authority, occurrence roles, placement policy
and dependency pins remain unchanged: Mere `3a8f3813`, Genet `4bc75583` and
Dramatis `0234e7ee`. This does not adopt upstream's newer inspection work or
reopen F199's T2/G4b2 queue.

## Automated qualification

- [Before-status regression](before-status.log): the previous runtime fails the
  real host test after a three-tick authored schedule, displaying "Shared
  dynamics paused · fixed ticks 3." This is the negative control.
- [Final focused composition suite](composition.log): **84 passed, zero failed,
  zero ignored**, with four test threads; 238 unrelated library tests filtered.
  The four added tests exercise a finished schedule through real controls,
  all-pinned bodies reaching the shared rest result, loss of a required mapping
  after one valid tick, and an unsupported named channel refused through the UI.
  They check disabled Step/Pause, explicit replay or repair, exact last-valid
  pose preservation and unchanged authored recipe/source. The existing geometry,
  scroll-away, pause, retained-material and Keep/history tests also pass.
- [Native build](build.log): locked `knot` binary and `collection_acceptance`
  example built using the reusable `C:/t/cargo-targets/knot-editor` target, two
  build jobs. No isolated target or Cargo home was created.

The earlier 662-passing workspace receipt belongs to `63aa695`. These focused
and native results are not a new whole-workspace or supplier qualification.
Existing supplier baseline golden failure remains in the preceding review.

Test-setup diagnostics are preserved: the first regression setup used a private
helper and the wrong attribute-name type; the first broader run omitted the
fixture's presentation-version refresh and expected a channel name where the
adapter reports its refusing path. Both were corrected before the final gate.
The first build named the package rather than its `knot` binary target.

## Native qualification

Both final processes reopen the existing explicitly marked public fixture at
`C:\Users\mark_\Code\testing\knot-editor\relationship-dynamics\native-windows\kept-fixture`.
The [read-only inspection](fixture-inspection.json) identifies the original
immutable recipe, with one authored position and two placement steps. Producer
source stays exactly 41 bytes, SHA-256
`8155c926a5f084c721cdeb968c909b45b74bb4b76f24666b38b9a51d008b0a08`.
Neither process edits source or retains another version.

| Process | Result | Reviewed captures | Actual pixels | Presented frames |
| --- | --- | ---: | --- | ---: |
| Wide fresh process | pass | 9 | 2200 × 1504 | 500 |
| Compact fresh process | pass | 9 | 840 × 1504 | 500 |

The executable SHA-256 is
`a9326c8e3d74b86cca1887e2ddea77d70aa5a18149704fb610e8a2f76864d8db`.
Its source is archived in [qualified-source.patch](qualified-source.patch).
[Native matrix](native-matrix.json), [wide receipt](status-qualified-wide.done)
and [compact receipt](status-qualified-compact.done) bind screenshots to
presented frames and product facts. Both say `RESULT ok`, with zero unpresented
redraws. The requested logical height of 900 was constrained by Windows; actual
sizes above come from PNG headers and matching presentation data.

The [read-only verifier](verify_native.py) checks completion at exactly 120 ticks,
a frozen final pose, explicit replay with a new low tick count, and identical
final poses across both runs within each process. Keep captures that precise
completed pose; one Undo restores original placement and Redo restores Keep.
Roles and the slot persist, runtime clears, and source remains unchanged. The
full 222 by 202 logical-pixel camera box, including its border, remains visible
on playback/completion/Keep captures. All 18 final images were reviewed; the
finish label is visible at both sizes. Disabled-control semantics and refusal
are checked by the host tests, rather than inferred from screenshot colors.

Native commands run on subsequent presented frames. The automated host test
checks the immediate zero-tick reset; native assertions and captures permit
the few ticks already executed between dispatch and observation. The initial
[harness receipt](initial-harness.done) incorrectly used compact menu navigation
on a wide window and expected tick zero on that later frame. Its nine diagnostic
captures remain under `native-windows/status-wide`; it is not a passing gate.
A rejected `<` operator was changed to the harness's supported `<= 119` before
the final runs; [parser diagnostic](scenario-parser-diagnostic.log) is retained.
No production source changed during those scenario corrections.

The wide/compact scenario templates live under `scenarios/dynamics_status_*`;
substitute the original item ID from the marked fixture inspection. PNGs and
launch logs remain in `native-windows/status-qualified-wide` and
`native-windows/status-qualified-compact`. Undo/Redo may focus other controls
and scroll the camera partly offscreen; the preceding one-shot reveal contract
applies to playback actions. The existing wide Windows menu layout and compact
Readings overlay remain outside this slice.

## Remaining gates

Mark suggested retrying Firefox in a private window. The existing private
window was identified, but Computer Use again stopped inspection because
browser URL policy enforcement is unsupported for the current Windows browser.
There is no new Firefox receipt. Verification remains parked while blocked;
the user's private window was not navigated or closed.

Original physics-coordinator review and coherent consumer adoption remain
separate. This receipt qualifies the bounded Knot consumer feedback change.
