# Normalize Area Guide

Scope: `crates/fission-pcode/src/midend/normalize/`

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

Shared quality-wave counters live at midend root (`../wave_stats.rs`), not under
normalize. Import via `crate::midend::wave_stats` (ADR 0012 Phase D0).

## Pass ownership (avoid duplicate policy)

| Concern | Primary owner | Related (do not duplicate policy) |
|---------|---------------|-----------------------------------|
| PHI / join copies at merge | `recovery/phi_recovery.rs` | `global_opt/gvn_join.rs` (GVN at joins) |
| IV / induction vs structured `for` | `recovery/iv_recovery.rs` | `recovery/for_loops.rs` (shape fold) |
| Temp inline vs copy propagation | `cleanup/` passes | `global_opt/copy_propagation_pass.rs` |
| Redundant load / stack stores | `global_opt/redundant_load.rs`, `memory/` | `AliasKey::Stack` — justify via store→load deps |

`apply_iv_recovery` and related hooks are wired from `recovery/mod.rs` (not a legacy flat `iv_recovery.rs` at `normalize/` root).

## Conventions

- Orchestration lives in `pipeline/`; behavior changes belong in the owning pass directory.
- `NirBuildStats` fields remain defined only in `midend/ir/build_stats.rs`.
- New normalize behavior must enter through the ActionGroup/pass registry and an existing owner directory. Repeated special cases should be promoted into shared def-use, type-constraint, alias, or CFG facts before adding another narrow pass.
- Normalize/type recovery may consume substrate and action-pipeline facts, but must not reach into builder internals, structuring promotion policy, or render/printer output.
