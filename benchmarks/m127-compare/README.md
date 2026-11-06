# Exact-M127 arithmetic comparison (benchmark only)

This package compares the unchanged HyperFold canonical implementation, ark-ff
0.6.0 configured for the **same** prime `2^127-1`, and Microsoft's FourQlib x64 C
core at `1031567f23278e1135b35cc04e5d74c2ac88c029`. The extension basis is
`a+b*i`, `i²=-1`, in every implementation. Nothing here adds ark or C to the
production PCS dependency graph.

References: [ark-ff](https://docs.rs/ark-ff/0.6.0/ark_ff/),
[FourQlib](https://www.microsoft.com/en-us/research/project/fourqlib/).
FourQ is no longer actively maintained. Its unmodified source is built from an
external pinned checkout; its MIT notice is in `FOURQ-LICENSE`.

```sh
git clone https://github.com/microsoft/FourQlib.git /tmp/FourQlib
git -C /tmp/FourQlib checkout 1031567f23278e1135b35cc04e5d74c2ac88c029
FOURQ_ROOT=/tmp/FourQlib RUSTFLAGS='-C target-cpu=native' \
  cargo build --locked --release --manifest-path benchmarks/m127-compare/Cargo.toml
benchmarks/m127-compare/target/release/hyperfold-m127-compare --backend ours --check-only
python3 benchmarks/m127-compare/run.py --root /tmp/new-empty-comparison
```

Use the same pinned local Plonky3 checkout as the root project. Both Cargo locks,
source/binary hashes, compiler settings and machine details belong to each run.
The C configuration is `_AMD64_`, `__LINUX__`, `-O3 -march=native -fwrapv
-fno-strict-aliasing`; **no FourQ ASM/AVX macros** are selected. ark has `std,asm`
features, but its M127 no-carry multiplication predicate is false (checked at
compile time). Enabling the feature is not evidence that multiplication used
the no-carry assembly implementation. No constant-time property is inferred:
our current inverse and ark's binary-Euclidean inverse have different algorithms.

All measured work uses **32 resident OS workers**, created once per field
stage and synchronized before each window. Each worker repeats its local batches
for at least100ms, rather than repeatedly dispatching short Rayon tasks. Inputs,
conversion, preparation and calibration finish before the measured start. The
master wall spans start/end barriers; worker kernel clocks cover calibrated
blocks targeting1ms and exclude barriers, hashes and CSV/result collection.
Raw per-worker passes, items, kernel time, wall time and clock quantum are kept.
One-thread mode is accepted only with `--check-only`, for functional checks.
Neither worker durations nor aggregate wall throughput is an isolated
single-thread latency measurement.

Fold/LiftFold use full-width nonzero base/extension challenges derived from a
fixed seed with BLAKE3. Challenges are converted before timing and included in
the input fingerprint; they are not small hardcoded multiplication constants.
C has four specialized width/contract entries selected once per worker batch;
field width and canonicalization do not cause per-element runtime dispatch.

The three working sets contain 8192,32768,1048576 elements in total, or
256,1024,32768 per task. Every element occupies 16 base /32 extension bytes in
all implementations. Kernels include independent arithmetic, separate dependent
multiply/square chains, 256-element WHTs and inner products (also mixed E×F),
first F→E/later E→E affine folding, sumcheck evaluations and prefix batch
inversion with1024/4096-element chunks. `BatchInverseAlloc` also includes scratch
allocation/initialization/destruction for the same1024-chunk algorithm. This
common prefix algorithm compares arithmetic; it does not claim to reproduce all
library-specific batching optimizations or a complete PCS execution.

`Wht` includes its input copy and has no timed allocation; other arithmetic
inputs/outputs/scratch are preallocated. Chain kernels execute `N/32` dependent
operations per task from nondegenerate input, independently of other tasks.
Inverse kernels always read a nonzero source array and write separate output;
they do not repeatedly invert the previous result. Dot outputs one scalar per
256-element block; sumcheck outputs three scalars per task. Their denominator
is **input items**, not a fabricated count of primitive field operations.

`CanonicalRoundtrip`, `Encode`, `Decode` and `Copy` are separate boundary/control
metrics. Encode writes exactly16/32 little-endian bytes per item into aligned
limb storage; decode starts from already canonical input, excluding validation
of malformed untrusted encodings. Root tests still cover rejecting noncanonical
proof encodings. `Copy` helps expose dispatch/scheduling/FFI overhead, especially
for small workloads; do not subtract it blindly from arithmetic time.

`fourq-native` permits FourQ's redundant representation of zero. Canonical byte
export is outside its arithmetic window. `fourq-canonical` normalizes each
returned field element, also between primitive operations in composed kernels;
FourQ's atomic field routines retain their own internal algorithms. Their final
outputs are checked to be strictly below p. Our/ark arithmetic does not decode
Montgomery representation after every multiplication. FourQ crosses FFI once
per task batch, not once per element. Native throughput alone is not the cost
of replacing our canonical contract.

Each process runs differential checks before timing:10,000 seeded random base
vectors plus carry-heavy boundaries against ark/BigUint, then base/extension
kernel equality against both FourQ contracts and ark. Zero inverse is mapped to
zero **only in this checking harness**; timed inversion arrays exclude zero.
Actual PCS zero/Option behavior is unchanged. Canonical output hashes must match
across all compared implementations, workloads and independent process rounds.

Formal runs rotate four variants over three independent process rounds (12
processes); each kernel has calibration/warmup plus five windows of at least100ms.
Windows within one process are not independent processes. `passes` is the sum
of worker-local passes, so `work_items = passes * elements /32`. The output hash
combines the32 worker canonical-output hashes in index order; each backend has
exactly the same partitioning and must match. Copy is a separately reported
control rather than a quantity to subtract blindly. No compilation/tests or
large PCS runs overlap the measured processes.

The runner chooses the first32distinct physical cores from its permitted CPU
set and restricts only itself/descendants to that set. Exact CPU IDs, original
mask and hardware topology are recorded; host-wide services/settings are not
changed. This controls the process CPU set but grants no exclusive-core,
fixed-frequency or per-worker permanent-core guarantee on the shared host.

No arithmetic optimization or legacy lifetime removal belongs in the baseline
commit. Performance tests and differential checks are not a security proof.
