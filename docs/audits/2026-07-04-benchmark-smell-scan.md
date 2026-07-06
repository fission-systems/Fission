# Benchmark Smell Scan

- Repo: `/Users/sjkim1127/Fission`
- Findings: `99`

## Findings

| Severity | Kind | Location | Token | Detail |
|---|---|---|---|---|
| warning | `unexplained_numeric_threshold` | `crates/fission-ai/examples/analyze_pseudocode.rs:10` | `42` | Numeric threshold in a branch may need invariant documentation. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/execution.rs:65` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/execution.rs:126` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/execution.rs:180` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/execution.rs:323` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/execution.rs:678` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/execution.rs:733` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/execution.rs:788` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-ai/src/tools/mod.rs:87` | `0x14000000` | PE-style benchmark address appears in scanned source. |
| medium | `corpus_path_or_manifest` | `crates/fission-automation/AGENTS.md:10` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `crates/fission-automation/README.md:12` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `crates/fission-automation/config/sentinel_sets.toml:29` | `benchmark/binary/` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `crates/fission-automation/config/sentinel_sets.toml:32` | `benchmark/binary/` | Benchmark corpus path or manifest token appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-automation/config/timeout_rescue.json:3` | `0x140006380` | PE-style benchmark address appears in scanned source. |
| warning | `unexplained_numeric_threshold` | `crates/fission-cli/src/cli/ai.rs:217` | `0` | Numeric threshold in a branch may need invariant documentation. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/decomp.rs:13` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/inventory.rs:26` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/mod.rs:258` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/mod.rs:283` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/mod.rs:312` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/mod.rs:341` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/mod.rs:370` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/args/mod.rs:416` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/oneshot/mod.rs:489` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/oneshot/mod.rs:490` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/oneshot/mod.rs:491` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/oneshot/mod.rs:492` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/oneshot/mod.rs:493` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `crates/fission-cli/src/cli/oneshot/mod.rs:494` | `0x140001000` | PE-style benchmark address appears in scanned source. |
| warning | `unexplained_numeric_threshold` | `crates/fission-dynamic/src/debug/platform/windows/debugger/pe.rs:181` | `63` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-dynamic/src/debug/platform/windows/process_dump.rs:266` | `63` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-loader/src/loader/containers/mod.rs:128` | `3` | Numeric threshold in a branch may need invariant documentation. |
| high | `benchmark_function_name` | `crates/fission-loader/src/loader/formats/hex.rs:245` | `checksum` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `crates/fission-loader/src/loader/formats/hex.rs:253` | `checksum` | Benchmark corpus function name appears in non-test code. |
| warning | `unexplained_numeric_threshold` | `crates/fission-pcode/src/nir/builder/materialize/incremental.rs:178` | `3` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-pcode/src/nir/structuring/driver/mod.rs:427` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-pcode/src/nir/structuring/driver/mod.rs:466` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-pcode/src/nir/structuring/linear/mod.rs:104` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-pcode/src/nir/structuring/linear/mod.rs:113` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-pcode/src/nir/structuring/linear/recovery.rs:196` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-pcode/src/nir/structuring/linear/recovery.rs:223` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-signatures/src/fid/x86_decoder.rs:128` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-sleigh/src/runtime/spine/compiled_table/selection.rs:323` | `4` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-sleigh/src/runtime/spine/compiled_table/selection.rs:332` | `4` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-sleigh/src/runtime/spine/compiled_table/template_eval.rs:1248` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-static/benches/benchmark.rs:131` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-static/benches/benchmark.rs:143` | `0` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-static/src/analysis/function_discovery/discover.rs:966` | `4` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-static/src/analysis/optimizer/README.md:25` | `5` | Numeric threshold in a branch may need invariant documentation. |
| warning | `unexplained_numeric_threshold` | `crates/fission-static/src/analysis/optimizer/README.md:26` | `5` | Numeric threshold in a branch may need invariant documentation. |
| medium | `corpus_path_or_manifest` | `scripts/benchmark/setup.sh:22` | `benchmark/binary/` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/corpus/collect_github_release_samples.py:6` | `benchmark/binary/` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/corpus/collect_github_release_samples.py:29` | `benchmark/binary/` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/corpus/hash_and_manifest.py:26` | `benchmark/binary/` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/test/no_benchmark_fixture_refs_in_crates.py:12` | `benchmark/binary/` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/test/no_benchmark_fixture_refs_in_crates.py:15` | `canonical_rows.json` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/test/no_benchmark_fixture_refs_in_crates.py:16` | `smoke_corpus.json` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/test/no_benchmark_fixture_refs_in_crates.py:17` | `release_corpus.json` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `scripts/test/no_benchmark_fixture_refs_in_crates.py:18` | `parity_corpus.json` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:11` | `corpus/dev` | Benchmark corpus path or manifest token appears in scanned source. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:12` | `checksum` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_like_address` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:13` | `0x1400015b4` | PE-style benchmark address appears in scanned source. |
| medium | `corpus_path_or_manifest` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:14` | `fission-benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:19` | `crc32` | Benchmark corpus function name appears in non-test code. |
| medium | `corpus_path_or_manifest` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:22` | `corpus/dev` | Benchmark corpus path or manifest token appears in scanned source. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:23` | `rc4_init` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:23` | `rc4_crypt` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:23` | `crc32` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_like_address` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:24` | `0x140001530` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:24` | `0x140001624` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_like_address` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:24` | `0x140001745` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:44` | `checksum` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:49` | `crc32` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:53` | `rc4_init` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:53` | `rc4_crypt` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:75` | `checksum` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:76` | `crc32` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:102` | `checksum` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:103` | `crc32` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:103` | `rc4_init` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:103` | `rc4_crypt` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:104` | `checksum` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:105` | `crc32` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:106` | `rc4_init` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:106` | `rc4_crypt` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:113` | `count_bits` | Benchmark corpus function name appears in non-test code. |
| medium | `corpus_path_or_manifest` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:117` | `corpus/dev` | Benchmark corpus path or manifest token appears in scanned source. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:118` | `count_bits` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_like_address` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:119` | `0x140001530` | PE-style benchmark address appears in scanned source. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:159` | `count_bits` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:160` | `count_bits` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:184` | `count_bits` | Benchmark corpus function name appears in non-test code. |
| high | `benchmark_function_name` | `docs/proposals/2026-07-03-fission-0.1.2-decompiler-quality.md:186` | `count_bits` | Benchmark corpus function name appears in non-test code. |
| medium | `corpus_path_or_manifest` | `AGENTS.md:18` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `AGENTS.md:20` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `AGENTS.md:59` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `AGENTS.md:108` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `AGENTS.md:118` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |
| medium | `corpus_path_or_manifest` | `AGENTS.md:211` | `source_semantic_benchmark` | Benchmark corpus path or manifest token appears in scanned source. |

## Recent Heuristic-Labeled Commits

| Date | Commit | Subject |
|---|---|---|
| 2026-07-03 | `6c31adf3eedf` | Add decompiler quality anti-overfit guardrail |
| 2026-07-01 | `1ba05f057a95` | arch: anti-overfitting pass-gate architecture for NIR structuring pipeline |
| 2026-06-30 | `adf7427d6087` | chore: major repo cleanup — LFS, benchmark/, vendor MANIFEST |
| 2026-06-04 | `0cba9aed43c9` | cspec: Ghidra-style .ldefs-based exact (language_id, compiler_spec_id) → cspec resolution |
| 2026-05-05 | `9e45d740ca6b` | feat(core): ResourceProvider, EvidencePolicy, and resource path guard |
| 2026-05-05 | `8a481bf708ea` | feat(loader): identity Phase 2 scoring, DIE raw vs promoted, CLI summary |
| 2026-05-01 | `5d18db4c0d28` | Revert heuristic SLEIGH canonical path purge |
| 2026-05-01 | `ef8f36463aa3` | Purge heuristic SLEIGH canonical paths |
| 2026-04-30 | `feaf3757b428` | feat(fission-sleigh): Ghidra Sleigh 5-gap closure (cleanroom algorithm implementation) |
| 2026-04-30 | `a82756cd19c9` | feat(sleigh): 100% canonical parity — BRANCH/BRANCHIND/CALLIND SLA opcode alignment |
| 2026-04-22 | `eec748164e97` | feat: add comprehensive performance benchmarking dashboard with criterion.rs integration and medium binary samples |
| 2026-04-09 | `04a2e38211dd` | feat(pcode): HIR expressiveness phase 3 — EFLAGS recovery, prologue elimination, Cooper postdom structuring |
| 2026-04-01 | `bd73e412cb8e` | feat(nir): Implemented algorithmic For loop structuring and Post-Dom Region Recovery traversal |
| 2026-03-23 | `fdae0c11cb32` | Make follow discovery index-order independent for anti-overfit |
| 2026-03-03 | `deac984c175a` | feat: implement 8-gap decompiler improvements vs Ghidra |
| 2026-02-25 | `be19a1dc583d` | improvement: Track 2/3/4 + benchmark normalization (x86 80% → 90.1%) |
| 2026-02-24 | `b38ef4d317a4` | arch: fix x86/PE hardcoding in decompiler engine (7 fixes) |
| 2026-02-24 | `9b9dc2107043` | feat(bench): add compare_decompilers_v3.py + suite_example.yaml |
| 2026-02-16 | `e7d3d0735b20` | feat(decomp): expand native feature controls and add quality workflow gate |
| 2026-01-08 | `3f656f352609` | feat: Achieve 97.86% decompiler similarity with Ghidra |
