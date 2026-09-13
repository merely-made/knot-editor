# Knot Application Workspace Plan

**Date:** 2026-09-05
**Status:** A1 lifecycle, A2 live outline, session appearance controls, source-linked preview headings, exact fold readings, and the bounded desktop folded-source consumer are implemented through 2026-09-13; remaining workspace acceptance and G1-G3 product work remain open
**Owner:** Knot Editor

## Ruling

Knot's next application target is a coherent writing workspace:
safe document ownership, a source-first editor, and optional readings of that
same source. **Expanded 2026-09-05:** connected writing is a core product target:
documents and passages participate in an addressable graph with meaningful
relationships, saved queries, algorithms, and multiple editable presentations.
Evidence, revision, and sharing then become adjacent views of the
document, rather than separate administration applications.

The standalone application and an embedded Knot surface should use the same
product components and truthful authority snapshots. A host still owns window
placement, theme, and the policy for presenting a contributed surface. Knot
owns source selection once admitted, file and vault writes, evidence custody,
revision resolution, and publication decisions. Cambium and `workbench` supply
generic tabs, splits, floats, menus, command surfaces, and focus-preserving
component composition; they do not acquire document, vault, or peer authority.

This is an application plan, not a replacement for the completed retained
surface and second-provider work in
[`2026-08-24_knot_shared_surface_and_port_contribution_plan.md`](2026-08-24_knot_shared_surface_and_port_contribution_plan.md).
That plan's `knot.document.v1` remains the narrow embeddable seam. This plan
grows the standalone product around it and promotes host-visible surfaces only
when they have a product-owned snapshot and effect boundary.

## What exists and what the application has not adopted

| Concern | Existing Knot authority or API | Current standalone surface | Required application work |
| --- | --- | --- | --- |
| Source editing | `KnotDocumentSession` delegates to the one Cambium `TextInput`; `KnotEditor` retains file identity and baseline bytes for guarded writes. | Single-document New/Open/Save/Save As/Reload/Compare, a caller-entered path, shortcuts, and dirty-transition prompts. | Multiple documents, recovery, and headed acceptance. Djot is the default authoring route. |
| Derived readings | Outline, preview, and fold snapshots remain bound to the retained source and address. Existing `engine` APIs derive highlights, folds, and preview from that source buffer. | Optional live heading outline; desktop Djot/Knot source decoration, read-only visual folds, and Tinct appearance controls; optional live preview whose rendered headings select exact source spans. | Source ranges for every preview block, cross-launch preferences, and headed writing acceptance. |
| Lexical lenses | `rosette::project_rosette` returns source-addressable rhyme, stanza, meter, and lexicon-coverage readings with configurable geometry. | Not mounted. | An optional lens panel and source selection bridge. Rosette remains read-only and derived. |
| Files, vaults, and search | File sessions, `KnotVault`, and disk/vault search are separate authority APIs. | No chooser, document list, vault list, or search view. | A source adapter and document navigator that reports unavailable/locked/denied states without guessing authority. |
| Evidence | Clip provenance parses portable content references; host-injected stores retain and verify exact bytes; Web Annotation selectors preserve source and quote/position anchors. | No evidence affordance. | A clip/reference panel that can insert, inspect, fetch, and verify only through an admitted evidence authority. |
| Revisions and replication | `KnotDocumentProjection` exposes causal heads, conflicts, automatic text merges, and pending operations. | No revision or conflict view. | A review panel and explicit resolve flows. Titled drafts are a product capability still to be designed, not an existing sync type. |
| Publishing and sharing | Publication candidates, pinned-head tickets, recipient delegation, and revocation APIs exist. | No publishing interface. | A share flow that states what exact revision is offered, to whom, for how long, and whether it remains available. |

The implemented guarded save retains file identity and bytes from opening or
the last successful save. It refuses a dirty save after an observed external
change. Local file custody and an endpoint rejecting a stale opaque base token
still need separate visible refusal states. The file check does not establish
transactional coordination with arbitrary external writers.

## Workspace layout

The default layout should be deliberately quiet: a document navigator on the
left, the writing area in the middle, and one optional right-hand panel. The
right panel changes among Outline, Preview, References, Lenses, Changes, and Share;
it is not five permanent columns. Tabs represent open document sessions. A
document can move between tab stacks or a floating window through the shared
workspace tree, while its document identity and product state remain in Knot.

```text
Document navigator       Writing area                  Context panel
------------------       ---------------------------   ---------------------
Open / recent             title, source and state       Outline
files or vault             source editor                 Preview
search results             inline diagnostics             References
                           status: save / sync / lock     Lenses
                                                         Changes / Share
```

The source remains authoritative. Preview, outline, highlights, Rosette,
evidence anchors, search results, merge comparisons, and sharing status are
readings of a source or causal head. A range-bearing reading selects the source
range it describes; document-level facts select the relevant document or head.
Neither edits a second authoritative document representation. If a
reading cannot be generated, the writing area remains usable and the panel
explains why.

The workspace model is an application preference, not a vault fact. A saved
layout may remember open pane kinds, splits, floats, and panel visibility, but
must not silently reopen a locked vault, re-authorize evidence access, or
convert a stale share route into a current one.

## Product cuts

A1 and A2 form the first usable writing workspace. Layout, typography, command
access, and derived-view experiments can advance alongside file-lifecycle work;
they do not wait for all storage or sharing work to finish. The safe-save gates
control acceptance of the workspace. A3 and A4 add independently useful product
views as their concrete authority adapters become available.

G1-G3 below are the connected-writing sequence. G1 begins alongside A1/A2;
it does not wait for publishing or the lexical graph. These are planned
consumer integrations and domain work, not claims that the current standalone
editor already supports them.

### A1. File-safe writing workspace

**Dependency for tabs and docking:** a pushed Mere revision containing Workbench W5 S1/S2 and
`cambium::workspace`. Neither Knot's published `d82afa17` family pin nor the
in-flight `b9b0ee13` update should be treated as that adoption: the latter was
checked and does not contain `cambium/src/workspace.rs`. Align boundary types
on one immutable source identity before compiling the new workspace; do not
absorb the unrelated in-flight Cargo edits into this work. The single-document
file lifecycle below can use the current pinned controls and shared host.

Build the first standalone workspace around a product-owned `DocumentWorkspace`
model. It owns the open-document list, active document, transient recovery
state, and the mapping from a workbench tile to a document session. It does
not put source bytes or file handles in `workbench::Workspace`.

`New` creates an untitled Djot scratch document. `Open` and `Save As` request
a path through a host-provided picker or a caller-supplied path capability;
Cambium does not perform filesystem selection. Save As validates the Djot
target, records the file identity and baseline only after a successful write,
then changes the session from scratch to file-target posture. A failed or
cancelled picker leaves the scratch document intact.

On opening a file, retain a baseline that can distinguish the opened bytes and
file identity from the latest disk observation. Before saving a dirty buffer,
compare the current disk state with that baseline. If it changed, refuse the
ordinary Save and show an external-change state with three explicit actions:
reload the disk version, compare it with the buffer, or save the buffer to a
new path. Replacing the changed target requires a separately named, deliberate
overwrite action. The normal shortcut may never silently perform that action.

The first Compare presentation is two read-only source snapshots, labelled
buffer and disk, with an explicit refresh action. It records whether disk
bytes or identity differed from the saved baseline at observation time. It
does not claim to watch the file. Subsequent buffer edits mark the comparison
stale; a failed refresh replaces the old reading with an error. The disk
column remains explicitly historical even if an inner document Save changes
the file without changing the buffer. Inspection
preserves selection, undo, dirty state, the saved baseline, and any pending
close decision. Diff highlighting and deliberate overwrite are later work.

The write implementation must also preserve the old file on a failed replacement.
Its receipt records atomic-replacement behavior and the limit of external-writer
coordination; a pre-write comparison alone is not a proof against a concurrent
write between the comparison and replacement.

A native close or tab-close of a dirty document asks to Save, Discard, or
Cancel. Save proceeds only when its custody check succeeds. A session with an
unresolved external change remains open. Clean file documents, scratch
documents never edited, and read-only documents close directly.

The navigator starts modestly: Open, New, recent files, and currently open
documents. A vault or directory listing enters only through an adapter that
can state `available`, `locked`, `denied`, `unconfigured`, or `failed`; it
does not make every source look like a local file.

*Done when* a headed desktop receipt creates a scratch Djot document, edits it
through IME-capable text input, Save As writes and reopens it, and tab-close
preserves the chosen Save/Discard/Cancel outcome. A test which externally edits
an open file after the buffer becomes dirty proves that ordinary Save never
overwrites those bytes. The compare, reload, and Save As paths each leave a
truthful dirty/baseline state. A stale endpoint base token is displayed as an
endpoint refusal, not as an external local-file change.

### A2. Source, structure, and preview

The first A2 implementation slice is a live, optional heading outline. It
derives rows from the current source buffer, and activating a row selects
the original heading range and returns keyboard focus to the editor. A
source/address check rejects a row captured from an older document state.
Preedit text remains local to the input until committed. Outline visibility
is a view preference; selecting a heading changes neither saved bytes nor
the dirty baseline. Preview and folding remain separate acceptance work within A2.

