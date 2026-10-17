# Pool workspace prototype

The [machine-readable receipt](receipt.json) separates source/model checks, the temporary local Mere cohort, actual macOS interaction and fresh-process persistence. Six captures were inspected. No canonical supplier repin or standalone Pool crate is implied.

Reproduce with the checked-in `scenarios/pool.scn`, `pool_reopen.scn` and `pool_empty_reopen.scn`, in that order, using the same isolated `APPDATA` directory. Launch the first with disposable copies of the source and destination fixtures; later processes open the destination alone. Use Mere's `scripts/run_macos_scenario.py`, a fresh output directory per run and 120-second timeout. The first scenario also checks the compact window. `fixtures/destination.saved.djot` preserves the resulting saved composition separately from the initial destination fixture.

Build/test with `cargo --offline --config local-source.toml ...` only after updating its absolute local paths. Preserve the resulting local-cohort lock separately and restore the canonical application lockfile after checking. The receipt retains the local-cohort lock and the canonical lock hash/base revision. The native binary was built with the existing Homebrew 1.99.0 compiler; it is not a repository-toolchain 1.99.1 receipt.
