# Brakedown Benchmark Results

Measurements of Brakedown from the [ReedWeave benchmark results](https://github.com/jjllzhang/ReedWeave/blob/master/results/Brakedown/goldilocks.csv), using the implementation in [lcpc](https://github.com/conroi/lcpc).

## Setup & Parameters

- **Hardware & Runtime**: AMD EPYC 9754, 503 GiB RAM, 32 Rayon threads
- **Field**: Goldilocks (`2^64 - 2^32 + 1`)
- **Hash / Transcript**: BLAKE3 Merkle hashing; Merlin Fiat–Shamir transcript
- **Security Target**: 100-bit configured security parameter
- **Scheme Constants**: `SdigCode3` (linear-time encodable sparse-matrix erasure code family) with nominal rate approximately `1 / 1.521` (exact rates vary slightly with integer matrix dimensions)

## Measurement Methodology

- **Sampling**: Reported from the 32-thread measurements of the ReedWeave/lcpc Brakedown benchmark suite.
- **Timing Scope**: `Commit` measures commitment generation, `Open` measures evaluation proof generation, `Prover` is `Commit + Open` computed at full upstream precision prior to rounding (so it may differ from the sum of displayed `Commit` and `Open` values by `0.001 s`), and `Verify` measures proof verification.
- **Proof Size**: Reported in KiB (`1 KiB = 1024 bytes`) to 3 decimal places, matching the upstream `lcpc` / ReedWeave reported opening proof size.

## Results

`k` denotes the number of multilinear variables (`2^k` polynomial coefficients).

| k | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) |
|---:|---:|---:|---:|---:|---:|
| 20 | 0.018 | 0.022 | 0.040 | 32.799 | 5631.969 |
| 21 | 0.033 | 0.031 | 0.065 | 45.381 | 6784.820 |
| 22 | 0.078 | 0.045 | 0.123 | 64.755 | 8165.203 |
| 23 | 0.153 | 0.079 | 0.232 | 91.617 | 10269.727 |
| 24 | 0.256 | 0.119 | 0.375 | 138.912 | 12990.219 |
| 25 | 0.476 | 0.154 | 0.631 | 188.935 | 17038.297 |
| 26 | 0.917 | 0.224 | 1.141 | 270.715 | 22479.312 |
| 27 | 1.821 | 0.335 | 2.156 | 383.316 | 30374.258 |
| 28 | 3.477 | 0.474 | 3.951 | 536.740 | 41256.258 |
| 29 | 8.176 | 0.769 | 8.944 | 808.821 | 56844.969 |
| 30 | 18.273 | 1.258 | 19.531 | 1096.645 | 78568.727 |
| 31 | 43.529 | 2.172 | 45.701 | 1587.300 | 109544.938 |
| 32 | 114.399 | 4.201 | 118.600 | 2340.449 | 153032.695 |
| 33 | 282.968 | 9.253 | 292.221 | 3380.838 | 214743.664 |
| 34 | 630.463 | 22.545 | 653.008 | 4781.692 | 301759.391 |

## Scheme-Specific Parameters & Notes

Exact `SdigCode3` outer-code rates for each `k`, where `Rate (exact)` is `message_length / codeword_length` (`k0 / n0`) determined by integer matrix dimensions and `Rate (decimal)` is its floating-point value:

| k | Rate (exact) | Rate (decimal) |
|---:|---:|---:|
| 20 | `42432/64540` | 0.657453 |
| 21 | `60007/91271` | 0.657460 |
| 22 | `84863/129077` | 0.657460 |
| 23 | `120014/182542` | 0.657460 |
| 24 | `169725/258152` | 0.657461 |
| 25 | `240027/365082` | 0.657461 |
| 26 | `339450/516304` | 0.657461 |
| 27 | `480054/730163` | 0.657461 |
| 28 | `678899/1032606` | 0.657462 |
| 29 | `960108/1460325` | 0.657462 |
| 30 | `1357798/2065211` | 0.657462 |
| 31 | `1920216/2920649` | 0.657462 |
| 32 | `2715596/4130422` | 0.657462 |
| 33 | `3840432/5841298` | 0.657462 |
| 34 | `5431191/8260842` | 0.657462 |