The 2026-09-09 appearance slice adds session-level light/dark Tinct palettes,
highlight visibility, 12–24 px source type, writing width, and line spacing.
The desktop enables the optional `knot-document/highlight` feature and uses
Cambium's styled textarea over the existing `TextInput`. Djot and legacy Knot
receive the shared default note highlighter; Markdown, JSON, and read-only
documents retain plain rendering. This does not enable the broad `engine`
feature or change source bytes, selection, undo history, or save authority.
Embedded hosts retain plain presentation by default and own their styles when
opting in. Preferences survive document transitions within the running app;
cross-launch preference storage remains open.

The 2026-09-12 preview slice enables the existing engine in the desktop and
projects the current Djot or Knot source beside the editor. The pane is hidden
by default, labels its source address and renderer diagnostics, and never owns
editable text. Activating a rendered heading selects that heading's exact UTF-8
source span and returns focus to the editor. The public preview snapshot binds
the rendered document and heading rows to its source text and address. The
fold snapshot similarly publishes exact section, list, quote, code, and div
ranges, with guarded selection. The 2026-09-13 visual-fold slice adds an
explicitly read-only folded source reading for Djot and legacy Knot. It uses
the shared Cambium fold projection with conceal ranges beginning after each
opening line, so the opening line is retained and the remainder becomes a
semantic folded-content marker. The desktop keeps the full
`KnotFoldSnapshotV1` with each action, rejects stale address/source/fold-set
actions, normalizes nested and crossing ranges, and clears transient fold state
on source or document transitions. **Edit source** returns to the ordinary
styled textarea. Native protocol formats have no fold controls. Knot pins the
shared Mere revision containing `fold_projection` for the desktop receipt.

The next visible slice is file navigation, tabs and
recovery, followed by a document-to-document relation workflow. Larger
document responsiveness and headed typography, caret, and IME acceptance still
need measured receipts before this phase is called complete.
Outline row indices are transient positions in one reading, not durable passage
identities. G1 still needs explicit anchor and revision rules before persisting
relations to passages.

Adopt existing readouts without adding another text model. Outline uses the
shared editor's already-available lightweight parser through Knot-owned
snapshot types. Enable `knot-document`'s `engine` feature when the desktop
actually adopts preview or another reading requiring it; retain the narrow
default feature set for consumers that do not need that dependency graph.
Source is the default mode. Its decoration uses the existing highlights;
outline rows select source spans; fold controls conceal source ranges in the
editor view only. Preview is a second reading of the active source in the
right panel or a sibling tab, with a clear source/preview toggle and a
source-range return path for inspectable elements.

Preview may lag while parsing or rendering, but it labels the source revision
it represents and never becomes a save target. Parse or conversion failures
stay local to the reading and preserve the last valid diagnostic information;
they must not erase the source or make a dirty document appear clean.

*Done when* source edits update outline, folds, highlights, and preview from
one buffer; selecting an outline or preview item focuses its source range; a
Djot source -> preview -> source interaction leaves exact source bytes and
selection intact; and malformed source presents a diagnostic while Save and
source editing still work. Include Unicode/IME and a realistically large
document probe so source mapping and input behavior are demonstrated beyond
ASCII fixture text.

Two shared presentation seams now bound the remaining work. Inker's
`EngineDocument` needs optional source ranges for rendered blocks so paragraphs,
lists, quotes, code, and tables can return to their source without guessing from
rendered text. Cambium's styled textarea needs a folding projection which hides
source ranges while preserving byte offsets, caret motion, selection, undo, and
IME composition. Knot should consume those generic contracts rather than keep a
second block map or mutate its authoritative source.

### A3. References and lexical lenses

Add a `References` panel for the active source range and a `Lenses` panel for
Rosette. The references view distinguishes a declared clip reference from
bytes currently available to inspect. When evidence is available, it reports
canonical URI, content identity, media type, byte size, role, verification
result, and selector resolution. When custody is absent, denied, locked,
stale, or verification fails, it says so without inventing replacement bytes.

Clip insertion is an explicit command over an external document observation:
an admitted producer, such as Turnstone's browser, supplies the captured bytes,
canonical source URI, media type, artifact role, and selectors. Knot asks the
admitted evidence authority to retain that artifact and inserts provenance
through the existing versioned clip intent, then the appropriate save/commit
path. The standalone app must expose a producer/import adapter before offering
Capture. Referencing authored Djot is a separate operation and must not silently
archive it as an external observation. Insertion is never a background edit.

Rosette presents rhyme, meter, and coverage as optional readings. Its existing
geometry and lens toggles become per-workspace preferences. Coverage stays
visible. Alternative pronunciation providers and filtering thresholds require
a concrete consumer before they become settings. A compact list is enough for the first lens
panel, but Rosette's existing source-addressable graph projection remains an
alternative working view for relationships among lines, stanzas, and rhymes.
It should stay optional until a document-focused graph interaction earns its
place; Knot need not invent a generic graph schema to expose it. Selecting a
line, stanza, chord, or meter row selects its documented byte span in the
source. Coverage informs the writer; it does not claim that an unknown word
has a guessed pronunciation.

*Done when* a retained clip can be inspected from source selection through a
verified evidence read, while a denied or missing store is visibly distinct;
the same source span round-trips through a Web Annotation selector; and a
Rosette list or graph interaction focuses the intended source range while
unresolved tokens remain reported rather than inferred.

### A4. Changes, drafts, and sharing

The `Changes` panel first exposes only facts already provided by the causal
projection: the current document head, pending operations, automatic merges,
and concurrent versions. It provides source-first comparison and an explicit
resolve command whose result passes back to the owning sync/vault authority.
Conflicts appear in the writer's working document, rather than as detached
opaque records. A conflict view cannot mark a resolution complete until the
authority returns the resulting head or refusal.

Titled drafts are the next design gate, not a quick label attached to a local
dirty buffer. The desired review unit is a titled set of related causal edits
with an explicit base, authorship, and outcome: keep private, offer for
review, merge, supersede, or abandon. Before implementation, settle whether a
draft is represented by a new signed event family or a local grouping over
existing causal operations, and prove how it rebases when another draft
merges. Do not present it as live co-editing or turn remote cursors into a
requirement.

The `Share` panel is present only for a selected publishable document and an
admitted publishing authority. It names the source revision, publication
scope, recipient identity, optional expiry, pinned-head choice, endpoint
availability, and revocation outcome. It performs publish, ticket creation,
copy/export, and revocation through product commands. An unavailable endpoint
or unconfigured persona is actionable status, never a fake success state.

*Done when* a two-writer receipt shows a clean automatic merge and a real
conflict in the same document workspace, then records an explicit accepted
resolution or refusal. A sharing receipt creates a least-privilege,
pinned-head ticket for one selected revision, reads it as the recipient,
revokes it, and shows the new availability state without exposing vault keys
or local paths. A titled-draft slice may begin only after its representation
and rebase behavior are recorded and tested.

## Connected writing: Mere as the graph foundation

**Direction approved 2026-09-05.** Treat the graph as structured material that
can be authored, queried, and transformed. A network picture is one reading of
it. Obsidian, Anytype, and Notion are product comparison references for future
task-based evaluation; this plan makes no claim about their current features
or Knot's parity with them.

Keep three connected structures explicit:

| Structure | Examples | Editing meaning |
| --- | --- | --- |
| Authored document | ordered headings, passages, lists, references | change Djot through the document's edit and save commands |
| Knowledge graph | documents, passage references, claims, concepts, sources, attributed relationships | issue a graph-domain command against a known revision |
| Presentation | a query's table, network, reading sequence, filters, positions | change view parameters or layout; an explicit authoring action may change underlying facts |

Mere's graph kernel already has semantic, containment, arrangement, imported,
traversal, and provenance edge families. Its semantic vocabulary includes
`Cites`, `Quotes`, `Supports`, `Contradicts`, and `DependsOn`; its payloads
support semantic statements. Its query module includes traversal, paths, and
component algorithms. This is the starting inventory, not a reason to build a
second generic graph engine in Knot.

### Ownership and durable representation gate

Mere owns reusable graph representation, typed relation machinery, graph
algorithms, and projection contracts. Knot owns writing-specific semantics,
passage identity, accepted relation commands, and integration with its document
and vault authority. Cambium supplies controls and interactive composition;
Genet supplies rendering/platform behavior. Existing Mere projection grammar,
Scenograph, and Graphshell work is adopted where its contracts fit.

Before persisting the first authored relation, trace its complete owner and
storage path. Record whether it is encoded in authored Djot, a referenced
sidecar, or the existing vault's causal operations, and why. Pick one durable
owner for each fact. Link extraction and indexes are rebuildable readings;
user-authored relations must survive discarding those readings. A files-in-place
document can remain useful without its workspace, while export must disclose
which additional relationships and evidence travel with it.

