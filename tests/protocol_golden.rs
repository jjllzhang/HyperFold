//! Fixed old-native proof/transcript references; the exercised path is now owned.
//! Source 7bd0724311aed8b98aaaf889d40b16d83e13f0ba; seeds 17/18/19;
//! native-retained borrowed opening before memory-policy removal. The fingerprint
//! domain and post-challenge label below reproduce that fixed reference; do not
//! regenerate this CSV to accept changes to the protocol.
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::HyperFoldPcs,
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::ExtensionField;
use p3_field::extension::CubicTrinomialExtensionField;
use p3_goldilocks::Goldilocks;
fn fingerprint<F: SampleBase, E: ExtensionField<F>>(config: HyperFoldConfig) -> String {
    let pcs = HyperFoldPcs::<F, E>::setup(config, [17; 32]).unwrap();
    let mut rng = stream(&[18; 32], b"golden-message", config.mu as u64);
    let message = (0..config.k())
        .map(|_| F::sample(&mut rng))
        .collect::<Vec<_>>();
    let mut rng = stream(&[19; 32], b"golden-point", config.mu as u64);
    let z = (0..config.mu)
        .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect::<Vec<_>>();
    let y = evaluate(&message, &z);
    let (cm, data) = pcs.commit(message).unwrap();
    let (oracle, witness) = data.into_parts();
    let pcs = pcs.into_opening_only().unwrap();
    let (proof, mut prover, t) = pcs
        .open_once_profiled(&cm, &oracle, witness, &z, y)
        .unwrap();
    assert_eq!(pcs.diagonal_capacities().0, 0);
    assert_eq!(t.released_witness_elements, config.k());
    assert_eq!(t.witness_release_round, Some((config.depth(), 0)));
    let mut verifier = pcs.verify(&cm, &z, y, &proof).unwrap();
    let challenge = prover.sample::<F, E>(b"golden-post-agreement");
    assert_eq!(challenge, verifier.sample::<F, E>(b"golden-post-agreement"));
    let mut h = blake3::Hasher::new();
    h.update(b"hyperfold-native-golden-v1");
    for bytes in [
        codec::encode(&cm).unwrap(),
        codec::encode(&proof).unwrap(),
        codec::encode(&challenge).unwrap(),
    ] {
        h.update(&(bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    h.finalize().to_hex().to_string()
}
#[test]
fn one_shot_matches_frozen_native_bytes_and_threads() {
    let mut count = 0;
    for threads in [1, 32] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            for line in include_str!("fixtures/native-retained-golden.csv")
                .lines()
                .filter(|l| !l.starts_with('#'))
                .skip(1)
            {
                let row = line.split(',').collect::<Vec<_>>();
                assert_eq!(row.len(), 8);
                let field = match row[0] {
                    "Goldilocks" => BaseFieldKind::Goldilocks,
                    "M127" => BaseFieldKind::M127,
                    _ => panic!("unknown field"),
                };
                let code = match row[1] {
                    "Qa" => BaseCodeKind::Qa,
                    "Rs" => BaseCodeKind::Rs,
                    _ => panic!("unknown code"),
                };
                let c = HyperFoldConfig::testing(
                    field,
                    code,
                    row[2].parse().unwrap(),
                    row[3].parse().unwrap(),
                    row[4].parse().unwrap(),
                    row[5].parse().unwrap(),
                    row[6].parse().unwrap(),
                )
                .unwrap();
                let actual = match field {
                    BaseFieldKind::Goldilocks => {
                        fingerprint::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(c)
                    }
                    BaseFieldKind::M127 => fingerprint::<Mersenne127, M127Quad>(c),
                };
                assert_eq!(actual, row[7], "{c:?},threads={threads}");
                count += 1;
            }
        });
    }
    assert_eq!(count, 192);
}
