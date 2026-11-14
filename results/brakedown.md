# Brakedown benchmark results

These results report Brakedown measurements from the [ReedWeave benchmark results](https://github.com/jjllzhang/ReedWeave/blob/master/results/Brakedown/goldilocks.csv). The benchmark uses the Brakedown implementation in [lcpc](https://github.com/conroi/lcpc).

## Configuration and interpretation

- AMD EPYC 9754, 503 GiB RAM, 32 Rayon threads, Goldilocks field, and a 100-bit security parameter. Merkle hashing uses BLAKE3; the benchmark uses a Merlin transcript.
- The SdigCode3 code has a nominal rate of approximately 1/1.521. Exact rates, which vary with integer dimensions, are preserved in [`brakedown.csv`](brakedown.csv).
- Each row records the source CSV result for `2^k` polynomial coefficients, where `k` is `log_n` in the CSV. Commit and open times are in seconds, verification times are in milliseconds, and proof sizes are in KiB. The source contains one result row per size and thread count; this table selects the 32-thread results.

## Results

| k | Commit s | Open s | Verify ms | Proof KiB |
|---:|---:|---:|---:|---:|
| 20 | 0.017546 | 0.022476 | 32.799 | 5631.969 |
| 21 | 0.033349 | 0.031214 | 45.381 | 6784.820 |
| 22 | 0.077806 | 0.044857 | 64.755 | 8165.203 |
| 23 | 0.152750 | 0.079336 | 91.617 | 10269.727 |
| 24 | 0.256095 | 0.118814 | 138.912 | 12990.219 |
| 25 | 0.476434 | 0.154329 | 188.935 | 17038.297 |
| 26 | 0.917253 | 0.224186 | 270.715 | 22479.312 |
| 27 | 1.820610 | 0.334954 | 383.316 | 30374.258 |
| 28 | 3.476980 | 0.474165 | 536.740 | 41256.258 |
| 29 | 8.175787 | 0.768657 | 808.821 | 56844.969 |
| 30 | 18.272906 | 1.258288 | 1096.645 | 78568.727 |
| 31 | 43.528611 | 2.172083 | 1587.300 | 109544.938 |
| 32 | 114.399473 | 4.200896 | 2340.449 | 153032.695 |
| 33 | 282.967672 | 9.253040 | 3380.838 | 214743.664 |
| 34 | 630.463097 | 22.545054 | 4781.692 | 301759.391 |
