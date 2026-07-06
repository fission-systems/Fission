# Fission Decompiler Action Pipeline Contracts

This document defines the Ghidra-aligned decompiler stages, their Fission owners, and the
forbidden cross-stage behaviors that the action pipeline framework enforces.

Related code:

- Framework: [`crates/fission-pcode/src/nir/action_pipeline/`](../crates/fission-pcode/src/nir/action_pipeline/)
- Normalize driver: [`crates/fission-pcode/src/nir/normalize/pipeline/groups.rs`](../crates/fission-pcode/src/nir/normalize/pipeline/groups.rs) (`run_normalize_pipeline`)
- Canonical pass sequence: [`crates/fission-pcode/src/nir/normalize/pipeline/run.rs`](../crates/fission-pcode/src/nir/normalize/pipeline/run.rs) (`run_canonical_normalize_passes`)
- ActionGroup registry (Ghidra order, migration target): `build_normalize_pipeline` in `groups.rs`
- Structuring collapse: [`crates/fission-pcode/src/nir/structuring/collapse_driver.rs`](../crates/fission-pcode/src/nir/structuring/collapse_driver.rs)
- Telemetry: [`crates/fission-pcode/src/nir/types/build_stats.rs`](../crates/fission-pcode/src/nir/types/build_stats.rs)

## Stage Map

| Ghidra concept | Fission owner | Primary input | Primary output | Key responsibilities | Forbidden |
|---|---|---|---|---|---|
| `FuncdataBuild` | `nir::builder` | Raw p-code | HIR skeleton, CFG layout | Block graph init, basic lifting | SSA heritage, type inference, structuring |
| `HeritageValueRecovery` | `nir::builder::materialize` + heritage group | HIR skeleton | near-SSA HIR | Stack/param/local recovery, copy-prop, join coalescing, iblock phi elimination | Control-flow structuring, type inference beyond slot naming |
| `Normalize` | `nir::normalize::pipeline` | near-SSA HIR | normalized HIR | Dead code, CSE, SCCP, deindirect, memory recovery | Final prototype fixate, pseudocode rendering |
| `PrototypeTypes` | `nir::normalize::types` | normalized HIR | typed HIR | Type inference, callsite propagation, merge-type fixpoint | CFG restructuring |
| `BlockGraphStructuring` | `nir::structuring` | typed HIR + CFG facts | structured HIR | CollapseDriver rules, SESE regions, loop/if/switch recovery | Expression semantic repair, printing |
| `PrintC` | `nir::printer` | structured typed HIR | pseudocode string | Rendering only | Any semantic recovery |

## Substrate Boundary

`fission-pcode` is physically one crate today, but quality work must follow the
same dependency direction that a future split would require.

| Layer | May depend on | Must not depend on |
|---|---|---|
| Substrate (`types`, `cfg`, `support`, `action_pipeline`, telemetry, shared analysis facts) | P-code primitives and other substrate modules | builder, normalize, structuring, render policy |
| Builder/materialize | substrate, cspec/calling-convention facts | normalize policy, structuring promotion policy, render/printer |
| Normalize/type recovery | substrate, action pipeline, reusable analysis facts | builder internals, structuring promotion policy, render/printer |
| Structuring | substrate CFG facts, typed HIR | builder internals, normalize semantic repair, render/printer |
| Render/printer | structured HIR and formatting options | builder, normalize, structuring recovery, semantic analysis |

Existing cross-layer references are migration debt. Do not use them as precedent
for new fixes. Move reusable facts downward into substrate modules before adding
another owner-to-owner dependency.

## Single Normalize Entry Point

`normalize_hir_function` always dispatches through `run_normalize_pipeline`, which runs the
Ghidra-ordered ActionGroup registry via `build_normalize_pipeline().run()`.

Each ActionGroup maps 1:1 to a stage function in
[`stages.rs`](../crates/fission-pcode/src/nir/normalize/pipeline/stages.rs). Stage functions
contain the canonical pass sequence (conditional cleanups, admission gates, and fixpoints).
`run_canonical_normalize_passes` is a thin sequential wrapper over the same stage functions for
callers that bypass the ActionGroup driver.

## Normalize Action Groups (Ghidra order)

The ActionGroup registry in `groups.rs` declares these groups in order:

1. **proto_recovery** — early cleanup, flag recovery, prologue/epilogue
2. **deadcode_dynamic** — constants, SCCP, CSE, copy-prop, join coalescing, branch hoists
3. **type_early** — type signature fixpoint (bounded repeat)
4. **stackstall** — nzmask, subvar, cast/load/deindirect rule pool
5. **heritage_value_recovery** — memory slot surfacing, memory heritage, SSA cleanup
6. **memory_recovery** — pointer arithmetic, aggregates, float idioms (gated for large functions)
7. **merge** — merge-type fixpoint (bounded repeat)
8. **block_structure_1** — loop IV, break/continue, LICM
9. **cleanup** — jump resolver, rule pool, final type inference

