# Composition material belongs in Knot's mere

Status: accepted ownership decision; implementation qualification below.

## Decision

Knot's own mere is the retained-data home for its domain. Sharing implementation
with sibling apps does not change that ownership. The initial plaintext
`composition/collection.json` prototype is a legacy import source, not the
desktop's ongoing collection store. Moving a library to the Mere repository
would not accomplish this data integration.

The desktop submits copied `CollectionItem` data through a host-issued
`CompositionRetainPort`. Its explicitly selected target binds persona display
identity, space, writer and encryption profile. The resident rechecks live
authority, grant bounds and destination under its normal locks, then authors a
signed, sealed `RetainCompositionV1` operation through Knot's mutation gate.
This projection is separate from editable/publishable vault documents. Receipts
retain operation hashes; historical authors are distinct from current authority.

Retention starts neither networking nor publication. A space's existing
replication policy still applies when its owner runs replication; this is not a
promise that retained data can never sync.

The new event is additive for updated readers, but older Knot binaries may
refuse to replay an unfamiliar event. Upgrade participating readers before using
composition retention in a replicated space; no protocol-negotiation upgrade
or peer rollout is performed by the desktop.

## Attach an existing personal mere

Normal desktop builds include the capability, but ordinary launch grants no
destination. Explicitly name an existing data root and persona:

```sh
cargo run -p knot-desktop --bin knot -- \
  --mere-root /absolute/path/to/existing-data \
  --mere-persona YOUR-EXISTING-PERSONA-UUID \
  /absolute/path/to/essay.djot
```

Personae's configured startup-unlock policy applies; no passphrase or key goes
on the command line. Attachment refuses missing operation stores, absent device
identity, unavailable unlock, divergent existing vault material, or a store held
by another owner. It creates no persona, bootstraps no mere, migrates no unsynced
documents, stops no resident and starts no peer transport. Personae's existing
loader may upgrade a legacy device-identity encoding under its established
unlock policy. Reconcile divergent vault material through its owner first.

Then select the destination in **Readings → Collection**. Reads and writes run
on workers. Opening the panel does not select a target. Without one, Collect
refuses instead of writing a plaintext fallback. A `--no-default-features` build
remains file-only unless `mere-retention` is explicitly enabled.

Embedding hosts may supply an already-owned resident capability to
`run_desktop_with_targets`; they must not open a competing owner. A GUI persona
chooser and live-resident IPC attachment remain future work.

## Legacy import and failures

Existing sidecars remain untouched. Import is an explicit action into the chosen
mere, preserving item IDs, text and provenance. Items are independently retained;
a partial batch is not transactional. Same-writer retries reuse matching
identity/content after reopen; conflicting content is refused, not replaced.
Refresh preserves uncertain retry IDs and destination binding. New target
selection must not relabel old worker results. Successful import never deletes
the legacy file; backup/removal remains the user's decision.

The corrected path requires unlocked, admitted authority. That is more involved
than an always-writable sidecar, but avoids splitting retained material from
Knot's existing encryption and history. Immutable collection items are supported;
mutable organization, retraction and additional domain records such as saved
sessions are follow-ups, not disguised publishable prose documents.

## Qualification

The earlier five native captures qualified the sidecar prototype, not this
corrected route. The corrected route has separate evidence:

- Final `cargo test --locked --workspace`: 555 passed, zero failed, two ignored.
  The file-only desktop check and native desktop build pass. Strict Clippy passes
  for `knot-composition` and `knot-readings`; the broader desktop invocation is
  blocked by nine pre-existing `knot-editor` lint errors, not recorded as clean.
- Core real-store tests cover encrypted persistence/reopen, authority refusal,
  cross-writer history, idempotence and exclusion from publishable documents.
- `apps/desktop/tests/composition_mere.rs` drives the UI worker against a real
  resident, checks the signed operation and absence of plaintext fallback, then
  reopens and reads the same item and operation.
- The native `composition.scn` run on 2026-09-30 passed in 179 frames with five
  distinct nonblank captures. Visual review confirmed the lexical empty state,
  opt-in sound source, Night/light rhyme result, unavailable collection, and
  source registry. Local receipts are in
  `/Users/markik/Code/testing/knot-editor/composition-mere-20260930/receipts`.
  This native run deliberately supplies no mere authority; it does not qualify
  successful native retention or a real user's Personae startup-unlock flow.

The sound-result view extends below the captured viewport; this is not an
all-viewport layout acceptance claim. Whole-history reads use the existing
operation-store path; result bounds are not a claim of a globally bounded scan.
