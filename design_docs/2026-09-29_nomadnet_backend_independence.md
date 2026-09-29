# Backend-independent NomadNet publication

## Decision

Knot owns native source and explicit saved publication snapshots. The network
implementation is selected separately. Retinue is an optional embedded backend;
exporting a Micron site must not require it. Python RNS remains the protocol
reference. Reticulum-Go is a candidate independent backend and interoperability
peer, not a replacement definition of the protocol.

This change starts with static Micron pages. It does not convert Djot, Gemtext or
Scrolltext, execute page scripts, publish drafts, or add remote submission to an
external backend. Those are separate capabilities with separate acceptance.

## Boundaries

- `Site::publication()` captures the selected saved files.
- `PublishedSnapshotV1` is the validated, bounded handoff; its digest identifies
  the publication independently of a server's destination identity.
- Export materializes a fresh directory for an existing NomadNet/MeshChatX host.
  The operator chooses the live node, lifetime and audience. Export itself opens
  no listener and performs no network requests.
- The `retinue` Cargo feature enables the existing embedded Micron server and
  request client. A build without that feature still edits and exports Micron.
- An external Go service should own its persistent identity and configured
  interfaces. Knot should not translate Retinue identity internals into another
  implementation's storage format.

Retinue connected to an RNS TCP interface is still a Retinue endpoint. It does
not delegate endpoint cryptography, requests or Resources to RNS. Using another
endpoint implementation requires a server process/library integration, not just
changing the TCP address.

## Qualification matrix

Record exact application/stack revisions, configuration, source snapshot digest,
destination, commands and observed outcomes for each run. Never infer a passed
row from another row. Run all experiments on explicitly configured private test
interfaces before any operator chooses a public network.

| Server | Reader | Required evidence |
| --- | --- | --- |
| Existing stock NomadNet with exported pages | stock NomadNet and MeshChatX | Exact text, three-page navigation, missing-page behavior |
| Optional Retinue publisher | stock NomadNet and MeshChatX | Same, plus repeated requests over reused links and Resource completion |
| Reticulum-Go publisher | stock NomadNet and MeshChatX | Same, plus stable destination after process restart |
| Each supported server | independent compatible reader | Interrupted transfer, timeout, reconnect and bounded resource use |
| Each supported server over a constrained interface | independent reader | Measured byte counts, latency, loss recovery and no unexpected repeat requests |

Do not require microReticulum to supply application features it does not expose.
Test its supported endpoint/routing roles separately. A routing-only success is
not a NomadNet page-service receipt.

## Automated gates

The site crate is tested with no default features and with `retinue`. Export tests
exercise immutable saved content, native page paths and destination conflicts.
The embedded backend's existing loopback tests are local implementation tests;
they do not establish independent-client compatibility. CI also checks the
desktop with the optional backend enabled.

An external host test must additionally prove that generated page files are
treated as static data, that unlisted files are not served, and that restart does
not silently select a new identity. A successful build alone closes none of
those interoperability gates.

## Sources

- [RNS reference and recognized implementations](https://github.com/markqvist/Reticulum#community-implementations)
- [MeshChatX hosting and page paths](https://github.com/Quad4-Software/MeshChatX/blob/master/docs/en/nomad-network.md)
- [Reticulum-Go integration surfaces](https://reticulum-go.quad4.io/)
- [Existing Knot native small-web plan](2026-09-11_small_web_authoring_plan.md)

Implementation and local validation results are recorded separately from the
unrun independent-client matrix above.

## Local validation, 2026-09-29

On Linux with Rust 1.97.1, the site suite passed with no default features
(19 tests) and with `retinue` (24 tests). The default suite was repeated after
the neutral destination getter was added and passed. `cargo tree` for the site
crate's normal dependencies with no default features contains no Retinue.
The root lockfile lists `knot-site` as the only direct consumer of Retinue.
The resolved default desktop normal-dependency tree was also checked with
`cargo tree -p knot-desktop -e normal --prefix none --locked`; it contains no
Retinue package.

The export example produced a three-page Micron handoff with snapshot BLAKE3
`0159856a1c5f86ceddac3aa6a7a34686e0b7c2c55218cb24e22842dce2fc027f`.
The example rejected a noncanonical snapshot JSON before writing output, then
accepted the canonical bytes. This is export evidence, not network evidence.

Tests used `CARGO_BUILD_JOBS=1`, `CARGO_PROFILE_DEV_DEBUG=0`, and
`CARGO_PROFILE_TEST_DEBUG=0`. Desktop verification uses the same single-job
settings, CLI Git fetches, and a systemd scope with `MemoryHigh=1200M`,
`MemoryMax=1500M`, and `MemorySwapMax=128M` to respect the development laptop's
8 GB RAM budget. Both `cargo check -p knot-desktop -j 1 --locked` and the same
command with `--features retinue` passed. Missing pkg-config and Fontconfig
development prerequisites were supplied from extracted distribution packages
in a local build prefix; no repository dependency changes were needed.
The optional-backend CI job and desktop checks have been added; remote CI has
not run in this local receipt.

The external Go launcher also passed an independent Python RNS page fetch and
identity-restart check. Exact versions, commands and limitations are recorded in
[the Go backend receipt](2026-09-29_reticulum_go_backend.md). Stock NomadNet and
MeshChatX graphical-client checks and constrained-radio trials remain unrun.

Stock NomadNet 1.2.0 was also started through its actual `--daemon` CLI with the
export's `pages/` directory and explicit loopback-only UDP configuration. A
separate Python RNS 1.5.4 process fetched `/page/about.mu`, `/page/index.mu`, and
`/page/notes.mu` over one link and verified all response bytes against the
exported files. The daemon shut down cleanly. The reproducible harness is
`tools/knot-nomadnet-go/nomadnet_node_smoke.py --export EXPORT --timeout 25`, run
with Python containing NomadNet 1.2.0. This closes the static export-to-stock-node
serving check, not the full reader matrix or missing-page behavior for that node.
