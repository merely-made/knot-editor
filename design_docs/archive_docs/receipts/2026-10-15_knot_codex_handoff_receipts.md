# Knot handoff for Codex, 2026-10-15: receipts

Receipts for the archived [plan](../2026-11-02/2026-10-15_knot_codex_handoff.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `receipts/2026-10-15_mere_overmap_live/`: present

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Knot handoff for Codex, 2026-10-15 | 1-22 | 0 | 0 | 1 |
| 1. Historical snapshot as of 2026-10-15 | 23-62 | 0 | 2 | 0 |
| Task 2: capture the pending-merge card natively | 111-151 | 0 | 0 | 4 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `2` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `0020a50e6` | knot-editor | `93b16da3574` | Preserve signed retraction times in Knot projections | 12 |
| `ee324e517` | mere | `2ecc0f27840` | Qualify retained source assertions and shared opened-link view | 13 |
| `e7b5c50` | knot-editor | `485afc17e6e` | Keep compact timeline labels clear of assertion geometry | 14 |
| `90ae0c2` | knot-editor | `eb08d9f5939` | Record the overmap tier's native re-capture on b19eebf | 25 |
| `b19eebf` | knot-editor | `c14860e2cb7` | Repin Mere to 0c03dff3: carry merges to the route, drop the broker test | 25 |
| `0c03dff3d` | mere | `51511d9f73b` | Merge remote-tracking branch 'origin/main' | 29 |
| `3d9e799f` | murm | `d56bc862c4a` | Detach browser RTC handlers before dropping their Rust closures | 30 |
| `ae3d9f5d` | genet | `27e9afe5b5f` | Let wasm viewer artifacts build genet-render-host without classic Vello | 31 |
| `f4f9c0d8` | netrender | `c6e19867d5e` | Refuse the Classic opt-out outside wasm32 artifacts | 33 |
| `fb6360c12b` | murm | `220e9702b4d` | Merge the C1 WebRTC callback fix and the in-process transport kind into main | 89 |
| `75a2e6ae7` | mere | `04f14e5f894` | Name Murm at fb6360c12b and give script-engine-api a version requirement | 90 |
| `b2c8dcc28` | mere | `b5f070fd11d` | Merge branch 'main' of github.com:merely-made/mere | 90 |
| `fe9d98c` | knot-editor | `7ee24f9b571` | Repin Mere to 75a2e6ae and Murm to fb6360c12b: the one repin round (stack lift P | 90 |
| `6439fa88` | turnstone | `20b53e00bca` | Repin to Mere 75a2e6ae, Knot fe9d98c and Murm fb6360c12b (one repin round) | 91 |
| `7af8de32` | genet | `fb14b4c8fc0` | Name netrender and murm-wire by branch = main | 92 |
| `4a74b1cf` | mere | `50d7da92a28` | Family source convention: first-party rows name branch = main, one canonical URL | 93 |
| `a3621dc8` | murm | `696c351a5ff` | Name Mere by branch = main in the native workspace | 94 |
| `0234e7ee` | dramatis | `25ead7cfb8b` | Include release licenses and verify refused CXF metadata | 106 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `276b4664` (line 32).
