# HyperFold PCS research prototype

Rust implementation of the agreed HyperFold v3 plan, using Plonky3 v0.8.0 field and
Merkle/MMCS interfaces. **Research code, not production cryptography.** Correctness
tests and the supplied interactive-IOP parameter certificates are **not** a security
proof of this implementation, its Fiat–Shamir compilation, or a deployment.

## Implemented

- Goldilocks base field with cubic challenges (`X³-X-1`); custom canonical M127
  (`2¹²⁷-1`) base field with quadratic challenges (`X²+1`).
- Systematic QA-units coding by WHT; Goldilocks coefficient-message RS by a single
  forward radix-2 FFT; M127 ordinary systematic RS by Lagrange interpolation on
  `x_j=j`. No FFT or extension-valued encoding symbols are assumed for M127 RS.
- Exact H₂/H₄/P₈ operators, bottom-up HFC encoding, bundle-first Merkle leaves,
  pre-inverted diagonals, strided interleaved quadratic sumcheck, commit/open/verify.
- Plonky3 `MerkleTreeMmcs` / `PrunedMerklePaths`: repeated query bundles and shared
  authentication paths are deduplicated. Each intermediate opened bundle omits one
  deterministic parent-derived coordinate; the verifier restores it before hashing
  and checks **all** incoming folding equalities, including collisions.
- Uniform one-shot benchmark ownership: release forward diagonals after commit,
  release the original witness after its first lifted fold, retain first-tree
  hashes and recover only queried rows. Reusable borrowed library opens remain
  available; official timings always use the owning lifecycle.
- Canonical field bytes, framed Blake3 transcript, exact wire-size reporting,
  32-thread benchmark CLI and Criterion benchmark.

See [protocol mapping](docs/protocol.md), [test coverage](docs/testing.md), and
[experiment instructions](docs/experiments.md).

## Latest results

The [native M127 PCS comparison](experiments/m127-pcs-backends-2026-11-06/FINDINGS.md)
completed36 independent processes/108 measurements at four representative points.
The frozen custom backend had the lowest online/full-wall mean at all four;
ark/FourQ remain experimental adapters, and the production default stays custom.
The [largest two one-shot cases](experiments/one-shot-mu30-2026-11-06/REPORT.md)
both completed at about448 GiB using the release/query-recovery strategy.
Newly compiled cleanup code is checked separately; old observations are immutable.

## Historical retained benchmark results

The [native-retained unguarded follow-up report](experiments/retained-unguarded-2026-11-06/REPORT.md)
contains 142 complete cases (426 repetitions) and two actual process failures after
memory termination was disabled. Both extreme cases exited 137 with signal 9;
all incomplete timing and proof-size measurements are blank. Real memory traces,
exit records, global OOM-counter observations and portable analysis are included.
No recomputation was used, and the 142 previously measured cases are unchanged.

The subsequent [μ=30 queried-row attempts](experiments/mu30-recovered-2026-11-06/REPORT.md)
actually ran the two largest M127/r=2/c=4 cases with retained two-direction Setup
and all active memory guards disabled. Both exited 137 / signal 9, at observed
peaks of 497.416 / 497.934 GiB; no formal measurements completed. Thus this
strategy alone did not finish those cases on this host. Its incomplete metrics
stay blank and the original baseline is unchanged.

The [queried-row recovery comparison](experiments/query-rows-2026-11-06/REPORT.md)
completed 18 processes / 54 measured openings at M127 μ=26, r=2, c=4. Relative to
native retention, QA/RS mean open changed by +0.422% / +0.054%, with both round
ranges crossing zero; this study did not resolve a stable increase. Peak process
RSS fell from about 36.98 / 36.99 GiB to 32.98 / 32.99 GiB. The report includes a
prehashed-retention constructor control and does not extrapolate to μ=30.

The separate [first-tree rebuilding comparison](experiments/recompute-overhead-2026-11-06/REPORT.md)
measures a 31.453%–38.822% opening-time increase at four fitting M127 settings
(μ=24/26, r=2, c=4). Both Setup table directions stay live in this comparison;
it does not establish an exact μ=30 overhead. The official 142-case data is unchanged.

