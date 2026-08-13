# Handoff: goto-density work on Fission's structuring engine

Written 2026-08-14, at commit `108c089b7` on `main`. Everything below was
measured on this machine unless it says otherwise. Numbers without a
measurement behind them are marked as such.

---

## 1. What the work is

Fission is a decompiler. Its output carries far more `goto` statements than
other decompilers produce for the same functions, which is the single most
visible quality gap. The job is to reduce that without changing what the
decompiled code means.

### Where it stands

Corpus is `vendor/decbench-evalkit/decbench-evalkit-sample-set` — 224 binaries,
250 functions. Two output layers, NIR (semantic) and HIR (readable).

| | start of this session | now |
|---|---|---|
| NIR gotos | 2089 | **1214** (−41.9%) |
| HIR gotos | 1983 | **1207** (−39.1%) |

Against the references, on the 204 functions all three tools decompile:

| tool | gotos | Fission ÷ it |
|---|---|---|
| **Fission** | **1165** | — |
| Ghidra | 691 | 1.69x |
| angr | 420 | 2.77x |

Ghidra ratio went 2.84x → 1.69x today. Two files regressed across the whole
session (`bin_217` 2→3, `bin_027` 0→1); everything else improved or held.

**angr is better than Ghidra here, by a lot.** That surprised me and I had it
backwards earlier — worth stating plainly since it changes which reference to
aim at.

---

## 2. Architecture, as it is now

Three structuring strategies exist, corresponding to the three published
approaches:

| strategy | reference | file |
|---|---|---|
| rules over a live graph | Ghidra `CollapseStructure` | `crates/fission-midend-structuring/src/collapse_structure.rs` |
| schema match-and-fold | angr Phoenix | `crates/fission-midend-structuring/src/collapse_driver.rs` |
| reaching conditions | DREAM / angr `condition_processor` | `crates/fission-midend-structuring/src/reaching_driver.rs` |

Supporting pieces, all in `crates/fission-midend-structuring/src/`:

- `collapse_graph.rs` — a CFG that **shrinks** as regions fold. This was the
  root fix of the session: Fission structured over a static graph with side
  tables while every reference implementation folds a live one.
- `collapse_shapes.rs` — pure graph matchers (`Sequence`, `IfThen`,
  `IfThenElse`, `IfNoExit`, `SelfLoop`, `WhileDo`, `DoWhile`, `InfLoop`).
- `reaching_conditions.rs` — `reaching(n) = OR over preds p of (reaching(p) AND edge(p→n))`.
- `reaching_emit.rs` — turns those conditions back into nested `if`s.
- `boolean.rs` — truth-table decision procedure over guard atoms.
- `structuring_quality.rs` — **the comparator**; see below.

### The control flow through it

`crates/fission-pcode/src/midend/pass/structuring.rs`, in
`SeseStructuringPass::run`:

1. The existing SESE path produces a body.
2. `try_alternative_structurings` offers that body to each driver, which runs
   against a **fork** of the host and returns a candidate or declines.
3. A candidate replaces the baseline only if it **wins the comparison**.
4. `finalize_structured_body`, then normalize downstream.

### Two things that are load-bearing and non-obvious

**Host isolation** (`StructuringHost::lower_isolated` / `lower_observed`,
implemented in `crates/fission-pcode/src/midend/structuring/host_impl.rs`).
Lowering is *not* a query — `lower_block_stmts` mints names as a side effect,
so a driver that lowers a candidate to decide whether to keep it has already
changed the host when it declines. That leak reverted an earlier attempt at
this work and reappeared here. Every speculative lowering must route through
`lower_isolated`.

**The comparator** (`structuring_quality.rs`). angr keeps a transform when the
goto count strictly drops. That is the right idea and the wrong measurement
here, because goto density cannot see the ways a structuring gets worse. Each
axis was added because a driver scored an improvement while emitting something
worse:

- `switches` — hard veto. A switch dispatch is a cascade of two-way branches,
  which DREAM describes perfectly as nested `if`s, losing the `switch`.
