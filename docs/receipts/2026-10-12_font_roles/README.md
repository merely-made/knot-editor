# Knot font roles — October 12, 2026

Status: source and bounded macOS foreground native workflows are qualified.
This receipt is the consumer publication checkpoint; it does not qualify
unattended/default-ten-second startup, other platforms, or the entire stack.

Font implementation: `4fae1d8bdd94ff67eff97977b24810b37680de3a`, based on published
Knot `3af93adda472af2913a5d298f14deb10e85daee2`. Mere is frozen at
`8568f42fdf57f997274c74e63bd4ee045fe1a2b4`. Genet `2c7ef73`, Netrender
`c74b7cb`, Classic provider `2eae624`, and Dramatis `0234e7e` stay coherent.
The later shared coordinator `abc8e5ad5` is documentation-only, not a reason to
change this executable qualification family.

## What is adopted

Tabard owns FontRole/FontFamily/FontPreferences, the controlled five-role
editor, searchable staged picker, and inventory mechanism. Knot supplies its
installed plus metadata-derived bundled inventory, an explicit atomic-save
Apply boundary, local preference storage, semantic CSS mappings, and diagnostics.
Missing ordered requests remain saved. Theme/mode/workshop Apply and legacy
Light/Dark selection preserve them. Font Apply keeps the currently applied
definition frozen until explicit Theme Apply, including saved same-ID changes.
Failed saves preserve the applied preferences and editor baseline.

Writer SourceFace remains explicit IBM Plex Mono or system monospace; Code is
not the writer. Reader size, leading, measure, hard-wrap policy and heading
hierarchy remain host-owned. The family cascade is theme variables, application
family variables, then exact authored CSS with ordinary specificity/important
authority. Shared typed size/leading values remain retained in the theme but
are not newly consumed by Knot's family-only mapping.

Occurrence controls, their hidden button-shell probes and generated text
measurements share actual typography. Observe every admitted occurrence under
the shared compiler's 256 bound. Geometry changes invalidate cards and dependent
session dynamics before advancing motion; new layout reconciles hit targets.
Color-only or geometrically identical changes do not invent work. Runtime poses,
ticks, drag and reveal state are discarded, never written into source, authored
positions, placement trace/history, or retained material; no automatic resume.

## Evidence gates

The original narrow check caught an in-progress bool/unit save-result mismatch;
it is preserved. UI diagnostics also found stale controlled fields after a
programmatic Apply and fonts-only Apply unintentionally adopting a saved theme.
Mounted regressions now cover those boundaries. Cascade review corrected the
derived-theme/app-property order and button/probe selector parity.

The initial wider workspace passed 679 tests with four existing ignores before
the final frozen-definition and admission-limit regressions. The final committed
source [workspace gate](workspace-final.log) passes **681 tests, zero failures,
four existing ignores**, across 45 harnesses. Command logs retain their original Cargo whitespace, with
an exception scoped only to raw logs in this receipt directory.

Standalone document default tests pass 45 with one existing ignore; engine and
highlight pass 61 with one existing ignore. Final source locked all-target
check and both desktop/acceptance builds pass. The acceptance binary SHA-256 is
`c837ae7433c3cfdc42df23fecdc8225b8990b5a4c3ea67397231c5896f1adc2a`.
The baseline comparison used the same build directory. Restoring the identical
feature source relinked the acceptance executable to
`24340da3893445480a38782dfc14ef7ba764b689ca75ecde991b5032e03b4802`;
an immutable copy of that output is retained in the external evidence directory.

## Native procedure and limits

Use only a marked disposable `collection_acceptance` fixture with public test
keys, never a personal-wallet initializer. The document is the exact 41 bytes
`# Night light\n\nNight light by the river.\n`. The theme library is an isolated
copy of the checked-in fixture. Wide seed, wide fresh-process reopen, and 420px
compact picker scenarios exercise normal controls and local preferences, plus
separate signed/encrypted recipe retention in Knot's own mere. Foreground/GPU
work is serialized with Woodshed and Cleromancy.

Runtime `a2128e7d8d7929f5f0df168e2ae033e22f7b38ce` adds an explicitly configured
initial activation allowance for these marked synthetic scenarios only:
`KNOT_TEST_INITIAL_PRESENTATION_GRACE_SECS=60`. Exact public-test marker bytes
and an active scenario are required; values are bounded to 10–60 seconds.
On the first successful presentation the same lane restores its normal ten-second
watchdog before processing that frame. Subsequent failures cannot re-arm the
allowance. Six focused tests cover the boundary, bounds, one-shot transition,
and callback order. Ordinary and unmarked launches retain ten seconds.
This qualification procedure does not qualify unattended/default-ten-second
startup. The original strict-timeout failures below remain recorded separately.

The new runtime's [workspace rerun](workspace-startup.log) passes **687 tests,
zero failures, four existing ignores**, across the same 45 harnesses. Its locked
all-target check and desktop/acceptance build pass. Frozen acceptance SHA-256:
`6ab81ede6eaf88f27c2c0b6ba40f0c00ce2faa1cba93d8b9e0e3dfa5eefb5a26`.

The first activation-grace seed renders 193 presentations and nine nonblank
captures, restoring ten seconds exactly once. Font Apply, geometry invalidation,
cold reset and recipe retention assertions pass, but the overall receipt fails:
the authored typography fixture incorrectly declares library version 1. The
shared loader correctly requires version 2 and refuses that fixture. The failed
receipt/profile are preserved externally as `version-one-fixture-failure` and
`version-one-fixture-profile`; they are not accepted qualification evidence.
The fixture is corrected to version 2 with a focused library-loading regression;
the next matrix starts from a fresh marked synthetic profile.

