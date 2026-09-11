# Scroll site authoring

Owner: Knot Editor. Status: first local authoring slice implemented, 2026-09-11.

The first slice creates an ordinary folder containing `site.json`, three native
`.scroll` pages, and `assets/`. Knot owns draft files, per-page publication
metadata, and publication snapshots. Mere's Nematic renders the source; Tabard
continues to own portable appearance rather than sites.

Save retains native UTF-8 source without conversion. Publish locally captures
saved files and saved metadata into an immutable in-memory revision. A loopback
TLS server serves that revision until explicitly replaced or stopped. Draft
edits and saves cannot change an existing publication. Only configured pages
are published; assets are reserved for a later bounded adapter.

Author and dates occupy the three Scroll success header lines; language is a
MIME parameter and UDC classification selects status 20 through 29 (24 default).
The abstract is native Scrolltext returned for a `+` metadata request, never
front matter injected into the page. A single language per page is supported;
unmatched preferences receive that default as allowed by the specification.

Protocol evidence: [specification text retained by smolnet-portal](https://raw.githubusercontent.com/michael-lazar/smolnet-portal/master/docs/scroll_spec.txt),
and the pinned `scroll-protocol =0.1.0` wire/parser implementation already used by
Errand. This is the small-web Scroll protocol, not scroll.pub.

Done conditions: native safe-save and real Nematic preview; site creation and
page navigation; metadata editing; explicit local publication and stop controls;
bounded path/request handling; tests for draft/publication separation and wire
metadata; headed desktop inspection; an independent client receipt or an exact
record of what prevented it. External hosting, format conversion, peer hosting,
webrings, collective authority and theme expansion remain separate work.

## Verification receipt

The existing checkout began clean on `main`, with three local commits and six
remote commits of divergence. The implementation uses its existing Mere
`33287b0e` / Genet `9e8f9dc2` pins. No Mere source change was needed.

- `cargo test --manifest-path crates/knot-document/Cargo.toml --features engine --offline`:
  36 passed, one pre-existing ignored diagnostic. Tests include native CRLF and
  Unicode preservation, guarded external-change refusal, implicit conversion
  refusal, and Nematic Scroll heading/relation output.
- `cargo test --manifest-path crates/knot-scroll-site/Cargo.toml --offline`:
  four integration tests cover three linked pages, draft/publication isolation,
  exact response header/body boundaries, abstract requests, metadata validation,
  external config edits, bounded resources, and loopback stop.
- `cargo test -p knot-desktop --offline`: 37 library tests and four binary tests
  passed; one pre-existing ignored diagnostic. The added site test exercises
  page-bound metadata, navigation refusal while metadata is dirty, and explicit
  publication generations. `cargo build -p knot-desktop --offline` passed.

Independent client: smolnet-portal commit
`ac926d2ab37b53443795439a7181e36416253909`, using its original `ScrollRequest`,
`URLReference`, response parser and body reader under Python 3.13.12. The only
client configuration adjustment was allowing the chosen ephemeral loopback
port. Source was obtained separately under `.tmp/scroll-receipt`, retaining its
Human Software License; none of that implementation is vendored or linked into
Knot. `scripts/verify_scroll_client.py` is the small authored assertion driver.

Six independent exchanges passed: body and abstract for each of three pages.
The index used class 8/status 28, author `Sample Writer`, language `fr-CA`, UTC
publication/modification dates, UTF-8 `Café`, and CRLF source. The other pages
used default class 4/status 24 with blank author/dates. Body lengths were
88/68/68 bytes; abstract lengths 39/8/8. All used TLS 1.3 and received the TLS
close notification. A separate Python standard-library TLS connection verified
the generated certificate using the exported public certificate as its trust
anchor. This is a loopback receipt, not external-hosting or general client
compatibility acceptance.

Headed Windows receipt: launched the built `knot.exe`, created `C:/t/ks1` through
the site controls, edited author metadata and saved it, reopened that site by
passing the folder at launch, appended `x` to the index through the native text
editor, observed the live preview, verified Publish locally refused the unsaved
buffer, saved, and published revision 1 at `scroll://localhost:5699/`.
The independent client fetched all three desktop-published bodies byte-for-byte
against disk (71/68/68 bytes), including author `m` on the index. A preview link
opened About. Stop serving released port 5699. All serving processes used for
verification were then stopped.

The independent exchange caught an accepted-socket mode issue on Windows;
accepted sockets are explicitly returned to blocking mode before bounded TLS
reads. The native automation helper's Unicode text injection did not enter text
in this host, so headed edits used ordinary key events. Unicode source handling
is covered by the native file tests and independent UTF-8 exchange; a headed
IME acceptance receipt remains separate. The existing Windows atomic writer's
unsafe-code warning and unused patches in the nested document workspace remain.

## Dependency integration receipt

Integrated incoming `origin/main` at `2309dc5` with local Scroll commit
`5d26a39`, preserving all four local commits and all six incoming commits.
Aligned the locally added Tinct dependency with incoming Mere
`c328e6fc9028f0024a82c4a0086bf15605812630` and regenerated the lockfile.
The resolved graph has one Mere source revision and one p2panda source tag,
`mere-p2panda-net-0.7.3` at `e140e53b`; Genet remains `9e8f9dc2`.

The broader editor gate exposed two integration issues. Vault search now
explicitly selects dense storage because its sealed record persists a dense
VectorIndex, while the updated lexical provider defaults to sparse storage.
The existing sealed-vault endpoint test also reacquired a mutex it already
held; its public-revision assertion now runs after releasing that guard.

Merged-state checks: 110 editor-library tests passed; 36 document-engine
tests passed with one existing ignored diagnostic; four Scroll site tests
passed; desktop tests passed (37 library and four binary tests, one ignored
diagnostic), and the desktop build passed. The resident-retention feature
gate additionally exercises owner revocation and explicit regrant.
The independent smolnet-portal client repeated all six body/abstract exchanges
successfully against the unchanged standalone Scroll server, including exact
bytes, metadata, TLS 1.3, and close notification. That server was stopped.
The headed receipt above precedes this dependency merge.

Existing warnings remain for the Windows atomic writer's unsafe block, an
unused p2panda LogStore import, and the unused p2panda-stream patch.

## Explicit limits

Publication snapshots and localhost certificates live for the serving session.
They are not persistent signed releases or remote deployment. Single-language
pages fall back to their configured language for unmatched preferences. Assets
are reserved, not published. There is no Scroll source highlighting or outline,
section-jump navigation, external-link launching, automatic heading numbering,
or input-link prompt in this preview. Nematic reports its input-link degradation.
The first UI edits existing manifest pages; adding/removing/renaming pages uses
the ordinary files and manifest until a later site-management slice.
