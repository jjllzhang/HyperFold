# BrakingBase Benchmark Results

Measurements of [BrakingBase](https://github.com/arithmic/BrakingBase) for `k` (number of multilinear variables) from 20 to 30.

## Setup & Parameters

- **Hardware & Runtime**: AMD EPYC 9754, 503 GiB RAM, 32 Rayon threads
- **Field**: Goldilocks (`2^64 - 2^32 + 1`)
- **Hash / Transcript**: BLAKE2s-256
- **Security Target**: 100 bits

## Measurement Methodology

- **Sampling**: 1 setup, followed by 1 warmup and 3 measured samples; `Commit`, `Open`, and `Verify` timings are reported **arithmetic means**.
- **Timing Scope**: `Setup` (encoding matrix and parameter precomputation) and `Trim` (instance-size matrix trimming/preprocessing) are reported separately ahead of `Commit`. `Prover` is `Commit + Open` (mean) computed at full precision prior to rounding.
- **Proof Size**: Reported in KiB (`1 KiB = 1024 bytes`) rounded to 3 decimal places, converted from the total proof byte size reported by the benchmark.
- **Missing Values**: `—` denotes an unreported metric. The run at `k = 30` failed due to insufficient memory.

## Results

`k` denotes the number of multilinear variables (`2^k` polynomial evaluations).

| k | Setup (s) | Trim (s) | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 1.302 | 0.204 | 0.021 | 0.947 | 0.967 | 36.835 | 8988.609 |
| 22 | 5.036 | 0.905 | 0.084 | 3.091 | 3.175 | 33.941 | 10360.859 |
| 24 | 19.083 | 3.641 | 0.383 | 11.536 | 11.919 | 35.452 | 12051.078 |
| 26 | 79.702 | 16.390 | 2.010 | 48.262 | 50.272 | 42.175 | 14168.859 |
| 28 | 331.614 | 80.222 | 10.486 | 191.920 | 202.406 | 117.737 | 16837.766 |
| 30 | — | — | — | — | — | — | — |

## Scheme-Specific Parameters & Notes

- No per-`k` coding or query-count parameters were separately reported for BrakingBase; `k = 30` failed with out-of-memory on the 503 GiB host.
