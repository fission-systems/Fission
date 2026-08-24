# Preserve direct-callee extents and stable pointer evidence

## 1. Baseline Row Anchor

Measured on `main` at `2cd4bc2fc` with the official DecBench sample-set type
scorer on its fixed 109-function DWARF-bearing subset. The current result is 6
type-perfect functions and total type distance 1,205. Of 176 aligned argument
mismatches, 41 expect `char*` or `FILE*` (including accepted aliases) but are
rendered as an integer type. Thirty-two of those specifically accept `char*`
and are emitted as `ulonglong`.

The direct-callee prototype analysis does find pointer evidence for several of
these arguments, but its isolated lift truncates known function extents using
an assumed four bytes per instruction:

| corpus / optimization | callee extent | current instruction budget | measured instructions |
| --- | ---: | ---: | ---: |
| sample O2, coreutils helper | 117 bytes | 32 | more than the admitted prefix; the lift reports transfer past an early return |
| stripped held-out O0, coreutils helper | 204 bytes | 51 | 56 |
| stripped held-out O0, cronie helper A | 166 bytes | 41 | 55 |
| stripped held-out O0, cronie helper B | 166 bytes | 41 | 55 |

The O0 instruction counts were measured from the exact symbol-derived
`[start, start + size)` extents after copying and stripping the binaries; the
decompiler therefore received no DWARF or symbol-name type hints. The issue
repeats across coreutils and two cronie programs, at O0 and O2. The affected
sample callers include repeated `char*` arguments currently printed as
`ulonglong`.

This is type-evidence completeness work. The final fixed-denominator reruns
show a two-edit type-distance improvement with no perfect-count, structure, or
recompilation claim.

The first full-extent prototype measured 6 -> 6 perfect and type distance
1,205 -> 1,203 on the fixed sample subset, but it also exposed a held-out O0
regression: a caller argument changed from `char*` to `longlong`. The complete
callee still decompiles standalone with the correct `char*` formal. The old
truncated prefix happened to end while the first pointer-typed library call
still dominated inference.

The complete callee's normalized
prototype treats later unknown-call constraints as sufficient to erase an
earlier proven pointer use. The old stable prefix and the complete lift are two
observations of the same formal parameter, so pointer evidence must be merged
monotonically: a concrete dereference or pointer-typed call in the prefix
remains valid even when the rest of the function adds weaker unknown/scalar
uses. The complete lift remains authoritative for arity and effects.

## 2. Owner Proof

- [x] Decompiler direct-callee fact extraction / isolated lifting
- [ ] SLEIGH/raw p-code semantics
- [ ] Builder/materialize
- [ ] Normalize/structuring
- [ ] HIR presentation/printer
- [ ] Benchmark/automation

`direct_callee_max_bytes` already obtains an exact or conservatively bounded
byte extent from `LoadedBinary`. Immediately afterward,
`direct_callee_instruction_limit` divides that byte extent by four before
calling the lifter. That conversion is not justified for a variable-length
instruction set. The resulting preview can stop inside the known extent and
omit later calls, stores, returns, and their prototype constraints. Standalone
decompilation of the full helper recovers pointer parameters that the truncated
preview does not.

The raw p-code is not the first owner of the wrong fact: it is never requested
for the discarded tail. Type recovery cannot infer evidence that the fact
extractor omitted.

An experiment that added one nested direct-callee layer recovered one further
sample pointer, but made two large sample functions exceed the 45-second worker
budget and was rejected. It is not part of the production change.

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a lift has a byte extent and a separate instruction-count budget, a safe
architecture-neutral upper bound is one instruction per byte unless the ISA
model supplies a proven larger minimum instruction width. Apply the existing
global instruction cap after that conservative conversion. Never use an
average instruction width to decide that the tail of a known function does not
exist.

