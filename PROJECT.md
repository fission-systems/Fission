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
- **RegisterNamer per-call reconstruction fixed 2026-07-19** (commit
  `28cdfdad`): unrelated to the migration, found while investigating dev-loop
  throughput. `PreviewBuilder::register_namer()` rebuilt a fresh
  `RegisterNamer` (cloning `options.sla_register_map`, a
  `HashMap<(u64,u32), String>`) on every call across 46 hot-path call sites
  in varnode lowering. `options` is immutable for the builder's lifetime, so
  cached the result in a `OnceCell`. Worst measured case
  (`bounded_tlv_sum` in `semantic_stress_gcc_O3.exe`, SCC size 31 /
  irreducible control flow): 63.8s → 3.4s decomp time (~18x), almost
  entirely from `structuring_duration_ms` (60.0s → 1.9s).
- **Follow-up opened by the above: `SESE_REGION_PROOF_BUDGET_MS` is
  wall-clock, not deterministic** (`fission-midend-structuring/src/
  linear_recovery.rs`, `= 500.0`, checked via `Instant::now()` in
  `sese_region_proof_budget_exceeded`). The RegisterNamer fix sped up each
  candidate-region proof attempt enough that more attempts complete inside
  the same 500ms window, which changes *which* regions get promoted —
  confirmed via `preview_build_stats` (`promoted_region_count`,
  `region_proof_candidate_count` differ before/after) and confirmed NOT a
  reintroduction of hash-order nondeterminism (both before- and after-fix
  binaries are internally stable across repeat runs; they just disagree
  with each other). Net effect: decompiled output for functions that brush
  this budget depends on how fast the structuring pass happens to run on
  the day — different hardware, machine load, or any future perf change can
  silently shift output. This is the same category of concern as the
  hash-iteration determinism fix above, just triggered by wall-clock timing
  instead of hash seeding.
- **SESE_REGION_PROOF_BUDGET_MS fixed 2026-07-19** (commit `7460ffea`):
  replaced with `SESE_REGION_PROOF_BUDGET_CALLS: u64 = 20_000`, a count of
  `sese_region_proof_budget_exceeded()` calls since the structuring attempt
  began (`PreviewBuilder::sese_region_proof_calls: Cell<u64>`, reset
  alongside `structuring_start` in `CollapseDriver::run`). Trait method
  signature unchanged (`fn sese_region_proof_budget_exceeded(&self) ->
  bool`), only its implementation — no call-site changes needed at any of
  the existing check points in `sese_driver.rs`/`linear_recovery.rs`. Added
  one new trait method, `reset_sese_region_proof_budget()`. Validated:
  1312/1312 tests, `golden_corpus_check.py` clean on both `release` and
  `quick-release` builds (160 functions), `state_machine_score` 20/20
  uniform, release/quick-release byte-identical. One corpus function
  (`rc4_init`) changed output (all 10 candidate regions now complete
  instead of an early wall-clock bailout, recovering a `do/while` where a
  bare `for(;;)` fallback rendered before) — golden snapshot updated.
  At the time, deliberately left `IfLoweringBudget`'s 10ms-per-instance /
  5000ms-total checks (`linear_types.rs`) and the inline 5000ms checks in
  `loops.rs` (`try_lower_while`, `try_lower_multiblock_dowhile`,
  `lower_loop_body_subgraph`) untouched as a "same category, lower
  priority" follow-up — see below, this stopped being theoretical within
  the same day.
