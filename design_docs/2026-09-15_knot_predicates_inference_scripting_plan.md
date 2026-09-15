# Knot predicates, inference readings, and scripted readings plan

**Date:** 2026-09-15
**Status:** Scoped from Mark's three rulings of 2026-09-15. No code. Each
track has done-conditions; the decisions listed under each are his and are
not made here.
**Owner:** Knot Editor

## Ruling

Three tracks, decided together because they share one rule: every one of
them produces a reading or an interchange record, and none of them acquires
authority over source, relations, or files.

1. **Predicate identity.** Design the mapping from a predicate's display
   label to a stable identity before the first authored relation persists.
2. **esp integration.** Graduate Knot's search lanes from the lexical
   hashing provider to real embeddings, and add inference readings and
   suggestions on the same seam.
3. **Rhai readings.** Open a second rhai lane beside the fence evaluator:
   a script authors a reading over typed snapshots and returns rows,
   ranges, and suggestions.

The boundary drawn in the design pass holds: a script or a model may
propose, never assert; a reading is labelled with what produced it; the
writer's explicit command is the only path into the vault.

## What exists

Grounded 2026-09-15 against the tree, so the tracks build on seams rather
than inventing them.

**Relations.** `crates/knot-editor/src/relations.rs` retains an assertion as
a signed operation: id and author hashes, the admitted scope, a `predicate`
string validated only as non-empty and trimmed, subject and object endpoints
bound to a document head hash with a quote and byte position, and
author-only retractions. `sync.rs` folds relation events, keeps unverified
and rejected ones inspectable, and filters reads by admitted document ids.

**Mere's vocabulary.** The graph kernel names a relation vocabulary under
`https://mere.computer/ns/rel#` with sub-kinds including cites, quotes,
summarizes, elaborates, example-of, supports, contradicts, questions,
same-entity-as, hyperlink, user-grouped, and agent-derived. The linked-data
bridge resolves a bare slug or a full IRI against that list and reports
anything else as unrecognized, deferred to a raw-IRI path that is still open
there. The kernel deduplicates statements on sub-kind, predicate, and scope;
the workspace plan already requires that two authors' assertions on the same
endpoints stay distinguishable through that lowering.

**esp in Knot.** `crates/knot-editor/src/search.rs` already builds both
search lanes over esp's `LexicalEmbeddingProvider`, a feature-hashing
embedding with no model, through `SemanticSearch`; the vault lane's dense
index is sealed and reopened through the vault (`store_search_index`,
`load_search_index`); hits are capability-filtered by servitor scope grants.
esp offers the `EmbeddingProvider` trait, a BERT model over burn with ndarray
and WebGPU backends, index persistence through eidetic, `affinity_pairs` as a
clustering signal, and a streaming `InferenceProvider` with an armillary
actor. Its default feature set is serde-only. The August feature matrix is a
compile receipt for native and wasm and says it must be rerun.

**Rhai in Knot.** `script-rhai` implements inker's `BlockEvaluator` over a
shared sandboxed base engine: no file or network builtins, silenced print,
depth caps, and a per-call operation budget. The endpoint registers it behind
`KnotEffectPolicy`, whose run mode defaults to never, with an allowed
language list and a `max_ops` ceiling; a block run is an external-effect
intent and its output is consented, revision-bound derived state. The
desktop registers no evaluator today. The binding template is numen's
`rhai_bindings.rs`: register typed values and constructor functions on the
base engine, and take the script's final expression as the product.

**Snapshots a reading can see.** `KnotDocumentSnapshotV1` (source, format,
text, selection, dirty, write posture), `KnotOutlineSnapshotV1` (heading
label, level, byte span), `KnotFoldSnapshotV1` (container kind, byte span),
`KnotPreviewSnapshotV1` (rendered document and heading rows; blocks do not
yet carry source ranges), and the visible relation projection.

## Track 1: predicate identity

**Standards first.** A survey on 2026-09-15 proposed a layered adoption:
CiTO for predicates, Web Annotation for anchors, PROV-O for attribution and
retraction, Dublin Core Terms for structural relations. Checked against the
tree, most of it is already built on the Mere side:

