# Knot writing workspace and mesh publication

**Date:** 2026-10-29

**Status:** User direction recorded; implementation plan proposed.

**Decision owner:** Mark.

**Source baseline:** Knot `bdb1ff4e1c8b3c203a0698807c26db20e6a83cb3`;
Mere site/resident sources checked at `4fd8c95135737fb89af332acb851c9c015399070`.

This records the corrected workspace baseline and the writer's direction for
editing, projects and publication. It does not claim new runtime behavior or
turn historical receipts into current acceptance. Existing dated assessments
retain their original scope.

It complements the [writer ergonomics brief](2026-10-29_knot_writer_ergonomics_brief.md)
and Mere's [own-device serving brief](https://github.com/merely-made/mere/blob/main/design_docs/2026-10-29_own_device_serving_brief.md),
retaining their origin framing and shared vocabulary while adding source-level
limits and proposed acceptance gates. This direction does not schedule work.

## Existing workspace and the next writing features

The desktop already edits multiple documents. [DocumentWorkspace](../apps/desktop/src/documents.rs)
owns their sessions and identities. A workbench presents a forme in a selected
projection; its desktop split/tab projection supplies tiles, stacks, splits
and collapsing. A reading follows the focused document or stays pinned to a
particular document. Opening a duplicate activates the existing entry. Each
embeddable `knot-document` surface presents one document; that is not a limit
on the desktop workspace.

User-created document splits were deferred by the
[workspace slice](2026-09-23_knot_workspace_slice1_plan.md). New documents
currently join a stack, and `apply_layout` accepts divider movement only. The
next bounded layout change should make an explicit side-by-side command
available while retaining each document's identity, session and undo history.
Opening two documents side by side by default is a layout proposal to evaluate
at wide and compact sizes, not an implemented default.

Multi-cursor belongs in Genet's shared text-editing component. Knot consumes
that contract and still needs receiving checks for selection, IME and undo.

| Writing feature | Current baseline and intended continuation |
| --- | --- |
| Quick open | A command palette, site-page commands and Navigator exist. Add one name search over admitted documents, headings and site pages. |
| Rename heading or target | Add explicit source edits and current-reference updates. Preserve historical signed relation captures; use stable identities or superseding mappings rather than rewriting signed history. |
| Find usages | The Links reading already has backlinks bounded to open documents. Extend to admitted project passages and quotes, with inline results and explicit coverage. |
| Peek | Add an in-place, source-labelled linked-passage expansion. Current Open target navigates. |
| Problems | Aggregate reading findings across admitted project members. Preserve each finding's document, source version, provenance and stale status. |
| Breadcrumbs | [Contextual navigation](2026-10-23_knot_context_navigation.md) already provides nested outline ancestry and shared navigation controls. Refine placement and presentation from this baseline. |

The [document links plan](2026-09-29_knot_document_links_slice.md),
[link workflow](../apps/desktop/src/link_workflow.rs) and
[reading presentation](../apps/desktop/src/readings.rs) are the existing seams.

## Separate edit profile

The [Rhai reading lane](2026-09-15_knot_predicates_inference_scripting_plan.md)
stays read-only, labelled and suggestion-producing. Its prohibition on script
writes continues to apply to readings and automatic execution. A separate
writer-invoked macro is a bounded writer command: the host admits the edit
under that invocation's authority.

The edit profile is a pack's scoped grant plus invocation origin, using the
existing authority vocabulary. Automatic scripts act as servitors, a watch
wakes them, and their proposals are petitions for writer acceptance. The
trusted UI applies an admitted writer command with attribution.

An edit runtime receives copied inputs and returns proposals, without a live
buffer, filesystem writer, vault writer or signing key. The host records the
script name/hash, invocation origin, target identities, source witnesses,
scope and budgets. An invocation authorizes only that run; installed read
consent is not write consent and grants cannot convert a timer, on-save hook
or model execution into a writer command. Document-carried scripts retain the
existing revocable consent and changed-script invalidation requirements.

Immediately before application, the host rechecks invocation authority,
script identity, current document/site grants, source version and bytes,
write posture and applicable selection/IME constraints. Accepted edits enter
ordinary undo with script provenance. Saving remains a host command through
the normal external-change checks. Whether a particular macro command also
saves must be explicit in its command contract; applying a buffer edit does
not itself establish successful persistence.

[The source-edit adapter](../crates/knot-document/src/source_edit.rs) already
provides immutable source witnesses, stale refusal, one captured-selection
replacement, an ordinary undo step and an application receipt. It explicitly
refuses multi-range batches. General macros and project-wide rename still
need a batch transaction, undo and failure contract. Existing command-producing
Rhai fences derive requests without enacting them; they are not this profile.

