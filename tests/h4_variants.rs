//! Same-matrix H4 inverse equivalence and explicitly separated Pascal4 trials.
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    matrix_h::LocalMatrixKind,
    params::{BaseCodeKind, HyperFoldConfig},
    pcs::HyperFoldPcs,
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::ExtensionField;
use p3_field::extension::CubicTrinomialExtensionField;
use p3_goldilocks::Goldilocks;

const KINDS: [LocalMatrixKind; 3] = [
    LocalMatrixKind::Default,
    LocalMatrixKind::H4Dense,
    LocalMatrixKind::Pascal4,
];
fn point<F: SampleBase, E: ExtensionField<F>>(mu: usize) -> Vec<E> {
    let mut rng = stream(&[139; 32], b"r4-matrix-point", mu as u64);
    (0..mu)
        .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect()
}
fn message<F: SampleBase>(k: usize) -> Vec<F> {
    let mut rng = stream(&[138; 32], b"r4-matrix-message", k as u64);
    (0..k).map(|_| F::sample(&mut rng)).collect()
}
fn parameter_matrix<F: SampleBase, E: ExtensionField<F>>() {
    for backend in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
        for c in [2, 4] {
            for depth in [2, 3, 4] {
                let cfg = HyperFoldConfig::testing(
                    F::KIND,
                    backend,
                    2 + 2 * depth,
                    2,
                    4,
                    c,
                    E::DIMENSION,
                )
                .unwrap();
                let msg = message::<F>(cfg.k());
                let z = point::<F, E>(cfg.mu);
                let y = evaluate(&msg, &z);
                let mut default_bytes = None;
                for kind in KINDS {
                    let pcs = HyperFoldPcs::<F, E>::setup_with_local_matrix(cfg, [137; 32], kind)
                        .unwrap();
                    let (cm, data) = pcs.commit(msg.clone()).unwrap();
                    let (proof, mut p) = pcs.open(&cm, &data, &z, y).unwrap();
                    let mut v = pcs.verify(&cm, &z, y, &proof).unwrap();
                    let challenge = p.sample::<F, E>(b"agreement");
                    assert_eq!(challenge, v.sample::<F, E>(b"agreement"));
                    let bytes = (
                        codec::encode(&cm).unwrap(),
                        codec::encode(&proof).unwrap(),
                        challenge,
                    );
                    if kind == LocalMatrixKind::Default {
                        default_bytes = Some(bytes.clone());
                    }
                    if kind == LocalMatrixKind::H4Dense {
                        assert_eq!(default_bytes.as_ref().unwrap(), &bytes);
                    }
                    let mut bad = proof.clone();
                    bad.top.rows[0][0] += F::ONE;
                    assert!(pcs.verify(&cm, &z, y, &bad).is_err());
                    let mut bad = proof.clone();
                    bad.inner[0].rows[0][0] += E::ONE;
                    assert!(pcs.verify(&cm, &z, y, &bad).is_err());
                    let mut second = z.clone();
                    second[0] += E::ONE;
                    let y2 = evaluate(&msg, &second);
                    let (p2, mut t2) = pcs.open(&cm, &data, &second, y2).unwrap();
                    let mut v2 = pcs.verify(&cm, &second, y2, &p2).unwrap();
                    assert_eq!(
                        t2.sample::<F, E>(b"agreement"),
                        v2.sample::<F, E>(b"agreement")
                    );
                    let (oracle, witness) = data.into_parts();
                    let opening = pcs.into_opening_only().unwrap();
                    let (owned, mut t) = opening.open_once(&cm, &oracle, witness, &z, y).unwrap();
                    assert_eq!(bytes.1, codec::encode(&owned).unwrap());
                    let mut v = opening.verify(&cm, &z, y, &owned).unwrap();
                    assert_eq!(
                        t.sample::<F, E>(b"agreement"),
                        v.sample::<F, E>(b"agreement")
                    );
                    if kind == LocalMatrixKind::Pascal4 {
                        let legacy = HyperFoldPcs::<F, E>::setup(cfg, [137; 32]).unwrap();
                        assert!(legacy.verify(&cm, &z, y, &proof).is_err());
                        let (old_cm, old_data) = legacy.commit(msg.clone()).unwrap();
                        assert_ne!(old_cm.parameters_id, cm.parameters_id);
                        let (old_proof, _) = legacy.open(&old_cm, &old_data, &z, y).unwrap();
                        assert!(opening.verify(&old_cm, &z, y, &old_proof).is_err());
                    }
                }
            }
        }
    }
}
#[test]
fn r4_pcs_matrix_transcript_tamper_owned_identity_and_h4_exact_bytes() {
    parameter_matrix::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
    parameter_matrix::<Mersenne127, M127Quad>();
}
fn parallel<F: SampleBase, E: ExtensionField<F>>() {
    for kind in KINDS {
        for backend in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
            let cfg = HyperFoldConfig::testing(F::KIND, backend, 8, 2, 4, 4, E::DIMENSION).unwrap();
            let run = |threads| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .unwrap()
                    .install(|| {
                        let pcs =
                            HyperFoldPcs::<F, E>::setup_with_local_matrix(cfg, [141; 32], kind)
                                .unwrap();
                        let msg = message::<F>(cfg.k());
                        let z = point::<F, E>(cfg.mu);
                        let y = evaluate(&msg, &z);
                        let (cm, data) = pcs.commit(msg).unwrap();
                        let (proof, mut p) = pcs.open(&cm, &data, &z, y).unwrap();
                        let mut v = pcs.verify(&cm, &z, y, &proof).unwrap();
                        assert_eq!(
                            p.sample::<F, E>(b"agreement"),
                            v.sample::<F, E>(b"agreement")
                        );
                        (codec::encode(&cm).unwrap(), codec::encode(&proof).unwrap())
                    })
            };
            let one = run(1);
            assert_eq!(one, run(4));
            assert_eq!(one, run(32));
        }
    }
}
#[test]
fn r4_all_variants_parallel_equal_single_thread_both_fields_backends() {
    parallel::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
    parallel::<Mersenne127, M127Quad>();
}
#[test]
fn nondefault_variants_reject_other_arities() {
    for r in [2, 8] {
        let cfg =
            HyperFoldConfig::testing(Goldilocks::KIND, BaseCodeKind::Qa, 8, 2, r, 2, 3).unwrap();
        for kind in [LocalMatrixKind::H4Dense, LocalMatrixKind::Pascal4] {
            assert!(HyperFoldPcs::<Goldilocks,CubicTrinomialExtensionField<Goldilocks>>::setup_with_local_matrix(cfg,[0;32],kind).is_err());
        }
    }
}
fn fingerprints<F: SampleBase, E: ExtensionField<F>>() -> [String; 3] {
    let cfg =
        HyperFoldConfig::testing(F::KIND, BaseCodeKind::Rs, 8, 2, 4, 2, E::DIMENSION).unwrap();
    let pcs =
        HyperFoldPcs::<F, E>::setup_with_local_matrix(cfg, [142; 32], LocalMatrixKind::Pascal4)
            .unwrap();
    let msg = message::<F>(cfg.k());
    let z = point::<F, E>(cfg.mu);
    let y = evaluate(&msg, &z);
    let (cm, data) = pcs.commit(msg).unwrap();
    let (proof, _) = pcs.open(&cm, &data, &z, y).unwrap();
    pcs.verify(&cm, &z, y, &proof).unwrap();
    [
        blake3::hash(&cm.parameters_id).to_hex().to_string(),
        blake3::hash(&codec::encode(&cm).unwrap())
            .to_hex()
            .to_string(),
        blake3::hash(&codec::encode(&proof).unwrap())
            .to_hex()
            .to_string(),
    ]
}
#[test]
fn pascal4_golden_both_fields() {
    let actual = [
        fingerprints::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(),
        fingerprints::<Mersenne127, M127Quad>(),
    ];
    let expected = [
        [
            "4cee43af3f5be64bd9bad0d8847543d01d05e9da9f47c706383f6ee1e9f67681",
            "1fd60d3db88377754b3267f8cc81be3bf8f17108b590cf81f9d9c6e5295d99d8",
            "8f1a07d7cbab97d26fab9452de1496e31c758589512ca0678aa33084863517e0",
        ],
        [
            "2b5539496294dced1399b60d9430382392be52d3fe7af9ce5e4cebbdffc6c7b1",
            "1e7b393efdc83f337519b4c4228e8fa75df6f7ed97a29bf0421842d6aca1efc0",
            "4347f425c6cd9f8c9ba2a5f236902d1af7ec329a3d547b44710bbe3627edee03",
        ],
    ]
    .map(|r| r.map(str::to_string));
    assert_eq!(actual, expected);
}
