//! Uniform fresh-Setup benchmark lifecycle, separate from the PCS protocol.
//! All timings include actual conversion/release work; the top oracle is cleaned
//! only after verification and wire serialization. No memory-policy fallback.
use crate::{
    codec,
    params::HyperFoldConfig,
    pcs::{HyperFoldPcs, OpenTimings},
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::ExtensionField;
use std::time::{Duration, Instant};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
pub struct TrialRecord {
    pub setup: f64,
    pub input: f64,
    pub commit: f64,
    pub forward_release: f64,
    pub open: f64,
    pub verify: f64,
    pub serialization: f64,
    pub diagnostics: f64,
    pub cleanup: f64,
    pub wall: f64,
    pub timings: OpenTimings,
    pub forward_capacity: usize,
    pub inverse_capacity: usize,
    pub commitment_bytes: usize,
    pub eval_proof_bytes: usize,
    pub fingerprint: String,
    pub parameters_id: String,
    pub commitment_root: String,
    pub y0_hash: String,
    pub proof_hash: String,
    pub challenge_hash: String,
}
pub fn run_trial<F: SampleBase, E: ExtensionField<F>>(
    config: HyperFoldConfig,
) -> Result<TrialRecord> {
    let wall = Instant::now();
    let timer = Instant::now();
    let pcs = HyperFoldPcs::<F, E>::setup(config, [42; 32])?;
    let setup = ms(timer.elapsed());
    let timer = Instant::now();
    let mut rng = stream(&[27; 32], b"benchmark-message", config.mu as u64);
    let message = (0..config.k())
        .map(|_| F::sample(&mut rng))
        .collect::<Vec<_>>();
    let mut rng = stream(&[28; 32], b"benchmark-eval-point", config.mu as u64);
    let z = (0..config.mu)
        .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect::<Vec<_>>();
    let y = evaluate(&message, &z);
    let input = ms(timer.elapsed());
    let timer = Instant::now();
    let (cm, data) = pcs.commit(message)?;
    let commit = ms(timer.elapsed());
    let release_start = Instant::now();
    let (oracle, witness) = data.into_parts();
    let pcs = pcs.into_opening_only()?;
    let forward_release = ms(release_start.elapsed());
    let (forward_capacity, inverse_capacity) = pcs.diagonal_capacities();
    let timer = Instant::now();
    let (proof, mut prover, timings) = pcs.open_once_profiled(&cm, &oracle, witness, &z, y)?;
    let open = ms(timer.elapsed());
    assert_eq!(forward_capacity, 0);
    assert!(inverse_capacity > 0);
    assert_eq!(timings.released_witness_elements, config.k());
    assert_eq!(timings.witness_release_round, Some((config.depth(), 0)));
    let timer = Instant::now();
    let mut verifier = pcs.verify(&cm, &z, y, &proof)?;
    let verify = ms(timer.elapsed());
    let timer = Instant::now();
    let challenge = prover.sample::<F, E>(b"post-benchmark-agreement");
    if challenge != verifier.sample::<F, E>(b"post-benchmark-agreement") {
        return Err("transcript disagreement".into());
    }
    let agreement = ms(timer.elapsed());
    let timer = Instant::now();
    let cm_bytes = codec::encode(&cm)?;
    let proof_bytes = codec::encode(&proof)?;
    let serialization = ms(timer.elapsed());
    let timer = Instant::now();
    let mut hash = blake3::Hasher::new();
    hash.update(b"hyperfold-one-shot-lifetimes-v1");
    hash.update(&(cm_bytes.len() as u64).to_le_bytes());
    hash.update(&cm_bytes);
    hash.update(&proof_bytes);
    hash.update(&codec::encode(&challenge)?);
    let fingerprint = hash.finalize().to_hex().to_string();
    let parameters_id = cm
        .parameters_id
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let commitment_root = cm.root.iter().map(|b| format!("{b:02x}")).collect();
    let y0_hash = blake3::hash(&codec::encode(&cm.y0)?).to_hex().to_string();
    let proof_hash = blake3::hash(&proof_bytes).to_hex().to_string();
    let challenge_hash = blake3::hash(&codec::encode(&challenge)?)
        .to_hex()
        .to_string();
    let diagnostics = agreement + ms(timer.elapsed());
    let commitment_bytes = cm_bytes.len();
    let eval_proof_bytes = proof_bytes.len();
    // Caller-owned oracle remains alive through verification/serialization in both modes.
    std::hint::black_box(&oracle);
    let timer = Instant::now();
    drop(proof_bytes);
    drop(cm_bytes);
    drop(proof);
    drop(oracle);
    drop(pcs);
    drop(z);
    let cleanup = ms(timer.elapsed());
    Ok(TrialRecord {
        setup,
        input,
        commit,
        forward_release,
        open,
        verify,
        serialization,
        diagnostics,
        cleanup,
        wall: ms(wall.elapsed()),
        timings,
        forward_capacity,
        inverse_capacity,
        commitment_bytes,
        eval_proof_bytes,
        fingerprint,
        parameters_id,
        commitment_root,
        y0_hash,
        proof_hash,
        challenge_hash,
    })
}

pub const METRICS: [&str; 13] = [
    "setup_ms",
    "input_ms",
    "commit_ms",
    "forward_release_ms",
    "open_ms",
    "witness_release_ms",
    "first_tree_query_ms",
    "verify_ms",
    "serialization_ms",
    "diagnostics_ms",
    "cleanup_ms",
    "wall_ms",
    "online_total_ms",
];
impl TrialRecord {
    pub fn metrics(&self) -> [f64; 13] {
        [
            self.setup,
            self.input,
            self.commit,
            self.forward_release,
            self.open,
            ms(self.timings.witness_release),
            ms(self.timings.first_tree_query),
            self.verify,
            self.serialization,
            self.diagnostics,
            self.cleanup,
            self.wall,
            self.commit + self.forward_release + self.open + self.verify,
        ]
    }
}
