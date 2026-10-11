# Shared live-dynamics host qualification — October 11, 2026

Status: implemented and qualified for this bounded macOS slice. Publication is
a separate Git operation, not inferred from these test and native receipts.

The runtime source is `552640f3a9afd6774705cc30762098bd67eef5a0`, based on
published Knot `19b99042d6c6708854f64e6187429da839a1c82c`. The shared adapter is
published Mere `3a8f38130ba846eb25717c46bd05d12af18bdce8`.

## Scope and ownership

Pictograph's `ProjectionDynamics::new_relationship` runs the shared Seiche
controller with exact compiled occurrence mappings, disclosed relationship
endpoint pairs, and occurrence-specific Seeded/Anchored/Pinned roles. Repeated
source references are not deduplicated. Disposable simulation topology creates
no kernel assertions or acquisition authority. The old relation-free constructor
keeps its refusal behavior. This shared commit preserves Mere's preceding
coordinator-owned adoption of the qualified Knot `19b9904` milestone.

Knot supplies explicit spring, charge, and bounded spring choices. The bounded
choice authors an ordinary shared Schedule with `Stop::Frames(120)`, not a
private force, threshold, or clock. Start/Resume, Pause, Step, and Restart control
only this session. A completed controller keeps its final pose; Step is a no-op
until an explicit Start/Restart reseeds it.

Authored slots, occurrence-role overrides, and placement traces are retained as
immutable signed/encrypted versions in Knot's own mere. Nonempty roles require
presentation v4; empty maps preserve older serialization. Poses, ticks, and
controller handles never enter retention or undo. Reopen starts at authored
positions with no runtime. Source rebind refuses incompatible copied facts or
anchors rather than reassigning retained role/placement identity.

Static visibility and foreground/background categories affect paint, not physics
participation. All compiled occurrences and disclosed edges participate. Motion
uses the original fixed plane, with out-of-plane display coordinates clamped to
its boundary. Active playback updates the paint leaf; completion, pause, or
refusal reconciles semantic hit targets once. Placement/selection and role/recipe
authoring are guarded during playback.

Hidden/unmounted/ancestor-clipped scenes and Theme Workshop pause without
automatic resume. Native eligibility uses the preceding presentation result,
observed in AfterFrame; a failed presentation freezes the next attempt. This is
not a wall-clock simulator or a guarantee that a failed current acquisition can
retroactively undo the tick already prepared for that attempt.

## Refreshed dependency family

| Provider | Frozen revision |
| --- | --- |
| Mere | `3a8f38130ba846eb25717c46bd05d12af18bdce8` |
| Genet | `4bc75583865c2fb862b7acd7fe16dc28eb591042` |
| Dramatis | `0234e7eea6f2ddf2330a71b9425da01b06ae0391` |
| Netrender | `ef5412d760a978584c8c1801e8a765c51aecd403` |
| Maintained Vello | `5862bba26c8a9f56f6a5fdc8ea6966b360a5d63a` |

All Mere rows, including the standalone document manifest, advance together.
Classic/encoding remain 0.11.0; wgpu is 30.0.1. Sparse is not enabled. The
transport patch tag remains `mere-p2panda-net-0.7.5-upstream-20261011`.
The standalone ignored lock was regenerated locally, not tracked for publication.
See [Cargo source identities](dependency-identity.json).

The second refresh observed Mere main `8699bf5f33f442b72711e54cb2d6a419de345344`:
three subsequent commits only change semantic-core documentation and recovered
object QUIC transport/exploration. None changes this projection/dynamics/host
family. Published Genet remains the frozen revision above; its unrelated local
HEAD is not used as published-current evidence.

## Automated gates

Evidence logs are copied here; original diagnostic logs remain under
`/Users/markik/Code/testing`, prefixed `knot-live-dynamics`.

- [Workspace all-target tests](workspace-test.log): 43 harnesses, **662 passed,
  zero failed, four existing ignored**. This includes source-role identity,
  historical bytes, atomic refusals, 256-occurrence deterministic playback,
  hidden-scene no-auto-resume, completed Step stability, the shared 120-frame
  schedule, and actual signed/encrypted retention/reopen.
  Source was `a24fb3e6b81b6f5903df7cee99ae6d403f0c7693`; the sole subsequent
  runtime change at `552640f` is the running-status copy. Final all-target check,
  build, and the complete native matrix use that final source.
- [Final all-target check](check.log): locked desktop check passed.
- [Standalone default](document-default.log): 45 passed, zero failed, one
  existing ignored. [Engine/highlight](document-engine-highlight.log): 61 passed,
  zero failed, one existing ignored, using the regenerated ignored local lock.
