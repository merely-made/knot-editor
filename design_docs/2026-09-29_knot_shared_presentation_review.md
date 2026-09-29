# Shared Graph sizing and scenario focus

Date: 2026-09-29
Status: Automated checks and both native acceptance captures passed.

## Scope and revisions

Knot's presentation follow-up consumes Mere
`ac1a5e5dc14daf61bc7c0a70e3151bb9a5107471`, a bounded change on its existing
`8fce5365d9aca943582c83e8fd4a22eb05ec021c` base. Genet remains at `34626a6c`.
This does not upgrade Knot to unrelated Mere main changes. The independent
recovery implementation and earlier graph hit-target/footer fixes are retained.

## Fixes and boundaries

The shared Graph reserves Sessions space only when there are sessions or a
meaningful New session action. With neither, the graph takes the full available
width (wide layout) or body height (narrow layout). Both canvas sizing and cached
node layout follow this choice. Populated and mintable session views retain
their existing layout; a Knot-only CSS hide would not have corrected canvas
dimensions.

Mesquite's scenario click path previously requested full-element reveal whenever
a target was clipped. Knot's source textbox measured 578 pixels high, with 206
pixels already visible at a 1100 x 420 viewport. Revealing the whole editor
scrolled the command toolbar from y=56 to y=-91. The scenario itself still
reported success because focus landed, so Knot now separately asserts that the
toolbar stays stationary.

The shared fix clicks the visible part of a tall textbox when it provides at
least a 44 x 44 pixel hit area. The classification follows the actual matched
node, not whether the selector used role or class. Small visible slivers,
ordinary non-text controls, offscreen controls, and unrevealable targets keep
the existing reveal/refusal behavior. This is a scenario-input correction,
not a change to Knot's layout, ordinary tile scrolling, or root overflow.

## Automated evidence

- Before the dependency change, the new Knot toolbar assertion failed with
  actual y=-91 versus expected y=56, despite `RESULT ok (14 frames, 0 captures)`.
- Against the new pin, all four Knot scenario-lane tests pass, including that
  toolbar regression and all five step-9 scenario fixtures.
- `cargo test --workspace --locked` and
  `cargo clippy --locked -p knot-desktop --lib -- -D warnings` passed. Desktop
  library: 192 passed, 1 existing ignored; launcher: 7 passed. Existing unused
  import/variable and unused-patch warnings remain unchanged.
- Mere Graph library: 16 passed; host routing: 1 passed. Coverage includes empty
  wide/narrow views, the New session action, and empty/populated/empty cache
  invalidation.
- Shared scenario host: 18 passed; click-on-scroll: 3 passed; scroll requests:
  10 passed; Mesquite library: 17 passed; host library: 15 passed. Tall editor
  coverage verifies clipping, stationary chrome, real focus and typing for
  role and class selectors. Small/large clipped and offscreen buttons retain
  reveal behavior.

## Native artifact

The exact candidate binary SHA-256 is
`06c5716e35c3620ba66b0739b28b1908ee39f98efef98cc857f8d92637e6bedc`.
The isolated app, fixtures, wrapper and per-lane receipts are under
`/Users/markik/Code/testing/knot-editor/shared-presentation-20260929/`.
Scenario copies add test-only foreground grace before the existing product
assertions. Focused writing passed with one 2200 x 1400 nonblank capture,
digest `ce514f88751b4c52`, PNG SHA-256
`f4e97b61654b96033a788a786095b3786b1f2a19e9f8cad8b0a729b9100e8d96`.
Whole-frame review confirmed the source caret, intact command toolbar and status
bar, with no dirty edit. Receipt: `RESULT ok`, 6013 total scenario frames,
1 capture, 0 blank frames. Foreground grace is test setup, not a performance claim.

The first Graph launch was blocked by the locked Mac. After the user resumed,
the last-tab Graph scenario passed: `RESULT ok`, 6009 total scenario frames,
1 capture, 0 blank frames, digest `869feddd336a659a`. Its 2200 x 1400 PNG SHA-256
is `07c57074ffbf3baecb8f7f4e4b65a75c849122eb1b3d199d9f303f2aff1c161f`.
Whole-frame review confirmed the empty Sessions column is absent, the graph
fills the available width, the `field_notes` label is unobscured, and command
and status chrome remain intact. The original source tab is closed as asserted.

The lockfile diff was checked to be exactly the Mere commit substitution;
no package version or dependency list drift was introduced. The earlier Mere
main checkout is untouched; these fixes live on `codex/knot-shared-presentation`
so the pin can remain a bounded update rather than an unrelated main upgrade.
The branch was published to Mere's origin and its remote ref verified at the
exact pinned commit above. Knot's own integration remains a local commit until
separately pushed. The native test process exited after writing its receipt.
