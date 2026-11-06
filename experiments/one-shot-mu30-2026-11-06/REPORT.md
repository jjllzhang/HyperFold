# One-shot lifetime experiment: M127 μ=30, r=2, c=4

Completed recording 2 processes: 2 measured, 0 runtime failures, 6 complete measured trials. All modes use prehashed queried-row recovery, 32 threads and a fresh Setup for EVERY trial, including warmup.

| Code | Mode | Processes measured | Commit ms | Open ms | Online total ms | Full wall ms | Max process RSS GiB |
|---|---|---:|---:|---:|---:|---:|---:|
| qa | release | 1 | 162202.643925 | 170408.452490 | 337389.687310 | 513010.416024 | 447.937916 |
| rs | release | 1 | 184713.654345 | 168739.561869 | 358240.001596 | 532743.864822 | 447.941017 |

| Code | Mode | Setup ms | Input ms | Forward release ms | Witness release ms (within open) | Serialization ms | Cleanup ms |
|---|---|---:|---:|---:|---:|---:|---:|
| qa | release | 78610.252594 | 77982.775373 | 4755.761949 | 1179.640298 | 5.209429 | 19021.959553 |
| rs | release | 77444.881984 | 78133.742234 | 4766.508796 | 1181.662819 | 2.806007 | 18922.000028 |

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

Source: `66eef8443d7a71850da42ef7080624ec6051eab9`. Binary SHA-256: `813413a6f4d8462d58ab8a2f544af97efdf8722327e1bb01d66ad2ce4520f490`.
