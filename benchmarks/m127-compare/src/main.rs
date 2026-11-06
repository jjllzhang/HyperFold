//! Benchmark-only exact-p comparisons; no alternative PCS production dependency.
use ark_ff::{BigInt, Fp2, Fp2Config, Fp128, MontBackend, MontConfig, MontFp, PrimeField};
use clap::Parser;
use hyperfold_pcs::field_m127::{M127Quad, Mersenne127, P};
use p3_field::BasedVectorSpace;
use rayon::prelude::*;
use std::{
    fs::OpenOptions,
    hint::black_box,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(MontConfig)]
#[modulus = "170141183460469231731687303715884105727"]
#[generator = "43"]
struct ArkConfig;
type AF = Fp128<MontBackend<ArkConfig, 2>>;
struct QuadConfig;
impl Fp2Config for QuadConfig {
    type Fp = AF;
    const NONRESIDUE: AF = MontFp!("-1");
    const FROBENIUS_COEFF_FP2_C1: &[AF] = &[MontFp!("1"), MontFp!("-1")];
    fn mul_fp_by_nonresidue_in_place(x: &mut AF) -> &mut AF {
        *x = -*x;
        x
    }
    fn mul_fp_by_nonresidue_and_add(y: &mut AF, x: &AF) {
        *y = *x - *y;
    }
    fn mul_fp_by_nonresidue_plus_one_and_add(y: &mut AF, x: &AF) {
        *y = *x;
    }
    fn sub_and_mul_fp_by_nonresidue(y: &mut AF, x: &AF) {
        *y += x;
    }
}
type AE = Fp2<QuadConfig>;