[verify-native.py](verify-native.py) is read-only and verifies successful
processes, original PNG hashes/dimensions, paired state/presentation records,
requested fonts, source identity, realized remeasurement/runtime reset,
unchanged authored positions, cold reopen, and unchanged profile/library bytes.
It does not substitute for inspecting full-frame images.

### Accepted foreground matrix

All processes use runtime `a2128e7`, the frozen `6ab81ede…` acceptance binary,
and explicitly marked public-key fixtures. The runtime source is unchanged by
the later fixture/test corrections (`fcb8f3b`, `f5ecacf`); its current files are
recorded in [the source manifest](runtime-source-sha256.txt). The final font
integration suite passes eight tests and the full scenario-lane suite passes
nine, including actual 420px Menu → View → Appearance → shared picker control
routing. Their additions follow the 687-test workspace gate above.

| Fresh process | Presentations | Nonblank captures | Result |
| --- | ---: | ---: | --- |
| Wide seed | 194 | 9 | Accepted |
| Wide retained-data reopen | 35 | 2 | Accepted |
| 420px compact picker/staging | 30 | 2 | Accepted |
| Total | 259 | 13 | Accepted |

Original PNGs, process results and paired presentation/state receipts are retained
under [native](native). All thirteen full-frame images were reviewed by the
owner; the shared coordinator independently reviewed the wide/reopen matrix.
The [verification ledger](native/verified-native.json) records binary and PNG
SHA-256, exact 41-byte source identity, requested font stacks, remeasurement,
runtime invalidation, and unchanged authored placement. Every process logs the
one-time initial-60 → normal-10-second restoration, exits normally, and has no
external timeout. No feature assertion was removed.

Fresh-process reopening restores Reading IBM Plex Mono, Heading Source Serif 4,
Label IBM Plex Mono and the unavailable Code request, with fallback diagnostics.
The signed/encrypted recipe in Knot's own mere restores one authored position,
one placement step, one pinned motion role and its retained schedule. Runtime
ticks and poses remain zero; no automatic replay occurs. Source, theme library
and preference bytes are identical after both reopen and compact staging.
Compact Code and Apply controls are visible in its staged full-frame capture;
the compact leg intentionally stages only, without applying or saving.

The first compact script omitted the root Menu action and correctly failed.
Its [failed receipt](native-failures/compact-menu-omission) is preserved; the
corrected script and 420px in-process regression precede the accepted retry.
The version-1 fixture failure above is also retained. Default-ten-second
startup failures below are historical evidence, not accepted matrix members.
After completion, native inventory and exact executable-path checks confirm no
owned acceptance process remains. This lane created no Keeper, broker, or
sleep-prevention process; the GPU reservation was explicitly released.

### Earlier default-timeout and activation diagnostics

External evidence root:
`/Users/markik/Code/testing/knot-editor/font-roles-20261012/native`.
Four original LaunchServices attempts have zero successful presentations and
zero captures before any scenario action, at the original ten-second watchdog.
Prepared persistent application bundles launched through native control also
record cold/warm zero-presentation failures. Their failed receipts are preserved
under [native failures](native-failures). None is counted as accepted evidence.
No font preferences were applied and no relationship recipe was retained by
those original GUI attempts. They did not execute picker/save/reopen or scene
acceptance; only the later accepted matrix above qualifies those workflows.

A clean detached build of untouched published Knot `3af93ad` with Mere
`ef0994ba` and the same Genet/Netrender/Classic family reproduces the zero-frame
watchdog failure. Baseline binary SHA-256:
`18f758703265fb965c0ffdb16046271f1c80c3adb9fd40a992308330a08e5d63`.
Its [frame trace](native-failures/baseline-current-frame/native.log) records
successful rasterization and unavailable acquisition: raster 13–23ms, acquire
9–22us, no clear/composition/presentation; app_active and key are false while
the native window is visible/on its active space. This is not evidence of a
font-feature rendering regression or a provider-quality refusal.

Ordinary diagnostic windows without a scenario were observed separately.
AX exposed the correct document/controls while the surface looked blank.
AX Raise alone did not establish successful presentation. A normal targeted
keyboard action in the live baseline subsequently produced rendered source;
[its trace](native-failures/baseline-interactive-trace/native.log) records acquire
2119us, clear 1905us, composition 183809us, presentation 428us. This is an
activation positive control only, not font-role acceptance. Those diagnostic
runners were bounded to sixty seconds and terminated their owned apps on exit;
this external cleanup timeout did not change the scenario watchdog.

The CUA optional launch_app API was absent on this Mac. Cold getApp calls and
strict-scenario activation attempts arrived after the watchdog, sometimes
returning cached AX or inactive/missing-app errors. Fresh emitted AX followed
by native action worked on the longer-lived ordinary diagnostic. The last
warm prepared-bundle experiment still failed. Deadlines, rendering guards,
feature assertions and supplier source were not changed in those attempts.
The later startup-only opt-in above changes only the initial fixture deadline;
it preserves the post-start watchdog and all feature assertions.
[verify-native.py](verify-native.py) now passes for the accepted matrix above,
while preserving these failed/default-startup observations separately.

This is not exact-face/glyph-coverage or remote/frozen-font fidelity. Physical
IME, formal OS accessibility/contrast, other platforms and arbitrary
occurrence-specific/structural CSS remain separate. Label maps occurrence
controls/cards; shared GraphCanvas ten-pixel annotations still use its existing
placement estimator. No arbitrary annotation-font geometry or separately
retained graph-leaf font adoption is claimed. Existing broader supplier Canvas
golden/table failures are not repaired or requalified by this consumer slice;
no all-green stack assertion is made.

[Owning plan](../../../design_docs/2026-10-12_knot_font_roles.md).
