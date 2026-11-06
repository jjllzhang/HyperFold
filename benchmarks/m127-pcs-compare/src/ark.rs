//! Same p and X²+1, all hot arithmetic in ark-ff 0.6.0.
use super::Engine;
use ark_ff::{BigInt, Field, Fp2, Fp2Config, Fp128, MontBackend, MontConfig, MontFp, PrimeField};
#[derive(MontConfig)]
#[modulus = "170141183460469231731687303715884105727"]
#[generator = "43"]
pub struct Config;
pub type AF = Fp128<MontBackend<Config, 2>>;
pub struct QConfig;
impl Fp2Config for QConfig {
    type Fp = AF;
    const NONRESIDUE: AF = MontFp!("-1");
    const FROBENIUS_COEFF_FP2_C1: &[AF] = &[MontFp!("1"), MontFp!("-1")];
    fn mul_fp_by_nonresidue_in_place(x: &mut AF) -> &mut AF {
        *x = -*x;
        x
    }
    fn mul_fp_by_nonresidue_and_add(y: &mut AF, x: &AF) {
        *y = *x - *y
    }
    fn mul_fp_by_nonresidue_plus_one_and_add(y: &mut AF, x: &AF) {
        *y = *x
    }
    fn sub_and_mul_fp_by_nonresidue(y: &mut AF, x: &AF) {
        *y += x
    }
}
type AE = Fp2<QConfig>;
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq, Hash)]
pub struct Ark;
impl Engine for Ark {
    type Repr = AF;
    const ZERO: AF = MontFp!("0");
    const ONE: AF = MontFp!("1");
    const TWO: AF = MontFp!("2");
    const NEG_ONE: AF = MontFp!("-1");
    const GENERATOR: AF = MontFp!("43");
    const EIGHT: AF = MontFp!("8");
    #[inline]
    fn from_canonical(x: u128) -> AF {
        AF::from_bigint(BigInt([x as u64, (x >> 64) as u64])).expect("canonical input")
    }
    #[inline]
    fn canonical(x: AF) -> u128 {
        let a = x.into_bigint().0;
        (a[0] as u128) | ((a[1] as u128) << 64)
    }
    #[inline]
    fn equal(a: AF, b: AF) -> bool {
        a == b
    }
    #[inline]
    fn add(a: AF, b: AF) -> AF {
        a + b
    }
    #[inline]
    fn sub(a: AF, b: AF) -> AF {
        a - b
    }
    #[inline]
    fn mul(a: AF, b: AF) -> AF {
        a * b
    }
    #[inline]
    fn neg(a: AF) -> AF {
        -a
    }
    #[inline]
    fn square(a: AF) -> AF {
        a.square()
    }
    #[inline]
    fn inverse(a: AF) -> AF {
        a.inverse().expect("nonzero")
    }
    #[inline]
    fn halve(a: AF) -> AF {
        a * MontFp!("85070591730234615865843651857942052864")
    }
    #[inline]
    fn qmul(a: [AF; 2], b: [AF; 2]) -> [AF; 2] {
        let c = AE::new(a[0], a[1]) * AE::new(b[0], b[1]);
        [c.c0, c.c1]
    }
    #[inline]
    fn qsquare(a: [AF; 2]) -> [AF; 2] {
        let c = AE::new(a[0], a[1]).square();
        [c.c0, c.c1]
    }
    #[inline]
    fn qinverse(a: [AF; 2]) -> [AF; 2] {
        let c = AE::new(a[0], a[1]).inverse().expect("nonzero");
        [c.c0, c.c1]
    }
    #[inline]
    fn qmixed(a: [AF; 2], b: AF) -> [AF; 2] {
        let mut c = AE::new(a[0], a[1]);
        c.mul_assign_by_fp(&b);
        [c.c0, c.c1]
    }
}