Do not silently manufacture a new `.knot` format, RDF database, generic query
language, or replication protocol. Audit current statement identity, scope,
attribution, and retraction against the consumer before extending them. Record
an explicit mapping for user-defined predicate names and migration/rename
behavior; display labels alone are not stable predicate identity. A missing
portable envelope or graph mutation command is an implementation gap.

The current bridge needs particular care. `linked-data/src/statements.rs`
applies recognized relations to existing targets and reports unknown predicates
and absent targets separately. Preserve those inputs pending resolution rather
than implying arbitrary links already become complete graph facts. In
`graph-kernel/src/graph/edge_data.rs`, semantic statement deduplication by
subkind, predicate, and scope can update metadata on an existing statement.
G1 must prove that two independent authors or evidence assertions about the
same endpoints remain distinguishable; the current dedup behavior must not
silently collapse their authorship or evidence histories.

### G1. Links, passages, and inspectable relations

Start with document links, backlinks, and heading/passage targets. A writer
can select a passage and link it to another document, then optionally name the
relationship: cites, supports, contradicts, or a user-defined predicate. The
ordinary linking gesture must work before relation forms are opened. Do not
require classifying every paragraph or promoting every word into a node.

Give an authored relationship an inspectable identity, endpoints, predicate,
author, scope, revision, and any evidence or qualification. Distinguish an
assertion from an extracted hyperlink, an imported statement, or an inferred
suggestion. Acceptance of a suggestion creates an attributable authoring event
without erasing the suggestion's origin. Retraction preserves its history.

Document identity is not its current filename. Define moves, renames, copies,
duplicate opens, deleted targets, and unavailable peers. Passage references
carry document/revision context and an anchor policy; byte offsets alone are
not durable identity. After edits, resolve or mark the target stale/ambiguous.
An external evidence selector continues to name the captured artifact, not the
authored passage that cites it. Backlinks respect access scope.

**Model seam, grounded 2026-09-06:** extend the authenticated event and
projection seam in [`sync.rs`](../crates/knot-editor/src/sync.rs) with a distinct
authored-relation record/event and a relation projection. The existing event
fold handles document Put/Delete/Resolve by document id; relation changes must
not enter that replacement path. Preserve independent assertion ids, author/op
provenance, predicate, scope, endpoints, evidence/qualification, and retractions
before lowering any reading into Mere's graph representation.

[`VaultDocument.id`](../crates/knot-editor/src/vault.rs) and `document_heads`
already provide stable document identity and operation context for admitted
vault documents. Ordinary file sessions still need a durable identity mapping
with explicit rename/copy behavior; do not derive this identity from a filename
or the filesystem identity used by the save guard. Decide where that mapping
lives before enabling persistent authored relations for files-in-place.

**Vault-first implementation boundary, 2026-09-07:** use existing admitted vault
ids for the first authored-relation model. Assertion identity comes from the
signed operation hash; the signed writer and space supply attribution and scope.
A retraction names one assertion, causally observes it, and belongs to the same
author. Independent assertions about the same endpoints remain independent.
Keep rejected relation operations inspectable without allowing them to erase
valid relations or interrupt the document projection. Retained checkpoint state
must include relation history, and older document-only checkpoints must remain
readable without changing their receipt identity merely by reserialization.

The existing checkpoint is a persisted observation, not yet a replay base:
`projection_with_cipher` decrypts the full retained operation log. Snapshot
persistence and full-log replay therefore do not prove recovery after old keys
are forgotten. A separate recovery gate must load a verified checkpoint base,
apply its retained tail, and prove that document and relation reads work when
base-operation keys are unavailable. When closed history contains relation events,
pruning must retain every closed-history epoch needed by the current full-log
reader, including captured source documents, until that gate passes.

This model does not add desktop link gestures or file identity persistence.
The next files-in-place design gate is a Knot-owned catalog with opaque document
ids and explicit path bindings: an application-mediated rename preserves an id;
a copy receives a fresh id; an externally moved file requires an explicit rebind
when identity cannot be established. A matching filename or content digest alone
cannot decide whether a document is a move, a copy, or a different document.
Decide catalog storage, portability, duplicate-open handling, and recovery before
wiring this into ordinary file sessions. The current save guard stays separate.

**File catalog decisions, 2026-09-07:** make adoption explicit at the directory
source boundary. The host chooses a metadata-only Redb catalog outside the
scanned root. Catalogs retain a canonical root binding, relative paths, and opaque
UUID document ids. Opening the catalog against a different root is refused;
root relocation, portable export/import, and merging catalogs need a later
explicit migration. A single writable handle owns catalog updates, with each
change committed before replacing its in-memory state.

An existing canonical path retains its registered id across an atomic save or
replacement. This is an explicit logical path binding, not evidence that an
external replacement has the same author or contents. Save custody and source
revision checks still apply. Canonical path aliases share a binding; distinct
hardlink paths and copied paths receive distinct ids. Missing paths retain their
catalog records. Rebinding requires an absent old path and an unbound new regular
file under the root, and never moves or overwrites source files.

The first host integration remains opt-in. Hosts must arrange explicit rebind
before scanning a moved destination as a new document; a destination already
registered under another id is refused rather than silently merging identities.
The later rename/recovery UI must account for that ordering. This mapping alone
does not admit disk documents to the vault's authenticated relation log: captured
revisions and the disk-to-vault relation authority bridge remain separate work.

**File revision bridge decisions, 2026-09-08:** a catalog-backed directory may
explicitly prepare a bounded observation of one indexed file's bytes. Preparation
returns the catalog document ID, display title, media type, and exact bytes; it
does not save the editor buffer or author an operation. A caller separately
admits that observation to a chosen signed/encrypted space as
`CaptureFileRevision`. The signature attributes the capture to its writer; it
does not prove that writer originally authored the disk contents or owns the
catalog identity. Catalog adoption alone grants no replication or disclosure
authority.

A capture operation is an immutable revision target for relations. It stays
outside document Put/Delete/Resolve folding, editable vault documents, and the
publication document-version API. Its exact signed operation hash supplies the
revision identity. A later capture of the same file ID does not replace older
captures or move an existing passage reference. Relations validate quotes against
the retained revision, including UTF-8 byte boundaries. File deletion or later
editing does not retract a signed historical capture.

Preparation reads disk, so unsaved editor changes are excluded. It bounds the
read using the caller's byte limit and checks the current indexed path under the
configured root. Filesystem checks remain observations rather than a transaction
with external writers. Prepared bytes can be reviewed and authored later even
if the disk changes; the operation attests to those prepared bytes, not to the
file being current at signing time. Desktop capture controls must make this
distinction visible before adoption.

The capture getter requires both document ID and exact operation hash under the
admitted space cipher. Host admission still governs access to retained bytes and
relation endpoints. Captures introduce a new event variant: older readers need
upgrading before joining a space that authors them. Checkpoint serialization is
unchanged; captures are recovered from the retained operation log. Until verified
checkpoint-base replay exists, any closed capture also holds closed-history
encryption epochs, including captures created before their first relation.

**Desktop review and storage authority, 2026-09-08:** the first desktop capture
control prepares and displays a saved-file revision in memory. It shows the
catalog ID, observed path, media type, exact source text, byte count, and configured
read limit. Dirty editor changes are explicitly excluded; editing and ordinary
Save leave this historical observation intact until Refresh. New, Open, Reload,
and Save As clear it after a successful transition. A failed refresh replaces the
old reading with an error. Discard releases the prepared reading. Non-UTF-8 bytes
remain valid library captures, but this source-text panel refuses them rather
than displaying a lossy substitute for review.

The shared capture type and bounded reader belong in `knot-file-catalog`, so the
desktop keeps its narrow dependency graph and `knot-editor` preserves its public
capture API through a reexport/adapter. Reviewing is neither a signed event nor
permission to store or share the bytes. The desktop requires explicit selection of a host-issued destination before retention.

The resident-owned retention adapter accepts an explicit document-ID and byte
limit grant, binds immutable prepared bytes to a space, writer, and encryption
profile, and returns the signed operation ID. It rechecks lock, writer admission,
destination, and grant at retention. Revocation applies to all clones of a port.
An exact same-writer capture is reused across retries and resident reopen;
changed metadata or bytes remain distinct observations. Lookup refuses incomplete
causal history. A clone-shared async gate in `KnotSyncStore` now serializes capture
lookup and authoring with ordinary Knot authoring and operation acceptance.
The `KnotSyncStore::join` LogSync callback uses that same acceptance path.
The resident lock continues to protect the port's live vault authority while
the store gate protects operation history. Direct writes through the low-level
Muniment handle returned by `sync_store()` remain outside this contract; hosts
must use the Knot mutation APIs. This is in-process coordination among clones,
not a transaction across independently constructed store owners.

The desktop now accepts host-issued destinations through the lightweight
`knot-capture` contract and `run_desktop_with_targets` library entrypoint.
`KnotResidentRetainPort` snapshots an existing granted resident capability and
rechecks its destination and authorization on the worker. Persona labels and
stable IDs come from the owning host; the adapter cannot attest a display
identity independently of that binding.