- **IfLoweringBudget / loops.rs wall-clock checks fixed 2026-07-19**
  (commit `6dad16cc`): the follow-up above turned out to matter in
  practice almost immediately — `golden_corpus_check.py`'s determinism
  sub-check caught `bounded_tlv_sum` producing 2 distinct outputs across
  5 repeat runs, intermittently (stable across 40 back-to-back runs in
  isolation, but flipped once right after a heavy 160-function corpus
  sweep — a load-dependent heisenbug, not a hash-iteration one).
  Replaced with `STRUCTURING_TOTAL_WORK_BUDGET: u64 = 200_000`, a count
  of checkpoint calls since `CollapseDriver::run` began, shared via
  `Rc<Cell<u64>>` (`StructuringHost::structuring_total_work_counter()`
  — plain `Cell<u64>` doesn't work here since `IfLoweringBudget::
  checkpoint()` has no `&host` reference and needs its own live handle
  onto the same counter `loops.rs`'s direct checks touch). The per-
  instance 10ms wall-clock check is removed outright, not replaced —
  it was already OR'd with a deterministic `subcalls >
  CONDITION_RECOVERY_SUBCALL_LIMIT` check, but the OR meant whichever
  fired first (timing-dependent or deterministic) decided the actual
  trip point; now `subcalls` alone decides it. Validated: 1357/1357
  tests, 3x back-to-back `golden_corpus_check.py` runs (160 functions +
  8-repeat determinism checks each) all clean on release and
  quick-release, 40/40 uniform on `bounded_tlv_sum` specifically, 6-
  function hand-curated set untouched, release/quick-release byte-
  identical. All three `structuring_start`-adjacent wall-clock budgets
  flagged during the RegisterNamer perf investigation are now
  deterministic.
- **`--all` batch decompile ignored `--timeout-ms` entirely, fixed
  2026-07-19** (commit `0808b8a3`): found while sweeping the corpus for
  perf outliers — `decomp --all --limit 40 --timeout-ms 3000` on
  `semantic_stress_clang_O0.exe` hung 70+ minutes on one function
  (`state_machine_score`). `run_worker_fanout_fanin` (the `--all` worker
  pool, used whenever more than one function is selected) called
  `render_one_function_inner` directly with no timeout wrapper; the only
  existing enforcement (`render_one_function_on_large_stack`'s
  `recv_timeout`) was used solely by the single-function (`--addr`) path.
  A stuck function permanently occupied a worker-pool slot — fatal when
  `resolve_worker_count` returns 1 (common for small function counts),
  since the sole worker never reaches the rest of the queue. Fixed by
  routing batch tasks through `render_one_function_on_large_stack` too.
  **Not a real fix for the underlying hang** — Rust has no thread-abort
  primitive, so the stuck function's own thread is still abandoned
  running in the background for the process's lifetime; this only stops
  it from blocking the *queue*. The deeper issue: `timeout_ms` is
  threaded through five call layers (`render_one_function_inner` →
  `render_with_rust_sleigh` → `select_nir_output_from_prebuilt_pcode_
  with_facts` → `select_nir_output_from_pcode_with_facts` →
  `render_selection_from_pcode` → `render_nir_from_pcode_with_decomp_
  context`) only to land on a parameter literally named `_timeout_ms`
  in `rendering/render.rs` — explicitly unused. There is currently no
  cooperative-cancellation checkpoint anywhere in the structuring/
  rendering pipeline that consults the user's requested timeout; at the
  time this was written the `IfLoweringBudget`/`loops.rs` checks were
  still fixed wall-clock constants (10ms/5000ms) unrelated to
  `--timeout-ms` — since converted to `STRUCTURING_TOTAL_WORK_BUDGET`
  (see above), but still not wired to the user's requested budget, just
  a fixed internal ceiling. Real fix would be wiring a shared deadline/
  cancellation token through those existing checkpoints — tracked as
  further follow-up, out of scope for this pass.
- **ELF-format nondeterminism found and FIXED 2026-07-19** (found while
  validating the `discover_all_entry_specs()` caching perf fix, commit
  `57a1ce3e`; root-caused and fixed in commit `80c3c550`).

  **Repro**: `control_flow_gcc-elf_x64_O0`'s `main` (`0x401269`) and
  `control_flow_gcc-aarch64_O0`'s `__dcigettext` (`0x401dd0`) flipped
  between distinct outputs across repeated runs (~50/50 for x64, 3+
  variants for aarch64). PE binaries were unaffected — this was never
  about x86 vs aarch64, it was ELF vs PE, and even that turned out to
  be a corpus-sampling artifact (see root cause).

  **Root cause**: `fission-midend-normalize/src/recovery/
  variable_merge.rs::collect_direct_copies` returned
  `std::collections::HashSet<(String, String)>` — fully-qualified,
  bypassing this crate's `FxBuildHasher` type alias entirely. The
  earlier crate sweep (commit `6fadc75e`) only converted `use
  std::collections::HashMap;`-style *imports*; it never caught explicit
  `std::collections::HashSet`/`HashMap` qualification, which turned out
  to be scattered across 13 files. `transitive_copy_aliases()` iterates
  that set to drive a union-find merge (`for (a, b) in eligible_copies`),
  and `name_priority()` returns the same tier (`1`) for *every*
  uVar/iVar/xVar/bVar/temp-prefixed name with no secondary tiebreak —
  so for two same-tier temps that get merged, which one survives as the
  displayed name depended on the order those pairs were encountered,
  which depended on std's per-process-random `RandomState` iteration
  order. Nothing about this is ELF-specific; it just happened that none
  of the 160 PE golden-corpus functions triggered a same-tier merge tie,
  while these two ELF test functions did.

  **How it was found**: a new `FISSION_NORM_TRACE=1` diagnostic (env-
  gated, hooks both `action_pipeline::run_pass_logged` and the legacy
  free-function driver's `run_pass_logged` in
  `fission-midend-normalize::pipeline::run`) hashes `(body, locals,
  params, return_type)` after every normalize pass and logs `pass=...
  hash=...`. Bisecting a matched good/bad trace pair showed the hash
  sequence identical for the first 66 passes, diverging exactly at the
  first `variable_merge` call — pointing straight at the file. (Dead
  end worth recording: the first version of this hash covered the
  *whole* `HirFunction` including `callee_observed_max_arity`/
  `callee_summaries`, which produced a different hash on literally every
  run regardless of final output equality — some field in that tree has
  independently-unstable `Debug` output. Narrowing the hash to just the
  fields that actually determine rendered text fixed the false-positive
  noise and made the real signal visible.) A separate `FISSION_TEMP_TRACE=1`
  diagnostic (`fission-pcode/src/midend/builder/mod.rs`, logs every
  `next_unused_temp_binding_name` call) had already proven temp-name
  *allocation* itself was byte-identical between good/bad runs, which is
  what motivated looking downstream at consumption/coalescing instead of
  allocation. Both trace tools are left in the codebase, env-gated, in
  case this class of bug resurfaces elsewhere.

  **Ruled out along the way, with evidence** (real dead ends, not the
  cause): a `rayon::join` race between the concurrent Go/Apple/DWARF/
  Rust/C++ analyzers in `fission-loader/src/loader/mod.rs` (forced
  sequential execution as a diagnostic; flakiness unchanged, not a
  race); DWARF cyclic type-reference resolution order in
  `fission-loader/src/loader/dwarf/types.rs::collect_type_names` (fixed
  anyway in commit `30dd7a01` since it's a real latent bug, but fixing
  it alone didn't change this repro's flakiness); the C++ RTTI analyzer
  iterating `binary.iat_symbols`/`global_symbols` unsorted (real bug for
  binaries *with* C++ RTTI symbols, but the repro binaries are pure C
  with none, so that path returns empty deterministically regardless);
  `fission-sleigh`'s frontend-selection path (clean `Vec`/`BTreeSet`
  iteration only).

  **Validated**: 1742/1742 tests, `golden_corpus_check.py` clean (160
  functions + 10-repeat determinism), 6-function hand-curated set
  untouched, `state_machine_score` 20/20 uniform, release/quick-release
  byte-identical. The actual repros: `control_flow_gcc-elf_x64_O0`'s
  `main` 30/30 uniform (was ~50/50), `control_flow_gcc-aarch64_O0`'s
  `__dcigettext` 30/30 uniform (was 3+ distinct outputs across 5 runs).
- **Perf sweep round 3, 2026-07-19/20** (corpus-wide `--all` batch sweep
  across all 76 dev-corpus binaries, using `sample` on aarch64/x64 to
  profile the worst outliers each round):
  1. **`RuntimeFrontendArtifacts` deep-cloned under an exclusive `Mutex`
     on every cache access** (commit `db04f32e`) — the single biggest
     win of this round. `control_flow_gcc-aarch64_O0/O2` dominated the
     sweep (16 of the top 25 slowest functions, every one from those two
     binaries); `sample` on the actual `--all` batch (not a lone
     single-function run, which didn't reproduce it) showed 35% of one
     worker's time in `_pthread_mutex_firstfit_lock_wait` inside
     `RuntimeSleighFrontend::from_entry`, plus heavy time cloning the
     whole compiled SLEIGH constructor/subtable/pattern-node graph.
     Wrapped `RuntimeFrontendArtifacts.compiled`/`RuntimeSleighFrontend.
     compiled` in `Arc<CompiledFrontend>` so a cache hit is a refcount
     bump instead of a deep clone. Corpus-wide effect: mean decomp_sec
     0.60s → 0.18s, median 0.45s → 0.06s (all 2990 functions in the
     dev corpus). Almost every aarch64 function dropped from 2-6s to
     0.6-0.7s.
  2. **`RegisterNamer::hw_name_at`'s SLA-map fallback did a full linear
     scan on every call** (commit `d8acaee5`) — found profiling
     `accumulate_pairs` (8.3s for 628 bytes, disproportionate vs peers).
     Added `sla_map_by_offset: HashMap<u64, Vec<(u32, String)>>`, built
     once alongside `sla_map`, so the "any size ≥ prefer_size at this
     offset" fallback is an O(1) lookup + small scan instead of O(map
     size). 8.3s → 6.1s, output byte-identical. Remaining cost there is
     genuine SESE-region-search recursion depth (irreducible control
     flow) — a harder, separate problem, not addressed.
  3. Two outliers left unaddressed as of this round, both look like
     genuine per-function cost rather than a bug: `_nl_load_domain`
     (`control_flow_gcc-aarch64_*`, ~10.4s, but it's a real 5332-byte
     function — profiled and the cost is spread across many legitimate
     call sites, no single dominant site) and `bounded_tlv_sum`
     (`semantic_stress_gcc_O3.exe`, down to ~3.2s from the original
     63.8s bug fixed earlier this session).
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
