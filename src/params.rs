//! Exact versioned parameter imports. Small correctness configurations are explicitly
//! uncertified; only the published experiment rows carry the IOP certificate label.
use crate::sampling::{SampleBase, nonzero, stream};
use p3_field::batch_multiplicative_inverse;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
pub const CERTIFICATE_VERSION: &str = "hfc-tight-total100-gold-m127-v3-2026-11-05";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BaseFieldKind {
    Goldilocks,
    M127,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BaseCodeKind {
    Rs,
    Qa,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HyperFoldConfig {
    pub base_field: BaseFieldKind,
    pub backend: BaseCodeKind,
    pub mu: usize,
    pub mu0: usize,
    pub r: usize,
    pub c: usize,
    pub extension_degree: usize,
    pub eta_num: u64,
    pub eta_den: u64,
    pub num_queries: usize,
    pub d0_lower: usize,
    pub b0_lower: usize,
    pub td_upper: u64,
    pub certified: bool,
}
#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(&'static str),
    #[error("malformed or inconsistent proof: {0}")]
    Proof(&'static str),
    #[error("Merkle authentication failed")]
    Merkle,
    #[error("codec error: {0}")]
    Codec(String),
}
pub type Result<T> = core::result::Result<T, Error>;
impl HyperFoldConfig {
    pub fn nu(self) -> usize {
        self.r.ilog2() as usize
    }
    pub fn depth(self) -> usize {
        (self.mu - self.mu0) / self.nu()
    }
    pub fn k0(self) -> usize {
        1 << self.mu0
    }
    pub fn k(self) -> usize {
        1 << self.mu
    }
    pub fn n(self, i: usize) -> usize {
        self.c * self.k0() * (1usize << (i * self.nu()))
    }
    pub fn validate(self) -> Result<()> {
        if ![2, 4, 8].contains(&self.r) || ![2, 4].contains(&self.c) {
            return Err(Error::Config("r or c"));
        }
        if self.mu0 == 0
            || self.mu0 > 10
            || self.mu <= self.mu0
            || self.mu > 30
            || !(self.mu - self.mu0).is_multiple_of(self.nu())
        {
            return Err(Error::Config("size/divisibility"));
        }
        if self.num_queries == 0 || self.num_queries > 10000 || self.eta_den == 0 {
            return Err(Error::Config("queries/eta"));
        }
        if self.certified && !CERTIFIED_CONFIGS.contains(&self) {
            return Err(Error::Config("certificate mismatch"));
        }
        Ok(())
    }
    pub fn experiment(
        field: BaseFieldKind,
        backend: BaseCodeKind,
        mu: usize,
        r: usize,
        c: usize,
    ) -> Result<Self> {
        CERTIFIED_CONFIGS
            .iter()
            .find(|x| {
                x.base_field == field && x.backend == backend && x.mu == mu && x.r == r && x.c == c
            })
            .copied()
            .ok_or(Error::Config("no certified row"))
    }
    /// For functional tests only; never label these parameters 100-bit certified.
    pub fn testing(
        field: BaseFieldKind,
        backend: BaseCodeKind,
        mu: usize,
        mu0: usize,
        r: usize,
        c: usize,
        e: usize,
    ) -> Result<Self> {
        let x = Self {
            base_field: field,
            backend,
            mu,
            mu0,
            r,
            c,
            extension_degree: e,
            eta_num: 1,
            eta_den: 1 << 20,
            num_queries: 24,
            d0_lower: 0,
            b0_lower: 0,
            td_upper: 0,
            certified: false,
        };
        x.validate()?;
        Ok(x)
    }
    pub fn id(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(CERTIFICATE_VERSION.as_bytes());
        h.update(&postcard::to_allocvec(&self).expect("fixed config serialization"));
        *h.finalize().as_bytes()
    }
}
include!("certified_params.rs");
#[derive(Debug)]
pub struct DiagonalLevel<F> {
    pub forward: Vec<F>,
    pub inverse: Vec<F>,
}
#[derive(Debug)]
pub struct DiagonalTables<F> {
    pub levels: Vec<DiagonalLevel<F>>,
}
impl<F: SampleBase> DiagonalTables<F> {
    pub fn new(config: HyperFoldConfig, seed: &[u8; 32]) -> Self {
        let levels = (1..=config.depth())
            .map(|i| {
                let len = (config.r - 1) * config.n(i - 1);
                let mut forward = vec![F::ZERO; len];
                forward
                    .par_chunks_mut(4096)
                    .enumerate()
                    .for_each(|(chunk, values)| {
                        let mut rng = stream(seed, b"diagonal", ((i as u64) << 32) | chunk as u64);
                        for x in values {
                            *x = nonzero(&mut rng);
                        }
                    });
                let inverse = batch_multiplicative_inverse(&forward);
                DiagonalLevel { forward, inverse }
            })
            .collect();
        Self { levels }
    }
    pub fn has_forward(&self) -> bool {
        self.levels.iter().all(|l| !l.forward.is_empty())
    }
    pub fn has_inverse(&self) -> bool {
        self.levels.iter().all(|l| !l.inverse.is_empty())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::{Field, PrimeCharacteristicRing};
    use p3_goldilocks::Goldilocks;
    #[test]
    fn certificates_and_rejections() {
        assert_eq!(CERTIFIED_CONFIGS.len(), 144);
        for x in CERTIFIED_CONFIGS {
            x.validate().unwrap();
            assert_eq!(
                x.extension_degree,
                if x.base_field == BaseFieldKind::Goldilocks {
                    3
                } else {
                    2
                }
            );
            assert_eq!(x.mu0, 6 + (x.mu - 6) % x.nu());
        }
        let mut x = CERTIFIED_CONFIGS[0];
        x.num_queries -= 1;
        assert!(x.validate().is_err());
        assert!(
            HyperFoldConfig::testing(BaseFieldKind::Goldilocks, BaseCodeKind::Qa, 10, 6, 8, 2, 3)
                .is_err()
        );
    }
    #[test]
    fn diagonals_nonzero_and_inverse() {
        let c =
            HyperFoldConfig::testing(BaseFieldKind::Goldilocks, BaseCodeKind::Qa, 6, 2, 4, 2, 3)
                .unwrap();
        let t = DiagonalTables::<Goldilocks>::new(c, &[1; 32]);
        for l in t.levels {
            for (a, b) in l.forward.iter().zip(l.inverse) {
                assert!(!a.is_zero());
                assert_eq!(*a * b, Goldilocks::ONE);
            }
        }
    }
}
