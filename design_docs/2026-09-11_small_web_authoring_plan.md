# Native small-web authoring and serving

**Status (2026-09-13):** Gemini/Spartan local publication and reviewed Titan/
Spartan submission implemented and checked through the headed review/send flow.
Micron has source-preserving syntax, a native preview for text, headings,
emphasis, ordinary links and tables, and a saved-snapshot NomadNet server.
Full presentation fidelity and partial refresh remain open. Micron request-form
editing and explicit remote submission have focused automated receipts below and
a headed acceptance taken the same day against a controlled request handler and
a real stock `nomadnet` 1.4.2 node; response-cap ordering, configurable request
bounds and two preview-pane defects remain open from it.
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

**2026-09-13 scope consolidation:** Tabard's verified external contracts,
native reader adapter and foreign-export done-conditions now live in
`mere/design_docs/mere_docs/implementation_strategy/2026-07-05_theme_modes_plan.md`,
section "Tabard small-web adapters (2026-09-13 scope)". Knot owns the selection,
preview and persistence consumer, with `apps/desktop/src/appearance.rs` as its
current palette seam. A reader setting does not change published author CSS.

The next shared Micron presentation and interaction scope lives in
`mere/design_docs/nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md`,
section "Micron completion scope (2026-09-13)". Knot supplies headed authoring and
preview receipts alongside Turnstone's browsing receipt. Djinn's first resident
acceptance is an explicitly handed-off saved Gemini snapshot that remains
readable after Knot exits and after the resident restarts; this does not wait
for Micron forms. Progress on these scopes is recorded below without treating
library implementation as a headed consumer receipt.

## Micron request-form consumer (2026-09-13)

Knot keeps Micron form values in an ephemeral preview-side editor. Opening a
form requires an active Micron document, including a loose `.mu` file; its exact
source bytes and document address are retained with the form. Text, masked
text, checkbox, and radio values are editable without changing authored source,
the site manifest, an address, or a publication. A field change after review
invalidates that review; a source or address change requires reopening the
form. The shared Nematic form model rejects malformed, partial, and table
controls rather than treating an incomplete projection as sendable.

Each Micron request action is separately prepared from the current values.
Knot displays a local review, redacting values from masked fields. Send is a
second explicit action. It only accepts a full `destination:/absolute/path`
target and an explicit `KNOT_NOMADNET_TCP=host:port` interface. The client uses
an ephemeral Reticulum identity, bounded typed string-map request, path
discovery, a bounded request timeout, and a transient response pane. A timeout
has an unknown remote outcome and is never retried automatically. Local `:/`
aliases have no standalone remote authority and are refused. Knot's loopback
`StaticNode` remains pages-only: publishing a local Micron site never creates a
request handler or executes page files.

Automated receipts pass with the isolated Cargo home from `C:/t`:
`cargo test --manifest-path C:/Users/mark_/Code/repos/knot-editor/crates/knot-site/Cargo.toml --offline --locked`
passes 16 tests, including a real Reticulum loopback handler that decodes the
typed map. The follow-up expands that response to a 4096-byte Resource and
requires the server to finish receiving its proof. Both form clients now await
Retinue's bounded `Endpoint::shutdown` after receiving a reply; abrupt `close`
or Drop could discard the queued acknowledgement. The three focused client
tests pass after this correction. `cargo test --manifest-path C:/Users/mark_/Code/repos/knot-editor/apps/desktop/Cargo.toml --lib scroll_site::tests --offline --locked`
passes 16 desktop tests. Those cover preparation, source/address and field
review invalidation, cancellation/disconnected stale-result suppression, and
response visibility when the site panel is closed. They are automated seams and
do not by themselves establish a headed form-flow or interoperability with a
stock handler.