The workspace requires explicit destination selection, retains the immutable
reviewed revision on a worker, and reports the original document, destination,
and signed operation separately from the current review. Selection and review
never authorize a write. Only one request runs at once. Switching or discarding
a review does not cancel an issued write; failures permit an exact retry without
claiming rollback. The host wake hook delivers completion without further input.

The remaining storage gate is real persona/resident selection in the standalone
launcher, which still injects an empty destination list. An owning application
must supply an already-open resident and grant. `StartupUnlockedPersonalVault::open`
may migrate documents and `local_device_root` may create an identity; neither is
a neutral destination picker. Default document and desktop builds keep their
narrow feature set; the real resident test fixture is opt-in. Replication remains
a separately selected authority. Headed acceptance and long-history latency
remain unverified.

Destination rows now use short labels with distinct space/writer prefixes; the
selected detail keeps full persona, space, writer, and encryption identities
visible before Retain. Selection has an accessible pressed state and never starts
a write. The worker releases its capability before announcing completion, so
closing or reopening cannot race its final owner-handle drop.

The in-process owner lifecycle gate exercises host-selected destinations, explicit
document/byte grants, live revocation, and explicit re-grant and reselection. It
must refuse a revoked request without extending the signed tail, then deduplicate
an exact retry after renewed authority. This remains a supplied-host fixture,
not standalone attachment to a running process.

**Standalone attachment boundary:** the default launcher cannot attach to the
running `knot_sync_host`. Its current process exposes replication and pairing,
not a desktop capture service; an in-memory `KnotFileCapturePort` cannot be passed
across processes. Do not reopen that owner's vault from the desktop.

Reuse Graphshell's existing admitted application route. At Mere `33287b0`,
`serve_app_broker` applies `AllowedAppRoutes`, binds the application to a local
link, and creates an `AdmittedEndpointContext` before the host's endpoint factory
runs. `ResidentEndpoint` accepts an `IntentSink` and routes `IntentInvocation`
separately from projection/resource reads. The relevant Mere sources are
`ports/graphshell/src/native/app_broker.rs`, `app_admission.rs`,
`endpoint_catalog.rs`, `app_client.rs`, and `crates/chirograph/src/lib.rs`.
The next storage implementation should
add a bounded Knot capture intent to that admitted route, with a host-issued
capture grant tied to the same existing resident. The caller must never supply
its own grant or infer authority from a persona label.

Before writing the remote client, settle the signed-receipt return contract.
At this pin `chirograph::IntentResult` has only `Accepted`, `Rejected`, and
`Stale`; it has no result payload. Either evolve that shared contract with
consumer/version negotiation, or specify a bounded, session-owned receipt
resource associated with the request. The existing Knot resource implementation
only returns resources disclosed to that session, so this also needs explicit
receipt disclosure and revocation semantics. Do not use projection state as an
operation acknowledgement. The broker composition must select an existing
resident, authenticate the app, revoke the capture grant with its session, bound
request bytes before decoding, and distinguish refusal from an uncertain write
outcome after disconnect. Keep this in the current broker rather than adding a
parallel local socket/authentication system. Done when an admitted desktop
retains and retries through that route, a revoked/unadmitted client cannot write,
and reconnect/restart preserves one store owner and the signed receipt.

The next presentation receipt should verify input, scrolling, destination identity
readability, and close behavior in a headed window. Long-history measurements
should bound the worker scan and storage-lock wait before promoting a lookup-index
design.

Retention currently scans retained operations synchronously while holding the
resident lock. Run it on a worker so the input/render thread and the executor
handling queued ingress remain free to make progress. Measure representative
long histories before claiming interactive latency. Any future lookup index
remains rebuildable from signed captures;
it cannot become a second source of revision identity or authorization.

The first catalog stores a versioned binding list and commits each newly discovered
path separately. Before making it a default for large trees, measure initial
registration and unchanged scans with representative directory sizes; consider
batched registration and indexed lookups from those results. No large-directory
latency or scaling receipt is claimed by the functional tests.

Catalog rebind and directory refresh are two operations. If the metadata commit
succeeds but the filesystem scan fails, the source reports that the binding was
committed and directs the caller to retry refresh; it must not imply the rebind
was rolled back. Directory discovery continues to skip symlinks through
`DirEntry::metadata`. Direct catalog binding can canonicalize an explicitly
provided file alias; discovery does not recursively follow linked directories.

The recommended anchor rule is to retain the captured document/head and
quote/position as the authored fact. Following newer text is then an explicit
projection policy with resolved, stale, ambiguous, and unavailable results;
it does not silently rewrite the captured claim. A deliberate re-anchor is a
new attributable operation. Keep the external-artifact
[`SpecificResource`](../crates/knot-editor/src/web_annotation.rs) boundary intact.
The first gate should replay two authors' distinct assertions, retract one,
and rebuild without collapsing the other or exposing inaccessible backlinks.

*Done when* an essay links to two source documents and selected passages, and
shows a support and a contradiction as separately inspectable assertions.
Rename one source and edit one target passage: references either resolve
correctly or visibly report their exact unresolved state. Restart, rebuild
derived indexes, and verify authored relations persist. Revoke access to one
source and confirm that backlinks and counts do not disclose its private facts.

### G2. Saved questions and coordinated presentations

Implement one useful saved question: "claims in this essay, their supporting
passages, and contradictory sources." Its bounded definition records roots,
relation filters, direction, scope, and traversal limit. Saving the question
does not freeze or duplicate its results. A pinned result names its source
revision separately from a live reading.

Knot must define the saved-question record and its authorization, versioning,
and result shape. Mere's serializable facet filters and linked-data read-query
facilities are candidate execution paths, not a pre-existing saved-query
authority. Its relation matrix is a useful reading, not a general result-table
engine. The first bounded definition should reuse these paths where suitable
without exposing a mandatory query language to the writer.

Show the same result identities as a table/list and a graph. Share selection
through existing coordination contracts while retaining each view's camera,
scroll, and arrangement. Clicking a relationship opens its inspector. Editing
its predicate from either presentation submits the same validated domain
command; both update from the resulting authoritative revision. A node drag
changes layout unless the user invokes a named relation/grouping action.
Board, timeline, and other presentations follow concrete field requirements.

*Done when* one saved question reopens, both views show identical permitted
result membership, and editing a relation in either view updates the other.
A refused stale edit preserves both the current facts and the user's proposed
change for review. A layout-only drag leaves semantic relations unchanged.
An empty, truncated, unavailable, or stale result has a distinct explanation.

### G3. Explainable graph operations

Use existing Mere algorithms for a bounded path-between-passages operation
and a claim-support coverage reading. Record input graph revision, access
scope, predicate/direction filters, algorithm version, parameters, and limits.
Paths retain the identities of the traversed relationships so the writer can
inspect the evidence. An unsupported-claim result means "no qualifying support
in this permitted scope," not a judgment that the claim is false.

Audit each selected algorithm's actual direction and edge-weight semantics.
Existing traversal/path helpers do not by themselves implement the proposed
predicate-sensitive claim analysis. Retain multiple semantic statements per
endpoint pair in the explanation even if an algorithm traverses one structural
edge. A directed dependency view must not silently use an undirected path.

Grouping, centrality, similarity, and dependency analysis remain subsequent
operations chosen for a writing task. Their outputs are derived and may be
discarded. Promoting a result into an authored group or relation is an explicit
command with provenance. Apply access filtering before computation so hidden
material cannot leak through paths, clusters, counts, or suggested neighbors.

*Done when* a known fixture gives an independently expected path and support
coverage result; results disclose scope and limits; a relevant source edit
invalidates the old result; and unauthorized material does not influence the
visible output. Cancellation and stale asynchronous completion must not stall
or replace the active document view.

## Writing tasks and interaction acceptance

Use an essay revision, a poem, a research note with citations, and a two-writer
review as shared fixtures across A and G cuts. Record the actual interaction
sequence, authored result, keyboard route, and headed evidence. Feature counts
alone are not a parity receipt.

The first writing workspace specifies find/replace, undo grouping, paste,
indentation/list continuation, link insertion, and navigation. These are
adoption or implementation checks, not assumed capabilities of a text area.
Document state owns the source and undo history; each view owns selection,
scroll, and folds. Closing one view retains a document shown elsewhere;
closing its final dirty view invokes the save/recovery decision.

Compare focused writing, source beside preview, and research with references
on the same material at narrow and regular sizes, with keyboard-only use.
Make recovery, automatic file saving, and peer publication separately
configurable actions. Private drafts require a demonstrated local-to-shared
transition, not merely a name on already-replicated operations.

## Settings and recovery boundaries

There is no universal Knot settings bucket.

| Scope | Examples | Owner |
| --- | --- | --- |
| App/workspace preference | theme, UI zoom, writing width, font and line spacing, preview mode, panel visibility, Rosette geometry, workspace layout | standalone Knot application; embedded presentation respects host policy |
| Transient/recovery session | open documents, unsaved recovery copy, active tab, selection, last source path subject to local policy | `DocumentWorkspace`; recovery never writes the source target without a custody check |
| Persona/vault policy | paired writers, relay hints, device label, unlock state, vault retention | existing persona-scoped Knot settings and vault/resident authority |
| Document and causal facts | authored source, clip provenance, revision heads, conflicts, publication choice | document, evidence, sync, and publishing authorities |

