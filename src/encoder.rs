//! Message blocks are natural-order contiguous subtrees. Public codeword matrices
//! use rows (`pi[j]`, `pi[j+n_prev]`, ...) and never confuse physical/logical indices.
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
/// Borrow only the retained top codeword and folding parameters; never owns or
/// caches the first intermediate dense codeword. Rows are natural physical rows.
#[derive(Debug)]
pub struct RecoveredFirstMatrix<'a, F, E> {
    top: &'a RowMajorMatrix<F>,
    h: &'a LocalMatrix<F>,
    inverse: &'a [F],
    beta: [E; 8],
    height: usize,
    #[cfg(test)]
    reads: Option<&'a std::sync::atomic::AtomicUsize>,
}
impl<'a, F: Field, E: Field + Algebra<F>> RecoveredFirstMatrix<'a, F, E> {
    pub fn new(
        config: HyperFoldConfig,
        top: &'a RowMajorMatrix<F>,
        h: &'a LocalMatrix<F>,
        inverse: &'a [F],
        beta: [E; 8],
    ) -> Self {
        assert!(config.depth() >= 2);
        assert_eq!(top.width, config.r);
        assert_eq!(top.values.len(), config.n(config.depth()));
        assert_eq!(inverse.len(), config.n(config.depth() - 1) * (config.r - 1));
        Self {
            top,
            h,
            inverse,
            beta,
            height: config.n(config.depth() - 2),
            #[cfg(test)]
            reads: None,
        }
    }
}
impl<F: Field, E: Field + Algebra<F>> p3_matrix::Matrix<E> for RecoveredFirstMatrix<'_, F, E> {
    fn width(&self) -> usize {
        self.h.r
    }
    fn height(&self) -> usize {
        self.height
    }
    unsafe fn row_unchecked(
        &self,
        j: usize,
    ) -> impl IntoIterator<Item = E, IntoIter = impl Iterator<Item = E> + Send + Sync> {
        // Matrix's safe row() bounds j. All indexing here remains checked.
        #[cfg(test)]
        if let Some(reads) = self.reads {
            reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let r = self.h.r;
        let mut row = [E::ZERO; 8];
        for (b, value) in row[..r].iter_mut().enumerate() {
            let q = j + b * self.height;
            *value = fold_row(
                self.h,
                &self.inverse[q * (r - 1)..(q + 1) * (r - 1)],
                &self.beta,
                &self.top.values[q * r..(q + 1) * r],
            );
        }
        row.into_iter().take(r)
    }
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
    #[test]
    fn recovered_first_rows_match_dense_and_only_queries_are_read() {
        use p3_field::ExtensionField;
        use p3_matrix::Matrix;
        use std::sync::atomic::{AtomicUsize, Ordering};
        fn check<F: SampleBase, E: ExtensionField<F>>() {
            let mut rng = stream(&[93; 32], b"query-row-reference", 0);
            for r in [2usize, 4, 8] {
                for c in [2, 4] {
                    for depth in [2, 3, 4] {
                        for backend in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
                            let config = HyperFoldConfig::testing(
                                F::KIND,
                                backend,
                                2 + depth * r.ilog2() as usize,
                                2,
                                r,
                                c,
                                E::DIMENSION,
                            )
                            .unwrap();
                            let base = BaseCodeImpl::new(config, &[44; 32]).unwrap();
                            let h = LocalMatrix::new(r);
                            let diags = DiagonalTables::new(config, &[44; 32]);
                            let msg: Vec<F> =
                                (0..config.k()).map(|_| F::sample(&mut rng)).collect();
                            let top = encode_top(config, &base, &h, &diags, &msg, depth);
                            let alphas: Vec<E> =
                                (0..config.nu()).map(|j| E::from_usize(j + 17)).collect();
                            let b = beta(&alphas);
                            let dense = fold_level(config, &h, &diags, &top, depth, &b);
                            let mut lazy = RecoveredFirstMatrix::new(
                                config,
                                &top,
                                &h,
                                &diags.levels[depth - 1].inverse,
                                b,
                            );
                            let reads = AtomicUsize::new(0);
                            lazy.reads = Some(&reads);
                            for j in 0..dense.height() {
                                assert_eq!(
                                    lazy.row(j).unwrap().into_iter().collect::<Vec<_>>(),
                                    dense.row(j).unwrap().into_iter().collect::<Vec<_>>()
                                );
                            }
                            assert!(lazy.row(lazy.height()).is_none());
                            let hashes = crate::merkle_commitment::hash_leaves(&dense);
                            let (root, native) = crate::merkle_commitment::commit(dense.clone());
                            let mut indices =
                                vec![0, 1, 2, lazy.height() / 2, lazy.height() - 1, 1];
                            indices.sort_unstable();
                            indices.dedup();
                            let expected = crate::merkle_commitment::open(&native, &indices);
                            drop(native);
                            drop(dense); // Neither descriptor nor hash tree owns the dense codeword.
                            reads.store(0, Ordering::Relaxed);
                            let (other, tree) =
                                crate::merkle_commitment::commit_prehashed(lazy, hashes);
                            assert_eq!(root, other);
                            assert_eq!(reads.load(Ordering::Relaxed), 0); // Constructor never enumerates lazy rows.
                            let opening = crate::merkle_commitment::open(&tree, &indices);
                            assert_eq!(reads.load(Ordering::Relaxed), indices.len());
                            assert_eq!(
                                crate::codec::encode(&expected).unwrap(),
                                crate::codec::encode(&opening).unwrap()
                            );
                            crate::merkle_commitment::verify(
                                root,
                                config.n(depth - 2),
                                r,
                                &indices,
                                &opening.rows,
                                &opening.paths,
                            )
                            .unwrap();
                            let mut bad = opening.rows.clone();
                            bad[0][0] += E::ONE;
                            assert!(
                                crate::merkle_commitment::verify(
                                    root,
                                    config.n(depth - 2),
                                    r,
                                    &indices,
                                    &bad,
                                    &opening.paths
                                )
                                .is_err()
                            );
                            let mut swapped = opening.rows.clone();
                            swapped.swap(0, 1);
                            assert!(
                                crate::merkle_commitment::verify(
                                    root,
                                    config.n(depth - 2),
                                    r,
                                    &indices,
                                    &swapped,
                                    &opening.paths
                                )
                                .is_err()
                            );
                            let mut swapped = opening.rows.clone();
                            swapped[0].swap(0, 1);
                            assert!(
                                crate::merkle_commitment::verify(
                                    root,
                                    config.n(depth - 2),
                                    r,
                                    &indices,
                                    &swapped,
                                    &opening.paths
                                )
                                .is_err()
                            );
                            let mut bad = opening.paths.clone();
                            bad.sibling_hashes[0][0] ^= 1;
                            assert!(
                                crate::merkle_commitment::verify(
                                    root,
                                    config.n(depth - 2),
                                    r,
                                    &indices,
                                    &opening.rows,
                                    &bad
                                )
                                .is_err()
                            );
                        }
                    }
                }
            }
        }
        check::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
        check::<Mersenne127, M127Quad>();
    }
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