Narrow edit grants can limit documents or sites. Existing search grants
demonstrate access-before-computation, but currently distinguish disk and
vault lanes. Per-document/site granularity is additional work. An automatic
script always produces suggestions whose acceptance is a new writer command.

## Graph projects and useful hosts

A project has membership, authored order/structure and settings. The chosen
direction is a signed graph record referencing `knot-catalog` file identities;
one file can belong to several projects while staying under local filesystem
control. Site manifests are a useful smaller precedent.

[The catalog](../crates/knot-file-catalog/src/lib.rs) already has durable file
IDs and explicit rebind after the old path disappears. It does not automatically
follow arbitrary moves, and catalog relocation remains future work. Syncing
the project graph must not confer file access or invent a device-local path:
each device resolves admitted local bindings and reports unavailable members.
Concurrent ordering and relocation require explicit contracts.

An ordinary folder plus manifest and a Finder package are presentation options
for the same project identity. Whether a package contains member bytes or
references external files remains open; neither packaging choice should
silently move files or make project membership into a write grant.

Standalone products share surface contracts through Genet and choose useful
host layouts. Each standalone app should be useful alone. Portable published
renderings let recipients read material without installing its native module.
Tinderbox's attributed notes, saved-query agents and export templates are
research leads for graph/readings design, not implementation dependencies.

## Mesh publication direction

A site should remain readable when the publishing editor closes or its laptop
sleeps, provided a reachable serving replica holds the requested publication.
If every reachable copy is on a sleeping or disconnected device, an external
holder is necessary. A transport relay alone is not such a holder.

The first profile uses complete replicas of each small published snapshot on
enrolled serving devices. Transfer may avoid sending bytes already present;
that does not require sharding. Large-media placement can be addressed later.
Independent readers choose and retry replicas, so serving needs no elected
leader. Concurrent publication and selection remain a separate authority
problem and must preserve refusal on ambiguous current state.

The publisher retains authority over source, release selection and signatures.
A serving daemon receives only the released bytes, sufficient authenticated
publication evidence and its own endpoint credentials. It needs neither vault
access nor the site's publishing key. Site identity is distinct from each
mirror's network identity: do not copy the site's private key to replicas just
to make them share an address.

### What already exists

- [Knot site](../crates/knot-site/src/lib.rs) confines serving to an immutable
  snapshot. The saved-site publication path binds exact captured bytes to a
  signed community contribution.
- The shared `site-model` `PublishedSnapshotV1` has canonical bytes and content
  identity. Its `version` is a schema version, not a publication sequence; the
  record alone is not a self-authenticating public release. `SiteSelection`
  references Gemot signed history and is explicitly not a new signed envelope.
- Mere's resident site owner copies selected bytes into resident custody and
  serves without reopening the author's directory or vault. Governed Micron
  serving persists a host identity but retains an explicit loopback boundary.
  Knot's [local Micron server](../crates/knot-site/src/nomadnet.rs) uses a
  process identity and loopback interface.
- The [August device-resident receipt](../crates/knot-editor/docs/2026-08-22_device_resident_v1_receipt.md)
  covers paired private sync and verified evidence serving/reopening. It
  explicitly leaves physical two-device qualification open. The later
  [native small-web work](2026-09-11_small_web_authoring_plan.md) and resident
  process work do not establish public multi-device failover.

### Proposed serving contract

1. **Stable identity and authenticated release selection.** A reader starts
   from a trusted site identity, discovers candidate providers, obtains the
   selected release and verifies its exact content. Discovery is a routing
   hint, not publication authority. Reuse or extend Gemot's existing authority
   evidence deliberately; do not silently replace governed selection or its
   withdrawal checks with an unrelated public envelope. A cold reader must
   receive enough evidence to verify without contacting the sleeping author.
2. **Freshness separate from authenticity.** Authenticate the release's order
   or causal position and retain previously observed valid state. Reject known
   rollback and surface conflicting publications. A first-time reader isolated
   with an old valid copy cannot prove that no newer release exists. The wire
   representation and concurrent-publish policy remain design work; the V1
   schema version must never be used as a release counter.
3. **Durable replication before protected publication completes.** Capture
   and authorize the release, transfer the whole snapshot and its evidence,
   then collect durable acknowledgements for that exact release from the
   configured independent holders. Until that policy is met, report replication
   pending. Offline/local publication can exist with an explicit weaker status.
   Acknowledgement proves the observed handoff, not indefinite retention.
