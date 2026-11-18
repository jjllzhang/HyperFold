//! Four canonical u128 elements per pack, with their scalar memory layout intact.
//! Multiplication uses three radix-2^52 limbs and AVX-512 IFMA instructions.
use core::{
    arch::x86_64::*,
    iter::{Product, Sum},
    mem::transmute,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign},
};
use p3_field::{
    Algebra, Field, PackedField, PackedFieldPow2, PackedValue, PrimeCharacteristicRing,
    impl_packed_field_pow_2,
    interleave::{interleave_u128, interleave_u256},
    op_assign_macros::{
        impl_add_assign, impl_add_base_field, impl_div_methods, impl_mul_base_field,
        impl_mul_methods, impl_packed_field_div, impl_packed_value, impl_sub_assign,
        impl_sub_base_field, impl_sum_prod_base_field, ring_sum,
    },
};

use super::Mersenne127;

const WIDTH: usize = 4;
const LOW_LIMBS: u8 = 0x55;
const HIGH_LIMBS: u8 = 0xaa;
// Each adjacent pair holds the low 64 bits and high 63 bits of one field element.
const PRIME: __m512i = unsafe {
    transmute([
        u64::MAX,
        u64::MAX >> 1,
        u64::MAX,
        u64::MAX >> 1,
        u64::MAX,
        u64::MAX >> 1,
        u64::MAX,
        u64::MAX >> 1,
    ])
};

/// Four M127 elements using AVX-512F/DQ/VL/IFMA, selected at compile time.
/// All lanes retain the scalar type's canonical representation in `[0, 2^127-1)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
#[must_use]
pub struct PackedMersenne127AVX512(pub [Mersenne127; WIDTH]);

impl PackedMersenne127AVX512 {
    #[inline]
    pub const fn broadcast(value: Mersenne127) -> Self {
        Self([value; WIDTH])
    }

    #[inline]
    fn to_vector(self) -> __m512i {
        // SAFETY: the transparent wrapper and four transparent u128 scalars occupy
        // exactly 512 bits; every bit pattern of __m512i is valid.
        unsafe { transmute(self) }
    }

    /// Every adjacent 64-bit pair must encode a canonical M127 element.
    #[inline]
    unsafe fn from_vector(vector: __m512i) -> Self {
        // SAFETY: the caller guarantees the scalar invariant; sizes match exactly.
        unsafe { transmute(vector) }
    }
}

impl From<Mersenne127> for PackedMersenne127AVX512 {
    #[inline]
    fn from(value: Mersenne127) -> Self {
        Self::broadcast(value)
    }
}

impl Add for PackedMersenne127AVX512 {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        // SAFETY: canonical operands sum to less than 2P; reduction restores [0,P).
        unsafe { Self::from_vector(reduce(add_128(self.to_vector(), rhs.to_vector()))) }
    }
}

impl Sub for PackedMersenne127AVX512 {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        unsafe {
            let difference = sub_128(self.to_vector(), rhs.to_vector());
            // Negative differences wrap modulo 2^128 and have bit 127 set.
            // Clearing that bit and subtracting one adds P to the signed value.
            let correction = _mm512_maskz_mov_epi64(
                LOW_LIMBS,
                _mm512_shuffle_epi32::<0x4e>(_mm512_srli_epi64::<63>(difference)),
            );
            let result = sub_128(_mm512_and_si512(difference, PRIME), correction);
            // SAFETY: differences lie in (-P,P), so this correction is canonical.
            Self::from_vector(result)
        }
    }
}

impl Neg for PackedMersenne127AVX512 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        // SAFETY: P xor x equals P-x; canonicalize maps the zero input back to zero.
        unsafe { Self::from_vector(canonicalize(_mm512_xor_si512(self.to_vector(), PRIME))) }
    }
}

impl Mul for PackedMersenne127AVX512 {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self {
        // SAFETY: multiply reduces all four canonical products to [0,P).
        unsafe { Self::from_vector(multiply(self.to_vector(), rhs.to_vector())) }
    }
}