Each group records its `GhidraActionConcept` via `record_ghidra_action_stage`.

## Dead-Code Owners (Ghidra-aligned)

| Ghidra analog | Fission owner | Notes |
|---|---|---|
| `ActionDeadCode` (SSA temps) | `normalize/analysis/defuse.rs` — `defuse_dead_assignment_pass` | Single-pass SSA dead temp removal |
| `ActionDeadCode` (fixpoint after SCCP) | `defuse_dead_assignment_fixpoint_pass` | Absorbs former `apply_wide_dead_assignment_pass` |
| `ActionDeadCode` (consumed bits) | `normalize/global_opt/bit_consume.rs` | Bit-level consumed-mask dead code |
| Memory-SSA dead stores | `normalize/global_opt/dead_store.rs` | Stack-slot store removal |

Do not reintroduce parallel dead-code layers (`prune_unused_*` as standalone pass owners,
duplicate wide-defuse wrappers, or redundant temp-elimination passes outside defuse/cleanup).

## Bitmask Owners (Ghidra-aligned)

| Ghidra analog | Fission owner | Notes |
|---|---|---|
| `ActionNonzeroMask` | `normalize/global_opt/nz_mask.rs` | Global non-zero mask propagation |
| `subflow.cc` (bit-width prune) | `normalize/idioms/subflow.rs` | Early + final subflow pruning only |
| Consumed-bit dead code | `normalize/global_opt/bit_consume.rs` | Separate from nz_mask/subflow |

## Removed Narrow Idiom Passes (no Ghidra analog)

These passes and their dedicated tests were deleted; do not reintroduce without a Ghidra
Rule/Action reference:

- `security_cookie`, `xor_swap`, `string_copy`, `recurrence`, `call_artifact`, `bitstream`
- `likely_trash` (global_opt)

Retained idiom owners: `prologue`, `subflow`, `split_flow`, `branch_hoist`.

## Const-Fold / Copy-Join / Conditional-Move Owners

| Concern | Canonical owner | Secondary |
|---|---|---|
| Global constant propagation | `global_opt/sccp.rs` (`apply_sccp_pass`) | — |
| Local stmt-tree fold | `analysis/defuse.rs` (`constant_folding_pass`) | `rule_normalizer` `RuleFoldConstants` |
| Copy propagation | `recovery/copy_prop.rs` | absorbs inline-single-use via cleanup loop |
| Join coalescing | `recovery/join_coalesce.rs` | `gvn_join_hoist` at hoist boundary |
| Conditional move | `arith/conditional_move.rs` (`apply_conditional_move_pass`) | `conditional_select_pass` in cleanup loop |

## Admission Gates

Centralized in [`action_pipeline/gates.rs`](../crates/fission-pcode/src/nir/action_pipeline/gates.rs):

| Gate | Threshold | Used by |
|---|---|---|
| Large function | >220 stmts or >160 locals | `memory_recovery`, expensive memory passes |
| Early cleanup budget | >2000 stmts or >300 blocks | jump resolver admission |
| Jump resolver candidates | ≤16 | VSA jump resolver |
| Type signature FP | max 6 rounds | `type_early`, merge loop inner |
| Merge-type loop | max 4 rounds | `merge` group |
| Rule pool | max 15 rounds | `rule_normalization` ActionPool |

## Structuring Contracts

- **CollapseDriver** (`structuring/collapse_driver.rs`) owns Ghidra-style collapse rule dispatch.
- **RegionProof** requires explicit follow/postdom anchor when available (`structured_with_follow`).
- **CFG fallthrough** uses dom-tree edge classification (`cfg_analysis/follow.rs`), not layout index arithmetic.

## Stage Regression Gates

`NirBuildStats` counters (`ghidra_action_*_count`) track which stages ran. The framework provides
`stage_boundary_violation(expected, observed)` to detect passes registered under the wrong concept.

Contract tests live in:

- `nir/normalize/pipeline/heritage_contracts.rs`
- `nir/action_pipeline/concept.rs` (sequence stability)
- `nir/structuring/collapse_driver.rs` (rule order)

## Anti-Patterns

1. Do not add normalize passes that recover stack/param slots — use **heritage_value_recovery**.
2. Do not add structuring reducers that patch expression semantics — fix at normalize owner.
3. Do not reintroduce deleted narrow idiom passes without a Ghidra Rule/Action reference.
4. Do not duplicate dead-code or bitmask transform layers across defuse, cleanup, and global_opt.
5. Do not add sample-specific address/name guards — use CFG/dominance/def-use invariants.
6. Do not add owner-to-owner dependencies when a shared analysis fact can be moved into substrate.

## Validation

```bash
cargo nextest run -p fission-pcode
cargo check -p fission-pcode -p fission-decompiler -p fission-automation
python3 scripts/audit/nir_boundary_scan.py --root . --format markdown --output docs/audits/YYYY-MM-DD-nir-boundary-scan.md
```

For semantic changes, rerun source-semantic benchmark rows with stale caches disabled.
