# Document-to-document links slice

Date: 2026-09-29
Status: Implemented and accepted for the bounded local-document slice.
Base: Knot `ca9091e`, following Commands acceptance.

## Context and decision

The design pass orders document-to-document relations after Commands and before
Typography. G1 begins with ordinary links and makes naming a relationship
optional. The desktop has one authoritative source buffer per document and an
existing guarded save path. Its standalone launcher supplies no signed relation
space or author capability.

For this slice, an authored inline link belongs to the Djot source. Inserting one
is an ordinary undoable editor operation; saving uses the same custody checks as
other edits. A relationship qualifier uses an existing canonical Mere predicate
IRI in the link's `rel` attribute. Extracted links and backlinks remain derived
readings over documents already open in this window.

This accepts the source-file owner option for ordinary links. The signed causal
relation model remains the owner of attributable assertion/retraction history
when a host supplies that authority. A new sidecar relation store would duplicate
existing ownership and would need identity, portability and permission rules;
it is not introduced by this slice.

## Scope

- Document command opens a target chooser using a captured source selection.
  The initial action is an ordinary link; choosing a relationship is optional.
- Source must have a saved local path and an editable supported markup format.
  Dirty source is allowed. Unsaved targets require Save As before they can be
  linked. Read-only targets can be referenced without granting them write access.
- Source key, path, address, exact committed text and selection are checked
  before insertion. Closing or rebinding either endpoint makes the pending
  action refuse rather than redirect it to another document.
- The inserted target is a relative URL with encoded path components. Source
  labels escape markup punctuation. The source editor retains undo and selection.
- A Links reading shows outgoing links and backlinks from open documents,
  identifies that scope, and resolves local targets through existing open/activate
  behavior. Preview links share the same source-bound navigation seam.
- Cancel and Escape return to source editing without altering source bytes.

## Consequences and boundaries

The document remains useful outside its workspace, and its authored links travel
with the saved source. A relative path is a portable file link, not a catalog UUID
or a signed assertion identity. Moving/copying source or target files may require
editing their links; this slice does not infer document moves or rewrite links
on Save As. Missing targets are shown explicitly. Passage/heading anchors,
automatic re-anchoring, user-defined predicate definitions, signed assertions,
retractions and cross-space backlinks retain their existing G1 design gates.

Backlinks cover only admitted open document snapshots in this window. A dirty
buffer contributes its current links and is labelled accordingly. Closed files
are not scanned or opened just to populate the reading. External URL activation
does not fetch or open a network resource through this local-document workflow.

## Acceptance

Verify ordinary link insertion, optional relationship round-trip, Unicode and
reserved-character paths/labels, undo/redo, save/drop/reopen, duplicate target
activation, exact-source stale refusal, source/target close and Save As refusal,
read-only and scratch availability, cancellation/focus, and derived backlink
refresh. Meaningful tests must exercise the actual source and parser rather
than compare only generated strings. Native acceptance must open the exact
candidate with isolated fixtures, exercise the gesture and navigation, inspect
the whole captured reading, and retain source/file hashes and capture receipts.

## Verification receipt

- Frozen `cargo test --workspace --locked`: 465 passed, zero failed, one
  existing ignored test. Desktop library: 218 passed, one ignored. Includes
  parser round-trips for Djot and legacy Knot, nine workflow guards, native-host
  menu/palette tests, save/drop/reopen, dirty backlinks, and pinned-preview
  navigation retaining an already-open read-only target.
- Strict library/binary Clippy passed for desktop and readings. All-target
  Clippy completed with existing test-code warnings; `-D warnings` is not a
  clean all-target gate (existing scroll-site/workspace test warnings remain).
- Link readings refuse source snapshots over 1 MiB or more than 4096 links.
  The parsed AST supplies facts; unsupported source-span syntax retains no
  selectable span instead of inventing one.
- macOS isolated candidate SHA-256:
  `fd89a760cbb3d403856ad14bd44113c9b29c7c3cab87efc6f4f510c69ea1f0bb`.
  Native interaction exercised ordinary empty-selection insertion, Undo,
  keyboard-selected text with `supports`, guarded Save, existing target
  activation and backlink return. Multiline selection was refused without
  editing. Only the isolated essay was saved.
- Final native reopen/navigation receipt: `RESULT ok`, 2420 frames, three
  2200x1400 captures, zero blank, three distinct digests. CUA opened Links via
  Cmd+Shift+P/query/Enter during launch grace; the scenario then asserted exact
  document identity, clean source, link count and reading state and drove
  target/backlink navigation. Whole-frame images were inspected.
- Native root: `/Users/markik/Code/testing/knot-editor/document-links-20260929`.
  Final receipt: `receipts/links/scenario.done`; workspace and Clippy logs are
  alongside it. Two failed 2429-frame selector/setup runs (four captures each)
  remain in separate receipt directories; their assertions were not counted
  as acceptance.
- Saved essay SHA-256:
  `bf536259b238f8072be632d2fbc0fddd7584949d7a98cbd8bfd656ae7486a292`.
  Target unchanged SHA-256:
  `beffa253b180102284f4110a48a18a64e8957e105dbf404033d25cf1acbb1175`.

Native review exposed long-URL crowding; rows now show decoded targets and short
relationship labels with separate actions, while accessible descriptions retain
the exact encoded URL and canonical IRI. A 900px host test verifies that layout.
The return capture also shows source selection paint extending across the pane
boundary, consistent with the separately observed Commands selection-paint
issue; its cause and preexisting status are not established here. It remains a
shared-renderer follow-up, not a navigation or source-byte acceptance claim.
Windows/Linux window managers and full screen-reader interaction were not
qualified on this macOS host. The architecture decision above guided storage:
no sidecar or signed relation authority was introduced.
