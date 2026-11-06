# Reproducible experiments

## Commands and measurement contract

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --locked --release --bin bench
./target/release/bench --list
./target/release/bench --threads 32 --mu 20..=30 --r 2,4,8 --c 2,4 \
  --fields goldilocks,m127 --backends qa,rs --repetitions 3 \
  --memory-mode retained --max-estimated-gib 480 --output bench-results/results.csv
```

Default CLI memory ceiling is a conservative **64 GiB**, not a claim that all
configurations fit. Change it only for the actual host budget. Every selected
configuration is either measured or written as `skipped_memory` with an estimate;
blank timing columns are **not** measurements. `--memory-mode auto` selects the
explicit low-memory path only when the retained-path estimate exceeds the ceiling;
`retained` and `low` force the respective paths. The estimate includes 5% plus
0.5 GiB headroom and uses `digest_storage` for full digest payload. It is an
allocation model, not a guarantee about physical RSS. `--list` allocates no Setup data.
`--resume` skips completed measured rows in the same output; use it only with the
**same binary, parameters, flags, seeds and hardware**. Interrupted partial attempts
may leave raw samples, so aggregate only samples corresponding to completed means.

`20..=30` means even sizes 20,22,24,26,28,30. `--mu 20,24` is also accepted.
`--quick` instead uses mu=10, small uncertified bottom blocks and 24 queries.
`--threads 1` is supported for functionality, but official timing runs use 32.
Do not run multiple timing sweeps concurrently.

Setup is performed once per configuration and excluded from the three core
measurements (its time is recorded separately). Each configuration has **1 warmup**
plus 3 timed repetitions by default. Each repetition deterministically regenerates
input and evaluation point **outside** the timed regions. Message ownership passes
to commit without a timed full-vector clone. The same field/mu input stream is
used across code backends and rates for comparison.

- **Commit** includes HFC encoding, top Merkle tree, z0 sampling and y0 evaluation.
- **Open** includes weight generation, all sumcheck/fold/inner trees, deduplicated
  paths and cross-layer coordinate removal; excludes final Postcard serialization.
- **Verify** includes round/terminal checks, base encoding, coordinate restoration,
  native multiproof authentication and every local fold check.
- **Size** includes canonical Postcard bytes. Report `commitment_bytes`,
  `eval_proof_bytes`, and their sum `proof_size_bytes`; `proof_size_kb` means KiB
  (1024 bytes). No proof bytes or hash framing are silently omitted.

`results.raw.csv` contains each measured repetition; `results.csv` contains arithmetic
means. Verification and final transcript agreement are checked in every measured
and warmup iteration. Raw files flush per sample and means per completed case.

Setup seed `[42;32]`; message seed `[27;32]`; evaluation-point seed `[28;32]`. The
field/mu-specific stream is fixed. Multiple independent Setup seeds are **not**
part of this initial sweep; results do not estimate a distribution across codes.

## Hardware/build provenance

Initial host: Linux x86_64 `ps`, AMD EPYC 9754, 128 physical cores /256 logical
threads, four NUMA nodes, approximately503 GiB RAM. One local Rayon pool of32
threads; not CPU-pinned, no frequency-governor tuning. Native build enables host
Goldilocks SIMD; M127 is scalar arithmetic. Record OS, Rust/Cargo, source commit,
Plonky3 commit, Cargo.lock/executable hashes, flags, command, UTC start/end and
peak RSS alongside each run. CPU contention, NUMA placement, Setup-seed variation
and timing noise remain limitations; no cross-machine absolute ranking is claimed.

The first traversal on 2026-11-05 was intentionally stopped for review fixes.
Its partial old-kernel timings are retained separately and must not be mixed with
revised-kernel measurements. The revised 144-case traversal uses a fixed copied
binary and one process per configuration, `/usr/bin/time -v` resource records and
a live available-memory guard. The largest explicit low-memory model is about
471 GiB including headroom. Completion and actual RSS are determined from each
case's files, never from the model.

In low-memory mode diagonal direction conversion is outside commit/open timing
and recorded as `diagonal_transition_ms` (sum of both conversions per repetition).
For a one-shot total cost include setup and diagonal transitions as well. First-tree
reconstruction remains inside open time. The CSV `low_memory` flag is required when
comparing configurations; different memory strategies have different time costs.

## Criterion

`cargo bench --bench pcs` uses32 threads and one certified Goldilocks QA case
(mu20,r8,c2,e3), sample size10, warmup1s, target measurement3s (Criterion may
increase iterations). It measures commit/open/verify and wire serialization size
work independently; timing scopes differ slightly from the sweep, and the fixed
Criterion opening point differs from the sweep's random extension point. Preserve
Criterion sample/estimates/benchmark JSONs, not only console confidence intervals.

## Retained-only follow-up policy

The formal follow-up measurements disable recomputation with `--memory-mode retained`.
All completed cases from the earlier auto-policy traversal had `low_memory=false`;
they can be reused with their original command/resource evidence and explicit import
provenance. A new output directory preserves the original manifest rather than
rewriting its policy. For example:

```sh
python3 scripts/run_sweep.py --binary target/release/bench \
  --output-dir bench-results/retained-only --memory-mode retained \
  --max-estimated-gib 480 --min-available-gib 12 --repetitions 3 \
  --allow-memory-skips --reuse-completed-from /path/to/previous-sweep
