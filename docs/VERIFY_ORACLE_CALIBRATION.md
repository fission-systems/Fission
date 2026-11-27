# Verification Oracle Calibration & Dependency Matrix

Because no single verifier in `fission-dir` is a universal correctness oracle
([ADR 0017](adr/0017-ghidra-is-not-an-oracle.md),
[ADR 0018](adr/0018-decbench-is-not-a-correctness-oracle.md)), Fission tracks
shared dependencies across tiers explicitly and calibrates every comparison
boundary with injected faults (#1254).

## Oracle Dependency Matrix

| Lane | ISA Spec | Lifter | Evaluator | Comparator / Scope | Independent of Fission Lift | Common-Mode Blind Spots |
|---|---|---|---|---|---|---|
| `concrete_prehir_hir` | Shared (`cspec`/SLEIGH) | Shared (`PreHIR -> HIR`) | Shared (`fission-dir/src/eval.rs`) | `diff_prehir_hir_detailed` over sampled inputs (`proved == false`) | No | Shared wrong lift or shared wrong eval; non-sampled inputs |
| `ground_truth_emulator` | Shared (`cspec`/SLEIGH) | Emulator JIT/interp vs PreHIR/HIR | `fission-emulator` vs `eval.rs` | `check_ground_truth_with` over `InitialState` runs | No | Shared SLEIGH defects; host/hardware differences outside HLE |
| `symbolic_prehir_hir` | Shared (`cspec`/SLEIGH) | Shared (`PreHIR/HIR -> lower_sym`) | `fission-solver` + differential `z3` | `check_symbolic_equivalence` over modeled acyclic domain | No | Shared wrong PreHIR/HIR semantics; loops/unmodeled calls/memory |
| `native_replay_fixture` | Independent host compiler/ABI | Compiled machine binary vs emulator/HIR/Python | Host CPU vs emulator vs `run_hir` vs `python3` | `scripts/test/replay_fixture.py` `classify_observation` | Yes | Limited to fixed fixture functions and sampled inputs; requires fresh witness identity |

## Calibration Controls

- **Python replay / harness matrix (offline on any OS):**
  ```bash
  python3 scripts/test/replay_fixture.py --calibrate
  python3 -m unittest discover -s scripts/test -p 'test_*.py' -v
  ```
  Exercises positive agreement (`proved=false`), shared wrong lift (`emulator == hir != native`),
  HIR value/store/outside-write/Python-export corruption, missing observation, truncated trace,
  asymmetric timeout, native harness failure (even when `emulator == hir`), and stale witnesses
  (`sleigh_manifest_sha256`, `fission_revision`, `fixture_sha256`, `source_sha256`, `abi`,
  `compiler_flags`).

- **Rust comparator & CLI serialization matrix:**
  - `crates/fission-dir/tests/oracle_calibration.rs`
  - `verify_fields_tests` in `crates/fission-cli/src/cli/oneshot/mod.rs`
