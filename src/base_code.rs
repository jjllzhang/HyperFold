//! QA-units WHT, coefficient-message radix-2 RS and evaluation-message Lagrange RS.
//! Base matrices extend linearly to the challenge field; message basis is explicit.
use crate::{
    params::{BaseCodeKind, Error, HyperFoldConfig, Result},
    sampling::{SampleBase, nonzero, stream},
};
use p3_field::{Algebra, Field, PackedValue, batch_multiplicative_inverse};

/// Transform in natural Walsh character order; in odd characteristic H²=nI.
pub fn wht_in_place<T: Field>(buf: &mut [T]) {
    assert!(buf.len().is_power_of_two());
    let mut half = 1;
    while half < buf.len() {
        for chunk in buf.chunks_exact_mut(half * 2) {
            let (lo, hi) = chunk.split_at_mut(half);
            let (lp, ls) = T::Packing::pack_slice_with_suffix_mut(lo);
            let (hp, hs) = T::Packing::pack_slice_with_suffix_mut(hi);
            for (u, v) in lp.iter_mut().zip(hp) {
                let (a, b) = (*u, *v);
                *u = a + b;
                *v = a - b;
            }
            for (u, v) in ls.iter_mut().zip(hs) {
                let (a, b) = (*u, *v);
                *u = a + b;
                *v = a - b;
            }
        }
        half *= 2;
    }
}
pub trait BaseCode<F: Field>: Send + Sync {
    fn k0(&self) -> usize;
    fn n0(&self) -> usize;
    fn encode_slice<T: Field + Algebra<F>>(&self, msg: &[T], out: &mut [T]);
    /// Compare every supplied coordinate against the same complete encoded message.
    /// Invalid shapes/indices fail; duplicate indices are compared independently.
    fn check_encoded_at<T: Field + Algebra<F>>(&self, msg: &[T], expected: &[(usize, T)]) -> bool {
        if msg.len() != self.k0() || expected.iter().any(|&(j, _)| j >= self.n0()) {
            return false;
        }
        let mut encoded = vec![T::ZERO; self.n0()];
        self.encode_slice(msg, &mut encoded);
        expected.iter().all(|&(j, value)| encoded[j] == value)
    }
    fn encode_base_slice(&self, msg: &[F], out: &mut [F]) {
        self.encode_slice(msg, out)
    }
}
#[derive(Debug)]
enum Encoder<F> {
    Qa {
        scaled: Vec<F>,
    },
    Fft {
        bitrev: Vec<usize>,
        twiddles: Vec<Vec<F>>,
    },
    Lagrange {
        parity_transposed: Vec<F>,
    },
}
#[derive(Debug)]
pub struct BaseCodeImpl<F> {
    pub k0: usize,
    pub c: usize,
    encoder: Encoder<F>,
}
impl<F: SampleBase> BaseCodeImpl<F> {
    pub fn new(config: HyperFoldConfig, seed: &[u8; 32]) -> Result<Self> {
        let k0 = config.k0();
        let c = config.c;
        let n = k0 * c;
        if k0 > 1024 {
            return Err(Error::Config("base block too large"));
        }
        let encoder = match config.backend {
            BaseCodeKind::Qa => {
                let mut rng = stream(seed, b"qa-units", 0);
                let scale = F::from_usize(k0).inverse();
                Encoder::Qa {
                    scaled: (0..(c - 1) * k0)
                        .map(|_| nonzero::<F>(&mut rng) * scale)
                        .collect(),
                }
            }
            BaseCodeKind::Rs => {
                if let Some(root) = F::fft_root(n.ilog2() as usize) {
                    let bitrev = (0..n)
                        .map(|i| i.reverse_bits() >> (usize::BITS - n.ilog2()))
                        .collect();
                    let mut twiddles = Vec::new();
                    let mut half = 1;
                    while half < n {
                        let step = root.exp_u64((n / (2 * half)) as u64);
                        twiddles.push(step.powers().take(half).collect());
                        half *= 2;
                    }
                    Encoder::Fft { bitrev, twiddles }
                } else {
                    // Barycentric interpolation from x_a=a to x_(k+b)=k+b, all in base F.
                    let weights: Vec<F> = (0..k0)
                        .map(|a| {
                            (0..k0)
                                .filter(|&i| i != a)
                                .map(|i| F::from_usize(a) - F::from_usize(i))
                                .product::<F>()
                        })
                        .collect();
                    let weights = batch_multiplicative_inverse(&weights);
                    let mut parity = Vec::with_capacity((n - k0) * k0);
                    for y in k0..n {
                        let gaps: Vec<F> = (0..k0).map(|a| F::from_usize(y - a)).collect();
                        let prod: F = gaps.iter().copied().product();
                        let inv = batch_multiplicative_inverse(&gaps);
                        parity.extend((0..k0).map(|a| prod * weights[a] * inv[a]));
                    }
                    Encoder::Lagrange {
                        parity_transposed: parity,
                    }
                }
            }
        };
        Ok(Self { k0, c, encoder })
    }
}
impl<F: Field> BaseCode<F> for BaseCodeImpl<F> {
    fn k0(&self) -> usize {
        self.k0
    }
    fn n0(&self) -> usize {
        self.k0 * self.c
    }
    fn check_encoded_at<T: Field + Algebra<F>>(&self, msg: &[T], expected: &[(usize, T)]) -> bool {
        if msg.len() != self.k0 || expected.iter().any(|&(j, _)| j >= self.n0()) {
            return false;
        }
        if let Encoder::Lagrange { parity_transposed } = &self.encoder {
            // Borrow existing pairs and coefficient rows, preserving all duplicate expectations.
            expected.iter().all(|&(j, value)| {
                if j < self.k0 {
                    msg[j] == value
                } else {
                    let start = (j - self.k0) * self.k0;
                    T::batched_linear_combination(msg, &parity_transposed[start..start + self.k0])
                        == value
                }
            })
        } else {
            // QA and coefficient-message FFT retain the full-encoding fallback.
            let mut encoded = vec![T::ZERO; self.n0()];
            self.encode_slice(msg, &mut encoded);
            expected.iter().all(|&(j, value)| encoded[j] == value)
        }
    }
    fn encode_slice<T: Field + Algebra<F>>(&self, msg: &[T], out: &mut [T]) {
        assert_eq!(msg.len(), self.k0);
        assert_eq!(out.len(), self.n0());
        match &self.encoder {
            Encoder::Qa { scaled } => {
                out[..self.k0].copy_from_slice(msg);
                let mut scratch = [T::ZERO; 1024];
                let scratch = &mut scratch[..self.k0];
                scratch.copy_from_slice(msg);
                wht_in_place(scratch);
                for (dst, g) in out[self.k0..]
                    .chunks_exact_mut(self.k0)
                    .zip(scaled.chunks_exact(self.k0))
                {
                    for ((v, &b), &a) in dst.iter_mut().zip(scratch.iter()).zip(g) {
                        *v = b * a;
                    }
                    wht_in_place(dst);
                }
            }
            Encoder::Fft { bitrev, twiddles } => {
                for (x, &j) in out.iter_mut().zip(bitrev) {
                    *x = if j < msg.len() { msg[j] } else { T::ZERO };
                }
                let mut half = 1;
                for tw in twiddles {
                    for chunk in out.chunks_exact_mut(2 * half) {
                        let (a, b) = chunk.split_at_mut(half);
                        for ((u, v), &w) in a.iter_mut().zip(b).zip(tw) {
                            let x = *u;
                            let y = *v * w;
                            *u = x + y;
                            *v = x - y;
                        }
                    }
                    half *= 2;
                }
            }
            Encoder::Lagrange { parity_transposed } => {
                out[..self.k0].copy_from_slice(msg);
                for (v, row) in out[self.k0..]
                    .iter_mut()
                    .zip(parity_transposed.chunks_exact(self.k0))
                {
                    *v = T::batched_linear_combination(msg, row);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_m127::{M127Quad, Mersenne127};
    use p3_field::{PrimeCharacteristicRing, extension::CubicTrinomialExtensionField};
    use p3_goldilocks::Goldilocks;
    fn check<F: SampleBase, T: p3_field::ExtensionField<F>>() {
        for kind in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
            for c in [2, 4] {
                let config = HyperFoldConfig::testing(F::KIND, kind, 6, 3, 2, c, 3).unwrap();
                let code = BaseCodeImpl::<F>::new(config, &[3; 32]).unwrap();
                let mut rng = stream(&[61; 32], b"base-reference-test", c as u64);
                let msg: Vec<T> = (0..8)
                    .map(|_| {
                        <T as p3_field::BasedVectorSpace<F>>::from_basis_coefficients_fn(|_| {
                            F::sample(&mut rng)
                        })
                    })
                    .collect();
                let mut out = vec![T::ZERO; 8 * c];
                code.encode_slice(&msg, &mut out);
                match &code.encoder {
                    Encoder::Qa { scaled } => {
                        let mut gs = scaled.clone();
                        for g in gs.chunks_exact_mut(8) {
                            wht_in_place(g);
                        }
                        assert_eq!(&out[..8], &msg);
                        for m in 1..c {
                            for w in 0..8 {
                                let naive: T =
                                    (0..8).map(|u| msg[u] * gs[(m - 1) * 8 + (w ^ u)]).sum();
                                assert_eq!(out[m * 8 + w], naive);
                            }
                        }
                    }
                    Encoder::Fft { .. } => {
                        let root = F::fft_root((8 * c).ilog2() as usize).unwrap();
                        for (j, &y) in out.iter().enumerate() {
                            let x = root.exp_u64(j as u64);
                            let naive = msg.iter().rev().fold(T::ZERO, |acc, &a| acc * x + a);
                            assert_eq!(y, naive);
                        }
                    }
                    Encoder::Lagrange { .. } => {
                        for (y, &actual) in out.iter().enumerate() {
                            let mut naive = T::ZERO;
                            for (a, &message_value) in msg.iter().enumerate() {
                                let basis: F = (0..8)
                                    .filter(|&i| i != a)
                                    .map(|i| {
                                        (F::from_usize(y) - F::from_usize(i))
                                            / (F::from_usize(a) - F::from_usize(i))
                                    })
                                    .product();
                                naive += message_value * basis;
                            }
                            assert_eq!(actual, naive);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn base_codes_matches_naive() {
        check::<Goldilocks, Goldilocks>();
        check::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
        check::<Mersenne127, Mersenne127>();
        check::<Mersenne127, M127Quad>();
    }
    #[test]
    fn wht_matches_naive_and_roundtrip() {
        for log in 3..=5 {
            let n = 1 << log;
            let original: Vec<Goldilocks> =
                (0..n).map(|i| Goldilocks::from_usize(i * i + 9)).collect();
            let mut a = original.clone();
            wht_in_place(&mut a);
            for (j, &y) in a.iter().enumerate() {
                let naive: Goldilocks = original
                    .iter()
                    .enumerate()
                    .map(|(i, &x)| if (i & j).count_ones() % 2 == 0 { x } else { -x })
                    .sum();
                assert_eq!(y, naive);
            }
            wht_in_place(&mut a);
            let inv = Goldilocks::from_usize(n).inverse();
            for (x, y) in a.iter().zip(original) {
                assert_eq!(*x * inv, y);
            }
        }
    }
}