```

`--reuse-completed-from` accepts only the same binary hash, repetitions, thread count
and memory ceilings, and only completed retained-mode means/raw samples. Every reused
case retains its old command and a `reuse.json` record. The runner rejects a retained
sweep containing a low-memory measurement. It never falls back to recomputation.

For M127, mu30/r2/c4, the late retained payload alone is approximately 592 GiB:
16 GiB message + 128 GiB forward/inverse diagonals + 192 GiB top tree + 256 GiB
intermediate trees. This excludes small allocations and other transient buffers;
the guarded model with headroom is higher. On the original 503 GiB host this is
an explicit memory exclusion under the retained-only policy. Goldilocks fitting
at the same indices does not establish that this larger-element configuration fits.
A larger-memory runner is needed for a comparable complete-retention measurement.

`--allow-memory-skips` continues after such exclusions. `finished.json` distinguishes
processed, measured, skipped-memory and raw-sample counts; an exclusion has blank
timings and is never called a completed measurement.

## Actual attempts above the model ceiling

An estimate-only exclusion can be followed by an actual retained attempt when that
is the requested experiment contract. To avoid competing with the base traversal,
`scripts/attempt_retained_extremes.py` queues the two explicit M127 mu30/r2/c4
cases after the base sweep completes. It uses the same pinned binary, threads,
seeds and repetitions with `--memory-mode retained`, raises only the estimate
ceiling, and monitors actual host available memory every 0.1 seconds. The live
12 GiB available-memory guard remains in force. A guard stop is an observed
termination under that budget, not an operating-system OOM result or a proof that
no other memory policy could succeed.

The queue requires an explicit `actual-attempt-policy.json` with `enabled=true`
and `memory_mode="retained"`; setting enabled false revokes pending work. Commands,
partial raw samples, resource usage, memory traces and stop reasons are preserved
under `actual-attempts/`. A stopped case has blank aggregate measurements. The
original base table remains immutable; use `final-results.csv`,
`final-results.raw.csv` and `final-finished.json` for the combined final outcomes.
No recomputation fallback is permitted by this path.

## Follow-up with programmatic memory termination disabled

For an explicitly requested unrestricted follow-up, the actual-attempt runner
accepts `--disable-memory-guard`. Its policy file must also set
`memory_guard_enabled=false`; mismatched policy/arguments are rejected. This
removes the pre-start available-memory check and the live low-available-memory
termination branch. Memory observations and explicit policy revocation remain.
The estimate ceiling is bypassed for the two selected cases as before; retained
mode and the pinned prover binary stay the same.

Record this follow-up in a separate evidence root. Earlier guard-stopped samples
remain historical data. The runner records process exit status, system memory,
swap/OOM counters and observed memory traces. With the guard disabled it attempts
both cases once even if one process fails, leaving incomplete aggregate metrics
blank. A nonzero process exit is recorded as `runtime_failed`; identify its actual
cause from the resource/kernel evidence rather than assuming it was an OOM.

```sh
python3 scripts/attempt_retained_extremes.py --root /path/to/new-followup \
  --estimate-bypass-gib 1024 --disable-memory-guard
