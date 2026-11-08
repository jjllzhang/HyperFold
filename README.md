# HyperFold

Rust research implementation of a multilinear polynomial commitment scheme with recursive folding, sumcheck and Merkle commitments:

- **QA**: a quasi-abelian base-code backend.
- **RS**: a Reed-Solomon base-code backend.

Both backends support **Goldilocks** (`2^64 - 2^32 + 1`) with cubic extension-field challenges and **M127** (`2^127 - 1`) with quadratic extension-field challenges.

This is experimental research software, not an audited or production-ready cryptographic library. The bundled v3 parameter profiles target a **100-bit IOP error budget** under their certificate assumptions. Parameter checks, tests and measurements do not establish full PCS, Fiat-Shamir, hash or zero-knowledge security.

## Code layout

| Path | Purpose |
|---|---|
| `src/pcs.rs` | Setup, commitment, one-shot opening and verification |
| `src/{base_code,matrix_h,encoder}.rs` | Base codes, local mixing matrices and recursive encoding |
| `src/{sumcheck,transcript,merkle_commitment,codec}.rs` | Sumcheck, challenges, Merkle commitments and serialization |
| `src/field_m127.rs` | M127 arithmetic and its quadratic extension |
| `src/{params,certified_params}.rs` | Certified configurations and parameter validation |
| `src/{bin/bench,benchmark_trial}.rs` | Native benchmark CLI and the shared fresh-Setup trial lifecycle |
| `benches/pcs.rs` | Criterion harness, separate from the four-table measurement workflow |
| `tests/pcs.rs` | PCS regression tests, including the consolidated golden module |
| `tests/fixtures/native-retained-golden.csv` | Fixed transcript/proof fingerprints |
| `scripts/` | Dependency configuration, parameter import, benchmark orchestration and plotting |
| `data/` | Goldilocks and M127 parameter certificates |
| `results/` | Four measured field/backend result tables |
| `figures/` | PNG and PDF figures generated from those tables |

## Build and test

Use **Rust 1.95+** and **Python 3.11+**. The checked toolchain is Rust 1.95.0 and Python 3.12.9 on Linux x86-64. Run the commands below from the HyperFold repository root.

### 1. Configure the pinned Plonky3 dependency

