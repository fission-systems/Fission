# Troubleshooting and glossary

Moved out of `README.md`. Common local failures and the vocabulary the
codebase and its docs assume.

## Troubleshooting

| Symptom | First check |
|---|---|
| Missing Sleigh specs | `utils/` is committed — a full checkout has them. Check for a sparse/filtered clone, then check resource status. |
| CLI cannot find resources | Check `FISSION_RESOURCE_ROOT`, `--resource-root`, and `PathConfig::detect` behavior. |
| Raw p-code mismatch | Start in `fission-sleigh` before interpreting NIR output. |
| NIR is wrong but p-code is right | Investigate NIR materialization, normalization, or type hint application. |
| HIR is unreadable but NIR is right | Investigate structuring, cleanup, or printer consume behavior. |
| Report counters disagree | Trace the counter back to `NirBuildStats`. |
| Loader identifies unsupported container | Extract an executable child explicitly instead of raw-loading the container. |
| A test passes locally but CI fails | Check LFS pulls, OS-specific paths, feature flags, and reusable workflow inputs. |

## Glossary

| Term | Meaning |
|---|---|
| CFG | Control-flow graph. |
| HIR | High-level intermediate representation for readable pseudocode. |
| NIR | Normalized intermediate representation with strict semantic requirements. |
| P-code | Ghidra-style low-level instruction semantics representation. |
| Sleigh | Language specification system used for instruction decode and semantics. |
| Dominance | Graph relation used to reason about control-flow ownership. |
| Post-dominance | Graph relation used to reason about exits and structured regions. |
| SCC | Strongly connected component, often used for loop analysis. |
| RegionProof | Evidence that a region can be safely promoted during structuring. |
| NirBuildStats | Canonical NIR telemetry contract. |
| FactStore | Aggregated facts and provenance consumed by decompilation contexts. |
| FID | Function identification through signatures. |
| LFS | Git Large File Storage. |
| Taint | A label on data indicating it originated from a symbolic (untrusted) source. |
| Shadow Memory | Parallel memory map tracking symbolic AST node IDs alongside concrete bytes. |
| Concolic | Execution that combines concrete runs with symbolic state to explore multiple paths. |
| HLE | High-Level Emulation: OS syscall interception without full kernel emulation. |
| TTD | Time-Travel Debugging: record/replay execution via memory and register snapshots. |
| SymExpr | Symbolic expression AST node in `fission-solver`. |
| SAT | Boolean satisfiability problem. Used to check if a path constraint can be fulfilled. |

## Roadmap

- Improve x86 and x86-64 pseudocode quality on small sample binaries first.
- Continue strengthening control-flow recovery for if, else, switch, loop, break, and continue structures.
- Improve pointer, array, struct, and field-access expression recovery.
- Improve calling convention, parameter, local-variable, return-value, accumulator, and induction-variable cleanup.
- Maintain raw p-code parity gates for Sleigh changes.
- Improve FID and name recovery relative to signature ecosystems.
- Expand architecture and file-format breadth after x86/x86-64 quality is strong enough.
- Expand the pure-Rust symbolic execution engine: implement DPLL/CDCL bit-blasting in `fission-solver`.
- Add more P-Code taint propagation opcodes in `fission-emulator`.
- Connect taint-tracking results to decompiler type recovery.
- Continue the normalize `action_pipeline`/`ActionGroup` migration stage by stage (see `PROJECT.md` for current status and backlog); each stage's imperative `run_stage_*` free function is replaced with declarative passes, validated with a real-binary before/after check.

