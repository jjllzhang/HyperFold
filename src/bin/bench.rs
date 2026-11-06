//! Reproducible sequential experiment sweep. All prover/verifier parallel work
//! uses the requested local Rayon pool; no background services or remote effects.
use clap::Parser;
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad, Mersenne127},
    params::{self, BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::{FirstTreeStrategy, HyperFoldPcs, OpenOptions},
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
    /// Explicit queried-row strategy; requires retained memory mode and a new output.
    /// No estimate model or estimate-only exclusion is applied to this strategy.
    #[arg(long, value_parser=["recovered"])]
    first_tree_strategy: Option<String>,
}
fn validate_modes(args: &Args) -> Result<(), &'static str> {
    if args.first_tree_strategy.is_some() && (args.memory_mode != "retained" || args.resume) {
        return Err(
            "recovered requires explicit --memory-mode retained and a fresh output (no --resume)",
        );
    }
    Ok(())
}
fn strategy_suffix(args: &Args, fingerprint: &str) -> String {
    if args.first_tree_strategy.is_some() {
        format!(",prehashed_recovered,retained_both,{fingerprint}")
    } else {
        String::new()
    }
}
fn strategy_header(args: &Args) -> &'static str {
    if args.first_tree_strategy.is_some() {
        ",first_tree_strategy,diagonal_storage,fingerprint"
    } else {
        ""
    }
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
type Measurements = (f64, f64, f64, usize, usize, f64, String);
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
    let mut reference_fingerprint = None;
    let mut fingerprint = String::new();
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
        let (proof, mut prover) = if args.first_tree_strategy.is_some() {
            assert!(pcs.diagonals.has_forward() && pcs.diagonals.has_inverse());
            pcs.open_with_strategy(&cm, &data, &z, y, FirstTreeStrategy::PrehashedRecovered)?
        } else {
            pcs.open_with_options(
                &cm,
                &data,
                &z,
                y,
                OpenOptions {
                    recompute_first_tree: low,
                },
            )?
        };
        let open = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let mut verifier = pcs.verify(&cm, &z, y, &proof)?;
        let verify = start.elapsed().as_secs_f64() * 1000.0;
        let challenge = prover.sample::<F, E>(b"post-benchmark-agreement");
        if challenge != verifier.sample::<F, E>(b"post-benchmark-agreement") {
            return Err("transcript disagreement".into());
        }
        let cm_bytes = codec::encode(&cm)?;
        let proof_bytes = codec::encode(&proof)?;
        let sizes = (cm_bytes.len(), proof_bytes.len());
        if args.first_tree_strategy.is_some() {
            let mut hash = blake3::Hasher::new();
            hash.update(b"hyperfold-formal-recovered-bench-v1");
            hash.update(&(cm_bytes.len() as u64).to_le_bytes());
            hash.update(&cm_bytes);
            hash.update(&proof_bytes);
            hash.update(&codec::encode(&challenge)?);
            let current = hash.finalize();
            if let Some(expected) = reference_fingerprint {
                if current != expected {
                    return Err("non-deterministic proof/transcript fingerprint".into());
                }
            } else {
                reference_fingerprint = Some(current);
            }
            fingerprint = current.to_hex().to_string();
        }
        if repetition > 0 {
            if repetition > 1 && sizes != bytes {
                return Err("non-deterministic proof size".into());
            }
            bytes = sizes;
            transition_sum += transition_ms;
            for (acc, v) in sum.iter_mut().zip([commit, open, verify]) {
                *acc += v;
            }
            let estimate = if args.first_tree_strategy.is_some() {
                String::new()
            } else {
                format!("{:.9}", estimated_gib(config, low))
            };
            let suffix = strategy_suffix(args, &fingerprint);
            writeln!(
                raw,
                "{},{},{},{},{},{},{estimate},{commit:.6},{open:.6},{verify:.6},{},{},{setup_ms:.6},{low},{transition_ms:.6}{suffix}",
                name(config),
                config.mu0,
                config.extension_degree,
                config.num_queries,
                repetition,
                config.certified,
                sizes.0,
                sizes.1
            )?;
            raw.flush()?;
        }
    }
    // Both Setup directions must remain live even through the final recovered opening.
    if args.first_tree_strategy.is_some() {
        std::hint::black_box(&pcs);
    }
    Ok((
        sum[0] / args.repetitions as f64,
        sum[1] / args.repetitions as f64,
        sum[2] / args.repetitions as f64,
        bytes.0,
        bytes.1,
        transition_sum / args.repetitions as f64,
        fingerprint,
    ))
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    validate_modes(&args)?;
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
            "field,backend,mu,r,c,mu0,extension_degree,queries,estimated_gib,certified_iop,low_memory{}",
            strategy_header(&args)
        );
        for c in cases {
            let estimate = if args.first_tree_strategy.is_some() {
                String::new()
            } else {
                format!("{:.3}", estimated_gib(c, low_memory(c, &args)))
            };
            let suffix = strategy_suffix(&args, "");
            println!(
                "{},{},{},{},{estimate},{},{}{suffix}",
                name(c),
                c.mu0,
                c.extension_degree,
                c.num_queries,
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
        &format!(
            "field,backend,mu,r,c,status,mu0,extension_degree,queries,threads,repetitions,commit_time_ms,open_time_ms,verify_time_ms,commitment_bytes,eval_proof_bytes,proof_size_bytes,proof_size_kb,estimated_gib,security_scope,low_memory,diagonal_transition_ms{}",
            strategy_header(&args)
        ),
    )?;
    let mut raw = output(
        &args.output.with_extension("raw.csv"),
        args.resume,
        &format!(
            "field,backend,mu,r,c,mu0,extension_degree,queries,repetition,certified_iop,estimated_gib,commit_time_ms,open_time_ms,verify_time_ms,commitment_bytes,eval_proof_bytes,setup_time_ms,low_memory,diagonal_transition_ms{}",
            strategy_header(&args)
        ),
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
        if args.first_tree_strategy.is_none() && gib > args.max_estimated_gib {
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
            "running {} threads={} estimate_model={} strategy={}",
            name(config),
            args.threads,
            if args.first_tree_strategy.is_some() {
                "not_available".to_string()
            } else {
                format!("{gib:.3}GiB")
            },
            if args.first_tree_strategy.is_some() {
                "prehashed_recovered"
            } else {
                "legacy"
            }
        );
        let measured = pool.install(|| match config.base_field {
            BaseFieldKind::Goldilocks => {
                run::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(config, &args, &mut raw)
            }
            BaseFieldKind::M127 => run::<Mersenne127, M127Quad>(config, &args, &mut raw),
        });
        match measured {
            Ok((ct, ot, vt, cb, pb, dt, fingerprint)) => {
                let total = cb + pb;
                let suffix = strategy_suffix(&args, &fingerprint);
                let estimate = if args.first_tree_strategy.is_some() {
                    String::new()
                } else {
                    format!("{gib:.3}")
                };
                writeln!(
                    results,
                    "{},measured,{},{},{},{},{},{ct:.6},{ot:.6},{vt:.6},{cb},{pb},{total},{:.6},{estimate},{},{low},{dt:.6}{suffix}",
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
    fn recovered_requires_explicit_retained_fresh_output_and_labels_schema() {
        let legacy = Args::try_parse_from(["bench"]).unwrap();
        assert!(validate_modes(&legacy).is_ok());
        assert_eq!(strategy_header(&legacy), "");
        assert_eq!(strategy_suffix(&legacy, "x"), "");
        for mode in ["auto", "low"] {
            let args = Args::try_parse_from([
                "bench",
                "--memory-mode",
                mode,
                "--first-tree-strategy",
                "recovered",
            ])
            .unwrap();
            assert!(validate_modes(&args).is_err());
        }
        let args = Args::try_parse_from([
            "bench",
            "--memory-mode",
            "retained",
            "--first-tree-strategy",
            "recovered",
        ])
        .unwrap();
        assert!(validate_modes(&args).is_ok());
        assert_eq!(
            strategy_suffix(&args, "abc"),
            ",prehashed_recovered,retained_both,abc"
        );
        let mut args = args;
        args.resume = true;
        assert!(validate_modes(&args).is_err());
    }
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
