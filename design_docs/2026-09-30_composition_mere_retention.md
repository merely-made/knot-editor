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
organization is a separate authored projection, described below. Retraction and
additional domain records such as saved sessions remain follow-ups, not
disguised publishable prose documents.

## Collection organization

The first organization slice adds label, collection, author notes, tags, order
and reversible archive state. `OrganizeCompositionV1` appends a signed, encrypted
operation; it does not replace `RetainCompositionV1`. Original text, source
anchors, analysis, recipe and historical author/receipt stay immutable.

An edit identifies the original writer and item ID, its expected organization
revision and the exact selected destination. The resident checks current
authority and grant bounds under its mutation gate. Stale edits refuse and ask
for refresh; concurrent forks refuse projection pending explicit reconciliation,
not an arbitrary last-writer-wins overwrite. A currently admitted writer can
organize material retained by another admitted device in the same space. History
is bounded, including metadata operations. New event variants require updated
participating readers; no mixed-version upgrade negotiation is added here.
There is no organization-fork resolution UI yet; the strict projection refuses
such a history instead of presenting a falsely confirmed collection.

Readings → Collection offers search, deterministic ordering by collection/order/
label/identity, editing and archive/restore. Archive hides rows from the ordinary
view without removing their history; Include archived items makes restore
available. Search covers original text and current organization. Search and
archive visibility are transient presentation choices, not new plaintext stores.
Tags are comma-separated in the editor; there is no destructive purge action.

Acceptance covers encrypted metadata reopen, immutable original receipts,
stale/destination/bounds refusal, worker dispatch and archive/restore. The
`collection_acceptance` example supplies marked disposable synthetic authority
using public test keys. It must never initialize a personal wallet. The paired
`collection_organization.scn` and `collection_organization_reopen.scn` exercise
native organization and fresh-process restore. The compact variants use the
actual View menu at 420 logical px. Qualification below separates native
controls from automated edited-metadata checks and personal-wallet onboarding.

### Organization qualification (2026-10-05)

Committed runtime: `21ce01576d0ba4cbac081a5b6a4098399c76b38a`, Mere
`07db35e2f154df9a14a8cfdc165cc72ddae61f13`, Genet
`bd3e8861b2932d62b73c9936ac3a317f1df3ffdb`. Default-feature workspace/all-target
tests passed: **574 passed, 0 failed, 2 existing ignored**, across 42 test
executables. This includes eight real-store retention/organization tests, two
real desktop-worker store tests, and the seven scenario-lane tests. The ordinary
desktop binary and synthetic-authority native instrument built successfully.

Final macOS native 420×900 run: four reviewed captures (editor, editor actions,
restored item, archived item); a separate fresh process reopens archived state,
restores it and selects the original quotation, with one reviewed capture.
Both final receipts are `RESULT ok`. The empty notes field's missing visual
height was found by capture review and fixed; Save/Cancel are visible when the
editor's end is revealed. The source/status chrome remains stationary while
the Readings drawer scrolls. The drawer intentionally overlays the narrow
source; its partial source visibility is not a full-width reading claim.

Local evidence root:
`/Users/markik/Code/testing/knot-editor/collection-organization-20261005.TGUMZ3/`.
Tests: `workspace-final-tests.log`. Final native artifacts:
`narrow/final-captures/` and `narrow/qualified-reopen-captures/`;
logs `narrow/final-launch.log` and `narrow/qualified-reopen-launch.log`.
Native instrument SHA-256:
`b215e9fafaa872b07ef9a5aa82725d1dc48f41c9c9c8a1b0c6b4c1cee4e38eb5`.
The launcher increases only the initial settle to 900 frames for foregrounding.
Earlier diagnostic runs remain evidence of an overstrict fixture guard, a
wide-only selector at compact width, and reused paint-output collisions; they
are not substituted for the passing final captures. Reopen needs the marked
test store/source, not a fabricated publishable-document vault directory.

