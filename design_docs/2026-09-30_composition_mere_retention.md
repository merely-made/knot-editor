# Composition material belongs in Knot's mere

Status: accepted ownership decision; implementation qualification below.

## Decision

Knot's own mere is the retained-data home for its domain. Sharing implementation
with sibling apps does not change that ownership. The initial plaintext
`composition/collection.json` prototype is a legacy import source, not the
desktop's ongoing collection store. Moving a library to the Mere repository
would not accomplish this data integration.

The desktop submits copied `CollectionItem` data through a host-issued
`CompositionRetainPort`. Its explicitly selected target binds persona display
identity, space, writer and encryption profile. The resident rechecks live
authority, grant bounds and destination under its normal locks, then authors a
signed, sealed `RetainCompositionV1` operation through Knot's mutation gate.
This projection is separate from editable/publishable vault documents. Receipts
retain operation hashes; historical authors are distinct from current authority.

Retention starts neither networking nor publication. A space's existing
replication policy still applies when its owner runs replication; this is not a
promise that retained data can never sync.

The new event is additive for updated readers, but older Knot binaries may
refuse to replay an unfamiliar event. Upgrade participating readers before using
composition retention in a replicated space; no protocol-negotiation upgrade
or peer rollout is performed by the desktop.

## Attach an existing personal mere

Normal desktop builds include the capability, but ordinary launch grants no
destination. Explicitly name an existing data root and persona:

```sh
cargo run -p knot-desktop --bin knot -- \
  --mere-root /absolute/path/to/existing-data \
  --mere-persona YOUR-EXISTING-PERSONA-UUID \
  /absolute/path/to/essay.djot
```

Personae's configured startup-unlock policy applies; no passphrase or key goes
on the command line. Attachment refuses missing operation stores, absent device
identity, unavailable unlock, divergent existing vault material, or a store held
by another owner. It creates no persona, bootstraps no mere, migrates no unsynced
documents, stops no resident and starts no peer transport. Personae's existing
loader may upgrade a legacy device-identity encoding under its established
unlock policy. Reconcile divergent vault material through its owner first.

Then select the destination in **Readings → Collection**. Reads and writes run
on workers. Opening the panel does not select a target. Without one, Collect
refuses instead of writing a plaintext fallback. A `--no-default-features` build
remains file-only unless `mere-retention` is explicitly enabled.

Embedding hosts may supply an already-owned resident capability to
`run_desktop_with_targets`; they must not open a competing owner. A GUI persona
chooser and live-resident IPC attachment remain future work.

## Legacy import and failures

Existing sidecars remain untouched. Import is an explicit action into the chosen
mere, preserving item IDs, text and provenance. Items are independently retained;
a partial batch is not transactional. Same-writer retries reuse matching
identity/content after reopen; conflicting content is refused, not replaced.
Refresh preserves uncertain retry IDs and destination binding. New target
selection must not relabel old worker results. Successful import never deletes
the legacy file; backup/removal remains the user's decision.

The corrected path requires unlocked, admitted authority. That is more involved
than an always-writable sidecar, but avoids splitting retained material from
Knot's existing encryption and history. Immutable collection items are supported;
mutable organization, retraction and additional domain records such as saved
sessions are follow-ups, not disguised publishable prose documents.

## Qualification

### Shared relationship recipe continuation (2026-10-05)

Status: implemented and qualified for the bounded slice below. Knot and
Woodshed consume one Scenograph relationship recipe and the shared Scenomise compiler. Knot's
first adapter uses actual Mora sound relationships over exact selected-source
occurrences. Shared definitions, edit validation, arrangement and relationship
routing belong in Mere; source interpretation and navigation remain in Knot.

The retention extension is a typed projection-recipe collection item. It keeps
the authored snapshot, disclosed analysis and exact source anchors so an
immutable version can reopen without rerunning analysis or reacquiring its
source. Existing signed/encrypted collection authority and bounds apply. Recipe
edits are explicitly retained as new versions; they never overwrite history.
Return to source checks the document hash and occurrence span. A saved recipe
confers neither source access nor script execution permission.

Acceptance requires actual sound-edge disclosure, duplicate source occurrences,
shared edit/compile/refusal behavior, typed encrypted save/reopen, and bounded
UI controls. Native qualification will identify the exact paths captured. See
the cross-domain continuation in
`mere/design_docs/mere_docs/implementation_strategy/2026-08-15_projection_grammar_adoption_plan.md`
for the common contract and Woodshed lane. Woodshed's current adoption is a
wire-compatible retained session plus a standalone compiler instrument; visible
host editing remains follow-on work. No domain facts or owner actions move into
the shared authoring crate.

