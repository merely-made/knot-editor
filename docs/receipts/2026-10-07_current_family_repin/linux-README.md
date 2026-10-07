# Knot Linux current-family compile follow-up, 2026-10-07

The locked workspace all-target check **passed** on `thinkpad-l14-f` at exact published Knot `0096591a0af98bf4777d8bfe718c0423dea9fc90`. This is a Linux compile gate. No Linux tests, native windows, screen-reader sessions or package gates were run. Complete Turnstone S0 and foreign-browser accessibility acceptance remain separate.

[Preflight](linux-preflight.json) records clean main `8610058b52ad9ea652be5674f918da1edf180003`, no live compiler/app owner, Rust 1.98.1 and the existing `/home/markik/Code/target/CACHEDIR.TAG` signature. Origin was fetched and the clean checkout fast-forwarded only to the requested published commit; [stdout](linux-ff.stdout.log) and [stderr](linux-ff.stderr.log) preserve the result. Remote Mere and other repository checkouts were untouched. No target, Cargo home or worktree was introduced.

[Inputs](linux-inputs.json) verifies all six tracked qualification inputs byte for byte against the Windows [final-input archive](final-inputs.json), including the exact baseline LF notice and its Git attribute. The ignored standalone lock is excluded because this gate uses the workspace root lock. The root lock retained SHA-256 `312ba490f88a322b67fe2296882e6c5c679cbfd06c7398e314eee64223545d59` after both commands.

| Command | Actual result | Evidence |
| --- | --- | --- |
| `nice -n 10 cargo +1.98.1 check --workspace --all-targets --locked -j 1` | PASS, exit 0; 08:36:32 to 08:45:20 UTC, 8m48 | [start](linux-workspace-check-start.json), [result](linux-workspace-check-result.json), [stdout](linux-workspace-check.stdout.log), [stderr](linux-workspace-check.stderr.log) |
| `nice -n 10 cargo +1.98.1 metadata --format-version 1 --locked` | PASS, exit 0; 08:45:20 to 08:45:24 UTC | [start](linux-metadata-start.json), [result](linux-metadata-result.json), [raw metadata](linux-metadata.stdout.log), [stderr](linux-metadata.stderr.log) |

The immediate Popen observation raced the `nice` exec and recorded 0. A later [owned Cargo observation](linux-priority-observation.json) verifies nice 10, and an [owned compiler-lineage observation](linux-compiler-priority-observation.json) verifies rustc inherited nice 10. These are observations at recorded times; the raw start record is preserved.

[Selected-family witness](linux-family-witness.json) derives exactly one Mere source, `57b4893db6909d5ed9c4ccae30216f0d8164201a`, and one Genet source, `965b64e206a47d1c8808472de9aa461233638768`, from the hashed metadata. Fontsan 0.7.0 selects `libz-sys,wuff`, and all eight path packages belong to this exact Knot checkout. The existing unused p2panda-stream patch and platform-specific unused-variable warnings remain warnings.

[Postflight](linux-postflight.json) records the same main revision, no tracked source/index diff and completed gate subprocesses. The existing marked target is retained for ordinary stack builds; its build ownership was released when the commands finished. Newly generated receipt files remain owned documentation evidence on the remote checkout.

[Remote hashes](linux-remote-artifact-hashes.json) bind the copied host outputs; every copy was verified locally. [Follow-up hashes](linux-artifact-hashes.json) cover all other `linux-*` files. All 47 originally published Windows receipt blobs and their original artifact index remain unchanged. Integration review approved publication of this compile-only documentation follow-up.
