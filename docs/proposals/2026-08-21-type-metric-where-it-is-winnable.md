# The type metric: which half of it is winnable

## 1. Baseline Row Anchor

Passing Glaurung on the sample-set means +27 perfect functions, and the gap is
not in one metric:

```text
              Union   GED   Types   Recompile   Union - GED
Glaurung         82    69      22           5            13
Fission          56    51       9           2             5
gap             +27   +18     +13          +3
```

The last column is what each tool scores *outside* structure. Fission's Union
is only five above its GED count, so the type and recompile metrics contribute
almost nothing that structure has not already won. Glaurung's contribute
thirteen. Structure has been this cycle's whole effort and its cheap defects
are spent; types are unexplored and worth up to thirteen.

Measured locally against DWARF ground truth on `corpus/dev`, which carries
debug info at every optimisation level and no symbol table:

```text
opt    functions   perfect     acc    TP    FP    FN
-O0          153    34%      0.642   65%   22%   13%
-O2          152    17%      0.441   40%   15%   45%
```

**A corpus caveat that decides the whole approach.** `corpus/scale` -- the
split used for the structure work -- scores 37% perfect on types against
DecBench's 3.8%. Ten times easier, because it is `-O0` with symbols and every
variable sits in a predictable stack slot. Optimising against it would teach
nothing about the scored case. `corpus/dev` at `-O2` is the representative
shape and is what this uses.

This also corrects a note carried from an earlier cycle. "76% of ground-truth
variables never match, capping type work at 0.239" is not a universal ceiling;
it is what that corpus measured. Here `-O0` misses 13% and `-O2` misses 45%.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [ ] Normalize · [ ] Structuring
- [x] Type/data recovery · [ ] Printer · [ ] Benchmark/automation

Where the variables live, at each optimisation level:

```text
                              -O0     -O2
stack slot (rbp_offset)      100%     21%
no stack slot                   0%     79%
```

And the ground-truth record `type_match` builds from DWARF has exactly these
fields: `arg_index, is_arg, name, rbp_offset, size, type`. **There is no
register field.**

That is decisive about scope. `compute_type_match` matches in three passes --
arguments by ABI position, stack variables by calibrated offset, everything
else by exact name. A non-argument local that the compiler kept in a register
has an empty `rbp_offset`, so the offset pass cannot reach it, and the only
remaining path is its *source name*, which no decompiler recovers from a
stripped binary.

So at `-O2`, the 79% of variables with no stack slot are unwinnable by any
decompiler beyond the arguments among them -- not unwinnable by Fission,
unwinnable by Ghidra and Glaurung too. Chasing them is chasing a number the
metric cannot award.

The winnable half is where a variable *is* matched and the type is wrong. At
`-O0` that is 22% of all ground-truth variables: found, mistyped. That is type
inference and nothing else.

## 3. Generality / Invariant Proof

Not formulated: this proposal scopes the work, it does not implement it. The
invariant belongs to whichever confusion the next measurement names.

## 4. Risk And Ownership Check

The risk in this area is the one `AGENTS.md` now calls out under "fitting the
metric instead of the row". Type accuracy is a matching game, and there are
ways to score better without recovering anything: emitting more variables
raises the chance one matches by offset, and emitting the most common
ground-truth spelling everywhere would score against a corpus where `int` is
189 of 571 variables. Both improve the number and recover nothing.

The test stands: state why the output is better without mentioning the metric.
"This variable is a pointer and we printed it as an integer" passes. "This
spelling matches more often" does not.

## 5. Validation Matrix

- [x] `compute_type_match` against DWARF ground truth over 305 functions,
      153 at `-O0` and 152 at `-O2`, across gcc and clang, 32- and 64-bit.
- [x] Storage census of every ground-truth variable at both levels.
- [x] Cross-checked `corpus/scale` to establish it is unrepresentative here.
- [x] Type-confusion census: which (ground truth, emitted) pairs make up the
      22%.
- [x] Whether assert strings can open the metric's exact-name pass.

A harness defect found and corrected in passing: `compute_type_match` returns
its own `fn` key -- false negatives -- which silently overwrote the function
name being recorded. The false-negative figures were derived independently and
proved identical, so no reported number changes, but the storage census could
not run until the collision was fixed.

### The confusion census

`compute_type_match` returns counts, not pairs, so its three passes were
replayed to record what was emitted every time a matched variable failed.
Across `corpus/dev` at `-O0`: 553 variables matched, 428 right, **125
mistyped**. Of those 125, **57 are function arguments** -- matched by ABI
position, which is the place we should have the best information there is.

```text
class                              count   examples
32-bit widened to 64-bit            ~40    int -> longlong (20), int -> ulonglong (4)
pointer emitted as an integer       ~20    char* -> ulonglong (8), FILE* -> ulonglong
64-bit narrowed to 32-bit            15    long/size_t -> uint (12)
pointer with the wrong pointee      ~15    Node* -> int*, float* -> uint*
```

**Nearly half of every mistype is width alone**, with signedness incidental.
That is consistent with typing from the register a value occupies rather than
from the operation performed on it: a 32-bit `int` living in a 64-bit register
becomes `longlong`. The second class, pointers printed as integers, is a
separate and equally concrete failure.

Neither needs the metric to justify fixing. "A `char *` was printed as
`ulonglong`" and "a 32-bit quantity was declared 64-bit" are both wrong on
their own terms.

### Assert strings: measured and declined

The metric's third pass matches by exact source name, which is the only route
to a register-resident local, and assert expressions are the one place a
stripped binary spells a variable's name outright -- glibc stores them as
`<expression>\0<file>.c\0<function>\0`, so the expression is recoverable by
reading the string physically before any `.c` filename.

It works, and the yield is too small. On the scored sample-set, 16 of 150
binaries (11%) carry any assert expression at all, giving **53 expressions and
49 distinct identifiers** across the whole corpus -- and most are `NULL`,
`result`, `datalen`. Release builds define `NDEBUG` and delete asserts
outright, so this is a property of what is in the binaries rather than of the
extraction.

Two earlier attempts at this measurement were wrong and are worth recording:
scanning all strings for expression-shaped text matched English prose from
error messages ("to", "not", "failed"), and tightening the pattern without
anchoring on the filename matched binary noise (`H+E`, `t%H`). Only the
adjacency structure separates a real assert from a coincidence.

## 6. AI Review / Prompt Firewall

- No external model was consulted.
- Measurement ran on `corpus/dev`, which the benchmark does not score.
  Production code was not touched.
