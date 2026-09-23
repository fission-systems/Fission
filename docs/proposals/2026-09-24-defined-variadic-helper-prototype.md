# Decompiler Change Proposal: Defined Variadic Helper Prototypes

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O1.exe`
- Function: MinGW runtime helper `__report_error` (the benchmark source `advanced_patterns.c` does not contain this linked helper).
- Address: `0x140001a70`
- Reproduction: `target/release/fission_cli decomp <binary> --project --no-header --no-warnings -o /tmp/fission-issue56-baseline-project.c`
- Current direct output: fixed prototype `void __report_error(const char *msg, unsigned long long param_2, unsigned long long param_3, unsigned long long param_4)`; body forwards `msg` and a `va_list`-typed local to `vfprintf`.
- Current project output: calls to this definition render with only the format-string argument. The corresponding machine code also places values in argument registers at call sites; for example `0x140001c07` sets `RCX=format`, `RDX=GetLastError()`, and calls the helper.
- Compile baseline: Clang GNU C17 reports 66 errors for the complete project output, including 9 `__report_error` diagnostics of `too few arguments, expected 4, have 1`. The remaining diagnostics are outside this issue.
- Semantic cases / semantic scores: N/A for this declaration/call-contract defect; the measured gate is the generated project compile contract and targeted call-site preservation.

## 2. Owner Proof

- [x] Normalize / type-data recovery
- [x] Decompiler facts / prototype propagation
- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Structuring
- [ ] Printer-only
- [ ] Benchmark/automation

Evidence:

```text
The Windows x64 helper saves incoming register values into its home area,
forms a pointer at the first unnamed argument slot, and forwards that va_list
to vfprintf. Its current normalized definition nevertheless exposes four
fixed parameters. The project's real calls use different argument counts.

`seed_whole_program_call_arity_facts` and
`record_interprocedural_arity_facts` currently reduce calls to a maximum count
and turn that count into ordinary parameter-name hints for any resolved
defined callee. The direct-callee prototype path then stores an inferred
register arity as `locked_exact_arity`. Neither the ordinary function model
nor `NirCallPrototypeSummary` carries a defined-function variadic contract;
`NirFunctionType.variadic` covers function-pointer aliases, while the printer's
ellipsis exception is limited to known runtime symbol names.
```

The first wrong fact is the conflation of observed caller maximum arity with a
callee's fixed named-parameter count. The callee's ABI/home-slot and va_list
evidence must be represented before prototype propagation and rendering.

## 3. Generality / Invariant Proof

Generalized rule:

```text
For a defined callee, do not treat the maximum observed call-site argument
count as an exact fixed prototype when the callee body proves a variadic
entry contract. On Windows x64, derive the fixed prefix from the ABI home-slot
cursor used as a va_list: the slot at which that cursor begins is the first
unnamed argument. Preserve the fixed prefix and mark the remaining call
contract variadic. Caller-count disagreement alone is not sufficient proof.
```

ISA/ABI check: the rule is grounded in Windows x64 home-slot/parameter-slot
facts; it must not test a function name, address, binary identity, or corpus
row. Calling-convention details remain ABI data, while the function/prototype
representation and propagation are shared.

Comparable coverage:

- Known imported variadic prototypes (`fprintf`/`printf`) already preserve a fixed prefix and do not lock total arity.
- A synthetic defined helper with a one-parameter fixed prefix and one, two, or three trailing arguments.
- A fixed-arity Windows x64 control whose four register parameters are semantically consumed and must remain fixed.

## 4. Risk And Ownership Check

- Existing owner: `types/entry_param_promotion.rs` owns entry-register and existing variadic-register-save promotion; `facts.rs` owns caller-observed arity facts and direct-callee prototype summaries.
- Shared substrate candidate: typed prototype contract. Extend the existing summary/function contract rather than infer a second private flag in the printer.
- Interaction: variadic evidence must be available before entry-param promotion, call-site argument pruning, and interprocedural arity-hint seeding can make the false fixed-arity fact irreversible.
- Known cases that must not change: imported exact-arity APIs, known imported variadic runtime APIs, ordinary fixed-arity internal functions, and Windows x64 functions with genuinely used register parameters.
- Risk: false-positive variadic classification could weaken a real fixed prototype or preserve spurious arguments; require a positive va_list/home-slot proof and a fixed-arity negative control. Project output currently also has unrelated compile diagnostics; compare only the nine target diagnostics for this acceptance gate.
- Telemetry: no new metric planned.

## 5. Validation Matrix

- [ ] Targeted invariant tests: normalize entry-prototype classification and defined-callee summary/pruning; synthetic one-fixed-plus-1/2/3-tail cases and fixed-arity control.
- [ ] Normalize tests: `cargo nextest run -p fission-midend-normalize`.
- [ ] P-code tests: `cargo nextest run -p fission-pcode` and `cargo check -p fission-pcode`.
- [ ] Decompiler/CLI checks: `cargo check -p fission-decompiler` and `cargo build -p fission-cli --release`.
- [ ] Focused row: rerun the helper and full project with caches disabled; verify fixed prefix plus ellipsis, preserved recovered caller arguments, and zero `__report_error` C17 arity diagnostics.
- [ ] Regression sample: rerun other known imported variadic calls and a fixed-arity Windows x64 internal function; compare their signatures and call arguments.
- [ ] Formatting: `cargo fmt --all --check`; `git diff --check`.

## 6. AI Review / Prompt Firewall

- Was another AI model asked for implementation advice? No.
- No benchmark identity was sent to another model.
- Planned tests include a synthetic invariant outside the motivating row.

## 7. Review Notes

- Production code must contain no binary/function/address/corpus guards: [x] requirement recorded.
- No semantic/readability quality claim will be made from this compile-contract fix alone: [x].
- Existing owner extension is preferred; do not add a printer-only name exception: [x].
