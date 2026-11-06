# One-shot lifetime experiment: M127 μ=26, r=2, c=4

Completed recording 8 processes: 8 measured, 0 runtime failures, 24 complete measured trials. All modes use prehashed queried-row recovery, 32 threads and a fresh Setup for EVERY trial, including warmup.

| Code | Mode | Processes measured | Commit ms | Open ms | Online total ms | Full wall ms | Max process RSS GiB |
|---|---|---:|---:|---:|---:|---:|---:|
| qa | retained | 2 | 9230.749376 | 10446.283034 | 19691.083129 | 31058.375611 | 32.970970 |
| qa | release | 2 | 9345.448765 | 10554.873293 | 20209.131786 | 31161.008112 | 27.973759 |
| rs | retained | 2 | 11665.054533 | 10404.757761 | 22082.229022 | 33442.450087 | 32.983879 |
| rs | release | 2 | 11684.938179 | 10573.043023 | 22569.146260 | 33518.027433 | 27.983192 |

| Code | Mode | Setup ms | Input ms | Forward release ms | Witness release ms (within open) | Serialization ms | Cleanup ms |
|---|---|---:|---:|---:|---:|---:|---:|
| qa | retained | 4916.831614 | 4852.032092 | 0.000000 | 0.000000 | 1.443281 | 1596.684733 |
| qa | release | 4884.301849 | 4853.308841 | 294.781205 | 75.181456 | 1.227031 | 1212.707636 |
| rs | retained | 4909.560333 | 4856.014710 | 0.000000 | 0.000000 | 0.960028 | 1593.412062 |
| rs | release | 4882.905913 | 4851.898514 | 298.867050 | 75.602727 | 1.019854 | 1212.760305 |

| Code | Metric | Release−retained ms | Change | Round difference SD ms | Round percent range |
|---|---|---:|---:|---:|---:|
| qa | open_ms | 108.590259 | 1.039511% | 12.650423 | 0.954952% to 1.123880% |
| qa | wall_ms | 102.632501 | 0.330450% | 86.867144 | 0.132595% to 0.528559% |
| qa | online_total_ms | 518.048657 | 2.630879% | 95.860364 | 2.284586% to 2.977797% |
| rs | open_ms | 168.285262 | 1.617388% | 134.017285 | 0.702444% to 2.543238% |
| rs | wall_ms | 75.577346 | 0.225992% | 179.622091 | -0.153371% to 0.607485% |
| rs | online_total_ms | 486.917238 | 2.205019% | 155.838810 | 1.698778% to 2.715582% |

`retained` is the constructor-matched control: it keeps forward/inverse parameters and the borrowed original witness. `release` transitions to OpeningOnlyPcs after commit, drops forward storage without inversion, consumes only OwnedWitness and frees its allocation after the first complete lifted fold. Caller-owned top oracle survives verify/serialization and is destroyed in cleanup in both modes. No proof/transcript change or full-table clone is used. Actual capacity and release-round markers are checked against each record.

## Cost boundaries and repetition structure

Each process executes one warmup plus the recorded repetitions; each trial creates fresh Setup and fully cleans its oracle/parameters before the next Setup. The two-mode pilot alternates AB/BA in independent processes. Means within a process are not additional independent processes: `outcomes.csv` gives each process mean/sample SD, `strategy-means.csv` the mean/SD across successful process means, and `round-contrasts.csv` every matched round difference. Small differences comparable with variation are descriptive and do not establish zero overhead or formal statistical significance.

Full wall includes Setup, input generation/claimed-value evaluation, commit, forward release, open, verify, serialization, transcript/fingerprint diagnostics and cleanup, excluding CSV output. Online total is exactly commit+forward_release+open+verify. Witness-release and first-tree-query clocks are subparts of open; they are not added twice. Fresh Setup costs are reported, not amortized away or hidden outside the comparison. First-tree query includes recovery/path extraction/compression and its existing cache/order effects. Process peak RSS spans Setup/warmup/all trials and cleanup, not only opening.

Every complete trial verifies, has matching prover/verifier post-challenges, and shares commitment/proof/challenge fingerprints and wire size with its matched backend across modes/rounds. Size is commitment plus evaluation proof bytes. Incomplete cases retain partial raw evidence but have blank timing/size aggregates. Exit/signal/global OOM observations are kept separately; signal9/global counters alone do not prove PID-specific OOM attribution. No active available-memory guard or estimate exclusion is enabled, and host swap/kernel/cgroup settings are unchanged.

The previous official142/426, whole-tree diagnostic24, queried-row diagnostic54 and failedμ30 trials remain immutable separate datasets. This new outer Setup-per-trial lifecycle must not be silently treated as the old cached-Setup lifecycle. There is no successful nativeμ30 baseline for an exact same-size overhead percentage. Payload estimates (80GiB potential extra release atμ30) are not measured RSS bounds or success guarantees. Tests/engineering reviews are not cryptographic security proofs.

## Reproduction

Hardware/build/source, seeds42/27/28, native binary hash, exact case commands/contracts, memory traces and actual exit/resource records are preserved. Parameters are the unchanged certified uniform M127 quadratic rows; every recorded query count/depth/degree is checked. One shared host, no affinity binding. For the largest-case release-only follow-up there is no matched retained control in this dataset.

From the recorded source with its pinned local Plonky3 checkout:

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --example one_shot_lifetimes
python3 scripts/run_one_shot_lifetimes.py --binary target/release/examples/one_shot_lifetimes --root /tmp/new-empty-pilot
```

The separate largest follow-up adds `--mu 30 --modes release --rounds 1 --continue-on-failure`.
Archive-only numeric reproduction, with no executable required:

```sh
mkdir evidence
tar -xzf raw-evidence.tar.gz -C evidence
python3 analyze_one_shot_lifetimes.py --root evidence/data --parameters m127-parameters.csv --output regenerated
```

The archive omits the large executable but retains its SHA-256. SHA256SUMS covers every other snapshot file.

Source: `7c19ef969514bb1b8915dc7d3a998384899137fc`. Binary SHA-256: `813413a6f4d8462d58ab8a2f544af97efdf8722327e1bb01d66ad2ce4520f490`.
