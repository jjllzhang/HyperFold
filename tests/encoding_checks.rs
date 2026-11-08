//! Independent Newton interpolation and full-codeword coordinate-check references.
use hyperfold_pcs::{
    base_code::{BaseCode, BaseCodeImpl},
    field_m127::{M127Quad, Mersenne127},
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    sampling::{SampleBase, stream},
};
use p3_field::{Algebra, ExtensionField, Field, PrimeCharacteristicRing};
use p3_goldilocks::Goldilocks;

fn newton<T: Field + Algebra<Mersenne127>>(msg: &[T], n: usize) -> Vec<T> {
    let k = msg.len();
    let mut divided = msg.to_vec();
    for order in 1..k {
        let inverse = Mersenne127::from_usize(order).inverse();
        for i in (order..k).rev() {
            divided[i] = (divided[i] - divided[i - 1]) * inverse;
        }
    }
    (0..n)
        .map(|x| {
            let x = Mersenne127::from_usize(x);
            let mut value = divided[k - 1];
            for i in (0..k - 1).rev() {
                value = divided[i] + value * (x - Mersenne127::from_usize(i));
            }
            value
        })
        .collect()
}
fn check<F: SampleBase, T: ExtensionField<F>>(kind: BaseCodeKind, log: usize, c: usize) {
    let cfg = HyperFoldConfig::testing(F::KIND, kind, log + 3, log, 8, c, T::DIMENSION).unwrap();
    let code = BaseCodeImpl::<F>::new(cfg, &[42; 32]).unwrap();
    let mut rng = stream(&[77; 32], b"encoding-check", log as u64);
    let msg = (0..cfg.k0())
        .map(|_| T::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect::<Vec<_>>();
    let mut full = vec![T::ZERO; code.n0()];
    code.encode_slice(&msg, &mut full);
    let all = full.iter().copied().enumerate().collect::<Vec<_>>();
    assert!(code.check_encoded_at(&msg, &all));
    let indices = [0, cfg.k0() - 1, cfg.k0(), code.n0() - 1, cfg.k0(), 0];
    let mut selected = indices
        .iter()
        .rev()
        .map(|&j| (j, full[j]))
        .collect::<Vec<_>>();
    assert!(code.check_encoded_at(&msg, &selected));
    selected.push((0, full[0] + T::ONE));
    assert!(!code.check_encoded_at(&msg, &selected));
    assert!(code.check_encoded_at(&msg, &[]));
    assert!(!code.check_encoded_at(&msg[..msg.len() - 1], &[]));
    let mut oversized = msg.clone();
    oversized.push(T::ZERO);
    assert!(!code.check_encoded_at(&oversized, &all));
    for index in [code.n0(), usize::MAX] {
        assert!(!code.check_encoded_at(&msg, &[(index, T::ZERO)]));
    }
    for j in [0, cfg.k0() - 1, cfg.k0(), code.n0() - 1] {
        assert!(!code.check_encoded_at(&msg, &[(j, full[j] + T::ONE)]));
    }
}
#[test]
fn coordinate_checks_all_encoders_and_message_fields() {
    use p3_field::extension::CubicTrinomialExtensionField;
    for kind in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
        for c in [2, 4] {
            check::<Goldilocks, Goldilocks>(kind, 3, c);
            check::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(kind, 3, c);
            check::<Mersenne127, Mersenne127>(kind, 3, c);
            check::<Mersenne127, M127Quad>(kind, 3, c);
        }
    }
}
fn systematic_newton<T: ExtensionField<Mersenne127>>() {
    for log in [1usize, 2, 3, 6, 7, 8, 10] {
        for c in [2, 4] {
            let cfg = HyperFoldConfig::testing(
                BaseFieldKind::M127,
                BaseCodeKind::Rs,
                log + 3,
                log,
                8,
                c,
                T::DIMENSION,
            )
            .unwrap();
            let code = BaseCodeImpl::<Mersenne127>::new(cfg, &[42; 32]).unwrap();
            let mut rng = stream(&[78; 32], b"newton-reference", log as u64);
            let msg = (0..cfg.k0())
                .map(|_| T::from_basis_coefficients_fn(|_| Mersenne127::sample(&mut rng)))
                .collect::<Vec<_>>();
            let reference = newton(&msg, code.n0());
            let mut full = vec![T::ZERO; code.n0()];
            code.encode_slice(&msg, &mut full);
            assert_eq!(full, reference, "log={log},c={c}");
            assert_eq!(&full[..cfg.k0()], &msg);
            check::<Mersenne127, T>(BaseCodeKind::Rs, log, c);
        }
    }
}
#[test]
fn m127_systematic_rs_matches_independent_newton_reference() {
    systematic_newton::<Mersenne127>();
    systematic_newton::<M127Quad>();
}
