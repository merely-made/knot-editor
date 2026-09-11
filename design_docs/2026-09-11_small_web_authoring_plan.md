# Native small-web authoring and serving

**Status (2026-09-11):** Gemini/Spartan local publication and reviewed Titan/
Spartan submission implemented; Micron source editing implemented, native
rendering and NomadNet access/serving blocked on admissible protocol evidence.
Scroll baseline is landed at `6fcd7e4`.

## Scope and ownership

Extend portable ordinary-file authoring through Gemini/Gemtext, Titan upload,
Spartan presentation/submission, and Micron source editing where supported.
Knot owns saved source, manifest validation, editor drafts and explicit
publication selection. Shared `smolweb` crates own wire grammar and transport;
Mere's Nematic engines own native parsing/presentation. Turnstone owns routing,
sessions and authoring-pane composition. Every projection is derived from
native bytes. Save As does not imply cross-format conversion.

Existing Scroll metadata and abstract behavior must remain intact. This work
does not authorize arbitrary-code serving or publicly exposed listeners.
Publication is a bounded immutable snapshot, with loopback serving stopped
when its owner closes. Selecting a submission link must only prepare an
effect; sending requires a separate explicit action over reviewed bytes.

## Findings (2026-09-11)

- `crates/knot-scroll-site` at the baseline owns an explicit three-page Scroll
  manifest and immutable local publication. Generalize and rename it to
  `crates/knot-site`; old version-1 manifests default to Scroll.
- `smolweb/crates/gemini-protocol/src/server.rs` already owns TLS serving;
  `spartan-protocol/src/server.rs` owns bounded requests and body submissions.
  Static sources must reject submitted data and never execute files.
- `mere/crates/system/errand/src/serve/mod.rs` exposes `Source` and native
  `Item::Document` bytes. It is a reusable content adapter, separate from
  filesystem authority. Native protocol handlers can consume the same snapshot.
- Turnstone's `src/shell/mod.rs` already registers Scroll with the shared
  fetch/render path. Existing acceptance covers Gemini input, Titan and Spartan
  effects. Extend evidence and local-file authoring rather than duplicate them.
- Titan's shared upload helper needs pre-connect field validation and a bounded
  response before Knot adopts it for reviewed saved-source publication.
- No native Micron parser or NomadNet page-service adapter was found in the
  current owned crates. Raw `.mu`/`.micron` editing can preserve bytes; native
  presentation and Reticulum page access are not established by that support.
  GPL/AGPL reference implementations remain black-box compatibility targets.

## Implementation lanes and done-conditions

1. **Knot site authority (primary agent):** generalize manifest/profile selection,
   build native linked Gemini/Spartan sites, retain Scroll metadata, compose
   shared loopback servers. Done when exact saved bytes are independently fetched,
   unpublished paths fail, submissions cannot mutate static sites, publication
   replacement is explicit, and Stop releases the listener.
2. **Native document and desktop adapters (Terra):** raw Gemtext/Micron formats,
   same-format guarded saves, native renderer selection, site controls and
   explicit submission review. Done when source round trips preserve CRLF and
   Unicode, incompatible conversions fail before writing, dirty changes cannot
   leak into publication, and unavailable Micron presentation is visible.
3. **Shared Titan adapter (Luna):** validate request metadata before connecting,
   bound response bytes without silent truncation, retain TOFU and optional
   client identity. Done when malformed requests never reach a listener and
   oversized replies produce a visible error.
4. **Turnstone composition (Terra):** reuse existing protocol registration and
   effect composer, expose native local files through Knot, add Scroll acceptance.
   Done when registered sessions render native pages, local authoring preserves
   source, and explicit Gemini/Titan/Spartan flows remain usable.

## Persistent serving proposal, not rollout

The canonical ownership proposal is
`mere/design_docs/mere_docs/implementation_strategy/2026-08-22_djinn_family_resident_services_plan.md`,
section 13. It compares Knot-owned serving, Djinn-held ordinary snapshots, and
Djinn-supervised governed hosting. The preferred persistent step is explicit
snapshot handoff to Djinn while keeping ordinary sites independent of Gemot.
Certificate identity, restart policy, audience, retention, authority changes,
resource bounds and external-client restart acceptance remain proposal gates.
The current work only runs loopback listeners within the authoring process.