trait Element: Copy + Send + Sync + 'static {
    const DIM: usize;
    const FOURQ: bool = false;
    fn from_words(x: [u128; 2]) -> Self;
    fn words(self) -> [u128; 2];
    fn stored_canonical(self) -> bool {
        true
    }
    fn add(self, _b: Self) -> Self {
        unreachable!()
    }
    fn sub(self, _b: Self) -> Self {
        unreachable!()
    }
    fn mul(self, _b: Self) -> Self {
        unreachable!()
    }
    fn square(self) -> Self {
        unreachable!()
    }
    fn inverse(self) -> Self {
        unreachable!()
    }
    fn halve(self) -> Self {
        unreachable!()
    }
    fn neg(self) -> Self {
        unreachable!()
    }
    fn mixed(self, b: Self) -> Self {
        self.mul(Self::from_words([b.words()[0], 0]))
    }
    fn lift_fold(self, b: Self) -> Self {
        let a = Self::from_words([self.words()[0], 0]);
        let b = Self::from_words([b.words()[0], 0]);
        a.add(b.sub(a).mul(Self::from_words([17, 19])))
    }
}
macro_rules! ops {
    ($f:path, $ring:path) => {
        fn add(self, b: Self) -> Self {
            self + b
        }
        fn sub(self, b: Self) -> Self {
            self - b
        }
        fn mul(self, b: Self) -> Self {
            self * b
        }
        fn square(self) -> Self {
            p3_field::PrimeCharacteristicRing::square(&self)
        }
        fn inverse(self) -> Self {
            p3_field::Field::try_inverse(&self)
                .unwrap_or(<Self as p3_field::PrimeCharacteristicRing>::ZERO)
        }
        fn halve(self) -> Self {
            p3_field::PrimeCharacteristicRing::halve(&self)
        }
        fn neg(self) -> Self {
            -self
        }
    };
}
impl Element for Mersenne127 {
    const DIM: usize = 1;
    fn from_words(x: [u128; 2]) -> Self {
        Self::new(x[0])
    }
    fn words(self) -> [u128; 2] {
        [self.canonical(), 0]
    }
    fn mixed(self, b: Self) -> Self {
        self * b
    }
    fn lift_fold(self, b: Self) -> Self {
        self + (b - self) * Self::new(17)
    }
    ops!(p3_field::Field, p3_field::PrimeCharacteristicRing);
}
impl Element for M127Quad {
    const DIM: usize = 2;
    fn from_words(x: [u128; 2]) -> Self {
        Self::new(x.map(Mersenne127::new))
    }
    fn words(self) -> [u128; 2] {
        let x: &[Mersenne127] = self.as_basis_coefficients_slice();
        [x[0].canonical(), x[1].canonical()]
    }
    fn mixed(self, b: Self) -> Self {
        let c: &[Mersenne127] = b.as_basis_coefficients_slice();
        self * c[0]
    }
    fn lift_fold(self, b: Self) -> Self {
        let a: Mersenne127 = self.as_basis_coefficients_slice()[0];
        let b: Mersenne127 = b.as_basis_coefficients_slice()[0];
        Self::from(a) + Self::new([Mersenne127::new(17), Mersenne127::new(19)]) * (b - a)
    }
    ops!(p3_field::Field, p3_field::PrimeCharacteristicRing);
}
fn af_words(x: AF) -> u128 {
    let a = x.into_bigint().0;
    u128::from(a[0]) | (u128::from(a[1]) << 64)
}
macro_rules! ark_ops {
    () => {
        fn add(self, b: Self) -> Self {
            self + b
        }
        fn sub(self, b: Self) -> Self {
            self - b
        }
        fn mul(self, b: Self) -> Self {
            self * b
        }
        fn square(self) -> Self {
            ark_ff::Field::square(&self)
        }
        fn inverse(self) -> Self {
            ark_ff::Field::inverse(&self).unwrap_or_default()
        }
        fn neg(self) -> Self {
            -self
        }
    };
}
impl Element for AF {
    const DIM: usize = 1;
    fn from_words(x: [u128; 2]) -> Self {
        Self::from(x[0])
    }
    fn words(self) -> [u128; 2] {
        [af_words(self), 0]
    }
    fn halve(self) -> Self {
        self * AF::from(1u128 << 126)
    }
    fn mixed(self, b: Self) -> Self {
        self * b
    }
    fn lift_fold(self, b: Self) -> Self {
        self + (b - self) * AF::from(17u64)
    }
    ark_ops!();
}
impl Element for AE {
    const DIM: usize = 2;
    fn from_words(x: [u128; 2]) -> Self {
        Self::new(AF::from(x[0]), AF::from(x[1]))
    }
    fn words(self) -> [u128; 2] {
        [af_words(self.c0), af_words(self.c1)]
    }
    fn mixed(mut self, b: Self) -> Self {
        self.mul_assign_by_fp(&b.c0);
        self
    }
    fn lift_fold(self, b: Self) -> Self {
        let a = self.c0;
        let b = b.c0;
        let mut x = Self::new(AF::from(17u64), AF::from(19u64));
        x.mul_assign_by_fp(&(b - a));
        Self::new(a, AF::default()) + x
    }
    fn halve(self) -> Self {
        Self::new(self.c0.halve(), self.c1.halve())
    }
    ark_ops!();
}

