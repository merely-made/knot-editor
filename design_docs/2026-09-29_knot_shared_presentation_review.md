# Shared Graph sizing and scenario focus

Date: 2026-09-29
Status: Prior presentation pins passed automated and native acceptance checks.
The diagnostics adoption below has separate Windows automated evidence.

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

## Shared diagnostics adoption

The follow-up pins Mere `ca2351b3fa21de1ca40a8553eed20fc503faf15c`
and Genet `19c206873ab08ae227217892d9e74d0df18b349a` in the root workspace,
desktop manifest and excluded document workspace. Mere retains the Graph sizing
and tall-textbox behavior above while adding bounded Apparatus observations and
Mesquite diagnostic attachments. Taproot remains the semantic selection seam;
Mesquite owns scenario execution and receipts. Product diagnostics remain
product-owned; this adoption adds no Knot diagnostic producer or Inspector UI.

The locked root graph has one source each for Mere, Genet and Netrender, with
no caller-local path overrides. Package/version comparison with the previous
lock, after normalizing the intentional source changes, adds only
`mere-apparatus` 0.0.1 and `genet-text` 0.1.0, and advances `netrender-vello`
from 0.10.0 to the version required by the new graph, 0.10.1.

On Windows with Rust 1.97.1 at Knot base `8a454fb7`,
`cargo test --workspace --locked --offline -j2` passed 425 tests with two
existing ignores. All four scenario-lane tests passed,
including the stationary-toolbar regression and all five step-9 fixtures.
The in-memory resident route and real editor network tests also passed.
The excluded document workspace also passed 34 tests with its existing manual
outline-performance ignore, including Windows atomic replacement checks.
The concurrent Nomadnet merge at `701d0f0b` is preserved. Strict desktop clippy
initially rejected a Unix-only mutable binding in the recovery options helper
and the backend-free Micron config's redundant struct update. The narrow cfg
adjustments retain Unix file permissions and Retinue's default map limits.
`cargo clippy --locked --offline -p knot-desktop --lib -j2 -- -D warnings`
then passed.
The integrated default desktop gate passed 210 tests with the existing timing
probe ignore, including all four scenario-lane tests again.
`cargo check --locked --offline -p knot-desktop --features retinue -j2`
also passed, qualifying the preserved optional backend alongside the default.
The native captures above qualify their recorded older pins; they were not
repeated for this dependency adoption. Automated receipts are under
`C:/Users/mark_/Code/testing/knot-editor/apparatus-adoption/`.
