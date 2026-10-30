# Custody and projection client receiver

Status: Source migrated; focused overlay regression passed.

Knot now selects custody-client and projection-door explicitly. Projection
clients use `door::projection`; the mock server's typed envelopes name their
payload slots, and its dev dependency selects door-serve for nonce generation.
The old mixed-client import is removed.

The lock receives Mere `c6290142866eefead385e728c5c5da24ac6f624a` and preserves upstream
Dramatis `051e95a68011072f172b9ec4c0f04d4e5259df5b` and Murm
`1f601488f851c4e54672cd439b7c45728255ea65` for the newly landed secure-channel
publishing admission. Current Mere's
Genet contracts select `565458ea3ab816940484079330eb165a878a22e0`, including the
new events package and Taffy 0.16. Eidetic is aligned to Mere's tested
`01ee97b0b95d679430986dffd54c64ce9b3b5ea3`: the prior f599 supplier lacked
`Journal::prefix_digest`, required by the receiving Pandect implementation.
Unrelated registry selections and NetRender remain at the upstream baseline.
The newer Dramatis and Murm revisions arrived with upstream `d400d0c`; the
receiving change preserves that work.

On T-PC, under the shared 3 GiB, zero-swap, single-job gate:

`cargo test --locked -p knot-editor --features mere-overlay --lib mere_overlay::tests::failed_refresh_purges_cached_actions_and_merge_choices`

passed one test (151 filtered out). The subsequent sync-host check in that
command bundle omitted its required peers feature and did not execute. A
separate peers-feature check remains pending; this receipt does not qualify
peer runtime, the full desktop UI, or Windows. The failed older Eidetic build
and the command-selection error are retained in the run logs.

Evidence is retained under `/home/markik/Code/testing/custody-contract-20261029`,
notably `root-split-knot-receive-2.log`; resolved-edge audits are under
`/home/markik/Code/testing/custody-projection-split-20261029`.
