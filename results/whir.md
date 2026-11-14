# WHIR Benchmark Results

Measurements of [WHIR](https://github.com/worldfnd/whir) for `k` (number of multilinear variables) from 20 to 30.

## Setup & Parameters

- **Hardware & Runtime**: AMD EPYC 9754, 503 GiB RAM, 32 Rayon threads
- **Field**: `Goldilocks3` (cubic extension of the 64-bit Goldilocks field `2^64 - 2^32 + 1`)
- **Hash / Transcript**: BLAKE3
- **Security Target**: 100 bits (configured security parameter; equivalence with parameter selection in other protocols has not been established)
- **Scheme Constants**: `evaluations = 1` (single evaluation point opened), `pow_bits_max = 20` (maximum Proof-of-Work grinding bits per round)

## Measurement Methodology

- **Sampling**: Medians of 3 measured runs per configuration. `Prover` is the median of the per-run `Commit + Open` sums, so it may differ from the sum of the displayed `Commit` and `Open` medians.
- **Precision Note**: Upstream outputs have `0.1 ms` resolution for `Commit`/`Open` at `k <= 24`, `0.1 s` resolution for `Commit`/`Open` at `k >= 26`, `0.1 ms` resolution for `Verify`, and `0.1 KiB` resolution for `Proof` (`1 KiB = 1024 bytes`). Values in the table are zero-padded to 3 decimal places solely for cross-protocol formatting consistency and do not imply additional measurement precision.
- **Verification**: All runs passed verification (`ok`).

## Results

`k` denotes the number of multilinear variables (`2^k` evaluations).

| k | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) |
|---:|---:|---:|---:|---:|---:|
| 20 | 0.049 | 0.228 | 0.278 | 2.900 | 137.900 |
| 22 | 0.176 | 0.705 | 0.882 | 1.700 | 157.100 |
| 24 | 0.587 | 2.300 | 2.887 | 1.900 | 184.300 |
| 26 | 2.200 | 8.900 | 11.100 | 2.100 | 202.000 |
| 28 | 8.800 | 36.000 | 44.800 | 2.200 | 226.600 |
| 30 | 36.900 | 148.200 | 185.300 | 2.600 | 246.400 |

## Scheme-Specific Parameters & Notes

- All benchmarked configurations share `evaluations = 1` and `pow_bits_max = 20` as noted in Setup & Parameters.