Limits: native runs use public disposable test keys, never a personal wallet.
Native saving uses unchanged details; edited values and encrypted metadata
reopen are covered by worker/store tests, not a claim that every native field
was typed. No screen-reader, Windows/Linux/browser, organization 400%-zoom or
release-package/signing acceptance is added. The upstream `54bb8cd` merge reports
a separate Windows 400%-zoom root-scroll/unpainted-bottom issue; this slice
does not fix or requalify it. GUI persona attachment and source overlays remain
the next bounded slices, not completed by these receipts.

## Follow-on contextual workspace

Accepted direction (2026-10-05), not implemented by the organization slice:
an independently toggleable generated background situates foregrounded text or
material among related concepts and configurable categories. Lenses/planes
separate authored facts, source-derived readings, inferred suggestions and
curation; a visual or physics grouping must not silently author collection
membership or source claims. Foreground readability, keyboard access, static/
reduced-motion presentation and bounded/off-path compute remain requirements.

Mere's projection and dynamics grammars provide reusable recipes over one
disclosed domain binding. Arrangements are positions; dynamics are motion;
seeded/anchored/pinned roles control how positions behave. Graphshell's saved
scene facets and owner-authored graph events are precedents, not permission to
move Knot's domain into Graphshell's store. Current published reference:
Mere `289c9a98d6b0f17a441f184839035784fadb865e`, README design vocabulary,
`ports/graphshell/src/product.rs` and `personal_sync.rs`, and the dynamics grammar
plan. The plan's earlier headline status is not evidence that all later tracks
or the portable dynamics-spec save/reopen track are complete.

Cross-platform acceptance must distinguish the portable retained contract from
host rendering, accessibility and compute backends. CPU/static fallbacks and
explicit opt-in provider/model/resource choices are required design targets;
Connecting Burn/embedding providers to this contextual background is not claimed
as landed. The October 7 direction prioritizes shared scene adoption: refresh
the stack, qualify a coherent repin, and connect projection editing, dynamics
and foreground/background strata. Owner/persona attachment remains separate
work, not a prerequisite for testing scenes with disposable authority.

### Shared scene adoption continuation (2026-10-07)

Woodshed already has contextual musical backgrounds and shared graph swatches;
the missing work is adoption of the newer interaction/runtime contracts, not
inventing all background behavior. Knot's relationship view currently realizes
compiled positions as measured native controls. A dependency update alone does
not turn either host into the full Graphshell workbench.

The implementation boundary is the portable Sceno/Scenograph/Scenomise/
Scenotime contracts and reusable Cambium components, not copied Graphshell web
product code. Hosts retain domain facts, exact source actions and their own
Mere stores. Foreground/background strata are authored presentation roles;
they must not become new ownership, collection membership or source claims.

The first bounded editor continuation uses shared
`RecipeEdit::SetRelationshipKind`: choose all disclosed relationships or one
disclosed category, compile the candidate, then install only a successful edit.
Changing categories keeps a selected explanation if it remains visible, and
clears it only if excluded. Occurrence selection, source anchors and the entire
copied disclosure remain unchanged. The authored recipe
gets a new content revision; explicit retention saves a new version through the
existing encrypted capability. No new provider, inference or script is enabled.
An undisclosed category refuses without changing the previous reading.

At inspected Mere main `d041cc69b`, the portable `DynamicsSpec` is still not in
source. Existing per-item placement roles, transitions and live Seiche physics
are useful primitives, but are not a completed portable dynamics recipe/editor.
The subsequent integration target is an editable scene with configurable
foreground/background categories and supported motion under one binding,
including camera navigation, static/reduced-motion presentation, keyboard
actions and host-owned save/reopen. No native scene, screen-reader or full
dynamics acceptance is implied by the category-editor continuation.

#### Current-stack and category qualification

Runtime `81f0039b482e5dff651e241e475b00b74285015d`, dependency alignment
`493cccc`: all tracked Mere declarations use
`d041cc69b588b6f1dadd22308c2bc4059496cabd`. Genet stays at
`d851a9db0cd1ff7837768250f21e9dff63455940`, the revision declared by that Mere
tree; the independently newer Genet head is not mixed into this source graph.
Burn 0.22.0 and CubeCL 0.11.0 remain unchanged. The preview accepts the newer
non-exhaustive Block contract with an explicit unsupported-kind fallback.

