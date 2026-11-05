//! Rejection sampling, not modular reduction. Setup seeds are public reproducibility
//! inputs; substituting a seeded XOF for ideal Setup coins is outside the IOP certificate.
use crate::field_m127::{Mersenne127, P};
use p3_field::{Field, PrimeCharacteristicRing, PrimeField64};
use p3_goldilocks::Goldilocks;

pub trait SampleBase: Field {
    const KIND: crate::params::BaseFieldKind;
    fn fft_root(_log: usize) -> Option<Self> {
        None
    }
    fn sample(reader: &mut blake3::OutputReader) -> Self;
}
impl SampleBase for Goldilocks {
    fn fft_root(log: usize) -> Option<Self> {
        use p3_field::TwoAdicField;
        (log <= 32).then(|| Self::two_adic_generator(log))
    }
    const KIND: crate::params::BaseFieldKind = crate::params::BaseFieldKind::Goldilocks;
    fn sample(reader: &mut blake3::OutputReader) -> Self {
        loop {
            let mut b = [0; 8];
            reader.fill(&mut b);
            let x = u64::from_le_bytes(b);
            if x < Self::ORDER_U64 {
                return Self::from_u64(x);
            }
        }
    }
}
impl SampleBase for Mersenne127 {
    const KIND: crate::params::BaseFieldKind = crate::params::BaseFieldKind::M127;
    fn sample(reader: &mut blake3::OutputReader) -> Self {
        loop {
            let mut b = [0; 16];
            reader.fill(&mut b);
            let x = u128::from_le_bytes(b) & P;
            if x < P {
                return Self::new(x);
            }
        }
    }
}
pub fn stream(seed: &[u8; 32], role: &[u8], index: u64) -> blake3::OutputReader {
    let mut h = blake3::Hasher::new();
    h.update(b"hyperfold-setup-v1");
    h.update(seed);
    h.update(&(role.len() as u64).to_le_bytes());
    h.update(role);
    h.update(&index.to_le_bytes());
    h.finalize_xof()
}
pub fn nonzero<F: SampleBase>(r: &mut blake3::OutputReader) -> F {
    loop {
        let x = F::sample(r);
        if !x.is_zero() {
            return x;
        }
    }
}