- `empty_if_shells` — hard veto. Match-fold emitted `if (c) { }` where the
  existing path folds a short-circuit `&&`.
- `nesting_depth` — **budget**, one level per jump removed. Nesting is the
  *price* of trading a jump for a condition, not a defect. Banning any increase
  looked right and rejected a candidate going from ten jumps to none.

The comparator judges each body twice: raw (exact at that stage) and after
`finalize_structured_body` + `normalize_function_body` (closer to what ships).
A candidate must **win on raw and not lose on cleaned**. Measured: raw only =
1225 jumps / 7 regressions; cleaned only = 1313 / 0; both = **1214 / 2**.

---

## 3. How to measure anything

Every claim below came from these. Run them; do not trust reasoning about what
the code should do (see §5).

### Corpus

```bash
cd vendor/decbench-evalkit/decbench-evalkit-sample-set
cargo build --release -p fission-cli          # from repo root first
rm -rf results && python3 run_fission.py      # ~5m20s, NIR
grep -ro 'goto ' results | wc -l
FISSION_LAYER=hir python3 run_fission.py      # HIR into results-hir/
```

Per-file comparison against a saved baseline directory — **always check
per-file, not just the total**; a net gain can hide regressions:

```python
def counts(d):
    return {os.path.relpath(os.path.join(r,f),d):
            open(os.path.join(r,f),errors='replace').read().count('goto ')
            for r,_,fs in os.walk(d) for f in fs}
```

### Reference outputs

Already generated, in this session's scratchpad — regenerate with
`run_ghidra_sample.py` / `run_angr_sample.py` found alongside them:

- Ghidra: `ghidra_out/` (716 total, 219 files)
- angr: `angr_out/` (428 total, 207 files)

**Compare only on the intersection** (204 files). The three tools decompile
different subsets and raw totals are not comparable.

Ghidra gotcha, already paid for once: DecBench addresses are link-time vaddrs
but Ghidra assigns its own image base for PIE binaries. Retry with
`addr + program.getImageBase().getOffset()` or every PIE binary silently
returns "no function at address".

### The driver's own census

`FISSION_PREVIEW_DIAG=1` makes the drivers report per function:

- `[DIAG] alternatives not asked` — baseline already jump-free
- `[DIAG] DREAM offered: stmts=N` / `[DIAG] DREAM declined: <reason>`
- `[DIAG] DREAM accepted: gotos A -> B` / `rejected: ..., worse on [...]`

`DeclineReason` in `reaching_driver.rs` enumerates the refusals.

**The census goes stale the moment you change the driver.** It has misled me
three times in this session — twice I acted on a bucket that had already
vanished. Re-measure after every change.

### Current funnel (250 functions)

| | n |
|---|---|
| already jump-free, drivers not asked | 53 |
| single-block functions, never reach the pass | 53 |
| **asked** | **144** |
| → offered a candidate | 116 |
| → **accepted** | **81** |
| offers rejected | 35 (29 are ties with nothing to gain) |
| declined outright | 28 (cycle-survived 14, unplaceable-terminator 10, guards-too-large 4) |

---

## 4. Dead ends — do not repeat these

Each was built, measured, and abandoned. The reasoning that motivated each one
was plausible; that is exactly why they are worth recording.

| attempt | result |
|---|---|
| cross-jump reversion (AST and sibling-arm) | the copied tail contained a goto, so copying multiplied it: net zero |
| fall-through tail reversion | 0 firings across the corpus |
| `ruleCaseFallthru` | 0 opportunity |
| `ruleBlockIfNoExit` via larger tails | net negative |
| node splitting | 0 of 250 functions have irreducible SCCs |
| short-circuit condition folding | 0 successes, 12,794 rejections, 96.4% blocked by un-hoistable `Load`s |
| `FISSION_COLLAPSE_LOOP=1` on the static graph | +51 gotos, 34 files worse |
| the "53 functions that never reach the driver" bucket | 49 are single-block functions holding **6 gotos between them** |
| comparator margin (require winning by ≥2) | corpus moved 2 jumps, all 7 regressions unchanged |
| raising `collapse_driver::MAX_CONCESSIONS` | **unsound as written** — its `concede_one_edge` only calls `virtualize_edge` and emits no `goto`/label, so it would silently delete control flow |
| stronger boolean simplifier | fires 45 times across 25 functions, **changes no output** — see below |

