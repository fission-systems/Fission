# Loop entry definition binding reservations

## 1. Measured baseline

Baseline main `6ecd26d954274ab3cbd3f1a4fbf22bf59d96d427`, native CLI
SHA256 `dbbdf1d2ec469d810d6b04abc8372f83f4a1381f4992388f70aa98707d06a6d8`.
Fresh no-DB `decomp --addr ... --json --layer both --prehir` evidence is stored
under `benchmark/artifacts/decbench/2026-10-04-loop-entry-values` in the primary
checkout. Investigation traces are `/private/tmp/fission-loop-entry-baseline`.

| Program / configuration | Function / binary / address | Observed defect |
| --- | --- | --- |
| coreutils O2-noinline | main, bin_009.elf, 0x3920 | Entry load of optind reserves uVar44, emits rax instead; first loop guard reads uVar44 before its only latch assignment. NIR 9328 chars. |
| libexpat O2-noinline | xcscmp, bin_020.elf, 0x3350 | Entry character reserves uVar0, emits rax; loop comparison reads uninitialized uVar1. NIR 1079 chars. |
| libexpat O2 | notationCmp, bin_087.elf, 0x34d0 (unscored function) | Inlined character comparison reproduces entry/latch split. Entry character reserves uVar0, emits rax; loop comparison reads uninitialized uVar1. |

These are three distinct functions across two programs and two optimization
configurations. The two libexpat functions share a source comparison routine;
they are useful controlled inlining coverage, not independent defect counts.
At proposal time, focused real-binary output and def-use traces established
the defect; behavioral probes were added during validation. Do not claim
whole-function equivalence.
Accepted sample-set baseline: Union 75/250, GED 65/240 (mean 33.675), types
18/228, byte perfect 1/166, recompiles 133/166. Frozen fixed91 Union 23/91.

## 2. Owner proof

Owner: builder/materialize, before normalization. PreHIR already reads a
loop-carried name whose initializer is emitted under a different name.

```
[MINT] def@0x3b75:263 <- "uVar44"
materialized-output-binding block=0x3b75 op_seq=263 ... lhs=rax rhs=Var("optind")
if (uVar44 < argc) { do { ... uVar44 = ...; } while (...); }
```

`loop_phi_entry_binding_name` reserves exact defining-op materialization names
but inserts its merge reservation at the definition's own block. Existing
`merge_binding_name_for_materialized_output` only consults successor/merge
blocks, so the seed reservation loses to later ABI-surface name selection.
Same-storage widening aliases must retain the reservation as well. Diagnose
remaining narrow/wide cover interactions separately rather than bypassing
interference checks globally.

## 3. Generalized invariant

A proven loop phi entry definition reserved before statement generation must
initialize the same scalar carrier that its phi/latch readers consume. Consult
the reservation only when the exact block/op/storage definition table and
address/sequence materialization table agree. Unrelated definitions occupying the same register
remain separate; ABI hardware naming alone cannot override that identity.

Production conditions use SSA def sites, storage keys, existing merge tables,
and existing cover proofs, without function/address/ISA/compiler guards.
The result is better without referring to a score: the loop's first iteration
reads a value that was actually initialized at its incoming execution point.
Memory loads/calls stay at their original evaluation point and count.

## 4. Ownership / risk

Extend existing phi entry binding reservation and merge assignment owner.
No new pass, state map, production dependency, owner dependency, or telemetry.
Reference only: Ghidra `merge.cc` Merge::mergeOp and trimOpInput treat phi
operands as a shared value after cover restrictions, inserting copies at the
incoming execution point when required. No code copied or runtime dependency.

Risks: narrow/full-width alias identities, return surfaces, cmov defaults,
call result snapshots, hidden registers, multiple entry paths, parameter slots,
cover rejection, reserved values suppressed as passthrough expressions.
Maintain existing checks and add definition-specific tests. Do not initialize
all temporaries or move effectful initializers to function declarations.

## 5. Validation matrix

- Focused latch-before-entry regression, seed-first order, narrower/wider alias
  and unrelated redefinition controls using existing loop-carried tests.
- `cargo nextest run -p fission-pcode`; related normalization/structuring tests,
  `cargo check -p fission-pcode -p fission-decompiler`, strict clippy and fmt.
- Fresh release CLI, three motivating functions, inspect PreHIR/NIR/HIR and
  compare actual initialized carriers/effect sites.
- All 250 functions decompiled with no DB; all three DecBench metrics freshly
  evaluated after all outputs are ingested, separate checkpoint directory.
  Record perfect-function gains/losses, compiler count, types, byte and GED.
