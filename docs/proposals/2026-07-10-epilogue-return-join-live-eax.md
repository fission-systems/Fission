# Decompiler Change Proposal: Epilogue return-join live primary return

## 1. Baseline Row Anchor

- Binary: `control_flow_gcc-m32_O2.exe`
- Function: `saturating_add` @ `0x401680`
- Current: multi-path RET (pop ebp; ret) emits bare `return;` while EAX holds
  `a+b` / cmov INT_MIN; only `return 2147483647` on the dedicated INT_MAX block
- Semantic: overflow/underflow cases fail compile or wrong value

## 2. Owner Proof

- [x] Builder/control terminator: `lower_return_terminator_impl`
- SLEIGH Return input is the **return address** (stack load), not EAX
- Shared epilogue block has no EAX def; values come from predecessors
- `predecessor_primary_return_expr` requires all pred exprs equal → fails when
  arms differ (sum vs INT_MIN); recovery then drops to `return;`

## 3. Generality / Invariant Proof

```text
When a multi-predecessor return block has no primary-return-register def and
only epilogue-side-effect noise before RET (epilogue-style return join), the
returned expression is the live primary return register binding at the join.
Differing predecessor values are path-sensitive writes into that register;
they must not require equal lowered exprs to recover `return reg`.
```

ISA-agnostic: primary return register from namer; epilogue join via CFG preds +
absence of local return-reg def (not x86-only string gates).

## 4. Risk

- Over-admitting impure exits as return joins → mitigated by existing
  `is_epilogue_style_return_join_block` guards (pred≥2, no local return def,
  no side-effect consume of return reg)
- Must not break count_bits loop exit (explicitly avoided by those guards)

## 5. Validation

- Synthetic: multi-pred epilogue RET with live EAX → `return eax` / `return uVar`
- Real: saturating_add no empty `return;` on non-void paths
- Regression: count_bits loop still structures
