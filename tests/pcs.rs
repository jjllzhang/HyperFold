use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    params::{BaseCodeKind, HyperFoldConfig},
    pcs::{FirstTreeStrategy, HyperFoldPcs, OpenOptions, Proof},
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

#[test]
fn immutable_statement_and_public_parameter_binding() {
    type F = Goldilocks;
    type E = CubicTrinomialExtensionField<F>;
    let config = HyperFoldConfig::testing(F::KIND, BaseCodeKind::Rs, 8, 2, 4, 2, 3).unwrap();
    let pcs = HyperFoldPcs::<F, E>::setup(config, [31; 32]).unwrap();
    let mut rng = stream(&[3; 32], b"statement-test", 0);
    let msg = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
    let (cm, data) = pcs.commit(msg).unwrap();
    let z = point::<F, E>(config.mu, &mut rng);
    let y = evaluate(&data.message, &z);
    let (p, _) = pcs.open(&cm, &data, &z, y).unwrap();
    let wrong_setup = HyperFoldPcs::<F, E>::setup(config, [32; 32]).unwrap();
    assert!(wrong_setup.verify(&cm, &z, y, &p).is_err());
    let mut wrong_z = z.clone();
    wrong_z[0] += E::ONE;
    assert!(pcs.verify(&cm, &wrong_z, y, &p).is_err());
    assert!(pcs.verify(&cm, &z[..z.len() - 1], y, &p).is_err());
    let formal = HyperFoldConfig::experiment(F::KIND, BaseCodeKind::Rs, 20, 8, 2).unwrap();
    assert!(HyperFoldPcs::<F, BinomialExtensionField<F, 2>>::setup(formal, [0; 32]).is_err());
}

#[test]
fn low_memory_matches_retained_bytes_and_transcript() {
    fn check<F: SampleBase, E: ExtensionField<F> + AlgebraIdentity<F>>() {
        let mut rng = stream(&[81; 32], b"memory-modes", 0);
        for r in [2usize, 4, 8] {
            for depth in [1, 2, 3, 4] {
                for c in [2, 4] {
                    for kind in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
                        let cfg = HyperFoldConfig::testing(
                            F::KIND,
                            kind,
                            2 + depth * r.ilog2() as usize,
                            2,
                            r,
                            c,
                            <E as BasedVectorSpace<F>>::DIMENSION,
                        )
                        .unwrap();
                        let regular = HyperFoldPcs::<F, E>::setup(cfg, [41; 32]).unwrap();
                        let mut low =
                            HyperFoldPcs::<F, E>::setup_low_memory(cfg, [41; 32]).unwrap();
                        assert_eq!(regular.id, low.id);
                        let msg: Vec<F> = (0..cfg.k()).map(|_| F::sample(&mut rng)).collect();
                        let (cm, data) = regular.commit(msg.clone()).unwrap();
                        let (cm_low, data_low) = low.commit(msg).unwrap();
                        assert_eq!(codec::encode(&cm).unwrap(), codec::encode(&cm_low).unwrap());
                        assert!(low.open(&cm_low, &data_low, &[], E::ZERO).is_err());
                        low.prepare_open();
                        assert!(
                            low.diagonals
                                .levels
                                .iter()
                                .all(|l| l.forward.capacity() == 0)
                        );
                        for _ in 0..2 {
                            let z = point::<F, E>(cfg.mu, &mut rng);
                            let y = evaluate(&data.message, &z);
                            let (p, mut t) = regular.open(&cm, &data, &z, y).unwrap();
                            let (q, mut u) = low
                                .open_with_options(
                                    &cm_low,
                                    &data_low,
                                    &z,
                                    y,
                                    OpenOptions {
                                        recompute_first_tree: true,
                                    },
                                )
                                .unwrap();
                            assert_eq!(codec::encode(&p).unwrap(), codec::encode(&q).unwrap());
                            let mut v = low.verify(&cm_low, &z, y, &q).unwrap();
                            let a = t.sample::<F, E>(b"memory-agreement");
                            assert_eq!(a, u.sample::<F, E>(b"memory-agreement"));
                            assert_eq!(a, v.sample::<F, E>(b"memory-agreement"));
                        }
                        low.prepare_commit();
                        assert!(
                            low.diagonals
                                .levels
                                .iter()
                                .all(|l| l.inverse.capacity() == 0)
                        );
                        for (a, b) in regular.diagonals.levels.iter().zip(&low.diagonals.levels) {
                            assert_eq!(a.forward, b.forward);
                        }
                        let (cm2, _) = low.commit(data_low.message).unwrap();
                        assert_eq!(codec::encode(&cm).unwrap(), codec::encode(&cm2).unwrap());
                    }
                }
            }
        }
    }
    check::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
    check::<Mersenne127, M127Quad>();
}

