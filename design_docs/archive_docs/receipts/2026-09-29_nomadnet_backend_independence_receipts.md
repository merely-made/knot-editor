# Backend-independent NomadNet publication: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-29_nomadnet_backend_independence.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Receipts that stayed in the repository under `support/receipts/` (checked present on 2026-11-02):

- `support/receipts/smolweb-browser-receiving/`: present
- `support/receipts/smolweb-browser-receiving/process-graceful.json`: present
- `support/receipts/smolweb-browser-receiving/process-integrated.json`: present

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Boundaries | 17-35 | 0 | 0 | 1 |
| Qualification matrix | 36-54 | 0 | 1 | 0 |
| Local validation, 2026-09-29 | 78-118 | 1 | 4 | 0 |
| Governed host and browser receiving, 2026-10-22 | 119-156 | 0 | 4 | 0 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `2` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `3596dce3` | knot-editor | `3f3882cfecd` | Drain native client handoff before browser receiving probe | 139 |
| `1f660dcb4` | mere | `83bece36f58` | Integrate concurrent Knot checkpoint receiving without changing native hosting | 140 |
| `a622c9607` | turnstone | `2b3323f116b` | Receive shared MIME sessions on current Turnstone cohort | 151 |
| `8eebb5a3` | turnstone | `7299178a72b` | Integrate current main cohort with projection reading controls | 152 |

Ambiguous (more than one main commit has the same subject and author time):

- `cab743c11` (turnstone, line 139, "Drain native page proofs and link closure before teardown"): `8d22304270c` or `d2c4abc5222`

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `4ef1b302` (line 154).
