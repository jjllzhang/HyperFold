# HyperFold

Rust research prototype of the HyperFold multilinear polynomial commitment scheme (PCS), combining recursive folding, sumcheck, and Merkle commitments.

Supported configurations:

- **Base codes**: Quasi-Abelian (`QA`) and Reed-Solomon (`RS`)
- **Fields**: **Goldilocks** (`2^64 - 2^32 + 1`, cubic extension challenges) and **M127** (`2^127 - 1`, quadratic extension challenges)

> **Note**: This is experimental research software, not an audited or production-ready cryptographic library. The bundled parameter profiles target a 100-bit interactive IOP error budget under their certificate assumptions (Fiat–Shamir, hash, and zero-knowledge security are not certified).

## Repository layout

| Path | Description |
|---|---|
| `src/` | Core protocol (`pcs.rs`, `encoder.rs`, `base_code.rs`, `matrix_h.rs`, `sumcheck.rs`, `merkle_commitment.rs`, `field_m127.rs`, `params.rs`) and the `bench` CLI |
| `tests/` | PCS correctness and regression tests |
| `data/` | Parameter certificates (`*.csv`) and [readable parameter summary](data/parameter-profiles.md) |
| `scripts/` | Optional Plonky3 setup, parameter scripts, benchmark runner, and plotting |
| `results/` | Recorded 32-thread benchmark CSVs |
| `figures/` | Performance plots generated from `results/` |

## Setup and build

Requires **Rust 1.95+** (and **Python 3.11+** for helper scripts).

Cargo fetches [Plonky3](https://github.com/Plonky3/Plonky3) at the revision pinned in `Cargo.toml`. The first build needs network access.

### Build and test

```sh
RUSTFLAGS="-C target-cpu=native" cargo build --release --bin bench
RUSTFLAGS="-C target-cpu=native" cargo test --release -- --test-threads=1
```

Native x86-64 builds with AVX-512F/DQ/VL/IFMA use four-lane M127 packing; other targets use scalar M127 arithmetic.

Encoding writes into uninitialized output buffers in parallel, using a small reusable base-block scratch instead of clearing each full buffer. Base encoding is fused with the bottom encoding layers in subtrees of up to 512 KiB. Smaller inputs use smaller subtrees to keep enough parallel tasks for work stealing. The final layer writes the public row layout separately.

To use a clean local Plonky3 checkout at the pinned revision instead, run `python3 scripts/configure_plonky3.py /path/to/Plonky3`. This changes `Cargo.toml` to use local paths; Cargo will update the lockfile on the next build.

## Benchmarks

The bundled parameter set covers **144 configurations** across `mu` in `{20, 22, 24, 26, 28, 30}` (`2^mu` base-field elements), folding arity `r` in `{2, 4, 8}`, and rate `1/c` with `c` in `{2, 4}` (see [data/parameter-profiles.md](data/parameter-profiles.md)).

```sh
# List matching configurations
target/release/bench --list --fields goldilocks,m127 --backends qa,rs --mu 20 --r 8 --c 2

# Run benchmarks for a subset (uses 32 threads; large mu requires substantial RAM)
python3 scripts/benchmark_pcs.py \
  --fields goldilocks --backends qa --mu 20 --r 8 --c 2 \
  --output-dir ../hyperfold-results
```

Omit the filter flags (`--fields`, `--backends`, `--mu`, `--r`, `--c`) to run all 144 configurations.

Both benchmark commands default to `--matrix specialized` (fixed H₂, H₄ and H₈ circuits). Use `--matrix general` for symmetric Pascal matrices, `H[i,j] = binom(i+j,i)`. Matrix families have distinct PCS parameter IDs, so their proofs are not interchangeable; the library exposes this choice through `HyperFoldPcs::setup_with_matrix` and `matrix_h::MatrixKind`.

The Python runner writes general results to `OUTPUT_DIR/general/`; pass that directory as `--input-dir` when plotting them. Use a fresh `--output-dir` for new runs to avoid mixing measurements.

For RS encoding and thread scaling, the focused example reports raw samples for
base encoding, complete recursive encoding, and `commit` separately:

```sh
RUSTFLAGS="-C target-cpu=native" cargo build --release --example bench_encoding
target/release/examples/bench_encoding \
  --fields goldilocks --mu 20 --r 8 --c 2 --threads 32 --repetitions 5 > encoding.csv
```

It times each phase separately, runs one warm-up per phase and configuration, and
excludes Setup, input generation, witness copies, and output destruction from the
timers. Base encoding reuses its
output buffer; complete encoding includes allocation and the final layout change.
The CSV includes commitment roots so before/after runs can check identical output.
Use the same CPU affinity for both versions when comparing measurements.
Run each configuration and version in a fresh process to avoid allocator state
carrying over from other configurations. The filters also accept comma-separated
lists for exploratory sweeps.

The [encoding optimization measurements](results/encoding-optimization/summary.csv)
include median speedups and raw samples in the same directory.
[Metadata](results/encoding-optimization/metadata.json) records the baseline revision,
build flags, CPU affinity, timing scope, and harness adaptation for the older API.
These measurements compare `cf4b3e2` with `2bb0cc9`, before the integration of
PR #17's fused subtrees, uninitialized buffers, and M127 packing. They do not
measure the combined implementation.

## Results and figures

Recorded 32-thread measurements (AMD EPYC 9754; timings in `ms`, `proof_bytes` in bytes) are included in `results/`:

- [Goldilocks QA](results/goldilocks-qa.csv) / [Goldilocks RS](results/goldilocks-rs.csv)
- [M127 QA](results/m127-qa.csv) / [M127 RS](results/m127-rs.csv)

Regenerate plots from `results/`:

```sh
python3 -m pip install -r scripts/requirements-plot.txt
python3 scripts/plot_results.py --input-dir results --output-dir ../hyperfold-figures
```

| Field | `c=2` | `c=4` |
|---|---|---|
| Goldilocks | [PNG](figures/hyperfold-goldilocks-c2.png) / [PDF](figures/hyperfold-goldilocks-c2.pdf) | [PNG](figures/hyperfold-goldilocks-c4.png) / [PDF](figures/hyperfold-goldilocks-c4.pdf) |
| M127 | [PNG](figures/hyperfold-m127-c2.png) / [PDF](figures/hyperfold-m127-c2.pdf) | [PNG](figures/hyperfold-m127-c4.png) / [PDF](figures/hyperfold-m127-c4.pdf) |

## License

[MIT](LICENSE)
