# Native small-web authoring and serving: receipts

Receipts for the archived [plan](../2026-11-02/2026-09-11_small_web_authoring_plan.md) (archived under DOC_POLICY §8; the policy puts run evidence here and not in the plan). This file indexes the evidence; the evidence itself is not in the repository. Line numbers below are in the archived file, which begins with a one-paragraph archive header (line 3).

## 2026-11-02, where the evidence lives

Cited in the plan under `Code/testing/knot-editor/` (outside this repository, so path cites, not links). Existence was checked against `C:/Users/mark_/Code/testing/knot-editor/` on 2026-11-02:

- `micron-navigation-a1/`: present

Receipts that stayed in the repository under `support/receipts/` (checked present on 2026-11-02):

- `support/receipts/errand-adoption/consumer.json`: present
- `support/receipts/gemini-publication-runtime/native.json`: not found at this path
- `support/receipts/gemini-publication-runtime/publisher.json`: present
- `support/receipts/micron-publication-runtime/integrated-process.json`: present
- `support/receipts/micron-publication-runtime/process.json`: present
- `support/receipts/native-scroll-server/consumer.json`: present
- `support/receipts/scroll-publication-runtime/process.json`: present
- `support/receipts/shared-site-revisions/desktop.json`: present
- `support/receipts/shared-site-revisions/integration.json`: present
- `support/receipts/shared-site-revisions/normal-build.json`: present
- `support/receipts/shared-site-revisions/site.json`: present

Scratch paths the plan names, which are build or run output and not kept: `C:/t/knot-desktop-immutable-final-tests.log`, `C:/t/knot-input-trace-20260911.err`, `C:/t/knot-micron-nav-a1-target`, `C:/t/knot-nomadnet-immutable-final.log`, `C:/t/knot-nomadnet-stock-client-20260911-recalled/index.mu`, `C:/t/knot-stock-final-sequence-20260911.log`, `C:/t/knot-stock-final-sequence-20260911/`, `C:/t/knot-ui-spartan-headed-20260911`, `C:/t/knot-ui-titan-headed-a-20260911`, `C:/t/knot-ui-titan-headed-b-20260911`, `C:/t/knot-ui-trust-20260911`, `C:/t/retinue-nomadnet-capture-20260911.md`, `C:\t\cargo-targets\knot-editor`, `C:\t\micron-headed-20260913`.

## 2026-11-02, inline run evidence in the archived plan

Sections that carry hashes, pass counts or captures. Counts are matches of the words "SHA-256"/64-hex strings, "pass", and "capture" in the section.

| Section | Lines | SHA-256 or 64-hex | pass | capture |
| --- | --- | --- | --- | --- |
| Knot Errand host adoption (2026-10-24) | 5-73 | 0 | 2 | 0 |
| Governed Micron host continuation (2026-10-22) | 74-101 | 2 | 1 | 0 |
| Governed native Scroll process receipt (2026-10-22) | 102-135 | 0 | 1 | 0 |
| Native Scroll wire ownership (2026-10-21) | 136-174 | 0 | 3 | 0 |
| Saved-site publication into a Moot (2026-10-21) | 175-216 | 0 | 1 | 1 |
| Optional saved-site checkpoints (2026-10-21) | 217-247 | 0 | 1 | 2 |
| Findings and progress (2026-10-21) | 248-287 | 0 | 5 | 2 |
| Shared site model extraction (2026-10-20) | 346-365 | 0 | 3 | 0 |
| Micron request-form consumer (2026-09-13) | 399-533 | 0 | 7 | 2 |
| Progress | 534-555 | 0 | 2 | 0 |
| Initial integration receipts (superseded where noted below) | 556-598 | 2 | 7 | 1 |
| Follow-up acceptance investigation | 599-633 | 0 | 3 | 0 |
| Headed submission receipt | 634-676 | 4 | 0 | 0 |
| Partial Micron preview and native NomadNet publication | 677-762 | 4 | 4 | 5 |
| Broader Micron syntax and client interoperability (2026-09-13) | 763-795 | 0 | 3 | 0 |
| Shared reading presentation receipt (2026-09-13) | 796-817 | 0 | 3 | 0 |
| Micron navigation consumer (2026-09-16) | 818-922 | 0 | 5 | 1 |
| Follow-up: in-page scrolling and the focus rule (2026-09-16) | 923-1043 | 0 | 8 | 2 |

## 2026-11-02, old hashes to main hashes

The plan keeps the hashes it was written with (Mark, 2026-11-02: "Same: live docs only"). Many are pre-merge hashes. Each row below is the commit on the repository's local `main` with the same author time and subject; `13` other hash(es) cited in the plan are already ancestors of their repository's main and need no row.

