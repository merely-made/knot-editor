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

Launch an untitled document with `cargo run -p knot-desktop`, or append
`-- path/to/document.djot` to open a file at startup.

The root `Cargo.lock` is committed because this repository ships application
binaries as well as libraries. `knot-document` is an excluded nested workspace
so a surface-only consumer can resolve it without paying for the sync and
publishing graph.

## Embedding

Hosts mount the `knot.document.v1` surface through Genet's generic retained
surface contract. The host owns placement, focus, windowing, and shell policy.
Knot continues to own the document and rechecks every requested effect.

## Application development

The standalone application exposes one document with source editing, New,
Open, Save, Save As, Reload, Compare, and an unsaved-change prompt. Enter a `.djot` or
`.knot` path in the toolbar for Open or Save As; Save As requires a new target.
Saving checks the opened file's identity and bytes for external changes.
Compare displays read-only snapshots of your buffer and the disk text. Refresh
comparison rereads the disk; it does not reload the document or authorize an
overwrite. A buffer edited after comparison is labelled stale.
The optional Outline lists headings from the current source. Selecting a
heading selects its source range and, in editable documents, returns keyboard
focus to the editor. Uncommitted IME text is excluded from the reading.
Ctrl/Cmd+N creates a document, Ctrl/Cmd+O opens the entered path, Ctrl/Cmd+S
saves, and Ctrl/Cmd+Shift+S saves to the entered path.
The broader editor library has capabilities that this surface
does not yet present. The [application workspace plan](design_docs/2026-09-05_knot_application_workspace_plan.md)
connects those capabilities to writing, typed document relationships, saved
graph questions, coordinated presentations, evidence, review, and sharing UI,
using Cambium's reusable components. Its phases are planned work, not a list of
features already available in the standalone application.

The source history was extracted from Mere with path-preserving Git history.
The earlier plans and receipts remain under [`design_docs`](design_docs) and
[`crates/knot-editor/docs`](crates/knot-editor/docs).

Knot Editor is licensed under MPL-2.0.