impl_add_assign!(PackedMersenne127AVX512);
impl_sub_assign!(PackedMersenne127AVX512);
impl_mul_methods!(PackedMersenne127AVX512);
ring_sum!(PackedMersenne127AVX512);
impl_add_base_field!(PackedMersenne127AVX512, Mersenne127);
impl_sub_base_field!(PackedMersenne127AVX512, Mersenne127);
impl_mul_base_field!(PackedMersenne127AVX512, Mersenne127);
impl_div_methods!(PackedMersenne127AVX512, Mersenne127);
impl_packed_field_div!(PackedMersenne127AVX512);
impl_sum_prod_base_field!(PackedMersenne127AVX512, Mersenne127);

impl PrimeCharacteristicRing for PackedMersenne127AVX512 {
    type PrimeSubfield = Mersenne127;

    const ZERO: Self = Self::broadcast(Mersenne127::ZERO);
    const ONE: Self = Self::broadcast(Mersenne127::ONE);
    const TWO: Self = Self::broadcast(Mersenne127::TWO);
    const NEG_ONE: Self = Self::broadcast(Mersenne127::NEG_ONE);

    #[inline]
    fn from_prime_subfield(value: Mersenne127) -> Self {
        Self::broadcast(value)
    }

    #[inline]
    fn halve(&self) -> Self {
        self.mul_2exp_u64(126)
    }

    #[inline]
    fn mul_2exp_u64(&self, exp: u64) -> Self {
        // Since 2^127 = 1 mod P, multiplication by 2^exp rotates 127 bits.
        // Rotation preserves canonicality: no canonical input has all 127 bits set.
        unsafe { Self::from_vector(rotate_left_127(self.to_vector(), (exp % 127) as u32)) }
    }

    #[inline]
    fn div_2exp_u64(&self, exp: u64) -> Self {
        self.mul_2exp_u64(127 - exp % 127)
    }
}

impl Algebra<Mersenne127> for PackedMersenne127AVX512 {}

// SAFETY: repr(transparent) preserves the size and alignment of [Mersenne127;4].
impl_packed_value!(PackedMersenne127AVX512, Mersenne127, WIDTH);

// SAFETY: the packing has the required array layout and lane-wise field operations.
unsafe impl PackedField for PackedMersenne127AVX512 {
    type Scalar = Mersenne127;
}

// Existing Plonky3 permutations move complete 128-bit elements without changing them.
impl_packed_field_pow_2!(
    PackedMersenne127AVX512;
    [(1, interleave_u128), (2, interleave_u256)],
    WIDTH
);

/// Add four independent u128 values, wrapping at 128 bits.
#[inline]
unsafe fn add_128(left: __m512i, right: __m512i) -> __m512i {
    unsafe {
        let sum = _mm512_add_epi64(left, right);
        let carry = _mm512_cmplt_epu64_mask(sum, left) & LOW_LIMBS;
        _mm512_mask_add_epi64(sum, carry << 1, sum, _mm512_set1_epi64(1))
    }
}

/// Subtract four independent u128 values, wrapping at 128 bits.
#[inline]
unsafe fn sub_128(left: __m512i, right: __m512i) -> __m512i {
    unsafe {
        let difference = _mm512_sub_epi64(left, right);
        let borrow = _mm512_cmplt_epu64_mask(left, right) & LOW_LIMBS;
        _mm512_mask_sub_epi64(difference, borrow << 1, difference, _mm512_set1_epi64(1))
    }
}

/// Map P to zero; all other input lanes must already be below P.
#[inline]
unsafe fn canonicalize(value: __m512i) -> __m512i {
    unsafe {
        let equal = _mm512_cmpeq_epi64_mask(value, PRIME);
        let low = equal & (equal >> 1) & LOW_LIMBS;
        _mm512_mask_mov_epi64(value, low | (low << 1), _mm512_setzero_si512())
    }
}

/// Reduce four unsigned values below 2P using 2^127 = 1 mod P.
#[inline]
unsafe fn reduce(value: __m512i) -> __m512i {
    unsafe {
        let carry = _mm512_maskz_mov_epi64(
            LOW_LIMBS,
            _mm512_shuffle_epi32::<0x4e>(_mm512_srli_epi64::<63>(value)),
        );
        canonicalize(add_128(_mm512_and_si512(value, PRIME), carry))
    }
}

