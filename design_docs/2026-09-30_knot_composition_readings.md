# Composition readings: lexical connections, sound, and collected material

**Retention correction:** [Knot's own mere](2026-09-30_composition_mere_retention.md)
is now the collection authority. The plaintext-store description and native
receipts below record the first prototype, not the corrected storage route.
The sidecar is retained only as an explicit legacy import source.

Status: first implementation slice, with model/integration checks and a scoped
native sound-and-collection capture. This is not a complete third-party plugin
system or acceptance of every reference workflow and viewport.

## Ownership and opt-in behavior

The three reading surfaces share a selection but do not share authority:

| Concern | Owner | Retention |
| --- | --- | --- |
| Reference editions, source choices, lexical entries and connections | Mere `reference-data` | Local, versioned data packs and enabled-source choices |
| Sound analysis and scripted readings | `knot-readings`, using Mora and CMUdict | Derived results; retained only by explicit collection |
| Personally selected passages, senses and analysis notes | `knot-composition` | A separate authored collection, independent of open tabs and reference installation |

The desktop composes these in Readings. No document text is transmitted. Opening
the surface does not run analysis or enable sources. Startup does not create a
composition store. Explicit lookup, collection or source-management actions
initialize its local storage. Open English WordNet is an inert suggestion in
the source panel, not a reserved manifest that could conflict with a selected
edition or subset. Imports start disabled. The bundled CMUdict
provider must be enabled explicitly each launch. Each sound layer starts off.

## Connected lexical reading

Choose **Sources**, import a normalized local pack with its expected entries
BLAKE3 digest, then explicitly enable it. **Lexical** supports typed lookup or
the exact selected word/phrase, definitions, examples, pronunciations, typed
connections, traversal history and collecting a particular sense. Lookup is
exact and case-sensitive in this slice: it does not infer a lemma, translate,
disambiguate a sense or tokenize a selected passage into separate queries.

Reference identities retain source, version and entry. Sense-targeted
connections retain the target sense, and WordNet concepts retain their synset
identity. The app shows at most 32 query entries; traversal history is bounded
to 64 steps. Changing the document or selection marks earlier selection-derived
results as such; it does not silently reinterpret them.

Registry packs are data, not scripts. Their schema and manifest carry language,
features, edition, license, attribution and an optional upstream address.
An expected digest verifies the selected content; it does **not** authenticate
its publisher or establish license compatibility. Review provenance and terms
before importing a pack. Registry validation rejects unsafe identifiers,
duplicate identities, broken targets and configured resource-limit violations.
An installed edition does not get silently replaced by another edition.

### Offline WordNet conversion

The companion `knot-wordnet-import` command converts a bounded subset of a local,
uncompressed WN-LMF XML file into the normalized pack format. It does not fetch
data. Supply the source/version matching the XML lexicon and explicit lemmas;
the converter includes their one-hop connections and reports omitted relation
types. Example (check the input's lexicon/version before substituting values):

```sh
cargo run -p knot-composition --bin knot-wordnet-import -- \
  --input /absolute/path/english-wordnet.xml \
  --output /absolute/path/my-wordnet-subset.json \
  --source oewn --version 2025 --lemma tide --lemma estuary
```

Use the printed entries digest in the desktop import field. Output is a new
file, not an overwrite. A full WordNet distribution is not a promised runtime
import size: this first adapter deliberately builds selected subsets. Consult
the command's usage for the supported relation and resource bounds.
Choose the desired lemmas together before installation: two different subsets
under the same source/version are conflicting immutable packs, not additive
updates. Expanding an installed subset needs an explicit future replacement or
distinct-pack workflow; the importer does not silently merge them.

## Sound-pattern reading

Enable bundled CMUdict, choose layers, select source text and use **Read selected
sounds**. Perfect rhyme, slant rhyme, assonance, alliteration and candidate meter
are independent switches. Changing a layer invalidates the old result rather
than automatically rerunning it. Alternative pronunciations remain selectable;
the first pronunciation is explicitly identified when used as a default.

Unknown words remain unresolved. Incomplete pronunciation coverage suppresses
whole-selection meter. Meter is a candidate lexical-stress reading, not an
assertion about the author's intended performance. The first slice analyzes
the selection as a whole, not a separate scansion for each poetic line.

The analyzer rejects invalid UTF-8 boundaries and excessive selections (16 KiB,
128 tokens, and 4,096 candidate pairs). The panel shows at most 256 connections
and discloses that limit. Collected sound notes retain the selected quotation,
document hash, byte range, layer choices, alternatives and CMUdict/Mora provenance.

These are selectable result layers. Source-editor color overlays and an Illume
legend/priority compositor are **not yet implemented**. They need one shared
range-mapping and conflict policy rather than independently coloring source
text from each analyzer. Plain text labels remain necessary alongside color.

## Durable collection

The collection lives beside desktop preferences under
`composition/collection.json`; reference data lives separately under
`composition/references`. It is local plaintext, not encrypted or synchronized.
The collection retains copies, so disabling or removing access to a source does
not erase previously collected text. The schema supports kinds, named
collections, ordering, tags, author notes and source references; the current
desktop exposes collection and review, not a full organization editor.

Document anchors retain the exact quotation, UTF-8 byte range, original address
and full-source BLAKE3 hash. **Return to source** requires the original document
to be open and unchanged. It never guesses a nearby location after edits.
Scratch quotations remain useful copies but cannot be returned to by an
ambiguous scratch address; save the document before collecting a navigable
anchor. Sense provenance records `entry#sense` within an exact pack edition.

Writes use a same-directory temporary file, synchronization, atomic publication,
and concurrency checks. Malformed stores are reported rather than replaced by
empty collections. Keep normal backups: atomic publication is not a backup.

## Scripting and further extension

`ReadingBackend` passes typed copied input and owned output independently of
runtime value types. The existing Rhai reading evaluator is the first adapter;
its operation budget remains enforced. A backend descriptor identifies its
implementation and whether budgeting is native, externally isolated or absent.
Implementing this Rust trait is not itself sandboxing untrusted code.

Genet's existing engine interface also owns DOM realms, callbacks and jobs.
Lexical and sound workers need not implement that interface merely to analyze
text. Python is worth evaluating as an optional, separately isolated analysis
worker, particularly for linguistic libraries; it is not introduced as a new
Genet DOM runtime here. Boa, Piccolo, Vano/Rune and Python adapters require their
own authority and budget tests before being exposed as user-selectable backends.

Next slices are source-overlay composition, collection organization, additional
language/source adapters, and trusted lifecycle management for third-party
modules. Neither a network catalog installer nor a recommendation/default-source
policy is part of this local, explicit first slice.

## Verification

Shared Mere is pinned to `9310518be5423862c1038f7d7a7578d5089d3c51`; Genet remains
`b1eb3af197111ecde34421bbe1bad2e6024ca564`. No relative shared-crate path override
remains in the committed manifest. That shared revision is now published and
merged into Mere's main branch.

- Shared registry owner reported 24 tests, all-target strict Clippy and a wasm32
  compile check passing on its committed source.
- Final `cargo test --locked --offline --workspace`: **539 passed, 0 failed,
  2 ignored**. The ignored cases are the existing long-outline timing probe
  and the opt-in real-upstream pack test; the latter was run separately and
  passed against the final generated pack.
- Desktop composition checks cover exact sense traversal and provenance,
  stale/cross-document selections, scratch-address refusal, malformed stores,
  lazy startup and durable collection without source mutation.
- The offline importer was exercised against the actual 85 MiB OEWN 2025 XML:
  135,969 input entries yielded a 34-entry `tide` subset. Explicit disabled,
  enabled, lookup and reopened lookup checks passed. Original sense/concept IDs
  and paired OEWN/Princeton notices were retained. The data was a temporary test
  artifact, not installed into the user's normal profile or bundled with Knot.
- Strict library Clippy passes for desktop, readings and composition. The
  readings and composition crates also pass all-target strict Clippy. The
  desktop's all-target strict invocation still finds pre-existing test-only
  lints in `scroll_site.rs` and `workspace.rs`; it is not represented as green.

Native evidence lives under
`/Users/markik/Code/testing/knot-editor/composition-20260930/receipts`.
The verified optional sample pack and its acceptance receipt are retained under
the sibling `upstream` directory; they are not installed or enabled in the user's
normal profile.
The `composition-v3-*` run returned `RESULT ok`, 180 frames, five nonblank
captures with distinct digests, all visually reviewed. At a 1280-by-900 logical
viewport it showed the lexical empty state, all-off sound opt-in state, a
`Night ↔ light` perfect-rhyme result, readable collected quotation/analysis and
the source-import panel. The scenario also activated Return to source and
kept the source clean. Its source fixture and scenario are checked in under
`scenarios/fixtures/composition` and `scenarios/composition.scn`; the headless
scenario test additionally reopens the resulting collection.

The initial native attempt used generic text selection but reached analysis
with an empty source selection and failed its assertions. Its failed receipt
and captures are retained. Subsequent runs used the existing preview-heading
selection action and passed; this does not qualify the generic pointer-selection
path. Auto-exiting native runs also do not establish external Accessibility
interaction acceptance. Native populated lexical results, source import,
restart, narrow viewport and zoom acceptance remain separate follow-ups.

Pure model tests, desktop integration tests and native visual acceptance are
reported separately; a passing model suite is not evidence that a panel renders
well at every size.
