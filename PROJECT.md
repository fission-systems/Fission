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
- **Migration slices landed 2026-07-19:** added `CleanupPass`,
  `GatedFollowupPass`, and `AdmissionGatedPass` to `action_pipeline` (new
  file `cleanup_pass.rs`) — the primitives needed to express the
  `if pass { cleanup }` and `if admission.eligible { pass }` idioms as
  ordinary `ActionGroup` passes instead of free-function control flow. Moved
  four chains of `run_stage_deadcode_dynamic` out of `stages.rs` into
  declarative passes registered directly in `groups.rs`'s
  `deadcode_dynamic` group, in order: `constant_ptr_recovery` →
  `cleanup_constant_ptr`; `conditional_const` → `cleanup_conditional_const`;
  `entry_param_promotion` → `cleanup_defuse_6`; the SCCP admission chain
  (`sccp` → `cleanup_sccp` → `constant_folding_after_sccp` →
  `cleanup_elim_8` → `wide_dead_assignment`). Each slice verified against a
  full crate test gate (1311 tests) plus real-binary NIR/HIR comparison.
  **Caveat from the SCCP slice:** an early draft used the budget-gated
  `CleanupPass` for a step whose original bare call had *no* budget gate —
  this measurably changed pass admission for larger bodies and was caught
  by the real-binary diff before commit (fixed by using `fn_pass` instead).
  The committed SCCP slice still has one confirmed cosmetic side effect:
  synthetic variable numbering (`xVarN`/`uVarN`) can shift on some
  functions versus the pre-slice build, with identical control flow, pass
  sequence, and per-pass stmt/local shape at every step (verified via
  `FISSION_PREVIEW_PERF` trace diff) and unchanged semantic case-pass rate
  on the real corpus row that exposed it — not a structural or semantic
  regression, but worth re-checking on future slices with the same
  before/after trace-diff technique, not text-diff alone.
  **`deadcode_dynamic` fully migrated 2026-07-19** (commit `3be5a75a`): all
  9 original chains (`constant_ptr_recovery`, `conditional_const`,
  `entry_param_promotion`, `sccp`, `cse`, `defuse_dead_assignment`,
  `copy_propagation`, `remove_dead_callee_param_loads`,
  `join_coalescing`, `branch_prefix_hoist`, `gvn_join_hoist` — note some
  chains bundle more than one original `if` block) are now declarative
  `ActionGroup` passes in `groups.rs`. `run_stage_deadcode_dynamic` and its
  `stage_pass` registration are deleted — `deadcode_dynamic` is the first
  fully-migrated stage, proving the pattern scales past a single chain.
  Also deleted `run_canonical_normalize_passes` (`pipeline/run.rs`): a
  second, parallel legacy driver with zero real callers (grep-verified)
  that called the old stage functions directly, bypassing the ActionGroup
  pipeline — migrating `deadcode_dynamic` would have silently emptied it
  further, so it was removed instead of patched.
- **Determinism fix landed 2026-07-19** (commits `d57b57e2`, `d1c2c33a`,
  `d7da0216`): unrelated to the migration itself, but found via the same
  before/after real-binary diff discipline this migration established.
  `region_external_exit_nodes` and `current_explicit_merge_binding_expr`
  each had an unsorted `HashSet`/`HashMap` iteration feeding a `.first()`/
  `.find_map()` pick — fixed individually, then the whole
  `fission-pcode::midend` + `fission-midend-structuring` boundary was
  swapped from std's per-process-random `RandomState` to a fixed-seed
  hasher (`rustc_hash::FxBuildHasher`) to close the class generally. See
  commit `d7da0216` for the full diagnostic writeup (deterministic-hasher
  experiment, ruled-out hypotheses, residual quality caveat for
  `state_machine_score`).
