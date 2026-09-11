# Knot Editor

Knot Editor is a files-in-place, local-first native-source editor for Djot, Scrolltext and Gemtext. It can run as a
standalone desktop application or contribute the same retained document
surface to hosts such as Turnstone.

Knot owns document authority: opening, editing, saving, revisions, evidence,
vaults, peer replication, and publishing. Mere supplies reusable graph,
identity, policy, transport, and resident contracts. Genet and Cambium supply
the document presentation and desktop host.

## Workspace

- `crates/knot-document`: the narrow one-document model and reusable Cambium
  surface.
- `crates/knot-site`: native small-web site folders and explicit loopback
  publication snapshots, reusable by other Knot hosts.
- `crates/knot-file-catalog`: durable file identities without the editor's
  preview, publishing, or replication dependencies.
- `crates/knot-capture`: lightweight host-issued retention targets and receipts.
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

## Scroll sites

Choose **Site**, select **Scroll**, enter a new folder path, and select **Create site**.
The parent folder must exist. Knot creates `site.json`, `index.scroll`,
`about.scroll`, `notes.scroll`, and an empty `assets/` directory. The three pages
link to one another. **Open site** opens an existing folder; the same folder can
be passed at launch: `cargo run -p knot-desktop -- path/to/site`.

Select a page or follow one of its local preview links to edit native Scrolltext.
The preview uses Mere's Nematic Scroll engine and the committed source buffer.
**Save** retains the source's exact UTF-8 text, including existing line endings.
Save As accepts another `.scroll` filename and refuses implicit conversion to
Djot/Knot. External source changes retain the usual Compare/Reload safeguards.
Scroll syntax highlighting and a source-addressed outline remain unavailable.

**Metadata** edits the selected page's author, BCP47 language, UDC class (0–9,
default 4), UTC dates, and native Scrolltext abstract. Dates may be blank; Knot
does not invent a publication date. The supported date entry is RFC3339 with
a `Z` suffix. **Save metadata** updates `site.json` with an external-change check.
Metadata edits must be saved or discarded before changing pages or closing.
These values are separate from the body: author and dates occupy Scroll's
header lines, language is a MIME parameter, and UDC chooses status 20–29.
An abstract request (`+` before the language list) returns the abstract instead
of the body, with the resource's same metadata.

**Publish locally** captures saved pages and metadata into a process-local
snapshot and serves it at the displayed `scroll://localhost:PORT/` address.
Saving later drafts does not update it. Publish locally again to replace it;
**Stop serving** or closing Knot releases it. Choose the port before starting
(5699 by default; 0 chooses a free port). Changing the port requires stopping
and publishing again. Only IPv4 loopback is bound. A new self-signed localhost
certificate is generated per serving session, so an independent client may
require accepting a new local identity after restarting.

This is the Scroll small-web protocol and `text/scroll`, not scroll.pub. It
requires neither Turnstone nor Gemot nor a vendor service. This initial slice
serves only the manifest's `.scroll` pages, with one language per page and a
16 MiB total snapshot limit (1 MiB per body or abstract). Assets, nested page
paths, automatic language variants, persistent releases, external hosting, and
other output formats remain future work. Preview links navigate plain local
page paths; external links and section jumps are displayed but not followed.
The engine reports its input-link rendering limitation in the preview.

Validation and the independent-client receipt are recorded in the
[Scroll site plan](design_docs/2026-09-11_scroll_site_authoring_plan.md).
Run the reusable site's focused tests with
`cargo test --manifest-path crates/knot-site/Cargo.toml`.

## Gemini, Spartan and Micron files

Choose **Gemini** or **Spartan** before creating a site. Both create native
Gemtext `index.gmi`, `about.gmi` and `notes.gmi` files. Their previews use the
shared native renderer. **Publish locally** serves exact saved bytes through
the corresponding shared protocol server (Gemini TLS on port 1965 by default,
Spartan TCP on port 300; use 0 for an available port). Static Spartan sites
refuse uploads and never execute page files. The same explicit snapshot,
replacement, loopback and Stop boundaries apply as for Scroll.

Knot also opens `.gmi`/`.gemini` and `.mu`/`.micron` ordinary files. Micron support
currently means raw editing and exact saves; its native parser and NomadNet
page transport need independently specified adapters. Micron preview and
serving report that limitation. Gemtext, Micron and Scrolltext cannot be
converted by renaming a Save As target.

The submission controls prepare either a saved file for **Titan** or a typed
**Spartan** body. Review the target, MIME, byte count, digest and exact body,
then choose **Send reviewed bytes**. Preparation does not connect. A review
holds immutable bytes even if the file later changes. Sending consumes it;
redirects are returned as receipts and are not followed automatically. Titan
accepts an optional masked token, which is cleared on sending or cancellation.
It uses durable first-contact certificate pins; a changed certificate refuses
an upload. A timeout can leave the remote outcome unknown, so check the endpoint
before preparing a retry. This first Knot upload surface does not select client
certificates. A compatible endpoint may require one; Turnstone has a separate
identity-selection flow.

The [native small-web plan](design_docs/2026-09-11_small_web_authoring_plan.md)
records tests, independent client receipts, remaining Micron/Reticulum gates,
and the separate Djinn persistent-serving proposal. The site-library Gemini
implementation was browsed with Lagrange 1.21.1; ordinary readers need neither
Turnstone nor Gemot.

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
Appearance controls switch between Tinct light and dark palettes, source
highlighting, 12–24 px type, compact or relaxed spacing, and narrow or wide
writing areas. These preferences last for the app session and survive document
changes. Highlighting decorates editable Djot and legacy Knot source through
Cambium's existing text input; read-only, Markdown, and JSON views remain plain.
Embedded hosts opt into the `highlight` feature and
`knot_document_view_with_highlighting`, and supply their own syntax stylesheet.
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
An existing `KnotResidentSource` can issue a `KnotFileCapturePort` with an explicit
`KnotCaptureGrant` of document IDs and a byte limit. `prepare` binds the reviewed
revision to its space and writer; `retain` rechecks authority and returns a signed
operation receipt. Exact same-writer retries reuse the retained operation,
including after reopening the resident. The port does not open a vault, migrate
documents, or select a persona. Host destination selection remains required.
Capture lookup and signing share an async store gate with ordinary Knot writes
and incoming replication. Run the blocking retention port on a worker; direct
writes through the underlying Muniment handle bypass this coordination.
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
An owning host can launch `knot_desktop::run_desktop_with_targets` with granted
`KnotRetainPort` capabilities. `KnotResidentRetainPort::new` binds a host-selected
persona display identity to an existing resident's `KnotFileCapturePort`.
Destination choices use short labels; the selected detail shows the full persona,
space, writer, and encryption identity before Retain.
Choose a destination, then **Retain reviewed revision** to store the exact reviewed
snapshot. Retention runs on a worker and rechecks current authorization. Its
receipt identifies the original document, destination, and signed operation;
switching documents does not redirect an in-flight write. Retry reuses an exact
same-writer capture. Retaining a revision does not save the editor buffer or
select a replication authority.

The default executable supplies no destinations. Persona bootstrap and resident
selection in that launcher remain open work. The `resident-retention-tests`
feature enables the real resident integration fixture; default desktop builds
keep the editor runtime dependency disabled.

The source history was extracted from Mere with path-preserving Git history.
The earlier plans and receipts remain under [`design_docs`](design_docs) and
[`crates/knot-editor/docs`](crates/knot-editor/docs).

Knot Editor is licensed under MPL-2.0.
