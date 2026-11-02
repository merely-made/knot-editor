# Scroll site authoring: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-11_scroll_site_authoring_plan.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Scratch paths the plan names, which are build or run output and not kept: `C:/t/ks1`.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Scroll site authoring | 1-39 | 0 | 0 | 1 |
| Verification receipt | 40-94 | 0 | 4 | 0 |
| Dependency integration receipt | 95-122 | 0 | 5 | 0 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `1` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `33287b0e` | mere | `a06fd627f3b` | Require ordered keys for canvas search | 44 |
| `9e8f9dc2` | genet | `2aaa1fab2d4` | Harden Fleece quote conformance | 44 |
| `2309dc5` | knot-editor | `2263ec79db8` | Advance Gemot group compatibility | 97 |
| `5d26a39` | knot-editor | `0c31acc8f65` | Add native Scroll site authoring and loopback publication | 98 |
| `c328e6fc9028f0024a82c4a0086bf15605812630` | mere | `2141e28f30a` | Adapt Gemot group identifiers | 100 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `ac926d2ab37b53443795439a7181e36416253909` (line 60), `e140e53b` (line 102).
