# Knot contextual navigation prototype

**Date:** 2026-10-23

**Status:** Persistent route restoration and semantic Source/Preview scroll continuation passed focused regressions and fresh-process headed native qualification.
**Owner:** Knot Editor

## Behavior

Knot uses Cambium's shared scope controls and trail. Navigation remains
separate from playback, editor commands, source history, and document
authority. The desktop stores the route graph and view bookmarks in its local
`preferences.json`; it does not store document text there.

The bounded route is:

1. A live outline heading.
2. Its containing heading, repeated until the top-level heading.
3. The exact open document session.
4. Its owning open site, when `DocumentSite` has a live site binding.

Outline parents come from the nearest preceding heading with a lower outline
level. This supports arbitrarily nested sections, including skipped levels.
Source, Preview, Outline, and other readings remain peer views. They are not
steps in the contextual route. Existing heading selection from Outline or
Preview enters that heading's context.

Back and Forward use Cambium history. Up follows the live parent relation. Down
enters a section at the current source caret when one contains it, otherwise
the first child section. From a site, Down returns to its last visited bound
document, then to the first currently bound document if there is no visit.
Breadcrumbs target exact route identities.

## Preservation and refusal

Each in-memory route carries a stable document address or site root plus a
runtime admission binding. That binding is never serialized. On startup, Knot
binds saved routes only to documents and sites the launcher already opened;
it never opens a path from history. Closing and reopening a file during the
same window creates a different admission, so its old route stays unavailable.
Across restarts, durable routes can be rebound to the newly admitted open
session. The persisted route also keeps a random admission identity so distinct
same-address sessions do not collapse into one history entry.

If the saved navigation graph cannot be decoded, Knot keeps its raw preference
value through appearance saves and automatic view bookmarks. It replaces that
value only after the user accepts a valid route.

Bookmarks carry the source BLAKE3 digest, editor selection including affinity,
peer view, and semantic scroll position. Source positions are UTF-8 byte
anchors sampled from the first visible line in the laid-out editor. Preview
positions use a byte offset in the rendered Preview text plus a digest of that
rendered text. This works inside long paragraphs when no heading is visible.
Protocols whose text cannot be measured fall back to a visible source heading
range or rendered block index. The enclosing source digest must still match
before any anchor is applied. Knot waits for the destination tile to render,
then uses Cambium's semantic text-position or element-reveal hook. It does not
save raw scroll pixels or source snapshots. A replaced source revision, closed
document, or closed site refuses without moving history. A document route may
still be visited after edits, while an obsolete bookmark is skipped.

Writable sessions retain selection direction and affinity. The current
read-only source-span API restores the same selected byte range but normalizes
it to a forward selection with standard endpoint affinity.

Navigation never opens or reloads a document, copies a source snapshot into a
new session, changes document text, or invokes Undo/Redo. The source remains
the single editable buffer. Source, Preview, Outline, and other readings remain
peer views; the saved view selects an already open matching reading on return.
The file catalog is not promoted into a collection scope, and site-root
scrolling remains outside this bounded slice.

## Scope boundary

Knot has no collection-owner navigation authority in this slice. The existing
file catalog is not treated as a parent collection. Site routing is available
only for a document already bound to an open site. Down from a site chooses the
last visited matching page, then the first open bound page; choosing among
multiple site pages remains future work.

The checked-in headed scenario `scenarios/context_navigation.scn` exercises
nested heading ancestry, Up, Back/Forward, the breadcrumb trail, and semantic
scroll return. `scenarios/context_scroll.scn` exercises manual Source and
Preview scrolling, then Back restoration. `scenarios/context_scroll_reopen.scn`
asserts the persisted Preview view and semantic scroll anchor in a second
process. Run the two scroll scenarios with the same isolated settings profile
and `context_scroll.djot` as the launch document; the fixture bytes must remain
identical.

## Qualification and local build

The original slice's eleven focused tests and Windows scenario passed before
history persistence and scroll continuation were added. The original Windows
scenario passed with 32 presented frames and one nonblank capture, visually
reviewed with a compact titlebar and visible breadcrumb trail. The fixture's SHA-256 remained
`eeb62129c64b381047b1fd12dfb57f215a0ff9baea2f1cb523437f17fe960b43`.