Preferences should remain user-configurable and portable only where their
scope permits. The workspace must tolerate missing optional panels and an
unavailable resident without corrupting its saved layout. Recovery material is
private local state with a clear restore/discard affordance and a bounded
retention policy set by the user. Recovery of sealed-vault material must respect
the vault's encryption and lock state; a generic plaintext recovery file is not
an acceptable shortcut. Lock state is live authority state, not a persisted
preference that can unlock a vault on restoration.

After adopting the shared host API, Knot supplies persisted UI zoom at startup
and applies user changes through that host's zoom mechanism. Respect its stated
launch-override precedence. Verify typing, pointer selection, IME placement, and
panel layout at narrow and regular window sizes and multiple zoom levels; do
not implement an independent CSS scale with different input coordinates.

## Cambium adoption boundary

Use the existing shared `workbench` and `cambium::workspace` composition after
the Knot pin contains the published interface. A `DocumentTile` maps to a
Knot-owned session; a right-panel tile maps to a derived reading or an
authority-backed component. Component-local UI state may follow a tile inside
one projection. Durable document identity, source text, permissions, and
authority status remain in Knot and are re-read when a tile enters another
projection.

Use `Component` for retained component state and typed events, the shared tab
bar for tabs, `tree_view` for navigation and outline, `detail_panel` for inert
authority facts, `command_surface` for actions, and `setting_row` for scoped
preferences. Generic control events still need Knot's effect validation.
Menus, disclosure controls, popovers, and lists should be adopted as existing Cambium furniture before
adding Knot-specific replacements. The first implementation should exercise
one document tab stack, a navigator, the writing component, and a replaceable
context slot. It should not require floating windows, arbitrary multi-window
restoration, or a universal dashboard to ship the safe writing cut.

## Stop rules

- For these cuts, do not add a second authoritative source buffer, editable preview DOM, or serialized
  component tree.
- Do not make normal Save overwrite a file changed since its retained baseline.
- Do not treat a local file baseline mismatch as a causal-head or endpoint
  token mismatch.
- Do not make Cambium, Turnstone, or a host own vault keys, evidence bytes,
  sync resolution, or publication authority.
- Do not let an evidence reference imply that the referenced bytes are present
  or verified.
- Do not call Rosette, preview, search, or any other derived reading durable
  document authority.
- Do not start titled-draft implementation until its causal representation and
  rebase rule are settled.

## Overarching questions to investigate

- **Writing and source visibility.** Compare source with adjacent preview,
  source with inline derived annotations, and a reading-focused arrangement on
  the same prose and verse documents. Record selection, undo, source fidelity,
  and attention costs before deciding whether richer projected editing needs
  its own slice. These cuts do not settle that longer-term interaction.
- **Private drafting and shared history.** Prove which edits remain local and
  when an offer becomes visible to peers. A title over already-replicated
  operations does not make a draft private. Separate local recovery, saved
  revisions, and explicitly offered review units in the UI.
- **Document and graph navigation.** Compare an outline/list with Rosette's
  graph for finding and revising related passages. Promote graph interactions
  that improve that task; retain straightforward document navigation.
- **One document in several views.** Test the document/view split specified
  above: view positions are independent, with optional linked navigation.
  Shared text and undo history survive view moves and closes without opening
  a second resident store owner.
- **Standalone and embedded composition.** Start with the existing narrow
  embedded document. Decide from a Turnstone consumer whether navigator and
  contextual panels belong inside a contributed workspace or are separately
  mounted contributions, so embedded Knot does not duplicate the host's tabs.
- **Bounded derived work.** Measure input response while preview, search, and
  lexical analysis run on representative long documents. Tag asynchronous work
  with document identity and revision, discard stale results, and expose
  actionable resource limits without freezing the writing view.

## Findings and progress
- 2026-09-12 A2 source-linked preview-heading and fold contract: ordinary Djot and
  legacy Knot documents now have an optional live, read-only desktop preview
  derived from the retained source buffer. Rendered headings select their exact
  source syntax and request editor focus. Preview and fold snapshots reject
  stale or forged source-bound rows; fold rows cover sections, lists, quotes,
  code blocks, and divs. Native Scroll, Gemtext, and Micron files retain their
  separate site preview route. Inker block source ranges and Cambium visual fold
  concealment remain open shared-library work.
- 2026-09-12 A2 validation: Windows, Rust 1.97.1, Mere `1777b198`, and Genet
  `9e8f9dc2`. The `knot-document` engine/highlight suite passed 47 tests with
  its existing manual large-document probe ignored. The desktop passed 52
  library and 4 launcher tests with its existing timing probe ignored.
  `git diff --check` passed. The existing Windows atomic-replacement unsafe
  warning and unused root patch notice were unchanged.
- 2026-09-09 A2 appearance and source-decoration slice: the standalone desktop
  now derives light and dark UI and syntax palettes from Tinct and exposes
  session controls for highlighting, 12–24 px source type, compact/relaxed line
  spacing, and narrow/wide writing measure. Djot and legacy Knot highlighting
  use Cambium's styled textarea over the existing authoritative `TextInput`;
  Markdown, JSON, read-only, and default embedded surfaces remain plain. The
  settings survive New, Open, and Reload without changing source, selection,
  dirty state, undo history, or save authority. Cross-launch persistence,
  source-linked preview/folding, and headed visual/IME acceptance remain open.
- 2026-09-09 A2 validation: Windows, Rust 1.97.1, Mere `33287b0`, Genet
  `9e8f9dc2`. The default desktop suite passed 36 library and 4 launcher tests;
  its existing diagnostic timing probe remained ignored. The standalone
  `knot-document` workspace with `--features highlight` passed 31 tests with
  its existing manual performance probe ignored. `git diff --check` passed.
  The existing Windows atomic-replacement unsafe warning was unchanged.
- 2026-09-09 main integration and destination refinement: `0a46ebd` brings the
  earlier Retain implementation onto `main`, preserving the public revision work
  and Mere `33287b0` pin from `e312458`. Luna refined concise destination choices,
  full selected identity details, and pressed state. Terra expanded the real
  resident fixture through revocation, visible refusal, explicit re-grant and
  reselection, and owner reopen with the same signed operation. Worker completion
  now releases its resident capability before waking the UI. The source file and
  dirty buffer remain independent of the reviewed snapshot throughout.
- 2026-09-09 integrated validation: Windows, Rust 1.97.1, Mere
  `33287b0efbe0bd45bd46b87a598d5f4fc62414e2`, Genet unchanged at
  `9e8f9dc2f3ddc0af1658580bb51964462a03923f`. From the repository with
  `CARGO_HOME=C:\Users\mark_\.cargo` and `RUST_TEST_THREADS=1`:

  ```powershell
  cargo test -p knot-editor --test retain_host --offline --locked -j 2 --no-fail-fast
  # 4 passed.
  cargo test -p knot-desktop --features resident-retention-tests --offline --locked -j 2 --no-fail-fast
  # 32 library + 4 launcher + 1 real resident test passed; 1 existing timing probe ignored.
  ```

  Focused formatting and diff checks passed. The default normal dependency tree
  still excludes the editor runtime. These receipts are windowless; standalone
  broker attachment and headed acceptance remain open. Probe cleanup is deferred.
- 2026-09-09: added the lightweight `knot-capture` host contract, existing-resident
  adapter, reusable desktop entrypoint, and explicit worker Retain controls.
  Reviews and destination selection do not write. Tests cover duplicate refusal,
  wake-only completion, target/review changes during retention, mismatched
  receipts, failure/retry, disconnected workers, and close deferral. A deferred
  close clears any earlier Discard flag. The adapter checks both the live and
  prepared destination before retaining.
- 2026-09-09 validation: Windows, Rust 1.97.1, isolated worktree based on `b303527`.
  With `CARGO_HOME=C:\Users\mark_\.cargo`, `RUST_TEST_THREADS=1`, and
  `--target-dir C:/Users/mark_/Code/repos/knot-editor/target`:

  ```powershell
  cargo test -p knot-editor --test retain_host --offline --locked -j 2 --no-fail-fast
  # 4 passed: signed readback, reopen/dedup, live authority refusals, shared capability.
  cargo test -p knot-desktop --offline --locked -j 2 --no-fail-fast
  # 31 library + 4 launcher tests passed; 1 existing timing probe ignored.
  cargo test -p knot-desktop --features resident-retention-tests --offline --locked -j 2 --no-fail-fast
  # 31 library + 4 launcher + 1 real resident integration test passed; 1 ignored.
  ```

  The real resident fixture is enabled with `--features resident-retention-tests`.
  It drives Review, destination selection, and Retain through the public desktop
  harness; after review it changes disk bytes and the unsaved buffer, then checks
  that the signed operation contains the earlier review and leaves the later
  source untouched. Receipt text includes the original document and full
  operation ID. This is windowless acceptance. The default normal dependency
  tree excludes `knot-editor`; Mere and Genet pins are unchanged. Focused Rust
  formatting and diff checks passed. Shared Cargo cache/build contention slowed
  verification. The existing Windows replacement unsafe-code warning remains.

