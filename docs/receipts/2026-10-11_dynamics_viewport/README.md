# Dynamics preview layout, October 11, 2026

The preceding qualified source is `63aa695`. Its native review recorded motion
bodies partly or wholly below the visible Readings panel. This continuation
keeps the camera above a compact motion toolbar, with optional authoring and
explanation controls below it. Visible labels are Play, Pause, Keep placement,
Step once and Restart; the full existing accessible action names remain in ARIA.

A playback action queues one host-owned `scroll_into_view` request for the
preview/toolbar group. The existing Rootstock API resolves ancestor scrolling;
Knot does not calculate a second scroll or layout path. The session request is
consumed after dispatch. Scrolling away remains possible, freezes motion and
does not trigger repeated reveals or automatic resume. This flag enters neither
the portable recipe nor history/retention. Physics, source bytes, dependency
pins, occurrence roles and the explicit Keep policy retain their authority.

## Automated qualification

- [Before-layout regression](before-layout.log): the unchanged layout fails
  after Start at a 260-pixel panel width. Its camera rectangle is
  `(12, 862, 222, 202)` and its visible rectangle is absent. This is the
  deliberate negative control, not a failing final gate.
- [Final focused composition suite](composition.log): **80 passed, zero failed**,
  four test threads; 238 unrelated library tests are filtered out. The geometry
  test uses real desktop styles, bundled fonts and the normal host hooks at
  outer widths **256, 260 and 380**, height 600. The smallest size supplies
  232 content pixels after the sheet's horizontal padding. It requires the
  complete 220 by 200 camera plus its border and all three primary actions to
  be visible, then checks motion, pause, a consumed reveal and free scroll-away.
  Scrolling away must leave the camera hidden and freeze its ticks/pose; recipe
  and document source stay unchanged.
- [Native build](reveal-build.log): locked desktop and marked-public-fixture
  acceptance example built in the existing stable Knot target.

The earlier full-workspace result of 662 passing tests belongs to `63aa695`.
This layout continuation has its own focused and native receipts; it does not
claim a new whole-workspace or shared-supplier pass. Existing supplier baseline
golden failure remains recorded in the preceding review.

Intermediate gate logs are retained. The initial selector setup and oversized
view tuple had compile diagnostics, corrected before qualification. The first
reorder exposed the camera but left Keep below the viewport at the narrow size;
short labels and a primary toolbar row corrected that. The final geometry test
also requires the declared camera width, so a shrunk box cannot stand in for the
complete camera.

## Native qualification

Both final processes reopen the existing explicitly marked public fixture at
`C:\Users\mark_\Code\testing\knot-editor\relationship-dynamics\native-windows\kept-fixture`.
They select the original immutable recipe by the item ID from
[fixture-inspection.json](fixture-inspection.json), with producer source exactly
41 bytes, SHA-256
`8155c926a5f084c721cdeb968c909b45b74bb4b76f24666b38b9a51d008b0a08`.
Neither process retains an extra version or edits source.

| Process | Result | Captures | Actual pixels | Presented frames |
| --- | --- | ---: | --- | ---: |
| Wide fresh process | pass | 8 | 2200 × 1504 | 219 |
| Compact fresh process | pass | 8 | 840 × 1504 | 221 |

All **16 final captures** were visually reviewed. Each receipt says `RESULT ok`
and records zero unpresented redraws. Windows constrained the requested logical
height of 900; sizes above come from PNG headers and matching presentation data.
The final executable SHA-256 is
`9b9196b5141f60628760ce2435dc1fc2164a13b106141dd66b9e2ce6e370b487`.
PNGs, pre-launch binary identities and process logs remain under the named
`viewport-revealed-wide` and `viewport-revealed-compact` evidence directories.

[native-matrix.json](native-matrix.json) and the read-only
[verifier](verify_native.py) check paired presentation/product fields, PNG/source
hashes, advancing motion, frozen pause and exact Keep/Undo/Redo pose identity.
For Start, running, Pause, stable pause and Keep, the captured painted and
visible camera rectangles match, with dimensions **222 × 202 logical pixels**.
The geometry comes from the same host-owned layout as the displayed frame.

The first layout-only binary, SHA-256
`09ec8a702fbffac8b6c691bc7a6f2daa6f41c23d42cfc5517d061a233af08797`,
passed its state scenarios but visual review found its top camera edge clipped
by focus scrolling. Those captures and [layout-native-matrix.json](layout-native-matrix.json)
remain separate. [layout-source.patch](layout-source.patch) identifies that
candidate. The final reveal and geometry observation have their own executable
and native receipts. Both source phases are relative to `63aa695`; the complete
final source delta is [qualified-source.patch](qualified-source.patch).

The checked-in wide/compact scenarios are item-ID substitution templates.
Concrete scenarios used here are preserved beside their `.done` receipts.
Initial reopen and Undo/Redo may reveal other controls and leave part of the
camera out of view; the qualified automatic reveal belongs to playback actions.
The wide Windows menu still stacks, and the compact Readings tile overlays or
crops source content. This resolves the dynamics interaction defect, not all
desktop layout or private-custody acceptance.

Mark explicitly parked Firefox verification while browser control is blocked.
Independent physics coordinator review, coordinated adoption and F199's broader
T2/G4b2 queue remain separate. Fleece owns the Turnstone receiving-clip slice.
No consumer pin or shared force policy changed here.
