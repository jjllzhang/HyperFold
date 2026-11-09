//! Exact H₂ᵀ, corrected H₄ᵀ, Pascal P₈, and optional 37-addition H₈ᵀ operators.
use p3_field::{Algebra, Field};
/// The r=8 matrix. Other arities retain their historical operators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocalMatrixKind {
    #[default]
    Pascal8,
    Add37,
}
impl LocalMatrixKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pascal8 => "pascal8",
            Self::Add37 => "add37",
        }
    }
}
const ADD37_MATRIX: [usize; 64] = [
    26, 3, 6, 25, 31, 46, 30, 2, 21, 4, 5, 23, 30, 38, 28, 3, 16, 16, 7, 16, 30, 23, 23, 11, 43,
    31, 17, 41, 72, 65, 59, 23, 33, 6, 8, 32, 43, 56, 41, 5, 7, 10, 5, 6, 12, 9, 8, 6, 12, 5, 3,
    13, 21, 20, 18, 5, 19, 13, 8, 17, 28, 28, 23, 9,
];
const ADD37_GATES: [(usize, usize); 37] = [
    (6, 4),
    (3, 8),
    (9, 5),
    (7, 8),
    (5, 0),
    (2, 12),
    (12, 10),
    (14, 14),
    (13, 15),
    (16, 15),
    (1, 11),
    (8, 17),
    (19, 9),
    (20, 20),
    (18, 4),
    (22, 3),
    (10, 23),
    (23, 1),
    (25, 13),
    (18, 26),
    (21, 27),
    (28, 21),
    (28, 11),
    (29, 24),
    (30, 0),
    (29, 17),
    (22, 32),
    (29, 32),
    (26, 27),
    (36, 36),
    (34, 24),
    (34, 37),
    (25, 39),
    (30, 39),
    (39, 41),
    (16, 37),
    (43, 32),
];
const ADD37_OUTPUTS: [usize; 8] = [33, 31, 40, 42, 35, 43, 38, 44];
#[derive(Debug)]
pub struct LocalMatrix<F> {
    kind: LocalMatrixKind,
    pub r: usize,
    pub matrix: Vec<F>,
    pub inverse: Vec<F>,
}
fn binom(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    (0..k).fold(1, |a, i| a * (n - i) / (i + 1))
}
impl<F: Field> LocalMatrix<F> {
    pub fn new(r: usize) -> Self {
        Self::new_with_kind(r, LocalMatrixKind::Pascal8)
    }
    /// Add37 is defined only for r=8; the legacy constructor remains unchanged.
    pub fn new_with_kind(r: usize, kind: LocalMatrixKind) -> Self {
        assert!([2, 4, 8].contains(&r));
        assert!(kind == LocalMatrixKind::Pascal8 || r == 8);
        let matrix = if r == 2 {
            [1, 1, 1, 2].map(F::from_usize).to_vec()
        } else if r == 4 {
            [2, 3, 1, 1, 1, 2, 3, 4, 1, 1, 2, 3, 3, 4, 1, 2]
                .map(F::from_usize)
                .to_vec()
        } else if kind == LocalMatrixKind::Add37 {
            ADD37_MATRIX.map(F::from_usize).to_vec()
        } else {
            (0..r)
                .flat_map(|i| (0..r).map(move |j| F::from_usize(binom(i + j, i))))
                .collect()
        };
        let inverse = invert(&matrix, r);
        Self {
            kind,
            r,
            matrix,
            inverse,
        }
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
            8 if self.kind == LocalMatrixKind::Add37 => {
                let mut v = [T::ZERO; 45];
                v[..8].copy_from_slice(x);
                for (i, &(a, b)) in ADD37_GATES.iter().enumerate() {
                    v[i + 8] = v[a] + v[b];
                }
                for (dst, src) in x.iter_mut().zip(ADD37_OUTPUTS) {
                    *dst = v[src];
                }
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
            8 if self.kind == LocalMatrixKind::Pascal8 => {
                pascal_lower(x, true);
                pascal_upper(x, true);
            }
            4 | 8 => {
                let old = *x;
                for (i, v) in x.iter_mut().enumerate().take(self.r) {
                    *v = (0..self.r)
                        .map(|j| old[j] * self.inverse[i * self.r + j])
                        .sum();
                }
            }
            _ => unreachable!(),
        }
        for a in 1..self.r {
            x[a] *= diag_inv[a - 1];
        }
    }
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
fn invert<F: Field>(a: &[F], n: usize) -> Vec<F> {
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
    fn cases() -> [(usize, LocalMatrixKind); 4] {
        [
            (2, LocalMatrixKind::Pascal8),
            (4, LocalMatrixKind::Pascal8),
            (8, LocalMatrixKind::Pascal8),
            (8, LocalMatrixKind::Add37),
        ]
    }
    fn checks<F: SampleBase, T: p3_field::ExtensionField<F>>() {
        for (r, kind) in cases() {
            let h = LocalMatrix::<F>::new_with_kind(r, kind);
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
        for (r, kind) in cases() {
            let h = LocalMatrix::<F>::new_with_kind(r, kind);
            if kind == LocalMatrixKind::Add37 {
                assert_eq!(determinant(h.matrix.clone(), 8), F::from_usize(962));
                assert_eq!(h.matrix, ADD37_MATRIX.map(F::from_usize));
            }
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
    fn add37_gate_count_and_indices() {
        assert_eq!(ADD37_GATES.len(), 37);
        for (i, &(a, b)) in ADD37_GATES.iter().enumerate() {
            assert!(a < 8 + i && b < 8 + i);
        }
        assert!(ADD37_OUTPUTS.iter().all(|&i| i < 45));
    }
    #[test]
    fn all_square_minors_nonzero_both_fields() {
        all_minors::<Goldilocks>();
        all_minors::<Mersenne127>();
    }
}
