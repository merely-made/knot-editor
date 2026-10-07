# Knot shared accessibility contract repin, 2026-10-07

Status: locked workspace check and standalone gates passed; the retained workspace timing failure is repaired and its three recovery tests passed on the original unified feature graph. Normal coordinated publication is qualified.

The clean primary was fast-forwarded from `ae3352e818977be2bf4cd56875b868874ab84fd7` to published `2967972ce2953fa15c5e44ac0977c3114e1df67d`, preserving its relationship-scene work. All 43 active Mere declarations advance from `57b4893db6909d5ed9c4ccae30216f0d8164201a` to `f1d169c755e082b5119c2485762fd28f4226d8fb`: 35 workspace, two desktop and six standalone document rows. Genet remains `965b64e206a47d1c8808472de9aa461233638768`; other source declarations are unchanged. [Row map](manifest-row-map.json) records the exact changes. [Original Rust input proof](unchanged-rust-inputs.json) checks all 129 Rust files against the admitted baseline after Git newline normalization. The final candidate changes only one test witness in one Rust file; its complete production prefix remains byte-identical, as recorded below.

The shared revision adds typed, ordered foreign accessibility updates and explicit activation, resync and action contracts. This consumer repin does not activate native foreign semantics or qualify human assistive technology. Mere's optional Knot dependency is unchanged: identity ruling 58 remains a separate seed-fix gate.

The first locked metadata invocation refused the new Inker dependency edge. Its [stderr](metadata.stderr.log) and [exit 101](metadata-result.json) are retained. Actual offline Cargo resolution produces no package additions, removals or registry version/checksum changes. After replacing only the immutable Mere source query, the sole difference is Inker's new dependency on the already-present AccessKit package. [Root comparison](root-lock-comparison.json) covers 1302 records; [standalone comparison](standalone-lock-comparison.json) covers 218. The standalone lock is normally ignored and is retained inside the raw input archives.

[Root metadata witness](family-witness.json) selects one Mere `f1d169c7`, one unchanged Genet `965b64e2`, and one wgpu `30.0.1`. [Standalone default metadata witness](standalone-family-witness.json) records its smaller graph separately. Commands use Rust 1.98.1, BelowNormal priority, one job and the stable `C:/t/cargo-targets/knot-editor` target. Raw process results retain full arguments, PID, times and exit codes. No Cargo home or worktree was introduced; the stable target remains ordinary reusable build data.

The preliminary [208-input archive](qualified-source-inputs.zip) preserves the source before Cargo repaired the AccessKit lock edge. The final [208-input archive](final-source-inputs.zip) freezes the resolved root and standalone locks before tests. JSON manifests bind each raw path and archive by SHA-256; post-test guards match these exact bytes before the test repair. The earlier coordinated-family receipt remains historical and unchanged.

Native window, screen-reader, Linux, packaging and Turnstone full acceptance remain separate gates. Coordinated publication order is Knot, then Redshank, then Turnstone.

The original full workspace run completed with **589 passed, one failed, three ignored across 42 executables**, exit 101. Only `recovery_runtime::tests::failed_write_is_reported_and_later_edit_can_retry` failed: it slept 750 ms after offering a record, but the worker starts its 650 ms timer only after receiving it. The same exact linked executable passed that test alone, exit 0. Its source and executable hashes remain in the [control identity](same-binary-control-identity.json); the original failed aggregate stays explicit.

The approved repair replaces that sleep with the existing bounded FIFO flush barrier, accepting an already-published refusal or the expected failed write while refusing unexpected barrier errors. All existing error-message, failed-ID, retention, retry and reopened-disk assertions remain. [Source audit](recovery-test-source-audit.json) proves the full 11,308-byte production prefix is raw-byte-identical; every other Rust file is unchanged. [Repaired 208-input archive](repaired-source-inputs.zip) binds the final tested candidate. [After guard](repaired-source-after-guard.json) confirms every raw input hash remained unchanged.

| Actual gate | Result |
| --- | --- |
| Workspace all-target locked check | exit 0 |
| Original full workspace all-target tests, no-fail-fast | exit 101; 589 passed, one timing-witness failure, three ignored |
| Exact original linked executable, failed test alone | exit 0; one passed |
| Standalone document default features | exit 0; 47 passed, one existing ignore; doc-tests passed |
| Standalone document engine features | exit 0; 60 passed, one existing ignore; doc-tests passed |
| Standalone engine locked metadata | exit 0; [one Mere/Genet source and AccessKit 0.24.1](standalone-engine-family-witness.json) |
| Repaired recovery module, original workspace feature unification | exit 0; three passed, other library tests filtered |

[Exact commands and totals](gate-summary.json) link every actual process result and raw log. The tested workspace set has 590 distinct passing tests and three existing ignores after replacing the one failed witness with the repaired recovery-module result. This is a composed tested-set receipt, not a claim that a second full aggregate was run. All original runtime suites passed, and runtime bytes are unchanged by the repair.

A package-only desktop rerun began recompiling dependencies because its feature-unification scope differs from the full workspace. It was stopped through normal process control and [retained as superseded](desktop-package-command-superseded.json), not classified as failed code. Its [log custody](superseded-log-custody.json) preserves raw bytes. The replacement uses `cargo +1.98.1 test --workspace --locked --lib recovery_runtime::tests -j1` and only relinks the changed desktop test executable. No unrelated integration binaries were repeated. No native application or human assistive-technology acceptance is inferred.
