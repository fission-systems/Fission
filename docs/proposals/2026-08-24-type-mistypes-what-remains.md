# What is left of the type mistypes, and which third of it is not cheap

## 1. Baseline Row Anchor

Re-censused against DWARF on `corpus/dev` at `-O0` with the current build,
replaying `compute_type_match`'s three passes to recover the pairs it only
counts:

```text
matched variables 554   correct 466   MISTYPED 88
mistypes by pass  arg 50   offset 38
```

**88, down from 125** when this thread started -- the type work since has cut a
third of them. But the composition inverted, and the class that now looks
cheapest is the one that is not.

```text
class                              count   share
pointer's pointee wrong               26     30%   Node* -> int*, tm* -> int*
pointer printed as an integer         26     30%   char* -> ulonglong
width right, `long` unspellable       21     24%   long -> uint
other                                 15     17%
```

For comparison, the old census: ~40 were 32-bit widened to 64 (`int` ->
`longlong` twenty times). **That class is essentially gone.** What replaced it
at the top is its mirror -- and the mirror is a different problem.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [x] Normalize/type recovery
- [x] Signature/type resource ownership · [ ] Structuring · [ ] Printer

The 21 look like a one-line fix and are not. `type_name_is_informative`
discards `"long"` outright:

```rust
!matches!(type_name.trim(), "" | "int" | "long" | "void")
```

so `ftell` returning `long` never reaches `callsite_type_prop`'s return-type
propagation, which is gated on that predicate. Unfiltering it would not help.
`NirType::Int { bits, signed }` has no notion of `long`: on a 32-bit target
`int` and `long` are both four bytes and signed, so the model cannot tell them
apart, and `win_type_name_to_nir("long")` can only produce the same `Int` it
produces for `int`. **The filter is honest about the model it is filtering
into.**

The metric's normalizer confirms the consequence: `uint` and `int` both fold to
`{"int"}`, while ground truth for these variables wants `{"long int", "long
long"}`. There is no intersection, so getting the width exactly right still
scores zero.

`file_size` is the textbook row -- `long here = ftell(fp)` -- and we already
recover its `FILE *`. Only the `long` is unreachable.

## 3. Generality / Invariant Proof

Not formulated: this scopes the remaining work rather than doing it. The
invariant a fix must establish is that a declaration's *spelling* provenance
survives when its width and signedness do not distinguish it -- which is a
change to `NirType`, not to a predicate.

## 4. Risk And Ownership Check

The trap this measurement removes: relaxing `type_name_is_informative` looks
like a 21-row win and would instead flood the pipeline with `long` values
indistinguishable from `int`, replacing a miss with a differently-spelled miss
and possibly displacing types that were right.

An earlier hypothesis was also checked and refuted. The class was expected to
be eight-byte arguments split across two 32-bit slots -- `days_between(time_t,
time_t)` renders as four `uint` parameters on `gcc-m32`, which fits perfectly.
It is 6 of 27. The other 21 have ground-truth size 4: they are a 32-bit
platform's `long`, four bytes and correctly four bytes in our output.

## 5. Validation Matrix

- [x] Census replayed through the metric's own matching passes, 554 matched
      variables at `-O0`.
- [x] Split by build: 15 of the width rows on 32-bit, 12 on 64-bit; by
      ground-truth size: 21 at four bytes, 6 at eight.
- [x] `normalize_type` behaviour on `uint`/`int`/`long` confirmed directly.
- [x] Anchor rows read against source: `file_size` (`long` from `ftell`),
      `days_between` (`time_t` args).
- [ ] The other 15%. Unclassified, and the smallest class.

## 6. Where the next cheap work is

Two of the three classes converge on one mechanism -- **pulling a type from a
known signature and propagating it**. The pointer-as-integer class is being
attacked from format-string specifiers (`0098aa77e`); the `long` class would
come from callee return types, and cannot until the model can hold the answer.

That leaves the pointee class, 26 rows and the largest, as the next thing that
is neither in flight nor blocked on a model change. It is also the hardest:
`Node*` against `int*` is structure recovery, not spelling.

## 7. AI Review / Prompt Firewall

- No external model was consulted. Measured on `corpus/dev`, which the
  benchmark does not score. No production code was written for this.
