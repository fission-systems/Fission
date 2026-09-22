# Decompiler Change Proposal

## 1. Baseline Row Anchor

- Binary: `libc_types_gcc_O2.exe`
- Function: `tm_year_of`
- Address: `0x1400016d0`
- Corpus row or benchmark command: `fission-benchmark` dev corpus, focused `tm_year_of` run; direct probe with `fission_cli decomp --addr 0x1400016d0 --layer both --prehir --debug-decomp --json --no-db --no-warnings`
- Current output summary: direct output uses `int * param_1` and `param_1[5]`; no debug parameter or struct-field hint is applied.
- Semantic cases passed / total: DecBench focused baseline: `17/35` cases overall (`gcc -O1/-O2/-O3/-Os`: `5/5` each; `gcc -O0` and `gcc-m32 -O0/-O2`: compile errors).
- Failure category: direct type/data recovery gap; benchmark compile failures additionally expose the generated translation unit's missing `tm` declaration.
- Relevant benchmark/static/readability observations: the three passing rows produce the expected scalar result but retain `int * param_1`; the direct probe reports `debug_struct_field_hits=0`, `debug_struct_promotions=0`, and no explicit parameter type hit.

After the loader and rendering fixes, the same cache-disabled focused run on
commit `149df3a71` passes `35/35` cases: all seven compiler/optimization
rows are semantic `1.00`, type-match `1.00`, bare-compile `ok`, and have no
runner errors. The final NIR surface is:

```c
typedef struct fission_agg36 { /* recovered tm fields */ } fission_agg36;
typedef fission_agg36 tm;

int tm_year_of(const tm* t) { return t->tm_year + 1900; }
```

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [x] Type/data recovery:
- [x] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
raw p-code for tm_year_of:
  IntAdd unique <- RCX, const(0x14:8)
  Load temp:u32 <- space3, unique
  Copy RAX:u32 <- temp
  IntZExt RAX:u64 <- RAX:u32
  IntAdd RAX:u32 <- RAX:u32, const(0x76c:4)

DWARF contains a valid function-scoped parameter:
  tm_year_of @ 0x1400016d0
  t: const tm *
  location: DW_OP_reg2 RCX

The same DWARF unit also contains quotient_of's malformed local location:
  DW_OP_reg0 RAX, <decoding error> f0

The current location-list parser propagates that one expression error from
extract_local_var_info through analyze_functions_inner, so no later DWARF
function facts reach the type/data recovery owner.

The first parser fix exposed two independent declaration-contract defects in
the renderer: qualified aliases such as `const tm *` were not emitted, and a
same-name scalar spill could overwrite the aggregate alias recovered for the
formal. Unknown aggregate fields were also laid out as one-byte values even
though emitted `undefined` is a four-byte C typedef. These were fixed at the
renderer owner, with the scalar-spill merge selecting the most informative
definition and field widths inferred from recovered offsets.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
DWARF location descriptions are per-variable facts. If one location-list
expression cannot be decoded, that variable's location is Unknown; it must not
discard unrelated function and parameter facts from the compilation unit.
Valid single-register descriptions in later DIEs remain eligible for debug
parameter and aggregate-type recovery.

Surface declaration rule:

```text
For one recovered surface alias, merge duplicate binding definitions by
information content. A recovered aggregate layout must not be replaced by a
scalar compiler spill, and emitted unknown fields must preserve their proven
offsets in the target C layout.
```
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is independent of function name, address, binary, and ISA.
- [x] Register interpretation remains in the existing target DWARF-register map.
- [x] Synthetic invariant test uses a malformed location expression followed by a valid register expression, without a compiler tuple or corpus row.

Comparable coverage:

- Similar shape 1: optimized GCC location lists with constant prefixes and later register materialization.
- Similar shape 2: any compilation unit containing a producer-specific or truncated DWARF location expression before another valid function DIE.
- Synthetic invariant test: malformed `DW_OP_reg0` plus invalid trailing opcode is classified as `Other`/`Unknown` while a valid register expression remains recognized.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `fission-loader/src/loader/dwarf/functions.rs` owns DWARF function and location extraction.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [x] None; this is a loader-local malformed-input boundary.
- Why extending that owner is sufficient, or why a new pass/helper is needed: the parser already represents unsupported or ambiguous locations as `DwarfLocation::Unknown`; the missing rule is to apply that contract per location expression instead of propagating a decode error to the whole compilation unit.
- Possible interaction with existing normalize/structuring/materialize passes: none; recovered debug facts are optional overlays on existing p-code-derived types.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: no new telemetry; existing debug hint counters should become nonzero only when valid facts are available.
- Known cases that must not change: valid register/stack locations, location-list agreement rules, raw p-code, and functions without usable debug facts.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-loader -E 'test(location_list)'`
  - Expected signal: malformed expression is downgraded to an unknown location entry rather than returned as a parser error.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-loader`
  - Expected signal: loader tests pass.
- [x] Focused benchmark row:
  - Command: focused `tm_year_of` run with `FISSION_BENCHMARK_NO_CACHE=1` before and after the local release build.
  - Result: `17/35` baseline → `35/35` after `149df3a71`; all seven rows are semantic-perfect, type-perfect, compile-clean, and error-free.
- [x] Smoke or automation sample:
  - Command: direct `fission_cli decomp` for `tm_year_of`, plus `cargo nextest run -p fission-emulator`.
  - Expected no-regression signal: direct raw p-code and emulator tests remain unchanged.
- [x] Optional related checks:
  - Command: `cargo check`, `cargo fmt --all --check`, `git diff --check`, and release CLI build.
  - Expected signal: workspace/build hygiene remains green.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Information exposed in the AI prompt:
  - [x] Structural failure pattern only
- Redaction confirmed:
  - [x] Function names removed from any external implementation prompt
  - [x] Addresses removed from any external implementation prompt
  - [x] Binary paths removed from any external implementation prompt
  - [x] Corpus row ids removed from any external implementation prompt
  - [x] Compiler tuple / row-identifying labels removed from any external implementation prompt
- Ghidra guidance confirmed:
  - [x] No Ghidra output-style guidance used
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: focused DecBench corpus rows, cache disabled, `35/35` after the fix.
  - Synthetic invariant test command/result: malformed DWARF location, qualified alias, duplicate aggregate/scalar alias, and unknown-field layout tests pass.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
