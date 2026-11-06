use super::*;
use hyperfold_pcs::{
    codec,
    field_m127::{M127Quad as E, Mersenne127 as F},
    params::{BaseCodeKind, BaseFieldKind, HyperFoldConfig},
    pcs::{Commitment, HyperFoldPcs, Proof},
    sampling::stream,
    sumcheck::evaluate,
};
use std::{
    collections::hash_map::DefaultHasher,
    mem::{align_of, size_of},
};
type A = Base<ark::Ark>;
type AQ = Quad<ark::Ark>;
type Q = Base<fourq::FourQ>;
type QQ = Quad<fourq::FourQ>;
fn bytes<T: Serialize>(x: &T) -> Vec<u8> {
    codec::encode(x).unwrap()
}
fn same<T: Serialize, U: Serialize>(a: T, b: U) {
    assert_eq!(bytes(&a), bytes(&b))
}
fn hash<T: Hash>(v: T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}
fn differential<B: Engine>() {
    assert_eq!(size_of::<Base<B>>(), 16);
    assert_eq!(size_of::<Quad<B>>(), 32);
    let mut rng = stream(&[9; 32], b"adapter-differential", 0);
    let edges = [
        0,
        1,
        2,
        P - 1,
        P - 2,
        (1 << 64) - 1,
        1 << 64,
        (1 << 126) + 1,
    ];
    for n in 0..10000 {
        let x = if n < edges.len() {
            F::new(edges[n])
        } else {
            F::sample(&mut rng)
        };
        let y = F::sample(&mut rng);
        let a = Base::<B>::new(x.canonical());
        let b = Base::<B>::new(y.canonical());
        same(a + b, x + y);
        same(a - b, x - y);
        same(a * b, x * y);
        same(-a, -x);
        same(a.square(), x.square());
        same(a.halve(), x.halve());
        assert_eq!(a.canonical(), x.canonical());
        assert_eq!(a.into_bytes().into_iter().collect::<Vec<_>>(), bytes(&x));
        if n < 128 {
            same(a.try_inverse(), x.try_inverse());
        }
        let z = F::sample(&mut rng);
        let w = F::sample(&mut rng);
        let aq = Quad([a, Base::<B>::new(z.canonical())]);
        let bq = Quad([b, Base::<B>::new(w.canonical())]);
        let ex = E::new([x, z]);
        let ey = E::new([y, w]);
        same(aq + bq, ex + ey);
        same(aq - bq, ex - ey);
        same(-aq, -ex);
        same(aq * bq, ex * ey);
        same(aq.square(), ex.square());
        same(aq.halve(), ex.halve());
        same(aq * b, ex * y);
        if n < 128 {
            same(aq.try_inverse(), ex.try_inverse());
        }
    }
    same(Quad::<B>::GENERATOR, E::GENERATOR);
    same(Base::<B>::GENERATOR, F::GENERATOR);
    assert_eq!(
        <Quad<B> as AlgebraIdentity<Base<B>>>::algebra_id(),
        <E as AlgebraIdentity<F>>::algebra_id()
    );
    // Frobenius a+bi -> a-bi, using p-th powering rather than a backend-specific shortcut.
    let x = Quad([Base::<B>::new(123), Base::<B>::new(456)]);
    let mut v = Quad::<B>::ONE;
    let mut b = x;
    let mut e = P;
    while e > 0 {
        if e & 1 == 1 {
            v *= b
        }
        b = b.square();
        e >>= 1
    }
    assert_eq!(v, Quad([x.0[0], -x.0[1]]));
    let mut r1 = stream(&[19; 32], b"same-XOF", 0);
    let mut r2 = r1.clone();
    for _ in 0..1000 {
        same(Base::<B>::sample(&mut r1), F::sample(&mut r2));
    }
    let mut t1 = [0; 32];
    let mut t2 = [0; 32];
    r1.fill(&mut t1);
    r2.fill(&mut t2);
    assert_eq!(t1, t2);
    for x in [P, u128::MAX] {
        assert!(codec::decode::<Base<B>>(&x.to_le_bytes(), 16).is_err());
        let mut bad = x.to_le_bytes().to_vec();
        bad.extend([0; 16]);
        assert!(codec::decode::<Quad<B>>(&bad, 32).is_err());
    }
}
#[test]
fn ark_arithmetic() {
    differential::<ark::Ark>()
}
#[test]
fn fourq_arithmetic() {
    differential::<fourq::FourQ>()
}
#[test]
fn fourq_redundant_zero_and_layout() {
    assert_eq!(align_of::<Q>(), 16);
    assert_eq!(align_of::<QQ>(), 16);
    let p = fourq::redundant_zero();
    let z = Q::ZERO;
    assert_eq!(p, z);
    assert_eq!(hash(p), hash(z));
    assert_eq!(p.cmp(&z), std::cmp::Ordering::Equal);
    assert!(p.is_zero());
    assert!(!p.is_one());
    assert_eq!(p.try_inverse(), None);
    same(p, z);
    assert_eq!(p.to_string(), "0");
    let reference = |a: Q| F::new(a.canonical());
    let closed = |a: Q| {
        let limbs = a.0.0;
        let raw = limbs[0] as u128 | ((limbs[1] as u128) << 64);
        assert!(raw <= P);
    };
    for a in [p, z, Q::ONE, Q::NEG_ONE, Q::new(P - 2)] {
        for b in [p, z, Q::ONE, Q::NEG_ONE] {
            let x = reference(a);
            let y = reference(b);
            for (actual, expected) in [
                (a + b, x + y),
                (a - b, x - y),
                (a * b, x * y),
                (-a, -x),
                (a.square(), x.square()),
                (a.halve(), x.halve()),
            ] {
                closed(actual);
                same(actual, expected);
            }
            same(a.try_inverse(), x.try_inverse());
            if let Some(i) = a.try_inverse() {
                closed(i);
            }
        }
    }
    let values = [
        Quad([p, z]),
        Quad([z, p]),
        Quad([p, p]),
        Quad([Q::ONE, p]),
        Quad([p, Q::ONE]),
        Quad([Q::NEG_ONE, p]),
        Quad([p, Q::new(P - 2)]),
    ];
    for a in values {
        for b in values {
            let x = E::new(a.0.map(reference));
            let y = E::new(b.0.map(reference));
            for (actual, expected) in [
                (a + b, x + y),
                (a - b, x - y),
                (a * b, x * y),
                (-a, -x),
                (a.square(), x.square()),
                (a.halve(), x.halve()),
                (a * b.0[0], x * reference(b.0[0])),
            ] {
                for c in actual.0 {
                    closed(c);
                }
                same(actual, expected);
            }
            same(a.try_inverse(), x.try_inverse());
            if let Some(i) = a.try_inverse() {
                for c in i.0 {
                    closed(c);
                }
                assert_eq!(a * i, QQ::ONE);
            }
        }
    }
    let inverse = QQ::ONE.inverse();
    assert_eq!(inverse, QQ::ONE);
    same(inverse, E::ONE);
    assert_eq!(inverse.0[1].0.0, [u64::MAX, u64::MAX >> 1]);
    for a in [
        Quad::<fourq::FourQ>([p, z]),
        Quad::<fourq::FourQ>([p, p]),
        Quad::<fourq::FourQ>([z, p]),
    ] {
        assert!(a.is_zero());
        assert_eq!(a.try_inverse(), None);
        same(a, E::ZERO);
        assert_eq!(hash(a), hash(QQ::ZERO));
    }
}
#[derive(Debug, PartialEq)]
struct Bundle {
    setup: Vec<u8>,
    cm: Vec<u8>,
    proof: Vec<u8>,
    challenge: Vec<u8>,
    point: Vec<u8>,
    y: Vec<u8>,
}
fn generate<F: SampleBase, E: ExtensionField<F>>(cfg: HyperFoldConfig) -> Bundle {
    let pcs = HyperFoldPcs::<F, E>::setup(cfg, [42; 32]).unwrap();
    let mut setup = Vec::new();
    for level in &pcs.diagonals.levels {
        setup.extend(bytes(&level.forward));
        setup.extend(bytes(&level.inverse));
    }
    let mut rng = stream(&[27; 32], b"message", cfg.mu as u64);
    let m = (0..cfg.k())
        .map(|_| F::sample(&mut rng))
        .collect::<Vec<_>>();
    let mut rng = stream(&[28; 32], b"point", cfg.mu as u64);
    let z = (0..cfg.mu)
        .map(|_| E::from_basis_coefficients_fn(|_| F::sample(&mut rng)))
        .collect::<Vec<_>>();
    let y = evaluate(&m, &z);
    let (cm, d) = pcs.commit(m).unwrap();
    let (o, w) = d.into_parts();
    let pcs = pcs.into_opening_only().unwrap();
    let (proof, mut pt, t) = pcs.open_once_profiled(&cm, &o, w, &z, y).unwrap();
    assert_eq!(pcs.diagonal_capacities().0, 0);
    assert_eq!(t.released_witness_elements, cfg.k());
    assert_eq!(t.witness_release_round, Some((cfg.depth(), 0)));
    let mut vt = pcs.verify(&cm, &z, y, &proof).unwrap();
    let challenge = pt.sample::<F, E>(b"post-adapter-check");
    assert_eq!(challenge, vt.sample::<F, E>(b"post-adapter-check"));
    Bundle {
        setup,
        cm: bytes(&cm),
        proof: bytes(&proof),
        challenge: bytes(&challenge),
        point: bytes(&z),
        y: bytes(&y),
    }
}
fn verify<F: SampleBase, E: ExtensionField<F>>(cfg: HyperFoldConfig, b: &Bundle) {
    let pcs = HyperFoldPcs::<F, E>::setup(cfg, [42; 32])
        .unwrap()
        .into_opening_only()
        .unwrap();
    let cm: Commitment<E> = codec::decode(&b.cm, b.cm.len()).unwrap();
    let mut proof: Proof<F, E> = codec::decode(&b.proof, b.proof.len()).unwrap();
    let z: Vec<E> = codec::decode(&b.point, b.point.len()).unwrap();
    let y: E = codec::decode(&b.y, b.y.len()).unwrap();
    let mut vt = pcs.verify(&cm, &z, y, &proof).unwrap();
    same(
        vt.sample::<F, E>(b"post-adapter-check"),
        codec::decode::<E>(&b.challenge, b.challenge.len()).unwrap(),
    );
    proof.final_table[0] += E::ONE;
    assert!(pcs.verify(&cm, &z, y, &proof).is_err());
    let mut cm = cm;
    cm.root[0] ^= 1;
    assert!(
        pcs.verify(
            &cm,
            &z,
            y,
            &codec::decode::<Proof<F, E>>(&b.proof, b.proof.len()).unwrap()
        )
        .is_err()
    );
}
#[test]
fn pcs_cross_backend_and_thread_agreement() {
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let parallel = rayon::ThreadPoolBuilder::new()
        .num_threads(32)
        .build()
        .unwrap();
    for code in [BaseCodeKind::Qa, BaseCodeKind::Rs] {
        for r in [2usize, 4, 8] {
            for c in [2, 4] {
                for depth in 1..=4 {
                    let cfg = HyperFoldConfig::testing(
                        BaseFieldKind::M127,
                        code,
                        1 + depth * r.ilog2() as usize,
                        1,
                        r,
                        c,
                        2,
                    )
                    .unwrap();
                    let reference = one.install(|| generate::<F, E>(cfg));
                    let a = parallel.install(|| generate::<A, AQ>(cfg));
                    let q = parallel.install(|| generate::<Q, QQ>(cfg));
                    assert_eq!(a, one.install(|| generate::<A, AQ>(cfg)));
                    assert_eq!(q, one.install(|| generate::<Q, QQ>(cfg)));
                    assert_eq!(reference, a);
                    assert_eq!(reference, q);
                    for b in [&reference, &a, &q] {
                        parallel.install(|| {
                            verify::<F, E>(cfg, b);
                            verify::<A, AQ>(cfg, b);
                            verify::<Q, QQ>(cfg, b);
                        });
                    }
                }
            }
        }
    }
}

