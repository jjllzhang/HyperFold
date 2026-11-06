//! One official lifecycle for every certified field/code/size selection.
use clap::Parser;
use hyperfold_pcs::{
    benchmark_trial::{METRICS, TrialRecord, run_trial},
    field_m127::{M127Quad, Mersenne127},
    params::{BaseCodeKind, BaseFieldKind, CERTIFIED_CONFIGS, HyperFoldConfig},
    sampling::SampleBase,
};
use p3_field::ExtensionField;
use p3_field::extension::CubicTrinomialExtensionField;
use p3_goldilocks::Goldilocks;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const SCHEMA: &str = "hyperfold-one-shot-bench-v1";
#[derive(Parser, Debug)]
#[command(
    about = "Fresh Setup, one-shot release and queried-row recovery; no legacy memory switches/resume"
)]
struct Args {
    #[arg(long, default_value_t = 32)]
    threads: usize,
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
    #[arg(long, default_value_t = 1)]
    trial_round: usize,
    /// List selected certified rows without allocating Setup or running timings.
    #[arg(long)]
    list: bool,
    /// New summary path; companion .raw.csv must also be absent.
    #[arg(long, required_unless_present = "list")]
    output: Option<PathBuf>,
}
fn mus(input: &str) -> Result<Vec<usize>> {
    let v = if let Some((a, b)) = input.split_once("..=") {
        let a = a.parse::<usize>()?;
        let b = b.parse::<usize>()?;
        if a > b || b > 30 {
            return Err("invalid mu range".into());
        }
        (a..=b).filter(|v| v % 2 == 0).collect()
    } else {
        input
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<Vec<usize>, _>>()?
    };
    if v.is_empty() || v.iter().any(|v| !(20..=30).contains(v) || v % 2 != 0) {
        return Err("mu must select certified even sizes20..30".into());
    }
    Ok(v)
}
fn configs(args: &Args) -> Result<Vec<HyperFoldConfig>> {
    if args.threads != 32 || args.repetitions == 0 || args.trial_round == 0 {
        return Err("benchmark requires32threads and positive repetitions/trial-round".into());
    }
    let mus = mus(&args.mu)?;
    let fields = args
        .fields
        .iter()
        .map(|f| match f.as_str() {
            "goldilocks" => Ok(BaseFieldKind::Goldilocks),
            "m127" => Ok(BaseFieldKind::M127),
            _ => Err("unknown field"),
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let codes = args
        .backends
        .iter()
        .map(|f| match f.as_str() {
            "qa" => Ok(BaseCodeKind::Qa),
            "rs" => Ok(BaseCodeKind::Rs),
            _ => Err("unknown backend"),
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if args.r.iter().any(|r| ![2, 4, 8].contains(r)) || args.c.iter().any(|c| ![2, 4].contains(c)) {
        return Err("r/c must select certified rows".into());
    }
    let selected = CERTIFIED_CONFIGS
        .iter()
        .filter(|c| {
            fields.contains(&c.base_field)
                && codes.contains(&c.backend)
                && mus.contains(&c.mu)
                && args.r.contains(&c.r)
                && args.c.contains(&c.c)
        })
        .copied()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("empty configuration selection".into());
    }
    Ok(selected)
}
fn identity(c: HyperFoldConfig, args: &Args) -> Vec<String> {
    vec![
        SCHEMA.into(),
        format!("{:?}", c.base_field),
        format!("{:?}", c.backend),
        c.mu.to_string(),
        c.r.to_string(),
        c.c.to_string(),
        c.mu0.to_string(),
        c.depth().to_string(),
        c.extension_degree.to_string(),
        c.num_queries.to_string(),
        args.threads.to_string(),
        args.trial_round.to_string(),
        "prehashed_recovered".into(),
        "opening_only".into(),
        "consumed_after_first_round".into(),
        "true".into(),
    ]
}
const ID: &str = "schema_version,field,backend,mu,r,c,mu0,depth,extension_degree,queries,threads,trial_round,first_tree_strategy,parameter_lifecycle,witness_lifecycle,setup_per_trial";
const PROOF: &str = "commitment_bytes,eval_proof_bytes,proof_size_bytes,fingerprint,parameters_id,commitment_root,y0_hash,proof_hash,challenge_hash";
fn proof_cells(r: &TrialRecord) -> Vec<String> {
    vec![
        r.commitment_bytes.to_string(),
        r.eval_proof_bytes.to_string(),
        (r.commitment_bytes + r.eval_proof_bytes).to_string(),
        r.fingerprint.clone(),
        r.parameters_id.clone(),
        r.commitment_root.clone(),
        r.y0_hash.clone(),
        r.proof_hash.clone(),
        r.challenge_hash.clone(),
    ]
}
fn fresh(path: &Path, header: &str) -> Result<BufWriter<File>> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
    writeln!(file, "{header}")?;
    file.flush()?;
    Ok(file)
}
fn outputs(path: &Path) -> Result<(BufWriter<File>, BufWriter<File>)> {
    let raw = path.with_extension("raw.csv");
    for p in [path, raw.as_path()] {
        match std::fs::symlink_metadata(p) {
            Ok(_) => return Err(format!("output must be new: {}", p.display()).into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    let stats = METRICS
        .iter()
        .flat_map(|m| [format!("{m}_mean"), format!("{m}_sample_sd")])
        .collect::<Vec<_>>()
        .join(",");
    let summary = fresh(
        path,
        &format!("{ID},status,measured_repetitions,{stats},{PROOF}"),
    )?;
    let raw = fresh(
        &raw,
        &format!(
            "{ID},repetition,{},forward_capacity_elements,inverse_capacity_elements,released_witness_elements,released_witness_capacity,witness_release_level,witness_release_subround,{PROOF}",
            METRICS.join(",")
        ),
    )?;
    Ok((summary, raw))
}
fn run<F: SampleBase, E: ExtensionField<F>>(
    c: HyperFoldConfig,
    args: &Args,
    summary: &mut impl Write,
    raw: &mut impl Write,
) -> Result<()> {
    let mut reference = None;
    let mut samples = Vec::with_capacity(args.repetitions);
    let mut last = None;
    for repetition in 0..=args.repetitions {
        let r = run_trial::<F, E>(c)?;
        if let Some(expected) = reference.as_ref() {
            if expected != &r.fingerprint {
                return Err("nondeterministic proof/transcript".into());
            }
        } else {
            reference = Some(r.fingerprint.clone());
        }
        if repetition == 0 {
            continue;
        }
        let mut row = identity(c, args);
        row.push(repetition.to_string());
        row.extend(r.metrics().map(|x| format!("{x:.6}")));
        row.extend([
            r.forward_capacity.to_string(),
            r.inverse_capacity.to_string(),
            r.timings.released_witness_elements.to_string(),
            r.timings.released_witness_capacity.to_string(),
            r.timings
                .witness_release_round
                .map(|v| v.0.to_string())
                .unwrap_or_default(),
            r.timings
                .witness_release_round
                .map(|v| v.1.to_string())
                .unwrap_or_default(),
        ]);
        row.extend(proof_cells(&r));
        writeln!(raw, "{}", row.join(","))?;
        raw.flush()?;
        samples.push(r.metrics());
        last = Some(r);
    }
    let mut row = identity(c, args);
    row.extend(["measured".into(), samples.len().to_string()]);
    for m in 0..METRICS.len() {
        let mean = samples.iter().map(|r| r[m]).sum::<f64>() / samples.len() as f64;
        let sd = if samples.len() > 1 {
            (samples.iter().map(|r| (r[m] - mean).powi(2)).sum::<f64>()
                / (samples.len() - 1) as f64)
                .sqrt()
        } else {
            0.
        };
        row.extend([format!("{mean:.6}"), format!("{sd:.6}")]);
    }
    row.extend(proof_cells(&last.unwrap()));
    writeln!(summary, "{}", row.join(","))?;
    summary.flush()?;
    Ok(())
}
fn main() -> Result<()> {
    let args = Args::parse();
    let cases = configs(&args)?;
    if args.list {
        println!("{ID}");
        for c in cases {
            println!("{}", identity(c, &args).join(","))
        }
        return Ok(());
    }
    let (mut summary, mut raw) = outputs(args.output.as_ref().unwrap())?;
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build()?
        .install(|| {
            for c in cases {
                match c.base_field {
                    BaseFieldKind::Goldilocks => run::<
                        Goldilocks,
                        CubicTrinomialExtensionField<Goldilocks>,
                    >(
                        c, &args, &mut summary, &mut raw
                    )?,
                    BaseFieldKind::M127 => {
                        run::<Mersenne127, M127Quad>(c, &args, &mut summary, &mut raw)?
                    }
                }
            }
            Ok(())
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_and_removed_flags() {
        let args = Args::try_parse_from(["bench", "--list"]).unwrap();
        assert_eq!(configs(&args).unwrap().len(), 144);
        assert_eq!(mus("20..=30").unwrap(), vec![20, 22, 24, 26, 28, 30]);
        assert!(mus("30..=20").is_err());
        for legacy in [
            vec!["--resume"],
            vec!["--memory-mode", "retained"],
            vec!["--max-estimated-gib", "64"],
            vec!["--first-tree-strategy", "recovered"],
        ] {
            let mut argv = vec!["bench", "--list"];
            argv.extend(legacy);
            assert!(Args::try_parse_from(argv).is_err());
        }
        let args = Args::try_parse_from(["bench", "--list", "--threads", "1"]).unwrap();
        assert!(configs(&args).is_err());
    }
    #[test]
    fn old_outputs_are_unchanged() {
        for (index, which) in ["summary", "raw", "both"].iter().enumerate() {
            let dir = std::env::temp_dir().join(format!(
                "hyperfold-one-shot-output-{}-{index}",
                std::process::id()
            ));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("results.csv");
            let raw = path.with_extension("raw.csv");
            if *which != "raw" {
                std::fs::write(&path, b"old-summary\n").unwrap()
            }
            if *which != "summary" {
                std::fs::write(&raw, b"old-raw\n").unwrap()
            }
            assert!(outputs(&path).is_err());
            if *which != "raw" {
                assert_eq!(std::fs::read(&path).unwrap(), b"old-summary\n")
            } else {
                assert!(!path.exists())
            }
            if *which != "summary" {
                assert_eq!(std::fs::read(&raw).unwrap(), b"old-raw\n")
            } else {
                assert!(!raw.exists())
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