**Headed acceptance, 2026-09-13.** Those two gates are now closed. The
artifacts are at `C:\t\micron-headed-20260913`, whose `RECEIPT.md` indexes the
captures, logs, scenarios and scripts. Two independent instruments observed the
sends: a controlled public-RNS `nomadnetwork/node` request handler that appends
every received typed map to `handler/requests.jsonl` with a wall-clock
timestamp, and a real stock `nomadnet` 1.4.2 daemon under WSL whose stock
executable page records the `field_*` environment it receives to
`stock-node/submissions.jsonl`. Stock NomadNet was treated as a black box
throughout. The desktop binary was Knot `fae329c`, built from `C:/t` with the
isolated Cargo home and an absolute manifest under Rust 1.97.1:
`cargo build --manifest-path C:/Users/mark_/Code/repos/knot-editor/apps/desktop/Cargo.toml --locked --offline`.
Knot has no self-drive lane, so the window was driven by a foreground-guarded,
DPI-aware click/key script with PrintWindow captures, with
`KNOT_NOMADNET_TCP=127.0.0.1:42541` for the handler and `…:42543` for the stock
daemon.

As of 2026-09-16 that artifact tree is gone: `C:\t\micron-headed-20260913` was
deleted on or before that date along with the rest of the `C:/t` Micron scratch
family, so the paths named above are dead references, kept as the record of where
the work was done rather than as somewhere to look. Nothing committed was lost
and the acceptance stands on the tests and fixtures named in this section, which
still run and pass. Receipt artifacts now live under `Code/testing/knot-editor/`,
not `C:/t`.

Opening the page, opening the form and editing fields produced no request beyond
the ordinary page fetch. **Open Micron form controls** → edit → **Prepare Submit
all** showed the reviewed map with the masked value redacted; **Send reviewed
Micron request** then produced exactly one observed map at the handler with the
edited values (`field_hd_text="edited café 雪"`,
`field_hd_empty="filled"`, `field_hd_mask="secret"`,
`field_hd_checks="red,blue"`, `field_hd_radio="blue"`) and a visible
`Micron reply (172 bytes)` echoing it. A malformed local target — a stray `:`
before the destination hash, an authoring error in the fixture — was refused
before anything was sent: `Micron target must be a 32-digit destination and a
plain absolute path`. Reload after editing the file produced the stale-form
notice and **Discard stale form**. Against a 40 s handler delay, **Cancel Micron
request** gave `Micron request cancelled locally. Its remote outcome may be
unknown; do not retry automatically.` and the pane was unchanged after the late
reply arrived; letting the same delay run to Knot's fixed 30 s deadline gave
`Micron request failed: Micron request timed out; the remote outcome may be
unknown. Review before retrying.` Both cases issued one request and no retry. A
send to the stock daemon with default values was recorded as one
`submissions.jsonl` line of the same map shape plus `link_id`, with a
195-byte reply shown in the app; the node passes the `str → str` map verbatim as
`field_*` environment variables and does no splitting or typing of its own.

Four findings came out of this run against the consumer, and follow-up commits closed them. First, a 5 MiB reply fails as
`Remote Micron handler returned an invalid response` before the 4 MiB cap is
compared: Retinue's single-segment `MAX_SEGMENT_SIZE` is 1,048,575 bytes and
Knot unpacks the received Response before checking its size, so the cap is
unreachable and the message names the wrong cause. Turnstone caps received bytes
before decoding and reports correctly; the follow-up commit moves Knot's check
ahead of unpacking, so the cap is now compared against the received bytes
(with a small envelope allowance) and an oversized reply names the size, while
the decoded body is still checked as before. Multi-segment responses remain a
separate transport gate. Second,
Knot's request bounds (30 s, 4 MiB) were compile-time defaults with no
environment override, unlike Turnstone's `TURNSTONE_NOMADNET_*` knobs, so the
timeout receipt needed a 40 s handler path; the follow-up commit reads
`KNOT_NOMADNET_TIMEOUT_SECS` (default 30, clamped 1–120) and
`KNOT_NOMADNET_MAX_RESPONSE_BYTES` (default 4 MiB, clamped up to 64 MiB) in the
desktop app, named beside `KNOT_NOMADNET_TCP` in the prepared-request text, with
knot-site left env-free. Third, the preview pane was read as losing its scroll
offset during submission redraws. It does not: the offset is the window
viewport's, because `.knot-scroll-preview` grows to content height and its
`overflow: auto` never makes it a scroll container (the `aside` box itself moves
with the wheel, from y 145 to y -735 in a harness run at 1100×730). The host
preserves that offset across a rebuild and only re-clamps it to the new content
height (`crates/cambium/cambium-rootstock/src/owned_layout.rs`, `layout_resolved`
line 142; `frame.rs` lines 184-190 copy both scroll planes when the layout is
rebuilt from scratch). What the headed run saw was that clamp: sending collapses
the form panel from fields, review and reply to two lines, and on a page that
short the new maximum offset is near zero, so the document snaps up. A test now
pins that a submission redraw on a long page moves the offset by nothing and
leaves the cancel control reachable. The remaining question — whether the
preview pane should scroll independently of the source pane, which needs a
bounded height on the flex row rather than a change to the submission path — is
a layout decision left open. Fourth, Knot kept the previous reply text after a
form was closed and reopened; `close_micron_form` now clears the result and
response unless a send is still in flight, so a reopened form starts empty while
closing only the site panel still leaves a finished reply visible. Inline form widgets, partial refresh, outgoing
request Resources, multi-segment responses, authentication and dynamic page
hosting in Djinn all remain outside this receipt.

