# Relationship dynamics consumer review, October 11, 2026

Status: the published production consumer is reviewed and qualified for this
bounded Windows slice. This receipt supplements the preceding
[macOS qualification](../2026-10-11_live_dynamics/README.md).

The fixture-only gap was superseded by Knot's published `195758b` consumer and
final runtime `552640f3a9afd6774705cc30762098bd67eef5a0`. This pass adopts that
implementation. Its reviewed tree is `f7c4a035b300b6901eaf7e44c145b19e1ff5326c`,
whose tree matches published `21e1b0f6f739ef023fc71152fb1f8ce80354a3e2`, plus
the new consumer test and an appearance-test navigation repair in this commit.
Runtime and dependency pins are unchanged.
The separate selected-clip export work is preserved.

## Consumer proof and automated gates

The added `real_sound_consumer_preserves_occurrences_and_disclosed_edges_change_motion`
test uses an actual Mora sound disclosure and the production runtime. It proves
that repeated occurrences of one source remain distinct bodies with separate
Pinned and Seeded roles; disclosed edges change motion; deterministic replay
agrees; and simulation cannot rewrite the copied material. Its negative control
removes only compiled relationships at the binding seam, preserving Knot's
requirement for explained source relationships.

- [Focused tests](focused-windows.log): `cargo test --locked -p knot-desktop
  --lib dynamics`, **13 passed, zero failed**.
- [Native build](build-windows.log): `cargo build --locked -p knot-desktop
  --bin knot --example collection_acceptance`, passed.
- [Full workspace](workspace-windows.log): `cargo test --workspace --all-targets
  --locked --no-fail-fast`, **44 harnesses, 659 passed, zero failed, six
  platform/existing ignored**. Nothing is filtered. Counts belong to Windows;
  the earlier macOS cohort's totals are not substituted.
- [Dependency identity](dependency-identity.json): one Mere source at
  `3a8f38130ba846eb25717c46bd05d12af18bdce8`, one Genet source at
  `4bc75583865c2fb862b7acd7fe16dc28eb591042`, one Dramatis source at
  `0234e7eea6f2ddf2330a71b9425da01b06ae0391`, and wgpu 30.0.1.

All commands reuse `C:\t\cargo-targets\knot-editor`. Known warnings about the
unused transport patch, Win32 `ReplaceFileW` unsafe code and the existing
recovery-test unused variable are preserved in the logs. Supplier-wide testing
is not repeated here: the preceding receipt records five focused adapter tests
passing and the reproduced untouched topic-radial golden failure in the wider
canvas suite (401 passed, one failed, 18 ignored). No golden is changed.

The [first workspace run](initial-workspace-windows.log) stopped at 410 passing
tests, ten failing appearance tests and three existing ignored. The theme suite
assumed a plain-row **Appearance** button; Windows' actual client frame exposes
**View > Appearance**. The test helper now opens that real menu when the quick
button is absent, preserving the platform default, all theme assertions and the
production command path. No application behavior is changed. Its focused rerun
is preserved in [appearance navigation](appearance-navigation-windows.log).

## Native Windows and fresh-process reopen

The frozen native executable is
`C:\t\cargo-targets\knot-editor\debug\examples\collection_acceptance.exe`,
SHA-256 `3f5e311fb035e3b76940e5c6dbc613218bd8b9a09ff04c223237d9e9bcdc83e2`.
The acceptance example uses its explicitly marked public test fixture, not a
personal wallet. Its source remains exactly 41 bytes, SHA-256
`8155c926a5f084c721cdeb968c909b45b74bb4b76f24666b38b9a51d008b0a08`.

| Process | Scenario | Result | Captures | Actual pixels | Presented frames |
| --- | --- | --- | ---: | --- | ---: |
| Wide seed | `relationship_live_dynamics_windows.scn` | pass | 7 | 2200 × 1504 | 292 |
| Wide fresh reopen | `relationship_live_dynamics_reopen.scn` | pass | 2 | 2200 × 1504 | 176 |
| Compact fresh reopen | `relationship_live_dynamics_compact_reopen.scn` | pass | 6 | 840 × 1504 | 215 |

The display constrained the requested logical heights of 900; the actual pixel
sizes above are measured from PNG headers and matching presentation metadata.
All 15 full captures were visually reviewed. Each final receipt explicitly says
`RESULT ok`, with zero blank captures and zero unpresented redraws. Process exit
alone is insufficient: the acceptance host also exits normally after a failed
scenario.

Start-to-running ticks advance from 2 to 19, with a changed motion digest.
Pause and a subsequent 12-frame wait both retain tick 19 and the exact pose
digest. Hide/reveal remains paused; retain preserves its tick and pose without
serializing runtime. Fresh processes recover one authored position, two trace
steps, one motion role and the shared slot, with zero runtime positions and
ticks. Authored-position identity agrees across all three processes.

The [native matrix](native-matrix.json) records each PNG hash, presentation
sequence and product fields. Raw scenario receipts are committed alongside it.
Original PNGs, process logs and the marked fixture are retained under
`C:\Users\mark_\Code\testing\knot-editor\relationship-dynamics\native-windows`.
The read-only [verifier](verify-native.py) accepts that root and the executable
as two absolute-path arguments; it validates pairing, source bytes, PNG sizes,
authored-state identity, advancement, pause stability and discarded runtime.

Two failed navigation runs are preserved as `wide-seed.done` and
`wide-windows.done`. The original macOS seed expected **Show Preview**, while
the Windows app frame initially says **Preview**. The first adaptation then
used an absent close class. The final Windows scenario changes only the first
caption to **Preview**, preserving **Hide Preview** and every motion/retention
assertion. It runs from a fresh marked fixture. No physics assertion is relaxed.

Visual residuals remain: the wide Windows app header stacks menu labels, the
compact readings panel overlays part of the source, and scenario scrolling can
partly clip a scene in reopen/reveal captures. The motion pair itself is visible
in both viewport sizes; authoring and playback controls are reachable. This is
bounded functionality qualification, not complete visual or accessibility
acceptance.

## Ownership and open gates

The controller receives exact compiled occurrence mappings and disclosed edge
endpoints. Foreground/background roles and visibility filters affect paint;
they do not silently decide body participation. Running poses stay in the
session. Source, retained authored placement, roles and recipes retain their
separate authority.

Knot uses compiled card footprints in a fixed plane, then clamps its normalized
glyph display to that plane. This is not a physical wall or a guarantee of glyph
collision behavior. One fixed tick per eligible host frame is not a wall-clock
simulation. Its explicit shared 120-frame schedule is independent of F202's
editable 60-step comparison default. Unsupported named channels, groups and
scene/body forms remain refusals.

Firefox remains unqualified: the Windows Computer Use helper stopped because
it cannot enforce URL policy for that browser. Browser automation stopped, with
no workaround. The original physics coordinator's independent review and
coordinator-owned adoption pins remain open. F199's T2/G4b2 pause stands. This
receipt does not grant other consumers, private-custody retention or inferred
ambient neighborhoods acceptance.

An earlier duplicate consumer draft was superseded and removed from the shared
checkout after being archived at
`C:\Users\mark_\Code\testing\knot-editor\relationship-dynamics\superseded-consumer-draft.zip`.
Its optional **Keep placement** action is not included in this reviewed runtime.
Mark's subsequent explicit answer, **Preview; keep placement explicitly**, now
authorizes a separate continuation over this qualified source.