**On the simplifier specifically**, since it looks like the obvious next lever:
the hypothesis was that a weak condition simplifier bounds DREAM-style
structuring, as claripy does for angr. It was built (`boolean.rs`, truth tables
over atoms) and it does fire. It changes nothing, because
`materialize_branch_conditions` binds every edge condition to a fresh boolean
*variable*, so reaching conditions are already near-canonical trees over
variables and the syntactic folds already caught what mattered. angr's
simplifier earns its keep on raw condition expressions; Fission sidesteps that
problem by construction.

**Caps that turned out to be guarding what the comparator already guards**, and
cost real gains until raised:

- `MAX_NESTING_DEPTH` 8 → 64 (the comparator judges depth against the actual
  baseline; a constant can only be wrong in one direction)
- `MAX_CONCESSIONS` 3 → 128 (measured: 3 = −175, 8 = −221, 24 = −232,
  128 = −244, uncapped = −244; runtime flat throughout)

Caps that are **real**, because they guard downstream pipeline cost that the
comparator cannot see: `MAX_DECISIONS = 16` and
`MAX_GUARD_FORMULA_SIZE = 8000`. One region with 39 decisions and depth 9 —
unremarkable on both — had guards totalling **160,423 expression nodes** and
cost **45 seconds downstream** against 3.1s on the existing path. Healthy
candidates run 2 to 2,008 nodes. The two populations do not overlap.

---

## 5. Method — this is the part that actually mattered

Every time I reasoned about where a bound or a blocker was, I was wrong. Every
time I measured, I was right. Three examples from one day:

- "Cycles survive because the fold loop only folds cyclic shapes." Correct
  reasoning, fixed it, **changed the count by exactly zero**. Instrumenting
  where the loop actually stopped found the real cause: `lower_shape` declined
  `SelfLoop` outright, on 36 of 106 functions.
- "The execution check fails because the interpreter has no memory model."
  Built one. **Coverage moved by zero functions.** The real cause was the
  *arguments* — a boundary sweep hands a pointer parameter the integers 0, 1,
  −1.
- "Terminal `Goto` means a tail call whose target left the graph." **All 30
  resolve to a block in the same function.**

**A check that passes proves nothing until you have watched it fail.** Before
trusting any verification, inject a bug it must catch. This is how the
execution-differential check was found to be worthless in its first two forms —
it passed, and it also passed with every `if` condition in every function
negated.

**Goto density scores real regressions as improvements.** Three times in one
day: a deleted `for` loop, a dropped `return` (signature degrading to
`undefined`), a `switch` rewritten as nested `if`s. Each was caught by a
different accident. The corpus reported "no regressions" for all three.

---

## 6. What guards correctness

`structuring_quality` covers only failures already observed, by construction.
The independent check is execution:

`crates/fission-dir/tests/phase2_corpus_ground_truth.rs` — decompile a corpus
function, evaluate the body, run the real machine code through
`fission-emulator`, compare. Twelve functions ground; it catches
"negate every `if` condition" (four `list_sum` divergences).

It took three attempts to make it catch anything, and its reach is capped for a
structural reason: ~72 of 900 attempts bail on indirect call dispatch and ~212
on raw register or global reads. Supplying the registers from the emulator was
tried and bought nothing — the unblocked functions stop at the very next call.
Interpreting past that means resolving indirect targets and executing callees,
which is being an emulator, and there already is one.

Running it at scale found two real defects in the checker itself: the emulator
side was normalised to the return type while the interpreter side was compared
raw (`mul_ints(1,-1)` reported 4294967295 against −1, one 32-bit pattern read
two ways), and uninitialized locals were seeded to 0 against the module's own
documented contract.

---

