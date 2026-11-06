# Correctness validation scope

The repository's tests are local, seeded and reproducible. They are **not** a
security proof or exhaustive adversarial review. `#[cfg(test)]` reference code is
independent of the optimized matrix/FFT/WHT paths under test.

| Requirement | Implemented coverage |
|---|---|
| M127 arithmetic | Exact BigUint comparison on boundary/random values; canonical codec rejection; inverse/halving; primitive generators and extension Frobenius/inverses. |
| matches_naive | Walsh matrix, XOR convolution, ordinary RS direct evaluation/Lagrange interpolation, explicit H matrices, recursive generator-matrix product, quadratic Boolean sum. |
| Local algebra/order | WHT roundtrip, H inverse, natural FFT output versus direct evaluation, block-strided sumcheck equals beta-weighted message fold, encoder/fold commutation for every r/c/backend in both fields. |
| SIMD | Generic WHT pack/scalar suffix path matches naive Walsh; native release suite enables this host's AVX512 Goldilocks packing. |
| Ragged/multi-batch | MMCS ascending/descending/repeated heights, multiple matrices and simultaneous indices; native pruned frontier verification and tamper rejection. |
| PCS parameter matrix | mu=9/10/11/12, r=2/4/8, c=2/4, both backends; Goldilocks degree2/3 and M127 degree2; two random extension opening points each. These small parameters are functional, not certified. |
| Parallel agreement | 1/4/32-thread commit/proof bytes identical; these are correctness tests, not 1-thread performance benchmarks. |
| Memory ownership and fixed references | 96 pre-cleanup native-retained commitment/proof/post-challenge fingerprints match the current owning path at1/32 functional threads; both fields/codes/arities/rates, depths1–4. Pointer/capacity and tracked-drop tests cover parameter/witness ownership, release and errors. Borrowed repeated opens remain equivalent. |
| Large parallel kernels | Random non-base extension inputs, odd lane lengths above 4096, scalar/reference fold and in-place butterfly comparisons at 1/4/32 threads. |
| Transcript | Prover/verifier final additional challenge equality, framing-domain tests, public-parameter and statement binding. |
| Negative tests | Claimed value, y0/root, parameters id, opening point, round values, folded roots, final table, opened base/extension values, sibling hashes, missing/extra proof pieces and malformed/trailing wire. |
| Parameters | Importer checks 144 exact four-term rational error sums; runtime refuses altered certified rows, invalid sizes/divisibility and wrong field/extension degree. |

Run the normal and native release suites from README. No claim that randomized
correctness tests validate the IOP proof, PRG or Fiat–Shamir soundness follows.

The official benchmark CLI has separate selection, removed-flag and no-clobber
checks. Isolated controller smoke traverses both fields/codes/r/c at mu20 with
one warmup and one measured trial per process; it is tagged functional smoke and
is not a new comparative performance result. Archive analysis independently
recomputes means/SD and validates commands, release markers and resource records.
Historical analyzers remain standalone.
