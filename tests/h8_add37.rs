//! Optional Add37 correctness; historical Pascal fixtures stay unchanged.
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

fn point<F: SampleBase, E: ExtensionField<F>>(mu: usize) -> Vec<E> {
    let mut rng = stream(&[139; 32], b"add37-point", mu as u64);
    (0..mu)
        .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect()
}
fn message<F: SampleBase>(k: usize) -> Vec<F> {
    let mut rng = stream(&[138; 32], b"add37-message", k as u64);
    (0..k).map(|_| F::sample(&mut rng)).collect()
}
fn parameter_matrix<F: SampleBase, E: ExtensionField<F>>() {
    for backend in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
        for c in [2, 4] {
            for depth in [2, 3, 4] {
                let cfg = HyperFoldConfig::testing(
                    F::KIND,
                    backend,
                    2 + 3 * depth,
                    2,
                    8,
                    c,
                    E::DIMENSION,
                )
                .unwrap();
                let pcs = HyperFoldPcs::<F, E>::setup_with_local_matrix(
                    cfg,
                    [137; 32],
                    LocalMatrixKind::Add37,
                )
                .unwrap();
                let msg = message::<F>(cfg.k());
                let z = point::<F, E>(cfg.mu);
                let y = evaluate(&msg, &z);
                let (cm, data) = pcs.commit(msg.clone()).unwrap();
                let (proof, mut prover) = pcs.open(&cm, &data, &z, y).unwrap();
                let mut verifier = pcs.verify(&cm, &z, y, &proof).unwrap();
                assert_eq!(
                    prover.sample::<F, E>(b"agreement"),
                    verifier.sample::<F, E>(b"agreement")
                );
                let mut bad = proof.clone();
                bad.top.rows[0][0] += F::ONE;
                assert!(pcs.verify(&cm, &z, y, &bad).is_err());
                let mut bad = proof.clone();
                bad.inner[0].rows[0][0] += E::ONE;
                assert!(pcs.verify(&cm, &z, y, &bad).is_err());
                let mut second = z.clone();
                second[0] += E::ONE;
                let (p2, mut t2) = pcs
                    .open(&cm, &data, &second, evaluate(&msg, &second))
                    .unwrap();
                let mut v2 = pcs
                    .verify(&cm, &second, evaluate(&msg, &second), &p2)
                    .unwrap();
                assert_eq!(
                    t2.sample::<F, E>(b"agreement"),
                    v2.sample::<F, E>(b"agreement")
                );
                let encoded = codec::encode(&proof).unwrap();
                let (oracle, witness) = data.into_parts();
                let opening = pcs.into_opening_only().unwrap();
                let (owned, mut t) = opening.open_once(&cm, &oracle, witness, &z, y).unwrap();
                assert_eq!(encoded, codec::encode(&owned).unwrap());
                let mut v = opening.verify(&cm, &z, y, &owned).unwrap();
                assert_eq!(
                    t.sample::<F, E>(b"agreement"),
                    v.sample::<F, E>(b"agreement")
                );
                if depth == 2 {
                    let legacy = HyperFoldPcs::<F, E>::setup(cfg, [137; 32]).unwrap();
                    assert!(legacy.verify(&cm, &z, y, &proof).is_err());
                    let (old_cm, old_data) = legacy.commit(msg).unwrap();
                    assert_ne!(old_cm.parameters_id, cm.parameters_id);
                    let (old_proof, _) = legacy.open(&old_cm, &old_data, &z, y).unwrap();
                    assert!(opening.verify(&old_cm, &z, y, &old_proof).is_err());
                    let explicit = HyperFoldPcs::<F, E>::setup_with_local_matrix(
                        cfg,
                        [137; 32],
                        LocalMatrixKind::Pascal8,
                    )
                    .unwrap();
                    let (explicit_cm, explicit_data) =
                        explicit.commit(old_data.message.clone()).unwrap();
                    assert_eq!(
                        codec::encode(&old_cm).unwrap(),
                        codec::encode(&explicit_cm).unwrap()
                    );
                    let (explicit_proof, _) =
                        explicit.open(&explicit_cm, &explicit_data, &z, y).unwrap();
                    assert_eq!(
                        codec::encode(&old_proof).unwrap(),
                        codec::encode(&explicit_proof).unwrap()
                    );
                }
            }
        }
    }
}
#[test]
fn add37_pcs_matrix_transcript_tamper_owned_and_identity() {
    parameter_matrix::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
    parameter_matrix::<Mersenne127, M127Quad>();
}
fn parallel<F: SampleBase, E: ExtensionField<F>>() {
    let cfg =
        HyperFoldConfig::testing(F::KIND, BaseCodeKind::Qa, 11, 2, 8, 4, E::DIMENSION).unwrap();
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let pcs = HyperFoldPcs::<F, E>::setup_with_local_matrix(
                    cfg,
                    [141; 32],
                    LocalMatrixKind::Add37,
                )
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
#[test]
fn add37_parallel_equals_single_thread_both_fields() {
    parallel::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
    parallel::<Mersenne127, M127Quad>();
}
#[test]
fn add37_rejects_other_arities() {
    for r in [2, 4] {
        let cfg =
            HyperFoldConfig::testing(Goldilocks::KIND, BaseCodeKind::Qa, 8, 2, r, 2, 3).unwrap();
        assert!(HyperFoldPcs::<Goldilocks,CubicTrinomialExtensionField<Goldilocks>>::setup_with_local_matrix(cfg,[0;32],LocalMatrixKind::Add37).is_err());
    }
}
fn fingerprints<F: SampleBase, E: ExtensionField<F>>() -> [String; 3] {
    let cfg =
        HyperFoldConfig::testing(F::KIND, BaseCodeKind::Rs, 8, 2, 8, 2, E::DIMENSION).unwrap();
    let pcs = HyperFoldPcs::<F, E>::setup_with_local_matrix(cfg, [142; 32], LocalMatrixKind::Add37)
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
fn add37_golden_both_fields() {
    let actual = [
        fingerprints::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(),
        fingerprints::<Mersenne127, M127Quad>(),
    ];
    let expected = [
        [
            "419eb558d1af37924b72b81d064f51f79b29caeb67853ed56d52bf8fba062bb7",
            "d817192ce14ba5c1ff009e9b5d5cb3c9dc8db2a9c5c5f700343532ee3af5d705",
            "60b9b368059033031a454f906f13882dbfa675f4d8a474678424de4d6a30ad79",
        ],
        [
            "59affdcf8cacd42fb4661bcc9d36b5325d91fe44f3325c171ea40186b2e5f225",
            "8d9a78378a78371aebd9d25027841d317792f43f0e19cad22ffed4bdd6398b4e",
            "687c53088292744b4c3dbe056083516822a238389907ac362db2e68739155e96",
        ],
    ]
    .map(|r| r.map(str::to_string));
    assert_eq!(actual, expected);
}