`TMPDIR=/tmp cargo test --workspace --all-targets --locked --offline` passes
**581 tests, 0 failures, 2 existing ignored**, across 42 test executables.
This includes category filtering/no-op/refusal, source/selection preservation,
real desktop control retention, the real encrypted-worker/store reopen,
the updated relationship scenario and existing compact/high-zoom regressions.
Locked metadata resolves from outside the repository config search path;
the standalone document manifest also resolves. The ordinary locked/offline
desktop binary builds. Focused file formatting and whitespace checks pass;
the existing unused p2panda-stream patch and directory variable warnings remain.

Evidence root:
`/Users/markik/Code/testing/knot-editor/scene-adoption-20261007.rZcfdR/`,
with `workspace-tests.log`, `desktop-build.log` and `metadata.json`.
Desktop binary SHA-256:
`c0cea602b72a4a4b78b807364db00098e33c47175ae11a09b52e5ddd19fffcf1`.
An earlier integration attempt used two lingering desktop Mere pins and failed
on duplicate tinct/apparatus types; the recorded passing gate uses the final
uniform graph. No new native capture review, human screen-reader test, personal
wallet onboarding, other-platform run or release packaging is claimed.

#### Coordinated family follow-up, Windows, 2026-10-07

Candidate baseline `ec0224e6023dc0282730e6ddc388c666ac520769` now aligns all
43 Mere declarations to `57b4893db6909d5ed9c4ccae30216f0d8164201a` and all 14
Genet declarations to `965b64e206a47d1c8808472de9aa461233638768`, including the
independent document and desktop manifests. All 128 tracked Rust files remain
identical to the published baseline after Git line-ending normalization.
The lock retains registry versions except the qualified fontsan backend change
from fontsan-woff2 to wuff/wuff-capi. Root and standalone engine metadata each
resolve exactly one Mere revision and one Genet revision.

The Windows locked workspace all-target check passes. The first test run failed
seven WordNet cases because Git checked out the embedded OEWN notice with CRLF,
which its existing validator rejects. A per-asset LF attribute restores exactly
the existing Git notice bytes; neither Rust code nor the validator is changed.
The failed control remains recorded. The WordNet rerun passes 12 tests, and the
full locked no-fail-fast workspace rerun passes **579 tests, 0 failures, 3 existing
ignored**, across 42 executables. Standalone document tests pass 47 with default
features and 60 with the engine feature, each with one existing ignore and
passing doc-tests. The third workspace ignore is Windows symbolic-link
privilege; this cohort is separate from the preceding macOS qualification.

The [current-family receipt](../docs/receipts/2026-10-07_current_family_repin/README.md)
contains exact manifest rows, input and artifact hashes, source-identity metadata,
raw commands/results and the failed notice bytes. Integration review approved
publication; U9 remains ordered Knot then Redshank then Turnstone. Linux
current-family verification belongs to the final Turnstone integration owner.
No native or screen-reader acceptance, identity seed/vault repair, release
packaging or complete Turnstone S0 acceptance is inferred from these tests.

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
visual acceptance was subsequently qualified below; other platforms remain unrun.

Responsive recipe continuation (2026-10-05): implementation
`e4cf739c958b87331b966bb3ba5e4498dead3d14` coherently adopts Mere
`5011e2f90e988775410fe4b4213e9895fe64538e` in the workspace, app and standalone
document declarations; Genet remains `bd3e8861`. The lockfile changes 57 Mere
source identities, without introducing a second engine version. The shared
horizontal-reveal fix brings the selected spatial occurrence fully into view.
No solver, recipe interpretation, source action or retention authority is copied
or changed. Stable classes on the existing explanation, source anchor and notice
paragraphs permit capture targeting without making those paragraphs interactive.

