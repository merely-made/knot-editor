# Native small-web authoring: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut from [the small-web authoring plan](archive_docs/2026-11-02/2026-09-11_small_web_authoring_plan.md), archived whole with its Gemini, Spartan, Titan, Scroll and Micron authoring history and inline run evidence. Local publication, reviewed submission, the Errand host adoption, governed native Scroll and Micron serving, the Moot handoff and optional site checkpoints are qualified in the archived plan. Open: Micron fidelity, multi-segment responses, Go multipart interoperability, independent preview scrolling, the persistent-serving gates and the limits carried from the Scroll site and NomadNet records.

## Rulings in force

The archived plan's scope and ownership, implementation lanes and persistent-serving ownership sections stay in force and are cited by section. Its numbered Micron-form decisions (the "Decision 19" focus rule and the choices recorded in the Micron request-form consumer section) are grandfathered and keep their numbers. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `small_web_authoring D<n>`. The `support/receipts/` folder the archived plan links (errand-adoption, micron-publication-runtime, native-scroll-server, shared-site-revisions and others) stays in this repository.

## Decisions

### small_web_authoring D1: cut the plan, draft first (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The 1,037-line plan is cut under DOC_POLICY section 8 and archived whole after this draft is reviewed.

Links: area documentation · replaces 2026-09-11_small_web_authoring_plan · summary old plan cut to open work and archived whole

### small_web_authoring D2: old hashes (2026-11-02)