- **CiTO alignment exists.** `linked-data/src/vocab.rs` aligns every
  recognized sub-kind in three categories: exact (`owl:equivalentProperty`),
  approximate (`rdfs:subPropertyOf`), and Mere-only. Committed rows: cites
  is `cito:cites`, quotes is `cito:includesQuotationFrom`, supports is
  `cito:agreesWith`, contradicts is `cito:disagreesWith`, same-entity-as is
  `owl:sameAs`; summarizes, elaborates, and questions are subproperties of
  `cito:cites`; hyperlink is a subproperty of `rdfs:seeAlso`; user-grouped
  and agent-derived stand alone. The alignment is emitted as quads in a
  vocabulary named graph and dropped on ingest, so instance data is never
  rewritten. That is the survey's second tier, anchored to the 2026-05-22
  statements-over-schema stance.
- **PROV attribution exists.** Export stamps `prov:wasAttributedTo` and
  `prov:generatedAtTime` on statements, names statements under
  `urn:mere:statement`, and puts each scope in its own named graph.
- **Web Annotation is not emitted anywhere yet.** Knot's endpoint is the
  material for it.

Where the survey's rows differ from Mere's table, the difference is a Mere
ruling and is recorded here for the audit, not decided: supports is
`agreesWith` in Mere and `cito:supports` in the survey; contradicts is
`disagreesWith` in Mere while CiTO also has `disputes` and `refutes`;
elaborates is a `cites` subproperty in Mere and `cito:extends` in the survey;
hyperlink is `rdfs:seeAlso` in Mere while `cito:linksTo` is the exact CiTO
term. Knot consumes Mere's table as it stands. A change lands in
`vocab.rs` and its stance doc.

**Three faces.** A predicate has a label, an identity, and a behaviour, kept
apart: the label is what the writer sees and types and may change; the
identity is an IRI an assertion stores and export names; the behaviour is
the Mere sub-kind it lowers to, if any.

**Core predicates** are the writer-facing subset of Mere's vocabulary. Their
identity is the Mere IRI, their behaviour the matching sub-kind, and their
standard alignment comes from Mere's table, not from Knot. Proposed set:
cites, quotes, supports, contradicts, questions, elaborates, example-of,
summarizes. Hyperlink and agent-derived are origins, not choices;
same-entity-as waits for a consumer. The contradicts split the survey
raises is handled by specialization rather than by three writer predicates:
a defined predicate may declare itself a subproperty of `cito:refutes` and
still lower to the contradicts sub-kind.

**Defined predicates** are the writer's own. A definition is its own signed
operation in the space: `KnotPredicateDefinitionV1` with the operation hash
as id, the author, the scope, a slug, a label, an optional description, an
optional inverse, an optional `subproperty_of` naming a core IRI or any
standard IRI, an optional Mere sub-kind to lower to, and an optional
`supersedes`. The IRI is minted once, at first definition, and never
changes:

```
urn:knot:author:<verifying key hex>:rel:<first definition hash hex>
```

The namespace is the author's ed25519 verifying key, because that is what
signs the definition and what a peer verifies. Knot's `PersonaId` is a local
UUID that never leaves the machine and cannot serve. A rename is a
superseding definition with a new label and the same IRI. Retirement is a
superseding definition carrying `replaced_by`; a retired predicate refuses
new assertions and keeps old ones readable under their historical label. A
definition's `subproperty_of` exports as `rdfs:subPropertyOf` in the
vocabulary graph, exactly as Mere exports its own alignments, so a custom
"corroborates" under `cito:supports` still reads to a stranger.

**Assertions** change `predicate: String` to a reference: a core IRI or a
defined IRI. Validation rejects a bare label that is neither, so nothing is
invented at write time. Fixtures that say `supports` map to the core IRI;
that is the only migration.

**Anchors** export as Web Annotation. Each endpoint becomes an
`oa:SpecificResource` whose source is the document at its head, with an
`oa:TextQuoteSelector` and an `oa:TextPositionSelector`. Two corrections to
the survey, both consequential:

- The position selector counts characters, not bytes. Knot's byte offsets
  stay the internal truth and are converted at export; the exported source
  text is the same head the offsets were captured against.
- A quote selector re-anchors on `prefix` and `suffix`, which Knot's
  endpoint does not capture. Add both to the endpoint at the same V2 change
  as the predicate reference, so the endpoint changes once.

Turn 3's reference anchors take the same form when reference endpoints
arrive: a `FragmentSelector` conforming to Media Fragments for `t=` and
`xywh=`, and to RFC 3778 for `page=`.