The new `relationship_recipe_compact.scn` uses the native compact View menu,
selects the second spatial sound occurrence, preserves spacing 24, explains the
perfect rhyme and its method/limits, returns to the exact original source, and
refuses stale rebind and unavailable retention authority without dirtying source.
Its headless regression uses bundled fonts and the real responsive frame hook
at 420×900 and 1280×900 / 400% UI zoom. Initial test-helper chrome/font/frame-hook
mismatches and an unsupported bare-tag scenario selector were corrected; those
failures are not attributed to product behavior.

Committed-source native runs pass four captures at each setting (997 and 1007
frames respectively). All eight were visually reviewed: the selected `light`
card is revealed rather than clipped horizontally; explanation and source-anchor
captures disclose Mora, opt-in CMUdict, first-pronunciation choice and the intended
performance limitation; the retention refusal is visible. Long content remains
vertically scrollable at 400% rather than being compressed into the short viewport.
Source/status and drawer chrome remain bounded. Pre-adoption captures are preserved:
their text assertions passed but the selected card was clipped and high-zoom
explanation/refusal text was outside the captured checkpoint.

Evidence: `/Users/markik/Code/testing/knot-editor/recipe-responsive-20261005/receipt.json`,
with final captures under `final-narrow/captures` and `final-zoom400/captures`.
The ad-hoc signed test wrapper uses isolated APPDATA settings and no Mere target.
External scenario copies only extend initial settle 120→900 for native launch.
This is responsive UI qualification, not successful native personal-wallet
retention, fresh-process collection restoration, release signing, or other-platform
acceptance. Existing encrypted real-store automated coverage remains separate.

Final updated-pin gates: desktop 306 passed, 0 failed, 1 existing ignore;
composition/readings 61 passed, 0 failed, 1 existing ignore; locked desktop build
and locked metadata pass. The full desktop gate includes the compact regression
and both real-store desktop-worker encrypted-retention checks. No broad strict
Clippy or standalone excluded document-crate test claim is made. Binary SHA-256:
`b64c3cf1225812ec6d3d665d146cd97f25762d67023c7f43017331af0da41aea`.

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

### Stack seams P1: Knot measures its recipe card (2026-10-05)

Mere's stack seams P1 (`dd2cb6fd`, on main from `19e6dc9f`) made relationship
compilation a method of a `ProjectionCompiler` built from host-supplied
`ItemSizes`. Its plan's ruling S20 has the host supply "the representation's
measured size"; the compiler "writes in no size of its own". Before P1 Knot
never had a card of its own: it drew each card at the 164×68 footprint
scenomise wrote in, inside a fixed 184×84 cell. Every Mere row, the ruling 585
`cubecl-runtime` row included, moves from `07db35e2` to `19e6dc9f`.

Mark's rulings are recorded in Mere's burn plan 13.46 (`102aa548`):

- Who adapts Knot. Mark: **"My Knot lane adapts Knot"**.
- Knot's card size under P1. Mark: **"Measured from Knot's font"**. The desktop
  measures each recipe's widest occurrence label in its own font, plus button
  padding, and gives the compiler that size.
- How to measure. Mark: **"Read the previous layout (Recommended)"**. The drawn
  string is probed at weight 600 and read through `painted_rect` in the frame
  hook. The result is stored by label set, and the probes are removed once
  measured.
- The first frame after labels change. Mark: **"Hide the scene one frame
  (Recommended)"**.
- The card's shape. Mark: **"One line, no width cap (Recommended)"**. The widest
  label sets every card's width.
- Validation's nominal size. Mark: **"Named 164×68 constant (Recommended)"**. The
  constant is validation-only and lives in knot-composition, shared by the
  desktop's four action gates and the tests. Only the view's draw call takes
  the measured compiler.

As built:

- `knot-composition` names `VALIDATION_CARD` (164×68) and `validation_compiler()`.
  Retention validation, the build, rebind, reopen and spacing gates and the
  tests use it. Accept, refuse and every issue are the same at any card size
  (`tests/recipes.rs`); retained material holds no geometry.