- 2026-09-09 public revision boundary: synced vault editable resources now
  disclose their exact signed document-head operation as
  `EditableTextV1.public_revision`. `KnotResidentSource` and `KnotEndpoint`
  expose the same authority-issued value for host adapters. The private
  `base_token` remains a save concurrency capability and is never reused or
  hashed as provenance. Files-in-place, fixtures, conflicts, and unsynced
  vaults disclose no public revision. This supplies Scenograph's durable
  revision path for Turnstone once its immutable Mere and Knot pins advance;
  sources without such identity may use only Scenograph's non-serialized
  runtime witness path.

- 2026-09-09 coordinated mutations: Terra added one async gate shared by clones
  of `KnotSyncStore`; Luna supplied coordination tests, with root review and
  cancellation/checkpoint assertions. Capture lookup and signing now share the
  gate with ordinary authoring, LogSync acceptance, checkpoint creation, and
  epoch-pruning revalidation/commit. Internal authoring accepts its own operation
  without reacquiring the gate. Revoked writers are rechecked after waiting,
  including when a matching capture already exists. This closes the previously
  recorded simultaneous-ingress gap for Knot mutation APIs. Raw Muniment writes
  and independently constructed owners remain outside the guarantee. Host
  selection, worker scheduling, and desktop Retain controls remain next.

- 2026-09-09 validation: the shared checkout acquired concurrent `endpoint.rs`
  work, so verification used `C:\Users\mark_\Code\worktrees\knot-editor-coordination-20260909`
  at `c0df2f9` plus only this slice's patch. All three changed Rust files were
  hash-compared with the shared checkout. Dependency pins remained unchanged.
  On Windows with Rust 1.97.1, `CARGO_HOME=C:\Users\mark_\.cargo` and
  `RUST_TEST_THREADS=1`, the following command ran from that isolated checkout:

  ```powershell
  cargo test -p knot-editor --lib --test capture_retention --test file_relations --test authored_relations --test file_catalog --no-fail-fast --offline --locked -j 2 --target-dir C:/Users/mark_/Code/repos/knot-editor/target
  ```

  The library passed 109 of 110 tests, including all four new coordination tests.
  All 12 integration tests passed; the existing Windows symlink test remained
  ignored. The existing publishing loopback test again timed out at
  `publish_host.rs:730` with `ConnectionLost(TimedOut)`. The isolated retry below
  passed in 2.67 seconds:

  ```powershell
  cargo test -p knot-editor --lib publish_host::tests::p2panda_loopback_uses_the_real_noise_and_notochord_path --offline --locked -j 2 --target-dir C:/Users/mark_/Code/repos/knot-editor/target -- --exact --nocapture
  ```

  This gives 122 distinct passing tests across runs, not an uninterrupted green
  aggregate gate. The recurring aggregate loopback timeout remains open. Focused
  formatting and diff checks passed. Concurrent endpoint edits were excluded
  from this receipt and commit; no headed UI or non-Windows claim is made.

- 2026-09-08 resident capture retention: Luna implemented the resident-owned
  port and Terra implemented and reviewed the closed-history lookup. The port
  takes explicit grants, prepares immutable destination-bound revisions, and
  returns signed receipts with retry reuse. Retention checks current writer,
  vault lock, grant, destination, and Commons key availability. It reads current
  encryption keys after rotation and leaves the editable vault projection alone.
  Separate ports share the resident lock across lookup and authoring. Desktop
  destination selection, Retain controls, and headed acceptance remain open.

- 2026-09-08 retention validation: Windows, Rust 1.97.1, unchanged dependency
  pins, `CARGO_HOME=C:\Users\mark_\.cargo`, `RUST_TEST_THREADS=1`. The final-source
  command `cargo test -p knot-editor --lib --test capture_retention --test
  file_relations --test authored_relations --test file_catalog --offline --locked
  -j 2` built every selected executable. The library run passed 105 of 106 tests;
  the existing publishing loopback test again failed at `publish_host.rs:730`
  with `ConnectionLost(TimedOut)`. Cargo stopped before running integrations.
  The freshly built executables were then run directly to avoid another shared
  package-cache wait:

  ```powershell
  .\target\debug\deps\capture_retention-750fb988cb180059.exe --test-threads=1
  .\target\debug\deps\authored_relations-1ae0da33a0e10c3d.exe --test-threads=1
  .\target\debug\deps\file_catalog-71914480ac349b77.exe --test-threads=1
  .\target\debug\deps\file_relations-8c1c19a782f0633f.exe --test-threads=1
  .\target\debug\deps\knot_editor-f72dff44235e04e9.exe publish_host::tests::p2panda_loopback_uses_the_real_noise_and_notochord_path --exact --nocapture --test-threads=1
  ```

  These runs passed 5 retention, 2 authored-relation, 4 catalog, 1 file-relation,
  and the isolated loopback test (6.11 seconds). One existing Windows symlink
  test remains ignored. There are 118 distinct passing tests across the runs;
  this is not an uninterrupted green aggregate gate. The recurring loopback
  timeout in aggregate runs remains open. Focused formatting and `git diff --check`
  passed. No headed UI or non-Windows receipt is claimed.

- 2026-09-08 desktop saved-revision review: added an explicit Review/Refresh/
  Discard surface for catalogued files with source text, path, ID, media type,
  byte count, and a configurable capture limit (default 1,048,576 bytes;
  `--capture-max-bytes` requires the paired catalog options). The review reads
  saved disk bytes and labels unsaved changes as excluded. It survives ordinary
  Save as a historical reading; successful source transitions clear it. Failed
  refresh removes the old payload, and non-UTF-8 text is refused by this panel.
  A source-path/catalog-ID agreement check refuses a rebound-away binding.
  The capture type and bounded reader moved to `knot-file-catalog`; the editor's
  public API and signed event shape are preserved. Capture validation remains
  separate from destination/storage authorization. No persona is unlocked and
  no source bytes are persisted by this surface.
  `cargo test -p knot-file-catalog -p knot-desktop --offline --locked -j 2`
  passed **29 desktop tests and 8 catalog tests**; the existing diagnostic
  outline probe remains ignored. New harness receipts cover dirty-buffer
  exclusion, literal source display, historical inner Save, explicit refresh,
  missing/non-UTF-8/over-limit refresh failures, successful-only transition
  clearing, and a rebind mismatch. A Unix-only symlink capture case is present
  but was not run on Windows. `cargo tree -p knot-desktop --offline --locked
  -e features -i knot-document` confirms default document features only.
  These are automated receipts; headed review usability and rendering at the
  maximum configured byte limit remain unmeasured. Probe cleanup stays deferred.
  Editor compatibility: the initial `cargo test -p knot-editor --lib --test
  file_relations --test authored_relations --test file_catalog --offline --locked
  -j 2` run passed 103 of 104 library tests and stopped on the existing
  publishing loopback's `ConnectionLost(TimedOut)` server outcome. The isolated
  `cargo test -p knot-editor --lib
  publish_host::tests::p2panda_loopback_uses_the_real_noise_and_notochord_path
  --offline --locked -j 2 -- --exact --nocapture` retry passed. Running the same
  three integration targets separately passed all 7 tests, including historical
  file relations and legacy checkpoint decoding; the existing Windows symlink
  privilege case remains ignored. This records 148 distinct passing tests across
  the focused runs, with the initial network timeout retained as a test
  reliability follow-up. Windows/Rust 1.97.1 and `RUST_TEST_THREADS=1` were used;
  Mere remains `2b1ce46e5a15328b4bf4d350ec4b0252d9b404a1`, Genet remains
  `9e8f9dc2f3ddc0af1658580bb51964462a03923f`. The lockfile adds only the catalog's
  `zeroize` dependency. Formatting, diff checks, and documentation links pass.

- 2026-09-08 signed file-revision bridge: `KnotFileRevisionV1` and catalog-only
  `DirectorySource::capture_file_revision` prepare bounded disk observations.
  The signed `CaptureFileRevision` event stores those exact bytes separately
  from current vault documents. Both local relation authoring and replay may
  validate endpoints against these retained captures; exact `file_revision`
  reads remain separate from the publication-facing `document_version` API.
  Later captures preserve older operation identities and passage references.
  Structurally invalid captures are refused locally and excluded from replay's
  verified source map. A capture triggers conservative closed-history epoch
  retention even before an assertion references it. This slice adds no desktop
  capture action, automatic replication, passage re-anchoring, or graph view.
  Validation: `cargo test -p knot-editor --lib --test file_relations --test
  authored_relations --test file_catalog --offline --locked -j 2` passed
  **104 library tests and 7 integration tests**. The existing Windows
  symlink-privilege case remains ignored. The public API receipt prepares disk
  bytes, changes the file, signs the prepared observation, authors UTF-8 passage
  relations, captures the newer contents, and reopens the signed store with the
  historical relation intact. Current vault documents and publication-version
  reads remain isolated. Other new tests cover size bounds, arbitrary source
  bytes, invalid captures/quotes, writer admission, and capture-only epoch holds.
  Existing authored-relation and legacy-checkpoint receipts pass. Validation
  used Windows, Rust 1.97.1, and `RUST_TEST_THREADS=1`; Mere remains at
  `2b1ce46e5a15328b4bf4d350ec4b0252d9b404a1`, Genet at
  `9e8f9dc2f3ddc0af1658580bb51964462a03923f`. Formatting, diff checks, and local
  documentation links pass. No headed UI receipt or dependency pin change.

