# Native small-web authoring and serving

**Status (2026-09-11):** Gemini/Spartan local publication and reviewed Titan/
Spartan submission implemented and checked through the headed review/send flow.
Micron has an evidence-qualified partial preview and a native saved-snapshot
NomadNet server. Unsupported Micron source remains inert and visible.
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

## Initial integration receipts (superseded where noted below)

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

## Follow-up acceptance investigation

The follow-up keeps Djinn and Tabard deferred while examining Micron evidence,
headed submission, and Turnstone's convergence failure. Initial headed input
investigation found that submission controls were missing from the desktop's
focused-text lookup, which owns caret and IME state. In particular, the unknown
Spartan textarea fell through to the document's text slot. Native Spartan site
preview also selected plain Gemtext rather than the Spartan engine, preventing
native submission prompts from reaching the existing explicit composer.
These are application defects independent of the UIA helper's unsupported
set-value request. Final acceptance results are recorded below when verified.

The independent Python receiver now accepts an explicit test port, a nonempty
success body, and an expected certificate-refusal mode. A local API probe sent
the saved 64-byte index to `localhost:50597`, received status 20 plus a 22-byte
reply, then retried explicitly against a fresh certificate on that same port.
The durable pin refused the changed certificate; the receiver recorded a TLS
handshake failure and zero application bytes. These remain API/fixture
receipts until the corresponding headed flow is completed. All probe receivers
exited. Existing user certificate records were not used.

The focused desktop suite passes eight tests, including regression probes
against the prior behavior: replacing the Spartan renderer with Gemtext makes
the real loaded-site Submit-button test fail; restoring the old body focus
fallback makes the clicked-caret insertion test append instead of prepend.
The current host harness exercises pointer placement and VK_PACKET-style text
without changing the document source. The pinned host harness exposes no
public IME lifecycle method, so that lifecycle is not claimed as separately
tested. A dummy-token DOM check retains password typing and hides the token
from painted text. Response bodies are inert and explicitly capped at 8 KiB.
The complete locked offline desktop suite passed 49 tests (45 library, four
binary), with one existing ignored diagnostic. The first invocation could not
replace the running acceptance executable; after stopping that owned process,
the same check passed.

### Headed submission receipt

The Windows Security prompt was gone on fresh inspection. The rebuilt
`ef724af` desktop then completed both headed submission paths using the
independent Python receiver, with fixture-only trust records under
`C:/t/knot-ui-trust-20260911`.

- Spartan: clicked the native **Submit locally** prompt, entered `a`, and
  prepared the body. Review displayed `spartan://localhost:65025/upload`,
  `text/plain`, one byte, and the body. No receiver receipt existed before
  **Send reviewed bytes**. The receiver captured one request with exactly `a`
  (SHA256 `ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb`).
  Knot displayed `Reply 2 text/plain (14 bytes)` and `Body accepted.`.
- Titan: entered `titan://localhost:65026/upload` with physical key events,
  prepared the saved 75-byte index, and inspected its target, MIME, digest,
  and source before sending. The receiver captured one request, `text/plain`,
  without a token. Its body and the saved file both had SHA256
  `f9fdc6d7ace957017adb1feeb04274b1d0a01a996f013ff8bf113039377399ea`.
  Knot displayed `Reply 20 text/plain (22 bytes)` and `Saved source accepted.`.
- Certificate change: a fresh fixture certificate on the same Titan address
  was refused after a second explicit review/send. Knot displayed the changed
  certificate error with pinned and observed fingerprints. The receiver
  recorded TLS handshake refusal and zero application bytes.

Public receiver receipts are in `C:/t/knot-ui-spartan-headed-20260911`,
`C:/t/knot-ui-titan-headed-a-20260911`, and
`C:/t/knot-ui-titan-headed-b-20260911`. All three receivers exited normally.
These results supersede the earlier unverified headed-send status. They cover
local unauthenticated submissions and certificate continuity; external capsule
authentication and full IME lifecycle remain separate acceptance boundaries.

The Windows helper's bulk text injection still did not populate the target;
physical keys did. A focused `CAMBIUM_HOST_KEY_TRACE=1` run recorded the helper
as `Character("v")` with Ctrl held, not injected Unicode text. A physical `p`
arrived without Ctrl and appeared in the body. This identifies the unhandled
clipboard-paste path; it does not support blaming a string UIA setter or the
fixed field-focus mapping. The trace is retained at
`C:/t/knot-input-trace-20260911.err`, and its app process was stopped.
UIA click coordinates also remained unscrolled after the
review moved below the viewport, while clicking the visibly scrolled button
worked. Neither helper behavior is counted as a successful bulk-input or
accessibility acceptance receipt.

## Partial Micron preview and native NomadNet publication

