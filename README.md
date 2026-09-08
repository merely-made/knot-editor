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
- `crates/knot-file-catalog`: durable file identities without the editor's
  preview, publishing, or replication dependencies.
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

The library's first connected-writing model retains signed assertions between
captured vault document revisions, including optional source passages, predicates,
and author-only retraction history. These records remain separate from document
replacement and from graph presentation. Hosts must supply admitted document ids
for filtered relation reads. The desktop does not yet expose link authoring;
graph views remain planned work.

Hosts can opt into durable ordinary-file identities with `KnotFileCatalog` and
`DirectorySource::with_catalog`, then serve that source through
`KnotEndpoint::from_directory_source`. The host selects a local catalog outside
the scanned root. Registered paths retain their ids across saves and restart;
copies get distinct ids, and moved files require explicit rebind before their
new paths are registered. The default desktop and directory discovery do not
automatically create catalogs.

For file relations, `DirectorySource::capture_file_revision(id, max_bytes)`
prepares a bounded snapshot of disk bytes under a catalog ID. A host can then
explicitly sign and seal `KnotSyncEvent::CaptureFileRevision` in an admitted space
and use its operation hash in a relation endpoint. Captures stay outside editable
vault documents and publication reads; `KnotSyncStore::file_revision` reads an
exact retained capture. Later disk edits leave earlier relation targets intact.
Preparation excludes unsaved editor changes and does not replicate anything.
The host must authorize storage and disclosure of captured bytes separately.
Desktop signing/link controls remain open; older replicas need upgrading before
reading spaces containing the new capture event.

The desktop can explicitly enable a catalog at launch:

```sh
cargo run -p knot-desktop -- --catalog-root ./notes --catalog ./state/files.redb ./notes/essay.djot
```

The document path is optional; both catalog options are required together. The
root and catalog's parent directory must already exist, and the catalog must be
outside its root. The desktop displays the current document's ID, or why it is
not catalogued. Save As creates a distinct binding for a new path. Files outside
the catalog root can still be edited and saved; catalog status does not grant or
remove file-write authority. A catalog failure never undoes a successful save.
Use the displayed retry action for catalog errors. Directory navigation, catalog
configuration within the window, and rename/rebind controls remain future work.

With a catalog enabled, **Review saved revision** prepares the current file's
saved bytes for inspection. The panel shows the document ID, path, media type,
size, and source text. Unsaved editor changes are excluded. The reading stays
historical through editing and ordinary Save; **Refresh saved revision** reads
disk again, and **Discard saved revision** clears it. Successful New, Open,
Reload, or Save As also clears the reading. Read failures replace the old
reading with an error; non-UTF-8 files are refused by this text-review panel.

The default capture limit is 1,048,576 bytes. Add `--capture-max-bytes N` alongside
the catalog options to choose another limit, including zero for empty files
only. Review keeps bytes in memory and does not sign, persist, or share them.
An explicit persona/space storage adapter is the next gate for retention controls.

The source history was extracted from Mere with path-preserving Git history.
The earlier plans and receipts remain under [`design_docs`](design_docs) and
[`crates/knot-editor/docs`](crates/knot-editor/docs).

Knot Editor is licensed under MPL-2.0.
