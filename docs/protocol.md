# Protocol and implementation mapping

## Version and sources

Implementation target: agreed `impl-plan-hyperfold-pcs.md` v3.0, including the H4
erratum confirmed on 2026-11-05; original HyperFold manuscript Protocols 4.1/4.2,
Algorithm 1 and local fold identity; supplied `hyperfoldable_codes.md` §§2–4;
Li et al. (2026), *More Efficient SNARKs via Quasi-Abelian Codes*, Algorithm 3.
The private source documents are not republished in this repository. Mathematical
parameter derivation is separate from implementation correctness.

## Modules

| Module | Contract |
|---|---|
| `field_m127` | Canonical `u128` residues, exact four-limb 128×128 multiplication folded modulo M127; Plonky3 traits; quadratic basis X²+1. |
| `params` / `certified_params` | Exact official 144 rows, strict certificate/config match, nonzero diagonals and Setup-only batch inverse. |
| `sampling` | Public-seed role/index-separated Blake3 XOF, rejection rather than modular-reduction sampling. |
| `base_code` | QA scaled WHT multipliers; Goldilocks Fourier Vandermonde; M127 Lagrange parity block transposed for contiguous inner products. |
| `matrix_h` | H₂/H₄ and Pascal P₈ circuits; H₄ dense inverse precomputed in Setup; P₈ forward/inverse staircase circuits. |
| `encoder` | Natural-order intermediate subtrees; top codeword directly written as Merkle row bundles; single-pass row-major folding. |
| `sumcheck` | Equality weights, base-field first round, degree-2 evaluations at 0/1/2, stride equal to the lower-level message length. |
| `mmcs` | Domain-separated canonical Blake3 adapters over Plonky3 binary cap-0 MMCS and native pruned multiproof. |
| `transcript` | Versioned, length-framed public parameter/statement binding and rejection-sampled power-basis challenges. |
| `pcs` / `codec` | Protocol sequencing, final checks, coordinate omission/restoration and canonical Postcard wire encoding. |

## Basis and layout invariants

Message index bit `s` is multilinear variable `X_s` (zero-based in code). At layer
`i`, split the message into `r` contiguous blocks of length `k_(i-1)`; the block's
least-significant bit is folded first. A subround pairs indices
`(2*m)*k_(i-1)+w` and `(2*m+1)*k_(i-1)+w`, not adjacent scalar elements.

Logical codeword `pi[a*n_prev+j]` is physically stored at `values[j*r+a]` in every
committed/folded matrix. The initial bottom-up encoder uses natural subtree layout
until its final layer, then writes row bundles directly. Folding writes its next
row directly from source rows `j'+b*n_(i-2)`. No full-size transpose is inserted.

The corrected H₄ circuit is the matrix with rows `(2,3,1,1)`, `(1,2,3,4)`,
`(1,1,2,3)`, `(3,4,1,2)`: 10 additions plus 3 diagonal scalings. H₂ uses 2+1;
P₈ uses 56 additions plus 7 diagonal scalings. Both fields have all nonempty
square minors tested. An incorrect earlier example circuit is **not** used.

Goldilocks RS interprets a message block as coefficients. M127 systematic RS
interprets it as the values at the first `k0` nodes. In either case it is the
**message table**, not the internal univariate RS polynomial, on which the PCS
multilinear sumcheck acts. These base encoders are linear/injective and MDS; their
encoder costs differ and must be labelled in comparisons.

## Transcript and wire format

- The public-parameter id binds the full configuration, certificate version,
  Setup seed, and Plonky3 extension algebra identity. Roles for generated QA
  multipliers and diagonal entries are separate.
- Commit: observe top Merkle root, sample `z0` in the challenge field, evaluate
  `y0`. **`z0` is implicit/reconstructed from root and parameters**, so the wire
  commitment carries `parameters_id`, root, and `y0` rather than transmitting
  redundant challenge coordinates. Protocol 4.1's logical commitment still has
  `(root,z0,y0)`.
- Eval: observe `y0`, opening point and claimed value; sample gamma; observe each
  quadratic round before its alpha, and each folded root before the next layer.
  Observe the final clear message table before drawing queries.
- Full independent top-level query list is sampled first. Only then sort/dedup
  each projected layer. Deduplication does not reduce the security query count.
- Top bundles transmit `r` base elements. Each intermediate bundle transmits
  `r-1` extension elements, omitting the coordinate determined by its smallest
  parent query. Recover the full row, authenticate it, then check **every** parent
  that projects to it. Finally compare the last folds against `Enc0(final_table)`.
- Canonical base-field components use fixed 8/16 bytes, extension elements 24/32
  bytes in the official profiles. Postcard adds lengths/framing; sizes include all
  framing. Decode rejects out-of-range residues, trailing data and noncanonical
  reserialization. Caller-supplied byte limits gate incoming wire size. This is
  not a general untrusted-network resource-hardening claim.

The transcript's byte-hash adapters use `blake3` directly to provide role prefixes
(leaf `0x00`, node `0x01`, transcript `0x02`); commitment geometry and multiproof
logic are Plonky3's `MerkleTreeMmcs`, not a substitute home-grown Merkle scheme.

## Performance boundaries

No allocations occur per QA block: its transform scratch is fixed stack storage;
Goldilocks WHT uses Plonky3 packing where available. RS twiddles/bit reversal,
Lagrange parity, matrix inverse and diagonal inverse are Setup data. First-round
base-field messages and the first codeword inverse transform stay in the base
field before extension-weighted combination. Block-strided sumcheck subdivides
large lanes into independent parallel chunks. Later message/weight folds reuse
allocations, then release excess capacity at level boundaries. Intermediate encoder
butterflies update disjoint lanes in place; only the final bundle layout uses a
second output buffer.

Default Setup retains both diagonal directions; default open retains all intermediate
trees. Explicit `setup_low_memory` retains one direction. `prepare_commit` and
`prepare_open` invert the same allocation in bounded parallel chunks; missing-direction
calls return a configuration error. Parameter IDs, transcript and wire proof are
identical in both modes. Conversion is separately timed in the CLI.

`open_with_options(recompute_first_tree=true)` drops the first intermediate tree
once its successor matrix exists. After queries are drawn, it opens and drops all
retained smaller trees before reconstructing the first tree from the top matrix and
saved first folding challenges. Reconstruction, hashing and path opening are inside
open time. This is a specific time/memory tradeoff, not constant-memory streaming.
The resource model uses Plonky3's full retained digest payload, all live matrices,
diagonals, messages, weights and transient shrink/fold buffers, plus explicit
headroom. Actual RSS must still be measured.
