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

## Shared source-edit proposal adapter — 2026-10-18

Status: implemented and source/model-qualified in the working tree on Knot
`7e196c534e64b50cf6f00bf4d95e3beb493e5a9f`. Not committed or published; native
GUI observation is not claimed. The shared API is now authorized and consumed,
not deferred pending a third application.

The existing local-document link form builds a `mere-source-edit::Proposal`
through `KnotDocumentSession::propose_selection_edit`. The immutable candidate
exposes its preview and exact base source stamp without changing the single
retained input or writing a file. `apply_source_proposal` accepts one replacement
covering the captured current selection; unsupported multi-range batches refuse
without mutation. The shared crate validates exact source, UTF-8 ranges, overlap,
document instance, revision and candidate receipt. Knot retains write authority,
IME and selection checks, all local target/path/readability checks, and the
existing explicit save and external-file baseline guards.

The session applies through its ordinary `Edit` intent so successful edits clear
a previous edit refusal consistently. Save history remains intact, and a later
Save still refuses an external change. Typing coalescence is broken on both
sides of insertion, giving it one undo step with the original directional
selection and affinities restored by Undo. Source edits, Undo, Redo, Reload,
and address-changing Save As invalidate prior source stamps; reopening creates
a fresh session instance. An edit followed by Undo cannot revive a link form.

Normal selection-only session intents do not advance the source revision. Public
raw `input_mut` borrows invalidate conservatively, including the desktop's
focused-text-slot clipboard/keyboard integration. The private editable-view lens
is different: it borrows on build/rebuild as well as events, so it must not bump
a stamp merely on borrowing. It captures a transient before-message source
witness and compares after message dispatch; only an actual text change advances
the stamp. Relayout while the form is open and while choosing a target is tested
without stamp churn. This adds no second authoritative editor or save path.
`KnotDocumentSurfaceState::apply` already delegates directly to session `apply`;
the adapter skips no surface synchronization callback. Existing dismissal still
restores the source document and focus.

The following initial local-patch qualification is historical and is superseded
by the published-supplier receiving qualification below. It used the canonical
Git `branch = "main"` manifest dependency with an explicit
**local qualification-only patch for `mere-source-edit`**:

```sh
cargo test --locked --offline -p knot-desktop --test document_links_ui \
  --config 'patch."https://github.com/merely-made/mere.git".mere-source-edit.path="/Users/markik/Code/repos/mere/crates/system/source-edit"' \
  -- --test-threads=1 --nocapture
```

The unpublished new crate does not resolve through the existing Git supplier
without this override. The working-tree lock entry is the locally patched
package, not a published supplier qualification. Existing locked package
versions and source revisions remain unchanged; no other supplier is repinned.
The standalone document lock is ignored by Git and is retained only with the
qualification artifacts. Publication and clean Git-resolution qualification
remain outstanding.

Evidence is retained outside the repository at
`/Users/markik/Code/testing/shared-source-edit-20261018/knot/`:

- Standalone document suite: 51 passed, one existing ignored test; the same
  result on Rust 1.97.1. Engine-enabled suite: 64 passed, one ignored.
- Link-form model suite: 10 passed, including same-text edit/Undo refusal.
- Desktop two-file harness: four passed and one child helper ignored in the
  parent suite. The save/reopen and conflict/reopen workflows each spawn an
  independent test process to reopen the actual saved file and parse its links.
  The helper runs once per child. This is fresh-process source qualification,
  not native GUI observation.
- Existing workspace link-menu/layout/focus scenario: one passed.
- Strict standalone library Clippy passed. Strict test-target Clippy retains
  existing `useless_format` in `hard_wrap.rs:352` and
  `items_after_test_module` in `writer.rs:297`; those files are unchanged.
  Desktop compilation retains the existing unused `path` warning in
  `crates/knot-editor/src/directory.rs:53` and the linker unwind-size warning.
- Formatting, diff checks, and unchanged-existing-package lock comparisons
  passed. Source hashes, final command logs, and exact changed paths are in
  `qualification.json` in the testing directory.

The two-file harness uses real temporary local Djot files, Unicode selections
and filenames, a read-only target and a named relationship. It verifies review
without mutation, explicit insertion and save, exact Undo/Redo, reparsed source
links, guarded external-change refusal retaining the draft, and independent
process reopening of both saved and externally edited source. It does not
claim a reviewed desktop screenshot or whole-product acceptance.

### Immutable snapshot deduplication — 2026-10-18

The follow-on replaces the link form's separate `address: String`,
`text: String`, and `stamp: SourceStamp` with one `base: SourceSnapshot`.
That removes two net retained state fields and three local identity/revision/byte
comparisons. The shared snapshot owns the immutable source witness and its
stamp; its `check` method is now the sole implementation of that guard.
`DocKey`, `DocIdentity`, source path, write posture, format, file permissions,
selected target identity/path/readability and IME/selection authorization remain
Knot-owned.

