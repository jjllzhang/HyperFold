//! Offline 32-persistent-worker kernels; CSV goes to stdout, never results/.
//! Run: taskset -c <32 physical CPUs> target/release/deps/m127_kernels-* --windows 5
//! Each worker window accumulates >=100ms of actual kernel work after calibration.
//! Worker timing excludes setup/checks; outer wall includes barriers and checks.
use clap::Parser;
use hyperfold_pcs::{
    base_code::{BaseCode, BaseCodeImpl},
    field_m127::{M127Quad, Mersenne127, P},
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    sampling::{SampleBase, stream},
};
use p3_field::{ExtensionField, Field, PrimeCharacteristicRing};
use std::{
    hint::black_box,
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

struct Job {
    name: String,
    k0: usize,
    c: usize,
    iterations: usize,
    systematic: usize,
    parity: usize,
    run: Box<dyn FnMut(usize) -> Duration + Send>,
}
fn arithmetic(worker: usize) -> Vec<Job> {
    let mut rng = stream(&[71; 32], b"kernel-input", worker as u64);
    let xs: Vec<_> = (0..1024)
        .map(|_| {
            let x = Mersenne127::sample(&mut rng);
            if x == Mersenne127::ZERO {
                Mersenne127::ONE
            } else {
                x
            }
        })
        .collect();
    let es: Vec<_> = (0..1024)
        .map(|i| M127Quad::new([xs[i], xs[(i + 17) % 1024]]))
        .collect();
    let mut jobs = Vec::new();
    for op in ["F_square", "E_square", "F_inverse"] {
        let x = xs.clone();
        let e = es.clone();
        let iterations = if op == "F_inverse" { 1024 } else { 262144 };
        let expected_f = if op == "F_inverse" {
            x[(iterations - 1) % 1024].pow_u128(P - 2)
        } else {
            x[(iterations - 1) % 1024] * x[(iterations - 1) % 1024]
        };
        let expected_e = e[(iterations - 1) % 1024] * e[(iterations - 1) % 1024];
        jobs.push(Job {
            name: op.into(),
            k0: 0,
            c: 0,
            iterations,
            systematic: 0,
            parity: 0,
            run: Box::new(move |n| {
                let mut f = Mersenne127::ZERO;
                let mut z = M127Quad::ZERO;
                let start = Instant::now();
                for i in 0..n {
                    match op {
                        "F_square" => f = black_box(black_box(x[i % 1024]).square()),
                        "E_square" => z = black_box(black_box(e[i % 1024]).square()),
                        _ => f = black_box(black_box(x[i % 1024]).inverse()),
                    }
                }
                let elapsed = start.elapsed();
                if op == "E_square" {
                    assert_eq!(z, expected_e);
                } else {
                    assert_eq!(f, expected_f);
                }
                elapsed
            }),
        });
    }
    jobs
}
fn encoding<T: ExtensionField<Mersenne127> + Send + 'static>(
    worker: usize,
    tag: &str,
    codes: &[(usize, usize, Arc<BaseCodeImpl<Mersenne127>>)],
) -> Vec<Job> {
    let mut jobs = Vec::new();
    for (k0, c, code) in codes {
        let (k0, c) = (*k0, *c);
        let mut rng = stream(&[72; 32], b"kernel-message", worker as u64 + k0 as u64);
        let msg: Vec<T> = (0..k0)
            .map(|_| T::from_basis_coefficients_fn(|_| Mersenne127::sample(&mut rng)))
            .collect();
        let coeff: Vec<_> = (0..k0).map(|_| Mersenne127::sample(&mut rng)).collect();
        let reference: T = msg.iter().zip(&coeff).map(|(&a, &b)| a * b).sum();
        for batched in [false, true] {
            let (msg, coeff) = (msg.clone(), coeff.clone());
            jobs.push(Job {
                name: format!("{tag}_row_{}", if batched { "batched" } else { "serial" }),
                k0,
                c,
                iterations: 2048,
                systematic: 0,
                parity: 1,
                run: Box::new(move |n| {
                    let mut result = T::ZERO;
                    let start = Instant::now();
                    for _ in 0..n {
                        result = black_box(if batched {
                            T::batched_linear_combination(black_box(&msg), black_box(&coeff))
                        } else {
                            black_box(&msg)
                                .iter()
                                .zip(black_box(&coeff))
                                .map(|(&a, &b)| a * b)
                                .sum()
                        });
                    }
                    let elapsed = start.elapsed();
                    assert_eq!(result, reference);
                    elapsed
                }),
            });
        }
        let mut full = vec![T::ZERO; k0 * c];
        code.encode_slice(&msg, &mut full);
        // Deterministic synthetic projected query set, NOT a PCS transcript query trace.
        let mut indices: Vec<_> = (0..160).map(|i| (i * 37 + 11) % (k0 * c)).collect();
        indices.sort_unstable();
        indices.dedup();
        let expected: Vec<_> = indices.iter().map(|&j| (j, full[j])).collect();
        let systematic = indices.iter().filter(|&&j| j < k0).count();
        let parity = indices.len() - systematic;
        {
            let (code, msg, reference) = (code.clone(), msg.clone(), full.clone());
            let mut out = vec![T::ZERO; k0 * c];
            jobs.push(Job {
                name: format!("{tag}_full_encode"),
                k0,
                c,
                iterations: 8,
                systematic: k0,
                parity: (c - 1) * k0,
                run: Box::new(move |n| {
                    let start = Instant::now();
                    for _ in 0..n {
                        code.encode_slice(black_box(&msg), black_box(&mut out));
                        black_box(&out);
                    }
                    let elapsed = start.elapsed();
                    assert_eq!(out, reference);
                    elapsed
                }),
            });
        }
        for selected in [false, true] {
            let (code, msg, expected) = (code.clone(), msg.clone(), expected.clone());
            let mut out = vec![T::ZERO; k0 * c];
            jobs.push(Job {
                name: format!(
                    "{tag}_check_{}",
                    if selected { "api" } else { "full_reference" }
                ),
                k0,
                c,
                iterations: 8,
                systematic,
                parity,
                run: Box::new(move |n| {
                    let mut ok = false;
                    let start = Instant::now();
                    for _ in 0..n {
                        ok = black_box(if selected {
                            code.check_encoded_at(black_box(&msg), black_box(&expected))
                        } else {
                            code.encode_slice(black_box(&msg), black_box(&mut out));
                            black_box(&expected).iter().all(|&(j, v)| out[j] == v)
                        });
                    }
                    let elapsed = start.elapsed();
                    assert!(ok);
                    elapsed
                }),
            });
        }
    }
    jobs
}
const MIN_WINDOW: Duration = Duration::from_millis(100);
#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 5)]
    windows: usize,
    /// Exact kernel names, e.g. F_square,E_full_encode. Empty runs every kernel.
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
}
type WorkerPanic = Box<dyn std::any::Any + Send>;
fn checked_run(job: &mut Job, count: usize, failure: &mut Option<WorkerPanic>) -> Duration {
    if failure.is_some() {
        return Duration::ZERO;
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (job.run)(count))) {
        Ok(elapsed) => elapsed,
        Err(error) => {
            *failure = Some(error);
            Duration::ZERO
        }
    }
}
fn main() {
    let args = Args::parse();
    let windows = args.windows;
    assert!(windows >= 5, "at least five measured windows");
    let mut codes = Vec::new();
    for k0 in [64usize, 128, 256] {
        for c in [2, 4] {
            let cfg = HyperFoldConfig::testing(
                BaseFieldKind::M127,
                BaseCodeKind::Rs,
                k0.ilog2() as usize + 3,
                k0.ilog2() as usize,
                8,
                c,
                2,
            )
            .unwrap();
            codes.push((
                k0,
                c,
                Arc::new(BaseCodeImpl::<Mersenne127>::new(cfg, &[42; 32]).unwrap()),
            ));
        }
    }
    let workers: Vec<_> = (0..32)
        .map(|w| {
            let mut jobs = arithmetic(w);
            jobs.extend(encoding::<Mersenne127>(w, "F", &codes));
            jobs.extend(encoding::<M127Quad>(w, "E", &codes));
            jobs.retain(|job| args.only.is_empty() || args.only.contains(&job.name));
            jobs
        })
        .collect();
    for name in &args.only {
        assert!(
            workers[0].iter().any(|job| &job.name == name),
            "unknown kernel {name}"
        );
    }
    let names: Vec<_> = workers[0].iter().map(|j| j.name.clone()).collect();
    let barrier = Arc::new(Barrier::new(33));
    let mut wall = Vec::new();
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = workers
            .into_iter()
            .enumerate()
            .map(|(worker, mut jobs)| {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    let mut rows = Vec::new();
                    let mut failure = None;
                    for (index, job) in jobs.iter_mut().enumerate() {
                        // Calibration is outside all measured windows. The original iteration
                        // count is a multiple of1024 for arithmetic, preserving its input cycle.
                        barrier.wait();
                        let initial = job.iterations;
                        let calibration = checked_run(job, initial, &mut failure);
                        barrier.wait();
                        let multiplier = (MIN_WINDOW.as_nanos() * 3 / 2)
                            .div_ceil(calibration.as_nanos().max(1))
                            .clamp(1, 100_000) as usize;
                        let batch = initial.checked_mul(multiplier).unwrap();
                        for window in 0..=windows {
                            barrier.wait();
                            let mut elapsed = Duration::ZERO;
                            let mut iterations = 0usize;
                            let mut batches = 0usize;
                            // Local repetition, NOT repeated thread-pool task dispatch. Every
                            // batch black-boxes each input/output inside its timed kernel loop.
                            while elapsed < MIN_WINDOW && failure.is_none() {
                                elapsed += checked_run(job, batch, &mut failure);
                                iterations += batch;
                                batches += 1;
                            }
                            barrier.wait();
                            if window > 0 {
                                rows.push((
                                    index,
                                    format!(
                                        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                                        job.name,
                                        job.k0,
                                        job.c,
                                        worker,
                                        window,
                                        iterations,
                                        elapsed.as_nanos(),
                                        elapsed.as_secs_f64() * 1e9 / iterations as f64,
                                        job.systematic,
                                        job.parity,
                                        initial,
                                        calibration.as_nanos(),
                                        batch,
                                        batches,
                                        MIN_WINDOW.as_nanos()
                                    ),
                                ));
                            }
                        }
                    }
                    if let Some(error) = failure {
                        std::panic::resume_unwind(error);
                    }
                    rows
                })
            })
            .collect();
        for _ in &names {
            barrier.wait();
            barrier.wait(); // untimed calibration
            for window in 0..=windows {
                let start = Instant::now();
                barrier.wait();
                barrier.wait();
                if window > 0 {
                    wall.push(start.elapsed().as_nanos());
                }
            }
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    println!(
        "kernel,k0,c,worker,window,iterations,worker_ns,ns_per_operation,selected_systematic,selected_parity,calibration_iterations,calibration_ns,batch_iterations,batches,min_window_ns,outer_wall_ns"
    );
    for rows in results {
        for (index, row) in rows {
            let window: usize = row.split(',').nth(4).unwrap().parse().unwrap();
            println!("{row},{}", wall[index * windows + window - 1]);
        }
    }
}
