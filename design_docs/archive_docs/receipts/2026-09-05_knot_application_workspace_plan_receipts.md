# Knot Application Workspace Plan: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-05_knot_application_workspace_plan.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `validation/2026-10-10_tabard_application_acceptance.json`: present

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

Machine-local paths the plan names (other machines; not retained here): `/Users/markik/Code/output/turnstone-knot-notes-20261018/`.

Scratch paths the plan names, which are build or run output and not kept: `/tmp/knot-tabard-header-final-gates.log`, `/tmp/knot-tabard-header-native-build.log`, `C:\t\knot-a1`.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Application appearance adoption, 2026-10-09 | 9-73 | 2 | 6 | 1 |
| Scoped next sequence: multi-document workspace | 271-379 | 0 | 4 | 0 |
| A4. Changes, drafts, and sharing | 416-449 | 0 | 1 | 0 |
| G1. Links, passages, and inspectable relations | 507-765 | 0 | 2 | 22 |
| Findings and progress | 931-1459 | 0 | 53 | 17 |
| Saved-source research note intake — 2026-10-18 | 1472-1519 | 1 | 4 | 1 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `0` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `7019f07d36a9da8c1a3edbbcbabad606e9cb3278` | mere | `83017804a24` | Keep shared workshop header controls visible at narrow widths | 29 |
| `7422e90613f9017e5bb790e3acb48f61776b2eda` | genet | `bb82fb866db` | Make shared native frame readback nonblocking and bound synchronous waits | 30 |
| `ab191d4` | knot-editor | `04333d887a4` | Repin Mere to qualified graph semantics source | 31 |
| `11236fd4` | mere | `e57baaa7e71` | Qualify shared preview rendering with guarded native shaders | 57 |
| `7019f07d` | mere | `83017804a24` | Keep shared workshop header controls visible at narrow widths | 57 |
| `91c6238deee39738cf7c0b28cf6b35df52f51caf` | mere | `734ac968b50` | Match captured omission of unchecked Micron choice groups | 165 |
| `33287b0` | mere | `a06fd627f3b` | Require ordered keys for canvas search | 698 |
| `b4f14f2c` | mere | `bec65ece891` | Merge WS4 R0 to R5: smolweb renders through one path (rulings S66, S70 to S75) | 933 |
| `1777b198` | mere | `3884e62288c` | Add evidence-qualified Micron preview subset | 977 |
| `9e8f9dc2` | genet | `2aaa1fab2d4` | Harden Fleece quote conformance | 978 |
| `0a46ebd` | knot-editor | `dc68e34e5c7` | Add host-injected desktop revision retention | 998 |
| `e312458` | knot-editor | `7073cbceae8` | Advance Mere projection dependency | 1000 |
| `33287b0efbe0bd45bd46b87a598d5f4fc62414e2` | mere | `a06fd627f3b` | Require ordered keys for canvas search | 1007 |
| `9e8f9dc2f3ddc0af1658580bb51964462a03923f` | genet | `2aaa1fab2d4` | Harden Fleece quote conformance | 1008 |
| `b303527` | knot-editor | `83dfccfd9ac` | Coordinate Knot capture and replication mutations | 1028 |
| `c0df2f9` | knot-editor | `93e19ef9d95` | Add resident-owned file capture retention | 1075 |
| `2b1ce46e5a15328b4bf4d350ec4b0252d9b404a1` | mere | `ffeeca44dc1` | eidetic-search: CandidateIndex, a token-prefix lookup for lanes that narrow befo | 1167 |
| `2b1ce46e` | mere | `ffeeca44dc1` | eidetic-search: CandidateIndex, a token-prefix lookup for lanes that narrow befo | 1243 |
| `54fd2b9` | knot-editor | `962e16b9954` | Pin mere 2b1ce46e across the workspace (candidate index) | 1283 |
| `b9b0ee13` | mere | `d09c9e546b9` | Close Pelt's blocking web surface seam | 1297 |
| `35952af` | knot-editor | `c4a95a7a462` | rustfmt: reflow three files | 1298 |
| `74912aa` | knot-editor | `50aed7e49d9` | Add guarded single-document writing workflow and workspace plan | 1334 |
| `d82afa17` | mere | `d7dbf6b6e78` | Generate the Circuit workspace graph instead of committing it | 1345 |
| `a8d23e3` | knot-editor | `ded76b9d0cc` | Adopt the canonical rustfmt policy | 1431 |
| `4ec1cda` | knot-editor | `1056059a0bc` | Record shared resident reader native and reopen qualification | 1515 |
| `500bc78a7fa9e0b9abe8f35d189c465f93d3ccab` | mere | `63d97969fe8` | Use supported solid divider for resident overmap tier | 1516 |
| `7114f91ce928937ac0891d7db816655706843204` | genet | `54c16622941` | parley: load complex-script segmenter dictionaries at runtime | 1517 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `491c376cf2b01fc11132cf8f86419dec114ae032` (line 45).
