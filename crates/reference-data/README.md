# reference-data

Backend-neutral lexical reference contracts and a local, opt-in registry. Only
`serde`, `serde_json`, and `blake3` are dependencies. This is data infrastructure,
not an executable mod manifest, dictionary engine, downloader, or UI.

No upstream dictionary is installed or enabled by this crate. The checked-in
`fixtures/bilingual.json` is small **synthetic test data**, not OEWN or CMUdict.
Hosts may advertise real source metadata without an invented digest, then obtain
and convert licensed upstream data separately. URLs are inert metadata: the
crate never follows them, runs code, extracts archives, or accesses the network.

## Host use

```rust
use reference_data::{LookupQuery, SourceKey, SourceRegistry};
# fn demo(root: &std::path::Path) -> reference_data::Result<()> {
let mut registry = SourceRegistry::open(root)?;
let status = registry.import_json(
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/bilingual.json")),
    Some("c9e6147071c79ff7e6ae3f736a31ac21e6841ac4477fe68ccff4e7c4ed1b5de3"),
)?;
assert!(status.installed && !status.enabled); // New installs never enable themselves.
registry.set_enabled("fixture", "1", true)?; // Explicit host/user choice.
let entries = registry.lookup(&LookupQuery {
    lemma: "bank".into(),
    language: Some("en".into()),
    sources: vec![SourceKey { source: "fixture".into(), version: "1".into() }],
}, 8)?;
assert_eq!(entries[0].senses[0].definition, "The sloping edge of a river.");
registry.set_enabled("fixture", "1", false)?;
# Ok(()) }
```

`advertise(SourceManifest)` records available metadata only. `list()` distinguishes
available, installed, and enabled; `set_enabled` persists explicit opt-in.
Reimporting identical installed data preserves its enabled state. Replacing a
source/version with different data or metadata is refused: use a new version.
Lookup requires **both** enabled state and explicit query source selection. No
sources means no results. Lemmas are exact and case-sensitive, with no stemming,
Unicode normalization, language folding, ranking, or hidden fallback. `entry`
also respects enabled state. Results are owned values; pack indexing is by exact
lemma and source-local entry ID. A host can navigate a relation to an entry and,
when present, to its exact `target_sense`.

## Normalized JSON schema v1

The fixture is a complete usable example. Fields follow the public serde types:

- `NormalizedPack`: `schema_version` (1), `manifest`, `entries`.
- `SourceManifest`: `id`, `version`, `label`, `language`, `features`, `license`,
  `attribution`, `upstream`, `digest`.
- `ReferenceId`: `source`, `version`, `entry`. IDs and versions are opaque ASCII
  letters/digits/underscore/hyphen/dot, 1–128 bytes with at least one alphanumeric
  character, never filesystem paths. Significant sense ID dots (including final
  `..` in OEWN identifiers) are preserved, not normalized away. Versions
  additionally permit nonempty dot-separated segments (e.g. `1.0`).
- `LexicalEntry`: `id`, `lemma`, `language`, optional `part_of_speech`, `senses`,
  `pronunciations`, `relations`.
- `LexicalSense`: `id`, optional source-local `concept_id` (e.g. a synset ID),
  `definition`, `examples`, `relations`.
- `LexicalRelation`: typed `kind`, target `ReferenceId`, optional `target_sense`.
  Targets must exist in the same pack; sense targets must exist in that entry.
- `Pronunciation`: `notation`, `value`, optional `variant`. A notation label is
  data, not an executable pronunciation provider.

Enum values are snake_case. Features are `definitions`, `examples`, `relations`,
`pronunciations`; data must be covered by declared capabilities. Parts of speech
are `noun`, `verb`, `adjective`, `adverb`, `other`; relations are `synonym`,
`antonym`, `hypernym`, `hyponym`, `meronym`, `holonym`, `derivation`, `related`.
A converter must report unsupported upstream distinctions, not silently relabel
them. Language tags have nonempty ASCII alphanumeric hyphen-separated segments;
this is syntax validation, not an IANA language registry. `mul` permits entries
with distinct explicit languages. Unknown fields and enum variants are rejected.

### Digest contract

`manifest.digest` is lowercase, 64-character **BLAKE3**, covering exactly the
UTF-8 bytes returned by `serde_json::to_vec(&entries)` using these schema types.
Struct field order follows the declarations; entry/list order is significant.
Optional `concept_id` and `target_sense` are omitted when absent; other optional
fields serialize as `null`, and lists as arrays. JSON outer whitespace/key order
does not affect this entries-payload digest because import parses and then
serializes the typed entries. Do not substitute upstream SHA-256 or a hash of
raw upstream XML. Converters call `digest_entries` after constructing entries.

Catalog-only manifests may have `digest: null`; installed packs must supply a
valid digest. The optional `expected_digest` import argument lets a host pin a
checksum obtained independently. Self-declared digests detect corruption, not
authenticity. Licenses and attribution remain source metadata, not a legal
determination. The immutable disk filename is a separate BLAKE3 hash of the
**complete imported JSON bytes**, verified again on reopen.

## Bounds and storage

`Limits::default()` caps pack bytes at 32 MiB, total installed bytes at 128 MiB,
entries at 100,000, senses at 200,000, relations at 500,000, registry bytes at
1 MiB, sources at 128, individual strings at 16 KiB, lemmas at 256 bytes, query
sources at 16, and results at 256. Hosts can use `open_with_limits` for a deliberate
bounded larger source. Import also rejects invalid UTF-8, unsupported schema,
unsafe IDs, mismatched source/version/language, undeclared capabilities,
duplicate IDs/features/relations, dangling references, and digest mismatch.

The caller owns a **private trusted directory**. This is not a sandbox against
a different process maliciously changing that directory during I/O. Root,
packs, registry, and lock symlinks are rejected when checked. Pack filenames are
generated hashes, not supplied paths. Saved registry and all referenced packs
are bounded and fully revalidated on reopen; corrupt existing files are not
silently repaired or overwritten. There is no destructive uninstall/GC API.

One registry handle holds a standard-library OS advisory exclusive file lock.
Concurrent opens return `Busy`; drop or process exit releases it. The lock file
persists and is never unlinked, avoiding inode replacement races. Requires Rust
with `File::try_lock` (stable since 1.89). This single-writer handle is a storage
mechanism, not application-global source policy.

Pack publication uses a synced temporary file plus atomic create-new hard link;
existing files are never replaced. Registry updates use a synced temporary file
and atomic rename, with directory sync on Unix. Failure can leave an inert,
unreferenced content-addressed pack; no failed validation publishes metadata.
On platforms where rename-over-existing or hard links are unavailable, mutation
fails safely; no non-atomic fallback is attempted. Power-loss durability beyond
filesystem sync guarantees is not claimed. If registry rename succeeds but
directory sync fails, `CommitUncertain` reports valid-but-durability-uncertain
disk state and blocks further mutations on that handle; close and reopen before
continuing. The old in-memory snapshot must not be used to infer the commit.

## Verification

Run `cargo test --locked -p reference-data`. Tests exercise the actual sealed
JSON fixture, opt-in/disable/reopen, multilingual exact lookup, corruption and
digest refusal, hostile paths, duplicates, relation target senses, bounded input,
failed publication, and competing/crashed writers. No external source download
or runtime execution is involved.
