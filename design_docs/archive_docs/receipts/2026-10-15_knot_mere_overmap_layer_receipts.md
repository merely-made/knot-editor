# Knot's resident mere as an overmap layer: receipts

Receipts for the archived [plan](../2026-11-02/2026-10-15_knot_mere_overmap_layer.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `receipts/2026-10-15_mere_overmap_live/README.md`: present
- `receipts/2026-10-16_catalog_folds/README.md`: present
- `receipts/2026-10-18_shared_resident_materialization/QUALIFICATION.md`: present

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Knot's resident mere as an overmap layer | 1-14 | 0 | 3 | 0 |
| Checks (2026-10-15) | 48-68 | 0 | 3 | 1 |
| Catalog fold curation (2026-10-16) | 69-98 | 0 | 0 | 1 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `1` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `19573b513d3809290973a0342b530091e8ca7ba1` | mere | `f194c9e8605` | Record P1's checks in the stack lift plan | 11 |
| `3d9e799f` | murm | `d56bc862c4a` | Detach browser RTC handlers before dropping their Rust closures | 12 |
| `ae3d9f5d` | genet | `27e9afe5b5f` | Let wasm viewer artifacts build genet-render-host without classic Vello | 12 |
| `0234e7ee` | dramatis | `25ead7cfb8b` | Include release licenses and verify refused CXF metadata | 13 |
| `f4f9c0d8` | netrender | `c6e19867d5e` | Refuse the Classic opt-out outside wasm32 artifacts | 13 |
| `500bc78a7` | mere | `63d97969fe8` | Use supported solid divider for resident overmap tier | 103 |
| `d4ac777` | knot-editor | `753477cd194` | Expose fork node before inspecting its resident card | 103 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `276b4664` (line 13).