Platform facts from standing the stock instrument up: Windows `nomadnet.exe`
cannot start at all (`ModuleNotFoundError: termios`), and under WSL every file
on `/mnt/c` is mode 0777, so NomadNet treats every page as executable and the
pages had to be shebang scripts whose stdout is the served page.

**2026-09-13 saved-snapshot handoff:** `knot-site` now exports
`PublishedSnapshotV1` and `PublishedPageV1`, with canonical encoding, a digest,
bounded decoding, and reconstruction of a read-only `Publication`. The contract
contains selected saved bytes and metadata, never an authoring-directory handle.
It validates the index, case-insensitive page-name uniqueness, format, UTF-8,
page/site byte bounds and a separate 24 MiB encoded-message bound. Thirteen
standalone tests pass with `--offline --locked`, including three snapshot
regressions. The contract is published at `6b68405`; resident lifetime and
ordinary-client restart acceptance belong to Djinn's next gate.

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
- 2026-09-16: Micron navigation consumer on branch `micron-nav-a1`: Mere
  bumped to `709dfdf2`, collapsible headings toggle in the Micron preview, and
  in-page links stay label text because the desktop host has no programmatic
  scroll. See the dated section at the end.

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

## Broader Micron syntax and client interoperability (2026-09-13)

Knot `317593383a4bacb60600c5532812f7df4757bed9` adopts Mere
`4ac591c9070b5624d9f58b6b511e3a5485e6c87b` and Retinue
`ef1c47a602cb742293c109b0c443a9ceb8c1dce7`. The shared `nematic.micron` engine
uses the stock NomadNet 1.4.2 Guide and captured outputs to preserve syntax
independently of its portable preview. New Micron sites use native headings
and same-node page links. Existing files retain their exact source on save.

The desktop projects tables through ordinary cells and preserves diagnostics
for unsupported presentation and controls. A preview link can open only a page
in the active site's manifest while the current document belongs to that site.
Native destination selectors must match the active publication; hex spelling
is case-insensitive. Parent traversal, foreign nodes and unlisted pages fail
that lookup. These are local preview rules, not new transport addresses.

Final locked checks on the published code: 44 document tests passed with one
ignored, all 10 site tests passed, and 53 desktop library tests passed with one
ignored. Dependency trees resolve the immutable Mere/Genet/Netrender family
and the Retinue revision above. These checks do not constitute a new headed
click or complete visual-fidelity receipt.

The Retinue update compresses resource bodies only when smaller. Stock Go
`view-mu` v0.119.0 now retrieves tested compressible 128 KiB native pages
exactly. The same deterministic incompressible 128 KiB body fails in that Go
client against both Retinue and stock NomadNet, while Python succeeds against
both. Full plain multipart Go interoperability remains open.

