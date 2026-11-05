//! Reproducible sequential experiment sweep. All prover/verifier parallel work
//! uses the requested local Rayon pool; no background services or remote effects.
use clap::Parser;
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    params::{self, BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::{HyperFoldPcs, OpenOptions},
    resources::estimated_gib,
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::extension::CubicTrinomialExtensionField;
use p3_field::{AlgebraIdentity, BasedVectorSpace, ExtensionField};
use p3_goldilocks::Goldilocks;
use std::{
    fs::{File, OpenOptions as FileOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};
#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = 32)]
    threads: usize,
    /// Comma-separated even sizes or inclusive even sweep, e.g. 20..=30.
    #[arg(long, default_value = "20..=30")]
    mu: String,
    #[arg(long, value_delimiter = ',', default_value = "2,4,8")]
    r: Vec<usize>,
    #[arg(long, value_delimiter = ',', default_value = "2,4")]
    c: Vec<usize>,
    #[arg(long, value_delimiter = ',', default_value = "qa,rs")]
    backends: Vec<String>,
    #[arg(long, value_delimiter = ',', default_value = "goldilocks,m127")]
    fields: Vec<String>,
    #[arg(long, default_value_t = 3)]
    repetitions: usize,
    #[arg(long)]
    quick: bool,
    /// Print all selected cases and conservative memory estimates; allocate no setup.
    #[arg(long)]
    list: bool,
    /// Memory safety ceiling (GiB); cases above this are recorded as skipped_memory.
    #[arg(long, default_value_t = 64.0)]
    max_estimated_gib: f64,
    #[arg(
        long,
        default_value = "/home/zjl/zkp-research/experiments/hyperfold-bench/results.csv"
    )]
    output: PathBuf,
    #[arg(long)]
    resume: bool,
    /// auto selects low-memory only when retained storage exceeds the ceiling.
    #[arg(long, default_value="auto", value_parser=["auto", "retained", "low"])]
    memory_mode: String,
}
fn mus(input: &str) -> Result<Vec<usize>, String> {
    if let Some((a, b)) = input.split_once("..=") {
        let a = a.parse::<usize>().map_err(|e| e.to_string())?;
        let b = b.parse::<usize>().map_err(|e| e.to_string())?;
        if a > b || b > 30 {
            return Err("invalid mu range".into());
        }
        Ok((a..=b).filter(|x| x % 2 == 0).collect())
    } else {
        input
            .split(',')
            .map(|s| {
                s.parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())
            })
            .collect()
    }
}
fn low_memory(c: HyperFoldConfig, args: &Args) -> bool {
    args.memory_mode == "low"
        || (args.memory_mode == "auto" && estimated_gib(c, false) > args.max_estimated_gib)
}
fn name(c: HyperFoldConfig) -> String {
    format!(
        "{:?},{:?},{},{},{}",
        c.base_field, c.backend, c.mu, c.r, c.c
    )
}
fn output(path: &PathBuf, resume: bool, header: &str) -> std::io::Result<BufWriter<File>> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let existing = resume && path.exists() && path.metadata()?.len() > 0;
    let file = FileOptions::new()
        .create(true)
        .write(true)
        .append(resume)
        .truncate(!resume)
        .open(path)?;
    let mut w = BufWriter::new(file);
    if !existing {
        writeln!(w, "{header}")?;
    }
    Ok(w)
}
type Measurements = (f64, f64, f64, usize, usize, f64);
type BenchError = Box<dyn std::error::Error + Send + Sync>;
fn run<F: SampleBase, E: ExtensionField<F> + AlgebraIdentity<F>>(
    config: HyperFoldConfig,
    args: &Args,
    raw: &mut impl Write,
) -> Result<Measurements, BenchError> {
    let setup_start = Instant::now();
    let low = low_memory(config, args);
    let mut pcs = if low {
        HyperFoldPcs::<F, E>::setup_low_memory(config, [42; 32])?
    } else {
        HyperFoldPcs::<F, E>::setup(config, [42; 32])?
    };
    let setup_ms = setup_start.elapsed().as_secs_f64() * 1000.0;
    let mut sum = [0.0; 3];
    let mut transition_sum = 0.0;
    let mut bytes = (0, 0);
    for repetition in 0..=args.repetitions {
        let mut rng = stream(&[27; 32], b"benchmark-message", config.mu as u64);
        let message: Vec<F> = (0..config.k()).map(|_| F::sample(&mut rng)).collect();
        let mut rng = stream(&[28; 32], b"benchmark-eval-point", config.mu as u64);
        let z: Vec<E> = (0..config.mu)
            .map(|_| {
                <E as BasedVectorSpace<F>>::from_basis_coefficients_fn(|_| F::sample(&mut rng))
            })
            .collect();
        let y = evaluate(&message, &z);
        let start = Instant::now();
        pcs.prepare_commit();
        let commit_transition = start.elapsed();
        let start = Instant::now();
        let (cm, data) = pcs.commit(message)?;
        let commit = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        pcs.prepare_open();
        let transition_ms = (commit_transition + start.elapsed()).as_secs_f64() * 1000.0;
        let start = Instant::now();
        let (proof, mut prover) = pcs.open_with_options(
            &cm,
            &data,
            &z,
            y,
            OpenOptions {
                recompute_first_tree: low,
            },
        )?;
        let open = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let mut verifier = pcs.verify(&cm, &z, y, &proof)?;
        let verify = start.elapsed().as_secs_f64() * 1000.0;
        if prover.sample::<F, E>(b"post-benchmark-agreement")
            != verifier.sample::<F, E>(b"post-benchmark-agreement")
        {
            return Err("transcript disagreement".into());
        }
        let sizes = (codec::encode(&cm)?.len(), codec::encode(&proof)?.len());
        if repetition > 0 {
            if repetition > 1 && sizes != bytes {
                return Err("non-deterministic proof size".into());
            }
            bytes = sizes;
            transition_sum += transition_ms;
            for (acc, v) in sum.iter_mut().zip([commit, open, verify]) {
                *acc += v;
            }
            writeln!(
                raw,
                "{},{},{},{},{},{},{:.9},{commit:.6},{open:.6},{verify:.6},{},{},{setup_ms:.6},{low},{transition_ms:.6}",
                name(config),
                config.mu0,
                config.extension_degree,
                config.num_queries,
                repetition,
                config.certified,
                estimated_gib(config, low),
                sizes.0,
                sizes.1
            )?;
            raw.flush()?;
        }
    }
    Ok((
        sum[0] / args.repetitions as f64,
        sum[1] / args.repetitions as f64,
        sum[2] / args.repetitions as f64,
        bytes.0,
        bytes.1,
        transition_sum / args.repetitions as f64,
    ))
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    if args.threads == 0
        || args.repetitions == 0
        || !args.max_estimated_gib.is_finite()
        || args.max_estimated_gib <= 0.0
    {
        return Err("threads/repetitions/memory ceiling must be positive".into());
    }
    let ms = if args.quick { vec![10] } else { mus(&args.mu)? };
    let mut cases = Vec::new();
    for f in &args.fields {
        let field = match f.as_str() {
            "goldilocks" => BaseFieldKind::Goldilocks,
            "m127" => BaseFieldKind::M127,
            _ => return Err("unknown field".into()),
        };
        for &mu in &ms {
            for &r in &args.r {
                for &c in &args.c {
                    for b in &args.backends {
                        let backend = match b.as_str() {
                            "qa" => BaseCodeKind::Qa,
                            "rs" => BaseCodeKind::Rs,
                            _ => return Err("unknown backend".into()),
                        };
                        let config = if args.quick {
                            if ![2usize, 4, 8].contains(&r) {
                                return Err("invalid arity".into());
                            }
                            let mu0 = 2 + (mu - 2) % (r.ilog2() as usize);
                            HyperFoldConfig::testing(
                                field,
                                backend,
                                mu,
                                mu0,
                                r,
                                c,
                                if field == BaseFieldKind::Goldilocks {
                                    3
                                } else {
                                    2
                                },
                            )?
                        } else {
                            HyperFoldConfig::experiment(field, backend, mu, r, c)?
                        };
                        cases.push(config);
                    }
                }
            }
        }
    }
    if args.list {
        println!(
            "field,backend,mu,r,c,mu0,extension_degree,queries,estimated_gib,certified_iop,low_memory"
        );
        for c in cases {
            println!(
                "{},{},{},{},{:.3},{},{}",
                name(c),
                c.mu0,
                c.extension_degree,
                c.num_queries,
                estimated_gib(c, low_memory(c, &args)),
                c.certified,
                low_memory(c, &args)
            );
        }
        return Ok(());
    }
    let completed = if args.resume && args.output.exists() {
        std::fs::read_to_string(&args.output)?
            .lines()
            .skip(1)
            .filter_map(|l| {
                let p: Vec<_> = l.split(',').collect();
                if p.len() > 5 && p[5] == "measured" {
                    Some(p[..5].join(","))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut results = output(
        &args.output,
        args.resume,
        "field,backend,mu,r,c,status,mu0,extension_degree,queries,threads,repetitions,commit_time_ms,open_time_ms,verify_time_ms,commitment_bytes,eval_proof_bytes,proof_size_bytes,proof_size_kb,estimated_gib,security_scope,low_memory,diagonal_transition_ms",
    )?;
    let mut raw = output(
        &args.output.with_extension("raw.csv"),
        args.resume,
        "field,backend,mu,r,c,mu0,extension_degree,queries,repetition,certified_iop,estimated_gib,commit_time_ms,open_time_ms,verify_time_ms,commitment_bytes,eval_proof_bytes,setup_time_ms,low_memory,diagonal_transition_ms",
    )?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build()?;
    for config in cases {
        if completed.contains(&name(config)) {
            continue;
        }
        let low = low_memory(config, &args);
        let gib = estimated_gib(config, low);
        if gib > args.max_estimated_gib {
            writeln!(
                results,
                "{},skipped_memory,{},{},{},{},{},,,,,,,,{gib:.3},{},{low},",
                name(config),
                config.mu0,
                config.extension_degree,
                config.num_queries,
                args.threads,
                args.repetitions,
                if config.certified {
                    "certified_conservative_IOP_100_not_FS"
                } else {
                    "functional_only"
                }
            )?;
            results.flush()?;
            eprintln!("skipped {} estimated {gib:.1} GiB", name(config));
            continue;
        }
        eprintln!(
            "running {} threads={} estimated={gib:.3}GiB",
            name(config),
            args.threads
        );
        let measured = pool.install(|| match config.base_field {
            BaseFieldKind::Goldilocks => {
                run::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(config, &args, &mut raw)
            }
            BaseFieldKind::M127 => run::<Mersenne127, M127Quad>(config, &args, &mut raw),
        });
        match measured {
            Ok((ct, ot, vt, cb, pb, dt)) => {
                let total = cb + pb;
                writeln!(
                    results,
                    "{},measured,{},{},{},{},{},{ct:.6},{ot:.6},{vt:.6},{cb},{pb},{total},{:.6},{gib:.3},{},{low},{dt:.6}",
                    name(config),
                    config.mu0,
                    config.extension_degree,
                    config.num_queries,
                    args.threads,
                    args.repetitions,
                    total as f64 / 1024.0,
                    if config.certified {
                        "certified_conservative_IOP_100_not_FS"
                    } else {
                        "functional_only"
                    }
                )?;
                results.flush()?;
                println!(
                    "{} commit={ct:.3}ms open={ot:.3}ms verify={vt:.3}ms proof={total}B",
                    name(config)
                );
            }
            Err(e) => {
                results.flush()?;
                return Err(e);
            }
        }
    }
    eprintln!(
        "results: {} (certificate {})",
        args.output.display(),
        params::CERTIFICATE_VERSION
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_and_resource_estimate() {
        assert_eq!(mus("20..=30").unwrap(), vec![20, 22, 24, 26, 28, 30]);
        assert_eq!(mus("20,24").unwrap(), vec![20, 24]);
        assert!(mus("30..=20").is_err());
        assert!(mus("20..=100").is_err());
        let c =
            HyperFoldConfig::experiment(BaseFieldKind::M127, BaseCodeKind::Rs, 30, 2, 4).unwrap();
        assert!(estimated_gib(c, false) > 500.0);
    }
}
