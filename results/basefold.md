# BaseFold benchmark results

These measurements cover the optimized BaseFold implementation with Goldilocks or M127 base-field encoding, and challenges and folding over the cubic or quadratic extension, respectively. They are separate from the HyperFold benchmark results in this directory.

## Configuration and interpretation

- Rate 1/2; base-code message length 64 and codeword length 128. RS has minimum distance 65. QA uses the systematic code `(m, H diag(a / 64) H m)` with random nonzero base-field diagonal entries.
- AMD EPYC 9754, 32 physical cores pinned to CPUs 0–31, BLAKE3 Merkle hashing, shared Merkle multiproofs, optimized release build (`target-cpu=native`, thin LTO, one codegen unit).
- One warmup followed by three measured samples. Times and proof sizes in the tables are medians. The prover median is taken from each sample’s commit-plus-open sum; it may differ from the sum of the two displayed medians. Setup, input generation and external evaluation are excluded; transcript writing is included in opening. Proof size includes the commitment.
- Every measured sample was verified. `—` denotes an unmeasured metric. Distance and query entries are calculated parameters, not measurements of the actual minimum distance.
- The query counts use the preceding conditional ordinary knowledge-soundness calculation with `gamma = 2^-24`, query-error budget `2^-102`, `beta = 1 - delta + k*gamma`, and `q = ceil(102 / -log2(beta))`; `delta = min(J_gamma(J_gamma(Delta)), (Delta + k*gamma)/3) - 10^-6`, where `J_gamma(x) = 1 - sqrt(1 - x*(1-gamma))` and `Delta` is the calculated relative distance lower bound.
- These counts do **not** certify 100-bit PCS round-by-round knowledge soundness or Fiat–Shamir security. The recursive distance bound and its applicability to this implementation require independent justification. QA additionally assumes the imported random-base-code lower bounds `d0 >= 44` (Goldilocks) and `d0 >= 62` (M127); the probability of a bad QA base code is not included in this conditional ledger, and the fixed benchmark seed does not by itself certify its distance.
- Results at `2^30` were not run. Measured peak RSS at `2^28` was approximately 150 GiB (Goldilocks) and 200 GiB (M127); linear extrapolation gives roughly 600 GiB and 800 GiB at `2^30`, exceeding the machine’s 503 GiB RAM. These figures are extrapolations, not measured peaks. QA odd exponents and QA `2^29` were not requested or measured.

## Results

`k` means the polynomial has `2^k` base-field evaluations. `Distance` is a conditional relative-distance lower bound. `Peak RSS` is process peak resident memory, not memory consumption for the timed prover stage alone.

### RS: Goldilocks / cubic extension

| k | Distance | Queries | Commit s | Open s | Prover s | Verify ms | Proof bytes | Peak RSS GiB |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.28474760 | 844 | 0.042 | 0.151 | 0.193 | 25.823 | 1830112 | 0.58 |
| 21 | 0.27156425 | 893 | 0.077 | 0.312 | 0.383 | 33.732 | 2252552 | 1.16 |
| 22 | 0.25867736 | 945 | 0.135 | 0.532 | 0.667 | 40.363 | 2738432 | 2.33 |
| 23 | 0.24609858 | 1002 | 0.384 | 0.978 | 1.362 | 42.256 | 3289384 | 4.68 |
| 24 | 0.23383847 | 1062 | 0.734 | 2.030 | 2.764 | 50.497 | 3958096 | 9.38 |
| 25 | 0.22190666 | 1128 | 1.451 | 4.060 | 5.511 | 60.594 | 4709576 | 18.74 |
| 26 | 0.21031167 | 1198 | 2.946 | 7.952 | 10.900 | 67.712 | 5567504 | 37.49 |
| 27 | 0.19906095 | 1275 | 6.085 | 15.881 | 21.996 | 83.342 | 6569048 | 75.00 |
| 28 | 0.18816082 | 1357 | 12.778 | 34.676 | 47.353 | 94.499 | 7723456 | 150.00 |
| 29 | 0.17761652 | 1447 | 27.333 | 69.081 | 96.379 | 176.721 | 9011000 | 300.00 |
| 30 | 0.16743219 | 1544 | — | — | — | — | — | — |