Forms and partials remain inert in the preview; colors, underline, alignment,
folding, indentation and anchor scrolling need further native work. Wide tables
also expose the retained Smolweb session's lack of horizontal scrolling.
Djinn's resident serving and Tabard's theme exports remain separate work.

### Shared reading presentation receipt (2026-09-13)

The initial receipt used Mere `b5750a96`, Genet `101d9e9` and its matching
Netrender `3961aca` through committed Git selectors. Micron previews retain
source color, background, underline, alignment and section indentation. Styled
link children remain inside their existing navigation control; saving and
publication authority do not change. The same wrappers have an ordinary
document-preview fallback.

The focused immutable Micron gate passes four tests with `--locked --offline`.
The complete desktop library suite passes 54 tests with one existing ignored
outline timing receipt.
The follow-up at Knot `cf3afe8` advances Mere to `dce5cc97`, including clipping
and outline traversal through presentation wrappers. The same desktop library
suite again passes 54 tests with one ignored, using `--locked --offline` and
one immutable Mere/Genet/Netrender source identity each.
It includes a native DOM check for styled link children and nested section
layout, while asserting unchanged source bytes and a clean editor buffer.
This is an automated projection receipt; headed appearance and interaction
comparisons remain open. Djinn's CLI publication/restart receipt lives in its
resident-services plan. A desktop publish-to-Djinn action is still separate.

## Micron navigation consumer (2026-09-16)

This is Knot's half of phase A1 in Mere's
`design_docs/nematic_docs/implementation_strategy/2026-09-15_micron_navigation_plan.md`.
It is on branch `micron-nav-a1`: the bump in `6157a0c`, preview folding in
`4680eb5`, and this record. Stock behaviour, the fold-extent rules and the
decision numbers below are that plan's.

**Bump.** All 35 `merely-made/mere.git` pins moved from `d075c6be` to
`709dfdf2218cf31afdca86987fa6a59d056b83ed`. That commit carries N1 (the
navigation table and `InlineSpan::InPage`), P1 (`inker::FoldState`), P1b
(`inker::FoldMarkers`) and P1c. It was not in the local Cargo git database, so
mere.git was fetched once, online; every other command ran offline. A targeted
`cargo update -p inker` rewrote the 52 Mere lock entries, but it also changed
the dependency lists of 14 unrelated packages, adding and removing none:
twelve crates moved onto windows-sys 0.60.2, pliron onto hashbrown 0.17.1 and
spin 0.12.3, and hashbrown 0.14.5 dropped its two dependencies. Main's lock is
stable under `--locked`, so those edges were not stale. That lock was
discarded. The committed lock changes only the Mere revision on its 52
entries, and `cargo metadata --locked --offline` accepts it. `cargo tree -d --workspace` is line-for-line identical to main's
once paths and revisions are normalized. Retinue appears once (`2563202b`,
through knot-site), so no pin moved.

The bump broke two things: the exhaustive `inline()` matches in
`apps/desktop/src/document_preview.rs` and `apps/desktop/src/scroll_site.rs`
(E0004). Both now render `InPage` as its label. Knot has no `EngineDocument`
literal, and no Knot test asserted the Nematic diagnostics N1 renamed.

**Folds (decision 3).** `DesktopState.micron_folds` holds the preview's
`inker::FoldState` and a `FoldMarkers` token (decision 17), beside the document
and never inside it. After each dispatch, `after_dispatch` reconciles it
against the current source whenever overrides exist and the address or text
has moved. The rule matches `SmolwebDocumentSession::replace_document`. At the
same address every heading key still present keeps its state (decisions 10 and
11), so an edit elsewhere, even one that shifts every line, keeps it, while
editing that heading, marker flip included, drops it. A new address or a
non-Micron document starts fresh.

The preview and the reconcile use the same `lower_micron`. With a current
navigation table, top-level blocks inside `FoldState::hidden` are not built at
all. Each collapsible heading renders as a full-row button showing the marker
glyph, with the heading text as `aria-label` and `aria-expanded`. Cambium's
runner activates a focused button on Enter and Space, so the keyboard path is
the host's own. A toggle re-lowers the source, finds its fold by key, and
changes only `micron_folds`: never source, the document, the site manifest or
an address. A link inside a collapsible heading stops propagation, so the link
wins the click, as the fold region's hit test does in document-canvas.

