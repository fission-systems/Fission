# Decompiler Change Proposal: Defined Variadic Helper Prototypes

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O1.exe`
- Function: MinGW runtime helper `__report_error` (the benchmark source `advanced_patterns.c` does not contain this linked helper).
- Address: `0x140001a70`
- Reproduction: `target/release/fission_cli decomp <binary> --project --no-header --no-warnings -o /tmp/fission-issue56-baseline-project.c`
- Baseline direct output: fixed prototype `void __report_error(const char *msg, unsigned long long param_2, unsigned long long param_3, unsigned long long param_4)`; body forwards `msg` and a `va_list`-typed local to `vfprintf`.
- Baseline project output: calls to this definition render with only the format-string argument. The corresponding machine code also places values in argument registers at call sites; for example `0x140001c07` sets `RCX=format`, `RDX=GetLastError()`, and calls the helper.
- Compile baseline: Clang GNU C17 reports 66 errors for the complete project output, including 9 `__report_error` diagnostics of `too few arguments, expected 4, have 1`. The remaining diagnostics are outside this issue.
- Semantic cases / semantic scores: N/A for this declaration/call-contract defect; the measured gate is the generated project compile contract and targeted call-site preservation.

## 1.1 Re-measured Result

- The direct release-CLI definition is now `void __report_error(const char* msg, ...)`; project callsites retain their recovered extra operands.
- The focused builder regression confirms that only the fixed register prefix becomes named formals; unnamed variadic register state uses a declared live-register binding. The fixed-arity control still names all consumed parameters.
- Clang GNU C17 on the baseline project output reports 66 diagnostics, including 9 `__report_error` arity errors. A repeated final release run reports 57 diagnostics and 0 `__report_error` arity errors.
- Whole-project totals varied from 54 to 57 diagnostics across release reruns because unrelated import-thunk prototypes (notably `__iob_func`) vary between inferred forms. One rerun briefly emitted an unrelated one-parameter declaration for that zero-argument import. The target helper result remained fixed across runs; this variance is not counted as a `__report_error` result or a quality score.

## 2. Owner Proof

- [x] Loader debug-type extraction and normalize / type-data recovery
- [x] Decompiler facts / prototype propagation
- [x] Builder entry-register formal binding
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

`llvm-dwarfdump` provides declaration-level confirmation: the helper DIE has
one `DW_TAG_formal_parameter` (`msg`) followed by
`DW_TAG_unspecified_parameters`, and its `argp` local has type `va_list`. The
loader already recognizes the equivalent DWARF marker for function-pointer
types, but `DwarfFunctionInfo` drops it for ordinary functions.

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
callee's fixed named-parameter count. The fixed prefix and variadic marker
already exist in DWARF; the loader and prototype contracts must preserve them
before caller arity facts can lock a fixed signature. The ABI home-slot cursor
is corroborating evidence, not the source of this target's classification.

## 3. Generality / Invariant Proof

Generalized rule:

```text
For a defined callee, preserve declaration-level variadic evidence
(`DW_TAG_unspecified_parameters`, or an equivalent explicit signature source)
and its fixed named-parameter count. Do not treat caller-observed arity as an
exact fixed prototype when that evidence marks a variadic tail. Call-site
observations may widen lower bounds, but cannot erase the variadic contract.
Caller-count disagreement alone is not sufficient proof.
```

ISA/ABI check: the rule is grounded in Windows x64 home-slot/parameter-slot
facts; it must not test a function name, address, binary identity, or corpus
row. Calling-convention details remain ABI data, while the function/prototype
representation and propagation are shared.

Comparable coverage:

- Known imported variadic prototypes (`fprintf`/`printf`) already preserve a fixed prefix and do not lock total arity.
- A synthetic defined helper with a one-parameter fixed prefix and one, two, or three trailing arguments, plus a fixed-arity control.
- A fixed-arity Windows x64 control whose four register parameters are semantically consumed and must remain fixed.

## 4. Risk And Ownership Check

- Existing owners: DWARF extraction in `fission-loader`; function/call prototype facts in `fission-decompiler::facts`; entry-register formal binding in `fission-pcode::builder`; entry-register promotion and call-site pruning in `fission-midend-normalize`; signature emission in the existing HIR printer.
- Shared substrate candidate: typed function/call prototype contract. Extend the existing models rather than infer a second private flag in the printer.
- Interaction: variadic evidence must be available before entry-param promotion, call-site argument pruning, and interprocedural arity-hint seeding can make the false fixed-arity fact irreversible.
- Known cases that must not change: imported exact-arity APIs, known imported variadic runtime APIs, ordinary fixed-arity internal functions, and Windows x64 functions with genuinely used register parameters.
- Risk: dropping or misapplying the DWARF variadic marker could weaken a real fixed prototype or preserve spurious arguments; test both the unspecified-parameters marker and ordinary fixed-arity signatures. Project output currently also has unrelated compile diagnostics; compare only the nine target diagnostics for this acceptance gate.
- Telemetry: no new metric planned.

## 5. Validation Matrix

- [x] Targeted invariant tests: DWARF variadic-marker extraction; normalize defined-callee summary/pruning; fixed-prefix builder binding and fixed-arity control.
- [x] Normalize tests: `cargo nextest run -p fission-midend-normalize` (432 passed).
- [x] P-code tests and check: 1,132 passed with three failures excluded after each was reproduced at unchanged `a727d1abd`; four skipped; `cargo check -p fission-pcode` passed. `cargo nextest run -p fission-pcode` without exclusions still reports those three baseline failures.
- [x] Decompiler/CLI checks: `cargo nextest run -p fission-decompiler` (77 passed), both crate checks passed, and `CARGO_BUILD_JOBS=1 cargo build -p fission-cli --release` passed.
- [x] Focused row: debug and release runs show fixed prefix plus ellipsis, preserve recovered caller operands, and reduce `__report_error` C17 arity diagnostics from 9 to 0.
- [x] Regression sample: existing imported-variadic and exact fixed-arity IAT tests pass; the synthetic fixed-arity control also passes.
- [x] Formatting: `cargo fmt --all --check`; `git diff --check`.

## 6. AI Review / Prompt Firewall

- Was another AI model asked for implementation advice? No.
- No benchmark identity was sent to another model.
- Planned tests include a synthetic invariant outside the motivating row.

## 7. Review Notes

- Production code must contain no binary/function/address/corpus guards: [x] requirement recorded.
- No semantic/readability quality claim will be made from this compile-contract fix alone: [x].
- Existing owner extension is preferred; do not add a printer-only name exception: [x].
