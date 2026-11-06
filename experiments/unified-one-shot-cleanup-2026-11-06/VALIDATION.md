# Cleanup validation

The sole official runtime performs fresh Setup, commits, releases the forward
allocation, consumes the message after its first complete lifted fold, recovers
queried rows, verifies and serializes, then cleans up caller-owned state.
Obsolete whole-tree reconstruction, table-direction toggles, estimator policies
and old diagnostic launchers are removed.

Ordinary and native root suites each pass 33 runtime tests and 3 compile-fail
checks. Both Clippy builds and documentation succeed. Native field-adapter
suites each pass 5 tests after the API cleanup. Python evidence and failure
regressions pass 11 checks, including child-process cleanup and stopped-run
archive reconstruction. Those failure fixtures are synthetic, not PCS
performance data.

The 96 frozen old-native commitment/proof/challenge references match under
1 and 32 functional threads (192 checks). The final native CLI binary completed
24 mu=20 processes across both fields, QA/RS, r=2/4/8 and c=2/4, each with one
warmup and one measured functional trial. Its two CSVs reproduce byte-for-byte
from the raw archive. All 24 parameter, commitment, proof, challenge and byte-size
fingerprints equal the pre-deletion smoke. Production field arithmetic is
byte-identical to frozen source c587b84.

Execution source and binary are pinned in manifest.json. Controller failure
fixes and postprocessing are separately versioned. This smoke checks functional
behavior; its timings do not support a speed-change claim. Historical datasets
remain separate and immutable. This cleanup has not rerun the largest cases or
a 72-case performance sweep. See docs/memory-migration.md for removed selectors
and historical source references.
