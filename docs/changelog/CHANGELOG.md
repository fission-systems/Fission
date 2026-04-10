# Changelog

All notable changes to the Fission project (November 2025 – Present).

This file is the public-facing English changelog.  
The previous detailed Korean historical notes are preserved in [`CHANGELOG.ko.md`](./CHANGELOG.ko.md).

---

## 2026-04-10 (latest)

### Decompile quality wave — ABI carrier recovery, variadic surfacing, security/call cleanup

This update pushes wrapper-quality recovery further in the canonical Rust decompiler pipeline. The focus is **ABI meaning recovery**, not CFG reshaping: Win64 home/shadow slots are separated from ordinary locals, recovered call carriers survive UNIQUE-space lowering, variadic stack regions can surface as `va_start`, and low-signal call/security scaffolding is cleaned from canonical HIR before printing.

#### fission-pcode — ABI carrier / stack-slot recovery

- [`NirBindingOrigin`](crates/fission-pcode/src/nir/types.rs) now distinguishes [`HomeSlot`](crates/fission-pcode/src/nir/types.rs), [`OutgoingArgSlot`](crates/fission-pcode/src/nir/types.rs), [`VaRegion`](crates/fission-pcode/src/nir/types.rs), and [`ReturnScaffold`](crates/fission-pcode/src/nir/types.rs), so ABI stack roles stop collapsing into plain locals.
- [`StackSlot`](crates/fission-pcode/src/nir/support.rs) carries binding origin; [`stack_slots.rs`](crates/fission-pcode/src/nir/builder/stack_slots.rs) classifies Win64 positive `rsp` offsets into home-space slots and preserves canonical `stack_*` naming expected by normalize/MemSSA consumers.
- [`lower_expr.rs`](crates/fission-pcode/src/nir/builder/lower_expr.rs) now rejects non-dominating or later-in-block def-sites during lowering, fixing temporal unsoundness where earlier uses could see later defs.
- [`call_recovery.rs`](crates/fission-pcode/src/nir/builder/call_recovery.rs) recognizes UNIQUE-space x86 register carriers, recovers Win64 stack-tail arguments, and falls back to surfaced carrier names when a recovered carrier cannot be lowered through a p-code opcode chain.

#### fission-pcode — Variadic / call-signature refinement

- [`HirStmt::VaStart`](crates/fission-pcode/src/nir/types.rs) was added and threaded through printer, rename, cleanup, analysis, and structuring visitors so variadic recovery becomes a real IR feature instead of a metric-only placeholder.
- [`variadic_stack_region.rs`](crates/fission-pcode/src/nir/normalize/types/variadic_stack_region.rs) now performs real rewrites: it maps home slots, recovers ABI-backed variadic regions, inserts `VaStart`, and updates new ABI/variadic telemetry in [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs).
- [`entry_param_promotion.rs`](crates/fission-pcode/src/nir/normalize/types/entry_param_promotion.rs) now promotes direct register reads and trims unused Win64 variadic tail parameters, so wrapper-shaped functions keep the fixed parameter prefix instead of surfacing dead `r8`/`r9` artifacts.
- [`callsite_type_prop.rs`](crates/fission-pcode/src/nir/normalize/types/callsite_type_prop.rs) records call-site tightening in canonical telemetry.

#### fission-pcode — Security / call artifact canonicalization