4. **Resident custody and measured reachability.** The daemon verifies, stores,
   announces and serves released content without the editor running. Start with
   enrolled own devices, then add explicitly configured friends or rented
   mirrors. Retries and peer choice distribute requests; exact load balancing
   is not an automatic guarantee. Storage, bandwidth and retention policy are
   explicit on each holder.
5. **Advisory sleep handoff.** Replicate continuously and at publication time.
   Before planned sleep, try to verify another holder for the current release
   and expose risk if none answers. A bounded keep-awake request can help; power
   loss, forced sleep, partitions and simultaneous sleep remain possible. It is
   never the sole durability mechanism.
6. **Mobile participation without an uptime promise.** Phones author, fetch
   and hand off publications. Serving while active is useful; dependable
   background hosting is not a portable mobile guarantee. Persistent desktop,
   NAS, Pi or external hosts supply the dependable availability tier.

The UI should distinguish facts such as these, with observation times:

| State | Meaning |
| --- | --- |
| Released locally; replication pending | Release exists, but the configured remote-copy policy is incomplete. |
| Copied to 3 devices | Three distinct devices acknowledged durable custody of this exact release. |
| 2 replicas recently reachable | Recent checks succeeded through stated routes; this is not a future uptime promise. |
| Reader verified release R | A reader fetched and authenticated R through an observed serving route. |

An own-device probe is not evidence of public Internet reachability. A stable
mesh identity also needs a compatible reader or gateway; ordinary browsers do
not acquire mesh discovery or independent signature verification automatically.
Any gateway's availability and verification boundary must be explicit.

### Public and private holding

Public snapshots may be held by readers who opt in, friends, or rented hosts
without publishing authority. A signature prevents an altered release from
verifying; it does not force service, establish newest state, guarantee harmless
executable content or recall bytes already copied. Public mirroring is not
private-vault replication. A lower-trust host may hold sealed private blobs
under a separate key/access policy; plaintext readers and writers remain
explicitly admitted. Existing governed withdrawal rules retain their scope.

### Transport and platform evidence

[Reticulum announces](https://reticulum.network/manual/understanding.html)
authenticate destinations and establish routes. A site still needs a mapping
from its identity/release to independently keyed serving destinations.
[Iroh's content-discovery work](https://www.iroh.computer/blog/iroh-global-content-discovery)
likewise separates provider discovery from endpoint connectivity and verifies
downloaded content; its global discovery implementation is described as
experimental. Neither transport alone is a site replication/retention policy.

[Apple's sleep guidance](https://developer.apple.com/library/archive/qa/qa1340/_index.html)
distinguishes idle sleep from forced sleep: an application cannot make lid
closure or forced sleep into an indefinite hosting promise.
[iOS background execution](https://developer.apple.com/documentation/uikit/extending-your-app-s-background-execution-time)
is time-limited, while Android has explicit
[foreground-service restrictions](https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start).
Android can support some background service arrangements; the portable
product rule is not to count phones as dependable unattended replicas.

The Beaker comparison needs a historical correction: its
[Homebase](https://github.com/beakerbrowser/homebase) already supported
self-hosted Hyperdrive serving, daemon operation and HTTP mirroring. Peer
hosting was not absent. Knot's proposed contribution is managed replication,
observable custody and independently qualified failover. Beaker's
[archive account](https://github.com/beakerbrowser/beaker/blob/master/archive-notice.md)
should be kept separate from an inferred explanation of its outcome.

## First acceptance gate and consequences

The first mesh slice should prove this exact scenario with independent devices:

1. Device A publishes release R and transfers its complete content and
   authenticated publication state to independently keyed device B.
2. B durably acknowledges R and survives its own process restart.
3. Stop A and make its storage and routes unavailable.
4. Start cold reader C with only the trusted site identity. C discovers B,
   verifies publication authority and exact bytes, and reads R.
5. Repeat with tampered bytes, unavailable providers, an older valid release,
   conflicting current selections and known rollback. The cold-reader old-copy
   case must report limited freshness knowledge honestly.

Process tests should precede this physical-device gate. Exercise editor-closed
serving, host restart, independent reader routes and planned/abrupt loss as
distinct evidence. No existing receipt cited above completes this gate.

Knot owns saved-file admission, release intent and product wording. Mere owns
reusable snapshot, authority and resident contracts. Transport adapters own
their connectivity mechanisms; the selected host owns daemon lifecycle,
storage admission and OS power integration. This makes low-trust mirrors
possible while adding real work in discovery, portable authority evidence,
durable acknowledgements and truthful availability status.

The writing features and mesh can proceed as separate bounded workstreams.
The immediate documentation corrections do not enact source-edit scripts,
graph projects, split commands or public hosting.
