# Composition material belongs in Knot's mere: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-30_composition_mere_retention.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `background-categories-20261007.1yLcLd/`: not found at this path
- `boundary-family-20261008.knV7Gq/README.md`: not found at this path
- `collection-organization-20261005.TGUMZ3/`: not found at this path
- `composition-mere-20260930/receipts`: not found at this path
- `editorial-20261008.9rqdPX/README.md`: not found at this path
- `receipts/2026-10-07_current_family_repin/README.md`: present
- `receipts/2026-10-11_authored_scene/README.md`: present
- `receipts/2026-10-11_dynamics_status/README.md`: present
- `receipts/2026-10-11_dynamics_viewport/README.md`: present
- `receipts/2026-10-11_keep_motion_placement/README.md`: present
- `receipts/2026-10-11_live_dynamics/README.md`: present
- `receipts/2026-10-11_relationship_consumer_review/README.md`: present
- `recipe-responsive-20261005/receipt.json`: not found at this path
- `relationship-recipe-20261005/receipts`: not found at this path
- `relationship-scene-20261007.B3zrsH/`: not found at this path
- `scene-adoption-20261007.rZcfdR/`: not found at this path
- `scene-controls-20261007.DuIjHI/`: not found at this path

The folders under `receipts/`, `design_docs_receipts/` and `validation/` were tracked in this repository as `docs/receipts/`, `design_docs/receipts/` and `design_docs/validation/` until commit `ed2ce9d` (2026-11-02), which moved 834 files (113,533,394 bytes) to `Code/testing/knot-editor/`, each copy written from its committed blob and checked by SHA-256 before removal. The recorded SHA-256 values inside the receipts are unchanged. The moved `run-*.py`, `verify*.py`, `run_native.py` and `*.sh` scripts locate this repository from their own path (`Path(__file__).resolve().parents[3]`, `parents[2]` or `$KNOT_SRC`), so from their new home they no longer find it; copy one back into a `docs/receipts/<name>/` folder of a checkout, or pass the repository root, to rerun it.

Machine-local paths the plan names (other machines; not retained here): `/Users/markik/Code/testing/genet-boundary-20261008.R85RvP/README.md`, `/Users/markik/Code/testing/knot-editor/background-categories-20261007.1yLcLd/`, `/Users/markik/Code/testing/knot-editor/boundary-family-20261008.knV7Gq/README.md`, `/Users/markik/Code/testing/knot-editor/collection-organization-20261005.TGUMZ3/`, `/Users/markik/Code/testing/knot-editor/composition-mere-20260930/receipts`, `/Users/markik/Code/testing/knot-editor/editorial-20261008.9rqdPX/README.md`, `/Users/markik/Code/testing/knot-editor/recipe-responsive-20261005/receipt.json`, `/Users/markik/Code/testing/knot-editor/relationship-recipe-20261005/receipts`, `/Users/markik/Code/testing/knot-editor/relationship-scene-20261007.B3zrsH/`, `/Users/markik/Code/testing/knot-editor/scene-adoption-20261007.rZcfdR/`, `/Users/markik/Code/testing/knot-editor/scene-controls-20261007.DuIjHI/`.