The session adds `source_snapshot` and checked `selected_source` operations.
The unpublished `propose_selection_edit` now takes the original snapshot and
captured selection explicitly and calls `base.propose`; it cannot silently
recapture a newer base. Common selection validation and range normalization are
factored in the Knot adapter and reused for proposal preparation/application.
There is no old-signature compatibility shim or new mutable buffer. The prior
one-step undo, refusal-state transitions, explicit save and external-file
baseline checks are unchanged.

Against the prior working-tree slice, the desktop workflow production portion
shrinks by ten lines; the session adapter production portion grows by 35 lines
for the checked snapshot/selection API. The production total is +25 lines, with
114 additional test/adapted-test lines (+139 Rust lines overall); this removes
state and guard duplication rather than claiming an overall line reduction.
The baseline and exact per-file increments are retained with the new evidence.

Follow-on qualification artifacts are separate at
`/Users/markik/Code/testing/source-edit-dedup-20261018/knot/`; earlier receipts
remain unchanged. Document default and Rust 1.97.1 suites each pass 53 cases
(one existing ignored), engine passes 66 (one ignored), link-form model passes
11, and the two-file desktop harness passes four parent cases plus both
independent fresh-process reopen helpers. The existing link-menu layout/focus
scenario passes. New cases prove a snapshot retains the original proposal base,
refuses after edit/Undo returning to identical bytes, and refuses forged source
bytes with a reused current stamp without changing source, selection or stamp.
The form-level forged snapshot case also refuses without insertion.

Strict library Clippy, formatting, and diff checks pass. Existing package and
supplier identities remain unchanged. The same explicit new-crate-only local
patch and publication caveat apply; normal Git-only resolution has not been
qualified because the shared crate is unpublished. This remains source/model,
UI-harness, and fresh-process qualification, without native GUI observation.

### Published supplier receiving qualification — 2026-10-18

Current state: the shared supplier is published at
[Mere `70deef2c8cf3d3410ae35b496a9396514070b24f`](https://github.com/merely-made/mere/commit/70deef2c8cf3d3410ae35b496a9396514070b24f).
This consumer commit records Knot's qualified Git-backed adapter.
The earlier local-patch and unpublished-supplier caveats above describe those
historical receipts; they no longer describe the current receiving graph.

The new dependency alone uses the canonical Git URL and this exact published
`rev`. This is an authorized bounded exception to the usual `branch = "main"`
family convention. It is a remote Git intake, with no path dependency, command
configuration patch, environment override, or alternate URL.

Normal branch-main resolution coupled the existing Mere packages to the new
revision. Asking Cargo to preserve old Cambium/Mere failed because that source
lacks `mere-source-edit`. The newer Mere rendering closure then failed against
old root Genet; advancing Genet exposed 16 pre-existing Knot library-test calls
to the removed `taproot::matching` API. These diagnostic attempts were not
accepted as receiving qualification, and no unrelated source was adapted.
Instead, both locks were restored and regenerated by normal Cargo resolution
with the exact new-contract revision. This preserved all existing package
versions and supplier revisions, including the established Mere, Genet, Murm,
Dramatis and rendering families.

In both root and ignored standalone locks, the sole package-source change
relative to the pre-publication local-patch graph is `mere-source-edit` from a
local-patch row to the published Git revision. Cargo metadata resolves its
manifest from the Cargo Git checkout, not the local Mere working tree. No
existing package is removed or added; the contract package was already present
in the qualified local-patch graph. Cargo-generated dependency ordering changes
in the locks are mechanical.

Receiving checks use ordinary locked commands, without overrides:

```sh
cargo test --locked --manifest-path crates/knot-document/Cargo.toml --lib
cargo test --locked -p knot-desktop --test document_links_ui -- --test-threads=1 --nocapture
```

The receiving suite repeats 53 default tests, 66 engine tests, 53 tests on Rust
1.97.1 (each with one existing ignored), 11 link-model cases, four desktop
workflow cases and two independently executed fresh-process reopen helpers, and
the existing menu/layout/focus case. Strict standalone library Clippy and scoped
desktop Clippy pass, with the known unrelated warnings and strict all-test lint
limitations preserved. Formatting and diff checks pass. This is source/model,
UI-harness and fresh-process qualification; native GUI observation is not claimed.

The Git-only metadata, exact lock-source comparison, successful final logs,
failed broad-resolution diagnostic logs, and hashes are retained separately in
`/Users/markik/Code/testing/source-edit-publication-20261018/knot/`. Earlier
receipts remain unchanged. Supplier publication and receiving qualification are
complete. This commit publishes the scoped adapter without changing other suppliers.
