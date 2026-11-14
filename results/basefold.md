# BaseFold Benchmark Results

Measurements of an independently optimized version of [plonkish_basefold](https://github.com/hadasz/plonkish_basefold) using BLAKE3 Merkle hashing, shared Merkle multiproofs, and native release optimization.

## Setup & Parameters

- **Hardware & Runtime**: AMD EPYC 9754, 503 GiB RAM, 32 physical cores pinned to CPUs 0–31; optimized release build (`target-cpu=native`, thin LTO, single codegen unit)
- **Field & Challenges**: Goldilocks base field (`2^64 - 2^32 + 1`) with cubic extension challenges, or M127 base field (`2^127 - 1`) with quadratic extension challenges
- **Hash / Transcript**: BLAKE3 Merkle hashing with shared Merkle multiproofs
- **Security Target**: Conditional ordinary knowledge soundness (`gamma = 2^-24`, query-error budget `2^-102`)
- **Scheme Constants**: Code rate `1/2`; base-code message length `64`, codeword length `128`. `RS` (Reed–Solomon) has minimum distance `65`; `QA` (Quasi-Abelian) uses the systematic code `(m, H diag(a / 64) H m)` where `H` is the 64×64 Walsh–Hadamard matrix and `diag(a / 64)` uses random nonzero base-field diagonal entries

## Measurement Methodology

- **Sampling**: 1 warmup followed by 3 measured samples; reported timings and proof sizes are **medians**. `Prover` is the median of each sample's `Commit + Open` sum, so it may differ slightly from the sum of the displayed `Commit` and `Open` medians.
- **Timing Scope**: Setup, input generation, and external evaluation are excluded; transcript writing is included in `Open`. `Verify` measures proof verification against the commitment. `Peak RSS` is process peak resident memory, not memory consumption for the timed prover stage alone.
- **Proof Size**: Reported in KiB (`1 KiB = 1024 bytes`) rounded to 3 decimal places; **includes** the commitment.
- **Verification & Missing Values**: Every measured sample passed verification. `—` denotes an unmeasured metric. `2^30` benchmarks were not run (linear extrapolation from `2^28` gives ~600 GiB for Goldilocks and ~800 GiB for M127, exceeding the machine's 503 GiB RAM); QA odd exponents and QA `2^29` were not requested or measured.

## Results

`k` denotes the number of multilinear variables (`2^k` base-field polynomial evaluations).

### RS — Goldilocks (cubic extension)

| k | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) | Peak RSS (GiB) |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.042 | 0.151 | 0.193 | 25.823 | 1787.219 | 0.58 |
| 21 | 0.077 | 0.312 | 0.383 | 33.732 | 2199.758 | 1.16 |
| 22 | 0.135 | 0.532 | 0.667 | 40.363 | 2674.250 | 2.33 |
| 23 | 0.384 | 0.978 | 1.362 | 42.256 | 3212.289 | 4.68 |
| 24 | 0.734 | 2.030 | 2.764 | 50.497 | 3865.328 | 9.38 |
| 25 | 1.451 | 4.060 | 5.511 | 60.594 | 4599.195 | 18.74 |
| 26 | 2.946 | 7.952 | 10.900 | 67.712 | 5437.016 | 37.49 |
| 27 | 6.085 | 15.881 | 21.996 | 83.342 | 6415.086 | 75.00 |
| 28 | 12.778 | 34.676 | 47.353 | 94.499 | 7542.438 | 150.00 |
| 29 | 27.333 | 69.081 | 96.379 | 176.721 | 8799.805 | 300.00 |
| 30 | — | — | — | — | — | — |

### RS — M127 (quadratic extension)

| k | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) | Peak RSS (GiB) |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.086 | 0.168 | 0.254 | 18.409 | 1428.094 | 0.78 |
| 21 | 0.162 | 0.341 | 0.514 | 20.776 | 1693.719 | 1.55 |
| 22 | 0.435 | 0.661 | 1.098 | 24.140 | 1984.469 | 3.11 |
| 23 | 0.857 | 1.242 | 2.099 | 28.619 | 2322.219 | 6.24 |
| 24 | 1.674 | 2.439 | 4.108 | 31.381 | 2669.000 | 12.49 |
| 25 | 3.338 | 4.826 | 8.164 | 35.989 | 3075.938 | 25.03 |
| 26 | 6.802 | 9.549 | 16.342 | 42.943 | 3519.156 | 49.99 |
| 27 | 13.955 | 18.789 | 32.752 | 55.598 | 3990.375 | 99.99 |
| 28 | 29.663 | 40.695 | 70.328 | 62.546 | 4501.375 | 199.99 |
| 29 | 60.368 | 83.313 | 143.901 | 94.143 | 5112.969 | 399.99 |
| 30 | — | — | — | — | — | — |

### QA — Goldilocks (cubic extension)

