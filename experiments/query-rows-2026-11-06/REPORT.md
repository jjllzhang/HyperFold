# Queried-row recovery: constructor-controlled opening comparison

M127 μ=26, r=2, c=4, quadratic extension, QA/RS, 32 Rayon threads. A = original native retention; B = prehashed retention; C = prehashed queried-row recovery. B/C share one leaf-hashing function and the same pinned native tree constructor.

Relative to native retention, mean open changed by +44.187 ms (+0.422%) for QA and +5.658 ms (+0.054%) for RS. The sample SDs of the three round differences were 156.849 / 33.218 ms. See the process-level ranges below; these are descriptive measurements, not a bound on future runs. Both round-difference ranges cross zero: this experiment did not resolve a stable increase; it does not establish zero overhead. Maximum process RSS changed from 36.978 / 36.991 GiB to 32.983 / 32.987 GiB for QA/RS.

| Code | Strategy | Open mean (s) | SD across process means (s) | First-tree query (ms) | Max process RSS (GiB) |
|---|---|---:|---:|---:|---:|
| QA | native | 10.4615 | 0.1221 | 2.0314 | 36.978 |
| QA | prehashed | 10.5722 | 0.0218 | 1.7551 | 36.996 |
| QA | recovered | 10.5057 | 0.0878 | 1.5707 | 32.983 |
| RS | native | 10.4968 | 0.0414 | 1.4385 | 36.991 |
| RS | prehashed | 10.5339 | 0.0474 | 2.2188 | 36.985 |
| RS | recovered | 10.5025 | 0.0744 | 1.3011 | 32.987 |

| Code | Contrast | Added open (ms) | Net change | Range across rounds | SD of round differences (ms) |
|---|---|---:|---:|---:|---:|
| QA | recovered − native | 44.187120 | +0.422% | -1.051% to +1.967% | 156.849449 |
| QA | prehashed − native | 110.707993 | +1.058% | +0.472% to +2.210% | 101.707188 |
| QA | recovered − prehashed | -66.520873 | -0.629% | -1.558% to -0.091% | 85.463059 |
| RS | recovered − native | 5.658488 | +0.054% | -0.287% to +0.339% | 33.217648 |
| RS | prehashed − native | 37.105611 | +0.353% | -0.284% to +1.111% | 73.706596 |
| RS | recovered − prehashed | -31.447123 | -0.299% | -1.383% to +0.392% | 100.450481 |

C−A is the net change relative to normal operation. B−A identifies changes from the prehashed construction entry. C−B compares retention and recovery with that entry held fixed; it includes the changed working set and allocation lifetime, rather than isolating pure arithmetic.

## Measurement structure and limits

Each backend uses three independent process rounds in ABC/BCA/CAB order. Each process uses one common commitment, one warmup, then three measured openings. Thus each strategy/backend has three independent process means and nine measured openings; the nine openings are not nine independent processes. The tables preserve each process mean, within-process sample SD and cross-process variation. Small changes comparable to these variations do not establish a stable increase or zero cost. No percentage is extrapolated to μ30.

Both Setup diagonal directions stay alive throughout all modes. Setup, commit, claimed-value evaluation, verification, final serialization and fingerprint calculation are outside open. All tree construction, dense-codeword release, recovery and multiproof work stay inside open. The optional first-tree query clock measures row access/recovery, native path extraction and cross-layer compression. A/B open the first tree before other intermediate trees; C opens it after the others have been opened and released. Its sub-timing therefore also includes ordering/cache-state differences and is not a pure recovery arithmetic timer. Peak RSS spans Setup, commit and all openings, not just query time.

The first dense intermediate codeword has a 4 GiB payload at μ26. C retains its hashes, borrows only the top codeword/diagonals/challenge in its Matrix descriptor, and releases the dense matrix after its successor is folded, before committing that successor. The measured process peak need not fall by exactly 4 GiB. This study does not demonstrate whether μ30 fits.

Every generated proof was verified, with matching prover/verifier post-challenges. Commitment, proof and post-challenge fingerprints and wire sizes match across strategies and rounds per backend. The size column counts commitment plus encoded evaluation proof. Correctness/byte equality and tampering tests also passed at both base fields and r=2/4/8, c=2/4, depths1/2/3/4, including 1/4/32-thread comparisons and a test-only counter showing no lazy-row access during tree construction. Tests are not a cryptographic security proof.

## Reproduction

Source, native executable hash, exact commands/status, thread count and seeds are in manifest and the raw evidence archive. Seeds are 42/27/28 for Setup/message/point. Hardware is AMD EPYC 9754, 128 physical / 256 logical CPUs, four NUMA nodes, about 503 GiB RAM, Rust 1.95.0. Native CPU release flags; no CPU pinning and a shared host. The same certified uniform-profile M127 rows and unchanged Plonky3 pin are used; the two relevant parameter rows are checked against each raw record. The official 142 configurations and 426 samples remain unchanged.

From the source commit recorded in manifest, with the existing pinned local Plonky3 checkout:

```sh
RUSTFLAGS='-C target-cpu=native' cargo build --offline --locked --release --example query_row_overhead
python3 scripts/profile_query_rows.py --binary target/release/examples/query_row_overhead --output-dir /tmp/query-row-new-empty
```

To reproduce all numeric analysis without the executable, from this report folder:

```sh
mkdir evidence
tar -xzf raw-evidence.tar.gz -C evidence
python3 analyze_query_rows.py --root evidence/data --parameters m127-parameters.csv --output regenerated
```

Raw archive includes every case command, zero-exit status, GNU time resource record and measured CSV; its large executable is omitted but SHA-256 retained. The analyzer checks process order, parameter queries, repetitions, fingerprints, resource peaks, means/SDs and contrasts against raw evidence. SHA256SUMS covers all other snapshot files.

Source commit: `4bd4757b95709d8b693f5e06e910e1615ae27278`. Executable SHA-256: `0278ec8aa38c0ac9d04589235983dea109c86ae3c748ae87f9b0107d8a6160b7`.
