# Recovering a pointee's shape by watching where the pointer is dereferenced

## 1. Baseline Row Anchor

The largest remaining class of type mistakes is a pointer whose *pointee* is
wrong -- 26 of the 88 at `-O0`, measured in
`2026-08-24-type-mistypes-what-remains.md`. We get the indirection right and
the thing pointed at wrong.

- Binary: `fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O0.exe`
- Function: `list_sum` at `0x140001530`
- Source: `int list_sum(const Node *head)` over
  `struct Node { int value; struct Node *next; }`
- Current output: `head` is `fission_agg16 *`, `cur` is `int *`. Both should be
  `Node *`. The same row misses on clang (`uint *`) and on `gcc-m32`
  (`int *`).

`kv_lookup` repeats it over `struct Kv { int key; int value; }`, emitting
`uint *` for `Kv *`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [x] Normalize/type recovery
- [ ] Signature resources · [ ] Structuring · [ ] Printer

Every current type mechanism pulls a type from a *known signature* -- a callee
parameter, a return type, a format specifier. That dictionary can only name
types someone shipped a declaration for. A program's own `struct Node` is not
in it and never will be.

But p-code states the shape outright. Each `LOAD`/`STORE` names its address,
and an address is a base plus a constant, so the set of offsets a value is
dereferenced at *is* the layout of what it points to.

## 3. Generality / Invariant Proof

```text
A pointer's pointee is described by the set of (offset, width) pairs at which
the pointer is dereferenced. Two pointers with the same set point at the same
shape, whatever it is called.
```

This recovers *shape*, not *name*: it yields "four bytes at 0, eight at 8", not
`Node`. Whether that is enough for the metric is section 6's open question.

**Deliberately an abstract pass, not symbolic execution.** One walk over the
ops with a value-to-(base, offset) map, lattice merge at joins, no path
splitting and no solver. That line is the design constraint, not an
implementation detail -- see section 4.

ISA-agnostic check:

- [x] Reads only `IntAdd`/`Copy`/`LOAD`/`STORE` and varnode identity.
- [x] Verified on x86-64 gcc, x86-64 clang, and 32-bit gcc.

## 4. Risk And Ownership Check

**Cost, measured before designing.** Current decompilation is a median 0.77s
per function (90th percentile 2.85s), over a median 556 p-code ops (90th
percentile 1,828, max 2,788). A constant-time-per-op abstract pass at a few
hundred nanoseconds is on the order of **0.2ms -- under 0.03% of the current
time**, and comparable to `prune_unused_temp_bindings`, which already rebuilds
a whole-body `DefUseMap` at each of some thirty stages.

That holds *only* while the pass stays abstract. Per-path symbolic execution
with a solver is seconds per function -- the `fission-emulator` symbolic tests
run 6-20s and the selfjit differential times out at 120s. The temptation
arrives later, phrased as "split the paths for accuracy"; that is the point at
which this becomes a hundred times slower, and it is written here so the next
person crosses the line knowingly.

## 5. Validation Matrix

- [x] Feasibility on the anchor, and on two more compilers.
- [ ] Prevalence: how many of the 26 pointee rows show a recoverable layout.
- [ ] Whether recovering the *shape* moves the metric, which scores *names*.
- [ ] Aliasing: two different structs reached through one base.
- [ ] Production implementation, tests, corpus regression.

Feasibility, from `raw-pcode` alone with no Rust changes:

```text
target                              source struct                 recovered
list_sum   (gcc x86-64)   Node { int; Node*; }            (0,4) (8,8)
list_sum   (clang x86-64) same                            (0,4) (8,8)
list_sum   (gcc -m32)     same, 32-bit pointer            (0,4) (4,4)
kv_lookup  (gcc x86-64)   Kv { int key; int value; }      (0,4) (4,4)
```

Exact on every one, across two compilers and both widths, including the 32-bit
pointer shrinking `next` from eight bytes to four. Stack-frame accesses appear
under the stack pointer's own base and separate themselves without a rule.

A defect in the probe found and fixed along the way: a negative displacement is
written as the unsigned two's complement of *the operand's own width*, so
sign-extending at 64 bits regardless turned a 32-bit `-8` into
`18446744073709551608`. Only stack offsets were affected; the struct extraction
was already correct.

## 6. The question this does not answer

The metric scores type *names*. This recovers *shapes*. `(0,4) (8,8)` is not
`Node *`, and `normalize_type` will not accept it.

Two ways that could still pay, both unmeasured:

- A shape distinguishes `Node *` from `int *` even without the name. If the
  scorer's normalization admits an aggregate pointer where ground truth wants a
  named struct pointer, shape alone converts rows. Our current `fission_agg16 *`
  on `head` suggests the machinery for emitting aggregates exists and is simply
  mis-sized and mis-applied.
- Shape plus a known signature gives a name: a pointer with layout `(0,4)
  (8,8)` passed to a function whose parameter is `Node *` names the shape, and
  every other pointer with that layout inherits it.

The second is the interesting one, and it is the same mechanism the rest of
this cycle's type work uses -- pull from a known signature, then propagate --
with the shape acting as the equivalence class that decides how far it travels.

## 7. AI Review / Prompt Firewall

- No external model was consulted. Measured on `corpus/dev`, which the
  benchmark does not score. The probe lives in a scratch directory; no
  production code was written.
