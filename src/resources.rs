//! Principal allocation model, not a measured RSS bound. The guard adds headroom
//! for allocator metadata, hashing scratch, small proofs and transient reallocs.
use crate::{
    mmcs,
    params::{BaseFieldKind, HyperFoldConfig},
};
use p3_goldilocks::Goldilocks;

pub fn estimated_gib(c: HyperFoldConfig, low_memory: bool) -> f64 {
    let f = if c.base_field == BaseFieldKind::Goldilocks {
        8usize
    } else {
        16
    };
    let e = f * c.extension_degree;
    let d = c.depth();
    let n = c.n(d);
    let k = c.k();
    let diag = (n - c.n(0)) * f * if low_memory { 1 } else { 2 };
    let tree = |i: usize, element: usize| {
        let values = c.n(i);
        // Same binary-tree geometry for both fields, including leaf digests and root.
        let digests = mmcs::mmcs::<Goldilocks>()
            .digest_storage(&[values / c.r])
            .expect("validated power-of-two geometry")
            .digest_bytes;
        values * element + digests
    };
    let fixed = f * k + diag + tree(d, f);
    let mut peak = fixed.max(f * k + diag + 2 * f * n);
    let mut retained = 0usize;
    for i in (1..=d).rev() {
        let len = c.k0() * (1 << (i * c.nu()));
        let folded = len / c.r;
        let f_cap = if i == d { len / 2 } else { len };
        // Round storage and a conservative moving shrink_to_fit allocation.
        peak = peak.max(fixed + retained + e * (len + f_cap + folded));
        peak = peak.max(fixed + retained + e * (f_cap + 2 * folded));
        if i > 1 {
            peak = peak.max(fixed + retained + e * (c.n(i - 1) + 2 * folded));
            if low_memory && i == d - 1 {
                retained -= tree(d - 1, e);
            }
            retained += tree(i - 1, e);
            peak = peak.max(fixed + retained + 2 * e * folded);
        }
    }
    if low_memory && d >= 3 {
        peak = peak.max(fixed + tree(d - 1, e) + e * c.k0());
    }
    1.05 * peak as f64 / (1u64 << 30) as f64 + 0.5
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{BaseCodeKind, CERTIFIED_CONFIGS};
    #[test]
    fn extreme_configs_account_for_retained_digests_and_rebuild_peak() {
        let c =
            HyperFoldConfig::experiment(BaseFieldKind::M127, BaseCodeKind::Rs, 30, 2, 4).unwrap();
        assert!(estimated_gib(c, false) > 590.0);
        assert!(estimated_gib(c, true) > 448.0); // includes transient second matrix before eviction
        assert!(estimated_gib(c, true) < 480.0);
        for &c in CERTIFIED_CONFIGS {
            assert!(estimated_gib(c, true) < 480.0);
            assert!(estimated_gib(c, true) <= estimated_gib(c, false));
        }
    }
}