| Hash in the plan | Repository | Main equivalent | Subject | First cited at line |
| --- | --- | --- | --- | --- |
| `38fc774` | knot-editor | `688b4957a97` | Compare the text-value attribute as str in the document view test | 22 |
| `1f660dcb4` | mere | `83bece36f58` | Integrate concurrent Knot checkpoint receiving without changing native hosting | 77 |
| `42b683108` | mere | `d2728fbc262` | Merge remote-tracking branch 'origin/main' into scroll-resident-integration | 104 |
| `a07a375e` | mere | `dfeaa26d3dd` | Qualify coordinated pool receiving and align Scroll response contract | 121 |
| `cdafbf8` | knot-editor | `1c6f3ead041` | Record integrated Gemini publisher receiving gate | 145 |
| `b37be4f` | knot-editor | `7e932e314ae` | Publish saved site snapshots into existing Moot collections | 177 |
| `b775eae24` | mere | `e5208acec21` | Merge branch 'gemini-runtime-integration' | 214 |
| `4232d5622` | mere | `08c4eed18a6` | Merge remote-tracking branch 'origin/main' | 258 |
| `b8d0e2f` | knot-editor | `f07f6cb0563` | Admit Cambium 0.6 in the shared Knot document surface | 267 |
| `e94cb23e` | mere | `071e6947ffd` | Distinguish H4 adapters from incoming shell qualification scope | 268 |
| `6fcd7e4` | knot-editor | `d790e11cce0` | Merge dependency updates with Scroll site authoring | 286 |
| `80bb318` | knot-editor | `82b863bc061` | Lock qualified borrowed lowering through Mere and NetRender | 361 |
| `d4756e0fb` | mere | `7d2d8412766` | Adopt borrowed Classic lowering across native and web retained paths | 361 |
| `fae329c` | knot-editor | `ace29eb40d6` | Drain Micron Resource acknowledgements before client shutdown | 443 |
| `6b68405` | knot-editor | `0027c3cc53d` | Add validated saved-site snapshots for resident publication | 531 |
| `b503756` | knot-editor | `e0e2387d1a9` | Add native Gemtext and Micron document formats | 539 |
| `709dfdf2` | mere | `540506a8dd6` | Correct P1b's byte-identity scope in the navigation plan | 548 |
| `5dff2f93` | mere | `390eb312172` | Document Cambium's scroll request | 552 |
| `ef724af` | knot-editor | `6130901fdf2` | Fix native submission controls and response presentation | 637 |
| `1777b19840a6478a0faf3ab060a3ff71d5532321` | mere | `3884e62288c` | Add evidence-qualified Micron preview subset | 679 |
| `1f8eee5` | knot-editor | `847c6b5b0e0` | Preview evidence-qualified Micron source | 686 |
| `f1db43e` | knot-editor | `f278576f034` | Render native preview badges and emphasis | 686 |
| `ce6ba53` | knot-editor | `e4e21cb5108` | Paint native preview background while scrolling | 687 |
| `317593383a4bacb60600c5532812f7df4757bed9` | knot-editor | `f56249e092a` | Integrate native Micron preview | 765 |
| `4ac591c9070b5624d9f58b6b511e3a5485e6c87b` | mere | `18030af39e1` | Preserve Micron syntax and render native links and tables | 766 |
| `101d9e9` | genet | `99707babc89` | Align host content placement with transformed input | 798 |
| `b5750a96` | mere | `563fb777464` | Classify open predicates through the shared relation taxonomy | 798 |
| `3961aca` | netrender | `993359b71b4` | Compose external canvas images through scaled scene layers | 799 |
| `cf3afe8` | knot-editor | `7d176d0a4e8` | Align Knot with qualified Micron clipping and outline support | 808 |
| `dce5cc97` | mere | `caa6d116396` | Preserve Micron presentation content in clipping and outlines | 808 |
| `6157a0c` | knot-editor | `6d33c73e4dd` | Consume mere 709dfdf2 for Micron navigation | 822 |
| `4680eb5` | knot-editor | `7ffa2393979` | Hold Micron preview fold state and toggle collapsible headings | 823 |
| `d075c6be` | mere | `8c4078c3ba8` | Keep link predicates through the paragraph inline rewrite | 826 |
| `709dfdf2218cf31afdca86987fa6a59d056b83ed` | mere | `540506a8dd6` | Correct P1b's byte-identity scope in the navigation plan | 827 |
| `d13343f` | knot-editor | `70ff32dd1d0` | Record the Micron navigation consumer in the small-web plan | 870 |
| `fe550eb` | knot-editor | `915d1496a92` | Restore record equality on index identity; harden preferences for newer files | 903 |
| `3a6ea01` | knot-editor | `22e8cc3386a` | Consume mere 5dff2f93 for Cambium's scroll request | 925 |
| `45752a7` | knot-editor | `c53fd75cd21` | Give every Knot control one visible focus outline | 926 |
| `ba25e1a` | knot-editor | `2fe0d907e13` | Scroll the Micron preview to in-page link targets | 926 |
| `5dff2f935ebc50e5324b82875db2c9004439fd5c` | mere | `390eb312172` | Document Cambium's scroll request | 929 |
| `5ae30ca` | genet | `974f61622d4` | Publish the taproot chain and tighten the seam pins it exposed | 970 |

Hex strings that resolve to no commit in the sibling repositories checked (SHA-256 or BLAKE3 prefixes and digests, commits of repositories or forks not checked, build identifiers, or the word `ed25519`): `5bbaf38250b3f61469bb6c1f7ab693c1` (line 711), `483dcfc4871b95cc297daa9d4641215c` (line 732).
