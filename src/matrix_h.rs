//! The exact H₂ᵀ, corrected H₄ᵀ, and symmetric Pascal P₈ operators.
use p3_field::{Algebra, Field};
#[derive(Debug)]
pub struct LocalMatrix<F> {
    pub r: usize,
    pub matrix: Vec<F>,
}
fn binom(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    (0..k).fold(1, |a, i| a * (n - i) / (i + 1))
}
impl<F: Field> LocalMatrix<F> {
    pub fn new(r: usize) -> Self {
        assert!([2, 4, 8].contains(&r));
        let matrix = if r == 2 {
            [1, 1, 1, 2].map(F::from_usize).to_vec()
        } else if r == 4 {
            [2, 3, 1, 1, 1, 2, 3, 4, 1, 1, 2, 3, 3, 4, 1, 2]
                .map(F::from_usize)
                .to_vec()
        } else {
            (0..r)
                .flat_map(|i| (0..r).map(move |j| F::from_usize(binom(i + j, i))))
                .collect()
        };
        Self { r, matrix }
    }
    pub fn forward<T: Field + Algebra<F>>(&self, x: &mut [T; 8], diag: &[F]) {
        for a in 1..self.r {
            x[a] *= diag[a - 1];
        }
        match self.r {
            2 => {
                let a = x[0] + x[1];
                x[0] = a;
                x[1] = a + x[1];
            }
            4 => {
                let s01 = x[0] + x[1];
                let s23 = x[2] + x[3];
                let t123 = x[1] + s23;
                let t013 = s01 + x[3];
                let w0 = s01.double() + t123;
                let w2 = s23.double() + t013;
                x[0] = w0;
                x[1] = w2 + t123;
                x[2] = w2;
                x[3] = w0 + t013;
            }
            8 => {
                pascal_upper(x, false);
                pascal_lower(x, false);
            }
            _ => unreachable!(),
        }
    }
    pub fn unscale_inverse<T: Field + Algebra<F>>(&self, x: &mut [T; 8], diag_inv: &[F]) {
        match self.r {
            2 => {
                let b = x[1] - x[0];
                x[0] -= b;
                x[1] = b;
            }
            8 => {
                pascal_lower(x, true);
                pascal_upper(x, true);
            }
            4 => h4_inverse(x),
            _ => unreachable!(),
        }
        for a in 1..self.r {
            x[a] *= diag_inv[a - 1];
        }
    }
}
// Eleven binary additions/subtractions and two exact halvings. This returns
// M^-1 y, not 2 M^-1 y: callers retain their original challenge weights.
#[inline]
fn h4_inverse<T: Field>(x: &mut [T; 8]) {
    let a = x[1] - x[2];
    let b = x[3] - x[0];
    let q = x[0] - a;
    let h = q.halve();
    let b3 = (b + b) + b;
    let t = (x[2] + q) - b3;
    let x2 = t.halve();
    let x0 = (x2 + b) - a;
    let x1 = h - x0;
    let x3 = b - h;
    x[..4].copy_from_slice(&[x0, x1, x2, x3]);
}
fn pascal_lower<T: Field>(x: &mut [T; 8], inverse: bool) {
    if inverse {
        for stage in (0..7).rev() {
            for j in stage + 1..8 {
                x[j] -= x[j - 1];
            }
        }
    } else {
        for stage in 0..7 {
            for j in (stage + 1..8).rev() {
                x[j] += x[j - 1];
            }
        }
    }
}
fn pascal_upper<T: Field>(x: &mut [T; 8], inverse: bool) {
    if inverse {
        for stage in 0..7 {
            for j in (stage + 1..8).rev() {
                x[j - 1] -= x[j];
            }
        }
    } else {
        for stage in (0..7).rev() {
            for j in stage + 1..8 {
                x[j - 1] += x[j];
            }
        }
    }
}
#[cfg(test)]
pub(crate) fn invert<F: Field>(a: &[F], n: usize) -> Vec<F> {
    let mut a = a.to_vec();
    let mut inv = vec![F::ZERO; n * n];
    for i in 0..n {
        inv[i * n + i] = F::ONE;
    }
    for k in 0..n {
        let p = (k..n)
            .find(|&i| !a[i * n + k].is_zero())
            .expect("invertible H");
        for j in 0..n {
            a.swap(k * n + j, p * n + j);
            inv.swap(k * n + j, p * n + j);
        }
        let c = a[k * n + k].inverse();
        for j in 0..n {
            a[k * n + j] *= c;
            inv[k * n + j] *= c;
        }
        for i in 0..n {
            if i != k {
                let t = a[i * n + k];
                for j in 0..n {
                    let ak = a[k * n + j];
                    let ik = inv[k * n + j];
                    a[i * n + j] -= t * ak;
                    inv[i * n + j] -= t * ik;
                }
            }
        }
    }
    inv
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        field_m127::{M127Quad, Mersenne127},
        sampling::{SampleBase, stream},
    };
    use p3_field::extension::CubicTrinomialExtensionField;
    use p3_goldilocks::Goldilocks;
    fn checks<F: SampleBase, T: p3_field::ExtensionField<F>>() {
        for r in [2usize, 4, 8] {
            let h = LocalMatrix::<F>::new(r);
            let mut rng = stream(&[7; 32], b"matrix-test", 0);
            let d: Vec<F> = (0..r - 1)
                .map(|_| crate::sampling::nonzero(&mut rng))
                .collect();
            let dinv: Vec<_> = d.iter().map(Field::inverse).collect();
            for _ in 0..10 {
                let original: [T; 8] = core::array::from_fn(|_| {
                    <T as p3_field::BasedVectorSpace<F>>::from_basis_coefficients_fn(|_| {
                        F::sample(&mut rng)
                    })
                });
                let mut x = original;
                h.forward(&mut x, &d);
                for (i, &actual) in x.iter().enumerate().take(r) {
                    let expect: T = (0..r)
                        .map(|j| {
                            original[j]
                                * h.matrix[i * r + j]
                                * if j == 0 { F::ONE } else { d[j - 1] }
                        })
                        .sum();
                    assert_eq!(actual, expect);
                }
                h.unscale_inverse(&mut x, &dinv);
                assert_eq!(&x[..r], &original[..r]);
            }
        }
    }
    #[test]
    fn circuit_matches_naive_and_roundtrip() {
        checks::<Goldilocks, Goldilocks>();
        checks::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
        checks::<Mersenne127, Mersenne127>();
        checks::<Mersenne127, M127Quad>();
    }
    fn r4_reference<F: SampleBase, T: p3_field::ExtensionField<F>>() {
        let mut rng = stream(&[157; 32], b"r4-reference", 0);
        let h = LocalMatrix::<F>::new(4);
        assert_eq!(
            h.matrix,
            [2usize, 3, 1, 1, 1, 2, 3, 4, 1, 1, 2, 3, 3, 4, 1, 2].map(F::from_usize)
        );
        let inverse = invert(&h.matrix, 4);
        let half = F::TWO.inverse();
        let twice_inverse = [2i64, -3, 4, -1, -1, 2, -3, 1, 4, -1, 2, -3, -3, 1, -1, 2];
        let expected: Vec<F> = twice_inverse
            .iter()
            .map(|&v| {
                let a = F::from_usize(v.unsigned_abs() as usize) * half;
                if v < 0 { -a } else { a }
            })
            .collect();
        assert_eq!(inverse, expected);
        for trial in 0..128 {
            let input: [T; 8] =
                core::array::from_fn(|_| T::from_basis_coefficients_fn(|_| F::sample(&mut rng)));
            let input = match trial {
                0 => [T::ZERO; 8],
                1 => [T::ONE; 8],
                2 => [-T::ONE; 8],
                _ => input,
            };
            let d: Vec<F> = (0..3).map(|_| crate::sampling::nonzero(&mut rng)).collect();
            let di: Vec<F> = d.iter().map(Field::inverse).collect();
            let mut fast = input;
            let mut naive = input;
            h.unscale_inverse(&mut fast, &di);
            for i in 0..4 {
                naive[i] = (0..4).map(|j| input[j] * inverse[i * 4 + j]).sum();
                if i > 0 {
                    naive[i] *= di[i - 1];
                }
            }
            assert_eq!(fast, naive); // arbitrary input, not merely an encoder image
            let mut image = input;
            h.forward(&mut image, &d);
            for (i, &actual) in image.iter().enumerate().take(4) {
                let expected: T = (0..4)
                    .map(|j| {
                        input[j] * h.matrix[i * 4 + j] * if j == 0 { F::ONE } else { d[j - 1] }
                    })
                    .sum();
                assert_eq!(actual, expected);
            }
            assert_eq!(&image[4..], &input[4..]);
            h.unscale_inverse(&mut image, &di);
            assert_eq!(image, input);
        }
    }
    #[test]
    fn r4_fast_matches_literal_and_arbitrary_inverse_four_types() {
        r4_reference::<Goldilocks, Goldilocks>();
        r4_reference::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
        r4_reference::<Mersenne127, Mersenne127>();
        r4_reference::<Mersenne127, M127Quad>();
    }
    fn determinant<F: Field>(mut a: Vec<F>, n: usize) -> F {
        let mut d = F::ONE;
        for k in 0..n {
            let Some(p) = (k..n).find(|&i| !a[i * n + k].is_zero()) else {
                return F::ZERO;
            };
            if p != k {
                d = -d;
                for j in 0..n {
                    a.swap(k * n + j, p * n + j);
                }
            }
            let pivot = a[k * n + k];
            d *= pivot;
            let inv = pivot.inverse();
            for i in k + 1..n {
                let t = a[i * n + k] * inv;
                for j in k + 1..n {
                    let v = a[k * n + j];
                    a[i * n + j] -= t * v;
                }
            }
        }
        d
    }
    fn all_minors<F: Field>() {
        for r in [2usize, 4, 8] {
            let h = LocalMatrix::<F>::new(r);
            let mut count = 0;
            for rm in 1usize..1 << r {
                for cm in 1usize..1 << r {
                    if rm.count_ones() != cm.count_ones() {
                        continue;
                    }
                    let rows: Vec<_> = (0..r).filter(|i| rm >> i & 1 == 1).collect();
                    let cols: Vec<_> = (0..r).filter(|j| cm >> j & 1 == 1).collect();
                    let mut a = Vec::new();
                    for &i in &rows {
                        for &j in &cols {
                            a.push(h.matrix[i * r + j]);
                        }
                    }
                    assert!(
                        !determinant(a, rows.len()).is_zero(),
                        "r={r},rows={rm},cols={cm}"
                    );
                    count += 1;
                }
            }
            if r == 8 {
                assert_eq!(count, 12869);
            }
        }
    }
    #[test]
    fn all_square_minors_nonzero_both_fields() {
        all_minors::<Goldilocks>();
        all_minors::<Mersenne127>();
    }
}
