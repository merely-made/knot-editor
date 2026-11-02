# Knot workspace slice 1: frame, tiles, status, sites and graph: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-23_knot_workspace_slice1_plan.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `capture-pairing/combined`: present
- `catalog-diagnostics/combined`: present
- `images/2026-09-23_tabs/`: present
- `images/2026-09-24_3c/`: present
- `images/2026-09-24_3d/`: present
- `images/2026-09-24_4a/`: present
- `images/2026-09-25_4a_repin/`: present
- `images/2026-09-25_4b1/`: present
- `images/2026-09-25_4b3/`: present
- `images/2026-09-25_4c/`: present
- `images/2026-09-25_5b/`: present
- `images/2026-09-25_5c/`: present
- `images/2026-09-26_5d/`: present
- `images/2026-09-26_7a/`: present
- `images/2026-09-26_7b/`: present
- `images/2026-09-26_chip_squeeze/`: present
- `images/2026-09-26_repin_0418/`: present
- `images/2026-09-26_step6/`: present
- `images/2026-09-27_7c/`: present
- `receipts/2026-10-29_forme_workspace/README.md`: present

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

Scratch paths the plan names, which are build or run output and not kept: `/tmp/knot-step9.nJt4Xd/focused-app/scenario.done`, `/tmp/knot-step9.nJt4Xd/focused/scenario.done`, `/tmp/knot-step9.nJt4Xd/root-foreground-probe/scenario.done`, `C:\t\cargo-targets\knot-editor`.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Knot workspace slice 1: frame, tiles, status, sites and graph | 1-18 | 0 | 2 | 0 |
| What slice 1 is | 19-48 | 0 | 1 | 0 |
| Findings from the assessment | 49-120 | 0 | 0 | 3 |
| Design calls this plan makes | 133-186 | 0 | 1 | 1 |
| Steps and done-conditions | 187-278 | 0 | 7 | 4 |
| Risks | 279-292 | 0 | 0 | 1 |
| Progress | 293-1495 | 0 | 76 | 18 |
| September 30 diagnostics continuation: original cohort | 1496-1556 | 1 | 8 | 6 |
| September 30 combined Commands, typography and diagnostics qualification | 1557-1599 | 1 | 6 | 2 |
| Related material | 1600-1613 | 0 | 1 | 1 |
| Forme second-host adoption (2026-10-29) | 1614-1642 | 0 | 1 | 0 |
| Progress | 1649-1682 | 0 | 3 | 0 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `0` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `250fd238` | mere | `fe904e31774` | Remove the dead ipc-channel patch row; record L1 and L2 pass B | 52 |
| `532f1fad` | genet | `dba5edef002` | Repin netrender to aba7d837b, the head local builds already use | 52 |
| `c5a5946` | knot-editor | `cae40b0fb13` | Record the Knot design pass: layout rulings, frames, and slice order | 52 |
| `44f0519` | knot-editor | `ac257c1b158` | Align knot-editor to mere 250fd238 and genet 532f1fad | 90 |
| `3ff250b` | knot-editor | `c9773c7a47d` | Plan workspace slice 1: frame, tiles, status, sites and graph | 295 |
| `fe5adc1a` | mere | `69c50a23166` | Add the reservoir index to pandect (reservoir plan V1) | 315 |
| `ad7f63e2` | mere | `048f3db8541` | Give the Cambium host one self-driving scenario lane | 350 |
| `2a93c836` | mere | `092de686e5f` | Let a host mark tabs with live state | 351 |
| `9ad5990b` | mere | `4be8b86522b` | Add a Cambium status bar of severity chips with anchored popovers | 351 |
| `fec7c63` | knot-editor | `6e0f13c0257` | Align knot-editor to mere 9ad5990b and drive the desktop from scenarios | 352 |
| `75cf5cc` | knot-editor | `c138cec57eb` | Model the desktop's open documents and their tiles | 356 |
| `ce20159` | knot-editor | `059610a94a4` | Hold the desktop's document state per entry in DocumentWorkspace | 360 |
| `52a4957` | knot-editor | `f09f16eab1c` | Keep the site panel on the focused tab's page | 410 |
| `175b5829` | mere | `171503f8b26` | Keep a text field's selected text in its rendering | 440 |
| `52d0a146` | mere | `f510182a9d0` | Keep a single-line text field on one line with its caret in view | 440 |
| `5be48683` | mere | `ce0007fd8e5` | Add an anchored popover and draw the status bar's chips with it | 440 |
| `d938c708` | mere | `ee8519e6119` | Paint the focused field's caret and selection where its text is | 440 |
| `3bf7b0276e6` | genet | `569887ed8c7` | Keep the text inside an inline-block in the retained text frame | 446 |
| `22ebde46ae3` | genet | `17d975883df` | Resolve percentage insets against their own axis of the containing block | 448 |
| `99ba03e6406` | genet | `aa9ef66038e` | Keep an inline-block's own boxes when its content is prepared | 450 |
| `16ca28adda3` | genet | `cf237471918` | Leave positioned children out of a block's intrinsic sizes | 452 |
| `2fae9ef4` | mere | `95656ee4a72` | Repin genet to 16ca28adda3 for inline-block borders and anchor widths | 455 |
| `3bb1684` | knot-editor | `06883a5c0e0` | Align knot-editor to mere bbd19bb3 and genet 16ca28ad | 455 |
| `a663b69` | knot-editor | `946ea025261` | Align knot-editor to mere 5f0689a9 and genet 22ebde46 | 455 |
| `e8fa4b20` | mere | `a4526291927` | Repin genet to 22ebde46ae3 for inline-block text and percentage insets | 455 |
| `8a44e944de1` | genet | `a1a1f2df3b4` | Give form controls the HTML rendering section's intrinsic sizes | 470 |
| `d2342bf7742` | genet | `1d3137a697c` | Size atomic inline boxes against their real containing block (K7) | 497 |
| `3e8d797fda5` | genet | `fbf4a6e571b` | Merge remote-tracking branch 'origin/main' into inline-text-geometry | 499 |
| `e67fdad259d` | genet | `ec5c36c4693` | Shrink unadmitted atoms to fit and wrap atom text at its width (K7) | 499 |
| `2b80f599` | mere | `a9a726217ac` | Repin genet to d2342bf7742 for atomic inline sizing (K7) | 500 |
| `3d14997` | knot-editor | `bc00121d768` | Align knot-editor to mere 2b80f599 and genet d2342bf7 | 500 |
| `3f3b51d` | knot-editor | `f565f0f2c2b` | Align knot-editor to mere b0e29383 and genet 3e8d797f | 500 |
| `b0e29383` | mere | `91a7ab6246d` | Repin genet to 3e8d797fda5 for K7's second atomic-inline slice | 500 |
| `ad20ad17a74` | genet | `05ef6cf8c95` | Paint the text of blockified spans that start a stacking context | 544 |
| `aeeb5bc7` | mere | `d1a5cc92862` | Repin genet to ad20ad17a74 so blockified stacking roots paint their text | 551 |
| `0d341dee` | mere | `a5e25502ee9` | Lay a fold projection out by line, with a gutter cell beside each | 590 |
| `5621ca05768` | genet | `b07659f5a0d` | Give a line box that ends in a break its height when it is empty | 602 |
| `20fa75c` | knot-editor | `caee6777e01` | Align knot-editor to mere 0d341dee and genet 5621ca05 | 607 |
| `c3a5681e` | mere | `930be14d718` | Repin genet to 5621ca05768 so empty lines keep their height | 607 |
| `6a206411` | mere | `8c58dd22a0c` | Keep a moved caret in view vertically, in its pane or the window | 689 |
| `78d014c` | knot-editor | `b3f6d85fc85` | Align knot-editor to mere 6a206411 | 695 |
| `e8f83f89` | mere | `03dde0585a4` | Scroll a clicked element into view and click the part that shows | 717 |
| `9a2e4eeb` | mere | `0cb2d5908e5` | Let the planes outside a scroll request's nearest follow it into view | 723 |
| `b666512513c` | genet | `bca9866c66c` | Size form controls in their own box model, and let content set the flex minimum | 728 |
| `6afb472a0c6` | genet | `c6162f5d2f5` | Build each line box from the boxes on it, per CSS 2.1 10.8 | 729 |
| `6562b7c2` | mere | `27bffb87581` | Lay a popover's anchor out as flex, so its trigger sets its height | 734 |
| `4b67c97` | knot-editor | `4b8a20ab696` | Align knot-editor to mere 9a2e4eeb and genet 6afb472a | 735 |
| `2ea1a6e7` | mere | `934f8fe2244` | Walk caret follow and scroll requests out through one routine, by scrollport | 777 |
| `18e41e44c36` | genet | `bccb7e8d690` | Align atoms on their own baselines, per CSS 2.1 10.8.1 and 17.5.3 | 778 |
| `e3d7061d` | mere | `692cce14823` | Repin genet to 18e41e44c36 for atom baselines (slice B) | 778 |
| `0c98775` | knot-editor | `9be6028e2ad` | Align knot-editor to mere e3d7061d and genet 18e41e44 | 779 |
| `149b8053` | mere | `e4b3f9c5c97` | Merge origin/main: a status bar chip's popover opens in one click | 845 |
| `365807a4` | mere | `f7b4b9646ec` | Let a status bar chip open its popover in one click while another is open | 845 |
| `1b62fd0b218` | genet | `efbb09541e7` | Keep a text field's label as its name when its text is its children | 846 |
| `87b81fb8` | mere | `10c8df0e548` | Keep a status bar's chips whole beside a long message | 961 |
| `37b0bced` | mere | `c4f686cc0fe` | Merge origin/main: D2 judgments beside the status bar's whole chips | 965 |
| `518ca1dc` | mere | `1913b498c9e` | Say that Genet does not paint the status message's ellipsis | 974 |
| `e812260` | knot-editor | `b96427149de` | Align knot-editor to mere 37b0bced | 976 |
| `0418391f` | mere | `8a4743a25d9` | Merge origin/main: insigne's phase B and modulus's card-sized atlas beside the g | 1119 |
| `0cf4f30` | genet | `b2044b077a7` | Forward NetrenderOptions' optional features into RenderCore's boot | 1119 |
| `d265f03` | knot-editor | `8e03f33f75a` | Align knot-editor to mere 0418391f and genet 0cf4f30 | 1120 |
| `a464dc2a` | mere | `8aeede687d2` | Move djinn to knot-editor d265f033, dropping the old-genet copies | 1129 |
| `34626a6c` | genet | `74f0fb52eb6` | Allow hosts to handle selector click delivery | 1253 |
| `ba44756c` | mere | `77efabf33fe` | Reveal clipped scenario targets before delivering clicks | 1253 |
| `8fce5365` | mere | `60aa234f5c8` | Unify Cambium scenario execution in Mesquite | 1287 |
| `7bf0e448` | genet | `122deb38753` | Scope semantic selectors before dispatch and expose projection lowering | 1498 |
| `7eca52e0` | mere | `401ececc35f` | Bound diagnostic delivery and seal capture observations at presentation | 1498 |
| `bd5912fbbb8f468defc3bbeee7eac5a4f7d2b2f3` | mere | `d6e76012091` | Merge command menus and font measures with bounded diagnostics | 1560 |
| `69a2383b2ad777b884a72f31f8f8fb7ece275c0b` | genet | `450f45b694c` | Merge font-backed host layout with qualified semantic diagnostics | 1561 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `284e3545a1f45555` (line 1545), `ab04a4774d14b042` (line 1545), `ee1723dfc65cd21b` (line 1545), `146052a06c775be0` (line 1584), `4980dd84cc255771` (line 1588), `b80c627efd470851` (line 1588), `e6d27eeda5959289` (line 1589).
