# First-tree rebuilding overhead: isolated paired measurements

On this machine, rebuilding the first intermediate Merkle tree increased measured opening time by **31.453%–38.822%** for the four fitting configurations below. This is the net effect of the existing opening strategy, including its changed memory footprint. It is not an exact estimate for μ=30, where the retained baseline did not complete.

| M127 configuration | Retained open (s) | Rebuild open (s) | Added time (s) | Increase | Sample SD, retained / rebuild (s) |
|---|---:|---:|---:|---:|---:|
| μ=24, QA, r=2, c=4 | 2.828 | 3.878 | 1.050 | 37.133% | 0.072 / 0.111 |
| μ=24, RS, r=2, c=4 | 2.833 | 3.724 | 0.891 | 31.453% | 0.107 / 0.038 |
| μ=26, QA, r=2, c=4 | 10.466 | 14.325 | 3.859 | 36.873% | 0.094 / 0.243 |
| μ=26, RS, r=2, c=4 | 10.382 | 14.412 | 4.030 | 38.822% | 0.107 / 0.131 |

Each mean uses three measured openings after one unrecorded warmup. These are descriptive results from one machine and fixed seeds, not confidence bounds or a claim of a universal percentage. There are four pairs, eight sequential processes and 24 raw measurements. Within each process there is one common commitment reused by all openings. Process order alternates between strategies across the four pairs; there is no randomization or CPU affinity binding.

## What was timed

The only strategy switch is `OpenOptions::recompute_first_tree`. Both strategies retain the same forward and inverse Setup diagonal tables through all measured openings. Setup, commitment generation, evaluation of the claimed value, proof serialization, fingerprint calculation and verification are outside the opening timer. The rebuild path includes reconstructing the first intermediate tree and producing its multiproof; that work remains in open time. The strategy also changes allocation/working-set behavior, so these differences are not an isolated timer around the tree constructor. No diagonal-table conversion is performed or included. A lifecycle that changes diagonal-table storage must be measured separately.

Every opening is verified. Prover and verifier post-opening challenges agree. The commitment, encoded proof and post-opening challenge have identical fingerprints across all repetitions and both strategies for each configuration. Wire sizes also match; the `proof_size_bytes` column includes the commitment plus proof encoding, as in the official sweep. `means.csv` includes process peak RSS from GNU time, covering Setup/commit/all openings rather than an isolated opening peak.

## Provenance and reproduction

Run date: 2026-11-06 UTC, completed 06:41:55Z. Source: `68c1272949fd46dbc684457ee46819dac56dcfec`. Hardware: AMD EPYC 9754, 128 physical cores / 256 logical CPUs, four NUMA nodes, about 503 GiB RAM. Rayon uses 32 threads; the host is shared and no affinity is enforced. Rust 1.95.0, Plonky3 `0d803737b681774186cd283b1fd2d8e493655c82`, native CPU release build. M127 quadratic extension, certificate `hfc-tight-total100-gold-m127-v3-2026-11-05`, existing uniform-profile queries. Setup/message/evaluation-point seed bytes: 42/27/28. Exact build, binary hash, order, lock hash, full hardware metadata and per-process commands/status/resources are preserved.

From a checkout of that source with its existing local Plonky3 dependency available:

```bash
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --example recompute_overhead
python3 scripts/profile_recompute_overhead.py --binary target/release/examples/recompute_overhead --output-dir /tmp/hyperfold-new-empty-profile
```

The output directory must be empty. For one configuration, the underlying command is:

```bash
target/release/examples/recompute_overhead --mu 24 --backend qa --strategy retained --threads 32 --repetitions 3 --output /tmp/retained.csv
target/release/examples/recompute_overhead --mu 24 --backend qa --strategy rebuild --threads 32 --repetitions 3 --output /tmp/rebuild.csv
```

`paired.csv`, `means.csv` and `raw-samples.csv` are the convenient result tables. `raw-evidence.tar.gz` holds all per-process commands, exit status, GNU time resource records and raw CSVs, plus the launch script and metadata. The pinned executable is omitted from the archive; its SHA-256 is recorded in `manifest.json`. Source copies of the harness/controller and their helper are included. `validation.json` records recomputed means/SDs, zero exit codes, executable hash verification and fingerprint agreement. SHA256SUMS covers every other snapshot file.

## Lower-cost candidate and scope limits

The existing rebuilding path discards the first intermediate tree, then regenerates its full codeword and hashes after query positions are known. A candidate proposed by Architect is to keep the tree hashes, release the first intermediate codeword after producing its successor, and recover only the queried rows from the retained top codeword and folding challenge. Multiproofs would reuse the retained hashes. This avoids full codeword/tree regeneration but requires a hash/leaf separation API and equality/tampering tests; it has not been implemented or benchmarked here.

In a commit-then-only-open lifecycle, a further candidate is to retain the inverse diagonal table prepared by Setup and release the forward table after the final commitment, avoiding the current low-memory path's table inversion transition. This restricts subsequent commitments; release cost and end-to-end memory peak must be accounted for. Neither proposal is a demonstrated guarantee that μ=30 fits this machine. Any optimized results must be labeled separately and include their recovery cost rather than subtracting it from reported open time.

The official 142 completed configurations and 426 samples are unchanged by this diagnostic study. The two μ=30 M127 cases actually failed with exit 137 / signal 9 under full retention with programmatic memory protection disabled; no normal μ=30 opening baseline exists for computing a same-size overhead percentage. This experiment establishes engineering behavior, not a cryptographic security proof.
