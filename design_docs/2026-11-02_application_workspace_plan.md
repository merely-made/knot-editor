# Knot application workspace: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut from [the application workspace plan](archive_docs/2026-11-02/2026-09-05_knot_application_workspace_plan.md), archived whole with its appearance adoption, A1 and A2 history, the multi-document sequence and the 530-line findings log. The multi-document workspace, recovery and navigator were delivered by [workspace slice 1](archive_docs/2026-11-02/2026-09-23_knot_workspace_slice1_plan.md) and the slices after it. Open: A3, A4, G1 to G3, headed IME and large-document acceptance for A1/A2, and one broker-routed capture item.

## Rulings in force

The archived plan's ruling (section "Ruling"), workspace layout, product cuts, the connected-writing ownership gate, writing-task acceptance, settings and recovery boundaries, Cambium adoption boundary and stop rules stay in force, cited by section and grandfathered. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `application_workspace D<n>`.

## Decisions

### application_workspace D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all to Code/testing". The application-appearance acceptance ledger moved from `design_docs/validation/` to `Code/testing/knot-editor/validation/2026-10-10_tabard_application_acceptance.json`; the receipt folders it names are in `Code/testing/knot-editor/receipts/`.

Links: area receipts · applies DOC_POLICY section 8 · summary acceptance ledger and receipts moved to Code/testing/knot-editor

### application_workspace D2: cut the plan, draft first (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The 1,516-line plan is cut under DOC_POLICY section 8 and archived whole after this draft is reviewed.

Links: area documentation · replaces 2026-09-05_knot_application_workspace_plan · summary old plan cut to open work and archived whole

### application_workspace D3: old hashes (2026-11-02)