```

This option changes the worker's code-level termination policy; it does not change
host RAM, swap, Linux overcommit, cgroup limits or the operating system's response
to allocation failure. All such settings should be captured with the run evidence.

Each new supplemental attempt now writes an immutable `execution-contract.json`
before launch: binary hash, full command, guard policy, repetition count and
accountable directive. A completed attempt is reused only when that contract,
its original command and status guard setting match. Legacy statuses without a
contract, changed contracts and incomplete directories are rejected before any
command evidence is overwritten; preserve those directories and use a new one.

The analyzer accepts `measured`, `memory_guard_stopped` and `runtime_failed` as
distinct states. It requires nonzero exit evidence and blank incomplete metrics
for failures. Resource signals, stop reasons and available global OOM-counter
changes are carried into the report without treating a signal as OOM proof.
`--validate-only` checks the evidence without generating figures or rewriting it.
Regression checks run with `python3 -m unittest discover -s scripts -p 'test_*.py'`;
the failure-classification fixture is explicitly synthetic and never published as
an experimental result.

## Isolated first-tree rebuild profile

`examples/recompute_overhead.rs` uses the existing `OpenOptions` switch to compare
retention and first-tree rebuilding. Both profiles keep the same two-direction
Setup tables alive and use one common commitment followed by one warmup and three
openings. Setup and commit are outside the open timer; no diagonal conversion is
performed. Every opening is verified and its commitment/proof/post-challenge
fingerprint must agree across repetitions and strategies.

`scripts/profile_recompute_overhead.py` runs M127 mu24/26, r2/c4, QA and RS, 32
threads, in independent sequential processes with counterbalanced strategy order.
It reports descriptive opening-time deltas, sample standard deviations and
process RSS. These fitting-size observations do not establish the exact overhead
at mu30, where the full-retention baseline failed. The comparison measures the
net effect of the rebuilding strategy, including its changed working set.

The completed [paired measurements and raw evidence](../experiments/recompute-overhead-2026-11-06/REPORT.md)
contain eight processes, 24 raw samples and four matching proof/transcript fingerprints.

## Queried-row recovery with constructor controls

The finalized supplement is
`/home/zjl/zkp-research/notes/impl-plan-hyperfold-query-row-recovery.md`.
`FirstTreeStrategy` selects native retention (A), prehashed retention (B), or
prehashed queried-row recovery (C); the legacy rebuilding option remains supported.
B/C share the canonical parallel leaf hashing function and the pinned Plonky3
`new_over_leaf_digests` constructor. C's tree owns only a borrowed row descriptor,
not the dense first intermediate codeword. The dense matrix is released once its
successor is folded, before the successor's tree is committed (immediately after
first-tree construction at depth two; no operation at depth one). Queries use
native `open_multi_batch` and the existing wire format and omission rules.

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --example query_row_overhead
python3 scripts/profile_query_rows.py --binary target/release/examples/query_row_overhead \
  --output-dir /tmp/hyperfold-query-rows-new-empty
```

The controller uses M127 mu26/r2/c4, QA/RS, 32 threads, both Setup directions,
and three rounds ordered ABC/BCA/CAB. Each strategy process reuses a common
commitment, executes one warmup and three measured openings, for 18 processes and
54 measurements. It preserves exact commands, exit status, resources and raw data.
A diagnostic smoke run can use `--mu 20 --rounds 1 --repetitions 1`; it is not the
specified mu26 measurement.

Opening totals include leaf hashing, row recovery, allocation/release and native
multiproof extraction. `first_tree_query_ms` is one coarse timing of the first tree's
query extraction plus cross-layer compression, including row recovery in C; it is
not pure recovery arithmetic and must not be subtracted from the opening total.
Verification and final proof encoding are outside open. Compare C−A for the overall
strategy, B−A for construction-entry changes, and C−B for storage/recovery with the
same constructor. Report the three independent process means and their variation,
not nine independent processes per strategy. These new diagnostics do not alter
the 142-case official baseline or establish an exact mu30 overhead.

A/B query the first tree before the remaining intermediate trees, while C queries
it after they are opened and released. The first-tree sub-timing therefore also
reflects query ordering and cache state. Use total open for the net strategy
comparison; the sub-timing is not an isolated row-fold arithmetic measurement.

The completed [18-process / 54-sample report](../experiments/query-rows-2026-11-06/REPORT.md)
contains every process mean and within-/across-process variation, constructor
contrasts, RSS, raw archive and an archive-only numeric analysis helper.

## Formal queried-row attempts at the two largest configurations

