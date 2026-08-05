# Decompiler Change Proposal: Memory-Promotion-SSA Stack-Pointer Recognition Fix

Date: 2026-08-05

## 1. Context

Following the register-name Cover-violation work earlier today
(`2026-08-05-cover-violation-diagnostic.md`), the user asked to keep
surveying the codebase for "implemented but never wired up" subsystems.
The standing, explicitly flagged candidate was the memory-promotion-SSA
subsystem landed 2026-07-26
(`docs/proposals/2026-07-26-heritage-memory-promotion-and-cover-coalescing.md`):
`NirScalarSsa.memory_values` / `memory_inputs` / `memory_operation_inputs` /
`memory_operation_outputs` / `memory_phis` -- exact stack/RAM address SSA
chains, computed in `scalar_ssa.rs`, with zero external readers.

The plan was to build a Cover/HighVariable-equivalent grouping layer on top
of `memory_values` (mirroring `build_out_of_ssa_facts`), then wire it into
`stack_slots.rs`'s flat `self.locals: BTreeMap<i64, StackSlot>` naming
(which has no size field at all, so two different-sized stack variables
placed at the same offset by the compiler at different points in a
function would silently share one display name -- the memory-side analog
of the register bug fixed earlier today).

## 2. A deeper problem than "unconsumed"

To find concrete repro cases before designing the Cover layer, a
diagnostic (`stack_slot_diagnostics.rs`, `scan_stack_slot_size_ambiguities`)
was built to cross-check `self.locals`' offsets against
`scalar_ssa.memory_values`'s Stack-region storage keys. Running it against
the real corpus produced **zero results** -- including on trivially simple
functions with obvious stack locals (`_checksum`, 3 locals, straightforward
`gcc -O0` code).

Tracing directly (temporary `[STACK_TRACE]`/`[MEMSSA_TRACE]`/`[PTRRES_TRACE]`
eprintln instrumentation, removed after use) showed `NirScalarSsa.memory_values`
was **completely empty** for this function, not just missing size
ambiguities. This is a materially different and more foundational problem
than "computed but unconsumed": the analysis was not producing meaningful
data at all under realistic conditions.

## 3. Root cause

`scalar_ssa.rs`'s `resolve_pointer_value` recognizes the function's stack
pointer only at its `SsaValueDefinition::Input` case:

```rust
SsaValueDefinition::Input => (value.storage.space_id == REGISTER_SPACE_ID
    && options.cspec_stack_pointer_offset == Some(value.storage.offset))
    .then(|| PointerRange::exact(SsaMemoryRegion::Stack, 0)),
```

Two independent problems, confirmed in order via direct tracing (not
assumed):

1. **Wrong space-id constant.** `REGISTER_SPACE_ID` (`builder_types.rs`) is
   the *legacy* register-space id (`1`). The active rust-sleigh backend
   uses `RUST_SLEIGH_REGISTER_SPACE_ID` (`4`) -- confirmed directly via
   `[PTRRES_TRACE]`: the stack-pointer SSA value's `storage.space_id` was
   `4`, so `value.storage.space_id == REGISTER_SPACE_ID` was `false` for
   *every* function on this backend, unconditionally. The codebase already
   has the correct helper for this exact problem,
   `is_register_space_id()` (checks all three known space ids -- legacy,
   rust-sleigh, rust-sleigh-alt), used correctly everywhere else in the
   builder (`stack_slots.rs`, `materialize/`) -- `resolve_pointer_value`
   was simply not calling it.

