# ERA benchmark results

These results measure ERA using the [anonymous artifact provided with the paper](https://anonymous.4open.science/r/agnostir-F3B4/). A message with `num_variables = k` has `2^k` elements.

## Configuration and interpretation

- AMD EPYC 9754, 503 GiB RAM, 32 threads, and BLAKE3-256 hashing. The field is the secp256k1 scalar field. The security model is recorded as conditional 100-bit.
- One warmup followed by three measured samples. Commit, open, and verify times are arithmetic means. Setup time is reported separately. Open time corresponds to the benchmark's `prove_ms` metric.
- For `num_variables` 20 through 26, `row_log2 = 16`, the assumed distance is 0.22037, and the query count is 836. For 28 and 30, `row_log2 = 20`, the assumed distance is 0.225055, and the query count is 816. The corresponding `rows` values are 16, 64, 256, 1024, 256, and 1024; `eta` values are 4, 6, 8, 10, 8, and 10.
- All measured samples were verified. Proof and commitment sizes are reported separately; the commitment is 32 bytes in every configuration. Peak RSS is in GiB. Per-run measurements are in [`era.csv`](era.csv).

## Results

| k | Setup s | Commit s | Open s | Verify ms | Proof bytes | Peak RSS GiB |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.967383 | 0.114602 | 0.144301 | 61.429 | 2726824 | 0.559 |
| 22 | 0.968206 | 0.231376 | 0.364443 | 71.602 | 4010824 | 1.283 |
| 24 | 0.970317 | 0.787144 | 1.255580 | 104.658 | 9147016 | 4.133 |
| 26 | 0.967788 | 2.894531 | 4.414541 | 251.375 | 29692584 | 15.026 |
| 28 | 4.500281 | 15.458164 | 16.617000 | 1167.638 | 40541384 | 58.801 |
| 30 | 4.506331 | 59.970134 | 65.636214 | 1337.658 | 60594472 | 231.728 |
