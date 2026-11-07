//! Untimed certified PCS byte snapshots for baseline/candidate differential checks.
//! Exactly 32 Rayon workers. Output directory must be new. No benchmark CSV writes.
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::HyperFoldPcs,
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::{ExtensionField, extension::CubicTrinomialExtensionField};
use p3_goldilocks::Goldilocks;
use std::path::Path;
fn snapshot<F: SampleBase, E: ExtensionField<F>>(config: HyperFoldConfig, output: &Path) {
    let pcs = HyperFoldPcs::<F, E>::setup(config, [42; 32]).unwrap();
    let mut rng = stream(&[27; 32], b"benchmark-message", config.mu as u64);
    let message = (0..config.k())
        .map(|_| F::sample(&mut rng))
        .collect::<Vec<_>>();
    let mut rng = stream(&[28; 32], b"benchmark-eval-point", config.mu as u64);
    let z = (0..config.mu)
        .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect::<Vec<_>>();
    let y = evaluate(&message, &z);
    let (cm, data) = pcs.commit(message).unwrap();
    let (oracle, witness) = data.into_parts();
    let pcs = pcs.into_opening_only().unwrap();
    let (proof, mut prover) = pcs.open_once(&cm, &oracle, witness, &z, y).unwrap();
    let mut verifier = pcs.verify(&cm, &z, y, &proof).unwrap();
    let challenge = prover.sample::<F, E>(b"kernel-byte-agreement");
    assert_eq!(challenge, verifier.sample::<F, E>(b"kernel-byte-agreement"));
    println!("file,bytes,blake3");
    for (name, bytes) in [
        ("commitment.bin", codec::encode(&cm).unwrap()),
        ("proof.bin", codec::encode(&proof).unwrap()),
        ("challenge.bin", codec::encode(&challenge).unwrap()),
        ("parameters.bin", config.id().to_vec()),
    ] {
        std::fs::write(output.join(name), &bytes).unwrap();
        println!("{name},{},{}", bytes.len(), blake3::hash(&bytes).to_hex());
    }
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        5,
        "usage: pcs_bytes FIELD CODE MU NEW_OUTPUT_DIR"
    );
    let field = match args[1].as_str() {
        "m127" => BaseFieldKind::M127,
        "goldilocks" => BaseFieldKind::Goldilocks,
        _ => panic!("field"),
    };
    let code = match args[2].as_str() {
        "qa" => BaseCodeKind::Qa,
        "rs" => BaseCodeKind::Rs,
        _ => panic!("code"),
    };
    let config = HyperFoldConfig::experiment(field, code, args[3].parse().unwrap(), 8, 4).unwrap();
    let output = Path::new(&args[4]);
    std::fs::create_dir(output).unwrap();
    rayon::ThreadPoolBuilder::new()
        .num_threads(32)
        .build()
        .unwrap()
        .install(|| match field {
            BaseFieldKind::M127 => snapshot::<Mersenne127, M127Quad>(config, output),
            BaseFieldKind::Goldilocks => {
                snapshot::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(config, output)
            }
        });
}
