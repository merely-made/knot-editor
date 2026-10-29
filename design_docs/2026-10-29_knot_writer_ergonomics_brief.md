# Knot writer ergonomics brief: what a writing environment borrows from an IDE

**Date:** 2026-10-29
**Kind:** research brief digesting a chat chain with Mark (2026-10-29). Mark asked
how far Knot is from an IDE, then turned the comparison toward writing.
Analysis, vocabulary alignment and prior art are added here.
**Status:** direction only. Nothing is scheduled. The ruling that Knot is not an IDE
stands.
**Grounded against:** Knot `main` at `bdb1ff4`.
**Anchors:**
[Pelt/Knot direction](https://github.com/merely-made/genet/blob/main/docs/2026-07-24_pelt_knot_direction.md)
(the "authoring browser, not an IDE" ruling),
[port plan non-goals](2026-07-25_knot_port_plan.md#6-non-goals),
[design pass](2026-09-23_knot_design_pass.md),
[commands slice](2026-09-29_knot_commands_slice.md),
[predicates, inference and scripted readings](2026-09-15_knot_predicates_inference_scripting_plan.md),
[shared source-edit proposals](https://github.com/merely-made/mere/blob/main/design_docs/archive_docs/2026-10-28/2026-10-18_source_edit_proposal_plan.md),
Mere [TERMINOLOGY](https://github.com/merely-made/mere/blob/main/design_docs/TERMINOLOGY.md).
The stack-level half of the same chain (serving sites from your own devices, Beaker's
lessons, apps as modules) is in Mere's
[own-device serving brief](https://github.com/merely-made/mere/blob/main/design_docs/2026-10-29_own_device_serving_brief.md).

## Origin framing

Mark, 2026-10-29, after a feature-by-feature comparison with an IDE:

> I don't intend to make this an IDE. It's just a comparison, for the record. And
> it's been illuminating, because I do think some ergonomics from IDEs are, like,
> things that word processors could really use, to be honest.

On working with several documents:

> I do think that being able to work on multiple documents at the same time just
> seems like, you know, obvious slam dunk. You know, I mean, what with Pelt existing
> and the ability, you know, to do recursive splits just seems very straightforward
> to be able to do more than one document at a time.

On scripts that edit:

> Extensibility. Definitely extensibility, 100%. And I do think that script should be
> able to deal with, you know, source, and should be able to interact with source.
> However, the point with the Rai [Rhai], I think, was to accomplish a particular set
> of goals. Maybe we can have a different, I don't know, capability profile or
> something.

On projects:

> what is the difference between a project model and, like, storing things in, like,
> I guess, the graph in some type of a way like that, right? Isn't it kind of just,
> like, I don't know, like a file system anyway?

> [...] represent with the graph in such a way that it's replicable and, you know,
> accessible from your other devices without trading away authority.

## What Knot is, by category

Knot is a **writing environment** and, by its own ruling, an **authoring browser**.
A word processor is built to produce a formatted page. Knot is built to show
everything around the text, keep files where they are, and publish. IDEs share that
shape (palette, outline, folds, preview, diagnostics, splits), which is why their
ergonomics transfer. Their language, toolchain and debugging layers do not transfer,
and are not wanted.

## Correction recorded

The chain first said Knot is built around one document at a time. That is wrong for
the desktop app. `apps/desktop/src/documents.rs` keeps many open documents as tiles
in the Workbench tree, with stacks and collapsing. A second open of the same file
switches to the existing session (resolved by catalog identity, else canonical
path). Readings follow the focused document or stay pinned to one. "One document"
describes only the embeddable `knot-document` surface.

Vocabulary alignment: **Pelt** is the tiled reference browser. Knot's splits ride
the **workbench**, whose **forme** is the split/tab tree. Both use the same
composition machinery, so multi-document needs nothing from Pelt itself. What
remains is polish: an "open beside" command and a side-by-side default.

## Ergonomics worth borrowing

| Ergonomic | What it is for a writer | Seam and owner | State on `main` |
|---|---|---|---|
| Multi-document splits | Draft beside notes or a source text | Workbench tiles (Knot desktop) | Exists; needs an "open beside" command |
| Multi-cursor | Edit repeated phrases or list items together | Genet text-editing primitive | Absent |
| Find and replace | In one document and across a site or project | Knot command catalog; project-wide replace as `mere-source-edit` batches | No find command in `workspace_commands.rs` |
| Quick open | Jump to a document, heading or site page by name | Command palette items | Palette exists; lists commands only |
| Rename a target | Rename a heading or link target; update every link and relation | Document-links slice plus revision-checked source-edit batches | Absent |
| Find usages | Backlinks and quotes of this passage, inline | Relations and document links | Data exists; no inline view |
| Peek | Expand a linked passage in place | Read-only reader tile or popover | Absent |
| Problems pane | One list of every reading's findings across documents | Readings typed by kind (design pass) | Readings are per tile |
| Breadcrumbs | Current outline position in the title or status bar | Outline plus status bar chips | Outline is a tile |

## Scripts that edit source

The existing vocabulary already supplies what the chain called a "capability
profile." No new term is needed:

- **pack / mod** split bundles by trust depth. A pack holds scripts and macros; a mod
  holds wasm components and reaches further.
- **servitor** is a resident helper with a grant that proposes changes through the
  participant gate.
- **petition** is that proposed change; **watch** is what wakes a participant.
- The **trusted UI** commits directly, with attribution.
- `mere-source-edit` already gives Knot revision-checked, previewable UTF-8 edit
  proposals.

The profile is a pack's grant plus how the script runs:

1. **Reading lane, unchanged.** Rhai readings stay read-only and labelled with what
   produced them, as the predicates plan rules.
2. **Writer-invoked macros.** A command from a pack, run by the writer, returns a
   `mere-source-edit` proposal. Knot rechecks it against the document revision, can
   preview it, and applies it as one undo step labelled with the pack and script.
   This is the writer's own command, so the rule that only the writer's explicit
   command reaches the vault holds.
3. **Watched scripts.** Scripts woken by a watch (on save, on a timer, from a model)
   are servitors. Their source edits arrive as petitions the writer accepts or
   rejects, never as direct commits.
4. **Scope.** The grant names which documents or sites a pack may propose edits to.
   It is reviewed at install, beside its watches.

## Project model as a graph record

A file system provides containment: each file has one parent. A writing project
needs three things containment does not give:

- **Membership:** which files belong. A file may belong to several projects.
- **Order and structure:** like Scrivener's binder.
- **Settings:** for example publish target and defaults.

Proposal: a project is a signed graph node whose **nested graph** holds membership
and order, pointing at `knot-catalog` file identities that survive renames and
moves. It is authored and syncs to the owner's devices like relations, so it
replicates without moving authority. Files stay in place.

On disk and in Finder it is either:

- an ordinary folder plus a manifest, as site folders already work with `site.json`;
  or
- a macOS package, shown by Finder as one document, like Scrivener's `.scriv`.

Site folders are the existing small project model. A general one can grow from them.

## Prior art

| Tool | Relevance to Knot |
|---|---|
| Scrivener | Binder, research beside the manuscript, compile to publish. Closest in workflow. |
| Tinderbox, Storyspace (Eastgate) | Notes with attributes, maps, links; agents as saved queries that collect notes. Closest in spirit to relations and the graph. |
| Obsidian, Logseq | Local files, backlinks, graph, plugins. Closest in "files stay in place." |
| Ulysses, iA Writer | Focused writing and direct publishing; iA's syntax highlighting is a small reading. |
| Zettlr | Academic writing: citations and outline. |
| ProWritingAid, Grammarly | Pure reading layers with no authoring. |
| WorldWideWeb, Amaya, Netscape Composer | The browser-editor lineage behind "authoring browser." |

## Sidequests (nothing scheduled)

1. **Multi-cursor** in the Genet text-editing primitive, with IME and accessibility
   kept intact.
2. **Find and replace**: a document command first, then project-wide replace as a
   batch of source-edit proposals with one preview.
3. **Invoked-macro proof**: one Rhai pack command that returns a source-edit proposal
   through the adapter Knot's link workflow already uses.
4. **Project record**: choose nested graph versus manifest; settle the Finder
   representation and whether site folders become one kind of project.
5. **Problems pane** over typed readings across open documents.
6. **Rename a target** with link and relation updates as one undoable batch.
