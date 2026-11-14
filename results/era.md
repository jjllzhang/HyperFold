# ERA Benchmark Results

Measurements of ERA using the [anonymous artifact provided with the paper](https://anonymous.4open.science/r/agnostir-F3B4/).

## Setup & Parameters

- **Hardware & Runtime**: AMD EPYC 9754, 503 GiB RAM, 32 threads
- **Field**: secp256k1 scalar field
- **Hash / Transcript**: BLAKE3-256
- **Security Target**: Conditional 100-bit

## Measurement Methodology

- **Sampling**: 1 warmup followed by 3 measured samples; `Commit`, `Open`, and `Verify` timings are **arithmetic means**.
- **Timing Scope**: `Setup` (parameter/encoding precomputation) is reported separately ahead of `Commit`. `Open` measures the evaluation-opening proof generation phase. `Prover` is `Commit + Open` (mean) computed at full precision prior to rounding.
- **Proof Size**: Reported in KiB (`1 KiB = 1024 bytes`) rounded to 3 decimal places; **excludes** the commitment (fixed at 32 bytes in every configuration).
- **Verification**: All measured samples passed verification. `Peak RSS` is process peak resident memory in GiB.

## Results

`k` denotes the number of multilinear variables (`2^k` field elements).

| k | Setup (s) | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) | Peak RSS (GiB) |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.967 | 0.115 | 0.144 | 0.259 | 61.429 | 2662.914 | 0.56 |
| 22 | 0.968 | 0.231 | 0.364 | 0.596 | 71.602 | 3916.820 | 1.28 |
| 24 | 0.970 | 0.787 | 1.256 | 2.043 | 104.658 | 8932.633 | 4.13 |
| 26 | 0.968 | 2.895 | 4.415 | 7.309 | 251.375 | 28996.664 | 15.03 |
| 28 | 4.500 | 15.458 | 16.617 | 32.075 | 1167.638 | 39591.195 | 58.80 |
| 30 | 4.506 | 59.970 | 65.636 | 125.606 | 1337.658 | 59174.289 | 231.73 |

## Scheme-Specific Parameters & Notes

Per-`k` protocol parameters:
- `row_log2`: `log2` of the row length in the 2D polynomial matrix layout (`2^row_log2` elements per row).
- `rows`: number of rows in the 2D polynomial matrix.
- `eta`: ERA protocol batching/folding parameter for the chosen matrix shape.
- `Assumed Distance`: assumed relative minimum distance of the underlying code.
- `Queries`: number of Merkle/codeword queries sampled by the verifier.
- `Commitment (B)`: commitment size in bytes (32-byte Merkle root, excluded from `Proof (KiB)`).

| k | row_log2 | rows | eta | Assumed Distance | Queries | Commitment (B) |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 16 | 16 | 4 | 0.22037 | 836 | 32 |
| 22 | 16 | 64 | 6 | 0.22037 | 836 | 32 |
| 24 | 16 | 256 | 8 | 0.22037 | 836 | 32 |
| 26 | 16 | 1024 | 10 | 0.22037 | 836 | 32 |
| 28 | 20 | 256 | 8 | 0.225055 | 816 | 32 |
| 30 | 20 | 1024 | 10 | 0.225055 | 816 | 32 |