The new recipe kind is backward-readable by updated readers when absent from
older records; older binaries cannot deserialize new recipe records. Upgrade
participating readers before retaining recipes in a mixed-version space.

Shared dependency: Mere `c79bb8c203b52817991e0d7ba1c4ce3c3a2aac34`, uniformly
pinned across Knot; Genet remains `bd3e8861b2932d62b73c9936ac3a317f1df3ffdb`.
The analysis revision incorporates the captured selection, pronunciation choice,
actual relationships, method/provider and layer flags, independently of the
whole-document hash used for exact navigation. Authored edits have their own
content-derived recipe revision. Relationship recipes support spaced grids;
ordinal source order is not an invented scatter distance.

Qualification (2026-10-05): desktop check/build passed; focused composition UI
tests pass 25/25, the recipe scenario passes 1/1, and the actual cross-domain
adapter rebind test passes 1/1. Composition passes 31 tests
with its existing external-dataset test ignored; editor retention passes six
tests, including separately retained encrypted recipe versions and reopen.
Readings passes 30 tests. Editor/capture/file-catalog suites pass 203 tests.
Both real desktop-worker collection tests pass, including edited recipe
retention and resident/UI reopen. Its first failure was a harness redraw timing
gap: confirmation reached the DOM before the new button received painted bounds.
Delivering the normal next redraw fixes it at the unchanged 1100×800 viewport;
no input bypass or geometry workaround is used. Final `TMPDIR=/tmp cargo test
--locked -p knot-desktop` passes 305 tests, with zero failures and one existing
ignored test. All four Collapse tests pass. Logs for this run:
`/tmp/knot-recipe-desktop-publish.log`, `/tmp/knot-recipe-core-all.log`,
`/tmp/knot-recipe-composition-tests.log`, `/tmp/knot-recipe-readings-tests.log`,
and `/tmp/knot-woodshed-recipe-rebind.log`. New/touched recipe Rust files pass
focused rustfmt and whitespace checks; workspace-wide formatting still reports
pre-existing differences in unrelated files. No broad strict-Clippy claim is
made for the app workspace.

Native `KnotRecipeTest.app` at 1280×1100 passed the recipe scenario with four
captures, all reviewed: actual Night/light sound occurrences, grid spacing
16→24, the explained perfect rhyme with provider/first-pronunciation disclosure,
exact original-source action, explicit stale-rebind refusal, and unavailable
retention-authority refusal. Capture directory:
`/Users/markik/Code/testing/knot-editor/relationship-recipe-20261005/receipts`;
prefix `relationship-recipe-v1-`, receipt `scenario.done`. The first background
launch was occluded and failed capture; the successful retry used a longer
startup settle and raised the window. This is not successful personal-wallet
retention acceptance; that authority was not attached. Narrow/high-zoom recipe
visual acceptance and other platforms remain unrun.

Native binary SHA-256:
`63c2e1b84e0af76fdac189627e1027439a1bd5a7d7221e1bb4a1504db341a393`.
The test binary includes the recipe implementation on the published shared pin;
subsequent changes before publication are test/receipt documentation only.
Woodshed's generated disclosure fixture is copied byte-for-byte from its
published `c92e7c96779ef316f5d8244a59de7cbc84f42a0b` export into
`apps/desktop/tests/fixtures/woodshed_relationships.json` for the actual
Mora-to-musical shared-recipe rebind test. It is copied evidence, not a live
connection or a grant to modify a Working Set.

The earlier five native captures qualified the sidecar prototype, not this
corrected route. The corrected route has separate evidence:

- Final `cargo test --locked --workspace`: 555 passed, zero failed, two ignored.
  The file-only desktop check and native desktop build pass. Strict Clippy passes
  for `knot-composition` and `knot-readings`; the broader desktop invocation is
  blocked by nine pre-existing `knot-editor` lint errors, not recorded as clean.
- Core real-store tests cover encrypted persistence/reopen, authority refusal,
  cross-writer history, idempotence and exclusion from publishable documents.
- `apps/desktop/tests/composition_mere.rs` drives the UI worker against a real
  resident, checks the signed operation and absence of plaintext fallback, then
  reopens and reads the same item and operation.
- The native `composition.scn` run on 2026-09-30 passed in 179 frames with five
  distinct nonblank captures. Visual review confirmed the lexical empty state,
  opt-in sound source, Night/light rhyme result, unavailable collection, and
  source registry. Local receipts are in
  `/Users/markik/Code/testing/knot-editor/composition-mere-20260930/receipts`.
  This native run deliberately supplies no mere authority; it does not qualify
  successful native retention or a real user's Personae startup-unlock flow.

The sound-result view extends below the captured viewport; this is not an
all-viewport layout acceptance claim. Whole-history reads use the existing
operation-store path; result bounds are not a claim of a globally bounded scan.
