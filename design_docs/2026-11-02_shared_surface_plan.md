# Knot shared surface and port contribution: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut from [the shared surface plan](archive_docs/2026-11-02/2026-08-24_knot_shared_surface_and_port_contribution_plan.md), archived whole with its findings, lanes and receipts. G0, K0, S0a/S0b, T0 and P0 are done and the contract is frozen at v1 (archived plan section 7, lines 696-707; Status at line 17); F0 continues in [the application workspace plan](2026-11-02_application_workspace_plan.md). Open: Hest H3 and H4 native acceptance, and one receipt carried from the archived authoring consumer plan.

## Rulings in force

The archived plan's section 1 ruling, section 3 boundary (data-only description, retained Cambium session, Knot document model), section 5 ownership rules and section 6 stop rules stay in force, cited by those numbers and grandfathered. The F-1 to F-4 freeze rulings (F-1 unify, F-2 cut all four, F-3 keep all three, F-4 keep all eight; archived lines 696-707) are part of the frozen v1 contract. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `shared_surface D<n>`.

## Decisions

### shared_surface D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all to Code/testing". The Hest H3 and H4 receiving qualifications (`hest-h3-20261020/qualification.json`, `hest-h4-20261020/qualification.json`) now live under `Code/testing/knot-editor/receipts/`, cited as paths.

Links: area receipts · applies DOC_POLICY section 8 · summary Hest receipts moved to Code/testing/knot-editor

### shared_surface D2: cut the plan, draft first (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The 797-line plan is cut under DOC_POLICY section 8 and archived whole after this draft is reviewed.

Links: area documentation · replaces 2026-08-24_knot_shared_surface_and_port_contribution_plan · summary old plan cut to open work and archived whole

### shared_surface D3: old hashes (2026-11-02)

Mark, 2026-11-02: "Same: live docs only". This plan cites main hashes; the archived plan keeps its own. Mapping: [the receipts file](archive_docs/receipts/2026-08-24_knot_shared_surface_and_port_contribution_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes; archived docs keep old ones

### shared_surface D4: Knot's copy is canonical over Mere's twin (2026-11-02)

Mark, 2026-11-02: "Knot canonical; stub mere". Mere's archived copy (`repos/mere/design_docs/archive_docs/2026-10-06_completed_plans/2026-08-24_knot_shared_surface_and_port_contribution_plan.md`) is a twin; the Mere-side stub is Mark's.

Links: area documentation · summary this repository's copy is canonical

## Open work

### O1. Hest H3: native window-manager acceptance

H3 receiving is qualified in software on Mere `c9033723b64`: desktop library 342 passed (one existing ignore), the actual `command_chrome` harness 9 passed, and the unchanged standalone document export with the engine enabled 66 passed (archived lines 745-777; the numbers at line 762). The portable window queue carries Hest command ids, versioned opaque arguments and declared effects; admission is pending and is not reported as terminal `Done`. The archived plan says native window-manager acceptance and physical platform review "remain separate".

**Done when:** a native run under a real window manager sends actual caption-control and resize invocations into the host-supplied queue and drains them through the existing event-source and close-policy seam, with a malformed contract leaving the queue empty, recorded as a receipt with the machine and commit.

### O2. Hest H4: native presentation and assistive-technology review

H4 receiving is qualified in software: three receiving tests plus the consent/freshness and budget cases pass against the actual writable endpoint and derived document, with the opt-in `rhai-command` evaluator producing request data and authored bytes exact (archived lines 780-801; the tests at line 793). The plan leaves "native presentation/human AT, MSRV and backend publication" separate, and actual command enactment is Graphshell's host gate, owned by Mere.

**Done when:** a native run shows the command-producing fence's run action working for a consenting writer and absent for an excluded language, with a human assistive-technology review, plus an MSRV (1.97.1) check of the receiving tests. Mere's Graphshell host gate stays Mere's.

### O3. Selected-clip receipts from the Livery and scripted producers

Carried from the archived authoring consumer plan. Static selected-range clipping is complete; Livery and scripted documents remain producer-specific seams (`archive_docs/2026-11-02/2026-07-27_knot_authoring_consumer_plan.md`, "Selected-range clipping. Static complete; Livery and scripted remain" and its 2026-10-06 note). Knot accepts and preserves an explicit selector and falls back to `selector: None` where the producer cannot select.

**Done when:** once Livery retains source spans and layout for pointer mapping and scripted documents produce a retained selection, an optional headed Turnstone receipt shows a selected clip from each producer inserted with its selector.

## Findings

- 2026-11-02: Knot's side of the F-1 descriptor follow-through is done: `knot_document_descriptor()` (`crates/knot-document/src/document_view.rs:191-197`) carries only the provider id, surface id, label and `accepted_source`, the shape the freeze left (archived lines 696-707). The pointer-capture routing gap that the archived plan left "for the T lane" is Turnstone's, not re-checked here.
- 2026-11-02: `knot-document` now allows Cambium `>=0.4.0, <0.7.0` (`crates/knot-document/Cargo.toml:34`), wider than the `>=0.4.0, <0.6.0` the 2026-10-20 note records.
- 2026-11-02: the audit read the archived plan's section 8 done-conditions (lines 712-726) against its section 7 Progress entries and found each evidenced; that reading was not repeated line by line here. F0, the only lane left, is owned by the application workspace plan.

## Progress

- 2026-11-02: plan cut from the shared surface plan; decisions D1 to D4 recorded. The evidence index and old-to-main hash table are in `archive_docs/receipts/2026-08-24_knot_shared_surface_and_port_contribution_plan_receipts.md`.