- 2026-09-08 desktop catalog adoption: extracted the existing V1 catalog and
  tests into `knot-file-catalog`, preserving the `knot-editor` public reexports.
  The desktop depends directly on this crate and keeps `knot-document` on its
  default features. `cargo tree -p knot-desktop -e features -i knot-document`
  confirms that adopting the catalog does not enable the document engine.
  Paired `--catalog-root ROOT --catalog PATH` options opt in; malformed options
  and a catalog that cannot open stop startup explicitly. The ordinary launch
  still opens one file or an untitled document without a catalog.
  The workspace reads the actual session source path for its document ID.
  A path-field edit is only a proposed future target. New clears the current
  identity; successful Open and Save As bind the resulting file, while ordinary
  Save preserves identity. Catalog updates complete before a successful save
  accepts a pending close. Binding failure does not roll back saved source or
  reopen the save decision. The catalog status and retry action remain distinct
  from file-save status; unsaved and outside-root files have no catalog ID.
  Catalog state is updated on lifecycle actions and explicit retry, rather than
  writing metadata on each edit/focus event. Headed acceptance, in-window catalog
  configuration, document navigation, rename/rebind controls, and the revision
  bridge into authored relations remain open.
  Automated receipt: `cargo test -p knot-file-catalog -p knot-desktop --offline
  --locked -j 2` passed 24 desktop tests and 6 catalog tests; the existing
  diagnostic outline timing probe remains ignored. The desktop harness covers
  actual-source identity, Save As allocating a new binding, dirty-close success
  despite an outside-root catalog refusal, and explicit retry preserving the
  document snapshot. These are automated UI receipts, not headed acceptance.
  Compatibility receipt: `cargo test -p knot-editor --lib --test file_catalog
  --offline --locked -j 2` passed 97 editor unit tests and 4 catalog integration
  tests; the existing Windows symlink-privilege case remains ignored. Both gates
  used Mere `2b1ce46e5a15328b4bf4d350ec4b0252d9b404a1` and Genet
  `9e8f9dc2f3ddc0af1658580bb51964462a03923f`, with no dependency pin changes.

- 2026-09-07 G1 file catalog: added an explicit root-bound catalog and directory
  adoption path, with opaque durable document ids and atomic Redb metadata
  commits. A second catalog owner is refused. Existing path bindings survive
  source replacement and restart; copies and distinct hardlink paths receive
  separate ids. Unavailable bindings remain inspectable, with explicit rebind
  to an unbound in-root target after the old path is absent. Root changes,
  malformed/version-mismatched catalogs, duplicate ids/paths, and catalog files
  inside the scanned root are refused. File bodies are never stored here.
  `DirectorySource::with_catalog` opts in; the existing default discovery ids
  remain unchanged. `KnotEndpoint::from_directory_source` lets an admitted host
  serve that source using the existing read/write custody paths. Catalog adoption
  does not grant writes or persist host-owned runtime facets.
  Desktop catalog settings, root relocation/migration, rebind-after-discovery
  conflict resolution, and the file revision/admission bridge into authored
  relations remain open. This is not completion of G1's writing UI.
  Validation on Windows/NTFS, Rust 1.97.1, Mere `2b1ce46e`, and Genet `9e8f9dc2`:
  `cargo test -p knot-editor --lib --test file_catalog --offline --locked -j 2`
  passed **103 library tests** and the four non-symlink integration cases.
  The direct symlink-alias fixture failed to create a link with Windows error
  1314 (required privilege unavailable). It is explicitly ignored on Windows,
  remains enabled on Unix, and can be selected with `--ignored` on a Windows
  host with symbolic-link privileges; it is not a passing local receipt.
  A final `cargo test -p knot-editor --test file_catalog --offline --locked -j 2`
  confirms the supported gate after that test annotation. Source replacement
  includes Knot's actual edit/save path, plus an external remove/rename fixture.
  The default discovery, copy/hardlink, restart, rebind, malformed catalog,
  exclusive-owner, and missing-path tests all ran. Focused Rust formatting,
  Git diff checks, and local documentation links passed. No pins or probes changed.

- 2026-09-07 G1 vault relation model: distinct signed assertion and retraction
  events retain operation-derived identities, author and space attribution,
  predicate, captured document/head endpoints, and optional quote/UTF-8 byte
  position, evidence reference, and qualification. Local authoring validates
  captures before signing. Replay isolates invalid relation events from the
  document fold and retains unverified captures separately. Evidence strings
  remain opaque author-supplied references, not fetched or verified artifacts.
  Active relation reads and history reads are separate, both filtered by the
  caller's admitted endpoint ids. This filter is not a new authorization system;
  the host must supply admission appropriate to the captured revision and quotes.
  The raw projection and checkpoint are privileged observations.
  Checkpoints retain assertion/retraction and rejected/unverified history.
  Empty new fields preserve existing document-only snapshot serialization.
  Full-log replay and checkpoint persistence are the implemented recovery seam;
  bootstrap from a checkpoint after historical key erasure remains open.
  When a closed relation event exists, communal pruning retains all closed-log
  epochs needed by the current reader, including referenced document versions.
  Pending ciphertext is excluded from projection decoding and remains protected
  by the existing pending/tail retention rules.
  Desktop link gestures, ordinary-file identity mapping, automatic re-anchoring,
  cross-space relations, saved graph queries, and Mere graph lowering remain open.
  Validation: `cargo test -p knot-editor --lib --test authored_relations --offline --locked -j 2`
  passed **97 library tests and 2 integration tests**, zero failures or ignored
  tests, with `RUST_TEST_THREADS=1` on Windows and Rust 1.97.1. Final dependency
  identities were Mere `2b1ce46e5a15328b4bf4d350ec4b0252d9b404a1` and Genet
  `9e8f9dc2f3ddc0af1658580bb51964462a03923f`; the concurrent dependency update
  was committed separately as `54fd2b9`. This slice changes no dependency pins.
  Focused Rust formatting, Git diff checks, and local documentation links passed.
  This is a library-model receipt; no desktop graph UI or headed acceptance is
  claimed. Existing outline diagnostics were left untouched.

- 2026-09-06 A2 outline: added source-addressed heading snapshots
  and guarded selection. Row selection checks exact source/address, canonical
  heading identity within that reading, and UTF-8 byte boundaries. It preserves
  source, undo, dirty baseline, and save refusal; read-only sessions permit this
  view-local selection without granting editing authority. Inspection of the
  pinned shared host showed that outline is available independently of its
  preview feature. Knot exposes its own plain snapshot types over that existing
  readout and keeps the desktop on the narrow dependency set. The broader engine
  opt-in is reserved for preview adoption.
  The current committed dependency pin is now `b9b0ee13` at repository head
  `35952af`, so the earlier working-tree-only pin caveat does not apply to this
  slice's validation.
  The ignored default-feature probe observed **573,780 source bytes, 4,000 headings,
  168,311 microseconds** for one snapshot in the Windows debug test build.
  This measures parsing and snapshot construction, not UI or keystroke latency.
  These are single diagnostic runs on a machine with concurrent compiles and
  substantial memory pressure, not a controlled performance baseline.
  The windowless desktop probe rendered **119,290 source bytes and 200 heading
  rows**: showing the outline plus layout took **3,057,622 microseconds**;
  an edit, outline refresh, and layout took **6,294,769 microseconds**.
  The probe passed its row-count gate, but these multi-second observations do
  not establish acceptable writing latency. Next performance acceptance needs
  matched outline-hidden/visible runs on an otherwise quiet machine, separating
  source layout, readout construction, and retained row updates. Use that evidence
  to decide whether scheduling, row virtualization, or incremental work is needed.
  Headed visual and interaction acceptance remains open.
  Literal source spans were checked. Existing parser labels joined
  the continued heading words `soft` and `wrapped` into `softwrapped`, omitted
  `:symbol:` and a footnote marker, and retained the tested styled text and
  literal smart punctuation. These label limitations remain shared-parser work;
  the source ranges and authoritative text remain intact.
  Final validation on Windows/NTFS with Rust 1.97.1 used
  `CARGO_HOME=C:\Users\mark_\Code\.cargo-knot-a1`,
  `CARGO_TARGET_DIR=C:\t\knot-a1`, one build job, and one test thread:
  - `cargo test --manifest-path crates/knot-document/Cargo.toml --offline -j 1`:
    **27 passed**, one diagnostic ignored, zero doctests.
  - `cargo test --manifest-path crates/knot-document/Cargo.toml --features engine --offline -j 1`:
    **33 passed**, one diagnostic ignored, zero doctests.
  - `cargo test -p knot-desktop --offline --locked -j 1`:
    **19 passed**, one diagnostic ignored.
  - The document `outline_snapshot_large_unicode_and_atom_fixture_receipt` and
    desktop `outline_long_document_probe` each passed when explicitly selected
    with `-- --ignored --nocapture`; their measurements are recorded above.
  Focused Rust formatting, Git diff checks, and local documentation links passed.
  Full workspace and headed application acceptance are not claimed by this gate.
