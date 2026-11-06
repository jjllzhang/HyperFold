//! Same fresh-Setup outer lifecycle for control and opening-only ownership.
use clap::{Parser, ValueEnum};
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad as E, Mersenne127 as F},
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::{
        CommitData, Commitment, CommittedOracle, FirstTreeStrategy, HyperFoldPcs, OpenTimings,
        OpeningOnlyPcs, OwnedWitness, Proof,
    },
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
    transcript::Transcript,
};
use p3_field::BasedVectorSpace;
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Retained,
    Release,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Backend {
    Qa,
    Rs,
}
#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = 26)]
    mu: usize,
    #[arg(long, value_enum)]
    backend: Backend,
    #[arg(long, value_enum)]
    mode: Mode,
    #[arg(long, default_value_t = 32)]
    threads: usize,
    #[arg(long, default_value_t = 3)]
    repetitions: usize,
    #[arg(long, default_value_t = 1)]
    trial_round: usize,
    #[arg(long)]
    output: PathBuf,
}
enum TrialState {
    Retained {
        pcs: HyperFoldPcs<F, E>,
        data: CommitData<F>,
    },
    Release {
        pcs: OpeningOnlyPcs<F, E>,
        oracle: CommittedOracle<F>,
        witness: Option<OwnedWitness<F>>,
    },
}
impl TrialState {
    fn open(
        &mut self,
        cm: &Commitment<E>,
        z: &[E],
        y: E,
    ) -> Result<(Proof<F, E>, Transcript, OpenTimings)> {
        Ok(match self {
            Self::Retained { pcs, data } => {
                pcs.open_profiled(cm, data, z, y, FirstTreeStrategy::PrehashedRecovered)?
            }
            Self::Release {
                pcs,
                oracle,
                witness,
            } => pcs.open_once_profiled(
                cm,
                oracle,
                witness.take().ok_or("witness already consumed")?,
                z,
                y,
            )?,
        })
    }
    fn verify(&self, cm: &Commitment<E>, z: &[E], y: E, proof: &Proof<F, E>) -> Result<Transcript> {
        Ok(match self {
            Self::Retained { pcs, .. } => pcs.verify(cm, z, y, proof)?,
            Self::Release { pcs, .. } => pcs.verify(cm, z, y, proof)?,
        })
    }
    fn capacities(&self) -> (usize, usize) {
        match self {
            Self::Retained { pcs, .. } => (
                pcs.diagonals
                    .levels
                    .iter()
                    .map(|l| l.forward.capacity())
                    .sum(),
                pcs.diagonals
                    .levels
                    .iter()
                    .map(|l| l.inverse.capacity())
                    .sum(),
            ),
            Self::Release { pcs, .. } => pcs.diagonal_capacities(),
        }
    }
}
fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
struct Record {
    setup: f64,
    input: f64,
    commit: f64,
    forward_release: f64,
    open: f64,
    verify: f64,
    serialization: f64,
    diagnostics: f64,
    cleanup: f64,
    wall: f64,
    timings: OpenTimings,
    forward_capacity: usize,
    inverse_capacity: usize,
    commitment_bytes: usize,
    eval_proof_bytes: usize,
    fingerprint: String,
}
fn trial(args: &Args, config: HyperFoldConfig) -> Result<Record> {
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
    let mut forward_release = 0.0;
    let mut state = match args.mode {
        Mode::Retained => TrialState::Retained { pcs, data },
        Mode::Release => {
            let timer = Instant::now();
            let (oracle, witness) = data.into_parts();
            let pcs = pcs.into_opening_only()?;
            forward_release = ms(timer.elapsed());
            TrialState::Release {
                pcs,
                oracle,
                witness: Some(witness),
            }
        }
    };
    let (forward_capacity, inverse_capacity) = state.capacities();
    let timer = Instant::now();
    let (proof, mut prover, timings) = state.open(&cm, &z, y)?;
    let open = ms(timer.elapsed());
    match args.mode {
        Mode::Retained => {
            assert!(forward_capacity > 0);
            assert_eq!(timings.released_witness_elements, 0);
        }
        Mode::Release => {
            assert_eq!(forward_capacity, 0);
            assert_eq!(timings.released_witness_elements, config.k());
            assert_eq!(timings.witness_release_round, Some((config.depth(), 0)));
        }
    }
    assert!(inverse_capacity > 0);
    let timer = Instant::now();
    let mut verifier = state.verify(&cm, &z, y, &proof)?;
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
    let diagnostics = agreement + ms(timer.elapsed());
    let commitment_bytes = cm_bytes.len();
    let eval_proof_bytes = proof_bytes.len();
    // Caller-owned oracle remains alive through verification/serialization in both modes.
    std::hint::black_box(&state);
    let timer = Instant::now();
    drop(proof_bytes);
    drop(cm_bytes);
    drop(proof);
    drop(state);
    let cleanup = ms(timer.elapsed());
    Ok(Record {
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
    })
}
const HEADER: &str = "field,backend,mu,r,c,mu0,depth,extension_degree,queries,threads,mode,trial_round,repetition,first_tree_strategy,parameter_lifecycle,witness_lifecycle,setup_per_trial,setup_ms,input_ms,commit_ms,forward_release_ms,open_ms,witness_release_ms,first_tree_query_ms,verify_ms,serialization_ms,diagnostics_ms,cleanup_ms,wall_ms,online_total_ms,forward_capacity_elements,inverse_capacity_elements,released_witness_elements,released_witness_capacity,witness_release_level,witness_release_subround,commitment_bytes,eval_proof_bytes,proof_size_bytes,fingerprint";
fn profile(args: &Args) -> Result<()> {
    let backend = match args.backend {
        Backend::Qa => BaseCodeKind::Qa,
        Backend::Rs => BaseCodeKind::Rs,
    };
    let config = HyperFoldConfig::experiment(BaseFieldKind::M127, backend, args.mu, 2, 4)?;
    if let Some(parent) = args.output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.output)?;
    writeln!(output, "{HEADER}")?;
    output.flush()?;
    let mut reference = None;
    for repetition in 0..=args.repetitions {
        let r = trial(args, config)?;
        if let Some(ref expected) = reference {
            if *expected != r.fingerprint {
                return Err("non-deterministic proof/transcript".into());
            }
        } else {
            reference = Some(r.fingerprint.clone());
        }
        if repetition == 0 {
            continue;
        }
        let (parameters, witness) = match args.mode {
            Mode::Retained => ("reusable_both", "borrowed_retained"),
            Mode::Release => ("opening_only", "consumed_after_first_round"),
        };
        let row = vec![
            "M127".to_string(),
            format!("{backend:?}"),
            args.mu.to_string(),
            "2".into(),
            "4".into(),
            config.mu0.to_string(),
            config.depth().to_string(),
            config.extension_degree.to_string(),
            config.num_queries.to_string(),
            args.threads.to_string(),
            format!("{:?}", args.mode),
            args.trial_round.to_string(),
            repetition.to_string(),
            "prehashed_recovered".into(),
            parameters.into(),
            witness.into(),
            "true".into(),
            format!("{:.6}", r.setup),
            format!("{:.6}", r.input),
            format!("{:.6}", r.commit),
            format!("{:.6}", r.forward_release),
            format!("{:.6}", r.open),
            format!("{:.6}", ms(r.timings.witness_release)),
            format!("{:.6}", ms(r.timings.first_tree_query)),
            format!("{:.6}", r.verify),
            format!("{:.6}", r.serialization),
            format!("{:.6}", r.diagnostics),
            format!("{:.6}", r.cleanup),
            format!("{:.6}", r.wall),
            format!("{:.6}", r.commit + r.forward_release + r.open + r.verify),
            r.forward_capacity.to_string(),
            r.inverse_capacity.to_string(),
            r.timings.released_witness_elements.to_string(),
            r.timings.released_witness_capacity.to_string(),
            r.timings
                .witness_release_round
                .map(|p| p.0.to_string())
                .unwrap_or_default(),
            r.timings
                .witness_release_round
                .map(|p| p.1.to_string())
                .unwrap_or_default(),
            r.commitment_bytes.to_string(),
            r.eval_proof_bytes.to_string(),
            (r.commitment_bytes + r.eval_proof_bytes).to_string(),
            r.fingerprint,
        ];
        assert_eq!(row.len(), HEADER.split(',').count());
        writeln!(output, "{}", row.join(","))?;
        output.flush()?;
    }
    Ok(())
}
fn main() -> Result<()> {
    let args = Args::parse();
    if args.threads == 0 || args.repetitions == 0 {
        return Err("positive threads/repetitions required".into());
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build()?
        .install(|| profile(&args))
}
