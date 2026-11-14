# WHIR benchmark results

These results measure [WHIR](https://github.com/worldfnd/whir) for `num_variables` from 20 to 30. In the table, `k` denotes `num_variables`. Each row summarizes three runs with the same configuration.

## Configuration and interpretation

- AMD EPYC 9754, 503 GiB RAM, and 32 Rayon threads.
- Configured security parameter: 100 bits. Field: Goldilocks3. Hash: BLAKE3. The benchmark parameters `evaluations` and `pow_bits_max` were set to 1 and 20, respectively. The security parameter is a configuration value; equivalence with the parameter selection used in other result files has not been established.
- Times and proof sizes are medians of three runs. Prover time is the median of the per-run commit-plus-open sums, so it need not equal the sum of the displayed commit and open medians. The input timings are reported at their original precision; conversion from milliseconds to seconds does not add precision.
- The per-run measurements are in [`whir.csv`](whir.csv). All runs were marked `ok`.

## Results

| k | Commit s | Open s | Prover s | Verify ms | Proof KiB |
|---:|---:|---:|---:|---:|---:|
| 20 | 0.0490 | 0.2282 | 0.2784 | 2.9 | 137.9 |
| 22 | 0.1758 | 0.7053 | 0.8823 | 1.7 | 157.1 |
| 24 | 0.5865 | 2.3000 | 2.8865 | 1.9 | 184.3 |
| 26 | 2.2000 | 8.9000 | 11.1000 | 2.1 | 202.0 |
| 28 | 8.8000 | 36.0000 | 44.8000 | 2.2 | 226.6 |
| 30 | 36.9000 | 148.2000 | 185.3000 | 2.6 | 246.4 |
