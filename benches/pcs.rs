use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use hyperfold_pcs::{
    codec,
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::HyperFoldPcs,
    sampling::{SampleBase, stream},
    sumcheck::evaluate,
};
use p3_field::PrimeCharacteristicRing;
use p3_field::extension::CubicTrinomialExtensionField;
use p3_goldilocks::Goldilocks as F;
type E = CubicTrinomialExtensionField<F>;
fn benches(c: &mut Criterion) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(32)
        .build()
        .unwrap();
    let config =
        HyperFoldConfig::experiment(BaseFieldKind::Goldilocks, BaseCodeKind::Qa, 20, 8, 2).unwrap();
    let pcs = pool.install(|| HyperFoldPcs::<F, E>::setup(config, [42; 32]).unwrap());
    let message = || {
        let mut r = stream(&[27; 32], b"criterion-message", 0);
        (0..config.k())
            .map(|_| F::sample(&mut r))
            .collect::<Vec<_>>()
    };
    let z: Vec<E> = (0..config.mu).map(|i| E::from_usize(i + 2)).collect();
    let m = message();
    let y = pool.install(|| evaluate(&m, &z));
    let (cm, data) = pool.install(|| pcs.commit(m).unwrap());
    let (proof, _) = pool.install(|| pcs.open(&cm, &data, &z, y).unwrap());
    let mut group = c.benchmark_group("Goldilocks_QA_mu20_r8_c2_e3_threads32_IOP100");
    group.sample_size(10);
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.measurement_time(std::time::Duration::from_secs(3));
    group.bench_function("commit", |b| {
        b.iter_batched(
            message,
            |m| pool.install(|| pcs.commit(m).unwrap()),
            BatchSize::LargeInput,
        )
    });
    group.bench_function("open", |b| {
        b.iter(|| pool.install(|| pcs.open(&cm, &data, &z, y).unwrap()))
    });
    group.bench_function("verify", |b| {
        b.iter(|| pool.install(|| pcs.verify(&cm, &z, y, &proof).unwrap()))
    });
    group.bench_function("wire_size", |b| {
        b.iter(|| codec::encode(&proof).unwrap().len())
    });
    group.finish();
}
criterion_group!(pcs, benches);
criterion_main!(pcs);
