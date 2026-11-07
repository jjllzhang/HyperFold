//! Canonical scalar M127 arithmetic, with Plonky3's quadratic power basis X²+1.
//! This implementation is not claimed constant-time. No SIMD representation is assumed.
use core::{
    fmt,
    iter::{Product, Sum},
    ops::*,
};
use num_bigint::BigUint;
use p3_field::extension::{
    Binomial, BinomialExtensionField, BinomiallyExtendable, ExtensionAlgebra,
};
use p3_field::integers::QuotientMap;
use p3_field::{Field, Packable, PrimeCharacteristicRing, PrimeField, RawDataSerializable};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const P: u128 = (1u128 << 127) - 1;
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct Mersenne127(u128);
pub type M127Quad = BinomialExtensionField<Mersenne127, 2>;
impl Mersenne127 {
    pub const fn new(x: u128) -> Self {
        let s = (x & P) + (x >> 127);
        Self(if s >= P { s - P } else { s })
    }
    pub const fn canonical(self) -> u128 {
        self.0
    }
    pub fn pow_u128(mut self, mut e: u128) -> Self {
        let mut y = Self::ONE;
        while e != 0 {
            if e & 1 != 0 {
                y *= self;
            }
            self *= self;
            e >>= 1;
        }
        y
    }
}
impl fmt::Display for Mersenne127 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Serialize for Mersenne127 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.to_le_bytes().serialize(s)
    }
}
impl<'de> Deserialize<'de> for Mersenne127 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let x = u128::from_le_bytes(<[u8; 16]>::deserialize(d)?);
        if x >= P {
            return Err(serde::de::Error::custom("noncanonical M127"));
        }
        Ok(Self(x))
    }
}
impl Add for Mersenne127 {
    type Output = Self;
    #[inline]
    fn add(self, b: Self) -> Self {
        Self::new(self.0 + b.0)
    }
}
impl Sub for Mersenne127 {
    type Output = Self;
    #[inline]
    fn sub(self, b: Self) -> Self {
        if self.0 >= b.0 {
            Self(self.0 - b.0)
        } else {
            Self(P - (b.0 - self.0))
        }
    }
}
impl Neg for Mersenne127 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        if self.0 == 0 { self } else { Self(P - self.0) }
    }
}
impl Mul for Mersenne127 {
    type Output = Self;
    #[inline]
    fn mul(self, b: Self) -> Self {
        // Exact 128×128 ->256 via four 64-bit products, with explicit carries.
        let mask = u64::MAX as u128;
        let (a0, a1) = (self.0 & mask, self.0 >> 64);
        let (b0, b1) = (b.0 & mask, b.0 >> 64);
        let p00 = a0 * b0;
        let p01 = a0 * b1;
        let p10 = a1 * b0;
        let p11 = a1 * b1;
        let mid = (p00 >> 64) + (p01 & mask) + (p10 & mask);
        let lo = (p00 & mask) | ((mid & mask) << 64);
        let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
        // hi < 2^126, so the following 127-bit fold cannot overflow u128.
        Self::new((lo & P) + ((hi << 1) | (lo >> 127)))
    }
}
#[allow(clippy::suspicious_arithmetic_impl)]
impl Div for Mersenne127 {
    type Output = Self;
    fn div(self, b: Self) -> Self {
        self * b.inverse()
    }
}
macro_rules! assign {($t:ident,$f:ident,$op:tt)=>{impl $t for Mersenne127 {#[inline] fn $f(&mut self,b:Self){*self=*self $op b;}}};}
assign!(AddAssign,add_assign,+);
assign!(SubAssign,sub_assign,-);
assign!(MulAssign,mul_assign,*);
assign!(DivAssign,div_assign,/);
impl Sum for Mersenne127 {
    fn sum<I: Iterator<Item = Self>>(i: I) -> Self {
        i.fold(Self::ZERO, |a, b| a + b)
    }
}
impl Product for Mersenne127 {
    fn product<I: Iterator<Item = Self>>(i: I) -> Self {
        i.fold(Self::ONE, |a, b| a * b)
    }
}
impl PrimeCharacteristicRing for Mersenne127 {
    type PrimeSubfield = Self;
    const ZERO: Self = Self(0);
    const ONE: Self = Self(1);
    const TWO: Self = Self(2);
    const NEG_ONE: Self = Self(P - 1);
    fn from_prime_subfield(f: Self) -> Self {
        f
    }
    #[inline]
    fn square(&self) -> Self {
        // Three wide products; all intermediates fit u128 for canonical a < p.
        // This is a separate reduction from Mul: do not remove Mul's Self::new.
        let a0 = self.0 & u64::MAX as u128;
        let a1 = self.0 >> 64;
        let t0 = a0 * a0;
        let t1 = a0 * (2 * a1) + (t0 >> 64);
        let u = a1 * (2 * a1) + (t1 >> 63);
        let v = (t0 & u64::MAX as u128) | ((t1 & ((1u128 << 63) - 1)) << 64);
        let s = u + v;
        Self((s & P) + (s >> 127))
    }
    #[inline]
    fn halve(&self) -> Self {
        Self((self.0 >> 1) | ((self.0 & 1) << 126))
    }
}
impl Packable for Mersenne127 {}
impl RawDataSerializable for Mersenne127 {
    const NUM_BYTES: usize = 16;
    fn into_bytes(self) -> impl IntoIterator<Item = u8> {
        self.0.to_le_bytes()
    }
}
impl Field for Mersenne127 {
    type Packing = Self;
    // Sage GF(2^127-1).multiplicative_generator(), order checked in tests.
    const GENERATOR: Self = Self(43);
    fn try_inverse(&self) -> Option<Self> {
        if self.0 == 0 {
            None
        } else {
            // x_j = a^(2^j - 1); 126 squarings and 12 multiplies in total.
            let x2 = self.square() * *self;
            let x4 = x2.exp_power_of_2(2) * x2;
            let x8 = x4.exp_power_of_2(4) * x4;
            let x16 = x8.exp_power_of_2(8) * x8;
            let x32 = x16.exp_power_of_2(16) * x16;
            let x64 = x32.exp_power_of_2(32) * x32;
            let x96 = x64.exp_power_of_2(32) * x32;
            let x112 = x96.exp_power_of_2(16) * x16;
            let x120 = x112.exp_power_of_2(8) * x8;
            let x124 = x120.exp_power_of_2(4) * x4;
            let x125 = x124.square() * *self;
            Some(x125.exp_power_of_2(2) * *self)
        }
    }
    fn order() -> BigUint {
        BigUint::from(P)
    }
}
macro_rules! unsigned {($($t:ty),*)=>{$(impl QuotientMap<$t> for Mersenne127 {
    fn from_int(x:$t)->Self{Self::new(x as u128)}
    fn from_canonical_checked(x:$t)->Option<Self>{((x as u128)<P).then_some(Self(x as u128))}
    unsafe fn from_canonical_unchecked(x:$t)->Self{Self(x as u128)}
})*};}
unsigned!(u8, u16, u32, u64, u128);
macro_rules! signed {($($t:ty),*)=>{$(impl QuotientMap<$t> for Mersenne127 {
    fn from_int(x:$t)->Self{let x=x as i128;if x>=0{Self::new(x as u128)}else{-Self::new(x.unsigned_abs())}}
    fn from_canonical_checked(x:$t)->Option<Self>{let x=x as i128;(x.unsigned_abs()<=P/2).then(||Self::from_int(x))}
    unsafe fn from_canonical_unchecked(x:$t)->Self{Self::from_int(x)}
})*};}
signed!(i8, i16, i32, i64, i128);
impl PrimeField for Mersenne127 {
    fn as_canonical_biguint(&self) -> BigUint {
        BigUint::from(self.0)
    }
}
impl ExtensionAlgebra<Self, 2, Binomial<Self>> for Mersenne127 {
    #[inline]
    fn ext_square(a: &[Self; 2], out: &mut [Self; 2]) {
        out[0] = (a[0] + a[1]) * (a[0] - a[1]);
        out[1] = (a[0] * a[1]).double();
    }
    fn ext_mul(a: &[Self; 2], b: &[Self; 2], out: &mut [Self; 2]) {
        let ac = a[0] * b[0];
        let bd = a[1] * b[1];
        out[0] = ac - bd;
        out[1] = (a[0] + a[1]) * (b[0] + b[1]) - ac - bd;
    }
}
impl BinomiallyExtendable<2> for Mersenne127 {
    fn binomial_algebra_id() -> Vec<u8> {
        b"hyperfold-m127-power-basis-v1:X^2+1".to_vec()
    }
    const W: Self = Self::NEG_ONE;
    const DTH_ROOT: Self = Self::NEG_ONE;
    // Sage GF(p^2,modulus=X^2+1).multiplicative_generator() = 8+i.
    const EXT_GENERATOR: [Self; 2] = [Self(8), Self(1)];
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::BasedVectorSpace;
    fn next(s: &mut u128) -> u128 {
        *s = s
            .wrapping_mul(0xda942042e4dd58b5)
            .wrapping_add(0x9e3779b97f4a7c15);
        *s
    }
    #[test]
    fn arithmetic_matches_biguint() {
        let p = BigUint::from(P);
        let mut s = 123;
        let mut xs = vec![0, 1, 2, P - 1, P - 2, 1 << 64, (1 << 126) + 1, u128::MAX];
        for _ in 0..1024 {
            xs.push(next(&mut s));
        }
        for &a in &xs {
            let b = next(&mut s);
            let (x, y) = (Mersenne127::new(a), Mersenne127::new(b));
            let (aa, bb) = (BigUint::from(a) % &p, BigUint::from(b) % &p);
            assert_eq!((x * y).as_canonical_biguint(), (&aa * &bb) % &p);
            assert_eq!((x + y).as_canonical_biguint(), (&aa + &bb) % &p);
            assert_eq!((x - y).as_canonical_biguint(), (&aa + &p - &bb) % &p);
            assert_eq!(x.halve().double(), x);
            if !x.is_zero() {
                assert_eq!(x * x.inverse(), Mersenne127::ONE);
            }
        }
    }
    #[test]
    fn specialized_squares_and_inverse_match_independent_reference() {
        let p = BigUint::from(P);
        let mut seed = 0x123456789abcdefu128;
        let mut inputs = vec![
            0,
            1,
            2,
            P - 1,
            P - 2,
            (1 << 63) - 1,
            1 << 63,
            (1 << 64) - 1,
            1 << 64,
            (1 << 126) - 1,
            1 << 126,
            u128::MAX,
        ];
        inputs.extend((0..10_000).map(|_| next(&mut seed)));
        for (index, &input) in inputs.iter().enumerate() {
            let a = Mersenne127::new(input);
            let b = Mersenne127::new(next(&mut seed));
            let aa = BigUint::from(a.canonical());
            let bb = BigUint::from(b.canonical());
            assert_eq!(a.square().as_canonical_biguint(), (&aa * &aa) % &p);
            assert_eq!(a.square(), a * a);
            assert!(a.square().canonical() < P);
            let z = M127Quad::new([a, b]);
            let squared = z.square();
            assert_eq!(squared, z * z);
            let coefficients: &[Mersenne127] = squared.as_basis_coefficients_slice();
            assert_eq!(
                coefficients[0].as_canonical_biguint(),
                ((&aa * &aa) + &p - ((&bb * &bb) % &p)) % &p
            );
            assert_eq!(
                coefficients[1].as_canonical_biguint(),
                (BigUint::from(2u8) * &aa * &bb) % &p
            );
            assert!(coefficients.iter().all(|x| x.canonical() < P));
            if a.is_zero() {
                assert!(a.try_inverse().is_none());
            } else {
                let inverse = a.inverse();
                assert_eq!(a * inverse, Mersenne127::ONE);
                assert!(inverse.canonical() < P);
                if index < 128 {
                    assert_eq!(
                        inverse.as_canonical_biguint(),
                        aa.modpow(&BigUint::from(P - 2), &p)
                    );
                    assert_eq!(inverse, a.pow_u128(P - 2));
                }
            }
        }
        // Original Mul still needs its final normalization for these canonical inputs.
        let minus_one = Mersenne127::new(P - 1);
        assert_eq!(minus_one * minus_one, Mersenne127::ONE);
        assert_eq!(minus_one.square(), Mersenne127::ONE);
        for a in [Mersenne127::ZERO, Mersenne127::ONE, minus_one] {
            for z in [
                M127Quad::new([a, a]),
                M127Quad::new([a, Mersenne127::ZERO]),
                M127Quad::new([Mersenne127::ZERO, a]),
            ] {
                assert_eq!(z.square(), z * z);
            }
        }
    }
    #[test]
    fn inverse_chain_exponents_and_operation_count() {
        // Mirror each straight-line instruction in try_inverse using integer exponents.
        let mut squares = 0;
        let mut multiplies = 0;
        let mut step = |x: &BigUint, shift: usize, y: &BigUint| {
            squares += shift;
            multiplies += 1;
            (x << shift) + y
        };
        let a = BigUint::from(1u8);
        let x2 = step(&a, 1, &a);
        let x4 = step(&x2, 2, &x2);
        let x8 = step(&x4, 4, &x4);
        let x16 = step(&x8, 8, &x8);
        let x32 = step(&x16, 16, &x16);
        let x64 = step(&x32, 32, &x32);
        let x96 = step(&x64, 32, &x32);
        let x112 = step(&x96, 16, &x16);
        let x120 = step(&x112, 8, &x8);
        let x124 = step(&x120, 4, &x4);
        let x125 = step(&x124, 1, &a);
        let exponent = step(&x125, 2, &a);
        assert_eq!(exponent, BigUint::from(P - 2));
        assert_eq!((squares, multiplies), (126, 12));
    }
    #[test]
    fn canonical_codec() {
        for n in [0, 1, P - 1] {
            let x = Mersenne127::new(n);
            let b = postcard::to_allocvec(&x).unwrap();
            assert_eq!(b, n.to_le_bytes());
            assert_eq!(postcard::from_bytes::<Mersenne127>(&b).unwrap(), x);
        }
        assert!(postcard::from_bytes::<Mersenne127>(&P.to_le_bytes()).is_err());
        assert!(postcard::from_bytes::<Mersenne127>(&u128::MAX.to_le_bytes()).is_err());
    }
    #[test]
    fn extension_inverse_frobenius_and_generators() {
        let factors = [
            2u128,
            3,
            7,
            19,
            43,
            73,
            127,
            337,
            5419,
            92737,
            649657,
            77158673929,
        ];
        for q in factors {
            assert_ne!(
                Mersenne127::GENERATOR.pow_u128((P - 1) / q),
                Mersenne127::ONE
            );
        }
        let generator = M127Quad::GENERATOR;
        let order = BigUint::from(P) * BigUint::from(P) - BigUint::from(1u8);
        fn pow(mut x: M127Quad, n: BigUint) -> M127Quad {
            let mut a = M127Quad::ONE;
            for b in n.to_bytes_le() {
                for j in 0..8 {
                    if b >> j & 1 == 1 {
                        a *= x;
                    }
                    x = x.square();
                }
            }
            a
        }
        for q in factors {
            assert_ne!(pow(generator, &order / BigUint::from(q)), M127Quad::ONE);
        }
        let mut s = 17;
        for _ in 0..64 {
            let x = M127Quad::from_basis_coefficients_fn(|_| Mersenne127::new(next(&mut s)));
            if !x.is_zero() {
                assert_eq!(x * x.inverse(), M127Quad::ONE);
            }
            assert_eq!(
                pow(x, BigUint::from(P)),
                M127Quad::new([
                    <M127Quad as BasedVectorSpace<Mersenne127>>::as_basis_coefficients_slice(&x)[0],
                    -<M127Quad as BasedVectorSpace<Mersenne127>>::as_basis_coefficients_slice(&x)
                        [1]
                ])
            );
        }
        let bytes = postcard::to_allocvec(&generator).unwrap();
        assert_eq!(bytes.len(), 32);
    }
}
