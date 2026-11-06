//! Interleaved quadratic sumcheck, natural message ordering and block stride.
use p3_field::{Algebra, Field};
use rayon::prelude::*;
/// Boolean equality weights. Bit i corresponds to coordinate `z[i]`.
pub fn eq_table<E: Field>(z: &[E]) -> Vec<E> {
    let mut table = vec![E::ZERO; 1 << z.len()];
    table[0] = E::ONE;
    let mut n = 1;
    for &x in z {
        let (lo, rest) = table.split_at_mut(n);
        let hi = &mut rest[..n];
        lo.par_iter_mut().zip(hi.par_iter_mut()).for_each(|(a, b)| {
            let v = *a;
            *a = v * (E::ONE - x);
            *b = v * x;
        });
        n *= 2;
    }
    table
}
/// Evaluate a base/extension message without materializing a full extension eq table.
pub fn evaluate<T: Field, E: Field + Algebra<T>>(msg: &[T], z: &[E]) -> E {
    assert_eq!(msg.len(), 1 << z.len());
    let split = z.len() / 2;
    let low = eq_table(&z[..split]);
    let high = eq_table(&z[split..]);
    msg.par_chunks_exact(low.len())
        .zip(high.par_iter())
        .map(|(m, &h)| m.iter().zip(&low).map(|(&f, &w)| w * f).sum::<E>() * h)
        .sum()
}
/// Construct only one N-sized table; four sqrt(N)-sized factors keep peak memory bounded.
pub fn combined_weights<E: Field>(z0: &[E], z: &[E], gamma: E) -> Vec<E> {
    assert_eq!(z0.len(), z.len());
    let split = z.len() / 2;
    let a = eq_table(&z0[..split]);
    let b = eq_table(&z0[split..]);
    let c = eq_table(&z[..split]);
    let d = eq_table(&z[split..]);
    let mut out = vec![E::ZERO; 1 << z.len()];
    out.par_chunks_exact_mut(a.len())
        .enumerate()
        .for_each(|(j, row)| {
            let gd = gamma * d[j];
            for w in 0..row.len() {
                row[w] = a[w] * b[j] + c[w] * gd;
            }
        });
    out
}
/// Degree <=2 evaluations at 0,1,2, pairing even/odd BLOCKS, not adjacent scalars.
pub fn round_values<T: Field, E: Field + Algebra<T>>(f: &[T], w: &[E], stride: usize) -> [E; 3] {
    assert_eq!(f.len(), w.len());
    assert!(f.len().is_multiple_of(2 * stride));
    f.par_chunks_exact(2 * stride)
        .zip(w.par_chunks_exact(2 * stride))
        .map(|(f, w)| {
            let (f0, f1) = f.split_at(stride);
            let (w0, w1) = w.split_at(stride);
            f0.par_chunks(4096)
                .zip(f1.par_chunks(4096))
                .zip(w0.par_chunks(4096))
                .zip(w1.par_chunks(4096))
                .map(|(((a, b), c), d)| {
                    let mut g = [E::ZERO; 3];
                    for j in 0..a.len() {
                        g[0] += c[j] * a[j];
                        g[1] += d[j] * b[j];
                        g[2] += (d[j].double() - c[j]) * (b[j].double() - a[j]);
                    }
                    g
                })
                .reduce(
                    || [E::ZERO; 3],
                    |a, b| core::array::from_fn(|i| a[i] + b[i]),
                )
        })
        .reduce(
            || [E::ZERO; 3],
            |a, b| core::array::from_fn(|i| a[i] + b[i]),
        )
}
/// Allocate only when lifting the first base-field message to E (and in references).
pub fn fold_table<T: Field, E: Field + Algebra<T>>(f: &[T], stride: usize, alpha: E) -> Vec<E> {
    assert!(f.len().is_multiple_of(2 * stride));
    let mut out = vec![E::ZERO; f.len() / 2];
    let one_minus = E::ONE - alpha;
    out.par_chunks_exact_mut(stride)
        .zip(f.par_chunks_exact(2 * stride))
        .for_each(|(dst, src)| {
            let (lo, hi) = src.split_at(stride);
            dst.par_chunks_mut(4096)
                .zip(lo.par_chunks(4096))
                .zip(hi.par_chunks(4096))
                .for_each(|((dst, a), b)| {
                    for j in 0..dst.len() {
                        dst[j] = one_minus * a[j] + alpha * b[j];
                    }
                });
        });
    out
}
/// Fold in disjoint even/odd block pairs, then compact the computed even blocks.
/// The phase barrier is necessary: parallel writes directly to the compacted prefix
/// could otherwise overwrite another worker's source. truncate retains capacity.
pub fn fold_table_in_place<E: Field>(f: &mut Vec<E>, stride: usize, alpha: E) {
    assert!(f.len().is_multiple_of(2 * stride));
    let one_minus = E::ONE - alpha;
    f.par_chunks_exact_mut(2 * stride).for_each(|pair| {
        let (lo, hi) = pair.split_at_mut(stride);
        lo.par_chunks_mut(4096)
            .zip(hi.par_chunks(4096))
            .for_each(|(a, b)| {
                for j in 0..a.len() {
                    a[j] = one_minus * a[j] + alpha * b[j];
                }
            });
    });
    let new_len = f.len() / 2;
    for block in 1..new_len / stride {
        f.copy_within(2 * block * stride..(2 * block + 1) * stride, block * stride);
    }
    f.truncate(new_len);
}
pub fn eval_round<E: Field>(g: [E; 3], x: E) -> E {
    let c2 = (g[2] - g[1].double() + g[0]).halve();
    let c1 = g[1] - g[0] - c2;
    g[0] + x * (c1 + x * c2)
}
pub fn final_weighted_sum<E: Field>(
    f0: &[E],
    z0: &[E],
    z: &[E],
    alpha_coords: &[E],
    mu0: usize,
    gamma: E,
) -> E {
    let high = |a: &[E]| {
        (mu0..a.len())
            .map(|j| (E::ONE - a[j]) * (E::ONE - alpha_coords[j]) + a[j] * alpha_coords[j])
            .product::<E>()
    };
    let a = eq_table(&z0[..mu0]);
    let b = eq_table(&z[..mu0]);
    let (h0, h1) = (high(z0), gamma * high(z));
    f0.iter()
        .enumerate()
        .map(|(j, &f)| f * (a[j] * h0 + b[j] * h1))
        .sum()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::beta;
    use p3_field::{PrimeCharacteristicRing, extension::CubicTrinomialExtensionField as Cubic};
    use p3_goldilocks::Goldilocks as F;
    type E = Cubic<F>;
    #[test]
    fn eq_and_evaluation_match_naive() {
        let z: Vec<E> = (2..7).map(E::from_usize).collect();
        let table = eq_table(&z);
        for (j, &actual) in table.iter().enumerate() {
            let expected: E = z
                .iter()
                .enumerate()
                .map(|(s, &x)| if j >> s & 1 == 1 { x } else { E::ONE - x })
                .product();
            assert_eq!(actual, expected);
        }
        let f: Vec<F> = (0..32).map(F::from_usize).collect();
        let expected: E = f.iter().zip(&table).map(|(&a, &b)| b * a).sum();
        assert_eq!(evaluate(&f, &z), expected);
        assert_eq!(
            combined_weights(&z, &z, E::TWO),
            table.iter().map(|&x| x * E::from_u8(3)).collect::<Vec<_>>()
        );
    }
    #[test]
    fn parallel_strided_in_place_matches_reference() {
        use crate::sampling::{SampleBase, stream};
        use p3_field::BasedVectorSpace;
        let mut rng = stream(&[61; 32], b"large-strided-fold", 0);
        let alpha = E::from_basis_coefficients_fn(|_| F::sample(&mut rng));
        for stride in [1, 17, 8193] {
            for pairs in [1, 2, 4] {
                let f: Vec<E> = (0..2 * stride * pairs)
                    .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
                    .collect();
                let w: Vec<E> = (0..f.len())
                    .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
                    .collect();
                let expected: Vec<E> = f
                    .chunks_exact(2 * stride)
                    .flat_map(|p| {
                        (0..stride).map(move |j| (E::ONE - alpha) * p[j] + alpha * p[stride + j])
                    })
                    .collect();
                let mut g = [E::ZERO; 3];
                for (fp, wp) in f.chunks_exact(2 * stride).zip(w.chunks_exact(2 * stride)) {
                    for j in 0..stride {
                        for (t, v) in g.iter_mut().enumerate() {
                            let t = E::from_usize(t);
                            *v += ((E::ONE - t) * fp[j] + t * fp[stride + j])
                                * ((E::ONE - t) * wp[j] + t * wp[stride + j]);
                        }
                    }
                }
                for threads in [1, 4, 32] {
                    rayon::ThreadPoolBuilder::new()
                        .num_threads(threads)
                        .build()
                        .unwrap()
                        .install(|| {
                            assert_eq!(round_values(&f, &w, stride), g);
                            assert_eq!(fold_table(&f, stride, alpha), expected);
                            let mut actual = f.clone();
                            let capacity = actual.capacity();
                            fold_table_in_place(&mut actual, stride, alpha);
                            assert_eq!(actual, expected);
                            assert_eq!(actual.capacity(), capacity);
                        });
                }
            }
        }
    }
    #[test]
    fn sumcheck_matches_naive_hypercube_and_beta() {
        for nu in 1usize..=3 {
            let r = 1 << nu;
            let stride = 4;
            let original: Vec<E> = (0..r * stride).map(|i| E::from_usize(i * i + 1)).collect();
            let mut f = original.clone();
            let mut w: Vec<E> = (0..f.len()).map(|i| E::from_usize(i + 3)).collect();
            let alpha: Vec<E> = (0..nu).map(|s| E::from_usize(s + 4)).collect();
            for &a in &alpha {
                let g = round_values(&f, &w, stride);
                for t in [E::ZERO, E::ONE, E::TWO, E::from_u8(11)] {
                    let mut expected = E::ZERO;
                    for block in 0..f.len() / (2 * stride) {
                        for j in 0..stride {
                            let i = block * 2 * stride + j;
                            let fv = (E::ONE - t) * f[i] + t * f[i + stride];
                            let wv = (E::ONE - t) * w[i] + t * w[i + stride];
                            expected += fv * wv;
                        }
                    }
                    assert_eq!(eval_round(g, t), expected);
                }
                f = fold_table(&f, stride, a);
                w = fold_table(&w, stride, a);
            }
            let b = beta(&alpha);
            for j in 0..stride {
                let expect: E = (0..r).map(|a| b[a] * original[a * stride + j]).sum();
                assert_eq!(f[j], expect);
            }
        }
    }
}
