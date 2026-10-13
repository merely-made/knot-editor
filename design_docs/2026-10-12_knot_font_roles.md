# Knot shared font-role adoption

Status: implemented and CPU-qualified at `4fae1d8`; native qualification is
pending an effective foreground launch. This is not merged into published main.

Baseline: published Knot `3af93adda472af2913a5d298f14deb10e85daee2`.
Shared source: Mere `8568f42fdf57f997274c74e63bd4ee045fe1a2b4`, with Genet
`2c7ef73bfa44c07744e0643e3c6f98be3cbd7947`, Netrender
`c74b7cb28090c28b1c6d80b08dcdea18aff48d3e`, Classic provider
`2eae6243881d5b24c984d5ee35dde509ca5db782`, and Dramatis
`0234e7eea6f2ddf2330a71b9425da01b06ae0391`.

## Contract

Reuse Tabard's optional interface, reading, heading, label, and code family
requests, shared controlled role editor, and host-local font inventory. The
application owns explicit Apply and its atomic preference save; editing fields
does not apply or mutate an authored theme. Preserve ordered requests, including
unavailable names, across theme/mode selection, workshop Apply, and reopening.
An empty preference inherits defaults. Resolution discloses fallback without
rewriting requested intent.

Fonts-only Apply keeps the currently applied theme definition frozen. Saving a
different definition under the same theme ID does not expose it through a font
change; explicit Theme Apply remains the adoption boundary.

Knot's writer SourceFace remains explicit IBM Plex Mono or system monospace.
The code role is not the writer. Writing/reading point size, line height,
measure, and detected hard-wrap policy remain Knot-owned. Consume shared
authored role family defaults, then application family preferences, before
exact authored CSS; ordinary specificity and important declarations retain
their authority. Shared authored size/leading fields are retained by Tabard but
are not new Knot writing-metric controls.

## Geometry and runtime

Occurrence cards and measurement probes must share realized label typography.
Changes to realized metrics invalidate card geometry and semantic hit targets
before resuming paint or binding dynamics. Discard the dependent session-only
controller/poses; never bake them into source, authored positions, placement
trace, undo history, or retained material. Do not auto-resume. Color-only
changes must not create a measurement/redraw loop. Explicit writer precedence
keeps its bundled Plex advance valid; a system-font probe must describe the
actual source font and size rather than a theme's code family.

Persistent hidden, noninteractive button shells mirror occurrence-control font
selectors; their generated child labels measure text without frame padding.
Observe every admitted label, including glyph-fallback geometry. The shared
relationship compiler already limits admission to 256 occurrences. This is a
geometry fingerprint, not exact realized face identity; equal widths and line
boxes do not require resetting dynamics. Arbitrary occurrence-specific or
structural CSS is not a general scene-layout authoring contract.

The Label mapping covers occurrence controls and their card measurements.
Shared GraphCanvas's ten-pixel annotation labels still use its existing
placement estimator; this adoption does not newly qualify arbitrary font
metrics for that annotation layout or the separately retained graph leaf.

## Acceptance gates

- Mounted shared picker/input staging, explicit Apply, preference reopening,
  failed-save recovery, unavailable-name retention, and legacy empty fields.
- Theme/mode/workshop Apply preserve application requests. Real bundled-font
  geometry and authored declaration precedence are checked in the host.
- Measured-card invalidation, hit-target reconciliation, stopped transient
  dynamics, unchanged document bytes, authored placement and history.
- Locked workspace tests/check/build; standalone document default and engine
  configurations retain a single coherent supplier family.
- Committed-source wide and compact native flows, reviewed full-frame captures,
  fresh-process reopen, exact preferences/library/source invariants. Foreground
  testing is serialized with Woodshed and Cleromancy.

Formal OS accessibility/physical IME, glyph coverage, remote face fidelity,
other-platform acceptance, and unrelated supplier baseline failures remain
separate. Final evidence and publication status will be recorded in the
feature receipt, not inferred from shared supplier qualification.

## October 12 gate

Final locked workspace all-targets: 681 passed, zero failed, four existing
ignored tests. Standalone document configurations pass 45 default and 61
engine/highlight tests, each with its existing ignore. Locked desktop all-target
check and desktop/acceptance builds pass. Mounted tests cover all listed host
boundaries, including all 256 admitted labels and the last Han-script label.

Native scripts have not reached their first action. Four LaunchServices attempts
and recorded cold/warm prepared-bundle attempts stall with zero presentations
and captures at the unchanged ten-second watchdog. The unmodified published
`3af93ad` baseline behaves the same. Its tracing shows completed raster work
followed by unavailable backbuffer acquisition, with application/key focus false.
An ordinary, non-scenario baseline window renders after native keyboard
activation; that diagnostic is not font-feature acceptance. No guard or feature
assertion was weakened. Native picker/save/scene and fresh-process reopening
remain pending; do not repin other consumers to a falsely qualified milestone.

See the [gate and diagnostic receipt](../docs/receipts/2026-10-12_font_roles/README.md).