/// Rotate canonical values left within their 127 significant bits, for shift < 127.
#[inline]
unsafe fn rotate_left_127(value: __m512i, shift: u32) -> __m512i {
    unsafe {
        if shift == 0 {
            return value;
        }
        let swapped = _mm512_shuffle_epi32::<0x4e>(value);
        let shift_vector = _mm512_set1_epi64(shift as i64);
        let (left, right) = if shift < 64 {
            let cross = _mm512_maskz_mov_epi64(
                HIGH_LIMBS,
                _mm512_srlv_epi64(swapped, _mm512_set1_epi64((64 - shift) as i64)),
            );
            let left = _mm512_or_si512(_mm512_sllv_epi64(value, shift_vector), cross);
            let right = _mm512_maskz_mov_epi64(
                LOW_LIMBS,
                _mm512_srlv_epi64(swapped, _mm512_set1_epi64((63 - shift) as i64)),
            );
            (left, right)
        } else {
            let left = _mm512_maskz_mov_epi64(
                HIGH_LIMBS,
                _mm512_sllv_epi64(swapped, _mm512_set1_epi64((shift - 64) as i64)),
            );
            let cross = _mm512_maskz_mov_epi64(
                LOW_LIMBS,
                _mm512_sllv_epi64(swapped, _mm512_set1_epi64((shift - 63) as i64)),
            );
            let right = _mm512_or_si512(
                _mm512_srlv_epi64(value, _mm512_set1_epi64((127 - shift) as i64)),
                cross,
            );
            (left, right)
        };
        _mm512_or_si512(_mm512_and_si512(left, PRIME), right)
    }
}

