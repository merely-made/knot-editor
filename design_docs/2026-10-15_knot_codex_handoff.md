# Knot handoff for Codex, 2026-10-15

**Status:** handoff, written for a Codex agent to execute in order. Claude
lanes did the design and most implementation on 2026-10-15. Mark moved
implementation to Codex and kept Claude for design. Every decision below is
Mark's and is ruled; do not re-decide one. If a task needs a decision that
isn't written here, stop and ask Mark with concrete options.

## 1. Where Knot stands

Knot `main` is `90ae0c2` (receipt-only), on code `b19eebf`.

| Pin | Value |
|---|---|
| Mere | `0c03dff3d`, through `https://www.github.com/merely-made/mere.git` |
| Murm | `3d9e799f`, through the plain `https://github.com/merely-made/murm.git` URL (moves to Murm `main` in task 1) |
| Genet | `ae3d9f5d` |
| vello fork | `276b4664` |
| Netrender | `f4f9c0d8`, with Mere's `netrender.git` patch bridge |

What landed today, oldest first:

- **Application door.** Custody, admission and identity come from
  `graphshell_endpoint::door::*` (feature `door`), not `graphshell::native`.
- **Mere overmap.** With `--mere-root`, `--mere-persona` and
  `--mere-domain <domain>`, the desktop reads Djinn's `mere/<domain>` route as
  app `knot-editor` (feature `mere-overlay`; `crates/knot-editor/src/mere_overlay.rs`
  and `apps/desktop/src/mere_overlay.rs`). It draws that mere's sessions and
  archives as an overmap tier above Knot's catalog graph
  (`SessionPlacement::Tier`, `apply_and_lay_out`). Design:
  `2026-10-15_knot_mere_overmap_layer.md` and
  `2026-10-15_knot_overmap_layer_rendering.md`.
- **Merges.** Knot's overlay carries Branch, Merge into source, the pending
  merge's per-conflict choices, Apply and Cancel to Djinn's route. Djinn's
  test `knot_overlay_merges_a_branch_through_the_door` drives it end to end.
- **Serving half.** Tests and `k2_peer` use `graphshell-host`. Knot's library
  code names nothing from `graphshell`; only `view` and `sessions` in two
  tests still do, and task 1 removes them.
- **backstep.** `edit-history` was renamed to `backstep`
  (`recipe/history.rs`: `use backstep::History`). It is published on
  crates.io as 0.1.0.
- **Native receipts.** `docs/receipts/2026-10-15_mere_overmap_live/`. The
  latest is `tier-after-b19eebf/`. Its seed run is marked `RESULT fail` only
  because the presentation watchdog tripped after the last step; every
  assertion passed.

Last full Knot check (2026-10-15): 623 passed, 4 ignored.

### Mark's rulings that bind this work

- **Overmap.** Catalog graph stays the graph; the mere route is an overmap
  tier above it. A session node expands in place into its card and grows the
  tier. Archive nodes use hover-peek / click-pin popovers. Loose archives
  hang off an Archive shelf node. Past N codicils (a host setting, default 3)
  they collapse into "+N". Labels show the name only, flip left near the
  tier's controls, and cut long names in the middle.
- **Vocabulary.** *Fork* is a full copy that goes its own way. *Branch* is a
  full copy meant to merge back. *Pick off* tears out a part, named
  "X (picked from Y)". Sessions never use "leaf".
- **Merge-in (M1–M8).** Pending merge with per-conflict Source, Target, Edit
  by hand, or Add as new; three-way deletions; `MergedFrom` provenance; the
  source is kept and marked merged, and undo clears the mark. A failed Apply
  re-plans and carries the surviving choices.
- **Labels per host.** Option helpers return values; Knot keeps its own
  wording ("automatic", the empty label, "Default: …").
- **`--mere-domain` launch flag.** Keep it.
- **Git.** Land on `main` as work goes green. Fetch first; merge
  `origin/main`; never rebase or force-push. Mark has authorized pushing.

## 2. Tasks, in order

### Task 1: the cross-repository repin round (Mere, Knot, Mere, Turnstone)

This lane owns the round's middle. Follow the remote mere route spec §7
exactly: `mere/design_docs/mere_docs/technical_architecture/2026-10-15_remote_mere_route_spec.md`.

Murm is already done. Murm `main` is `fb6360c12b`, which merges the C1
callback fix (`3d9e799f`) and the in-process kind (`38e9c9b8`). Use
`M = fb6360c12b`.

1. **§7.2, Mere.** Replace every `38e9c9b8dffe19ff374bd226375e27eea2eb98f6`
   in Mere's root `Cargo.toml` (40 occurrences at `61fba39d9`) with Murm `M`'s
   full hash, then run the §7.2 checks. Call the pushed commit `R`.
