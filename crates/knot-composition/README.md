# knot-composition

The desktop's active collection is retained through `retention::CompositionRetainPort`
in **Knot's own mere**, not in the standalone JSON store. The resident owns
admission, encryption, signed history and receipts. `CollectionStore` remains a
validated explicit legacy-import/interchange utility; it is not a fallback when
mere authority is absent. See the [retention decision](../../design_docs/2026-09-30_composition_mere_retention.md).

Personal, local composition collections retain independent copies of words,
passages, senses, pronunciations and notes. They are shared across an author's
documents/readings, not a collaborative network. No background lookup, download,
normalization, anchor relocation or pack installation occurs in storage.

`CollectionItem::new(kind, label, text)` provides a stable UUID and fluent builders
for collection, copied pack provenance, document provenance, author notes, tags
and order. All fields are serde data. `CollectionStore::open(path)` is read-only;
`items()`, `collect(item)` and `remove(id)` expose explicit durable operations.
Items preserve insertion order as well as their author-chosen ordering key.

`DocumentAnchor::capture(address, source, byte_range)` copies the exact quotation
and complete-source BLAKE3 hash. `validate_source(current_source)` checks that
original UTF-8 byte range and hash; stale anchors remain stale even when an
identical quotation occurs elsewhere. An optional absent hash checks only the
original range and exact quotation, without a claim of whole-source identity.

Storage uses a private same-directory temporary file, file synchronization,
atomic replacement and directory synchronization on Unix. A stable OS-locked
sidecar and on-disk digest check reject stale/concurrent writers. Unsupported,
malformed, oversized or unknown-field stores are not repaired or overwritten.
`DurabilityUncertain` specifically means replacement succeeded but directory
sync failed: in-memory state is updated; inspect/reopen instead of blind retry.
Limits: 16 MiB per store, 10,000 items, 1 MiB each for copied text/author notes,
64 tags and pack references per item. JSON is local plaintext, not a sealed vault.

## Explicit offline WordNet conversion

```sh
cargo run -p knot-composition --bin knot-wordnet-import -- \
  --input /path/to/english-wordnet-2025.xml \
  --output /path/to/new-subset.json \
  --source oewn --version 2025 --lemma tide --lemma estuary
```

The caller supplies uncompressed UTF-8 WN-LMF locally. Exact source/version must
match the XML Lexicon. There is no fetching, installation or overwrite. Output
is a validated shared `reference-data` pack, initially disabled when explicitly
imported into a registry. Stdout is a JSON conversion report: retain it alongside
the output and review its exclusions before importing. Choose all required lemmas
for the initial subset; different data under the same installed source/version
is an immutable registry conflict, not an automatic merge or replacement.

The subset contains exact case-sensitive selected lemmas and one-hop lexical
neighbors, without recursive graph expansion. Original entry/sense/concept IDs
remain verbatim, and sense-specific relationships retain exact target sense IDs.
Same-synset membership produces synonym links. Only exact supported relation
labels are mapped; unsupported labels (including instance/member/substance
subtypes) are counted, not coerced to generic relations. Inflected forms, ILI
definitions, syntactic behavior and discarded XML attributes are also reported.
This is a lexical subset converter, not a lossless archival WN-LMF round trip.

OEWN output includes paired Princeton/OEWN attribution and upstream notices from
[`LICENSE.md`](https://github.com/globalwordnet/english-wordnet/blob/main/LICENSE.md)
and [`WNDB_License.txt`](https://github.com/globalwordnet/english-wordnet/blob/main/WNDB_License.txt),
with the CC BY 4.0 license URL and explicit subset/conversion-change disclosure.
The historical 2023 notice in the upstream WNDB file is retained, not silently
rewritten to the converted edition. An unreviewed OEWN license URL is refused.
Source identity and digests do not establish authenticity or upstream endorsement.

Offline input limits include 256 MiB XML, 32 levels of XML nesting, 16 KiB text
fields, bounded record counts, 1024 distinct diagnostic keys per map, and a
conservative 512 MiB retained-memory accounting ceiling (not an RSS guarantee).
Output is limited to 4096 entries and the shared 32 MiB pack limit. Internal
DTD/entity declarations, unknown general entities, extensions/external lexicons,
multiple lexicons, dangling imported targets and ambiguous multi-definition
flattening are refused. The external DTD URL is never fetched.

Tests do not download upstream data. An explicit real-upstream acceptance lane
can validate a converted local OEWN2025 tide pack with:

```sh
KNOT_TEST_OEWN_PACK=/path/to/new-subset.json \
  cargo test -p knot-composition --test wordnet_import \
  local_upstream_oewn_pack -- --ignored --nocapture
```