Scratch paths the plan names, which are build or run output and not kept: `/tmp/knot-recipe-composition-tests.log`, `/tmp/knot-recipe-core-all.log`, `/tmp/knot-recipe-desktop-publish.log`, `/tmp/knot-recipe-readings-tests.log`, `/tmp/knot-woodshed-recipe-rebind.log`.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Organization qualification (2026-10-05) | 112-152 | 2 | 2 | 6 |
| Current-stack and category qualification | 215-244 | 2 | 2 | 1 |
| Coordinated family follow-up, Windows, 2026-10-07 | 245-274 | 0 | 4 | 0 |
| Static relationship scene continuation | 275-301 | 0 | 0 | 1 |
| Static scene qualification, macOS, 2026-10-07 | 302-355 | 4 | 3 | 5 |
| Compact camera controls continuation, 2026-10-07 | 356-435 | 8 | 9 | 9 |
| Retained background-category lenses (2026-10-07) | 436-496 | 4 | 1 | 3 |
| Shared editorial adoption (2026-10-08) | 497-591 | 4 | 5 | 4 |
| Boundary-lookup continuation (2026-10-08) | 592-671 | 4 | 9 | 2 |
| Authored occurrence placement continuation (2026-10-11) | 672-692 | 0 | 2 | 1 |
| Editing and acceptance | 712-740 | 0 | 0 | 2 |
| Shared live-dynamics host continuation (2026-10-11) | 752-806 | 0 | 0 | 2 |
| Dynamics viewport continuation (2026-10-11) | 807-848 | 0 | 1 | 2 |
| Dynamics completion and refusal continuation (2026-10-11) | 849-885 | 0 | 3 | 2 |
| Shared relationship recipe continuation (2026-10-05) | 888-1035 | 4 | 22 | 12 |
| Stack seams P1: Knot measures its recipe card (2026-10-05) | 1036-1141 | 0 | 1 | 0 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `0` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `21ce01576d0ba4cbac081a5b6a4098399c76b38a` | knot-editor | `5bbaefa714c` | Keep empty author notes visible and qualify editor action reveal | 114 |
| `07db35e2f154df9a14a8cfdc165cc72ddae61f13` | mere | `69fb1dc6424` | DOC_README: the djinn test harness plan's status, forks 1 to 8 ruled | 115 |
| `bd3e8861b2932d62b73c9936ac3a317f1df3ffdb` | genet | `e737eced766` | design_docs: plan Livery-owned line breaking over Parley as a library | 116 |
| `54bb8cd` | knot-editor | `48bdf90bcb3` | Merge origin/main 9d0b955 into the Mere pre.4 repin | 148 |
| `289c9a98d6b0f17a441f184839035784fadb865e` | mere | `811fb3b89c0` | Reconcile Knot compact recipe qualification | 168 |
| `d041cc69b` | mere | `cd815443eb4` | E1b probe: two sessions joined as subtrees in one window, for headed screen-read | 206 |
| `81f0039b482e5dff651e241e475b00b74285015d` | knot-editor | `1c765524a0a` | Edit and retain disclosed relationship categories through shared recipes | 217 |
| `493cccc` | knot-editor | `94fb7b629de` | Align Knot with current Mere scene and session contracts | 218 |
| `d041cc69b588b6f1dadd22308c2bc4059496cabd` | mere | `cd815443eb4` | E1b probe: two sessions joined as subtrees in one window, for headed screen-read | 219 |
| `d851a9db0cd1ff7837768250f21e9dff63455940` | genet | `a78fcb2e5a7` | Hide what a person cannot perceive from taproot and the styled projection | 220 |
| `ec0224e6023dc0282730e6ddc388c666ac520769` | knot-editor | `4fcb130550b` | Record current stack and relationship editor integration gate | 247 |
| `57b4893db6909d5ed9c4ccae30216f0d8164201a` | mere | `a2e0982899f` | Merge current main and repin browser prerequisite to current Genet and Weld | 248 |
| `965b64e206a47d1c8808472de9aa461233638768` | genet | `2d30a0d39c7` | Align fontsan backend with Servo and test valid WOFF2 | 249 |
| `bbd72a8f20a9e8a4153d21457bc0d1416a72c2df` | knot-editor | `fcfd2c4cf2d` | Keep relationship scene viewport within the compact drawer | 304 |
| `655ea47` | knot-editor | `34552186679` | Keep shared scene hit targets transparent and reveal canvas in native receipts | 305 |
| `cf3741d` | knot-editor | `8ee98f351ac` | Add retained static relationship scene with authored strata and camera | 305 |
| `57b4893d` | mere | `a2e0982899f` | Merge current main and repin browser prerequisite to current Genet and Weld | 306 |
| `bbd72a8` | knot-editor | `fcfd2c4cf2d` | Keep relationship scene viewport within the compact drawer | 306 |
| `965b64e2` | genet | `2d30a0d39c7` | Align fontsan backend with Servo and test valid WOFF2 | 307 |
| `ea9e6da74` | mere | `b3c6ff50261` | Record authorized viewer and speed main integration gates | 307 |
| `f1d169c7` | mere | `38a50a234b9` | Carry foreign accessibility through native surface producers | 369 |
| `14cd06e` | knot-editor | `8b1f6fd3c4b` | Adopt shared foreign accessibility contracts coherently | 370 |
| `356a832cf` | mere | `62cbc4ad31e` | dynamics grammar G3: the landing entry records main 6050114e's doc-only merge | 371 |
| `d4191004db50129bb4b88d5b243df98307a1cab1` | knot-editor | `4fe68a019b3` | Disclose scene pan controls without retaining UI state | 381 |
| `43c5c17` | knot-editor | `cc8f0099dc9` | Reveal expanded camera rows in native reopen receipts | 382 |
| `eabd443` | knot-editor | `eb4d38e5acc` | Lend the signing seed instead of copying it (Mere vault lock ruling 49) | 420 |
| `4d984f3a285ff7f5e156151bb561530586b49fde` | knot-editor | `8914604b935` | Merge remote-tracking branch 'origin/main' into codex/knot-composition | 422 |
| `96c0ed97d` | mere | `cdd42404be5` | Scenograph editor plan: ruling SE34, E3 and E4 wait for S1; Turnstone follow-up  | 463 |
| `122968da05ba67e412458ac1ad7b2aaeb35b551e` | knot-editor | `4d7313217fa` | Retain background-category lenses in Knot static scenes | 468 |
| `b555eaed6` | mere | `57c2e99caf2` | distillery probe: repin genet to e84f9c7f, matching the root manifest | 502 |
| `e84f9c7f` | genet | `350fff454ab` | fix: preserve positioned formatter root admission | 503 |
| `aa8a59d94d7acc2f1925c405118b21e413a2092e` | knot-editor | `5db16993c90` | Give app-owned single-line fields a visible editing affordance | 549 |
| `f39a63a7` | knot-editor | `2e9a705ade1` | Adopt shared recipe option declarations and bounded editorial history | 550 |
| `341d5ca9` | knot-editor | `73da7ae2cf3` | Adapt Knot focus and field styling to app-owned Cambium textboxes | 551 |
| `aa8a59d9` | knot-editor | `5db16993c90` | Give app-owned single-line fields a visible editing affordance | 551 |
| `6cb2284a` | genet | `86217ede10f` | docs: record published Forms Phase A and ThinkPad qualification | 576 |
| `15713014e2e23b887360471552f75f60684f5384` | genet | `e27294d1327` | Index retained paragraph boundary ownership without changing break policy | 595 |
| `227062a92e0698621c488251590dba3437d780e7` | mere | `66a5dabe16b` | Adopt qualified Genet boundary lookup repair coherently | 603 |
| `ec46d0ac0f474f6129b5b0e4fcd6e4b8a534689d` | knot-editor | `94e0ec4d146` | Adopt published boundary lookup repair and coherent Mere family | 609 |
| `227062a92` | mere | `66a5dabe16b` | Adopt qualified Genet boundary lookup repair coherently | 612 |
| `1eacea2` | knot-editor | `4c8d91870e6` | Record shared editorial native qualification and layout performance hold | 630 |
| `0e78fcffb140395a5c0d2f64a4dfda2b81751681` | mere | `9756feb38c5` | Capture real producer resources and qualify production host gates | 684 |
| `4bc75583865c2fb862b7acd7fe16dc28eb591042` | genet | `8063e2861b8` | Adopt complete producer-resource Netrender source | 685 |
| `0234e7eea6f2ddf2330a71b9425da01b06ae0391` | dramatis | `25ead7cfb8b` | Include release licenses and verify refused CXF metadata | 686 |
| `ef5412d760a978584c8c1801e8a765c51aecd403` | netrender | `26d93312042` | Admit complete producer resources and qualify sparse workshop replay | 687 |
| `3a8f38130ba846eb25717c46bd05d12af18bdce8` | mere | `3d0e84886d3` | Bind disclosed occurrence relationships to shared projection dynamics | 766 |
| `19b9904` | knot-editor | `841ecb26bc6` | Qualify authored scenes with native retention reopen and compact captures | 773 |
| `63aa695` | knot-editor | `e72ab0a92e0` | Keep scene motion placement as one undoable edit | 809 |
| `156626f` | knot-editor | `ed704785636` | Keep dynamics preview and playback controls visible together | 851 |
| `8f61cfe52` | mere | `c6bfc69ddd2` | Prototype retained signed-object recovery through mesh admission | 877 |
| `c79bb8c203b52817991e0d7ba1c4ce3c3a2aac34` | mere | `9858e30778e` | Share bounded relationship recipe authoring and compilation | 918 |
| `e4cf739c958b87331b966bb3ba5e4498dead3d14` | knot-editor | `b2b4bd10332` | Adopt shared horizontal reveal and cover compact relationship recipes | 959 |
| `5011e2f90e988775410fe4b4213e9895fe64538e` | mere | `b11fbdfcae9` | Reveal nested horizontal targets through the shared host | 960 |
| `bd3e8861` | genet | `e737eced766` | design_docs: plan Livery-owned line breaking over Parley as a library | 961 |
| `c92e7c96779ef316f5d8244a59de7cbc84f42a0b` | woodshed | `41837f88594` | Disclose Woodshed relationships for shared recipe adoption | 1007 |
| `19e6dc9f` | mere | `b62bb888b8b` | Merge origin/main: Knot compact recipe qualification (289c9a98) | 1038 |
| `dd2cb6fd` | mere | `70a2695b470` | scenes: one arrangement catalog; hosts compile through a ProjectionCompiler (sta | 1038 |
| `07db35e2` | mere | `69fb1dc6424` | DOC_README: the djinn test harness plan's status, forks 1 to 8 ruled | 1044 |
| `102aa548` | mere | `78601c6b9c4` | burn plan 13.46: how Knot measures its P1 card (previous layout, one hidden fram | 1046 |
| `b7b52e56` | mere | `acf665cd825` | burn plan 13.46: Knot's probe retries stop after a few frames; Knot pushes after | 1103 |
| `306a808` | knot-editor | `1ba90f23734` | Stop measuring recipe cards after three frames without a usable rect | 1126 |
| `37f7e39b` | mere | `24edd53efed` | burn plan 13.46: Knot fixes the empty band (the scene fits its cards) before pus | 1126 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `5862bba26c8a9f56f6a5fdc8ea6966b360a5d63a` (line 688).
