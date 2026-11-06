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
    fn lift_fold(self, b: Self, alpha: Self) -> Self {
        let a = Self::from_words([self.words()[0], 0]);
        let b = Self::from_words([b.words()[0], 0]);
        a.add(b.sub(a).mul(alpha))
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
    fn lift_fold(self, b: Self, alpha: Self) -> Self {
        self + (b - self) * alpha
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
    fn lift_fold(self, b: Self, alpha: Self) -> Self {
        let a: Mersenne127 = self.as_basis_coefficients_slice()[0];
        let b: Mersenne127 = b.as_basis_coefficients_slice()[0];
        Self::from(a) + alpha * (b - a)
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
    fn lift_fold(self, b: Self, alpha: Self) -> Self {
        self + (b - self) * alpha
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
    fn lift_fold(self, b: Self, alpha: Self) -> Self {
        let a = self.c0;
        let b = b.c0;
        let mut x = alpha;
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
        alpha: *const u64,
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
fn rust_batch<T: Element>(k: Kernel, a: &[T], b: &[T], out: &mut [T], scratch: &mut [T], alpha: T) {
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
                rust_batch(k, a, b, o, s, alpha);
            }
            return;
        }
    }
    let one = T::from_words([1, 0]);
    let zero = T::from_words([0, 0]);
    macro_rules! each {
        ($x:ident,$y:ident,$body:expr) => {{
            for ((&$x, &$y), o) in a.iter().zip(b).zip(out) {
                let _ = $y;
                *o = $body;
            }
        }};
    }
    match k {
        Kernel::Add => each!(x, y, x.add(y)),
        Kernel::Sub => each!(x, y, x.sub(y)),
        Kernel::Mul => each!(x, y, x.mul(y)),
        Kernel::Square => each!(x, y, x.square()),
        Kernel::Inverse => each!(x, y, x.inverse()),
        Kernel::Halve => each!(x, y, x.halve()),
        Kernel::Neg => each!(x, y, x.neg()),
        Kernel::Mixed => each!(x, y, x.mixed(y)),
        Kernel::Fold => each!(x, y, x.add(y.sub(x).mul(alpha))),
        Kernel::LiftFold => each!(x, y, x.lift_fold(y, alpha)),
        Kernel::CanonicalRoundtrip => each!(x, y, T::from_words(x.words())),
        Kernel::Copy => out.copy_from_slice(a),
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
    alpha: T,
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
                (&alpha as *const T).cast(),
            )
        };
    } else {
        rust_batch(k, a, b, out, scratch, alpha)
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
fn challenge() -> [u128; 2] {
    let bytes = blake3::derive_key("hyperfold-m127-compare/full-width-alpha-v2", &[0x57; 32]);
    [0, 1].map(|j| {
        Mersenne127::new(u128::from_le_bytes(
            bytes[j * 16..(j + 1) * 16].try_into().unwrap(),
        ))
        .canonical()
        .max(1)
    })
}
// One explicit set of disjoint preallocated buffers, shared by resident timing
// workers and the parallel correctness adapter; no hidden allocation.
#[allow(clippy::too_many_arguments)]
fn execute<T: Element>(
    k: Kernel,
    a: &[T],
    b: &[T],
    out: &mut [T],
    scratch: &mut [T],
    encoded: &mut [FqBase],
    canonical_input: &[FqBase],
    alpha: T,
    canonical: bool,
) {
    match k {
        Kernel::Encode => {
            if T::FOURQ {
                // FqBase/FqQuad and encoded limbs are explicitly16-aligned; the
                // copy/encode branches do not access the unused scratch argument.
                unsafe {
                    fq_batch(
                        16,
                        2 * T::DIM,
                        a.len(),
                        a.as_ptr().cast(),
                        a.as_ptr().cast(),
                        encoded.as_mut_ptr().cast(),
                        encoded.as_mut_ptr().cast(),
                        1,
                        (&alpha as *const T).cast(),
                    )
                };
            } else {
                for (&x, dst) in a.iter().zip(encoded.chunks_mut(T::DIM)) {
                    for (&v, out) in x.words()[..T::DIM].iter().zip(dst) {
                        *out = FqBase([v as u64, (v >> 64) as u64]);
                    }
                }
            }
        }
        Kernel::Decode => {
            if T::FOURQ {
                unsafe {
                    fq_batch(
                        17,
                        2 * T::DIM,
                        out.len(),
                        canonical_input.as_ptr().cast(),
                        canonical_input.as_ptr().cast(),
                        out.as_mut_ptr().cast(),
                        out.as_mut_ptr().cast(),
                        1,
                        (&alpha as *const T).cast(),
                    )
                };
            } else {
                for (src, dst) in canonical_input.chunks(T::DIM).zip(out) {
                    let mut words = [0u128; 2];
                    for (j, x) in src.iter().enumerate() {
                        words[j] = u128::from(x.0[0]) | (u128::from(x.0[1]) << 64);
                    }
                    *dst = T::from_words(words);
                }
            }
        }
        Kernel::BatchInverseAlloc => {
            let mut allocated = vec![T::from_words([0, 0]); a.len()];
            batch(
                Kernel::BatchInverse,
                a,
                b,
                out,
                &mut allocated,
                canonical,
                alpha,
            );
        }
        _ => batch(k, a, b, out, scratch, canonical, alpha),
    }
}
struct Case<T: Element> {
    a: Vec<T>,
    b: Vec<T>,
    out: Vec<T>,
    scratch: Vec<T>,
    encoded: Vec<FqBase>,
    canonical_input: Vec<FqBase>,
    alpha: T,
}
impl<T: Element> Case<T> {
    fn new(a: &[[u128; 2]], b: &[[u128; 2]]) -> Self {
        Self {
            a: a.iter().map(|&x| T::from_words(x)).collect(),
            b: b.iter().map(|&x| T::from_words(x)).collect(),
            out: vec![T::from_words([0, 0]); a.len()],
            scratch: vec![T::from_words([0, 0]); a.len()],
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
            alpha: T::from_words(challenge()),
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
    fn local_pass(&mut self, k: Kernel, canonical: bool) {
        execute(
            k,
            black_box(&self.a),
            black_box(&self.b),
            &mut self.out,
            &mut self.scratch,
            &mut self.encoded,
            &self.canonical_input,
            self.alpha,
            canonical,
        );
        black_box(&self.out);
        black_box(&self.encoded);
    }
    // Functional checks only: timing uses the synchronous resident workers below.
    fn pass(&mut self, k: Kernel, canonical: bool) {
        let chunk = self.a.len() / 32;
        let alpha = self.alpha;
        self.out
            .par_chunks_mut(chunk)
            .zip(self.scratch.par_chunks_mut(chunk))
            .zip(self.a.par_chunks(chunk))
            .zip(self.b.par_chunks(chunk))
            .zip(self.encoded.par_chunks_mut(chunk * T::DIM))
            .zip(self.canonical_input.par_chunks(chunk * T::DIM))
            .for_each(|(((((o, s), a), b), encoded), input)| {
                execute(k, a, b, o, s, encoded, input, alpha, canonical);
            });
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
enum Job<T: Element> {
    Init(Case<T>),
    Run { kernel: Kernel, warmup: bool },
    Stop,
}
struct WindowResult {
    worker: usize,
    passes: u64,
    quantum: u64,
    kernel_ns: u128,
    wall_ns: u128,
    hash: String,
}
fn measure<T: Element>(
    args: &Args,
    canonical: bool,
    out: &mut std::fs::File,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::sync::{Arc, Barrier, mpsc};
    std::thread::scope(
        |scope| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let ready = Arc::new(Barrier::new(33));
            let go = Arc::new(Barrier::new(33));
            let done = Arc::new(Barrier::new(33));
            let (tx_result, rx_result) = mpsc::channel();
            let mut senders = Vec::new();
            for worker in 0..32 {
                let (tx, rx) = mpsc::sync_channel(1);
                senders.push(tx);
                let (ready, go, done, results) =
                    (ready.clone(), go.clone(), done.clone(), tx_result.clone());
                scope.spawn(move || {
                    let mut state: Option<Case<T>> = None;
                    let mut quantum = 1u64;
                    while let Ok(job) = rx.recv() {
                        match job {
                            Job::Init(c) => state = Some(c),
                            Job::Stop => break,
                            Job::Run { kernel, warmup } => {
                                let c = state.as_mut().expect("initialized worker");
                                if warmup {
                                    c.out.fill(T::from_words([0, 0]));
                                    let before = Instant::now();
                                    c.local_pass(kernel, canonical);
                                    quantum = (1_000_000 / before.elapsed().as_nanos().max(1))
                                        .clamp(1, 4096)
                                        as u64;
                                }
                                // Preparation and calibration finish before the master starts wall time.
                                ready.wait();
                                go.wait();
                                let begin = Instant::now();
                                let duration =
                                    Duration::from_millis(if warmup { 20 } else { args.window_ms });
                                let (mut passes, mut kernel_ns) = (0u64, 0u128);
                                while begin.elapsed() < duration {
                                    let block = Instant::now();
                                    for _ in 0..quantum {
                                        c.local_pass(kernel, canonical);
                                    }
                                    kernel_ns += block.elapsed().as_nanos();
                                    passes += quantum;
                                }
                                let wall_ns = begin.elapsed().as_nanos();
                                done.wait();
                                // Canonical export/hash is strictly after all timed workers finish.
                                let hash = c.result_hash(kernel);
                                results
                                    .send(WindowResult {
                                        worker,
                                        passes,
                                        quantum,
                                        kernel_ns,
                                        wall_ns,
                                        hash,
                                    })
                                    .expect("result reader");
                            }
                        }
                    }
                });
            }
            for &n in &args.sizes {
                assert!(n >= 256 && n.is_power_of_two());
                let (a, b) = inputs(n);
                let whole = Case::<T>::new(&a, &b);
                let ih = blake3::hash(
                    format!(
                        "{}:{}:{}",
                        canonical_hash(&whole.a),
                        canonical_hash(&whole.b),
                        canonical_hash(&[whole.alpha])
                    )
                    .as_bytes(),
                )
                .to_hex()
                .to_string();
                drop(whole);
                for (worker, tx) in senders.iter().enumerate() {
                    tx.send(Job::Init(Case::<T>::new(
                        &a[worker * n / 32..(worker + 1) * n / 32],
                        &b[worker * n / 32..(worker + 1) * n / 32],
                    )))?;
                }
                for kernel in Kernel::ALL {
                    if !args.kernels.is_empty()
                        && !args.kernels.iter().any(|x| x == &format!("{kernel:?}"))
                    {
                        continue;
                    }
                    for window in 0..=args.windows {
                        for tx in &senders {
                            tx.send(Job::Run {
                                kernel,
                                warmup: window == 0,
                            })?;
                        }
                        ready.wait();
                        let begin = Instant::now();
                        go.wait();
                        done.wait();
                        let elapsed = begin.elapsed().as_secs_f64();
                        let mut results = (0..32)
                            .map(|_| rx_result.recv())
                            .collect::<Result<Vec<_>, _>>()?;
                        results.sort_by_key(|r| r.worker);
                        if window == 0 {
                            continue;
                        }
                        let passes = results.iter().map(|r| r.passes).sum::<u64>();
                        let items = passes * (n / 32) as u64;
                        let total_ns = results.iter().map(|r| r.kernel_ns).sum::<u128>();
                        let mut hash = blake3::Hasher::new();
                        for r in &results {
                            hash.update(r.hash.as_bytes());
                        }
                        let list = |values: Vec<String>| values.join(";");
                        writeln!(
                            out,
                            "{},{},{},{},32,{},{},{},{},{:.9},{:.6},{:.6},{},{},{},{},{},{},{}",
                            args.backend,
                            T::DIM,
                            format_args!("{kernel:?}"),
                            n,
                            size_of::<T>(),
                            window,
                            passes,
                            items,
                            elapsed,
                            elapsed * 1e9 / items as f64,
                            total_ns as f64 / items as f64,
                            ih,
                            hash.finalize().to_hex(),
                            list(results.iter().map(|r| r.kernel_ns.to_string()).collect()),
                            list(
                                results
                                    .iter()
                                    .map(|r| (r.passes * (n / 32) as u64).to_string())
                                    .collect()
                            ),
                            list(results.iter().map(|r| r.passes.to_string()).collect()),
                            list(results.iter().map(|r| r.wall_ns.to_string()).collect()),
                            list(results.iter().map(|r| r.quantum.to_string()).collect())
                        )?;
                        out.flush()?;
                    }
                }
            }
            for tx in senders {
                tx.send(Job::Stop)?;
            }
            Ok(())
        },
    )
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
        assert_eq!(
            BigUint::from(f.square().canonical()),
            (BigUint::from(x) * BigUint::from(x)) % &p
        );
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
    println!("challenge={:?}", challenge());
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
        "backend,extension_degree,kernel,elements,threads,bytes_per_element,window,passes,work_items,elapsed_seconds,wall_ns_per_item,mean_worker_ns_per_item,input_hash,result_hash,thread_elapsed_ns,thread_work_items,thread_passes,thread_wall_ns,thread_quantum"
    )?;
    drop(pool);
    match args.backend.as_str() {
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
    }
}