`Cargo.lock` pins registry dependencies, but this repository also uses **local Plonky3 path dependencies**. Obtain a clean checkout of [Plonky3](https://github.com/Plonky3/Plonky3) at the required commit:

```sh
git clone https://github.com/Plonky3/Plonky3.git ../Plonky3
git -C ../Plonky3 checkout --detach 0d803737b681774186cd283b1fd2d8e493655c82
python3 scripts/configure_plonky3.py ../Plonky3
```

If you already have that clean, pinned checkout, pass its path to `configure_plonky3.py` instead. The helper checks the revision and cleanliness, then rewrites the local dependency paths in `Cargo.toml`; it does not fetch or modify Plonky3. Do not commit machine-specific path changes.

### 2. Build the benchmark binary and run correctness checks

```sh
export RUSTFLAGS="-C target-cpu=native"
cargo build --release --locked --bin bench
cargo test --release --locked -- --test-threads=1
```

The release profile uses optimization level 3, ThinLTO and one codegen unit. `target-cpu=native` targets the build host, so rebuild on the measurement machine and record the compiler and flags. The correctness tests do not run the full performance grid; golden tests cover both one-thread and 32-thread execution.

Install plotting dependencies, preferably in an isolated environment:

```sh
python3 -m venv ../hyperfold-venv
. ../hyperfold-venv/bin/activate
python3 -m pip install -r scripts/requirements-plot.txt
```

The **complete 31-test Python suite also requires Linux and at least 32 distinct physical cores in the allowed CPU set**, because it exercises the runner's CPU-affinity checks. Its synthetic CLI and temporary fixtures do not measure PCS performance or replace the bundled result tables:

```sh
python3 -m unittest discover -s scripts -p 'test_*.py' -v
```

On a machine with fewer allowed physical cores, run only the plotting tests instead. Passing this subset is **not** a pass of the complete 31-test suite:

```sh
python3 -m unittest discover -s scripts -p 'test_plot_results.py' -v
```

Plotting is pinned to **Matplotlib 3.10.6**.

## Run HyperFold benchmarks

The supplied profiles cover `mu=20,22,24,26,28,30`, `r=2,4,8` and `c=2,4`, for both fields and both backends: **144 configurations**. The input has `2^mu` base-field elements, `r` is the folding arity and `1/c` is the code rate. Base size `mu0` and query counts come from the selected certificate, not independent CLI tuning knobs.

The HyperFold performance workflow requires **32 threads**. The Python runner requires Linux and at least **32 distinct physical cores in the allowed CPU set**, then restricts process affinity to one logical CPU per selected core. This is not exclusive access to the host or per-worker pinning. NUMA placement is inherited.

### 1. List parameters without running measurements

`--list` does not allocate Setup, generate proofs or write measurements. This example selects four configurations:

```sh
target/release/bench --list --threads 32 \
  --fields goldilocks,m127 --backends qa,rs --mu 20 --r 8 --c 2
```

Omit the field/backend/size filters to list all 144 certified configurations. Use `--help` on the binary or scripts for their supported options.

### 2. Run a single configuration

Use the orchestration script to preserve raw samples and validate a complete configuration before updating its result row:

```sh
python3 scripts/benchmark_pcs.py \
  --binary target/release/bench \
  --fields goldilocks --backends qa --mu 20 --r 8 --c 2 \
  --repetitions 3 --rounds 1 --role protocol_performance \
  --output-dir ../hyperfold-results-one \
  --raw-dir ../hyperfold-raw-one
```

**Choose fresh output locations. Do not create `--raw-dir` beforehand:** it must be absent. Raw evidence includes the copied executable and its SHA-256, selection, sample CSVs, process logs and event records. Keep a separate record of the source revision, build command, toolchain and host; the executable hash alone does not identify its build provenance.

`--output-dir` defaults to the bundled `results/` directory and replaces matching rows while retaining other existing rows. Always specify a separate directory for new measurements. Reusing a result directory can mix experiment batches even though the raw directory is new.

### 3. Run the complete grid

This launches 144 measurement processes, each with one warmup and three measured trials. It is a **high-resource run**, not a quick test: the largest configurations can require hundreds of GiB of RAM. Check available memory, system load and CPU placement first. The runner does not impose a process memory limit or certify a dedicated machine.

```sh
python3 scripts/benchmark_pcs.py \
  --binary target/release/bench \
  --fields goldilocks,m127 --backends qa,rs --mu '20..=30' \
  --r 2,4,8 --c 2,4 \
  --repetitions 3 --rounds 1 --role protocol_performance \
  --output-dir ../hyperfold-results-full-new \
  --raw-dir ../hyperfold-raw-full-new
```

With `--rounds 1`, the three measured trials are **repetitions within one process**, not three independent-process measurements. Each trial uses fresh Setup and a one-shot opening lifecycle. `--rounds` can request additional independent processes; all measured samples contribute to the resulting mean. Failed configurations do not produce completed result rows. Do not fill missing new results with old measurements or select only the fastest repetitions.

## Recorded results

The four bundled CSVs each contain 36 configurations from the unified 144-configuration run, using one warmup and three same-process measured trials per configuration:

- [Goldilocks QA](results/goldilocks-qa.csv) and [Goldilocks RS](results/goldilocks-rs.csv)
- [M127 QA](results/m127-qa.csv) and [M127 RS](results/m127-rs.csv)

The run used 32 workers on an AMD EPYC 9754 host with native release code. The host was not certified dedicated. The table schema is:

```text
mu,r,c,commit_ms,open_ms,verify_ms,proof_bytes
```

Times are in **milliseconds**. `proof_bytes` is in **bytes**, and sums the serialized commitment and evaluation proof. Commit/Open/Verify columns exclude Setup, input generation, the separate opening-only transition, serialization and cleanup. Raw records contain these other phases; the four-table schema is not an end-to-end wall-time table. Verification timing excludes the subsequent serialization step.

Matching `mu/r/c` across fields does not mean identical work: extension degrees, query counts and RS encoder implementations differ. The tables are measured results for these profiles, not a universal backend or field speed ranking, an independent-process stability study or a cryptographic security proof.

## Plot results

Regenerate the four field-by-`c` comparisons from the complete bundled tables, without running benchmarks:

```sh
python3 scripts/plot_results.py \
  --input-dir results --output-dir ../hyperfold-figures-new
```

The script validates all four complete grids and writes **four PNGs and four PDFs**. It plots time in ms and proof size in **KiB** (`bytes / 1024`). A single-configuration result directory is not a complete plotting input. `--output-dir` defaults to the bundled `figures/`, so use an alternate directory to keep the shipped plots unchanged.

| Field | `c=2` | `c=4` |
|---|---|---|
| Goldilocks | [PNG](figures/hyperfold-goldilocks-c2.png) / [PDF](figures/hyperfold-goldilocks-c2.pdf) | [PNG](figures/hyperfold-goldilocks-c4.png) / [PDF](figures/hyperfold-goldilocks-c4.pdf) |
| M127 | [PNG](figures/hyperfold-m127-c2.png) / [PDF](figures/hyperfold-m127-c2.pdf) | [PNG](figures/hyperfold-m127-c4.png) / [PDF](figures/hyperfold-m127-c4.pdf) |
