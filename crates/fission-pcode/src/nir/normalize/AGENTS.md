# Normalize Area Guide

Scope: `crates/fission-pcode/src/nir/normalize/`

## Directory map

| Directory | Role |
|-----------|------|
| `arith/` | Integer/boolean arithmetic surface normalization |
| `cleanup/` | Labels, temps, casts, fallthrough cleanup |
| `analysis/` | `defuse`, `expr_key` (shared def-use and pure-expr helpers) |
| `global_opt/` | SCCP, LICM, CSE, GVN join, redundant load, dead store, mem SSA |
| `types/` | Type inference, callsite/interproc propagation, variadic/entry-param |
| `recovery/` | PHI, flags, IV, for-loop folding |
| `memory/` | Slots, aggregate fields, pointer-arithmetic recovery |
| `idioms/` | Bitstream, branch hoist, callee-save prologue cleanup |
| `pipeline/` | `normalize_hir_function` orchestration (`run.rs`) |
| `wave_stats.rs` | Normalize wave telemetry into `NirBuildStats` |

Legacy flat modules at crate root of `normalize/`: `wave_stats.rs` only; other passes live under the directories above.

## Conventions

- Orchestration lives in `pipeline/`; behavior changes belong in the owning pass directory.
- `NirBuildStats` fields remain defined only in `nir/types.rs`.
