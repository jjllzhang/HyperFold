use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    params::{BaseCodeKind, HyperFoldConfig},
    pcs::{HyperFoldPcs, Proof},
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::extension::{BinomialExtensionField, CubicTrinomialExtensionField};
use p3_field::{AlgebraIdentity, BasedVectorSpace, ExtensionField, PrimeCharacteristicRing};
use p3_goldilocks::Goldilocks;
fn point<F: SampleBase, E: ExtensionField<F>>(n: usize, rng: &mut blake3::OutputReader) -> Vec<E> {
    (0..n)
        .map(|_| <E as BasedVectorSpace<F>>::from_basis_coefficients_fn(|_| F::sample(rng)))
        .collect()
}
fn matrix<F: SampleBase, E: ExtensionField<F> + AlgebraIdentity<F>>() {
    let mut rng = stream(&[2; 32], b"pcs-test", 0);
    for mu in [9, 10, 11, 12] {
        for r in [2usize, 4, 8] {
            for c in [2, 4] {
                for kind in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
                    let mu0 = 2 + (mu - 2) % (r.ilog2() as usize);
                    let config = HyperFoldConfig::testing(
                        F::KIND,
                        kind,
                        mu,
                        mu0,
                        r,
                        c,
                        <E as BasedVectorSpace<F>>::DIMENSION,
                    )
                    .unwrap();
                    let pcs = HyperFoldPcs::<F, E>::setup(config, [3; 32]).unwrap();
                    let msg: Vec<F> = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
                    let (cm, data) = pcs.commit(msg).unwrap();
                    for _ in 0..2 {
                        let z = point::<F, E>(mu, &mut rng);
                        let y = evaluate(&data.message, &z);
                        let (proof, mut prover) = pcs.open(&cm, &data, &z, y).unwrap();
                        let bytes = codec::encode(&proof).unwrap();
                        let decoded: Proof<F, E> = codec::decode(&bytes, bytes.len()).unwrap();
                        let mut verifier = pcs.verify(&cm, &z, y, &decoded).unwrap();
                        assert_eq!(
                            prover.sample::<F, E>(b"agreement"),
                            verifier.sample::<F, E>(b"agreement")
                        );
                        assert_eq!(bytes, codec::encode(&decoded).unwrap());
                    }
                }
            }
        }
    }
}
#[test]
fn pcs_parameter_matrix_end_to_end_gold_cubic() {
    matrix::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
}
#[test]
fn pcs_parameter_matrix_end_to_end_gold_quad_functional_only() {
    matrix::<Goldilocks, BinomialExtensionField<Goldilocks, 2>>();
}
#[test]
fn pcs_parameter_matrix_end_to_end_m127() {
    matrix::<Mersenne127, M127Quad>();
}
#[test]
fn tamper_rejection_and_strict_codec() {
    type F = Goldilocks;
    type E = CubicTrinomialExtensionField<F>;
    let config = HyperFoldConfig::testing(F::KIND, BaseCodeKind::Qa, 10, 4, 8, 4, 3).unwrap();
    let pcs = HyperFoldPcs::<F, E>::setup(config, [0; 32]).unwrap();
    let mut rng = stream(&[5; 32], b"tamper-test", 0);
    let msg = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
    let (cm, data) = pcs.commit(msg).unwrap();
    let z = point::<F, E>(config.mu, &mut rng);
    let y = evaluate(&data.message, &z);
    let (proof, _) = pcs.open(&cm, &data, &z, y).unwrap();
    pcs.verify(&cm, &z, y, &proof).unwrap();
    assert!(pcs.verify(&cm, &z, y + E::ONE, &proof).is_err());
    assert!(pcs.open(&cm, &data, &z, y + E::ONE).is_err());
    let mut cm_bad = cm.clone();
    cm_bad.y0 += E::ONE;
    assert!(pcs.verify(&cm_bad, &z, y, &proof).is_err());
    cm_bad = cm.clone();
    cm_bad.root[0] ^= 1;
    assert!(pcs.verify(&cm_bad, &z, y, &proof).is_err());
    cm_bad = cm.clone();
    cm_bad.parameters_id[0] ^= 1;
    assert!(pcs.verify(&cm_bad, &z, y, &proof).is_err());
    for mode in 0..10 {
        let mut bad = proof.clone();
        match mode {
            0 => bad.rounds[0][0][0] += E::ONE,
            1 => bad.roots[0][0] ^= 1,
            2 => bad.final_table[0] += E::ONE,
            3 => bad.top.rows[0][0] += F::ONE,
            4 => bad.inner[0].rows[0][0] += E::ONE,
            5 => bad.top.paths.sibling_hashes[0][0] ^= 1,
            6 => {
                bad.inner[0].rows.pop();
            }
            7 => {
                bad.rounds[0].pop();
            }
            8 => {
                bad.top.paths.sibling_hashes.push([0; 32]);
            }
            _ => {
                bad.inner[0].paths.sibling_hashes[0][0] ^= 1;
            }
        }
        assert!(pcs.verify(&cm, &z, y, &bad).is_err(), "mode={mode}");
    }
    let bytes = codec::encode(&proof).unwrap();
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(codec::decode::<Proof<F, E>>(&trailing, trailing.len()).is_err());
    assert!(codec::decode::<Proof<F, E>>(&bytes, bytes.len() - 1).is_err());
    for cut in [0, 1, 32, bytes.len() / 2, bytes.len() - 1] {
        assert!(codec::decode::<Proof<F, E>>(&bytes[..cut], bytes.len()).is_err());
    }
}
#[test]
fn parallel_matches_single_thread() {
    type F = Goldilocks;
    type E = CubicTrinomialExtensionField<F>;
    let config = HyperFoldConfig::testing(F::KIND, BaseCodeKind::Rs, 10, 4, 4, 2, 3).unwrap();
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let pcs = HyperFoldPcs::<F, E>::setup(config, [8; 32]).unwrap();
                let mut rng = stream(&[6; 32], b"parallel-test", 0);
                let msg = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
                let (cm, data) = pcs.commit(msg).unwrap();
                let z = point::<F, E>(config.mu, &mut rng);
                let y = evaluate(&data.message, &z);
                let (p, mut t) = pcs.open(&cm, &data, &z, y).unwrap();
                let mut v = pcs.verify(&cm, &z, y, &p).unwrap();
                assert_eq!(
                    t.sample::<F, E>(b"agreement"),
                    v.sample::<F, E>(b"agreement")
                );
                (codec::encode(&cm).unwrap(), codec::encode(&p).unwrap())
            })
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one, run(32));
}
