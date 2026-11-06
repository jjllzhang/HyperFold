//! Criterion measures the same full fresh-Setup lifecycle as the official CLI.
use criterion::{Criterion, criterion_group, criterion_main};
use hyperfold_pcs::{
    benchmark_trial::run_trial,
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
};
use p3_field::extension::CubicTrinomialExtensionField;
use p3_goldilocks::Goldilocks;
fn benches(c: &mut Criterion) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(32)
        .build()
        .unwrap();
    let config =
        HyperFoldConfig::experiment(BaseFieldKind::Goldilocks, BaseCodeKind::Qa, 20, 8, 2).unwrap();
    let mut g =
        c.benchmark_group("Goldilocks_QA_mu20_r8_c2_e3_threads32_fresh_Setup_one_shot_full_cycle");
    g.sample_size(10);
    g.warm_up_time(std::time::Duration::from_secs(1));
    g.measurement_time(std::time::Duration::from_secs(3));
    g.bench_function("setup_input_commit_release_open_verify_wire_cleanup", |b| {
        b.iter(|| {
            pool.install(|| {
                run_trial::<Goldilocks, CubicTrinomialExtensionField<Goldilocks>>(config).unwrap()
            })
        })
    });
    g.finish();
}
criterion_group!(pcs, benches);
criterion_main!(pcs);
