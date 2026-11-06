//! HyperFold Protocols 4.1/4.2, compiled with the versioned research transcript.
//! Only certified experiment rows claim an interactive IOP bound; Fiat–Shamir,
//! public-seed Setup, Merkle and implementation security are not established here.
use crate::{
    base_code::{BaseCode, BaseCodeImpl},
    encoder::{RecoveredFirstMatrix, beta, encode_top, fold_level, fold_row},
    matrix_h::LocalMatrix,
    mmcs::{self, Digest, Opening, Tree},
    params::{DiagonalTables, Error, HyperFoldConfig, Result},
    sampling::SampleBase,
    sumcheck,
    transcript::Transcript,
};
use core::marker::PhantomData;
use p3_commit::Mmcs;
use p3_field::{AlgebraIdentity, BasedVectorSpace, ExtensionField};
use p3_merkle_tree::PrunedMerklePaths;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "E: Serialize",
    deserialize = "E: serde::de::DeserializeOwned"
))]
pub struct Commitment<E> {
    pub parameters_id: Digest,
    pub root: Digest,
    pub y0: E,
}
#[derive(Debug)]
pub struct CommitData<F> {
    pub message: Vec<F>,
    pub tree: Tree<F>,
    root: Digest,
}
/// Top-level commitment storage, kept by the caller through verification and
/// serialization. Splitting CommitData moves its allocations without copying them.
#[derive(Debug)]
pub struct CommittedOracle<F> {
    tree: Tree<F>,
    root: Digest,
}
/// A message witness associated with exactly one committed oracle. Moving this
/// into open_once permits its allocation to be freed after the first subround.
#[derive(Debug)]
pub struct OwnedWitness<F> {
    message: Vec<F>,
    root: Digest,
}
impl<F> OwnedWitness<F> {
    pub fn values(&self) -> &[F] {
        &self.message
    }
}
impl<F> CommitData<F> {
    pub fn into_parts(self) -> (CommittedOracle<F>, OwnedWitness<F>) {
        (
            CommittedOracle {
                tree: self.tree,
                root: self.root,
            },
            OwnedWitness {
                message: self.message,
                root: self.root,
            },
        )
    }
}
enum WitnessSource<'a, F> {
    Borrowed(&'a [F]),
    Owned(Vec<F>),
}
impl<F> WitnessSource<'_, F> {
    fn values(&self) -> &[F] {
        match self {
            Self::Borrowed(values) => values,
            Self::Owned(values) => values,
        }
    }
    fn release_owned(&mut self) -> (usize, usize) {
        match self {
            Self::Borrowed(_) => (0, 0),
            Self::Owned(values) => {
                let removed = std::mem::take(values);
                let allocation = (removed.len(), removed.capacity());
                drop(removed);
                allocation
            }
        }
    }
}
struct ProverInput<'a, F> {
    tree: &'a Tree<F>,
    root: Digest,
    witness: WitnessSource<'a, F>,
}
impl<'a, F> ProverInput<'a, F> {
    fn borrowed(data: &'a CommitData<F>) -> Self {
        Self {
            tree: &data.tree,
            root: data.root,
            witness: WitnessSource::Borrowed(&data.message),
        }
    }
    fn owned(oracle: &'a CommittedOracle<F>, witness: OwnedWitness<F>) -> Result<Self> {
        if oracle.root != witness.root {
            return Err(Error::Proof("witness/oracle binding"));
        }
        Ok(Self {
            tree: &oracle.tree,
            root: oracle.root,
            witness: WitnessSource::Owned(witness.message),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "E: Serialize",
    deserialize = "E: serde::de::DeserializeOwned"
))]
pub struct CompressedOpening<E> {
    pub rows: Vec<Vec<E>>,
    pub paths: PrunedMerklePaths<u8, 32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "F: Serialize, E: Serialize",
    deserialize = "F: serde::de::DeserializeOwned,E: serde::de::DeserializeOwned"
))]
pub struct Proof<F, E> {
    pub rounds: Vec<Vec<[E; 3]>>,
    pub roots: Vec<Digest>,
    pub final_table: Vec<E>,
    pub top: Opening<F>,
    pub inner: Vec<CompressedOpening<E>>,
}
/// Explicit time/memory tradeoff. Rebuilding is included in open time.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenOptions {
    pub recompute_first_tree: bool,
}
/// Mutually exclusive first-intermediate-tree storage strategies. The default
/// public opening API retains the original native tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FirstTreeStrategy {
    #[default]
    NativeRetained,
    Rebuild,
    PrehashedRetained,
    PrehashedRecovered,
}
/// Optional coarse profiling of first-tree query extraction and owned-witness
/// release. All measured work remains inside opening time.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenTimings {
    pub first_tree_query: Duration,
    /// Included in open; zero for the original borrowed-witness APIs.
    pub witness_release: Duration,
    pub released_witness_elements: usize,
    pub released_witness_capacity: usize,
    /// (folding level, zero-based sumcheck subround).
    pub witness_release_round: Option<(usize, usize)>,
}
#[derive(Debug)]
pub struct HyperFoldPcs<F, E> {
    pub config: HyperFoldConfig,
    pub id: Digest,
    pub base: BaseCodeImpl<F>,
    pub h: LocalMatrix<F>,
    pub diagonals: DiagonalTables<F>,
    _extension: PhantomData<E>,
}
/// Parameters after their final commitment. Only inverse diagonals remain;
/// recommitting or restoring forward storage is not part of this type's API.
///
/// ```compile_fail
/// use hyperfold_pcs::{pcs::OpeningOnlyPcs, field_m127::{Mersenne127, M127Quad}};
/// fn cannot_commit(pcs: OpeningOnlyPcs<Mersenne127, M127Quad>) {
///     pcs.commit(vec![]);
/// }
/// ```
/// ```compile_fail
/// use hyperfold_pcs::{pcs::OpeningOnlyPcs, field_m127::{Mersenne127, M127Quad}};
/// fn cannot_restore(mut pcs: OpeningOnlyPcs<Mersenne127, M127Quad>) {
///     pcs.prepare_commit();
/// }
/// ```
#[derive(Debug)]
pub struct OpeningOnlyPcs<F, E> {
    inner: HyperFoldPcs<F, E>,
}
impl<F: SampleBase, E: ExtensionField<F> + AlgebraIdentity<F>> OpeningOnlyPcs<F, E> {
    /// Allocated forward/inverse diagonal capacities, in base-field elements.
    pub fn diagonal_capacities(&self) -> (usize, usize) {
        (
            self.inner
                .diagonals
                .levels
                .iter()
                .map(|l| l.forward.capacity())
                .sum(),
            self.inner
                .diagonals
                .levels
                .iter()
                .map(|l| l.inverse.capacity())
                .sum(),
        )
    }
    /// Consume only the witness; the caller retains the oracle until after verify.
    ///
    /// ```compile_fail
    /// use hyperfold_pcs::{pcs::{OpeningOnlyPcs, Commitment, CommittedOracle, OwnedWitness},
    ///     field_m127::{Mersenne127 as F, M127Quad as E}};
    /// fn cannot_reuse(pcs: OpeningOnlyPcs<F,E>, cm: Commitment<E>, oracle: CommittedOracle<F>,
    ///                 witness: OwnedWitness<F>, z: Vec<E>, y:E) {
    ///     pcs.open_once(&cm, &oracle, witness, &z, y).unwrap();
    ///     pcs.open_once(&cm, &oracle, witness, &z, y).unwrap();
    /// }
    /// ```
    pub fn open_once(
        &self,
        cm: &Commitment<E>,
        oracle: &CommittedOracle<F>,
        witness: OwnedWitness<F>,
        z: &[E],
        y: E,
    ) -> Result<(Proof<F, E>, Transcript)> {
        self.inner.open_internal(
            cm,
            ProverInput::owned(oracle, witness)?,
            z,
            y,
            FirstTreeStrategy::PrehashedRecovered,
            None,
        )
    }
    pub fn open_once_profiled(
        &self,
        cm: &Commitment<E>,
        oracle: &CommittedOracle<F>,
        witness: OwnedWitness<F>,
        z: &[E],
        y: E,
    ) -> Result<(Proof<F, E>, Transcript, OpenTimings)> {
        let mut timings = OpenTimings::default();
        let (proof, transcript) = self.inner.open_internal(
            cm,
            ProverInput::owned(oracle, witness)?,
            z,
            y,
            FirstTreeStrategy::PrehashedRecovered,
            Some(&mut timings),
        )?;
        Ok((proof, transcript, timings))
    }
    pub fn verify(
        &self,
        cm: &Commitment<E>,
        z: &[E],
        y: E,
        proof: &Proof<F, E>,
    ) -> Result<Transcript> {
        self.inner.verify(cm, z, y, proof)
    }
}
impl<F: SampleBase, E: ExtensionField<F> + AlgebraIdentity<F>> HyperFoldPcs<F, E> {
    pub fn setup(config: HyperFoldConfig, seed: [u8; 32]) -> Result<Self> {
        Self::setup_mode(config, seed, false)
    }
    /// Store one diagonal direction; call prepare_open() after commit().
    pub fn setup_low_memory(config: HyperFoldConfig, seed: [u8; 32]) -> Result<Self> {
        Self::setup_mode(config, seed, true)
    }
    fn setup_mode(config: HyperFoldConfig, seed: [u8; 32], single: bool) -> Result<Self> {
        config.validate()?;
        if config.base_field != F::KIND
            || config.extension_degree != <E as BasedVectorSpace<F>>::DIMENSION
        {
            return Err(Error::Config("field or power basis degree"));
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"hyperfold-public-parameters-v1");
        hasher.update(&config.id());
        hasher.update(&seed);
        hasher.update(&E::algebra_id());
        let id = *hasher.finalize().as_bytes();
        let base = BaseCodeImpl::new(config, &seed)?;
        let h = LocalMatrix::new(config.r);
        let diagonals = if single {
            DiagonalTables::new_single(config, &seed)
        } else {
            DiagonalTables::new(config, &seed)
        };
        Ok(Self {
            config,
            id,
            base,
            h,
            diagonals,
            _extension: PhantomData,
        })
    }
    /// Consume parameters after their last commitment, retaining the inverses
    /// already computed by Setup. This does no resampling or inversion.
    pub fn into_opening_only(mut self) -> Result<OpeningOnlyPcs<F, E>> {
        if !self.diagonals.has_inverse() {
            return Err(Error::Config(
                "opening-only transition requires precomputed inverses",
            ));
        }
        for level in &mut self.diagonals.levels {
            drop(std::mem::take(&mut level.forward));
        }
        Ok(OpeningOnlyPcs { inner: self })
    }
    /// Setup conversion only; benchmark separately from commit/open.
    pub fn prepare_commit(&mut self) {
        self.diagonals.prepare_commit();
    }
    /// Setup conversion only; benchmark separately from commit/open.
    pub fn prepare_open(&mut self) {
        self.diagonals.prepare_open();
    }
    fn commit_prefix(&self, root: Digest) -> (Transcript, Vec<E>) {
        let mut t = Transcript::new(&self.id);
        t.observe(b"top-root", &root);
        let z0 = (0..self.config.mu)
            .map(|_| t.sample::<F, E>(b"commit-z0"))
            .collect();
        (t, z0)
    }
    fn eval_prefix(&self, cm: &Commitment<E>, z: &[E], y: E) -> Result<(Transcript, Vec<E>, E)> {
        if cm.parameters_id != self.id || z.len() != self.config.mu {
            return Err(Error::Proof("statement binding"));
        }
        let (mut t, z0) = self.commit_prefix(cm.root);
        t.observe(b"commit-y0", &cm.y0);
        t.observe(b"open-point", &z);
        t.observe(b"open-value", &y);
        let gamma = t.sample::<F, E>(b"combination-gamma");
        Ok((t, z0, gamma))
    }
    pub fn commit(&self, message: Vec<F>) -> Result<(Commitment<E>, CommitData<F>)> {
        if !self.diagonals.has_forward() {
            return Err(Error::Config("call prepare_commit for forward diagonals"));
        }
        if message.len() != self.config.k() {
            return Err(Error::Config("message length"));
        }
        let pi = encode_top(
            self.config,
            &self.base,
            &self.h,
            &self.diagonals,
            &message,
            self.config.depth(),
        );
        let (root, tree) = mmcs::commit(pi);
        let (_, z0) = self.commit_prefix(root);
        let y0 = sumcheck::evaluate(&message, &z0);
        Ok((
            Commitment {
                parameters_id: self.id,
                root,
                y0,
            },
            CommitData {
                message,
                tree,
                root,
            },
        ))
    }
    pub fn open(
        &self,
        cm: &Commitment<E>,
        data: &CommitData<F>,
        z: &[E],
        y: E,
    ) -> Result<(Proof<F, E>, Transcript)> {
        self.open_with_options(cm, data, z, y, OpenOptions::default())
    }
    pub fn open_with_options(
        &self,
        cm: &Commitment<E>,
        data: &CommitData<F>,
        z: &[E],
        y: E,
        options: OpenOptions,
    ) -> Result<(Proof<F, E>, Transcript)> {
        let strategy = if options.recompute_first_tree {
            FirstTreeStrategy::Rebuild
        } else {
            FirstTreeStrategy::NativeRetained
        };
        self.open_internal(cm, ProverInput::borrowed(data), z, y, strategy, None)
    }
    /// Open with a typed storage strategy, without profiling clocks.
    pub fn open_with_strategy(
        &self,
        cm: &Commitment<E>,
        data: &CommitData<F>,
        z: &[E],
        y: E,
        strategy: FirstTreeStrategy,
    ) -> Result<(Proof<F, E>, Transcript)> {
        self.open_internal(cm, ProverInput::borrowed(data), z, y, strategy, None)
    }
    /// Diagnostics only: all clocks and extraction work remain inside open time.
    pub fn open_profiled(
        &self,
        cm: &Commitment<E>,
        data: &CommitData<F>,
        z: &[E],
        y: E,
        strategy: FirstTreeStrategy,
    ) -> Result<(Proof<F, E>, Transcript, OpenTimings)> {
        let mut timings = OpenTimings::default();
        let (proof, transcript) = self.open_internal(
            cm,
            ProverInput::borrowed(data),
            z,
            y,
            strategy,
            Some(&mut timings),
        )?;
        Ok((proof, transcript, timings))
    }
    fn open_internal(
        &self,
        cm: &Commitment<E>,
        input: ProverInput<'_, F>,
        z: &[E],
        y: E,
        strategy: FirstTreeStrategy,
        mut timings: Option<&mut OpenTimings>,
    ) -> Result<(Proof<F, E>, Transcript)> {
        let ProverInput {
            tree: top_tree,
            root: prover_root,
            mut witness,
        } = input;
        if !self.diagonals.has_inverse() {
            return Err(Error::Config("call prepare_open for inverse diagonals"));
        }
        if cm.root != prover_root || witness.values().len() != self.config.k() {
            return Err(Error::Proof("prover data mismatch"));
        }
        let (mut t, z0, gamma) = self.eval_prefix(cm, z, y)?;
        let config = self.config;
        let d = config.depth();
        let mut weights = sumcheck::combined_weights(&z0, z, gamma);
        let mut f = Vec::<E>::new();
        let mut trees: Vec<Option<Tree<E>>> = Vec::with_capacity(d - 1);
        let mut rounds = Vec::with_capacity(d);
        let mut roots = Vec::with_capacity(d - 1);
        let mut running = cm.y0 + gamma * y;
        let mut first_beta = None;
        let mut first_dense = None;
        let mut recovered_tree = None;
        for i in (1..=d).rev() {
            let stride = config.k0() * (1 << ((i - 1) * config.nu()));
            let mut level = Vec::with_capacity(config.nu());
            let mut alphas = Vec::with_capacity(config.nu());
            for s in 0..config.nu() {
                let first = i == d && s == 0;
                let g = if first {
                    sumcheck::round_values(witness.values(), &weights, stride)
                } else {
                    sumcheck::round_values(&f, &weights, stride)
                };
                if g[0] + g[1] != running {
                    return Err(Error::Proof("incorrect claimed evaluation"));
                }
                t.observe(b"sumcheck-round", &g);
                let a = t.sample::<F, E>(b"sumcheck-alpha");
                running = sumcheck::eval_round(g, a);
                if first {
                    f = sumcheck::fold_table(witness.values(), stride, a);
                    // Only the owning API drops this Vec. All later subrounds use f.
                    if matches!(witness, WitnessSource::Owned(_)) {
                        let start = timings.is_some().then(Instant::now);
                        let (len, capacity) = witness.release_owned();
                        if let Some(timing) = timings.as_mut() {
                            timing.witness_release = start.unwrap().elapsed();
                            timing.released_witness_elements = len;
                            timing.released_witness_capacity = capacity;
                            timing.witness_release_round = Some((i, s));
                        }
                    }
                } else {
                    sumcheck::fold_table_in_place(&mut f, stride, a);
                }
                sumcheck::fold_table_in_place(&mut weights, stride, a);
                level.push(g);
                alphas.push(a);
            }
            rounds.push(level);
            // truncate retains capacity; release it before accumulating another tree.
            weights.shrink_to_fit();
            f.shrink_to_fit();
            if i > 1 {
                let b = beta(&alphas);
                if i == d {
                    first_beta = Some(b);
                }
                let next = if i == d {
                    let matrices = mmcs::mmcs::<F>().get_matrices(top_tree);
                    fold_level(config, &self.h, &self.diagonals, matrices[0], i, &b)
                } else if let Some(matrix) = first_dense.as_ref() {
                    fold_level(config, &self.h, &self.diagonals, matrix, i, &b)
                } else {
                    let matrices = mmcs::mmcs::<E>().get_matrices(
                        trees
                            .last()
                            .and_then(Option::as_ref)
                            .expect("previous tree"),
                    );
                    fold_level(config, &self.h, &self.diagonals, matrices[0], i, &b)
                };
                // The successor matrix is complete; release the sole dense owner
                // before committing the next tree. No descriptor borrows it.
                first_dense = None;
                if strategy == FirstTreeStrategy::Rebuild && trees.len() == 1 {
                    // Its matrix was needed above; drop it before building the second tree.
                    trees[0] = None;
                }
                let first = trees.is_empty();
                let (root, tree) = if first && strategy == FirstTreeStrategy::PrehashedRecovered {
                    let digests = mmcs::hash_leaves(&next);
                    let matrices = mmcs::mmcs::<F>().get_matrices(top_tree);
                    let matrix = RecoveredFirstMatrix::new(
                        config,
                        matrices[0],
                        &self.h,
                        &self.diagonals.levels[d - 1].inverse,
                        b,
                    );
                    let (root, lazy) = mmcs::commit_prehashed(matrix, digests);
                    recovered_tree = Some(lazy);
                    if d >= 3 {
                        first_dense = Some(next);
                    } // d=2: release now.
                    (root, None)
                } else if first && strategy == FirstTreeStrategy::PrehashedRetained {
                    let digests = mmcs::hash_leaves(&next);
                    let (root, tree) = mmcs::commit_prehashed(next, digests);
                    (root, Some(tree))
                } else {
                    let (root, tree) = mmcs::commit(next);
                    (root, Some(tree))
                };
                t.observe(b"folded-root", &root);
                roots.push(root);
                trees.push(tree);
            } else {
                t.observe(b"final-table", &f);
            }
        }
        // Truncation above deliberately reuses allocations; now release their unused
        // capacity before retaining the clear terminal table or opening the trees.
        drop(weights);
        f.shrink_to_fit();
        let queries = draw_queries(&mut t, config);
        let top_indices = project(&queries, config.n(d - 1));
        let top = mmcs::open(top_tree, &top_indices);
        let mut inner: Vec<Option<CompressedOpening<E>>> = (0..d - 1).map(|_| None).collect();
        let compress_opening = |pos: usize, indices: &[usize], o: Opening<E>| {
            let i = d - 1 - pos;
            let np = config.n(i - 1);
            let prev = project(&queries, config.n(i));
            let mut rows = o.rows;
            for (j, row) in indices.iter().zip(&mut rows) {
                let p = *prev
                    .iter()
                    .find(|&&p| p % np == *j)
                    .expect("projected query");
                row.remove(p / np);
            }
            CompressedOpening {
                rows,
                paths: o.paths,
            }
        };
        let mut rebuild = false;
        // Consume retained trees first. Rebuilding earlier would recreate the original
        // peak while all smaller trees were still live.
        for (pos, tree) in trees.into_iter().enumerate() {
            if let Some(tree) = tree {
                let start = (pos == 0 && timings.is_some()).then(Instant::now);
                let indices = project(&queries, config.n(d - 2 - pos));
                inner[pos] = Some(compress_opening(pos, &indices, mmcs::open(&tree, &indices)));
                if let Some(start) = start {
                    timings.as_mut().unwrap().first_tree_query = start.elapsed();
                }
            } else {
                assert_eq!(pos, 0);
                rebuild = recovered_tree.is_none();
            }
        }
        if rebuild {
            let matrices = mmcs::mmcs::<F>().get_matrices(top_tree);
            let next = fold_level(
                config,
                &self.h,
                &self.diagonals,
                matrices[0],
                d,
                &first_beta.expect("first folding challenges"),
            );
            let (root, tree) = mmcs::commit(next);
            if root != roots[0] {
                return Err(Error::Proof("reconstructed tree root"));
            }
            let start = timings.is_some().then(Instant::now);
            let indices = project(&queries, config.n(d - 2));
            inner[0] = Some(compress_opening(0, &indices, mmcs::open(&tree, &indices)));
            if let Some(start) = start {
                timings.as_mut().unwrap().first_tree_query = start.elapsed();
            }
        }
        if let Some(tree) = recovered_tree {
            let start = timings.is_some().then(Instant::now);
            let indices = project(&queries, config.n(d - 2));
            inner[0] = Some(compress_opening(0, &indices, mmcs::open(&tree, &indices)));
            if let Some(start) = start {
                timings.as_mut().unwrap().first_tree_query = start.elapsed();
            }
        }
        let inner = inner
            .into_iter()
            .map(|o| o.expect("opened intermediate tree"))
            .collect();
        Ok((
            Proof {
                rounds,
                roots,
                final_table: f,
                top,
                inner,
            },
            t,
        ))
    }
    pub fn verify(
        &self,
        cm: &Commitment<E>,
        z: &[E],
        y: E,
        proof: &Proof<F, E>,
    ) -> Result<Transcript> {
        if !self.diagonals.has_inverse() {
            return Err(Error::Config("call prepare_open for inverse diagonals"));
        }
        let config = self.config;
        let d = config.depth();
        if proof.rounds.len() != d
            || proof.rounds.iter().any(|r| r.len() != config.nu())
            || proof.roots.len() != d - 1
            || proof.inner.len() != d - 1
            || proof.final_table.len() != config.k0()
        {
            return Err(Error::Proof("proof shape"));
        }
        let (mut t, z0, gamma) = self.eval_prefix(cm, z, y)?;
        let mut running = cm.y0 + gamma * y;
        let mut betas = Vec::with_capacity(d);
        let mut alpha_coords = vec![E::ZERO; config.mu];
        for pos in 0..d {
            let i = d - pos;
            let mut alpha = Vec::with_capacity(config.nu());
            for (s, &g) in proof.rounds[pos].iter().enumerate() {
                if g[0] + g[1] != running {
                    return Err(Error::Proof("sumcheck consistency"));
                }
                t.observe(b"sumcheck-round", &g);
                let a = t.sample::<F, E>(b"sumcheck-alpha");
                running = sumcheck::eval_round(g, a);
                alpha_coords[config.mu0 + (i - 1) * config.nu() + s] = a;
                alpha.push(a);
            }
            betas.push(beta(&alpha));
            if i > 1 {
                t.observe(b"folded-root", &proof.roots[pos]);
            } else {
                t.observe(b"final-table", &proof.final_table);
            }
        }
        if sumcheck::final_weighted_sum(
            &proof.final_table,
            &z0,
            z,
            &alpha_coords,
            config.mu0,
            gamma,
        ) != running
        {
            return Err(Error::Proof("terminal sumcheck"));
        }
        let mut pi0 = vec![E::ZERO; config.n(0)];
        self.base.encode_slice(&proof.final_table, &mut pi0);
        let queries = draw_queries(&mut t, config);
        let top_indices = project(&queries, config.n(d - 1));
        mmcs::verify(
            cm.root,
            config.n(d - 1),
            config.r,
            &top_indices,
            &proof.top.rows,
            &proof.top.paths,
        )?;
        let diag = &self.diagonals.levels[d - 1].inverse;
        let mut expected: Vec<(usize, E)> = top_indices
            .iter()
            .zip(&proof.top.rows)
            .map(|(&j, row)| {
                (
                    j,
                    fold_row(
                        &self.h,
                        &diag[j * (config.r - 1)..(j + 1) * (config.r - 1)],
                        &betas[0],
                        row,
                    ),
                )
            })
            .collect();
        for pos in 0..d - 1 {
            let i = d - 1 - pos;
            let np = config.n(i - 1);
            let indices = project(&queries, np);
            let compressed = &proof.inner[pos];
            if compressed.rows.len() != indices.len()
                || compressed.rows.iter().any(|r| r.len() != config.r - 1)
            {
                return Err(Error::Proof("compressed row shape"));
            }
            let mut rows = Vec::with_capacity(indices.len());
            for (&j, short) in indices.iter().zip(&compressed.rows) {
                let &(p, v) = expected
                    .iter()
                    .find(|(p, _)| p % np == j)
                    .ok_or(Error::Proof("missing parent query"))?;
                let mut row = short.clone();
                row.insert(p / np, v);
                rows.push(row);
            }
            mmcs::verify(
                proof.roots[pos],
                np,
                config.r,
                &indices,
                &rows,
                &compressed.paths,
            )?;
            for &(p, v) in &expected {
                let slot = indices
                    .binary_search(&(p % np))
                    .map_err(|_| Error::Proof("projected query"))?;
                if rows[slot][p / np] != v {
                    return Err(Error::Proof("folded coordinate consistency"));
                }
            }
            let diag = &self.diagonals.levels[i - 1].inverse;
            expected = indices
                .iter()
                .zip(&rows)
                .map(|(&j, row)| {
                    (
                        j,
                        fold_row(
                            &self.h,
                            &diag[j * (config.r - 1)..(j + 1) * (config.r - 1)],
                            &betas[pos + 1],
                            row,
                        ),
                    )
                })
                .collect();
        }
        for (j, v) in expected {
            if pi0[j] != v {
                return Err(Error::Proof("terminal codeword fold"));
            }
        }
        Ok(t)
    }
}
fn draw_queries(t: &mut Transcript, c: HyperFoldConfig) -> Vec<usize> {
    (0..c.num_queries)
        .map(|_| t.index(c.n(c.depth() - 1).ilog2() as usize))
        .collect()
}
fn project(q: &[usize], n: usize) -> Vec<usize> {
    let mut x: Vec<usize> = q.iter().map(|q| q % n).collect();
    x.sort_unstable();
    x.dedup();
    x
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::field_m127::{M127Quad, Mersenne127};
    use crate::params::BaseCodeKind;
    use p3_field::PrimeCharacteristicRing;
    #[test]
    fn opening_only_moves_allocations_and_retains_identical_inverses() {
        type F = Mersenne127;
        type E = M127Quad;
        let cfg = HyperFoldConfig::testing(F::KIND, BaseCodeKind::Qa, 8, 2, 4, 4, 2).unwrap();
        let pcs = HyperFoldPcs::<F, E>::setup(cfg, [123; 32]).unwrap();
        let inverses: Vec<_> = pcs
            .diagonals
            .levels
            .iter()
            .map(|l| l.inverse.clone())
            .collect();
        let message: Vec<_> = (0..cfg.k()).map(F::from_usize).collect();
        let pointer = message.as_ptr();
        let (_, data) = pcs.commit(message).unwrap();
        let top_pointer = mmcs::mmcs::<F>().get_matrices(&data.tree)[0]
            .values
            .as_ptr();
        let (oracle, witness) = data.into_parts();
        assert_eq!(pointer, witness.values().as_ptr());
        assert_eq!(
            top_pointer,
            mmcs::mmcs::<F>().get_matrices(&oracle.tree)[0]
                .values
                .as_ptr()
        );
        let only = pcs.into_opening_only().unwrap();
        for (level, inverse) in only.inner.diagonals.levels.iter().zip(inverses) {
            assert_eq!(level.forward.len(), 0);
            assert_eq!(level.forward.capacity(), 0);
            assert_eq!(level.inverse, inverse);
        }
        assert!(only.inner.diagonals.has_inverse());
        assert!(!only.inner.diagonals.has_forward());
        let low = HyperFoldPcs::<F, E>::setup_low_memory(cfg, [123; 32]).unwrap();
        assert!(low.into_opening_only().is_err());
    }
    #[test]
    fn owning_witness_release_deallocates_and_error_paths_drop_owned_values() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Tracked(Arc<AtomicUsize>);
        impl Drop for Tracked {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let mut values = Vec::with_capacity(16);
        for _ in 0..8 {
            values.push(Tracked(drops.clone()));
        }
        let mut source = WitnessSource::Owned(values);
        assert_eq!(source.release_owned(), (8, 16));
        assert_eq!(drops.load(Ordering::Relaxed), 8);
        match source {
            WitnessSource::Owned(values) => assert_eq!(values.capacity(), 0),
            _ => unreachable!(),
        }
        let error = || -> Result<()> {
            let _input = WitnessSource::Owned(vec![Tracked(drops.clone()), Tracked(drops.clone())]);
            Err(Error::Proof("intentional early failure"))
        };
        assert!(error().is_err());
        assert_eq!(drops.load(Ordering::Relaxed), 10);
        let original = [17, 23];
        let mut borrowed = WitnessSource::Borrowed(&original);
        assert_eq!(borrowed.release_owned(), (0, 0));
        assert_eq!(borrowed.values(), &original);
    }
}