**Provenance** exports as PROV-O: the assertion `prov:wasAttributedTo` the
author key as `urn:knot:author:<hex>` and `prov:wasGeneratedBy` the signing
operation as `urn:knot:op:<hash>`; the scope is the named graph; a
retraction is `prov:wasInvalidatedBy` its retraction operation. One
correction to the survey: Knot's operations carry no wall-clock time, only
signed causal order, so no `prov:invalidatedAtTime` or `generatedAtTime` is
exported. Export never invents a time. Adding an author-asserted time to the
operation header is possible but is a trust decision, listed below.

**Lowering** into a Mere graph carries the assertion id as the statement id,
so the kernel's content dedup never collapses two authors. That is the G1
proof restated at the predicate level.

*Done when* a fixture with one core and two defined predicates round-trips:
assert; supersede one definition with a new label; export to JSON-LD; load
the export in oxigraph, which the tree already carries; and answer a query
for `cito:agreesWith` statements that returns the core-supports assertion
through the alignment graph and a query for the defined predicate's
superproperty that returns it. Import into a Mere graph shows two
independent authors' assertions on the same endpoints as distinct
statements. The exported quote and position selectors re-anchor the quote
in the exported source at character positions. Validation refuses an
unknown bare label. The display shows the current label on an assertion
signed under the old one. A retired definition cannot be used for a new
assertion. No exported statement carries a time.

**Decisions for Mark.**

1. Namespace form for defined predicates: the author verifying key, as
   proposed; the space id; or a resolvable HTTPS base once a persona has a
   published address. Recommendation: the key now, with an HTTPS alias
   later as a second `owl:equivalentProperty`.
2. The contradicts split: one writer predicate specialized by definitions,
   as proposed; or asking Mere for disputes and refutes sub-kinds.
3. Whether to add an author-asserted time to operation headers so PROV
   times can be exported, accepting that it is claimed, not verified.
4. Whether the core set is exactly the eight above.
5. Whether an inverse is a field on the definition or a reading over
   direction, as G3 treats it.
6. Whether to raise the four mapping differences with Mere's table now or
   leave them for the vocabulary's own audit.

## Track 2: esp integration

Four slices, the last one gated.

**E1 Provider graduation.** A provider choice becomes an app preference:
lexical (the default, portable, no model), BERT on CPU, BERT on WebGPU.
Every index record carries provider identity: model id, dimensions, metric,
and esp revision. A sealed index built under one provider is refused for a
query under another, with a rebuild offered; it is never silently reused.
Building runs on the armillary actor so the interface never blocks, and the
status bar carries `index · building 148/214`. The index remains a
rebuildable, discardable reading, sealed through the vault as today.

**E2 Related passages.** A reading over a selected span or the active
section: nearest neighbours across both lanes, fused by reciprocal rank with
the lexical rank so the model never fully replaces exact terms. Every result
row carries provider, index revision, scope, k, and threshold, and selects a
source range. Access filtering is applied before computation: the candidate
set is the subject's visible set, never the whole index filtered afterwards,
so a hidden document cannot shape a neighbour list. Whether the lexical rank
comes from the hashing provider's sparse output or from eidetic-search's BM25
index is a decision below.

**E3 Suggestions.** `affinity_pairs` over the visible set yields suggested
bins for unfiled captures, which is the Galton underlay in frame 2i, and
suggested relations between passages. A suggestion is a reading with its
parameters attached. Accepting one is the explicit command that creates an
assertion, whose origin records the provider, parameters, and index revision
so the agent-derived lineage survives. Nothing is accepted on the writer's
behalf.

**E4 Inference, gated.** vates and the decoder features are not in this
plan beyond keeping the seam compilable. Drafting and summarising need a
model custody decision first: where weights live, how they are fetched, and
what consent that takes.

*Done when* (1) a headed desktop run builds a BERT index off-thread over the
field-notes fixture directory, the status bar reports progress and provider
identity, and a query against a mismatched sealed index refuses and offers a
rebuild; (2) related passages for a selected span returns fused rows labelled
with provider and revision, selecting source ranges, and a fixture document
the subject cannot read neither appears nor changes the neighbour set;
(3) an accepted affinity suggestion becomes an assertion carrying its origin
and parameters; (4) the esp feature matrix is rerun at Knot's pinned revision
and passes native, with wasm recorded as a compile receipt.

**Decisions for Mark.** Whether lexical stays the default or BERT on CPU
does once weights are present. Model weight custody. Whether to adopt
eidetic-search's BM25 for the lexical rank or keep the hashing provider.

## Track 3: rhai readings

