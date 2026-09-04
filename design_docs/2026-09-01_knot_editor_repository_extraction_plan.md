# Knot Editor Repository Extraction Plan

**Date:** 2026-09-01  
**Status:** E0 complete; E1 and E2 implemented and green in open consumer PRs
**Owner:** Knot Editor

## Ruling

Knot Editor is an independent document product and an embeddable Mere port.
Its port relationship does not make Mere the owner of its source.

Knot owns files-in-place authoring, document and vault authority, revisions,
evidence, peer replication, and publishing. Mere owns the generic graph,
identity, policy, transport, and resident contracts that Knot consumes. Genet
owns the retained surface and native host contracts. Turnstone and other hosts
consume Knot through those generic seams.

Pelt and Graphshell remain in Genet and Mere respectively because each is its
parent platform's first-party reference application. Knot differs because its
document authority remains meaningful in a standalone process and in multiple
hosts.

## Repository shape

- `knot-document` owns one-document state, write posture, edit/save intents,
  the Cambium component, and the erased retained-surface constructor. It is a
  nested workspace so its narrow resolver stays independent of the broad
  editor graph.
- `knot-editor` owns directory and vault sources, evidence, search, sync,
  publishing, resident composition, and Graphshell adapters.
- `knot-desktop` owns the standalone native process and composes the concrete
  `knot-document` surface without host-specific product state.

Mere and Turnstone may contain integration code, but Knot-specific product
semantics remain here. Generic host traits must not gain Knot methods during
the cutover.

## Migration gates

### E0. Independent source

Extract both Knot crates and the desktop wrapper with their Git history. Add a
standalone workspace, immutable Mere and Genet pins, an application lockfile,
continuous integration, and repository-facing documentation.

Done when a clean checkout resolves independently and passes the document,
editor-library, and desktop compile gates.

### E1. Consumer cutover

Change Turnstone and Djinn from Mere-local paths to one immutable Knot Editor
revision. A consuming workspace must resolve one source identity for every
Mere and Genet contract that crosses the boundary.

Done when Turnstone's Knot authoring checks and Djinn's resident Knot checks
pass against the external repository.

### E2. Mere source removal

Remove `ports/knot`, `ports/knot-document`, their workspace entries, and stale
Mere-owned wording. Keep historical integration plans as links or clearly
marked snapshots rather than competing authority.

Done when Mere's workspace metadata resolves without local Knot packages, the
focused Djinn gates pass, and a repository search finds only intentional Knot
consumer and integration references.

## Stop rules

- Do not accept duplicate Cargo source identities for Graphshell, Chirograph,
  Sceno, transport, or Genet surface types.
- Do not make Turnstone own file, vault, evidence, or sync authority.
- Do not delete Mere's source copies until both external consumers compile
  against an immutable pushed revision.
- Preserve unrelated work by performing each cutover in an isolated worktree.

## E0 receipt

The preserved-history repository is public at
<https://github.com/merely-made/knot-editor>. A fresh clone of immutable
revision `4434584ec8b5448cfacfdef515cf60839cb38c52` independently resolved the
root and narrow document workspaces, then passed:

- `cargo test --manifest-path crates/knot-document/Cargo.toml --features engine`:
  15 passed;
- `cargo test -p knot-editor --lib --locked`: 94 passed;
- `cargo test -p knot-desktop --locked`: 1 passed.

[Hosted Windows CI run 33572522617](https://github.com/merely-made/knot-editor/actions/runs/33572522617)
passed the document, editor-library, and desktop-host gates at revision
`2e01c78851c8d9d0243b472b2b7a3fb4726ad4bc`.

The resolved graph contained one source identity apiece for the Mere contracts
crossing the boundary and for Cambium, Genet host, DOM, and layout contracts.

## E1 and E2 review receipt

Consumers pin pushed revision `c4d15aa66eee46060081902ba8459f01c9c82f98`.

- [Turnstone PR #4](https://github.com/merely-made/turnstone/pull/4) passes two
  five-test Knot authoring suites and five ignored external-endpoint tests
  against the executable built from this repository.
- [Mere PR #5](https://github.com/merely-made/mere/pull/5) removes both local
  Knot packages. Its metadata contains one Git identity for each Knot package
  and one identity for every audited Mere and Genet boundary package. Djinn's
  live pairing, joined-sync, route-reopen resident test passes against the
  external package.

Both PRs are mergeable and clean. E1 and E2 close on their merge rather than
turning review-state changes into default-branch claims.

## Progress

- 2026-09-03: **the platform-boundary repoint landed** (P4's first consumer, per
  mere `design_docs/mere_docs/implementation_strategy/`
  `2026-09-02_platform_boundary_and_repository_topology_plan.md`). Both
  workspaces now resolve one source per repository: genet.git at
  `115d348deddc344d949754e63beaece47cf49f34` and mere.git at
  `b57d2021bac2bb32febfd5b96098384a63ef58a4`. `cambium`,
  `cambium-genet-winit-host`, `illume`, `inker`, `knot-editor-host` and
  `nematic` moved from genet.git to mere.git; `knot-document`'s
  `genet-host-api` became `mere-surface-api`, the Mere half of that crate's
  split; `fleece`, `genet-probe`, `genet-scripted-dom`, `layout-dom-api`,
  `genet-taffy`, `parley` and `ipc-channel` stay on genet.git at the new
  revision. E1 and E2 are unblocked and stay open: their consumer PRs are
  stale and are redone as fresh commits against a pushed revision of this
  repository, which the stop rule above still gates.

- 2026-09-04: **E2 landed in mere** at `d666e1604cdbeb127527fce62fb97e41504bdc9e`
  (not pushed). `ports/knot`, `ports/knot-document` and `ports/knot/desktop`
  are removed, `ports/knot/editor-host` moved to `crates/inker/knot-editor-host`
  as integration code, and Djinn consumes `knot-editor` 0.0.3 and
  `knot-document` 0.0.1 from this repository at
  `fcd004b655b595038eba0a7e49f209b8477edadf`. mere's unpatched `cargo metadata`
  names those two packages from one revision of this repository and no local
  package by either name; its workspace check is green patched and unpatched,
  and Djinn's live pairing, joined-sync, route-reopen resident test passes
  against the external package. Turnstone's half of E1 is green at its own
  pushed `e68e2764e4d`. The stale PRs (turnstone #4, mere #5) are superseded by
  these fresh commits and are Mark's to close.