Receipt: [original native scenario](https://github.com/merely-made/mere/blob/7632bbd32a00891b858335209a39f36f5777c0ae/support/family-promotion/contextual-navigation/knot-native-final/scenario.done).
The actual captured raster was 2464 by 1504; this is one desktop viewport gate,
not a responsive-size matrix. It predates persistence and semantic scrolling.
The continuation gates passed: 28 Preview-related unit tests, 8 contextual
navigation tests, malformed-history retention 1/1, closed/reopened route
refusal 1/1, scroll-observation regression 1/1, source/Preview return scenario
1/1, and Preview byte capture/Back/restart 1/1. Logs are retained under
`.cargo/local/context-continuation-*.log`.

Fresh-process Windows native qualification passed at 2464 × 1504. The first
process presented 65 frames and captured the deep Source and Preview return;
the second process presented 6 frames and restored the Preview view and deep
paragraph position; a third fresh process presented 12 frames and asserted
Preview byte 2761 exactly. The first two captures were visually reviewed. All
three runs used the unchanged fixture, SHA-256
`bc93b9b441a3f14af104071f20635686b5757c7e994e5b84edfd49d6a16a1ec0`.
Receipts: [first process](https://github.com/merely-made/mere/blob/7632bbd32a00891b858335209a39f36f5777c0ae/support/family-promotion/contextual-navigation-continuation/knot-text-native/scenario.done),
[fresh-process reopen](https://github.com/merely-made/mere/blob/7632bbd32a00891b858335209a39f36f5777c0ae/support/family-promotion/contextual-navigation-continuation/knot-text-reopen-native/scenario.done),
and [exact-byte fresh-process check](https://github.com/merely-made/mere/blob/7632bbd32a00891b858335209a39f36f5777c0ae/support/family-promotion/contextual-navigation-continuation/knot-text-stable-native/scenario.done).

The restored active view is qualified as Preview. The idle Source pane may
return to its top position when Preview was the last active view; persistence
of that inactive pane's independent viewport is not claimed. A document-bound
Preview can also remain visible while the command-row toggle says “Preview”:
that label describes the following Preview control, while the restored pane is
bound to the exact document session.

The final receiving-source gate passed 357 library tests with one ignored
diagnostic using one test thread. The default parallel run passed 356, with one
failure in the existing recovery-runtime timing test; that test passed alone
1/1, and the serial aggregate passed. The final focused checks after sharing
outline snapshots through `Arc` passed the outline focus and stale-source
filters (1/1 each), contextual-history tests (3/3), and nested route/bookmark
tests (3/3). The outline row retains its source snapshot and refuses delayed
actions after source edits. In short windows the navigation arrows remain
available while the trail collapses, and the source editor retains its 24px
minimum height. Final build passed; its eight compiler warnings are existing
unused-variable warnings. The rebuilt binary then passed the native return
scenario with 64 presented frames and a fresh-process exact-byte reopen with
11 presented frames. Preview reopened at byte 2761; the fixture hash stayed
unchanged. These final checks omit new PNGs and retain the earlier reviewed
rasters. Receipts: [final return](https://github.com/merely-made/mere/blob/7632bbd32a00891b858335209a39f36f5777c0ae/support/family-promotion/contextual-navigation-continuation/knot-final-native/scenario.done)
and [final exact reopen](https://github.com/merely-made/mere/blob/7632bbd32a00891b858335209a39f36f5777c0ae/support/family-promotion/contextual-navigation-continuation/knot-final-reopen-native/scenario.done).

The portable dependency intake uses the published Mere revision
`7632bbd32a00891b858335209a39f36f5777c0ae` and Genet revision
`2b5c01811e6051ecd815cb407ff1ec9b40724fc9`. The root `Cargo.toml` keeps the
AccessKit and document-session patches on their canonical Git sources. The
portable graph has no local path overrides. `cargo metadata --offline --locked`
passed, followed by the locked/offline focused gates: command chrome 12/12,
nested topology/bookmarks 3/3, unsaved text and undo restoration 1/1, closed
route refusal 1/1, peer-view scroll sampling 1/1, and Preview Back/restart 1/1.
The collapse-preference migration/default test also passes 1/1 with the new
1150-pixel menu threshold; a saved threshold remains user-controlled.
Compact logs are under
[`knot-checks`](https://github.com/merely-made/mere/tree/main/support/family-promotion/contextual-navigation-publication/knot-checks/).

The native build passed with
`cargo build --locked --offline -p knot-desktop --bin knot`, using the stable
target `C:/t/cargo-targets/knot-editor` and build directory
`C:/t/cargo-targets/knot-editor/build`. With that build-directory override,
Cargo placed the executable at
`C:/t/cargo-targets/knot-editor/build/debug/deps/knot.exe`; its SHA-256 is
`395320bd8a6a99166eb0b29f35e77047621e512a5edab8bf3a700b5f084bf31cf`. The
original contextual-navigation scenario passed on this binary in 31 presented
frames with zero captures. The fixture remained byte identical
(`eeb62129c64b381047b1fd12dfb57f215a0ff9baea2f1cb523437f17fe960b43`). A
separate 9-frame native probe opened the following Preview at 1000 × 700
logical pixels and passed caption containment.

Native rectangle inspection found the caption-controls group at
`(918,3,90,30)` before the app-local right-side reservation, ending 8px past a
1000px host viewport. The 12px reservation moves it to `(906,3,90,30)`, ending
at 996. Headless and native measurements differed by 12px; this reserved
space keeps the real caption outline inside the native viewport. The final
visual capture is at
[`portable-baseline-final`](https://github.com/merely-made/mere/tree/main/support/family-promotion/contextual-navigation-publication/knot-native/portable-baseline-final/capture/).
The parent accepted its Close-outline containment. The capture scenario itself
failed because its isolated preference file used `menu_below` instead of the
serialized `menu-below` key, and its Preview selector was unavailable in the
resulting compact menu. Treat that raster as visual evidence only; the
separate no-capture native scenarios are the execution passes.

The platform host frame selects Knot's plain command row on macOS and Linux,
and its client titlebar on Windows. Knot keeps the menu, route trail, and editor
commands in its own chrome; the shared frame supplies only layout and native
caption policy. Explicit client titlebar mode keeps the native macOS caption
controls. At narrow widths, Knot shortens the route-trail slot so caption
controls remain in view. The configurable menu-collapse threshold defaults to
1150 pixels. If a user keeps the full menu at a narrower size, the dedicated
Readings action is hidden below 1200 pixels and Outline below 1050 pixels;
both commands remain available in the menu. The dedicated Preview action stays
visible in full-menu mode and is labeled “Show following Preview” or “Hide
following Preview” so it describes the focused-document behavior. Headless
bounds checks cover caption controls at 540, 700, and 1000 pixels, plus full-menu
actions at the 1150-pixel default boundary and at 1000 pixels with a saved
900-pixel threshold.

This prototype persists one active pane's reading position per route visit.
The following Preview toggle can coexist with an independently restored,
document-bound Preview. Independent Source and Preview bookmarks are the next
bounded change. Workspace tile layout remains transient.