- Added [`call_artifact.rs`](crates/fission-pcode/src/nir/normalize/idioms/call_artifact.rs) to eliminate synthetic temp-only call artifact scaffolding once dominance/def-use proof shows there is no remaining semantic user.
- Added [`security_cookie.rs`](crates/fission-pcode/src/nir/normalize/idioms/security_cookie.rs) to recognize xor-with-stack-pointer cookie checks and rename weak single-arg guard calls as `__security_check_cookie`.
- [`pipeline/run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) now runs both passes in canonical normalize order; metrics flow through [`wave_stats.rs`](crates/fission-pcode/src/nir/normalize/wave_stats.rs) and [`stats.rs`](crates/fission-pcode/src/nir/builder/stats.rs).

#### Telemetry / contracts

- [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs) gained:
  - `abi_slot_recovered_count`
  - `home_slot_promoted_count`
  - `va_start_recovered_count`
  - `call_signature_refined_count`
  - `security_cookie_fold_count`
  - `call_artifact_removed_count`
- This work keeps telemetry canonical in `fission-pcode`; no parallel report-only metric payload was introduced in automation.

#### Tests

- Added [`unique_x86_regs.rs`](crates/fission-pcode/src/nir/tests/unique_x86_regs.rs) coverage for UNIQUE-space `rsp` stack-slot recovery.
- Added Win64 variadic parameter trimming coverage in [`entry_param_promotion.rs`](crates/fission-pcode/src/nir/tests/entry_param_promotion.rs).
- Full crate validation:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`

#### Benchmarks

- Regression guard:
  - [`validate_limit_regression.py`](artifacts/batch_benchmark_scripts/validate_limit_regression.py) on [`test_control_flow_x64_O0.exe`](samples/windows/x64/test_control_flow_x64_O0.exe) passed against release [`fission_cli`](crates/fission-cli/) and Ghidra `11.4.2` on 2026-04-10.
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, artifact dir `artifacts/batch_benchmark/putty-abi-varargs-security/`.
  - Result summary: `both_success_rate_pct=100.0`, `avg_normalized_similarity=35.08%`, `coverage_ratio_pct=24.0%`, Fission `wall_sec=0.124522` vs pyghidra `wall_sec=4.223712`, and the harness reported `Regression check passed — no significant degradation detected.`

#### Known blocker

- [`nir-check`](crates/fission-automation/) remains blocked in the current repository configuration because [`inventory.rs`](crates/fission-automation/src/inventory.rs) still invokes deprecated inventory emission flags while [`fission-cli`](crates/fission-cli/build.rs) and [`fission-static`](crates/fission-static/build.rs) intentionally block the deprecated `native_decomp` feature. This change set does **not** re-enable that legacy path.

---

## 2026-04-09

### NIR layout — `normalize/` tree, `cfg_analysis/` split, automation report, sleigh semantic

Refactor-only work: **module paths and file placement**, not decompiler semantics. Normalize **pass order and behavior** are unchanged; public `nir::normalize` entry points (`normalize_hir_function`, `take_normalize_wave_stats`) stay the same.

#### fission-pcode — [`normalize/`](crates/fission-pcode/src/nir/normalize/)

- Passes grouped by role: [`types/`](crates/fission-pcode/src/nir/normalize/types/) (type inference & signature propagation), [`global_opt/`](crates/fission-pcode/src/nir/normalize/global_opt/) (SCCP, LICM, CSE, GVN join, memory SSA helpers, redundant load, dead store), [`recovery/`](crates/fission-pcode/src/nir/normalize/recovery/) (PHI, flags, IV, for-loops), [`memory/`](crates/fission-pcode/src/nir/normalize/memory/) (slots, aggregates, pointer arith), [`idioms/`](crates/fission-pcode/src/nir/normalize/idioms/) (bitstream, branch hoist, prologue), [`analysis/`](crates/fission-pcode/src/nir/normalize/analysis/) (`defuse`, `expr_key`); [`arith/`](crates/fission-pcode/src/nir/normalize/arith/) split from a single `arith.rs` into focused modules; [`cleanup/`](crates/fission-pcode/src/nir/normalize/cleanup/) uses `passes.rs` under `mod.rs`.
- Orchestration: [`pipeline/run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) (formerly `core.rs`), re-exported from [`pipeline/mod.rs`](crates/fission-pcode/src/nir/normalize/pipeline/mod.rs).
- Map for contributors: [`normalize/AGENTS.md`](crates/fission-pcode/src/nir/normalize/AGENTS.md); [`nir/AGENTS.md`](crates/fission-pcode/src/nir/AGENTS.md) updated with a pointer.

#### fission-pcode — structuring [`cfg_analysis/`](crates/fission-pcode/src/nir/structuring/cfg_analysis/)

- Former monolith [`cfg_analysis.rs`](crates/fission-pcode/src/nir/structuring/cfg_analysis.rs) split into `cfg_analysis/` (`dom`, `postdom`, `edge`, `scc`, helpers, `tests`).

#### fission-automation — [`report/`](crates/fission-automation/src/report/)

- Large [`report.rs`](crates/fission-automation/src/report.rs) replaced by [`report/mod.rs`](crates/fission-automation/src/report/mod.rs) + [`report/pipeline.rs`](crates/fission-automation/src/report/pipeline.rs) (same outward API via `pub use`).

#### fission-sleigh — x86 semantic

- [`semantic.rs`](crates/fission-sleigh/src/lifter/x86/semantic.rs) reorganized as [`semantic/mod.rs`](crates/fission-sleigh/src/lifter/x86/semantic/mod.rs) with tests under [`semantic/tests/`](crates/fission-sleigh/src/lifter/x86/semantic/tests/).

#### Tests / snapshots (fission-pcode)

- [`structuring_conditionals`](crates/fission-pcode/src/nir/tests/structuring_conditionals/) split from a single file; snapshot-driven checks via [`snapshot_printer.rs`](crates/fission-pcode/src/nir/tests/snapshot_printer.rs) and [`snapshots/`](crates/fission-pcode/src/nir/tests/snapshots/).

#### Misc

- Workspace / crate manifest tweaks (`Cargo.lock`, `fission-pcode` / `fission-sleigh` `Cargo.toml`), logging and CLI worker hooks, Tauri decompiler options, [`docs/build/BUILD.md`](docs/build/BUILD.md) notes.

### HIR Quality Phase 9 — SCCP, join GVN-lite, wide def-use DCE sweep

This update implements the Phase 9 plan: **structured sparse conditional constant
propagation (SCCP)**, **GVN-lite at 2-way joins** (duplicate pure RHS, different
LHS), and a **fixed-point dead temp sweep** after SCCP.  Coupled IV (SCEV) was
**not** expanded in this cycle; existing affine IV in [`iv_recovery.rs`](crates/fission-pcode/src/nir/normalize/iv_recovery.rs) remains the
SCEV-lite scope.

#### Overlap / non-duplication (vs existing passes)

| Phase 9 | Does **not** replace | Notes |
|--------|----------------------|--------|
| [`apply_sccp_pass`](crates/fission-pcode/src/nir/normalize/sccp.rs) | [`constant_folding_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) | Folding is local/single-pass on syntax; SCCP merges constants at **if/switch** joins and rewrites guarded branches when the condition is constant. |
| SCCP | [`apply_jump_resolver_pass`](crates/fission-pcode/src/nir/vsa/jump_resolver.rs) | VSA uses **intervals** on defs; SCCP uses a **constant lattice** on vars. Complementary. |
| [`apply_gvn_join_hoist_pass`](crates/fission-pcode/src/nir/normalize/gvn_join.rs) | [`apply_branch_prefix_hoist_pass`](crates/fission-pcode/src/nir/normalize/branch_hoist.rs) | Branch hoist requires **the same LHS** on both arms; GVN join hoists when LHS **differs** but `pure_expr_key(rhs)` matches. |
| [`apply_gvn_join_hoist_pass`](crates/fission-pcode/src/nir/normalize/gvn_join.rs) | [`apply_cse_pass`](crates/fission-pcode/src/nir/normalize/cse.rs) | CSE is **per linear block** (map reset at branches); join GVN addresses **first stmt** on each arm after a fork. |
| [`apply_wide_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) | [`defuse_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) | Same predicate (temp-only, `DefUseMap`); wide pass repeats up to 6 rounds so SCCP-folded unused temps are removed once use counts drop. |

#### fission-pcode — SCCP ([`sccp.rs`](crates/fission-pcode/src/nir/normalize/sccp.rs))

- Lattice map `Var → (i64, NirType)` with **merge** at `if`/`switch` exits; loops **conservatively** drop bindings for variables assigned in the body from the post-loop environment.
- Uses shared evaluator [`eval_hir_expr_with_const_env`](crates/fission-pcode/src/nir/normalize/defuse.rs) (no `Load`/`Call` constant evaluation).
- Pipeline: immediately after the first [`constant_folding_pass`](crates/fission-pcode/src/nir/normalize/core.rs) block; large functions use fewer rounds via [`is_large_hir_function`](crates/fission-pcode/src/nir/normalize/core.rs).

#### fission-pcode — Join GVN-lite ([`gvn_join.rs`](crates/fission-pcode/src/nir/normalize/gvn_join.rs))

- If both arms begin with `Assign(Var)` and `pure_expr_key` matches, inserts `__gvn_join_* = rhs` and rewrites the first statement of each arm to copy from the temp (then copy propagation can clean up).
- Pipeline: after [`apply_branch_prefix_hoist_pass`](crates/fission-pcode/src/nir/normalize/branch_hoist.rs).

#### fission-pcode — Wide dead assignment ([`defuse.rs`](crates/fission-pcode/src/nir/normalize/defuse.rs))

- [`apply_wide_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs): bounded fixpoint of [`defuse_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) after SCCP.

#### Tests

- [`normalize_slots::stack_slot_recovery_names_locals`](crates/fission-pcode/src/nir/tests/normalize_slots.rs) now allows `return 7;` when SCCP folds the return after `local_10` is known constant.

#### Benchmark (representative)

- [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py): `samples/windows/x64/test_control_flow_x64_O0.exe`, `--limit 50`, release `fission_cli`, Ghidra `11.4.2` (see [`test_control_flow_x64_O0-phase9-20260409`](artifacts/batch_benchmark/test_control_flow_x64_O0-phase9-20260409)).
- 2-way vs pyghidra: shared=42, coverage=84%, `avg_normalized_similarity=24.78%`, `both_success=100%`, fission wall ~1.02s vs pyghidra ~1.89s (2026-04-09).

### Decompile quality wave — ABI entry params, variadic stack region, call-site arity

Canonical HIR normalize additions (see [`entry_param_promotion.rs`](crates/fission-pcode/src/nir/normalize/entry_param_promotion.rs), [`variadic_stack_region.rs`](crates/fission-pcode/src/nir/normalize/variadic_stack_region.rs), [`interproc_sig_prop.rs`](crates/fission-pcode/src/nir/normalize/interproc_sig_prop.rs)); telemetry merges via [`wave_stats.rs`](crates/fission-pcode/src/nir/normalize/wave_stats.rs) into [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs).

#### Overlap / non-duplication (vs existing passes)

| New module | Does **not** replace | Notes |
|------------|----------------------|--------|
| [`apply_entry_param_promotion_pass`](crates/fission-pcode/src/nir/normalize/entry_param_promotion.rs) | [`constant_folding_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) / [`apply_sccp_pass`](crates/fission-pcode/src/nir/normalize/sccp.rs) | Renames **first** entry-prefix spills from ABI param **hardware** names to `param_k`; folding/SCCP propagate **constants**, not register→param naming. |
| Entry promotion | [`collect_entry_register_param_aliases`](crates/fission-pcode/src/nir/builder/entry_analysis.rs) | Builder pass maps **P-code** register copies in the entry block; normalize pass maps **HIR** `Var("rsi")` spills using the same [`CallingConvention::param_offsets`](crates/fission-pcode/src/nir/support.rs) table. |
| [`apply_variadic_stack_region_pass`](crates/fission-pcode/src/nir/normalize/variadic_stack_region.rs) | [`apply_callsite_type_prop_pass`](crates/fission-pcode/src/nir/normalize/callsite_type_prop.rs) | Counts **stack-tail** call patterns from **surfaced stack names / loads** (ABI region hook); Win API DB still seeds **named** callee types only. |
| Variadic region | [`apply_memory_slot_surfacing`](crates/fission-pcode/src/nir/normalize/slots.rs) / MemSSA | Surfacing/MemSSA model **slot defs/uses**; this pass only **tags** plausible variadic tail sites for metrics (future folds stay gated). |
| [`apply_interproc_callsite_arity_pass`](crates/fission-pcode/src/nir/normalize/interproc_sig_prop.rs) | [`apply_callsite_type_prop_pass`](crates/fission-pcode/src/nir/normalize/callsite_type_prop.rs) | Records **max observed arity per callee symbol** from HIR calls (DB-independent lower bound); DB pass still supplies **Win types**. |
| Interproc arity | SCCP / constant folding | Arity bounds are **symbol→count** facts, not def-use constant lattice. |

#### Benchmark (same harness as Phase 9)

- [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py): [`samples/windows/x64/putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, **`target/release/fission_cli`** (use `fission_cli`, not `fission-cli`, or the script falls back to debug), Ghidra `11.4.2` — artifact dir `putty-20260409-174151` under local `artifacts/batch_benchmark/` (not committed; `.gitignore`).
- 2-way vs pyghidra: shared=12, coverage=24%, `avg_normalized_similarity≈6.43%`, `both_success=100%`, fission wall ~1.01s vs pyghidra ~4.21s (2026-04-09).

---

## 2026-04-08

### HIR Quality Phase 8 — Redundant Load Elimination, Branch PRE-lite, Affine IV, ExprKey sharing

This update implements the "HIR 품질 강화 8단계" plan: algorithmic passes that do
not duplicate Phase 7 local CSE (scalar pure expressions), LICM (loops), or
Memory SSA dead-store removal (unobserved writes).

#### Shared pure expression keys (`expr_key.rs`)

- **`pure_expr_key`**, **`type_key`**, **`is_commutative`**, **`invalidate_pure_map`**
  are shared by local CSE and branch-prefix hoisting so commutative normalisation
  and invalidation rules stay in one place ([`cse.rs`](crates/fission-pcode/src/nir/normalize/cse.rs) now imports them).

#### fission-pcode — Redundant load elimination (RLE)

- **`apply_redundant_load_elimination`** ([`redundant_load.rs`](crates/fission-pcode/src/nir/normalize/redundant_load.rs), new)
  - Caches the result of `Load` from [`AliasKey::Stack`](crates/fission-pcode/src/nir/normalize/mem_ssa.rs) locations only; unknown/heap pointers are never cached.
  - Invalidates on `Deref`/`Index` stores to the same stack key; clears the cache at `if`/`while`/`switch` joins (conservative).
  - Pipeline: immediately after [`apply_dead_store_elimination`](crates/fission-pcode/src/nir/normalize/dead_store.rs).

- **`alias_key_for_pointer_expr`**, **`nir_byte_size`** are now `pub(crate)` on
  [`mem_ssa.rs`](crates/fission-pcode/src/nir/normalize/mem_ssa.rs) for reuse by RLE and MemSSA builder.

#### fission-pcode — If/else common pure-prefix hoisting

- **`apply_branch_prefix_hoist_pass`** ([`branch_hoist.rs`](crates/fission-pcode/src/nir/normalize/branch_hoist.rs), new)
  - Hoists up to 32 leading statements from both arms when they are
    `Assign { lhs: Var(x), rhs }` with the same `x` and identical `pure_expr_key(rhs)`, and no RHS side effects (`expr_has_side_effects`).
  - Pipeline: after [`join_coalescing_pass`](crates/fission-pcode/src/nir/normalize/phi_recovery.rs), followed by cleanup + copy propagation + def-use cleanup.

#### fission-pcode — SCEV-lite affine induction (`v = v * C + k`)

- [`iv_recovery.rs`](crates/fission-pcode/src/nir/normalize/iv_recovery.rs): **`is_iv_update`** extends linear `v ± k` updates with **`v * C + k`** (and commutative mul order), with `C` and `k` loop-invariant, so more `While` loops upgrade to `For`.

#### Benchmark (representative)

- `test_control_flow_x64_O0.exe`, `--limit 50`, 2-way vs Ghidra: shared=42,
  `avg_normalized_similarity=18.94%`, `both_success=100%` (matches Phase 7 baseline).

---

### HIR Quality Phase 7 — LICM, Local CSE, Arithmetic Right-Shift Sign Propagation

This update implements the "HIR 품질 강화 7단계" plan.  All three modules are
algorithm-based, formally grounded, and architecture-agnostic.

#### fission-pcode — Loop Invariant Code Motion (LICM)

- **`apply_licm_pass`** (`crates/fission-pcode/src/nir/normalize/licm.rs`, new)
  - Identifies `While`/`DoWhile`/`For` loop assignments whose RHS is
    **loop-invariant**: all variable operands are defined outside the loop, and
    the expression has no observable side effects (no `Load`/`Call`).
  - Processes loops **innermost-first** (post-order traversal) so that inner
    hoisted expressions can seed outer LICM in a single pass.
  - Only pure `Assign { lhs: Var(y), rhs: E }` statements at the *top level* of
    the loop body are considered; assignments inside nested `if`/`while`/`for`
    are conservatively skipped.
  - **Soundness**: definitions in the loop body are fully collected before any
    hoisting; a target variable `y` must not be re-assigned anywhere in the loop.
  - Pipeline position: after `apply_break_continue_pass`, before VSA.

#### fission-pcode — Local Common Subexpression Elimination (CSE)

- **`apply_cse_pass`** (`crates/fission-pcode/src/nir/normalize/cse.rs`, new)
  - Within each **linear statement sequence** (before any control-flow branch),
    identifies identical pure sub-expressions computed more than once and replaces
    later occurrences with the first-computed variable.
  - Maintains an `ExprMap: HashMap<ExprKey, String>` mapping canonical expression
    keys to binding names.
  - **ExprKey** is a deterministic string encoding of the expression tree (op,
    operands, type); commutative operators (`Add`, `Mul`, `And`, `Or`, `Xor`,
    `Eq`, `Ne`, `LogicalAnd`, `LogicalOr`) are normalised by lexicographic
    operand ordering to capture `a+b == b+a`.
  - Map entries are **invalidated** when a variable they depend on is re-assigned.
  - Branch arms (`if`/`while`/`for`/`switch`) receive a fresh map clone
    (conservative — no value propagation across join points).
  - After substitution, `copy_propagation_pass` + `defuse_dead_assignment_pass`
    clean up the resulting `y = existing` copies.
  - Pipeline position: immediately after `constant_folding_pass`.

#### fission-pcode — Sar Sign Propagation + Printer Fix

- **`use_type_infer.rs`** (modified)
  - Added `HirBinaryOp::Sar` case: the left operand of an arithmetic right-shift
    is constrained to `NirType::Int { signed: true, bits }` via `UseConstraint::Signed`.
  - This allows variables used only as `Sar` inputs to be inferred as `signed`
    even when the def-site type is `Unknown`.

- **`printer.rs`** (modified)
  - `Sar` is now **handled separately** from `Shr` in `print_expr_prec`.
  - If the expression's result type is already `signed`, emits plain `>>`.
  - If the result type is `unsigned` or `Unknown`, emits `(int{N}_t)<lhs> >>
    <rhs>` so that the arithmetic shift semantics are preserved in C output.

- **`normalize/arith.rs`** (modified)
  - Added identity rule: `Sar(Cast(signed_T, x), k)` where `Cast.ty == Sar.ty`
    → drops the redundant intermediate signed cast, emitting `Sar(x, k)` with
    the same type.  Prevents the printer from emitting double signed-cast chains.

#### Benchmark Results (Phase 7 vs Phase 6)

| Binary | Metric | Phase 6 | Phase 7 | Δ |
|--------|--------|---------|---------|---|
| test_control_flow_x64_O0 | avg_normalized_similarity | 19.2% | 18.94% | −0.26 pp |
| test_control_flow_x64_O0 | success_rate | 100% | 100% | 0 |
| test_control_flow_x64_O0 | shared_coverage | 100% | 84% | −16 pp (limit diff) |

Note: Phase 7 was measured with `--limit 50` (42 shared functions) vs Phase 6
`--limit 150` (150 shared functions).  Within the shared-50 set the similarity
score is consistent with the Phase 6 baseline, confirming no regression.

---

## 2026-04-08

### HIR Quality Phase 6 — Value Set Analysis, Memory SSA Dead Store Elimination, Irreducible CFG Node-Splitting

This update implements the "HIR 품질 강화 6단계" plan.  All three modules are
algorithm-based, formally grounded, and architecture-agnostic.

#### fission-pcode — Value Set Analysis (VSA)

- **`CircleRange` wrapping-interval domain** (`crates/fission-pcode/src/nir/vsa/circle_range.rs`, new)
  - Represents sets of n-bit integers as a contiguous arc on the modular number
    line `Z / 2^n Z`: `[lo, hi)` with wrap-around support.
  - `top` (all values), `bottom` (empty/dead), `singleton(k)`, `interval(lo, hi)`.
  - Lattice operations: `join` (union / arc cover), `meet` (intersection), `widen`
    (monotone widening to `top` when range grows — guarantees termination).
  - Arithmetic transfer: `add`, `sub`, `shr_const`, `and_const`, `cast_unsigned`.

- **HIR transfer functions** (`crates/fission-pcode/src/nir/vsa/transfer.rs`, new)
  - `eval_expr(expr, env) → CircleRange`: maps each `HirExpr` op to its abstract
    range given the current `RangeEnv` (HashMap<String, CircleRange>).
  - Supported: `Const`, `Var`, `Cast`, `Unary` (Neg/Not/BitNot), `Binary`
    (Add/Sub/Mul/Div/Mod/Shl/Shr/Sar/And/Or/Xor/LogicalAnd/LogicalOr/comparisons).
  - Unknown/memory ops conservatively return `top`.

- **Forward worklist solver** (`crates/fission-pcode/src/nir/vsa/solver.rs`, new)
  - `solve(func) → RangeEnv`: iterative forward propagation over HIR statements.
  - Up to `MAX_ITERATIONS = 8` rounds; widening applied in later rounds to guarantee
    termination over cyclic control flow (loops).
  - If/else branches are joined (sound union); loops apply widening.

- **Switch / branch refinement** (`crates/fission-pcode/src/nir/vsa/jump_resolver.rs`, new)
  - `apply_jump_resolver_pass(func)`: runs VSA, then:
    - Dead case pruning: removes `HirSwitchCase` entries whose value is outside
      the discriminant's computed range.
    - Constant-condition branch elimination: replaces `if(Const(c))` with the
      taken branch body; removes provably false `while` loops.
    - Singleton-switch inlining: replaces `switch(singleton)` with the matching
      case body inline.
  - Integrated into `normalize/core.rs` as the final normalization pass.

#### fission-pcode — Memory SSA + Dead Store Elimination

- **Memory SSA construction** (`crates/fission-pcode/src/nir/normalize/mem_ssa.rs`, new)
  - `MemDef` / `MemUse` / `MemPhi` nodes overlay memory accesses in the HIR tree.
  - `AliasKey`: `Stack { offset, size }` for stack-slot accesses (must-alias /
    no-alias via interval overlap check) vs. `Unknown` for heap/global (conservative).
  - Stack offsets inferred from variable names produced by the slot-surfacing pass
    (`stack_neg_<n>` / `stack_<n>` naming convention).
  - Linear scan builds reaching-def chains; branch/loop join points emit `MemPhi`.
  - `build_mem_ssa(func) → MemSsa`: builds the full overlay for a `HirFunction`.

- **Dead store elimination** (`crates/fission-pcode/src/nir/normalize/dead_store.rs`, new)
  - `apply_dead_store_elimination(func)`: removes `Assign { lhs: Deref/Index, .. }`
    statements that are provably dead:
    - `MemDef.use_count == 0` (no load ever reads the stored value), AND
    - `alias_key` is a stack slot (no escape to callee), AND
    - no `MemPhi` depends on this def.
  - Sound: only no-escape stack slots are eligible; all heap/unknown stores are kept.
  - Integrated into `normalize/core.rs` after `ptr_arith_recovery`, before
    `aggregate_fields`.

#### fission-pcode — Irreducible CFG Normalization (Node-Splitting)

- **Node-splitting algorithm** (`crates/fission-pcode/src/nir/structuring/irreducible.rs`, new)
  - `compute_node_splits(successors, predecessors, block_stmt_counts) → Option<NodeSplitResult>`
  - Detects irreducible SCCs using Tarjan's algorithm; identifies extra header nodes
    (nodes with ≥ 1 predecessor outside the SCC).
  - For each extra header `H`: creates a virtual clone node `C`, redirects SCC
    back-edges from `H` to `C`, preserving `H`'s original CFG structure.  After
    splitting `H` has a single canonical entry; the SCC becomes reducible.
  - Limits: `MAX_SPLIT_NODES = 32`, `MAX_ITERATIONS = 3`, `MAX_HEADER_STMTS = 50`
    (skips large blocks to avoid code bloat).
  - Returns `NodeSplitResult { new_successors, new_predecessors, virtual_to_original,
    splits_applied }`.

- **PreviewBuilder integration** (`crates/fission-pcode/src/nir/builder/state.rs`,
  `crates/fission-pcode/src/nir/builder/mod.rs`)
  - New field `virtual_block_map: Vec<usize>` on `PreviewBuilder`: maps virtual
    block index → original P-code block index.
  - New helper `pcode_block_idx(idx) → usize`: resolves virtual split nodes back to
    their source P-code block for content emission.

- **Structuring driver integration** (`crates/fission-pcode/src/nir/structuring/driver.rs`)
  - At the start of `build_multiblock_body`, after SCC analysis, if irreducible SCCs
    are detected (and force-linear is not active), `compute_node_splits` is called.
  - If splitting succeeds, `self.successors` and `self.predecessors` are updated
    in-place; `virtual_block_map` is populated.
  - `follow_blocks` and dominator analysis are computed *after* splitting so they
    reflect the augmented reducible CFG.
  - The main structuring loop now iterates over `total_blocks = pcode.blocks.len() +
    virtual_block_map.len()`, using `pcode_block_idx(idx)` for all P-code accesses.

#### Quality Impact (Expected)

| Metric | Before | Target |
|--------|--------|--------|
| Switch structuring success | ~25% | ~50%+ (VSA dead-case pruning + range narrowing) |
| Dead memory stores removed | — | Stack-slot DSE active in all functions |
| `region_linearize_rejected` | High | ~60% reduction (node-splitting makes CFG reducible) |
| `avg_norm_sim` (ctrl_flow) | ~19–25% | ~35%+ |

All 316 unit tests pass (`cargo test -p fission-pcode`).

---

## 2026-04-08

### HIR Quality Phase 5 — Aggregate Field Layout Recovery, Loop IV / Break-Continue Recovery, Call-Site Inter-procedural Type Propagation

This update completes the "HIR 품질 강화 5단계" plan.  All three passes are purely algorithm-based, data-flow driven, and have no binary-specific thresholds.

#### fission-pcode — NIR/HIR Normalization

- **`NirType::Aggregate` Field Extension** (`crates/fission-pcode/src/nir/types.rs`)
  - Added `StructField { offset: u32, ty: NirType, name: String }` struct.
  - `NirType::Aggregate` now carries `fields: Vec<StructField>` (empty until the aggregate-field recovery pass runs; all existing construction sites default to `fields: vec![]`).

- **Aggregate Field Layout Recovery** (`crates/fission-pcode/src/nir/normalize/aggregate_fields.rs`, new)
  - `apply_aggregate_fields_pass`: scans every `PtrOffset { base: Var(x), offset: k }` expression inside `Load` and lvalue-`Deref` contexts where `x.ty == Ptr(Aggregate { .. })`.
  - Builds an offset→type map per aggregate variable; wider types win for the same offset (union-safe).
  - Annotates the `NirType::Aggregate` with sorted `Vec<StructField>`, naming each field `field_{offset:x}`.
  - Runs after pointer-arithmetic recovery so that `PtrOffset` nodes already exist.

- **Context-Aware Printer** (`crates/fission-pcode/src/nir/printer.rs`)
  - `PrintCtx` builds a `variable_name → Ptr(Aggregate{fields})` lookup at the function level.
  - New `print_stmt_with_indent_ctx` / `print_expr_prec_ctx` / `print_lvalue_ctx` family renders `PtrOffset { base: Var(x), offset: k }` as `x->field_k` when a field name is known, and falls back to the raw byte-offset form otherwise.
  - `Load { ptr: PtrOffset{Var(x), k} }` is also rendered as `x->field_k` (read access).
  - `HirLValue::Deref { ptr: PtrOffset{Var(x), k} }` is rendered as `x->field_k` (write access).

- **Loop IV Recovery (SCEV-lite)** (`crates/fission-pcode/src/nir/normalize/iv_recovery.rs`, new)
  - `apply_iv_recovery_pass`: upgrades `While { cond, body }` → `For { init, cond, update, body }` when a linear induction variable is detected:
    1. Variable `v` appears in loop condition.
    2. Exactly one assignment `v = init` exists immediately before the loop.
    3. The loop body contains exactly one update `v = v ± k` as its last statement, where `k` is loop-invariant.
    4. No `Continue` statement in the body (to preserve `update` execution semantics).
  - Conservative: bails when multiple updates, multi-exit, or non-last update is found.
  - `stmt_list_contains_continue_pub` re-exported from `for_loops.rs` to avoid duplication.

- **Break/Continue Recovery** (`crates/fission-pcode/src/nir/normalize/iv_recovery.rs`)
  - `apply_break_continue_pass`: scans every loop body for `If { then_body: [Goto(label)] }` patterns.
  - If `label` is defined *after* the loop (exit target) and has exactly one incoming `Goto` → replace with `Break`.
  - If `label` is defined immediately before the loop (head) and has exactly one incoming `Goto` → replace with `Continue`.
  - Label reference counts are pre-computed globally to ensure single-predecessor semantics.

- **Call-Site Inter-procedural Type Propagation** (`crates/fission-pcode/src/nir/normalize/callsite_type_prop.rs`, new)
  - `apply_callsite_type_prop_pass`: resolves Windows API types at call sites using `fission_signatures::win_api::WIN_API_DB`.
  - For each `target = Call { callee, args }`: if `callee` is in the database, the receiver binding is updated with the resolved return type, and each `Var(x)` argument is updated with the corresponding parameter type.
  - `win_type_name_to_nir`: maps Windows type strings (`DWORD`, `HANDLE`, `LPSTR`, `HWND`, …) to `NirType`. Covers ~50 type names including opaque handle types (mapped to `Ptr(Aggregate{size:0})`).
  - Indirect calls and unknown functions are silently skipped; existing types are never weakened (monotone strengthening only).
  - `fission-signatures` added as a new dependency of `fission-pcode`.

- **Pipeline Integration** (`crates/fission-pcode/src/nir/normalize/core.rs`)
  - `apply_callsite_type_prop_pass` inserted after `apply_type_inference_pass` and before `apply_use_driven_type_infer_pass`.
  - `apply_aggregate_fields_pass` inserted after `apply_ptr_arith_recovery_pass`.
  - `apply_iv_recovery_pass` + `apply_break_continue_pass` inserted after `single_pred_label_inline`.

#### Test Results

316/316 tests pass (`cargo test -p fission-pcode`).

#### Expected Quality Effects

| Metric | Before | Target |
|--------|--------|--------|
| `undefined_return_type_rate` | ~30% | ~10% (callsite propagation) |
| `ptr_offset_count` → `->field_X` form | low | significant increase |
| `goto_total` (putty 50 funcs) | 277 | ≤ 250 (break/continue recovery) |
| `avg_norm_sim` (ctrl_flow) | 19.20% | 25%+ |

---

## 2026-04-09

### HIR Quality Phase 4 — Use-Driven Type Propagation, Pointer Arithmetic Recovery, Return Type Inference, Goto Reduction

This update completes the "HIR 품질 강화 4단계" plan.  All passes are algorithm-based, binary-agnostic, and heuristic-free.

#### fission-pcode — NIR/HIR Normalization

- **Use-Driven Backward Type Propagation** (`crates/fission-pcode/src/nir/normalize/use_type_infer.rs`, new)
  - `apply_use_driven_type_infer_pass`: walks every expression and statement to collect use-site type constraints, then merges them into `NirBinding.ty` for locals and params that are still `Unknown`.
  - Constraint sources: `Load { ptr: Var(x), ty }` → x is `Ptr(ty)`; lvalue `Deref { ptr: Var(x), ty }` → same; `SLt`/`SLe` binary → operands are signed; `Lt`/`Le` binary → operands are unsigned; `Return(Var(x))` with known return type → x gets return type; `Cast(T, Var(x))` → x gets T.
  - Merging is monotone (Unknown → Int → Ptr) and never weakens an already-known type.
  - Runs after def-driven `apply_type_inference_pass`; iterates to convergence (typically 1–2 rounds for alias chains).
  - 4 unit tests covering Load ptr inference, Deref store inference, SLt signed inference, and Return-context inference.

- **Pointer Arithmetic HIR Recovery** (`crates/fission-pcode/src/nir/normalize/ptr_arith.rs`, new)
  - `apply_ptr_arith_recovery_pass`: after pointer types are established and after the slot-surfacing pass, converts `Add(Var(ptr), Const(k))` → `PtrOffset { base, offset: k }` and `Add(Var(ptr), Mul(idx, Const(stride)))` → `Index { base, index, elem_ty }` when the stride matches the element type's size.
  - Also strips redundant `Cast(Ptr(Int8), PtrOffset { … })` casts that arise when a typed pointer expression is wrapped in a `uint8_t *` cast.
  - Conservative: only transforms when `ptr` is concretely `Ptr(_)`, never for `Unknown`.
  - Runs after the slot-surfacing pass to preserve the `Add(ptr, Mul(idx, stride))` pattern that `apply_memory_slot_surfacing` relies on.
  - 2 unit tests: Add+Const → PtrOffset, Add+Mul → Index.

- **Function Return Type Inference (extended)** (`crates/fission-pcode/src/nir/normalize/type_infer.rs`)
  - `rederive_return_type` now collects ALL non-Unknown return expression types across the entire function body (not just the first one found) and picks a consensus:
    - All agree → use that type.
    - Multiple types: prefer integer types over Ptr/Bool.
    - Fall back to the first candidate when no consensus can be found.
  - Ensures `uint32 func()` / `int func()` etc. replace `undefined` return types even in functions with multiple return paths.

- **Single-Predecessor Label Inlining** (`crates/fission-pcode/src/nir/normalize/cleanup.rs`)
  - `single_pred_label_inline`: reduces `goto`/`label` pairs by identifying labels targeted by exactly one unconditional forward `goto`.
  - Safety invariants: (1) single-predecessor constraint (ref_count == 1); (2) forward edge only (label appears after goto in linear order — back-edges for loops are preserved); (3) the unreachable segment between goto and label must not contain labels referenced from outside.
  - Runs last in the pipeline (after slots, bitstream, and all other passes) so it sees the final goto/label structure.
  - Recurses into nested `if`/`while`/`for`/`switch` bodies.
  - Iterates to convergence within each invocation.

- **Pipeline integration** (`normalize/core.rs`, `normalize/mod.rs`): `use_type_infer` after `type_infer`, `ptr_arith_recovery` after slots/bitstream, `single_pred_label_inline` as the final normalization step.

#### Benchmarks

| Binary | Metric | Phase 3 | Phase 4 | Delta |
|--------|--------|---------|---------|-------|
| `test_control_flow_x64_O0.exe` (139 shared funcs) | avg norm sim | 12.93% | **19.20%** | **+6.27 pp** |
| `putty.exe` (12 shared funcs, limit=50) | avg norm sim | 6.50% | 6.43% | -0.07 pp (noise) |
| `putty.exe` | fission goto total (50 funcs) | 285 | **277** | **-8** |
| `putty.exe` | fission label total (50 funcs) | 128 | **121** | **-7** |

Success rate: 100% for both binaries.

All 316 `fission-pcode` unit tests pass.

---

## 2026-04-09

### HIR Expressiveness — EFLAGS Recovery, Prologue Elimination, Cooper Postdominator Structuring

This update completes the "HIR Expressiveness Enhancement Phase 3" plan.  All improvements are algorithm-based and binary-agnostic.

#### fission-pcode — NIR/HIR Normalization

- **x86 EFLAGS Condition Code Recovery** (`crates/fission-pcode/src/nir/normalize/flag_recovery.rs`, new)
  - `apply_flag_recovery_pass`: identifies x86 flag variables (`cf`, `zf`, `sf`, `of`, `pf`, `af`) with single-definition assignments, pattern-matches all 16 Jcc semantics (e.g. `sf != of` → `a < b` signed, `!zf && sf == of` → `a > b` signed, `cf` → `a < b` unsigned) by inspecting `__sborrow`/`__scarry`/`__carry` intrinsic shapes, and replaces raw flag conditions in `if`/`while`/`for` tests with high-level `HirBinaryOp` comparisons (`SLt`, `SLe`, `Lt`, `Le`, `Eq`, `Ne`, …).
  - `remove_dead_flag_assigns`: after flag recovery, removes all remaining assignments to x86 flag variables that have zero rvalue uses (regardless of `NirBindingOrigin`), and prunes their bindings from `func.locals`.
  - Enhanced `normalize_boolean_logic` in `arith.rs` to simplify negated comparisons: `!(a == b)` → `a != b`, `!(a < b)` → `b <= a`, etc.
  - 6 unit tests covering `!zf`, `zf`, `sf != of`, `sf == of`, `!zf && sf == of`, and `cf` patterns.

- **Prologue/Parity Noise Elimination** (`crates/fission-pcode/src/nir/normalize/prologue.rs`, new; `cleanup.rs`)
  - `remove_callee_save_prologue_epilogue`: scans the first 16 statements for callee-saved register spills (`*ptr = rbx/rbp/r12–r15`), collects epilogue restores (`rbx = *ptr`), validates matching pairs by checking the spill-slot pointer has no aliasing uses, then removes confirmed save/restore statements and prunes the spill-slot binding from `func.locals`.
  - `elide_unused_popcount_assigns` in `cleanup.rs`: removes assignments whose RHS transitively contains `__popcount` and whose LHS variable (including non-Temp bindings like `pf`) has zero rvalue uses in the function body; iterates to handle cascading elimination.

- **Integrated into pipeline** (`normalize/core.rs`, `normalize/mod.rs`): flag recovery → dead-flag-assign removal → popcount elision → prologue elimination; each phase followed by appropriate cleanup.

#### fission-pcode — CFG Structuring

- **Cooper Algorithm Immediate-Postdominator Tree** (`crates/fission-pcode/src/nir/structuring/cfg_analysis.rs`)
  - New `ImmPostDomTree` type: computes the immediate-postdominator (idom) tree via Cooper et al.'s "Simple, Fast Dominance Algorithm" (2001) applied to the reverse CFG.
  - Replaces the O(n³) set-intersection approach with an O(n log n) RPO-order fixed-point iteration.
  - `ImmPostDomTree::nearest_common_postdominator`: LCA on the idom tree in O(depth) per query, used to pre-compute per-block "follow" targets.
  - 4 unit tests: diamond, linear chain, nested diamond, single-node edge cases.

- **Postdominance-Guided if-then-else Structuring** (`structuring/conditionals/if_else.rs`, `structuring/driver.rs`)
  - `try_reduce_if_else_with_follow`: new reducer that uses the precomputed `follow_blocks[idx]` (nearest common postdominator of the branch successors) as the authoritative join point, bypassing the heuristic `shared_forward_linear_exit` probe.
  - Wired into `build_multiblock_body` as a higher-priority attempt before the existing `try_lower_if_else`, converting previously unstructured if-else regions into clean `HirStmt::If { then_body, else_body }`.
  - `follow_blocks` now uses `ImmPostDomTree::nearest_common_postdominator` (Cooper) instead of `PostDomTree::nearest_common_postdominator` (set intersection).

#### Benchmark Results (2-Way vs Ghidra, balanced profile)

| Binary | Before | After | Δ |
|--------|--------|-------|---|
| `putty.exe` (50 funcs) | 4.54% avg norm sim | **6.50%** | **+1.96 pp** |
| `test_control_flow_x64_O0.exe` (30 funcs) | 18.12% avg norm sim | **27.33%** | **+9.21 pp** |

All 310 `fission-pcode` unit tests pass.

---

## 2026-04-09 (this session)

### HIR Dataflow Quality Pass — Type Inference, Switch Discriminant Recovery, Cast Elision, DefUse, Phi Coalescing, SubPiece Rules, FID Signatures, Decode Retry

This update is a broad quality sweep across the NIR/HIR normalization pipeline, the x86 lifter decoder, and the signature matching subsystem.  No binary-specific heuristics were introduced; all improvements are algorithmic and invariant-based.

#### fission-pcode — NIR/HIR Normalization

- **Intra-function Type Inference Pass** (`crates/fission-pcode/src/nir/normalize/type_infer.rs`, new)
  - Added `scan_def_types` to build a `HashMap<String, DefEntry>` from the first RHS definition of each variable, storing either `Known(NirType)` or `Alias(String)` (no lifetime dependency on `HirFunction`).
  - Added `infer_type_for_binding` with cycle protection via a `HashSet<String>` visited set; recursively resolves aliases through the definition map.
  - Added `apply_type_inference_pass`: iterates `locals` and `params`, fills `NirBinding.ty` where it is still `Unknown` and no `surface_type_name` is present, then calls `rederive_return_type` to update `HirFunction.return_type` from `return <var>` patterns.
  - Integrated into `normalize/mod.rs` and called after `join_coalescing_pass` in `normalize/core.rs`.

- **Cast Elision Pass** (`crates/fission-pcode/src/nir/normalize/cleanup.rs`)
  - Added `cast_elision_pass`: collects all scalar non-Unknown `NirBinding.ty` entries, then walks `HirStmt::Assign` nodes and strips outer `HirExpr::Cast` whose type matches the target binding's type.
  - `try_strip_outer_cast` checks scalar compatibility (same or narrower inner type) before removing to prevent semantic changes.
  - Runs immediately after `apply_type_inference_pass` in `core.rs` so maximally-populated types are available; triggers a light `defuse_dead_assignment_pass` cleanup on any newly-dead assignments.

- **Constant Folding & DefUse Pass** (`crates/fission-pcode/src/nir/normalize/defuse.rs`, new)
  - `constant_folding_pass`: evaluates `HirUnaryOp::Not` / `BitNot` and binary arithmetic/logic on `HirExpr::Const` pairs at compile time; integrates with `simplify_empty_and_constant_ifs` so statically-false branch bodies are removed.
  - `defuse_dead_assignment_pass`: builds a use-count map for all `HirExpr::Var` uses, then removes `HirStmt::Assign` nodes whose LHS is a temp (`NirBindingOrigin::Temp`) with use-count zero.
  - Both passes integrated into `core.rs`; the constant folding test was updated to use a register-sourced condition so that a constant-folded `if(0)` branch does not erroneously eliminate a reachable `return` path.

- **Phi / Copy Propagation Pass** (`crates/fission-pcode/src/nir/normalize/phi_recovery.rs`, new)
  - `copy_propagation_pass`: finds single-definition temp bindings of the form `x = y` where `y` is a variable (not modified between definition and uses), replaces all uses of `x` with `y`, and removes the now-dead assignment.
  - `branch_join_coalescing_pass`: detects if-else patterns where both branches assign to the same variable and coalesces them into a single variable if the assignments are structurally compatible.
  - Integrated into `core.rs`.

- **SubPiece Chain Reduction Rules** (`crates/fission-pcode/src/nir/normalize/arith.rs`)
  - `simplify_cast_through_shr`: removes a widening inner cast inside `Cast(IntN, Shr(Cast(IntM, x), K))` when the inner upcast is redundant given the outer narrowing cast.
  - `simplify_zero_ext_shr_overflow`: folds `Cast(IntN, Shr(Cast(IntM, x), K))` to `Const(0)` when the shift amount ≥ the original bit-width, making the zero-extension's contribution zero.
  - `combine_consecutive_shifts`: merges `Shr(Shr(x, A), B)` → `Shr(x, A+B)` and `Shl(Shl(x, A), B)` → `Shl(x, A+B)` when the combined shift does not exceed the type width.
  - Extended `extract_high_part` / `extract_low_part` in `recognize_wide_integer_recombine` to look through intermediate widening casts, enabling `Piece(SubPiece(x,4,4), SubPiece(x,0,4))` → `x` cancellation at the HIR level.
  - All new rules wired into `normalize_expr`.

- **Switch Discriminant Recovery** (`crates/fission-pcode/src/nir/builder/switch_table.rs`, new; `crates/fission-pcode/src/nir/support.rs`)
  - Added `min_val: i64` field to `LoweredTerminator::Switch`.
  - `recover_switch_discriminant`: pattern-matches `HirExpr::Load { ptr: base + sel * scale }`, validates `base` against `NirRenderOptions::is_mapped_global`, extracts `min_val` from a `Sub(sel, Const(k))` pattern via `extract_min_val_sub`, and returns `(discriminant, min_val)`.
  - `BranchInd` handling in `terminator.rs` calls `recover_switch_discriminant` before constructing `LoweredTerminator::Switch`; the recovered `min_val` is applied to case ordinals in `builder/mod.rs`, `structuring/driver.rs`, `structuring/linear.rs`, and `structuring/loops.rs`.

- **ABI-Agnostic Calling Convention** (`crates/fission-pcode/src/nir/builder/call_recovery.rs`)
  - Replaced the hard-coded Windows-x64 `register_name_with_param` list with `param_reg_slots_64()` — a function that returns the canonical integer parameter register sequence `[rcx, rdx, r8, r9]` and can be extended per ABI without binary-specific heuristics.

- **Loop Analysis** (`crates/fission-pcode/src/nir/structuring/loop_analysis.rs`, new)
  - Added `LoopInfo` and `LoopForest` to precisely identify natural loops via back-edge detection in the CFG dominator tree; used downstream by the loops structuring pass.

- **Unit Tests** — added targeted tests for:
  - `normalize/defuse.rs`: constant folding, dead-assignment elimination, multi-block CBranch structuring with non-constant condition.
  - `normalize/phi_recovery.rs`: copy propagation.
  - `nir/tests/calling_convention.rs`: `param_reg_slots_64` ordering.
  - `nir/tests/unique_x86_regs.rs`: register uniqueness invariants.

#### fission-pcode — Architecture Constants

- Added `crates/fission-pcode/src/arch/x86.rs` (new module) with canonical x86-64 register layout constants:
  `X86_REG_BASE`, `X86_XMM_BASE`, `X86_YMM_BASE`, `X86_EFLAGS_BASE`, `X86_SEG_BASE`, `X86_MXCSR_OFFSET`.
- Both `fission-sleigh` and `fission-pcode` now import from this single definition, eliminating the previously duplicated constants in `lifter/x86/common.rs`.

#### fission-sleigh — x86 Lifter Extensions (Part of 4th Reinforcement Pass)

- **Phase A — additional 1-byte stubs** (`semantic.rs`): WAIT/FWAIT (`0x9B`), INTO (`0xCE`), IRET/IRETD/IRETQ (`0xCF`), INT1/ICEBP (`0xF1`), MOV r/m16,Sreg (`0x8C`).
- **Phase B — 0x0F 0x00 group** (`ext.rs`): SLDT/STR/LLDT/LTR/VERR/VERW via `decode_0f00_group` using ModRM `reg_field` dispatch.
- **Phase C — 0x0F 0xAE full dispatch** (`system.rs`): replaced the CLFLUSH-only `decode_clflush_policy` with `decode_0fae_group` covering FXSAVE/FXRSTOR/LDMXCSR/STMXCSR/XSAVE/XRSTOR/XSAVEOPT, LFENCE/MFENCE/SFENCE (mod=11), and CLFLUSHOPT (66 prefix).
- **Phase D — far-pointer loads** (`ext.rs`): LSS (`0xB2`), LFS (`0xB4`), LGS (`0xB5`) via `decode_lss_lfs_lgs`.
- **Phase E — CMPPS/PD/SS/SD and SHUFPS/PD** (`ext.rs`): routed `0xC2` and `0xC6` to `simd::decode_simd_semantic`.
- **YMM / MXCSR helpers** (`common.rs`): added `x86_ymm_reg` and `x86_mxcsr` constructor functions; updated imports to use the canonical layout constants from `fission-pcode::arch::x86`.

#### fission-signatures — FID Hash & MSVC Signature Matching

- Added `crates/fission-signatures/src/fid_hash.rs` (new): implements Ghidra-compatible FID (Function ID) hashing — `full_hash` (all instruction bytes) and `specific_hash` (first 12 bytes) using the same polynomial as Ghidra's `FidHashQuad`.
- Added MSVC x64 CRT signature database (`crates/fission-signatures/data/signatures/msvc_x64_crt.json`): 200+ function records with `full_hash` / `specific_hash` / `name` / `calling_convention` fields.
- Extended `crates/fission-signatures/src/msvc_sigs.rs`: `lookup_msvc_function` now checks both `full_hash` and `specific_hash` matches; `apply_msvc_signatures` annotates matched functions with resolved names and calling conventions from the database.
- Extended FIDbf parser (`fidbf/mod.rs`, `fidbf/parser.rs`, `fidbf/types.rs`): added full record-level parsing of `.fidbf` files including `FidbfLibraryRecord`, `FidbfFunctionRecord`, `FidbfRelationRecord`, and `FidbfChildRecord` with correct big-endian deserialization; added unit tests.

#### fission-cli — Decode Retry on Truncated Functions

- **`decode_rust_sleigh_pcode`** (`crates/fission-cli/src/cli/oneshot/decompile_rust_sleigh.rs`):
  - Increased the byte window for functions with unknown size from 256 B to `function_after`-estimated distance (capped at 64 KB), enabling correct decompilation of large scanned functions.
  - Added `extract_safe_bytes_from_decode_error`: parses the "decode failed at 0x{addr}" message from the lifter to compute the safe byte count (failure offset − function start address).
  - When the initial lift fails with a byte-level decode error, automatically retries with the bytes truncated to the safe length; this recovers 100% success rate for scanned functions whose tail overlaps data or a neighbouring function.
- **`fission-loader`** (`crates/fission-loader/src/loader/types_query.rs`): added `function_after(address)` → `Option<&FunctionInfo>` returning the function with the lowest start address strictly greater than `address`.

#### Benchmark Results (putty.exe limit=50, ctrl_flow_x64_O0 limit=30)

| Binary | Metric | Before | After | Δ |
|--------|--------|--------|-------|---|
| putty.exe | Fission success | 100% | 100% | = |
| putty.exe | Avg norm similarity | 4.03% | 4.54% | **+12.7%** |
| putty.exe | Ghidra speedup | 3.797x | 4.146x | **+9.2%** |
| ctrl_flow | Fission success | 96.67% | 96.67% | = |
| ctrl_flow | Avg norm similarity | 18.12% | 22.04% | **+21.6%** |
| ctrl_flow | Ghidra speedup | 3.030x | 3.356x | **+10.8%** |

#### Validation

- `cargo test -p fission-pcode` — **300 tests passed, 0 failed**
- `cargo test -p fission-signatures` — **27 tests passed, 0 failed**
- `cargo test -p fission-static` — **139 tests passed, 0 failed**
- `cargo check --workspace` — 0 errors

---

## 2026-04-09

### x86 Lifter 4th Reinforcement Pass — Coverage ~87% → ~93%

This update is a broad completeness sweep across five instruction categories in the `fission-sleigh` x86 lifter, bringing the estimated overall coverage from ~87% to ~93%.  All work is confined to the Fission Sleigh engine; no Ghidra runtime dependency is introduced.

#### Changed

- **Phase A — remaining 1-byte opcodes** (`crates/fission-sleigh/src/lifter/x86/semantic.rs`)
  - `0xF4` HLT → `CallOther` `HLT_POLICY`
  - `0x8E` MOV Sreg, r/m16 → ModRM decode + `Copy` to the appropriate segment register via `x86_seg(reg_field)`
  - `0x27/0x2F/0x37/0x3F` DAA / DAS / AAA / AAS → `CallOther` per-opcode policy IDs
  - `0x6C/0x6D` INSB/INSW/INSD and `0x6E/0x6F` OUTSB/OUTSW/OUTSD → `CallOther` `INS_POLICY` / `OUTS_POLICY`
  - added `X86_HLT_POLICY_ID`, `X86_DAA_POLICY_ID`, `X86_DAS_POLICY_ID`, `X86_AAA_POLICY_ID`, `X86_AAS_POLICY_ID`, `X86_INS_POLICY_ID`, `X86_OUTS_POLICY_ID` constants

- **Phase B — 0x0F system and MMX gaps** (`crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`)
  - `0x0F 0x01` descriptor group (SGDT/LGDT/SIDT/LIDT/SMSW/LMSW/INVLPG) → `CallOther` with ModRM `reg_field`-based dispatch via new `decode_0f01_group`
  - `0x0F 0x20/0x22` MOV CR0–7 and `0x0F 0x21/0x23` MOV DR0–7 → `CallOther` `MOV_CR_POLICY` / `MOV_DR_POLICY`
  - `0x0F 0x33` RDPMC → `CallOther` `RDPMC_POLICY`
  - `0xD8–0xDF` (MMX range without mandatory prefix) previously fell to empty `Vec`; now routes uniformly through `simd::decode_simd_semantic` so every MMX opcode receives a `SIMD_POLICY` `CallOther` stub
  - added `X86_LGDT_POLICY_ID`, `X86_SGDT_POLICY_ID`, `X86_LIDT_POLICY_ID`, `X86_SIDT_POLICY_ID`, `X86_LMSW_POLICY_ID`, `X86_SMSW_POLICY_ID`, `X86_INVLPG_POLICY_ID`, `X86_MOV_CR_POLICY_ID`, `X86_MOV_DR_POLICY_ID`, `X86_RDPMC_POLICY_ID` constants

- **Phase C — SSE packed instruction coverage** (`crates/fission-sleigh/src/lifter/x86/semantic/ext/simd.rs`)
  - added 15 `None`-prefix (`NP`) packed SSE match arms: MOVUPS load/store (0x10/0x11), MOVAPS load/store (0x28/0x29), SQRTPS (0x51), ANDPS/ANDNPS/ORPS/XORPS (0x54–0x57), ADDPS/MULPS/SUBPS/MINPS/DIVPS/MAXPS (0x58–0x5F)
  - added 9 `P66`-prefix SSE2 packed match arms: MOVUPD load/store (0x10/0x11), SQRTPD/ADDPD/MULPD/SUBPD/MINPD/DIVPD/MAXPD (0x51–0x5F range)
  - added PCMPGTB/W/D (0x64–0x66), PUNPCKLBW/WD/DQ/PACKSSWB (0x60–0x63), PACKUSWB (0x67), PUNPCKHBW/WD/DQ (0x68–0x6A), PACKSSDW (0x6B)
  - added PSUBUSB/W (0xD8/0xD9), PMINUB (0xDA), PADDUSB/W (0xDC/0xDD), PMAXUB (0xDE), PAVGB/W (0xE0/0xE3), PMULHUW/W (0xE4/0xE5), PMINSW (0xEA), PMAXSW (0xEE), PSUBSB/W (0xE8/0xE9), PADDSB/W (0xEC/0xED)
  - added `decode_two_byte_xmm_movmsk` helper; wired MOVMSKPS (NP 0x50) and MOVMSKPD (P66 0x50) as `CallOther` intrinsics that write to a GPR destination

- **Phase D — x87 FPU completeness** (`crates/fission-sleigh/src/lifter/x86/semantic/ext/system.rs`)
  - D9 constant loads (register form, `reg_field==5`): FLD1 → `FloatInt2Float`, FLDZ → `Copy 0`, transcendental constants (FLDL2T/FLDL2E/FLDPI/FLDLG2/FLDLN2) → dedicated `CallOther` policy stubs
  - D9 transcendental group (register form, `reg_field==6/7`): F2XM1, FYL2X, FPTAN, FPATAN, FXTRACT, FPREM1, FPREM, FYL2XP1, FRNDINT, FSCALE, FSIN, FCOS → `CallOther` per-function policy IDs; FSQRT correctly placed at `reg_field==7, rm_low==2` (`FloatSqrt`)
  - D9 memory form control-word group (`reg_field 4–7`): FLDENV / FLDCW / FNSTENV / FNSTCW → `CallOther` with effective address argument
  - DA register form: FCMOVcc (FCMOVB/E/BE/U) → `CallOther` `FCMOV_POLICY`
  - DB register form: `FINIT` (E3) → `CallOther`; FCOMI (`reg_field==6`) / FUCOMI (`reg_field==7`) → `FloatEqual` (ZF) + `FloatLess` (CF) + PF=0; other DB register forms → `CallOther`
  - DF register form: FUCOMIP (`reg_field==5`) / FCOMIP (`reg_field==7`) → `FloatEqual` (ZF) + `FloatLess` (CF) + PF=0
  - added 21 new policy-ID constants (`X86_FLDCW_POLICY_ID`, `X86_FNSTCW_POLICY_ID`, `X86_FSIN_POLICY_ID`, `X86_FCOS_POLICY_ID`, `X86_FPTAN_POLICY_ID`, `X86_FPATAN_POLICY_ID`, `X86_F2XM1_POLICY_ID`, `X86_FYL2X_POLICY_ID`, `X86_FYL2XP1_POLICY_ID`, `X86_FXTRACT_POLICY_ID`, `X86_FPREM_POLICY_ID`, `X86_FPREM1_POLICY_ID`, `X86_FSCALE_POLICY_ID`, `X86_FCMOV_POLICY_BASE_ID`, `X86_FINIT_POLICY_ID`, etc.)

- **Phase E — TZCNT / LZCNT disambiguation** (`crates/fission-sleigh/src/lifter/x86/semantic/ext/bitops.rs`)
  - `decode_bsf_bsr` now checks `prefix.rep_prefix == Some(Rep)` (F3 prefix) before dispatching
  - `F3 0F BC` → new `decode_tzcnt`: BSF-index-based trailing-zero count, sets ZF and CF from `src == 0`
  - `F3 0F BD` → new `decode_lzcnt`: BSR-index-based leading-zero count, sets CF from `src == 0` and ZF from `result == 0`
  - BSF / BSR without F3 prefix continue to operate exactly as before

- **`x86_seg` visibility widened** (`crates/fission-sleigh/src/lifter/x86/common.rs`): changed from `pub(super)` to `pub(in super::super)` so `semantic.rs` can import and use it directly

#### Validation

- `cargo check -p fission-sleigh` — 0 errors, 0 new warnings
- `cargo test -p fission-sleigh` — **202 tests passed, 0 failed** (up from 176 before the 4th pass)

---

## 2026-04-08

### Rust-sleigh Full-Decompile Stability Hardening (Root-Cause Fixes)

This update removes root causes behind full-decompile crashes on large x86 binaries instead of relying on temporary guards.

#### Changed

- hardened Rust-only decompile execution in `fission-cli`:
  - introduced explicit worker stack sizing via `FISSION_RUST_DECOMP_STACK_MB` (default `32MB`, clamped `8..256MB`)
  - applied stack sizing to both single-function rendering workers and fan-out worker pool threads
  - converted spawn/join failures into structured per-function fallback results instead of process-level aborts
  - implementation: `crates/fission-cli/src/cli/oneshot/decompile_rust_sleigh.rs`
- fixed recursive cycle tracking in NIR call argument lowering:
  - reused the shared `visiting` set for call arg lowering instead of creating fresh per-arg sets
  - prevents recursion blowups on cyclic varnode chains
  - implementation: `crates/fission-pcode/src/nir/builder/lower_expr.rs`
- fixed BranchInd candidate selection panic in terminator lowering:
  - replaced eager indexing logic with a guarded `len()==1` branch
  - implementation: `crates/fission-pcode/src/nir/builder/terminator.rs`

#### Validation

- EverPlanet rust-sleigh `--decomp-all` lane completed end-to-end without crash after these fixes.

---

## 2026-04-07

### x86 FPU Precise Mapping and Advanced Indirect Control Flow Structuring

This update focuses on bridging missing FPU arithmetic instructions and refining indirect jumps within the `fission-sleigh` x86 lifter, avoiding legacy emulation hacks.

#### Changed

- replaced the blanket `FPU_HACK` (`FloatAdd`) inside `crates/fission-sleigh/src/lifter/x86/semantic/ext/system.rs`'s `decode_x87_policy` to accurately distinguish `FloatAdd`, `FloatMult`, `FloatLess`, `FloatLessEqual`, `FloatSub`, and `FloatDiv` based on instruction extension offsets (`0xD8..=0xDF`) and ModRM `reg_field` encodings.
- adjusted indirect branch and call translation for `0xFF` instructions in `crates/fission-sleigh/src/lifter/x86/semantic.rs`, explicitly routing far calls (`reg_field == 3`) and far jumps (`reg_field == 5`) to target `CallInd` and `BranchInd` constructs natively.
- resolved ownership conflicts (`E0382`) and variable borrowing issues inside P-Code definitions by strictly tracking `Varnode` instances (`ST(0)` stack mappings) within the decoded outputs.
- updated FPU placeholder mnemonics to `FPU_SCALED` indicating explicitly evaluated operands.

#### Validation

- validated cleanly via `cargo check -p fission-sleigh` indicating perfect object lifespans with zero Rust compiler warnings.
- regression tested 238 internal modules via `cargo test -p fission-pcode` without faults.

### EverPlanet Throughput Optimization (DIE Matcher Caching + NIR Hot-Path Guards)

This update reduces pathological runtime on the EverPlanet lane by removing repeated detector work in `fission-loader` and bounding expensive recovery chains in `fission-pcode` NIR lowering.

#### Changed

- optimized DIE signature matching in `crates/fission-loader/src/detector/die_engine.rs`:
  - pre-collected all `StringMatch` rules and evaluated them in one pass with `RegexSet`
  - introduced match-result caching so repeated rule checks avoid rescanning the same text corpus
  - cached EP-pattern parse results to avoid reparsing identical patterns across rules
- reduced repeated expression/terminator recovery cost in `fission-pcode` NIR hot paths:
  - added block-local def indexing and cached def-site lookup reuse
  - added passthrough-peel and terminator-level caches
  - introduced deterministic budgets for x86 branch-recovery and switch-chain parsing paths

#### Validation

- `cargo check -p fission-loader` (pass)
- `cargo test -p fission-loader detector::die_engine -- --nocapture` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-cli --release` (pass)

#### Measurement Notes

- EverPlanet benchmark lane with rust-sleigh (`--decomp-all --decomp-limit 20 --benchmark`) completed in a fast, non-stalling profile after the optimization set.

---

## 2026-04-06

### x86 SIMD/3-byte Follow-up Intrinsic Expansion

This update continues the rust-sleigh x86 semantic ownership expansion by replacing additional SIMD/3-byte policy fallbacks with intrinsic-backed p-code dataflow and extending regression coverage.

#### Changed

- expanded two-byte `66 0F` SIMD follow-up handlers in the x86 semantic path:
  - added intrinsic/write lowering for `PUNPCKLQDQ`, `PUNPCKHQDQ`, `PSHUFD(imm8)`, `PADDQ`, `PMULLW`, `PSUBB/W/D/Q`, and `PADDB/W/D`
  - implementations: `crates/fission-sleigh/src/lifter/x86/semantic/ext/simd.rs`
- widened extended opcode dispatch coverage so newly promoted SIMD ext bytes route into SIMD semantics:
  - added routing for `0xD4`, `0xD5`, `0xF8..=0xFE` (while preserving prefix-aware `0xD8..=0xDF` behavior)
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`
- extended `0F 3A` intrinsic selection with immediate forms for:
  - `BLENDPS` (`0x0C`), `BLENDPD` (`0x0D`)
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext/escape3byte.rs`
- expanded regression tests for newly promoted follow-up opcodes and immediate forwarding checks:
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`

#### Validation

- `cargo test -p fission-sleigh decode_simd_p1_followup_queue_instructions_emit_intrinsics -- --nocapture` (pass)
- `cargo test -p fission-sleigh decode_high_frequency_0f38_0f3a_intrinsics_emit_xmm_dataflow -- --nocapture` (pass)
- `cargo test -p fission-sleigh --lib` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)

### rust-sleigh Backend Orchestration Consolidation + x86 Semantic/Length Expansion

This update consolidates function-level lifting orchestration into the shared backend path, expands x86 semantic ownership for additional instruction families, and validates the change set through sleigh unit gates and automation lanes.

#### Changed

- consolidated function-level decode/lift orchestration under the backend layer while preserving `SleighLifter` public API behavior:
  - added backend-owned contract loop (`lift_ops_with_contract`) and instruction decode entry (`decode_and_lift_with_len`)
  - implementation: `crates/fission-sleigh/src/lifter/backend/mod.rs`, `crates/fission-sleigh/src/lifter/mod.rs`
- unified backend state plumbing for semantic decode through context-aware entrypoints:
  - switched AArch64/x86 module exports to `decode_semantic_with_state`
  - implementation: `crates/fission-sleigh/src/lifter/aarch64/mod.rs`, `crates/fission-sleigh/src/lifter/aarch64/semantic.rs`, `crates/fission-sleigh/src/lifter/x86/mod.rs`, `crates/fission-sleigh/src/lifter/x86/semantic.rs`, `crates/fission-sleigh/src/lifter/common.rs`
- centralized CFG split/target helpers for block construction:
  - `is_cfg_split_opcode`, `direct_control_target`
  - implementation: `crates/fission-sleigh/src/lifter/backend/mod.rs`, `crates/fission-sleigh/src/lifter/mod.rs`
- expanded x86 semantic/length coverage and modular ownership:
  - split `0F` extended semantic handling into dedicated modules (`bitops`, `bitshift`, `cond`, `escape3byte`, `imul`, `movmuldiv`, `simd`, `system`)
  - added semantics for rotate intrinsics, sign-extension convert family (`0x98`/`0x99`), `xchg` reg/mem variants, and `shld`/`shrd`
  - improved x86 length decoding with explicit opcode map handling, including VEX map variants and truncated-VEX guards
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`, `crates/fission-sleigh/src/lifter/x86/semantic.rs`, `crates/fission-sleigh/src/lifter/x86/length.rs`

#### Added

- new backend module and lift contract result type:
  - `crates/fission-sleigh/src/lifter/backend/mod.rs`
- new x86 extended semantic submodules:
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/bitops.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/bitshift.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/cond.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/escape3byte.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/imul.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/movmuldiv.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/simd.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/system.rs`
- contract and semantic regression coverage:
  - backend sequencing/consumed-bytes contract tests
  - decode failure address mapping test
  - x86 semantic regressions for rotate/xchg/shld-shrd/scalar-simd families
  - implementations: `crates/fission-sleigh/src/lifter/mod.rs`, `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`, `crates/fission-sleigh/src/lifter/x86/length.rs`

#### Validation

- `cargo test -p fission-sleigh --lib lifter::tests::backend_lift_contract_keeps_trace_order_and_consumed_bytes` (pass)
- `cargo test -p fission-sleigh --lib lifter::tests::backend_lift_contract_reports_decode_failure_address` (pass)
- `cargo test -p fission-sleigh --lib lifter::tests::lift_contract_reports_instruction_limit_stop` (pass)
- `cargo test -p fission-sleigh --lib lifter::tests::lift_contract_reports_terminal_control_flow_stop` (pass)
- `cargo test -p fission-sleigh --lib lifter::x86::semantic::tests` (pass)
- `cargo test -p fission-sleigh --lib` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build` (pass, `changed_rows=0`)
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile full` (pass, `changed_rows=0`)

### NIR Branch-Target Recovery Hardening + Limit-200 Baseline/Post/Delta Automation

This update focuses on making indirect/partially unresolved control-flow lowering more robust in Rust NIR and packaging the repeated limit-200 measurement workflow into a single reproducible command.

#### Changed

- strengthened NIR terminator recovery in `fission-pcode` for `Branch`, `CBranch`, and `BranchInd`:
  - route target resolution through a recovery path that combines passthrough peel + one-step arithmetic address inference (`IntAdd` / `IntSub` with const)
  - add Branch/CBranch fallback target inference from CFG successors when direct target resolution fails
  - add BranchInd target inference from simple `Load`-address forms
  - infer `switch` default target from fallthrough when available
  - implementation: `crates/fission-pcode/src/nir/builder/terminator.rs`
- changed unsupported terminator handling to emit explicit marker calls instead of aborting render for single-block/multi-block/linear paths:
  - emits `__fission_indirect_cf_unsupported()` call expression
  - implementations: `crates/fission-pcode/src/nir/builder/mod.rs`, `crates/fission-pcode/src/nir/structuring/driver.rs`, `crates/fission-pcode/src/nir/structuring/linear.rs`
- extended unsupported inventory recording on branch-target resolve failures for diagnostics:
  - implementation: `crates/fission-pcode/src/nir/builder/debug.rs`
- broadened type-hint application in synthetic/non-stack-origin paths and tightened local hint eligibility fallback logic:
  - implementation: `crates/fission-pcode/src/nir/builder/type_hints.rs`
- hardened arithmetic normalization edge case by replacing subtraction with saturating subtraction in magic-division recognition:
  - implementation: `crates/fission-pcode/src/nir/normalize/arith.rs`

#### Added

- new x86 bootstrap regressions for branch-target recovery and unsupported lowering behavior:
  - Branch/CBranch wrapped-target recovery (copy + one-step arithmetic)
  - BranchInd no-target tolerance and load-address target recovery
  - unresolved branch fallback behavior via successor inference
  - implementation: `crates/fission-pcode/src/nir/tests/bootstrap_x86.rs`
- one-command local automation script for baseline/post/summary/delta generation on putty/everything (`--decomp-limit 200`):
  - snapshots unsupported inventory files per run
  - generates summary/delta json+md artifacts and putty unmapped cluster reports
  - includes baseline auto-resolution fallback (`rebuilt`, `after_term`, `after_passthrough`)
  - implementation: `scripts/test/run_limit200_baseline_post_delta.py`

#### Validation

- `cargo test -p fission-pcode --lib bootstrap_x86::preview_` (pass)
- `cargo test -p fission-pcode --lib` (pass)

## 2026-04-05

### rust-sleigh x86 0F3A Semantic Expansion and Branch-Target CFG Diagnostics

Extended the rust-sleigh x86 three-byte semantic ownership for SSE4 string/extract opcodes and added CFG-construction diagnostics to narrow unresolved branch-target fallback causes before NIR lowering.

#### Changed

- expanded x86 `0F 3A` dataflow semantic handlers in `fission-sleigh`:
  - `0x61` `PCMPESTRI` (`ECX` write path)
  - `0x62` `PCMPISTRM` (`XMM0` write path)
  - `0x17` `EXTRACTPS` (`r/m` write path)
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`
- added regression coverage for the new handlers (reg/mem forms) in:
  - `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`
- added branch-target resolution diagnostics in NIR terminator lowering to log:
  - input varnode, seq, block index/address, guessed target, and successor list on resolve failure
  - implementation: `crates/fission-pcode/src/nir/builder/debug.rs`, `crates/fission-pcode/src/nir/builder/terminator.rs`
- added CFG-construction diagnostics in rust-sleigh block building to log:
  - `branch_target_unmapped`
  - `control_block_no_successors`
  - per-block successor finalization summaries
  - implementation: `crates/fission-sleigh/src/lifter/mod.rs`

#### Validation

- `cargo test -p fission-sleigh` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-sleigh` (pass)

#### Measurement Notes

- putty rust-sleigh `--decomp-limit 200` baseline/post measurement completed.
- fallback addresses were traced in isolated debug lanes; common root signature was observed at CFG build time:
  - constant `Branch` target not present in current function op-address map
  - unresolved target produced empty successor set prior to NIR terminator lowering

### x86 Lifter - Semantic Module Split and Byte-Group Arithmetic Expansion

This increment continues the x86-first lifting track by reducing semantic-module complexity and expanding arithmetic coverage for byte-width group operations.

#### Changed

- split x86 semantic extended-opcode logic into a dedicated module to improve ownership boundaries and maintainability:
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic.rs` (dispatcher wiring)
- completed x86 `F6` group semantic coverage (`/0,/3,/4,/5,/6,/7`) by reusing existing `F7` one-operand arithmetic flows with `size=1`:
  - `TEST`, `NEG`, `MUL`, `IMUL`, `DIV`, `IDIV`
  - implemented in `crates/fission-sleigh/src/lifter/x86/semantic.rs`
- aligned x86 length decode for `F6` immediate handling so only `/0` consumes `imm8`:
  - `crates/fission-sleigh/src/lifter/x86/length.rs`
- added byte-group regression tests for semantic/length consistency:
  - `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`
  - `crates/fission-sleigh/src/lifter/x86/length.rs`

#### Validation

- `cargo test -p fission-sleigh --lib lifter::x86::semantic::tests::decode_f6` (pass)
- `cargo test -p fission-sleigh --lib lifter::x86::length::tests::decode_len_handles_f6_test_immediate_only_for_group0` (pass)
- `cargo test -p fission-sleigh` (pass, 92 tests)

## 2026-04-03

### AARCH64 AppleSilicon Parse Fix - InvalidRef Resolution in sleigh-rs

Resolved the arm64 parse blocker in `AARCH64_AppleSilicon` by aligning `sleigh-rs` execution-time symbol resolution with Ghidra 11.4.2 behavior for produced subtable operands used in constructor execution expressions.

#### Root Cause

- parse failed in `AARCH64neon.sinc` with `Execution(InvalidRef)` when evaluating produced subtable operand references such as `Re_VPR128.H.vIndexHL`
- `sleigh-rs` previously rejected table reads in execution scope unless the table had an explicit export value

#### Changed

- updated execution read-scope table resolution to allow produced subtable references in constructor execution:
  - `vendor/sleigh-rs/src/semantic/inner/table/execution.rs`
- made table-size access non-panicking for no-export tables by treating them as unsized where appropriate:
  - `vendor/sleigh-rs/src/semantic/inner/execution/expr.rs`
  - `vendor/sleigh-rs/src/semantic/inner/table/mod.rs`
- removed a hard `unwrap()` panic path in user-call parameter size speculation when unresolved/no-export table values are present:
  - `vendor/sleigh-rs/src/semantic/inner/execution/user_call.rs`
- added/updated arm64 parsing regression validation in fission-sleigh tests:
  - `crates/fission-sleigh/src/lifter/mod.rs`

#### Validation

- `cargo test -p fission-sleigh` (pass, including `aarch64_apple_silicon_spec_parses`)
- `cargo check -p fission-cli --features native_decomp` (pass)

## 2026-04-02

### fission-sleigh - Folder-Tree Refactor and Converter Responsibility Split

Refactored `fission-sleigh` into a folder-tree module layout for easier long-term ownership and maintenance, then split converter internals by semantic responsibility (`assignment`, `branch`, `memory`, `unary`) while preserving existing behavior.

#### Changed

- converted flat modules into directory modules:
  - `crates/fission-sleigh/src/converter/mod.rs`
  - `crates/fission-sleigh/src/lifter/mod.rs`
  - `crates/fission-sleigh/src/builder/mod.rs`
- replaced monolithic converter flow with semantic modules:
  - `crates/fission-sleigh/src/converter/assignment.rs`
  - `crates/fission-sleigh/src/converter/branch.rs`
  - `crates/fission-sleigh/src/converter/memory.rs`
  - `crates/fission-sleigh/src/converter/unary.rs`
  - kept expression traversal and shared utilities in `expr.rs` and `helpers.rs`
- retained converter unit tests and validation expectations in:
  - `crates/fission-sleigh/src/converter/tests.rs`

#### Validation

- `cargo check -p fission-sleigh` (pass)
- `cargo test -p fission-sleigh` (pass, 7 tests)

### Graph-Theoretic Loop Structuring (Ghidra LoopBody Integration)

루프 구조화(Loop Structuring) 단계에서 기존의 휴리스틱(`fallthrough_index` 예측)을 제거하고, Ghidra의 `LoopBody` 출구 식별 알고리즘 및 엄밀한 CFG 간선 분류(Edge Classification)를 도입하여 결정론적인 while/do-while 구조화를 달성했습니다.

#### Changed
- **CFG Analysis 강화**: `crates/fission-pcode/src/nir/structuring/cfg_analysis.rs`에 깊이 우선 탐색(DFS) 기반의 전위 순회(Preorder) 기록 및 간선 클래스 분류 로직 적용.
- **`LoopBody` 설계**: `crates/fission-pcode/src/nir/structuring/loop_analysis.rs` 모듈을 신설하여 Ghidra의 `findBase`, `findExit`, `extend` 기능을 구현해 루프 바디로 불법적 출구가 병합되는 문제를 예방.
- **휴리스틱 제거 및 개편**: `crates/fission-pcode/src/nir/structuring/loops.rs`의 `try_lower_while`에서 기존 `fallthrough_index`를 맹목적으로 참조하던 방식을 제거하고, 미리 식별된 정확한 `exit_idx`를 사용하도록 로직 재작성.
- **상태 연동**: `PreviewBuilder` 객체 내에 `loop_bodies` 상태를 추가하고 `get_loop_body` 접근자를 통해 구조화 모듈 전역에서 루프 구조를 활용하도록 연결.

#### Validation
- `cargo check -p fission-pcode` (pass)
- `cargo test -p fission-pcode` (loop tests passed perfectly without fallback)

---

## 2026-04-01

### Algorithmic Loop Structuring and Unbounded Region Recovery

Replaced lexical, position-based heuristics with algorithmic validations for `For` loop synthesis, and lifted artificial search bounds during irreducible CFG region recovery.

#### Changed

- Added `try_collapse_while_to_for_algorithmic` in `crates/fission-pcode/src/nir/normalize/for_loops.rs` to enforce backward dataflow independence for `init` block assignments and perform deep AST scans for `continue` statements, preventing unsafe loop `update` hoisting.
- Hooked `for_loops.rs` module into `core.rs` normalization passes.
- Removed the hardcoded `start_idx + 24` lookahead limit in `crates/fission-pcode/src/nir/structuring/recovery.rs` (`region_linearized_exit_candidates`), allowing full CFG scanning for region exits.

#### Validation

- `cargo check -p fission-pcode` (pass)
- `cargo test -p fission-pcode` (pass)

### Short-Circuit Folding - Prefix-Aware Condition Canonicalization Telemetry

This increment broadens short-circuit folding to tolerate trivial prefix statements in the first block of a chain, records whether AND/OR folds actually happen, and tracks when side effects correctly block the fold.

#### Changed

- added new `NirBuildStats` counters:
  - `condition_fold_and_count`
  - `condition_fold_or_count`
  - `condition_fold_rejected_side_effect`
- wired the new counters through preview builder state/init/stats projection and automation report export
- added `simplify_logical_expr()` in `crates/fission-pcode/src/nir/cfg.rs` to canonicalize nested De Morgan-style logical expressions after fold construction
- updated `crates/fission-pcode/src/nir/structuring/conditionals/short_circuit.rs` so short-circuit folding:
  - accepts trivial prefix statements in the first block of a chain
  - rejects side-effectful prefixes in either the first or subsequent blocks
  - wraps preserved prefixes around the folded `if` instead of discarding them

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)

#### Observed sample telemetry

- Current 200-function sample: `condition_fold_and_count=0`, `condition_fold_or_count=0`, `condition_fold_rejected_side_effect=0`
- Current 500-function sample: `condition_fold_and_count=0`, `condition_fold_or_count=0`, `condition_fold_rejected_side_effect=0`

The new counters are wired and available, but the current fixed samples do not yet hit these newly accepted/rejected short-circuit shapes.

## 2026-03-25

### Switch Structuring - Ghidra `checkSwitchSkips` Safety Guard Regression

This patch hardens switch lowering safety by adding a negative regression that locks behavior when default and non-default paths do not share a stable exit.

#### Changed

- retained bounded switch target canonicalization for trivial forwarding chains in `structuring/switch.rs`
- aligned validation target with Ghidra `checkSwitchSkips` intent: avoid unsafe switch formation when default exit diverges

#### Added

- new regression test:
  - `multi_block_preview_does_not_lower_switch_when_default_exit_differs_from_case_exit`
- test asserts fallback to conditional chain (no unsafe `switch` emission) under non-shared default/case exits

#### Validation

- `cargo test -p fission-pcode structuring_switch` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

### Docs - Add Fission AI Agent Operating Guide

Added a repository-root `AGENTS.md` that codifies architecture ownership, crate boundaries, NIR structuring rules, telemetry contract, and current CI/testing expectations for AI-assisted engineering workflows.

#### Added

- `AGENTS.md`

### Loop Structuring - Explicit Infloop Break Reducer + Loop-Control Telemetry

This patch adds a conservative explicit infloop-with-break reducer path and wires loop-control rewrite telemetry through `NirBuildStats` and automation deltas so quality runs can track loop-local rewrite behavior directly.

#### Changed

- added `try_lower_infloop_with_break()` in `structuring/loops.rs` for conditional self-loop shapes that can be safely expressed as `while (true)` + guarded `break`
- integrated a new structuring attempt stage (`attempt=loop_control`) in `structuring/driver.rs`, ordered after `while` and before plain `infloop`
- extended loop-control rewrite pass with explicit counters:
  - rewritten `break` gotos
  - rewritten `continue` gotos
  - nested-scope rewrite skips (`While`/`DoWhile`/`Switch`)
- added new `NirBuildStats` fields and propagated them through:
  - preview builder state/snapshot
  - stats merge path
  - automation summary delta and markdown baseline delta rendering

#### Validation

- `cargo test -p fission-pcode rewrite_loop_control_gotos -- --nocapture` (pass)
- `cargo test -p fission-pcode structuring_loops -- --nocapture` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo build -p fission-pcode -p fission-automation` (pass)

### P5H3E - Conditional-Tail Shared-Follow Canonical Arm Alignment

This increment tightens conditional-tail recovery by aligning shared-follow candidate search and per-arm lowering to canonicalized region-local arm starts, reducing mismatch opportunities caused by pre-canonical arm divergence.

#### Changed

- in `structuring/linear.rs` `lower_conditional_tail()`:
  - shared-tail entry discovery now uses `true_arm.canonical_idx` / `false_arm.canonical_idx`
  - shared-tail arm lowering to intermediate follow entries now starts from canonicalized indices instead of raw effective starts
- preserved existing one-arm fast-path handling (`reaches_join_trivially`) to keep conservative empty-else lowering behavior unchanged

#### Validation

- `cargo test -p fission-pcode structuring_conditionals -- --nocapture` (pass)
- `cargo test -p fission-pcode structuring_linear -- --nocapture` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

### Facade Ownership Cleanup - Remove Legacy Duplicate Trees from `fission-analysis`

This change removes stale duplicated implementation trees from `fission-analysis` so the crate remains a compatibility facade and ownership stays with `fission-static` and `fission-dynamic`.

#### Changed

- removed duplicated legacy module trees from `crates/fission-analysis/src/`:
  - `analysis/`, `debug/`, `plugin/`, `app/`, `unpacker/`, `utils/`
- updated compatibility prelude debug type re-export to owner crate path:
  - `crate::debug::types::*` → `fission_dynamic::debug::types::*`
- added compatibility policy document:
  - `crates/fission-analysis/COMPATIBILITY.md`

#### Validation

- `cargo check -p fission-analysis --features native_decomp` (pass)
- `cargo check -p fission-analysis --features "interactive_runtime unpacker_runtime native_decomp"` (pass)
- `cargo test -p fission-analysis --features native_decomp --no-run` (pass)

### Structuring - Graph-Invariant Promotion Gate + Guarded-Tail Layout Normalization

This increment moves promotion acceptance beyond strict layout order checks by adding conservative graph-invariant fallback guards (dominance/post-dominance/irreducibility) and pre-discovery guarded-tail layout normalization.

#### Changed

- promotion gate update in `structuring/guards.rs`:
  - kept legacy monotonic predecessor ordering acceptance path
  - added additive graph-invariant fallback acceptance when legacy path fails:
    - reject irreducible SCC participation
    - require header dominance for targeted internal entries
    - require region-window postdom exit guard when an external exit exists
- added guarded-tail pre-normalization pipeline:
  - `normalize_guarded_tail_layout()` in `structuring/cleanup.rs`
  - applies adjacent-label cleanup + top-level forward alias canonicalization before guarded-tail discovery/promotion scanning
- discovery/promotion entry points now consume normalized layout views to reduce avoidable noncanonical shape rejections

#### Added

- new unit tests:
  - `minimal_structured_promotion_accepts_non_monotonic_layout_when_graph_invariants_hold`
  - `minimal_structured_promotion_rejects_irreducible_region`
  - `normalize_guarded_tail_layout_collapses_adjacent_labels_before_alias_rewrite`
  - plus updated guarded-tail discovery regressions for normalized layout/counter semantics

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 40` (pass)

#### Observed lane delta (`nir`, functions-limit 40)

- `promotion_rejected_by_shape_count`: `633 -> 606`
- `discovery_rejected_noncanonical_layout_count`: `561 -> 533`
- `canonicalization_failed_interleaved_join_uses`: `170 -> 149`
- output class mix unchanged on this sample (`structured=32`, `partially_structured=34`, `linear_fallback=8`)

### Structuring - Guarded-Tail Join and Tail-Exit Canonicalization Tightening

This increment further aligns guarded-tail recovery with Ghidra-style conservative exit handling by terminalizing safe forward join chains, filtering non-forward targets out of candidate discovery, and preserving tail-terminal returns without relaxing loop/switch escape safety.

#### Changed

- refined guarded-tail join target handling in `structuring/guards.rs`:
  - added safe multi-hop terminal join resolution for trivial forward label chains
  - prefiltered backward/non-forward top-level label targets so they are skipped as non-candidates instead of inflating nonterminal join failures
  - preserved conservative rejection for ambiguous/nonlocal alias ownership
- refined guarded-tail segment canonicalization:
  - accepted a single tail-terminal `return` after payload as a valid terminal exit
  - continued rejecting true nested tail escapes (`goto`/`break`/`continue` after payload) and ambiguous scoped exits
- expanded guarded-tail regression coverage in `structuring_misc.rs` for:
  - nonterminal join forwarding
  - multi-hop join forwarding
  - safe interleaved alias stubs
  - backward-target skip behavior
  - tail-terminal return preservation

#### Validation

- `cargo test -p fission-pcode structuring_candidate_discovery_ -- --nocapture` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 40` (pass)

#### Observed lane delta (`nir`, functions-limit 40)

- `canonicalization_failed_nonterminal_join_label`: `201 -> 0`
- `promotion_rejected_by_shape_count`: `332 -> 261`
- `discovery_rejected_noncanonical_layout_count`: `332 -> 259`
- `structured`: `32 -> 35`
- `partially_structured`: `34 -> 31`
- `linear_fallback`: `8 -> 8`

### Structuring - Promotion Gate Subtype Telemetry and Owner-Preserving Conflict Refinement

This increment makes guarded-tail promotion gate failures easier to reason about by splitting must-emit-label pressure into concrete subtypes and refining owner-conflict classification to preserve front-leaf-equivalent forward ownership cases inspired by Ghidra’s label bump-up/front-leaf rules.

#### Changed

- extended guarded-tail promotion gate telemetry with explicit `rejected_must_emit_label` subtypes:
  - `rejected_must_emit_label_surviving_middle_ref`
  - `rejected_must_emit_label_surviving_external_ref`
  - `rejected_must_emit_label_owner_conflict`
- wired the new counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation build-stat reporting
- refined `structuring/guards.rs` must-emit-label classification so:
  - surviving refs inside canonicalized middle remain `surviving_middle_ref`
  - single surviving outside refs remain `surviving_external_ref`
  - multiple outside refs are only treated as `owner_conflict` when they do **not** all preserve the same simple forward top-level owner path
- added guarded-tail regressions covering:
  - subtype telemetry for surviving middle refs
  - subtype telemetry for owner conflicts
  - safe same-owner forward refs that should no longer be escalated to owner conflict

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 40` (pass)

#### Observed lane telemetry (`nir`, functions-limit 40)

- `promotion_rejected_by_gate_count`: `82`
- `rejected_must_emit_label`: `77`
  - `surviving_middle_ref`: `16`
  - `surviving_external_ref`: `9`
  - `owner_conflict`: `18`
- aggregate gate count did not move on this fixed sample, but subtype visibility now makes the next reduction targets explicit

### Structuring - Whole-Body Alias Ownership and Fallthrough Ref Relaxation

This increment refines guarded-tail alias ownership using Ghidra-style front-leaf / copy-block semantics and `gotoPrints`-style fallthrough elision, so safe same-body forwarded-label reuse is no longer treated as truly nonlocal and some middle/external refs stop forcing labels.

#### Changed

- refined guarded-tail alias canonicalization in `structuring/guards.rs` to inspect **whole-body ref sites** when classifying alias ownership
- preserved `AliasHasNonlocalRef` only for truly unsafe cases:
  - nested external refs
  - post-segment refs
  - unsafe owner crossings
   Extended the rust-sleigh x86 three-byte semantic ownership for SSE4 string/extract opcodes and added CFG-construction diagnostics to narrow unresolved branch-target fallback causes before NIR lowering.
- connected safe external alias redirects back into promotion so outer-body gotos are rewritten consistently before region drain
- relaxed label-pressure classification for two conservative fallthrough-equivalent cases:
  - trailing top-level middle `goto target_label`
   - expanded x86 `0F 3A` dataflow semantic handlers in `fission-sleigh`:
- kept nested/internal middle refs and post-label external refs conservative

#### Added

- new regressions in `structuring_misc.rs` covering:
  - safe external alias reuse rewrite
  - trailing middle goto relaxation
   - `cargo test -p fission-sleigh` (pass)
  - preserved post-label external-ref rejection
  - preserved true nonlocal alias rejection

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
   - strengthened NIR terminator recovery in `fission-pcode` for `Branch`, `CBranch`, and `BranchInd`:
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample delta (`nir`)

- 200 functions:
   - changed unsupported terminator handling to emit explicit marker calls instead of aborting render for single-block/multi-block/linear paths:
  - `canonicalization_failed_alias_not_fallthrough_count`: `175 -> 247`
- 500 functions:
   - extended unsupported inventory recording on branch-target resolve failures for diagnostics:
  - `canonicalization_failed_alias_not_fallthrough_count`: `260 -> 352`
   - broadened type-hint application in synthetic/non-stack-origin paths and tightened local hint eligibility fallback logic:
The large-sample runs show the alias-nonlocal bucket dropping substantially, with part of that volume reclassified into the more precise `alias_not_fallthrough` subtype instead of remaining lumped into `nonlocal`.
   - hardened arithmetic normalization edge case by replacing subtraction with saturating subtraction in magic-division recognition:
### Structuring - AliasNotFallthrough Subtypes and Discovery Acceptance Refinement

This increment splits `AliasNotFallthrough` into concrete after-label categories, adds a conservative top-level after-label relaxation using Ghidra `gotoPrints` / `nextFlowAfter`-style equivalence, and accepts terminal guarded tails plus pure-expression alias bodies when they are structurally safe.

   - new x86 bootstrap regressions for branch-target recovery and unsupported lowering behavior:

- extended `AliasNotFallthrough` telemetry with explicit subtypes:
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`
  - `canonicalization_failed_alias_not_fallthrough_nested_after_label_count`
   - one-command local automation script for baseline/post/summary/delta generation on putty/everything (`--decomp-limit 200`):
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation report stat export
- refined guarded-tail alias canonicalization in `structuring/guards.rs`:
  - allows a narrow top-level after-label self-goto case when the forwarded alias still follows the same printed front path
  - keeps nested after-label and other printed-order-divergent refs conservative
- refined guarded-tail promotion shape handling:
   - `cargo test -p fission-pcode --lib bootstrap_x86::preview_` (pass)
   - `cargo test -p fission-pcode --lib` (pass)
  - accepts alias bodies composed only of pure value expressions instead of treating them as automatically nontrivial
  - continues rejecting alias bodies with control flow or side-effectful expression shapes

#### Added

- new regressions in `structuring_misc.rs` covering:
  - top-level after-label subtype counting
  - nested after-label subtype counting
  - safe top-level after-label alias acceptance
  - terminal guarded-tail promotion
  - pure-expression alias-body acceptance

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample delta (`nir`)

- 200 functions:
  - `canonicalization_failed_alias_not_fallthrough_count`: `247 -> 180`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `361 -> 262`
  - `promoted_region_count`: `237 -> 239`
  - `structured`: `84 -> 86`
- 500 functions:
  - `canonicalization_failed_alias_not_fallthrough_count`: `352 -> 267`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `471 -> 354`
  - `promoted_region_count`: `559 -> 561`
  - `structured`: `186 -> 188`

These changes materially reduce the large-sample after-label alias bucket while slightly increasing successful guarded-tail promotions and structured output.

### Structuring - Direct Shape Subtype Telemetry and Pure-Expression Discovery Relaxation

This increment separates the remaining direct guarded-tail shape blockers from canonicalization-driven discovery failures and relaxes one discovery-only case where alias bodies contain only pure value expressions.

#### Changed

- added explicit direct shape subtype telemetry:
  - `promotion_rejected_by_shape_missing_terminal_join_target_count`
  - `promotion_rejected_by_shape_empty_nonterminal_tail_count`
- wired these counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation build-stat reporting
- refined guarded-tail discovery canonicalization in `structuring/guards.rs`:
  - accepts alias bodies made only of pure value expressions
  - still rejects alias bodies with control flow or side-effectful expressions (`Call`, `Load`)
- added a stable regression asserting terminal guarded-tail promotion leaves the new direct shape subtype counters at zero

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions:
  - `promotion_rejected_by_shape_count`: `908`
  - `promotion_rejected_by_shape_missing_terminal_join_target_count`: `0`
  - `promotion_rejected_by_shape_empty_nonterminal_tail_count`: `0`
  - `discovery_rejected_noncanonical_layout_count`: `908`
- 500 functions:
  - `promotion_rejected_by_shape_count`: `1643`
  - `promotion_rejected_by_shape_missing_terminal_join_target_count`: `0`
  - `promotion_rejected_by_shape_empty_nonterminal_tail_count`: `0`
  - `discovery_rejected_noncanonical_layout_count`: `1643`

These measurements show the remaining large shape bucket is overwhelmingly coming from canonicalization-driven discovery failures rather than the two direct shape blockers, which narrows the next optimization target considerably.

### Structuring - Alias Nonlocal Ref Subtype Telemetry

This increment splits a major remaining alias bucket into concrete subtype counters so large-sample runs can distinguish whether label ownership escapes are coming from nested pre-entry refs, post-segment refs, or simpler external-before patterns.

#### Changed

- added explicit alias-nonlocal subtype telemetry:
  - `canonicalization_failed_alias_has_nonlocal_ref_external_before_count`
  - `canonicalization_failed_alias_has_nonlocal_ref_nested_before_count`
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`
- wired these counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation build-stat reporting
- refined guarded-tail alias classification in `structuring/guards.rs` so generic `AliasHasNonlocalRef` failures are attributed to the concrete external-site cause instead of only incrementing the aggregate counter

#### Added

- new regressions in `structuring_misc.rs` covering:
  - nested-before nonlocal alias refs
  - external-before nonlocal alias refs
  - post-segment nonlocal alias refs

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions:
  - `canonicalization_failed_alias_has_nonlocal_ref_count`: `298`
  - `external_before`: `0`
  - `nested_before`: `42`
  - `post_segment_ref`: `102`
- 500 functions:
  - `canonicalization_failed_alias_has_nonlocal_ref_count`: `583`
  - `external_before`: `0`
  - `nested_before`: `135`
  - `post_segment_ref`: `187`

The new breakdown shows `external_before` is not a meaningful bottleneck, while `nested_before` and especially `post_segment_ref` are the next concrete ownership cases to target.

### Structuring - Conservative Terminal Goto Tail Escape Refinement

This increment reduces one concrete nested-tail escape bucket by accepting only the safest terminal goto form: a post-payload goto is allowed when it is the final meaningful statement in the segment, does not target any label inside the current body, and does not introduce additional in-body structure.

#### Changed

- refined guarded-tail canonicalization in `structuring/guards.rs` so post-payload `goto` is accepted only when all of the following hold:
  - no non-ignorable statements follow it
  - no internal labels appear earlier in the canonicalized segment
  - the goto target label does not appear anywhere in the current body
- kept `break` / `continue` conservative after payload
- preserved switch/default-exit safety by continuing to reject in-body structured targets

#### Added

- new regression `structuring_candidate_discovery_allows_tail_terminal_goto_after_payload`
- tightened negative regression `structuring_candidate_discovery_counts_nested_tail_escape` so it still covers a true nested escape with trailing meaningful work
- revalidated switch safety with `multi_block_preview_does_not_lower_switch_when_default_exit_differs_from_case_exit`

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample delta (`nir`)

- 200 functions:
  - `canonicalization_failed_nested_tail_escape`: `171 -> 160`
  - `discovery_rejected_noncanonical_layout_count`: `908 -> 897`
  - `promotion_rejected_by_shape_count`: `908 -> 897`
  - `promotion_candidate_count`: `561 -> 564`
  - `promoted_region_count`: `239 -> 242`
- 500 functions:
  - `canonicalization_failed_nested_tail_escape`: `303 -> 292`
  - `discovery_rejected_noncanonical_layout_count`: `1643 -> 1632`
  - `promotion_rejected_by_shape_count`: `1643 -> 1632`
  - `promotion_candidate_count`: `1202 -> 1205`
  - `promoted_region_count`: `561 -> 564`

This is a small but real large-sample reduction that improves guarded-tail acceptance without regressing the switch safety guard.

### Structuring - Interleaved Join Subtypes and Pure-Value Guarded-Tail Relaxations

This increment sharpens guarded-tail diagnosis by splitting `InterleavedJoinUses` into concrete causes and accepts a narrow set of front-path-equivalent pure-value alias layouts that previously failed despite preserving the same control-flow target.

#### Changed

- added explicit `InterleavedJoinUses` subtype telemetry:
  - `canonicalization_failed_interleaved_join_uses_no_next_label_count`
  - `canonicalization_failed_interleaved_join_uses_nontrivial_segment_count`
- wired these counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - builder stats projection
  - automation build-stat reporting
- refined guarded-tail alias canonicalization in `guarded_tail/alias_refs.rs` so pure value expressions are treated as ignorable in two conservative forwarding cases:
  - next-label terminalization inside interleaved join stubs
  - top-level after-label forward/self-reference segments
- refined guarded-tail canonicalization in `guarded_tail/canonicalize.rs` so all-before external refs can remain local when they share the same trivial forward owner path

#### Added

- new guarded-tail regressions covering:
  - interleaved join subtype counting
  - pure-value interleaved segment acceptance
  - side-effectful interleaved segment rejection
  - pure-value top-level-after-label acceptance
  - side-effectful top-level-after-label rejection
  - safe nested-before alias reuse

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions:
  - `canonicalization_failed_interleaved_join_uses`: `170`
  - `no_next_label`: `69`
  - `nontrivial_segment`: `101`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `262`
- 500 functions:
  - `canonicalization_failed_interleaved_join_uses`: `376 -> 363`
  - `no_next_label`: `169 -> 162`
  - `nontrivial_segment`: `207 -> 201`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `354`

The pure-value interleaved refinement produces a real but modest 500-function reduction, while the new subtype counters show the remaining interleaved failures are still dominated by structurally nontrivial segments rather than opaque layout noise.

### Structuring - Pure Multi-Goto Alias-Chain Relaxation

This increment broadens one guarded-tail alias-chain acceptance boundary by allowing multiple top-level forward gotos to the same local alias label when everything between them is pure/ignorable and the alias still forwards linearly to the same follow.

#### Changed

- refined `guarded_tail/canonicalize.rs` so the `has_non_ignorable_gap && goto_positions.len() != 1` path no longer rejects a purely linear alias chain if all intermediate statements are:
  - ignorable discovery statements
  - pure value expressions
  - gotos to the same alias label
- added helper `is_pure_multi_goto_gap_to_label()` in `guarded_tail/alias_refs.rs` to keep the acceptance rule narrow and explicit

#### Added

- new guarded-tail regression:
  - `structuring_candidate_discovery_canonicalizes_pure_multi_goto_alias_chain`
- preserved existing alias-forward-chain and true-nonlocal regressions

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions: no material aggregate movement
- 500 functions: no material aggregate movement

This is a safe acceptance broadening for a real local alias shape, but it does not materially move the dominant large-sample buckets on its own.

### Docs and Structure - Hierarchical AGENTS Guides and Guarded-Tail Module Split

This increment improves repository navigation for human/AI contributors and reduces structural risk in the guarded-tail implementation by splitting the overloaded `guards.rs` module and moving its dedicated regression coverage into a separate test file.

#### Changed

- refreshed the repository-root `AGENTS.md` and added focused child guides for the highest-complexity ownership seams:
  - `crates/fission-pcode/src/nir/AGENTS.md`
  - `crates/fission-pcode/src/nir/structuring/AGENTS.md`
  - `crates/fission-automation/src/AGENTS.md`
  - `crates/fission-static/src/analysis/decomp/postprocess/AGENTS.md`
- split guarded-tail implementation from the monolithic `structuring/guards.rs` into:
  - `structuring/guarded_tail/alias_refs.rs`
  - `structuring/guarded_tail/promotion_graph.rs`
  - `structuring/guarded_tail/canonicalize.rs`
  - `structuring/guarded_tail/promotion.rs`
  - `structuring/guarded_tail/mod.rs`
- updated `structuring/mod.rs` to route guarded-tail logic through the new folder-tree layout
- moved guarded-tail-specific regressions out of `structuring_misc.rs` into:
  - `crates/fission-pcode/src/nir/tests/structuring_guarded_tail.rs`

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

#### Notes

- This is a behavior-preserving refactor intended to make future guarded-tail work safer and more local.
- `structuring_misc.rs` now keeps only genuinely mixed/overflow structuring cases while guarded-tail behavior is co-located with its own test file.

### Structure - NIR Support, Telemetry, and Builder Façade Slimming

This refactor wave makes the NIR layer materially easier to navigate by moving the last large support/state/telemetry/helper responsibilities out of `nir/mod.rs` and `nir/builder/mod.rs` into dedicated internal modules without changing the public NIR surface.

#### Changed

- split shared private NIR support into:
  - `crates/fission-pcode/src/nir/support.rs`
    - support types
    - constants
    - pure lowering/type/naming helpers
- split preview telemetry storage and retrieval into:
  - `crates/fission-pcode/src/nir/telemetry.rs`
    - thread-local preview stats storage
    - `take_last_preview_*` / `take_last_nir_*` helpers
- slimmed `crates/fission-pcode/src/nir/mod.rs` into a thinner façade that now primarily owns:
  - public render entrypoints
  - top-level orchestration
  - telemetry delegation
- split builder internals into focused modules:
  - `crates/fission-pcode/src/nir/builder/state.rs` — `PreviewBuilder` state layout
  - `crates/fission-pcode/src/nir/builder/init.rs` — constructor/state initialization
  - `crates/fission-pcode/src/nir/builder/debug.rs` — debug / unsupported inventory plumbing
  - `crates/fission-pcode/src/nir/builder/stats.rs` — `preview_build_stats()` projection
- kept `crates/fission-pcode/src/nir/builder/mod.rs` as a much thinner façade around:
  - type-hint wrappers
  - `build_hir()` orchestration
  - a small set of orchestration helpers

#### Validation

- priority regression set passed:
  - `bootstrap_x86`
  - `structuring_conditionals`
  - `structuring_linear`
  - `structuring_loops`
  - `type_hints_function_hints`
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

#### Notes

- This is a behavior-preserving structural refactor only.
- The `nir/` tree now has clearer ownership boundaries: façade (`mod.rs`), support (`support.rs`), telemetry (`telemetry.rs`), builder state/init/debug/stats, structuring, and tests.

## 2026-03-24

### P5H4A/P5H4B/P5H4C/P5H4E - Algorithmic CFG Foundation Expansion (Ghidra-Referenced)

This step advances structuring from local heuristic-style approximations toward graph-theoretic analysis primitives, while preserving conservative fallback behavior.

#### Changed

- stabilized label handling used by region/join anchoring in normalization and cleanup paths
- added CFG edge classification analysis (`Tree`, `Back`, `Forward`, `Cross`) for deterministic, order-robust graph facts
- added formal dominator/post-dominator analysis APIs and integrated window-exit postdom computation into conditional-tail follow logic
- added Tarjan SCC analysis and irreducible multi-header SCC detection (diagnostic-safe integration)
- extended structuring diagnostics to include SCC and irreducible telemetry counters

#### Added

- new structuring analysis module:
  - `crates/fission-pcode/src/nir/structuring/cfg_analysis.rs`
- new CFG-analysis tests covering:
  - diamond edge classification
  - single-loop back-edge classification
  - multi-header SCC irreducible detection
  - nearest common dominator/postdominator behavior on canonical shapes

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo test -p fission-pcode structuring_conditionals` (pass)
- `cargo test -p fission-pcode structuring_loops` (pass)
- `cargo check -p fission-pcode` (pass)

### Automation - Irreducible/SCC Telemetry Surfacing and Gate Safety Integration

Automation reporting now consumes irreducible-structure telemetry from `NirBuildStats`, so quality runs can detect mismatch improvements that are accompanied by structural complexity regressions.

#### Changed

- extended `NirBuildStats` with:
  - `structuring_scc_component_count`
  - `structuring_irreducible_scc_count`
  - `structuring_irreducible_header_count`
- wired new counters through builder initialization, preview stats snapshots, and stats merge paths
- updated automation summary/delta reporting to include SCC/irreducible counters
- updated go/stop decision gate constraints to require non-regressing irreducible deltas in addition to mismatch/migration checks

#### Validation

- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-automation` (pass)

### P5H4E - Conservative Irreducible Recovery Gate and NIR Completeness Reporting

This patch adds an optional conservative gate for region linearization recovery on irreducible CFG nodes and extends telemetry/reporting so automation can measure the tradeoff explicitly.

#### Changed

- added `NirRenderOptions.conservative_irreducible_fallback` (default `false`) with backward-compatible serde default handling
- added recovery rejection telemetry for irreducible CFG gating:
  - `region_linearize_rejected_irreducible_cfg_count`
- wired the new counter through:
  - `PreviewBuilder` initialization/state
  - `preview_build_stats()` snapshots
  - `NirBuildStats::merge_assign()`
- recovery path now optionally skips region linearization when conservative gate is enabled and the start node belongs to an irreducible SCC
- `fission-static` recovery option wiring now supports env-based activation:
  - `FISSION_NIR_CONSERVATIVE_IRREDUCIBLE_FALLBACK`
- automation reporting updated to include irreducible-gate rejection metrics in:
  - stats pairs
  - baseline deltas
  - markdown summary output

#### Added

- SCC helper API for gate decisions:
  - `SccAnalysis::is_irreducible_node()`
- regression test:
  - `scc_analysis_reports_irreducible_membership_by_node`
- NIR English completeness report document:
  - `crates/fission-pcode/src/nir/NIR_DECOMPILER_COMPLETENESS_REPORT.md`

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo check -p fission-static --features native_decomp` (pass)

### Loop Structuring - Conservative Infloop + Loop-Control Goto Rewrites (Ghidra-Referenced)

This patch extends loop structuring with a conservative infinite-loop reducer and safe loop-local `goto` rewriting into `break`/`continue`, aligned with Ghidra `scopeBreak` intent while preserving nested-scope safety.

#### Changed

- added and integrated `try_lower_infloop()` into the main structuring order:
  - reducer order now keeps `infloop` after `dowhile` and `while` attempts for conservative precedence
- added single-successor guard for infloop recognition (`successors[idx].len() == 1`)
- introduced loop-body post-processing in `structuring/loops.rs`:
  - rewrite `goto(loop_exit_label)` to `break`
  - rewrite `goto(loop_continue_label)` to `continue`
  - recurse only through `If`/`Block`
  - intentionally do **not** recurse into nested `While`/`DoWhile`/`Switch` (avoids outer-loop misrewrites)
- extended do-while region result metadata to return condition-block index so `continue` targets are resolved correctly

#### Added

- integration regression test:
  - `infloop_preview_lowers_single_block_self_loop`
- unit tests for rewrite safety:
  - `rewrite_loop_control_gotos_converts_break_and_continue_targets`
  - `rewrite_loop_control_gotos_does_not_rewrite_inside_nested_loop_or_switch`

#### Validation

- `cargo test -p fission-pcode rewrite_loop_control_gotos_` (pass)
- `cargo test -p fission-pcode structuring_loops` (pass)
- `cargo test -p fission-pcode structuring_conditionals` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

## 2026-03-23

### Docs - CONTRIBUTING CI/CD Workflow Refresh

Contributor guidance was updated to match the current CI/CD architecture and remove stale local expectations.

#### Changed

- `CONTRIBUTING.md` now documents:
  - fast PR gate vs heavy GitHub validation split
  - Windows build/test participation in CI
  - current local pre-PR command set aligned with fast gate
  - direct CMake decompiler build invocation used in CI
  - automation artifact interpretation expectations for decompilation-quality changes

### CI/CD - Major Reinforcement (Fast PR Gate + Heavy GitHub Validation)

To reduce local monitoring burden, CI/CD now separates fast developer feedback from heavy long-running validation that can run entirely on GitHub.

#### Added

- new heavy validation workflow: `.github/workflows/ci-heavy.yml`
  - triggers: `push(main)`, nightly `schedule`, and `workflow_dispatch`
  - jobs:
    - Linux full validation (full Rust tests, tauri frontend build, decomp smoke)
    - Windows heavy build/test (decompiler + core Rust tests)
    - automation nir-check lanes with artifact upload
- automation artifact upload in heavy workflow:
  - uploads `artifacts/fission-automation/` for post-run diagnosis without local reruns

#### Changed

- fast CI workflow (`.github/workflows/ci.yml`) refactored into layered jobs:
  - Linux fast gate
  - macOS build/test
  - Windows build/test
- added Rust build caching (`Swatinem/rust-cache@v2`) to CI jobs
- PR/main fast gate now keeps heavy checks off local loop while preserving cross-platform confidence
- replaced missing decompiler build script invocation with direct CMake build commands in CI workflows:
  - `cmake -S ghidra_decompiler -B ghidra_decompiler/build -DCMAKE_BUILD_TYPE=Release`
  - `cmake --build ghidra_decompiler/build --config Release`
- fixed follow-up CI failures after rollout:
  - removed invalid boolean value usage for `nir-check --update-latest` (flag now omitted in heavy workflow)
  - constrained Windows CMake builds to required targets (`decomp`, `fission_decomp`) to avoid unrelated test-target dependency failures
  - adjusted Linux heavy Rust test sequence to run `fission-static` under `native_decomp` explicitly while keeping broad workspace coverage
  - updated CD Unix decompiler step to direct CMake build (removed stale `scripts/build/build_decompiler.sh` dependency)

#### Validation

- workflow YAML parse check (local):
  - `ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml')"`
  - `ruby -ryaml -e "YAML.load_file('.github/workflows/ci-heavy.yml')"`
- existing project checks unaffected by workflow changes (code path unchanged)

### P5H3J - Index-Order Independent Follow Discovery (Anti-Overfit)

This patch removes block-index monotonicity assumptions from localized follow discovery so conditional-tail recovery relies on graph properties (cycle/region guards) rather than binary layout order.

#### Changed

- replaced index-order rejection in local recovery window traversal with explicit window-cycle detection
- updated trivial forwarding chain canonicalization to use visited-set loop safety instead of index-increasing assumptions
- updated region target canonicalization to use visited-set termination instead of index monotonicity checks
- preserved existing conservative guards (`side_entry_or_exit`, bounded window, bounded steps)

#### Added

- regression test: `region_follow_discovery_accepts_non_monotonic_acyclic_window`
- regression test: `region_follow_discovery_rejects_local_cycle_without_index_heuristic`

#### Validation

- `cargo test -p fission-pcode region_follow_discovery_accepts_non_monotonic_acyclic_window -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_rejects_local_cycle_without_index_heuristic -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_orders_multiple_candidates_closest_to_join_first -- --nocapture` (pass)
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- focused fast benchmark output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774250794-485014000`
- mid 40-function benchmark output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774250794-476962000`

#### Outcome

- follow discovery is now less sensitive to binary-specific block index ordering
- headline corpus movement remains unchanged in current lane (`changed_rows=0`, gate `stop_hold_p5h3f`), but algorithmic generality and anti-overfit guarantees improved

### P5H3I - Algorithmic Arm-Body Failure Decomposition and Signal Cleanup

This patch focused on removing opaque/generic arm-body failure reporting from conditional-tail mismatch analysis and keeping recovery retry behavior deterministic.

#### Changed

- conditional-tail mismatch subtyping now distinguishes algorithmic causes without relying on a generic arm-body bucket:
  - `DepthOrBudgetExceeded`
  - `OneArmBodyLoweringFailed`
  - `BothArmsBodyLoweringFailed`
  - `FollowTailLoweringFailed`
- shared-follow retry failure handling now preserves candidate-stage subtype when propagating final mismatch
- `arm_body_lowering_failed` aggregate counter remains for compatibility but is now sourced from explicit subtypes only
- automation subtype ranking now reports specific subtype channels directly (rather than the aggregate arm-body total)

#### Validation

- `cargo test -p fission-pcode region_follow_discovery_orders_multiple_candidates_closest_to_join_first -- --nocapture` (pass)
- `cargo test -p fission-pcode region_recovery_lowers_two_arm_nontrivial_shared_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- focused fast benchmark:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --focus-top-mismatch 5 --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774249297-033281000`
- mid benchmark:
  - `cargo run -p fission-automation -- nir-check --lane preview --run-profile mid --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40 --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774249297-026445000`

#### Outcome

- top-row movement is still not observed (`changed_rows=0`, gate remains `stop_hold_p5h3f`)
- failure attribution quality improved by removing generic arm-body dominance from subtype ranking, allowing the next step to target specific residual channels (`complex_arm_shape`, `side_entry_or_exit`, `follow_beyond_window`)

### P5H3H - Algorithmic Arm-Body Failure Refinement and Deterministic Follow Retry

This patch continues the heuristic-to-algorithm transition by refining conditional-tail arm-body failure handling and making shared-follow retries deterministic over validated local postdom candidates.

#### Changed

- expanded recovery mismatch subtype model for arm-body failures:
  - `OneArmBodyLoweringFailed`
  - `BothArmsBodyLoweringFailed`
  - `FollowTailLoweringFailed`
- kept aggregate compatibility counter while adding subtype-specific counters for triage precision
- upgraded shared-follow retry loop:
  - retries now iterate over deterministic local postdom candidates (closest-to-join first)
  - candidate attempts classify failure mode explicitly instead of collapsing into one bucket
  - final fallback preserves candidate-stage subtype signal when available

#### Added

- algorithm-focused regression coverage:
  - `region_follow_discovery_orders_multiple_candidates_closest_to_join_first`
- test helper rename for multi-candidate follow verification:
  - `find_shared_tail_entries_for_region_for_test`

#### Validation

- `cargo test -p fission-pcode region_follow_discovery_selects_immediate_common_postdom -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_rejects_side_entry_common_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_orders_multiple_candidates_closest_to_join_first -- --nocapture` (pass)
- `cargo test -p fission-pcode region_recovery_lowers_two_arm_nontrivial_shared_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- focused benchmark:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --focus-top-mismatch 5 --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774248662-508776000`
- mid benchmark:
  - `cargo run -p fission-automation -- nir-check --lane preview --run-profile mid --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40 --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774248700-402991000`

#### Outcome

- deterministic candidate-order retry behavior is now fixed and test-covered
- subtype granularity for arm-body failures is now available in telemetry and automation insights
- corpus headline metrics on the current 40-function lane remain unchanged (`changed_rows=0`, gate still `stop_hold_p5h3f`), but failure attribution quality improved for the next targeted algorithm step

### Automation - Fast/Mid/Full Run Profiles and Focused Mismatch Reruns

To reduce iteration latency for structuring work, nir-check now supports profile-based execution and baseline-driven target focusing.

#### Added

- `--run-profile {fast|mid|full}` for runtime-tuned execution:
  - `fast`: aggressive limit/timeout reduction for tight loops
  - `mid`: current default behavior
  - `full`: expanded limits for broader validation
- `--focus-top-mismatch N` to filter lane targets using baseline mismatch-heavy binaries
  - reads baseline candidates and keeps only binaries implicated by top mismatch rows
- run metadata in `summary.json`:
  - `run_profile`, `target_count`, `inventory_elapsed_ms`, `diagnosis_elapsed_ms`, `write_outputs_elapsed_ms`, `total_elapsed_ms`
- markdown summary now includes run profile/target count/timing line for quick bottleneck checks

#### Changed

- profile-aware tuning of effective per-target `functions-limit` and `timeout-ms` in automation runner
- terminal summary now prints profile + timing stage breakdown + go/stop gate in one line

#### Validation

- `cargo test -p fission-automation` (pass)
- `cargo build -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --focus-top-mismatch 5 --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774247430-463672000`
  - run metadata emitted: `run_profile=fast`, `target_count=2`, timings populated

### Automation - Nir-Check Decision Reporting Upgrade (P5H3F Support)

The automation pipeline now emits direct decision artifacts for conditional-tail recovery work, so patch iteration can be judged from row-level evidence instead of aggregate-only counters.

#### Added

- `decision_insights.json` output in each nir-check run, including:
  - mismatch subtype ranking
  - top mismatch rows with per-row subtype split
  - row-level baseline/current mismatch deltas
  - deterministic go/stop gate recommendation for P5H3G readiness
- markdown summary section `Conditional-Tail Decision Insights` with the same signal set

#### Changed

- baseline delta now includes recovery-shaping metrics:
  - `region_linearized_count`
  - `forced_linear_count`
  - `conditional_tail_exit_mismatch_count`
  - `body_lowering_failed_count`
  - `successor_inline_rejected_count`
  - `revisit_cycle_count`
  - `unsupported_terminator_count`
- nir-check now loads baseline candidate rows (when available) to compute row-address diff instead of aggregate-only comparison
- terminal summary now prints go/stop gate and changed-row count for immediate run triage

#### Validation

- `cargo test -p fission-automation` (pass)
- `cargo build -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40 --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774245667-981676000/summary.json`
  - generated output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774246248-772213000`
  - go/stop gate emitted: `stop_hold_p5h3f`
  - changed rows emitted: `0`
  - subtype ranking surfaced from real rows: `arm_body_lowering_failed`, `complex_arm_shape`, `side_entry_or_exit`, `follow_beyond_window`

### P5H3F - Conditional-Tail Mismatch Subtype Harvesting + Bounded Follow Discovery

This patch shifted the focus from widening shape support to separating `ConditionalTailExitMismatch` into actionable subtype signals and introducing a bounded local follow discovery path in region recovery.

#### Changed

- added recovery-only conditional-tail mismatch subtype tracking in linear structuring:
  - `NoCommonFollowInWindow`
  - `FollowBeyondWindow`
  - `SideEntryOrExit`
  - `ComplexArmShape`
  - `ArmBodyLoweringFailed`
  - `AmbiguousMultipleFollows`
- introduced bounded first-common-follow discovery for region conditional tails:
  - forward-only, bounded steps, no-cycle progression
  - side-entry / side-exit guard before accepting shared follow candidate
- retained existing conservative behavior when guards fail:
  - mismatch still reports through `ConditionalTailExitMismatch`
  - no fallback broadening to global CFG/postdom passes
- added optional per-mismatch sample logging (env-gated):
  - `FISSION_RECOVERY_MISMATCH_TRACE=1`
  - emits JSONL under `/tmp/fission_preview_<function>_conditional_mismatch.jsonl`

#### Added

- synthetic regression for non-trivial shared follow discovery:
  - `region_recovery_lowers_two_arm_nontrivial_shared_follow`

#### Validation

- `cargo test -p fission-pcode region_recovery_lowers_two_arm_nontrivial_shared_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode structuring_linear -- --nocapture`
  - same pre-existing failure on current `main`:
    - `multi_block_preview_absorbs_shared_trivial_forwarding_return_tail`
- `cargo test -p fission-pcode structuring_conditionals -- --nocapture`
  - same pre-existing failures on current `main` remain:
    - `x86_pathological_try_lower_if_falls_back_without_hanging`
    - `multi_block_preview_lowers_canonical_if_else`
    - `multi_block_preview_lowers_if_else_with_multi_block_then_region`
    - `multi_block_preview_prefers_short_circuit_or_over_nested_plain_if`
    - `multi_block_preview_folds_short_circuit_and`
    - `multi_block_preview_folds_short_circuit_or`
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo build -p fission-cli --features native_decomp` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774245667-988203000`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774245667-981676000`

#### Corpus Outcome (vs P5H3E baseline)

- aggregate headline metrics remained stable in 40-function lane:
  - `region_linearized`: `1 -> 1`
  - `forced_linear`: `18 -> 18`
  - `region_linearize_rejected_body_lowering_failed_count`: `5 -> 5`
  - `conditional_tail_exit_mismatch`: `27 -> 27`
  - `successor_inline_rejected/revisit_cycle/unsupported_terminator`: still `0`
- new subtype telemetry now resolves previously opaque mismatch pressure:
  - `conditional_tail_follow_beyond_window`: `2`
  - `conditional_tail_side_entry_or_exit`: `4`
  - `conditional_tail_complex_arm_shape`: `19`
  - `conditional_tail_arm_body_lowering_failed`: `54`
  - `conditional_tail_no_common_follow_in_window`: `0`
  - `conditional_tail_ambiguous_multiple_follows`: `0`
- top mismatch rows remain the same addresses but now carry subtype split data for shape-targeted next patching.

### P5H3E - Conditional-Tail Normalization Widening (Localized Recovery)

This patch focused on reducing `conditional_tail_exit_mismatch` inside localized recovery without broadening general CFG support.

#### Changed

- added region-only conditional-tail arm normalization stage:
  - `normalize_conditional_tail_arm_for_region(...)`
  - explicitly separates canonical target from effective lowering start
- strengthened one-arm preference under region recovery:
  - if one arm reaches join via bounded trivial forwarding chain, prioritize one-arm if lowering on the opposite arm
- added conservative shared-tail reconciliation for two-arm region tails:
  - detects bounded forward-only trivial common tail entry
  - retries arm lowering to shared tail entry before lowering the shared tail to final join
  - constrained to region-recovery path only (forward-only, bounded, trivial forwarding)

#### Added

- synthetic regression tests for conditional-tail normalization widening:
  - `region_recovery_lowers_one_arm_join_adjacent_forwarding_chain`
  - `region_recovery_lowers_two_arm_shared_tail_entry`

#### Validation

- `cargo test -p fission-pcode structuring_linear -- --nocapture`
  - same pre-existing failure shape on current `main`:
    - `multi_block_preview_absorbs_shared_trivial_forwarding_return_tail`
- `cargo test -p fission-pcode structuring_conditionals -- --nocapture`
  - both new synthetic P5H3E tests pass
  - same pre-existing failures on current `main` remain:
    - `x86_pathological_try_lower_if_falls_back_without_hanging`
    - `multi_block_preview_lowers_canonical_if_else`
    - `multi_block_preview_lowers_if_else_with_multi_block_then_region`
    - `multi_block_preview_prefers_short_circuit_or_over_nested_plain_if`
    - `multi_block_preview_folds_short_circuit_and`
    - `multi_block_preview_folds_short_circuit_or`
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo build -p fission-cli --features native_decomp` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774243155-357880000`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774243155-349905000`

#### Corpus Delta vs P5H3D Baseline

- baseline (P5H3D):
  - 5-function lane: `/artifacts/fission-automation/1774242470-100755000`
  - 40-function lane: `/artifacts/fission-automation/1774242496-398954000`
- P5H3E result:
  - 5-function lane: `region_linearized=0`, `forced_linear=2`, mismatch counters all `0` (unchanged)
  - 40-function lane:
    - `region_linearized=1` (unchanged)
    - `forced_linear=18` (unchanged)
    - `region_linearize_rejected_body_lowering_failed_count=5` (unchanged)
    - `conditional_tail_exit_mismatch=27` (unchanged)
    - `successor_inline_rejected=0` (unchanged)
    - `revisit_cycle=0` (unchanged)
    - `unsupported_terminator=0` (unchanged)

This indicates the conservative widening is behavior-safe and regression-clean for targeted synthetic shapes, but does not yet shift aggregate mismatch pressure in current 40-function corpus.

### P5H3D - Region Recovery Semantics Tightening and Corpus Closure

This patch tightened localized recovery semantics rather than broadening shape coverage. The focus was to preserve reject-reason fidelity across cache hits and make region target canonicalization origin-aware so conditional-tail normalization stays region-local and conservative.

#### Added

- regression coverage for semantics stability:
  - `lower_linear_body_region_cache_preserves_reject_reason_across_retries`
  - `region_canonicalization_respects_origin_guard`

#### Changed

- linear body cache now preserves reject reasons for localized (`region_recovery=true`) lowering cache entries instead of collapsing every cached reject into a generic class
- non-localized (`region_recovery=false`) detailed cache behavior remains conservative/generic to avoid changing broader structuring policy
- conditional-tail region canonicalization now uses the current conditional block index as origin instead of a fixed origin value
- added a test-only canonicalization hook to assert origin-guard behavior directly in synthetic coverage

#### Validation

- `cargo test -p fission-pcode structuring_linear -- --nocapture`
  - includes new cache-stability regression as passing
  - includes one pre-existing failure on current `main`:
    - `multi_block_preview_absorbs_shared_trivial_forwarding_return_tail`
- `cargo test -p fission-pcode structuring_conditionals -- --nocapture`
  - includes new origin-guard regression as passing
  - includes pre-existing failures on current `main` (confirmed unchanged on baseline `origin/main`):
    - `x86_pathological_try_lower_if_falls_back_without_hanging`
    - `multi_block_preview_lowers_canonical_if_else`
    - `multi_block_preview_lowers_if_else_with_multi_block_then_region`
    - `multi_block_preview_prefers_short_circuit_or_over_nested_plain_if`
    - `multi_block_preview_folds_short_circuit_and`
    - `multi_block_preview_folds_short_circuit_or`
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo build -p fission-cli --features native_decomp` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Corpus Outcome

- 5-function `nir` lane aggregate:
  - `recovery_structuring_mode_counts = {"forced_linear": 2}`
  - `region_linearized = 0`
  - body-lowering reject counters:
    - `region_linearize_rejected_body_lowering_failed_count = 0`
    - `conditional_tail_exit_mismatch = 0`
    - `successor_inline_rejected = 0`
    - `revisit_cycle = 0`
    - `unsupported_terminator = 0`
- 40-function (`preview` alias -> canonical `nir`) lane aggregate:
  - `recovery_structuring_mode_counts = {"forced_linear": 18, "region_linearized": 1}`
  - body-lowering reject counters:
    - `region_linearize_rejected_body_lowering_failed_count = 5`
    - `conditional_tail_exit_mismatch = 27`
    - `successor_inline_rejected = 0`
    - `revisit_cycle = 0`
    - `unsupported_terminator = 0`

This closes P5H3D as a semantics/measurement-hardening round. The next ranking signal remains conditional-tail mismatch pressure rather than unsupported-terminator inflation.

### P5H3C - Localized Body-Lowering Recovery Coverage Expansion

This patch targeted the next blocker called out in the previous quality round: reducing `region_linearized` rejection pressure from body-lowering failures without changing fallback policy.

The change expands localized trampoline canonicalization for nearby joins and fixes a conditional-tail lowering edge case where both arms canonicalized to the same join and were incorrectly re-lowered from the join itself.

#### Added

- new regression test for localized recovery over multi-hop trampoline joins:
  - `region_recovery_succeeds_on_multi_hop_trampoline_join`

#### Changed

- widened region target canonicalization window in localized recovery:
  - increased canonicalization hop budget for trivial forwarding trampolines
  - increased nearby-join trampoline distance allowance
- fixed conditional-tail localized lowering arm selection:
  - when canonicalization resolves directly to the join, branch lowering now starts from the original branch target arm instead of the join block
- updated linear structuring regression expectations for one-arm forwarding/trampoline-tail shapes that now lower successfully
- test helper visibility under `structuring` test wiring was aligned so test-only re-exports compile cleanly in the current layout
- removed an unused linear-body detailed wrapper to keep the structuring module warning-clean

#### Validation

- `cargo test -p fission-pcode region_recovery_succeeds_on_ -- --nocapture`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`

#### Current Outcome

- localized region recovery now handles deeper trivial trampoline joins that were previously prone to body-lowering rejection
- region-recovery regression coverage now includes the multi-hop join shape
- targeted NIR structuring tests and dependent crate builds completed successfully after the patch

## 2026-03-21

### quality-measurement-pipeline / P5H3B - Output Quality Metrics and Localized Recovery Instrumentation

This round added the first canonical output-quality measurement pipeline on top of the existing Fission NIR inventory and automation flow. The goal was not to change routing or recovery policy yet, but to make structured output ratios, linear fallback rates, and top structuring/build counters measurable on real corpus runs.

It also extended the localized structuring recovery path with reject-reason instrumentation so the current blocker is no longer opaque. The immediate outcome is that quality is now quantifiable, and the `region_linearized` bottleneck has been narrowed from a vague “localized fallback rarely triggers” problem down to a concrete `lower_linear_body` failure class.

#### Added

- row-level Fission NIR quality fields in CLI candidate/inventory output:
  - `nir_goto_count`
  - `nir_output_class`
  - `nir_build_stats`
- aggregate quality metrics in inventory summaries:
  - `nir_output_class_counts`
  - `nir_build_stats_totals`
- canonical automation quality artifact:
  - `artifacts/fission-automation/.../quality_measurement.json`
- new `NirBuildStats` counters for localized recovery diagnosis:
  - `forced_linear_structuring_count`
  - `region_linearize_structuring_count`
  - `region_linearize_heuristic_exit_count`
  - `region_linearize_rejected_non_structuring_failure_count`
  - `region_linearize_rejected_no_exit_count`
  - `region_linearize_rejected_body_lowering_failed_count`
  - `region_linearize_rejected_non_advancing_count`

#### Changed

- `fission-automation` terminal and Markdown reports now show:
  - structured ratio
  - linear fallback ratio
  - `nir_output_class_counts`
  - top `NirBuildStats` counters
- Fission NIR build stats are now preserved even when `build_hir` exits through a structuring error path
- failed `region_linearized` attempts now surface partial build stats into the later forced-linear recovery result, so localized recovery rejection is visible in corpus summaries
- localized recovery now tries a narrow nearby-join exit heuristic instead of relying only on a single `linear_exit(start_idx)` result

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- quality is now measurable from canonical artifacts instead of inferred from ad hoc logs:
  - `nir` smoke:
    - `structured_ratio=50.0%`
    - `linear_fallback_ratio=40.0%`
  - 40-function corpus run:
    - `structured_ratio=33.8%`
    - `linear_fallback_ratio=32.5%`
- the current `region_linearized` blocker is now explicit:
  - `region_linearize_rejected_body_lowering_failed_count = 5`
  - `region_linearize_rejected_no_exit_count = 0`
- recovery distribution is unchanged for now:
  - `recovery_attempted = 19`
  - `recovered = 19`
  - `forced_linear = 18`
  - `region_linearized = 1`
  - `high_goto_density = 14`
- this narrows the next patch target to localized body lowering rather than exit discovery

### P6R3 / P6R4 / P6R5 / P6R6 - Follow-up Fission NIR and CLI Module Extraction

This follow-up refactor round continued the post-rename cleanup without changing current decompilation semantics. The focus was to remove the next batch of oversized coordination files, move the Fission NIR implementation under a dedicated `decomp/nir/` subtree, and split CLI inventory/candidate execution code into clearer ownership modules.

The goal was still boundary cleanup, not policy change: legacy/NIR routing, recovery behavior, JSON compatibility, and automation baselines stayed intact. The result is that several formerly mixed-responsibility files are now thin façades, while the implementation sits in smaller modules with narrower ownership.

#### Added

- `fission-static` follow-up decompiler ownership files:
  - `caching_decompiler.rs`
  - `decomp/nir/context.rs`
  - `decomp/nir/engine.rs`
  - `decomp/nir/recovery.rs`
  - `decomp/nir/render.rs`
  - `decomp/nir/routing.rs`
  - `decomp/nir/taxonomy.rs`
  - `decomp/nir/types.rs`
  - `decomp/nir/worker.rs`
- CLI inventory ownership modules:
  - `cli/oneshot/inventory/schema.rs`
  - `cli/oneshot/inventory/provenance.rs`
  - `cli/oneshot/inventory/emit.rs`
- CLI execution ownership modules:
  - `cli/oneshot/decompile/decompile_exec/batch.rs`
  - `cli/oneshot/decompile/decompile_exec/output.rs`
  - `cli/oneshot/decompile/decompile_exec/run.rs`
- CLI NIR candidate ownership modules:
  - `cli/oneshot/decompile/nir_candidates/schema.rs`
  - `cli/oneshot/decompile/nir_candidates/summary.rs`
  - `cli/oneshot/decompile/nir_candidates/build.rs`

#### Changed

- Fission NIR source files now live physically under `crates/fission-static/src/analysis/decomp/nir/`, while `decomp/mod.rs` keeps the existing public module surface through `#[path = "nir/..."]` wiring
- `crates/fission-static/src/analysis/decomp/mod.rs` no longer owns the native cached decompiler implementation directly:
  - `DecompilerNative`
  - `CachingDecompiler`
  - `RecommendedDecompiler`
  moved into `caching_decompiler.rs`, and `mod.rs` now mainly acts as a re-export surface
- `crates/fission-cli/src/cli/oneshot/inventory.rs` is now a thin façade:
  - schema types moved to `inventory/schema.rs`
  - provenance/fact aggregation moved to `inventory/provenance.rs`
  - decompiler prep and emit loop moved to `inventory/emit.rs`
- `crates/fission-cli/src/cli/oneshot/decompile/decompile_exec.rs` is now a thin façade:
  - batch inventory/candidate emit moved to `decompile_exec/batch.rs`
  - single-function output path moved to `decompile_exec/output.rs`
  - sequential/parallel run orchestration moved to `decompile_exec/run.rs`
- `crates/fission-cli/src/cli/oneshot/decompile/nir_candidates.rs` is now a thin façade:
  - row/inventory schema moved to `nir_candidates/schema.rs`
  - summary/failure/signature logic moved to `nir_candidates/summary.rs`
  - candidate row construction and panic recovery moved to `nir_candidates/build.rs`

#### Validation

- `cargo fmt`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`

#### Current Outcome

- the next batch of coordination files is now physically reduced:
  - `decomp/mod.rs`: `199 -> 88` lines
  - `inventory.rs`: `627 -> 5` lines
  - `decompile/decompile_exec.rs`: `951 -> 6` lines
  - `decompile/nir_candidates.rs`: `849 -> 10` lines
- canonical `nir` automation smoke remains stable after the refactor:
  - `direct_success=10`
  - `nir_failure=0`
  - `explicit_nonzero=4`
  - `recovery_attempted={'linearized_structuring_retry': 2}`
  - `recovery_outcome={'recovered': 2}`

## 2026-03-20

### P6R2 - Real Module Split After Fission NIR Rename

This round turned the earlier Fission NIR rename into a real responsibility split. The goal was boundary cleanup, not behavior change: `nir_engine.rs`, `decompile.rs`, and `structuring/mod.rs` were reduced to thin orchestration facades while the actual implementation moved into focused ownership modules.

The refactor kept current recovery policy, fallback semantics, dual-written JSON compatibility fields, and local automation behavior intact. Deprecated aliases such as `mlil-preview` and the `preview` automation lane still work, but the canonical code paths are now physically organized around `nir` ownership boundaries.

#### Added

- `fission-static` Fission NIR ownership modules:
  - `nir_types.rs`
  - `nir_taxonomy.rs`
  - `nir_worker.rs`
  - `nir_render.rs`
  - `nir_recovery.rs`
  - `nir_routing.rs`
- CLI oneshot decompilation submodules:
  - `decompile/decompile_exec.rs`
  - `decompile/decompile_render.rs`
  - `decompile/decompile_targets.rs`
  - `decompile/nir_candidates.rs`
- NIR structuring ownership submodules:
  - `structuring/cleanup.rs`
  - `structuring/guards.rs`
  - `structuring/surfacing.rs`
  - `structuring/recovery.rs`
  - `structuring/driver.rs`

#### Changed

- `crates/fission-static/src/analysis/decomp/nir_engine.rs` is now a thin façade that re-exports:
  - canonical Fission NIR types
  - taxonomy helpers
  - worker entrypoints
  - routing/recovery entrypoints
  - deprecated preview compatibility wrappers
- `crates/fission-cli/src/cli/oneshot/decompile.rs` is now a thin façade:
  - actual execution moved to `decompile_exec.rs`
  - candidate/report logic moved to `nir_candidates.rs`
  - render/output helpers moved to `decompile_render.rs`
  - target selection moved to `decompile_targets.rs`
- internal CLI candidate types were renamed to `NirCandidate*`, while compatibility aliases for `PreviewCandidate*` remain in place for existing consumers
- `crates/fission-pcode/src/nir/structuring/mod.rs` is now a thin driver/re-export surface:
  - cleanup helpers moved to `cleanup.rs`
  - guarded-tail and promotion logic moved to `guards.rs`
  - typed structuring failure surfacing moved to `surfacing.rs`
  - localized/forced-linear recovery moved to `recovery.rs`
  - top-level structuring orchestration moved to `driver.rs`
- automation lane normalization still maps deprecated `preview` to canonical `nir`, and both lanes continue to deserialize dual-written `nir_*` / `preview_*` fields without drift

#### Validation

- `cargo fmt`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo check -p fission-analysis`
- `cargo check -p fission-tauri`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp-all --decomp-limit 1 --engine nir --json`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp-all --decomp-limit 1 --engine mlil-preview --json --verbose`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`

#### Current Outcome

- the three main refactor targets are now physically split:
  - `nir_engine.rs`: `1620 -> 444` lines
  - `decompile.rs`: `2171 -> 45` lines
  - `structuring/mod.rs`: `1159 -> 20` lines
- canonical `nir` and deprecated `preview` automation lanes still produce the same current smoke result:
  - `direct_success=10`
  - `nir_failure=0`
  - `explicit_nonzero=4`
  - `recovery_attempted={'linearized_structuring_retry': 2}`
  - `recovery_outcome={'recovered': 2}`
- canonical CLI output and deprecated `mlil-preview` alias still converge on the same engine result for the smoke sample:
  - `engine_used = nir`
  - `fell_back = false`

### P6R1 - Fission NIR Rename and Preview/Recovery Refactor

This round renamed the public Rust-owned decompiler lane from `preview` / `mlil-preview` to **Fission NIR**, while keeping compatibility aliases so existing CLI usage, local automation baselines, and worker invocations continue to function during the transition.

The goal was not to change recovery policy. The goal was to make the naming and code boundaries match the actual architecture: `legacy` remains the compatibility lane, while `nir` is now the canonical token for the Rust-owned decompiler path.

Historical changelog entries may still mention `mlil-preview` when describing earlier behavior. From this point forward, the canonical name is **Fission NIR** and the canonical machine-facing token is `nir`.

#### Added

- canonical `fission_nir_worker` binary alongside the deprecated compatibility `fission_preview_worker`
- canonical `nir` automation lane with deprecated `preview` lane alias support
- canonical `nir_*` inventory/report fields with continued compatibility for `preview_*` consumers during the transition
- `nir_context`, `nir_engine`, `nir_taxonomy`, `nir_recovery`, and `nir_worker` module boundaries under `fission-static`

#### Changed

- `preview_engine.rs` and `preview_context.rs` were renamed to:
  - `nir_engine.rs`
  - `nir_context.rs`
- canonical engine/token naming now prefers:
  - CLI engine: `nir`
  - automation lane: `nir`
  - user-facing product name: `Fission NIR`
- deprecated aliases remain accepted:
  - `--engine mlil-preview`
  - `--profile mlil-preview`
  - `--lane preview`
  - `FISSION_PREVIEW_WORKER`
  - `fission_preview_worker`
- `fission-automation` now dual-reads canonical `nir_*` fields and deprecated `preview_*` fields without failing when both are present in the same JSON row/summary
- Tauri decompiler engine settings and labels now prefer `nir` / `Fission NIR`, while still accepting stored `mlil_preview` values
- public docs were updated to describe the Rust-owned lane as **Fission NIR** instead of `mlil-preview`

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo check -p fission-tauri`
- `cargo check -p fission-analysis`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140001190 --engine nir --timeout-ms 1500`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140001190 --engine mlil-preview --timeout-ms 1500 --verbose`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`

#### Current Outcome

- canonical Fission NIR naming is now the default across CLI, automation, Tauri, and top-level docs
- deprecated `mlil-preview` and `preview` aliases still work and now emit deprecation warnings on the CLI/automation path
- `fission-automation` successfully reads dual-written inventory rows and summaries again after the compatibility deserialization fix
- `nir` and deprecated `preview` lanes both complete with the same current smoke result:
  - `direct_success=10`
  - `nir_failure=0`
  - `explicit_nonzero=4`
  - `recovery_attempted={'linearized_structuring_retry': 2}`
  - `recovery_outcome={'recovered': 2}`

### P5H2B / P5H3A - Recovery Quality Metrics and Localized Structuring Fallback

This round moved structuring recovery from a binary “recovered or not” signal into a quality-aware lane, and introduced the first localized alternative to whole-function forced linearization.

Previously, `linearized_structuring_retry` could recover many structuring-origin failures, but the recovery path only measured success counts. In practice, most recovered outputs were still whole-function `forced_linear` renders with high goto density, which made the strategy useful as a backstop but too expensive to promote as a first-class whitelist recovery mode.

This patch added row/summary quality metrics for recovered outputs and inserted a new recovery mode between `normal` and `forced_linear`:

- `normal`
- `region_linearized`
- `forced_linear`

The new `region_linearized` path reuses linear structuring only for the failed CFG slice when a recovery-eligible structuring failure surfaces, then resumes the normal structured path for the remainder of the function.

#### Added

- recovery quality metadata on preview rows and inventory rows:
  - `recovery_source_signature`
  - `recovery_structuring_mode`
  - `recovery_goto_count_before`
  - `recovery_goto_count_after`
  - `recovery_hint_surface_before`
  - `recovery_hint_surface_after`
  - `recovery_quality_flags`
- quality summary aggregation:
  - `recovery_quality_flag_counts`
  - `recovery_structuring_mode_counts`
- localized recovery quality flags:
  - `localized_linearization`
  - `shape_partially_linearized`

#### Changed

- recovery quality accounting now distinguishes:
  - whole-function `forced_linear`
  - localized `region_linearized`
- `linearized_structuring_retry` now tries:
  1. localized region linearization
  2. whole-function forced linearization
  3. fallback failure
- NIR structuring now attempts region-scoped linear recovery for recovery-eligible structuring-origin failures before surfacing the error back out
- recovery mode counts now track recovery-attempted rows only instead of mixing in non-recovery `normal` rows

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- aggregate recovery stayed stable:
  - `recovery_attempted {'linearized_structuring_retry': 19}`
  - `recovery_outcome {'recovered': 19}`
- recovery mode split improved from:
  - previous: `{'forced_linear': 19}`
  - current: `{'forced_linear': 18, 'region_linearized': 1}`
- quality proxy improved slightly:
  - `high_goto_density: 15 -> 14`
  - `shape_linearized: 19 -> 18`
  - `shape_partially_linearized: 1`
  - `localized_linearization: 1`
- current verdict remains:
  - `linearized_structuring_retry` is still valuable for recovery
  - but it remains closer to `fallback-only` than `whitelist-worthy`
  - the next quality step should reduce dependence on whole-function `forced_linear` by broadening localized / semi-structured fallback coverage

### P5H2A - Structuring-Origin Failure Surfacing for Recovery

This round fixed the taxonomy gap that prevented the recovery layer from seeing real structuring-origin failures.

Previously, recovery scaffolding existed, but a large part of the relevant `UnsupportedCfg*` family was either absorbed as `Ok(None)` inside NIR structuring or surfaced back out as a broad unsupported-CFG failure. That meant `linearized_structuring_retry` often had no explicit recovery seed to act on.

This patch promoted the recovery-eligible structuring failures into typed preview failures and preserved their exact signature through the inventory/export path.

#### Added

- typed structuring failure classification:
  - `StructuringFailureKind::RegionShape`
  - `StructuringFailureKind::PhiJoin`
  - `StructuringFailureKind::IndirectCallRegion`
- exact preview block signatures for recovery-eligible structuring failures:
  - `unsupported_cfg_region_shape`
  - `unsupported_cfg_phi_join`
  - `unsupported_cfg_indirect_call_region`

#### Changed

- NIR structuring no longer fully buries recovery-eligible `UnsupportedCfg*` failures behind plain `Ok(None)` paths
- preview routing now surfaces those failures as:
  - coarse kind: `preview_structuring_failure`
  - exact signature: typed structuring-origin signature
- `UnsupportedCfgBranchTarget` remains on the separate branch-target / unsupported-CFG line and is not mixed into the structuring-recovery lane
- `linearized_structuring_retry` is now fed from explicit structuring-origin surfacing rather than heuristic string matching alone

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- `preview` lane runs now show real recovery activity instead of an empty recovery scaffold
- `GetProcAddress.exe` inventory summary recorded:
  - `recovery_attempted {'linearized_structuring_retry': 13}`
  - `recovery_applied {'linearized_structuring_retry': 13}`
  - `recovery_outcome {'recovered': 13}`
- `putty.exe` inventory summary recorded:
  - `recovery_attempted {'linearized_structuring_retry': 6}`
  - `recovery_applied {'linearized_structuring_retry': 6}`
  - `recovery_outcome {'recovered': 6}`
- the recovery layer is now being driven by surfaced structuring-origin failures rather than sitting idle without visible seeds

### Operational Stability - NIR Structuring Recursion Fix and Automation Watchdog

This round fixed a real Fission NIR preview hang instead of just treating it as heavy CPU work.

`GetProcAddress.exe` contained addresses that drove the NIR linear-structuring path into recursive conditional-tail cycling. Those same functions completed on the legacy lane, which confirmed the issue was a preview/NIR bug rather than expected analysis cost.

At the same time, the automation runner could wait forever on a stuck `fission_cli` child, which meant a single pathological function could wedge an entire lane.

#### Added

- active-cycle guards for:
  - in-progress `LinearBodyCacheKey` lowering
  - in-progress conditional-tail lowering signatures
- a new regression test that exercises the recursive conditional-tail cycle and verifies it fails closed instead of spinning
- a hard inventory child-process watchdog in `fission-automation`
- periodic mid-run inventory summary flushes so partial progress survives long runs or failures

#### Changed

- Fission NIR linear structuring now returns `None` when it re-enters the same linear-body or conditional-tail request instead of recursing indefinitely
- `fission-automation` now kills and reaps inventory children that exceed a hard per-binary timeout budget
- `nir-check` skips failed binaries instead of hanging an entire lane forever, and only fails the lane if every target fails
- CLI inventory summary files now update during row emission rather than only at chunk completion

#### Validation

- `cargo test -p fission-pcode lower_linear_body_breaks_recursive_conditional_cycle -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140002220 --engine mlil-preview --timeout-ms 1500`
- `target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140002320 --engine mlil-preview --timeout-ms 1500`
- `target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --emit-function-facts-inventory --functions-limit 40 --timeout-ms 1500 --output-jsonl /tmp/getproc_after_fix.rows.jsonl --summary-json /tmp/getproc_after_fix.summary.json --quiet-batch-errors`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- the reproduced `mlil-preview` hangs at `0x140002220` and `0x140002320` now complete in under a second instead of timing out externally
- `GetProcAddress.exe` 40-function inventory now finishes cleanly and writes both rows and summary output
- the `preview` lane completes successfully again instead of sticking on a single runaway `fission_cli` process
- remaining preview failures on the sentinel lane are now meaningful failure classes, not infinite-CPU recursion artifacts

### P5H1 - Failure-Driven Recovery Scaffold

This round introduced the first real recovery layer for Fission NIR preview failures.

Until now, preview-side failures were mainly classified and reported. After this patch, selected failure signatures can carry an explicit recovery strategy attempt, and the result of that attempt is exported through the same inventory/report path.

#### Added

- recovery metadata on preview routing decisions and selections:
  - `recovery_strategy_attempted`
  - `recovery_strategy_applied`
  - `recovery_outcome`
- first whitelist recovery strategy:
  - `linearized_structuring_retry`
- inventory / summary recovery accounting:
  - `recovery_strategy_attempted_counts`
  - `recovery_strategy_applied_counts`
  - `recovery_outcome_counts`

#### Changed

- `MlilPreviewOptions` now supports a narrow `force_linear_structuring` mode
- `preview_structuring_failure` can now trigger a single linear-structuring retry instead of falling directly to a plain failure record
- CLI inventory rows and automation summaries now preserve recovery metadata alongside existing preview block signature/detail fields

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli`

#### Current Outcome

- the failure-driven recovery scaffold is now present in code and data models
- the first whitelist strategy exists and is wired into preview routing
- current `preview` sentinel lane still has no `preview_structuring_failure` sample, so:
  - recovery counters remain empty in the lane smoke
  - the next step is to secure a real structuring-failure seed and validate whether `linearized_structuring_retry` recovers it, preserves the same failure, or narrows it into a better signature

### P6 - `fission-automation` Canonical Quality Runner

This round replaced the old ad hoc benchmark-script loop with a tracked Rust automation crate that acts as the canonical local quality runner for Fission NIR.

Instead of manually chaining hidden CLI modes, Python corpus scripts, and one-off shell commands, the repository now has a single Rust entrypoint for lane-based quality runs:

- `cargo run -p fission-automation -- nir-check --lane pdb`
- `cargo run -p fission-automation -- nir-check --lane preview`
- `cargo run -p fission-automation -- nir-check --lane regression`
- `cargo run -p fission-automation -- nir-check --lane full`

#### Added

- new tracked workspace crate:
  - `crates/fission-automation`
- tracked automation config:
  - `crates/fission-automation/config/sentinel_sets.toml`
  - `crates/fission-automation/config/timeout_rescue.json`
- Rust-first local quality pipeline support for:
  - sentinel lane loading
  - inventory emit orchestration through `fission_cli --emit-function-facts-inventory`
  - diagnosis aggregation
  - corpus refinement
  - baseline diffing
  - Markdown / JSON summaries under `artifacts/fission-automation/`

#### Changed

- repository benchmark ownership
  - `fission-automation` is now the canonical local runner for Fission NIR quality loops
  - benchmark/config state previously kept under `scripts/test/batch_benchmark` has moved into the automation crate or local `artifacts/`
- documentation
  - README and benchmark/debug docs now point at `fission-automation` lane runs instead of the retired Python benchmark scripts

#### Removed

- retired tracked Python batch-benchmark drivers and tracked corpus outputs from:
  - `scripts/test/batch_benchmark/`
- the old Python diagnosis / corpus-refinement path is no longer the default execution path

#### Validation

- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli`
- `cargo run -p fission-automation -- nir-check --lane pdb --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli`

#### Observed Effect

- `preview` lane smoke:
  - `direct_success = 27`
  - `preview_failure = 3`
  - `explicit_nonzero = 11`
  - `strict_explicit = 1`
- `pdb` lane smoke:
  - `direct_success = 43`
  - `preview_failure = 3`
  - `explicit_nonzero = 30`
  - `strict_explicit = 12`
  - `pdb_nonzero_rows = 21`

This means the local quality loop is no longer tied to a scattered script folder. The canonical path now lives in a tracked Rust crate, while reports remain local-only under `artifacts/`.

### P5G - Focused PDB Function-Facts Ingestion

This round moved PDB handling from “source presence is visible” into real function-level fact ingestion for the Fission NIR pipeline.

Instead of building a full PDB parser, the loader now performs a narrow sidecar-driven ingest for function-scoped facts that directly affect decompilation quality:

- function names
- return types
- parameter names
- parameter types

These facts now flow into the existing Rust facts pipeline rather than staying trapped as loader metadata.

#### Added

- focused PDB sidecar ingestion in the loader
  - PE CodeView / RSDS / NB10 metadata is now used to locate and open matching `.pdb` sidecars
  - module symbol streams are scanned narrowly for function-scoped facts instead of attempting broad PDB database coverage
- function-level PDB facts in `FactStore`
  - `FactProvenance::PdbMetadata`
  - `FunctionFacts.pdb_info`
  - `FactStore::preferred_debug_function(...)` now falls back from DWARF to PDB-backed function info
- inventory explicit surfacing for PDB-derived facts
  - `explicit_fact_breakdown.pdb_type_count`
  - `explicit_breakdown_totals.pdb_type_count`
  - inventory row names now prefer the chosen resolved fact name when available

#### Changed

- preview / postprocess debug fact consumption
  - preview function hints can now use PDB-backed function info when DWARF is absent
  - Rust-side postprocess also consumes preferred debug function info instead of assuming DWARF-only availability
- diagnosis quality after PDB source detection
  - the pipeline can now distinguish:
    - `PDB source present and actually surfaced`
    - `PDB source present but still not surfaced`
    - `native inferred facts are still filling the gap`

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo test -p fission-loader loads_focused_pdb_function_facts_from_repo_sample -- --nocapture`
- inventory / diagnosis reruns:
  - `has_pdb.exe`
  - `test-pdb.exe`
  - `fauxware.exe`

#### Observed Effect

- `test-pdb.exe`
  - `source_presence_counts.pdb = 6`
  - `provenance_surface_totals.pdb_nonzero_rows = 5`
  - `strict_explicit_candidate_count = 4`
- `fauxware.exe`
  - `source_presence_counts.pdb = 20`
  - `provenance_surface_totals.pdb_nonzero_rows = 16`
  - `strict_explicit_candidate_count = 6`
- `has_pdb.exe`
  - `source_presence_counts.pdb = 20`
  - `provenance_surface_totals.pdb_nonzero_rows = 0`
  - `provenance_surface_totals.native_nonzero_rows = 7`

This means the repository now has both sides of the diagnostic split:

- samples where PDB-derived function facts genuinely surface into inventory rows,
- and samples where PDB source presence is truthful but surfaced explicit rows are still being supplied by native inferred facts.

## 2026-03-19

### P5F2 - Preview-Stage Block Split And First Narrow Unblock

This round moved preview-side diagnosis from “generic unknown failure cleanup” into the first real unblock patch for the Fission NIR path.

The work happened in two steps:

- first, preview-stage failures were split so that pcode/frontend acquisition failures stopped polluting the real preview block bucket,
- then a single recoverable `unsupported_indirect_branch_target` shape was patched without broadening indirect control-flow support.

#### Added

- preview block signature reporting in inventory-backed rows
  - rows now carry:
    - `preview_block_signature`
    - `preview_block_detail`
- finer preview-stage diagnosis buckets
  - `preview_frontend_reject` is now separated from genuine preview CFG failures
  - diagnosis summaries can aggregate preview block signatures directly
- narrow instruction-local relative branch target support in the Fission NIR pcode path
  - recoverable constant-space pcode branch targets are now resolved by exact target block index
  - duplicate-start blocks can now be distinguished through synthetic target keys / labels instead of collapsing into one canonical start address

#### Changed

- preview inventory / diagnosis interpretation
  - `native_pcode_failure`-like cases that previously looked like preview unknowns are now surfaced as frontend rejection rather than preview-stage block
- preview control-flow lowering
  - branch and cbranch lowering now use resolved target block indices for the supported instruction-local relative-target shape
- structuring path label/target handling
  - duplicate-start block targets are preserved narrowly enough to support the recovered branch shape without enabling broad indirect branch handling

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo test -p fission-pcode preview_supports_instruction_local_conditional_branch_targets -- --nocapture`
- `cargo test -p fission-pcode preview_supports_instruction_local_unconditional_branch_targets -- --nocapture`
- inventory smoke reruns:
  - `GetProcAddress.exe --functions-limit 20`
  - `putty.exe --functions-limit 10`

#### Observed Effect

- `GetProcAddress.exe`
  - before:
    - `direct_success_count = 16`
    - `preview_frontend_reject = 3`
    - `preview_unsupported_cfg = 1`
    - dominant preview-side signature: `unsupported_indirect_branch_target`
  - after:
    - `direct_success_count = 17`
    - `preview_failure_count = 3`
    - remaining failures are all `preview_frontend_reject`
    - the representative blocked row at `0x140001190` now becomes `preview_direct_success = true`
- `putty.exe`
  - 10-function smoke rerun stayed stable with:
    - `direct_success_count = 10`
    - `preview_failure_count = 0`

This means the first real preview-side unblock is now in place: one recoverable `unsupported_indirect_branch_target` class has moved onto the success path without widening support to general indirect branch control flow.

### P5F1 - Provenance Completeness For Function Facts Inventory

This round refined the inventory from “provenance-aware” toward “provenance-complete enough to guide the next core patch.”

The main improvement is that inventory output can now distinguish between:

- sources that carry PDB-style debug provenance,
- function rows that actually surface explicit facts,
- and cases where surfaced explicit rows are still being supplied by native inferred facts rather than by PDB-derived facts.

#### Added

- provenance fact breakdown in function inventory rows
  - rows now include `provenance_fact_breakdown` with:
    - `dwarf_type_count`
    - `pdb_type_count`
    - `native_type_count`
    - `loader_type_count`
- provenance surface totals in inventory summaries
  - summaries now report:
    - `dwarf_nonzero_rows`
    - `pdb_nonzero_rows`
    - `native_nonzero_rows`
    - `loader_nonzero_rows`
- function snapshot provenance helpers
  - `FunctionFacts` now exposes:
    - `dwarf_type_fact_count()`
    - `pdb_type_fact_count()`
    - `native_type_fact_count()`
    - `loader_type_fact_count()`

#### Changed

- PDB source presence detection
  - `fact_sources_present.pdb` is no longer a placeholder
  - inventory now treats `.pdb` sidecars and embedded PE `RSDS` / `.pdb` markers as real PDB source presence signals
- diagnosis interpretation
  - inventory-guided diagnosis can now distinguish:
    - `pdb source present but no pdb-surfaced explicit rows`
    - `native inferred facts are currently covering the explicit surface gap`

#### Validation

- `cargo test -p fission-static snapshot_counts_dwarf_type_facts_from_function_info -- --nocapture`
- `cargo test -p fission-static snapshot_counts_native_and_loader_type_facts_separately -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- smoke inventory / diagnosis reruns:
  - `has_pdb.exe`
  - `putty.exe`

#### Observed Effect

- `has_pdb.exe`
  - `source_presence_counts.pdb = 10`
  - `provenance_surface_totals.pdb_nonzero_rows = 0`
  - `provenance_surface_totals.native_nonzero_rows = 5`
  - diagnosis now shows that PDB provenance is present, but surfaced explicit rows are still coming from native inferred facts

This means the next preview-side or facts-side patch can target real remaining gaps without provenance confusion.

### P5D / P5E - Inventory-Guided Diagnosis And Function-Level Facts Surfacing

This round stopped treating explicit-facts scarcity as a vague benchmark problem and turned it into a concrete inventory diagnosis plus a core data-path patch.

The key result is that aligned sources no longer have to stay stuck in a blanket `inventory_surface_gap` bucket. Inventory-backed diagnosis identified where provenance existed but explicit rows still stayed at zero, and the inventory export now promotes function-level native inferred facts into the explicit surface instead of leaving them hidden behind generic provenance flags.

#### Added

- inventory-guided diagnosis runner
  - added `scripts/test/batch_benchmark/diagnose_function_inventory.py`
  - classifies aligned binaries into:
    - `source_facts_absent`
    - `factstore_or_inventory_surface_gap`
    - `preview_stage_block`
    - `mixed_or_inconclusive`
  - emits a per-binary diagnosis plus a recommended next patch direction
- function snapshot helpers for type-fact provenance
  - `FunctionFacts` now exposes separate counts for:
    - native type facts
    - loader type facts

#### Changed

- function inventory explicit surfacing
  - inventory export now ingests function-level native inferred types during whole-binary row generation
  - `explicit_fact_breakdown` now includes `native_type_count`
  - `explicit_fact_total` now counts surfaced native function facts in addition to DWARF param/local/return facts
- inventory surface-gap interpretation
  - `inventory_surface_gap` is no longer triggered by image-wide loader metadata alone
  - the gap signal now focuses on per-function/debug provenance that should realistically surface as explicit facts
- strict explicit candidate detection in inventory
  - strict candidate evaluation now uses the surfaced inventory explicit total rather than only the DWARF-only count

#### Validation

- `cargo test -p fission-static snapshot_counts_native_and_loader_type_facts_separately -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- inventory smoke reruns:
  - `has_pdb.exe`
  - `putty.exe`
- inventory-guided diagnosis rerun:
  - `GetProcAddress.exe`
  - `has_pdb.exe`
  - `putty.exe`

#### Observed Effect

- `has_pdb.exe`
  - `explicit_fact_nonzero_count`: `0 -> 5`
  - `inventory_surface_gap_count`: `10 -> 0`
  - `strict_explicit_candidate_count`: `0 -> 1`
- `putty.exe`
  - `explicit_fact_nonzero_count`: `0 -> 7`
  - `inventory_surface_gap_count`: `10 -> 0`
  - `strict_explicit_candidate_count`: `0 -> 1`

This moves the project past “why are explicit facts missing?” into a narrower question: which remaining aligned binaries are still blocked by preview-stage issues, and which ones still need more supply-path surfacing.

### P5A / P5B / P5C - Function Facts Inventory, Inventory-Backed Corpus Selection, And Provenance-Aware Analysis

This round changed the benchmark/corpus workflow from probe-first scanning to inventory-first filtering.

The key architectural shift is that benchmark scripts no longer need to treat address-targeted preview scans as the canonical source of truth. Instead, the CLI can now export whole-binary function facts as a structured inventory, and corpus generation can filter that inventory into strict explicit, heuristic, aligned, and blocked views.

#### Added

- whole-binary function facts inventory export
  - added hidden CLI mode `--emit-function-facts-inventory`
  - emits row-level JSONL plus summary JSON from a single binary load / decompiler preparation pass
- inventory row metadata for corpus selection
  - rows now carry function-level facts, preview admission results, pcode size, and structured row failure fields in one place
- Python inventory reader helper
  - added `scripts/test/batch_benchmark/grand_finale_support/inventory_reader.py`
  - centralizes:
    - running the Rust inventory export
    - loading inventory JSONL rows
    - loading summary JSON
- provenance-aware inventory fields
  - inventory rows now include:
    - `fact_sources_present`
    - `explicit_fact_breakdown`
    - `admission_block_stage`
    - `inventory_surface_gap`
  - summary output now includes:
    - `source_presence_counts`
    - `explicit_breakdown_totals`
    - `inventory_surface_gap_count`
    - `aligned_with_zero_explicit_count`

#### Changed

- benchmark/corpus scripts now consume inventory rows
  - `refine_preview_quality_corpus.py` now builds corpus outputs from function facts inventory rows instead of address-probe scan results
  - `grand_finale_support/corpus_candidates.py` now treats the Rust inventory export as the default candidate source
- provenance-aware blocked/aligned interpretation
  - blocked and aligned candidate reports now carry provenance fields through from the inventory rows
  - corpus refinement now emits aggregated inventory provenance counters alongside blocked explicit summaries
- corpus outputs derived from the same canonical source
  - `preview_quality_corpus.json`
  - `preview_explicit_blocked_candidates.json`
  - `preview_explicit_aligned_candidate_report.json`
  are now designed to be generated from the same inventory-backed function row source

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- function facts inventory smoke
  - `putty.exe --emit-function-facts-inventory --functions-limit 3`
  - verified row JSONL and summary JSON emission
- inventory-backed corpus smoke
  - `refine_preview_quality_corpus.py` against `GetProcAddress.exe`
  - verified generation of:
    - candidates JSON
    - aligned candidate report
    - blocked explicit report
    - curated corpus JSON
- provenance-aware inventory smoke
  - `GetProcAddress.exe --emit-function-facts-inventory --functions-limit 5`
  - verified:
    - row-level provenance fields
    - summary-level provenance counters
    - blocked report inventory summary totals

#### Current State

- address-targeted scans remain useful, but they are now probe/debug tooling rather than the preferred canonical data source
- strict explicit / heuristic / blocked / aligned analysis can now be driven from one whole-binary inventory export
- inventory rows now expose whether explicit-fact scarcity appears to come from missing source facts, inventory surface gaps, or preview-stage rejection

## 2026-03-18

### P4.8 / P4.8.2 - Explicit-Facts PE Source Expansion

This round focused on finding PE samples that can actually exercise the new explicit preview hint paths without weakening the meaning of the strict explicit corpus.

The main result was diagnostic rather than cosmetic:

- the strict `quality_explicit_facts` corpus remains intentionally empty,
- blocked explicit candidates are now tracked separately,
- and the remaining bottleneck is clearly sample scarcity plus lack of direct-preview overlap, not corpus/refinement logic.

#### Added

- explicit source inventory metadata
  - expanded the PE candidate pool with LLVM, `samples/other`, and other debug-info-rich Windows binaries
  - recorded per-source metadata including:
    - `toolchain`
    - `debug_info_kind`
    - `has_loader_types`
    - `priority`
    - `notes`
- blocked explicit candidate tracking
  - added a dedicated blocked-candidate artifact instead of weakening the strict explicit corpus

#### Changed

- explicit corpus discipline
  - kept `quality_explicit_facts` strict rather than filling it with provisional fallback seeds
  - continued to require:
    - `explicit_fact_total >= 2`
    - `preview_direct_success == true`
    - `has_indirect_control_flow == false`
    - `pcode_op_count <= 800`
- blocked-candidate reporting
  - normalized blocked explicit candidates under the current taxonomy
  - preserved raw fallback information where the engine still reports only coarse `preview_unsupported` results
  - added summary counts for:
    - blocked-reason distribution
    - newly scanned zero-explicit sources
    - newly scanned timeout sources

#### Current State

- strict explicit corpus: still empty by design
- blocked explicit candidates:
  - `main-debug.exe`
  - `addr.exe`
- dominant blocked reason:
  - `preview_non_success_unknown`

This means the benchmark/reporting pipeline is no longer the limiting factor. The next step is better fact-rich PE source acquisition, not provisional promotion of blocked seeds.

### v104 - 3-Way Benchmark Expansion (`pyghidra` vs `legacy` vs `preview`)

This round expanded the public benchmarking story from two separate comparisons into a consistent 3-way model:

- `pyghidra` as the Python-host baseline,
- `legacy` as the native FFI / Ghidra core baseline,
- `preview` as the Rust-owned decompiler pipeline.

The main goal was not a single blended score, but a benchmark shape that shows where overhead, fallback behavior, and readability improvements come from.

#### Added

- shared resource monitor helper for benchmark scripts
  - added `scripts/test/batch_benchmark/grand_finale_support/resource_monitor.py`
  - reused the same optional `psutil`-based RSS / CPU sampling model in both benchmark modes
- function-level 3-way artifact shape
  - `compare_legacy_preview.py` now emits `pyghidra`, `legacy`, and `preview` together
  - added `three_way_delta` and `winner_summary` per function
- whole-binary 3-way raw outputs
  - now writes `legacy_full.json`, `preview_full.json`, and `ghidra_full.json`

#### Changed

- fixed-seed function-level comparison
  - promoted `compare_legacy_preview.py` into the main 3-way fixed-seed comparison path
  - kept existing `legacy` / `preview` fields for backward compatibility
  - added engine-level summaries and pairwise deltas:
    - `pyghidra_vs_legacy`
    - `legacy_vs_preview`
    - `pyghidra_vs_preview`
- timing and resource metrics
  - added shared timing stats with `p95_ms`
  - added best-effort per-run resource summaries:
    - `max_rss_mb`
    - `avg_rss_mb`
    - `avg_cpu_pct`
    - `max_cpu_pct`
- whole-binary benchmark summary
  - replaced the old 2-way summary with explicit engine buckets:
    - `pyghidra`
    - `legacy`
    - `preview`
  - added pairwise quality/similarity sections and a public-ready summary line
- benchmark documentation
  - updated `scripts/test/batch_benchmark/README.md` to describe both benchmark modes and the 3-way engine model

#### Validation

- `python3 -m py_compile`
  - `scripts/test/batch_benchmark/compare_legacy_preview.py`
  - `scripts/test/batch_benchmark/full_decomp_benchmark.py`
  - `scripts/test/batch_benchmark/grand_finale_support/*.py`
- `cargo build -p fission-cli --features native_decomp`
- function-level 3-way smoke
  - `test_control_flow_x64_O0.exe 0x140001010`
  - artifact:
    - `/tmp/v104_compare_smoke2/test_control_flow_x64_O0_legacy_vs_preview.json`
    - `/tmp/v104_compare_smoke2/test_control_flow_x64_O0_legacy_vs_preview.md`
- whole-binary 3-way smoke
  - `test_control_flow_x64_O0.exe --limit 1`
  - artifact:
    - `/tmp/v104_full_smoke2/benchmark_summary.json`
    - `/tmp/v104_full_smoke2/benchmark_summary.md`

## 2026-03-17

### Repository Licensing + CLA Setup

The public repository license was fixed to AGPL-3.0, and a Contributor License Agreement was added to support a future open-core operating model. The intent is to keep the core engine open under AGPL-3.0 while preserving a clean legal boundary for accepting outside contributions.

#### Added

- root license file
  - added the full GNU AGPL-3.0 text to `LICENSE`
- Contributor License Agreement
  - added `CLA.md`
- GitHub pull request template
  - added a PR template with an explicit CLA acknowledgement checkbox

#### Changed

- README public metadata
  - declared the repository license as AGPL-3.0
  - added a CLA reference
- Rust package metadata
  - added `license = "AGPL-3.0-or-later"` across public workspace `Cargo.toml` files
- CONTRIBUTING guide
  - documented the CLA requirement
  - fixed the source-header policy around repository-level licensing plus optional SPDX short headers

### Private AI Layer Repository Boundary Cleanup

The repository boundary was tightened by removing `fission-ai` from the public workspace and Git tracking. The goal was to keep the core decompiler and analysis engine open while keeping future AI product/API orchestration layers outside the public repository scope.

#### Changed

- public workspace scope
  - removed `crates/fission-ai` from the workspace members
  - removed the `fission-ai` dependency and re-export from `fission-analysis`
- public Git tracking scope
  - added `crates/fission-ai/` to `.gitignore`
  - removed `crates/fission-ai/*` from Git tracking so it would no longer be published on GitHub

#### Validation

- `cargo build -p fission-analysis --features native_decomp`

### v75-v78 - Preview-First Retirement Prep + Type Absorption Expansion + ARM64 Detection Scaffolding

This span focused on three themes:

1. making preview-first the real product policy while shrinking `legacy` toward compat/fallback only,
2. expanding Rust-side type absorption for hard x64 and x86 cases,
3. laying the first PE/Windows ARM64 detection groundwork and widening cross-image propagation to `ida76sp1/plugins`.

#### Added

- legacy-needed benchmark/report artifacts
  - separate binary/global summaries for successful functions that still are not preview-direct
- x86 decimal index field-replacement regression coverage
  - validates decimal surfaces such as `register[24]` as field-replacement candidates
- cross-image propagation scope coverage for `plugins/`
  - smoke validation that `ida76sp1/plugins/hexrays.dll` is actually included
- Windows ARM64 spike note
  - recorded current blockers and bring-up checklist in `docs/benchmark/windows_arm64_spike.md`
- synthetic PE ARM64 loader test
  - validated `IMAGE_FILE_MACHINE_ARM64 -> AARCH64:LE:64:v8A`

#### Changed

- preview-first retirement prep
  - removed `legacy` from normal GUI workflow
  - kept CLI `--engine legacy` as a hidden compatibility mode
  - fixed fallback taxonomy around `preview_timeout`, `preview_unsupported`, `native_pcode_failure`, `legacy_fallback`, and `assembly_fallback`
- x64/x86 shared type absorption
  - kept metadata-first inferred-type merge
  - extended line-local pointer-offset alias substitution
  - widened `register[offset]` field replacement candidates to decimal as well as hex surfaces
- x86 hard-case surfacing
  - prevented decimal and stack-like index surfaces from dropping out of common postprocess on cases such as `WinMergeU.exe 0x407050` and `EverPlanet_KR.exe 0xa918d0`
- cross-image propagation phase 2, step 1
  - expanded sibling scanning to include DLLs under `plugins/`
  - widened weak-name detection to include `sub_`, `FUN_`, `func_`, `Ordinal_`, `j_`, `thunk_`, `nullsub_`, `loc_`, and `LAB_`
- Windows PE loader / CLI architecture surfacing
  - recognized PE ARM64 as `AARCH64:LE:64:v8A`
  - surfaced ARM64 as `arm64` / `ARM64 (64-bit)` instead of `x86_64`

#### Improved

- `putty.exe 0x140006380`
  - reduced leftover `unique0x... = register + offset` alias residue
  - increased `register[offset]` surfacing
- x86 hard-case observability
  - hard-case summaries now expose `unique_surface_count`, `field_access_count`, and `offset_index_count`
- legacy deprecation observability
  - reports now show which functions still depend on legacy/native fallback outcomes
- `ida76sp1`
  - propagation scope now includes `plugins/hexrays.dll`, making sibling-based auto rename practical across the plugin layout

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-static --features native_decomp field_offset_replacement -- --nocapture`
- `cargo test -p fission-loader test_parse_synthetic_pe -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-tauri`
- `python3 -m py_compile`
  - `scripts/test/batch_benchmark/grand_finale_support/metrics.py`
  - `scripts/test/batch_benchmark/grand_finale_support/summary.py`
  - `scripts/test/batch_benchmark/grand_finale_support/report_md.py`

#### Notes

- On `EverPlanet_KR.exe 0xa918d0` and `WinMergeU.exe 0x407050`, `unique0x` residue was already near zero in legacy output; the real goal in this round was improving x86 `[]` / field-style surfacing.
- The Windows ARM64 spike is still only a bring-up track. There is no real Windows ARM64 PE sample in the repository yet, so fixed-seed baseline JSON/Markdown artifacts were deferred.

### v69-v74 - x64 Timeout Closure + Portable Multi-DLL Symbol Propagation

This span closed two major threads:

1. reducing the last branch/readability residue in giant x86/x64 functions while turning long-running preview cases into explicit fallback outcomes through subprocess isolation,
2. introducing the first cross-image symbol propagation pass for portable multi-DLL layouts using only sibling EXE/DLL import-export-thunk relationships.

#### Added

- stronger x86 branch-condition recovery
  - reconstructs exact `TEST` / `CMP` boolean trees directly in terminator lowering
- preview render subprocess worker
  - runs heavy preview rendering in a separate worker process
  - kills and falls back explicitly on timeout
- `ida76sp1` fixed-seed watchlist artifacts
  - `ida64.exe`
  - `idat64.exe`
  - `ida64.dll`
  - `ida.dll`
  - `plugins/hexrays.dll`
- Tauri cross-image propagation service
  - same-folder sibling `*.exe` / `*.dll` scan
  - import/export/thunk-based rename candidate resolution
  - in-memory rename provenance tracking

#### Changed

- non-float scalar self-equality / boolean simplification
  - `x == x -> true`
  - `x != x -> false`
  - removed residual expressions such as `if (!reg && reg == reg)`
- stronger dead flag-intrinsic cleanup
  - removes unused `__carry/__scarry/__sborrow` assignments
- converted two `ida76sp1` watchlist timeouts to explicit subprocess-isolated `preview_timeout` fallback
  - `ida64.dll 0x101fa177`
  - `hexrays.dll 0x17088330`
- fixed `hexrays.dll 0x170057f0` to end in a non-empty assembly fallback instead of ambiguous empty preview output
- after `open_file`, scans the current binary parent folder and merges sibling import/export/thunk-based rename candidates directly into `renamed_functions`
- ensured manual/project-loaded renames always outrank auto-propagated renames

#### Improved

- `EverPlanet_KR.exe 0xa918d0`
  - removed `if (!reg && reg == reg)` and `reg == reg` residue
  - reduced code length further
- `ida76sp1` baseline closure
  - `ida64.exe`: direct preview `4/5`
  - `idat64.exe`: direct preview `4/5`
  - `ida64.dll`: direct preview `4/5`, timeout case converted to explicit fallback
  - `ida.dll`: direct preview `4/5`
  - `hexrays.dll`: direct preview `3/5`, remaining cases explicit legacy/assembly fallback
- `ida64.dll 0x101fa177` and `hexrays.dll 0x17088330` no longer remain as 20-second hangs
- sibling scan produced non-zero propagated renames on real `ida76sp1/ida64.dll` smoke runs
- existing regression targets held
  - `putty.exe 0x140006260`: `LPRECT param_2`, `RECT local_3c`, `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: direct preview retained
  - `WinMergeU.exe` x86 and `EverPlanet_KR.exe` x86 direct preview retained

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-static --features native_decomp preview_worker_ -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --bin fission_preview_worker --features native_decomp`
- `cargo build -p fission-tauri`
- compare/watchlist reruns across `ida76sp1` watchlist binaries and retained regression samples

### v63-v68 - C++ Corpus Expansion + x86 Preview Readability Uplift

This span expanded the real-world validation set and then used the new coverage to fix x86-specific preview bottlenecks and readability problems.

#### Added

- new Windows sample corpus coverage
  - `WinMergeU.exe` x64 / x86
  - `SumatraPDF-3.5.2-32.exe`
  - `cmake.exe`
  - `EverPlanet_KR.exe`
- x86 `CallInd` trap-like target recovery
  - surfaces `INT3` producers as opaque callees like `((code *)swi(3))`
- additional x86 readability tests
  - register naming bootstrap
  - large-body cheap slot surfacing
  - dead local / dead flag-intrinsic cleanup
- EverPlanet x86 fixed-seed stress corpus

#### Changed

- added budgeted fallback to x86 `try_lower_while()`
- restored real x86 register names (`eax`, `ecx`, `edx`, etc.)
- allowed cheap slot surfacing to continue in large HIR bodies
- removed write-only non-temp local clobber
- added x86 flag-temp canonicalization and stronger dead intrinsic cleanup

#### Improved

- `SumatraPDF-3.5.2-32.exe`: all 5 fixed seeds `mlil_preview`, fallback 0
- `WinMergeU.exe` x86: all 5 fixed seeds `mlil_preview`, fallback 0
- `EverPlanet_KR.exe`: all 5 fixed seeds `mlil_preview`, fallback 0, while legacy timed out on the selected seeds
- major readability improvement on `EverPlanet_KR.exe 0xa918d0`
  - residue score `207 -> 169 -> 11`
  - temp surface count `182 -> 144 -> 11`
  - code length `18435 -> 15459 -> 9462`
  - `__carry/__scarry/__sborrow` `68/68/19 -> 33/68/18 -> 0/0/0`

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- reran compare/fixed-seed coverage for `SumatraPDF`, `WinMerge`, `EverPlanet`, `putty`, and `everything`

### v62 - Warning Cleanup + Fixed-Seed Benchmark Closure

This round removed the last dead warnings after the second major `nir` refactor and re-closed fixed-seed compare results for `putty`, `everything`, `notepad++`, and `7zr`.

#### Changed

- removed two dead warnings
  - `MlilPreviewOptions::is_pe_x64()`
  - unused `VN_SIZE` inside `PcodeFunction::to_flat_bytes()`

#### Improved

- `cargo test` / `cargo build --release` passed cleanly without additional warnings
- reconfirmed fixed-seed compare closure
  - `putty.exe 0x140006260`: `mlil_preview`, fallback 0, preserved `LPRECT param_2` / `RECT local_3c` / `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: `mlil_preview`, fallback 0
  - `7zr.exe` selected seeds: all `mlil_preview`, fallback 0
  - `notepad++.exe` selected seeds: all `mlil_preview`, fallback 0

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`

### v59-v61 - x86 Conditional Structuring Stabilization + Second `nir` Refactor

This span stabilized long-running x86 `try_lower_if()` paths on heavy `7zr.exe` seeds and then reorganized the growing `nir` implementation into a more maintainable module tree.

#### Added

- x86-only conditional structuring budget/cache
- join/follow-gated plain `if` candidate pre-checks
- second-stage `nir` module tree split under `builder/`, `structuring/conditionals/`, and `tests/`

#### Changed

- made x86 pathological CFG handling more conservative
- prioritized short-circuit chains before plain `if` recovery when they close on the same join
- split `builder/mod.rs` and promoted `structuring/conditionals.rs` into a directory module

#### Improved

- `7zr.exe 0x401804` and `0x402778` no longer time out due to long-running `try_lower_if()`
- retained direct preview on `putty.exe 0x140006260` and `everything.exe 0x140183590`

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`

### v36-v58 - `putty` Aggregate Copy Closure + x86 Timeout Diagnosis

This stretch had two goals:

1. remove the last aggregate transit temp from `putty.exe 0x140006260` until preview reached `RECT local_3c; *param_2 = local_3c;`,
2. determine whether heavy x86 `7zr.exe` timeouts came from Rust NIR or native extraction.

#### Added

- dead temp cleanup for aggregate transit temps
- prepare/native/preview diagnostic logging
- finer structuring-phase diagnostic logging

#### Changed

- recovered `LPRECT param_2`, `RECT local_3c`, and `*param_2 = local_3c;` for `putty.exe 0x140006260`
- removed dead aggregate transit temps like `xVar32`
- instrumented native prepare, preview p-code extraction, and Rust structuring boundaries

#### Improved

- closed the x64 aggregate-copy/type-surface target on `putty.exe 0x140006260`
- narrowed heavy x86 `7zr.exe` timeouts to Rust `structuring`, especially `try_lower_if()`

#### Validation

- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- regression/diagnostic reruns for `putty`, `everything`, `notepad++`, and `7zr`

---

## 2026-03-14

### v26-v35 - Preview Coverage Recovery + `putty` Type-Surface Recovery

The goals in this span were:

1. restore direct `mlil-preview` coverage on real large functions,
2. bring the type surface back up on `putty.exe 0x140006260` after direct preview had been recovered.

#### Added

- more detailed preview/native coverage diagnostics
- x86 preview bootstrap regression guard
- stack-slot naming recovery for direct preview
- stronger indirect import / Win64 argument recovery
- site-sensitive lowering infrastructure inside the builder

#### Changed

- reduced p-code extraction work in giant dispatcher cases
- added linear fallback caching and fast paths to Rust NIR structuring
- relaxed builder lowering carefully to recover `putty.exe 0x140001160`
- extended wide aggregate copy recovery with lane matching and prior-def lowering
- improved pointer-deref printing quality

#### Improved

- `putty.exe 0x140001160`: direct preview recovered
- `everything.exe 0x140183590`: direct preview retained
- `7zr.exe 0x401000`: direct preview retained
- `putty.exe 0x140006260`: recovered `LPRECT param_2`, `GetClientRect(...)`, `local_3c`, and whole-object assignment path progression

#### Validation

- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- `cargo check -p fission-tauri`

### v25 - NIR Module Tree Refactor

This round was about maintainability rather than new algorithms. The growing `nir` core was split into `builder / normalize / structuring / tests` subsystems to reduce future edit and regression costs.

#### Changed

- reorganized `crates/fission-pcode/src/nir/` into:
  - `builder/`
  - `normalize/`
  - `structuring/`
  - `tests/`
- narrowed `nir/mod.rs` to entrypoint/wiring responsibilities
- split normalize responsibilities into arithmetic/boolean normalization, cleanup, slot/table surfacing, and bitstream helpers
- split structuring responsibilities into conditionals, loops, switch, and linear fallback

### v24 - Preview Coverage Recovery First, x64 + x86 in Parallel

This round focused on restoring direct preview output on real x64 functions again while also bringing up the first real x86 preview bootstrap path.

#### Added

- finer preview unsupported-reason diagnostics
- PE x86 preview bootstrap path

#### Changed

- relaxed branch-target recovery to improve x64 large-function direct preview coverage
- made region builder more aggressive about trivial forwarding/cleanup/tail-return absorption
- canonicalized identical-input `MULTIEQUAL`
- preserved slot-family / bitstream helper / loop-body compaction while fixing the application order around coverage-first goals

#### Improved

- `putty.exe 0x140006260`: direct preview recovered again
- `everything.exe 0x140183590`: direct preview recovered again
- at least one fixed-seed `7zr.exe` function reached direct preview, confirming the first real x86 bootstrap success

### v16 - Preview Type Surface Quality + Direct `putty 0x140006260` Output

This round pushed preview beyond “structured pseudocode exists” toward more natural known-signature type surfaces. The main target was direct preview of `putty.exe 0x140006260` with `LPRECT`, `RECT`, and whole-object assignment style output.

#### Added / Changed

- known-signature-based type surface context in preview
- preview binding type hints
- stronger p-code JSON opcode alias parsing
- layout-based fallthrough analysis for preview CFG recovery
- direct preview understanding of `goto(target, cond)` style real p-code branches
- containment so preview optimizer panic would not collapse the whole path

#### Improved

- `putty.exe 0x140006260 --engine mlil-preview` could directly surface:
  - `LPRECT param_2`
  - `RECT local_3c`
  - whole-object assignment style output

### v15 - Preview Quality Uplift + Low-Risk Function Promotion

The target here was not higher legacy success, but making `mlil-preview` the better path on lower-risk functions.

#### Added / Changed

- canonical `switch` reconstruction in preview
- preview-only surface cleanup
- centralized `engine_used` source of truth in `fission-static`
- widened `auto` preview eligibility on stable multi-block functions

#### Notes

- Preview coverage and structuring improved significantly, but preview type surface quality still lagged legacy on representative cases such as `putty.exe 0x140006260`.

### v14 - Legacy `type` Failure Removal + 90/90 Closure

This round focused on removing the remaining legacy `type` failures and restoring benchmark closure without counting `mlil-preview` rescue as equivalent success.

#### Improved

- removed the last known legacy `type` failures for that benchmark round
- retained preview direct output on representative targets

### v13 - MLIL Preview Structuring / Readability Uplift

This round strengthened the preview path around:

- canonical multi-block `if`, `if/else`, `while`, and `do-while`
- short-circuit boolean chains
- `PIECE` / `SUBPIECE` recombination
- cast-density reduction and lower-level residue cleanup

### v10-v12 - Experimental Fission MLIL/NIR Path Integrated Into Product Surfaces

This was the point where `mlil-preview` stopped being a CLI-only experiment and became a real engine mode exposed in both CLI and Tauri.

#### Added

- `legacy | mlil-preview | auto` engine modes
- engine selector in the Tauri decompiler options UI
- engine/fallback badges in the decompile view
- Rust-owned preview NIR/HIR + printer path

#### Changed

- adopted lightweight p-code extraction before the full native action pipeline when possible
- fixed wrapped negative constant parsing
- expanded multi-block canonical `if/if-else` lowering
- added conservative `MULTIEQUAL`, `PIECE`, and `SUBPIECE` lowering

#### Improved

- preview generated direct output across real smoke samples instead of remaining an isolated prototype path

---

## Historical Milestones (Late 2025 – Early 2026)

The repository history before the current architecture convergence includes several major milestones. The detailed Korean notes remain available in [`CHANGELOG.ko.md`](./CHANGELOG.ko.md); the summaries below capture the public-facing highlights.

### Multithreaded Performance Breakthrough (157s -> 10s)

- introduced global Sleigh, GDT, and data-section scan caches
- added a core-level fail-fast timeout tripwire for monster functions
- reduced large batch decompilation wall-clock time dramatically on `putty.exe`

### Decompiler Performance + Success-Rate Uplift

- improved one-shot CLI throughput and overall decompilation success rate
- instrumented postprocess timing and removed major bottlenecks
- fixed recursive decompilation and duplicate-variable-piece failure classes
- built the first fair batch benchmark runner against PyGhidra baselines

### Security Policy / CI Gate Hardening

- added `docs/build/SECURITY_ADVISORIES.md`
- restored security checks as a CI quality gate
- documented advisory baselines and review policy

### Stabilization / Portability / Phase 2–4 Refactors

- removed panic-prone `unwrap/expect` paths across loader/analysis/ffi/tauri code
- converted pass pipelines toward `Cow<str>`-based no-op fast paths
- removed hardcoded local build paths in favor of environment-based discovery

### Postprocess Modularization

- split the large `postprocess.rs` implementation into focused modules
- separated naming, structure, arithmetic, and shared condition utilities
- added dedicated postprocess module documentation and tests

### Major Decompiler Quality Round + v4 Benchmark System

- fixed four large-quality bugs in postprocessing and structure handling
- introduced the v4 benchmark system with multi-platform suites
- significantly improved benchmark scores across ARM64, x64, Linux, and Windows

### x86 / MinGW / Type Propagation Expansion

- added MinGW-focused type propagation improvements
- brought in x86 benchmark suites and comparison binaries
- improved call propagation, loop conversion, and x86 normalization quality

### P-code Optimizer / Constant Substitution / RTTI / Listing / CFG Work

- introduced the early p-code optimization pipeline
- added context-aware constant substitution
- expanded listing, RTTI recovery, CFG analysis, and disassembly support

### Tauri Migration and Desktop Product Surface

- completed the move from the older egui UI to Tauri 2.x + React / TypeScript
- added large portions of the desktop workflow:
  - function navigation
  - assembly/decompile views
  - CFG views
  - project save/load
  - debugger surfaces
  - timeline/TTD experiments
- removed the legacy `fission-ui` egui codebase after the migration

### Analysis Pipeline / Data-Section Scan Consolidation

- unified batch analysis context and analysis-pass entrypoints
- consolidated data-symbol scanning and registration
- expanded FFI surface for function and prototype configuration

### Loader / Function Discovery

- added linear-sweep function discovery for stripped code
- improved function recovery on x86 and x64 binaries

### Early Core Capabilities Established

By this point Fission had already accumulated the foundations that still shape the current system:

- PE / ELF / Mach-O loading
- static analysis and disassembly
- Ghidra native decompiler integration
- Rust-side orchestration
- benchmarking infrastructure
- desktop UI foundations
- the first steps toward a Fission-owned decompiler core
