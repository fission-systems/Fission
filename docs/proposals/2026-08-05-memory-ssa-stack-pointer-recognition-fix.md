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

## 8. Third pass: the phi-connectivity gap was `write_effect`, not the union-find

Section 7 guessed the gap was in `build_memory_ssa`'s phi-operand
population. Direct tracing (`FISSION_MEMHIGH_TRACE2`, printing every
`memory_values` entry's storage/definition alongside its `dynamic_guards`
entry when applicable, removed after use) proved that guess wrong and found
the real mechanism.

`matrix_multiply`'s offset `-44` (`local_2c`) has 6 memory values: an
`Input`, two `Phi`s (blocks 1 and 3, correctly forced-union-chained via
`union_values` into one 5-member group -- the phi machinery was never
broken), and three `Operation`-defined values. Two of those three
(`SsaOpSite{block:2,op:2}` and `{block:7,op:59}`) have
`guard=Some((Some(Stack), Exact, Store))` -- genuine, precisely-resolved
stack writes, correctly included in the phi-chain group. The third,
the one causing the false positive
(`SsaOpSite{block:7,op:42}`), has `guard=Some((None, Unknown, Store))`.
Cross-referencing the raw p-code: this op is `local_18[computed_index] =
xmm0_da` -- a store through the function's *output array parameter*,
nowhere near offset `-44`. Its pointer chain (rooted in a parameter
register, not the stack pointer) correctly fails to resolve to any region
at all in `resolve_pointer_value`.

**Root cause**: `MemoryLayout::write_effect` (scalar_ssa.rs), when a
guard's `region` is `None`, conservatively assumed the write "might touch
any storage sharing the same raw p-code space id" -- which, because
`SsaMemoryStorageKey`'s `space_id` is just the p-code memory-space number
(shared by *all* pointer-based accesses, stack or heap, in this SLEIGH
model) and not itself region-discriminating, silently included every
*Stack* partition too. Every write through a pointer that doesn't
provably trace back to the stack pointer (the overwhelmingly common case:
any store into a parameter/heap/array pointer) fabricated a phantom
"definition" of *every* promoted stack slot in the function. Since a
phantom value has no real value-flow relationship to anything, it never
ends up as a phi operand, and sits alone in its own `SsaMemoryHighVariable`
group with a narrow, spuriously-interfering cover.

**Fix**: narrowed the `None`-region branch to exclude `SsaMemoryRegion::Stack`
partitions. Justification: given this session's earlier fix
(Section 2-3) made `resolve_pointer_value` correctly resolve *every*
stack-pointer-derived chain to `Some(Stack)`, a `None` result is now a
*positive* proof the pointer's def-use chain does not trace back to the
stack pointer -- and a non-escaping local (never had its address taken)
cannot be written through an unrelated pointer under normal C/C++
semantics. `read_effect` needed no change: it only ever expands through
`exact_access`, which already requires `SsaGuardRangePrecision::Exact` and
was never affected by this gap.

### Validation

- [x] `cargo nextest run -p fission-pcode`: 962 passed, 1 skipped.
- [x] `cargo nextest run --workspace` (excluding `selfjit_matches_cranelift`):
      only the same pre-existing, unrelated `fission-emulator` baseline
      failures.
- [x] Concrete repro (`matrix_multiply`, `memory_layouts_clang_O0.exe`):
      the `stack_slot_cover_violation` for `local_2c` is gone.
- [x] Before/after decompile output on a real corpus binary: byte-identical
      (this subsystem still has no production consumer).

### Real-corpus re-measurement (dev + realworld + holdout + adversarial)

| Bucket | Before this fix | After |
|---|---:|---:|
| `stack_slot_cover_violation` instances | 908 | **225** |
| Distinct functions with >=1 violation | 185 | **113** |

### What's left: a second, separate, larger gap

Tracing the smallest remaining case (`___w64_mingwthr_add_key_dtor`'s
`home_0`, a register spill slot never address-taken) found a *different*
mechanism responsible for most of what's left: `write_effect`'s `Call`
case unconditionally treats every function call as writing to every
promoted stack slot (`guard.kind == SsaDynamicGuardKind::Call =>
self.storages().collect()`). This is a deliberate, generally-*correct*
conservative assumption -- a callee genuinely can write to a caller's
stack slot if its address escaped into an argument -- but is imprecise for
the common case of a non-escaping local sitting near an unrelated call
(`calloc`/`EnterCriticalSection`/`LeaveCriticalSection` in the traced
case, none of which receive `home_0`'s address). Closing this gap properly
needs real escape analysis (does any instruction take a given slot's
address and pass it to a call) that does not exist anywhere in this
memory-promotion-SSA subsystem yet -- a distinctly larger piece of work
than the two fixes landed this session, called out explicitly rather than
attempted in the same pass. `scan_stack_slot_cover_violations`'s doc
comment documents this precisely so a future session doesn't have to
re-derive it.

## 9. Fourth pass: real escape analysis, closing the `Call` gap

User explicitly chose to build the escape analysis rather than stop at
Section 8's guard-narrowing pattern, accepting the larger scope.

**Design.** A `Call`/`CallInd` p-code op carries exactly one input (the
target) in this codebase's lifted p-code -- confirmed by inspecting real
corpus raw p-code, not assumed -- so arguments are never data-flow-connected
to the call op itself; they're register- or stack-resident by calling-
convention position. This rules out "does this call's own inputs include a
stack address" as a viable check and requires two separate mechanisms:

- **Register-ABI conventions** (x64, AArch64, etc.): a stack address is
  "taken" if it is ever assigned into one of `RegisterNamer::int_param_offsets`
  (already-existing per-calling-convention ABI knowledge, reused rather than
  duplicated) -- deliberately whole-function-wide rather than per-call-site
  (if a slot's address is ever placed in an argument register anywhere,
  every call in the function conservatively might receive it; this trades
  precision for not needing per-call-site reaching-value analysis).
- **Stack-ABI conventions** (x86-32 cdecl/stdcall): arguments are pushed via
  ordinary `Store` ops before the call, with no argument-register
  involvement at all. Handled by the same general mechanism: an address is
  "taken" if it is ever used as a `Store`'s *value* operand (spilled/staged
  anywhere, not just at a call -- also covers `&local` written into a
  global or struct) or a `Return`'s operand.

