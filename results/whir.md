# WHIR Benchmark Results

Measurements of [WHIR](https://github.com/worldfnd/whir) for `k` (number of multilinear variables) from 20 to 30.

## Setup & Parameters

- **Hardware & Runtime**: AMD EPYC 9754, 503 GiB RAM, 32 Rayon threads
- **Field**: `Goldilocks3` (cubic extension of the 64-bit Goldilocks field `2^64 - 2^32 + 1`)
- **Hash / Transcript**: BLAKE3
- **Security Target**: `100` bits is the parameter generator's target for the protocol's round-by-round soundness components (including initial, per-round, and final checks), not a certified bound of `2^-100` on the full protocol's soundness error. For these Goldilocks3 runs, the implementation reported a minimum per-component security level of `100.00` bits under its parameter-analysis model, including PoW where applicable. This is not a sum of all failure probabilities, and equivalence with other protocols' security targets has not been established.
- **Scheme Constants**: Initial Reed–Solomon code rate `1/2` (default `rate = 1`, specifying the inverse rate as `2^rate`); the code rates in later folding rounds are not fixed at `1/2`. Initial and subsequent folding factors are `4` variables each (`initfold = 4`, `fold = 4`); decoding regime is `Johnson`. `evaluations = 1` (single evaluation point opened), `pow_bits_max = 20` (configured PoW budget used in parameter selection).

## Measurement Methodology

- **Sampling**: Medians of 3 independent runs per configuration, each generating a new commitment and opening proof and verifying it once. `Prover` is the median of the per-run `Commit + Open` sums, so it may differ from the sum of the displayed `Commit` and `Open` medians.
- **Timing Scope**: `Commit` measures commitment generation; `Open` measures generation of the single-point evaluation-opening proof after commitment; `Prover` is their per-run sum. `Verify` measures verification of that proof (one verification per run). Compilation is not included. The available aggregate data do not establish whether other setup or input-generation work is included in each timer.
- **Proof Size**: `Proof` is the upstream-reported proof size in KiB (`1 KiB = 1024 bytes`), with the median of three independent runs reported. The available results do not establish whether it includes the initial commitment; do not assume this column has the same inclusion convention as other schemes.
- **Precision Note**: Upstream outputs have `0.1 ms` resolution for `Commit`/`Open` at `k <= 24`, `0.1 s` resolution for `Commit`/`Open` at `k >= 26`, `0.1 ms` resolution for `Verify`, and `0.1 KiB` resolution for `Proof` (`1 KiB = 1024 bytes`). Values in the table are zero-padded to 3 decimal places solely for cross-protocol formatting consistency and do not imply additional measurement precision.
- **Verification**: All runs passed verification (`ok`).

## Results

`k` denotes the number of multilinear variables (`2^k` evaluations).

| k | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) |
|---:|---:|---:|---:|---:|---:|
| 20 | 0.049 | 0.228 | 0.278 | 1.800 | 137.900 |
| 22 | 0.176 | 0.705 | 0.882 | 1.700 | 157.100 |
| 24 | 0.587 | 2.300 | 2.887 | 1.900 | 184.300 |
| 26 | 2.200 | 8.900 | 11.100 | 2.100 | 202.000 |
| 28 | 8.800 | 36.000 | 44.800 | 2.200 | 226.600 |
| 30 | 36.900 | 148.200 | 185.300 | 2.600 | 246.400 |

## Scheme-Specific Parameters & Notes

- All benchmarked configurations share the initial `1/2` RS rate, folding and decoding settings, `evaluations = 1`, and `pow_bits_max = 20` as noted in Setup & Parameters. Later-round rates depend on the folding stage.
