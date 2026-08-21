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
- [ ] Type-confusion census: which (ground truth, emitted) pairs make up the
      22%. **This is the next measurement and the one that names the fix.**

A harness defect found and corrected in passing: `compute_type_match` returns
its own `fn` key -- false negatives -- which silently overwrote the function
name being recorded. The false-negative figures were derived independently and
proved identical, so no reported number changes, but the storage census could
not run until the collision was fixed.

## 6. AI Review / Prompt Firewall

- No external model was consulted.
- Measurement ran on `corpus/dev`, which the benchmark does not score.
  Production code was not touched.
