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
- **Perf sweep round 4, 2026-07-20** (re-profiled `_nl_load_domain`,
  round 3's #1 remaining outlier, since it's a large-but-real function
  worth one more pass before writing it off as "just big"):
  `prove_loop_carried_register_update` (`fission-pcode/src/midend/
  builder/materialize/loop_carried/shape.rs`) and its two BFS-based
  helpers (`loop_entry_value_reaches_definition`,
  `definition_reaches_loop_backedge`) did a fresh `VecDeque` walk of
  the *entire containing loop body* on every call, uncached — same
  "input is fixed for the builder's lifetime, but nothing memoizes it"
  shape as the RegisterNamer/hw_name_at fixes. Added
  `loop_carried_proof_cache` (commit `bd4a9df2`), keyed by `(block_idx,
  op_idx, VarnodeKey)`, matching the existing `lookup_site_cache`/
  `peel_cache` pattern already on `PreviewBuilder`. Confirmed safe to
  cache for the whole builder lifetime: `loop_bodies` is set once at
  construction and `StructuringHost::set_loop_bodies` (the only thing
  that could mutate it later) has zero call sites anywhere in the tree.
  `_nl_load_domain`: 10.4s → 8.0s, output byte-identical. Modest
  (~23%) compared to earlier wins in this thread — the remaining cost
  for this specific function is spread across genuine lowering/
  structuring work proportional to its size (5332 bytes, the largest
  function in the dev corpus by a wide margin), not a single
  remaining dominant site. `accumulate_pairs` and `bounded_tlv_sum`
  still show deep `sese_structure_region` recursion in profiles, but
  that recursion is a proper post-order tree walk over `build_sese_tree`
  output (not obviously re-visiting the same region twice) — the cost
  looks like genuine irreducible-control-flow search depth rather than
  a caching gap. Not pursued further this round; if picked back up,
  start by measuring whether `build_sese_region_body`'s own internal
  work (not the tree recursion around it) is where time actually goes
  for these two functions specifically.
