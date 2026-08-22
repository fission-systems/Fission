# Preserve scalar storage width through type recovery

## 1. Baseline Row Anchor

Measured against current `main` (`eb4f0de3c`) on the held-out
`fission-benchmark/corpus/dev` split.  The scan decompiled 146 C rows covering
GCC/Clang, x86-32/x86-64, at `-O0`, then replayed the type scorer's argument,
stack-offset, and name matching passes.  It found 19 matched 32-bit values
declared as `longlong`/`ulonglong`.

Representative rows:

| binary | function | address | source variable | current declaration |
| --- | --- | --- | --- | --- |
| `advanced_patterns_gcc_O0.exe` | `list_sum` | `0x140001530` | `int total` | `longlong local_4` |
| `data_structures_gcc_O0.exe` | `sum_array` | `0x140001530` | `int sum` | `longlong local_4` |
| `control_flow_gcc_O0.exe` | `checksum` | `0x1400015b4` | `unsigned int sum` | `ulonglong local_4` |
| `semantic_stress_gcc_O0.exe` | `rolling_hash32` | `0x140001530` | `uint32_t hash` | `longlong local_4` |
| `crypto_gcc_O2.exe` | `rc4_init` | `0x140001530` | `int key_len` | `longlong` parameter 3 |

The same O0 census spans six source programs and both GCC and Clang.  A
separate current O0/O2 argument scan found the `rc4_init` defect at both
optimisation levels, so the invariant is not confined to one compiler or
optimisation tuple.

Focused commands use `fission_cli decomp <binary> --addr <address> --layer
both --prehir --json --no-header --no-warnings`.  NIR and HIR currently agree
on each wrong declaration.  This proposal is type-recovery work; no semantic
case result is being claimed as its baseline.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize/type recovery
- [ ] Structuring
- [ ] Printer
- [ ] Benchmark/automation

The first wrong fact is created in builder heritage, before normalization.
The raw p-code for `list_sum` stores, loads, adds, and returns the accumulator
through 4-byte varnodes.  Return recovery reports the exact truncating shape:

```text
source=register:0:4
expr=Cast<u32>(Cast<u64>(Var("local_4")))
```

Nevertheless, the captured PreHIR and final NIR/HIR all declare both the
stack slot and function return as signed 64-bit:

```c
longlong list_sum(ulonglong param_1) {
    longlong local_4;
    ...
    return local_4;
}
```

Thus lifting retains the 4-byte machine operations, structuring only arranges
the loop, and the printer faithfully prints the type it receives. Diagnostics
showed `run_incremental_heritage` resolving every occurrence of a reused
unique-space address temporary without an operation site. The site-free def
lookup selected the later 8-byte definition and pre-created the real 4-byte
slot as 64-bit before ordinary lowering began.

The first site-aware implementation exposed a second owner-local defect. The
existing callee-saved push filter parsed `asm_mnemonic` as disassembly even
though Rust-SLEIGH fills it with p-code opcode labels. A `push rbp` store was
therefore registered as a source stack local and collided with a real local.
The production fix recognizes this scaffold from p-code (`SP -= width` plus a
same-instruction store) and the resolved `.cspec` preserved-register set.

## 3. Generality / Invariant Proof

Generalized rule:

```text
A scalar stack binding's storage width is the width of its resolved memory
accesses, not the width of a containing physical register.  Type recovery may
refine signedness or semantic kind at that same width, but may not widen the
binding solely because its value passed through a wider register.  An explicit
narrow return extraction constrains the function result, but does not by itself
permit narrowing genuinely wider intermediate arithmetic.

When SLEIGH reuses unique-space storage, each address occurrence must resolve
against the definition reaching that operation site. Register-save prologue
stores are frame scaffolding rather than source scalar storage and must be
excluded only after p-code shape and ABI-preservation facts prove that role.
```

ISA-agnostic check:

- [x] The rule is stated in p-code varnode/access widths and def-use facts.
- [x] No ISA enum, register spelling, function name, address, or binary id is
      part of the production condition.
- [x] The synthetic test will model mixed physical-container and scalar
      storage widths directly.

Comparable coverage:

