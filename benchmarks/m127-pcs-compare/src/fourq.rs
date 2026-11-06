//! FourQ native redundant [0,p] values, strict canonical values only on the wire.
//! Pointers refer only to explicit C-layout limb arrays; no ark or u128 layout casts.
#[cfg(test)]
use super::Base;
use super::Engine;
use hyperfold_pcs::field_m127::P;
#[derive(Copy, Clone, Default, Debug)]
#[repr(C, align(16))]
pub struct Limbs(pub [u64; 2]);
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq, Hash)]
pub struct FourQ;
unsafe extern "C" {
    fn hf_fq_add(a: *const Limbs, b: *const Limbs, c: *mut Limbs);
    fn hf_fq_sub(a: *const Limbs, b: *const Limbs, c: *mut Limbs);
    fn hf_fq_mul(a: *const Limbs, b: *const Limbs, c: *mut Limbs);
    fn hf_fq_square(a: *const Limbs, c: *mut Limbs);
    fn hf_fq_neg(a: *const Limbs, c: *mut Limbs);
    fn hf_fq_inverse(a: *const Limbs, c: *mut Limbs);
    fn hf_fq_halve(a: *const Limbs, c: *mut Limbs);
    fn hf_fq_qmul(a: *const [Limbs; 2], b: *const [Limbs; 2], c: *mut [Limbs; 2]);
    fn hf_fq_qsquare(a: *const [Limbs; 2], c: *mut [Limbs; 2]);
    fn hf_fq_qinverse(a: *const [Limbs; 2], c: *mut [Limbs; 2]);
    fn hf_fq_qmixed(a: *const [Limbs; 2], b: *const Limbs, c: *mut [Limbs; 2]);
}
impl Engine for FourQ {
    type Repr = Limbs;
    const ZERO: Limbs = Limbs([0, 0]);
    const ONE: Limbs = Limbs([1, 0]);
    const TWO: Limbs = Limbs([2, 0]);
    const NEG_ONE: Limbs = Limbs([u64::MAX - 1, u64::MAX >> 1]);
    const GENERATOR: Limbs = Limbs([43, 0]);
    const EIGHT: Limbs = Limbs([8, 0]);
    #[inline]
    fn from_canonical(x: u128) -> Limbs {
        Limbs([x as u64, (x >> 64) as u64])
    }
    #[inline]
    fn canonical(x: Limbs) -> u128 {
        let n = (x.0[0] as u128) | ((x.0[1] as u128) << 64);
        if n == P { 0 } else { n }
    }
    #[inline]
    fn equal(a: Limbs, b: Limbs) -> bool {
        a.0 == b.0 || (Self::canonical(a) == 0 && Self::canonical(b) == 0)
    }
    #[inline]
    fn add(a: Limbs, b: Limbs) -> Limbs {
        let mut c = Self::ZERO;
        // SAFETY: all pointers reference live fixed-size initialized arrays; output is disjoint.
        unsafe { hf_fq_add(&a, &b, &mut c) };
        c
    }
    #[inline]
    fn sub(a: Limbs, b: Limbs) -> Limbs {
        let mut c = Self::ZERO;
        // SAFETY: all pointers reference live fixed-size initialized arrays; output is disjoint.
        unsafe { hf_fq_sub(&a, &b, &mut c) };
        c
    }
    #[inline]
    fn mul(a: Limbs, b: Limbs) -> Limbs {
        let mut c = Self::ZERO;
        // SAFETY: all pointers reference live fixed-size initialized arrays; output is disjoint.
        unsafe { hf_fq_mul(&a, &b, &mut c) };
        c
    }
    #[inline]
    fn square(a: Limbs) -> Limbs {
        let mut c = Self::ZERO;
        // SAFETY: live 16-byte input/output, disjoint; nonzero inverse enforced by Field.
        unsafe { hf_fq_square(&a, &mut c) };
        c
    }
    #[inline]
    fn neg(a: Limbs) -> Limbs {
        let mut c = Self::ZERO;
        // SAFETY: live 16-byte input/output, disjoint; nonzero inverse enforced by Field.
        unsafe { hf_fq_neg(&a, &mut c) };
        c
    }
    #[inline]
    fn inverse(a: Limbs) -> Limbs {
        let mut c = Self::ZERO;
        // SAFETY: live 16-byte input/output, disjoint; nonzero inverse enforced by Field.
        unsafe { hf_fq_inverse(&a, &mut c) };
        c
    }
    #[inline]
    fn halve(a: Limbs) -> Limbs {
        let mut c = Self::ZERO;
        // SAFETY: live 16-byte input/output, disjoint; nonzero inverse enforced by Field.
        unsafe { hf_fq_halve(&a, &mut c) };
        c
    }
    #[inline]
    fn qmul(a: [Limbs; 2], b: [Limbs; 2]) -> [Limbs; 2] {
        let mut c = [Self::ZERO; 2];
        // SAFETY: repr(C) Limbs arrays have the C 2x2 u64 layout; all arrays disjoint.
        unsafe { hf_fq_qmul(&a, &b, &mut c) };
        c
    }
    #[inline]
    fn qsquare(a: [Limbs; 2]) -> [Limbs; 2] {
        let mut c = [Self::ZERO; 2];
        // SAFETY: live fixed 32-byte arrays; disjoint output, nonzero inverse checked above.
        unsafe { hf_fq_qsquare(&a, &mut c) };
        c
    }
    #[inline]
    fn qinverse(a: [Limbs; 2]) -> [Limbs; 2] {
        let mut c = [Self::ZERO; 2];
        // SAFETY: live fixed 32-byte arrays; disjoint output, nonzero inverse checked above.
        unsafe { hf_fq_qinverse(&a, &mut c) };
        c
    }
    #[inline]
    fn qmixed(a: [Limbs; 2], b: Limbs) -> [Limbs; 2] {
        let mut c = [Self::ZERO; 2];
        // SAFETY: fixed explicit limb layout; live 32/16/32-byte arrays, output disjoint.
        unsafe { hf_fq_qmixed(&a, &b, &mut c) };
        c
    }
}
#[cfg(test)]
pub(crate) fn redundant_zero() -> Base<FourQ> {
    Base::wrap(Limbs([u64::MAX, u64::MAX >> 1]))
}
