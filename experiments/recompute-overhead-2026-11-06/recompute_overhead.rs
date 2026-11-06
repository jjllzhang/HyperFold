//! Paired profiling only: keep identical two-direction Setup tables and change
//! only OpenOptions::recompute_first_tree. One common commitment, then warmup +
//! repeated openings; this does not replace the official commit/open sweep.
use clap::{Parser, ValueEnum};
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad as E, Mersenne127 as F},
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::{HyperFoldPcs, OpenOptions},
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::BasedVectorSpace;
use std::{fs::File, io::Write, path::PathBuf, time::Instant};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Strategy {
    Retained,
    Rebuild,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Backend {
    Qa,
    Rs,
}
#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    mu: usize,
    #[arg(long, value_enum)]
    backend: Backend,
    #[arg(long, value_enum)]
    strategy: Strategy,
    #[arg(long, default_value_t = 32)]
    threads: usize,
    #[arg(long, default_value_t = 3)]
    repetitions: usize,
    #[arg(long)]
    output: PathBuf,
}
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn profile(args: &Args) -> Result<()> {
    let backend = match args.backend {
        Backend::Qa => BaseCodeKind::Qa,
        Backend::Rs => BaseCodeKind::Rs,
    };
    let config = HyperFoldConfig::experiment(BaseFieldKind::M127, backend, args.mu, 2, 4)?;
    let pcs = HyperFoldPcs::<F, E>::setup(config, [42; 32])?;
    assert!(pcs.diagonals.has_forward() && pcs.diagonals.has_inverse());
    // Both modes retain BOTH diagonal directions. No prepare_* conversions occur.
    let mut rng = stream(&[27; 32], b"benchmark-message", config.mu as u64);
    let message = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
    let (cm, data) = pcs.commit(message)?;
    let cm_bytes = codec::encode(&cm)?;
    let mut rng = stream(&[28; 32], b"benchmark-eval-point", config.mu as u64);
    let z: Vec<E> = (0..config.mu)
        .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect();
    let y = evaluate(&data.message, &z);
    if let Some(parent) = args.output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut output = File::create(&args.output)?;
    writeln!(
        output,
        "field,backend,mu,r,c,extension_degree,queries,threads,strategy,repetition,open_time_ms,verify_time_ms,proof_size_bytes,fingerprint"
    )?;
    let mut reference = None;
    for repetition in 0..=args.repetitions {
        let start = Instant::now();
        let (proof, mut prover) = pcs.open_with_options(
            &cm,
            &data,
            &z,
            y,
            OpenOptions {
                recompute_first_tree: matches!(args.strategy, Strategy::Rebuild),
            },
        )?;
        let open_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let mut verifier = pcs.verify(&cm, &z, y, &proof)?;
        let verify_ms = start.elapsed().as_secs_f64() * 1000.0;
        let challenge = prover.sample::<F, E>(b"post-benchmark-agreement");
        if challenge != verifier.sample::<F, E>(b"post-benchmark-agreement") {
            return Err("transcript disagreement".into());
        }
        let proof_bytes = codec::encode(&proof)?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"hyperfold-recompute-overhead-v1");
        hash.update(&(cm_bytes.len() as u64).to_le_bytes());
        hash.update(&cm_bytes);
        hash.update(&proof_bytes);
        hash.update(&codec::encode(&challenge)?);
        let fingerprint = hash.finalize();
        if let Some(expected) = reference {
            if expected != fingerprint {
                return Err("non-deterministic commitment/proof/transcript".into());
            }
        } else {
            reference = Some(fingerprint);
        }
        if repetition > 0 {
            writeln!(
                output,
                "M127,{backend:?},{},2,4,{},{},{},{:?},{},{open_ms:.6},{verify_ms:.6},{},{}",
                config.mu,
                config.extension_degree,
                config.num_queries,
                args.threads,
                args.strategy,
                repetition,
                cm_bytes.len() + proof_bytes.len(),
                fingerprint.to_hex()
            )?;
            output.flush()?;
        }
    }
    // Keep every Setup buffer live through all measured openings.
    std::hint::black_box(&pcs);
    Ok(())
}
fn main() -> Result<()> {
    let args = Args::parse();
    if args.threads == 0 || args.repetitions == 0 {
        return Err("threads/repetitions must be positive".into());
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build()?
        .install(|| profile(&args))
}