#[test]
fn queried_row_strategies_match_native_bytes_transcript_and_tampering() {
    fn check<F: SampleBase, E: ExtensionField<F> + AlgebraIdentity<F>>() {
        let mut rng = stream(&[94; 32], b"query-row-pcs", 0);
        for r in [2usize, 4, 8] {
            for c in [2, 4] {
                for depth in [1, 2, 3, 4] {
                    for backend in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
                        let cfg = HyperFoldConfig::testing(
                            F::KIND,
                            backend,
                            2 + depth * r.ilog2() as usize,
                            2,
                            r,
                            c,
                            E::DIMENSION,
                        )
                        .unwrap();
                        let pcs = HyperFoldPcs::<F, E>::setup(cfg, [48; 32]).unwrap();
                        let msg = (0..cfg.k()).map(|_| F::sample(&mut rng)).collect();
                        let (cm, data) = pcs.commit(msg).unwrap();
                        for _ in 0..2 {
                            let z = point::<F, E>(cfg.mu, &mut rng);
                            let y = evaluate(&data.message, &z);
                            let (native, mut t) = pcs.open(&cm, &data, &z, y).unwrap();
                            let expected = codec::encode(&native).unwrap();
                            let challenge = t.sample::<F, E>(b"query-row-agreement");
                            for strategy in [
                                FirstTreeStrategy::NativeRetained,
                                FirstTreeStrategy::PrehashedRetained,
                                FirstTreeStrategy::PrehashedRecovered,
                            ] {
                                let (proof, mut u, times) =
                                    pcs.open_profiled(&cm, &data, &z, y, strategy).unwrap();
                                assert_eq!(expected, codec::encode(&proof).unwrap());
                                assert_eq!(challenge, u.sample::<F, E>(b"query-row-agreement"));
                                let mut v = pcs.verify(&cm, &z, y, &proof).unwrap();
                                assert_eq!(challenge, v.sample::<F, E>(b"query-row-agreement"));
                                if depth == 1 {
                                    assert!(times.first_tree_query.is_zero());
                                } else {
                                    let mut bad = proof.clone();
                                    bad.inner[0].rows[0][0] += E::ONE;
                                    assert!(pcs.verify(&cm, &z, y, &bad).is_err());
                                    let mut bad = proof.clone();
                                    if let Some(sibling) =
                                        bad.inner[0].paths.sibling_hashes.first_mut()
                                    {
                                        sibling[0] ^= 1;
                                        assert!(pcs.verify(&cm, &z, y, &bad).is_err());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    check::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
    check::<Mersenne127, M127Quad>();
}

#[test]
fn queried_row_pcs_matches_one_four_and_thirty_two_threads() {
    type F = Mersenne127;
    type E = M127Quad;
    let cfg = HyperFoldConfig::testing(F::KIND, BaseCodeKind::Qa, 14, 2, 4, 4, 2).unwrap();
    let pcs = HyperFoldPcs::<F, E>::setup(cfg, [49; 32]).unwrap();
    let mut rng = stream(&[95; 32], b"query-row-parallel", 0);
    let (cm, data) = pcs
        .commit((0..cfg.k()).map(|_| F::sample(&mut rng)).collect())
        .unwrap();
    let z = point::<F, E>(cfg.mu, &mut rng);
    let y = evaluate(&data.message, &z);
    let reference = codec::encode(&pcs.open(&cm, &data, &z, y).unwrap().0).unwrap();
    for threads in [1, 4, 32] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        for strategy in [
            FirstTreeStrategy::PrehashedRetained,
            FirstTreeStrategy::PrehashedRecovered,
        ] {
            let p = pool.install(|| {
                pcs.open_with_strategy(&cm, &data, &z, y, strategy)
                    .unwrap()
                    .0
            });
            assert_eq!(reference, codec::encode(&p).unwrap());
        }
    }
}
