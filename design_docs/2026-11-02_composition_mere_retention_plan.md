# Composition in Knot's mere: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut from [the composition retention plan](archive_docs/2026-11-02/2026-09-30_composition_mere_retention.md), archived whole with its retention history, the October 5 to 11 continuations and their inline run evidence. The ownership decision is accepted and implemented; the bounded native, dynamics and organization qualifications are recorded in the archived plan. Open: personal-custody acceptance, the persona chooser, the codex handoff's tasks 2 and 3, the overmap qualification gaps, and the dynamics-adoption gates the archived plan leaves.

## Rulings in force

The archived plan's Decision section (lines 7-31) is the ownership ruling and stays in force: Knot's own mere is the retained-data home for its domain; the desktop submits copied `CollectionItem` data through a host-issued `CompositionRetainPort` whose explicitly selected target binds persona display identity, space, writer and encryption profile; the resident rechecks live authority, grant bounds and destination, then authors a signed, sealed `RetainCompositionV1` operation through Knot's mutation gate; retention starts neither networking nor publication; the plaintext `collection.json` prototype is a legacy import source only. The archived plan's attach, legacy-import, organization and recipe sections are grandfathered. The overmap contract and rendering records, now archived, keep Mark's "catalog, route as overmap" ruling (`archive_docs/2026-11-02/2026-10-15_knot_mere_overmap_layer.md`, "Ruling"). DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `composition_mere_retention D<n>`.

## Decisions

### composition_mere_retention D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all to Code/testing". The receipt folders this plan cites (the October 7 family repins, the October 11 dynamics receipts, catalog folds, the overmap live run and others) were among the 22 folders of `docs/receipts/` and now live in `Code/testing/knot-editor/receipts/`, cited as paths. The moved scripts anchored at `parents[3]` or `$KNOT_SRC` no longer find the repository by themselves.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/knot-editor

### composition_mere_retention D2: cut the plan, draft first (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The 1,138-line plan is cut under DOC_POLICY section 8 and archived whole after this draft is reviewed.

Links: area documentation · replaces 2026-09-30_composition_mere_retention · summary old plan cut to open work and archived whole

### composition_mere_retention D3: old hashes (2026-11-02)

Mark, 2026-11-02: "Same: live docs only". This plan cites main hashes; the archived plan keeps its own. Mapping: [the receipts file](archive_docs/receipts/2026-09-30_composition_mere_retention_receipts.md).

Links: area documentation · summary live docs cite main hashes; archived docs keep old ones

### composition_mere_retention D4: the codex handoff's tasks 2 and 3 become open items here (2026-11-02)

Mark, 2026-11-02: "Carry 2-3 into a cut plan, archive". Tasks 2 and 3 of the 2026-10-15 handoff become O3 and O4 below, in this plan because it owns the overmap work; the handoff is archived. Task 1 was done on 2026-10-15 (`7ee24f9`).

Links: area overmap · draws on 2026-10-15_knot_codex_handoff · summary handoff tasks 2 and 3 carried as O3 and O4

## Open work

### O1. Personal-custody native acceptance

The archived plan's native matrices use marked synthetic authority and are "not personal-wallet, human assistive-technology, cross-platform native, signing/release, inferred ambient neighborhood or G4b live-execution qualification" (archived lines 667-670); its responsive UI qualification is "not successful native personal-wallet retention, fresh-process collection restoration, release signing, or other-platform acceptance" (lines 991-996).

**Done when:** a native run attaches an existing personal mere with real personal-wallet custody, retains a collection item through the Collection panel, and restores it in a fresh process, with the machine, commit and unchanged-source evidence recorded; Mark names the platforms.

### O2. Persona chooser and live-resident attachment

Attachment is by launch flags (`--mere-root`, `--mere-persona`); "a GUI persona chooser and live-resident IPC attachment remain future work" (archived lines 32-59).

**Done when:** a writer selects a persona and destination in the window, and attaches to an already running resident without opening a competing owner, with the refusals the attach section lists (missing operation store, absent device identity, unavailable unlock, divergent vault material) shown in words.

### O3. Native capture of the pending-merge card (handoff task 2)

