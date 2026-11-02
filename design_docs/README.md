# Knot Editor design documents

This is the index that [DOC_POLICY](DOC_POLICY.md) §6 names (its Knot Editor addendum says so). Documents archived on 2026-11-02 are listed once, under the two Archived sections at the end, each with its landing commits or cut note in its own header. Run receipts are outside the repository, in `Code/testing/knot-editor/`, and are cited as paths.

- [Writer ergonomics brief (October 29)](2026-10-29_knot_writer_ergonomics_brief.md): digests Mark's IDE comparison. Knot stays an authoring browser, not an IDE. Records the correction that the desktop already edits several documents as Workbench tiles, the IDE ergonomics worth borrowing (multi-cursor, find and replace, quick open, rename, peek, problems pane), scripts that edit source through pack grants and `mere-source-edit` proposals, and a project as a signed graph record. Nothing scheduled.

- [Writing workspace and mesh publication direction](2026-10-29_knot_writing_workspace_and_mesh.md): corrects the desktop multi-document baseline, records separate writer-invoked macro authority and graph-project direction, and proposes full published-site replicas with resident serving, authenticated discovery, measured availability and an independent-device failover gate. This is a design record, with implementation and qualification still separate.

The dated plans record Knot's development while it lived in Mere and its move
to an independently consumable repository.

Document-facing wording names the actual format (Djot, Markdown, Gemtext, and
so on), or says document when no format has been selected. Knot identifies the
application/provider; it is not a generic name for a user's document. Existing
route, schema, action and compatibility-format identities stay stable.

Open work lives in these plans, each cut on 2026-11-02 from a longer plan that is now archived whole (see Archived, cut, below):

