# Decompiler Change Proposal

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/dev/binaries/control_flow_gcc_O0.exe`
- Function: `signum` @ `0x14000158c` (also affects `saturating_add`, any Jcc on cmp)
- Corpus: fission-benchmark dev control_flow gcc -O0
- Current output (local main before fix):
  `xVar8 = param_10 ? 1 : 0 <= param_10 ? 0 : 4294967295;`
  → non-zero always returns 1 (negative values wrong)
- Semantic: 1/5 cases (zero path only)
- Failure category: runtime_error / wrong signed compare recovery
- Asm: `cmp; jle` then nested `cmp; jns` — classic signed three-way

## 2. Owner Proof

- [x] Builder/materialize (register naming): first wrong fact
- [ ] Flag recovery: secondary (patterns exist but never see `zf`/`sf`/`of`)

Evidence:

```text
Raw p-code (register space size=1):
  ZF @ off=0x206, SF @ 0x207, OF @ 0x20b  (SLA: define register offset=0x200 size=1 [CF … ZF SF … OF])

RegisterNamer::hw_name_at intentionally returns None for 0x200..0x280 size=1,
so lower_expr falls back to the single name "reg". All flag writes alias:
  reg = __sborrow(...); reg = (diff < 0); reg = (diff == 0);
JLE condition BoolOr(ZF, SF!=OF) collapses to a ZF-only / truthiness form.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
x86 SLA register-space EFLAGS bit varnodes (size 1, offset 0x200 + bit)
must retain distinct canonical names {cf,pf,af,zf,sf,of,...} matching
utils/sleigh-specs/languages/x86/x86-64.slaspec and UNIQUE-space
unique_x86_register_name bit layout. Distinct names are required for
flag_recovery Jcc patterns (JLE = zf || sf!=of, etc.).
```

Comparable coverage:

- Similar shape 1: `clamp` jge/jle chains
- Similar shape 2: `saturating_add` signed overflow checks
- Synthetic invariant test: namer unit test + JLE HIR flag_recovery test

## 4. Risk And Ownership Check

- Existing owner: `RegisterNamer::hw_name_at` / SLA register map
- Shared analysis: none new
- Extending namer is sufficient; no new pass
- Telemetry: none
- Must not change: UNIQUE-space naming; non-flag 0x200..0x280 sizes

## 5. Validation Matrix

- [x] Targeted invariant test: register-space flag names + JLE recovery
- [ ] crate nextest fission-pcode (flag_recovery + register_model)
- [ ] Remeasure signum/clamp/saturating_add decomp
