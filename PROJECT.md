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
      - **Still not done**: SIB addressing (`base+index*scale`), the
        "specific hash" (needs actual scalar values + relocation-awareness
        — `OperandType.isAddress`/`hasRelocation` in
        `MessageDigestFidHasher.java`), wiring a query hash through
        `FidbfDatabase::identify_by_hashes` into an actual
        decompiler-facing "identified function" fact, and non-x86
        architectures (only x86-64 validated so far).
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
