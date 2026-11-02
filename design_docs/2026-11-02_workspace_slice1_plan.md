# Knot workspace: open work after slice 1

**Date:** 2026-11-02
**Status (2026-11-02):** cut from [the workspace slice 1 plan](archive_docs/2026-11-02/2026-09-23_knot_workspace_slice1_plan.md), archived whole with its steps, 1,200-line Progress log and inline run evidence. Slice 1 is complete with recorded visual differences (archived Status, line 6), and the Forme second-host continuation of 2026-10-29 is implemented with receiving and bounded native qualification. Open: user-created splits and the side-by-side command, pointer dragging, automatic session restoration, and the follow-ups carried from the archived native review, document-links and context-navigation records.

## Rulings in force

The archived plan's design calls and its steps and done-conditions (sections "Decisions ruled 2026-09-23", "Design calls this plan makes", "Steps and done-conditions") stay in force, cited by section and grandfathered. The Forme continuation's ownership split stands: Knot owns the open documents, tile roles and the local preferences store; Platen's `workspace-editor` owns the layout draft and acknowledged-save lifecycle; no document text, dirty state, source identity, catalog graph or source undo is part of a layout save (archived lines 1614-1620). DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `workspace_slice1 D<n>`.

## Decisions

### workspace_slice1 D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all to Code/testing". The Forme receipt folder (`2026-10-29_forme_workspace/`) and the other receipts this plan cites are in `Code/testing/knot-editor/receipts/`, cited as paths; the Forme runner `run_native.py` anchors at `parents[3]` and no longer finds the repository from there.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/knot-editor

### workspace_slice1 D2: cut the plan, draft first (2026-11-02)

Mark, 2026-11-02: "Archive landed + cut 7, draft first". The 1,679-line plan is cut under DOC_POLICY section 8 and archived whole after this draft is reviewed.

Links: area documentation · replaces 2026-09-23_knot_workspace_slice1_plan · summary old plan cut to open work and archived whole

### workspace_slice1 D3: old hashes (2026-11-02)

Mark, 2026-11-02: "Same: live docs only". This plan cites main hashes; the archived plan keeps its own. Mapping: [the receipts file](archive_docs/receipts/2026-09-23_knot_workspace_slice1_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes; archived docs keep old ones

## Open work

### O1. User-created splits and the explicit side-by-side command

User-made splits, tab drags between stacks and floats were declined in slice 1 ("the tree is unchanged and the gesture is logged") and "adopted later from the same model" (archived lines 41-47). New documents currently join a stack, and `apply_layout` accepts divider movement only; the next bounded layout change "should make an explicit side-by-side command available while retaining each document's identity, session and undo history" (`2026-10-29_knot_writing_workspace_and_mesh.md`, section "Existing workspace and the next writing features"). Opening two documents side by side by default is a layout proposal to evaluate at wide and compact sizes, not an implemented default.

**Done when:** an explicit command opens a document beside the focused one; each document keeps its identity, session and undo history; a headed run at a wide and a compact size shows the result; and Mark rules whether side by side becomes a default.

### O2. Pointer dragging

The shared Frisket separator "has semantic attributes but no pointer handler in the receiving cohort", so pointer resizing, tab drags, new splits and floats are separate follow-ups to the Forme slice (archived lines 1626-1629, 1656-1658). The handler is Mere's.

**Done when:** a pointer drag of the root divider previews through `FormeSession::edit` and applies or discards like the command route, in a headed run; tab drags and floats each get a decision of their own.

### O3. Automatic document-session restoration

Unlock may load the last saved layout "for exactly matching tile bindings"; the slice "does not reopen document sessions or promise automatic workspace restoration at launch" (archived lines 1633-1640). Workspace layout persistence for the tile arrangement exists through Forme; restoring the open documents does not.

**Done when:** Mark rules whether launch reopens the previous documents; if yes, a fresh-process receipt reopens them with their layout, refuses a changed binding set exactly as Unlock does, and never restores a layout authored for a different open set.

### O4. Presentation follow-ups from the archived native review

Recorded 2026-09-29 and not rechecked since (`archive_docs/2026-11-02/2026-09-29_knot_workspace_native_review.md`, lines 69-96): in the last-tab graph an empty session column consumes left-hand space and the default square node control obscures the start of a label; in the two-site view the saved-snapshot footer and the Close site control are cramped. The review's toolbar displacement after a source click was addressed by the scenario-focus fix ("toolbar stays stationary", `archive_docs/2026-11-02/2026-09-29_knot_shared_presentation_review.md:31`), and its sans-serif preview by the bundled Source Serif 4 preview (`2026-09-29_knot_typography_slice.md:12`).

**Done when:** a headed run at the default size shows the graph without the empty column and with the label legible, and the two-site footer uncramped, or Mark accepts the differences as recorded.

### O5. Native GUI review of the shared source-edit link adapter

The adapter (revision-bound selection proposals, one-step undo, guarded saving) is qualified at the source/model, UI-harness and fresh-process level only; "native GUI observation is not claimed" (`archive_docs/2026-11-02/2026-09-29_knot_document_links_slice.md`, "Published supplier receiving qualification"). The document-links record also names no reviewed desktop screenshot.

**Done when:** a native run inserts and removes a link through the form with reviewed captures, source bytes unchanged until Save, on the published supplier.

### O6. Choosing among multiple site pages in contextual navigation

Down from a site chooses the last visited matching page, then the first open bound page; "choosing among multiple site pages remains future work" (`archive_docs/2026-11-02/2026-10-23_knot_context_navigation.md`, "Scope boundary", line 82).

**Done when:** Mark rules how a writer chooses among open pages of one site; the chosen route then has a scenario beside `scenarios/context_navigation.scn` that asserts it.

## Findings

- 2026-11-02: the slice's scenario-scroll follow-up and the Taproot/Mesquite runner unification, which the archived Status calls "recorded below", landed on 2026-09-27 (archived "step 7c scenario-scroll follow-up" and "Taproot/Mesquite unification" entries); they are not open work.
- 2026-11-02: `apps/desktop/src/documents.rs` keeps document payloads apart from Workbench's tree, which is why a layout preview needs no snapshot of document contents (archived Findings, line 1645).
- 2026-11-02: the Forme implementation is `apps/desktop/src/workspace_forme.rs` and `documents.rs`; the complete desktop library gate passed 372 cases with one ignored timing probe on 2026-10-29 (archived Progress, lines 1675-1681).

## Progress

- 2026-11-02: plan cut from the workspace slice 1 plan; decisions D1 to D3 recorded. The evidence index and old-to-main hash table are in `archive_docs/receipts/2026-09-23_knot_workspace_slice1_plan_receipts.md`.