A **reading script** runs on the same sandboxed base engine as the fence
evaluator, under the same effect policy and operation budget, with one
difference: the host registers read-only bindings over typed snapshots, the
way numen registers its field constructors. The script's final expression is
the reading.

**Bindings, first surface.**

- `document()` → address, format, text, selection, revision.
- `outline()` → heading rows with label, level, and byte span.
- `sections()` → each heading with the span of its body.
- `folds()` → container kind and span.
- `links()` → inline links from inker's link walk, with target and span.
- `relations()` → the visible assertions: id, predicate label and identity,
  endpoints, author, active flag.
- `similar(text, k)` → E2's related passages, through the same
  capability-filtered search.
- `row(label, span)`, `note(text)`, `suggest(subject_span, predicate,
  object, why)` → constructors for the result.

No binding writes, fetches, or reaches outside the snapshots. The only
binding that touches a store is `similar`, and it goes through the search
authority's grants.

**Result shape.** A reading is rows, each optionally carrying a source range
or a document reference the panel makes selectable, plus optional text in an
inker format for rendering, plus suggestions. It is derived state labelled
with the source revision, script name, script hash, operation count, and
elapsed time. A source edit marks it stale; rerun rederives it.

**Presentation.** A Readings panel in the right-panel set, listing scripts
by name with Run and Refresh, showing rows as selectable lines in the outline
style, and reporting compile, runtime, and budget errors in the panel.
Suggestions render as the same suggestion rows E3 uses, with Accept as the
explicit command. The desktop registers the evaluator for this lane; the
fence lane's default stays never.

**Where scripts live.** First slice: `.rhai` files in the app preference
directory, listed by the panel. A script carried inside a note as a fence is
a later option and a decision below.

*Done when* three scripts run in the headed desktop over the field-notes
fixture: (1) "headings without a citation" returns rows that select their
source ranges; (2) "contradiction candidates" uses `relations()` and
`similar()` to produce suggestions, and accepting one creates an assertion
whose origin names the script and its hash; (3) a deliberate runaway hits the
operation budget and the panel reports it without a hang. A source edit marks
a reading stale and rerun rederives it. The fence lane is unchanged. A script
that attempts file or network access fails to resolve the call, proven by a
test against the binding set.

**Decisions for Mark.** Whether scripts live only in the preference
directory, also in notes as fences, or both. Whether readings rerun
automatically on edit within budget or only on demand. Whether this is a
Readings panel or folds into Lenses.

## Sequencing

Track 1 first, because G1 persistence waits on it. Track 3's first bindings
do not depend on it and can proceed against current snapshots. Track 2's E1
is independent. E3 and the `suggest` binding both need Track 1's predicate
reference and a shared suggestion sink, so they land last.

1. Track 1 design recorded and the definition operation implemented.
2. Track 3 with `document`, `outline`, `sections`, `folds`, `links`,
   `relations`, and the Readings panel.
3. Track 2 E1 and E2, adding `similar` to Track 3.
4. Track 2 E3 and Track 3 `suggest`, on one suggestion sink.

## Stop rules

- No script or model writes an assertion, a source, or a file. Acceptance
  is a command with provenance, always.
- No query language is exposed to the writer. Saved questions stay bounded
  records; scripts are readings, not queries over the vault.
- No RDF store. JSON-LD is interchange. The raw-IRI export path is a Mere
  gap, filled there.
- Access filtering before computation, for search, affinity, and every
  script binding.
- Every reading carries its provenance labels. An unlabelled reading is a
  defect.

## Open, because

- Mere's linked-data raw-IRI path is needed for defined-predicate export and
  lives in mere.
- Preview blocks lack source ranges (A2), so `sections()` is heading-based
  until they do.
- Model weight custody is undecided, which gates E4 and the BERT default.
- wasm is not a claimed Knot target; the matrix rows are compile receipts.

## Related material

- [Workspace plan](2026-09-05_knot_application_workspace_plan.md): ownership
  gate, G1 to G3, settings boundaries.
- [Design pass brief](2026-09-13_knot_design_pass_brief.md) and the design
  project's turn 3: the rendering and authority boundary.
- Mere: `crates/script/rhai`, `crates/conatus/numen/src/rhai_bindings.rs`,
  `crates/graph/linked-data/src/statements.rs`,
  `crates/graph/linked-data/src/vocab.rs` (the CiTO alignment table),
  `crates/graph/graph-kernel/src/graph/edge_data.rs`, `crates/intel/esp`,
  `design_docs/intel_docs/technical_architecture/2026-08-09_feature_target_matrix.md`.
