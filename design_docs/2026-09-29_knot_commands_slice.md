# Commands slice

Date: 2026-09-29
Status: Implemented; automated and native macOS palette acceptance passed.
Ready for integration into Knot main.
Base: Knot `8a454fb`, published to main before this slice began.

The candidate subsequently merged remote main `701d0f0` (NomadNet backend
decoupling) at `8e55ddd`; that independent work is preserved. The merged workspace
test run passed. While native acceptance was underway, remote main advanced to
`c92ad04` with coherent diagnostics pins and cfg-scoped Clippy fixes. Integration
must preserve that update as well as Commands, combining the shared revisions
before repinning; choosing either existing Mere pin alone would drop work.

## Scope

Implement slice 2 from the [design pass](2026-09-23_knot_design_pass.md#slice-order):
one command catalog, the palette, a shared Cambium menu bar, and Windows/Linux
client-drawn title-bar integration. Commands retain ids, shortcuts, disabled
reasons and children across their surfaces. Existing document and site action
methods remain the authority for writes, close/reload decisions and publication.

macOS retains the OS title bar and a plain command row through slices 2–5.
Native macOS menus/title-bar integration remain slice 6; no interim drawn
macOS menu is introduced. Typography, automatic rail/chip collapse, fonts and
the later document-relation workflow are not part of this change.

## Implementation lanes

- Knot: typed command identity, catalog, guarded dispatch, shortcuts, palette,
  existing toolbar projection and platform-specific chrome integration.
- Cambium: shared menu bar and keyboard semantics, compact Menu overflow,
  full command-tree activation paths, disabled-item handling and required
  host key vocabulary/mapping.
- Review: independent command/authority inventory, platform seam audit,
  integration review, final pin, receipts and native acceptance.

Existing free worktrees were reused; checkout names do not imply their current
scope. Mere's command work starts from the bounded `ac1a5e5dc` pin rather than
upgrading unrelated dependencies.

## Acceptance plan

| Area | Required checks |
| --- | --- |
| Catalog | Stable unique ids; displayed shortcuts match dispatch; one catalog feeds all surfaces; unavailable actions carry a reason |
| Dispatch | Recheck current availability at activation; stale menu state never bypasses source custody or current document/site ownership |
| Palette | Cmd/Ctrl+Shift+P opens; explicit query field filters; arrows and Enter activate the intended id; Escape and dismissal restore focus; input never edits the source accidentally |
| Editing | Ordinary text, selection, undo/redo and clipboard shortcuts remain routed to the focused text field, including path, form and palette fields |
| Lifecycle | Empty workspace, read-only sources, dirty documents, restored candidates, canceled/failed writes, multiple documents and multiple sites retain their existing guards |
| Menu bar | ARIA roles, roving top-level focus, arrow/Home/End navigation, submenu entry/exit, F10 or Alt entry where applicable, compact overflow, disabled items and reasons |
| Frame | Windows/Linux drag region with no-drag controls, title, short toolbar, caption verbs through the existing host queue, dirty-close protection |
| macOS | Existing native title bar and plain command row remain; palette gets a headed keyboard/control receipt |
| Regression | Existing workspace, recovery and five step-9 scenarios remain green; shared click/scroll tests stay intact |

Headless tests can force the client-frame projection on this Mac and verify
layout, semantics and queued window verbs. They do not prove Windows Snap,
window-manager drag/resize, Linux compositor behavior or platform native
accessibility. Those runtime claims require corresponding platform receipts.

Tests, exact revisions, capture hashes, limitations and any remaining acceptance
gates will be recorded after verification rather than inferred from compilation.

## Integration notes

The shared menu bar uses the existing `CommandItem` and `CommandEvent` contract,
including original-tree activation paths. It renders its own ARIA hierarchy:
the existing selection bar and command-menu wrappers do not provide menubar
roles, nested path prefixes or top-level roving focus. This is an implementation
adjustment to the design pass, not a second command catalog.

The first shared candidate passed all 238 Cambium library tests with default
features disabled, including seven menu-bar tests, and all five cambium-winit
library tests. Integration also exercises the separate rootstock/native-host
key vocabulary; a runner-level key conversion alone is not sufficient proof of
the desktop F10 route.

Strict shared Clippy was attempted. It is not a green gate: the bounded base has
existing lint errors in Meristem and Cambium (type complexity, implied bounds,
and other untouched code). No unrelated shared lint cleanup is included here.
Knot's own strict library lint and integration results are recorded separately
when available.

Integration review added explicit checks for:

- Read-only Compare versus write operations, scratch Save availability, and
  closing a different document while retention is busy.
- File-backed recovery candidates retaining Compare/Reload against their
  original, without treating their scratch write posture as a missing origin.
- Site-specific page/metadata command ids and resolving event paths against
  the rendered catalog before rechecking the current authority.
- Re-entering a menu after focus leaves, menu Edit commands returning to the
  document, palette Tab/query synchronization, and path-popover cancellation.
- Caption controls remaining within a 540px viewport. A first test reproduced
  the Close button at x=588.5; compact mode now leaves reading commands in the
  menu and uses short caption controls with unchanged accessible names.

The read-only Save As UI intentionally changed from an enabled action that
later refused to a disabled command with a reason. The lower-level Save As
refusal test remains, and verifies that no destination file is written. Scratch
Save similarly explains that Save As is required; these are availability
projections, not new write authority.

## Native diagnostic progress

Unlocking the desktop allowed real macOS keyboard acceptance on the candidate
and NomadNet-merged binary. Cmd+Shift+P, query filtering across Tab, Enter to
toggle Appearance, and Escape back to source were exercised. Typing a temporary
`FOCUSCHECK` marker after Escape made the source dirty; Cmd+Z restored Saved.
No save was performed, and the copied fixture SHA-256 remained unchanged.

The first durable palette capture failed: 2413 scenario frames, one capture,
one blank frame, digest `a5543af33cd56725`. A baseline-plus-palette diagnostic
also failed at 2414 frames: the baseline was visible, but opening the palette
produced the same entirely black digest. Failed receipts are retained in
`receipts/palette-failed-2413` and `receipts/palette-failed-2414` under the local
artifact directory below. These observations isolate the palette-open scene;
they do not establish that window occlusion caused the failure.

Removing both palette overflow clips yielded two visible captures, but whole-
frame inspection rejected that variant because command rows spilled outside
the dialog. Keeping only a bounded inner command-list scroll region passed
the same 2414-frame scenario with two captures, zero blank frames, and palette
digest `f9821fe248ecdb76`. Its binary SHA-256 was
`f1003afcbf7f0f6cf247cff7787703d650f612b23425383b7f3109ab141a10c2`.
The outer dialog remains overflow-visible; only the list clips and scrolls.

Subsequent interaction testing found that keyboard selection could move below
the visible rows without revealing them. A wheel action addressed to the list
also moved the entire workspace. The focused repair now reveals the active row only when its query/identity
changes, retaining wheel scrolling across unrelated events. Transform-free
centering keeps painted and hit-test geometry aligned. A 640x340 regression
covers full-row reveal, dialog bounds and stationary workspace chrome. Native
retesting confirmed keyboard reveal and wheel scrolling at the visible list
center. The earlier AX-addressed wheel action is not treated as proof of ordinary
wheel routing. Native AX initially exposed anonymous rows; the combined shared
pin now exposes their names and disabled reasons. Full native accessibility
remains unqualified. A source-selection paint spill over Appearance controls was also
observed separately; its cause and whether it predates this slice are unverified.

## Verification receipt

Production source revision: Knot `216be40` (the following receipt update is
documentation-only). It contains remote main `c92ad04`, including NomadNet
backend decoupling and shared diagnostics adoption.

Final shared pin: `8cb9b3095f71be31ae3097209d7b53d13b947beb`, published and
independently verified on Mere branch `codex/knot-command-menu`. Its parents
are Commands `ba3ecfda5` and diagnostics `ca2351b3`; all Mere manifest and
lockfile references use the combined pin, alongside Genet
`19c206873ab08ae227217892d9e74d0df18b349a`. The Mesquite merge retains both
semantic matching/rechecks and the earlier partially visible tall-textbox
focus behavior. Mere main was not changed.

Final commands all exited successfully:

```sh
CARGO_TARGET_DIR=/Users/markik/Code/repos/knot-editor/target cargo test --locked --workspace
CARGO_TARGET_DIR=/Users/markik/Code/repos/knot-editor/target cargo clippy --locked -p knot-desktop --lib -- -D warnings
CARGO_TARGET_DIR=/Users/markik/Code/repos/knot-editor/target cargo build --locked -p knot-desktop --bin knot
CARGO_TARGET_DIR=/Users/markik/Code/repos/knot-editor/target cargo check --locked -p knot-desktop --features retinue
```

The workspace run includes desktop library **199 passed, 1 ignored** (the
existing outline timing probe), launcher **7 passed**, command chrome **8 passed**,
and scenario lane **4 passed**, including all five step-9 scenarios and the
stationary-toolbar regression. Remaining workspace suites and doc tests passed.
Optional resident-retention tests were not enabled by this invocation; the
Retinue result above is a compilation check, not a runtime receipt.

Shared merged-candidate tests passed: Cambium **239**, mere-view **16**,
rootstock **45**, native-host unit **16**, and native-host scenario **24**,
plus the Mesquite and cambium-winit library/integration suites. These cover
menu keys/reentry, semantic clicks, tall-textbox stationarity and graph sizing.
The earlier strict shared-Clippy limitation remains; it is not claimed green.

Local artifacts: `/Users/markik/Code/testing/knot-editor/commands-20260929/`.
Logs: `accepted-workspace-tests.log`, `accepted-desktop-clippy.log`,
`accepted-retinue-check.log`. Earlier failures and diagnostic variants are
retained separately.

Final native capture:

- Binary SHA-256:
  `6c9509d2605155b169a15af5e6442419d912aab83305fbebfbc4c99937207223`
- Receipt: `receipts/palette/scenario.done`, **RESULT ok**, **2414 frames**,
  **2 captures**, **0 blank frames**, **2 distinct digests**.
- Baseline digest: `63daa084b3d2b367`.
- Palette digest: `d1fb44c7e3a4e05b`.
- Palette PNG: 2200x1400; SHA-256
  `d1ff22331d15c4e4029f7491925b9a42d6c9899aab3a8e972619f801dd484208`.
- Fixture SHA-256 before and after native testing:
  `55750ca64366fa50f7ea77d60c731b27ac6948a5b752cb4796fddf8dd99df34c`.

Whole-frame review confirmed the contained, centered palette, visible selected
row and shortcuts, dimmed source, and intact toolbar/status. Theme-derived
primary/on-primary fill supplies selection feedback without relying on the
renderer’s outline support; text contrast is tested in both themes.

The combined-pin interactive build immediately before the selection-fill-only
change (SHA-256 `48e48addae145c307a692c5a65ff08a048331f3004ecc16c8c5697d98402fa06`)
passed Cmd+Shift+P, twelve Down presses revealing View Preview, wheel scrolling
inside the visible list, filtering across Tab, Enter activation, Escape/source
focus, and typing followed by undo back to Saved. The final capture above
qualifies the subsequent visual-only selection fill. No source save occurred.

Remaining validation limits: actual Windows/Linux client-frame window-manager
behavior and complete native accessibility need platform receipts. The separate
source-selection paint spill noted above remains a follow-up, not attributed
to this Commands change without evidence.