Infer the complete prototype and, when its instruction budget is larger than
the former stable prefix, infer that prefix under the same isolated context.
Supplement only missing pointer slots in the complete prototype. Never replace
one concrete pointer pointee with another, never turn a scalar-only prefix into
a pointer, and never use prefix arity/effects to delete complete-function facts.
```

- [x] Production logic depends only on the bounded byte extent and existing
      resource cap, not on ISA, function, address, project, compiler, or corpus.
- [x] The rule is valid for one-byte-minimum variable-length ISAs and merely
      spends a conservative bounded budget on fixed-width ISAs.
- [x] Evidence repeats in at least three functions, two programs, two projects,
      and both O0 and O2 builds.
- [x] Synthetic coverage will state only byte-budget boundary behavior.
- [x] Prefix evidence is additive only for pointer slots and cannot erase or
      override complete-function evidence.

## 4. Risk And Ownership Check

- Existing owner: `fission-decompiler::facts::direct_callee_instruction_limit`.
- Shared substrate: the existing loader function extent and SLEIGH lift API.
- No new pass, fact map, dependency, vendor integration, or ISA-local branch.
- The existing 32-instruction floor and 512-instruction ceiling remain. The
  downstream 2,000-p-code-op preview limit also remains.
- More complete callees can legitimately change effect summaries, call
  prototypes, and caller parameter types. Full NIR/HIR output audits are
  therefore required; aggregate type totals alone are insufficient.
- Unknown-size functions retain the existing bounded default byte extent.
- The existing caller context is cloned, so speculative preview facts cannot
  mutate the outer host.
- The supplemental prefix retains the old 32..512 bounded budget. It is decoded
  only when distinct from the complete budget; its effects and arity authority
  are discarded.
- Functions longer than 512 instructions remain deliberately truncated by the
  explicit resource cap rather than an implicit average-width assumption.

The output is better without reference to a benchmark metric because an
isolated preview of a known function extent must not silently discard valid
machine instructions, calls, or side effects based on a false average width;
and a proven pointer use in one prefix remains a pointer use when later unknown
calls add weaker constraints.

## 5. Validation Matrix

- [x] Targeted unit tests cover the 32 floor, per-byte complete budget, old
      stable-prefix budget, 512 cap, additive pointer merge, complete arity
      authority, and concrete-pointee precedence.
- [x] Focused O2 sample reruns: two independent cronie `get_range` rows recover
      `bits` from `ulonglong` to `uchar *`. A base-passwd row moves an expected
      `_node**` argument from integer to a pointer-shaped `longlong *` without
      changing its metric distance.
- [x] Focused stripped O0 reruns: all five recorded callers are byte-identical
      to the baseline after stable-prefix merging, including the `char*` row
      that regressed under complete-only inference.
- [x] Official sample type scorer replay on the fixed 109-function subset:
      perfect 6 -> 6, total distance 1,205 -> 1,203, pointer-miss rows 76 -> 76;
      two improved rows and zero regressed rows.
- [x] Full sample NIR: 250/250 functions, 224/224 binaries, no failures;
      exactly `bin_037`, `bin_043`, and `bin_160` changed; gotos 1,079 -> 1,079.
      Wall time was 343.61 seconds.
- [x] Full sample HIR: 250/250 functions, 224/224 binaries, no failures; the
      same three files changed; gotos 1,077 -> 1,077. Wall time was 346.75
      seconds.
- [x] The rejected nested-preview experiment reproduced worker timeouts on
      `bin_039` and `bin_068`. After removing it, isolated reruns completed in
      9.19 and 1.87 seconds respectively with valid output.
- [x] `cargo nextest run -p fission-decompiler`: 60 passed.
- [x] `cargo nextest run -p fission-pcode`: 1,015 passed, one skipped.
- [x] `cargo check -p fission-pcode -p fission-decompiler`
- [x] `cargo build --release -p fission-cli`
- [x] Cache-disabled external Docker fixed 40-row run (`--variant-limit 1`,
      `--no-resume`): all row keys and NIR/HIR code were byte-identical to the
      saved baseline; semantic/type/GED/recompile/status/goto fields were
      unchanged. Semantic 22/40 perfect, mean 0.6442; type 22/40, mean 0.8127;
      GED 15/40, mean distance 5.225; 22 ok / 9 compile errors / 9 assertion
      failures. This is local regression evidence only.
- [x] Owner-boundary and NIR-boundary scans: zero findings.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Glaurung's one-grandcallee design was read as a cleanroom invariant reference,
  measured locally, and rejected here on the sample timeout gate. No code was
  copied and no runtime/build dependency was added.
- Row identities occur only in this local proposal and measurement artifacts.
  Production code and tests use the extent/budget invariant only.

## 7. Review Notes

- [x] Production code will contain no hardcoded binary, function, address,
      corpus, compiler, or optimization guard.
- [x] The change extends the existing owner rather than adding a pass.
- [x] Vendor implementations are not linked, invoked, or copied.
- [x] The type-distance claim is backed by the official fixed sample scorer;
      perfect-rate and leaderboard movement are not claimed.
