# Local document recovery: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-29_knot_local_recovery_plan.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `recovery-native-20260929/KnotRecoveryTest.app`: not found at this path
- `recovery-visuals-20260929/knot-visual-followups-graph-footer/`: not found at this path

Machine-local paths the plan names (other machines; not retained here): `/Users/markik/Code/testing/knot-editor/recovery-native-20260929/KnotRecoveryTest.app`, `/Users/markik/Code/testing/knot-editor/recovery-visuals-20260929/knot-visual-followups-graph-footer/`.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Local document recovery | 1-8 | 0 | 1 | 0 |
| Milestone | 9-19 | 0 | 1 | 0 |
| Acceptance checks | 38-56 | 0 | 0 | 2 |
| Automated verification and native acceptance | 85-143 | 7 | 8 | 1 |
| Parallel presentation follow-ups | 144-176 | 2 | 3 | 3 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `2` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `ff46cbe` | knot-editor | `7aad64e7e86` | test: close native workspace receipt review | 87 |
| `2f9a9fe` | knot-editor | `908ec4fae2f` | Fix Knot graph controls and site footer spacing | 152 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `2d4c1bd` (line 152), `6edaca38c426051a` (line 155), `b7e691ced4e7ae4b` (line 156), `e60e236d88867dbe` (line 156).