Theme-export research is scoped separately. A client's installed theme/settings,
app chrome, document palette, and author-supplied styling are different surfaces.
No foreign theme exporter or Gemini stylesheet mechanism is implied here.

The originating research lane verified Lagrange's
[official palette documentation](https://raw.githubusercontent.com/skyjake/lagrange/dev/res/about/help.gmi):
`palette.txt` supports Dark/Light sections and named RGB colors, including
ordered neutral intensities, accents and reserved status colors. This is a
client-installed UI palette, not an author-controlled page theme. A future
Tabard/Tinct exporter would map semantic roles, report unsupported roles, and
verify contrast in Lagrange, especially link icons across document themes.
The [Geopard README](https://github.com/ranfdev/Geopard) describes GTK4 and
per-domain generated colors but establishes no supported theme-import contract;
that target remains discovery-gated. GTK implementation CSS is not an import
API. Reader-installed themes in Knot/Turnstone, client UI exports, and native
author styling are separate adapters. Titan transport adds no theme mechanism.

## Progress

- 2026-09-11: verified clean Knot main, preserved dirty Mere work, inspected
  shared servers and existing Turnstone acceptance, assigned three bounded
  Luna/Terra lanes. Implementation and independent receipts remain underway.
- 2026-09-11: native document formats landed in `b503756` (41 tests passed,
  one existing ignored diagnostic); shared Titan preflight/bounds landed in
  `smolweb` at `882baeb1` (35 tests plus one doctest passed). Lagrange 1.21.1
  loaded the native site-library example at loopback port 50372 and followed
  `/about.gmi`. The independent standard-library client verified all three
  saved bodies (64/62/62 bytes), TLS 1.3 close notification, root index mapping
  and unpublished-path refusal. The example process then stopped. This is a
  site-library interoperability receipt; desktop UI verification is separate.

## Integration receipts and remaining gates

- `knot-site`: nine integration tests passed. These retain all four Scroll
  tests and cover old manifest defaults, immutable native publication, static
  Spartan submission refusal, listener cancellation, immutable preparation,
  one-shot redirect handling, durable Titan trust pins and certificate-change
  refusal before application bytes.
- Knot editor library: 110 tests passed. Desktop: 41 library and four binary
  tests passed, one existing diagnostic ignored; desktop build passed using
  the committed lockfile. Document adapter: 41 passed, one existing ignored.
- Headed Knot opened the Gemini site with port 1965, displayed native source
  beside Gemtext preview, and published three saved pages through the UI.
  Lagrange 1.21.1 independently loaded the index and followed `/about.gmi`.
  **Stop serving** released the listener; an independent socket then refused
  connection. Startup format/port initialization has a regression test.
- `scripts/submission_fixture.py` is an independent one-request Python
  receiver. The `knot-site` `submit` example sent the same saved 64-byte index
  through Titan TLS and Spartan. Both captures exactly matched disk with SHA256
  `3beafae72e6ca7f9ce8c0962e2278a84d605e1ed566eb638dfad765e4edbf8bd`.
  Each received one request; responses 30 and 3 were returned without following
  their redirect. These are submission-API receipts, not headed-send receipts.
  Full UI sending remains unverified: automation could click controls but its
  bulk text injection did not populate the target. The final body-field layout
  was visually checked and accepted a physical key after a coordinate click.
  A compatible external capsule,
  real authentication policy and manual end-to-end upload remain acceptance gates.
- Turnstone `d3dde62` admits Scroll/Gemtext/Micron into the local Knot surface
  and saves exact native source bytes. An explicit clean-scratch Rust 1.97.1
  build and headed Scroll scenario passed; the screenshot displayed its title,
  body and Citation link. Full library suite: 453 passed, nine ignored, one
  failure in the existing partition-healing convergence test. That broader
  suite is not green. Its canonical gap analysis records the detailed receipt.
- Micron remains raw source with an explicit unavailable native preview/server.
  No guessed control grammar was retained. Mere's Nematic fidelity plan records
  the clean-room evidence gate. Djinn persistent serving and foreign theme
  exports remain proposals; this work introduces no resident or public listener.

For repeatable local submission checks, start the Python fixture with a new
output directory, then run `cargo run --manifest-path crates/knot-site/Cargo.toml
--example submit -- SAVED_FILE PRINTED_ENDPOINT MIME TRUST_RECORD_PATH`.
Use a fixture-specific trust path. The receiver writes only its captured body,
public receipt, and (for Titan) fixture-local TLS material.
