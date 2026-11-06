//! Benchmark-only native field adapters. Protocol types and algorithms are unchanged.
use core::{
    fmt,
    hash::{Hash, Hasher},
    iter::{Product, Sum},
    marker::PhantomData,
    ops::*,
};
use hyperfold_pcs::{field_m127::P, params::BaseFieldKind, sampling::SampleBase};
use num_bigint::BigUint;
use p3_field::{integers::QuotientMap, *};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
pub mod ark;
pub mod fourq;
/// Static engine: native representation never escapes through canonical wire interfaces.
pub trait Engine: Copy + Clone + Default + fmt::Debug + Eq + Hash + Send + Sync + 'static {
    type Repr: Copy + Default + fmt::Debug + Send + Sync;
    const ZERO: Self::Repr;
    const ONE: Self::Repr;
    const TWO: Self::Repr;
    const NEG_ONE: Self::Repr;
    const GENERATOR: Self::Repr;
    const EIGHT: Self::Repr;
    fn from_canonical(x: u128) -> Self::Repr;
    fn canonical(x: Self::Repr) -> u128;
    fn equal(a: Self::Repr, b: Self::Repr) -> bool;
    fn add(a: Self::Repr, b: Self::Repr) -> Self::Repr;
    fn sub(a: Self::Repr, b: Self::Repr) -> Self::Repr;
    fn mul(a: Self::Repr, b: Self::Repr) -> Self::Repr;
    fn neg(a: Self::Repr) -> Self::Repr;
    fn square(a: Self::Repr) -> Self::Repr;
    fn inverse(a: Self::Repr) -> Self::Repr;
    fn halve(a: Self::Repr) -> Self::Repr;
    fn qadd(a: [Self::Repr; 2], b: [Self::Repr; 2]) -> [Self::Repr; 2];
    fn qsub(a: [Self::Repr; 2], b: [Self::Repr; 2]) -> [Self::Repr; 2];
    fn qneg(a: [Self::Repr; 2]) -> [Self::Repr; 2];
    fn qhalve(a: [Self::Repr; 2]) -> [Self::Repr; 2];
    fn qmul(a: [Self::Repr; 2], b: [Self::Repr; 2]) -> [Self::Repr; 2];
    fn qsquare(a: [Self::Repr; 2]) -> [Self::Repr; 2];
    fn qinverse(a: [Self::Repr; 2]) -> [Self::Repr; 2];
    fn qmixed(a: [Self::Repr; 2], b: Self::Repr) -> [Self::Repr; 2];
}
#[derive(Copy, Clone, Default, Debug)]
#[repr(transparent)]
pub struct Base<B: Engine>(B::Repr, PhantomData<B>);
impl<B: Engine> Base<B> {
    const fn wrap(x: B::Repr) -> Self {
        Self(x, PhantomData)
    }
    pub fn canonical(self) -> u128 {
        B::canonical(self.0)
    }
    pub fn new(x: u128) -> Self {
        let s = (x & P) + (x >> 127);
        Self::wrap(B::from_canonical(if s >= P { s - P } else { s }))
    }
}
impl<B: Engine> PartialEq for Base<B> {
    #[inline]
    fn eq(&self, b: &Self) -> bool {
        B::equal(self.0, b.0)
    }
}
impl<B: Engine> Eq for Base<B> {}
impl<B: Engine> Hash for Base<B> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.canonical().hash(h)
    }
}
impl<B: Engine> PartialOrd for Base<B> {
    fn partial_cmp(&self, b: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(b))
    }
}
impl<B: Engine> Ord for Base<B> {
    fn cmp(&self, b: &Self) -> core::cmp::Ordering {
        self.canonical().cmp(&b.canonical())
    }
}
impl<B: Engine> fmt::Display for Base<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical())
    }
}
impl<B: Engine> Serialize for Base<B> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.canonical().to_le_bytes().serialize(s)
    }
}
impl<'de, B: Engine> Deserialize<'de> for Base<B> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = u128::from_le_bytes(<[u8; 16]>::deserialize(d)?);
        if n >= P {
            return Err(serde::de::Error::custom("noncanonical M127"));
        }
        Ok(Self::wrap(B::from_canonical(n)))
    }
}
impl<B: Engine> Add for Base<B> {
    type Output = Self;
    #[inline]
    fn add(self, b: Self) -> Self {
        Self::wrap(B::add(self.0, b.0))
    }
}
impl<B: Engine> Sub for Base<B> {
    type Output = Self;
    #[inline]
    fn sub(self, b: Self) -> Self {
        Self::wrap(B::sub(self.0, b.0))
    }
}
impl<B: Engine> Mul for Base<B> {
    type Output = Self;
    #[inline]
    fn mul(self, b: Self) -> Self {
        Self::wrap(B::mul(self.0, b.0))
    }
}
impl<B: Engine> Neg for Base<B> {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self::wrap(B::neg(self.0))
    }
}
#[allow(clippy::suspicious_arithmetic_impl)]
impl<B: Engine> Div for Base<B> {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        self * b.inverse()
    }
}
impl<B: Engine> PrimeCharacteristicRing for Base<B> {
    type PrimeSubfield = Self;
    const ZERO: Self = Self::wrap(B::ZERO);
    const ONE: Self = Self::wrap(B::ONE);
    const TWO: Self = Self::wrap(B::TWO);
    const NEG_ONE: Self = Self::wrap(B::NEG_ONE);
    fn from_prime_subfield(f: Self) -> Self {
        f
    }
    #[inline]
    fn square(&self) -> Self {
        Self::wrap(B::square(self.0))
    }
    #[inline]
    fn halve(&self) -> Self {
        Self::wrap(B::halve(self.0))
    }
}
impl<B: Engine> Packable for Base<B> {}
impl<B: Engine> RawDataSerializable for Base<B> {
    const NUM_BYTES: usize = 16;
    fn into_bytes(self) -> impl IntoIterator<Item = u8> {
        self.canonical().to_le_bytes()
    }
}
impl<B: Engine> Field for Base<B> {
    type Packing = Self;
    const GENERATOR: Self = Self::wrap(B::GENERATOR);
    fn try_inverse(&self) -> Option<Self> {
        (!self.is_zero()).then(|| Self::wrap(B::inverse(self.0)))
    }
    fn order() -> BigUint {
        BigUint::from(P)
    }
}
impl<B: Engine> PrimeField for Base<B> {
    fn as_canonical_biguint(&self) -> BigUint {
        BigUint::from(self.canonical())
    }
}
impl<B: Engine> SampleBase for Base<B> {
    const KIND: BaseFieldKind = BaseFieldKind::M127;
    fn sample(r: &mut blake3::OutputReader) -> Self {
        loop {
            let mut b = [0; 16];
            r.fill(&mut b);
            let x = u128::from_le_bytes(b) & P;
            if x < P {
                return Self::wrap(B::from_canonical(x));
            }
        }
    }
}
macro_rules! unsigned {($($t:ty),*)=>{$(impl<B:Engine> QuotientMap<$t> for Base<B>{
 fn from_int(x:$t)->Self{Self::new(x as u128)}
 fn from_canonical_checked(x:$t)->Option<Self>{((x as u128)<P).then(||Self::wrap(B::from_canonical(x as u128)))}
 unsafe fn from_canonical_unchecked(x:$t)->Self{Self::wrap(B::from_canonical(x as u128))}
})*};}
unsigned!(u8, u16, u32, u64, u128);
macro_rules! signed {($($t:ty),*)=>{$(impl<B:Engine> QuotientMap<$t> for Base<B>{
 fn from_int(x:$t)->Self{let x=x as i128;if x>=0{Self::new(x as u128)}else{-Self::new(x.unsigned_abs())}}
 fn from_canonical_checked(x:$t)->Option<Self>{let x=x as i128;(x.unsigned_abs()<=P/2).then(||Self::from_int(x))}
 unsafe fn from_canonical_unchecked(x:$t)->Self{Self::from_int(x)}
})*};}
signed!(i8, i16, i32, i64, i128);
/// The basis coefficients are owned here; borrowing them does not assume ark Fp2 layout.
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Quad<B: Engine>(pub [Base<B>; 2]);
impl<B: Engine> Quad<B> {
    fn repr(self) -> [B::Repr; 2] {
        self.0.map(|v| v.0)
    }
    fn wrap(a: [B::Repr; 2]) -> Self {
        Self(a.map(Base::wrap))
    }
}
impl<B: Engine> Serialize for Quad<B> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de, B: Engine> Deserialize<'de> for Quad<B> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(<[Base<B>; 2]>::deserialize(d)?))
    }
}
impl<B: Engine> fmt::Display for Quad<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} + {}i", self.0[0], self.0[1])
    }
}
impl<B: Engine> Add for Quad<B> {
    type Output = Self;
    #[inline]
    fn add(self, b: Self) -> Self {
        Self::wrap(B::qadd(self.repr(), b.repr()))
    }
}
impl<B: Engine> Sub for Quad<B> {
    type Output = Self;
    #[inline]
    fn sub(self, b: Self) -> Self {
        Self::wrap(B::qsub(self.repr(), b.repr()))
    }
}
impl<B: Engine> Neg for Quad<B> {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self::wrap(B::qneg(self.repr()))
    }
}
impl<B: Engine> Mul for Quad<B> {
    type Output = Self;
    #[inline]
    fn mul(self, b: Self) -> Self {
        Self::wrap(B::qmul(self.repr(), b.repr()))
    }
}
#[allow(clippy::suspicious_arithmetic_impl)]
impl<B: Engine> Div for Quad<B> {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        self * b.inverse()
    }
}
impl<B: Engine> From<Base<B>> for Quad<B> {
    fn from(a: Base<B>) -> Self {
        Self([a, Base::ZERO])
    }
}
impl<B: Engine> Add<Base<B>> for Quad<B> {
    type Output = Self;
    #[inline]
    fn add(self, b: Base<B>) -> Self {
        Self([self.0[0] + b, self.0[1]])
    }
}
impl<B: Engine> Sub<Base<B>> for Quad<B> {
    type Output = Self;
    #[inline]
    fn sub(self, b: Base<B>) -> Self {
        Self([self.0[0] - b, self.0[1]])
    }
}
impl<B: Engine> Mul<Base<B>> for Quad<B> {
    type Output = Self;
    #[inline]
    fn mul(self, b: Base<B>) -> Self {
        Self::wrap(B::qmixed(self.repr(), b.0))
    }
}
impl<B: Engine> Algebra<Base<B>> for Quad<B> {}
impl<B: Engine> PrimeCharacteristicRing for Quad<B> {
    type PrimeSubfield = Base<B>;
    const ZERO: Self = Self([Base::ZERO, Base::ZERO]);
    const ONE: Self = Self([Base::ONE, Base::ZERO]);
    const TWO: Self = Self([Base::TWO, Base::ZERO]);
    const NEG_ONE: Self = Self([Base::NEG_ONE, Base::ZERO]);
    fn from_prime_subfield(f: Base<B>) -> Self {
        f.into()
    }
    #[inline]
    fn square(&self) -> Self {
        Self::wrap(B::qsquare(self.repr()))
    }
    #[inline]
    fn halve(&self) -> Self {
        Self::wrap(B::qhalve(self.repr()))
    }
}
impl<B: Engine> Packable for Quad<B> {}
impl<B: Engine> RawDataSerializable for Quad<B> {
    const NUM_BYTES: usize = 32;
    fn into_bytes(self) -> impl IntoIterator<Item = u8> {
        self.0.into_iter().flat_map(|x| x.canonical().to_le_bytes())
    }
}
impl<B: Engine> Field for Quad<B> {
    type Packing = Self;
    const GENERATOR: Self = Self([Base::wrap(B::EIGHT), Base::ONE]);
    fn try_inverse(&self) -> Option<Self> {
        (!self.is_zero()).then(|| Self::wrap(B::qinverse(self.repr())))
    }
    fn order() -> BigUint {
        BigUint::from(P) * BigUint::from(P)
    }
}
impl<B: Engine> BasedVectorSpace<Base<B>> for Quad<B> {
    const DIMENSION: usize = 2;
    fn as_basis_coefficients_slice(&self) -> &[Base<B>] {
        &self.0
    }
    fn from_basis_coefficients_fn<Fn: FnMut(usize) -> Base<B>>(f: Fn) -> Self {
        Self(core::array::from_fn(f))
    }
    fn from_basis_coefficients_iter<I: ExactSizeIterator<Item = Base<B>>>(
        mut i: I,
    ) -> Option<Self> {
        if i.len() != 2 {
            return None;
        }
        Some(Self([i.next()?, i.next()?]))
    }
}
impl<B: Engine> AlgebraIdentity<Base<B>> for Quad<B> {
    fn algebra_id() -> Vec<u8> {
        b"hyperfold-m127-power-basis-v1:X^2+1".to_vec()
    }
}
impl<B: Engine> ExtensionField<Base<B>> for Quad<B> {
    type ExtensionPacking = Self;
    fn is_in_basefield(&self) -> bool {
        self.0[1].is_zero()
    }
    fn as_base(&self) -> Option<Base<B>> {
        self.0[1].is_zero().then_some(self.0[0])
    }
}
impl<B: Engine> PackedFieldExtension<Base<B>, Self> for Quad<B> {
    fn packed_ext_powers(base: Self) -> Powers<Self> {
        base.powers()
    }
    fn from_ext_fn(f: impl Fn(usize) -> Self) -> Self {
        f(0)
    }
    fn extract(&self, lane: usize) -> Self {
        assert_eq!(lane, 0);
        *self
    }
    fn add_assign_lane(&mut self, lane: usize, value: Self) {
        assert_eq!(lane, 0);
        *self += value
    }
}
impl<B: Engine> AddAssign for Base<B> {
    #[inline]
    fn add_assign(&mut self, b: Self) {
        *self = *self + b
    }
}
impl<B: Engine> SubAssign for Base<B> {
    #[inline]
    fn sub_assign(&mut self, b: Self) {
        *self = *self - b
    }
}
impl<B: Engine> MulAssign for Base<B> {
    #[inline]
    fn mul_assign(&mut self, b: Self) {
        *self = *self * b
    }
}
impl<B: Engine> DivAssign for Base<B> {
    #[inline]
    fn div_assign(&mut self, b: Self) {
        *self = *self / b
    }
}
impl<B: Engine> Sum for Base<B> {
    fn sum<I: Iterator<Item = Self>>(i: I) -> Self {
        i.fold(Self::ZERO, |a, b| a + b)
    }
}
impl<B: Engine> Product for Base<B> {
    fn product<I: Iterator<Item = Self>>(i: I) -> Self {
        i.fold(Self::ONE, |a, b| a * b)
    }
}
impl<B: Engine> AddAssign for Quad<B> {
    #[inline]
    fn add_assign(&mut self, b: Self) {
        *self = *self + b
    }
}
impl<B: Engine> SubAssign for Quad<B> {
    #[inline]
    fn sub_assign(&mut self, b: Self) {
        *self = *self - b
    }
}
impl<B: Engine> MulAssign for Quad<B> {
    #[inline]
    fn mul_assign(&mut self, b: Self) {
        *self = *self * b
    }
}
impl<B: Engine> DivAssign for Quad<B> {
    #[inline]
    fn div_assign(&mut self, b: Self) {
        *self = *self / b
    }
}
impl<B: Engine> Sum for Quad<B> {
    fn sum<I: Iterator<Item = Self>>(i: I) -> Self {
        i.fold(Self::ZERO, |a, b| a + b)
    }
}
impl<B: Engine> Product for Quad<B> {
    fn product<I: Iterator<Item = Self>>(i: I) -> Self {
        i.fold(Self::ONE, |a, b| a * b)
    }
}
impl<B: Engine> AddAssign<Base<B>> for Quad<B> {
    #[inline]
    fn add_assign(&mut self, b: Base<B>) {
        *self = *self + b
    }
}
impl<B: Engine> SubAssign<Base<B>> for Quad<B> {
    #[inline]
    fn sub_assign(&mut self, b: Base<B>) {
        *self = *self - b
    }
}
impl<B: Engine> MulAssign<Base<B>> for Quad<B> {
    #[inline]
    fn mul_assign(&mut self, b: Base<B>) {
        *self = *self * b
    }
}

#[cfg(test)]
mod tests;
