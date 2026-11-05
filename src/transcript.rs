//! Versioned Fiat–Shamir transcript with explicit framing, uniform rejection
//! sampling, and algebra/configuration binding. This compilation is NOT certified
//! by the interactive IOP parameter tables.
use crate::sampling::SampleBase;
use p3_field::{BasedVectorSpace, ExtensionField};
use serde::Serialize;
#[derive(Clone, Debug)]
pub struct Transcript {
    state: [u8; 32],
    counter: u64,
}
impl Transcript {
    pub fn new(public_parameters_id: &[u8; 32]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(&[2]);
        h.update(b"HyperFold-PCS-transcript-v1");
        h.update(public_parameters_id);
        Self {
            state: *h.finalize().as_bytes(),
            counter: 0,
        }
    }
    pub fn observe<T: Serialize>(&mut self, label: &[u8], value: &T) {
        self.observe_bytes(
            label,
            &postcard::to_allocvec(value).expect("transcript serialization"),
        );
    }
    pub fn observe_bytes(&mut self, label: &[u8], bytes: &[u8]) {
        let mut h = blake3::Hasher::new();
        h.update(&[2, 0]);
        h.update(&self.state);
        h.update(&self.counter.to_le_bytes());
        h.update(&(label.len() as u64).to_le_bytes());
        h.update(label);
        h.update(&(bytes.len() as u64).to_le_bytes());
        h.update(bytes);
        self.state = *h.finalize().as_bytes();
        self.counter = 0;
    }
    fn draw(&mut self, label: &[u8]) -> blake3::OutputReader {
        let mut h = blake3::Hasher::new();
        h.update(&[2, 1]);
        h.update(&self.state);
        h.update(&self.counter.to_le_bytes());
        h.update(&(label.len() as u64).to_le_bytes());
        h.update(label);
        self.counter = self.counter.checked_add(1).expect("transcript counter");
        h.finalize_xof()
    }
    pub fn sample<F: SampleBase, E: ExtensionField<F>>(&mut self, label: &[u8]) -> E {
        let mut rng = self.draw(label);
        <E as BasedVectorSpace<F>>::from_basis_coefficients_fn(|_| F::sample(&mut rng))
    }
    pub fn index(&mut self, log: usize) -> usize {
        assert!(log <= 32);
        let mut b = [0; 8];
        self.draw(b"query-index").fill(&mut b);
        (u64::from_le_bytes(b) & ((1u64 << log) - 1)) as usize
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::extension::CubicTrinomialExtensionField;
    use p3_goldilocks::Goldilocks as F;
    type E = CubicTrinomialExtensionField<F>;
    #[test]
    fn framing_and_state() {
        let mut a = Transcript::new(&[1; 32]);
        let mut b = a.clone();
        a.observe_bytes(b"ab", b"c");
        b.observe_bytes(b"a", b"bc");
        assert_ne!(a.sample::<F, E>(b"x"), b.sample::<F, E>(b"x"));
        let mut b = a.clone();
        assert_eq!(a.index(0), 0);
        assert_eq!(b.index(0), 0);
        assert_eq!(a.sample::<F, E>(b"x"), b.sample::<F, E>(b"x"));
    }
}