- [Publishing protocol: open work](2026-11-02_publishing_protocol_plan.md): Phase B (the stable contract) and its entry question, the Turnstone named-recipient gate, and three residuals carried from the archived Graphshell plan (relay routing, the K2 reverse-direction reachability defect, writer admission).
- [Shared surface and port contribution: open work](2026-11-02_shared_surface_plan.md): Hest H3 and H4 native acceptance, and the optional headed Turnstone receipt of selected clips from the Livery and scripted producers. The contract is frozen at v1.
- [Application workspace: open work](2026-11-02_application_workspace_plan.md): A3 (references and lenses), A4 (changes, drafts, sharing), G1 to G3 (connected writing), headed IME and large-document acceptance for A1/A2, and the broker-routed capture intent.
- [Native small-web authoring: open work](2026-11-02_small_web_authoring_plan.md): Micron fidelity, multi-segment responses, Go multipart interoperability, independent preview scrolling, the persistent-serving gates, and the limits carried from the Scroll site and NomadNet records.
- [Composition in Knot's mere: open work](2026-11-02_composition_mere_retention_plan.md): The ownership decision, personal-custody acceptance, the persona chooser, the codex handoff's tasks 2 and 3 (the pending-merge capture and the full-suite re-run), the overmap qualification gaps and the dynamics-adoption gates.
- [Workspace after slice 1: open work](2026-11-02_workspace_slice1_plan.md): User-created splits and the side-by-side command, pointer dragging, automatic session restoration, and the follow-ups carried from the native review, document-links and context-navigation records.
- [Predicates, inference and scripted readings: open work](2026-11-02_predicates_inference_scripting_plan.md): E2 related passages with hybrid search, E3 suggestions, E4 inference (gated), the rest of the rhai reading lane, and E1's headed receipt.

Live documents:

- [Design pass](2026-09-23_knot_design_pass.md): the rulings on the brief's seven layout questions (title-bar menus over one command list, a status bar of authority chips, readings and the navigator as Workbench tiles, readings typed by kind, site tiles, the type and measure rules, measure-keeping collapse), the three arrangement frames, the stylesheet proposal, confirmed layout defects, and the ruled slice order. Slices 1 to 4 have landed (archived below); slices 5 and 6 are open, with the other items the archived slices left (section "Carried from archived slice records").
- [Repository extraction](2026-09-01_knot_editor_repository_extraction_plan.md): the independent-repository ruling, gates E0 to E2 (complete), stop rules, and the open question of one shared consumer revision versus each consumer's own pin.
- [Mark read adapter](2026-08-08_knot_mark_read_adapter.md): the Phase B choice recorded: a deliberately separate read-only projection for the Mark Protocol v1.0 working draft beside the private native publish lane; implemented as a bounded adapter, pending an external Demarkus-client receipt.

- [Composition readings](2026-09-30_knot_composition_readings.md): opt-in lexical source registry, Mora sound layers, durable collected material, offline WordNet conversion and the backend-neutral reading seam; implementation and acceptance limits are explicit.
- [Native metre editing and shared span intents](2026-09-30_knot_composition_readings.md#authored-metre-alignment-2026-10-17) — Qualified on macOS: focused Sound editor, span preview/cancel, keyboard and undo/redo, immutable typed sealed copies and exact fresh reopening. 72 focused cases, two native processes and six reviewed captures on Mere `9a29d570f3b`. Receipt (`Code/testing/knot-editor/receipts/2026-10-17_metre_editing/README.md`).

Site-model source correction (2026-10-22): `knot-site` keeps the renderer-free
portable model and now shares its canonical Mere `branch = "main"` source with
SDK consumers. The earlier fixed model revision split public snapshot types
when Cleromancy received the published SDK. Mere `4029ff099bd` → `dfeaa26d3dd` changes
only the model README, with no Rust or manifest change. Root and standalone
locks qualify the SDK cohort; the standalone lock remains an ignored local
qualification artifact. The failing real Git consumer control, correction,
software checks and publication are recorded once in
`mere/support/family-promotion/pool-cohort-rollout-20261022.json`.
Checkpoint, native and process acceptance retain their existing scopes.

The same receiving closure also aligns the optional Retinue entry in Knot's
root lock with qualified Murm `b12b7fc3734`. Retinue's package and inherited
manifest are byte-identical to the older source, and ordinary defaults do not
select it. This root-lock correction adds no behavior change; consumers retain
their qualified Knot `3fa92cfd1e8` code input.

## Archived 2026-11-02 (cut; history only)

Plans whose history outgrew their open work. Each is archived whole with a dated header, its inline run evidence and an evidence index in `archive_docs/receipts/`; the open work is in the plan named above.


- [Publishing secure-channel admission (October 29)](archive_docs/2026-11-02/2026-08-07_knot_publishing_protocol_plan.md): Murm N1/AT1 adoption keeps carrier facts separate from Noise identity, binds the v2 admission proof to the actual secure session and shared link, preserves authenticated-carrier mismatch refusal, and checks the ticket publisher before disclosure. Application payload/ALPN remain v1; admission v1 peers fail closed and must upgrade together. Earlier physical receipts retain their original scope.

- [Governed native Scroll process receipt](archive_docs/2026-11-02/2026-09-11_small_web_authoring_plan.md#governed-native-scroll-process-receipt-2026-10-22): independent saved-site publisher/resident qualification passed with native metadata, current authority and joined shutdown; the original Gemini fixture remains unchanged and passes.

- [Native Scroll wire ownership](archive_docs/2026-11-02/2026-09-11_small_web_authoring_plan.md#native-scroll-wire-ownership-2026-10-21): shared native encoder extraction with fixed donor bytes and actual local TLS receiving checks; Knot keeps content and host policy.

- [Saved-site publication into a Moot](archive_docs/2026-11-02/2026-09-11_small_web_authoring_plan.md#saved-site-publication-into-a-moot-2026-10-21): explicit CLI publication and signed local handoff; native publisher/resident process and Gemini/TLS qualification passed, with desktop controls still separate.

- [Optional saved-site checkpoints](archive_docs/2026-11-02/2026-09-11_small_web_authoring_plan.md#optional-saved-site-checkpoints-2026-10-21): opt-in host storage, explicit saved-file checkpoints and unchanged shared V1 snapshots; automated site, file-only desktop and CLI gates passed.

Current authority:

The composition plan also records the October 7 compact camera refinement:
wrapping Fit/Zoom controls, session-only Pan disclosure and a textual strata key.
Its current-family native qualification is separate from earlier scene receipts.
The next bounded slice adds retained background-category lenses over copied
relationships, with foreground protection and session-only category disclosure;
its qualification is recorded in the same composition plan.
The October 8 shared editorial slice adopts the declared arrangement controls
and bounded session undo on a coherent newer dependency family. G2 channels and
the G4a portable specification have now landed. Knot's subsequent shared
live-dynamics binding and its macOS/Windows consumer receipts are recorded
below. The initial `5db16993c90` qualification and its
layout-cost hold are preserved. The authorized Genet boundary-lookup repair is
now adopted through published Mere `66a5dabe16b` / Genet `15713014` on committed
Knot runtime `94e0ec4d146`: 616 workspace checks with nothing filtered, standalone
document default/engine gates, core/WGPU checks and 20 newly reviewed macOS
save/reopen captures pass. The unchanged large Micron repeat is 9.82 seconds
versus 384.58 in the preserved original-family diagnostic; this is not a release
benchmark. The publication hold is closed. The same composition plan retains
three reproduced upstream baseline failures and existing compact/scene-label
visual residuals; it does not infer live dynamics or release acceptance.

- Shared accessibility family repin, October 7 (`Code/testing/knot-editor/receipts/2026-10-07_foreign_accessibility_repin/README.md`): every Mere source moves coherently to `38a50a234b9`; Genet remains `2d30a0d39c7`. The admitted published relationship-scene source is preserved. Locked compile, standalone default/engine and the composed 590-test workspace set are qualified; the original timing failure and test-only barrier repair remain explicit. Human assistive-technology and native foreign semantics stay separate. Mere's optional Knot identity-seed pin remains under ruling 58.

- [Composition retention in Knot's mere](archive_docs/2026-11-02/2026-09-30_composition_mere_retention.md): supersedes the plaintext collection prototype; explicit owner attachment, signed sealed retention, and non-destructive legacy import. Adds separate authored organization, reversible archive and search; organization native qualification is recorded separately below. Shared relationship recipes adopt Mere's horizontal-reveal fix with eight reviewed compact/high-zoom captures. October 7 adds authored relationship-category controls and an opt-in shared static scene over copied disclosure, with explicit foreground/background roles, bounded camera controls, and encrypted presentation retention. Wide/narrow native save and fresh-process reopen qualification are recorded in the same plan; portable dynamics and inferred ambient neighborhoods remain subsequent work. Personal-wallet native retention remains a separate gate. The October 7 coordinated-family receipt (`Code/testing/knot-editor/receipts/2026-10-07_current_family_repin/README.md`) qualifies Mere `a2e0982899f` / Genet `2d30a0d39c7` on Windows with 579 workspace tests plus standalone default/engine tests; the retained CRLF failure and narrow LF-asset correction are explicit. Downstream publication, final Linux and native accessibility gates remain separate.

- [Authored occurrence placement continuation (October 11)](archive_docs/2026-11-02/2026-09-30_composition_mere_retention.md#authored-occurrence-placement-continuation-2026-10-11): shared SceneTrace positions, host-owned drag/keyboard/history, encrypted retention, and compact wrapped occurrence controls on the frozen Mere `9756feb38c5` family. Consumer receipt (`Code/testing/knot-editor/receipts/2026-10-11_authored_scene/README.md`): 647 workspace tests and 28 reviewed macOS captures including wide/compact fresh-process reopen. The later live-dynamics qualification is separate below; inferred ambient neighborhoods, personal-custody native acceptance and unattended background presentation remain open.
- [Shared live dynamics and Windows consumer review (October 11)](archive_docs/2026-11-02/2026-09-30_composition_mere_retention.md#shared-live-dynamics-host-continuation-2026-10-11): production `ProjectionDynamics::new_relationship` binding, separate occurrence roles, shared schedule and session controls, with explicit authored/runtime retention boundaries. macOS receipt (`Code/testing/knot-editor/receipts/2026-10-11_live_dynamics/README.md`): 662 workspace tests and 15 reviewed captures. Windows review (`Code/testing/knot-editor/receipts/2026-10-11_relationship_consumer_review/README.md`): real disclosed-edge negative control and 15 reviewed captures including wide/compact fresh-process reopen. Firefox, independent physics review and coordinator-owned adoption remain separate; the shared baseline golden failure and platform visual residuals are preserved.
- Explicit placement from motion (October 11) (`Code/testing/knot-editor/receipts/2026-10-11_keep_motion_placement/README.md`): Mark's preview/explicit-keep ruling, one-edit undo/redo, deliberate retention and fresh-process reopen, with exact pose identity and atomic refusal. Session playback never implicitly authors placement or changes source material.
- Dynamics preview layout (October 11) (`Code/testing/knot-editor/receipts/2026-10-11_dynamics_viewport/README.md`): the camera and compact motion toolbar are revealed together through the existing host API; geometry, scroll-away pause and native Keep/undo/redo qualify the bounded layout change. Firefox verification is parked while browser control remains blocked.
- Dynamics completion and refusal (October 11) (`Code/testing/knot-editor/receipts/2026-10-11_dynamics_status/README.md`): truthful controller-end feedback, guarded Step, deliberate replay after completion/refusal, 84 focused tests and 18 reviewed native captures. Private Firefox remains blocked; prior full-workspace and adoption gates stay separate.
- [Predicates, inference readings, scripted readings](archive_docs/2026-11-02/2026-09-15_knot_predicates_inference_scripting_plan.md): predicate identity mapping before the first authored relation persists; esp graduation from the lexical provider to embeddings, related-passage readings and suggestions; a rhai reading lane over typed snapshots. All readings, no new authority.

- [Workspace slice execution](archive_docs/2026-11-02/2026-09-23_knot_workspace_slice1_plan.md): multi-document and native design receipts; September 30 combined Commands, typography and diagnostics qualification uses Mere `d6e76012091` and Genet `450f45b694c`, with 260 desktop and 63 document tests, nine reviewed native images and restored stale-result control. Worker acceptance and file authority stay in Knot; original and combined cohorts are recorded separately.

- [Forme workspace adoption](archive_docs/2026-11-02/2026-09-23_knot_workspace_slice1_plan.md#forme-second-host-adoption-2026-10-29): Platen-backed Unlock, layout preview, Discard and acknowledged local Apply; exact tile-binding checks, independent receiving tests and bounded native save/reopen qualification. Pointer dragging and automatic document-session restoration remain separate.

- [Native small-web authoring](archive_docs/2026-11-02/2026-09-11_small_web_authoring_plan.md): Gemini, Titan, Spartan and Micron scope, shared protocol ownership, Turnstone composition and persistent-serving proposal.

- [Application workspace](archive_docs/2026-11-02/2026-09-05_knot_application_workspace_plan.md): the original document lifecycle, live outline, heading-linked Djot/Knot preview, read-only folded source, desktop file catalog and saved-revision review, coordinated resident capture retention, host-injected worker Retain controls, relation model, and the public signed-head revision boundary for projection provenance. The [workspace slice](archive_docs/2026-11-02/2026-09-23_knot_workspace_slice1_plan.md) subsequently delivered the desktop multi-document workspace, tabs and navigator. User-created splits remain follow-up work. Editable folds remain a separate Cambium coordinate-mapping project.

- [Publishing protocol](archive_docs/2026-11-02/2026-08-07_knot_publishing_protocol_plan.md): recipient-bound reading and withdrawal; the October 19 research handoff assessment records the missing source-owner join and durable publication decision before Turnstone cold-reopen acceptance.

- [Shared document surface and port contribution](archive_docs/2026-11-02/2026-08-24_knot_shared_surface_and_port_contribution_plan.md) Embedded Cambium 0.4/0.5 compatibility, canonical H3 window invocations and H4 command-producing Rhai fences are qualified. H4 exercises actual consented endpoint authority and derived request data while preserving authored source and unrelated supplier pins; native acceptance remains separate.

The [predicate and relation plan](archive_docs/2026-11-02/2026-09-15_knot_predicates_inference_scripting_plan.md)
records the link-model P2 projection continuation: optional signed retraction
time with legacy checkpoint-byte preservation. Supplier checks and Mere intake
qualification remain distinct from desktop authoring and automatic sync.

- [Saved-source research note intake](archive_docs/2026-11-02/2026-09-05_knot_application_workspace_plan.md#saved-source-research-note-intake--2026-10-18): owner implementation qualified, Turnstone headed acceptance tracked in the consumer receipt; exact scalar citation validation, distinct provenance over shared bytes and owner-verified offline reads through the existing clip authority.

## Archived 2026-11-02

Plans and slice records that landed. Each keeps its text and gains a dated header; evidence indexes and old-to-main hash tables are in `archive_docs/receipts/`.

- [Port founding plan](archive_docs/2026-11-02/2026-07-25_knot_port_plan.md): K0 to K7, Knot as a Mere port; the founding record, canonical here over Mere's archived copy.
- [Authoring consumer](archive_docs/2026-11-02/2026-07-27_knot_authoring_consumer_plan.md): the local Cambium editor over a disclosed text resource, with typed save intents, clipping and the sealed resolve cache.
- [Graphshell integration](archive_docs/2026-11-02/2026-08-02_knot_in_graphshell_plan.md): Knot in Graphshell as a process boundary (K0 to K3), including the K2 physical receipt; its Knot-search steps S0 and S1 are open and carried into the predicates successor.
- [Initial Djot editor work](archive_docs/2026-08-07/2026-06-24_djot_editor_knot_nodes_plan.md): the Meerkat-era Djot editor, knot nodes and web clips record (archived 2026-08-07 in Mere's history; moved here from `design_docs/archive/` on 2026-11-02).
- [Design pass brief](archive_docs/2026-11-02/2026-09-13_knot_design_pass_brief.md): materials, captured baseline frames, planned directions, and the layout questions a design pass over the standalone desktop has to answer before the multi-document slice adds tabs and a navigator. Answered by the design pass above.
- [Scroll site authoring](archive_docs/2026-11-02/2026-09-11_scroll_site_authoring_plan.md): native site files, preview, metadata and explicit loopback publication.
- [Commands slice](archive_docs/2026-11-02/2026-09-29_knot_commands_slice.md): the unified command catalog, configurable native/client chrome, menu and palette routes, and qualified platform-specific evidence.
- [Collapse slice](archive_docs/2026-11-02/2026-09-29_knot_collapse_slice.md): non-destructive workspace presentation: Workbench rails and drawers, minimum widths, the give-way policy and chip collapse; accepted after automated and native qualification with a shared text paint repair.
- [Document links slice](archive_docs/2026-11-02/2026-09-29_knot_document_links_slice.md): source-authored links, bounded backlinks and product-owned source navigation.
- [Shared source-edit link adapter, October 18](archive_docs/2026-11-02/2026-09-29_knot_document_links_slice.md#shared-source-edit-proposal-adapter--2026-10-18): one retained buffer, revision-bound selection proposals, one-step undo, explicit guarded saving; document/model and two-file desktop harness qualification with fresh-process reopening. The follow-on replaces three captured source fields with one shared immutable snapshot and centralizes source checks; 53 default/MSRV, 66 engine, 11 model and four desktop harness cases pass, with two independent reopen helpers. Receiving qualification now uses published [Mere `1d18424745f`](https://github.com/merely-made/mere/commit/1d18424745fc978669dab0e55d016eebe1dc7a91) through the canonical Git URL and an authorized exact-revision exception for this contract only; no local overrides or existing supplier repins. This consumer commit records the qualified Git-backed adapter; native GUI review remains separate.
- [Local recovery](archive_docs/2026-11-02/2026-09-29_knot_local_recovery_plan.md): private local crash recovery of unsaved text and exact selection; it never writes the original file; automated and isolated native crash/restore acceptance.
- [Shared presentation review](archive_docs/2026-11-02/2026-09-29_knot_shared_presentation_review.md): shared Graph sizing and scenario focus pins, with the diagnostics adoption; automated and native checks.
- [Typography slice](archive_docs/2026-11-02/2026-09-29_knot_typography_slice.md): bundled writer/reader fonts and font-backed measure; its existing platform receipt remains distinct from the combined diagnostics qualification.
- [Workspace native review](archive_docs/2026-11-02/2026-09-29_knot_workspace_native_review.md): native review of workspace slice 1: five scenarios, six reviewed frames, the Graph paint-clip fix and the presentation follow-ups it recorded.
- [NomadNet backend independence](archive_docs/2026-11-02/2026-09-29_nomadnet_backend_independence.md): backend-independent static Micron publishing: exported snapshots for any NomadNet host, Retinue optional, governed host and browser receiving.
- [Reticulum-Go backend](archive_docs/2026-11-02/2026-09-29_reticulum_go_backend.md): an external-CLI launcher serving an exported Micron snapshot through Reticulum-Go; verified, with the unknown-page check, MeshChatX and constrained-radio acceptance unrun.
- [Shared font-role adoption](archive_docs/2026-11-02/2026-10-12_knot_font_roles.md): optional application family requests, explicit writer precedence, and realized card geometry/dynamics safeguards; bounded macOS foreground-native qualification; default ten-second and unattended startup and other platforms stay outside.
- [Resident mere as an overmap layer (October 15)](archive_docs/2026-11-02/2026-10-15_knot_mere_overmap_layer.md): Mark's "catalog, route as overmap" ruling; the catalog stays the graph, Djinn's `mere/<domain>` route adds sessions and archives through the application door as `knot-editor`, and archive requests forward to Djinn. Repins every Mere row to `f194c9e8605` (Mere P1). Knot checks and the Djinn archive round trip pass; native foreground qualification passed on macOS the same day.
- [Rendering the mere overmap as a layer (October 15)](archive_docs/2026-11-02/2026-10-15_knot_overmap_layer_rendering.md): sessions as container nodes in a band above the catalog graph, each archive on the session it was saved from, an Archive shelf for the rest and New session last; mere-view grows `SessionPlacement::Overmap` and `ArchiveEntry::session`, Djinn names each codicil's session. Repins Mere to `2955011fa09`; macOS before/after captures pass. Live receipt (`Code/testing/knot-editor/receipts/2026-10-15_mere_overmap_live/README.md`). **Addendum (same day):** the in-graph tier, with session cards that grow the tier, codicil popovers, "+N", the Archive shelf, node-kind colours, and fork and pick-off naming.
- [Catalog fold curation (October 16)](archive_docs/2026-11-02/2026-10-15_knot_mere_overmap_layer.md#catalog-fold-curation-2026-10-16): production catalog selection folds, source-preserving summary projection, scoped local preferences and bounded session undo/redo. Native and fresh-process receipt (`Code/testing/knot-editor/receipts/2026-10-16_catalog_folds/README.md`) records eight reviewed captures, exact restored fold identity, unchanged member placements and source bytes on the current Mere supplier.
- [Shared resident materialization](archive_docs/2026-11-02/2026-10-15_knot_mere_overmap_layer.md#shared-resident-materialization-2026-10-18-qualified): Knot and Cleromancy adopt one shared reader; Knot preserves catalog ownership and invalidates cached actions after failed refresh. Shared reading and bounded native archive/fresh-client reopening are qualified; evidence (`Code/testing/knot-editor/receipts/2026-10-18_shared_resident_materialization/QUALIFICATION.md`) preserves failed histories and exact source identities.
- [Contextual navigation prototype](archive_docs/2026-11-02/2026-10-23_knot_context_navigation.md): shared Cambium Back/Forward and scope controls, nested live-outline ancestry, startup-only rebinding of durable document/site routes, semantic source/preview scroll bookmarks, and current authority limits.
- [Custody and projection receiver](archive_docs/2026-11-02/2026-10-29_custody_projection_receiver.md): the separated custody-client and projection-door receiver; published, focused overlay, peer-enabled host compilation and secure publishing tests passed.
- [Codex handoff (October 15)](archive_docs/2026-11-02/2026-10-15_knot_codex_handoff.md): where Knot stands after the Graphshell stack lift, the overmap tier and merge-in, the rulings that bind it, and three ordered tasks: the Mere/Knot/Turnstone repin round per the remote mere route spec §7 (Murm `main` `220e9702b4d`; Knot and Turnstone drop `graphshell`), a native capture of the pending-merge card, and a full suite re-run. Closed out 2026-11-02: task 1 was done 2026-10-15, and tasks 2 and 3 are carried into the plan cut from the composition retention plan.
