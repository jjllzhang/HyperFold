//! Run against frozen source 7bd0724: native retained-tree protocol reference.
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    params::{BaseCodeKind, HyperFoldConfig},
    pcs::HyperFoldPcs,
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::extension::CubicTrinomialExtensionField;
use p3_field::{BasedVectorSpace, ExtensionField};
use p3_goldilocks::Goldilocks;
fn generate<F: SampleBase, E: ExtensionField<F>>() {
    for backend in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
        for r in [2usize, 4, 8] {
            for c in [2, 4] {
                for depth in 1..=4 {
                    let config = HyperFoldConfig::testing(
                        F::KIND,
                        backend,
                        2 + depth * r.ilog2() as usize,
                        2,
                        r,
                        c,
                        <E as BasedVectorSpace<F>>::DIMENSION,
                    )
                    .unwrap();
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
                    let (proof, mut prover) = pcs.open(&cm, &data, &z, y).unwrap();
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
                    println!(
                        "{:?},{:?},{},{},{},{},{},{}",
                        F::KIND,
                        backend,
                        config.mu,
                        config.mu0,
                        r,
                        c,
                        config.extension_degree,
                        h.finalize().to_hex()
                    );
                }
            }
        }
    }
}
fn main() {
    println!(
        "# NativeRetained references from 7bd0724311aed8b98aaaf889d40b16d83e13f0ba; protocol unchanged from c587b84"
    );
    println!("field,backend,mu,mu0,r,c,extension_degree,fingerprint");
    rayon::ThreadPoolBuilder::new()
        .num_threads(32)
        .build()
        .unwrap()
        .install(|| {
            generate::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>();
            generate::<Mersenne127, M127Quad>();
        });
}