- 2026-09-06 follow-up: committed the single-document lifecycle and workspace
  plan as `74912aa`, leaving the pre-existing Mere pin edits in the working
  tree. Added immutable `KnotDiskComparisonV1` observations and a standalone
  Compare panel. The panel displays exact buffer and disk source as text,
  identifies comparison-time baseline changes, labels subsequent buffer edits
  stale, and offers explicit refresh and hide actions. Disk text remains
  labelled as a historical reading after an ordinary Save. A failed refresh
  removes the previous comparison. Successful New/Open/Reload/Save As clears it. Inspection preserves
  save refusal, selection, undo, dirty state, and pending close decisions.
  Two source columns are the initial comparison presentation; line-level diff
  highlighting remains open.
  The test environment continues to use the preserved `b9b0ee13` worktree pin;
  commits retain `d82afa17`. A separate source/API review found the required
  desktop host interfaces at `d82afa17`; this is not a fresh compile receipt
  for that revision.
  Final comparison validation used the same Windows/NTFS, Rust 1.97.1,
  isolated Cargo home, target directory, and three commands recorded below:
  **24 default document tests, 30 engine-enabled document tests, and 15 desktop
  harness tests passed**, with zero failures and zero document doctests.
  The desktop receipt includes edit -> Compare -> inner Save, retaining the
  old disk reading with its historical label while the saved buffer is clean.
  Focused formatting, diff checks, and relative documentation links passed.
  Headed acceptance, tabs, native picker, recent files, restart recovery, and
  A2-A4/G1-G3 implementation remain open.
- 2026-09-06: began A1 with Luna implementing standalone document commands and
  dirty-close handling, Terra implementing guarded file writes and Save As,
  and a separate Terra review. This first slice uses existing pinned Cambium
  controls and the shared desktop host. Workbench tab/docking adoption remains
  a separate dependency gate; the slice does not claim the complete workspace.
  Existing dependency-pin edits are retained and excluded from this work's
  change ownership. The implementation adds direct desktop control/test
  dependencies, `same-file` for file identity, and a Windows-only replacement
  adapter; existing Mere/Genet revision values are unchanged by this slice.
- 2026-09-06: implemented the single-document New/Open/Save/Save As/Reload
  workflow with a caller-entered path, command shortcuts, and Save/Discard/Cancel
  handling for dirty transitions and native close. Explicit read-only surface
  admission remains enforced; a file becoming read-only after editing can be
  rescued through a new Save As target. Save As preserves undo and selection.
  Compare, a native picker, recent files, recovery after restart, tabs/docking,
  and headed UI acceptance remain open A1 work. A2-A4 and G1-G3 are unchanged.
- 2026-09-06: saves retain file identity and bytes from opening or the previous
  successful save. Changed, removed, or byte-identically replaced files refuse
  dirty Save. Source is written and synced to a sibling temporary before
  replacement; Save As uses an exclusive hard-link commit and therefore requires
  filesystem support for hard links. Windows replacement uses `ReplaceFileW`
  with an exclusive recovery directory and retains recovery paths on partial
  failure. Other platforms copy basic permissions before rename; extended ACL
  and other filesystem metadata preservation remain unverified there. The
  baseline check is not transactional coordination with arbitrary external
  writers, and these tests do not establish power-loss durability.
- 2026-09-06: desktop harness coverage uses the combined application stylesheet,
  the real path text control and command shortcuts, and the host's own window
  command queue. Native-close cases explicitly supply the redraw frame requested
  by `KeepVisible` before interacting with newly created prompt controls. This
  remains windowless acceptance, distinct from headed input and visual checks.
- 2026-09-06: temporary and recovery names are independent of the document's
  basename, so a valid long document name does not become an invalid temporary
  name. A regression exercises Save As and a subsequent guarded Save with a
  230-character basename.
- 2026-09-06 validation: Windows/NTFS, Rust 1.97.1, current worktree with the
  preserved dependency edits. The following commands passed on the final source:

  ```powershell
  $env:CARGO_HOME='C:\Users\mark_\Code\.cargo-knot-a1'
  $env:CARGO_TARGET_DIR='C:\t\knot-a1'
  $env:CARGO_BUILD_JOBS='1'
  cargo test --manifest-path crates/knot-document/Cargo.toml --offline -j 1
  # 20 passed; 0 failed; 0 doc tests
  cargo test --manifest-path crates/knot-document/Cargo.toml --features engine --offline -j 1
  # 26 passed; 0 failed; 0 doc tests
  cargo test -p knot-desktop --offline --locked -j 1
  # 9 passed; 0 failed
  ```

  An isolated copied Cargo cache avoided contention with concurrent builds.
  Focused `rustfmt --check` and `git diff --check` also passed. The Windows
  replacement adapter emits the configured unsafe-code warning around its
  documented Win32 call. The narrow manifest reports unused optional patches.
  No headed, non-Windows, or full editor-library suite receipt is claimed here.

- 2026-09-05: expanded the product direction following the graph-as-data
  discussion. G1-G3 now make links and inspectable relations, saved questions
  with coordinated presentations, and explainable operations explicit
  consumer work alongside the writing workspace. Implementation remains open.
  Verified reusable starting points:
  `mere/crates/graph/graph-kernel/src/graph/edge_payload.rs`,
  `edge_taxonomy.rs`, and `query.rs`. Existing types and algorithms do not
  establish Knot persistence, domain mutation, or UI integration.
- 2026-09-05: Luna's seam review identified the explicit integration gaps:
  statement multiplicity, unknown-predicate ingestion, saved-query authority,
  relation-edit events, and algorithm direction semantics. Further inspected
  seams are `mere/crates/graph/linked-data/src/statements.rs`,
  `mere/crates/graph/graph-kernel/src/graph/filter.rs`, and
  `mere/crates/canvas/cartography/src/reading.rs`. Graph-capable arrangement
  state is not a substitute for semantic relation edits or their causal history.

- 2026-09-05: the independent Knot repository has a thin standalone
  document host and substantial product authority behind it. Published
  `a8d23e3` passed Windows CI with 9 document, 94 editor-library, and 1 desktop
  harness tests; this is not a headed acceptance receipt for the proposed UI.
  The inspected worktree has unrelated dependency-pin changes, outside this
  documentation pass. The application
  gap is UI adoption and coherent composition, not an absence of editor,
  evidence, replication, or sharing primitives.
- 2026-09-05: the current file save path compares a dirty buffer only with the
  latest disk bytes immediately before writing; it does not retain an
  open-time baseline. External-change protection therefore belongs in A1
  before accepting the writing workspace. See
  [`KnotEditor::save`](../crates/knot-document/src/editor.rs) and
  [`write_if_distinct`](../crates/knot-document/src/writer.rs).
- 2026-09-05: existing persona-scoped `KnotSettings` intentionally describes
  sync policy, not presentation preferences. Workspace preferences and
  recovery state need their own application scope. See
  [`settings.rs`](../crates/knot-editor/src/settings.rs).
- 2026-09-05: `knot-document` keeps derived readings behind its optional
  `engine` feature and the current desktop wrapper uses the narrow default.
  Desktop preview adoption must opt in there, without making every embeddable
  surface consumer resolve the engine graph. See the
  [document manifest](../crates/knot-document/Cargo.toml) and
  [desktop manifest](../apps/desktop/Cargo.toml).
- 2026-09-05: Cambium's current workspace composition is the intended
  foundation for the first workspace. Knot adoption remains unverified. Its
  persistence and component identity rules must be exercised as a consumer,
  while Knot keeps product identity and authority. Relevant code is
  `mere/crates/cambium/cambium/src/workspace.rs` and
  `mere/crates/cambium/workbench/lib.rs` and `float.rs`.

## Related material

- [`README.md`](../README.md)
- [`2026-08-24_knot_shared_surface_and_port_contribution_plan.md`](2026-08-24_knot_shared_surface_and_port_contribution_plan.md)
- [`2026-09-01_knot_editor_repository_extraction_plan.md`](2026-09-01_knot_editor_repository_extraction_plan.md)
- `mere/design_docs/mere_docs/research/2026-08-19_knot_lane_brief.md`
- `mere/design_docs/cambium_docs/implementation_strategy/2026-08-31_workbench_component_plan.md`
- `mere/design_docs/cambium_docs/implementation_strategy/2026-07-15_component_catalog_growth_plan.md`
- `mere/design_docs/cambium_docs/implementation_strategy/2026-09-03_host_ui_zoom_plan.md`
- `mere/design_docs/mere_docs/implementation_strategy/2026-08-15_projection_grammar_adoption_plan.md`
