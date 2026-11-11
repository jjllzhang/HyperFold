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
cargo test --release -- --test-threads=1
```

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
