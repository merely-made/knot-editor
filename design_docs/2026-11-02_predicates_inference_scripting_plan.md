# Knot predicates, inference readings and scripted readings: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut from [the predicates plan](archive_docs/2026-11-02/2026-09-15_knot_predicates_inference_scripting_plan.md), archived whole with Track 1 (predicate identity, landed 2026-09-15), Track 3's first slice (the rhai reading lane, landed 2026-09-15) and E1 (provider graduation, landed 2026-09-16). Open: E2 (related passages), E3 (suggestions), E4 (inference, gated), the rest of Track 3, E1's headed receipt, and Knot search on the hybrid seam carried from the archived Graphshell plan.

## Rulings in force

The archived plan's Ruling section (three tracks; a script or model may propose and never assert; a reading is labelled with what produced it), its Track 1 rulings (predicate identity, `did:key` minting, asserted time, the strictness setting), the 2026-09-16 rulings for Track 2 (lexical stays the default provider; BERT is a preference; weight custody is the writer's, verified by digest; a BERT index's model identity is its weights digest; Burn stays at `0.22.0-pre.3`) and Track 3 (a script may live in the preference directory or in a note, and an in-note script runs only after per-document consent), and its stop rules stay in force, cited by section and grandfathered (archived lines 11-29, 254-339, 393-402, 416-428). DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `predicates_inference_scripting D<n>`.

## Decisions

### predicates_inference_scripting D1: cut the plan, draft first (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The 566-line plan is cut under DOC_POLICY section 8 and archived whole after this draft is reviewed.

Links: area documentation · replaces 2026-09-15_knot_predicates_inference_scripting_plan · summary old plan cut to open work and archived whole

### predicates_inference_scripting D2: old hashes (2026-11-02)

Mark, 2026-11-02: "Same: live docs only". This plan cites main hashes; the archived plan keeps its own. Mapping: [the receipts file](archive_docs/receipts/2026-09-15_knot_predicates_inference_scripting_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes; archived docs keep old ones

### predicates_inference_scripting D3: Knot search on the hybrid seam moves here from the Graphshell plan (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The archived Graphshell plan's search section (approved 2026-08-02) is not built, and E2's lexical-rank decision is the same question, so both live in O2.

Links: area search · draws on 2026-08-02_knot_in_graphshell_plan · summary hybrid search carried into E2

## Open work

### O1. E1: the headed receipt and the feature matrix

E1 is implemented and receipted without a UI: the provider preference, the sealed `KnotIndexIdentity`, the refusal-and-rebuild behavior and a real-weights test (archived lines 495-512). Its headed receipt "waits on a desktop search surface", and the desktop preferences file passes the embedding section through uninterpreted (archived lines 505 and 522; `apps/desktop/src/preferences.rs:11,45-47`). `apps/desktop/src` does not reference `SearchProgress` (searched 2026-11-02), so no status-bar progress or desktop search surface exists.

**Done when:** a headed desktop run builds a BERT index off-thread over the field-notes fixture directory, the status bar reports progress and provider identity, and a query against a mismatched sealed index refuses and offers a rebuild; and the esp feature matrix is rerun at Knot's pinned revision and passes native, with wasm recorded as a compile receipt (archived lines 305-314, items 1 and 4).

### O2. E2: related passages, with hybrid search

E2 is a reading over a selected span or the active section: nearest neighbours across both lanes, fused by reciprocal rank with the lexical rank, every row carrying provider, index revision, scope, k and threshold and selecting a source range, with access filtering applied before computation (archived lines 282-291). **Open decision:** whether the lexical rank comes from the hashing provider's sparse output or from eidetic-search's BM25 index (lines 336-338).

The archived Graphshell plan approved the same seam on 2026-08-02 and it is unbuilt: Knot's search ran one engine (hash-bucket embeddings) wearing lexical clothing, so there was nothing to fuse; `crates/knot-editor/src` contains no call to `fuse` (searched 2026-11-02). Its steps were **S0**, a real lexical index over Knot documents (BM25, tantivy, keyed by something stable enough to fuse on that distinguishes a file in place from a vault document, with the choice between generalising the trail index and a Knot-specific index made inside `eidetic-search`, not in Knot), and **S1**, `fuse` over the lexical and semantic rankings with `k` and the weights exposed as settings (`archive_docs/2026-11-02/2026-08-02_knot_in_graphshell_plan.md` lines 398-456, S0 at 430, S1 at 445).

**Done when:** (a) Mark rules the lexical-rank source; (b) a Knot query returns hits neither engine ranked first alone and the weights demonstrably move the ranking; (c) related passages for a selected span returns fused rows labelled with provider and revision, selecting source ranges, and a fixture document the subject cannot read neither appears nor changes the neighbour set (archived lines 305-314, item 2).

### O3. E3: suggestions and the suggestion sink

`affinity_pairs` over the visible set yields suggested bins for unfiled captures and suggested relations between passages; a suggestion is a reading with its parameters attached, and accepting one is the explicit command that creates an assertion whose origin records the provider, parameters and index revision (archived lines 292-299). E3 and the `suggest` binding share one suggestion sink and land last (lines 403-415).

**Done when:** an accepted affinity suggestion becomes an assertion carrying its origin and parameters (archived lines 305-314, item 3), and nothing is accepted on the writer's behalf.

### O4. E4: inference, gated

vates and the decoder features are not in scope beyond keeping the seam compilable; drafting and summarising "need a model custody decision first: where weights live, how they are fetched, and what consent that takes" (archived lines 300-304).

**Done when:** Mark rules model custody for generated text, after which E4 takes its own dated gate with done-conditions.

### O5. Track 3 beyond its first slice

The first slice has `document`, `outline`, `sections`, `folds`, `links`, `relations`, `span`, `row` and `note`, the Readings tile, provenance labels and the sandbox receipts (archived lines 474-492; `crates/knot-readings`, `apps/desktop/src/readings.rs`). Not built: the `similar` binding (needs E2), the `suggest` binding (needs E3), and the second and third scripts of Track 3's done-condition. Still open from the plan: whether readings rerun automatically on edit within budget or only on demand, and whether this stays a Readings panel or folds into Lenses (archived lines 393-402).

**Done when:** three scripts run in the headed desktop over the field-notes fixture: "contradiction candidates" uses `relations()` and `similar()` to produce suggestions, and accepting one creates an assertion whose origin names the script and its hash; a deliberate runaway hits the operation budget and the panel reports it without a hang; and Mark rules the rerun policy and the panel question (archived lines 383-392).

## Findings

- 2026-11-02: Track 1 is in the code: predicate definitions, signed `DefinePredicate` operations and asserted time (archived lines 440-473; `crates/knot-editor/src/predicates.rs`, `relations.rs`).
- 2026-11-02: the link-model P2 continuation (optional signed retraction time, `withdrawn_at_ms`) is recorded in the archived plan's last section and consumed by Mere's Djinn; desktop authoring and automatic resident synchronization of relations are separate work (archived lines 544-568).
- 2026-11-02: open "because" items in the archived plan still stand: Mere's raw-IRI export path for defined predicates, preview blocks without source ranges (A2), model weight custody, and wasm not being a claimed Knot target (archived lines 429-437).

## Progress

- 2026-11-02: plan cut from the predicates plan; decisions D1 to D3 recorded. The evidence index and old-to-main hash table are in `archive_docs/receipts/2026-09-15_knot_predicates_inference_scripting_plan_receipts.md`.
