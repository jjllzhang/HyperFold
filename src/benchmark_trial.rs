//! One complete fresh-Setup trial shared by the CLI and Criterion.
use crate::{
    codec,
    params::HyperFoldConfig,
    pcs::HyperFoldPcs,
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
    pub cleanup: f64,
    pub wall: f64,
    pub commitment_bytes: usize,
    pub eval_proof_bytes: usize,
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
    let timer = Instant::now();
    let (oracle, witness) = data.into_parts();
    let pcs = pcs.into_opening_only()?;
    let forward_release = ms(timer.elapsed());
    let timer = Instant::now();
    let (proof, _) = pcs.open_once(&cm, &oracle, witness, &z, y)?;
    let open = ms(timer.elapsed());
    let timer = Instant::now();
    pcs.verify(&cm, &z, y, &proof)?;
    let verify = ms(timer.elapsed());
    let timer = Instant::now();
    let cm_bytes = codec::encode(&cm)?;
    let proof_bytes = codec::encode(&proof)?;
    let serialization = ms(timer.elapsed());
    let commitment_bytes = cm_bytes.len();
    let eval_proof_bytes = proof_bytes.len();
    // Retain the caller's top oracle through verification and serialization.
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
        cleanup,
        wall: ms(wall.elapsed()),
        commitment_bytes,
        eval_proof_bytes,
    })
}
pub const METRICS: [&str; 10] = [
    "setup_ms",
    "input_ms",
    "commit_ms",
    "forward_release_ms",
    "open_ms",
    "verify_ms",
    "serialization_ms",
    "cleanup_ms",
    "wall_ms",
    "online_total_ms",
];
impl TrialRecord {
    pub fn metrics(&self) -> [f64; 10] {
        [
            self.setup,
            self.input,
            self.commit,
            self.forward_release,
            self.open,
            self.verify,
            self.serialization,
            self.cleanup,
            self.wall,
            self.commit + self.forward_release + self.open + self.verify,
        ]
    }
}
