# Djot Editor + Knot Nodes + Web Clips Plan: receipts

Receipts for the archived [plan](../2026-08-07/2026-06-24_djot_editor_knot_nodes_plan.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

The plan cites no receipt path. Its run evidence is inline, indexed below.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| The idea | 77-89 | 0 | 0 | 1 |
| Re-scoped slices | 139-161 | 0 | 0 | 1 |
| What already exists (code-verified) | 162-248 | 0 | 1 | 1 |
| What is net-new | 249-283 | 0 | 0 | 2 |
| Phasing (done-conditions) | 588-662 | 0 | 0 | 1 |
| Cross-references | 797-826 | 0 | 1 | 1 |
| Progress | 882-1390 | 0 | 3 | 3 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `1` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `0ab66a7` | mere | `7ff57e3c08f` | meerkat: note_view — EngineDocument -> serval-view mapper (djot reframe slice 1) | 141 |
| `3d7c7ea` | mere | `db1081c6ae7` | meerkat: note_surface — EngineDocument -> serval Scene (djot reframe slice 1) | 141 |
| `4bf7a17` | mere | `5db347ca005` | kernel: inline Node body (knot note source) + snapshot round-trip (djot reframe  | 147 |
| `fa5f32a` | mere | `9ea2d0b5f3b` | meerkat: build context menu + submenu via overlay_at; capability-adoption plans | 1090 |
| `1b29cda` | mere | `fb69cb48bdd` | DocumentBlock::Table: variant + exhaustive matches + exporters + serval-view map | 1167 |
| `0a8bc35` | mere | `da3912b7d6d` | meerkat: repoint knot editor undo/redo + auto-pairs at serval's EditHistory | 1364 |
| `6d1ab51` | mere | `1c6cec13721` | meerkat: [[ node-link completion (Phase 3, on the shared completion machinery) | 1384 |
| `bab1aa9` | mere | `2c971151d0e` | meerkat: knot editor `/` slash menu (Phase 3 completion machinery) | 1384 |

Ambiguous (more than one main commit has the same subject and author time):

- `6a3ceace` (genet, line 1129, "xilem-serval: styled_textarea, per-range styled span runs fo"): `e4fc28e0223` or `7c0b7953dfa`
- `8a4a0a0` (genet, line 1374, "xilem-serval: reusable overlay `menu` view"): `4a07fcaa00d` or `3dd5fa6eb5b`
- `bf632fa` (genet, line 1362, "xilem-serval: EditHistory + wrap_selection — undo/redo + aut"): `604d10ac4b8` or `94c1b0bc1c3`

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `03661ce` (line 1130).
