# The compiler as a type oracle: does the gradient exist?

## 1. Baseline Row Anchor

The type metric loses most heavily at `-O2`, where 79% of ground-truth
variables have no stack slot and the harness's ground-truth record has no
register field, so the only remaining match path is the source name. Measured
on `corpus/dev`: `-O2` scores 17% perfect against `-O0`'s 34%.

This asks whether a different oracle is available. Types are not invisible in
a binary -- they are recorded in the code generator's choices. Signedness
reaches the object code through division, remainder, right shift and
relational compares: `idiv` against `div`, `movsx` against `movzx`, `sar`
against `shr`. So the compiler is a decision procedure for types, and a
decompilation can be scored by recompiling it and diffing against the
original.

Before building anything on that, the question is whether the signal is large
enough to follow.

## 2. Owner Proof

- [ ] any production owner

Nothing was implemented. This is a feasibility measurement.

## 3. Generality / Invariant Proof

Not formulated. The candidate invariant would be "a typing that recompiles
closer to the original is closer to correct", and section 5 is about whether
that quantity is measurable at all.

## 4. Risk And Ownership Check

This is the rare direction that cannot be overfitted: "our decompilation
recompiles to the original's instructions" is a statement about the binary,
not about a benchmark. It is also the only idea considered here that improves
the type metric and the byte-match metric with the same work.

The cost is a search. Types are per-variable and the space is exponential, so
it has to be coordinate descent driven by *where* the diff disagrees, not
enumeration.

## 5. Validation Matrix

- [x] Cross-compilation available locally: `clang --target=x86_64-unknown-linux-gnu -c`
      builds decompiled C to an object without a sysroot, which is all the
      comparison needs.
- [x] Source-side sensitivity: does a wrong type move the object code at all?
- [x] Output-side gradient: does it still move against a real decompilation?
- [ ] Whether the gradient can *find* a wrong type, not merely confirm a right
      one. **This is the next measurement and the one that decides the idea.**

**Source-side sensitivity.** Compile the corpus's own sources, change exactly
one type, recompile, count instructions moved:

```text
opt     movement across 8 programs
-O0     5% - 34% of instructions   (median ~10%)
-O2     6% - 73% of instructions   (median ~19%)
```

Not one mutation moved nothing. And the reaction is *larger* under
optimisation -- one signed-to-unsigned change rewrote 66 of `matrix_multiply`'s
91 instructions at `-O2`, because the compiler's vectorisation and
strength-reduction decisions turn on sign. **The oracle is strongest exactly
where the type metric is weakest.**

**Output-side gradient.** Against real decompiled output, with the mutated
variable chosen arbitrarily, 14 of 20 functions showed no movement at all --
an apparently fatal result. It was an artifact of the choice. Signedness only
reaches the object code through division, remainder, right shift or a
relational compare; changing the type of a variable that does none of those
legitimately changes nothing, so that measurement scored the mutation picker,
not the oracle.

Repeated with the mutation restricted to variables that participate in a
type-sensitive operation:

```text
function                orig/ours   d(as emitted)   d(mutated)     delta
count_bits                  11/11               3            4        +1
crc32                       50/47              40           48        +8
reverse_in_place            47/25              30           31        +1
manipulate_bitfields        17/ 8              13           13         0
rolling_hash32              46/40              34           58       +24
```

Four of five move, against six of twenty under arbitrary mutation.

**What this does not yet show.** Every delta here is positive, which says the
emitted type was already the better of the two -- the gradient confirms a
right answer. Using it to *find* a wrong one needs cases where a change
*reduces* the distance. Two appeared in the arbitrary-mutation run (-3 and
-5) and none in the targeted one, on five functions. The sample is far too
small to conclude either way, and a larger targeted run hunting for negative
deltas is the measurement that decides whether this is a search procedure or
only a verifier.

A second limit: where our structure differs wildly from the original the
baseline distance already exceeds the function's length -- `matrix_multiply`
compiles to 5 instructions against the original's 91 -- and no per-variable
signal is legible under that. An initial hypothesis that structural closeness
predicts gradient presence was tested and **refuted**: 17% of the closest
functions showed a gradient against 50% of the most distant. Whatever governs
it, it is not that.

## 6. AI Review / Prompt Firewall

- No external model was consulted.
- Measurement ran on `corpus/dev`, which the benchmark does not score, and on
  its own committed sources. No production code was written.
