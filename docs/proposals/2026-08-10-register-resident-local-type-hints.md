# Register-resident local type hints

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/.cache/decbench-local-eval/x86_O2/math.elf`
- Function: `binary_search`
- Address: `0x4008b0`
- Corpus row or benchmark command: `DECBENCH_NO_CACHE=1 ... decbench_local_eval.py --decompilers fission,fission-hir --skip-heldout`, followed by the same DecBench `type_match` evaluator on the `x86 O2 / math` unit
- Current output summary: parameters recover as `int *arr, int n, int target`; DWARF locals are `int lo`, `int hi`, and `int mid`, but the NIR declares the loop-carried `lo` register binding as `longlong rcx` and a copy of `hi` as `longlong rsi`.
- Semantic cases passed / total: NIR `0/6`, HIR `0/6` (both time out in the existing loop; this type-only change is not expected to repair the control-flow failure)
- Failure category: semantic `timeout`; DecBench type mismatch
- Relevant benchmark/static/readability observations: DecBench type-match is `0.8333333333`, GED is `6.0`, and byte-match is `0.1590909091`. Across the measured x86 O2 bucket, no function is a perfect type match (`Type1 = 0/18`). The row has three explicit parameter type hits and two register-local name hits, but zero explicit local type hits.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
fission-loader::DwarfLocalVar already carries both `name` and `type_name` plus
`DwarfLocation::Register`. `register_local_names_from_debug_function` resolves
the DWARF register to a SLEIGH register offset but retains only the name.
`NirFunctionHints` has stack-local name and type maps, but only a register-local
name map. `apply_function_name_hints` therefore reports two register-local name
hits for the anchored row while no register-local type can reach the binding.

The remaining unmatched `lo` local uses a DWARF location list with a pure
constant prefix (`0`) followed by one register (`RCX`). The loader previously
collapsed any location list containing a non-register range to `Unknown`, so
the existing register-name/type path could not identify this optimized-local
shape even though no competing register appears.

Ground truth: arr:int*, n:int, target:int, lo:int, hi:int, mid:int
NIR locals:   hi:int, rcx:longlong, rsi:longlong, mid:int, uVar24:int
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a debug local has a register location that is stable for its declared
scope, or a pure constant-valued prefix followed only by one agreed register,
resolve that register through the target's DWARF-to-SLEIGH register map.
If every nonempty debug type mapped to the same register offset agrees on one
declared type, carry that type beside the existing register-local name hint and
apply it to a Fission local whose recorded register origin has the same offset.
If a constant appears after register materialization, a computed/entry-value
location appears, registers disagree, or types conflict, emit no hint.
```

ISA-agnostic check:

- [x] Production condition is not gated on one ISA or calling convention.
- [x] ISA-specific register numbering remains in the existing DWARF register map and register model.
- [x] Synthetic tests use register-origin/type agreement rather than a compiler tuple or function name.

Comparable coverage:

- Similar shape 1: optimized loop induction/limit local retained in a GPR.
- Similar shape 2: register-resident pointer or aggregate reference with a stable debug location.
- Synthetic invariant test: matching register origin applies the agreed surface type; conflicting types for one offset are rejected by context assembly.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: DWARF location classification in `fission-loader`, `NirFunctionHints` assembly in `fission-decompiler::facts`, and `apply_preview_type_hints` in the p-code builder.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the loader already parses the authoritative location list, the facts layer already resolves DWARF registers to SLEIGH offsets, and the builder already applies equivalent parameter/stack-local type hints. The missing pieces are the conservative constant-prefix register classification and typed register-local transport.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no new pass or metric is added; one owner-local helper enforces the conflict-rejection rule while assembling debug facts.
- Possible interaction with existing normalize/structuring/materialize passes: the hint sets the declared surface type at the existing pre-normalize hint point; it does not change CFG, effects, evaluation order, or register materialization.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: existing `explicit_local_type_hits` also counts accepted register-local types; no parallel telemetry payload.
- Known cases that must not change: no-debug binaries, unstable/unknown locations, conflicting types sharing one register offset, bindings without a recorded register origin, and explicit stack/parameter types.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: targeted nextest expression over register type application, conflict rejection, merge precedence, and leading-constant location-list classification.
  - Result: register-origin name/type, conflict rejection, merge precedence, and leading-constant location-list tests passed.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode -p fission-decompiler && cargo nextest run -p fission-loader && cargo check -p fission-pcode -p fission-decompiler -p fission-static`
  - Result: pcode/decompiler `1036/1036` passed; loader `126/126` passed; all checks passed.
- [x] Focused benchmark row:
  - Command: rebuild release CLI, decompile `math.elf @ 0x4008b0`, then rerun DecBench type_match with `DECBENCH_NO_CACHE=1`.
  - Result: NIR/HIR `type_match 0.8333333333 -> 1.0`; NIR byte-match `0.1590909091 -> 0.28125`; HIR byte-match `0.1333333333 -> 0.28125`; GED remained NIR `6.0`, HIR `7.0`; semantic status remained `timeout`, `0/6` for both layers.
- [x] Smoke or automation sample:
  - Command: `DECBENCH_NO_CACHE=1 ... decbench_local_eval.py --decompilers fission,fission-hir --skip-heldout`
  - Result: all `165/165` targets produced with zero errors. NIR `Type1 48 -> 54`, `Union 88 -> 93`; HIR `Type1 44 -> 50`, `Union 95 -> 100`; GED0 and Byte1 stayed unchanged.
- [x] Optional related checks:
  - Command: focused `base-passwd` held-out NIR/HIR sample used in the baseline measurement.
  - Result: both layers produced all `12/12` targets with zero errors.
- [x] Boundary audit, if a new pass/helper/dependency was added:
  - Result: inspected loader location classification, owner-local conflict helper, and all `NirFunctionHints` merge sites; no pass or dependency boundary was added.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: none
- Redaction confirmed: no external implementation prompt was sent
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: focused `base-passwd` held-out sample produced `12/12` targets per layer with zero errors
  - Synthetic invariant test command/result: register type agreement/conflict and leading-constant location-list tests passed

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