/// Four independent products with exact 52-bit IFMA partial products.
#[inline]
unsafe fn multiply(left: __m512i, right: __m512i) -> __m512i {
    unsafe {
        let zero = _mm256_setzero_si256();
        let mask52 = _mm256_set1_epi64x((1i64 << 52) - 1);
        let mask23 = _mm256_set1_epi64x((1i64 << 23) - 1);
        let separate = _mm512_setr_epi64(0, 2, 4, 6, 1, 3, 5, 7);
        let left = _mm512_permutexvar_epi64(separate, left);
        let right = _mm512_permutexvar_epi64(separate, right);
        let left_lo = _mm512_castsi512_si256(left);
        let left_hi = _mm512_extracti64x4_epi64::<1>(left);
        let right_lo = _mm512_castsi512_si256(right);
        let right_hi = _mm512_extracti64x4_epi64::<1>(right);
        // Each operand has 52 + 52 + 23 significant bits.
        let a0 = _mm256_and_si256(left_lo, mask52);
        let a1 = _mm256_and_si256(
            _mm256_or_si256(
                _mm256_srli_epi64::<52>(left_lo),
                _mm256_slli_epi64::<12>(left_hi),
            ),
            mask52,
        );
        let a2 = _mm256_srli_epi64::<40>(left_hi);
        let b0 = _mm256_and_si256(right_lo, mask52);
        let b1 = _mm256_and_si256(
            _mm256_or_si256(
                _mm256_srli_epi64::<52>(right_lo),
                _mm256_slli_epi64::<12>(right_hi),
            ),
            mask52,
        );
        let b2 = _mm256_srli_epi64::<40>(right_hi);

        // Accumulate the base-2^52 convolution. Even the largest accumulator
        // stays below 5*2^52, so none of these 64-bit additions can overflow.
        let p0 = _mm256_madd52lo_epu64(zero, a0, b0);
        let mut p1 = _mm256_madd52hi_epu64(zero, a0, b0);
        p1 = _mm256_madd52lo_epu64(p1, a0, b1);
        p1 = _mm256_madd52lo_epu64(p1, a1, b0);
        let mut p2 = _mm256_madd52hi_epu64(zero, a0, b1);
        p2 = _mm256_madd52hi_epu64(p2, a1, b0);
        p2 = _mm256_madd52lo_epu64(p2, a0, b2);
        p2 = _mm256_madd52lo_epu64(p2, a1, b1);
        p2 = _mm256_madd52lo_epu64(p2, a2, b0);
        let mut p3 = _mm256_madd52hi_epu64(zero, a0, b2);
        p3 = _mm256_madd52hi_epu64(p3, a1, b1);
        p3 = _mm256_madd52hi_epu64(p3, a2, b0);
        p3 = _mm256_madd52lo_epu64(p3, a1, b2);
        p3 = _mm256_madd52lo_epu64(p3, a2, b1);
        let mut p4 = _mm256_madd52hi_epu64(zero, a1, b2);
        p4 = _mm256_madd52hi_epu64(p4, a2, b1);
        p4 = _mm256_madd52lo_epu64(p4, a2, b2); // a2*b2 < 2^46: no high part.
        p2 = _mm256_add_epi64(p2, _mm256_srli_epi64::<52>(p1));
        p1 = _mm256_and_si256(p1, mask52);
        p3 = _mm256_add_epi64(p3, _mm256_srli_epi64::<52>(p2));
        p2 = _mm256_and_si256(p2, mask52);
        p4 = _mm256_add_epi64(p4, _mm256_srli_epi64::<52>(p3));
        p3 = _mm256_and_si256(p3, mask52);

        // Split at bit 127, then add the high 127 bits to the low 127 bits.
        let high0 = _mm256_or_si256(
            _mm256_srli_epi64::<23>(p2),
            _mm256_slli_epi64::<29>(_mm256_and_si256(p3, mask23)),
        );
        let high1 = _mm256_or_si256(
            _mm256_srli_epi64::<23>(p3),
            _mm256_slli_epi64::<29>(_mm256_and_si256(p4, mask23)),
        );
        let high2 = _mm256_srli_epi64::<23>(p4);
        let sum0 = _mm256_add_epi64(p0, high0);
        let sum1 = _mm256_add_epi64(_mm256_add_epi64(p1, high1), _mm256_srli_epi64::<52>(sum0));
        let sum2 = _mm256_add_epi64(
            _mm256_add_epi64(_mm256_and_si256(p2, mask23), high2),
            _mm256_srli_epi64::<52>(sum1),
        );
        let sum1 = _mm256_and_si256(sum1, mask52);
        let low = _mm256_or_si256(
            _mm256_and_si256(sum0, mask52),
            _mm256_slli_epi64::<52>(sum1),
        );
        let high = _mm256_or_si256(_mm256_srli_epi64::<12>(sum1), _mm256_slli_epi64::<40>(sum2));
        let joined = _mm512_inserti64x4::<1>(_mm512_castsi256_si512(low), high);
        let interleave = _mm512_setr_epi64(0, 4, 1, 5, 2, 6, 3, 7);
        reduce(_mm512_permutexvar_epi64(interleave, joined))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_m127::P;
    use num_bigint::BigUint;

    fn next(seed: &mut u128) -> Mersenne127 {
        *seed = seed
            .wrapping_mul(0xda942042e4dd58b5)
            .wrapping_add(0x9e3779b97f4a7c15);
        Mersenne127::new(*seed)
    }

    fn check_arithmetic(left: PackedMersenne127AVX512, right: PackedMersenne127AVX512) {
        let prime = BigUint::from(P);
        let sum = left + right;
        let difference = left - right;
        let product = left * right;
        let negated = -left;
        for lane in 0..WIDTH {
            let a = BigUint::from(left.0[lane].canonical());
            let b = BigUint::from(right.0[lane].canonical());
            assert_eq!(BigUint::from(sum.0[lane].canonical()), (&a + &b) % &prime);
            assert_eq!(
                BigUint::from(difference.0[lane].canonical()),
                (&a + &prime - &b) % &prime
            );
            assert_eq!(
                BigUint::from(product.0[lane].canonical()),
                (&a * &b) % &prime
            );
            assert_eq!(
                BigUint::from(negated.0[lane].canonical()),
                (&prime - &a) % &prime
            );
            for value in [sum, difference, product, negated] {
                assert!(value.0[lane].canonical() < P);
            }
        }
        assert_eq!(left.square(), left * left);
        assert_eq!(left.halve().double(), left);
    }

    #[test]
    fn packed_arithmetic_matches_biguint_at_limb_boundaries_and_random_inputs() {
        let boundaries = [
            0,
            1,
            2,
            (1 << 23) - 1,
            1 << 23,
            (1 << 52) - 1,
            1 << 52,
            (1 << 52) + 1,
            (1 << 63) - 1,
            1 << 63,
            (1 << 64) - 1,
            1 << 64,
            (1 << 64) + 1,
            (1 << 104) - 1,
            1 << 104,
            (1 << 104) + 1,
            (1 << 126) - 1,
            1 << 126,
            P - 2,
            P - 1,
        ];
        for i in 0..boundaries.len() {
            for j in 0..boundaries.len() {
                let left = PackedMersenne127AVX512::from_fn(|lane| {
                    Mersenne127::new(boundaries[(i + lane) % boundaries.len()])
                });
                let right = PackedMersenne127AVX512::from_fn(|lane| {
                    Mersenne127::new(boundaries[(j + 3 * lane) % boundaries.len()])
                });
                check_arithmetic(left, right);
            }
        }
        let mut seed = 0x987654321abcdefu128;
        for _ in 0..10_000 {
            let left = PackedMersenne127AVX512::from_fn(|_| next(&mut seed));
            let right = PackedMersenne127AVX512::from_fn(|_| next(&mut seed));
            check_arithmetic(left, right);
        }
    }

    #[test]
    fn packed_power_of_two_rotations_match_biguint() {
        let mut seed = 0x123456789abcdefu128;
        let prime = BigUint::from(P);
        for iteration in 0..128 {
            let value = PackedMersenne127AVX512::from_fn(|lane| match (iteration, lane) {
                (0, 0) => Mersenne127::ZERO,
                (0, 1) => Mersenne127::ONE,
                (0, 2) => Mersenne127::new(P - 1),
                (0, 3) => Mersenne127::new((1 << 64) - 1),
                _ => next(&mut seed),
            });
            for exp in (0..=255).chain([u64::MAX - 1, u64::MAX]) {
                let multiplied = value.mul_2exp_u64(exp);
                let divided = value.div_2exp_u64(exp);
                for lane in 0..WIDTH {
                    let input = BigUint::from(value.0[lane].canonical());
                    assert_eq!(
                        BigUint::from(multiplied.0[lane].canonical()),
                        (&input << (exp % 127) as usize) % &prime
                    );
                    assert_eq!(
                        BigUint::from(divided.0[lane].canonical()),
                        (&input << ((127 - exp % 127) % 127) as usize) % &prime
                    );
                    assert!(multiplied.0[lane].canonical() < P);
                    assert!(divided.0[lane].canonical() < P);
                }
            }
        }
    }

    #[test]
    fn packed_layout_interleaving_and_scalar_operations() {
        assert_eq!(
            size_of::<PackedMersenne127AVX512>(),
            WIDTH * size_of::<Mersenne127>()
        );
        assert_eq!(
            align_of::<PackedMersenne127AVX512>(),
            align_of::<Mersenne127>()
        );
        let a = PackedMersenne127AVX512::from_fn(|i| Mersenne127::new(i as u128 + 1));
        let b = PackedMersenne127AVX512::from_fn(|i| Mersenne127::new(i as u128 + 11));
        for (block, first, second) in [
            (1, [1, 11, 3, 13], [2, 12, 4, 14]),
            (2, [1, 2, 11, 12], [3, 4, 13, 14]),
            (4, [1, 2, 3, 4], [11, 12, 13, 14]),
        ] {
            let (left, right) = a.interleave(b, block);
            assert_eq!(left.0.map(Mersenne127::canonical), first);
            assert_eq!(right.0.map(Mersenne127::canonical), second);
        }
        let scalar = Mersenne127::new(17);
        assert_eq!(a + scalar, a + PackedMersenne127AVX512::broadcast(scalar));
        assert_eq!(scalar + a, a + scalar);
        assert_eq!(scalar - a, PackedMersenne127AVX512::broadcast(scalar) - a);
        assert_eq!(scalar * a, a * scalar);
        assert_eq!((a / scalar) * scalar, a);
        assert_eq!((a / b) * b, a);
        assert_eq!([a, b].into_iter().sum::<PackedMersenne127AVX512>(), a + b);
        assert_eq!(
            [a, b].into_iter().product::<PackedMersenne127AVX512>(),
            a * b
        );

        // Packing must also work at scalar-aligned offsets and preserve its suffix.
        let mut values = [Mersenne127::ONE; 10];
        let (packs, suffix) = PackedMersenne127AVX512::pack_slice_with_suffix_mut(&mut values[1..]);
        for pack in packs {
            *pack *= scalar;
        }
        assert_eq!(suffix, &[Mersenne127::ONE]);
        assert_eq!(values[0], Mersenne127::ONE);
        assert_eq!(&values[1..9], &[scalar; 8]);
        assert_eq!(values[9], Mersenne127::ONE);
    }
}
