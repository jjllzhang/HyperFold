# Uniform one-shot experiments

The official CLI and Criterion share `benchmark::run_trial`. Every warmup and
measured trial constructs fresh Setup and inputs, commits, drops forward
allocations, consumes the original witness after its first complete lifted
Sumcheck subround, retains first-tree hashes and recovers queried rows, verifies,
serializes, then destroys the caller-owned top oracle and remaining parameters.
The already computed inverse table is retained. No table switching or whole-tree
reconstruction is selected by the CLI.

## Commands

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --locked --release --bin bench
./target/release/bench --list
# Four small certified cases; one warmup plus one measured trial, functional smoke.
python3 scripts/run_unified.py --binary target/release/bench \
  --output-dir bench-results/smoke-new --fields goldilocks,m127 --mu 20 \
  --r 2 --c 4 --backends qa,rs --smoke --repetitions 1
python3 scripts/analyze_unified.py --root bench-results/smoke-new \
  --output bench-results/smoke-report-new
python3 scripts/analyze_unified.py \
  --archive bench-results/smoke-report-new/raw-evidence.tar.gz \
  --output bench-results/smoke-rebuilt-new
```

The two numerical CSVs reproduce from the archive without a prover executable.
The runner takes explicit selections (`--fields`, `--backends`, `--mu`, `--r`,
`--c`), defaults to three measured trials plus one warmup, and runs one separate
process per case and independent round. `--mu 20..=30` selects even certified
sizes; `--mu 20,24` selects a subset. Goldilocks always uses cubic challenges and
M127 quadratic challenges. The executable's `--list` selects all144 certified
rows by default and allocates no Setup. Listing a row is not running it.

Only32-thread timings are accepted. One-thread support is exercised in functional
library tests. The controller chooses32 distinct allowed physical cores and
applies affinity to itself/descendants only; it does not reserve the host or
change NUMA/swap/kernel/governor settings. It records actual affinity, hardware,
compiler, source/binary/lock/parameter hashes, commands, memory observations and
GNU time peak RSS. Do not run competing performance loads.

Every evidence directory and summary/raw destination must be new. `--resume`,
`--quick`, `--memory-mode`, `--max-estimated-gib` and `--first-tree-strategy` are
removed from the official CLI; they are rejected rather than mapped silently.
There is no estimate exclusion, active memory threshold or automatic fallback.
The explicit policy file can cancel the owned workload. Failed/incomplete cases
retain commands, partial samples and actual resource/exit records; the controller
leaves their aggregate time/size blank. A signal alone is not PID-specific OOM
proof. `--continue-on-failure` explicitly permits later cases to be attempted.

## Timing and size contract

CSV schema `hyperfold-one-shot-bench-v1` is distinct from all historical schemas.
Setup seed is `[42;32]`, message seed `[27;32]`, point seed `[28;32]`, with the
existing role/mu XOF streams. Input includes evaluation of the claimed value.
There is no timed whole-input clone. Setup variation across multiple seeds is
not measured by this fixed-seed experiment.

- `commit_ms`: encoding/top tree, commitment point sampling and y0 evaluation.
- `forward_release_ms`: transition to opening-only parameters; no inversion.
- `open_ms`: all sumcheck/folds/intermediate commitments, witness release,
  queried-row recovery, multiproof extraction and cross-layer compression.
- `verify_ms`: transcript/round/terminal/encoding/fold/authentication checks.
- `serialization_ms`, `diagnostics_ms`, `cleanup_ms`: wire encoding, fingerprints
  and P/V extra-challenge agreement, and final caller-owned storage cleanup.
- `wall_ms`: fresh Setup/input plus all above; CSV output is outside this clock.
- `online_total_ms = commit + forward_release + open + verify`.

`witness_release_ms` and `first_tree_query_ms` are already inside open. The latter
includes recovery, path extraction and compression, and is not pure arithmetic.
Do not add these sub-times to total a second time. Size includes commitment and
evaluation-proof canonical bytes, including framing. Every warmup/trial verifies
and checks an additional P/V challenge. Raw rows flush per completed trial;
means and sample SD are written only after all requested trials complete.

Peak RSS spans the whole process, including Setup and warmup. Across independent
processes report peak mean and maximum separately. Repeats inside a process do
not increase the number of independent processes.

Criterion (`cargo bench --bench pcs`) uses the same full helper lifecycle,32
threads and certified Goldilocks QA/mu20/r8/c2/e3. Its sole full-cycle measurement
includes Setup, input, commit, releases, open, verify, wire and cleanup. Phase
metrics come from the CLI, not old cached-open Criterion labels. Preserve raw
Criterion estimates/samples when using it; no new Criterion performance run is
implied by a successful compile.

## History and comparisons

Historical data and portable analyzers remain immutable under `experiments/`.
Old commands/source can be restored at commit7bd0724 or the source commit in the
particular manifest; they are not active memory choices in the current CLI.
Do not combine cached-Setup/native-retained results with fresh-Setup one-shot
rows to infer arithmetic speedup.

- [Largest two one-shot successes](../experiments/one-shot-mu30-2026-11-06/REPORT.md)
- [Native M127 backend comparison](../experiments/m127-pcs-backends-2026-11-06/FINDINGS.md)
- [Old retained failures and142 completed rows](../experiments/retained-unguarded-2026-11-06/REPORT.md)
- [Old constructor controls](../experiments/query-rows-2026-11-06/REPORT.md)

The four-point native backend study keeps the custom arithmetic frozen and has
its own pinned binary/source, native ark/FourQ adapters and statistics. Its current
custom backend has the lowest total mean at each selected point; that does not
imply every stage has a stable winner or characterize untested ASM/fused paths.
This cleanup does not schedule a new72-case performance sweep or relabel old
largest-case observations as measurements of a newly compiled binary.
