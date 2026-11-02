# Rendering the mere overmap as a layer: receipts

Receipts for the archived [plan](../2026-11-02/2026-10-15_knot_overmap_layer_rendering.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `receipts/2026-10-15_mere_overmap_live/README.md`: present

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Checks (2026-10-15) | 95-120 | 0 | 3 | 1 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `2` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `a27911a` | knot-editor | `0c87e7c41b1` | Repin Mere to the application door and show its mere as an overmap | 14 |
| `392e0ecc0` | mere | `2955011fa09` | Keep an overmap archive chip's controls on one line | 97 |
| `19573b51` | mere | `f194c9e8605` | Record P1's checks in the stack lift plan | 98 |
| `ec5279784` | mere | `0c848139287` | Let mere-view show sessions as an overmap tier on the graph's canvas | 128 |
| `89aa14a36` | mere | `4a130d2b93d` | Grow the tier for its open card, and give tier nodes kinds and a shelf | 129 |
| `a0ffffea2` | mere | `0f6938f9308` | Name a fork of a thaw without a doubled from, and a picked-off part | 153 |