## 7. Where I would go next, and why

**The analysis that guided most of this session is now obsolete, and I kept
quoting it after it stopped being true.** The claimed floor for
graph-preserving rewrites was `Σ(P−1) ≈ 1365` on this corpus. Fission is at
**1214** — below it. Concessions change the graph, so the bound no longer
applies. **There is currently no valid picture of where the remaining 1214
gotos live.**

Recommended first move: **rank functions by `fission_gotos − ghidra_gotos` and
read the worst ones side by side.** Reasons:

- It found the biggest win of an earlier session (an admission gate skipping
  structuring entirely on large functions, purely on size).
- The data is already on disk (`ghidra_out/`), so it costs one script.
- It assumes nothing about where the problem is, which matters given §5.

Every improvement today was a **wrong refusal or an unnecessary cap**, not a
missing algorithm — `SelfLoop` declined outright (36 functions), a `while` test
block with statements (25), a folded `Sequence` forgetting its terminator (20),
a terminal clause's transfer dropped rather than emitted (18). It is worth
looking for more of that shape before assuming the remaining gap needs new
theory.

Also unexamined: angr's `region_identifier.py`. It is the one large piece of
how angr reaches 420 that has not been read. But measure *which Fission
functions would benefit* before reading it — that is the order that the
simplifier experiment got backwards.

**Possible simplification:** the match-fold driver is now redundant.
`FISSION_MATCH_FOLD=0` gives identical goto counts on every file, with no
runtime difference (it wins on ~5 files where DREAM then ties, so the output
differs byte-wise without differing in count).

---

## 8. Working constraints

- Push to `main` only. Do not cut a release tag unless explicitly asked.
- DecBench resubmission via `package.py` stays deferred.
- The workspace has **7 pre-existing `fission-emulator` failures** unrelated to
  this work (SLEIGH decode). A clean run shows exactly those seven and nothing
  else.
- `vendor/decbench-evalkit/.../binaries/<name>_ghidra/` project caches are
  expensive to regenerate — do not delete them.
- Full suite: `cargo nextest run --workspace --no-fail-fast`, ~110s.

---

## 9. Environment switches

| variable | effect |
|---|---|
| `FISSION_PREVIEW_DIAG=1` | driver census and per-decision diagnostics |
| `FISSION_MATCH_FOLD=0` | disable the match-fold driver (on by default) |
| `FISSION_DREAM=0` | disable the DREAM driver (on by default) |
| `FISSION_LAYER=hir` | corpus runner emits HIR instead of NIR |
| `FISSION_DIR_FULL_CORPUS=1` | ground-truth test sweeps without its attempt budget |

---

## 10. Commits from this session

Oldest first, `4f1326688..108c089b7`. The messages carry the measurements and
the reasoning, including the parts that were wrong; they are worth reading
before changing any of this.

```
11bbeff59  host isolation entry point, and what it caught
e3edf395d  the DREAM emitter
d005b103b  wire the DREAM driver, and measure it
2e3267658  drivers offer candidates instead of pre-empting
88cf540c0  let the comparator decide nesting depth
c56182274  bound the guards, not a stand-in for them
f1335f44c  name the DREAM refusals, then fix the biggest one
516310e99  two more counted refusals, and folding stops blinding rules
220411e89  ground truth over the corpus, and what it cannot check
9cab5381a  a memory model, and a correction to why execution still misses structuring
bae5833ec  give pointer parameters something to point at
a1786d003  keep the transfer a terminal clause ends on
1e06857d7  supply the caller state a body reads, and find the ceiling behind it
3c05ce2e0  concede an edge when nothing matches and a cycle remains
6d5b4dc1f  do not concede a node's last way in
4f4c9366f  the concession budget was guarding what the comparator guards
726cf55ff  concede only what leaves every node still reachable
9f952dc81  a shape for a cycle with no way out
d21116bf0  write out the jumps whose edges were removed before we saw them
9e395f0fe  judge candidates through the cleanup that follows them
108c089b7  decide what a guard means instead of what it looks like
```
