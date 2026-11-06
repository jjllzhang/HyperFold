# Native M127 PCS backend comparison

Experimental package only: production field arithmetic, protocol and Plonky3 stay unchanged. Prime `2^127-1`, quadratic basis `a+bi`, `i²=-1`. Same byte codecs, rejection sampling and protocol identity across all three statically dispatched backends.

| Operation | ours_frozen | ark_adapter | fourq_native_adapter |
|---|---|---|---|
| Base add/sub/mul/neg/square/inverse | Existing Rust M127 | ark-ff 0.6.0 native Fp128 Montgomery | Dedicated scalar C bridge to matching FourQ routine |
| Base halve | Existing Rust | Native multiplication by constant inverse 2 | Native fpmul1271 by constant 2^126 |
| E add/sub/neg/halve | Existing P3 extension | Native coefficient operations | Native coefficient operations |
| E mul/square/inverse | Existing P3 extension | Native ark Fp2 operations | Native fp2mul1271/fp2sqr1271/fp2inv1271 |
| E mixed multiply | Existing P3 | ark Fp2 mul_assign_by_fp | Two native fpmul1271 calls inside one C bridge |

Ark coefficients stay Montgomery internally. E explicitly owns `[Base; 2]`; basis borrowing does not reinterpret the ark Fp2 layout. FourQ uses explicit C-layout `[u64;2]` limbs, retains redundant zero `p` internally, and canonicalizes at wire/Hash/Ord/Display boundaries. Eq and zero detection recognize both zeros. Zero inverse returns None before entering C. External decoding rejects values >=p. No full canonical table clone. Native range closure is subject to the independent Theorist audit before formal timing.

Both adapted base types occupy 16 bytes and E 32 bytes. Ark alignment is library-dependent; FourQ alignment is 16. Scalar packing in all cases. FourQ has one scalar FFI crossing per corresponding field operation (or coefficient operation), with no runtime operation/degree dispatcher. This real integration cost stays inside PCS timings. No cross-language LTO, custom ASM or bulk PCS fusion. FourQ is MIT, fixed commit `1031567f23278e1135b35cc04e5d74c2ac88c029`, selected x64 C/noASM/noAVX macros; upstream is no longer maintained. Ark is a generic same-prime Montgomery instantiation, not a specialized named M127 backend. Its all-ones modulus does not enable the no-carry Montgomery fast path.

```sh
export FOURQ_ROOT=/home/zjl/zkp-research/reference/FourQlib
cargo test --release --manifest-path benchmarks/m127-pcs-compare/Cargo.toml
RUSTFLAGS='-C target-cpu=native' cargo test --release --manifest-path benchmarks/m127-pcs-compare/Cargo.toml
RUSTFLAGS='-C target-cpu=native' cargo build --release --manifest-path benchmarks/m127-pcs-compare/Cargo.toml
```

Correctness tests include random and carry boundaries, inverse/Frobenius, canonical raw/serde bytes, redundant zero contracts, same XOF consumption, Setup/proof/transcript equality, 3×3 cross-backend decoding/verification and tampering rejection. Small PCS configurations cover QA/RS, r2/4/8,c2/4,depth1–4; one-thread checks are functional only. All timings require exactly 32 threads.

The driver always uses the proven one-shot lifecycle: fresh Setup, input/claimed evaluation, commit, drop forward diagonals, consume witness after the first lifted Sumcheck subround, keep first-tree hashes and recover queried rows, verify/serialize, then destroy the caller-owned oracle. Setup, release, serialization, diagnostics, final cleanup and full wall are reported separately; online total includes commit+forward release+open+verify. Open includes witness release and first-tree query work. Proof size includes commitment+evaluation proof. CSV IO is outside per-trial wall. Peak RSS covers the whole process including Setup and warmup.

The current authorized experiment is four representative points (mu20/26,r2c4,QA/RS), three independent rounds with backend order ABC/BCA/CAB, one warmup plus three measured trials per process: 36 processes,108 formal trials. This does not change the production backend or overwrite any historical result. Tests and comparisons are not security proofs.
