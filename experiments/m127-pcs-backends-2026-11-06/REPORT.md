# Native M127 PCS representative-point comparison

Experiment role: formal_representative_comparison.

Validated 36 independent processes and 108 formal trials. One warmup + 3 measured trials per process; fresh Setup per trial,32 threads, same physical-core affinity. All signatures and certified main-profile parameters agree across backends.

| mu | code | field backend | commit (ms) | open (ms) | verify (ms) | online total (ms) | full wall (ms) | process peak RSS mean (GiB) | process peak RSS max (GiB) | proof bytes |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 20 | qa | ark_adapter | 176.2557 | 291.0548 | 6.8303 | 475.9367 | 637.2326 | 0.4894 | 0.4895 | 665798 |
| 20 | qa | fourq_native_adapter | 198.4947 | 279.2344 | 6.9024 | 486.1494 | 644.3138 | 0.4881 | 0.4927 | 665798 |
| 20 | qa | ours_frozen | 157.8406 | 229.8448 | 6.8906 | 396.3333 | 551.7190 | 0.4879 | 0.4886 | 665798 |
| 20 | rs | ark_adapter | 217.4378 | 264.7983 | 6.3815 | 490.1348 | 650.6449 | 0.4887 | 0.4898 | 600511 |
| 20 | rs | fourq_native_adapter | 271.5092 | 272.2773 | 6.6699 | 552.2261 | 713.3217 | 0.4889 | 0.4901 | 600511 |
| 20 | rs | ours_frozen | 191.5319 | 220.9245 | 6.1386 | 420.3591 | 578.6503 | 0.4895 | 0.5004 | 600511 |
| 26 | qa | ark_adapter | 9770.1464 | 13301.6275 | 14.2236 | 23382.1087 | 34676.7693 | 27.9961 | 27.9993 | 1511466 |
| 26 | qa | fourq_native_adapter | 10972.3311 | 13438.4195 | 14.2866 | 24719.8916 | 35809.5482 | 27.9974 | 28.0035 | 1511466 |
| 26 | qa | ours_frozen | 9481.7025 | 10481.0676 | 13.9250 | 20272.5470 | 31257.8785 | 28.0013 | 28.0036 | 1511466 |
| 26 | rs | ark_adapter | 13288.8021 | 13307.4096 | 12.9448 | 26904.8300 | 38114.5154 | 27.9988 | 28.0022 | 1329650 |
| 26 | rs | fourq_native_adapter | 16432.5887 | 13495.9456 | 12.9534 | 30237.8916 | 41497.6324 | 28.0013 | 28.0028 | 1329650 |
| 26 | rs | ours_frozen | 11941.7461 | 10741.0918 | 12.3010 | 22991.4612 | 34120.7513 | 27.9993 | 28.0043 | 1329650 |

Across-process sample SD and paired-round differences/ranges are in the CSV files. Within-process repeats are not additional independent process rounds. These are descriptive shared-host measurements, not confidence intervals or guarantees.

All three paths use opening-only parameters, first-subround witness release and first-tree queried-row recovery. Online total includes forward-table release and open includes witness release/query work. Full wall also includes fresh Setup, input/evaluation, serialization, diagnostics and final cleanup. Peak RSS covers the complete process and warmup. Historical cached-Setup and failed retained experiments are separate and unchanged.

Ark stays Montgomery internally and canonical conversion is included at actual protocol boundaries. FourQ uses audited redundant [0,p] x64 UINT128 C arithmetic with native base/E operations and thin scalar FFI, whose overhead is included. No cross-language LTO or bulk fusion; results characterize this integration, not every possible implementation. No backend has been selected for production by this report. Correctness tests and timing are not security proofs.

Source commit: c587b844072d1ff24260a976d6f4a224a77d995f; binary SHA256: dac23d2cd9e97f0d2122b9be7a8e40f451e8238402ae841552e0117cf796c637.