- [Final build](build.log): locked Knot binary and collection acceptance example
  built. Acceptance SHA-256:
  `50723e55be65ad89fcbe8f1b3bbea6b8dccd552e0a0c57597f924dc1cb0ec35d`.

The [shared adapter's five focused tests](shared-focused.log) pass using
`cargo test -p pictograph --lib --no-default-features --features canvas
projection_dynamics --locked` at the published adapter source. Its
[wider canvas library](shared-canvas.log) has
**401 passed, one failed, 18 ignored**: the arrangement golden at topic radial
weighted focus 0 observes `1455265069374486353`, expected
`12098838834426755199`. Both the exact untouched `8f61cfe52` baseline and the
rebased adapter reproduce those values; see the
[untouched baseline golden check](shared-baseline-golden.log). No golden was updated; this is not a
claim of an all-green supplier suite.

## Native matrix

Only marked disposable fixtures with public deterministic keys were used;
this is not personal-wallet acceptance. Woodshed released the foreground lane
before these runs. All final processes use the same frozen acceptance binary.
Full-frame PNGs and raw logs remain at
`/Users/markik/Code/testing/knot-editor/live-dynamics-20261011.5GjIih`.

| Final process | Logical viewport | Presented frames | Reviewed captures |
| --- | --- | ---: | ---: |
| `qualified-wide-seed` | 1100×900 | 292 | 7 |
| `qualified-wide-reopen` | 1100×900 | 176 | 2 |
| `qualified-compact-reopen` | 420×900 | 214 | 6 |

All three processes pass, and all 15 final captures were inspected at full frame.
They show rendered source and scene, reachable session controls, wrapped compact
authoring controls, and restored authored state. Running captures have changed
pose digests and increased ticks. Paused captures preserve exact pose digests
and tick counts across additional frames. Revealing a hidden scene does not
resume it. Fresh processes restore one role, the shared slot, one authored
position, and two trace steps, while runtime positions and ticks remain zero.

Across all captures the authored-position BLAKE3 stays
`48dea041f148dba84eeaafabccf2151101cfa34a258c0573198939f34832fd00`.
The document remains the exact instrument-generated 41 bytes, SHA-256
`8155c926a5f084c721cdeb968c909b45b74bb4b76f24666b38b9a51d008b0a08`.
See [verified native matrix](native-matrix.json). Reproduce its read-only checks
with `node verify-native.cjs /absolute/path/to/live-dynamics-evidence`.

## Preserved failures and limits

- Early compiler failures, fixture setup errors, and the mistaken twelve-tick
  expectation are preserved in the original logs. None was fixed by weakening
  source/retention assertions or changing shared physics thresholds.
- `wide-seed-raised` exposed a consumer lifecycle bug: the pre-frame hook reads
  a cleared current presentation receipt, producing ticks 0 despite a rendered
  scene. The fix records the result in AfterFrame and composes that observer
  with Mesquite. It does not change the renderer or supplier hook order.
- `wide-seed-final` and `wide-seed-charge-raised` advance once and legitimately
  finish under the shared velocity-rest heuristic. The two-word pinned fixture
  does not guarantee sustained unbounded playback. The bounded shared schedule
  explicitly provides duration for Pause/Resume acceptance.
- `wide-seed`, `wide-seed-charge`, and `wide-seed-scheduled` fail with zero
  presentations and the existing 10-second Occluded deadline. Exact temporary
  windows were subsequently bound and raised through their exposed native Raise
  action. The final runs pass without deadline or assertion changes. Reads after
  a successful Raise can report that the scenario has already quit; process
  receipts establish completion. Unattended background presentation is not
  qualified by this procedure.
- Named channels, groups, nested/nonflat scenes, raw unsupported terms, and
  unsupported body forms remain explicit binding refusals. No full grammar
  editor, ambient source expansion, implicit pose baking, or inference-driven
  scene is claimed. Other-platform, assistive-technology/IME, and personal
  custody acceptance remain open. Compact qualification is reopen/playback and
  authoring-control reachability, not a separate compact save workflow.
- This shared publication and Knot adoption do not certify Woodshed,
  Cleromancy, Graphshell, or other consumers. Coordinator-owned Mere Knot pins
  and Djinn knot-site pins are unchanged.

Canonical scope:
[composition retention plan](../../../design_docs/2026-09-30_composition_mere_retention.md#shared-live-dynamics-host-continuation-2026-10-11).
