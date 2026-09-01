# Knot Editor

Knot Editor is a files-in-place, local-first Djot editor. It can run as a
standalone desktop application or contribute the same retained document
surface to hosts such as Turnstone.

Knot owns document authority: opening, editing, saving, revisions, evidence,
vaults, peer replication, and publishing. Mere supplies reusable graph,
identity, policy, transport, and resident contracts. Genet and Cambium supply
the document presentation and desktop host.

## Workspace

- `crates/knot-document`: the narrow one-document model and reusable Cambium
  surface.
- `crates/knot-editor`: file, vault, evidence, sync, publishing, and
  Graphshell-facing authority.
- `apps/desktop`: the standalone native host for `knot-document`.

## Build

The workspace uses Rust 1.97.1 and pins its Mere and Genet source identities.

```sh
cargo test --manifest-path crates/knot-document/Cargo.toml
cargo test -p knot-editor --lib
cargo test -p knot-desktop
```

The root `Cargo.lock` is committed because this repository ships application
binaries as well as libraries. `knot-document` is an excluded nested workspace
so a surface-only consumer can resolve it without paying for the sync and
publishing graph.

## Embedding

Hosts mount the `knot.document.v1` surface through Genet's generic retained
surface contract. The host owns placement, focus, windowing, and shell policy.
Knot continues to own the document and rechecks every requested effect.

The source history was extracted from Mere with path-preserving Git history.
The earlier plans and receipts remain under [`design_docs`](design_docs) and
[`crates/knot-editor/docs`](crates/knot-editor/docs).

Knot Editor is licensed under MPL-2.0.