Mark, 2026-11-02: "Same: live docs only". This plan cites main hashes; the archived plan keeps its own. Mapping: [the receipts file](archive_docs/receipts/2026-09-05_knot_application_workspace_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes; archived docs keep old ones

## Open work

Each item keeps the done-condition the archived plan gave it. The constraints that bind all of them are the archived plan's: no new `.knot` format, RDF database, generic query language or replication protocol; one durable owner per fact; a reading proposes and the writer's explicit command is the only path into the vault.

### O1. A1/A2 headed IME and large-document acceptance

The archived plan says larger-document responsiveness and "headed typography, caret, and IME acceptance still need measured receipts before this phase is called complete" (archived lines 263-267), and its A1 and A2 done-conditions require IME-capable input and "a realistically large document probe" (lines 216-222, 364-370). The Scroll authoring record also leaves a headed IME receipt separate (`archive_docs/2026-11-02/2026-09-11_scroll_site_authoring_plan.md`, "Verification receipt").

**Done when:** a headed run edits a scratch Djot document through the platform IME (preedit and commit), saves and reopens it exactly, and a large-document probe records input latency and source mapping; Mark names the document size and the latency bound the probe must meet.

### O2. A3: References and lexical lenses

A `References` panel for the active source range and a `Lenses` panel for Rosette (archived lines 380-415). `apps/desktop/src` has no reference to Rosette (searched 2026-11-02), and the Readings tile (`apps/desktop/src/readings.rs`) is the rhai reading lane, not these panels.

**Done when:** a retained clip can be inspected from source selection through a verified evidence read, while a denied or missing store is visibly distinct; the same source span round-trips through a Web Annotation selector; and a Rosette list or graph interaction focuses the intended source range while unresolved tokens remain reported rather than inferred (archived lines 410-414).

### O3. A4: Changes, drafts and sharing

The Changes tile today shows a buffer against its file on disk and the saved revision a reviewer would retain (`apps/desktop/src/changes.rs:6-9`). The archived A4 scope is wider: the causal head, pending operations, automatic merges and concurrent versions with source-first comparison and an explicit resolve command; titled drafts as a design gate; and a `Share` panel for a selected publishable document (archived lines 416-448). Titled drafts must first settle whether a draft is a new signed event family or a local grouping, and how it rebases.

**Done when:** a two-writer receipt shows a clean automatic merge and a real conflict in the same document workspace, then an explicit accepted resolution or refusal; a sharing receipt creates a least-privilege, pinned-head ticket for one selected revision, reads it as the recipient, revokes it, and shows the new availability without exposing vault keys or local paths; a titled-draft slice begins only after its representation and rebase behavior are recorded and tested (archived lines 442-448).

### O4. G1: links, passages and inspectable relations

The ownership and durable-representation gate comes first: trace the first authored relation's owner and storage path, pick one durable owner per fact, and record an explicit mapping for user-defined predicates (archived lines 473-506; predicate identity landed in Track 1 of [the predicates plan](2026-11-02_predicates_inference_scripting_plan.md)). Source-authored document links and bounded backlinks are delivered (`archive_docs/2026-11-02/2026-09-29_knot_document_links_slice.md`); signed-assertion authoring from the desktop is not confirmed.

**Done when:** an essay links to two source documents and selected passages and shows a support and a contradiction as separately inspectable assertions; renaming one source and editing one target passage leaves references resolved or visibly unresolved in an exact state; restarting and rebuilding derived indexes keeps authored relations; and revoking access to one source leaves backlinks and counts that do not disclose its private facts (archived lines 759-764).

**Broker-routed capture intent (carried within G1).** The archived plan specifies a bounded Knot capture intent on the admitted resident route, with the signed-receipt return contract settled first, because `chirograph::IntentResult` carries no payload (archived lines 700-724). The Progress it records is the in-process resident port (`KnotResidentRetainPort`, line 660), not this route; whether the route was built was not traced. **Done when:** an admitted desktop retains and retries through that route, a revoked or unadmitted client cannot write, and reconnect or restart preserves one store owner and the signed receipt.

### O5. G2: saved questions and coordinated presentations

Archived lines 766-794. **Done when:** one saved question reopens, both views show identical permitted result membership, and editing a relation in either view updates the other; a refused stale edit preserves the current facts and the proposed change for review; a layout-only drag leaves semantic relations unchanged; and an empty, truncated, unavailable or stale result has a distinct explanation (lines 789-794).

### O6. G3: explainable graph operations

Archived lines 795-820. **Done when:** a known fixture gives an independently expected path and support-coverage result; results disclose scope and limits; a relevant source edit invalidates the old result; unauthorized material does not influence the visible output; and cancellation and stale asynchronous completion do not stall or replace the active document view (lines 816-820).

## Findings

- 2026-11-02: the multi-document workspace is delivered: `apps/desktop/src/documents.rs` (the `DocumentWorkspace`), `navigator.rs`, `recovery.rs` and `collapse.rs`, with their acceptance records archived (`archive_docs/2026-11-02/`).
- 2026-11-02: the application appearance adoption (Tabard workshop inside the desktop) is qualified on macOS only; Windows and Linux presentation, live screen-reader and IME, and the native export chooser were not exercised (archived lines 62-72). The acceptance ledger is `Code/testing/knot-editor/validation/2026-10-10_tabard_application_acceptance.json`.
- 2026-11-02: the owner side of the saved-source research note intake is qualified (`knot-editor --lib` 154 passed, 0 failed, 1 ignored, archived "Saved-source research note intake"); the consumer's headed cold-return acceptance is tracked in `repos/turnstone/design_docs/2026-09-06_page_lifecycle_plan.md`, not here.
- 2026-11-02: the archived findings log calls a "recurring aggregate loopback timeout" open (line 1095; a test-reliability item, no product claim); it was not re-run.

## Progress

- 2026-11-02: plan cut from the application workspace plan; decisions D1 to D3 recorded. The evidence index and old-to-main hash table are in `archive_docs/receipts/2026-09-05_knot_application_workspace_plan_receipts.md`.
