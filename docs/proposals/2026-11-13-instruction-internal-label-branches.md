# An instruction-internal branch is a constant-space op delta, and a select's two halves are one value

Filled from `docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`.

## 1. Baseline

AArch64 `csel`, `cinc` and `cset` (and the MIPS equivalents) lift to
`tmp = a; if (cond) goto L; tmp = b; L: use(tmp)`. Fission printed a call to a
function at address 2 and dropped the select:

```
int sum_to(int n) { sub_2(); return n + ...; }     // csel w0, wzr, w8, lt
```

Measured with `scripts/quality/support_matrix_probe.py` on one probe program
cross-compiled at `-O1` for each cell (counting functions whose output calls
`sub_N()` with N in `0x1..0xfff`, an address that is never code):

| cell | functions | with a phantom `sub_N()` |
|---|---|---|
| ELF AArch64 | 66 | 24 |
| ELF MIPS32 BE / LE | 67 / 67 | 21 / 21 |
| PE ARM64 | 531 | 239 |
| Mach-O arm64 | 6 | 3 |
| ELF x86-64, ARM32, PPC64, PE x64/x86 | 54, 69, 53, 77, 76 | 0 |

The same shape on a Cortex-M `thread_sleep` (DecBench sample set, `-O0`, Thumb
`mrs`): a phantom `sub_4()` and an unconditional `getCurrentExceptionNumber()`.

## 2. Owner

Two owners, found in this order:

1. **The lifter.** `template_eval.rs::normalize_direct_control_target` turns a
   constant branch destination into a code-space address. It spared a label
   destination only by recognising the relative sentinel *by value*, but the
   sentinel is masked to the destination's 4-byte width, so it no longer looks
   like one and the label was rewritten to an address (`space 3`, not constant).
   Every consumer (`relative_pcode_target_seq`, `instruction_local_branch_target_seq`)
   reads a relative branch as "constant space, signed op delta", so none saw it.
   The `.sla` is right (`const`, `Relative`, size 4); the same shape on the Ghidra
   12.0.4 and 12.2 snapshots.
2. **The builder.** With the branch recognised, the select is lowered, and a
   second defect appears that the phantom call had hidden: the value written
   before the skip was bound to a different variable than the guarded write, so
   the skipped path read an uninitialised local. `prove_same_block_register_join`
   joins the two writes only for registers, and then only if the SSA Cover does
   not claim the default value is live across the guarded write, which is the
   select's own shape (the SSA has no join for an instruction-local skip).

## 3. Invariant

- A label destination stays in the constant space; `finish` patches it to the op
  delta, as Ghidra's `PcodeCacher` does. Decided from the template (`goto <label>`),
  not from the value.
- Two writes to one storage made by the same machine instruction and separated only
  by that instruction's skip are one value: they share a binding, and the Cover
  test, which exists to stop unrelated reuse of a name, does not apply. A unique
  temporary is such a carrier as well as a register.

No ISA-specific rule: nothing names AArch64 or an opcode.

## 4. Validation and results

- Targeted tests: `instruction_internal_label_branches_are_constant_space_op_deltas`
  (lifter, AArch64 `csel` and x86 `tzcnt`/`bsf`; fails with the old normalizer);
  `instruction_local_select_through_a_temp_keeps_both_arms` (builder; fails with the
  builder change reverted); `aarch64_csel_cli` (end to end, oracle = `fission_cli
  verify` ground truth, i.e. the emulated machine code; fails with both reverted).
- `fission-sleigh`, `fission-pcode`, `fission-pcode-ir`, `fission-midend-normalize`,
  `fission-decompiler`, `fission-emulator`: 2,676 tests pass; fmt and the CI clippy
  line pass.
- Probe, base vs fixed release builds, same binaries, caches disabled: phantom calls
  AArch64 24 -> 0, MIPS 21 -> 0, PE ARM64 239 -> 0, Mach-O arm64 3 -> 0 (RISC-V 3 ->
  0, x86-32 1 -> 0); every other cell unchanged; `decomp --all` still returns code
  for the same functions.
- `fission_cli verify` ground truth against the emulator, on the fixture and the
  probe's `sum_to`: `sel_zero`, `sel_two`, `sel_inc`, `sel_cst`, `sum_to` are all
  `Equivalent` (before the fix they were `Unsupported` because of the phantom call;
  with the lifter fix alone `sum_to` and `sel_cst` were `Diverged`).
- DecBench sample set (same scorer for both builds): GED perfect 68/233 before and
  after, types perfect 16/222 before and after, total distance 7567 -> 7575. One row
  of 233 moved: Cortex-M `thread_sleep`, +8. The new output is the more faithful one
  (no phantom call, and the guarded `getCurrentExceptionNumber()` is guarded); the
  structure score compares node counts and prefers the larger output there, so this
  is **not** a quality-score claim.

## 5. What this does not fix

A wider builder rule (a def shadowed only by a guarded write keeps a binding; a
guarded write no longer ends an earlier def in the use scans) also fixed the select
but changed x86 `cmov` output (`history_search_pos` came out with the wrong
condition), so it was dropped; the x86 `cmov` default comes from an earlier
instruction and keeps the Cover test. `count_char` (`cinc x0, x0, eq`, where `x0`
is both the pointer parameter and the counter) now decompiles and `verify` reports
`Diverged`: the parameter and the counter share a name. That defect was already
there, behind the phantom call.

## 6. Follow-up: one rule instead of two (2026-11-13)

`count_char` (`cinc x0, x0, eq`) showed that the builder half of the fix above was
placed too late. A naming heuristic earlier in the chain
(`live_register_lhs_name_for_passthrough_join_store_producer`) named the guarded
half of the select after the register it is copied into -- `x0`, whose ABI
parameter name it picked even though `x0` had been reassigned before the loop -- so
the two halves were split again and the function returned the pointer parameter.

Both earlier builder rules (a unique temporary allowed in
`prove_same_block_register_join`, and the Cover bypass there) are replaced by one
rule at the head of the naming chain: the guarded half of one instruction's select
takes the binding of its default half. Removing either earlier rule with this in
place leaves every measured function `Equivalent`, so they are gone.

- `verify` ground truth on the probe's AArch64 ELF: 9 `Equivalent`, 0 `Diverged`
  (after the first fix: 8 and 1; before it: 7 and 0, the rest `Unsupported`).
- DecBench sample set against the first fix: no GED row moved (total distance 7575,
  perfect 68/233); types perfect 16/222 -> 17/222.