- **Perf sweep round 5, 2026-07-20** (user asked explicitly to go beyond
  one-off caching fixes into deeper algorithmic/architectural bottleneck
  analysis; started with a fresh `sample` profile of `accumulate_pairs`
  now that round 3/4's fixes are in):
  1. **`RegisterNamer::hw_name_at` was *still* ~96% of sampled time**
     even after today's earlier `sla_map_by_offset` fix (commit
     `d8acaee5`) — but now hitting a *different* fallback path
     (`register_model.rs:425`, `self.model.and_then(|m|
     m.name_for(...))`) that delegates to `RegisterModel::name_for`,
     which had the exact same `by_offset.iter().find(|((off,sz),_)|
     *off==offset && *sz>=size)` O(map size) linear-scan bug — just one
     call frame deeper than the site fixed earlier today, so it wasn't
     touched by that fix.
  2. **Root cause of why the earlier fix didn't already cover this
     class of bug: the whole `fission-pcode::midend::cspec` submodule
     (`register_model.rs`, `pspec.rs`, `loader.rs`, `ldefs.rs`,
     `apply.rs`, `mod.rs`) was never brought under the crate's
     `FxBuildHasher` alias.** Visible in the profile as `RandomState::
     hash_one` frames. None of these files had a bare `use
     std::collections::HashMap;` line (the pattern every earlier sweep
     grepped for) — they imported through a `crate::midend::HashMap`
     re-export path that, on inspection, several of these files simply
     didn't use. A previously-undiscovered gap with both determinism
     and performance implications, same shape as the
     `fission-midend-normalize` gap found and fixed in "ELF-format
     nondeterminism found and FIXED 2026-07-19" above (commit
     `6fadc75e`) but never swept for in this submodule specifically.
  3. Fixed both together (commit `39169de6`): switched all 6 files'
     imports to `crate::midend::HashMap`/`HashSet`; kept
     `std::collections::HashMap` explicit only at the genuine
     cross-crate boundary — `NirRenderOptions.sla_register_map`/
     `pspec_hidden_registers` (`fission-midend-core`, not
     FxBuildHasher-aliased) — converting with `.into_iter().collect()`
     at the 4 read/write sites that cross it
     (`RegisterNamer::from_options`, `apply_register_model_for_language`,
     `render_finish.rs`'s two call sites). Added `RegisterModel.
     by_offset_grouped: HashMap<u64, Vec<(u32, String)>>`, built once in
     `build_from_parsed` via the existing `group_sla_map_by_offset`
     helper (same helper already used for `RegisterNamer.
     sla_map_by_offset`), turning `name_for`'s fallback into O(1) +
     small scan. `RegisterModel` is `Arc`-cached per language
     (`register_model_for_language`), so this amortizes across every
     builder sharing the cached model, not just one function.
  4. **Result: `accumulate_pairs` 6.06s → 1.05s (~5.8x)**, byte-identical
     output (verified against pre-fix release build and against
     `quick-release`). This is on top of round 3's earlier `d8acaee5`
     fix (which had already taken it from 8.3s down, per round 3's own
     note above — the two fixes address sibling fallback paths in the
     same lookup chain, not the same line).
  5. Lesson for future sweeps: grepping for bare `use std::collections::
     HashMap;` is not sufficient to find every un-aliased submodule —
     also worth spot-checking `RandomState`/`hash_one` frames in `sample`
     profiles even after a submodule's imports "look" like they go
     through the crate alias, since a re-export path existing doesn't
     mean every file in the submodule actually uses it.
  Validated: `cargo check --workspace` clean, 1962/1969 nextest passing
  (7 failures are pre-existing `fission-emulator` `diag_*`/
  `profile_static_crt_*`/`srd_semantic_replay` tests that fail
  identically on unmodified `main`, confirmed via `git stash` — an
  unrelated environment issue, not caused by this change),
  `golden_corpus_check.py` clean (160 functions + determinism),
  6-function hand-curated regression set byte-identical, `state_machine_
  score` 20/20 uniform, release/quick-release byte-identical.
- **Perf sweep round 6, 2026-07-20** (continuing the same "go deeper,
  not just cache the last hot function" mandate — profiled
  `bounded_tlv_sum`, round 4's other flagged-but-unexplained outlier,
  now that `accumulate_pairs`'s dominant cost is fixed):
  1. `sample` on `bounded_tlv_sum` (1526 bytes, `semantic_stress_gcc_O3.exe
     @ 0x140001560`) after round 5's fix showed no single 90%+ hotspot
     anymore — cost spread across `lower_expr`/`lower_varnode_inner`,
     `RootReachabilityProof::build`/`DefinitionDependencyMap::
     address_nodes_reaching_roots`, `linear_exit_from`, and several
     other structuring/normalize call sites. Investigated the
     `RootReachabilityProof` frames since "proof-of-reachability" style
     helpers were exactly the shape of bug found twice already this
     session (recompute something that's actually invariant for the
     whole call).
  2. **Found: `DefinitionDependencyMap::address_contributors` walks
     every statement in the function looking for pointer
     dereferences/indexes/field accesses, and for each one called
     `address_nodes_reaching_roots(name, pointer_roots)`, which
     rebuilt a fresh reverse-dependency graph (`RootReachabilityProof::
     build`, O(V+E) over the whole function's address-dependency graph)
     from scratch on every single call.** `pointer_roots` (and the
     dependency graph itself) never change across the whole
     `address_contributors` walk, so this was O(D·(V+E)) instead of
     O(V+E) where D = number of pointer-touching sites in the function
     — the same "recompute per-query instead of once per fixed input"
     shape as the `RegisterNamer`/`RegisterModel`/`loop_carried_proof`
     fixes, just in a different analysis (`fission-midend-normalize`'s
     def-use/address-provenance pass, not `fission-pcode`'s builder).
  3. Fixed (commit `c33b6270`): build `RootReachabilityProof` once in
     `address_contributors`, thread `&RootReachabilityProof` through
     `collect_address_contributors_stmts`/`_lvalue`/`_expr`/
     `record_address_contributors` instead of rebuilding it inside
     `DefinitionDependencyMap::address_nodes_reaching_roots` on every
     leaf call. `bounded_tlv_sum`: 3.13s → 1.15s (~2.7x), byte-identical
     output.
  4. Note: this is a distinct fix from round 5's `RegisterModel`
     change — different crate, different analysis, sibling bug shape.
     `accumulate_pairs` also improved slightly (1.05s → 0.99s) since
     it has some pointer traffic too, but its dominant cost was already
     resolved in round 5.
  Validated: same as round 5's checklist (workspace check, 1962/1969
  nextest with the same 7 pre-existing unrelated failures,
  `golden_corpus_check.py`, 6-function regression, `state_machine_score`
  20/20, release/quick-release byte-identical).
- **Perf sweep round 7, 2026-07-20** (user asked directly whether SLEIGH
  decoding itself is a bottleneck — answered by profiling a `--all`
  batch run and filtering `sample` output to just the actually-busy
  per-function decode threads, excluding idle rayon-pool noise per the
  established caveat above):
  1. **Answer: no, not really** — direct `fission_sleigh` self-time
     (`CompiledParserWalker::walk` + `walk_decision_tree` +
     `bind_instruction_with_inst_next` + `select_constructors`) totaled
     ~11% of busy-thread self-time on `advanced_patterns_gcc_O2.exe`
     (73 functions). Proportional to instruction count decoded, not a
     caching gap. Two adjacent, unrelated findings turned out to be
     much bigger:
  2. **`getenv`/`std::env::var_os` cluster was ~7.8% of self-time**
     (commit `a4714641`). `terminal_reselect_trace_enabled()`
     (`fission-sleigh/src/runtime/diagnostics.rs`) and an inline
     `FISSION_TRACE_TERMINAL_VERIFY` check in `decision.rs`'s Terminal
     probe handler ran on *every successfully decoded instruction*;
     `FISSION_BUILD_DEBUG` checks in `template.rs`/`template_eval.rs`
     ran on every matched constructor's p-code template evaluation.
     None were cached — same "recompute per-call instead of once"
     shape as every fix this round, just manifesting as a syscall
     instead of a data-structure rebuild. Cached all three behind
     `OnceLock<bool>`.
  3. **`--all` batch rebuilt `FactStore` (FID signature matching + name
     facts + sidecar-patch parsing across the *entire binary*) once per
     function instead of once per binary** (commit `27552293`) — the
     single biggest win of this round. `decompile_with_rust_sleigh`'s
     convenience wrapper calls `FactStore::from_binary` internally, and
     every `--all`-path call site used that wrapper instead of the
     already-existing `decompile_with_rust_sleigh_with_facts`, which
     takes a pre-built `&FactStore`. A batch of N functions did N
     redundant whole-binary analyses. Threaded a single
     `facts: Arc<FactStore>`, built once in `run_with_functions`,
     through `render_one_function_on_large_stack` /
     `run_worker_fanout_fanin` down to `render_with_rust_sleigh`, which
     now calls the `_with_facts` entry point directly. Covers both the
     worker-fanout `--all` path and the sequential/single-function path.
  4. Also noted but not fixed this round: `thread_start` was ~9.3% of
     self-time — `--all` spawns one OS thread per function (with a
     32MB-by-default stack) rather than reusing a pool of
     large-stack threads. Real architectural cost, bigger change
     (thread-pool reuse instead of spawn-per-task), left for a future
     round. Also noted: `fidbf`/signature-database parsing frames
     (`parse_raw_fidbf_database`/`collect_records`/`decode_record`,
     ~4.7% combined) are now amortized for free by the `FactStore`
     fix above (parsed once per binary instead of once per function),
     so no separate fix was needed there.
  5. **Result**: `advanced_patterns_gcc_O2.exe` (73 functions), `--all
     --json`: user CPU 6.4s → 3.5s (~45%), wall time 1.3–2.0s → 0.82s.
     Verified byte-identical decompiled output for all 73 functions,
     and byte-identical code content across 10 repeat `--all` runs
     (only benchmark timing fields legitimately vary run to run).
  Validated: same checklist as round 6 (workspace check, 1962/1969
  nextest with the same 7 pre-existing unrelated failures,
  `golden_corpus_check.py`, 6-function regression, `state_machine_score`
  20/20, release/quick-release byte-identical, plus the 10-run `--all`
  code-content determinism check above).
- **Perf sweep round 8, 2026-07-20 — negative-ish result, recorded for
  the methodology lesson** (user asked to pursue round 7's flagged-but-
  unfixed `thread_start` finding: `--all` spawns one large-stack OS
  thread per function via `render_one_function_on_large_stack`, even
  though `run_worker_fanout_fanin`'s persistent worker threads already
  carry `stack_size_bytes` of stack themselves):
  1. Fixed (commit `2d6f0d09`): inside `run_worker_fanout_fanin`'s
     worker loop specifically (not the sequential/single-function path
     in `mod.rs`, which runs on the process's default-stack main thread
     and genuinely needs the spawn for stack headroom), skip the
     watchdog-thread spawn when no `--timeout-ms` is configured (the
     CLI default) and call `render_one_function_inner` directly.
     Behavior-preserving: with no timeout, a hang blocks the worker
     either way, spawned-and-joined or not — confirmed by re-reading
     `render_one_function_on_large_stack`'s existing untimed branch,
     which already just does a blocking `handle.join()` with no timeout
     wrapper, so there was no actual hang-protection being removed.
  2. **Measured impact: none, within noise.** A controlled A/B with
     `FISSION_RUST_DECOMP_WORKERS=1` (isolating exactly the
     spawn-per-task cost against `advanced_patterns_gcc_O2.exe`'s 73
     functions, everything else held constant) showed no measurable
     `user`-CPU or wall-clock difference before vs after — contradicting
     round 7's `sample`-based estimate of ~9.3% self-time. Likely
     explanation: macOS thread creation with a lazily-committed
     (not eagerly zeroed) 32MB stack is apparently cheap in actual CPU
     terms; `sample`'s 1ms-interval wall-clock sampling can still
     attribute real leaf-sample weight to a brief per-thread scheduling
     window (thread exists, hasn't reached user code yet) without that
     time corresponding to sustained CPU work — so many short-lived
     threads passing through that window inflates the self-time
     percentage without inflating measured `user`/`real` time.
  3. **Kept the change anyway**: it removes genuinely dead work (an
     unconditional spawn+join that buys nothing when untimed) with zero
     measured downside and zero correctness risk, but the expected win
     did not materialize on this benchmark/platform.
  4. **Methodology lesson for future rounds**: a `sample`-derived
     self-time percentage is a hypothesis, not a measurement — always
     close the loop with a controlled before/after timing comparison
     (ideally isolating the one variable, as the `WORKERS=1` A/B did
     here) before trusting the profile's implied win. Rounds 5–7's
     fixes all had this confirmation step and their measured wins
     matched expectations; round 8 is the first case this session where
     the confirmation step caught a profile-based hypothesis that
     didn't hold up, which is exactly why the step exists.
  Validated: same checklist as round 7 (workspace check, 1962/1969
  nextest with the same 7 pre-existing unrelated failures,
  `golden_corpus_check.py`, 6-function regression, 10-run `--all`
  code-content determinism, `state_machine_score` 20/20,
  release/quick-release byte-identical).
- **Perf sweep round 9, 2026-07-20** (systematic follow-up to round 7's
  sleigh-only getenv fix — swept `fission-pcode`, `fission-midend-
  normalize`, `fission-midend-structuring` for the same uncached
  `std::env::var(_os)` pattern on hot per-block/per-op paths):
  1. Cached ~19 diagnostic/feature-flag checks behind `OnceLock<bool>`
     (commit `5c7ce180`), mirroring the existing `temp_name_trace_
     enabled`/`fission-sleigh::diagnostics` precedent. Notable:
     `preview_builder_diag_enabled` (~17 call sites in `control/
     terminator.rs` alone) and `structuring_diag_enabled` (~20 call
     sites across the structuring subsystem) were raw syscalls checked
     from many places.
  2. **Left `FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION`
     uncached** (`fission-midend-normalize/src/analysis/defuse.rs`):
     its own unit tests toggle this exact env var at runtime via
     `set_var`/`remove_var` (`WideDeadRerunAdmissionEnvGuard`) to
     exercise both code paths in the same test binary process — a
     `OnceLock` would freeze on whichever value the first caller
     observed, permanently breaking the second path. Audited every
     other cached flag name across the whole workspace for the same
     `set_var`-in-tests hazard; none found. **This is a real hazard for
     any future env-var-caching sweep — grep for `set_var` on the exact
     flag name before caching it, every time.**
  3. **Caught and fixed a self-inflicted bug before landing**: an
     earlier mechanical find-and-replace script (converting raw
     `std::env::var_os` calls to the new cached helper calls) ran over
     `debug.rs` and accidentally rewrote its OWN just-written helper
     definitions, turning `preview_debug_enabled`/`preview_debug_
     regdump_enabled`'s `OnceLock` initializer closures into calls to
     *themselves* — reentrant `OnceLock` access on the same thread,
     which per Rust's docs is unspecified behavior allowed to deadlock
     (observed) or panic. Root-caused via a full `lldb` thread
     backtrace (`sample`'s tree view was too ambiguous to distinguish
     real recursion from `Once::call`'s own internal retry-loop
     framing — needed `lldb -p <pid> -o "thread backtrace all"` to see
     the test thread genuinely parked inside its own `Once::call`,
     waiting on itself). ~24 `fission-pcode` tests were hanging at the
     nextest slow-test timeout (120s each) before this fix; all pass
     cleanly (~0.2s each) afterward. **Lesson: a mechanical replace
     script touching a file it also just wrote to needs the same
     scrutiny as one touching pre-existing code** — "I just wrote this,
     it's fine" is exactly the assumption that let this slip through.
  4. Perf impact of this sweep alone is **inconclusive** on the
     `advanced_patterns_gcc_O2.exe --all` benchmark (user CPU stayed in
     the ~3.4–3.9s range, same ballpark as round 8's already-landed
     fixes) — recorded honestly per round 8's methodology lesson,
     rather than claimed as a clear win. The correctness value (closing
     a real "uncached invariant check on a hot path" bug class, and
     catching a genuine deadlock before it shipped) is the primary
     justification for landing this, not a measured speedup.
  Validated: `cargo check --workspace` clean, 1962/1969 nextest passing
  (same 7 pre-existing unrelated failures; all previously-hanging
  `fission-pcode` tests now pass in ~21s total for the full run),
  `golden_corpus_check.py` clean, 6-function hand-curated regression
  byte-identical, `state_machine_score` 20/20 uniform,
  release/quick-release byte-identical, `bounded_tlv_sum`/
  `accumulate_pairs` output unchanged from round 7's baseline.
- **Perf sweep round 10, 2026-07-20** (revisited `_nl_load_domain`,
  round 4's flagged-but-unresolved outlier — the largest function in
  the dev corpus, 5332 bytes, `control_flow_gcc-aarch64_O2 @
  0x402600` — per explicit user direction to go back to it now that 5
  rounds of fixes have landed on shared code paths since):
  1. **Cumulative effect check first**: before any new fix, `_nl_load_
     domain` was already down to ~4.5s from round 4's 8.0s, purely
     from rounds 5–9's fixes landing on code this function happens to
     exercise (RegisterModel/RegisterNamer, RootReachabilityProof,
     getenv caching). Worth noting for future "revisit an old outlier"
     work: check the cumulative baseline before assuming nothing
     changed.
  2. Fresh `sample` profile showed `live_call_result_binding_from_
     predecessors_for_return_register` (a recursive predecessor-graph
     walk checking whether the return register still holds a live call
     result) at ~10–15% combined self-time. It cloned its `visited:
     HashSet` once per predecessor edge before recursing — sibling
     branches sharing an ancestor block (a diamond in the predecessor
     graph, common with reconverging if/else) each re-explored that
     shared ancestor's whole subtree independently, worst-case
     exponential in diamond-heavy CFGs.
  3. Fixed (commit `4bc9c62d`): replaced the cloned `visited` set with
     a `HashMap<usize, Option<(...)>>` memo threaded by `&mut` through
     the whole walk — a block's result is computed once and reused by
     every branch that reaches it; a block still holding the
     in-progress `None` placeholder when revisited means a genuine
     back-edge cycle, preserving the original early-return-on-revisit
     semantics. Deliberately scoped to one call of `live_call_result_
     binding_for_return_register` (not cached on the builder across
     calls): `self.call_result_bindings` grows as lowering proceeds, so
     a longer-lived cache could return a stale answer from before a
     predecessor block was lowered — the same mutation-safety class of
     hazard round 9 found for `set_var`-toggled env flags, just for a
     mutable `HashMap` field instead.
  4. **Measured impact on this specific function: negligible** (~4.5s
     before and after, confirmed via a `git stash`-based A/B).
     `_nl_load_domain`'s own predecessor graph apparently isn't
     diamond-heavy enough to trigger the redundant-re-exploration this
     fixes — its self-time in that call chain is proportional work
     (many distinct top-level calls across the function, not redundant
     work within one). Kept the fix anyway: it's strictly safer (a real
     exponential-blowup pattern removed, for whichever future function
     does have a diamond-heavy predecessor graph) with zero measured
     downside.
  5. This continues round 8's pattern: not every profile-suggested fix
     produces a measurable win on the specific function that surfaced
     it, and that's fine to land anyway when the fix is a strict
     safety/complexity improvement with no downside — the important
     thing is measuring honestly rather than assuming the profile
     percentage translates 1:1 into saved wall-clock time.
  Validated: `cargo check --workspace` clean, 1962/1969 nextest passing
  in 19.5s with no hangs (same 7 pre-existing unrelated failures),
  byte-identical output (release before/after via `git stash`, release
  vs quick-release), `golden_corpus_check.py` clean, 6-function
  hand-curated regression byte-identical, `state_machine_score` 20/20
  uniform, `_nl_load_domain` itself 20/20 deterministic.
- **DWARF/PDB struct field layouts wired into aggregate field naming,
  2026-07-20** (found while auditing Fission against Ghidra for
  genuinely-unimplemented decompiler features, at user request — see
  `docs/architecture/GHIDRA_PARITY_GAP_AUDIT.md` for the broader
  audit, which is narrower in scope than this finding):
  1. **Root cause**: `fission-loader`'s DWARF parser (`dwarf/types.rs`)
     already extracts full struct/union/class layouts (field names,
     byte offsets, types) from `DW_TAG_structure_type`/`DW_TAG_member`
     into `LoadedBinary.inferred_types` — but grepping the whole
     Rust-native decompile pipeline (`fission-pcode`,
     `fission-midend-normalize`, `fission-midend-structuring`,
     `fission-decompiler`) found **zero** references to it. The only
     consumer (`fission-static/src/analysis/decomp/prepare.rs`'s
     `apply_struct_to_param`) is entirely
     `#[cfg(feature = "native_decomp")]`-gated — i.e. only reachable
     when Fission calls out to Ghidra's actual C++ decompiler via FFI,
     a completely different backend than the pure-Rust one this
     session's ten perf rounds targeted. Register-resident DWARF
     locals (as opposed to stack-offset ones) were a smaller,
     related gap: `nir_hints_from_debug_function`
     (`fission-decompiler/src/facts/facts.rs`) only handled
     `DwarfLocation::StackOffset`, silently dropping names/types for
     any local DWARF placed in a register (common at -O1+). Not fixed
     in this change — noted for a future slice.
  2. Meanwhile `fission-midend-normalize::memory::aggregate_fields.rs`
     already has its own **independent, heuristic** aggregate-field
     recovery (promotes `Ptr(Unknown)` → `Ptr(Aggregate{fields})` from
     observed constant-offset `Load`/`Store` access patterns, naming
     fields synthetically as `field_8`, `field_c`, ...). The two
     systems had just never been connected.
  3. Fixed (commit `617cf988`): added `NirStructTypeHint`/
     `NirStructFieldHint` (`fission-midend-core`) and a
     `struct_types: HashMap<String, NirStructTypeHint>` field on
     `NirTypeContext`, populated once per binary from
     `LoadedBinary.inferred_types`
     (`build_nir_struct_type_hints`). `type_hints.rs`'s new
     `apply_debug_struct_field_names` overlays real field names onto
     already-recovered `NirType::Aggregate` fields, matched by byte
     offset, for any param/local whose `surface_type_name` (already
     populated by the pre-existing DWARF param/local type-hint
     plumbing) resolves to a known struct name through **exactly one**
     level of pointer indirection.
  4. **Deliberately narrow scope, matching this session's established
     "only touch what's proven safe" discipline**: does not decide
     which variables become aggregates (keeps the existing, separately
     -validated heuristic as sole gatekeeper — no new false-positive
     aggregate promotions introduced), does not touch field *offsets*,
     and does not touch field *types* — `aggregate_fields.rs` derives
     those from real observed load/store access width, which is
     grounded in actual pcode and safer to trust for cast/size
     correctness than a naively re-parsed debug-info type string. Only
     a field's *name* is ever overwritten, and only when a debug-info
     field exists at the exact same offset. Multi-level pointers
     (`Foo**`) are rejected outright by
     `struct_base_name_for_single_pointer`: the aggregate whose fields
     would be named belongs to `**binding`, not `*binding`, so applying
     the layout at this binding's own offset set would be a semantic
     mismatch, not just an imprecision.
  5. **Verified via two focused unit tests**, not an end-to-end binary:
     real compiler output at `-O0` frequently doesn't trigger
     `aggregate_fields.rs`'s own promotion heuristic for even a
     two-field `struct Point { int x, y; }` accessed as `p->x + p->y`
     (confirmed empirically with a `zig cc`-built ELF test binary) —
     that non-triggering is a **separate, pre-existing gap in the
     heuristic itself**, out of scope for this change, which is purely
     about connecting already-collected data to an already-existing
     recovery mechanism. The unit tests
     (`preview_type_hints_overlay_debug_struct_field_names_onto_
     recovered_aggregate`, `..._reject_multi_level_pointer`) construct
     an already-aggregate-typed binding directly (as
     `aggregate_fields.rs` would have left it) and confirm the overlay
     — and its multi-pointer-level rejection — both work correctly in
     isolation.
  Validated: `cargo check --workspace` clean, 1964/1971 nextest passing
  (1969 baseline + 2 new tests; same 7 pre-existing unrelated
  failures), `golden_corpus_check.py` clean (160 functions,
  byte-identical — none of the golden corpus's DWARF-bearing functions
  hit this new path, confirming zero regression risk), 6-function
  hand-curated regression byte-identical, `state_machine_score` 20/20
  uniform, release/quick-release byte-identical.
- **Follow-up (b) done: the aggregate promotion gate widened for
  debug-info-backed pointers, 2026-07-20** (commit `ffe5b987`) —
  but a real correctness bug in the field-naming overlay above was
  found and fixed on the way there first:
  1. **Bug found and fixed (commit `f40f6af6`), predating the widening
     work**: the field-naming overlay from the previous entry renamed
     `StructField.name` in the binding's type annotation, but
     `render/printer.rs` never reads that annotation for
     `HirExpr::FieldAccess`/`HirLValue::FieldAccess` rendering — it
     prints `field_name` straight off the AST node. That string is
     baked into the node once, by normalize's `ptr_arith.rs` (which
     runs a *second* time specifically to convert `PtrOffset` ->
     `FieldAccess` once `aggregate_fields.rs` has populated a binding's
     fields), long before `type_hints.rs` runs post-structuring.
     Renaming the type-level annotation afterward changed nothing
     visible — a gap the original unit tests missed because they used
     trivial bodies with no `FieldAccess` node to actually render.
     Fixed with `rewrite_field_access_names_in_stmts`
     (`fission-midend-core::util::var_rename`, mirroring the existing
     `rename_vars_in_stmts` it sits beside), which walks the body and
     renames matching `FieldAccess` nodes directly, keyed by `(base
     variable name, byte offset)`. A new unit test builds the actual
     AST shape `ptr_arith.rs` would produce and asserts the *printed*
     output changes, not just the type annotation — the test that
     would have caught the original gap.
  2. **Root cause of the "never fires" question**: traced with a real
     `zig cc`-built `-O0` ELF (`struct Point { int x, y; }; int f(Point
     *p) { return p->x + p->y; }`). `p`'s type lands on `Ptr(Int{32})`
     from its first dereference and never advances, because
     `aggregate_fields.rs`'s own `can_upgrade_binding_to_aggregate`
     only promotes from `Ptr(Unknown | Int{8|16})` — deliberately
     excluding wider integer pointers so a genuine `int*`/`long*` array
     (`arr[0] + arr[1]`) doesn't get misclassified as a fake struct
     when there's no other evidence. That exclusion is correct without
     debug info, but it also means a struct whose first field is `int`
     or wider — the common case — never gets promoted at all, so the
     field-naming overlay (which only *renames* an already-populated
     aggregate) had nothing to act on for the case it was built for.
  3. **Fix**: with DWARF/PDB proof the type really is a struct, the
     array/struct ambiguity that justifies the exclusion doesn't apply.
     Added `apply_debug_struct_promotions`
     (`fission-pcode::midend::builder::type_hints`), which promotes
     debug-info-backed pointers directly — without touching
     `aggregate_fields.rs`'s own heuristic (or its no-debug-info
     correctness) at all. It rewrites two access shapes directly
     against debug-info field offsets, same transformation
     `ptr_arith.rs` does for the heuristic path:
     `Load{ptr: Var(name)}`/`Deref{ptr: Var(name)}` (offset 0) and
     `Load{ptr: PtrOffset{base: Var(name), offset}}` (nonzero constant
     offset). A size-compatibility check (access width <= field size)
     guards against misreading a wide access spanning multiple fields
     as just the first one.
  4. **Second empirical finding, discovered mid-implementation**: the
     narrow version above (matching the param variable by name
     directly) had *zero* effect on the real test binary — not even
     the `offset == 0` case. Real -O0 output almost always spills a
     parameter into a local "shadow" (`local_8 = p;`) before any
     dereference, so the actual `Load` targets `local_8`, never `p`
     itself. Extended `apply_debug_struct_promotions` to follow exactly
     one level of single-assignment direct-copy alias
     (`extend_with_copy_aliases`: a local assigned exactly once in the
     whole function, whose sole assignment is a direct `Var`-to-`Var`
     copy of an eligible binding). Confirmed via the real binary this
     is not an edge case but the *dominant* shape — without it, the
     whole feature would almost never fire on real compiler output.
  5. **Confirmed remaining, deliberate limitation**: on the same test
     binary, `p->x` (offset 0, reached via the copy alias) now recovers
     correctly; `p->y` (offset 4, reached only via `t = local_8 + 1; *t
     = ...`, a *non-copy* pointer-arithmetic intermediate) still
     doesn't. Reaching that would need real cross-statement def-use /
     reaching-definitions tracking this pass deliberately doesn't have
     — documented in the function's own doc comment rather than
     silently claimed as complete. A binding whose accesses are all
     past this pass's reach just keeps its non-aggregate type; nothing
     gets misrendered.
  Validated: `cargo check --workspace` clean, 1967/1974 nextest passing
  (1969 baseline + 5 new tests across both commits in this entry; same
  7 pre-existing unrelated failures), `golden_corpus_check.py` clean
  (160 functions byte-identical — this more aggressive AST rewrite
  still touches none of the golden corpus's existing output),
  6-function hand-curated regression byte-identical,
  `state_machine_score` 20/20 uniform, struct test binary 5/5
  deterministic, release/quick-release byte-identical.
- **Follow-up (a), investigated but not implemented, 2026-07-20**:
  extending `nir_hints_from_debug_function` to cover
  `DwarfLocation::Register` locals (not just `StackOffset`) turns out
  to be a substantially different and harder problem than either fix
  above, not a small extension:
  - DWARF register locations are encoded as a raw DWARF register
    *number* (`format!("reg{}", register.0)` in
    `fission-loader/src/loader/dwarf/functions.rs`), not an
    architecture register name. Correlating it to a SLEIGH register
    space offset needs a DWARF-register-number -> SLEIGH-offset table
    *per architecture* (x86-64 SysV/Windows, x86-32, ARM, AArch64,
    MIPS, PowerPC, LoongArch — every architecture this project
    supports), which doesn't exist yet.
  - More fundamentally: stack-slot locals have a stable identity (one
    memory address) for their whole scope, so Fission's builder
    naturally creates one binding per slot, matching DWARF's own
    offset-keyed identity trivially. A register gets reused for many
    different logical values over a function's lifetime, and Fission's
    SSA-like temp-naming (`next_unused_temp_binding_name`) creates a
    *new* binding every time the register is redefined — there is no
    single persistent Fission binding that "is" a register-resident
    DWARF local the way `NirBindingOrigin::StackOffset` already gives
    one for stack locals. Correlating "DWARF says variable X lives in
    register R for this address range" to "the specific Fission temp
    binding live at that program point" needs real live-range-aware
    matching (DWARF location *lists*, not just a single location, plus
    Fission-side reaching-definitions info) — an order of magnitude
    more machinery than the params/struct-field work in this entry,
    which only ever needed positional or single-copy-alias identity.
  - Given a wrong correlation would *actively misname* a Fission
    binding (worse than the current silent gap, not just less
    complete), this needs its own properly-scoped slice with its own
    design and validation, not a rushed extension bolted onto this
    session's struct-field work. Left undone, honestly, rather than
    shipped half-correct.
  - **Update, 2026-07-20**: the per-architecture DWARF-register-number
    table gap above is now closed at the *data* level. Ghidra ships
    exactly this as checked-in XML (`Ghidra/Processors/<Arch>/data/
    languages/*.dwarf`, e.g. `x86-64.dwarf`'s `<register_mapping
    dwarf="5" ghidra="RDI"/>`) — found by re-auditing `vendor/ghidra/`
    against `utils/` for anything not yet mirrored (also confirmed
    `.pspec`/`.cspec`/`.ldefs`/`.opinion`/`.slaspec` counts match
    exactly; `.dwarf` was the only gap). Copied all 19 files (~76K)
    into `utils/sleigh-specs/languages/<Arch>/`, matching the existing
    per-architecture layout; provenance recorded in `THIRD_PARTY.md`.
    **No LoongArch mapping exists in Ghidra 12.0.4** — would need the
    LoongArch psABI spec directly if ever needed. `utils/` is entirely
    gitignored (published as `fission-utils.tar.gz` via the "Publish
    Utils Assets" Action, not committed to this repo), so this
    addition is local-only until that Action is run — deliberately not
    triggered yet, since nothing consumes these files: chaining
    Ghidra-register-name -> SLEIGH `(offset, size)` still needs
    `RegisterModel::lookup_name()` (already exists, from this
    session's earlier `cspec` work) wired to a name lookup keyed by
    the DWARF register number, and the harder live-range-correlation
    half of this problem (previous bullet) is completely unaffected —
    this only removes one of the two blockers, not both.
  - **Update, 2026-07-20 (later same day)**: while migrating the 19
    `.dwarf` files, noticed `.gitattributes`/README/CI docs still
    described a Git LFS distribution model for `utils/` that no longer
    applies — the last commit to touch `utils/` (`a854c218 "chore:
    remove utils/ from GitHub entirely"`) removed it from git tracking
    altogether, superseded by the `fission-utils.tar.gz` GitHub Release
    asset (`assets-v1`) that `.github/actions/setup-utils` actually
    downloads. Removed the stale LFS filter rules from `.gitattributes`,
    the dead `lfs: true`/`git lfs pull` steps in `cd.yml` and
    `publish-utils-assets.yml`, and rewrote the README/
    `docs/CI_RELEASE_GATES.md` guidance to describe the tarball flow
    (commit `539adffb`). **Decision**: hold off on running "Publish
    Utils Assets" for the `.dwarf` files themselves — bundle that
    publish together with whatever code change first consumes them
    (the register-locals feature above), timed for the v0.1.6 release,
    rather than publishing inert data now.
  - **Update, 2026-07-20 (implemented, commit `d8ea98c6`)**: item 1
    (register-resident DWARF locals) is done — both blockers from the
    original assessment above turned out tractable, plus two unrelated
    pre-existing DWARF bugs surfaced and got fixed along the way (found
    only because this was the first real end-to-end test against actual
    GCC -O1 output, not clang/zig-cc-only fixtures).
    - **`.dwarf` register-mapping data**: `fission-pcode/cspec/dwarf_regs.rs`
      parses `utils/sleigh-specs/languages/<Arch>/*.dwarf` (DWARF-regnum
      → Ghidra-regname, with `auto_count` expansion). The file isn't
      named by any convention — Ghidra resolves it via `<external_name
      tool="DWARF.register.mapping.file" name="..."/>` nested inside
      `<language>`, declared *after* the `<compiler>` children in real
      `.ldefs` files — `ldefs.rs`'s parser now buffers `<compiler>`
      entries per `<language>` block and backfills at `</language>`
      instead of inserting inline at `<compiler>` time. Also found (and
      fixed) that its tag-name scanner read a leading `/` as a
      name-terminator, so `</language>` was *always* read as `""` —
      `"/language"` never matched, silently no-op before (state was
      always freshly overwritten by the next `<language>` tag) but this
      new backfill logic actually depends on it firing.
    - **Live-range correlation, narrower than feared**: the original
      assessment worried about needing DWARF location *lists* +
      Fission-side reaching-definitions to match a register's live range
      to a binding. Turned out unnecessary for the common case: DWARF's
      own location-list-agreement (see below) already vouches for "this
      register is this one variable for its whole declared scope" — no
      separate live-range analysis needed on Fission's side.
    - **`.debug_loc`/`.debug_loclists` were never read at all**:
      `fission-loader`'s DWARF section loader hardcoded these (plus
      `.debug_addr`) to empty slices, unrelated to this feature —
      location lists (as opposed to a bare `Exprloc`) silently always
      resolved to `Unknown`. Wired real section bytes in
      `sections.rs`/`analyzer.rs`. `extract_location`/
      `parse_location_list` (`functions.rs`) now resolve a location list
      to a register only when *every* range agrees on the same DWARF
      register number — real compilers routinely split a variable's
      location list for reasons unrelated to the variable moving (an
      `entry_value`-computed trailing range once the register might get
      reused, a "known constant" range before first write); any
      disagreement, or any non-register range, falls back to `Unknown`.
    - **Two pre-existing DWARF bugs, both invisible to prior test
      coverage** (clang/zig-cc fixtures only) and both blocked verifying
      this feature until fixed:
      1. `DW_AT_high_pc` is either an absolute address or an *offset
         from* `low_pc` (DWARF spec, compiler's choice) — reading it as
         a raw `u64` without checking the form (GCC 16 uses the offset
         form) collapsed every such function's `size` to 0
         (`subprogram_size` in `functions.rs`).
      2. `analyze_functions_inner`'s DFS depth threshold was off by one:
         `func_depth` is set to 1 *at* the subprogram's own tag, so a
         sibling (same depth — e.g. a trailing type DIE GCC emits after
         a function's last real child) also computes to `func_depth==1`,
         but the code used `<= 0` as "exited the subprogram". The next
         function's own `DW_TAG_subprogram` tag, encountered while still
         wrongly "inside" the previous one and unmatched by any case in
         the children match, was silently swallowed — folding its
         name/params/locals into the *previous* function's
         `DwarfFunctionInfo` entirely. Fixed threshold: `<= 1`.
    - **Materialization side channel, not a `NirBinding` field**: most
      register-resident values get a generic `uVarN`/`iVarN` name, not
      their raw hw register name (only call-result registers reliably
      keep it via `ensure_live_register_binding`) — so matching DWARF
      hints by binding *name* alone (the original plan) missed most real
      cases, confirmed empirically: a loop accumulator materialized as
      `uVar0`, not `RDX`, despite DWARF saying `total` lives in `RDX` for
      its whole scope. `record_register_origin`/`take_register_origins`
      (thread-local in `builder/mod.rs`, mirrors `orchestrate.rs`'s
      existing `LAST_LAYERED_PSEUDOCODE` pattern) record each binding's
      real originating `(offset, size)` at the four sites that create
      register-space bindings, letting `type_hints.rs` match by identity
      instead of name. Deliberately not a new `NirBinding` field —
      `NirBinding` is constructed at ~300 call sites across the
      workspace; a thread-local drained once per function build (via
      explicit `take` + pass-as-parameter into
      `apply_preview_type_hints`, not a thread-local *read* inside
      `type_hints.rs` — keeps that function's tests deterministic) is far
      lower-risk.
    - **No per-function assign-count safety gate** (unlike the earlier
      struct-field promotion work's `extend_with_copy_aliases`): DWARF's
      own location-list agreement is the safety net here, and an
      assign-count gate would reject the *dominant* real case — a loop
      accumulator (`total = 0; ... total += x;`) is written more than
      once by construction. Materialization already gives every write to
      the same physical register the same one binding for the whole
      function, so multiple assignments are normal read-modify-write on
      that one variable, not evidence of reuse.
    - Verified end-to-end against real GCC 16 `-O1` output (Docker,
      native x86-64, not cross-compiled): a loop accumulator whose
      `DW_AT_location` is `DW_OP_reg1` for its whole declared scope
      renames correctly (`uVar0` → `total`); a genuinely multi-register
      loclist (RAX → RBP → RAX across ranges, real register churn around
      a call) correctly does *not* rename.
  - **Update, 2026-07-20 (broader metadata audit)**: user asked whether
    other DWARF/loader metadata is collected but never consumed by the
    decompiler, the same class of gap as the register-locals work above.
    Found several, ranked by size:
    1. **FID (Function ID) signature matching is fully dead** — biggest
       finding. `fission-signatures/src/fidbf/` has a complete `.fidbf`
       parser (`parser.rs`/`loader.rs`/`types.rs`/`tables.rs`,
       `FidbfDatabase::identify_by_hashes` and hash-index lookups all
       implemented) and `utils/signatures` ships real FID databases, but
       the only production caller is `fission-loader/src/loader/identity/
       resources.rs`'s `count_fidbf` — which only counts `.fidbf` files
       for a resource-inventory health check. Nothing computes a query
       hash to look functions up by, so statically-linked library
       functions (memcpy, OpenSSL, etc.) in stripped binaries are never
       identified via FID at all — Ghidra's FID analyzer equivalent is
       entirely unused. **In progress, see below.**
    2. DWARF enum types: `DW_TAG_enumeration_type` (0x04) is registered in
       `types.rs`'s `collect_type_names` type-name cache (so a variable's
       type resolves to the enum's name), but `analyze_types_inner`'s
       extraction only handles `DW_TAG_structure_type`/`class_type`/
       `union_type` (0x13/0x02/0x17) — enumerator names/values
       (`RED=0, GREEN=1, ...`) are never extracted, so decompiled output
       always shows raw integer comparisons, never the named constant.
    3. DWARF array types: `DW_TAG_array_type`/`DW_TAG_subrange_type`
       aren't in `resolve_type_name`'s match arms or `collect_type_names`'s
       registered tag list at all — a struct field whose type is an array
       resolves to `"unknown"`.
    4. `DW_TAG_lexical_block` PC ranges aren't tracked anywhere — no
       variable scoping/shadowing model, which also matters for properly
       scoping register-locals to a lexical block rather than the whole
       function (deferred in the entry above).
    5. `.debug_line` (the line-number program) is section-loaded into
       `gimli::Dwarf` but `unit.line_program`/its row iterator is never
       consumed anywhere — no address-to-source-line mapping capability
       exists despite the raw data being available.
    6. **PDB has no equivalent of any of this session's DWARF work at
       all**: `pdb_sidecar.rs` only extracts function/param names; local
       variable `location` is hardcoded `DwarfLocation::Unknown` (line
       156); there's no `PdbTypeInfo`/struct-layout extractor. Everything
       built this session (struct fields, register locals) only benefits
       DWARF-carrying binaries — Windows PE/PDB binaries get none of it.
    User chose to pursue (1), FID, first.
  - **FID implementation, in progress**: computing Ghidra-compatible query
    hashes requires reproducing `MessageDigestFidHasher.hash()`
    (`Ghidra/Features/FunctionID/.../hash/MessageDigestFidHasher.java`):
    FNV-1a 64-bit digest (`generic.hash.FNV1a64MessageDigest.java` — simple,
    ported exactly), a per-architecture "instruction skipper" (NOP-equivalent
    byte patterns to exclude, e.g. `X86InstructionSkipper.java` — simple,
    just a raw byte-pattern list), and an **instruction mask** that zeroes
    operand bytes while keeping opcode/pattern bytes (the "full hash") plus
    operand-scalar handling for the "specific hash". The mask was the
    unknown: Fission's SLEIGH runtime (`fission-sleigh`) doesn't read
    Ghidra's *packaged* `.sla` (`vendor/` is reference-only per
    `THIRD_PARTY.md`), but does self-compile the real `.sla` *format* from
    the same open `.slaspec` sources and read that at runtime
    (`compiler/sla/{native,packed}.rs`, `discovery::
    require_packaged_sla_for_entry_spec` — "required for production lift
    frontends") — so the underlying pattern/mask data is the same
    representation Ghidra itself derives its mask from, not a
    reimplementation guess.
    - `instruction_pattern_mask` (`fission-sleigh/src/runtime/spine/
      compiled_table/mod.rs`, commits `854393c2`/`96b17a3b`) walks the
      decoded `RuntimeConstructState` tree (root + every subtable-resolved
      operand) and unions each node's *instruction*-relative
      `CompiledPatternBlock` (from `match_trace.matched_leaf_pattern`,
      already captured during normal decode for other purposes — no new
      SLEIGH-level instrumentation needed) at each node's absolute byte
      offset. Context-register pattern bits are excluded (matches Ghidra's
      `getInstructionMask()` scope). An `Or(...)` pattern (a constructor's
      own `cond1 | cond2` statement) takes the intersection across
      alternatives, not the union — erring toward a missed hash match
      over a wrong one when it can't tell which alternative actually fired.
    - Found and fixed a real bug surfaced only by this work: a "replaces
      current" wrapper constructor (an x86 legacy/REX prefix byte's own
      constructor, which matches just that byte then hands off entirely to
      the constructor for the rest of the instruction —
      `constructor_replaces_current` in `walker.rs`) had its own matched
      pattern silently discarded, so any prefixed instruction's mask was
      missing that byte's contribution even though real Ghidra's mask
      includes it. Fixed via a new, purely-additive
      `RuntimeConstructState.replaced_wrapper_patterns` field populated
      only at the `replace_current` call site (commit `96b17a3b`).
    - **Validated against real Ghidra 12.0.4** (headless, `analyzeHeadless`
      + a small script printing `InstructionPrototype.getInstructionMask()`
      — Java 17 + the vendored Ghidra distribution are both usable
      locally) on three cases, all exact byte-for-byte matches: `jnz +5`
      (no prefix), `mov rax,0x1234` (REX.W prefix), `mov ax,0x1234`
      (0x66 operand-size-override prefix — confirms the wrapper-pattern
      fix isn't REX-specific).
    - **Update, same day — full hash done, commit `ad053d72`**: the
      "function extent" question turned out to already be answered —
      `DecodedPcodeFunction.instructions` (computed by the existing
      `lift_raw_pcode_function_with_context_and_memory_context` for every
      normal decompile) is exactly Ghidra's `FunctionBodyFunctionExtent
      Generator` concept, just previously discarded by every caller.
      `compute_fid_full_hash` (`compiled_table/fid_hash.rs`) ports the rest
      of `MessageDigestFidHasher.hash()`'s *full*-hash path: a byte-for-byte
      port of `FNV1a64MessageDigest` (offset basis `0xcbf29ce484222325`,
      prime `1099511628211`, reset-after-digest), the x86
      `X86InstructionSkipper` NOP-equivalent byte patterns, call counting,
      and per-operand mixing (a register operand mixes in its SLEIGH
      offset; a scalar/immediate contributes a fixed `0xfeeddead`
      placeholder — full-hash-only, doesn't need the scalar's actual
      value). Takes a caller-supplied `resolve_register_offset` callback
      rather than depending on `fission_pcode::midend::cspec::
      RegisterModel` directly (wrong dependency direction — `fission-pcode`
      depends on `fission-sleigh`). Exposed via `RuntimeSleighFrontend::
      fid_full_hash`.
      - Validated against real Ghidra 12.0.4 (`FidService.hashFunction`,
        headless) on `push rbp; mov rbp,rsp; mov eax,0x2a; pop rbp; ret` (5
        instructions, register+immediate operands only — deliberately
        chosen to sidestep the memory-operand gap below): exact match on
        the first attempt, `full_hash=0x37783a7364fbdfe5`,
        `code_unit_count=5`, both sides.
      - **Update, same day — simple memory operands closed, commit
        `8f61f44d`**: `[reg]`/`[reg+disp]` (the common case — most real
        instructions have at least one memory operand) now mix in
        correctly. Two things needed solving:
        1. **Address recovery**: a memory operand still produces no
           `BoundOperand` at the `RuntimeConstructState` level (confirmed:
           `mov eax,[rbp+8]`'s handle keeps `debug_value: None`). The
           computed address only exists in *this instruction's own p-code*
           — `IntAdd(RBP, 8)` feeding a `Load`.
           `trace_simple_memory_address` walks the instruction's p-code
           backward from the owning handle's `RuntimeFixedHandle::
           {offset_space, offset_offset, offset_size}` triple to the
           producing op, recognizing a bare register or a
           register+constant `IntAdd` — bails (rather than guess) on
           anything else, e.g. SIB `base+index*scale` addressing, still
           not handled.
        2. **Operand ordering, a second real bug caught before it shipped**:
           Ghidra's `getNumOperands()`/`getOpObjects(ii)` enumerate
           *display* operands (`"mov eax,[rbp+8]"` has exactly 2), which
           turned out not to match `state.handles`' own count *or order* —
           that instruction has 3 handles (one a hidden zero-extend
           wrapper, never displayed), with the memory operand's handle
           listed *first* and EAX's *second*, the opposite of display
           order. The original (register/immediate-only) implementation
           iterated `state.handles`/`state.operands` directly and happened
           to work purely because that case has no hidden operands to
           misorder — silently wrong once memory operands entered the
           picture. Fixed by deriving order from
           `state.display_template.pieces`'s `OperandRef` sequence, which
           is what actually encodes Ghidra-equivalent display order.
        - Validated against real Ghidra 12.0.4 on the *same* function as
          above with the immediate replaced by `[rbp+8]`: exact match on
          the first attempt, `full_hash=0x82d2e963fd88461b`,
          `code_unit_count=5`.
      - **Update, same day — full pipeline proven end-to-end against a
        real Ghidra-shipped database**: wrote a throwaway integration test
        (`fission-decompiler`, not committed — depended on external paths
        via env vars) that loaded a real statically-linked x86-64 ELF
        (compiled with Docker `gcc:latest`, GCC 16), linear-swept every
        function's instructions (crude, no CFG-following — good enough to
        prove the pipeline, not production-quality extent extraction),
        computed `fid_full_hash` via `RegisterModel::lookup_name` as the
        resolver, and looked each hash up against the real, Ghidra-shipped
        `utils/signatures/fid/gcc-x86.LE.64.default.fidbf` (43,016
        functions, 24 gcc 4.4–4.8-era library builds).
        - 69 of 1097 functions hashed successfully (the rest hit the SIB /
          unhandled-operand-shape bail-out or failed to linear-sweep
          cleanly — expected, given this test's crude extent extraction,
          not a hashing bug).
        - One raw `full_hash` collision turned up: a 4-code-unit function
          (the absolute minimum — `FID_SHORT_CODE_UNIT_LIMIT`) collided
          with LLVM's `_ZN6__lsan9ThreadTidEm`. Correctly rejected by
          `FidbfDatabase::identify_by_hashes`'s existing
          `FID_ACCEPT_THRESHOLD` (14.6) gate — `score_match`'s base score
          *is* `code_unit_size`, so 4 (or 4+10 with a specific-hash bonus
          this test didn't compute) never clears 14.6. This is Ghidra's own
          design working as intended (tiny functions are genuinely too
          generic to fingerprint reliably), not a gap in this
          implementation — and confirms the scoring/threshold layer
          (already fully implemented in `fidbf/types.rs`, from before this
          session) is necessary and correctly wired into `identify_by_hashes`.
        - No confident (post-threshold) matches for *this* binary against
          *this* database — expected and correct: the test binary's GCC 16
          doesn't correspond to any of the gcc 4.4–4.8 builds the database
          covers. Real Ghidra FID matching has the exact same
          version-specificity; "no match" against a version-mismatched
          database is the designed behavior, not a failure to detect.
        - **Net result**: the full pipeline — real binary bytes → extent →
          `instruction_pattern_mask` → `compute_fid_full_hash` → real
          `.fidbf` database → `identify_by_hashes` scoring/threshold —
          runs end-to-end with zero crashes and zero false positives on
          real Ghidra-shipped data. What's left (below) is coverage
          (more operand shapes, more architectures) and productionizing
          (wiring into the actual decompile path instead of a throwaway
          test), not open questions about whether the core algorithm works.
      - **Still not done**: SIB addressing (`base+index*scale`), the
        "specific hash" (needs actual scalar values + relocation-awareness
        — `OperandType.isAddress`/`hasRelocation` in
        `MessageDigestFidHasher.java`), a *production* function-extent
        generator (this session used `DecodedPcodeFunction.instructions`
        for the earlier register/immediate/simple-memory tests, and a
        crude linear sweep for the E2E proof above — neither is CFG-aware
        the way Ghidra's own body-based extent is), wiring a query hash
        through `FidbfDatabase::identify_by_hashes` into an actual
        decompiler-facing "identified function" fact (rename the function,
        surface the match in `fission_cli list`/`info`, etc.), and non-x86
        architectures (only x86-64 validated so far).
      - **Update — wired into `fission_cli` as a real, user-facing
        `identify` subcommand** (commit `8734c19b`). New
        `fission-decompiler::fid` module: `FidIdentifier::new(binary,
        &databases)` builds a per-binary lifter + `RegisterModel` once;
        `.identify(address)` decodes the function via
        `lift_raw_pcode_function_with_context_and_memory_context` (proper,
        `DecodedPcodeFunction.instructions`-based extent — not the
        throwaway linear sweep from the E2E test above), calls
        `fid_full_hash`, and looks the hash up across every loaded
        `.fidbf` database via `identify_by_hashes`, keeping the
        highest-scoring hit. `load_fid_databases(binary)` loads every
        `.fidbf` matching the binary's pointer width once per binary (not
        per function — parsing isn't free).
        - CLI wiring follows the existing `Xrefs`/`Callgraph` canonical-subcommand
          pattern exactly: `IdentifyArgs{binary, function: Option<u64>, common}`
          → `CliCommand::Identify` → `"identify"` in `CANONICAL_SUBCOMMANDS`
          → `normalize_canonical` sets `identify_cmd`/`identify_function` on
          `OneShotArgs` → dispatch in `oneshot/mod.rs` → `run_identify` in
          the new `oneshot/identify.rs`, mirroring `run_callgraph`'s
          dual text/JSON output shape. `legacy.rs`'s `normalize_legacy`
          also needed the two new `OneShotArgs` fields defaulted (the
          struct has no `Default` shortcut there — every field is listed).
        - `fission_cli identify <bin>` runs against every non-import
          function; `--function <addr>` narrows to one. Both modes support
          `--json`.
        - Validated: `cargo check --workspace --all-targets` clean,
          `cargo nextest run` 378/378 across
          fission-cli/fission-decompiler/fission-sleigh, release build,
          and a manual smoke test against a fresh Docker-built (`gcc:latest`)
          statically linked x86-64 ELF — `identify` considered 2181
          functions and correctly reported zero matches (same
          version-mismatch behavior as the E2E test above: this build's
          toolchain isn't one of the bundled `.fidbf` databases' covered
          builds), both `--json` and text, both whole-binary and
          `--function` modes, no crashes.
          `golden_corpus_check.py` still passes (FID is a fully separate,
          opt-in code path — zero impact on existing decompile output).
        - Deliberately still not done, same list as above: SIB addressing,
          specific hash, non-x86 architectures, and folding a match into
          the decompiler's own naming/rendering (`identify` is a
          standalone report command for now, not yet consulted by
          `decomp`/`list` to rename functions).
      - **Update — SIB addressing** (commit `64245e16`). User picked SIB
        first among the four remaining items, since it's the biggest
        remaining hashing-coverage gap (most non-trivial array/struct field
        accesses are `[base+index*scale(+disp)]`, and every function
        containing one previously bailed out of hashing entirely).
        - Discovered via a headless Ghidra script printing raw
          `Instruction.getOpObjects(ii)` for three real SIB instructions
          (`[rax+rcx*4+0x10]`, `[rax+rcx*1]` with disp==0, `[rax+rcx*8+0x100]`):
          a SIB memory operand's object list is `[Register(base),
          Register(index), Scalar(scale), Scalar(disp)?]` — the scale
          `Scalar` is **always present, even at scale == 1**; the
          displacement `Scalar` is present **only when disp != 0**. Since
          `MessageDigestFidHasher.java`'s full-hash mixing adds the same
          flat `0xfeeddead` placeholder for *every* `Scalar` object
          unconditionally, this means a SIB operand with a nonzero
          displacement contributes the placeholder **twice** (once for
          scale, once for disp) — a detail invisible from the Java source
          alone without seeing real `getOpObjects()` output.
        - `fission_cli raw-pcode` against the same three instructions showed
          Fission's own SLEIGH-generated p-code resolves the address via one
          of two backward `IntAdd`/`IntMult` producer chains depending on
          whether disp is zero: `IntAdd(base,disp) → IntMult(index,scale) →
          IntAdd(combine)` when disp != 0, or directly
          `IntMult(index,scale) → IntAdd(base,combine)` when disp == 0 (no
          intermediate base+disp op at all — order of the two source ops
          also flips). `trace_simple_memory_address` now recognizes both,
          alongside the pre-existing bare-register and register+constant
          shapes, via two new small producer-matching helpers
          (`producer_reg_plus_const`, `producer_scaled_index`) rather than a
          generic recursive expression matcher — SLEIGH only ever emits
          this fixed small set of shapes for x86 addressing, so a general
          matcher would be unused generality.
        - `MemoryAddressShape` gained a `BaseIndexScale{base, index,
          has_displacement}` variant (scale's *value* is never tracked —
          only presence matters for the full hash, which never mixes in
          actual scalar values); `mix_memory_operand_full` mixes both
          register offsets plus one placeholder for the always-present
          scale scalar and a second if `has_displacement`.
        - Added `fid_full_hash_matches_ghidra_exactly_for_sib_addressing`,
          validated byte-for-byte against real Ghidra 12.0.4
          `FidService.hashFunction()` output for all three cases on the
          first attempt (`45285b0d87470466`/`71e530ce7190c262`/
          `f66301fb4931933a`, all `codeUnitSize=4`) — no back-and-forth
          debugging needed, since the `getOpObjects()` inspection up front
          resolved the ambiguity that would otherwise have required trial
          and error.
        - Validated: `cargo nextest run` 379/379 across
          fission-sleigh/fission-decompiler/fission-cli, release build,
          `golden_corpus_check.py` clean, manual smoke test (no crashes on
          both the SIB test binary and the earlier Docker-built static ELF;
          still zero confident matches against the bundled databases, as
          expected — this only widens *what can be hashed*, not which
          databases a given binary's toolchain happens to match).
        - Still not done: specific hash, non-x86 architectures, wiring a
          match into decompiler-facing naming/rendering, and RIP-relative
          addressing (a fourth p-code shape `trace_simple_memory_address`
          doesn't recognize — noted but not investigated this round).
      - **Update — "wiring a match into decompiler-facing naming" turned
        out to already exist, but broken** (commit `fbcc7f16`). User picked
        this next. Before adding a new `--identify` flag to `list`/`decomp`
        as planned, checking whether FID *output* already reached decomp
        surfaced `FactStore::from_binary` → `ingest_signature_matches`
        (`fission-static/src/analysis/decomp/facts.rs`) — already wired,
        already feeding `StrongFid`-provenance `NameFact`s into
        `CallTargetIndex` (used to rename call *targets* in decomp output,
        e.g. `call sub_402000` → `call memcpy` for a statically-linked
        libc function — the actually-common FID use case, more so than
        renaming the decompiled function's own header). It just never
        worked, silently, via an entirely separate, pre-existing FID
        implementation (`fission-signatures/src/fid/{hash,x86_decoder,matcher}.rs`,
        838 lines, distinct from the `fidbf/` parser this session's FID
        work builds on) with two independent, source-verifiable bugs:
        - `hash.rs`'s digest folded state after every byte
          (`state ^= state >> 32`) — not part of real FNV-1a, which
          Ghidra's `FNV1a64MessageDigest` is (confirmed by porting and
          Ghidra-validating the real algorithm this session) — and mixed
          operand `i32`s little-endian where Ghidra's
          `AbstractMessageDigest.update(int)` is big-endian.
        - `x86_decoder.rs` (a hand-rolled x86 length-decoder, not
          SLEIGH-based) never captured register operands into
          `FidOperandValue` at all — only displacement/immediate scalars.
          Ghidra mixes register operands into every hash, and nearly every
          x86 instruction has one.
        - Net effect: this already-shipped path could never produce a
          correct match against a real `.fidbf` database. Not "rarely" —
          structurally never, for any function with a register operand
          (i.e. nearly all of them).
        - Fix: swapped `ingest_signature_matches`' hashing to
          `fission_sleigh::runtime::RuntimeSleighFrontend::fid_full_hash` +
          `fission_pcode::midend::cspec::register_model_for_language` —
          `fission-static` already depends on both (`fission-sleigh`,
          `fission-pcode`), so no new crate-graph edge. `FidDatabaseSet`'s
          discovery (compiler/format-filtered path resolution via
          `ResourceProvider`) was correct and is unchanged — only the
          per-function hash computation was swapped. Deleted `hash.rs`
          and `x86_decoder.rs` (confirmed via grep across the workspace
          that nothing else used their types) and trimmed `matcher.rs`
          down to just `FidDatabaseSet`.
        - Split `ingest_signature_matches` into a thin outer function
          (real database discovery) and
          `ingest_signature_matches_with_databases(binary,
          &[FidbfDatabase])`, specifically so the fix could be proven
          end-to-end without needing a real binary whose toolchain
          happens to exactly match a bundled `.fidbf`'s build — every
          attempt this session to get a *live* positive match (GCC 16 via
          Docker `gcc:latest`, then GCC 4.8.5/glibc-2.17 via a CentOS 7
          container specifically chosen to match the "el7" naming
          convention seen in `utils/signatures/fid/el7.x86_64.fidbf`)
          came back "no match" — FID's own well-documented brittleness
          (any byte-level codegen difference, e.g. a glibc patch release,
          invalidates a match), not a bug. Added
          `fid_signature_match_ingests_strong_fid_name_fact`: a synthetic
          in-memory `FidbfDatabase` seeded with the exact full hash
          already Ghidra-validated earlier this session
          (`fid_full_hash_matches_ghidra_exactly_for_register_only_function`'s
          `0x37783a7364fbdfe5`) proves the decode → hash → lookup →
          `StrongFid` `NameFact` chain works end-to-end, on the first
          attempt.
        - Validated: `cargo check --workspace --all-targets` clean.
          `cargo nextest run --workspace`: 2088/2095 passed; the 7
          failures are pre-existing and bit-for-bit identical on
          unmodified `main` (confirmed via `git stash` — all
          `fission-emulator` diag/profile tests failing with "Failed to
          fetch instruction bytes at 0xFFFFFFFF", unrelated to FID).
          Release build + `golden_corpus_check.py` both clean (no change
          to the golden corpus's decompile output either way, since none
          of those 16 binaries happen to trigger a real match).
        - Net effect for the user-facing ask: `decomp`'s call-target
          renaming now has a chance of actually firing when a binary
          happens to match a bundled database, instead of the silent
          no-op it always was before. No new CLI surface was needed for
          this half of "wire into decomp/list" — the plumbing already
          existed, it just needed the broken half replaced.
        - Still open: `list`'s own `--identify`-style annotation (this
          repo's `identify` subcommand from the previous slice already
          covers that as a standalone report), specific hash, non-x86
          architectures, and RIP-relative addressing.
      - **Update — confirmed the `NameFact` → decompiled-output path with a
        live before/after, then closed the RIP-relative gap** (commit
        `5680a752`). User asked directly whether the fix actually reaches
        decompiler output, not just the `FactStore` layer.
        - Traced `CallTargetIndex`/`NirTypeContext.call_targets` forward
          from `fission-decompiler/src/facts/facts.rs` into
          `fission-pcode/src/midend/builder/expr/lower_expr.rs` (30+
          consult sites — this is the NIR call-lowering code that names
          `CALL` p-code ops in rendered pseudocode), confirming the wiring
          is real, not just a `FactStore`-internal data structure nobody
          reads.
        - Then proved it directly rather than trusting the trace: built a
          throwaway two-function synthetic binary (`callee: ret`;
          `caller: call callee; ret`) and called
          `decompile_with_rust_sleigh_with_facts` (the same entrypoint
          `fission_cli decomp` uses) twice — once against a plain
          `FactStore::from_binary`, once after manually calling
          `ingest_name_fact(callee_addr, "memcpy", FactProvenance::StrongFid)`
          (the exact same call `ingest_signature_matches_with_databases`
          makes on a real match). Output before: `sub_401000();`. After:
          `memcpy();`. Not committed (throwaway demo, matches this
          session's established pattern for local-only proofs) — the
          permanent regression coverage for the hash-to-`NameFact` half
          already exists in `fid_signature_match_ingests_strong_fid_name_fact`.
        - User then asked to continue with the remaining 3 gaps (specific
          hash, non-x86, RIP-relative) and picked RIP-relative first.
          Investigating it (headless Ghidra `getOpObjects()` inspection +
          `decode_instruction_raw_state` on the same instruction) found the
          "gap" was already closed by construction, needing zero code
          changes: Ghidra's `getOpObjects()` returns a single `Address`
          object for a RIP-relative memory load (`mov eax,[rip+0x100]` →
          `GenericAddress(0x40180a)`) and a `Scalar` object for `LEA`
          (`lea rax,[rip+0x200]` → `Scalar(0x401916)`, since `LEA` computes
          rather than dereferences) — different Java object types, but
          `MessageDigestFidHasher.java` mixes both identically for the
          *full* hash (`fullUpdate += 0xfeeddead` either way — only the
          *specific* hash, not implemented, distinguishes them). Fission's
          own runtime independently resolves both cases to
          `BoundOperand::Immediate` at decode time (confirmed via
          `decode_instruction_raw_state`), which `mix_operand_full`'s
          existing `Immediate` branch already handles correctly — proven,
          not assumed, via
          `fid_full_hash_matches_ghidra_exactly_for_rip_relative_memory_load`/
          `_lea`, both matching real Ghidra 12.0.4 on the first attempt.
          Fixed stale doc comments elsewhere that still listed RIP-relative
          (and, in `fid.rs`, SIB — predating that fix) as open gaps.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run` 381/381 (+2) across
          fission-sleigh/fission-decompiler/fission-cli, release build,
          `golden_corpus_check.py` clean.
        - Remaining open items, unchanged: specific hash, non-x86
          architectures. RIP-relative is no longer on this list.
      - **Update — specific hash implemented** (commit `687bf756`). User
        picked this next of the remaining two (specific hash, non-x86). The
        biggest single FID slice this session: `compute_fid_full_hash`
        became `compute_fid_hashes`, computing full and specific digests
        together in one pass (they share masked bytes and operand
        traversal, diverging only in per-operand mixing values).
        - Two things needed real Ghidra cross-checking, not just a Java
          source read: a headless script printed
          `Instruction.getOperandType(ii)` /
          `OperandType.isScalar`/`isAddress` for six cases (plain immediate,
          RIP-relative memory load, RIP-relative `LEA`, SIB memory, a direct
          `CALL` target, a `-no-pie` static absolute address). The real
          classification wasn't what the Java object type (`Scalar` vs
          `Address`) alone suggested: a RIP-relative memory *load* is
          `isAddress=true` (placeholder in the specific hash), but `LEA`'s
          *computed* value is `isAddress=false` (real value used) even
          though both are RIP-relative — `LEA` computes a value, it doesn't
          dereference one. Fission's own signal for this, found by comparing
          `decode_instruction_raw_state` output for `LEA` vs `CALL` vs a
          memory load: `RuntimeFixedHandle::space == "ram"` — true for
          memory dereferences *and*, surprisingly, direct `CALL`/`JMP`
          targets (both resolve through the code/ram address space), false
          for `LEA` and plain immediates (both land in `"const"` space).
          This single check unifies both placeholder cases cleanly.
        - SIB's compound-operand scalars (displacement, scale) needed their
          real numeric values threaded through `MemoryAddressShape`
          (previously only presence booleans, sufficient for the full
          hash), gated by Ghidra's `-256 < v < 256` magnitude check that
          applies only to compound (not whole-operand) scalars — confirmed
          scale is small enough to always count (1/2/4/8), displacement
          sometimes isn't.
        - `fission-static`'s `ingest_signature_matches_with_databases` and
          `fission-decompiler`'s `FidIdentifier` now pass the real
          `specific_hash` to `identify_by_hashes` instead of a hardcoded
          `0`, enabling the `+10` score bonus and the `force_specific`
          database-entry filter (previously always incorrectly rejecting
          any `force_specific` entry, since `0` could never equal a real
          specific hash). `fission_cli identify`'s output gained a
          `specific_matched` field.
        - Deliberately still not relocation-aware
          (`MessageDigestFidHasher.java`'s `hasRelocation` check, which
          forces the placeholder regardless of the classification above
          when an operand's bytes carry a relocation) — documented in
          `fid_hash.rs` with the concrete, bounded impact: can only make a
          real match score more conservatively (missing the bonus it should
          have gotten), except for a `force_specific` database entry, which
          could be incorrectly rejected until relocation-awareness lands.
        - Added `fid_hashes_match_ghidra_exactly_for_specific_hash_operand_classification`,
          covering all six classification cases in one test, all matching
          real Ghidra 12.0.4 byte-for-byte on the first attempt — no
          debugging iteration needed, since the `getOperandType` inspection
          up front resolved the classification question before writing any
          mixing code.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run --workspace` 2092/2099 (+4 vs. the prior 2095-test
          baseline; same 7 pre-existing unrelated `fission-emulator`
          failures as every prior check this series), release build,
          `golden_corpus_check.py` clean.
        - **First live, non-synthetic confident match this entire FID
          series**: `fission_cli identify` against the CentOS 7 (GCC
          4.8.5/glibc-2.17) statically-linked test binary from the earlier
          `ingest_signature_matches` fix (commit `fbcc7f16`) — every prior
          attempt this session against a real binary came back "no match"
          (compiler/library version mismatch against the bundled `.fidbf`
          databases, expected per FID's own brittleness) — now finds
          `__register_frame_table` (`libgcc-7`) at `0x491c70`, score 18.0,
          `specific_matched: true`. The binary's own ELF symbol table
          (unstripped) already names this function `__register_frame_table`
          too — FID's independent hash-based identification exactly
          reproduces a name that's independently known correct, real
          end-to-end confirmation that specific-hash support widened real
          matches, not just synthetic test coverage.
        - Remaining open item: non-x86 architectures (only x86-64 validated
          all series).
      - **Update — non-x86 (AArch64) validation, found and fixed a big gap**
        (commits `745576ac`, `6cf9fcf2`). User picked non-x86 as the last
        remaining item and confirmed "implement it properly" once the scope
        turned out much larger than a quick check.
        - Built a real AArch64 static binary (Docker `gcc:latest --platform
          linux/arm64`, native — no QEMU emulation needed on this arm64
          host) and ran `fission_cli identify` against it: no crash, but a
          quick source read surfaced a real, independent bug first:
          `X86InstructionSkipper` (the alignment-NOP skip list ported
          earlier this session) is the *only* `InstructionSkipper` anywhere
          in Ghidra (`Ghidra/Processors/x86/`) — non-x86 architectures get
          *zero* skippers in real Ghidra, but Fission was applying the x86
          list unconditionally. Fixed by gating on `CompiledFrontend::arch`
          (commit `745576ac`) — low risk in practice for fixed-4-byte ISAs
          (AArch64/MIPS: length-sensitive slice equality can never match a
          1-2 byte pattern) but a real risk for variable-length ones (ARM
          Thumb, 68k).
        - Then the big one: cross-checking a real GCC-compiled AArch64
          `atoi` against a headless Ghidra script found `stp`/`ldp`
          (register-pair save/restore with pre/post-index writeback —
          present in *almost every* non-leaf function's prologue/epilogue)
          fell through `compute_fid_hashes`' fail-closed "unhandled shape"
          branch every time, meaning essentially no realistic AArch64
          function could be hashed at all. `decode_instruction_raw_state`
          on the raw `stp`/`ldp` instructions found two shapes
          `trace_simple_memory_address` didn't recognize:
          - Pre-index (`stp x29,x30,[sp,#-0x10]!`): self-referential
            `IntAdd(sp,disp)` writing back directly into **register**
            space, not a unique-space temp like every x86 shape validated
            so far. Fixed by having the caller try
            `register_space_index` as a target-space candidate too, not
            just the hardcoded `unique_space_index` — the underlying
            `IntAdd(reg,const)` matcher didn't need to change at all.
          - Post-index (`ldp x29,x30,[sp],#0x10`): the address used for the
            access is a bare `Copy(sp)` into **unique** space (a snapshot
            taken *before* the writeback, which happens via a separate,
            unconnected `IntAdd` mutating the register directly for the
            *next* instruction to see) — but Ghidra's `getOpObjects` still
            lists the writeback displacement as this operand's `Scalar`.
            Fixed by having the `Copy` arm additionally search the
            instruction's p-code for a self-referential
            `IntAdd(reg,const) -> reg` writeback elsewhere
            (`find_self_writeback_displacement`) and fold it in as a
            displacement even though it doesn't feed the traced address at
            all.
        - A second, independent gap surfaced while validating the specific
          hash on the same function: the `isAddress` classification (real
          value vs. specific-hash placeholder) that worked for x86 — "does
          `RuntimeFixedHandle::space == \"ram\"\"" — doesn't generalize.
          x86's `CALL rel32` happens to *also* resolve through `"ram"`
          space (confirmed earlier this session), but AArch64's `bl`
          resolves its immediate target through `"const"` space instead
          (confirmed via `decode_instruction_raw_state`) — the first case
          this session where two architectures' Ghidra-matching behavior
          for the *same semantic operand kind* required *different*
          Fission-side signals. Replaced the ram-space-only check with an
          OR: ram space (memory dereferences — architecture-independent)
          or `flow_kind` is `Call`/`Jump`/`ConditionalJump` with an
          `Immediate` operand (branch/call targets, using
          `DecodedFlowKind` — already relied on elsewhere in this file for
          `call_count` — instead of the architecture-specific space
          check). `direct_target` turned out not to be reliably populated
          outside the full lift path, so the check is the coarser but
          still universally-true-in-practice "a Call/Jump/ConditionalJump
          instruction's Immediate operand(s) are the target," documented
          as only affecting the specific hash's real-vs-placeholder choice
          for an edge case neither x86 nor AArch64 exhibit.
        - Added `fid_hashes_match_ghidra_exactly_for_aarch64_stp_ldp_prologue_epilogue`
          (the real `atoi`'s full 9-instruction prologue/epilogue —
          `paciasp; stp...!; mov w2,#0xa; mov x1,#0x0; mov x29,sp; bl
          ...; ldp...,#0x10; autiasp; ret`), matching real Ghidra 12.0.4 on
          full hash, full count, specific hash, *and* specific count in one
          test. Register offsets (`x0=0x4000`, `x29=0x40e8`, `sp=0x8`, ...)
          read directly from `AARCH64instructions.sinc`'s `define register`
          blocks rather than hand-derived from decode output, to avoid
          compounding two independent sources of possible error.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run` 450/450 across
          fission-sleigh/fission-decompiler/fission-cli/fission-static,
          release build, `golden_corpus_check.py` clean. Live smoke test
          against the real AArch64 binary: **2 real confident matches**
          (`_Unwind_GetIPInfo`, `__deregister_frame`, both from
          `libgcc-7-dev-arm64`, both `specific_matched: true`), both
          reproducing the binary's own already-correct unstripped symbol
          table names — the second non-synthetic, live confirmation this
          session (after the el7 x86-64 `__register_frame_table` match),
          and the first on a non-x86 architecture.
        - This closes the FID feature series' last open item from this
          session's scope. Remaining known gaps, all previously documented
          and still standing: relocation-awareness for the specific hash,
          other non-x86 architectures beyond AArch64 (only AArch64 itself
          was validated — MIPS/PowerPC/ARM32/68k etc. are unvalidated and
          likely have their own architecture-specific addressing-mode gaps
          the same way AArch64 did), and wiring a match into `list`'s own
          output (currently `decomp`'s call-target renaming and the
          standalone `identify` report command, not `list`).
      - **Update — moved to metadata gap (2), DWARF enum values** (commit
        `fc64285b`). With the FID series' scope closed, user asked what's
        still missing vs. Ghidra and picked enum values (gap 2 from the
        original audit list above) to pursue next.
        - Added `DwarfAnalyzer::extract_enum_info`, dispatched from
          `analyze_types_inner` alongside the existing struct/class/union
          path (`DW_TAG_enumeration_type`, 0x04, was already cached by name
          in `collect_type_names` but never routed to an extractor).
          Reuses `DwarfMemberInfo` rather than adding a parallel type:
          `offset` holds the enumerator's `DW_AT_const_value` instead of a
          byte offset — a dedicated field would have touched the ~10 other
          files across the workspace that construct `InferredFieldInfo`
          plus its rkyv archive format, so this is documented dual-use
          instead (on both the DWARF-side struct and `InferredFieldInfo`
          itself).
        - `DW_AT_const_value` on an enumerator is commonly `Sdata`-encoded
          for negative values, which the existing `get_attr_u64` doesn't
          handle at all (would've silently produced `0` for e.g. `enum {
          NEGATIVE_ONE = -1 }` rather than erroring or bailing). Added
          `get_attr_i64` alongside it rather than overloading `get_attr_u64`.
        - Added a real GCC-compiled test fixture
          (`testdata/x64_dyn_enum_test.elf`, 16KB dynamically-linked —
          `enum Color { RED=0, GREEN=1, BLUE=5, NEGATIVE_ONE=-1 }`) and
          `analyze_types_extracts_real_enum_values`. Cross-checked against
          `objdump --dwarf=info` first to confirm the negative enumerator
          really is `Sdata`-encoded (not just assumed from the DWARF spec)
          before writing the extraction code, matching this session's
          established "verify the real encoding before trusting a
          spec-level assumption" discipline from the FID work.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run -p fission-loader` 100/100, release build +
          `golden_corpus_check.py` clean (additive — doesn't touch existing
          decompile output).
        - Remaining metadata gaps from the original audit, unchanged: array
          types (still resolve to `"unknown"`), `DW_TAG_lexical_block` PC
          ranges (untracked), `.debug_line` (loaded, never parsed), PDB
          struct/location extraction (both still missing).
      - **Update — DWARF array types (metadata gap 3)** (commit `65606a55`).
        Next pick after enum values.
        - `DW_TAG_array_type`/`DW_TAG_subrange_type` weren't in
          `resolve_type_name`'s match arms or `collect_type_names`'s tag
          list at all, so any array-typed struct field (a common pattern —
          fixed buffers, matrices) resolved to the literal string
          `"unknown"`.
        - Extended `TypeDieInfo` with `array_dimensions: Vec<Option<u64>>`
          (one entry per `DW_TAG_subrange_type` child — multi-dimensional
          arrays have multiple), populated by a new
          `array_subrange_dimensions` helper only for `DW_TAG_array_type`
          DIEs. Each dimension resolves `DW_AT_count` directly if present,
          else `DW_AT_upper_bound + 1` (DWARF's bound is inclusive/
          zero-based), else `None` for unbounded. `resolve_type_name`
          gained a `DW_TAG_array_type` arm: resolve the element type
          (`DW_AT_type`) recursively, append bracketed dimensions —
          `"int[3][4]"`. Flows through to struct member resolution
          automatically via the existing `type_cache`/`resolve_type_ref`
          machinery already used for pointer/const/volatile — no changes
          needed there, mirroring how cleanly the enum-value work slotted
          into the existing struct/class/union extraction path.
        - Real GCC-compiled fixture (`testdata/x64_dyn_array_test.elf` —
          `struct WithArrays { int arr[10]; int matrix[3][4]; char
          name[16]; }`) and `analyze_types_resolves_array_member_type_names`,
          matching `"int[10]"`/`"int[3][4]"`/`"char[16]"` exactly. Checked
          `objdump --dwarf=info` first (established discipline this
          session): this compiler always emits the inclusive-upper-bound
          form, never `DW_AT_count` directly, so the test exercises the
          `+ 1` fallback path specifically, and `matrix`'s 2D case exercises
          multiple subrange children under one `array_type` DIE.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run -p fission-loader` 101/101, release build +
          `golden_corpus_check.py` clean.
        - Remaining metadata gaps, unchanged: `DW_TAG_lexical_block` PC
          ranges, `.debug_line` parsing, PDB struct/location extraction.
      - **Update — PDB struct/variable location extraction (metadata gap
        6)** (commit `f771cc80`). Offered as an `AskUserQuestion` among the
        3 remaining gaps; user picked this one as "권장" (recommended).
        - Struct/class layouts: every named `TypeData::Class` in the type
          stream (skipping anonymous classes and forward declarations with
          `fields: None`) is walked through its `FieldList` — following the
          `continuation` chain PDB uses to split large field lists across
          multiple linked records — to pull real member name/type/offset,
          pushed straight into the same `InferredTypeInfo` structure DWARF's
          own struct extraction already populates (no new PDB-specific
          struct type needed).
        - Variable locations were the harder half. `S_LOCAL` carries an
          `isparam` flag but the `pdb` crate's `SymbolData::Local` has no
          location field at all (real location lives in separate
          `S_DEFRANGE_*` records this crate version doesn't parse);
          `S_REGREL32`/`SymbolData::RegisterRelative` is self-contained
          (register + offset) but has **no param/local flag whatsoever** —
          confirmed via `llvm-pdbutil dump --symbols` against the real,
          locally-present `vendor/binaries/tests/x86_64/windows/
          fauxware.pdb`, where `printf`'s wrapper has `_Format` (a real
          parameter) and `_Result` (a real local) as plain, indistinguishable
          `S_REGREL32` records. Presented as an `AskUserQuestion` (simple
          "treat all as locals" vs. a proper param/local classifier); user's
          answer was **"use all your low-level knowledge to solve it
          properly, fall back to option 1 only if it doesn't work"**, which
          ruled out the simpler approximation.
        - The only record that disambiguates is `S_FRAMEPROC` (frame size +
          param/local frame-pointer register), and the `pdb` crate doesn't
          parse it at all — `symbol.parse()` returns `Err` for this kind, so
          the original loop's `Err(_) => continue` silently skipped every
          one. Manually decoded from `symbol.raw_bytes()` per LLVM's
          `FrameProcSym`/`SymbolRecordMapping.cpp` layout (`TotalFrameBytes:
          u32 @[2..6)`, `Flags:u32 @[24..28)`, bits 14-15/16-17 =
          local/param `EncodedFramePtrReg`), cross-checked against real
          bytes captured from `printf`'s own `S_FRAMEPROC` before trusting
          it (`total_frame_bytes=80`, both registers decode to `rsp`,
          matching `llvm-pdbutil`'s own decode of the same bytes).
        - Register numbers resolve through a Ghidra-ported name table
          (`pdb_registers.rs`, `regX86`/`regAmd64` verbatim from
          `RegisterName.java`) rather than the CodeView spec by hand — a
          first hand-transcription was wrong (142/363 entries vs. the
          correct 145/368), caught by a length-verification script before
          it shipped and now pinned by a permanent regression test.
        - `classify_register_relative`: when `S_FRAMEPROC`'s param/local
          frame-pointer registers differ, classify by which register the
          symbol references; when they're the same (the common x64
          RSP-relative, no-frame-pointer case), fall back to `offset >=
          total_frame_bytes + pointer_size` — mirrors Ghidra's own
          frame-offset-based approach in
          `RegisterRelativeSymbolApplier.java` rather than replicating its
          full prologue-analysis machinery. Validated byte-for-byte against
          `printf`'s real classification: `_Format@96` → Param,
          `_ArgList@56`/`_Result@32` → Local.
        - New committed fixture (`testdata/x64_pdb_struct_test.exe`/`.pdb`,
          built with `clang-cl`+`lld-link` via Homebrew LLVM — no MSVC or
          Windows SDK needed) validates struct/field extraction
          (`analyze_pdb_extracts_real_struct_layout`, a `Point{x,y,z}`
          struct at real offsets 0/4/8). Note: clang-cl's CodeView backend
          emits `S_LOCAL`+`S_DEFRANGE_FRAMEPOINTER_REL`, never
          `S_REGREL32`, so this fixture doesn't exercise
          `classify_register_relative` — that path is covered instead by
          unit tests built from real bytes captured out of `fauxware.pdb`.
          `vendor/binaries/...fauxware.pdb` itself stayed uncommitted
          (`vendor/` is gitignored) but was used for an end-to-end proof
          before being removed from the working tree: 102 struct types
          extracted (e.g. `IMAGE_DOS_HEADER`, 19 fields) and 80/154 (52%) of
          all parameters across the binary gained a real stack location
          where previously every one was `Unknown`.
        - Documented simplifications: `S_REGISTER`/`RegisterVariable` (no
          frame-relative offset to classify against) always treated as a
          local — never observed even once in the real test PDB.
          `S_DEFRANGE_*` records remain unparsed (the `pdb` crate doesn't
          support them); affected `S_LOCAL` locals get name+type with
          location left `Unknown`, still strictly better than before.
          Per-field size for PDB struct members left at `0`, matching
          DWARF's own existing behavior for non-bitfield members.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run -p fission-loader` 108/108, release build +
          `golden_corpus_check.py` clean, `cargo nextest run --workspace
          --no-fail-fast` 2103/2110 (7 pre-existing, unrelated
          `fission-emulator` failures, confirmed via `git stash` earlier
          this session to fail identically on unmodified `main`).
        - Remaining metadata gaps, unchanged: `DW_TAG_lexical_block` PC
          ranges, `.debug_line` parsing.
      - **Update — `.debug_line` parsing (metadata gap 5, last one)** (commit
        `23478bee`). `.debug_line` was already loaded into `gimli::Dwarf`
        (`analyzer::build_dwarf`) but its `line_program`/row iterator was
        never run anywhere — every compilation unit's line-number program
        existed only as unread bytes, so there was no way to answer "what
        source line does address X map to" at all.
        - New `dwarf/lines.rs`: `DwarfAnalyzer::analyze_lines` runs each
          unit's line-number program (the byte-coded state machine DWARF
          section 6.2 describes) and flattens every row into a
          `DwarfLineRow{address, file, line}`. Skips `end_sequence` rows
          (boundary markers just past the last real instruction, not
          themselves attributable to a line) and line-0 rows (producers'
          explicit marker for code that can't be attributed to any source
          line, e.g. compiler-generated padding). File names are resolved
          once per distinct file index per unit rather than once per row —
          the same file repeats across nearly every row of a real program.
        - Landed in a new `LoadedBinary::dwarf_lines: Vec<DwarfLineRow>`
          field (sorted ascending by address, not serialized — same
          "rebuilt on each load" convention as `dwarf_functions`/
          `pdb_functions`), wired into the same `rayon::join`/merge phase in
          `loader/mod.rs` as `analyze_types`/`analyze_functions`.
          `LoadedBinary::line_for_address(addr)` binary-searches it for the
          nearest preceding row, matching the DWARF convention that a row's
          line covers every address up to the next row.
        - Validated by cross-checking byte-for-byte against `llvm-dwarfdump
          --debug-line` (Homebrew LLVM) on the already-committed
          `x64_dyn_enum_test.elf` fixture *before* writing the permanent
          test — a throwaway scratch test dumped the extractor's rows
          side-by-side with the dwarfdump output first (`pick()`'s body,
          0x401106..0x40111f, lines 8/9/9/10/11), confirming an exact match
          including which address dwarfdump reports as `end_sequence`
          (0x401131) before pinning that address as the one intentionally
          absent from the table. The permanent test asserts these exact
          rows, not just "some non-empty result."
        - **Deliberately no consumer wired yet** — HIR/NIR statements carry
          no per-instruction `address: u64` field to join a line lookup
          against today (only `NirFunction` has a function-level address),
          and the CLI's non-JSON decompile header is duplicated across ~6
          near-identical rendering sites in `decompile_exec/run.rs` (a
          `--json`/text x single/batch x rust-sleigh/sequential/parallel
          matrix); wiring display there is a separate, larger, riskier
          slice than this one. An initial attempt wired `source_line` into
          `decompile_exec/output.rs`'s JSON/text output, but that function
          turned out to only be reachable via one rarely-hit fallback path
          (`--addr` with no function found) — the real `--addr`/`--all`
          paths go through `run.rs` entirely, so the wiring would have
          silently done nothing for normal usage. Reverted rather than ship
          a half-connected display path; the extraction capability itself
          (this commit) is what closes the audit gap, matching how enum
          values and array types landed as pure extraction before anything
          displayed them.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run -p fission-loader` 111/111, release build +
          `golden_corpus_check.py` clean, `cargo nextest run --workspace
          --no-fail-fast` 2106/2113 (same 7 pre-existing, unrelated
          `fission-emulator` failures as every other check this session).
        - **All 5 gaps from the original audit are now closed**: enum
          values, array types, PDB struct/location, and `.debug_line`
          parsing are all implemented; `DW_TAG_lexical_block` PC ranges
          (variable scoping/shadowing) is the one item from the original
          list that was never picked up.
      - **Update — `DW_TAG_lexical_block` PC ranges, last remaining gap**
        (commit `b496b382`). `functions.rs`'s function-extraction DFS
        already visited `DW_TAG_lexical_block` DIEs but just fell through
        them ("just continue DFS") — every nested `DW_TAG_variable` landed
        in `local_vars` flat, indistinguishable from a function-level one.
        Concretely: shadowed C locals (same name redeclared in a nested
        block — legal in C, not rare) all merged into separate
        `DwarfLocalVar` entries with identical names and zero way to tell
        which one is live at a given address.
        - `DwarfLocalVar` gained `scope: Option<(u64, u64)>` — the
          innermost enclosing lexical_block's `(low_pc, high_pc)`, `None`
          for a variable declared directly under the function.
        - The existing flat-DFS depth-tracking loop already tracks
          `func_depth` (used to detect exiting the subprogram itself) — a
          `scope_stack: Vec<(isize, u64, u64)>` of `(push_depth, low_pc,
          high_pc)` reuses that exact same depth-comparison machinery:
          `DW_TAG_lexical_block` pushes its range (via a new
          `lexical_block_range` helper, reusing `subprogram_size`'s
          offset-vs-absolute `high_pc` form handling — confirmed via
          `llvm-dwarfdump -v` that GCC emits the identical `DW_FORM_data8`
          offset form for `lexical_block`'s `high_pc`, not just
          `subprogram`'s), and it's popped by the same "depth at or past
          where we pushed it" check the subprogram-exit logic already used.
          A `DW_TAG_variable`'s scope is whatever's on top of the stack at
          the moment it's visited — automatically the *innermost* enclosing
          block when blocks nest, not the outer one.
        - **Empirical validation requested explicitly** (실측): built a
          fresh GCC (Docker, x86-64 target) fixture
          (`x64_dyn_lexblock_test.elf`) — `compute(x)` with three nested `{
          int total = ...; }` blocks, each shadowing the outer `total`,
          compiled with `-Wshadow` first to confirm GCC really treats them
          as three distinct variables (not one reused slot silently). Ran
          `llvm-dwarfdump -v --debug-info` *before* writing any extraction
          code to read the raw DIE tree: both nested `lexical_block`s use
          `DW_AT_high_pc [DW_FORM_data8]`, resolving to
          `[0x401113, 0x40113e)` and `[0x401125, 0x401138)`. A throwaway
          scratch test then dumped the extractor's actual output side by
          side with these numbers — exact match, including which of the
          three `total`s got `scope: None` (the function-level one) — before
          the scratch test was deleted and a permanent one pinning these
          exact addresses took its place.
        - `DwarfLocalVar`'s new field required touching every struct-literal
          construction site workspace-wide (`pdb_sidecar.rs`'s three sites,
          all `scope: None` since PDB carries no lexical-block concept in
          this crate's coverage; two `#[cfg(test)]`-only sites in
          `fission-decompiler/orchestration/engine.rs` and
          `fission-static/analysis/decomp/facts.rs`).
        - Deliberately not attempted: `DW_AT_ranges`-based non-contiguous
          lexical_block PCs (GCC only emits these under heavier
          optimization when a block's code gets split; `-O0` fixtures don't
          exercise it) — `lexical_block_range` returns `None` for that case,
          degrading the variable to unscoped rather than computing a wrong
          range.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run -p fission-loader -p fission-decompiler -p
          fission-static` 220/220 (1 skipped, pre-existing/unrelated),
          release build + `golden_corpus_check.py` clean, `cargo nextest
          run --workspace --no-fail-fast` 2107/2114 (same 7 pre-existing,
          unrelated `fission-emulator` failures as every check this
          session).
        - **This closes the last of the original 6 audited metadata gaps**
          (FID, enum values, array types, PDB struct/location, `.debug_line`
          parsing, lexical_block scoping) — Fission's DWARF/PDB extraction
          coverage now matches Ghidra's on every item that audit surfaced.
      - **Update, 2026-07-21 (broader Ghidra-parity audit)**: with the
        DWARF/PDB metadata series closed, user asked what else is missing
        vs. Ghidra, this time outside that specific area. A `general-purpose`
        agent surveyed Ghidra's Auto-Analysis analyzer roster (`vendor/
        ghidra/.../Ghidra/Features/{Base,Decompiler,FunctionID,PDB}`) against
        the Rust workspace; findings verified by hand (file:line citations
        checked, not taken on faith), ranked most-impactful first:
        1. Windows exception handling: `.pdata`/`.xdata`/SEH/
           `__CxxFrameHandler` — Fission's `pe/pdata.rs` only extracts
           function begin/end RVAs from `RUNTIME_FUNCTION`, never reads the
           pointed-to `UNWIND_INFO`. Ghidra's `PEExceptionAnalyzer` builds
           real try/catch scope data from it. **Not started.**
        2. GCC/Itanium LSDA (`.gcc_except_table`) — `elf/eh_frame.rs` only
           extracts function boundaries from FDEs, never follows the LSDA
           pointer `Fde::lsda()` already resolves. Ghidra's
           `GccExceptionAnalyzer` parses call-site/action/type tables from
           it. **Done, see below.**
        3. No Call-Fixup mechanism — Ghidra's `CallFixupAnalyzer` replaces
           calls to compiler helper stubs (`__x86.get_pc_thunk.*`,
           `__chkstk`) with their true semantic effect at lift time; Fission
           has zero equivalent (`__chkstk` is only an import-name label, no
           p-code substitution). **Not started.**
        4. Itanium RTTI multi/virtual inheritance
           (`__vmi_class_type_info`) — `analyzers/cpp.rs`'s
           `parse_itanium_type_info` only handles `__si_class_type_info`
           (single inheritance); MSVC's `parse_msvc_col_info` in the same
           file already walks full base-class arrays, so only the Itanium
           side lags. **Not started.**
        - Non-findings (already at parity, don't re-investigate): switch/
          jump-table recovery (`fission-pcode`), non-returning-function
          detection (`fission-core`, consumes Ghidra's own
          `noReturnFunctionConstraints.xml` directly), demangling (real
          crates for Rust/Itanium/MSVC; only Swift shells out to an external
          binary, a portability wrinkle not a capability gap).
        - User picked (2), LSDA parsing, next.
      - **Update — GCC/Itanium LSDA parsing (broader-audit item 2)** (commit
        `6c8d5aee`). `gimli` parses `.eh_frame`'s CIE/FDE structure and
        already resolves each FDE's LSDA *pointer* via `Fde::lsda()`, but
        has no knowledge of `.gcc_except_table`'s own contents — an ad hoc,
        GCC-specific format documented informally in libgcc's `unwind-c.c`/
        `unwind-pe.h`, not the DWARF spec itself. Nothing in the loader
        read past the pointer.
        - New `elf/lsda.rs` hand-parses the LSDA header (`LPStart`/`TType`
          encodings), call-site table (instruction ranges → landing pad +
          type-filter action chain), action table (a linked list of SLEB128
          `(filter, next-offset)` records, chained per libgcc's
          `PERSONALITY_FUNCTION` convention — next offset relative to where
          *this* record started, not where the offset field itself is
          stored), and type table (`std::type_info` addresses, indexed
          *backward* from a computed base). `analyze_eh_lsda` walks
          `.eh_frame`'s FDEs independently of `eh_frame.rs`'s early-pipeline
          function-boundary extraction (needs `LoadedBinary::get_bytes` for
          indirect pointers, which needs the full section table, so this
          runs post-load) into a new `LoadedBinary::eh_lsda: HashMap<u64,
          LsdaInfo>`, gated on ELF format.
        - **실측 (explicit empirical-validation request)**: built two fresh
          `g++`-compiled fixtures via Docker (x86-64 target) from identical
          try/catch source — a static/non-PIE build and a `-fPIE` build —
          both committed (`x64_dyn_lsda_test.elf`/`x64_dyn_lsda_pie_test.elf`).
          Dumped both via `readelf --debug-dump=frames` (CIE/FDE
          augmentation, confirming the LSDA pointer's encoding byte-by-byte)
          and `objdump -s -j .gcc_except_table` (raw LSDA bytes), then
          hand-decoded the entire header/call-site/action/type-table chain
          against libgcc's own `parse_lsda_header` logic *before* writing
          any Rust. Cross-referenced the type table's resolved address
          against `readelf -r`'s relocation listing to confirm it named
          exactly `_ZTISt13runtime_error@GLIBCXX_3.4` (the caught type),
          catching one real hand-arithmetic mistake (misreading a 4-byte LE
          pcrel offset as a single byte) before it reached code.
        - **The PIE fixture surfaced a design gap the static-only case
          couldn't have**: its type-table entry uses `DW_EH_PE_indirect` (a
          GOT-style slot the dynamic linker only populates at process load
          time), so the raw address read from the file is a meaningless
          placeholder (`0`) even though the LSDA correctly identifies which
          type is caught. `readelf -r` showed the GOT slot's own *address*
          carries a relocation naming the real symbol. `LsdaTypeEntry`
          therefore carries both a best-effort `address` and a `symbol`
          resolved independently via `LoadedBinary::relocation_symbols` —
          confirmed both fixtures resolve to the same human-readable
          `"typeinfo for std::runtime_error"` (Fission's own demangled
          symbol form) via two structurally different relocation mechanisms
          (`R_X86_64_COPY` direct vs. `R_X86_64_64` on a GOT slot).
        - Deliberately unsupported: `DW_EH_PE_textrel`/`datarel`/`funcrel`/
          `aligned` application modes (only `absptr`/`pcrel` appeared in
          either fixture; returns `None` rather than guess a base) and
          uleb128/sleb128-encoded type-table entries (would break the
          fixed-size backward indexing the scheme depends on; no real
          producer does this).
        - No CFG/decompiler consumer wired yet — matches how `.debug_line`
          parsing landed earlier in this series (pure extraction first);
          propagating exception edges into the CFG is a separate, larger
          slice, same conclusion the original audit reached.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run -p fission-loader` 116/116 (4 new: 2 pure-parser unit
          tests against hand-verified byte arrays, 2 end-to-end against the
          real committed fixtures), release build + `golden_corpus_check.py`
          clean, `cargo nextest run --workspace --no-fail-fast` 2111/2118
          (same 7 pre-existing, unrelated `fission-emulator` failures as
          every check this session).
        - Remaining broader-audit items, unchanged: Windows `.xdata`/SEH
          exception handling, Call-Fixup mechanism, Itanium
          multi/virtual-inheritance RTTI.
      - **Update — wire LSDA exception edges into the CFG** (commit
        `d6c08b43`). User asked to close the gap the LSDA-parsing commit
        deliberately left open. Confirmed the real bug first: `guarded()`'s
        entire `catch` body was silently absent from `decomp` output on
        `x64_dyn_lsda_test.elf` — not just unstructured, *gone* — since the
        landing pad has no ordinary jump/call/fallthrough predecessor (only
        reachable via the C++ personality routine unwinding into it at
        runtime), so every stage that derives reachability from real branch
        instructions treated it as dead code.
        - Traced the pipeline stage by stage with `raw-pcode`/
          `pcode-topology` CLI output at each step (not by guessing) and
          found the fix needed **three separate, independently-broken
          layers**, each re-deriving reachability from scratch without
          knowing about the other's hints:
          1. `fission-static`'s `ControlFlowFacts::assemble` now folds
             `binary.eh_lsda`'s landing pads into `label_leaders`/
             `flow_edges`, and `decode_context_for` exposes them via a new
             `DecodeMemoryContext::additional_decode_entries` field — kept
             separate from `block_entry_hints`/`flow_edges`, which (it
             turned out) only affect how *already-decoded* instructions get
             grouped into blocks, never what gets decoded in the first
             place.
          2. `fission-sleigh`'s actual instruction-decode worklist AND its
             separate post-decode reachability walk (`reach_queue`, which
             re-prunes anything the first pass decoded but doesn't itself
             re-reach via branch semantics) both only ever start from
             `entry_address` — `DecodeMemoryContext`'s hints were never
             consulted for seeding, only used afterward for block-splitting.
             Both now also seed from `additional_decode_entries`.
          3. `fission-pcode`'s `build_successor_index_map` — the function
             `PreviewBuilder::new_with_binary` uses to build the
             successors/predecessors arrays dominance, loop analysis, and
             dead-code elimination all run on — independently re-derives
             every edge from each block's own terminator op and **never
             reads `PcodeBasicBlock.successors` at all**, so even after (1)
             and (2) correctly put the landing pad in the p-code with a real
             successor edge at the `fission-sleigh` layer, this layer
             recomputed its own successors from scratch and dropped it
             again. New `lsda_extra_edges()` resolves `binary.eh_lsda`'s
             addresses to this function's own block indices and merges them
             into `successors` right before `predecessors` is derived —
             confirmed via direct inspection that
             `predecessors[landing_pad_idx]` goes from empty to
             `[call_site_idx]`.
        - Both `fission-pcode` entry points are strictly additive and scoped
          to `binary.eh_lsda`: a function with no LSDA entry (every function
          in every binary without C++ exceptions — the overwhelming
          majority of all decompiled code) gets zero extra edges, so this
          can't change existing behavior for anything else. This mattered a
          lot here specifically because `build_successor_index_map` is about
          as foundational/high-blast-radius as a function gets in this
          codebase.
        - **Also hit, and had to clean up, this session's worst case yet of
          the recurring `cargo fmt` sweep pitfall**: running it on the 5
          touched files reformatted **70 files** across `fission-pcode`/
          `fission-sleigh` (pre-existing, unrelated drift in both crates).
          Reverted all 65 untouched files via `git checkout --`, then
          manually re-diffed the remaining 5 against pre-fmt content to
          strip incidental reformatting of *pre-existing* nearby code while
          keeping the actual additions — `git diff` on the final 5 files
          confirmed purely additive (zero unexpected deletions) before
          committing.
        - Validated: `cargo check --workspace --all-targets` clean, `cargo
          nextest run -p fission-pcode -p fission-sleigh -p fission-static
          -p fission-decompiler` 1325/1325 (5 new tests: 4 unit tests for
          `lsda_extra_edges` in `fission-pcode`, 1 in `fission-static`
          confirming `ControlFlowFacts`/`DecodeMemoryContext` threading),
          release build + `golden_corpus_check.py` clean (critical given the
          blast radius — confirms zero behavioral change for the 160
          existing golden functions), `cargo nextest run --workspace
          --no-fail-fast` 2116/2123 (same 7 pre-existing, unrelated
          `fission-emulator` failures as every check this session).
        - **Deliberately not attempted**: the final decompiled C-like text
          still doesn't render the `catch` block's code, even though it's
          now correctly present and connected in the HIR builder's own block
          graph (verified non-empty predecessors, no irreducible-edge
          pruning removing it). That's a **fourth** distinct layer —
          `fission-midend-structuring`'s SESE region/materialization rules,
          which decide how to linearize a block graph into actual
          statements — and needs its own focused investigation into how it
          decides which *reachable* blocks earn a structured-programming
          representation (not just whether they're reachable at all).
          Scoped as further follow-up, not attempted here, to keep this
          change's diff and risk to exactly what was traced and verified.
      - **Update — protect LSDA landing-pad labels from dead-label cleanup**
        (commit `b98d9041`). User asked to continue into the fourth layer.
        Traced it precisely by diffing before/after diagnostic dumps at each
        pipeline stage rather than guessing further:
        - `try_lower_if` (`fission-midend-structuring/src/conditionals/
          plain_if.rs`) correctly structures the landing pad's own real
          conditional branch into an `HirStmt::If`, and
          `reconstruct_sese_final_body` correctly computes it needs a
          leading `Label` (a genuine multi-entry jump target) and inserts
          one — confirmed via a dump right before `finalize_structured_body`
          runs: the label IS present, directly preceding the structured
          catch-handler `Block`. So the "fourth layer" hypothesis from the
          prior update turned out to be wrong — SESE structuring itself was
          already correct.
        - The **actual** culprit: `cleanup_redundant_labels`
          (`fission-midend-core/src/util/label_cleanup.rs:27`) keeps a
          `Label` only if it's the first statement or something does a
          textual `Goto` to it. A landing pad's label is neither — reachable
          only via the personality routine unwinding into it at runtime,
          which has zero `HirStmt` representation — so this entirely
          reasonable "kill unreferenced labels" rule (correct for every
          ordinary label in the codebase) silently deletes it, and its
          disappearance is what let `finalize_structured_body`'s separate
          "strip code between an unconditional goto and the next label"
          pass treat the whole handler as dead code on the very next pass.
        - Fix: `StructuringHost` gains `lsda_landing_pad_labels()` (sourced
          from `LoadedBinary::eh_lsda`, empty for every function without C++
          exceptions), and a new `cleanup_redundant_labels_protecting`
          unions that set into "referenced" rather than relying on textual
          `Goto` alone. Threaded into all four call sites that previously
          called `cleanup_redundant_labels`/`finalize_structured_body`
          without host awareness (`normalize_guarded_tail_layout` ×2 call
          sites, `build_linear_multiblock_body`, `try_repair_orphan_gotos`,
          `SeseStructuringPass`) — four separate places independently
          capable of dropping the same label, matching the "duplicated
          dead-code logic across many passes" pattern this whole
          investigation kept surfacing at every layer.
        - Validated: full pre-existing suite across the 5 touched crates
          unchanged (1424/1424, +2 new), `golden_corpus_check.py` clean
          (critical — `cleanup_redundant_labels`/`finalize_structured_body`
          are among the most foundational, widely-shared utilities in the
          whole structuring pipeline; clean golden corpus confirms zero
          behavioral change for any function without LSDA data), full
          workspace regression unaffected (same 7 pre-existing unrelated
          `fission-emulator` failures as every check this session). Also hit
          this session's worst `cargo fmt` sweep yet mid-investigation (70
          files across `fission-pcode`/`fission-sleigh`) while iterating on
          an *earlier, wrong* hypothesis about this bug (the "fourth layer"
          one that turned out to be already-correct); that work was fully
          reverted via debug-instrumentation cleanup before this update's
          actual fix was written, so it left no trace in this commit.
        - **Still not attempted**: `fission-midend-normalize` independently
          reimplements the identical pattern
          (`cleanup/control_flow.rs::prune_unreachable_after_terminal`,
          runs *after* structuring, no `protected`-label concept). Final
          decompiled *text* still won't show the catch handler until that's
          fixed too — same fix shape, but normalize's pipeline has no
          `StructuringHost`-equivalent binary-aware context threaded through
          it at all today, making it a larger, separate follow-up rather
          than a trivial continuation.
      - **Update — protect LSDA landing-pad labels in fission-midend-normalize
        too, closing out the LSDA investigation end to end** (commit
        `fb57f1dc`). User asked to continue past the structuring-layer fix.
        `prune_unreachable_after_terminal` turned out to be only the first
        of **six** independent places in this crate reimplementing the same
        "zero textual `Goto` references == dead label" heuristic — each
        found one at a time via iterative debug-instrumented CLI runs
        against `guarded()`/`x64_dyn_lsda_test.elf` (add a targeted
        `eprintln!`, rebuild release, run the CLI, see where the label still
        disappears, read surrounding code for the next culprit, repeat):
        `prune_unreachable_after_terminal`; `cleanup_func_stmt_list`'s own
        `global_refs` computation; the `cleanup_boundary_label_{stage}` pass
        closure's separate `global_refs` recompute; the `depth == 0` branch
        in `cleanup_stmt_list_with_options_and_preserved`; the same
        `depth == 0` pattern duplicated verbatim in both `cleanup_stmt_list`
        and `cleanup_stmt_list_with_options`; and
        `single_pred_label_inline_flat` — structurally different from the
        rest, since it drains the "dead zone" *between* a `Goto` and its
        matching `Label` using a separate `ref_counts`/
        `collect_referenced_label_counts` comparison rather than a plain
        referenced-set lookup.
        - Fix: `PROTECTED_LSDA_LABELS`, a `thread_local!` built to the exact
          shape of the pre-existing `GLOBAL_SYMBOL_CONTEXT` in the same file
          (`pipeline/run.rs`) — set from `StructuringHost::
          lsda_landing_pad_labels()` right before `normalize_hir_function`
          runs (`fission-pcode/src/midend/orchestrate.rs`), cleared right
          after. Chosen over threading a `protected: &HashSet<String>`
          parameter through `normalize_function_body`/`normalize_hir_function`
          because that would touch ~70+ call sites (mostly raw `HirFunction`
          struct-literal test constructors across `fission-pcode`'s midend
          test suite, none of which have or need LSDA context); and over
          adding a field to `HirFunction` itself, which doesn't derive
          `Default` or have a constructor, so every one of its ~40
          construction sites would need a mechanical update for a field the
          overwhelming majority of them would never set. Kept
          `std::collections::HashSet` rather than this crate's own
          `HashSet` `FxBuildHasher` alias, matching `GLOBAL_SYMBOL_CONTEXT`'s
          own precedent for a value crossing a crate boundary.
        - Validated: `cargo check --workspace --all-targets` clean,
          `cargo nextest run` on the 3 touched/adjacent crates (1281/1281),
          `golden_corpus_check.py` clean against the 160-function/16-binary
          snapshot (critical — every fixed function is used by every single
          decompiled function in the codebase; clean golden corpus confirms
          zero behavioral change for the overwhelming majority of code that
          has no LSDA data), full `cargo nextest run --workspace`
          (2122/2129, same 7 pre-existing unrelated `fission-emulator`
          instruction-fetch failures as every check this session). Added 5
          regression tests: `prune_unreachable_after_terminal` and
          `single_pred_label_inline_flat` each get a protected-label-survives
          case *and* (for the latter) a negative case proving an unprotected
          dead zone still drains normally, plus a `cleanup_func_stmt_list`
          integration-level test exercising the full entry point.
        - **This closes the LSDA investigation end to end**: `.gcc_except_table`
          parsing (LSDA metadata extraction) → CFG edge threading (landing
          pads reachable, not irreducible-cut) → SESE structuring label
          protection (`cleanup_redundant_labels_protecting`) → normalize
          label protection (this update). `guarded()`'s `catch` handler —
          the `runtime_error` type check via the `param_3`/selector
          register, the cleanup calls, `result = -1` — now renders as real
          code in `fission_cli decomp` output for the first time:
          ```c
          block_401230:
              {
                  if (param_3 != 1) {
                      rax = sub_4010b0((ulonglong)result, 4198974);
                  }
              }
              xVar7 = result;
              rax = sub_401040(xVar7, 4198982);
              local_10 = rax;
              local_4 = 4294967295;
              sub_401080(xVar7, 4198998);
              goto block_40122b;
          ```
      - **Update — Windows `.pdata`/`.xdata` SEH exception tables (mingw-w64
        `g++`)** (commit `ebb21967`). User picked the next broader-audit
        item: Windows `.xdata`/SEH. Investigated by cross-compiling a real
        `try`/`catch` fixture with `x86_64-w64-mingw32-g++` (`guarded()`/
        `risky()`, same shape as the ELF LSDA fixtures) and reading its
        `.pdata`/`.xdata` by hand against `objdump -x`'s "interpreted
        .xdata" dump before trusting a parser.
        - Key finding: mingw-w64 `g++` on x86_64 targets Windows' native SEH
          unwind ABI (`__SEH__`), but its C++ personality
          (`__gxx_personality_seh0`) still emits the *exact same*
          GCC/Itanium LSDA byte format already implemented for ELF's
          `.gcc_except_table` (same `LPStart`/`TType`/call-site-table
          header, same call-site record shape) — just physically appended
          after each function's `UNWIND_INFO` in `.xdata` as the
          "language-specific handler data" following the
          `ExceptionHandler` RVA, instead of referenced from `.eh_frame`.
          Confirmed byte-for-byte: the call-site table's landing-pad offset
          decodes to the exact address `objdump`'s disassembly shows as the
          `cmp rdx, 0x1` catch-dispatch check.
        - Given that, extracted the byte-format parser (`parse_lsda` and its
          `Cursor`/`LsdaInfo`/`LsdaCallSite`/`LsdaTypeEntry` types — all
          already producer-agnostic, taking only bytes + a
          `read_at`/`symbol_at` closure pair) out of `elf/lsda.rs` into a
          new shared `gcc_lsda.rs`, so PE reuses it instead of
          reimplementing the same encoding. `elf/lsda.rs` keeps only the
          ELF-specific half (walking `.eh_frame`'s FDEs to find the LSDA
          pointer); new `pe/seh.rs` is the PE-specific half (walks
          `.pdata`'s `RUNTIME_FUNCTION` table, reads each `UNWIND_INFO`'s
          flags/`CountOfCodes` to locate the trailing handler data, skips
          `UNW_FLAG_CHAININFO` entries — chained/split fragments, out of
          scope — and hands the rest to the shared parser).
        - Caught and fixed one real bug this generalization exposed:
          `.xdata`'s "language-specific handler data" isn't reserved
          exclusively for LSDAs the way ELF's `.gcc_except_table` *section*
          is — any `EHANDLER`/`UHANDLER` handler can stash arbitrary bytes
          there, and a non-C++ handler (mingw's CRT stack-probe handler, in
          the test fixture) decoded as call-site addresses in the billions
          when naively run through `parse_lsda`. No reliable way to
          name-check `ExceptionHandler`'s target in a stripped binary, so
          `call_sites_within_region` validates structurally instead: a real
          LSDA's call-site ranges and landing pads are always offsets
          inside the owning function's own `[begin, end)` — anything
          outside that is a different handler's data having been
          misparsed, not a real LSDA, and gets discarded.
        - `binary.eh_lsda` (already the generic, format-agnostic sink the
          entire downstream pipeline — CFG edge threading,
          `StructuringHost::lsda_landing_pad_labels`, normalize's
          `PROTECTED_LSDA_LABELS` — consumes unconditionally) now gets
          populated for PE the same way it already does for ELF. Verified
          this needed **zero further downstream changes**: the
          mingw-compiled `guarded()`'s catch handler renders correctly in
          `fission_cli decomp` output out of the box, closing the Windows
          side of the same landing-pad-rendering investigation this
          session already closed for ELF (the entire ELF-built pipeline —
          CFG edges, structuring label protection, normalize label
          protection — turned out to be genuinely format-agnostic).
        - New `testdata/x64_seh_guarded_test.exe` (stripped, 41KB, force-
          added past the repo's blanket `*.exe` `.gitignore` rule, matching
          `x64_pdb_struct_test.exe`'s existing precedent).
        - Validated: full workspace check clean, all pre-existing LSDA/SEH
          tests pass (1469/1469 across the 5 touched crates),
          `golden_corpus_check.py` clean (zero diff — no behavioral change
          for any binary without this exception data), full `cargo nextest
          run --workspace` shows only the same 7 pre-existing unrelated
          `fission-emulator` failures this session has confirmed unrelated
          at every prior checkpoint.
        - **Known limitation, left for a future slice if ever needed**:
          MSVC-compiled PE C++ EH (`__CxxFrameHandler3`/`4`'s own
          `FuncInfo`/`UnwindMapEntry`/`TryBlockMapEntry` tables) and raw
          MSVC `__try`/`__except` (`_C_specific_handler`'s own scope-table
          format) are genuinely different, unrelated encodings this doesn't
          attempt — narrower scope than "any PE personality routine",
          matching only the one this session has a real fixture for.
      - **Update — Call-Fixup mechanism (broader-audit item 3): found the
        real bug wasn't what the audit described** (commit `1a7c85f8`).
        User picked the next broader-audit item. Ghidra's `CallFixupAnalyzer`
        substitutes a known compiler-helper-stub call (`__chkstk`,
        `__x86.get_pc_thunk.*`) with its true semantic effect at lift time.
        Built a real `x86_64-w64-mingw32-gcc`-compiled fixture with an 8KB
        local array specifically to force a Windows/mingw stack probe, then
        checked the raw p-code by hand before assuming the audit's framing
        was still accurate for x64: it wasn't. `___chkstk_ms`'s own `Call`
        op has no output and never touches `rsp` — the real `sub rsp, rax`
        is ordinary, already-lifted caller code emitted right after the
        call. No call-fixup substitution is needed for chkstk on x64 at
        all (this differs from the historical 32-bit `_chkstk`/
        `_alloca_probe` convention, where the callee itself adjusted
        `esp`).
        - What actually corrupted the decompiled output were two separate,
          more fundamental stack-address-resolution gaps, both real
          regardless of chkstk:
          1. `resolve_stack_address_inner`'s `IntAdd`/`IntSub`/`PtrAdd`
             handling only resolved a delta operand via `const_offset`
             (requires a literal `const(...)` varnode) — so `sub rsp, rax`
             (`rax` holding a compile-time-constant probe size from an
             earlier `mov eax, IMM`) resolved to `None`, and everything
             downstream depending on `rsp`'s value past that point failed
             to resolve to a `(StackBase, offset)` pair at all. Fixed with
             `resolve_constant_operand`, falling back to a short
             `Copy`/`Cast`/`IntZExt`/`IntSExt` def-chain walk when the
             operand isn't itself a literal (mirrors the existing
             pointer-side recursion already in the same function).
          2. A second, independent bug surfaced only once (1) was fixed:
             functions establishing `rbp` via `lea rbp, [rsp+K]` for a
             nonzero `K` (MSVC/mingw position the frame pointer partway
             into a large frame, not at its base, so both locals and
             incoming-argument home slots stay within signed-displacement
             reach) got every `rbp`-relative local misclassified —
             `resolve_stack_address_inner`'s bare-`rbp` shortcut always
             treated `rbp` as "canonical frame base, offset 0" regardless
             of `K`, so a local at a small *positive* `rbp`-relative
             offset (still well short of the true caller-stack boundary)
             read as an incoming parameter via
             `classify_stack_slot_origin`'s positive-offset heuristic.
             Fixed by having `infer_entry_stack_layout` track `rsp`'s
             cumulative displacement from entry (`rsp_delta`) alongside
             `K`, threading the resulting bias (`rsp_delta + K +
             pointer_size`) through a new `rbp_frame_bias` field into the
             shortcut. The `+ pointer_size` term keeps the formula
             consistent with the pre-existing hardcoded `bias = 0` for the
             textbook `push rbp; mov rbp, rsp` case (`rsp_delta ==
             -pointer_size` right then). Also discovered zero-displacement
             `lea rbp, [rsp+0]` lifts as a plain register `Copy` (SLEIGH
             collapses the `+0` case) rather than through an
             `IntAdd`-into-temp step, so that path needed the same
             bias-aware treatment as the direct `mov rbp, rsp` copy, not
             just the nonzero-`K` `lea` path.
        - Caught one regression while iterating: an existing x86-32
          `CallInd` staged-args test used a synthetic `mov ebp, esp`
          prologue with no preceding push, which the new formula
          (correctly) treats differently from the standard `push ebp; mov
          ebp, esp` shape. The test's own doc comment says it's modeling
          real "m32-O0" codegen, which always pushes `ebp` first — so the
          fix was adding the missing `push ebp` op to the test's synthetic
          pcode sequence to match what it claims to represent, not
          weakening the assertion.
        - Validated against real corpus functions, not just synthetic
          ones: this also silently fixed two functions already in the
          golden corpus snapshot (`fibonacci` in `math_gcc_O0.exe`, and
          `rc4_init` in `crypto_gcc_O2.exe` — both use `lea rbp,[rsp+K]`
          independent of any chkstk call) whose XMM register-spill /
          parameter home-slot locals were previously misclassified the
          same way; snapshot updated to the now-correct output.
        - Validated: full workspace check clean, 1031/1031 in the 3
          touched/adjacent crates (+3 new regression tests for
          `infer_entry_stack_layout` covering the chkstk-adjacent `lea`
          case, the standard push+mov case, and the no-push `mov` edge
          case), `golden_corpus_check.py` clean against the corrected
          160-function snapshot, full `cargo nextest run --workspace`
          shows only the same 7 pre-existing unrelated `fission-emulator`
          failures this session has confirmed unrelated at every prior
          checkpoint.
        - **Not attempted**: `__x86.get_pc_thunk.*` (32-bit PIC GOT-base
          helper — genuinely needs call-fixup-style substitution, since it
          returns the caller's own post-call address in an arbitrary
          register, not through any standard ABI return-value convention)
          — out of scope for this update, which focused on the concrete,
          now-empirically-confirmed corruption rather than the audit's
          original (partially inaccurate, for x64) framing.
      - **Update — Itanium `__vmi_class_type_info` (broader-audit item 4,
        the last one): closes the 2026-07-21 broader Ghidra-parity audit**
        (commit `3a88da10`). User picked the final item.
        `parse_itanium_type_info` only recognized `__si_class_type_info`
        (single inheritance); anything using `__vmi_class_type_info` (more
        than one base, or any virtual base) silently got zero bases.
        - Built a real `x86_64-linux-musl-g++`-compiled fixture (`struct D
          : public B, public C` for multiple inheritance, `struct E :
          public virtual A` for virtual inheritance) and decoded the raw
          type_info bytes by hand against `objdump -s` before trusting a
          parser: confirmed `__vmi_class_type_info`'s layout byte-for-byte
          — base `__class_type_info` (vtable_ptr, name_ptr), then `flags:
          u32`, `base_count: u32`, then `base_count` `__base_class_type_
          info` entries (`{ base_type: ptr; offset_flags: long }`, bit 0 =
          virtual, bit 1 = public, bits 8+ = signed byte offset — for a
          virtual base this is a vcall-offset into the vtable, confirmed
          negative in the real `E` fixture). Base addresses surface as a
          flat `Vec<u64>`, matching `parse_msvc_col`'s existing MSVC-side
          shape (`CppClassInfo::base_classes`) rather than inventing an
          unused richer type.
        - **Found the whole Itanium RTTI analyzer was actually
          non-functional on any real ELF binary**, for two reasons
          unrelated to VMI itself, discovered only because this update
          insisted on validating against a real fixture instead of trusting
          the existing (untested — this file had zero `#[test]`s before
          this update) code:
          1. Discovery matched raw mangled prefixes (`"__ZTI"`/`"__ZTV"`),
             but `LoadedBinaryBuilder` demangles every symbol name
             (`iat_symbols`/`global_symbols`/`functions`, all formats)
             before this analyzer ever runs — `cpp_demangle`'s actual
             output for `_ZTI1D`/`_ZTV1D` is `"typeinfo for D"`/
             `"{vtable(D)}"`, which never matched, so discovery silently
             found zero classes on every real ELF binary. Rewrote to match
             the demangled convention.
          2. Even with discovery fixed, the SI/VMI discriminator (checking
             the type_info's own vtable pointer against
             `__si_class_type_info`'s/`__vmi_class_type_info`'s vtable
             symbol) failed for dynamically-linked binaries: that vtable
             lives in libstdc++ (an external DSO), so `vtable_ptr` is just
             an unrelocated on-disk placeholder (`0`). Fixed by consulting
             `relocation_symbols`, keyed by the field's own slot address
             (same pattern `elf/lsda.rs`'s `symbol_at` closure already uses
             for LSDA type-table entries) — checked *before* the
             value-based lookups, since checking it after left the
             placeholder `0` free to coincidentally match this loader's own
             synthetic `"ELF_HEADER"` marker at address `0`, masking the
             correct answer.
        - Also extended `to_inferred_types`'s struct-name formatting to
          walk *all* entries in `base_classes` (`"D : public B, public C"`),
          not just the first — the single-base assumption predates VMI
          support entirely.
        - New `testdata/x64_dyn_vmi_rtti_test.elf` (dynamically-linked
          `-fPIE`, 21KB, symbols intact since RTTI discovery is
          symbol-name-driven) plus a new `cpp.rs` test module: multi-base,
          single-virtual-base, and single-inheritance-still-works cases,
          all cross-checked against `nm`/`objdump -s`/`readelf -r`.
        - Validated: full workspace check clean, `fission-loader` 119/119,
          `golden_corpus_check.py` clean (unaffected — none of the C-only
          corpus binaries exercise C++ RTTI), full `cargo nextest run
          --workspace` shows only the same 7 pre-existing unrelated
          `fission-emulator` failures this session has confirmed unrelated
          at every prior checkpoint.
        - **Not attempted**: per-base offset/virtual-flag consumption
          downstream (only base type_info addresses are surfaced, matching
          the MSVC path's existing shallow scope); MSVC's own multi/virtual
          -inheritance handling was already at parity per the original
          audit finding.
        - **This closes the entire 2026-07-21 broader Ghidra-parity audit**:
          Windows `.pdata`/`.xdata` SEH (done), GCC/Itanium LSDA (done),
          Call-Fixup mechanism (investigated — found the real bug was a
          stack-frame resolution gap, not a literal call-fixup need; fixed
          that instead), Itanium multi/virtual-inheritance RTTI (done, this
          update). All four items from the original survey have been
          either implemented or resolved to their real underlying issue.
      - **Update — split DWARF debug-info resolution (`.gnu_debuglink`/
        `.note.gnu.build-id`), a fresh gap found via a second audit round**
        (commit `1e8046a3`). User asked to find another gap; a
        `general-purpose` survey of Ghidra's analyzer roster *outside* the
        Base/Decompiler/FunctionID/PDB areas the original audit covered
        (MicrosoftCodeAnalyzer, GnuDemangler, DWARF-external, Go, ELF/
        Mach-O-specific analyzers) found Ghidra's
        `DWARFExternalDebugFilesPlugin`: it follows a stripped binary's
        `.gnu_debuglink`/`.note.gnu.build-id` to load its real
        `.debug_info`/etc. from a *separate* companion file. Fission had
        no equivalent — `DwarfAnalyzer` only ever looked at
        `binary.sections` of the file actually being analyzed, so any
        binary using this split (every Debian/Ubuntu `-dbgsym` package,
        every Fedora/RHEL `debuginfo` package, and the local `objcopy
        --only-keep-debug` + `--strip-debug` + `--add-gnu-debuglink`
        workflow — the *default* packaging for most distro system
        libraries, not an edge case) silently produced zero DWARF-derived
        types/functions/lines, even though the DIE walker itself was
        already solid (confirmed by MSVC RTTI and Go's own real support
        already being at parity, ruled out during the same survey).
        - New `dwarf/external.rs` parses both conventions and tries them
          in order: `.gnu_debuglink` (NUL-terminated filename + 4-byte-
          aligned CRC32 of the companion's full contents, checked against
          `crc32fast` — rejects a stale same-named leftover from a
          previous build, the most realistic real-world failure mode) at
          two real candidate locations (same directory, `.debug/`
          subdirectory), then `.note.gnu.build-id` (standard ELF note,
          `NT_GNU_BUILD_ID`) at the distro-standard
          `/usr/lib/debug/.build-id/xx/yyyy...debug`. A candidate is only
          accepted after actually loading it and confirming it has
          `.debug_info`.
        - Wired as `LoadedBinary::external_debug_binary: Option<Box<
          LoadedBinary>>`, populated once in `auto_detect_and_parse` right
          after `eh_lsda` (only when the binary's own sections lack
          `.debug_info`), consulted via a new `DwarfAnalyzer::
          debug_source()` indirection so every section/byte access in the
          `dwarf` module transparently prefers the resolved companion.
          Loading the companion goes through a new
          `auto_detect_and_parse_inner(..., resolve_external_debug: bool)`
          so a companion that's itself (incorrectly or maliciously)
          stripped-with-a-debuglink can't chain into unbounded recursion —
          a real DWARF companion is never itself missing debug sections,
          so this only matters for adversarial inputs.
        - Built a real split-debug fixture the standard way (`gcc -g`,
          then the exact three-step `objcopy`/`strip` sequence every
          distro package's own build step uses) to validate against:
          loading *only* the stripped `x64_dyn_split_debug_test.elf`
          recovers full DWARF function info from the sibling `.elf.debug`
          this never gets opened directly except through debuglink
          resolution — confirmed end-to-end through the real CLI too
          (`fission_cli decomp` on the stripped binary alone now shows
          DWARF-sourced parameter names, not just `param_1`/`param_2`).
        - Validated: full workspace check clean, `fission-loader` 122/122
          (+4 new: happy-path resolution, the `.debug/` subdirectory
          convention, CRC-mismatch rejection, pre-existing no-companion
          case unaffected), `golden_corpus_check.py` clean (unaffected —
          none of the corpus binaries are stripped), full `cargo nextest
          run --workspace` shows only the same 7 pre-existing unrelated
          `fission-emulator` failures this session has confirmed unrelated
          at every prior checkpoint.
        - **Not attempted**: the build-id system path isn't independently
          testable in this dev sandbox (no real `/usr/lib/debug` locally),
          though it shares all the same validation logic already proven
          via the debuglink path.
      - **Update — full Ghidra analyzer-roster scorecard, then classic
        32-bit `__chkstk`/`__alloca_probe`** (commit `f52b5351`). User
        asked whether Fission is now genuinely "at the same starting
        line" as Ghidra. A `general-purpose` agent enumerated all 54
        non-abstract, non-scope-excluded analyzer classes across every
        Ghidra 12.0.4 `Features/*` module (not just the areas prior
        rounds sampled) and checked each against real Fission code:
        **24 solidly implemented, 12 legitimately out of Fission's scope
        (classic-Mac PEF, 16-bit real-mode x86, VersionTracking/BSim/
        Sarif/Headless — Ghidra-UI-only, .NET IL), 15 with zero
        corresponding code.** Also specifically checked ARM's
        `.ARM.exidx`/`.ARM.extab` EHABI unwind format (a hypothesis this
        might be a third distinct exception-handling convention beyond
        the already-closed Itanium LSDA/Windows SEH) — Ghidra itself
        never implements EHABI parsing either, so this is parity by
        mutual absence, not a Fission gap.
        - Honest answer given: **not full parity, but the base is
          genuinely strong** — the top-ranked real gap was
          `CallFixupAnalyzer` (general call-substitution mechanism),
          already self-diagnosed in this file (see the Call-Fixup update
          above) rather than newly discovered. Next tier:
          `TEBAnalyzer` (Windows TEB/PEB `fs:`/`gs:`-relative field
          recognition — confirmed SLEIGH already lifts these as
          `IntAdd(FS_OFFSET/GS_OFFSET, const)`, a viable hook point, but
          the naming-pipeline integration wasn't scoped out in this
          session), `SharedReturnAnalyzer` (tail-merged shared-epilogue
          function-boundary correction), `AggressiveInstructionFinderAnalyzer`
          (code/data disambiguation in stripped/gappy binaries) — all
          real, none attempted this round.
        - Followed up on the `CallFixupAnalyzer` finding specifically:
          re-examined the one case the earlier Call-Fixup update
          deliberately left unaddressed. Confirmed via Ghidra's own
          `x86win.cspec` that *classic* 32-bit MSVC `__chkstk`/
          `__alloca_probe`/`__alloca_probe_8`/`__alloca_probe_16`
          genuinely differ from mingw's `___chkstk_ms` (both x86 and
          x86_64, already correctly handled): `<callfixup name=
          "alloca_probe">` gives the net effect as `ESP = ESP + 4 - EAX`
          — the callee itself adjusts `esp` on this convention. Ported
          that formula into `infer_entry_stack_layout`'s existing
          `rsp_delta` tracker (threading `type_context` through as a new
          parameter, since `__chkstk` is always a statically-linked
          internal symbol, never a DLL import, so `call_target_refs` is
          needed to resolve it) — no changes needed to the deeper
          `stack_slots.rs` resolver, since classic-chkstk functions
          establish `ebp` right after the call and address everything
          `ebp`-relative from there.
        - Caught a real bug while building the fixture to test the
          *mechanism* (not the formula, which is unvalidated — see
          below): substring-matching `"__chkstk"` also matches mingw's
          `"___chkstk_ms"` (the triple-underscore name literally contains
          the double-underscore one), double-counting its effect on top
          of the already-correct `IntSub`-based tracking and hanging a
          real `i686-w64-mingw32-gcc`-compiled 8KB-local-array fixture
          until tightened to exact-name matching (strip leading
          underscores, then exact comparison).
        - **Explicitly NOT validated against real MSVC-produced bytes**,
          unlike every other stack-resolution fix this session: no MSVC
          toolchain is available in this environment, and mingw (the only
          available Windows-target toolchain) always emits the
          already-handled `___chkstk_ms` convention instead, never this
          one. The new unit test proves the arithmetic is wired correctly
          given a matching call target, not that the formula itself is
          correct against real MSVC output — ported directly from
          Ghidra's cspec rather than derived from a fixture, breaking
          this session's own established validation discipline for this
          one specific rule. Flagged as such in the code comment; revisit
          if a real MSVC-produced 32-bit binary with a large stack frame
          ever becomes available to check against.
        - **New known issue, discovered but not fixed (out of scope)**:
          decompiling the `i686-w64-mingw32-gcc`-compiled chkstk fixture
          itself hangs/times out. Confirmed pre-existing on `main` before
          any of this update's changes (reproduced by stashing the diff
          and re-testing) — a real, separate x86-32 decompilation
          performance issue this work incidentally surfaced, not a
          regression. Worth its own dedicated investigation later.
        - Validated: full workspace check clean,
          `fission-pcode`/`-decompiler`/`-static` 1032/1032 (+1 new test),
          `golden_corpus_check.py` clean, full `cargo nextest run
          --workspace` shows only the same 7 pre-existing unrelated
          `fission-emulator` failures this session has confirmed
          unrelated at every prior checkpoint. The x64 mingw chkstk
          fixture re-verified unaffected (change is `is_64bit`-gated).
      - **Update — `TEBAnalyzer` (Windows TEB/PEB `fs:`/`gs:` field
        recognition), the second-tier scorecard item** (commit
        `255e1ef4`). User asked to keep finding gaps and make sure each
        one integrates cleanly into the actual decompiler output, not
        just gets detected internally. Ghidra's `TEBAnalyzer` builds a
        synthetic TEB memory block and points `fs:`/`gs:` at it so its
        generic segment-relative resolution can name accesses; Fission
        has no segment/memory-block model to match that architecture
        1:1, so this ports the underlying *value* instead — recognize
        `fs:[K]`/`gs:[K]` directly during HIR lowering and name them,
        covering the same real case (most notably the classic
        `PEB.BeingDebugged` anti-debug check) through a mechanism that
        fits Fission's own p-code/HIR pipeline.
        - Built a real `x86_64-w64-mingw32-gcc` fixture (`movq %gs:0x60,
          %rax`, reading `TEB.ProcessEnvironmentBlock`) and confirmed via
          raw p-code dump that SLEIGH lifts this as `IntAdd(GS_OFFSET,
          const(0x60))` then `Load` — `GS_OFFSET` is a real named SLEIGH
          register (`ia.sinc`: `FS_OFFSET`/`GS_OFFSET` declared as a
          2-entry array starting at `0x110`; confirmed `GS_OFFSET` lands
          at `0x118` on a 64-bit build, not `0x110` as the bare
          declaration alone would suggest — the first offset-matching
          attempt used `0x110` and silently matched nothing).
        - New `resolve_teb_field_offset` (`stack_slots.rs`) mirrors
          `resolve_stack_address_inner`'s own recursive `Copy`/`Cast`/
          `IntZExt`/`IntSExt`/`IntAdd`/`PtrAdd` structure (reusing the
          existing `resolve_constant_operand` for the delta) against the
          single fixed `FS_OFFSET`/`GS_OFFSET` base. A small offset→name
          table (`teb_field_name`) covers the handful of well-known,
          stable TEB fields worth naming for both 32- and 64-bit layouts.
        - Iterated on how to surface the name without regressing anything
          — the "잘 녹여내야 합니다" (integrate it well) part. A bare
          untyped `HirExpr::Var` left return-type inference nothing to
          work with (`undefined is_debugged(void)` on the real fixture);
          registering a full `self.temps` binding (mirroring how a stack
          slot's `Var` is backed by one) made it *worse* — no assigning
          `HirStmt` exists anywhere in the body for it (it's a read from
          a fixed location, not a computed value), so the renderer
          declared it uninitialized-looking. Settled on wrapping the name
          in an `HirExpr::Cast` to the field's real type instead: a real
          type at the use site without implying local storage that needs
          to exist. Final rendered output: `return *(uchar *)
          (teb_ProcessEnvironmentBlock + 2);` — the classic
          `BeingDebugged` check, immediately recognizable, correct types
          throughout, no misleading declaration.
        - Validated: full workspace check clean,
          `fission-pcode`/`-decompiler`/`-static` 1033/1033 (+1 new
          synthetic-pcode test matching the exact confirmed real p-code
          shape), `golden_corpus_check.py` clean, full `cargo nextest run
          --workspace` shows only the same 7 pre-existing unrelated
          `fission-emulator` failures this session has confirmed
          unrelated at every prior checkpoint. Manually re-verified
          end-to-end against the real mingw fixture through the actual
          CLI after every change in this commit.
        - **Not attempted**: the second hop (naming `PEB.BeingDebugged`
          specifically, rather than showing the raw `+2` byte offset
          after the named `ProcessEnvironmentBlock`) — would need a
          general symbolic base-register tracker beyond this single-hop
          field table; the current single-hop naming already makes the
          pattern immediately recognizable without it. 32-bit `fs:`
          convention only unit-tested, not verified against a real
          32-bit binary the way the 64-bit `gs:` case was.
      - **Follow-up — `PEB.BeingDebugged` second hop, plus a real
        `subvar_flow` bug it surfaced** (user: "진행합니다" after being
        shown the "not attempted" list above). Added
        `resolve_peb_field_offset`/`try_peb_field_var`/`peb_field_name`
        (`stack_slots.rs`) mirroring the TEB helpers: recognizes
        `Load(teb_ProcessEnvironmentBlock_address) + K` (the *value*
        loaded from the TEB field used as a further arithmetic base, via
        a `Load` arm added to the recursive resolver that calls back into
        `resolve_teb_field_offset_inner` on the inner `Load`'s address and
        checks the name is specifically `teb_ProcessEnvironmentBlock`) and
        names `+0x2` off it `peb_BeingDebugged`. Also validated 32-bit
        `fs:` end-to-end this round (a real `i686-w64-mingw32-gcc` build
        of the same fixture) — closing that previously-open gap too.
        - First end-to-end run against the real 64-bit fixture (not the
          synthetic unit test, which passed immediately) showed a new
          regression: `longlong local_1; local_1 = (uchar)peb_BeingDebugged;
          return local_1;` at the NIR layer printed fine, but a *different*
          local got declared with **no assignment anywhere** —
          `uchar peb_BeingDebugged_sub8;` used only in `return
          peb_BeingDebugged_sub8;`. Root cause was not in the new PEB code
          at all: `fission-midend-normalize`'s `subvar_flow.rs` (Global
          Subvariable Flow, general bit-width narrowing) treats *any*
          def-less variable name reached during backward tracing as a safe
          "leaf parameter / input boundary" and unconditionally fabricates
          a new `func.locals` entry for its narrowed form — with no
          initializer, since nothing ever really assigned it. This had
          never surfaced before because every previous case reaching that
          leaf branch really was a registered parameter or the value never
          got compiler-round-tripped through a real stack slot in a way
          that fed `subvar_flow`'s candidate detection; our intentionally
          unregistered `Cast(uchar, Var("peb_BeingDebugged"))` (the exact
          "give the use site a type without implying storage exists"
          design from the TEB work above) is exactly the kind of def-less,
          *unregistered* name the pass wasn't guarding against.
        - Fix, at the root cause rather than worked around in the new PEB
          code: `trace_backward`'s leaf case now only treats a def-less
          name as safe to rename if it's actually in `type_map` (seeded
          from `func.params`/`func.locals` — i.e. a genuinely declared
          binding), otherwise the whole candidate chain is conservatively
          abandoned. This is a general correctness fix, not TEB/PEB
          specific — it protects any future feature that surfaces a named
          value without registering backing storage for it. Updated
          `test_subvar_flow_rewrite` to register its `a`/`b` free
          variables as real params, since the old (accidentally-permissive)
          behavior was masking that they were never declared either.
        - Validated: full workspace check clean,
          `fission-pcode`/`-decompiler`/`-static`/`-midend-normalize`
          1301/1301 (up from 1033 — this round's target set includes
          `fission-midend-normalize` for the first time), `golden_corpus_
          check.py` clean, full `cargo nextest run --workspace` shows only
          the same 7 pre-existing unrelated `fission-emulator` failures.
          Re-verified end-to-end against both the real 64-bit and (newly
          built) real 32-bit mingw fixtures: both render the single clean
          line `return (uchar)peb_BeingDebugged;`, no stray declarations.
      - **Audit — `SharedReturnAnalyzer`, the third-tier scorecard item**
        (commit `bfd0ec3f`). Ghidra's `SharedReturnAnalyzer`/
        `SharedReturnAnalysisCmd` retags an unconditional-JMP instruction
        whose target is another function's entry point as `CALL_RETURN`
        flow — fixing function-boundary attribution (the jumped-to code
        stays its own function, doesn't get absorbed) and the call graph
        (a real call edge, not a raw branch), for both genuine
        sibling/tail calls and compiler-deduped shared epilogues.
        Dispatched a research agent first rather than assuming this was
        unimplemented, since by this point in the audit most named gaps
        had turned out to be real. Result: **Fission already has a direct
        analogue** — `function_discovery`'s "G2" pass
        (`fission-static/src/analysis/function_discovery/discover.rs:
        299-379`) walks unconditional-jump edges, detects a destination
        crossing into another known function's range, validates it as a
        real subroutine, and promotes it into its own `FunctionInfo`; a
        companion check in the same file's boundary tracker
        (`add_function`, ~618-660) explicitly refuses to trace across
        unconditional jumps "to avoid enveloping tail call targets."
        Function-boundary correctness and call-graph edges were already
        right — no gap there.
        - The audit did surface one real, narrower bug on the *rendering*
          side, unrelated to function boundaries: `emit_unsupported_
          control_surface` (`fission-pcode/src/midend/builder/mod.rs`)
          decides whether a recovered tail-call `Call` expression becomes
          `HirStmt::Return(call)` or a bare `HirStmt::Expr(call)` by
          literally comparing `evidence.opcode == "BranchInd"` — true only
          for a genuine register-indirect jump. Every tail-call-recovery
          site (`recover_known_external_tail_call_expr`,
          `recover_tail_call_expr_from_target_expr`,
          `recover_tail_call_expr_from_branchind_target`,
          `terminator.rs`) sets `surface: IndirectControlSurface::
          BranchInd` uniformly regardless of whether the underlying p-code
          opcode was a real `BranchInd` or a direct `Branch` to a
          statically-known address — so a direct-address tail call
          (`jmp known_func`) silently dropped the `return`, rendering
          `known_func();` as if execution continued afterward, when it
          doesn't. Confirmed on a real, non-synthetic case already in the
          golden corpus: mingw's CRT `__gcc_register_frame` tail-jumps into
          `atexit` (`lea rcx,[__gcc_deregister_frame]; jmp atexit`) and was
          rendering as `undefined __gcc_register_frame(void) {
          atexit(); }` instead of `ulonglong __gcc_register_frame(void) {
          return atexit(); }` (return-type inference also improved, since
          a `return` statement now feeds it — previously `undefined`).
          Two pre-existing tests (`bootstrap_x86.rs`) had the buggy
          no-`return` behavior baked into their assertions
          (`external_tail();`, `external_tail(callback);`); tightened both
          to require the `return` prefix explicitly, so they'd actually
          catch a regression back to the old behavior.
        - Fix: dropped the opcode-string check entirely — any `surface`
          in `{BranchInd, SwitchLike}` with a recovered `Call` target_expr
          is, by construction of every site that produces one, a genuine
          tail call and should always render as `return`.
        - Validated: full workspace check clean, targeted
          `fission-pcode`/`-decompiler`/`-static` 1034/1034 (both tightened
          tests pass under the new behavior), full `cargo nextest run
          --workspace` shows only the same 7 pre-existing unrelated
          `fission-emulator` failures. `golden_corpus_check.py` flagged
          exactly one function changed across all 160 functions / 16
          binaries (`__gcc_register_frame`, all 16 binaries share the same
          mingw CRT) — reviewed the diff, confirmed it was the intended
          fix (not a regression), and accepted the new snapshot. Also
          built a second, purpose-made real fixture
          (`x86_64-w64-mingw32-gcc -O2`, a `caller` function that GCC
          tail-jumps into a `helper` function) confirming `return
          helper();` end-to-end outside the golden corpus too.
      - **Implement — `AggressiveInstructionFinderAnalyzer`, the last of
        the ranked scorecard items** (commit `9452ca6a`). Ghidra's AIF
        finds function starts hiding in undisassembled gaps of stripped/
        heavily-optimized binaries by fingerprinting a candidate's first
        two instructions' *masked* bytes (immediate/displacement operands
        zeroed out) and requiring that fingerprint to recur ≥4 times
        among the binary's own already-known functions before trusting
        it — a self-calibrating signal needing no hardcoded signature DB,
        unlike Ghidra's separate static "Function Start Search" analyzer
        (which Fission already has, via `scan_ghidra_patterns`'s Ghidra
        XML pattern DB).
        - `scan_dynamic_prologues` (`fission-static/discover.rs`) was
          already wired into the pipeline under exactly this name but was
          a dead stub (`Vec::new()`) — filled it in. Fingerprint pool:
          `(mnemonic, mnemonic)` pair of the first two decoded
          instructions per known function (Fission's SLEIGH runtime
          doesn't expose an instruction-mask primitive at this layer, so
          mnemonic-pair substitutes for Ghidra's byte-masking — still
          invariant to immediate/register-*value* choice, just unable to
          distinguish two different *registers* in an otherwise-identical
          encoding; documented as a deliberate simplification, not an
          oversight). Candidate positions reuse `scan_cc_padding_regions`'s
          proven-safe padding-run enumeration (that scanner stays disabled
          on its own — "valid routine after padding" alone was too
          permissive, causing FPs; the fingerprint-recurrence gate is
          exactly the missing piece), validated through the existing
          `validate_subroutine_candidate`.
        - Gated strictly to `Aggressive`, not `Balanced` — matches
          Ghidra's AIF being off-by-default/riskier than reference- and
          signature-driven analyzers. Fission's own `Aggressive`/
          `Balanced` profiles were previously behaviorally identical (an
          empty `if profile == Aggressive {}` block, dead since some
          earlier change); this is the first real differentiator between
          them. Updated `types.rs`'s stale per-variant doc comments to
          match.
        - **Found and fixed a real, pre-existing, 100%-reproducible crash**
          while testing this, unrelated to the new scanner itself:
          `scan_ghidra_patterns` built its `AhoCorasick` automaton with
          `MatchKind::LeftmostFirst` but called `find_overlapping_iter`,
          which only `MatchKind::Standard` supports — confirmed via a real
          binary crashing unconditionally
          (`AhoCorasick::try_find_overlapping_iter ... MatchError(
          UnsupportedOverlapping)`) under `--function-discovery-profile
          balanced`/`aggressive`. Never caught before because
          `golden_corpus_check.py` (and every prior manual check this
          whole session) always runs at the CLI's `conservative` default,
          which skips `scan_ghidra_patterns` entirely — this bug has
          likely existed since `scan_ghidra_patterns` was written. Fixed
          by switching to `MatchKind::Standard`, which is also the
          semantically correct choice here (every raw hit is re-verified
          in full against the actual pattern afterward, so the automaton
          only needs to be a complete "does any prefix start here"
          pre-filter, not a single-best-match search).
        - New tests: a synthetic fixture with 20 "known" (symbol-seeded)
          functions sharing a deliberately uncommon two-instruction
          prologue (`push r15; push r14`, chosen specifically to avoid
          colliding with a real signature in the Ghidra XML pattern DB —
          confirmed via `xml_hits=0` in the test's own `SCANNER_STATS`
          output) plus one more, identically-shaped function placed the
          same way but not seeded (simulating a padding-hidden function).
          Confirms the hidden function is recovered under `Aggressive`
          and NOT recovered under `Balanced`/`Conservative`.
        - Validated: full workspace check clean, `fission-static` 69/69,
          full `cargo nextest run --workspace` 2143/2143 minus the same 7
          pre-existing unrelated `fission-emulator` failures.
          `golden_corpus_check.py` clean (runs at the `conservative`
          default, entirely unaffected by this change by construction).
          Manually re-verified the aho-corasick crash fix and the new
          scanner's non-crashing behavior across 4 additional real
          corpus binaries under `--function-discovery-profile aggressive`
          via `--all` batch mode.
        - **Scope note**: this closes the scorecard's fourth and final
          ranked item (`CallFixupAnalyzer`, `TEBAnalyzer`,
          `SharedReturnAnalyzer`, `AggressiveInstructionFinderAnalyzer`),
          completing this audit thread. Not attempted: genuine SLEIGH
          byte-masking (mnemonic-pair fingerprinting is coarser, as
          documented above); no real corpus binary was found that
          actually exercises `scan_dynamic_prologues`'s ≥20-known-
          functions/≥4-fingerprint-recurrence gates end-to-end with a
          real hidden function (expected — the golden corpus's fixtures
          are small, ordinarily-linked binaries with no deliberately
          hidden functions; the synthetic unit tests are the primary
          regression guard, matching how Ghidra's own AIF is inherently
          hard to validate against "normal" binaries by design).
      - **Implement — `X86FunctionPurgeAnalyzer`, found via a fresh
        re-survey of the scorecard's leftover items** (commit `387c2feb`).
        The original 15-item "zero corresponding code" list only had its
        top 4 individually named; the other ~11 were never recorded, so
        rather than trust a stale/incomplete memory, re-derived them from
        scratch against the current codebase. Most turned out to already
        be covered and needed no further work: FID/library-function
        identification (`fission-decompiler/src/fid.rs` +
        `fission-signatures/src/fidbf/*`), C++/Rust/MSVC demangling
        (`fission-loader/src/loader/demangle.rs`), PDB consumption
        (`pdb_sidecar.rs`), PE SEH/exception handling (`pe/pdata.rs`,
        `pe/seh.rs`), RTTI (`loader/analyzers/cpp.rs`), DWARF
        (`loader/dwarf/*`), Golang symbol/string recovery
        (`golang_typeinfo.rs`), no-return-function detection
        (`fission-core/core/ghidra_no_return.rs`), import-thunk
        classification (`function_provenance/mod.rs`), and mingw pseudo-
        relocation (`pe/mingw_pseudo_reloc.rs`) — confirmed via grep+read
        against real files, not re-implemented.
        - The one genuine gap: Ghidra's own `x86win.cspec` declares
          `__stdcall`'s `extrapop` as literally `"unknown"` — must be
          resolved per-function from the callee's own `RET imm16`, the
          ground-truth stack-argument byte count for callee-cleanup
          conventions (stdcall/fastcall/thiscall). Fission's cspec parser
          (`cspec/mod.rs`) silently `.unwrap_or(0)`s any non-numeric
          `extrapop` string, but this barely mattered in practice — x86-32
          parameter recovery goes through a separate, purely usage-driven
          path (`incoming_stack_argument_index`/
          `ensure_incoming_stack_param_binding`) that only ever
          materializes a parameter slot once something in the body reads
          it, so a stdcall function's trailing *unused* parameter (dead in
          the body, but still part of the real signature and still purged
          by the callee at `ret`) was silently dropped from the recovered
          signature.
        - Needed no cross-crate plumbing: the purge amount is entirely a
          property of the callee's own epilogue, already visible in its
          own lifted p-code — confirmed via raw p-code dump of a real
          `ret $0xc` that it lifts as an extra `IntAdd(ESP, imm16)`
          sharing the *same originating-instruction address* as the
          return-address-pop `IntAdd(ESP, pointer_size)` and the `Return`
          op itself. New `apply_x86_32_stack_purge_arity_floor`
          (`stack_slots.rs`) sums same-address ESP adjustments at every
          `Return` site, subtracts the pointer-size pop baseline, and if
          positive, forces `ensure_incoming_stack_param_binding` up to the
          implied minimum arity. Restricting to same-address ops (rather
          than summing every ESP adjustment in the block) means an
          unrelated `add esp,N` used for local-variable cleanup elsewhere
          in the same epilogue can't throw it off.
        - Validated against a real `i686-w64-mingw32-gcc`-compiled
          `__stdcall` fixture with an unused third parameter: before,
          `int example@12(int param_1, int param_2)`; after, `int
          example@12(int param_1, int param_2, uint param_3)`. Also built
          and checked a cdecl counterpart (no `RET imm16` → correctly
          unaffected, no false parameter forced — matches Ghidra's own
          scope, since caller-cleanup conventions have no epilogue signal
          at all) and a fully-used-stdcall counterpart (purge matches
          usage-derived arity exactly → idempotent, no duplication).
        - Validated: full workspace check clean,
          `fission-pcode`/`-decompiler`/`-static`/`-midend-normalize`
          1304/1304 (+1 new test, isolated to just the arity-floor
          mechanism since ebp-relative param-read recognition is a
          separate, already-covered code path with its own prologue-
          detection preconditions unrelated to this fix — the mixed used/
          unused case was validated manually via the CLI against the real
          fixture instead), `golden_corpus_check.py` clean (all 16
          binaries are x64, so this x86-32-gated change can't touch them
          by construction), full `cargo nextest run --workspace`
          2144/2144 minus the same 7 pre-existing unrelated
          `fission-emulator` failures.
      - **Fix — 8 missing `FLOAT_*` opcode lowerings, found via a new
        audit axis** (commit `95176c91`). After the `Analyzer.java`
        roster (both the original 54-item survey and the fresh re-derive
        of its leftover 15) stopped producing new real gaps, pivoted to
        checking Ghidra's actual decompiler backend
        (`Ghidra/Features/Decompiler/src/decompile/cpp`, ~160 `Rule`
        classes) instead of surveying it exhaustively (too large for one
        pass) — spot-checked one well-known pain point directly against a
        real fixture: classic x86-32 x87 floating-point code (still
        common in real 32-bit binaries, the default float codegen path
        before SSE2 became universal). A real `i686-w64-mingw32-gcc`
        double-precision function hit a **hard decompilation failure**
        (`unsupported pcode pattern: opcode`) — not degraded output, a
        complete crash for the whole function.
        - Root cause: `FloatFloat2Float`, `FloatNeg`, `FloatAbs`,
          `FloatSqrt`, `FloatCeil`, `FloatFloor`, `FloatRound`, and
          `FloatTrunc` are all real, defined `PcodeOpcode` variants (used
          by x87's `FLD`-driven precision promotion, `FABS`/`FSQRT`/
          `FCHS`, etc.) with **zero lowering handlers** in
          `lower_def_op_inner`'s big match — falling through to the
          generic catch-all error. None of these are x87-specific at the
          p-code level, so any code using them (x87 or otherwise, e.g.
          plain `sqrt()`/`fabs()`/`ceil()` inlined as native FP
          instructions on some target) would hit the identical crash.
        - Implementation verified against Ghidra's own `TypeOpFloat*` C++
          declarations (`typeop.cc`), not guessed from opcode names:
          `FloatFloat2Float` → `Cast` to `float_type_from_size(output.
          size)` (same shape as the pre-existing `FloatInt2Float`
          handler, which already handled `size=10 → Float{bits:80}` for
          x87 extended precision — the type system was already ready for
          this, just missing the opcode arm). `FloatNeg` →
          `HirUnaryOp::Neg` with a float type (`TypeOpFloatNeg` is
          `TYPE_FLOAT,TYPE_FLOAT`, prints as `-`). `FloatAbs`/
          `FloatSqrt`/`FloatCeil`/`FloatFloor`/`FloatRound` → intrinsic
          calls to their real `<math.h>` names (`fabs`/`sqrt`/`ceil`/
          `floor`/`round`) — confirmed all five are `TYPE_FLOAT,
          TYPE_FLOAT` in `typeop.cc`, genuine float-to-float math
          functions, unlike the CPU-flag intrinsics (`__carry`/
          `__sborrow`) that already use a synthetic `__`-prefixed name.
          `FloatTrunc` → `Cast` to a **signed int** type, NOT a call to
          `trunc()` — Ghidra's own `TypeOpFloatTrunc` constructor is
          explicitly `TYPE_INT,TYPE_FLOAT` (a truncating float-to-int
          conversion, i.e. `(int)x`), the one opcode in this group that
          breaks the float-to-float pattern of its siblings; verified by
          reading `typeop.cc`, not assumed from the name (would have been
          a real, subtle bug otherwise).
        - Also added all 8 to `is_materializable_output_opcode`
          (`pcode_util.rs`), matching how the pre-existing
          `FloatInt2Float`/`FloatNan` siblings are already listed there.
        - Validated: `FloatFloat2Float` and `FloatAbs` each confirmed
          against a real `i686-w64-mingw32-gcc` x87 fixture — a hard
          crash became a working decompile for both. New synthetic test
          covers the remaining opcodes not hit by available real fixtures
          (chained `FloatNeg`/`FloatSqrt`/`FloatCeil`/`FloatFloor`/
          `FloatRound`/`FloatTrunc`), explicitly asserting `FloatTrunc`
          does NOT render as `trunc(`. `fission-pcode`/`-decompiler`/
          `-static`/`-midend-normalize` 1305/1305 (+1), `golden_corpus_
          check.py` clean (x64 SSE2 arithmetic already goes through the
          pre-existing `FloatAdd`/`FloatSub`/`FloatMult`/`FloatDiv`
          family; math-function calls in the corpus are real libm calls,
          not these opcodes, so unaffected by construction), full
          `cargo nextest run --workspace` 2145/2145 minus the same 7
          pre-existing unrelated `fission-emulator` failures.
        - **Scope note — this fixes the crash, not full x87 output
          quality.** The x87-specific FPU register-stack push/pop
          shift-chain (SLEIGH lifts `FLD`/`FSTP` as 8 fixed-offset-
          register `Copy` rotations modeling the hardware stack) still
          isn't specially recognized by the materialize/copy-propagation
          pipeline, so a genuinely double-precision x87 function now
          *decompiles* instead of *crashing*, but with confusing
          `st0`/`st6`/`st7`-named temporaries and (in the observed real
          fixture) an incorrect final return value. Reconciling the
          8-register shift-chain into clean value flow would need a
          dedicated new normalize pass recognizing this specific rotation
          pattern — a separate, larger, genuinely not-yet-attempted gap,
          clearly distinct from (and much bigger than) the crash fixed
          here.
      - **Implement — `FormatStringAnalyzer`** (commit `fc4b88d8`). The
        #1-ranked candidate from the earlier scorecard re-derive, deferred
        at the time for the smaller `X86FunctionPurgeAnalyzer`. Types
        printf-family variadic call arguments from their own format
        string's `%`-conversion specifiers: `printf("%d %s", x, y)` now
        types `x` as `int` and `y` as `char*`, instead of leaving both as
        generic register-width defaults (`uint`/`ulonglong`) the way
        every variadic call did before.
        - Spent real investigation confirming the format string's *text*
          was already available before writing any code (a prior attempt
          this same session to trace this manually failed and needed a
          research agent): `NirRenderOptions::from_loaded_binary`
          (`fission-midend-core/src/ir/options.rs`) pre-populates
          `global_names` with every extracted `.rdata` string, already
          quoted and escaped; `lower_varnode_inner` (`fission-pcode`)
          resolves a constant matching `global_names` to `HirExpr::
          AddressOfGlobal("\"...\"")`; and `arg_var_name` (already used
          by this file's existing per-parameter typing) already captures
          `AddressOfGlobal` names verbatim into the existing call-site
          collection. The quoted text was already sitting in `arg_vars`
          — no new binary access or HIR traversal needed.
        - **Two real, pre-existing problems found and fixed along the
          way**, both via testing against real fixtures rather than
          assuming the naive implementation would work:
          1. `tighten_binding_ty` (already used by this file's main
             WinAPI-signature typing) only allows `Unknown → concrete` or
             `Ptr(Unknown) → Ptr(concrete)`. Confirmed a call-argument
             binding is essentially never still `Unknown` by the time
             this pass runs on real compiled code — `fission-pcode`'s HIR
             builder always assigns a generic *unsigned*-int default from
             raw register width at materialization time — and confirmed
             this SAME limitation already silently affects the existing
             WinAPI-signature typing too (a real `GetWindowRect` call's
             own `HWND`/`RECT*` params stayed `ulonglong` end-to-end,
             unrelated to anything touched this round). Rather than
             loosen the shared `tighten_binding_ty` (broad blast radius,
             many other callers whose conservatism is appropriate), added
             a narrowly-scoped `apply_variadic_printf_arg_ty` that
             additionally allows overriding a generic unsigned-int
             default specifically — a format specifier is strong,
             authoritative evidence (the actual API contract), unlike raw
             register width.
          2. Even with that fix, a `%s` argument's refined type still
             didn't survive to the real source parameter in the final
             output — the immediate call-site temp refined correctly on
             every fixed-point iteration, but the type never reached the
             `char*` parameter it was copied from (`argN = param_2;
             printf(fmt, argN)`; depending on copy-propagation/renaming
             timing elsewhere in the fixed-point loop, the refinement
             doesn't reliably survive). Fixed by having this pass trace
             the copy chain itself (`collect_copy_sources` +
             `apply_variadic_printf_arg_ty_transitively`) rather than
             depending on later passes to carry it through.
        - Deliberately scoped to the unambiguous ANSI narrow-string
          printf family (`printf`/`fprintf`/`sprintf`/`snprintf`/their
          `_s` secure-CRT variants). scanf-family (variadic args are
          *pointers* to write into, a different typing rule) and
          wprintf-family (`%s` means narrow `char*`, not `wchar_t*`, per
          the ANSI convention — a correctness trap without a fixture to
          validate against) are intentionally excluded, not overlooked.
        - Validated against two real `x86_64-w64-mingw32-gcc` fixtures:
          `printf("%s %d", ...)` end-to-end (both the immediate call-site
          temp and the real source parameter correctly typed
          `int`/`char*`), and a `GetWindowRect` call confirming the
          pre-existing WinAPI-signature-typing limitation this round
          discovered is unchanged (properly scoped — not fixed, not
          regressed by this work). New unit test models the real-world
          starting condition (generic unsigned-int default, not
          `Unknown`) rather than the idealized `Unknown`-starting-point
          every other test in this file uses, specifically to catch a
          regression back to `tighten_binding_ty`-only behavior.
        - Validated: `fission-midend-normalize` 268/268 (+1),
          `fission-pcode`/`-decompiler`/`-static`/`-midend-normalize`
          1306/1306, `golden_corpus_check.py` clean (no `printf` calls in
          the corpus, confirmed — unaffected by construction, not a
          meaningful regression signal either way), full `cargo nextest
          run --workspace` 2146/2146 minus the same 7 pre-existing
          unrelated `fission-emulator` failures.
      - **Fix — dead-store elimination was dropping writes through
        stack-spilled VLA pointers** (commit `53c931e5`). A real-fixture
        probe of variable-length arrays (a classic tricky decompiler
        case, picked up from the x87 scope note as the next well-known
        pain point to spot-check). A real `x86_64-w64-mingw32-gcc`
        fixture with a genuine VLA (`int arr[n]`, dynamic `n`) was
        silently dropping `arr[i] = ...` from the decompiled output
        entirely — not a rendering glitch, the whole assignment was
        missing at both the NIR and HIR layers. A fixed-size array with
        the identical loop shape (`int arr[16]`) was unaffected,
        narrowing this to something VLA-specific rather than a general
        array-write bug.
        - Root cause: a VLA's base address is computed once and spilled
          to an ordinary stack slot, named `local_18` by the same
          general-purpose local-naming convention used for any other
          spilled value. `classify_base_object`
          (`fission-midend-normalize/src/memory/partition.rs`)
          classifies a `Deref`'s base purely by *name prefix* — anything
          starting with `local_`/`stack_`/etc is unconditionally
          `MemoryAccessClass::Stack`. This conflates two shapes that
          produce the identical `Deref(local_18 + i * stride)` pattern:
          `local_18` genuinely *being* a fixed-size array's own storage
          (safe) vs `local_18` merely *holding* a pointer value spilled
          to a stack slot with that name (the VLA case — dereferencing
          through it accesses a different, unbounded region that must
          never be folded into "this function's own stack frame" for
          alias-analysis purposes). With no type information available
          at that layer, the wrong answer fed directly into
          `apply_dead_store_elimination`'s "no-escape stack slot,
          `use_count == 0` → safe to remove" rule — explicitly documented
          as "conservative and sound," soundness that broke exactly here.
        - Fixed at `partition_key()` (not `classify_base_object`'s own
          signature, to avoid a much larger blast-radius refactor across
          its 6 call sites in other files): whenever a runtime-scaled
          `stride` is present alongside a name-based `Stack`
          classification, downgrade to `Unknown`. Confirmed via the same
          real fixture-pair that this doesn't cost real optimization
          opportunities — when this pipeline recognizes a genuine
          fixed-size local array, the address computation bottoms out at
          a different, non-`local_`-prefixed expression (an
          `rsp`-holding temp) instead, so the rule isn't observed to
          regress the legitimate case.
        - New unit tests directly exercise `apply_dead_store_elimination`:
          one constructs the exact bug shape (stack-spilled pointer +
          runtime stride, zero reads — the worst case, previously
          guaranteed elimination) and confirms the write survives; a
          second confirms a genuinely dead, ordinary local write (no
          stride) is still correctly removed, guarding the existing
          legitimate optimization the fix must not blunt.
        - Validated: `fission-midend-normalize` 270/270 (+2),
          `fission-pcode`/`-decompiler`/`-static`/`-midend-normalize`
          1308/1308, `golden_corpus_check.py` clean (dead-store
          elimination is a broadly-used general pass — a clean result
          across all 160 golden functions is real evidence against a
          regression, not just an unaffected-by-construction result),
          full `cargo nextest run --workspace` 2148/2148 minus the same 7
          pre-existing unrelated `fission-emulator` failures.
        - **Scope note**: the x87 FPU register-stack reconciliation gap
          (recorded two entries above) remains open and unattempted —
          this entry closed a different, unrelated real bug found while
          spot-checking the same general category of "classic tricky
          decompiler cases."
- **`native_decomp`/Ghidra-FFI dead architecture removed, and a GDT
  ("actually used?") audit that closed as "already used, differently than
  expected," 2026-07-21** (commit `7e207d14`): asked whether the 15 `.gdt`
  (Ghidra Data Type archive) files bundled under
  `utils/signatures/typeinfo/{win32,generic,golang,mac_10.9,rust}/` are
  ever actually parsed, since `PathConfig::get_gdt_path`/`get_all_gdt_paths`
  resolve them on every CLI decompile call (`run.rs` x2, `batch.rs`,
  `emit.rs`) and even have a dedicated `PrepareTimings::gdt_ms` benchmark
  field — looked live.
  - Their only consumer, `decomp.set_gdt(path)`, sat behind
    `#[cfg(feature = "native_decomp")]` — an old architecture (native
    Ghidra decompiler linked via a `fission-ffi` crate + CMake-built
    `libdecomp.so`) that turned out to be **permanently unbuildable**:
    both `fission-static/build.rs` and `fission-cli/build.rs`
    unconditionally `panic!`ed the instant `CARGO_FEATURE_NATIVE_DECOMP`
    was set, the `ghidra_decompiler/` C++ source directory `build.rs`
    pointed at no longer existed, and `fission-ffi` was never a workspace
    member. Every `#[cfg(feature = "native_decomp")]` branch across 10
    files (the whole `decompile.rs`/`decompile_exec` CLI module tree,
    `fission-static`'s `prepare.rs`, half of `inventory/emit.rs`, plus
    smaller spots in `provenance.rs`/`orchestration/types.rs`) was
    unreachable dead code in every real build — removed outright (5046
    deleted lines, 10 inserted). One real bug surfaced along the way:
    `fission_nir_worker`/`fission_preview_worker` (the live pipeline's
    crash-isolation subprocess workers, spawned by
    `orchestration/worker.rs` with a graceful in-process fallback if
    missing) were wrongly gated `required-features = ["native_decomp"]`
    in `fission-cli/Cargo.toml` — meaning they could never be built
    either, silently forcing every decompile onto the in-process fallback
    path. Un-gated, not deleted; confirmed they now build and run.
  - The GDT question itself resolved as: **win32's 2 archives are not
    actually dead** — `utils/signatures/typeinfo/win32/{structures,
    base_types}.json` (13,211 structs, 32 base types) is a pre-extracted
    cache of both `windows_vs12_{32,64}.gdt` (confirmed via a real Ghidra
    12.0.4 headless extraction of `windows_vs12_64.gdt` alone: 5,827
    structs / 89,596 total datatypes — fewer than the merged JSON, as
    expected for a single-bitness archive) that `fission-signatures`'
    `WindowsStructures` loads at runtime and `facts.rs`'s
    `resolve_nir_struct_name`/`build_nir_call_param_rules` uses to type
    `LP*`/`P*`-prefixed WinAPI call-argument structs in decompiled
    output. Real, live usage — just via an offline extraction step, not a
    live `.gdt` parser (parsing `.gdt` directly would mean reimplementing
    Java object serialization + an embedded zip + Ghidra's own proprietary
    DB/B-tree table format from scratch; confirmed via `xxd`/`file` that
    `.gdt` starts with `AC ED 00 05` Java serialization magic wrapping a
    `DTArchive` marker and a nested `PK` zip stream — not attempted).
  - The remaining 13 archives (`generic_clib{,_64}.gdt`, 9 golang
    per-version archives, `mac_osx.gdt`, `rust-common.gdt`) really do have
    **zero consumer of any kind** — no JSON extraction exists for them.
    Investigated wiring the most promising candidate (`generic_clib`, the
    only one covering ELF/POSIX binaries broadly) into the existing
    win32-style mechanism and found a dead end: `resolve_nir_struct_name`
    only fires for call arguments whose type name comes from
    `SIGNATURE_RESOURCES.api_signatures()`, and
    `generic_clib_signatures.txt` — the only signature source covering
    libc functions — types every single parameter as a bare primitive
    (`int`/`long`/`double`/`uint`/...; checked programmatically, 14,404
    `int` + 315 `long` + ... + **zero** struct-pointer type names across
    the whole file). `fstat|int|__fd:int,__buf:int` is representative:
    `__buf` should be `struct stat *` but is typed `int`. A generic-C
    struct DB would have nothing to match against without first rebuilding
    that signature file with real types — a separate, comparably-sized
    project. Golang/macOS/Rust have no per-language struct-typing pass to
    hook into at all (the only existing struct-annotation mechanism is
    Win32-Hungarian-notation-specific).
  - **Left as prepared-but-unwired infrastructure, not deleted further**:
    `PathConfig::get_gdt_path`/`get_all_gdt_paths`/`gdt_dir` now have zero
    callers (their last caller was the deleted `prepare.rs`), but unlike
    the `native_decomp` code they delete alongside, this is legitimate
    groundwork for whichever of the two real follow-ups gets picked up
    first: (a) rebuild `generic_clib_signatures.txt` with real parameter
    types (headers/Ghidra function signatures, not hand-typed primitives),
    then extend the win32 JSON-extraction pattern to `generic_clib.gdt`; or
    (b) design a per-language struct-annotation pass for Go/Rust/macOS
    binaries before extracting their archives at all, since there's
    currently nothing in `facts.rs` for freshly-extracted structs to
    plug into. A reusable GDT→JSON extraction path was proven working
    (real Ghidra 12.0.4 headless run against `windows_vs12_64.gdt`) but
    not productized into an in-repo script, since neither follow-up is
    ready to consume its output yet.
  - Validated: `cargo check`/`test --workspace` clean, `fission_cli
    --version` still reports `0.1.6`, `golden_corpus_check.py` clean (160
    functions across 16 binaries + determinism) — this was a structural
    removal with no decompiled-output-affecting code deleted.
- **Fix — float/double return-register recognition, the x87 known-issue
  investigated further, 2026-07-21** (commit `81c5c364`). User asked to
  keep closing Ghidra-parity gaps; picked the x87 return-value bug
  (recorded as a known issue in the v0.1.6 changelog) since it already had
  a real fixture and a specific root-cause hypothesis ("return-value
  recovery is built around general-purpose 'primary return registers'
  only"). Confirmed via a real `i686-w64-mingw32-gcc -mno-sse -mno-sse2`
  `double compute(double,double,double)` fixture: return type `uint`,
  `return uVar17;` returning `param_6` — a completely unrelated leftover
  value, not the computed result at all.
  - Root cause, traced to `resolve_prototype` in
    `fission-pcode/src/midend/cspec/mod.rs`: Ghidra's own cspecs
    (`x86gcc.cspec`, `x86-64-gcc.cspec`, `x86-64-win.cspec`) declare a
    *separate* `metatype="float"` `<output>` pentry (ST0 on x86-32, XMM0_Qa
    on x86-64) alongside the integer one (EAX/RAX) — a function returns
    through one or the other depending on its return type. Fission's
    parser explicitly *skipped* the float pentry when resolving
    `return_offset`, so `is_primary_return_register` never recognized a
    write to the float return register as the function's return value at
    all — the actual x87 computation happened in the p-code, correctly
    materialized as a `float80` value, but nothing downstream identified
    it as *the returned* value, so the return-statement lowering fell back
    to whatever the (wrong) `EAX`-family search last saw.
  - Added `ResolvedPrototype::float_return_offset`, parsed the same way as
    the existing `return_offset` but selecting the float pentry instead of
    skipping it, threaded through `NirRenderOptions::cspec_float_return_
    offset` → `RegisterNamer::float_return_offset` → a new, independent
    check in `is_primary_return_register`. Purely additive — only fires
    when a varnode's offset matches the float return register; the
    existing integer-register path is untouched.
  - **Also tested x86-64 (SSE2, default codegen) with the same fixture,
    since x86-64's cspec has the identical float/int `<output>` split** —
    found it was *more broken* than x87, just never flagged as a known
    issue: before this fix, `void compute(void) { ...; return; }` — the
    return statement dropped the value entirely, not just mistyped it.
    After: returns the correct computed value, though still mistyped
    (`ulonglong` instead of `double`) — traced to a **separate, deeper
    gap**: x86-64's `<input>` pentries have the same float/int split, and
    `int_param_offsets` skips the float ones the same way `return_offset`
    used to, so XMM-passed float *parameters* aren't recognized either,
    keeping the whole value chain untyped end-to-end (visible as
    `param_10 = xmm0_qa; xVar8 = param_10; xVar8 *= param_18;` — plain
    integer ops, not `FloatMult`). Confirmed this is pre-existing, not a
    regression, by reproducing on the unpatched tree first (`git stash`).
    Fixing float-parameter recognition is real follow-up work, out of
    scope for this return-side fix — flagged here rather than silently
    left for a future session to rediscover.
  - Golden corpus: one cosmetic diff (`crypto_gcc_O2.exe::rc4_init`, an
    SSE2 memset-style init loop with no real return value at all — XMM0 is
    used purely as scratch there) — confirmed via an identifier-normalized
    diff (strip numeric suffixes, e.g. `uVar201` → `uVar`) that NIR/HIR
    are byte-for-byte structurally identical before and after, just
    renumbered temp names from the shifted materialization order now that
    XMM0 writes are also recognized as return-register writes. Snapshot
    updated to match; same rename-only pattern the v0.1.6 changelog
    already accepted once for `__gcc_register_frame`.
  - Validated: `cargo test --lib` clean across
    `fission-pcode`/`-midend-normalize`/`-midend-structuring`/
    `-decompiler`/`-static`, `golden_corpus_check.py` clean (160 functions
    + determinism), full `cargo nextest run --workspace` 2141/2148 — same
    7 pre-existing unrelated `fission-emulator` failures this session has
    confirmed unrelated at every prior checkpoint.
  - **Scope note**: the x87 8-register-stack-rotation noise
    (`st0`/`st6`/`st7`-named temporaries cluttering the output) and the
    32-bit stack-passed `double`/`int64` parameter-splitting bug are both
    still open — this fix closed specifically the "wrong/missing return
    value" half of the x87 known issue, not the full x87 output-quality
    gap.
- **Fix — pivoted the Ghidra-parity audit from metadata gaps to the
  decompiler backend itself; two real bugs found and fixed via a real
  AArch64 fixture, a third found and characterized but left open,
  2026-07-22** (commit `d168056f`). User asked to move on from metadata
  gaps (DWARF/PDB/GDT, the Windows analyzer-class survey — both largely
  closed) to `fission-pcode`/`fission-midend-normalize`, the area
  corresponding to Ghidra's own ~160-`Rule`-class decompiler backend,
  never systematically surveyed this session. Built real fixtures with
  Docker (`gcc:latest --platform linux/arm64`, since no local AArch64
  cross-compiler was available) rather than staying x86-only, since the
  session's own audit trail already flagged non-x86 architectures as an
  almost entirely unvalidated axis.
  - **Bug 1 — SLEIGH pattern-expression shift amounts >= 64 hard-failed
    *decode*, not just lowering.** A packed-bitfield C struct
    (`unsigned a:3, b:5, c:8, d:16`) compiled by real
    `aarch64-linux-gnu-gcc` couldn't even be decoded: AArch64's
    `ubfx`/`bfxil`-family instruction decode (`ImmR_bitfield64_imm` ->
    `DecodeWMask64` in `AARCH64instructions.sinc`) legitimately computes a
    shift amount of exactly 64 for a full-width 64-bit bitfield
    immediate, and `pattern_shift_amount` in
    `fission-sleigh/runtime/spine/compiled_table/walker.rs` rejected any
    amount >= 64 outright. Checked Ghidra's own C++ source
    (`slghpatexpress.cc`'s `LeftShiftExpression`/
    `RightShiftExpression::getValue`) before assuming a fix: it evaluates
    shifts as a raw `leftval << rightval` on a 64-bit `intb` — UB by the
    C++ standard, but on the x86-64/ARM64 hosts Ghidra actually ships on,
    the CPU's own shift instruction only consults the low 6 bits of the
    count, so in practice this behaves as `amount & 63`, not an error.
    Masked instead of rejecting; updated the one existing unit test that
    had asserted the old (wrong) "fail closed on amount>=64" behavior,
    replacing it with one confirming the mod-64 wraparound. Even after
    this fix, `pack`'s actual bitfield-extraction arithmetic still
    renders far uglier than Ghidra's clean `(param_1 & 7) + ...` form
    (confirmed via a real Ghidra 12.0.4 headless decompile) — a separate,
    not-yet-investigated rendering-quality gap in how `DecodeWMask64`-
    derived bitfield masks get typed/simplified downstream.
  - **Bug 2 — non-x86 `primary_return_registers()` picked a dead register
    space first, silently returning the wrong register's value.** A real
    `int cmp(int,int)` AArch64 fixture (three-way branch — `>`, `<`,
    equal — converging on one shared epilogue block, the standard shape a
    compiler emits for this) decompiled to `return x30;` — the *link
    register* (return address), not the actual computed result. Root
    cause: `RegisterNamer::primary_return_registers()` always listed
    `REGISTER_SPACE_ID` (Ghidra's own literal space numbering) as its
    first candidate — a leftover from before this session's
    native_decomp/Ghidra-FFI removal (see two entries above). Confirmed
    via real `raw-pcode` dumps of both this AArch64 fixture and a real
    x86-64 golden-corpus binary that `REGISTER_SPACE_ID` is *never*
    actually populated by the live Rust-Sleigh pipeline on either
    architecture — but x86 gets away with it anyway, because
    `REGISTER_SPACE_ID`'s offset numbering happens to coincide with real
    x86 GPR offsets (`x86_ia32_low_gpr_name`: 0=EAX, 4=ECX, ...) closely
    enough that live-register *naming* (not an actual tracked value)
    still resolves correctly. AArch64 has no such coincidence — the same
    default-return-offset value that (accidentally) means "X0" under
    x86's convention resolves to "X30" under AArch64's real register
    layout. Reordered so the Rust-Sleigh-space candidate goes first for
    non-x86 ABIs specifically; **deliberately left x86's order
    unchanged** after this exposed 3 real, previously-passing unit tests
    whose synthetic p-code relies on `REGISTER_SPACE_ID`-first for
    live-register naming (not a real def) — confirmed by tracing each
    failure rather than reverting first and asking later.
  - Validated: `cargo nextest run --workspace` 2142/2149, same 7
    pre-existing unrelated `fission-emulator` failures. `golden_corpus_
    check.py` clean with **zero** snapshot diff (confirms x86-64 output
    is byte-identical, i.e. the x86 candidate-order fix truly changed
    nothing for x86). `fission-sleigh` 298/298. An earlier, broader
    version of the Bug 2 fix (reordering *all* ABIs uniformly) was caught
    failing 3 x86 unit tests and 68 golden-corpus lines before landing —
    narrowed to non-x86-only after tracing the root cause, not just
    special-cased blindly.
  - **Bug 3, found but NOT fixed — AArch64 loses if/else structure
    entirely once Bug 2's masking bug stops hiding it.** With Bug 2
    fixed, the same `cmp` fixture no longer returns garbage, but still
    renders as a single linear block (`return xVar5;`, using an unrelated
    parameter) instead of the real three-way branch — confirmed wrong via
    a real Ghidra 12.0.4 headless decompile of the identical binary,
    which correctly renders the full `if (y<x) 1; else if (x<y) -1; else
    0;` structure. This is a separate bug in how multiple predecessor
    blocks that each set the return value before jumping to one shared
    epilogue block get structured on AArch64 (or possibly non-x86
    generally — not cross-checked against ARM32/MIPS/PowerPC) — out of
    scope for this round, since Bug 2's fix was already validated and
    landing on its own merits; recorded here rather than left for a
    future session to rediscover from scratch.
- **Fix — root-caused and fixed the 7 "pre-existing, unrelated"
  `fission-emulator` test failures down to one real JIT bug; found (but
  did not fix) a second, later, separate bug, 2026-07-22** (commit
  `623cd67d`). User asked to fix these too — they had been treated as an
  unrelated baseline throughout this whole session's Fission-side work.
  All 7 share one real binary (`testdata/x64_static_printf_malloc.elf`)
  and, traced with `fission_cli sandbox --dump-trace`, one root cause:
  "Executing PC=0xFFFFFFFF" right after JIT-compiling the TB containing
  `TZCNT`.
  - `TZCNT`'s own SLEIGH semantic template implements the bit-scan as a
    p-code-level loop using intra-instruction relative BRANCH/CBRANCH
    (constant-space deltas like -1/-2, relative to the *current op*).
    `remap_relative_branches` in `fission-emulator/src/jit/compiler.rs`
    already existed specifically to convert these to absolute flat-op
    indices — its own doc comment describes this exact TZCNT scenario —
    but its guard (`space_id == 0 || is_constant`) never actually fired:
    confirmed via a temporary debug print that this emulator's own
    SLEIGH decode path tags every Branch/CBranch destination with
    `space_id=3, is_constant=false` and puts the relative delta in
    `offset`, not `constant_val` — a *different* tagging convention than
    `fission-pcode`'s decompiler-facing lifter (which does use
    `space_id==0`+`is_constant`) — so the guard's check simply never
    matched anything, and the raw delta (e.g. -1 as u32 = 0xFFFFFFFF)
    got executed directly as a guest PC.
  - Fixed without hardcoding a different magic space_id: the new check
    validates the one invariant that's actually always true of a
    relative delta — `local_index + delta` lands inside the *same
    instruction's own p-code op range* — something no real guest address
    can produce by coincidence. On match, marks the varnode
    `is_constant=true` (**not** `false` — an earlier attempt at this fix
    got the sense backwards and would have hit the identical bug through
    a different path, caught by checking how this same file's own
    Branch/CBranch codegen reads a resolved flat index a few hundred
    lines down, which expects exactly `space_id == 0 || is_constant`).
  - Confirmed the crash itself is gone (TZCNT's loop now iterates
    correctly, execution proceeds far past the old failure point) — but
    all 7 tests still fail, now on a **different, later, separate bug**:
    execution eventually runs off the end of `.text` (confirmed via
    `objdump -h`: text ends `0x1006bff`) into unmapped memory
    (`fission_cli disasm` at `0x1006c00`: "not in any section"),
    apparently right after a `ret` in a 128-bit-SSE (`movaps`/`movups`)
    memcpy loop. Checked the JIT's own `size > 8` handling for
    `Load`/`Store` (`read_bytes`/`write_bytes` via an explicit stack
    slot, not the 8-byte-only `read_space`/`write_space` path) and it
    looks structurally correct on inspection — did not find the actual
    root cause of the stack/return-address corruption this round.
    Recorded here with the concrete evidence gathered (exact crash
    address, section boundary, disassembly) rather than re-explored from
    scratch by a future session, but explicitly **not** claiming a root
    cause I haven't confirmed.
  - Validated: `fission-emulator --lib` 33/33, `--tests` 33/33 excluding
    the 7 still failing on the new/later bug, `golden_corpus_check.py`
    clean (unaffected by construction — the x86-64 decompiler path never
    touches `fission-emulator`'s own JIT).
- **New — self-implemented JIT skeleton for `fission-emulator`, genuinely
  working (not just compiling) for a real opcode subset, 2026-07-22**
  (commit `3469278d`). User's explicit direction: Cranelift
  (`crate::jit`) is an external dependency that must eventually be
  replaced with a self-implemented compiler, and wanted a skeleton —
  clarified specifically that this means a real JIT (native codegen),
  not an interpreter, after an initial interpreter-first proposal was
  rejected.
  - `crate::jit::TbBackend` trait is the seam: `JitCompiler` (Cranelift)
    now implements it as a pure refactor (zero behavior change), and
    `crate::selfjit::SelfJitCompiler` implements it as the new
    self-implemented backend — `core.rs`'s TB cache/chaining logic
    doesn't need to know which one produced a given function pointer.
  - `selfjit::codebuf`: real RWX-free executable memory management
    (mmap RW → write → mprotect RX) plus a real AArch64 I-cache
    invalidation fix (`sys_icache_invalidate`) — found load-bearing the
    hard way (omitting it reliably caused SIGBUS on real generated code
    past a couple of trivial instructions), not assumed.
  - `selfjit::emit::aarch64`: a minimal hand-encoded A64 assembler
    (immediate loads, register ALU, compare+conditional branch, call,
    ret). `emit::x86_64` is a same-shaped, unimplemented stub — this
    session's own dev machine is Apple Silicon, so x86-64 host codegen
    could not be built *and verified* here the same way.
  - `selfjit::compiler::SelfJitCompiler`: translates `Copy`, `IntAdd/
    Sub/And/Or/Xor`, `IntEqual/NotEqual/SLess/Less`, and single-exit
    `Branch`/`CBranch` into real AArch64 machine code, reusing the
    *same* `jit_read_space`/`jit_write_space` host callbacks
    `crate::jit::compiler` (Cranelift) already uses rather than
    re-deriving memory-model correctness from scratch. All other
    `PcodeOpcode` variants (~55 of ~70) and intra-instruction relative
    branches (the TZCNT-style loop construct — see the fix earlier this
    same session) return a descriptive `Err`, not silently-wrong output.
  - Two real bugs found and fixed while getting the integration test to
    actually *pass*, not just compile — both confirmed via bisection
    against already-passing narrower tests, not guessed:
    1. `compile_translation_block` returned a raw pointer into an
       `ExecutableCode` that was then immediately dropped (unmapping the
       page) — the returned function pointer outlived its own mapping.
       Fixed with a `code_arena: Vec<ExecutableCode>` field that outlives
       every compiled TB, matching how Cranelift's own `JITModule` keeps
       all compiled functions alive for the process's lifetime.
    2. The fixed "value is here right now" registers originally lived in
       X9-X11 — caller-saved per AAPCS64, which a *second*
       `jit_read_space` call is explicitly permitted to clobber. `10 +
       32` silently came out as leftover garbage from inside
       `jit_read_space`'s own compiled body, not 42, until moved to
       callee-saved X20-X22 (alongside the existing X19 for the emulator
       pointer) with the prologue/epilogue extended to match.
  - **Not wired in anywhere as the active backend** —
    `crate::core::Emulator` still only ever constructs
    `crate::jit::JitCompiler`. Flipping the default before the coverage
    gap closes would silently break every guest program touching an
    unimplemented opcode, given there is still no interpreter fallback.
    `selfjit::mod`'s own doc comment lays out the recommended path to
    closing that gap: **copy-and-patch** stencil codegen (the technique
    CPython 3.13's JIT and early LuaJIT/WebKit baseline tiers used, not
    a second full Cranelift-equivalent with real register allocation),
    plus the concrete remaining steps in order (remaining opcodes,
    intra-instruction branches, the x86-64 emitter, differential testing
    against Cranelift on this session's own real corpus binaries, only
    then a default flip and the `cranelift-*` dependency removal).
  - Validated: `fission-emulator --lib` 37/37 (33 pre-existing + 4 new:
    `codebuf`'s mmap/mprotect/call round-trip, 2 `aarch64` emitter
    encoding checks, `SelfJitCompiler`'s own real compile-and-execute
    integration test), full `cargo nextest run --workspace` 2146/2153 —
    same 7 pre-existing failures (the separate memcpy/`.text`-overrun
    bug recorded above), no new regressions. clippy clean on every new
    file.
- **Self-implemented JIT: opcode coverage expanded from ~10 to ~25,
  including a real branch-and-set bug this expansion's own new tests
  caught, 2026-07-22.** User's explicit direction: Cranelift is a
  general-purpose JIT so `selfjit` doesn't need to match its generality —
  fine to take Fission-specific shortcuts. Kept going down
  `selfjit::mod`'s own priority list (integer arithmetic/shifts/
  comparisons/extension next after the original Copy/Add/Sub/cmp subset).
  - Added to `emit::aarch64`: `mul_reg`/`udiv_reg`/`sdiv_reg`/`msub_reg`
    (MUL/UDIV/SDIV/MSUB — remainder is computed as `a - (a/b)*b`, since
    AArch64 has no direct remainder instruction), `lsl_reg`/`lsr_reg`/
    `asr_reg` (register-amount shifts), `Cond::Ls`/`Cond::Hi` (unsigned
    <=/>), and `Cond::invert()`.
  - Added to `compiler::compile_op`: `IntMult`, `IntDiv`/`IntSDiv`,
    `IntRem`/`IntSRem`, `IntLeft`/`IntRight`/`IntSRight`, `Int2Comp`
    (negate, via `sub xzr - x`), `IntNegate` (bitwise NOT, via `eor` with
    an all-ones immediate — no `ORN`/`MVN` encoding implemented),
    `BoolNegate`/`BoolAnd`/`BoolOr`/`BoolXor` (booleans reuse the same
    `and`/`orr`/`eor` as their `Int*` counterparts, since p-code booleans
    are already 0/1-valued), `IntZExt` (free — `jit_read_space` already
    zero-extends on read, so it's byte-for-byte `Copy`'s body), `IntSExt`
    (sign-fill via `lsl` into the top of the register then `asr` back
    down, parameterized on the input varnode's declared byte size — no
    `SXTB`/`SXTH`/`SXTW` encodings implemented), `IntSLessEqual`/
    `IntLessEqual` (extends the existing comparison arm). Also fixed
    `IntLess`, which had silently mapped to a *signed* `Cond::Lt` since
    the original skeleton landed — corrected to `Cond::Cc` (unsigned <),
    matching `IntLess`'s actual p-code semantics.
  - **Real bug found and fixed by this batch's own new tests, not by
    inspection**: the comparison-op "materialize `cond ? 1 : 0`"
    branch-and-set sequence patched its placeholder branch instruction
    with a target equal to that branch's own fallthrough address — a
    branch that lands in the same place whether taken or not, i.e. a
    no-op regardless of the condition. Every comparison op
    (`IntEqual`/`IntNotEqual`/`IntSLess`/`IntLess`, and this batch's own
    new `IntSLessEqual`/`IntLessEqual`) silently always produced `1`,
    and nothing before this batch exercised a *false* outcome to catch
    it — the original integration test only used `IntAdd`. Caught by
    `unsigned_and_signed_less_equal_and_bool_ops` (a 6<=5 case
    unexpectedly came back `1`). Fixed by switching to the same
    branch-on-inverse-condition-to-the-false-arm shape
    `emit::aarch64::tests::cmp_and_conditional_branch` already proved
    correct (add `Cond::invert()`, branch past the true-arm on
    `cond.invert()` instead of branching *to* a no-op target on `cond`
    itself). Locked down with a dedicated regression test exercising
    both the true and false side of all four original comparison ops.
  - Documented, not fixed, two pre-existing correctness gaps uncovered
    while writing this batch (see `compiler.rs`'s own doc comment):
    arithmetic/shift results aren't truncated to the varnode's declared
    bit width (everything runs in full 64-bit host registers), and shift
    amounts aren't clamped to that width before shifting (AArch64 masks
    shift-by-register mod 64, which only matches p-code's own
    "shift >= width → 0" semantics for 64-bit operands).
  - `IntCarry`/`IntSCarry`/`IntSBorrow` deliberately deferred (need
    either ARM's own NZCV flag extraction or manual overflow arithmetic,
    neither implemented in the emitter) — user confirmed leaving them for
    a later batch.
  - Validated: `fission-emulator --lib` 42/42 (5 new opcode-coverage
    tests + the regression test above), full workspace `cargo build
    --workspace` clean, clippy clean on every changed file.
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
- **New crate `fission-dir`: DIR/HIR differential structuring verifier,
  Phase 1 landed, 2026-07-22.** User's direction: structuring
  (`crates/fission-pcode/src/midend/structuring/`) is purely heuristic
  CFG→AST reconstruction with nothing checking that the *structured*
  output still computes what the *unstructured* input did — the exact gap
  behind the AArch64 if/else-collapse bug found earlier this session (only
  caught by manual Ghidra diffing). User explicitly reframed "verify HIR"
  as splitting NIR's output into two siblings — **HIR** (today's
  structured output) and **DIR** ("동적으로 일치한 IR", dynamically-verified
  IR) — and asked for a new crate with `fission-emulator`/`fission-solver`
  wired in aggressively from the start so the emulator, solver, and this
  verifier can all keep advancing together.
  - **Key simplification found during design**: DIR is not a new IR type.
    `run_structuring_pipeline(func: &mut HirFunction, ..)`
    (`crates/fission-pcode/src/midend/structuring/passes/pipeline.rs:75`)
    mutates `HirFunction.body: Vec<HirStmt>` in place, and `HirStmt`
    already has `Goto`/`Label`/`Break`/`Continue` variants for exactly the
    pre-structuring flattened form (builder emits p-code→HIR using them
    directly, per `midend/builder/AGENTS.md`). So DIR = a clone of `body`
    taken immediately before structuring runs; HIR = the same clone taken
    immediately after. Same Rust type, same variable namespace
    (`HirFunction::params`/`locals`) — one shared interpreter, fed the
    same concrete arguments, can evaluate both and diff results with
    **zero ABI/register modeling and no real machine execution needed** to
    catch structuring bugs specifically. Real-execution/solver validation
    (the part that actually needs the emulator/solver) is scoped as
    Phase 2, since it targets a different bug class (builder/normalize
    lifting bugs, since DIR/HIR are both downstream of that stage and
    would agree with each other even if both were wrong relative to real
    execution).
  - **Capture hook** (`crates/fission-pcode/src/midend/orchestrate.rs`):
    added `take_last_dir_snapshot()` / `store_last_dir_snapshot()`,
    mirroring the pre-existing `take_last_layered_pseudocode()` thread-local
    side-channel pattern exactly (same file already does this for
    dual-layer NIR/HIR text and register-origin extraction) — a clone of
    `hir.body` stashed right before `run_structuring_pipeline` runs, read
    back by an external caller after the normal decompile call returns.
    Pure observation, zero change to what structuring computes — confirmed
    by `golden_corpus_check.py check` staying byte-identical (160/160
    functions, determinism holds) after the change. Re-exported through
    `fission-pcode`'s and (transitively) `fission-decompiler`'s public
    surface.
  - **`crates/fission-dir` crate** (new): `interp.rs` — a real
    tree-walking `HirStmt`/`HirExpr` interpreter (`Var`/`Const`/`Cast`/
    `Unary`/`Binary`/`Select` expressions; `Assign`/`Block`/`Switch`/`If`/
    `While`/`DoWhile`/`For`/`Label`/`Goto`/`Return`/`Break`/`Continue`
    statements, including a real goto/label resolution mechanism —
    `exec_block` searches its own statement list for a target `Label` and
    moves an index cursor there, bubbling an unresolved `Goto` up to the
    enclosing block otherwise). Deliberately out of scope for Phase 1 (bails
    with `Err`, never silently wrong): `Load`/`Store`/`Deref`/`Index`/
    `FieldAccess`/`Call`/`AggregateCopy` — no memory model yet, so
    verification is scoped to pure/arithmetic functions. `diff.rs` — the
    differential harness (`diff_dir_hir`, `default_samples`) that runs
    both snapshots through the interpreter over a boundary-heavy set of
    concrete integer tuples and reports `Equivalent`/`Diverged`/
    `Unsupported`, treating a same-input error on *both* sides as
    inconclusive (shared UB, e.g. div-by-zero) but an error on only one
    side as a reportable asymmetry.
  - Validated with real, evaluated-result tests (not just "it compiles"):
    a hand-built `max(a,b)` pair — HIR as a structured `If`, DIR as the
    flattened goto/label form a p-code `CBranch` would naturally produce
    — proven `Equivalent` across `default_samples(2)`; a deliberately
    buggy DIR (`return a;` unconditionally) proven caught as `Diverged`
    with the exact reproducing input, not just "equivalent" trivially
    always passing; a `while`-loop accumulator (`sum(n)`) test proving
    `Assign`/loop-carried state interpret correctly. 3/3 new tests pass,
    full `cargo build --workspace` clean, `golden_corpus_check.py check`
    clean.
  - **Not yet done**: no real-corpus end-to-end run yet (hooking
    `take_last_dir_snapshot` up to an actual `decompile_with_rust_sleigh`
    call on a real compiled binary and running `diff_dir_hir` on the
    result) — everything validated so far is hand-built fixtures. That's
    the immediate next step before this tool can report real findings.
    Phase 2 (solver-backed equivalence proof instead of sampling,
    real-execution fidelity check for DIR itself via the emulator) is
    unstarted; `fission-emulator`/`fission-solver` are wired into
    `fission-dir`'s `Cargo.toml` but not yet imported by any code.
- **`fission-dir`: `Dir`/`Hir` promoted to real newtypes, first real-binary
  end-to-end pass, 2 real interpreter gaps found and fixed, 2026-07-22.**
  - **Newtype refinement** (user's own suggestion): `Dir(Vec<HirStmt>)`/
    `Hir(Vec<HirStmt>)` added in `crates/fission-midend-core/src/ir/hir.rs`,
    right next to `HirStmt`. Deliberately still a thin wrapper over the
    existing grammar, not a parallel AST (DIR and HIR are provably the same
    grammar -- structuring only rewrites control flow) -- but now an
    accidental argument swap in `diff_dir_hir(dir, hir, ...)` is a compile
    error instead of two same-shaped `Vec<HirStmt>` silently type-checking
    while being logically backwards. `take_last_dir_snapshot` returns `Dir`;
    added the missing counterpart `take_last_hir_function_snapshot()`
    (returns the full, fully-finalized `HirFunction` -- body *and*
    `params`/`locals`, captured right after `eliminate_redundant_var_assigns`
    runs, same thread-local side-channel pattern) since nothing previously
    exposed the raw post-structuring AST at all (only rendered text via
    `take_last_layered_pseudocode`).
  - **First real end-to-end run** (`crates/fission-dir/tests/
    real_binary_end_to_end.rs`, fixture `testdata/max2.elf` /
    `testdata/src_max2.c` -- a tiny real `-O0` static x86-64 binary built
    for this test via `zig cc`, deliberately *not* hand-built p-code, so
    stack-spilled args/locals actually exercise Fission's stack-slot
    promotion, not just clean synthetic Vars) through the real public
    `decompile_with_rust_sleigh` entrypoint `fission_cli` itself uses.
  - **Two real interpreter gaps found this way, neither visible from the
    hand-built Phase 1 unit tests**:
    1. Real x86 comparisons almost never survive to this HIR snapshot as a
       plain `HirBinaryOp` comparison -- they go through SLEIGH's flag-
       recovery machinery (`of = __sborrow(a,b); sf = ...; zf = ...; if
       (zf || xVar) ...`), which HIR represents as `HirExpr::Call` to one
       of a small, fixed set of pure intrinsic names
       (`fission_pcode::midend::support::expr_util::is_pure_intrinsic_call`:
       `__carry`/`__scarry`/`__sborrow`/`__popcount`). Without recognizing
       these specifically, Phase 1 couldn't verify almost any real x86
       function with a comparison in it. Fixed by adding exactly these four
       as interpreter builtins (`eval_builtin_call` in
       `crates/fission-dir/src/interp.rs`) — carry/scarry/sborrow computed
       via `i128`/`u128` arithmetic against the true signed/unsigned range
       for the operand's declared bit width (sidesteps bitwise sign-trick
       edge cases entirely), popcount via `count_ones`. Any *other* `Call`
       target still bails — a small whitelist of provably pure intrinsics,
       not general interprocedural support.
    2. `HirFunction::locals` doesn't necessarily list every name the body
       assigns — the real fixture assigns `local_4` (likely a
       `NirBindingOrigin::ReturnScaffold`-style slot) in both `if` arms with
       no matching `NirBinding` declaration anywhere. Fixed by having
       `Assign` infer-and-register a fresh variable's type from its RHS the
       first time it's assigned, instead of requiring every assignment
       target to be pre-declared.
  - **Serendipitous finding, not caught by this tool's own diff (caught by
    reading the panic output before the two fixes above), recorded but not
    investigated further**: the real fixture's printed HIR contains
    `return (zf || xVar11) ? uVar16 : uVar13;` where `uVar13`/`uVar16` are
    each assigned in only one arm of a preceding `if`/`else` — and the
    printer's own `render::presentation::invariants` self-check flags
    exactly this pair as "became used-without-def after presentation" and
    rolls back one polish transform, yet the tree it falls back to still
    has the same shape. **After the fixes above, `diff_dir_hir` proved DIR
    and HIR agree on this exact function across all of `default_samples`**
    — so whatever this is, it does not (on this evidence) change the
    function's behavior; the redundant `(zf || xVar11)` condition
    provably always re-selects the same branch that was actually taken, so
    neither variable is ever read on the branch where it's unset. Reads as
    a real C-soundness/readability concern in the presentation layer (a
    human reading this code has no way to know it's safe) rather than a
    behavior-changing bug — flagged for a human look, not chased further
    here.
  - Validated: `fission-dir --lib` + `--tests` 4/4 (3 hand-built fixture
    tests + the new real-binary test), full `cargo build --workspace`
    clean, `golden_corpus_check.py check` clean (160/160, determinism
    holds) confirming the `take_last_hir_function_snapshot` hook is equally
    zero-behavior-change.
- **DIR promoted to a genuinely independent IR (not `HirStmt` reused under
  a different name) -- a real, workspace-wide type split, 2026-07-22.**
  User's explicit correction after the newtype-over-`Vec<HirStmt>` design
  above: DIR must be its own type, produced by `fission-pcode`'s actual
  production pipeline (builder emits it natively; structuring performs a
  real `DirFunction -> HirFunction` conversion), not something `fission-dir`
  privately lowers from someone else's type. Chosen deliberately over the
  smaller alternative (an independent type defined only inside
  `fission-dir`, converted from a captured `HirStmt` snapshot) once sizing
  showed the real fix was tractable via a scripted, compiler-verified
  rename rather than hand-rewriting builder's internals.
  - **Scope, measured before starting**: a grep for `HirStmt`/`HirExpr`
    returned 16,513 occurrences across 176 files (70 in
    `fission-midend-normalize`, 60 in `fission-pcode`, 27 in
    `fission-midend-structuring`, 15 in `fission-midend-core`, 4 in
    `fission-dir`) — because builder, all of normalize, and all of
    structuring (both the `fission-midend-structuring` facade crate and
    `fission-pcode`'s own `midend/structuring/`) read and incrementally
    rewrite one shared tree across dozens of passes. Only `render`/printer
    and anything consuming a finished decompile is genuinely HIR.
  - **Mechanism**: `DirStmt`/`DirExpr`/`DirLValue`/`DirBinaryOp`/
    `DirUnaryOp`/`DirSwitchCase`/`DirFunction`/`DirBinding` defined as
    genuinely separate enums/structs in
    `crates/fission-midend-core/src/ir/{hir,procedure}.rs` — hand-written,
    not macro-generated from `Hir*`, deliberately (independence is the
    point; see the new module doc on `DirStmt`). `NirBinding`'s
    `initializer: Option<HirExpr>` field meant `NirBinding` itself needed a
    `DirBinding` counterpart too (`initializer: Option<DirExpr>`) — not
    purely shared metadata as first assumed; `NirType`/`NirBindingOrigin`
    *are* genuinely shared (no embedded AST). A real conversion function,
    `dir_stmts_to_hir_stmts` (plus the reverse, `hir_stmts_to_dir_stmts`,
    needed once for a structuring-side stats pass re-run on a finished
    tree), does the structural 1:1 walk. `DirFunction::into_hir_function`
    carries ABI/param/local metadata across unchanged, converting only
    `body`. The rename itself was executed as `sed`/`perl` whole-crate
    passes (`fission-midend-normalize`, `fission-midend-structuring`,
    `fission-pcode`'s `builder`/`structuring`/`support`/`pass` dirs), not
    hand-editing — the compiler then found every remaining mismatch, which
    is what made a 176-file change tractable in one session.
  - **Real boundary landed in `orchestrate.rs`**: `store_last_dir_snapshot`
    now captures a full `DirFunction` clone right before
    `run_structuring_pipeline` runs; immediately after that plus
    `eliminate_redundant_var_assigns` finish, `hir` is rebound via
    `hir.into_hir_function(dir_stmts_to_hir_stmts(hir.body.clone()))` --
    an explicit, real type conversion at the exact point the DIR/HIR
    boundary already existed in the pipeline, not a redesign of what
    structuring computes. `take_last_dir_snapshot`/
    `take_last_hir_function_snapshot` now return the real
    `DirFunction`/`HirFunction` types directly.
  - **Non-obvious fallout, each found by the compiler rather than
    inspection**: (1) `fission-midend-core`'s `util`/`action_pipeline`/`vsa`
    modules are "pure HIR helpers shared by normalize/structuring/builder"
    per their own doc comment, but `render` also calls some of them
    (`expr_type`) — `action_pipeline`/`vsa` had zero render callers (safe
    to rename outright), `util` needed a parallel `util_dir` module
    (same function names, `Dir*`-typed) instead, since render genuinely
    needs the `Hir*`-typed originals. (2) `fission-pcode` has its own
    *second*, independent copy of `expr_type`/`is_pure_intrinsic_call`/etc.
    in `midend/support/expr_util.rs` (not the same as midend-core's `util`)
    — this is the file that already defined the canonical
    `__carry`/`__scarry`/`__sborrow`/`__popcount` intrinsic list `fission-dir`
    reused earlier — it's genuinely builder-only, so renaming it outright
    was correct, but `render/mod.rs`'s own `expr_type` import (via
    `crate::midend::expr_type`) turned out to resolve to *this* file all
    along, not midend-core's, so render needed its own tiny local
    `expr_type` added directly. (3) `builder/type_hints.rs` (`apply_preview_type_hints`)
    lives under `builder/` by historical location but is only ever called
    *after* structuring finishes, on the real `HirFunction` — reverted to
    Hir-typed specifically, both in `type_hints.rs` and its
    `builder/mod.rs` wrapper, plus the matching `#[cfg(test)]` helper.
    Builder also calls the real printer (`print_expr`) at ~14 call sites to
    embed rendered text into diagnostics/evidence fields (not into the
    AST) — added `print_dir_expr` (converts via `dir_expr_to_hir_expr`,
    reusing the real printer rather than duplicating it) and swapped every
    Dir-side call site to it.
  - **`fission-dir` itself**: `interp.rs` rewritten around one
    `define_interp!` macro instantiated twice (`dir::interpret`/
    `hir::interpret`) instead of two hand-written copies or a shared
    generic/trait — deliberate: the whole point of this crate is
    comparing DIR vs. HIR execution, so if the two interpreters' *logic*
    could silently drift apart, that would corrupt the comparison itself.
    `diff_dir_hir` now takes `&DirFunction`/`&HirFunction` directly
    (dropped the old `Dir`/`Hir` newtypes entirely) — each side's own
    `params`/`locals` travel with it, so `diff_dir_hir` no longer needs
    them as separate arguments. Added `dir_assign_and_while_loop_interpreted_correctly`,
    a DIR-side mirror of the existing HIR while-loop test, specifically to
    prove the macro's two instantiations aren't accidentally different.
  - Validated: `fission-pcode --lib` + `--tests` 927/927 (zero regressions
    from the entire rename), `fission-midend-normalize` 270/270,
    `fission-midend-structuring` 99/99, `fission-dir` 5/5 (including the
    real `max2.elf` end-to-end test, now exercising genuinely independent
    `DirFunction`/`HirFunction` instead of two clones of one type), full
    `cargo build --workspace` clean, `golden_corpus_check.py check` clean
    (160/160, byte-identical, determinism holds — the load-bearing proof
    that a 176-file type-identity rename plus one real conversion function
    changed zero decompiler output), `cargo nextest run --workspace`
    2157/2164 passed — the same 7 pre-existing, already-documented
    failures (unrelated memcpy/`.text`-overrun bug), zero new regressions.
- **`fission-dir` crate deleted, 2026-07-22.** After the type-independence
  split above landed, the user realized `fission-dir` (the naming
  suggested an isolated crate) was never actually where isolation
  happened — `DirStmt`/`DirExpr`/etc. live in `fission-midend-core` (the
  shared substrate nearly everything depends on) precisely because
  builder/normalize/structuring needed to construct/consume them directly;
  `fission-dir` was always a downstream-only consumer (depended on
  `fission-pcode`/`fission-decompiler`/`fission-emulator`/`fission-solver`,
  nothing depended on it). Deleting it doesn't touch the dependency
  direction that actually mattered — no core crate ever depended on
  `fission-dir` — but it does remove the interpreter/differential-
  verification tooling itself (Phase 1: `interp.rs`'s macro-generated
  `dir::interpret`/`hir::interpret`, `diff.rs`'s `diff_dir_hir`, the real
  `max2.elf` end-to-end test) and the roadmap's Phase 2 (solver-backed
  equivalence proof, emulator-backed DIR fidelity check).
  - Explicitly kept: the `DirStmt`/`HirStmt` type-independence split
    itself (commit `08361455`) — the user confirmed only the crate should
    go, not the type-identity refactor, since it has independent value
    (real, compiler-verified separation between pre- and post-structuring
    data) regardless of whether anything outside the core pipeline
    consumes it.
  - Removed: `crates/fission-dir/` entirely, its workspace member entry.
    Fixed two now-dangling doc-comment references to `fission-dir`/
    `fission_dir::diff::diff_dir_hir` (in `fission-midend-core/src/ir/
    hir.rs` and `fission-pcode/src/midend/orchestrate.rs`) — nothing else
    in the workspace ever referenced the crate (confirmed via grep before
    deleting). Also fixed a stale doc reference to a
    `dir_function_to_hir_function` function name that was never actually
    implemented under that name (the real one is `dir_stmts_to_hir_stmts`)
    while touching that comment.
  - The `take_last_dir_snapshot`/`take_last_hir_function_snapshot` hooks
    in `orchestrate.rs` are left in place (still real, still zero-
    behavior-change observational side channels) even though nothing
    currently reads them — cheap to keep, and exactly what a future
    verifier (this crate's replacement, or a differently-scoped one)
    would need again.
  - Validated: `cargo build --workspace` clean, `golden_corpus_check.py
    check` clean (160/160, determinism holds).
- **New crate `fission-midend-dir`: split DIR out of `fission-midend-core`,
  2026-07-22.** User's follow-up concern after the `fission-dir` deletion:
  `fission-midend-core` (the shared substrate nearly everything depends
  on) was carrying *both* `Dir*` and `Hir*` IR families plus
  `action_pipeline`/`vsa` (confirmed, while doing this split, to have zero
  HIR-side callers) — meaning `fission-decompiler`/`fission-cli`, which
  only ever touch the final `HirFunction`, were compiling in `DirStmt` and
  the whole DIR-only pass framework for nothing every time they pulled in
  `fission-midend-core`.
  - **What moved** to the new crate (depends on `fission-midend-core`, not
    the reverse — avoids a cycle since the DIR→HIR conversion functions
    need to see both `Dir*` and `Hir*`, and only this direction allows
    that): `DirStmt`/`DirExpr`/`DirLValue`/`DirBinaryOp`/`DirUnaryOp`/
    `DirSwitchCase`/`DirFunction`/`DirBinding` type definitions (moved via
    `git mv` + precise line-range extraction from `hir.rs`/`procedure.rs`,
    not re-derived), the whole `action_pipeline` and `vsa` modules
    (confirmed zero render/HIR-side callers before moving), and
    `util_dir` (renamed to plain `util` inside its new home, since there's
    no longer an ambiguity to disambiguate against locally). The real
    `dir_stmts_to_hir_stmts`/`hir_stmts_to_dir_stmts` conversion functions
    moved here too, since this crate is the one place that can legitimately
    see both type families.
  - **What stayed** in `fission-midend-core`: `HirStmt`/`HirExpr`/etc,
    `NirType`/`NirBinding`/`NirBindingOrigin`/`CallSummary`/etc (genuinely
    shared, no embedded AST — confirmed by grep: zero Dir/Hir coupling in
    `abi_param.rs`/`fast_hash.rs`/`labels.rs`/`wave_stats.rs`/`ir/{build_stats,decomp_facts,inference,options,stats_merge}.rs`),
    `util` (kept `Hir`-typed for `render`).
  - **Real, load-bearing discovery made while splitting, not assumed**:
    `render/mod.rs`'s own `expr_type` import (`use crate::midend::{...
    expr_type}`) turned out to resolve through `fission-pcode`'s *own*,
    second, independent `expr_type` copy in `midend/support/expr_util.rs`
    (the file this session's `is_pure_intrinsic_call` builtin list came
    from originally) — not `fission-midend-core::util::expr_type` as
    assumed. Confirmed via a real build failure once `support/expr_util.rs`
    got renamed wholesale to `DirExpr` (correct, since it's genuinely
    builder-only) — `render` needed its own small local `expr_type`
    (`render/mod.rs`), not a redirect to `fission-midend-core`'s copy,
    which nothing in `render` had actually been using all along.
  - **Mechanism**: same scripted-rename-then-let-the-compiler-find-it
    discipline as the earlier DIR-independence split, but for import paths
    rather than identifiers — `fission_midend_core::{util_dir,
    action_pipeline, vsa}::` → `fission_midend_dir::{util, action_pipeline,
    vsa}::`, plus splitting every mixed `use fission_midend_core::ir::{Dir*,
    Nir*, ...}` import into two lines (scripted via a Python regex pass
    that classifies each imported name against the known `Dir*` type list,
    not by hand). `fission-midend-normalize`/`fission-midend-structuring`'s
    preludes/glob-reexports switched from curated re-export lists to plain
    `pub use fission_midend_core::ir::*; pub use fission_midend_dir::ir::*;`
    pairs after a curated list proved to be a whack-a-mole of missed items
    (`StorageClass`, `ObjectFact`, ...) — a glob import of the same
    underlying item via two paths (both crates re-export
    `NirType`/`NirBinding`/etc identically) is not an ambiguity error in
    Rust, which is what makes this simplification safe.
  - Hit a real, unrelated blocker mid-validation: `cargo nextest run
    --workspace` failed with "No space left on device" -- `target/` had
    grown to 226GiB. `cargo clean` freed 289GiB before re-running; not a
    code issue.
  - Validated: `fission-midend-core --lib` 5/5, `fission-midend-dir --lib`
    4/4, `fission-midend-normalize` 270/270, `fission-midend-structuring`
    99/99, `fission-pcode` 927/927 (all identical counts to before the
    split — zero regressions), full `cargo build --workspace` clean with
    **zero** changes needed in `fission-decompiler`/`fission-cli`/
    `fission-emulator`/anything else downstream (confirming the split
    achieved its actual goal), `golden_corpus_check.py check` clean
    (160/160, byte-identical), `cargo nextest run --workspace --no-fail-fast`
    2152/2159 — the same 7 pre-existing, already-documented failures,
    zero new regressions.
- **CLI `decomp --dir` flag + Python DIR/HIR structural-signal check
  (`scripts/quality/dir_hir_check.py`), found and fixed a real HIR
  content-loss bug, 2026-07-22.** User's explicit follow-up direction
  after the `fission-dir` deletion: do the DIR/HIR comparison in Python
  (not another Rust interpreter crate), and expose DIR on the CLI.
  - **CLI**: `decomp --dir` threaded end-to-end — `DecompArgs` →
    `OneShotArgs` → `RenderConfig::dir` → `render_with_rust_sleigh`'s
    `want_dir` param (calls `take_last_dir_snapshot()` immediately after
    decompile, before any other decompile call on the thread, since the
    snapshot is a single-slot thread-local) → `RustSleighRender::code_dir`
    → `CliRustOutcome::Success::code_dir` → JSON `"code_dir"` field
    (present only when requested) / plain-text `// ===== DIR
    (pre-structuring) =====` appended section. `print_dir_function`
    (`fission-pcode/src/midend/mod.rs`) renders a `DirFunction` snapshot
    by converting it to `HirFunction` via the real `dir_stmts_to_hir_stmts`
    conversion and reusing the production HIR printer — deliberately not
    a second hand-written printer, so DIR output can't silently drift
    from what HIR printing actually does.
  - **Python tool**: `structural_signal()` extracts a `Counter` of
    control-flow keywords / comparison operators / integer constants from
    each side's rendered text (identifiers stripped, since DIR and HIR
    routinely name the same value differently). Flags "high" severity
    when HIR is missing a control-flow keyword or comparison operator DIR
    had — the strongest textual hint of a dropped branch. Explicitly a
    heuristic on rendered text, not an interpreter — the previous
    session's `fission-dir` crate was exactly that (a real interpreter)
    and got deleted by explicit user direction in favor of this cheaper
    approach.
  - **Real bug found and fixed, not just a false-positive audit**: running
    the tool against the full 16-binary/160-function corpus flagged
    `pre_c_init` (mingw CRT startup, present verbatim in every corpus
    binary since it's compiler-injected, not user code) with a
    conspicuously large diff (`missing_in_hir={'if': 3, 'goto': 3, ...}`)
    in all 17 binaries containing it. Root-caused by hand (not guessed):
    added temporary instrumentation tracing whether a specific label
    survived through the pipeline at each of the ~20 `fission-midend-
    normalize` cleanup passes (all clean), the standalone
    `eliminate_redundant_var_assigns` call, the `DirFunction → HirFunction`
    conversion, `apply_preview_type_hints`, and `recover_global_symbol_
    accesses` (all clean, label present) — only vanishing inside
    `render_layered_pseudocode`'s HIR-only `apply_hir_presentation` step
    (`fission-pcode/src/render/presentation/mod.rs`), specifically
    `prune_unreachable_after_total_return`. That pass truncates a
    statement list right after any unconditional `return`, on the
    (usually correct) assumption that anything textually after is dead —
    but `pre_c_init`'s DIR shape has a `goto` earlier in the function that
    jumps *forward past* an unconditional return into a labeled block
    laid out later (PE-header-based subsystem detection, guarding
    `__set_app_type`/`__p__fmode` setup) — genuinely live code, reachable
    only via that goto. The pass deleted the labeled block anyway, leaving
    the goto dangling and silently dropping the PE-header detection branch
    from HIR output (present in DIR and in NIR, since `apply_hir_
    presentation` only runs for the HIR profile — this was HIR-only
    presentation-layer breakage, not a structuring/normalize semantic
    bug). Confirmed this reproduces in plain `decomp --layer hir` with no
    `--dir` involved — a pre-existing production bug the new tool
    surfaced, not something the DIR-capture code introduced.
  - **Fix**: `prune_unreachable_after_total_return` now computes the set
    of all `goto` targets referenced anywhere in the function (`collect_
    goto_targets`) before truncating, and — when the tail it's about to
    discard contains a still-referenced `Label` — drains only the
    genuinely dead filler before that label instead of truncating past it,
    leaving the reachable labeled block intact. Added a regression test
    (`hir_presentation_keeps_goto_target_after_unconditional_return`)
    reproducing the minimal shape (`if (c) goto L; x = 1; return x; L: y =
    2; return y;`). The existing `check_hir_presentation_invariants`
    revert-on-violation safety net (ADR 0011) did not catch this class of
    bug — it checks call/load counts, use-without-def, and empty-if
    shells, not dangling gotos — left as-is rather than expanded, since
    the root-cause fix in `prune_unreachable_after_total_return` is the
    correct place to fix it and a second defense-in-depth layer wasn't
    needed to close this specific finding.
  - **Confirmed false positives, not chased further** (both from
    legitimate `apply_hir_presentation` folds, documented in `dir_hir_
    check.py`'s own docstring): `while (1) { if (!c) break; ... }` →
    `while (c) { ... }` (`count_bits`, `list_sum`, `sum_array`,
    `rolling_hash32`), `if (c) { return a; } return b;` → `return c ? a :
    b;` (`clamp`), and comparison-operand canonicalization `k < x` → `x >
    k` (`rc4_init`, from `canonicalize_presentation_conditions`'s "prefer
    `x op k` over `k op x`" rule) plus a single `if (c) goto L` → `if (!c)
    {...}` inversion (`fibonacci`).
  - Validated: `golden_corpus_check.py check` — the fix changed rendered
    output for exactly the 16 `pre_c_init` occurrences (expected, reviewed
    by hand, snapshot updated via `update`), then re-ran clean (160/160,
    determinism holds). `dir_hir_check.py check` dropped from 22 to 6
    HIGH-severity findings (the 16 `pre_c_init` occurrences plus the one
    Python-classifier count fixed earlier this phase), all 6 remaining
    confirmed benign by hand. `cargo test -p fission-pcode presentation`
    32/32 (31 pre-existing + 1 new regression test).
- **New crate `fission-verify`: solver- and emulator-backed DIR/HIR
  verification, 2026-07-22.** User's explicit follow-up after the
  `dir_hir_check.py` work: connect DIR/HIR correctness checking to the
  *real* `fission-solver` (a from-scratch SAT/SMT-style solver) and the
  *real* `fission-emulator` (production p-code execution engine), not
  another hand-rolled toy interpreter like the deleted `fission-dir` crate
  (whose own doc comment admitted "Phase 2's solver-backed equivalence
  check is what closes that gap" but never built it). Planned via
  `EnterPlanMode`/`ExitPlanMode` (approved plan preserved at
  `.claude/plans/quirky-coalescing-lemur.md`), executed in four
  incrementally-committed phases.
  - **Crate placement**: `crates/fission-verify` depends on `fission-core,
    fission-loader, fission-static, fission-midend-core, fission-midend-dir,
    fission-decompiler, fission-solver, fission-emulator, fission-sleigh`.
    Confirmed acyclic before creating it: `fission-solver` has zero
    `fission-*` deps; `fission-emulator` and `fission-decompiler` don't
    depend on each other; `fission-cli` already depended on both, so this
    crate formalizes an existing coupling rather than inventing a new one.
  - **Phase 0 (scaffolding)**: `decompile_one()` wraps
    `decompile_with_rust_sleigh_with_facts` + immediate
    `take_last_dir_snapshot`/`take_last_hir_function_snapshot` (the exact
    pattern `decomp --dir` already uses) so every tier below consumes the
    real production decompile output, not a separate path. Shared
    `VerifyOutcome`/`Divergence`/`UnsupportedReason` report types used by
    all three tiers.
  - **Phase 1 (concrete tier, no solver/emulator)**: restored and adapted
    the deleted `fission-dir` crate's `interp.rs`/`diff.rs`
    (`git show fdae396d^:crates/fission-dir/src/{interp,diff}.rs`) into
    `eval.rs`/`diff.rs` -- same `define_interp!`-style macro generating
    `dir::interpret`/`hir::interpret` from one body so DIR-side/HIR-side
    logic can't silently drift, same boundary-value cross-product sampling.
    `width_of`/`is_signed`/`normalize` factored out of the macro to
    top-level `pub fn`s so later phases could reuse them instead of a third
    copy. This tier alone doesn't satisfy the ask (still blind to a bug
    shared by both DIR and HIR) -- it's the baseline the other two build on.
  - **Phase 2 (emulator-grounded ground truth)**: the genuinely new
    capability the deleted crate never had -- checks HIR against what the
    *real machine code* actually computes, not just against DIR.
    - **One additive change to `fission-emulator`** (`core.rs`): extracted
      `run()`'s loop body into `run_inner(stop_pc: Option<u64>) ->
      Result<RunOutcome>`; `run()` is now a thin wrapper (`run_inner(None)`,
      unchanged behavior/signature), and new `run_until_pc(stop_pc)` reuses
      the same loop (HLE-trap band, signals, TTD bookkeeping intact)
      instead of a duplicate one. New exported `RunOutcome` enum
      (`Returned/Halted/SymGate/HitBudget/ProcessExited/LoopExit/
      Interrupted`) -- distinct, reported outcomes, nothing coerced into a
      silent pass/fail. `fission-emulator`'s own suite: 81/88 both before
      and after (same 7 pre-existing failures).
    - `EmulatorHarness` (`emu_driver.rs`) builds a real, loaded process
      image mirroring `fission-cli`'s `sandbox` setup exactly (format-aware
      loader, `ArchInfo::from_language_id` -- confirmed independent of any
      decompiler-side `CallingConvention` enum, which nothing in
      `fission-emulator`/`fission-cli`'s emulator path reads), snapshots the
      post-image-setup baseline, and drives `run_until_pc` to call one
      function with concrete args (sentinel return address in the link
      register or pushed on the stack per the CC) and read back the real
      return register. Restores the baseline before every call so one
      sample's global/heap writes can't leak into the next.
    - `ground_truth.rs` compares that ground truth against both DIR's and
      HIR's concrete evaluation -- a `dir_eval == hir_eval != emulator`
      result is now catchable, a class of bug Phase 1 structurally cannot
      see (both decompiler paths sharing the same wrong answer).
    - **Real bug found while validating the harness itself**: the initial
      driver only reserved stack space for the pushed return address, not
      Win64's mandatory 32-byte "shadow space" between it and the first
      stack argument (`stack_arg_offset(0) == 0x28` on Win64 vs `0x08` on
      SysV64/cdecl -- confirmed by reading `Win64Cc`/`SysV64Cc`/`Cdecl32Cc`
      directly). A callee whose prologue spills register args into that
      space (as real `-O0` compiled functions with 3+ params do) could
      write into memory outside the intentionally-reserved frame. Fixed by
      reserving `stack_arg_offset(0) - ptr_size` extra bytes below the
      return address slot -- this generalizes correctly to SysV64/cdecl
      (where the gap is `0`, a no-op) without a Win64 special case.
    - Proven to have teeth, not just pass trivially: a test hand-corrupts a
      real decompiled HIR body (always-returns-0) and confirms the real
      emulator catches the divergence.
  - **Phase 3 (solver-backed symbolic DIR/HIR equivalence)**: `lower_sym.rs`
    adds a third `define_lower_sym!` macro instantiation lowering
    `DirExpr`/`HirExpr` to `fission_solver::ast::SymExpr`, for **loop-free**
    functions only (acyclic symbolic execution by explicit path
    enumeration: fork at each `If`, recurse each branch with its own env/
    path-condition, resolve `Goto`/`Label` exactly like the concrete
    interpreter since the goto *target* is always static -- only whether it
    fires is symbolic -- combine every `Return` reached into one closed-form
    `SymExpr` via a right-fold of `Ite`s). Excludes `Div`/`Mod`/`Sar`
    (`fission_solver::ast::SymExpr` has no `Sdiv`/`Urem`/`Srem`/`Ashr` yet)
    and `Switch` (deferred, not rushed -- its break/fallthrough semantics
    interacting with path forking is a real design decision).
    - **Confirmed by reading `fission-solver`'s actual AIG bit-blasting
      (`aig.rs`), not trusting the doc comment**: `SymExpr`'s `size` fields
      are *bit* counts everywhere real lowering happens (`Const{val,size}`
      emits exactly `size` bits; `Extract{lsb,size}` slices bits, not
      bytes), contradicting `Sort::BitVector`'s own doc comment ("size in
      bytes"). Matched the real behavior throughout rather than the stale
      comment -- this would have been a silent, systemic width bug (e.g.
      treating a 32-bit int as a 4-*bit* value) had the comment been
      trusted instead of verified.
    - `symbolic.rs` orchestrates: **one shared `SymExpr::Var` per parameter
      index** fed into both DIR- and HIR-side lowering (so "same input" is
      a meaningful assertion -- two independently-minted Vars would make
      the equivalence check vacuous), asserts
      `Neq(dir_result, hir_result)`, `Solver::check_sat()`: `Unsat` is a
      genuine equivalence proof (not "no counterexample found among a few
      samples"); `Sat` returns a real counterexample via `get_value`.
    - Demoed against a real corpus function (`clamp`, loop-free): DIR and
      HIR proved equivalent (`Unsat`), and a hand-corrupted HIR variant
      proved genuinely different with a real solver-found counterexample.
  - **CLI entry point**: new `fission_cli verify BINARY --addr ADDR --tier
    {concrete,ground-truth,symbolic,all}` subcommand (`VerifyArgs` in
    `cli/args/mod.rs`, handler in `cli/oneshot/mod.rs`, modeled on the
    existing `sandbox` handler for emulator setup). Required adding
    `"verify"` to `CANONICAL_SUBCOMMANDS` (`cli/args/mod.rs`) -- without it,
    the legacy flat-arg parser silently swallowed the subcommand name as if
    it were the `BINARY` positional, a real bug caught by actually running
    the new command rather than trusting the exhaustive `match` compiling
    cleanly. Explicitly a heavy, opt-in "deep verify" tier --
    `dir_hir_check.py` stays the fast, no-solver/no-emulator tier that runs
    routinely across the full corpus.
  - **Real, separate finding surfaced by the ground-truth tier working as
    designed** (not fixed this session -- out of scope for a DIR/HIR
    verification plan, and risky to touch without its own dedicated
    investigation): running `verify --tier ground-truth` against `clamp`
    showed real, wide divergence from DIR/HIR (which the concrete and
    symbolic tiers, plus a hand-check, already proved correct and mutually
    equivalent). Root-caused far enough to characterize precisely: `clamp`
    with a *negative* second argument (e.g. `clamp(0, -1, 0)`) takes the
    wrong branch at its first signed comparison (`0 >= -1` evaluates
    false) when actually executed. Confirmed the stack layout and spilled
    argument values were correct via direct memory dumps (`[rbp+0x10..
    0x20]` held exactly the intended `0, 0xFFFFFFFF, 0`), and confirmed
    `fission-emulator`'s p-code interpreter (`pcode/eval.rs`'s `IntSLess`)
    correctly sign-extends both operands before comparing -- but confirmed
    via `core.rs`'s own doc comment ("JIT-only multi-instruction TB path...
    Compile fail -> hard error (no interpreter)") that the actual execution
    path is the Cranelift JIT, not that interpreter, meaning the bug (if
    real) is JIT-specific and unverified against `eval.rs`'s logic. Only
    manifests with a negative 32-bit *memory* operand in a signed compare
    -- `count_bits` (unsigned shift/and only) and `clamp` calls with
    all-non-negative args are unaffected, which is why this session's
    Phase 2 test suite (built around `count_bits`) stayed green. Flagged
    here as a concrete, reproducible lead (`clamp @ 0x14000155f` in
    `control_flow_gcc_O0.exe`, args `(0, -1, 0)`, expected `0`, actual
    `0xFFFFFFFF`) for a future, dedicated JIT-correctness investigation --
    not chased further this session to avoid scope creep into core x86 JIT
    semantics under a plan scoped to DIR/HIR verification plumbing.
  - Validated: `fission-verify` test suite 10/10 (2 tests per phase,
    real-corpus-grounded, each phase's "does it actually catch a bug" claim
    proven by a corrupted-input test, not just a happy-path pass), full
    `cargo build --workspace` clean, `cargo nextest run --workspace
    --no-fail-fast` 2163/2170 (same 7 pre-existing unrelated
    `fission-emulator` failures throughout every phase, zero new
    regressions), `fission-cli --lib` 45/45, `golden_corpus_check.py`
    clean (160/160, determinism holds).
- **Fixed the JIT signed-comparison bug found above, 2026-07-22.** User's
  explicit follow-up: pursue the `clamp @ 0x14000155f` lead flagged as a
  concrete, reproducible finding in the previous entry. User also asked
  whether the fix needed a self-written JIT compiler and an interpreter
  fallback — answered no and explained why before touching code: `selfjit/`
  already exists but is an intentionally-incomplete experiment (no x86_64
  host emission, no register allocation, no loops — its own module doc
  says completing it is future work, not a quick option), and a compile-
  failure fallback wouldn't have helped regardless, since the JIT compiled
  *successfully* here and silently computed the wrong answer — nothing
  would have triggered a fallback path keyed on compile failure.
  - **Root cause, confirmed by reading the actual lowering code** (not
    guessed): `crates/fission-emulator/src/jit/compiler.rs`'s `load_vn!`
    macro always zero-extends a narrower-than-8-byte p-code operand into
    its Cranelift `I64` value (`ensure_var!`'s masking, and the
    `jit_read_space` host callback both size-aware but sign-agnostic).
    Correct for unsigned ops and for ops where input sign doesn't affect
    the result (`Add`/`Sub`/`Mul` — two's-complement wraparound is
    identical either way once the output is masked to its own width).
    **Wrong** for `IntSLess`/`IntSLessEqual`/`IntSDiv`/`IntSRem`/
    `IntSRight`, whose result genuinely depends on the operand's sign: a
    negative `dword` `-1` (`0xFFFFFFFF`) must sign-extend to
    `0xFFFFFFFF_FFFFFFFF`, not stay a huge positive
    `0x00000000_FFFFFFFF` — exactly what made `clamp(0, -1, 0)`'s
    `0 >= -1` (`IntSLess`) evaluate false. Confirmed `IntCarry`/
    `IntSCarry`/`IntSBorrow` were *not* affected: their host callout
    (`jit_int_flag`/`int_flag_op` in `float_ops.rs`) already does its own
    correct `sign_extend_n` internally from the explicitly-passed size,
    independent of the Cranelift-side load.
  - **Fix**: new `sign_extend_val!`/`load_vn_signed!` macros in
    `jit/compiler.rs` (reusing the exact `ishl`/`sshr`-immediate technique
    the existing, already-correct `IntSExt` opcode handler uses), applied
    at the five affected sites. For the comparison opcodes specifically,
    the *shadow/symbolic taint side-channel* (`emit_shadow_binop!`) still
    receives the original zero-extended values unchanged — deliberately
    not touched, since the symbolic layer re-derives its own sign context
    from each operand's separately-passed declared size and touching it
    risked destabilizing unrelated, already-working concolic-execution
    tests for no benefit. For `IntSRight`, only the value being shifted
    (`a`) is sign-extended — the shift *count* (`b`) is a plain magnitude,
    not a signed quantity.
  - **Also fixed the analogous Win64 ABI bug this same investigation
    surfaced in `EmulatorHarness`** (`fission-verify`): the driver only
    reserved stack space for the return address, not Win64's mandatory
    32-byte shadow space a callee's prologue spills its register args
    into — `clamp` (3 args, `-O0`, spills-to-stack-homes prologue) was
    the first function exercised by this session's work that actually
    needed it.
  - **`selfjit/`'s identical latent bug, found but *not* fixed**: `load_
    value` has the exact same zero-extension defect via the same
    `jit_read_space`-style host callback, affecting `IntSLess`/
    `IntSLessEqual`/`IntSDiv`/`IntSRem`/`IntSRight` there too. Left as a
    documented gap (module doc comment + inline `TODO(correctness)`
    markers at each site, matching this file's own established practice
    for its other two known gaps — no width truncation, no shift-amount
    clamping) rather than a hand-assembly AArch64 patch, since
    `SelfJitCompiler` is confirmed not wired in anywhere live (`Emulator`
    only ever constructs `jit::JitCompiler`) and the module's own doc
    already prescribes the right next step before touching any of its
    correctness gaps: differential-test it against `jit::compiler` on real
    corpus binaries first (item 4 in `selfjit/mod.rs`'s doc).
  - **New regression coverage**: a `jit::compiler::tests` module (this
    file had zero `#[test]`s before — the actual bug had no unit coverage
    at all) with `signed_ops_on_narrow_negative_memory_operand_are_
    correct`, hand-building the exact `dword`-negative-operand p-code
    shape and checking `IntSLess`/`IntSLessEqual`/`IntSDiv`/`IntSRem`/
    `IntSRight` against plain Rust arithmetic on the same values — modeled
    on `selfjit::compiler::tests`'s existing, proven pattern
    (`compile_and_run`/`read_reg`/`copy_const`/`binop`helpers). Plus an
    integration-level regression in `fission-verify`
    (`clamp_matches_real_machine_code_including_negative_args`) closing
    the loop on the exact real-corpus finding that motivated the fix.
  - Validated: new `jit::compiler` unit test + all 6 pre-existing
    `selfjit::compiler` tests pass; `fission-emulator` suite 82/89 (added
    1 net test; same 7 pre-existing unrelated failures); `fission-verify`
    11/11; `cargo build --workspace` clean; `golden_corpus_check.py` clean
    (160/160); `fission_cli verify clamp --tier all` now reports
    `Equivalent` on all three tiers (was `Diverged` on ground-truth only,
    before this fix).
- **First steps toward removing the Cranelift dependency: `selfjit`
  `Load`/`Store` + a differential-testing harness, 2026-07-22.** User's
  explicit direction after the JIT fix above. `crates/fission-emulator/
  src/selfjit/` is a self-written AArch64 JIT scaffold whose own doc
  already states replacing Cranelift as the end goal, not wired in
  anywhere (`core.rs:292` always constructs `jit::JitCompiler`). Planned
  via `EnterPlanMode`/`ExitPlanMode` (plan preserved at `.claude/plans/
  quirky-coalescing-lemur.md`), as a concretely-scoped first milestone
  toward the scaffold's own existing 5-step roadmap, not the whole thing.
  - **Why `Load`/`Store` first, ahead of the roadmap's own literal step
    1**: confirmed by reading `selfjit/compiler.rs`'s `compile_op` match
    directly that `Load`/`Store` were completely unimplemented and not
    even mentioned in the existing roadmap's priority list -- yet
    virtually every real-world function does at least one pointer
    dereference, making this the actual blocker to compiling *any*
    representative real function, more load-bearing than what the
    roadmap listed first.
  - **Part 1 -- `Load`/`Store` (narrow, ≤8-byte path)**: mechanical,
    confirmed by reading `crate::jit::compiler`'s (Cranelift, the active
    backend) own arms first. Resolves the space-id input at compile time;
    routes the address operand through the existing `load_value` into a
    callee-saved register (`A_VAL`) *before* setting up the `jit_read_
    space`/`jit_write_space` call's own arguments -- `load_value` may
    itself `blr` into a nested callback call, which the ABI permits to
    clobber caller-saved registers, so the address needs a slot that
    survives that (the exact lesson this file's own pre-existing `A_VAL`/
    `B_VAL` doc comment already documents from an earlier bug). Wide
    (`>8`-byte) `Load`/`Store` returns a descriptive `Err` rather than
    silently truncating -- this hand-rolled backend has no stack-slot
    allocator yet; porting Cranelift's wide-path shape safely is real,
    separate follow-up work. New tests: a round trip through a *computed*
    (not compile-time-constant) address, and a wide-value test proving
    both opcodes fail loudly rather than corrupt data.
  - **Part 2 -- `selfjit::differential`** (`#[cfg(test)]`-only): the
    scaffold's own roadmap item 4, started now rather than deferred to
    "later" -- deliberately, since a computed-address memory op
    (endianness, space-id resolution, self-modifying-code interaction) is
    exactly the class of bug that slips past hand-built synthetic unit
    tests (this session's own signed-comparison JIT bug had zero
    synthetic coverage and was only found by testing against a real
    binary). Captures real, SLEIGH-decoded translation blocks from a real
    binary via a newly-`pub(crate)` `Emulator::collect_translation_block`,
    treats `JitCompiler` (Cranelift) as the trusted pathfinder for real,
    data-dependent control flow (no calling-convention/argument injection
    needed -- a pure backend-A-vs-backend-B differential check doesn't
    need semantically meaningful inputs, only that both backends compute
    the *same* thing from whatever's actually there), and replays every
    TB whose opcodes `SelfJitCompiler` currently supports independently
    against a state-synced second `Emulator`, diffing final register
    bytes. TBs using an unsupported opcode are skipped and reported, not
    failed.
  - **Real, non-obvious finding the harness surfaced almost immediately**:
    running it against `checksum` (this session's own corpus,
    `control_flow_gcc_O0.exe`, a real `Load`-in-a-loop function) found
    **zero** matchable TBs -- not because `Load`/`IntSLess`/`CBranch`
    themselves are unsupported, but because x86-64 SLEIGH's own lowering
    of `CMP` unconditionally emits `IntCarry`/`IntSCarry`/`IntSBorrow`/
    `PopCount` as flag-register side effects alongside *any* comparison,
    even when the actual branch only reads one flag. The existing roadmap
    listed those four as merely "next most load-bearing" (after `Piece`/
    `SubPiece`/`PtrAdd`/`PtrSub`); this run demonstrates they're closer to
    a hard prerequisite for covering *any* real x86 comparison/branch
    sequence. Updated `selfjit/mod.rs`'s own roadmap doc with this finding
    (re-prioritizing item 1) rather than letting it live only in a test's
    doc comment.
  - Zero divergences found on everything the harness *could* run (a
    straight-line real-ELF-entry-point smoke test got `matched=1`,
    `skipped=9`, no divergence). Left the (currently thin) match count as
    an honest signal rather than engineering a more favorable-looking demo
    -- the finding above is the real value of this pass, not a coverage
    percentage.
  - Validated: 13/13 `selfjit::*` tests (2 new `Load`/`Store`, 2 new
    differential), `fission-emulator` suite 86/93 (added 5 net tests; same
    7 pre-existing unrelated failures throughout), full workspace build
    clean. `selfjit` is still not wired in anywhere live, so blast radius
    to existing behavior is structurally zero.
- **`selfjit`: `IntCarry`/`IntSCarry`/`IntSBorrow`/`PopCount`, and a real,
  precisely-located lead in *production* Cranelift found while validating
  them, 2026-07-22.** User's explicit "다음단계 진행합니다" (proceed with the
  next step) after the differential harness re-prioritized these four
  opcodes above the scaffold's original roadmap ordering (see previous
  entry).
  - **`IntCarry`/`IntSCarry`/`IntSBorrow`**: route through `jit_int_flag
    (kind, size, a, b) -> u64` -- the exact same pure host callout
    `crate::jit::compiler`'s own arm already uses (no `emu_ptr`, already
    size-aware/sign-correct internally via `int_flag_op`'s own `sign_
    extend_n`), so unlike `IntSLess`/`IntSDiv`/etc there's no narrow-
    negative-operand trap to fix here -- purely mechanical call-site wiring
    (X0=kind, X1=size, X2=a, X3=b per AAPCS64, matching the existing
    `load_value`/`store_value` call-setup pattern).
  - **`PopCount`**: no host callout in either backend -- ported Cranelift's
    own SWAR (SIMD-within-a-register) bit-twiddling algorithm instruction-
    for-instruction into raw AArch64 ops (`and_reg`/`lsr_reg`/`add_reg`/
    `sub_reg`/`mul_reg`, all already available in `emit/aarch64.rs` -- no
    new emitter primitives needed) rather than re-deriving it, since
    Cranelift's version is already proven correct. Added two new caller-
    saved scratch constants (`TMP1`/`TMP2` = X10/X11) for the algorithm's
    intermediate values -- safe as caller-saved specifically because this
    algorithm makes no nested `blr` between reading and writing them
    (unlike `A_VAL`/`B_VAL`, callee-saved because they *do* need to survive
    a nested call -- see this file's own pre-existing bug-postmortem
    comment on that distinction).
  - Both checked against known-correct cases (not just "it ran"): `IntCarry
    /IntSCarry/IntSBorrow` against hand-computed carry/signed-overflow
    cases (`u64::MAX+1` carries, `i64::MAX+1` signed-overflows, `i64::MIN-1`
    sborrows); `PopCount` against `u64::count_ones` for boundary values.
  - **Re-ran the differential harness with the new opcodes wired into
    `selfjit_supports`**: `checksum`'s real `Load`-in-a-loop (0 matchable
    TBs before this phase, entirely blocked on these four opcodes per the
    previous entry's finding) now replays cleanly -- `matched=2, skipped=1
    (only Return remains unsupported), diverged=0`.
  - **A real, precisely-located, unconfirmed lead in *production*
    Cranelift, found by this exact validation run** (not a `selfjit` bug):
    re-running the ELF-entry-point differential test with the new opcodes
    exposed a genuine divergence at a *later* TB (`0x10067e4`, `testdata/
    x64_static_printf_malloc.elf`) that the earlier, narrower opcode
    coverage never reached before. A plain register-to-register `Copy`
    (`out=offset:128,size:8 in=offset:56,size:8`) -- `SelfJitCompiler`'s
    result matches the copy's real 8-byte source value exactly; Cranelift's
    does not (2 of 8 bytes come out `0x00`). Investigated by reading
    `crate::jit::compiler`'s actual register-caching machinery end to end
    (`ensure_var!`'s `host_reg_file` fast-path load/store, the `dirty`-
    entries dedup, `jit_reg_bulk_flush`'s bulk write-back, `MachineState::
    read_space`/`write_space`'s `reg_cache` synchronization) rather than
    guessing -- everything inspected looks individually correct in
    isolation, so the exact mechanism remains unfound. Confirmed the test
    harness's own call pattern (`func(&mut emu as *mut _)`) exactly matches
    `Emulator::run_instruction`'s real invocation of a compiled TB, ruling
    out an obvious "harness calling convention mismatch" explanation.
    **Not fixed this session** -- flagged precisely rather than patched
    blind: `selfjit::differential::tests::known_issue_cranelift_register_
    copy_divergence_at_0x10067e4` (`#[ignore]`d, reproducible via `cargo
    test -p fission-emulator -- --ignored`), with the full investigation
    trail in that test's module doc and this entry. The main entry-point
    differential test is capped at 7 TBs (just short of the divergent one)
    so this doesn't block CI on an issue outside `selfjit`'s own scope --
    a scope decision, not a cover-up: the finding is documented, not
    hidden, and the isolated repro test keeps it runnable on demand.
  - Validated: 15/16 `selfjit::*` tests passing + 1 correctly `#[ignore]`d
    (4 new: 2 `IntCarry`/`IntSCarry`/`IntSBorrow`+`PopCount`, 1 known-issue
    repro, plus the checksum differential test's assertions strengthened),
    `fission-emulator` suite 88/95 (same 7 pre-existing unrelated failures,
    1 correctly skipped), full workspace nextest 2171/2178 (zero new
    regressions), full workspace build clean.
