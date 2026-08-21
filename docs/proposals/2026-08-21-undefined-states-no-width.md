# `undefined` names no C type and states no width

## 1. Baseline Row Anchor

- Command: release `fission_cli decomp <binary> --layer nir --addr <addr>` over
  the 250 scored sample-set functions.
- Current measured output: **42 occurrences of bare `undefined` across 34 of
  250 functions (14%)**, in three positions:

```c
undefined stdout;                                    // a global
undefined sub_2500(uint param_1, ulonglong param_2)  // a return type
undefined local_50;                                  // a local
```

Not `undefined8`. Bare. Ghidra's placeholder at least carries the width.

The benchmark's uniform recompilation fixup resolves it -- `"undefined":
"unsigned char"` -- so every one of these is compiled as a single byte.
`stdout` is the case that shows the cost: declared `undefined`, then read as
`ulonglong *`. An eight-byte pointer defined as one byte, and the codegen that
follows is not the program's.

Upstream DecBench made this scoreable in `#65`, which arrived while our
vendored checkout sat sixteen days behind: width-only spellings now count as
positive evidence that a type was *not* recovered. So this costs type score,
byte match, and a reader, in that order of newness and reverse order of
importance.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [ ] Normalize · [ ] Structuring
- [ ] Type/data recovery · [x] Printer · [ ] Benchmark/automation

Partly. `NirType::Unknown` carries no width -- every other variant does, `Int`
by `bits`, `Aggregate` by `size`, `Float` by `bits` -- so by the time a type
reaches rendering the width is already gone. Recovering it belongs upstream of
here.

But two of the three positions do not need it recovered, because in those
positions C does not permit an open answer:

- **A global's declaration** is a definition. `merge_global_decl_type` uses
  `Unknown` as its "not known yet" marker so a real type can still replace it,
  which is why the merge cannot commit and rendering must.
- **A function's return type** decides what every call site does with the
  result. `opaque_pcodeop_return_type_name` already answers this same question
  one layer down, rendering `Unknown` as `ulonglong`; the function's own
  signature did not.

A *local's* type is the third position and is left alone. There the open
answer is honest -- 24 of the 42 remain -- and committing a width there would
be asserting something not known, which is the failure this is fixing.

## 3. Generality / Invariant Proof

```text
Where C requires a type to be complete -- a definition, a signature -- an
undetermined NirType is rendered at word width. Where it does not, it stays
undetermined.
```

Word width is a default, not a recovery. It is chosen because it is never
*narrower* than the access that made us name the thing, which is the failure
that matters: one byte is narrower than every real access. When the size table
or the assignment context knows better, the merge has already taken that
answer and this never runs.

ISA-agnostic check:

- [x] Global width comes from `options.pointer_size`, the loaded image's own.
- [x] No binary, function, or address identity.

## 4. Risk And Ownership Check

`NirType::Unknown` keeps its meaning as an internal marker; only its *rendered
form* changes, and only in the two positions where C has no way to spell "not
known". The local declaration path is untouched deliberately.

## 5. Validation Matrix

- [x] `cargo nextest run -p fission-pcode -p fission-decompiler` (1,058 passed).
- [x] `cargo nextest run --workspace --no-fail-fast` against the established
      baseline.
- [x] DecBench sample-set rerun, 250 of 250 still decompile.

Measured on the sample-set:

```text
bare `undefined`     42 occurrences / 34 functions  ->  24 / 19
```

**43% removed**, 20 functions changed, and nothing else moves: gotos 1,118,
short-circuit terms 228, 33,259 lines, all identical. A type spelling should
not change control flow, and it does not.

`stdout` now reads `ulonglong stdout;`.

One sweep run reported a decompile failure that did not reproduce: the
function takes 0.35s standalone and the rerun is 250 of 250. It was a timeout
under eight-way parallel load, not a regression.

### Two tests pinned the old spelling

Both are named for parameter aliasing -- `LPRECT param_2` -- and both asserted
on the whole signature string including its `undefined` return. They now
assert on what they are about. This is the second time this cycle a test has
been found asserting on something other than its subject; the first two were
asserting on unreachable code.

## 6. AI Review / Prompt Firewall

- No external model was consulted.
- Found by reading upstream DecBench's new placeholder rule after a suggestion
  to check whether the scorer had moved, then measuring our own output against
  it. Production code names no binary or function.
