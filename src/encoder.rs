//! Message blocks are natural-order contiguous subtrees. Public codeword matrices
//! use rows (pi[j], pi[j+n_prev], ...) and never confuse physical/logical indices.
use crate::{
    base_code::{BaseCode, BaseCodeImpl},
    matrix_h::LocalMatrix,
    params::{DiagonalTables, HyperFoldConfig},
    sampling::SampleBase,
};
use p3_field::{Algebra, Field};
use p3_matrix::dense::RowMajorMatrix;
use rayon::prelude::*;

pub fn encode_top<F: SampleBase, T: Field + Algebra<F>>(
    config: HyperFoldConfig,
    base: &BaseCodeImpl<F>,
    h: &LocalMatrix<F>,
    diags: &DiagonalTables<F>,
    msg: &[T],
    depth: usize,
) -> RowMajorMatrix<T> {
    assert_eq!(msg.len(), config.k0() * (1 << (depth * config.nu())));
    let mut buf = vec![T::ZERO; msg.len() * config.c];
    buf.par_chunks_mut(base.n0())
        .zip(msg.par_chunks(base.k0()))
        .for_each(|(out, m)| base.encode_slice(m, out));
    if depth == 0 {
        return RowMajorMatrix::new(buf, 1);
    }
    for i in 1..=depth {
        let np = config.n(i - 1);
        let ni = config.r * np;
        let diag = &diags.levels[i - 1].forward;
        if i == depth {
            // Only the final layout change needs a second full-size output.
            let mut out = vec![T::ZERO; buf.len()];
            out.par_chunks_exact_mut(config.r)
                .enumerate()
                .for_each(|(j, row)| {
                    let mut x = [T::ZERO; 8];
                    for a in 0..config.r {
                        x[a] = buf[a * np + j];
                    }
                    h.forward(&mut x, &diag[j * (config.r - 1)..(j + 1) * (config.r - 1)]);
                    row.copy_from_slice(&x[..config.r]);
                });
            return RowMajorMatrix::new(out, config.r);
        }
        buf.par_chunks_exact_mut(ni).for_each(|group| {
            let mut lanes: [&mut [T]; 8] = core::array::from_fn(|_| -> &mut [T] { &mut [] });
            let mut remaining = group;
            for lane in lanes.iter_mut().take(config.r) {
                let (head, tail) = remaining.split_at_mut(np);
                *lane = head;
                remaining = tail;
            }
            butterfly_group(lanes, config.r, diag, h);
        });
    }
    RowMajorMatrix::new(buf, config.r)
}
// Each recursion owns disjoint coordinate intervals in every lane. No heap scratch
// and no unsafe aliasing are needed; even a single large subtree exposes many tasks.
fn butterfly_group<F: Field, T: Field + Algebra<F>>(
    lanes: [&mut [T]; 8],
    r: usize,
    diag: &[F],
    h: &LocalMatrix<F>,
) {
    let n = lanes[0].len();
    if n > 4096 {
        let mid = n / 2;
        let mut left: [&mut [T]; 8] = core::array::from_fn(|_| -> &mut [T] { &mut [] });
        let mut right: [&mut [T]; 8] = core::array::from_fn(|_| -> &mut [T] { &mut [] });
        for (a, lane) in lanes.into_iter().enumerate().take(r) {
            let (lo, hi) = lane.split_at_mut(mid);
            left[a] = lo;
            right[a] = hi;
        }
        let (dl, dr) = diag.split_at(mid * (r - 1));
        rayon::join(
            || butterfly_group(left, r, dl, h),
            || butterfly_group(right, r, dr, h),
        );
    } else {
        for j in 0..n {
            let mut x = [T::ZERO; 8];
            for a in 0..r {
                x[a] = lanes[a][j];
            }
            h.forward(&mut x, &diag[j * (r - 1)..(j + 1) * (r - 1)]);
            for a in 0..r {
                lanes[a][j] = x[a];
            }
        }
    }
}
pub fn beta<E: Field>(alpha: &[E]) -> [E; 8] {
    let r = 1 << alpha.len();
    assert!(r <= 8);
    core::array::from_fn(|a| {
        if a >= r {
            return E::ZERO;
        }
        alpha
            .iter()
            .enumerate()
            .map(|(s, &x)| if a >> s & 1 == 1 { x } else { E::ONE - x })
            .product()
    })
}
pub fn fold_row<F: Field, T: Field + Algebra<F>, E: Field + Algebra<T>>(
    h: &LocalMatrix<F>,
    diag_inv: &[F],
    beta: &[E; 8],
    row: &[T],
) -> E {
    let mut x = [T::ZERO; 8];
    x[..h.r].copy_from_slice(row);
    h.unscale_inverse(&mut x, diag_inv);
    (0..h.r).map(|a| beta[a] * x[a]).sum()
}
pub fn fold_level<F: Field, T: Field + Algebra<F>, E: Field + Algebra<T>>(
    config: HyperFoldConfig,
    h: &LocalMatrix<F>,
    diags: &DiagonalTables<F>,
    pi: &RowMajorMatrix<T>,
    i: usize,
    beta: &[E; 8],
) -> RowMajorMatrix<E> {
    let n = config.n(i - 1);
    let diag = &diags.levels[i - 1].inverse;
    let width = if i > 1 { config.r } else { 1 };
    let height = n / width;
    let mut out = vec![E::ZERO; n];
    out.par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(jprime, row)| {
            for (b, v) in row.iter_mut().enumerate() {
                let j = jprime + b * height;
                *v = fold_row(
                    h,
                    &diag[j * (config.r - 1)..(j + 1) * (config.r - 1)],
                    beta,
                    &pi.values[j * config.r..(j + 1) * config.r],
                );
            }
        });
    RowMajorMatrix::new(out, width)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        field_m127::{M127Quad, Mersenne127},
        params::BaseCodeKind,
        sampling::{SampleBase, stream},
    };
    use p3_field::extension::CubicTrinomialExtensionField;
    use p3_goldilocks::Goldilocks;
    fn logical<T: Field>(m: &RowMajorMatrix<T>, r: usize) -> Vec<T> {
        if r == 1 {
            return m.values.clone();
        }
        let n = m.values.len() / r;
        (0..m.values.len())
            .map(|q| m.values[(q % n) * r + q / n])
            .collect()
    }
    fn check<F: SampleBase, E: p3_field::ExtensionField<F>>() {
        let mut rng = stream(&[1; 32], b"fold-test", 0);
        for r in [2usize, 4, 8] {
            for c in [2, 4] {
                for kind in [BaseCodeKind::Rs, BaseCodeKind::Qa] {
                    let mu0 = 2;
                    let mu = mu0 + 2 * r.ilog2() as usize;
                    let config = HyperFoldConfig::testing(F::KIND, kind, mu, mu0, r, c, 3).unwrap();
                    let diags = DiagonalTables::new(config, &[9; 32]);
                    let h = LocalMatrix::new(r);
                    let base = BaseCodeImpl::new(config, &[4; 32]).unwrap();
                    let msg: Vec<F> = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
                    let pi = encode_top(config, &base, &h, &diags, &msg, 2);
                    let alpha: Vec<E> = (0..config.nu())
                        .map(|_| {
                            <E as p3_field::BasedVectorSpace<F>>::from_basis_coefficients_fn(|_| {
                                F::sample(&mut rng)
                            })
                        })
                        .collect();
                    let b = beta(&alpha);
                    let folded = fold_level(config, &h, &diags, &pi, 2, &b);
                    let kp = config.k() / r;
                    let fm: Vec<E> = (0..kp)
                        .map(|w| (0..r).map(|a| b[a] * msg[a * kp + w]).sum())
                        .collect();
                    let expected = encode_top(config, &base, &h, &diags, &fm, 1);
                    assert_eq!(folded.values, expected.values);
                    // Explicit independent recursive generator from basis messages + natural layout;
                    // compare to definition using base matrices and H entries (not optimized circuits).
                    fn naive<F: SampleBase>(
                        config: HyperFoldConfig,
                        base: &BaseCodeImpl<F>,
                        h: &LocalMatrix<F>,
                        diags: &DiagonalTables<F>,
                        msg: &[F],
                        depth: usize,
                    ) -> Vec<F> {
                        if depth == 0 {
                            let mut y = vec![F::ZERO; base.n0()];
                            base.encode_base_slice(msg, &mut y);
                            return y;
                        }
                        let kp = msg.len() / config.r;
                        let sub: Vec<Vec<F>> = msg
                            .chunks(kp)
                            .map(|m| naive(config, base, h, diags, m, depth - 1))
                            .collect();
                        let n = sub[0].len();
                        let d = &diags.levels[depth - 1].forward;
                        let mut out = vec![F::ZERO; config.r * n];
                        for a in 0..config.r {
                            for j in 0..n {
                                out[a * n + j] = (0..config.r)
                                    .map(|t| {
                                        h.matrix[a * config.r + t]
                                            * sub[t][j]
                                            * if t == 0 {
                                                F::ONE
                                            } else {
                                                d[j * (config.r - 1) + t - 1]
                                            }
                                    })
                                    .sum();
                            }
                        }
                        out
                    }
                    assert_eq!(logical(&pi, r), naive(config, &base, &h, &diags, &msg, 2));
                    if r <= 4 {
                        // Explicit G_d from the defining recursive map, not fast circuits.
                        let mut generator = Vec::with_capacity(msg.len() * pi.values.len());
                        let mut basis = vec![F::ZERO; msg.len()];
                        for a in 0..msg.len() {
                            basis[a] = F::ONE;
                            generator.extend(naive(config, &base, &h, &diags, &basis, 2));
                            basis[a] = F::ZERO;
                        }
                        let product: Vec<F> = (0..pi.values.len())
                            .map(|j| {
                                (0..msg.len())
                                    .map(|a| msg[a] * generator[a * pi.values.len() + j])
                                    .sum()
                            })
                            .collect();
                        assert_eq!(logical(&pi, r), product);
                    }
                }
            }
        }
    }
    #[test]
    fn parallel_in_place_butterfly_matches_scalar() {
        fn check_large<F: SampleBase, E: p3_field::ExtensionField<F>>() {
            let mut rng = stream(&[63; 32], b"large-butterfly", 0);
            for r in [2, 4, 8] {
                let n = 8193;
                let h = LocalMatrix::<F>::new(r);
                let diag: Vec<F> = (0..n * (r - 1)).map(|_| F::sample(&mut rng)).collect();
                let original: Vec<E> = (0..n * r)
                    .map(|_| {
                        <E as p3_field::BasedVectorSpace<F>>::from_basis_coefficients_fn(|_| {
                            F::sample(&mut rng)
                        })
                    })
                    .collect();
                let mut expected = original.clone();
                for j in 0..n {
                    let mut x = [E::ZERO; 8];
                    for a in 0..r {
                        x[a] = original[a * n + j];
                    }
                    h.forward(&mut x, &diag[j * (r - 1)..(j + 1) * (r - 1)]);
                    for a in 0..r {
                        expected[a * n + j] = x[a];
                    }
                }
                for threads in [1, 4, 32] {
                    let mut actual = original.clone();
                    rayon::ThreadPoolBuilder::new()
                        .num_threads(threads)
                        .build()
                        .unwrap()
                        .install(|| {
                            let mut lanes: [&mut [E]; 8] =
                                core::array::from_fn(|_| -> &mut [E] { &mut [] });
                            for (lane, chunk) in lanes.iter_mut().zip(actual.chunks_exact_mut(n)) {
                                *lane = chunk;
                            }
                            butterfly_group(lanes, r, &diag, &h);
                        });
                    assert_eq!(actual, expected);
                }
            }
        }
        check_large::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
        check_large::<Mersenne127, M127Quad>();
    }
    #[test]
    fn encoder_and_fold_match_naive() {
        check::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
        check::<Mersenne127, M127Quad>();
    }
}
