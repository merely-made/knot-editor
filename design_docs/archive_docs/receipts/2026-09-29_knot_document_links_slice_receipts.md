# Document-to-document links slice: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-29_knot_document_links_slice.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `document-links-20260929`: not found at this path

Machine-local paths the plan names (other machines; not retained here): `/Users/markik/Code/repos/mere/crates/system/source-edit`, `/Users/markik/Code/testing/knot-editor/document-links-20260929`, `/Users/markik/Code/testing/shared-source-edit-20261018/knot/`, `/Users/markik/Code/testing/source-edit-dedup-20261018/knot/`, `/Users/markik/Code/testing/source-edit-publication-20261018/knot/`.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Context and decision | 9-28 | 0 | 1 | 0 |
| Acceptance | 61-71 | 0 | 0 | 1 |
| Verification receipt | 72-116 | 6 | 3 | 3 |
| Shared source-edit proposal adapter — 2026-10-18 | 117-200 | 0 | 7 | 1 |
| Immutable snapshot deduplication — 2026-10-18 | 201-244 | 0 | 6 | 0 |
| Published supplier receiving qualification — 2026-10-18 | 245-297 | 0 | 2 | 0 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `2` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `ca9091e` | knot-editor | `75c5c7ca7bf` | Record Commands native acceptance and combined-pin verification | 7 |
| `7e196c534e64b50cf6f00bf4d95e3beb493e5a9f` | knot-editor | `9bca9fa73e0` | Move pinned Rust toolchain to 1.99.1 | 120 |
| `70deef2c8cf3d3410ae35b496a9396514070b24f` | mere | `1d18424745f` | Add shared revision-bound source edit proposals | 248 |
