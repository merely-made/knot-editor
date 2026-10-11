# Authored occurrence scene qualification — October 11, 2026

Status: implemented and qualified for this bounded macOS slice. Automated,
build, rendered native, and fresh-process reopening gates passed. Publication
is a separate Git operation, not inferred from this receipt.

Runtime source: `a5d817777297ad62c5b65ebcdac77ba034bbbdf2`, based on published
Knot `4d8b81c5ed8fbe29bac9fc762cb71f161e00c784`. Focused capture-only scenarios
were committed separately as `509f1bc`; they do not change the runtime binary.

## Scope

Knot extends its retained relationship scene with shared `SceneTrace`
occurrence translations. Drag previews are session-only; release commits one
host-owned history step. Keyboard controls use the same validated path. Explicit
Retain creates an immutable signed/encrypted collection version in Knot's own
mere. Copied disclosure, exact source anchors, and refusal boundaries remain
authoritative. Historical presentation v1/v2 bytes are unchanged when no trace
exists; authored positions require v3.

At viewport widths up to 640px, the existing full-name occurrence controls form
a wrapped list. The graph canvas keeps its actual authored/compiler positions.
This is not a universal drawer or canvas-label collision repair.

## Frozen dependency family

| Repository/provider | Revision |
| --- | --- |
| Mere | `0e78fcffb140395a5c0d2f64a4dfda2b81751681` |
| Genet | `4bc75583865c2fb862b7acd7fe16dc28eb591042` |
| Dramatis | `0234e7eea6f2ddf2330a71b9425da01b06ae0391` |
| Netrender | `ef5412d760a978584c8c1801e8a765c51aecd403` |
| Maintained Vello | `5862bba26c8a9f56f6a5fdc8ea6966b360a5d63a` |

Classic/encoding/shaders resolve to 0.11.0; wgpu is 30.0.1. Classic remains the
shipping renderer. Sparse is not enabled. The transport patch tag is
`mere-p2panda-net-0.7.5-upstream-20261011`. Metadata confirms one Cargo source
identity per shared package. The ignored standalone document lock was
regenerated locally; it is not a tracked distribution artifact.

## Automated evidence

Evidence root: `/Users/markik/Code/testing/knot-editor/authored-scene-20261011.w0Cma1`.

- `cargo-check.log`: `TMPDIR=/tmp cargo check -p knot-desktop --all-targets --locked`
  passed.
- `workspace-test-final2.log`: `TMPDIR=/tmp cargo test --workspace --all-targets --locked`
  passed: 43 harnesses, 647 passed, zero failed, four existing ignored tests.
  This includes the 420px compact full-name/bounds/non-overlap regression,
  retained trace validation/refusals, real host pointer capture and drag release,
  keyboard undo/redo, and encrypted-store reopening.
- `workspace-test.log`: earlier complete pass, 646 passed, zero failed, four
  ignored, before the compact regression and final review corrections.
- `workspace-test-final.log`: preserved failed compilation of the newly added
  compact regression's recursive text helper. The borrowed-string/owned-string
  error was corrected before the complete final2 pass; no test assertion was
  removed or weakened.
- `knot-document-default.log`: standalone `cargo test --locked` passed, 45
  passed, zero failed, one existing ignored test.
- `knot-document-engine-highlight.log`: standalone
  `cargo test --locked --features engine,highlight` passed, 61 passed, zero
  failed, one existing ignored test. These commands use the separate document
  manifest and its regenerated local ignored lock.
- `embed-bert-wgpu-check.log`: locked optional inference-feature `cargo check`
  passed. This is dependency/type qualification, not model-download, inference,
  GPU-runtime, or inference-driven scene acceptance.
- `desktop-build.log`: locked `cargo build -p knot-desktop --bin knot --example
  collection_acceptance` passed. The acceptance binary's SHA-256 is
  `511b96d40bfb37222cf16a3f7aa9f53bb4e615e7a70ce33904881b5fd4255033`.

## Native matrix

Woodshed released the macOS foreground lane before Knot's native execution.
Acceptance uses only marked disposable fixtures with public deterministic test
keys, never a personal wallet. The committed retained-scene scenarios require
one authored position and two trace steps, undo/redo, category/camera persistence,
retention, fresh-process reopening, and unchanged source text.

All four save/reopen processes passed. Logical dimensions are below; captures
are 2× native pixels (2200×1800 or 840×1800).

| Evidence directory | Logical size | Presented frames | Reviewed captures |
| --- | --- | ---: | ---: |
| `wide-seed-foreground` | 1100×900 | 286 | 6 |
| `wide-reopen` | 1100×900 | 221 | 6 |
| `compact-seed` | 420×900 | 290 | 6 |
| `compact-reopen` | 420×900 | 223 | 6 |
| `wide-controls` | 1100×900 | 177 | 2 |
| `compact-controls` | 420×900 | 179 | 2 |

All 28 full-frame captures were inspected: nonblank rendered surfaces, scene
positions/strata and restored categories, retained source disclosure, complete
directional controls, and uncropped compact occurrence rows. Shared canvas label
culling remains enabled. The first placement captures scroll above the button
bank, so the two focused controls replays explicitly capture all four directional
buttons, make two unsaved moves, then undo back to the retained state. They do
not retain a new version or alter the fixture source.

Every authored save/reopen capture reports one authored position and two trace
steps with exact position BLAKE3
`85859bb97d0d638a40da41da176ee56c50ef96ac41493df819da6fd96edd5824`.
Both focused replays restore that same fingerprint after undo. The final source
files equal the instrument's expected 41 bytes, SHA-256
`8155c926a5f084c721cdeb968c909b45b74bb4b76f24666b38b9a51d008b0a08`.
See [native matrix hashes](native-matrix.json).

The initial `wide-seed` launch failed before presenting any frames: 1385
unpresented redraws, macOS `Occluded`, and the shared 10-second presentation
deadline. Its receipt is preserved. Subsequent runs bind the exact temporary
app through native computer use and invoke its exposed window `Raise` action.
They pass without assertion changes or renderer deadline changes. Post-raise
accessibility reads sometimes report that the app/window has already quit;
successful process receipts and captures establish normal scenario completion.
This does not qualify unattended background presentation or fix an upstream
occlusion issue.

## Remaining limits

- No live dynamics binding in Knot's compact `GraphCanvasSwatch`, despite
  upstream G4b1 support. No private solver or inferred ambient neighborhood.
- Foreground/background attention, authored positions, motion pins/anchors,
  and Forme layout locks remain distinct.
- A narrow drawer at a wide viewport, personal-custody native retention,
  assistive technology/IME, and other-platform native acceptance remain open.
- The trace is bounded to 256 translation steps and the existing one-MiB
  material limit. Source rebind with an existing trace requires explicit reset.

Canonical implementation/boundaries:
[composition retention plan](../../../design_docs/2026-09-30_composition_mere_retention.md#authored-occurrence-placement-continuation-2026-10-11).