Mere `1777b19840a6478a0faf3ab060a3ff71d5532321` supplies the
`nematic.micron-subset` engine. Literal stock-client captures qualify LF
heading, divider, plain text, and balanced same-line bold/italic forms. Other
control sequences and link candidates remain visible inert source with a badge.
The engine retains a global partial-preview warning and does not invent a MIME
type. Its checked-in `CAPTURE_MANIFEST.md` records the evidence and limits.

Knot `1f8eee5` adopts this preview, `f1db43e` projects its badges and explicit
bold/italic styles, and `ce6ba53` paints the app preview pane while scrolling.
The headed recheck verified heading, divider, plain, bold, italic, the global
warning, and both unknown-source badges. Content stayed readable after a
424-pixel scroll. Thin outer canvas margins below the original viewport remain
black, an unresolved host painting limitation. The latest desktop library
suite passed 47 tests with one ignored; the earlier full run also passed its
four binary tests.

`knot-site` pins Retinue
`2a763c72ec7c7ff61b8cd7adb86d084a02d2c4d6`. Its `StaticNode` owns the
NomadNet destination and request envelope; Knot owns saved snapshots and the
dedicated runtime. A publication starts a loopback TCP Reticulum interface,
uses a fresh ephemeral identity, and displays its destination plus interface
address. It maps manifest pages into `/page/` requests and never executes page
files. Limits are caller-configurable through `NomadNetServerConfig`.
Replacing a publication swaps the saved snapshot; stopping cancels the runtime,
accepted sessions, and listener. This is an explicit local preview service;
persistent identities and resident/public hosting are outside this slice.

The clean revision-pinned site suite passed all 10 tests, including exact
saved-byte isolation, 128 KiB replacement, wrong-format refusal, and listener
shutdown. Log: `C:/t/knot-nomadnet-immutable-final.log`.

Headed publication: the actual Publish locally button started destination
`5bbaf38250b3f61469bb6c1f7ab693c1` on `127.0.0.1:5699`. A stock Windows RNS
1.5.3 client joined after publication, requested its path, opened a link, and
received `/page/index.mu`: 154 bytes, SHA-256
`898a86b7d1486d6e6ad63ed27293e46a187aa0999ed2f535c6ae0be690aabf240`, exactly
matching the saved fixture. The received file is retained at
`C:/t/knot-nomadnet-stock-client-20260911-recalled/index.mu`.
The first probe incorrectly waited for an announce callback; standard path
discovery had populated `Identity.recall` and `Transport.has_path` directly.
That fixture failure is not a serving-discovery failure.

The large-page check then exposed a real session-lifetime bug: the server
received the Resource proof and immediately dropped the session, sending a
link close before stock RNS completed its response callback. The captured
exchange contained 283 unique parts and the client's Resource proof. A
diagnostic proxy delaying only the close frame made the callback succeed.
Production uses no delay: the host retains the session for subsequent requests
or peer closure, bounded by its configured deadline. It selects the current
saved snapshot after receiving each request. Retinue's helper API now borrows
or returns session ownership instead of silently discarding it.

Final direct headed receipt on revision-pinned Retinue `2a763c7`: Publish locally
started `483dcfc4871b95cc297daa9d4641215c` on `127.0.0.1:5699`. A fresh stock
Windows RNS 1.5.3 reader joined after publication and fetched these pages over
**one link**, without a proxy or delay:

| Page | Received bytes | SHA-256 |
| --- | ---: | --- |
| `/page/notes.mu` | 134 | `0ac184d9d224ad09862743de0ba43ae60c0ad93ea8ec6ed4d110468e5146ba9a` |
| `/page/about.mu` | 131072 | `15601535eca4a38b7e31ad6494861121cb9f84ccf55d4beb6a707d4f7a87813d` |
| `/page/index.mu` | 154 | `898a86b7d1486d6e6ad63ed27293e46a187aa0999ed2f535c6ae0be690aabf240` |

All bodies matched the saved files exactly. Log and received files:
`C:/t/knot-stock-final-sequence-20260911.log` and
`C:/t/knot-stock-final-sequence-20260911/`. A native regression additionally
reuses one session across a large response and a newly published small snapshot.
The stock client emitted a socket warning during its own explicit shutdown;
all three response callbacks had already succeeded.

The actual Stop serving button then showed "Local serving stopped" and the
5699 listener disappeared. The task-owned desktop process and stock reference
nodes were stopped; their fixtures and capture logs were retained. The final
desktop suite on this exact Retinue pin passed 47 library tests (one ignored)
and four binary tests: `C:/t/knot-desktop-immutable-final-tests.log`.

Earlier independent native Retinue-to-stock Python NomadNet captures received
21-byte and 131072-byte pages; stock Python RNS also received a native Retinue
Resource response. Stock Go `view-mu` accepted the small response, but its large
Resource transfer repeated requests and timed out. That cross-client limitation
remains open. The capture ledger is
`C:/t/retinue-nomadnet-capture-20260911.md`. No GPL/AGPL implementation source
was used to implement the parser or page adapter.