- The recipe view draws only once its label set is measured. Until then the
  scene holds one probe per drawn label: a role-less, `aria-hidden` span whose
  label sits in an attribute and is shown only through `::after { content:
  attr(...) }`, at the pressed weight 600 on one line. It has no DOM text or
  role, so a scenario's `resolve` and `assert text` cannot match it.
- The desktop frame hook reads the probes' rects from the previous layout. The
  card is the widest probe rounded up, plus 1 px and the declared 22 px of
  padding and border, by the probe's line plus 14 px. The hook stores it with
  its label set, which removes the probes, and keeps frames coming while a
  probe still has no layout.
- Only a finite, positive probe rect counts as a measurement. A probe with no
  rect, or a zero or NaN one, keeps the scene hidden and is measured again on
  the next frame; the compiler is never handed such a size. This matches stack
  seams ruling S32 ("Typed compile issue"), which will refuse a card that is
  not finite and positive (`a_probe_that_measures_nothing_keeps_the_scene_hidden`).
  The 164×68 validation constant passes that check.
- A new label set (a new recipe or a rebind) is hidden for exactly one frame
  and measured once. Selection, spacing, zoom and theme changes neither hide
  the scene nor re-measure (`composition.rs`,
  `recipe_cards_are_measured_once_per_label_set_and_hidden_for_one_frame`).
- Controls (`recipe.rs`, `a_measured_card_holds_its_label_on_one_line_and_a_narrower_one_wraps`):
  each test label stays on one line at its measured card and wraps a pixel
  under the rounded probe width. The drawn button equals the compiled
  footprint.

P1 changes the spacing whatever the card: grid pitch is now the card plus the
gap, not a fixed 184×84 cell. The headless runners pump the frame hook for both
recipe scenarios and the real-store recipe test, as the native loop does.
Native recipe receipts are held until the seiche-speed lane's headed round is
done; they are rerun then at both window settings and compared with the
164×68 frames.

Two further rulings, Mere burn plan 13.46 (`b7b52e56`):

- A probe that never measures. The question: a probe that can never give a
  usable size, for example after a font or stylesheet failure, made the frame
  hook request frames forever. Mark: **"Stop after a few frames
  (Recommended)"**. After about 3 frames with no usable rect the hook stops
  requesting frames; the scene stays hidden, with a logged warning, and it
  measures again on the next label change or window resize.
- When Knot pushes. Mark: **"After the headed receipts (Recommended)"**. Once
  the seiche-speed lane's timing round is done, Knot's recipe receipts run at
  both window settings and their frames are compared with the old 164×68
  ones; then Knot pushes.

*Amended 2026-10-06:* the hook no longer keeps frames coming without end, as the
list above says. As built, `MEASURE_ATTEMPTS` is 3. Failed frames are counted
per label set and window size, the window size being the layout size times the
zoom, so a zoom alone is not a resize. The third failure logs
`knot: recipe cards not measured after 3 frames; ...` once and stops asking for
frames. A new label set or a resize starts a fresh count.
`a_probe_that_measures_nothing_keeps_the_scene_hidden` checks the cap, the
stop, the hidden scene with nothing stored, a resize, a zoom and a new label
set. With the cap removed, the same test fails.

Headed receipts at `306a808` and three rulings, Mere burn plan 13.46 (`37f7e39b`):

- Push Knot now? Mark: **"Fix the empty band first"**.
- The empty band. The scene kept a 90 px minimum height, so 31 px cards left
  about 59 px of empty panel above "Explain…". Mark: **"Fit the scene's bounds
  (Recommended)"**. The minimum is gone: the scene is exactly as tall as its
  laid-out cards. No recipe can draw an empty scene, because Scenograph refuses
  a recipe needing fewer than two occurrences and the compiler refuses a
  dataset below that, which the panel shows as "Recipe cannot be realized"
  (`a_recipe_below_its_minimum_shows_its_refusal_and_no_scene`). The 180 px
  minimum width stays: it is not ruled, and two cards are already wider.
- The selected first card stays partly scrolled off after Explain. This
  predates the change; the shared horizontal reveal does not re-run on a
  selection change. Mark: **"Report to the reveal's owner (Recommended)"**.
  Knot changes nothing.
