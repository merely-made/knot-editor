# Typography slice

Date: 2026-09-29
Status: Accepted; automated and native Typography checks passed.
Base: Knot `65396c1`, following document-links acceptance.

## Scope and ownership

Implement design-pass Typography before Collapse and Fonts: bundled IBM Plex
Mono as source default, a system-monospace option, bundled Source Serif 4 for
preview, 60/72/90ch and Full measures, optional hard-wrap following with one
character of slack, fixed 13px chrome and restrained preview heading scales.

Appearance preferences own presentation. Document bytes, write authority,
selection, IME composition and undo history remain owned by the existing document
session. Detection is a pure, bounded heuristic and must not rewrite source.
The preview uses the configured writer measure (72ch by default), never the
source's detected wrap column. Legacy `wide: true` preferences migrate to Full;
an explicit new measure wins. Unknown preference fields retain their existing
preservation contract.

Assets are unmodified, pinned Google Fonts binaries with copyright and OFL
notices, embedded through HostFont. Runtime network downloads, installed-family
inventory and a font marketplace/picker are reserved for the later Fonts slice.
The current pinned Genet contains real shaped `ch` advances and the Mere host
registers font resources on text-system rebuild. Integration exposed a missing
resolution step in Genet's public host-layout transaction, however: the full
document-frame path resolved `ch`, while the entry used by Mere did not. A narrow
Genet fix and a combined Mere dependency pin are therefore required by this
slice; no independent Mere behaviour change is intended.

## Verification gates

- Pure wrap detection checks hard/soft prose and conservative structural and
  Unicode/tab handling, with a bounded source budget and no rewriting.
- Font registration and `ch` geometry use actual bundled faces in the retained
  host, including relayout and a absent-font control.
- Appearance changes preserve bytes/selection/composition/save posture; old
  preferences migrate and new preferences survive restart.
- Source/preview geometry scales with writing size while chrome stays 13px.
- Native isolated fixtures and exact candidate digest; full-frame source and
  preview captures, appearance controls, detected wrapping, source hash unchanged.
- Full workspace tests and proportionate lint checks before commit/push.

## Receipt

The integrated candidate pins Mere
`1f587ba62babdcb3e38398b37f568edddcc20900` and Genet
`609c799dfc45e34cd4ce5ff5cc168b982718726d`. These are published on each
repository's `codex/knot-typography-ch` branch; their main branches are unchanged.

The Genet focused host-layout suite reports 23 passes, and Mere's retained-host
font suite passes both tests with the final dependency-only lockfile changes.
Knot's actual source/preview measure test at 12/24px and fixed-chrome geometry
test pass. All three bundled-face/geometry integration tests pass after replacing
a short italic/upright comparison with a longer mixed-text sample: the original
eight-letter painted bounds rounded to the same width despite distinct font
advances. Per-size Plex checks use the known 600/1000-em advance with at most 1px
painted-bounds tolerance, while the 72ch expected width remains fractional and
tightly checked. The first workspace run reached 252 passes and one ignored
test before that test-sample failure. The final complete, unskipped workspace
rerun passes 471 tests with one ignored and zero failed.
The pure detector's 13 isolated tests pass. Desktop/readings library and desktop
binary Clippy pass with warnings denied; the desktop binary builds successfully.

A separate workspace run excluding the old font-style assertion hit an
intermittent `revision_bell` assertion: the resumed scene was Revision(3) while
the first notice was Revision(2). This test passed in the final unskipped run;
no watcher or endpoint fix was made, so the earlier failure remains recorded
rather than being called repaired.

The rebuilt candidate digest is
`ff32b5e0cb4f839420aaae2f392dbbfdb3b5db7ffd5033c0d7e9c4f36d766267`.
The isolated `field_notes.djot` fixture digest is
`3d2a2bbd8fb43be0bcfa6086af5abd6f1f0b2f40f4ff05bc4fb8e9c2b4d331be`.
Its strict scenario asserts detected column 77 and source measure 78. After the
Mac was unlocked, the final native run returned `RESULT ok` in 2450 frames with
five 2200x1400 captures, zero blank captures and five distinct image digests.
Whole-frame review of source, preview, controls, changed appearance and restored
appearance confirmed the full-width source preserves the authored hard-wrapped
lines without extra wrapping, the preview uses the reader face, and the source
face/size changes restore cleanly. The preview's paragraph flow is independent
of source hard wrapping. Split panes clamp to available width and still wrap;
that is not a failed 78ch inference. The fixture hash is unchanged and the
scenario keeps the document clean throughout, with no save operation.
Test artifacts are under
`/Users/markik/Code/testing/knot-editor/typography-20260929`.

The accepted receipt is `receipts/typography/scenario.done`; per-image digests
are source `bf6302abffd77afe`, preview `aec64d9b1614854c`, controls
`0dd9b733968477b2`, changed `773ab8e429ef18c4`, restored `9ddde3988057f76c`.
Font licence disclosure was not opened in this native capture; embedding and
copyright notices are checked in source/assets. Preference persistence and
legacy migration are covered by automated file tests, not a native relaunch
capture. The crowded command toolbar and appearance panel remain for Collapse.

The first native run returned `RESULT ok` for state assertions (2447 frames,
five nonblank captures), but whole-frame review found extra source wrapping.
It is preserved under `typography-state-pass-visual-wrap-fail-2447` and is not
accepted. The real-font host test independently reproduced 432px for 72ch at
12px despite IBM Plex Mono's 0.6em advance. A new public Genet-entry proof-font
test reproduced 720px instead of 1440px at 20px before the shared fix.

## Limits and follow-up

Hard-wrap following is conservative: ASCII/Latin-1 columns, eight-column tabs,
plain prose only, and a 1 MiB budget. Unsupported Unicode or ambiguous structure
falls back to the configured measure; no text is rewritten. Split panes may
clamp a selected measure to available width. Collapse remains the next slice.

Genet's broader suite has three independently reproduced pre-existing failures
outside this fix: two library float/shape tests and one nested-table glyph
position test. They reproduced with the new resolution call disabled as well;
the focused suite is green, not the entire Genet suite. Source-selection painting
outside its pane also remains a separate known issue.
