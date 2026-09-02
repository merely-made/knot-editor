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