- **Two recurring migration pitfalls, worth checking on every future slice:**
  1. `cleanup_pass` (budget-gated, matches the original `run_cleanup_block`)
     vs `fn_pass` (ungated, matches original bare/unconditional calls) are
     easy to swap by accident since both take a `fn(&mut HirFunction)`-shaped
     callback — but picking the wrong one silently removes or adds the
     `EARLY_CLEANUP_BLOCK_STMT_LIMIT`/`BLOCK_LIMIT` admission gate. Caught
     once already (commit `a793dbb5` fixed a `4110b2ac` regression) only
     because the original code was re-read line-by-line, not because the
     real-binary regression set caught it (none of those 6 functions are
     anywhere near the 2000-stmt budget threshold). Always check whether the
     source chain used `run_cleanup_block` (→ `cleanup_pass`) or a bare call
     (→ `fn_pass`) before registering.
  2. Any chain whose body calls something that itself takes `diag`/`perf`
     (`apply_type_signature_fixed_point`, `run_cleanup_family_passes`) can't
     go through `fn_pass`/`GatedFollowupPass` — those primitives don't carry
     `diag`/`perf` to a callee. Keep those as a named `stage_pass` step
     (`run_stage_proto_recovery_head`, `run_stage_cast_elision` are the two
     precedents) rather than dropping the diag/perf forwarding silently.
- **Remaining backlog** (one `run_stage_*` function per row; each is its own
  scoped migration slice with its own before/after parity check — do not
  attempt more than one per change):

  | Stage function | Status |
  |---|---|
  | `run_stage_deadcode_dynamic` | **DONE** — fully migrated, function deleted |
  | `run_stage_proto_recovery` | **5 of 6 chains done** (commit `4110b2ac`) — `run_cleanup_family_passes` head kept as `stage_pass` (`run_stage_proto_recovery_head`; needs diag/perf-through-callee, separate slice) |
  | `run_stage_type_early` | **as-migrated-as-it-gets** — single call to `apply_type_signature_fixed_point(func, diag, perf)`, a complex sub-algorithm that itself needs diag/perf; no chains to decompose without extending `Pass` to carry diag/perf (bigger, separate proposal) |
  | `run_stage_stackstall` | **11 of 12 chains done** (commit `a793dbb5`) — `cast_elision` kept as `stage_pass` (`run_stage_cast_elision`; same diag/perf-through-callee reason as proto_recovery's head) |
  | `run_stage_heritage_value_recovery` | **poor candidate, skipped** — both `memory_slot_surfacing`/`memory_heritage` followups call `run_cleanup_family_passes` (diag/perf-through-callee), and there's a diag-gated `eprintln!` keyed on a runtime-computed `allow_expensive_passes` mode. Doesn't decompose with current primitives; left as `stage_pass` |
  | `run_stage_memory_recovery` | not started (large) |
  | `run_stage_merge` | **poor candidate, skipped** — `for round in 0..4` fixed-point loop calling `apply_type_signature_fixed_point(func, diag, perf)` every round with per-round `[DIAG]`/`[PERF]` prints keyed on `round + 1`; `fn_pass`/`GatedFollowupPass` support neither diag/perf-needing inner calls nor per-round numbering. Left as `stage_pass` |
  | `run_stage_block_structure_1` | **DONE** (commit `2fec85c3`) — all 6 chains migrated (`single_pred_label_inline`, `dowhile_decrement_condition_norm`, `loop_condition_trailing_temp_inline`, `iv_recovery`, `break_continue_recovery`, `licm`); no diag/perf-needing sub-calls, function deleted |
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
| 3 | Normalization Migration (Track B: action_pipeline for HirFunction) | Flatten `run_stage_*` if-chains into `ActionGroup` passes, one stage at a time | M2 pattern (independent track) | IN_PROGRESS — `deadcode_dynamic` and `block_structure_1` fully migrated (functions deleted); `proto_recovery`/`stackstall` mostly migrated (diag/perf-entangled heads kept as `stage_pass`); `type_early`/`heritage_value_recovery`/`merge` determined to be poor candidates and left as `stage_pass`; `memory_recovery`/`cleanup` not started |
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
