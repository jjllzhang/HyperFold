//! Focused encoding benchmark. Setup, input generation and witness copies are untimed.
use clap::Parser;
use hyperfold_pcs::{
    codec,
    encoder::encode_top,
    field_m127::{M127Quad, Mersenne127},
    matrix_h::MatrixKind,
    params::{BaseCodeKind, HyperFoldConfig},
    pcs::HyperFoldPcs,
    sampling::{SampleBase, stream},
};
use p3_field::{ExtensionField, PackedValue, extension::CubicTrinomialExtensionField};
use p3_goldilocks::Goldilocks;
use std::{hint::black_box, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Parser)]
struct Args {
    #[arg(long, value_delimiter = ',', default_value = "goldilocks,m127")]
    fields: Vec<String>,
    #[arg(long, default_value_t = 20)]
    mu: usize,
    #[arg(long, value_delimiter = ',', default_value = "2,4,8")]
    r: Vec<usize>,
    #[arg(long, value_delimiter = ',', default_value = "2,4")]
    c: Vec<usize>,
    #[arg(long, value_delimiter = ',', default_value = "1,32")]
    threads: Vec<usize>,
    #[arg(long, default_value_t = 5)]
    repetitions: usize,
    #[arg(long, default_value = "specialized", value_parser = ["specialized", "general"])]
    matrix: String,
}

fn run<F: SampleBase, E: ExtensionField<F>>(args: &Args, threads: usize) -> Result<()> {
    for &r in &args.r {
        for &c in &args.c {
            let config = HyperFoldConfig::experiment(F::KIND, BaseCodeKind::Rs, args.mu, r, c)?;
            let matrix = if args.matrix == "general" {
                MatrixKind::General
            } else {
                MatrixKind::Specialized
            };
            let pcs = HyperFoldPcs::<F, E>::setup_with_matrix(config, [42; 32], matrix)?;
            let mut rng = stream(&[27; 32], b"encoding-benchmark-message", args.mu as u64);
            let message: Vec<F> = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
            let mut base_output = vec![F::ZERO; config.k() * c];
            let mut base_samples = Vec::new();
            for repetition in 0..=args.repetitions {
                let start = Instant::now();
                pcs.base.encode_batch(&message, &mut base_output);
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                black_box(&base_output);
                if repetition > 0 {
                    base_samples.push(elapsed);
                }
            }

            let mut encode_samples = Vec::new();
            for repetition in 0..=args.repetitions {
                let start = Instant::now();
                let encoded = encode_top(
                    config,
                    &pcs.base,
                    &pcs.h,
                    &pcs.diagonals,
                    &message,
                    config.depth(),
                );
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                black_box(&encoded);
                drop(encoded);
                if repetition > 0 {
                    encode_samples.push(elapsed);
                }
            }

            let mut expected_commitment = None;
            for repetition in 0..=args.repetitions {
                // The public API consumes the witness; copy it before starting the timer.
                let witness = message.clone();
                let start = Instant::now();
                let (commitment, data) = pcs.commit(witness)?;
                let commit_ms = start.elapsed().as_secs_f64() * 1000.0;
                let commitment_bytes = codec::encode(&commitment)?;
                if let Some(expected) = &expected_commitment {
                    assert_eq!(expected, &commitment_bytes);
                } else {
                    expected_commitment = Some(commitment_bytes);
                }
                drop(data);
                if repetition > 0 {
                    let base_ms = base_samples[repetition - 1];
                    let encode_ms = encode_samples[repetition - 1];
                    let root: String = commitment.root.iter().map(|b| format!("{b:02x}")).collect();
                    println!(
                        "{:?},{},{},{r},{c},{threads},{},{},{repetition},{base_ms:.6},{encode_ms:.6},{commit_ms:.6},{root}",
                        F::KIND,
                        args.matrix,
                        config.mu,
                        config.k0(),
                        F::Packing::WIDTH,
                    );
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.repetitions == 0 || args.threads.contains(&0) {
        return Err("repetitions and thread counts must be positive".into());
    }
    for field in &args.fields {
        if !["goldilocks", "m127"].contains(&field.as_str()) {
            return Err(format!("unsupported field: {field}").into());
        }
    }
    println!(
        "field,matrix,mu,r,c,threads,k0,packing_width,repetition,base_ms,encode_ms,commit_ms,root"
    );
    for &threads in &args.threads {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()?;
        pool.install(|| -> Result<()> {
            for field in &args.fields {
                match field.as_str() {
                    "goldilocks" => {
                        run::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(&args, threads)?
                    }
                    "m127" => run::<Mersenne127, M127Quad>(&args, threads)?,
                    _ => unreachable!(),
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