The overmap receipt has no capture of the pending-merge card because the live fixture never makes a conflict. Steps, from the archived handoff (lines 111-137): (1) in Mere, extend `repos/mere/ports/mere-resident/examples/knot_overmap_live_host.rs` to seed one session, branch it (`MereSessions::branch_at`) and make conflicting title edits in the branch and its source, keeping the `PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET` marker and public test keys only; (2) in Knot, add `scenarios/overmap_live_merge.scn` beside `overmap_live.scn` and `overmap_live_reopen.scn`: open the branch's card, press "Merge into source", assert the pending card lists the conflict, choose Source, Apply, assert the source's title changed and the branch reads merged, capturing each step; (3) run it with `run_overmap_live.sh`. As of 2026-11-02 neither the scenario nor the fixture change exists (`scenarios/` holds `overmap_live.scn`, `overmap_live_reopen.scn`, `overmap_standalone.scn`; the Mere example contains no `branch_at`). The fixture change is Mere's work.

**Done when:** the run reports `RESULT ok` with nonblank captures of the pending card and the merged result, recorded under a new `tier-merge-<knot-commit>/` folder in `Code/testing/knot-editor/receipts/2026-10-15_mere_overmap_live/` (the handoff said "committed"; receipts no longer live in the repository, and the runner script needs `KNOT_SRC` set to a checkout). The handoff's native traps (a locked screen gives zero frames, a crash dialog can steal the foreground, foreground runs are serialized with the Woodshed and Cleromancy lanes, `TMPDIR=/tmp` for Unix sockets, no `timeout` on macOS) are archived lines 138-150.

### O4. Re-run the full Knot suite (handoff task 3)

The repin round's report did not re-run `knot-document`, `knot-site`, `font_roles` or `typography_fonts` (archived handoff lines 152-158). The handoff named revision `7ee24f9`, since superseded by later repins, so the done-condition applies to the revision locked when it is run.

The Windows workspace/peer baseline and matching post comparison for the [H7 source pass](2026-11-03_source_splits_plan.md) now exposes existing Readings/recipe-control lookup failures and the explicit Unix-only legacy checkpoint replacement refusal. That source pass owns the baseline/post comparison; it does not run the excluded standalone knot-document and knot-site packages or close O4.

**Done when:** on the locked revision, `knot-editor` with `mere-overlay`, `knot-document`, `knot-site` and `knot-desktop` (lib, bins, `scenario_lane`, `font_roles`, `typography_fonts`) are run, and each suite's count is recorded in the Progress entry of `repos/mere/design_docs/mere_docs/implementation_strategy/2026-10-28_stack_lift_plan.md` for that round.

### O5. Overmap qualification gaps

Left out of the archived overmap records: which mere a default launch should overlay (`archive_docs/2026-11-02/2026-10-15_knot_mere_overmap_layer.md`, "Not done"); owner restart, native merge visuals and compact layout for the shared resident reader; and cross-platform, global keyboard-shortcut, live assistive-technology and device-recovery qualification of catalog folds (same file, "Shared resident materialization" and "Catalog fold curation"). Branches wait on Mere's merge-in design (`repos/mere/design_docs/mere_docs/technical_architecture/2026-10-15_session_merge_in_note.md`).

**Done when:** Mark rules the default-launch choice (a launch flag stays required, or a default is named), and each remaining gap is either given its own dated gate or dropped.

### O6. Dynamics-adoption gates the archived plan leaves

The archived plan records these as open: the original physics coordinator's independent review and coordinator-owned adoption pins, Firefox verification (parked while browser control is blocked), G2's unified semantic channel registry and G4's portable `DynamicsSpec`, persistence and composition pickers, inferred ambient neighborhoods, and unattended background presentation (archived lines 374-375, 728, 764-765).

**Done when:** Mark names which of these Knot wants; each takes its own dated gate with a done-condition, and the rest are dropped.

## Findings

- 2026-11-02: the overmap fixtures and scenarios named in O3 were checked: `scenarios/` has three `overmap_*` scenarios and no merge scenario; `repos/mere/ports/mere-resident/examples/knot_overmap_live_host.rs` exists and has no `branch_at` or conflict seed.
- 2026-11-02: the receipts this plan cites exist in `Code/testing/knot-editor/receipts/` (checked with the receipts file's existence column).

## Progress

- 2026-11-02: plan cut from the composition retention plan; decisions D1 to D4 recorded. The evidence index and old-to-main hash table are in `archive_docs/receipts/2026-09-30_composition_mere_retention_receipts.md`.