| k | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) | Peak RSS (GiB) |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.044 | 0.159 | 0.205 | 44.222 | 2942.750 | 0.57 |
| 22 | 0.169 | 0.506 | 0.684 | 70.859 | 4575.344 | 2.33 |
| 24 | 0.726 | 2.163 | 2.886 | 96.878 | 6845.469 | 9.37 |
| 26 | 2.874 | 7.975 | 10.859 | 136.642 | 10039.453 | 37.50 |
| 28 | 12.673 | 34.725 | 47.265 | 199.646 | 14363.547 | 150.00 |
| 30 | — | — | — | — | — | — |

### QA — M127 (quadratic extension)

| k | Commit (s) | Open (s) | Prover (s) | Verify (ms) | Proof (KiB) | Peak RSS (GiB) |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.052 | 0.174 | 0.237 | 19.258 | 1518.469 | 0.77 |
| 22 | 0.291 | 0.694 | 0.994 | 25.558 | 2118.281 | 3.11 |
| 24 | 1.142 | 2.431 | 3.569 | 32.384 | 2874.750 | 12.49 |
| 26 | 4.649 | 9.463 | 14.122 | 44.325 | 3752.063 | 49.99 |
| 28 | 20.086 | 40.409 | 60.448 | 71.089 | 4850.781 | 199.99 |
| 30 | — | — | — | — | — | — |

## Scheme-Specific Parameters & Notes

`Distance` is a calculated conditional relative-distance lower bound, and `Queries` is a calculated query count rather than an empirical measurement of the actual minimum distance.

### RS — Goldilocks (cubic extension)

| k | Distance | Queries |
|---:|---:|---:|
| 20 | 0.28474760 | 844 |
| 21 | 0.27156425 | 893 |
| 22 | 0.25867736 | 945 |
| 23 | 0.24609858 | 1002 |
| 24 | 0.23383847 | 1062 |
| 25 | 0.22190666 | 1128 |
| 26 | 0.21031167 | 1198 |
| 27 | 0.19906095 | 1275 |
| 28 | 0.18816082 | 1357 |
| 29 | 0.17761652 | 1447 |
| 30 | 0.16743219 | 1544 |

### RS — M127 (quadratic extension)

| k | Distance | Queries |
|---:|---:|---:|
| 20 | 0.39139843 | 570 |
| 21 | 0.38383341 | 585 |
| 22 | 0.37631047 | 600 |
| 23 | 0.36883187 | 615 |
| 24 | 0.36140004 | 631 |
| 25 | 0.35401730 | 648 |
| 26 | 0.34668601 | 665 |
| 27 | 0.33940843 | 683 |
| 28 | 0.33218681 | 701 |
| 29 | 0.32502334 | 720 |
| 30 | 0.31792015 | 740 |

### QA — Goldilocks (cubic extension)

| k | Distance | Queries |
|---:|---:|---:|
| 20 | 0.15789557 | 1646 |
| 22 | 0.13932657 | 1885 |
| 24 | 0.12219796 | 2171 |
| 26 | 0.10654426 | 2511 |
| 28 | 0.09231348 | 2921 |
| 30 | 0.07946490 | 3416 |

### QA — M127 (quadratic extension)

| k | Distance | Queries |
|---:|---:|---:|
| 20 | 0.36872387 | 615 |
| 22 | 0.35391021 | 648 |
| 24 | 0.33930281 | 683 |
| 26 | 0.32491939 | 720 |
| 28 | 0.31077717 | 760 |
| 30 | 0.29689254 | 803 |

### Parameter selection

The query counts follow a conditional ordinary knowledge-soundness calculation. With `gamma = 2^-24` and query-error budget `2^-102`, the calculation sets `beta = 1 - delta + k*gamma` and `q = ceil(102 / -log2(beta))`, where `delta = min(J_gamma(J_gamma(Delta)), (Delta + k*gamma)/3) - 10^-6`, `J_gamma(x) = 1 - sqrt(1 - x*(1-gamma))`, and `Delta` is the calculated relative distance lower bound.

The resulting counts do not certify 100-bit PCS round-by-round knowledge soundness or Fiat–Shamir security. The recursive distance bound and its applicability to this implementation require independent justification. For QA, the calculation additionally assumes the imported random-base-code lower bounds `d0 >= 44` for Goldilocks and `d0 >= 62` for M127. The conditional error accounting does not include the probability of a bad QA base code, and the fixed benchmark seed does not itself certify its distance.

### Implementation and reproducibility notes

Both the RS and QA measurements use the same optimized BaseFold implementation, build settings, and hardware; timings can still vary slightly with system state. Goldilocks RS uses an FFT bottom encoder, while M127 RS uses direct evaluation. The QA bottom encoder and verifier share the same sampled base-field diagonal (embedded into the challenge field for verification), using a fixed benchmark setup seed.