- External fission-benchmark Docker paired clean main/candidate dev6 and fixed
  unscored32 are go/stop regression evidence only. No tuning on that pool.
- Bounded execution for loop entry, zero iterations and latch updates where
  recovered output can be compiled; disclose wrapper/behavior limits.
- `python3 scripts/audit/nir_boundary_scan.py --root .`.

## 6. AI firewall / review

No other model or subagent asked for implementation advice. No identity enters
production conditions. Synthetic invariant coverage complements real anchors;
fixed regression pool is not a scoring/tuning target. No new metric or oracle.

## 7. Results

**Rejected for delivery after broader inspection.** The candidate retained
Union 75/250, types 18/228, byte 1/166 and recompiles 133/166. GED perfect
remained 65/240, but mean worsened from 33.675 to 33.720833. Fixed91 retained
23/91 and 67/91 recompiles. The exact touch loop slice passed 128/128
instrumented cases (baseline diagnostic sentinel 72/128), with no whole-function
claim. Tests: 1,939 passed, one existing skip; 250 repeated outputs identical.

Broader inspection of login showed new reads of the uninitialized reserved
`xVar460` in PreHIR snapshots at the error-reporting join. Existing unrelated
reads of that name already failed, but the additional consumers are a new
reservation propagation defect. Exact-definition reservations alone do not
reach all split/predecessor-copy emission sites. Perfect-count retention and
crate green do not prove this safe. The loop reservation production change and
its three tests were removed from the delivery patch; evidence is preserved.
The next cycle must preserve the reserved incoming value through cover rejection
and equivalent predecessor copies without block-wide renaming. Do not bypass
interference checks or describe this candidate as landed quality improvement.

The independent narrow subtraction storage-view fix is measured separately in
`2026-10-04-narrow-subtraction-storage-views.md`.

### Reservation scope correction during validation

Two intermediate candidates are rejected, not accepted quality evidence. The
first added same-block storage reservations for widening outputs only and did
not initialize the narrow character carrier. Adding the narrow reservation
then initialized the xcscmp entry carrier but made notationCmp read the later character carrier while
loading earlier pointer values. This is a definition-scope violation: storage
identity at block entry cannot represent a later definition in that block.

The final candidate uses existing `materialized_output_names` keyed by exact
block/op/storage and `materialized_vns` keyed by defining address/sequence.
Phi entry reservation no longer inserts a block-entry merge binding. The
merge materializer consults that exact definition reservation before ABI
surface selection, retaining existing cover checks. No new state map/pass.
A regression assertion reads an earlier definition after reserving the later
one; it must retain its original value. Existing ABI-slot reservation test
now checks the exact-definition table instead of the block-entry table.

Additional experiments exposing the widened-alias proof to all storage roles
and publishing the selected name at the loop head did not resolve the
remaining narrow/wide reader mismatch; both were removed. The scoped final
candidate retains only exact-definition seed reservation and its existing
materialization owner. Character loop comparisons still have a separate
cover/alias recovery defect, which must not be described as solved.

### Required narrow arithmetic operand view

Fresh candidate scoring retained Union 75/250 but lost fallbackSort's existing
recompilation (133 to 132/166), so that intermediate candidate was rejected. Actual
compiler diagnosis: integer loop-carrier `uVar1266 -= r11`, where r11 is a
pointer-typed full-width carrier. Raw p-code is integer subtraction on narrow
storage; PreHIR already lacks the RHS storage cast, proving builder ownership.
The existing `coerce_integer_storage_view` used by comparisons expresses the
same machine-read invariant and also serves narrow integer subtraction.
Unlike the unconditional varnode-width coercion experiment, it leaves already
correct, same-width integer carriers unchanged.

Measured repetition in cached raw real-binary p-code: fallbackSort (bzip2 O2,
69 subtractions narrower than the pointer width), touch main (coreutils
O2-noinline,16), xcscmp (libexpat O2-noinline,2). These are not equivalent
compile failures; they establish repeated narrow bit-vector subtraction across
three programs/two configurations. The motivating regression is the concrete
invalid integer-minus-pointer expression above. Do not claim gains for the
other anchors unless fresh measurements show them.

Rule: a subtraction whose output storage is narrower than the ABI pointer
width consumes integer views of its input storage, not the full carrier's
later inferred pointer type. Reuse the existing storage-view coercion helper;
full-width address arithmetic keeps its existing owner. No ISA guard, new pass,
helper, or dependency. Add focused 32-bit read of a 64-bit carrier coverage,
rerun full crates and all fresh metrics/pool gates. Do not patch benchmark
fixup or printer to accept the invalid expression.
