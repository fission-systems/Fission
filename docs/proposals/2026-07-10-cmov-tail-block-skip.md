# Decompiler Change Proposal: Tail-of-block cmov skip

## 1. Baseline

- Binary: `control_flow_gcc-m32_O2.exe`
- Function: `saturating_add` @ `0x401680`
- Missing: under/overflow arm `cmovl eax, 0x80000000` when `b < 0`
- Assembly: after `cmp`/`mov edx, INT_MIN`, SLEIGH emits absolute `CBranch`
  to the next machine instruction (`ret` epilogue at `0x4016a2`), then a
  guarded `Copy eax <- INT_MIN` still in the **current** basic block

## 2. Owner

- [x] `nir/cfg.rs` same-block forward resolution + materialize consumer
- Existing absolute same-block skip only finds targets **inside** the block;
  here the target address is the **next block start**, so the skip was missed
  and the guarded copy was dropped or always/never applied

## 3. Invariant

```text
When a CBranch target address is strictly after the current op address, does
not appear as any op address in the current block, and all remaining ops in the
current block have addresses < that target, the branch is an instruction-local
skip of the block tail (cmov body). Resolve the exclusive end index to the
block end so materialize emits: if (!cond) { /* remaining ops */ }.
```

ISA-agnostic: address/CFG shape only; no CC enum.

## 4. Validation

- Unit: cfg absolute tail-skip resolves to `block.ops.len()`
- Unit: materialize `lower_block_stmts` wraps tail cmov body in `if` (synthetic)
- Real wiring: do **not** treat all same-block-forward CBranches as non-terminators
  without guarding INT_MAX/path quality — first attempt made `block_terminator_index`
  skip cmov guards and regressed saturating_add compare to `eax < eax`.

## 5. Shipped path (2026-07-10 follow-up)

1. Keep `eax = a+b` dominating all uses of the primary return live-in (F1/F3) —
   primary-return keep materialize + HW-name binding for return-reg writes.
2. **Terminator-side cmov tail (materialize only):**
   - `materialize_block_terminator_index` skips same-block-forward CBranch tails
     so `lower_block_ops_range` emits `if (!cond) { body }` (raw
     `block_terminator_index` unchanged for CFG consumers).
   - Guarded Copy of the ABI primary return register reuses the HW name
     (`eax`/`rax`), not a fresh `uVar` — temps were DCE'd and dropped INT_MIN.
3. Focused goldens: `sat_o2_cmov_block_probe_materialize`,
   `sat_o2_cmov_tail_renders_int_min_through_epilogue`; m32 O2 saturating_add
   shows `eax = param_1 + param_2`, compare `a` vs `sum`, and
   `eax = 2147483648` on the underflow arm.
