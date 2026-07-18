# Project: Fission NIR Transformation Pipeline Refactoring

Updated: 2026-07-19 — corrected against actual code state (previous revision
predated implementation and had drifted from reality; see track split below).

## Two pass-framework tracks (do not conflate)

There are two separate, independently-evolving pass-orchestration frameworks
in the tree. Neither subsumes the other — they operate on different IR shapes.

### Track A — `NirPass` / structuring (pre-structuring, block-CFG level)
- **Location:** `crates/fission-pcode/src/midend/pass/` (`func.rs`, `manager.rs`,
  `store.rs`, `structuring.rs`) — *not* `crates/fission-pcode/src/nir/pass/`
  as originally planned; landed under the existing `midend/` tree instead.
- **NirFunc**: wraps `PreviewBuilder`'s mutable pre-structuring state
  (`successors`, `predecessors`, `virtual_block_map`,
  `lowered_block_stmts_cache`, `locals`, `params`, `temps`, `loop_bodies`).
  Tracks `cfg_version`/`ir_version` for cache invalidation.
- **NirPass trait**: `fn run(&mut self, ir: &mut NirFunc<'_,'_>, store: &mut AnalysisStore) -> Result<PassResult, String>`,
  plus a mandatory `invariant_basis()` (dom tree / postdom tree / SCC / loop
  body / edge classification) — reviewed to block address/function-specific
  overfitting. Hard cap `MAX_STRUCTURING_PASSES = 6`.
- **AnalysisStore**: caches `CfgFactCache`, loop bodies, follow-blocks keyed
  on `cfg_version`.
- **Status: wired and live**, not a draft. `PreviewBuilder::build_multiblock_body()`
  → `CollapseDriver::run()` is the *only* structuring entry point and
  registers `EarlyReturnPass`, `IrreducibleReductionPass`,
  `SeseStructuringPass`, `OrphanGotoRepairPass` on a `PassManager`. Caveat:
  most of these passes are currently thin wrappers that call pre-existing
  free functions (e.g. `SeseStructuringPass::run` just calls
  `build_sese_region_body` / `structure_cfg_via_sese`) — the *container* is
  migrated, the *algorithm bodies* mostly are not yet.

### Track B — normalize (post-structuring, `HirFunction`/`Vec<HirStmt>` level)
- **Location:** `fission-midend-core::action_pipeline` (`Pass`, `PassCtx`,
  `PassOutcome`, `ActionGroup`, `Gate`, `Repeat`, `Pipeline`, `group`) +
  `fission-midend-normalize::pipeline` (`groups.rs`, `stages.rs`, `run.rs`).
- Operates on already-structured `HirFunction` — cannot reuse `NirFunc`
  (block-CFG shape) from Track A. Needs its own `Pass` impls, which already
  exist as a *separate* framework (`action_pipeline`), independently built at
  some earlier point and never fully adopted by normalize's own pass call
  sites.
- **Status before 2026-07-19:** `build_normalize_pipeline()` in `groups.rs`
  already used `Pipeline`/`ActionGroup` at the *stage* granularity (9 groups,
  Ghidra-ordered), but each group held exactly one `Pass` — a
  `CanonicalStagePass` wrapping an entire monolithic `run_stage_*` function
  (`pipeline/stages.rs`, 966 lines) full of hand-rolled
  `if run_pass_logged(func, "name", perf, pass_fn) { run_cleanup_block(...) }`
  chains (99 call sites total across `stages.rs`/`run.rs`). `run.rs` also
  duplicated telemetry/budget helpers that already existed in
  `action_pipeline` (`run_pass_logged`, `body_exceeds_early_cleanup_budget`).