2. **`options.cspec_stack_pointer_offset` is a real value in practice**
   (this contradicts an earlier, wrong hypothesis in this session that it
   requires per-function prototype/cspec data and is therefore rarely
   populated -- `[PTRRES_TRACE]` showed it correctly set to `Some(16)`,
   matching the real ESP offset, for a plain stripped test binary). So (1)
   was the actual, sole blocker; `cspec_stack_pointer_offset` was a red
   herring. Still, `resolve_pointer_value` was the *only* place in the
   builder depending on this cspec-sourced field for stack-pointer
   recognition, while `stack_slots.rs`'s `resolve_stack_address_inner`
   (the production stack-slot resolver) has always used
   `options.calling_convention`-keyed hardcoded per-architecture register
   offsets instead, needing no cspec data. Added a second source
   (`CallingConvention::native_stack_pointer_register_offset`, mirroring
   `resolve_stack_address_inner`'s existing per-arch table so a second
   consumer doesn't have to duplicate or drift from it) as a fallback, so
   `resolve_pointer_value` no longer has a *sole* dependency on cspec data
   for basic stack-pointer recognition, matching `resolve_stack_address_inner`'s
   robustness.

## 4. Fix

- `crates/fission-core/src/core/calling_convention.rs`: new
  `CallingConvention::native_stack_pointer_register_offset(self, is_64bit) -> Option<u64>`,
  extracting the per-architecture RSP-equivalent register offset table
  (mirrors the `StackBase::Rsp` arms already hardcoded in
  `stack_slots.rs::resolve_stack_address_inner`, so both consumers share
  one source of truth going forward).
- `crates/fission-pcode/src/midend/builder/scalar_ssa.rs`:
  `resolve_pointer_value`'s `Input` case now uses `is_register_space_id`
  (was: `== REGISTER_SPACE_ID`) and accepts either
  `options.cspec_stack_pointer_offset` or the new calling-convention-based
  offset.
- Also landed alongside (from the original diagnostic-first plan, kept as
  a real, currently-inert-but-ready diagnostic):
  `crates/fission-pcode/src/midend/builder/memory/stack_slot_diagnostics.rs`'s
  `scan_stack_slot_size_ambiguities`, gated behind `FISSION_PREVIEW_DIAG`
  like `scan_cover_violations`.

## 5. Validation

- [x] `cargo nextest run -p fission-pcode`: 962 passed, 1 skipped -- zero
      regressions from either the space-id fix or the new calling-convention
      fallback.
- [x] Direct before/after decompile-output diff on a real corpus binary
      (`control_flow_gcc-m32_O0.exe`, `fission_cli decomp --all`, git-stash
      A/B): **byte-identical**. Expected and correct -- `dynamic_guards`'
      `region`/`memory_values`/`memory_operation_inputs`/`memory_operation_outputs`/
      `memory_phis` still have no production consumer (confirmed:
      `crossing_guards`, the one `high_variables` field that *is* now
      consumed via this session's earlier Cover-violation guard, is never
      populated from `dynamic_guards.region` in a way that reaches
      `high_variables_interfere`, which only reads `cover`). This fix
      makes the underlying analysis produce **correct data**; it does not
      yet change decompiler output, because nothing downstream reads that
      data. That remains the next phase of work.
- [x] Real-corpus population check (`gcc`, `gcc-m32`, `clang` builds,
      dev corpus): `memory_values` now non-empty for essentially every
      function with real stack locals (e.g. `_checksum`: 0 -> 13;
      `rc4_init`: 0 -> 66), confirmed via temporary
      `[MEMVAL_COUNT]` instrumentation (removed after use).
- [x] `scan_stack_slot_size_ambiguities` re-run against the real corpus
      post-fix: zero ambiguities found. A real, non-buggy result --  the
      corpus's straightforward test-suite functions at these optimization
      levels don't happen to exercise same-offset/different-size stack
      slot reuse; the diagnostic is verified working (confirmed via
      `memory_values` population) and stays in place for when it's needed
      against harder corpora or as a live gate once a fix is designed for
      whatever it eventually finds.

## 6. What this does *not* do yet

This fix is a "pure enablement" change: `dynamic_guards`'s Stack-region
resolution, `memory_values`, `memory_operation_inputs`/`outputs`, and
`memory_phis` are now correct and populate broadly, but remain otherwise
exactly where they were before this session -- computed, self-consistent,
validated by their own tests, with zero external readers. `self.locals`
(`stack_slots.rs`) still has no size discriminant and still cannot detect
the same-offset-different-size stack-slot-reuse case this investigation
originally set out to measure. Building a real consumer (either a memory
Cover/HighVariable layer mirroring `build_out_of_ssa_facts`, or a lighter
per-storage-key live-range check directly gating `ensure_stack_slot_binding`)
is the natural next step, and now has real data to work from.

## 7. Second pass: a memory Cover/HighVariable layer, and why it's not ready

With real `memory_values` data available (Section 5), the next step was
building the Cover/HighVariable layer originally planned: does `self.locals`'
flat, identity-blind offset keying conflate two logically different stack
variables that happen to share one offset?

**A same-size blind spot in the first diagnostic.** `scan_stack_slot_size_ambiguities`
(Section 4) only catches different sizes at the same offset. Tracing a real
function (`util_app_gcc_O0.exe`'s largest function, `[STACK_DIST]` temporary
instrumentation, removed after use) showed this misses the actually common
case entirely: one stack offset commonly has 40+ separate `SsaMemoryValueId`s
in a single `-O0` function, *all the same size* -- normal SSA versioning
from one variable being written repeatedly (e.g. a loop-carried counter),
not evidence of anything wrong. Telling "one variable, many writes" apart
from "two variables, one disjoint-lifetime reuse" needs the Cover/interference
data, not a size or raw-count heuristic.

**New types**: `SsaMemoryHighVariableId`/`SsaMemoryHighVariable` (`ssa.rs`,
mirroring `SsaHighVariableId`/`SsaHighVariable` but for `SsaMemoryValueId`s)
and `NirScalarSsa.memory_high_variables`/`value_memory_high_variables`.

**`build_memory_out_of_ssa_facts`** (`scalar_ssa.rs`), mirroring
`build_out_of_ssa_facts`, but *forced-union only* -- there is no memory
analog of a scalar `Copy`/`Cast` speculative merge, since a `Store` never
reads a prior value at the address it writes (unlike a register `Copy`,
which explicitly does). `memory_phis` (control-flow-join merges) is the only
source of congruence between two `SsaMemoryValueId`s. `build_memory_value_cover`
mirrors `build_value_cover` verbatim, adapted to `SsaMemoryValue`/
`memory_operation_inputs`/`outputs`/`memory_phis`.

**`scan_stack_slot_cover_violations`** (`stack_slot_diagnostics.rs`): for
each `self.locals` offset, flags pairs of distinct `SsaMemoryHighVariable`s
with interfering covers -- the memory analog of `scan_cover_violations`.

### Real-corpus measurement: found a real signal, but not yet trustworthy

Corpus-wide (dev + realworld + holdout + adversarial, ~500+ functions):
**908 raw violation instances across 185 functions**. Before treating this
as a real bug count (the register-side work this session repeatedly showed
the cost of *not* verifying a raw diagnostic count before acting on it),
traced the smallest concrete case: `matrix_multiply` (`memory_layouts_clang_O0.exe`,
one violation, `local_2c` / offset `-44`, the innermost loop's induction
variable in a triple-nested matrix multiply).

The rendered pseudocode for `local_2c` is **completely correct and
unambiguous** (`for (local_2c = 0; local_2c < local_1c; local_2c = uVar63) { ... }`,
no visible naming conflict). The flagged pair (`SsaMemoryHighVariableId`
2 and 18) is a **false positive**: `memory_high_variables[18]` is a single,
short-lived memory value (cover `[block 7, 86..90)`) that was never
transitively unioned into the loop variable's phi-chain group, even though
tracing `ssa.memory_phis` directly shows the phi-chain *should* connect it
(nested-loop phi outputs correctly chain as operands of outer-loop phis for
this exact offset elsewhere in the same function). The exact reason this
one value's forced-union edge is missing was not isolated in this session
-- likely an incompleteness in how `build_memory_ssa`'s phi-operand list is
populated for this specific CFG shape (deeply nested loops), not in the
union-find/Cover logic itself (which is a direct, careful port of the
already-correct scalar version).

**Conclusion, stated plainly in the code** (`scan_stack_slot_cover_violations`'s
doc comment): this diagnostic's raw count is **not currently trustworthy**
as a bug count, and must not be used to gate any real materialize decision
until the phi-connectivity gap is found and fixed. It is real, tested,
additive infrastructure (962/962 `fission-pcode` tests pass with it in
place) and stays in the tree diagnostic-only, exactly as `scan_cover_violations`
did before this session's own root-cause work made it trustworthy enough to
act on.

### Honest status vs. the original plan

The original plan (Section 6) was "build the Cover layer, then wire it into
`ensure_stack_slot_binding`." That wiring step was **not** done this round
-- doing so on top of a diagnostic with a known, uninvestigated false-positive
source would repeat exactly the mistake this session's earlier register-name
work proved costly (a first "fix" that looked reasonable but was wrong,
requiring a full revert once traced). The concrete next step is narrower
and more tractable than "find real bugs": isolate why one specific memory
value's phi-operand edge in `matrix_multiply` didn't get captured, fix that,
then re-run this same corpus measurement to see how much of the 908 was
that one systematic gap versus real slot-reuse cases.