### RS: M127 / quadratic extension

| k | Distance | Queries | Commit s | Open s | Prover s | Verify ms | Proof bytes | Peak RSS GiB |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.39139843 | 570 | 0.086 | 0.168 | 0.254 | 18.409 | 1462368 | 0.78 |
| 21 | 0.38383341 | 585 | 0.162 | 0.341 | 0.514 | 20.776 | 1734368 | 1.55 |
| 22 | 0.37631047 | 600 | 0.435 | 0.661 | 1.098 | 24.140 | 2032096 | 3.11 |
| 23 | 0.36883187 | 615 | 0.857 | 1.242 | 2.099 | 28.619 | 2377952 | 6.24 |
| 24 | 0.36140004 | 631 | 1.674 | 2.439 | 4.108 | 31.381 | 2733056 | 12.49 |
| 25 | 0.35401730 | 648 | 3.338 | 4.826 | 8.164 | 35.989 | 3149760 | 25.03 |
| 26 | 0.34668601 | 665 | 6.802 | 9.549 | 16.342 | 42.943 | 3603616 | 49.99 |
| 27 | 0.33940843 | 683 | 13.955 | 18.789 | 32.752 | 55.598 | 4086144 | 99.99 |
| 28 | 0.33218681 | 701 | 29.663 | 40.695 | 70.328 | 62.546 | 4609408 | 199.99 |
| 29 | 0.32502334 | 720 | 60.368 | 83.313 | 143.901 | 94.143 | 5235680 | 399.99 |
| 30 | 0.31792015 | 740 | — | — | — | — | — | — |

### QA: Goldilocks / cubic extension

| k | Distance | Queries | Commit s | Open s | Prover s | Verify ms | Proof bytes | Peak RSS GiB |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.15789557 | 1646 | 0.044 | 0.159 | 0.205 | 44.222 | 3013376 | 0.57 |
| 22 | 0.13932657 | 1885 | 0.169 | 0.506 | 0.684 | 70.859 | 4685152 | 2.33 |
| 24 | 0.12219796 | 2171 | 0.726 | 2.163 | 2.886 | 96.878 | 7009760 | 9.37 |
| 26 | 0.10654426 | 2511 | 2.874 | 7.975 | 10.859 | 136.642 | 10280400 | 37.50 |
| 28 | 0.09231348 | 2921 | 12.673 | 34.725 | 47.265 | 199.646 | 14708272 | 150.00 |
| 30 | 0.07946490 | 3416 | — | — | — | — | — | — |

### QA: M127 / quadratic extension

| k | Distance | Queries | Commit s | Open s | Prover s | Verify ms | Proof bytes | Peak RSS GiB |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.36872387 | 615 | 0.052 | 0.174 | 0.237 | 19.258 | 1554912 | 0.77 |
| 22 | 0.35391021 | 648 | 0.291 | 0.694 | 0.994 | 25.558 | 2169120 | 3.11 |
| 24 | 0.33930281 | 683 | 1.142 | 2.431 | 3.569 | 32.384 | 2943744 | 12.49 |
| 26 | 0.32491939 | 720 | 4.649 | 9.463 | 14.122 | 44.325 | 3842112 | 49.99 |
| 28 | 0.31077717 | 760 | 20.086 | 40.409 | 60.448 | 71.089 | 4967200 | 199.99 |
| 30 | 0.29689254 | 803 | — | — | — | — | — | — |

## Implementation and reproducibility notes

The RS and QA measurements use the BaseFold implementation developed for this comparison, not the HyperFold executable. Goldilocks RS uses an FFT bottom encoder; M127 RS uses direct evaluation. The QA bottom encoder and verifier use the same sampled base-field diagonal, embedded into the challenge field for verification. The setup seed is fixed for these benchmarks. The two rounds of measurements use the same build settings and hardware; timings can vary with system state.

Raw benchmark files and scripts were retained in the originating workspace under `plonkish/benchmark/bench_data/basefold_mixed_2fields_20_28_20261113/` and `plonkish/benchmark/bench_data/basefold_qa_rs_extra_20261113/`. Those paths describe provenance and are not part of this repository.
