# Memory-Load Address Provenance Barrier

## 1. Baseline Row Anchor

- Binary: `crypto_clang_O0.exe`, `crypto_clang-m32_O0.exe`
- Function: `rc4_crypt`
- Address: `0x1400015f0`, `0x4015b0`
- Corpus row or benchmark command:
  `FISSION_HOST_PORT=8007 python3 runner/runner.py --corpus dev --function rc4_crypt --decompilers fission --run-mode local`
- Current output summary:
  byte loads and loop-carried scalar indices inherit the array base's pointer
  type. The generated C consequently applies `% 256` or `/ 256` to `uchar *`.
- Semantic cases passed / total: Clang x64 O0 `0/5`; Clang x86 O0 `0/5`
- Failure category: `compile_error`
- Relevant observations: optimized Clang rows pass `5/5`; the failure is tied
  to explicit O0 load temporaries, not to structuring budget or the printer.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
loaded_byte = Load(base + index)
accumulator = accumulator + loaded_byte
reduced = accumulator % 256

DefinitionDependencyMap currently records the address variables inside Load as
ordinary value dependencies of loaded_byte. address_contributors then walks that
same graph back to the known pointer base, classifies loaded_byte and every
downstream accumulator as pointer contributors, and type recovery renders the
already-wrong pointer types in both NIR and HIR output.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
A memory load creates a value-provenance boundary. The loaded value does not
derive pointer identity from the address used to fetch it. Address provenance
may flow through copies, casts, selects, and pointer arithmetic, but it must not
cross Load, value-producing Index/FieldAccess, or call-return boundaries merely
because their operands contain a known pointer root.
```

ISA-agnostic check:

- [x] Production condition is not gated on an ISA or calling convention.
- [x] No ISA-specific data is introduced.
- [x] The synthetic test uses neutral dataflow names and HIR memory semantics.

Comparable coverage:

- Similar shape 1: byte load feeding a wrapping or modulo accumulator.
- Similar shape 2: loaded integer feeding an array index while the container
  address remains a pointer.
- Synthetic invariant test: an address expression remains connected to its
  pointer root, while the value loaded through it does not.

## 4. Risk And Ownership Check

- Existing owner: `normalize/analysis/defuse.rs::DefinitionDependencyMap` and
  the existing type-recovery consumers of `address_contributors`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the graph currently loses the
  distinction between ordinary value dependencies and address provenance. A
  dedicated provenance edge set lets all existing consumers share the correct
  boundary without another cleanup pass.
- New pass/helper/metric: no new pass or metric; one analysis edge collector is
  added to the existing def-use owner.
- Possible interactions: parameter pointer recovery and signed pointee recovery
  continue to use the existing general dependency graph; only
  `address_contributors` consumes the barrier-aware graph.
- New owner-to-owner dependency:
  - [x] None
- Telemetry impact: none.
- Known cases that must not change: pointer copy chains, affine pointer
  arithmetic, pointer selects, and the existing scalar offset parameter tests.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(memory_load_value_does_not_inherit_address_provenance)'`
  - Expected signal: loaded value and downstream scalar are absent from address
    contributors; base aliases remain present.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode --no-fail-fast`
  - Expected signal: all existing tests pass.
- [x] Focused benchmark row:
  - Command: external local-docker `rc4_crypt` run across all eight variants.
  - Expected row-level improvement: both Clang O0 rows compile; passing rows do
    not regress.
- [x] Smoke or automation sample:
  - Command: focused Fission dev smoke set or equivalent source-semantic lane.
  - Expected no-regression signal: existing passing rows remain passing.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode -p fission-decompiler`
  - Expected signal: clean compile.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation prompt was used.
- The operator supplied the motivating row locally; production code and
  synthetic tests use only the structural provenance invariant.
- Ghidra output style is not used as an implementation target.
- Unseen or synthetic validation evidence: the neutral HIR provenance test plus
  the crate-wide regression gate.

## 7. Review Notes

- [x] Production code contains no hardcoded binary/function/address/corpus guards.
- [x] The change does not claim semantic improvement from benchmark-only edits.
- [x] The change extends shared def-use analysis and adds no duplicate pass.

## 8. Validation Results

- Targeted provenance test: `1 passed`.
- `cargo nextest run -p fission-pcode --no-fail-fast`:
  `1,187 passed, 9 skipped`.
- `cargo check -p fission-pcode -p fission-decompiler`: passed.
- Fresh local Linux bundle fingerprint:
  `5e06971c597df89655687e564b99bca335ac792b336398cdde1d70e2ffed58f5`.
- Focused eight-variant result:
  - Clang x64 O0: `compile_error (0/5)` to `5/5`.
  - Clang x86 O0: `compile_error (0/5)` to `5/5`.
  - GCC x64 O0/O2 and Clang x64/x86 O2 remained `5/5`.
  - GCC x86 O0 still has an independent entry-flag artifact; GCC x86 O2
    remains a runtime-semantic failure. Neither is claimed fixed here.
- Ten-function smoke: 52 rows, 0 adapter errors, 189/276 semantic cases, with
  the focused previously passing rows preserved. This run is local regression
  evidence only and is not publishable benchmark data.