2. **§7.3, Knot.** Knot onto `R`, using the six listed edits.
   - Edit 5 (`dropped: Vec::new()`) may already be satisfied; check it
     compiles.
   - Also sweep for `edit_history` (none expected) and for `MereViewRequest`
     / `SessionStep` matches without a wildcard arm. Both enums are
     `#[non_exhaustive]`.

   Call the pushed commit `K`.
3. **§7.4, Mere's Knot pin.** Move it to `K`, then run its checks. Call it
   `R2`.
4. **§7.5, Turnstone** (merely-made/turnstone, `main` `79f5f87`). Move it onto
   `K` and `R`, dropping `graphshell`. Call it `T`.
5. **Handoff.** Message the Woodshed lane (or update its handoff,
   `woodshed/design_docs/2026-10-15_woodshed_codex_handoff.md`) with `M` and
   `R2`, so it runs §7.6.

**Done when:**

- `K`, `R2` and `T` are pushed;
- Knot and Turnstone name no `graphshell` (`cargo tree`);
- every graph resolves one Mere and one Murm source;
- the lift plan's Progress section records the revisions and checks.

**Checks:** as §7.3. Use `TMPDIR=/private/tmp`, because knot-site rejects
the `/tmp` symlink. knot-document and knot-site are separate workspaces
(`--manifest-path`). Expect 623 passed, 4 ignored, with one test moved to
Djinn.

**Stop and report if:**

- anything fails beyond mechanical path, pin or wildcard fixes;
- a lockfile resolves a second Mere or Murm source that you can't remove.

### Task 2: capture the pending-merge card natively

The overmap receipt has no capture of the pending-merge card, because the
live fixture never makes a conflict.

1. In Mere, extend `ports/djinn/examples/knot_overmap_live_host.rs`.
   - Keep the `PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET` marker and public test
     keys only.
   - Seed one session, branch it (`MereSessions::branch_at`), then make
     conflicting title edits in the branch and its source.
   - Push Mere.
2. In Knot, add `scenarios/overmap_live_merge.scn` beside
   `scenarios/overmap_live.scn` and `overmap_live_reopen.scn`. It should:
   - open the branch's card and press "Merge into source";
   - assert the pending card lists the conflict;
   - choose Source, then Apply;
   - assert the source's title changed and the branch reads merged.

   Capture each step.
3. Run it with `run_overmap_live.sh` from that receipt directory, following
   its README for the exact commands, and commit the captures plus a README
   section.

**Done when:** the run reports `RESULT ok` with nonblank captures of the
pending card and the merged result, committed under a new
`tier-merge-<knot-commit>/` directory.

**Native traps:**

- **Locked screen.** It gives 0 presented frames. Check with
  `ioreg -n Root -d1 -a | grep -A1 CGSSessionScreenIsLocked`, wait for an
  unlock, and never count a stalled run.
- **Problem Reporter.** A crash dialog stole the foreground on 2026-10-15.
- **Serialization.** Foreground runs are serialized with the Woodshed and
  Cleromancy lanes. Ask before starting, keep runs short, and close every
  process you start.
- **TMPDIR.** Use `TMPDIR=/tmp` for anything with Unix sockets: macOS limits
  socket paths to 104 bytes, and the default temp dir is longer.
- **No `timeout` on macOS.** Background the run and poll
  `scenario.done`.

### Task 3: re-run the full Knot suite after task 1

The last repin's report didn't re-run knot-document, knot-site, font_roles
or typography_fonts, so run all of them on `K`.

**Done when:** each suite's count is recorded in the lift plan's Progress
entry for the round.

## 3. Not this lane's work

These are in Mere and Turnstone; they're listed so you don't start them by
accident.

- **Djinn's dialable host** (spec §8). It starts only once a non-persona
  trusted root exists (Mark, Q3). Device roots derive from the persona, with
  pairing issuing grants; place roots come from the place record (Q2).
- **Turnstone's remote overmap pane** (spec §9). This is the receipt for
  opening a remote mere's overmap over the network.
- **Push notices** (spec §10).
- **The Graphshell lift's remaining work.** L13 `personal_sync` waits for
  Mere's facet dissolution brief. `canvas_reader` waits for a host that needs
  a screen-reader producer.

## 4. Repository practice

- Several sessions share Mark's local checkouts. Work in your own clone or
  worktree, and commit only your own paths.
- The `weave` merge driver was removed from every family `.gitattributes` on
  2026-10-15. User git now has `merge.conflictStyle=zdiff3` and `rerere`.
  Mergiraf is on trial only in Mark's local `mere` checkout.
- Still check merged files by eye. Earlier merges dropped imports and
  duplicated blocks.
- Commit trailers in this family name the model that wrote the commit.
