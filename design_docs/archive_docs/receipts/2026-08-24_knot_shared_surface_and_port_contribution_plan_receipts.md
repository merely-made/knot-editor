# Knot Shared Surface and Port Contribution Plan: receipts

Receipts for the archived [plan](../2026-11-02/2026-08-24_knot_shared_surface_and_port_contribution_plan.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `receipts/hest-h3-20261020/qualification.json`: present
- `receipts/hest-h4-20261020/qualification.json`: present

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| 2026-08-25: the document surface now has its own dependency boundary | 193-210 | 0 | 3 | 0 |
| G0. Neutral descriptor and retained runtime erasure | 384-403 | 0 | 1 | 0 |
| 7. Progress | 563-711 | 0 | 12 | 2 |
| Hest invocation receiving (2026-10-20) | 745-779 | 0 | 4 | 0 |
| Hest H4 command-producing fences (2026-10-20) | 780-802 | 0 | 2 | 0 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `6` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `001448d55` | genet | `7da40983cbd` | Freeze the retained surface contract at v1 | 26 |
| `bbd0906` | genet | `960dc37b29d` | Add retained product surface contracts | 215 |
| `709ae402` | mere | `3e25116a013` | Add explicit Knot read-only posture | 482 |
| `4e60f93` | genet | `3ac44e2f1f5` | Harden retained accessibility visibility and state | 487 |
| `b8de627` | genet | `5fb45bbaf1d` | Project retained document accessibility semantics | 487 |
| `bbd09062804` | genet | `960dc37b29d` | Add retained product surface contracts | 591 |
| `1ac3727` | genet | `aa3421b26a5` | Repin WPT expectations under Livery | 607 |
| `fb82d12f` | mere | `e1c02c12223` | Extract reusable Knot document package | 618 |
| `80c2f17c` | mere | `e09ab082c10` | Merge branch 'main' into HEAD | 619 |
| `9ef5e3026` | mere | `c9033723b64` | Add Hest invocation and terminal outcomes with host-owned adapters | 762 |
| `6ef3cd37` | genet | `7c8539c1b95` | Merge remote-tracking branch 'origin/main' into codex/accesskit-025-20261020 | 766 |
| `e2f19a3f` | murm | `5b78c0e6768` | Align the Eidetic embedding-contract supplier cohort | 766 |
| `ec15eb26` | netrender | `d9d6e778df7` | Borrow scene sources across Classic tile and retained lowering | 766 |
| `e94cb23e88c8989355eb128b893f6cdbe559e074` | mere | `071e6947ffd` | Distinguish H4 adapters from incoming shell qualification scope | 782 |
