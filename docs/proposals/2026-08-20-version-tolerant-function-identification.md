# Version-tolerant function identification: measured and declined

## 1. Baseline Row Anchor

FID identifies statically linked library code by hashing masked instruction
bytes, so it matches only when the exact library build is in the database.
Measured this cycle on `vendor/binaries/tests/x86_64/elf_with_static_libc_
ubuntu_2004`: 466 functions hashed, 3 matched, and 328 of the 366 unmatched
names *are* present in the shipped databases -- the same functions built
differently.

The question this proposal answers: can an axis that survives a rebuild
replace or supplement the byte hash?

Measurement corpus: `fission-benchmark/corpus/scale`, which holds 258 programs
built at both `O0` and `O2-noinline` from the same sources, with symbols. Same
code, different codegen -- the same relationship a library version change
produces, and a *milder* one, since `-fno-inline` preserves function
boundaries.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [ ] Normalize · [ ] Structuring
- [ ] Type/data recovery · [ ] Printer · [x] Benchmark/automation

No production owner: nothing was implemented. This is a measurement filed so
the design is not re-litigated from first principles.

The design under test, drawn from what other tools do (radare2 zignatures
carry graph metrics and reference sets alongside bytes; retdec's `stacofin`
uses wildcarded patterns; angr and Glaurung use FLIRT, which radare2's own
documentation notes has "little chance" of surviving a version change):

```text
seed       string references  -> name a function outright
propagate  resolved callees become the caller's signature -> iterate
tiebreak   structure {branches, targets, calls} inside a candidate set
fast path  byte hash, unchanged, for the exact-build case
```

Note that Ghidra's FID record format cannot carry this: `FidbfFunction` holds
`{name, full_hash, specific_hash, code_unit_size, entry_point, flags}` and no
structural or reference data at all. Adopting the design would mean owning a
new format and a generation pipeline, not changing a matcher.

## 3. Generality / Invariant Proof

Not reached. The design was declined on measurement before an invariant was
formulated.

## 4. Risk And Ownership Check

The cost of the design is a new signature format, a database generation
pipeline, and a multi-axis matcher. The pre-registered acceptance criterion,
written down before the propagation run: **a seed of ~24% must reach ~60%
after propagation to be worth building; stalling near 30% means it is not.**

## 5. Validation Matrix

- [x] Axis discriminability over 20 `O0`/`O2-noinline` pairs, 8,417 functions
      present in both builds.
- [x] Coverage of each axis on the actual target population (a statically
      linked libc) as well as on application code.
- [x] Collision behaviour at database scale, 13,248 functions pooled from 40
      binaries.
- [x] Propagation gain with three admission rules.

Axis discriminability. Recall is whether the true name survived in the
candidate set across the rebuild; candidates is how many names came back:

```text
axis                          recall   median cands   <=5 cands   coverage
opcode seq (whole function)    39.9%              0        100%       100%
opcode seq (16-insn head)      48.7%              0        100%       100%
structure (exact 5-tuple)      50.4%             10         45%       100%
structure (branch,target,call) 69.7%          1,713         21%       100%
strings (full set)             86.5%              1         72%        13%
strings (any one)              97.8%              4         56%        13%
callees (any one)              99.4%*            21         29%        27%
strings or callees             99.1%*            16         32%        29%
```

\* the callee axis assumes callee names are already known, which a stripped
binary does not give -- it can refine, never seed.

Three things follow. Strings are the only axis that is tolerant, selective and
scale-robust at once. Structure is not a candidate generator at any setting:
made selective it loses half its recall, made recall-preserving it returns
1,713 candidates. And the byte axis is *not* worthless across a rebuild --
39.9% here, against the "roughly zero" this proposal started by assuming --
though that is an opcode-sequence proxy, which ignores operands entirely and
is therefore more forgiving than FID's real hash.

Coverage on the target population, functions of 8 instructions or more:

```text
                        static libc   application (dpkg)
with string references          24%                  41%
with callees                    66%                  73%
with either                     67%                  75%
string set unique in binary     92%                  95%
```

At database scale, pooling 13,248 functions from 40 binaries, 87% of distinct
string sets and 84% of single-string keys still resolve to exactly one name.
The axis does not collapse as the database grows.

Propagation gain, six targets, 2,602 functions, database of 8,203 named
functions:

```text
admission rule                       seed    final   precision
callees >= 2                        15.5%    18.1%        93%
callees >= 1                        15.5%    19.1%        93%
callees >= 1 + structure tiebreak    15.5%    23.5%        80%
```

**Declined.** Against a criterion of 60%, propagation reaches 18.1%, and the
23.5% variant buys its extra coverage by dropping precision to 80% -- which
for naming is the wrong trade, since a wrong name costs a reverse engineer
more than an absent one.

Propagation fails for a structural reason rather than a tuning one: at a 15%
seed most functions have zero or one resolved callee, so no signature forms
and the bootstrap never reaches its own threshold.

Both measurements are optimistic upper bounds. The database contained the
target's own `O0` twin, which no real database would. And the corpus is
application code, where 39-64% of functions carry strings, against 24% in the
statically linked libc that is the actual target -- so real seeding is worse,
not better.

## 6. AI Review / Prompt Firewall

- No external model was consulted.
- Vendored sources (`radare2-master/libr/include/r_sign.h`,
  `retdec-5.0/src/stacofin`, `angr-master/angr/flirt`,
  `glaurung-master/python/glaurung/tools/build_flirt_library.py`) were read
  only to establish what axes other tools carry.
- All measurement code lived in a scratch directory. No production code was
  written for this proposal.

## Postscript: what this cycle also established

Library identification does not appear to drive DecBench standing. Glaurung
ships 30 FLIRT signatures, extracted from its own hello-world test builds, and
places third. Ghidra's default distribution ships no FID databases at all --
they are a separate download -- and places eighth. Fission ships 57 databases
holding 1,834,901 functions and places eighth to ninth. Whatever separates
these tools, it is not this.

Pursue library identification as a product capability if it is wanted for its
own sake. Do not expect it to move a benchmark.