- **Migration slice landed 2026-07-19:** added `CleanupPass` and
  `GatedFollowupPass` to `action_pipeline` (new file `cleanup_pass.rs`) —
  the two primitives needed to express the `if pass { cleanup }` idiom as
  ordinary `ActionGroup` passes instead of free-function control flow. Moved
  the first chain (`constant_ptr_recovery` → `cleanup_constant_ptr`, the
  first `if` in `run_stage_deadcode_dynamic`) out of `stages.rs` into
  declarative passes registered directly in `groups.rs`'s `deadcode_dynamic`
  group. Verified byte-identical NIR/HIR output before/after on 4 real
  binaries + full crate test gate (1311 tests) + local docker corpus row.
- **Remaining backlog** (one `run_stage_*` function per row; each is its own
  scoped migration slice with its own before/after parity check — do not
  attempt more than one per change):

  | Stage function | `run_pass_logged` call sites (approx) |
  |---|---|
  | `run_stage_proto_recovery` | ~5 |
  | `run_stage_deadcode_dynamic` | ~8 remaining (1 of 9 chains migrated) |
  | `run_stage_type_early` | small |
  | `run_stage_stackstall` | medium |
  | `run_stage_heritage_value_recovery` | medium |
  | `run_stage_memory_recovery` | large |
  | `run_stage_merge` | small |
  | `run_stage_block_structure_1` | small |
  | `run_stage_cleanup` | large (round-limited fixed point, needs `Repeat::UntilStable` review, not just `GatedFollowupPass`) |

  Once a stage's chains are fully expressed as `ActionGroup` passes, delete
  its `run_stage_*` wrapper function and the `CanonicalStagePass` entry for
  it in `groups.rs`, and drop the now-dead local `run_pass_logged` /
  `run_cleanup_block` duplicates in `pipeline/run.rs` in favor of
  `action_pipeline`'s versions once no callers remain.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Exploration & Design | Codebase analysis and draft interfaces | None | DONE |
| 2 | Core Interfaces (Track A: NirPass/PassManager/AnalysisStore/NirFunc) | Implement + wire into structuring driver | M1 | DONE (container); pass bodies still mostly thin wrappers over legacy free fns |
| 3 | Normalization Migration (Track B: action_pipeline for HirFunction) | Flatten `run_stage_*` if-chains into `ActionGroup` passes, one stage at a time | M2 pattern (independent track) | IN_PROGRESS — 1 of ~9 stage functions started (`deadcode_dynamic`, 1 of 9 chains) |
| 4 | Driver Integration | Track A already integrated (`CollapseDriver::run`); Track B integration is `run_normalize_pipeline` (already the sole normalize entrypoint) | M2/M3 | DONE for both tracks' outer driver; inner migration (M3) ongoing |
| 5 | E2E & Verification | Run tests, source-semantic benchmark, and Forensic Auditor per migration slice | M3 | ONGOING — required per slice, not a final gate |

## Interface Contracts

### Track A — `NirPass` (structuring)
- Signature: `fn run(&mut self, ir: &mut NirFunc<'_,'_>, store: &mut AnalysisStore) -> Result<PassResult, String>`.
- `NirFunc` wraps `PreviewBuilder` mutable state; `AnalysisStore` caches
  `CfgFactCache`/loop bodies/follow-blocks keyed on `cfg_version`, lazily
  re-evaluated on mismatch.

### Track B — `Pass` (normalize)
- Signature: `fn run(&self, ctx: &mut PassCtx<'_>) -> PassOutcome` where
  `PassCtx { func: &mut HirFunction, perf, diag, stats, decomp_facts }`.
- `FnPass` wraps any existing `fn(&mut HirFunction) -> bool` pass
  (zero-friction migration of existing pass functions).
- `CleanupPass` wraps a budget-gated cleanup block (`fn(&mut HirFunction)`),
  reusing the existing `body_exceeds_early_cleanup_budget` gate.
- `GatedFollowupPass` runs its `then` passes only when `cond` reports
  `Changed` — the direct replacement for
  `if run_pass_logged(...) { ...cleanup... }`.
- `ActionGroup` supports `Repeat::{Once, UntilStable{max_rounds}}` and a
  `Gate` (admission check before the group runs at all).
