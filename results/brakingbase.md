# BrakingBase benchmark results

These results measure [BrakingBase](https://github.com/arithmic/BrakingBase) for variable counts from 20 to 30.

## Configuration and interpretation

- AMD EPYC 9754, 503 GiB RAM, and 32 Rayon threads. Field: Goldilocks. Security parameter: 100 bits. Hash: BLAKE2s-256.
- One setup, followed by one warmup and three measured samples. Commit, open, and verify times are the reported means. Setup and trim times are reported separately.
- Proof sizes are reported in bytes. The run at `k = 30` failed due to insufficient memory; `-` denotes a result that was not reported. The tabular data is also available in [`brakingbase.csv`](brakingbase.csv).

## Results

| k | Setup s | Trim s | Commit s | Open s | Verify ms | Proof bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 1.302 | 0.204 | 0.020713 | 0.946698 | 36.835 | 9204336 |
| 22 | 5.036 | 0.905 | 0.083537 | 3.091 | 33.941 | 10609520 |
| 24 | 19.083 | 3.641 | 0.382598 | 11.536 | 35.452 | 12340304 |
| 26 | 79.702 | 16.390 | 2.010 | 48.262 | 42.175 | 14508912 |
| 28 | 331.614 | 80.222 | 10.486 | 191.920 | 117.737 | 17241872 |
| 30 | - | - | - | - | - | - |
