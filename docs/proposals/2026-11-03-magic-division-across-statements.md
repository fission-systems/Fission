# Constant division recovered across a coalesced statement chain

Filled per [ADR 0006](../adr/0006-decompiler-quality-change-gate.md), before the
implementation. Measured numbers in section 1 are the baseline; the result is in
section 8, filled after the remeasure.

## 1. Baseline Row Anchor

Corpus: all 791 x86-64 ELFs under `~/fission-benchmark/decbench-data/binaries/{O0,O2,O2-noinline}`,
`fission_cli decomp --all --no-db`, `FISSION_BENCHMARK_NO_CACHE=1`, base = `main`
at `28dc1c88b`. 197,449 functions decompiled.

- Residual magic multiply (`* 1717986919 | 1374389535 | -1840700269 | 2863311531 |
  3435973837 | 715827883 | 954437177`) in **309 functions** (916 lines), across all
  three optimisation levels and many programs (coreutils, rsyslog, bash, iproute2, gnutls,
  tar, crazyflie, ...). Per constant, files affected: 1717986919: 55, 1374389535: 59,
  -1840700269: 48, 715827883: 24, 954437177: 35, 2863311531: 28, 3435973837: 16.
- Small anchors (all recovered to `x / N` by this change):

| Function | Binary | Address | Before | Shape |
|---|---|---|---|---|
| `base32_length_wrapper` | O2/coreutils/basenc | 0x3160 | `(uint)((uint)rdi * 1717986919 >> 33) - (uint)((uint)rdi >> 31) << 3` | signed, `sar` + sign correction |
| `iso_week_days` | O0/coreutils/date | 0x579f | `(uint)((uint)((long long)uVar16 * -1840700269 >> 32) + uVar16 >> 2) - (uint)((uint)uVar16 >> 31)` | signed, high-half + `x` form |
| `getLoadLimit` | O2-noinline/rsyslog/rsyslogd | 0x71760 | `((size << 6) + size) * 1374389535 / 137438953472` | unsigned, shift already a `Div 2^n` |
| `uinttostr` | O2/coreutils/sort | 0xe1b0 | `iVar5 = rdi * 3435973837 / 34359738368` | unsigned in a loop body |

- DecBench sample-set gate (`decbench_sample_set_gate.py check`, no cache):
  **perfect 68/233, total distance 7731**, types 16/222. None of the 250 scored rows
  contains a residual magic multiply.

## 2. Owner Proof

- [x] Normalize: `arith/` (constant-division recognizers), next to
  `recognize_magic_number_division` and `collapse_cdq_signed_mod_in_stmts`.

The first owner that creates the wrong fact is not the recognizer's pattern table.
`recognize_magic_number_division` is an expression rewriter run bottom-up from
`normalize_expr`; instrumented, it only ever receives the bare multiply, because
register coalescing leaves the idiom as a statement chain over one variable:

```text
rdi    = param_1 + 4
xVar10 = cast<u32>(rdi)                 // movsxd; the lifter drops the sign into a u32 cast
iVar11 = 31
uVar1  = cast<u32>(rdi) Sar iVar11      // sign mask
xVar10 = xVar10 Mul<i64> 1717986919
iVar11 = 33                             // the shift amount lives in a reused variable
xVar10 = xVar10 Sar<i64> iVar11
xVar10 = cast<u32>(xVar10) Sub cast<u32>(uVar1)
```

Two further facts the existing recognizer cannot use: `expr_type(Var)` is
`Unknown`, so it never learns the operand width, and for the unsigned anchors the
shift has already been canonicalised to `Div<u64> 2^n`, which it does not match.
HIR presentation later inlines the chain into the one-line expression seen in the
C, but presentation is not an allowed owner for recognition (ADR 0011).

## 3. Generality / Invariant Proof

Generalized rule:

```text
A statement's value is, after substituting the block-local reaching definition of
each operand (locals whose address never escapes, pure value expressions only),
floor(x*M / 2^n) [+ sign fix-up]. With d = ceil(2^n/M), e = M*d - 2^n > 0 this
equals x / d for every x of the operand range iff
  unsigned, x in [0, 2^w):                    (2^w - 1) * e <  2^n
  signed with `- (x >> (w-1))`, x in signed w-bit range:  2^(w-1) * e <= 2^n
plus: multiply width >= 2w (no overflow), shift >= w. The high-half form
`((hi_w(x*(M-2^w)) + x) >> k) - (x >> (w-1))` is the same family with M' = M-2^w
and n = w + k. The bound is the Granlund-Montgomery criterion; it is derived, not
copied, and checked exhaustively on narrow widths in the unit tests.
```

The operand's extension kind is **not** read from the wrapping cast (the lifter
emits a sign extension as `cast<u32>`); the idiom decides: a sign-correction term
means signed, a bare shift means unsigned. Operand identity between the multiply
and the sign mask is structural equality of the resolved operand.

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] No ISA / calling-convention gate. The rule reads only expression structure,
      widths from the binding table, and local def-use order.
- [x] No ISA data needed: widths come from `PreHirBinding` types.
- [x] Synthetic tests state the dataflow shape with no compiler tuple or name.

