# Composition readings: receipts

Receipts for the [composition readings plan](../../2026-09-30_knot_composition_readings.md) (live; DOC_POLICY §8 puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the current file.

## 2026-11-02, where the evidence lives

Cited under `Code/testing/knot-editor/` (outside this repository, so path cites, not links), checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `composition-20260930/receipts`: not found at this path
- `receipts/2026-10-16_opened_link/checks.json`: present
- `receipts/2026-10-17_metre_editing/README.md`: present
- `receipts/2026-10-22_checkpoint_confidentiality/qualification.json`: present

Folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked here as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02): 834 files, 113,533,394 bytes, each copy written from its committed blob and SHA-256 checked before removal. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`parents[3]`, `parents[2]` or `$KNOT_SRC`) and no longer find it from their new home.

Machine-local paths named (not retained here): `/Users/markik/Code/testing/knot-editor/composition-20260930/receipts`.

## 2026-11-02, inline run evidence

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Composition readings: lexical connections, sound, and collected material | 1-11 | 0 | 0 | 1 |
| Personal relation reading (2026-10-16) | 12-58 | 0 | 3 | 2 |
| Checkpoint confidentiality repair (2026-10-22) | 59-109 | 0 | 3 | 0 |
| Scripting and further extension | 219-238 | 0 | 1 | 0 |
| Verification | 239-291 | 0 | 6 | 2 |
| Authored metre alignment (2026-10-17) | 292-322 | 0 | 2 | 1 |

## 2026-11-02, old hashes to main hashes

This document is live, so it cites main hashes (Mark, 2026-11-02: "Same: live docs only"). On 2026-11-02 each pre-merge hash below was replaced by the commit on the repository's local `main` with the same author time and subject.

| Hash before | Repository | Main hash now cited | Subject |
| --- | --- | --- | --- |
| `4c4ea11d8` | mere | `9a29d570f3b` | Add revision-qualified span edits for metre and progressions |
| `ee324e517` | mere | `2ecc0f27840` | Qualify retained source assertions and shared opened-link view |
| `0020a50e6` | knot-editor | `93b16da3574` | Preserve signed retraction times in Knot projections |
| `e7b5c50` | knot-editor | `485afc17e6e` | Keep compact timeline labels clear of assertion geometry |
| `57f46d1` | knot-editor | `f17c3e808f8` | Read retained signed assertions in the opened-link swatch |
| `fc993748d9b56e99eb3a79bf5a07eb5ff371f885` | knot-editor | `16a4b482defc00bd79862303b00d281749c4bef8` | Protect Knot checkpoints and communal keys; repair legacy databases |
| `9310518be5423862c1038f7d7a7578d5089d3c51` | mere | `c7199781094ec8cb4397e743f93573069e0a8aad` | Add opt-in validated local lexical reference packs |
| `b1eb3af197111ecde34421bbe1bad2e6024ca564` | genet | `f499efe4698ba4b1c02c8bccd7c1f3a78b4638a8` | Place host text paint inside Livery field contexts |
| `28a0741` | woodshed | `e7731303dd9` | Adopt shared revision-qualified arrangement edit intents |

## 2026-11-04, lexical source consolidation

Mora 0.1.0 moves from `mora` revision `42c232c3ac60074795b742ffabc40c3116c1cc9b` to `crates/mora`, with original repository ancestry retained. CMUdict and Reference Data move from Mere to `crates/mora-cmudict` and `crates/reference-data`. The three `source-import.json` files list original blobs and SHA-256 values. Source, corpus and notice bodies are the canonical Git bytes; manifests change only source ownership and workspace wiring. The CMUdict corpus SHA-256 is `e975bd6570354ac852c636e5837c9e3ec27a1b6ccfe992930610e8bc4f0797ae`.

Native receiving tests: Mora 37, CMUdict 2, Reference Data 23 (one pre-existing ignored test), Knot sound readings 5 and editor rosette 3 pass. The locked offline whole-workspace/all-target check and lexical Clippy pass. Mora remains an independent, dependency-free workspace and passes the locked offline Rust 1.97.1 allocation-free library check. These gates cover library behavior and Knot's in-process consumers, not headed editor operation. Private bounded command summaries are in `testing/subsystem-extractions-20261104` under the shared Code root; aggregate cap 100 MB.
