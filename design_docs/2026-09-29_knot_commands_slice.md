# Commands slice

Date: 2026-09-29
Status: Implemented and headless-verified; native macOS acceptance blocked by
the locked desktop. Candidate branch only; not integrated into Knot main.
Base: Knot `8a454fb`, published to main before this slice began.

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

## Verification receipt

Shared pin: `ba3ecfda5cc748f05fe54460af0b98ed128bbf32`, published and independently
verified on Mere branch `codex/knot-command-menu`. It is a bounded descendant
of the prior `ac1a5e5dc` presentation pin; Mere main was not changed.

Final Knot commands (all exited successfully):

```sh
CARGO_TARGET_DIR=/Users/markik/Code/repos/knot-editor/target cargo test --locked --workspace
CARGO_TARGET_DIR=/Users/markik/Code/repos/knot-editor/target cargo clippy --locked -p knot-desktop --lib -- -D warnings
CARGO_TARGET_DIR=/Users/markik/Code/repos/knot-editor/target cargo build --locked -p knot-desktop --bin knot
```

The workspace run includes desktop library **198 passed, 1 ignored** (the
existing outline timing probe), desktop launcher **7 passed**, independent
command chrome **7 passed**, and scenario lane **4 passed**, including all five
step-9 scenarios and the clipped-editor stationary-toolbar regression. The
remaining workspace suites and doc tests passed as well. Optional feature-gated
resident-retention suites were not enabled by this invocation.

Shared tests: Cambium library **238 passed**, cambium-winit library **5 passed**,
rootstock library **42 passed**, native host library **16 passed**. The final two
host suites include end-to-end F10/Alt lowering; the earlier Cambium run includes
seven new menu-bar tests. See the shared-Clippy limitation above.

Local artifacts: `/Users/markik/Code/testing/knot-editor/commands-20260929/`.
`workspace-tests.log` and `desktop-clippy.log` preserve the final command output.
The isolated `KnotCommandsTest.app` wrapper is ready for interactive testing,
followed by `palette.scn` and its native capture receipt.

- Binary SHA-256:
  `f99f34a504ab9b32fcb58aa31b451bd6b24b599e95f35b67ba10b77dbd87d786`
- Copied `field_notes.djot` SHA-256 before native testing:
  `55750ca64366fa50f7ea77d60c731b27ac6948a5b752cb4796fddf8dd99df34c`
- Native launch request returned: "The Mac is locked and automatic unlock
  could not unlock it." No native acceptance or capture is claimed.

Next gate: unlock the Mac, launch the exact isolated candidate, exercise
Cmd+Shift+P, filtering, keyboard activation, Escape/focus return and unchanged
source bytes, then capture and inspect the whole palette frame. Only after
that receipt should this candidate be integrated into Knot main. Windows/Linux
window-manager and native accessibility validation remain separate platform
gates, even when macOS palette acceptance passes.
