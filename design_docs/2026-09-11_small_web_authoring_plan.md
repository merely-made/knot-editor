# Native small-web authoring and serving

**Status (2026-09-11):** in progress. Scroll baseline is landed at `6fcd7e4`.

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

Djinn already owns process lifetime and device services in
`mere/ports/djinn/src/resident.rs`; `resident_knot.rs` holds a personal document
source and optional sync host, and `resident_blobs.rs` manages scoped blob
custody. These are useful composition seams, not a general site-hosting daemon.

The preferred extension is a service that accepts immutable published content
plus explicit listener policy. Knot can own that service in process; Djinn
could later hold it while applications are closed. An ordinary site must work
without Gemot. Replicating governed moot records is a separate service with
membership, contribution, hosting and revocation checks. Discovery advertises
only enabled endpoints; it does not itself grant access or replicate content.

Three concrete options remain for review: keep serving tied to Knot; let Djinn
persist selected snapshots and supervise listeners; or let Djinn additionally
compose governed replica/hosting services. The second is the smallest persistent
step. Its done-conditions would include durable certificate identity, explicit
startup policy, interface/port selection, snapshot retention and replacement,
bounded resource use, restart recovery, status and Stop controls, and refusal
when authority expires. Public binding and daemon installation are separate
decisions. The existing author-offline Gemini proof in Mere demonstrates useful
transport/authority composition but does not establish production daemon policy.

Theme-export research is scoped separately. A client's installed theme/settings,
app chrome, document palette, and author-supplied styling are different surfaces.
No foreign theme exporter or Gemini stylesheet mechanism is implied here.

## Progress

- 2026-09-11: verified clean Knot main, preserved dirty Mere work, inspected
  shared servers and existing Turnstone acceptance, assigned three bounded
  Luna/Terra lanes. Implementation and independent receipts remain underway.