The [earlier guarded snapshot](experiments/retained-2026-11-06/REPORT.md) remains
available as historical evidence of the 12 GiB available-memory policy.

## Build and test

Tested with Rust/Cargo **1.95.0**, edition 2024. The plan's local reference is pinned
in provenance to Plonky3 commit `0d803737b681774186cd283b1fd2d8e493655c82`.
The initial `Cargo.toml` follows the agreed local path dependency layout:
`/home/zjl/BinaryField-Bench/vendor/Plonky3`.

For another machine, provide that checkout and rewrite **only** the local paths:

```sh
python3 scripts/configure_plonky3.py /path/to/Plonky3
cargo check --locked --all-targets
RAYON_NUM_THREADS=8 cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Add `--offline` when the locked registry dependencies are already cached. For
native SIMD validation and benchmarks on a compatible host:

```sh
RUSTFLAGS='-C target-cpu=native' RAYON_NUM_THREADS=8 cargo test --locked --release
RUSTFLAGS='-C target-cpu=native' cargo build --locked --release --bin bench
./target/release/bench --list
./target/release/bench --fields m127 --backends qa --mu 20 --r 2 --c 4 \
  --threads 32 --repetitions 1 --output bench-results/smoke-new.csv
RUSTFLAGS='-C target-cpu=native' cargo bench --locked --bench pcs
```

Every official warmup/trial uses fresh Setup and the same owning lifecycle;
summary/raw paths must be new. Legacy memory switches, estimates, resume and quick
mode are rejected. All selected rows are certified uniform-profile configurations;
small uncertified configurations are limited to functional tests. The CLI accepts
only32-thread timing runs. See the experiment guide for isolated-process commands,
phase costs, failure records and pure-archive reconstruction.

## Parameter provenance and security boundary

`data/` contains the exact two official v3 CSVs and their SHA-256 hashes, certificate
`hfc-tight-total100-gold-m127-v3-2026-11-05`. `scripts/import_params.py` checks all
four error terms using exact rational arithmetic and generates the Rust table:

```sh
python3 scripts/import_params.py data
cargo fmt
```

Import uses the main `extension_degree`, `eta_exact`, `queries` fields and never
`minimum_profile_*` or the 96-bit sensitivity table. The certified target is the
stated four-term **interactive IOP + commit-binding** bound below `2^-100`;
Fiat–Shamir, Merkle, public-seed PRG and grinding losses remain **uncertified**.
See `data/*parameters.csv` for each row's precise scope and budget.

Setup takes a public 32-byte seed for reproducibility. Seeded Blake3 XOF streams
use rejection sampling for independent-looking, uniform-domain nonzero base-field
values; replacing ideal Setup coins by a PRG is **not** a proved consequence of the
IOP certificate. Seeds/configuration/power basis are bound into the transcript.
M127 arithmetic and the prover are **not claimed constant-time**. The PCS is not
advertised as hiding or zero-knowledge.

## Minimal API sketch

```rust,no_run
use hyperfold_pcs::{pcs::HyperFoldPcs, params::{HyperFoldConfig, BaseCodeKind,
    BaseFieldKind}, sampling::{stream, SampleBase}, sumcheck::evaluate};
use p3_field::{PrimeCharacteristicRing, extension::CubicTrinomialExtensionField};
use p3_goldilocks::Goldilocks;
type E = CubicTrinomialExtensionField<Goldilocks>;
let cfg = HyperFoldConfig::experiment(BaseFieldKind::Goldilocks,
    BaseCodeKind::Qa, 20, 8, 2)?;
let pcs = HyperFoldPcs::<Goldilocks, E>::setup(cfg, [42; 32])?;
let mut rng = stream(&[27; 32], b"example-message", 0);
let f = (0..cfg.k()).map(|_| Goldilocks::sample(&mut rng)).collect();
let (cm, prover_data) = pcs.commit(f)?;
let z = vec![E::from_u8(7); cfg.mu];
let y = evaluate(&prover_data.message, &z);
let (proof, _) = pcs.open(&cm, &prover_data, &z, y)?;
pcs.verify(&cm, &z, y, &proof)?;
# Ok::<(), hyperfold_pcs::params::Error>(())
```

