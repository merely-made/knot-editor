# Knot publishing protocol: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut from [the publishing protocol plan](archive_docs/2026-11-02/2026-08-07_knot_publishing_protocol_plan.md), archived whole with Phase A's contract, work plan and receipts. Phase A is implemented and physically receipted (renewed 2026-08-19, archived plan lines 201-208) and runs over secure Notochord v2 admission since 2026-10-29 (lines 38-60). Open: Phase B, the Turnstone named-recipient gate, and three residuals carried from the archived Graphshell plan.

## Rulings in force

The direction "A, then B" and the Phase A contract, work plan, required receipts, out-of-scope list and stop rules are in the archived plan (sections 4 to 10) and stay in force, cited by those section numbers: they are grandfathered and keep their numbers. DOC_POLICY section 11 applies forward from 2026-11-02, so new decisions are keyed `publishing_protocol D<n>`. Mere's S14 ruling S61 (2026-10-06) made this lineage canonical and gave it Phase B (archived plan lines 589-594).

## Decisions

### publishing_protocol D1: cut the plan, draft first (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The 617-line plan is cut under DOC_POLICY section 8: its open items become this plan, and the old plan is archived whole after this draft is reviewed.

Links: area publishing · replaces 2026-08-07_knot_publishing_protocol_plan · summary old plan cut to open work and archived whole

### publishing_protocol D2: old hashes (2026-11-02)

Mark, 2026-11-02: "Same: live docs only". This plan cites main hashes; the archived plan keeps the ones it was written with. Its mapping is in [the receipts file](archive_docs/receipts/2026-08-07_knot_publishing_protocol_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes; archived docs keep old ones

### publishing_protocol D3: Knot's copy is canonical over Mere's twin (2026-11-02)

Mark, 2026-11-02: "Knot canonical; stub mere". Mere's archived copy of this plan (`repos/mere/design_docs/archive_docs/2026-10-06_completed_plans/2026-08-07_knot_publishing_protocol_plan.md`) is a twin. The Mere-side stub is Mark's to write.

Links: area documentation · draws on DOC_POLICY addendum · summary this repository's copy is canonical

## Open work

### O1. Phase B: the stable contract

Phase B is not entered (archived plan lines 561-566). It starts when Phase A's grammar has stopped changing under real use and a second party intends to implement it (lines 229-231). There is no `knot-protocol` crate (`crates/` holds knot-capture, knot-composition, knot-document, knot-editor, knot-file-catalog, knot-readings and knot-site), and the bounded versioned codec in `crates/knot-editor/src/publish_wire.rs` is still an implementation detail, not a published grammar (archived plan lines 429-430).

One question blocks reading the entry condition: the [Mark read adapter](2026-08-08_knot_mark_read_adapter.md) records the Phase B comparison choice as "bounded read-only projection beside the native lane", and Mere's pass asked whether that adapter counts as Phase B's entry (archived lines 589-594). That adapter has not yet produced its external Demarkus-client receipts. This is Mark's call.

**Done when:** (a) Mark rules whether the Mark adapter counts as Phase B's entry, recorded as a decision here; and, once the entry conditions hold, (b) the archived plan's section 9 steps are met in order: the Phase A packet and refusal fixtures are frozen as a corpus, the protocol document is written (transport and ALPN, Noise and Notochord roles, addressing and tickets, media types, frames, limits, catalog visibility, version identity, digest, cache rules, refusal privacy, revocation, inert document handling), `knot-protocol` is extracted for the codec and conformance vectors only, and a second implementation passes the vectors with every ambiguity it exposes corrected.

### O2. Turnstone named-recipient gate

Turnstone's next consumer chooses one saved document and each saved-source attachment independently. The inspected `KnotPublishSource` comes from the startup-unlocked signed vault owner, whereas Turnstone's research note uses a directory endpoint, so the single vault-document read is only the authority baseline (archived lines 18-36). The gap assessment, disclosure requirements and acceptance cases live once in `repos/turnstone/design_docs/2026-09-06_page_lifecycle_plan.md`, section "Named-recipient sharing boundary, 2026-10-19" (line 980 there, checked 2026-11-02).

**Done when:** the source-owning resident supplies an admitted, saved-revision-bound selected publication with durable selection and signed withdrawal, and Turnstone's consumer qualifies cold reopening against it. The host does not reopen the resident's operation store, and evidence transfer is not inferred from a retained reference.

### O3. Residuals carried from the archived Graphshell plan

None of these is ruled; each needs Mark to put it in or drop it.

- **Relay routing.** The acceptance ladder's rung 4 through the configured relay route is "a later deployment receipt, not a prerequisite for LAN publishing" (archived plan lines 488-492). **Done when:** a Phase A fetch and a post-revocation refusal pass across the configured relay route, with the holder and reader identities distinct.
- **K2 reverse-direction reachability.** The Windows-holder to Q-PC-visitor direction timed out before admission and left the file unchanged; the route was ticketed and direct-versus-relay selection was not instrumented (`archive_docs/2026-11-02/2026-08-02_knot_in_graphshell_plan.md`, "Physical receipt (2026-08-08)"). **Done when:** the cause is found with instrumented route selection and the reverse direction passes or is recorded as a platform limit.
- **Writer admission by allowlist.** On 2026-08-02 the archived Graphshell plan found that `KnotSyncStore` admits writers from a fixed set and rejects others as `unrecognized-knot-writer`; its answer was a Gemot capability query over a Knot lane in the place's lane set ("What actually remains", same file). Today the set is a `BTreeSet<[u8; 32]>` behind a lock (`crates/knot-editor/src/sync.rs:306`) that `admit_writer` (`sync.rs:683`) extends and the check at `sync.rs:332` consults. The crate has since gained Gemot constitution and delegation facts for communal spaces (`crates/knot-editor/src/authority.rs:21-22`, `:93`), so the residual may already be narrower than the 2026-08-02 text says; that was not re-traced. **Done when:** the remaining gap between place membership and the writer set is stated, and either a Moot-scoped Knot lane closes it or Mark rules that offline co-authoring in a place is not wanted.

## Findings

- 2026-11-02: Phase A's done-condition is met by the 2026-08-19 renewal: a ThinkPad holder and Q-PC reader, the reader built from mere `12c260d6e09` and using `fetch_published_document`, with the post-revocation refusal recorded by the holder (archived plan lines 201-208). The earlier physical receipts describe their original admission version and do not qualify the v2 transcript (lines 38-48).
- 2026-11-02: the publishing implementation is `crates/knot-editor/src/publish.rs`, `publish_carrier.rs`, `publish_client.rs`, `publish_host.rs` and `publish_wire.rs`.

## Progress

- 2026-11-02: plan cut from the publishing protocol plan; decisions D1 to D3 recorded. The evidence index and old-to-main hash table are in `archive_docs/receipts/2026-08-07_knot_publishing_protocol_plan_receipts.md`.
