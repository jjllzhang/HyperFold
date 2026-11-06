# Formal queried-row attempts at μ=30

Actually attempted two M127/r=2/c=4 configurations: 0 completed, 2 runtime failures, 0 complete measured repetitions. Strategy: **prehashed_recovered**, Setup: **retained_both**, threads: **32**. No active available-memory termination or estimate exclusion was used.

| Code | Status | Exit | Signal | GNU time peak RSS (GiB) | Minimum host available (GiB) | Global OOM-count delta |
|---|---|---:|---:|---:|---:|---:|
| QA | runtime_failed | 137 | 9 | 497.415535 | 0.000000 | 1 |
| RS | runtime_failed | 137 | 9 | 497.933971 | 0.000000 | 1 |

Each process is scheduled to prepare Setup once, then attempt one warmup and three measured repetitions, with a fresh commitment in each repetition. The counts above describe actual completions, not planned repetitions. Setup/input generation/claimed-value evaluation are outside commit/open/verify timers. Opening includes prehashing, codeword release, query-row recovery and native multiproof work. Both diagonal directions remain live; no forward-table release or whole-tree rebuilding is used. Proof size counts encoded commitment plus evaluation proof. Only complete verified repetitions with matching prover/verifier post-challenges and deterministic fingerprints enter raw records. Aggregate timing and size fields remain blank for every incomplete case, even if a partial measurement exists.

The processes were actually launched under a pinned binary and immutable execution contracts; no system swap/cgroup/kernel settings were changed. RSS spans the entire process, including Setup/warmup. Exit code and GNU time signal are reported independently. SIGKILL plus a global OOM-counter increase does not establish kernel-confirmed attribution to a particular PID. Memory traces and swap-counter changes are preserved; the old 498 GiB retained-path stop was a partial observation, not a full-peak baseline.

The configured queried-row path can release a 64 GiB first codeword at μ30 after its successor is folded. A failed attempt does not by itself show which allocation/lifecycle stage was reached; no source-inferred release point is presented as an observed event. The plan's staged payload estimates are static accounting, not measured RSS bounds. This run does not include the separate candidate of releasing forward diagonal tables.

These are new strategy-specific outcomes. They do not overwrite the original 142 complete retained-mode configurations / 426 samples, the prior failures, or the μ26 constructor-control study. They do not establish an exact μ30 time-overhead percentage relative to a completed native baseline. Correctness checks and experiments are not security proofs.

## Evidence and reproduction

Exact source, executable SHA-256, seeds42/27/28, certificate, commands, strategy/table/guard policy, exit/resource records and partial raw files are retained. `raw-evidence.tar.gz` omits the executable but records its hash; `resources.csv` includes swap/global OOM observations. Full hardware/build context is in metadata (AMD EPYC9754, about503GiB RAM, Rust1.95.0, native CPU build; no affinity pinning, shared host).

To reproduce actual attempts from the recorded source and pinned local Plonky3 checkout:

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --bin bench
python3 scripts/attempt_query_extremes.py --binary target/release/bench --root /tmp/recovered-extremes-new-empty
```

To reproduce numeric analysis from this archive without an executable:

```sh
mkdir evidence
tar -xzf raw-evidence.tar.gz -C evidence
python3 analyze_query_extremes.py --root evidence/data --parameters m127-parameters.csv --output regenerated
```

The original launched binary preceded the later CLI output-file guard fix; it ran only in freshly created case directories. That fix protects future manual output paths and does not change the measured proving code. Prior evidence remains immutable. SHA256SUMS covers every other snapshot file.

Source: `4f13ffd6e1383ed5b7a8b1bd0693caed70c30e0c`. Binary SHA-256: `91f4e7d6718c56510d62406fd888c54df87b9c46b02fa6dfa247cb916e3b5857`.