// Explicit limb storage: no casting a Rust u128 representation into C arrays.
#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct FqBase([u64; 2]);
#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct FqQuad([u64; 4]);
impl Element for FqBase {
    const DIM: usize = 1;
    const FOURQ: bool = true;
    fn from_words(x: [u128; 2]) -> Self {
        Self([x[0] as u64, (x[0] >> 64) as u64])
    }
    fn words(self) -> [u128; 2] {
        [
            Mersenne127::new(u128::from(self.0[0]) | (u128::from(self.0[1]) << 64)).canonical(),
            0,
        ]
    }
    fn stored_canonical(self) -> bool {
        (0..1).all(|j| (u128::from(self.0[2 * j]) | (u128::from(self.0[2 * j + 1]) << 64)) < P)
    }
}
impl Element for FqQuad {
    const DIM: usize = 2;
    const FOURQ: bool = true;
    fn from_words(x: [u128; 2]) -> Self {
        Self([
            x[0] as u64,
            (x[0] >> 64) as u64,
            x[1] as u64,
            (x[1] >> 64) as u64,
        ])
    }
    fn words(self) -> [u128; 2] {
        [0, 1].map(|j| {
            Mersenne127::new(u128::from(self.0[2 * j]) | (u128::from(self.0[2 * j + 1]) << 64))
                .canonical()
        })
    }
    fn stored_canonical(self) -> bool {
        (0..2).all(|j| (u128::from(self.0[2 * j]) | (u128::from(self.0[2 * j + 1]) << 64)) < P)
    }
}
unsafe extern "C" {
    fn fq_batch(
        op: u32,
        d: usize,
        n: usize,
        a: *const u64,
        b: *const u64,
        out: *mut u64,
        scratch: *mut u64,
        canonical: i32,
    );
}
#[derive(Clone, Copy, Debug)]
#[repr(u32)]
enum Kernel {
    Add,
    Sub,
    Mul,
    Square,
    Inverse,
    Halve,
    Neg,
    Mixed,
    ChainMul,
    ChainSquare,
    Wht,
    Dot,
    Fold,
    LiftFold,
    Sumcheck,
    BatchInverse,
    CanonicalRoundtrip,
    Copy,
    DotMixed,
    BatchInverse4096,
    BatchInverseAlloc,
    Encode,
    Decode,
}
impl Kernel {
    const ALL: [Self; 23] = [
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Square,
        Self::Inverse,
        Self::Halve,
        Self::Neg,
        Self::Mixed,
        Self::ChainMul,
        Self::ChainSquare,
        Self::Wht,
        Self::Dot,
        Self::Fold,
        Self::LiftFold,
        Self::Sumcheck,
        Self::BatchInverse,
        Self::CanonicalRoundtrip,
        Self::Copy,
        Self::DotMixed,
        Self::BatchInverse4096,
        Self::BatchInverseAlloc,
        Self::Encode,
        Self::Decode,
    ];
}
fn rust_batch<T: Element>(k: Kernel, a: &[T], b: &[T], out: &mut [T], scratch: &mut [T]) {
    if matches!(k, Kernel::BatchInverse | Kernel::BatchInverse4096) {
        let block = if matches!(k, Kernel::BatchInverse4096) {
            4096
        } else {
            1024
        };
        if a.len() > block {
            for (((a, b), o), s) in a
                .chunks(block)
                .zip(b.chunks(block))
                .zip(out.chunks_mut(block))
                .zip(scratch.chunks_mut(block))
            {
                rust_batch(k, a, b, o, s);
            }
            return;
        }
    }
    let one = T::from_words([1, 0]);
    let zero = T::from_words([0, 0]);
    let alpha = T::from_words([17, 19]);
    match k {
        Kernel::Add
        | Kernel::Sub
        | Kernel::Mul
        | Kernel::Square
        | Kernel::Inverse
        | Kernel::Halve
        | Kernel::Neg
        | Kernel::Mixed
        | Kernel::Fold
        | Kernel::LiftFold
        | Kernel::CanonicalRoundtrip
        | Kernel::Copy => {
            for ((&a, &b), o) in a.iter().zip(b).zip(out) {
                *o = match k {
                    Kernel::Add => a.add(b),
                    Kernel::Sub => a.sub(b),
                    Kernel::Mul => a.mul(b),
                    Kernel::Square => a.square(),
                    Kernel::Inverse => a.inverse(),
                    Kernel::Halve => a.halve(),
                    Kernel::Neg => a.neg(),
                    Kernel::Mixed => a.mixed(b),
                    Kernel::Fold => a.add(b.sub(a).mul(alpha)),
                    Kernel::LiftFold => a.lift_fold(b),
                    Kernel::CanonicalRoundtrip => T::from_words(a.words()),
                    Kernel::Copy => a,
                    _ => unreachable!(),
                };
            }
        }
        Kernel::ChainMul => out[0] = a.iter().fold(one, |x, &y| x.mul(y)),
        Kernel::ChainSquare => {
            let mut x = a[0];
            for _ in a {
                x = x.square();
            }
            out[0] = x;
        }
        Kernel::Wht => {
            out.copy_from_slice(a);
            for block in out.chunks_mut(256) {
                let mut half = 1;
                while half < block.len() {
                    for pair in block.chunks_exact_mut(2 * half) {
                        let (l, r) = pair.split_at_mut(half);
                        for (u, v) in l.iter_mut().zip(r) {
                            let (x, y) = (*u, *v);
                            *u = x.add(y);
                            *v = x.sub(y);
                        }
                    }
                    half *= 2;
                }
            }
        }
        Kernel::Dot | Kernel::DotMixed => {
            for ((a, b), o) in a.chunks(256).zip(b.chunks(256)).zip(out.chunks_mut(256)) {
                o[0] = a.iter().zip(b).fold(zero, |sum, (&x, &y)| {
                    sum.add(if matches!(k, Kernel::DotMixed) {
                        x.mixed(y)
                    } else {
                        x.mul(y)
                    })
                });
            }
        }
        Kernel::Sumcheck => {
            out[..3].fill(zero);
            for (f, w) in a.chunks_exact(2).zip(b.chunks_exact(2)) {
                out[0] = out[0].add(f[0].mul(w[0]));
                out[1] = out[1].add(f[1].mul(w[1]));
                out[2] = out[2].add(f[1].add(f[1]).sub(f[0]).mul(w[1].add(w[1]).sub(w[0])));
            }
        }
        Kernel::BatchInverse | Kernel::BatchInverse4096 => {
            scratch[0] = one;
            for i in 1..a.len() {
                scratch[i] = scratch[i - 1].mul(a[i - 1]);
            }
            let mut x = scratch[a.len() - 1].mul(a[a.len() - 1]).inverse();
            for i in (0..a.len()).rev() {
                out[i] = x.mul(scratch[i]);
                x = x.mul(a[i]);
            }
        }
        Kernel::Encode | Kernel::Decode | Kernel::BatchInverseAlloc => {
            unreachable!("allocation in common wrapper")
        }
    }
}
fn batch<T: Element>(
    k: Kernel,
    a: &[T],
    b: &[T],
    out: &mut [T],
    scratch: &mut [T],
    canonical: bool,
) {
    if T::FOURQ {
        assert_eq!(size_of::<T>(), 16 * T::DIM);
        assert!(align_of::<T>() >= 16);
        // FqBase/FqQuad are private repr(C) arrays of u64 limbs. Every supplied slice
        // has equal length and exclusive output/scratch storage. C is synchronous.
        unsafe {
            fq_batch(
                k as u32,
                2 * T::DIM,
                a.len(),
                a.as_ptr().cast(),
                b.as_ptr().cast(),
                out.as_mut_ptr().cast(),
                scratch.as_mut_ptr().cast(),
                canonical as i32,
            )
        };
    } else {
        rust_batch(k, a, b, out, scratch)
    }
}
fn canonical_hash<T: Element>(a: &[T]) -> String {
    let mut h = blake3::Hasher::new();
    for &x in a {
        for &w in &x.words()[..T::DIM] {
            h.update(&w.to_le_bytes());
        }
    }
    h.finalize().to_hex().to_string()
}
fn inputs(n: usize) -> (Vec<[u128; 2]>, Vec<[u128; 2]>) {
    let mut state = 0x7857a18ce39d2b11u128;
    let mut next = || {
        state = state
            .wrapping_mul(0xda942042e4dd58b5)
            .wrapping_add(0x9e3779b97f4a7c15);
        Mersenne127::new(state).canonical().max(1)
    };
    let a = (0..n).map(|_| [next(), next()]).collect();
    let b = (0..n).map(|_| [next(), next()]).collect();
    (a, b)
}
struct Case<T: Element> {
    a: Vec<T>,
    b: Vec<T>,
    out: Vec<T>,
    scratch: Vec<T>,
    worker_ns: Vec<u128>,
    worker_ids: Vec<usize>,
    encoded: Vec<FqBase>,
    canonical_input: Vec<FqBase>,
}
impl<T: Element> Case<T> {
    fn new(a: &[[u128; 2]], b: &[[u128; 2]]) -> Self {
        Self {
            a: a.iter().map(|&x| T::from_words(x)).collect(),
            b: b.iter().map(|&x| T::from_words(x)).collect(),
            out: vec![T::from_words([0, 0]); a.len()],
            scratch: vec![T::from_words([0, 0]); a.len()],
            worker_ns: vec![0; 32],
            worker_ids: vec![0; 32],
            encoded: vec![FqBase([0, 0]); a.len() * T::DIM],
            canonical_input: a
                .iter()
                .flat_map(|v| {
                    v[..T::DIM].iter().map(|&x| {
                        let x = Mersenne127::new(x).canonical();
                        FqBase([x as u64, (x >> 64) as u64])
                    })
                })
                .collect(),
        }
    }
    fn result_hash(&self, k: Kernel) -> String {
        if matches!(k, Kernel::Encode) {
            let mut h = blake3::Hasher::new();
            for block in &self.encoded {
                for limb in block.0 {
                    h.update(&limb.to_le_bytes());
                }
            }
            h.finalize().to_hex().to_string()
        } else {
            canonical_hash(&self.out)
        }
    }
    fn pass(&mut self, k: Kernel, canonical: bool) {
        let chunk = self.a.len() / 32;
        black_box(&self.a);
        black_box(&self.b);
        if matches!(k, Kernel::Encode) {
            self.encoded
                .par_chunks_mut(chunk * T::DIM)
                .zip(self.a.par_chunks(chunk))
                .zip(self.worker_ns.par_iter_mut())
                .zip(self.worker_ids.par_iter_mut())
                .for_each(|(((o, a), elapsed), id)| {
                    *id = rayon::current_thread_index().unwrap();
                    let start = Instant::now();
                    if T::FOURQ {
                        // Encode writes canonical little-endian u64 limbs to repr(C),16-aligned output.
                        unsafe {
                            fq_batch(
                                16,
                                2 * T::DIM,
                                a.len(),
                                a.as_ptr().cast(),
                                a.as_ptr().cast(),
                                o.as_mut_ptr().cast(),
                                o.as_mut_ptr().cast(),
                                1,
                            )
                        };
                    } else {
                        for (&x, dst) in a.iter().zip(o.chunks_mut(T::DIM)) {
                            for (&v, out) in x.words()[..T::DIM].iter().zip(dst) {
                                *out = FqBase([v as u64, (v >> 64) as u64]);
                            }
                        }
                    }
                    *elapsed = start.elapsed().as_nanos();
                });
            black_box(&self.encoded);
            return;
        }
        if matches!(k, Kernel::Decode) {
            black_box(&self.canonical_input);
            self.out
                .par_chunks_mut(chunk)
                .zip(self.canonical_input.par_chunks(chunk * T::DIM))
                .zip(self.worker_ns.par_iter_mut())
                .zip(self.worker_ids.par_iter_mut())
                .for_each(|(((o, a), elapsed), id)| {
                    *id = rayon::current_thread_index().unwrap();
                    let start = Instant::now();
                    if T::FOURQ {
                        // Decode is a native limb copy; all source words are canonical by construction.
                        unsafe {
                            fq_batch(
                                17,
                                2 * T::DIM,
                                o.len(),
                                a.as_ptr().cast(),
                                a.as_ptr().cast(),
                                o.as_mut_ptr().cast(),
                                o.as_mut_ptr().cast(),
                                1,
                            )
                        };
                    } else {
                        for (src, dst) in a.chunks(T::DIM).zip(o) {
                            let mut words = [0u128; 2];
                            for (j, x) in src.iter().enumerate() {
                                words[j] = u128::from(x.0[0]) | (u128::from(x.0[1]) << 64);
                            }
                            *dst = T::from_words(words);
                        }
                    }
                    *elapsed = start.elapsed().as_nanos();
                });
            black_box(&self.out);
            return;
        }
        self.out
            .par_chunks_mut(chunk)
            .zip(self.scratch.par_chunks_mut(chunk))
            .zip(self.a.par_chunks(chunk))
            .zip(self.b.par_chunks(chunk))
            .zip(self.worker_ns.par_iter_mut())
            .zip(self.worker_ids.par_iter_mut())
            .for_each(|(((((o, s), a), b), elapsed), id)| {
                *id = rayon::current_thread_index().unwrap();
                let start = Instant::now();
                if matches!(k, Kernel::BatchInverseAlloc) {
                    let mut allocated = vec![T::from_words([0, 0]); a.len()];
                    batch(Kernel::BatchInverse, a, b, o, &mut allocated, canonical);
                } else {
                    batch(k, a, b, o, s, canonical);
                }
                *elapsed = start.elapsed().as_nanos();
            });
        black_box(&self.out);
    }
}
#[derive(Parser)]
struct Args {
    #[arg(long)]
    backend: String,
    #[arg(long, value_delimiter = ',')]
    kernels: Vec<String>,
    #[arg(long, default_value = "8192,32768,1048576", value_delimiter = ',')]
    sizes: Vec<usize>,
    #[arg(long, default_value_t = 5)]
    windows: usize,
    #[arg(long, default_value_t = 100)]
    window_ms: u64,
    #[arg(long, default_value_t = 32)]
    threads: usize,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    check_only: bool,
}
fn measure<T: Element>(
    args: &Args,
    canonical: bool,
    out: &mut std::fs::File,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for &n in &args.sizes {
        assert!(n >= 256 && n.is_power_of_two());
        let (a, b) = inputs(n);
        let mut c = Case::<T>::new(&a, &b);
        let ih =
            blake3::hash(format!("{}:{}", canonical_hash(&c.a), canonical_hash(&c.b)).as_bytes())
                .to_hex()
                .to_string();
        for k in Kernel::ALL {
            if !args.kernels.is_empty() && !args.kernels.iter().any(|x| x == &format!("{k:?}")) {
                continue;
            }
            c.out.fill(T::from_words([0, 0]));
            c.pass(k, canonical); // warmup; no timed record
            for window in 1..=args.windows {
                let begin = Instant::now();
                let mut passes = 0u64;
                let mut worker_sum = 0u128;
                let mut thread_ns = [0u128; 32];
                let mut thread_items = [0u64; 32];
                while begin.elapsed() < Duration::from_millis(args.window_ms) {
                    c.pass(k, canonical);
                    passes += 1;
                    worker_sum += c.worker_ns.iter().sum::<u128>();
                    for j in 0..32 {
                        thread_ns[c.worker_ids[j]] += c.worker_ns[j];
                        thread_items[c.worker_ids[j]] += (n / 32) as u64;
                    }
                }
                let elapsed = begin.elapsed().as_secs_f64();
                let hash = c.result_hash(k);
                writeln!(
                    out,
                    "{},{},{},{},{},{},{},{},{},{:.9},{:.6},{:.6},{},{},{},{}",
                    args.backend,
                    T::DIM,
                    format_args!("{k:?}"),
                    n,
                    32,
                    size_of::<T>(),
                    window,
                    passes,
                    passes * n as u64,
                    elapsed,
                    elapsed * 1e9 / (passes * n as u64) as f64,
                    worker_sum as f64 / (passes * n as u64) as f64,
                    ih,
                    hash,
                    thread_ns
                        .iter()
                        .map(u128::to_string)
                        .collect::<Vec<_>>()
                        .join(";"),
                    thread_items
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(";")
                )?;
                out.flush()?;
            }
        }
    }
    Ok(())
}
fn check_kernels<A: Element, B: Element>(a: &[[u128; 2]], b: &[[u128; 2]], canonical: bool) {
    let mut x = Case::<A>::new(a, b);
    let mut y = Case::<B>::new(a, b);
    if canonical {
        for item in y.a.iter_mut().chain(&mut y.b) {
            *item = B::from_words(item.words());
        }
    }
    for k in Kernel::ALL {
        x.out.fill(A::from_words([0, 0]));
        y.out.fill(B::from_words([0, 0]));
        x.pass(k, true);
        y.pass(k, canonical);
        if canonical {
            assert!(
                y.out.iter().all(|&v| v.stored_canonical()),
                "canonical output invariant {k:?}"
            );
        }
        assert_eq!(x.result_hash(k), y.result_hash(k), "{k:?} dim{}", A::DIM);
    }
}
fn correctness() {
    use num_bigint::BigUint;
    let (a, b) = inputs(10000);
    let p = BigUint::from(P);
    let edge = [
        0,
        1,
        2,
        P - 1,
        P - 2,
        P, // redundant zero is accepted only by the FourQ checking adapter
        1 << 63,
        1 << 64,
        1 << 126,
        (1 << 64) - 1,
        (1 << 126) - 1,
    ];
    for (x, y) in a.iter().zip(&b).map(|(x, y)| (x[0], y[0])).chain(
        edge.into_iter()
            .flat_map(|x| edge.into_iter().map(move |y| (x, y))),
    ) {
        let f = Mersenne127::new(x);
        let g = Mersenne127::new(y);
        let h = AF::from(x);
        let j = AF::from(y);
        assert_eq!(f.mul(g).words(), h.mul(j).words());
        assert_eq!(
            BigUint::from(f.mul(g).canonical()),
            (BigUint::from(x) * BigUint::from(y)) % &p
        );
        assert_eq!(f.square().words(), h.square().words());
        assert_eq!(f.add(g).words(), h.add(j).words());
        assert_eq!(f.sub(g).words(), h.sub(j).words());
        assert_eq!(f.neg().words(), h.neg().words());
        assert_eq!(f.halve().words(), h.halve().words());
        assert_eq!(f.inverse().words(), h.inverse().words());
    }
    let (a, b) = inputs(16384);
    check_kernels::<Mersenne127, AF>(&a, &b, true);
    check_kernels::<M127Quad, AE>(&a, &b, true);
    for canonical in [false, true] {
        check_kernels::<Mersenne127, FqBase>(&a, &b, canonical);
        check_kernels::<M127Quad, FqQuad>(&a, &b, canonical);
    }
    let a: Vec<_> = (0..256)
        .map(|j| [edge[j % edge.len()], edge[(j + 3) % edge.len()]])
        .collect();
    let b: Vec<_> = (0..256)
        .map(|j| [edge[(j + 5) % edge.len()], edge[(j + 7) % edge.len()]])
        .collect();
    for canonical in [false, true] {
        check_kernels::<Mersenne127, FqBase>(&a, &b, canonical);
        check_kernels::<M127Quad, FqQuad>(&a, &b, canonical);
    }
    const {
        assert!(!<ArkConfig as ark_ff::MontConfig<2>>::CAN_USE_NO_CARRY_MUL_OPT);
    }
    assert_eq!(AF::MODULUS, BigInt([u64::MAX, u64::MAX >> 1]));
    assert_eq!(
        Element::square(AE::new(AF::default(), AF::from(1u64))).words(),
        [P - 1, 0]
    );
    println!(
        "correctness:10000 random+boundary base/BigUint checks;16384-element all-kernel base/extension ark/FourQ native/canonical checks passed; ark no-carry=false"
    );
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    assert!(args.threads > 0);
    if !args.check_only {
        assert_eq!(args.threads, 32, "timings only32threads");
    }
    assert!(args.windows >= 1 && args.window_ms >= 1);
    assert!(
        args.kernels
            .iter()
            .all(|name| Kernel::ALL.iter().any(|k| name == &format!("{k:?}"))),
        "unknown kernel"
    );
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build()?;
    pool.install(correctness);
    if args.check_only {
        return Ok(());
    }
    assert!(
        args.windows >= 5 && args.window_ms >= 100,
        "formal windows >=5 and >=100ms"
    );
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(args.output.as_ref().ok_or("--output required")?)?;
    writeln!(
        file,
        "backend,extension_degree,kernel,elements,threads,bytes_per_element,window,passes,work_items,elapsed_seconds,wall_ns_per_item,mean_worker_ns_per_item,input_hash,result_hash,thread_elapsed_ns,thread_work_items"
    )?;
    pool.install(|| match args.backend.as_str() {
        "ours" => {
            measure::<Mersenne127>(&args, true, &mut file)?;
            measure::<M127Quad>(&args, true, &mut file)
        }
        "ark" => {
            measure::<AF>(&args, true, &mut file)?;
            measure::<AE>(&args, true, &mut file)
        }
        "fourq-native" => {
            measure::<FqBase>(&args, false, &mut file)?;
            measure::<FqQuad>(&args, false, &mut file)
        }
        "fourq-canonical" => {
            measure::<FqBase>(&args, true, &mut file)?;
            measure::<FqQuad>(&args, true, &mut file)
        }
        _ => Err("unknown backend".into()),
    })
}
