# Memory/API and benchmark migration

The sole official lifecycle is standard two-direction Setup → final commit →
opening-only parameters (drop forward) → consume witness after its first lifted
fold → first-tree hashes plus queried-row recovery → verify/wire → caller oracle
and parameter cleanup. This is the already tested release/query-recovery strategy;
this cleanup changes no field arithmetic, parameter certificate or wire protocol.

The research API intentionally removes obsolete policy selectors. Existing
`HyperFoldPcs::commit`, borrowed repeated `open`, `verify`, `CommitData::into_parts`,
`OpeningOnlyPcs` and consuming `open_once[_profiled]` retain their ownership meaning.
A borrowed call does not silently consume its witness. Formal measurements use
only the consuming API and fresh Setup per trial.

| Removed active symbol/entry | Current replacement | Retained verification/history |
|---|---|---|
| `OpenOptions`, `FirstTreeStrategy`, `open_with_options`, `open_with_strategy`, borrowed `open_profiled` | `open` for borrowed use; `open_once[_profiled]` for owning use; both recover queried rows |96 frozen native references, owned/borrowed equivalence, P/V and tampering |
| Whole first-tree rebuild and native/prehashed full-retention selection | One hash-preserving row descriptor; dense owner freed after successor fold |MMCS native/prehashed tests, row-fold naive checks, old source/archives |
| `setup_low_memory`, `new_single`, `prepare_commit/open`, in-place direction inversion | Standard Setup computes both tables; `into_opening_only` frees forward once |Inverse checks, pointer/capacity/tracked-drop tests and compile-fail ownership |
| `resources::estimated_gib`, auto/low/retained switches and ceiling skips | Actual GNU time/process/host resource observations, no memory threshold |Old retained/unguarded/failure evidence unchanged |
| `--resume`, `--quick`, `--memory-mode`, `--max-estimated-gib`, `--first-tree-strategy` | New schema, new output paths; explicit certified selections and labelled smoke |CLI rejects old flags, preserves existing summary/raw files |
| Old recompute/query-row/one-shot comparison examples | Current `benchmark::run_trial`, CLI and Criterion |Pinned source commits and `experiments/` archives |
| `run_sweep`, `attempt_retained_extremes`, `attempt_query_extremes`, `profile_recompute_overhead`, `profile_query_rows`, `run_one_shot_lifetimes` launchers | `run_unified.py`; neutral `benchmark_evidence.py` |Immutable-contract regression tests and portable archived analyses |

The frozen source before cleanup is commit7bd0724, with experiment-specific older
source/binary hashes in each manifest. Old comparison code remains reproducible
from those revisions and raw archives, not from live fallback branches. Inert
fixture generator/provenance and mathematical reference tests remain in the tree.
The independent native-backend comparison package is retained as a useful active
comparison tool, with its completed results tied to frozen c587b84.

`schema_version=hyperfold-one-shot-bench-v1` records every phase, release markers,
canonical fingerprints and proof bytes. A full measured case requires all requested
trials to finish; failed/partial aggregate timing/size is blank. Raw partial files
and exit/resource evidence remain. New root/output paths are required, with no
silent resume or substitution of historical successes.

Only32-thread timings run. Functional tests cover1/4/32 threads. The cleanup is
validated separately from the earlier timing datasets; compiling it does not
create new measurements of the largest cases or a new72-case sweep. Standard
Setup and all original matrix/encoding/sumcheck/verification algorithms remain.