The v1.1 plan supplement (§9) fixes the full lifecycle: Setup once, one warmup
plus three measured repetitions, with a fresh commitment in every repetition.
The formal CLI accepts `--first-tree-strategy recovered` only with explicit
`--memory-mode retained` and without `--resume`. It retains both Setup directions,
never switches to whole-tree rebuilding, and appends
`first_tree_strategy=prehashed_recovered`, `diagonal_storage=retained_both` and a
commitment/proof/post-challenge fingerprint to the new mode's CSVs. Legacy default
CSV schemas remain unchanged. Repetitions must verify and agree on transcript and
fingerprint before raw data is written.

The existing estimator does not model queried-row storage: the new mode leaves
`estimated_gib` blank and bypasses estimate exclusions. Its peak RSS must be measured.
`attempt_query_extremes.py` copies a pinned binary into a new empty evidence
folder and runs QA/RS sequentially with no active available-memory termination.
It uses immutable command contracts and records RSS/HWM, MemAvailable, swap and
global OOM counters. Explicit cancellation is still honored through `policy.json`.
It does not alter swap/cgroup/kernel settings or the original 142-case data.

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --bin bench
python3 scripts/attempt_query_extremes.py --binary target/release/bench \
  --root /tmp/hyperfold-recovered-extremes-new-empty
```

For an inexpensive full-lifecycle smoke check, add `--mu 20`. Actual μ30 failures
retain all partial CSVs and real exit/resource records; aggregate timing/size fields
remain blank unless all three repetitions complete. `runtime_failed` does not
imply a kernel-confirmed per-process OOM. Query recovery is labeled separately from
native retention and prior whole-tree rebuilding results.

The completed [μ30 recovered-mode actual-attempt report](../experiments/mu30-recovered-2026-11-06/REPORT.md)
records two real exits137/signal9 and zero complete formal samples, with all
incomplete timing/size fields blank. Its archive-only helper separately validates
strategy, guard policy, real resources and blank outcomes without mixing the
original retained-mode data.

## Opening-only parameters and consumed message witnesses

The plan is `/home/zjl/zkp-research/notes/impl-plan-hyperfold-one-shot-lifetimes.md`.
After the final `commit`, `into_opening_only()` drops forward diagonal allocations
and returns `OpeningOnlyPcs`, which has no commit or prepare_commit API. Inverses,
base-code parameters, matrix and parameter ID remain unchanged. `CommitData::into_parts()`
moves its top tree and witness into `CommittedOracle` / `OwnedWitness`; it clones no
large arrays. `open_once` consumes only the witness, freeing it immediately after
its first complete lifted fold. The caller keeps the oracle through verification
and serialization. Original borrowed APIs continue supporting repeated opens.

`examples/one_shot_lifetimes.rs` controls the complete outer lifecycle: both
`--mode retained` and `--mode release` construct fresh Setup for every trial,
including warmup. They both use prehashed queried-row recovery. Release mode
additionally drops forward storage and consumes the witness. The retained control
keeps both Setup directions and the original witness through the same outer
cleanup point. Each process has one warmup and three measured trials.

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --example one_shot_lifetimes
python3 scripts/run_one_shot_lifetimes.py --binary target/release/examples/one_shot_lifetimes \
  --root /tmp/one-shot-pilot-new-empty
```

The default mu26 QA/RS pilot alternates AB/BA over two independent process rounds:
eight processes, 24 measured trials. Do not treat the three trials within each
process as three independent processes. A small smoke run can use `--mu 20 --rounds 1`.
The explicitly selected largest follow-up uses `--mu 30 --modes release --rounds 1
--continue-on-failure`, only after checking the pilot and implementation.

Raw columns separately report Setup, input/claimed evaluation, commit, forward
release, open, verify, serialization, diagnostics, cleanup and total wall. Wall
includes all those phases but excludes CSV output. `online_total_ms` is explicitly
commit+forward_release+open+verify. Witness release and first-tree query sub-timings
are already included in open; do not add them to wall again. Every trial verifies,
checks P/V post-challenges, fingerprints commitment/proof/challenge and fully drops
its caller-owned oracle/parameters before the next Setup. Capacity/release-round
columns record the actual ownership transition. The controller records peak RSS,
commands, contracts and exits with no estimate-only exclusions or active memory
threshold termination; explicit policy cancellation remains supported. All results
are separate from the prior official and diagnostic datasets.