- `list_sum`, `sum_array`, `checksum`, and `rolling_hash32` repeat the same
  4-byte stack accumulator shape across four source programs.
- Clang's `rc4_init` and `pointer_stride_sum` repeat it with a different frame
  layout and calibrated stack offset.
- The optimized `rc4_init` row supplies a non-O0 occurrence.

## 4. Risk And Ownership Check

- Existing owners: `builder/memory/stack_slots.rs`,
  `builder/materialize::memory_value_type_for_varnode`, and
  `fission-midend-normalize/src/types/{type_flow,type_infer,use_type_infer}.rs`.
- Shared fact: p-code memory access width plus existing def-use/type constraint
  graph.  No new pass is justified.
- The principal risk is narrowing a real 64-bit accumulator only because its
  final return truncates to 32 bits.  The implementation must require direct
  32-bit storage/operation evidence and must preserve genuinely wide
  intermediate arithmetic.
- Surface type hints remain locked and must not change.
- Known no-change cases: a 64-bit local explicitly truncated only at return;
  64-bit `size_t`, pointers, aggregates, and float stack slots. Windows LLP64
  `long` and `DWORD` remain 32-bit, as required by their compiled access width.
- Telemetry: none planned.
- New owner dependency: none.

## 5. Validation Matrix

- [x] Targeted owner test for two stack accesses whose reused address temporary
      resolves to different sites and widths; each binding retains its access
      width.
- [x] Targeted owner test that a `.cspec`-preserved register push expressed
      only as p-code is not admitted as a source stack slot.
- [x] `cargo nextest run -p fission-pcode` (1007 passed, 1 skipped)
- [x] `cargo check -p fission-pcode`
- [x] `cargo build --release -p fission-cli`
- [x] Focused real rows above, NIR and HIR.
- [x] Re-run the 146-row held-out O0 type census; expected fewer 32-to-64
      confusions with no new type-perfect regressions.
- [x] Re-run the O2 argument rows and the DecBench sample-set NIR/HIR sweep as
      regression evidence.

Measured held-out results against `eb4f0de3c`:

| layer | before perfect | after perfect | before accuracy | after accuracy |
| --- | ---: | ---: | ---: | ---: |
| NIR | 50/146 | **62/146** | 0.665468 | **0.701439** |
| HIR | 49/146 | **68/146** | 0.678058 | **0.748201** |

NIR gained 12 perfect rows and lost none. Twenty-five rows improved and five
non-perfect rows' partial scores fell. Manual inspection found no output
regression in those five: two are offset-calibration artifacts after corrected
slot names became parseable, while three are Windows LLP64 `long`/`DWORD`
values correctly changing from 64-bit to their compiled 32-bit width. The
local scorer fork normalizes `long` as 64-bit without target ABI context.

DecBench sample-set results, fixed denominator 250/250 functions:

| layer | coverage | changed files | gotos | `longlong` tokens |
| --- | ---: | ---: | ---: | ---: |
| NIR | 250/250 -> 250/250 | 116/224 | 1080 -> 1079 | 7407 -> 6980 |
| HIR | 250/250 -> 250/250 | 121/224 | 1078 -> 1077 | 7088 -> 6743 |

The one-goto reduction is incidental and is not claimed as a structuring
improvement. The two previously observed non-stack argument-width rows
(`rc4_init`, Clang O0 and GCC O2) remain unchanged; they are not incorrectly
claimed as fixed by stack heritage and stay as separate follow-up work.

The mandatory Docker path could not start because the local OrbStack Docker
socket was unavailable. Validation instead used the external
`fission-benchmark/corpus/dev` data and scorer directly, plus the official
`vendor/decbench-evalkit` sample runner, both with fixed denominators and
separate baseline/current executables. This is local regression evidence, not
an official leaderboard or release claim.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Measurements stayed on held-out `corpus/dev`; production code will contain
  no row identity.

## 7. Review Notes

- [x] Production code will contain no hardcoded binary/function/address/corpus
  guards.
- [x] The justification is independent of the metric: a 4-byte scalar stored
  and operated on at 4 bytes is incorrectly declared as 8 bytes.
- [x] Existing type/stack owners will be extended; no duplicate pass or metric
  is proposed.