Mark, 2026-11-02: "Same: live docs only". This plan cites main hashes; the archived plan keeps its own, including the pre-merge hashes at its lines 7 and 145. Mapping: [the receipts file](archive_docs/receipts/2026-09-11_small_web_authoring_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes; archived docs keep old ones

### small_web_authoring D3: landed records archived, their open points carried here (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The Scroll site authoring, NomadNet backend independence and Reticulum-Go backend records are archived as landed; the points they left open are items O5 and O6 below.

Links: area documentation · draws on 2026-09-11_scroll_site_authoring_plan · draws on 2026-09-29_nomadnet_backend_independence · draws on 2026-09-29_reticulum_go_backend · summary open points of three landed records live here

## Open work

### O1. Micron presentation fidelity

The preview renders text, headings, emphasis, links, tables, colour, underline, alignment and section indentation, with collapsible headings and in-page links (archived lines 278, 791-816). Forms and partials stay inert in the preview, wide tables expose the retained Smolweb session's lack of horizontal scrolling, and "headed appearance and interaction comparisons remain open" (lines 791-793, 814-815). Inline form widgets, partial refresh, outgoing request Resources, authentication and dynamic page hosting in Djinn were outside the headed receipt (lines 515-517).

**Done when:** a headed run against a stock NomadNet node exercises inline form widgets and a partial refresh with exact request bytes, a wide table scrolls horizontally, and the appearance and interaction comparison against the reference client is recorded; items outside Knot's ownership (Smolweb scrolling, Djinn hosting) are named with their owners.

### O2. Multi-segment responses

Knot's received-size check now precedes unpacking (`a45707d`), but Retinue's single-segment limit is 1,048,575 bytes and "multi-segment responses remain a separate transport gate" (archived line 489).

**Done when:** a response larger than one segment is received and rendered by the desktop, or refused with a message naming the segment limit, against a stock node, with the cap compared on the received bytes.

### O3. Go multipart interoperability

Stock Go `view-mu` v0.119.0 retrieves compressible 128 KiB native pages exactly; a deterministic incompressible 128 KiB body fails in that client against both Retinue and stock NomadNet, while Python succeeds against both; "full plain multipart Go interoperability remains open" (archived lines 786-789).

**Done when:** Go `view-mu` retrieves the incompressible body exactly from Retinue, or the failure is recorded as a Go-client limitation reproduced against stock NomadNet only.

### O4. Independent preview scrolling

Whether the preview pane should scroll independently of the source pane "is a layout decision left open"; it needs a bounded height on the flex row, not a change to the submission path (archived lines 508-513). Mark's call.

**Done when:** Mark rules yes or no; if yes, a headed run shows the preview scrolling with the source fixed and a submission redraw leaving the offset unchanged.

### O5. Persistent serving: the remaining gates

The ownership proposal is `repos/mere/design_docs/archive_docs/2026-10-28/2026-08-22_djinn_family_resident_services_plan.md`, section 13; the ordinary Gemini snapshot service has landed in Djinn. Still separate gates: headed desktop handoff to Djinn, public binding, additional resident protocols, governed hosting, and the desktop controls for the host policy the Errand adoption exposed only through the API (archived lines 39-40, 366-380, 816). The mesh-publication direction and its first acceptance gate (a cold reader reading a release after the publishing device stops) are in [the writing workspace and mesh record](2026-10-29_knot_writing_workspace_and_mesh.md), "First acceptance gate and consequences".

**Done when:** each gate has its own dated entry with a done-condition and a receipt; Mark names which come first.

### O6. Limits carried from the archived Scroll and NomadNet records

- **Scroll authoring limits** (`archive_docs/2026-11-02/2026-09-11_scroll_site_authoring_plan.md`, "Explicit limits", line 123): there is no Scroll source highlighting or outline, section-jump navigation, external-link launching, automatic heading numbering or input-link prompt; assets are reserved, not published; adding, removing and renaming pages uses the ordinary files and manifest; publication snapshots and localhost certificates live for the serving session only. A headed IME receipt for the Scroll editor also stays separate. **Done when:** Mark rules which of these become features; each takes its own item.
- **NomadNet qualification matrix** (`archive_docs/2026-11-02/2026-09-29_nomadnet_backend_independence.md`, "Qualification matrix", line 36): stock NomadNet and MeshChatX graphical-client checks and constrained-radio trials remain unrun (line 108), and independent reader implementations, radio qualification and client limits keep separate gates (line 134). **Done when:** each unrun row has a receipt recording exact versions, configuration, snapshot digest, commands and outcomes, and none is inferred from another row.
- **Reticulum-Go backend** (`archive_docs/2026-11-02/2026-09-29_reticulum_go_backend.md`, lines 58-62): an unknown-page request produced no rendered "Page Not Found" before the harness deadline, and MeshChatX and constrained-radio acceptance remain unrun. **Done when:** the unknown-page path renders in stock NomadNet's browser components against the Go server, or the cause is recorded.

## Findings

- 2026-11-02: the archived plan's A3-class finding (response cap before unpacking, request bounds, preview scroll, stale reply) was closed by `a45707d`, `7722ede`, `ffb508e` and `7069d38`; `apps/desktop/src/scroll_site.rs:591-604` reads `KNOT_NOMADNET_TIMEOUT_SECS` and `KNOT_NOMADNET_MAX_RESPONSE_BYTES`.
- 2026-11-02: the Errand host adoption (2026-10-24) is qualified at Knot `a4164d5` (the plan's `2474686`, a merge with the same subject and author time on main) against Errand `897fb963` (archived lines 5-72).
- 2026-11-02: `mere/crates/system/errand/src/serve/mod.rs`, cited at archived line 312, now lives at `repos/smolweb/crates/errand/src/serve/mod.rs`.

## Progress

- 2026-11-02: plan cut from the small-web authoring plan; decisions D1 to D3 recorded. The evidence index and old-to-main hash table are in `archive_docs/receipts/2026-09-11_small_web_authoring_plan_receipts.md`.

## Saved-site generation controls, 2026-11-04

The site tile receives Gemot S3 `c218f90` through `gemot-site-host`'s `generate`
feature. Four explicit targets (Gemtext, Scroll, Markdown and HTML), a new
export folder, loss refusal/opt-in and native-byte preservation feed an immutable
saved snapshot. Unsaved source or metadata refuses before starting. A background
build reports conversion diagnostics and exports a verified V2 artifact;
existing output refuses, and building never starts a publisher. Closing a site
releases its control draft. Native Micron publishing is unchanged and its
conversion refuses through the shared writer.

Shared writer and community scope lives once in
`repos/gemot/design_docs/2026-11-04_site_extraction_plan.md`, S3-S5. This control
slice does not close the persistent-serving or remote protocol gates above.
Required receipts are the three native build/widget cases and the full desktop
library suite, retained within the 100 MB shared lane at
`Code/testing/gemot/discovery-ui-20261104`. The automated widget uses the actual
site tile and Genet layout/hit testing; it is separate from native headed or
remote-machine qualification.

Qualification: Knot `2279669` is public. `knot-desktop-suite` passes 374 cases
with one helper ignored. `knot-full-site-widget` passes all three writer-control
cases, including clicking Markdown and Build inside the complete site tile.
Commit line-ceiling and locked all-target Clippy checks pass. The source-graph
audit retains every existing Git package record outside the two site packages;
the writer adds only pulldown-cmark's existing HTML-rendering escape dependency
and its dependency edge. These are automated Windows/native receipts, without
a headed or remote-machine claim.

## Headed generation qualification, 2026-11-05

The real Windows desktop, launched by `site_generation_acceptance` with marked
synthetic files, exercises Markdown selection, ordinary keyboard path entry,
existing-output refusal, completion without another input, and unsaved-source
refusal. Both exports retain identical checked bytes and the footer remains
not serving. The first native run exposed missing generation tracking in
`submissions_busy`; the fix includes generation in the existing wake drain.
The focused writer/widget cases pass with a regression assertion on that flag.
The helper's bulk Unicode insertion was ignored; ordinary key input worked.

Evidence is `Code/testing/gemot/headed-protected-20261105/headed-receipt.json`,
screenshots and fixture files. The lane's initial 100 MB estimate was refined
to 200 MB to preserve preallocated native storage files; the adjustment is
recorded and original evidence retained. This qualifies the local native
controls, separately from remote protocol delivery or installed personal custody.

The corrected desktop suite passes 374 cases with one helper ignored at two
test threads (`knot-headed-regression-qualified`). The earlier parallel run
missed the recovery typing test's flush deadline; its exact retry passes and
the bounded-concurrency full run passes. Both attempts remain in the lane.
The native fix and fixture are public at `0bccade`; line-ceiling and locked
all-target Clippy checks pass.
