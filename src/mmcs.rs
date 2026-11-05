//! Canonical byte hashing over Plonky3 binary MerkleTreeMmcs, including native
//! PrunedMerklePaths. Role prefixes separate leaf and internal-node hashing.
use crate::params::{Error, Result};
use p3_commit::Mmcs;
use p3_field::Field;
use p3_matrix::{Dimensions, dense::RowMajorMatrix};
use p3_merkle_tree::{MerkleTree, MerkleTreeMmcs, PrunedMerklePaths};
use p3_symmetric::{CryptographicHasher, MerkleCap, PseudoCompressionFunction};
use serde::{Deserialize, Serialize};
pub type Digest = [u8; 32];
#[derive(Clone, Copy, Debug)]
pub struct LeafHash;
impl<T: Field> CryptographicHasher<T, Digest> for LeafHash {
    fn hash_iter<I: IntoIterator<Item = T>>(&self, input: I) -> Digest {
        let mut h = blake3::Hasher::new();
        h.update(&[0]);
        h.update(&(T::NUM_BYTES as u64).to_le_bytes());
        let mut count = 0u64;
        let mut buffer = [0u8; 1024];
        let mut used = 0;
        for v in input {
            for b in v.into_bytes() {
                buffer[used] = b;
                used += 1;
                if used == buffer.len() {
                    h.update(&buffer);
                    used = 0;
                }
            }
            count += 1;
        }
        h.update(&buffer[..used]);
        h.update(&count.to_le_bytes());
        *h.finalize().as_bytes()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct NodeHash;
impl PseudoCompressionFunction<Digest, 2> for NodeHash {
    fn compress(&self, input: [Digest; 2]) -> Digest {
        let mut h = blake3::Hasher::new();
        h.update(&[1]);
        h.update(&input[0]);
        h.update(&input[1]);
        *h.finalize().as_bytes()
    }
}
pub type CanonicalMmcs<T> = MerkleTreeMmcs<T, u8, LeafHash, NodeHash, 2, 32>;
pub type Tree<T> = MerkleTree<T, u8, RowMajorMatrix<T>, 2, 32>;
pub fn mmcs<T: Field>() -> CanonicalMmcs<T> {
    MerkleTreeMmcs::new(LeafHash, NodeHash, 0)
}
pub fn commit<T: Field>(matrix: RowMajorMatrix<T>) -> (Digest, Tree<T>) {
    let (cap, tree) = mmcs::<T>().commit_matrix(matrix);
    (cap.roots()[0], tree)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "T: Serialize",
    deserialize = "T: serde::de::DeserializeOwned"
))]
pub struct Opening<T> {
    pub rows: Vec<Vec<T>>,
    pub paths: PrunedMerklePaths<u8, 32>,
}
pub fn open<T: Field>(tree: &Tree<T>, indices: &[usize]) -> Opening<T> {
    let (rows, paths) = mmcs::<T>().open_multi_batch(indices, tree);
    Opening {
        rows: rows.into_iter().map(|mut r| r.remove(0)).collect(),
        paths,
    }
}
pub fn verify<T: Field>(
    root: Digest,
    height: usize,
    width: usize,
    indices: &[usize],
    rows: &[Vec<T>],
    paths: &PrunedMerklePaths<u8, 32>,
) -> Result<()> {
    if rows.len() != indices.len()
        || rows.iter().any(|r| r.len() != width)
        || indices.is_empty()
        || indices.windows(2).any(|w| w[0] >= w[1])
        || indices.iter().any(|&i| i >= height)
        || paths.sibling_hashes.len() > indices.len() * height.ilog2() as usize
    {
        return Err(Error::Proof("Merkle geometry"));
    }
    let values: Vec<Vec<&[T]>> = rows.iter().map(|r| vec![r.as_slice()]).collect();
    mmcs::<T>()
        .verify_multi_batch(
            &MerkleCap::new(vec![root]),
            &[Dimensions { height, width }],
            indices,
            &values,
            paths,
        )
        .map_err(|_| Error::Merkle)
}
#[cfg(test)]
mod tests {
    use super::*;
    use p3_field::PrimeCharacteristicRing;
    use p3_goldilocks::Goldilocks as F;
    #[test]
    fn multiproof_ragged_heights_multi_batch() {
        let m = mmcs::<F>();
        for heights in [[16, 4, 4, 8], [4, 4, 8, 16], [8, 16, 4, 4]] {
            let matrices: Vec<_> = heights
                .iter()
                .enumerate()
                .map(|(j, &h)| {
                    RowMajorMatrix::new(
                        (0..h * (j + 1)).map(|v| F::from_usize(v + 17)).collect(),
                        j + 1,
                    )
                })
                .collect();
            let dims: Vec<_> = heights
                .iter()
                .enumerate()
                .map(|(j, &h)| Dimensions {
                    height: h,
                    width: j + 1,
                })
                .collect();
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
            assert!(paths.sibling_hashes.len() < indices.len() * 4);
        }
    }
    #[test]
    fn canonical_and_domains() {
        let x = F::from_u8(5);
        let a = LeafHash.hash_iter([x, x]);
        assert_ne!(a, NodeHash.compress([a, a]));
        let (root, t) = commit(RowMajorMatrix::new((0..64).map(F::from_usize).collect(), 4));
        let idx = [1, 2, 3, 9];
        let o = open(&t, &idx);
        verify(root, 16, 4, &idx, &o.rows, &o.paths).unwrap();
        let mut bad = o.rows.clone();
        bad[0][0] += F::ONE;
        assert!(verify(root, 16, 4, &idx, &bad, &o.paths).is_err());
        let singles: usize = idx
            .iter()
            .map(|&i| open(&t, &[i]).paths.sibling_hashes.len())
            .sum();
        assert!(o.paths.sibling_hashes.len() < singles);
    }
}
