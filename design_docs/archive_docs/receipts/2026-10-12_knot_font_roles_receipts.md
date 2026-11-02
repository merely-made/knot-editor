# Knot shared font-role adoption: receipts

Receipts for the archived [plan](../2026-11-02/2026-10-12_knot_font_roles.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `receipts/2026-10-12_font_roles/README.md`: present

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Knot shared font-role adoption | 1-17 | 0 | 1 | 0 |
| Acceptance gates | 64-82 | 0 | 0 | 1 |
| October 12 gate | 83-124 | 0 | 7 | 3 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `2` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `4fae1d8` | knot-editor | `4e27be418d7` | Adopt shared font roles with writer and measured dynamics authority | 6 |
| `a2128e7` | knot-editor | `54d58a26b1d` | Bound marked fixture startup grace to the first presentation | 6 |
| `f5ecacf` | knot-editor | `8d5202f9c6f` | Exercise the compact font picker through its root menu | 7 |
| `fcb8f3b` | knot-editor | `1b072d7a3c7` | Qualify fixture startup policy and correct typography library version | 7 |
| `3af93adda472af2913a5d298f14deb10e85daee2` | knot-editor | `9646d61f9fe` | Adopt the qualified native current-frame completion suppliers | 11 |
| `8568f42fdf57f997274c74e63bd4ee045fe1a2b4` | mere | `e9763627f90` | Qualify F203-F205 runtime and preserve owner integration gates | 12 |
| `2c7ef73bfa44c07744e0643e3c6f98be3cbd7947` | genet | `00ec3f008b4` | Use the production duration type in the native quality fence | 13 |
| `c74b7cb28090c28b1c6d80b08dcdea18aff48d3e` | netrender | `518daa79c7f` | Return checked Classic render failures through filter paths | 14 |
| `0234e7eea6f2ddf2330a71b9425da01b06ae0391` | dramatis | `25ead7cfb8b` | Include release licenses and verify refused CXF metadata | 16 |
| `3af93ad` | knot-editor | `9646d61f9fe` | Adopt the qualified native current-frame completion suppliers | 96 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `2eae6243881d5b24c984d5ee35dde509ca5db782` (line 15).