fn ragged<F: SampleBase>() -> Vec<Vec<u8>> {
    use p3_commit::Mmcs;
    use p3_matrix::{Dimensions, dense::RowMajorMatrix};
    let m = hyperfold_pcs::mmcs::mmcs::<F>();
    let mut evidence = Vec::new();
    for heights in [[16, 4, 4, 8], [4, 4, 8, 16], [8, 16, 4, 4]] {
        let matrices = heights
            .iter()
            .enumerate()
            .map(|(j, &h)| {
                RowMajorMatrix::new(
                    (0..h * (j + 1)).map(|v| F::from_usize(v + 17)).collect(),
                    j + 1,
                )
            })
            .collect();
        let dims = heights
            .iter()
            .enumerate()
            .map(|(j, &h)| Dimensions {
                height: h,
                width: j + 1,
            })
            .collect::<Vec<_>>();
        let (root, tree) = m.commit(matrices);
        let indices = [1, 2, 7, 8, 15];
        let (values, paths) = m.open_multi_batch(&indices, &tree);
        m.verify_multi_batch(&root, &dims, &indices, &values, &paths)
            .unwrap();
        let mut bad = paths.clone();
        bad.sibling_hashes[0][0] ^= 1;
        assert!(
            m.verify_multi_batch(&root, &dims, &indices, &values, &bad)
                .is_err()
        );
        evidence.push(bytes(&paths));
        evidence.push(bytes(&values));
    }
    evidence
}
#[test]
fn ragged_multi_batch_canonical_proofs_match() {
    assert_eq!(ragged::<F>(), ragged::<A>());
    assert_eq!(ragged::<F>(), ragged::<Q>());
    assert_eq!(align_of::<A>(), 8);
    assert_eq!(align_of::<AQ>(), 8);
}
