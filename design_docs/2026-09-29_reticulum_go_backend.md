# Reticulum-Go backend for NomadNet publishing

**Date:** 2026-09-29

**Status:** Implemented and verified as an external-CLI integration.

**Scope:** Run an installed Reticulum-Go static page server over an explicit Reticulum configuration, using an exported Knot Micron snapshot as its page tree.

## Decision

Reticulum-Go provides a NomadNet-compatible publishing backend without requiring Python at runtime. Its supported `reticulum-go pageserver` command serves the Reticulum `nomadnetwork.node` destination and static `/page/*` content. Knot does not implement Reticulum, LXMF, NomadNet request handling, destination hashing, announces, or page transport. This integration validates Knot's exported snapshot, stages its listed bytes, and invokes that upstream command.

The launcher uses the upstream CLI instead of linking `pkg/pageserver`. The CLI owns node startup, transport registration, announcements, signals, and shutdown. The upstream server can execute `.mu` pages when they are executable scripts, so the launcher rejects executable inputs and writes staged files without execute permissions.

## Export input

The input is a complete Knot Micron export generation: `snapshot.json` at the root and one file under `pages/` for each `snapshot.pages` entry. Version 1 uses slash-prefixed page keys such as `/index.mu`; each entry has metadata and exact source bytes encoded as a JSON integer array. `/index.mu` is required. The launcher checks the fields it needs, enforces export bounds, rejects symlinks, special files, executable files, unsafe paths, case-folded duplicate names, unexpected page files, and unexpected export-root entries, then copies manifest bytes to a private temporary tree. Page-file size is checked before reading and reads are bounded. Later edits to the export cannot change the staged copy.

The helper does not make a mutable directory into a retained Knot history store. Snapshot export is a point-in-time view; version history, withdrawal policy, Knot publication identities, and recipient authorization remain Knot concerns and are not represented by the generic NomadNet page server.

The validator checks the static-serving subset of the format. It does not
duplicate Rust's full language-tag/date rules or enforce canonical JSON, and
does not authenticate the snapshot. Knot's Rust exporter performs complete
`PublishedSnapshotV1` validation before this handoff.

## Identity and network boundary

`serve` requires an existing Reticulum-Go config file and an explicit persistent identity path outside the export. It never silently creates or selects a default config. The identity file may be absent on first start; upstream creates it at the supplied path, whose parent must exist. This identity is separate from Knot's author identity. Sharing a Knot signing key as a Reticulum identity needs a distinct, reviewed key-derivation and persistence contract; this helper does not do that.

Starting the upstream pageserver sends an initial announce on the interfaces enabled by the supplied config. The helper's `validate` command is offline. `serve` does not edit network configuration or add interfaces. Periodic announcements are disabled by the launcher; upstream still sends its initial announce. The integration smoke used UDP interfaces bound to 127.0.0.1 on both peers.

## Reticulum-Go support and licensing

The research checkout was `Quad4-Software/Reticulum-Go` commit `eda179dd5ea9a66ee96ce4753308d754d566a471`. The runtime smoke used the official Reticulum-Go v1.3.2 Linux amd64 release, commit `b0f46f88f09a`, SHA-256 `10e82e43fff5bf2e5272463be12007b673b2d29d73c7633cdcbd578326334f00`, built with Go 1.27.1. The independent page client used Python RNS 1.5.4. This establishes a tested `/page/index.mu` exchange for those versions; it does not establish compatibility of Knot's author identity with Reticulum identities or full NomadNet UI parity.

Reticulum-Go uses the custom Reticulum License, which includes use and dataset restrictions. This repository does not copy or translate Reticulum-Go source. The helper invokes an externally installed binary, so distributions that bundle that binary must retain its license and copyright notices and review those terms. The repository's third-party inventory records this external runtime.

## Commands

`knot-nomadnet-go validate --snapshot DIR` verifies an export without opening Reticulum or sending traffic. `knot-nomadnet-go serve` requires `--snapshot`, `--config`, and `--identity`, accepts `--reticulum-go` to select the upstream executable, and optionally accepts `--node-name`. It launches the upstream `pageserver` with a private copy of the verified page set, an empty private files directory, page statistics disabled, and source refresh disabled. It does not expose unrelated export contents.

## Verification

`go test -p 1 ./...` passed for the helper with Go 1.27.1. Unit coverage checks integer-array byte decoding, exact file-byte matching, executable/symlink and unlisted-file rejection, unsafe names, identity location, private staging, source-edit isolation, temporary-tree cleanup, and child exit propagation. The helper validated the exported three-page fixture (157 source bytes).

The reproducible [loopback smoke script](../tools/knot-nomadnet-go/loopback_smoke.py) started the helper twice with the same explicit identity path. Python RNS requested `/page/index.mu` with no page variables and received response bytes equal to the exported file on both starts. The destination hash stayed `5a5ad2809cff574117f01ff2a083a1ba`; both instances exited cleanly on SIGTERM. The script creates temporary Go and Python configs with one UDP interface on 127.0.0.1 and removes its identity and configs on exit.

Stock NomadNet 1.2.0 Browser, MicronParser and urwid widgets were exercised with
a minimal headless application scaffold against the Go server. The test
verified exact bytes and rendered headings for index, About and Notes, and
activated the parser's native `:/page/...` links between them. This exercises
stock browser components, not a full-screen interactive terminal session.
The [browser harness](../tools/knot-nomadnet-go/nomadnet_browser_smoke.py) uses
the included three-page fixture and paired loopback-only UDP interfaces.

A separate unknown-page request did not produce a rendered “Page Not Found”
response before the harness deadline. This is an outstanding negative-path
acceptance check, not part of the successful three-page result. MeshChatX and
constrained-radio acceptance remain unrun.