Choices made here, not captured from stock: a Reload or a reopen at the same
address reconciles rather than resetting, and a heading's accessible name
omits the marker glyph.

Decision 12's visible focus indicator is not established for these toggles.
They are in the Tab order, but Knot's stylesheet has no `:focus` rule anywhere,
the host's own focus overlay draws only the text caret, and no headed check has
looked. The host does restyle on `:focus`, so one sheet rule would supply an
indicator. Whether that rule covers only the preview or all of Knot's controls
is open.

**In-page links: blocked, not done.** Scrolling the preview to an in-page
target, opening closed ancestors first, needs a programmatic scroll, and
Knot's desktop view layer has none. `OwnedLayout::set_viewport_scroll` and
`set_element_scroll` are `pub(crate)` in cambium-rootstock. `HostPointer` has
only move, press and release. `AppCtx` exposes `painted_rect` read-only. Only
the test harness's `wheel` scrolls. The preview pane is not its own scroll
container either: the window viewport carries its offset, as recorded above.
`InPage` therefore renders as inert label text, with no request and no
navigation, and nothing opens folds on its behalf. The open options are:

- a public scroll request on Cambium's host, a Mere change outside A1's Knot
  half;
- a preview scroll container whose offset Knot owns, which answers the layout
  question left open above (whether the preview scrolls apart from the source
  pane);
- label-only in-page links as a recorded deviation for A1 and R1.

Decision 1 does not apply, since Knot has no address bar.

**Verification.** Rust 1.97.1 from `C:\t`, `--offline --locked`,
`CARGO_TARGET_DIR=C:/t/knot-micron-nav-a1-target`, logs under
`Code/testing/knot-editor/micron-navigation-a1/`:

- `cargo +1.97.1 test --manifest-path <worktree>/apps/desktop/Cargo.toml --lib`:
  75 passed and 1 ignored at the bump commit, 79 passed and 1 ignored with
  folding (`a1-07`, `a1-15`).
- `cargo +1.97.1 test --manifest-path <worktree>/Cargo.toml --workspace --exclude knot-desktop`
  at the bump commit: 214 passed, 1 ignored (knot-editor 187, knot-readings 19,
  knot-file-catalog 8, knot-capture 0) (`a1-10`).
- `cargo +1.97.1 test --manifest-path <worktree>/crates/knot-site/Cargo.toml`:
  17 passed, against a copy of main's untracked lock; knot-site has no Mere
  dependency (`a1-21`).
- `cargo +1.97.1 test --all-features --manifest-path <worktree>/crates/knot-document/Cargo.toml`:
  47 passed, 1 ignored, against main's untracked lock with the same revision
  swap (`a1-23`). Turnstone consumes this crate.
- `cargo +1.97.1 fmt --check` is clean on the desktop package before and
  after. knot-site fails only in `tests/submission.rs`, already at `fe550eb`
  (`a1-00`).

The new desktop tests inline the Mere fixture sources they use, byte-identical
to `guide-structure.mu` and probes 08 and 06b. They cover: the in-page label in
both inline paths (probe 03's links); closed extents absent from the DOM,
markers on collapsible headings only, and nested state kept across an outer
close (guide-structure, 06b); pointer, Enter and Space toggles with source, the
dirty flag, address and saved bytes unchanged (08); state kept across an edit
that shifts every line and dropped for an edited heading and a flipped marker
(guide-structure); and a link inside a collapsible heading not toggling it.

Eleven positive controls each broke one rule and watched the named test fail
at its assertion (`a1-08`, `a1-09`, `a1-20-*`): both label arms, the hidden
filter, the marker, the pointer toggle, keyboard activation (a no-op key
handler, with the click still working), a toggle that writes source, a reset on
every edit, no reconcile, no reconcile in `after_dispatch`, and no stop
propagation on heading links. The marker-flip assertion shares the reconcile
path with the heading-text assertion and has no control of its own.