Implemented as `compute_escaping_stack_storages` (scalar_ssa.rs), run once
per function inside `MemoryLayout::build` (now takes `pcode`/`ssa`/`options`
in addition to `guards`) and stored on a new `MemoryLayout.escaping` field.
`write_effect`'s `Call` case now returns `self.escaping` instead of
`self.storages()`.

**A real bug found via the test suite, not corpus tracing this time**:
`validate_scalar_ssa_with_context`'s cross-check re-derives `memory_values`
via a *second* `build_memory_ssa` call against a deliberately sparse
`expected_memory: NirScalarSsa` that historically only needed
`dynamic_guards` populated (memory-SSA construction never touched scalar
`values`/`operation_inputs`/`phis` before this session). `compute_escaping_stack_storages`
breaks that assumption -- it resolves pointer chains through exactly those
fields. Running against the sparse re-derivation, escape resolution
silently found nothing (empty `values` means every `resolve_pointer_value`
call fails immediately), diverging from the real build and failing
`StoragePartitionsMismatch` on a legitimate escaping-local test case. Fixed
by also cloning `values`/`operation_inputs`/`operation_outputs`/`phis` into
`expected_memory`.

**Test coverage**: the existing
`signed_stack_location_is_promoted_and_unknown_call_kills_it` test asserted
the *old* behavior (an unrelated unknown call always kills a stack write) as
a hard invariant -- exactly the imprecision this pass fixes. Split into two:
`..._does_not_kill_non_escaping_local` (address only ever dereferenced
directly -- must survive the call unchanged) and `..._kills_escaping_local`
(address also stored to an unrelated location before the call -- must still
be killed). Both pass, and between them pin down the exact boundary this
analysis draws.

### Validation

- [x] `cargo nextest run -p fission-pcode`: 963 passed (962 prior + replaced
      test), 1 skipped.
- [x] `cargo nextest run --workspace`: only the same pre-existing,
      unrelated `fission-emulator` baseline failures.
- [x] Concrete repros: both `matrix_multiply`'s `local_2c` (Section 8) and
      `___w64_mingwthr_add_key_dtor`'s `home_0` (this section) no longer
      appear in `scan_stack_slot_cover_violations`'s output.
- [x] Before/after decompile output on a real corpus binary: byte-identical
      (still diagnostic-only, wired into no production decision).

### Real-corpus re-measurement (dev + realworld + holdout + adversarial)

| Bucket | After Section 8 | After this pass | Total change from Section 7 baseline |
|---|---:|---:|---:|
| `stack_slot_cover_violation` instances | 225 | **12** | 908 -> 12 (**98.7%** reduction) |
| Distinct functions with >=1 violation | 113 | **7** | 185 -> 7 |

### What's left: a precision limit, not a bug

Traced the smallest remaining case (`_power`, `math_gcc-m32_O2.exe`, 1
violation). Every contributing memory value has a clean, genuine
`(Some(Stack), Exact, Store)` guard -- no phantom write-effect source this
time. The forced-union phi chain is also complete and correct (verified by
hand-tracing all `memory_phis` operand/output pairs for the offset into one
5-member union-find group). The false positive instead comes from
`SsaMemoryHighVariable`'s cover being a single *merged, per-block* range: a
5-member loop-carried phi-chain group's members individually touch
different, non-overlapping sub-ranges of a block, but their *merged* cover
spans the block almost entirely (`0..131`), which then numerically
"interferes" with a third, genuinely disjoint value's narrow real range
(`88..100`) even though no actual member was live during that window.

This is the same class of approximation Ghidra's own block-granular
`Cover`/`Merge` machinery has -- not specific to this implementation.
Resolving it precisely would need per-value point-in-time liveness instead
of per-block ranges, a materially larger redesign of the Cover
representation itself (affecting the scalar side too, not just memory).
Left as documented, accepted residual imprecision rather than pursued
further this session -- diminishing returns at 98.7% already reached, and
the remaining mechanism is understood well enough that a future session
doesn't have to re-discover it via tracing.