Comparable coverage: signed /5, /10, /100 (shifts 33, 34, 37), signed /7 high-half
form, unsigned /10 and /100, in O0, O2 and O2-noinline across many programs.

## 4. Risk And Ownership Check

- Existing owner: `recognize_magic_number_division` stays (expression-level, no
  regression path). `collapse_cdq_signed_mod_in_stmts` is the precedent for a
  block-local reaching-definition scan in `arith/`; this pass reuses its shape.
- Shared substrate: [x] Def-use / reaching-definition fact (block-local, kill on
  redefinition of the variable or any variable it mentions; cleared at labels,
  gotos, nested control flow; locals with escaping address are never tracked;
  no memory reads or calls are ever part of a tracked value).
- Why a new pass rather than extending the expression recognizer: the expression
  recognizer has no access to sibling statements or to binding types. A
  statement-level view is the invariant; it is not a pattern in a table.
- Interaction: runs once, late (beside the CDQ collapse), then the existing
  dead-assign passes remove the overwritten chain. No statement is deleted by
  this pass itself.
- Known cases that must not change: any multiply whose constants fail the bound;
  a chain where the dividend is redefined between uses; address-taken locals.
- Telemetry impact: none (no `NirBuildStats` field).

## 5. Validation Matrix

- [x] Targeted invariant tests: `cargo nextest run -p fission-midend-normalize magic_div`
  (14 tests: shapes, negatives, exhaustive soundness of both bounds for widths
  8..14 and all d <= 200, 32-bit edge points for the observed constants).
- [x] Crate-level gate: `cargo nextest run -p fission-midend-normalize -p fission-pcode -p fission-midend-prehir -p fission-midend-structuring -p fission-decompiler` -- 2142 passed.
- [x] Focused benchmark: sample-set gate, then published source CFGs of the changed
  functions (section 8).
- [x] Smoke sweep: all 791 ELFs base vs new; compare outputs before scores; rerun
  differing binaries with both CLIs to separate time-budget noise.
- [x] `cargo fmt --all --check`, `python3 scripts/audit/nir_boundary_scan.py --root .`.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice? [x] No.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits: [x] Confirmed

## 8. Result

Final build vs base (`main` at `28dc1c88b`), `FISSION_BENCHMARK_NO_CACHE=1`, same
machine, same 791 ELFs, output compared before scores.

**What moved (mechanical, text):**

- Functions with a residual magic multiply: **309 -> 148** (-52%). 142..178 binaries
  differ; the other binaries are byte-identical to base. Per-constant remainder is
  mostly shapes this pass does not cover (see below).
- All four anchors recovered (`(len + 4) / 5 << 3`, `iso_week_days` `/ 7`,
  `getLoadLimit` `(uint)((size << 6) + size) / 100`, `uinttostr` `(uint)rdi / 10`).
- Roughly 410 functions change text in total: ~210 lose a magic multiply, ~170 change
  only through interprocedural ripple (callee arity/type summaries; 44 arity-only
  decreases, 3 increases, the rest structural). The ripple is unverified in
  general; the one case checked against source (`sort`: `numcompare`/`keycompare`
  take two arguments, base printed four/three) moved toward the source.
- Noise control: 29 functions flipped between a decompile and the disassembly
  fallback (18 base-only, 11 new-only -- time-budget noise, excluded); ~130
  functions differ between two runs of the *same* base CLI and were excluded.

**What did not move (the metric):**

- Sample-set gate: **perfect 68/233, total distance 7731, types 16/222 -- identical to
  base, 0 of 233 rows differ** (no scored row contains a residual magic multiply).
- Published-source-CFG GED of the 172 changed functions that have one: total distance
  10015 -> 9948 (-67), perfect 43 -> 43, 9 improved / 0 worse / 163 identical. The 9
  improved rows are **two distinct functions** (`sshkey_fingerprint` linked into 8
  openssh binaries, -8 each; `copy_dir_entries`, -3). Every function whose only change
  was the division (e.g. `base32_length_wrapper`, `iso_week_days`, `getLoadLimit`) has
  identical GED and CFG node count: GED sees block topology, and `x / d` does not
  change it.

**Claim:** mechanical + output-text change. No DecBench quality claim: the scored
rows did not move and the unscored-population movement is two functions. Why the new
text is better without the metric: the old C evaluated `(uint)rdi * 1717986919 >> 33`
in 32 bits (wrong on overflow and on negative input -- the multiply is a 64-bit
operation in the machine code), and printed a magic constant, a shift and a sign
correction where the program says `/ 5`. The new C states the division. The unseen
`corpus/scale` split was not run.

**Not covered, remaining 148 functions:** unsigned high-half-plus-add forms
(`t = hi; q = (((x - t) >> 1) + t) >> k`), 64-bit dividends (`__int128` products),
quotients consumed by a multiply-subtract (`x - q * d`, the modulus idiom), and
products whose operand is a load or call result (never tracked). Each is a separate
recognizer; none was added without a measured repeat.

**Known leftover:** `touch.c:tm_year_str` keeps one dead copy (`rbx = param_1;`) that
only HIR presentation makes dead; the base had no such copy.
