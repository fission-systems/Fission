# WBCQ P3 evaluation: behavioural quotients on compiled, obfuscated and real code

Date: 2026-11-23. RFC: #1093, stage P3. Status: **measured**. Engine at the
commit of this document's PR (`fission-quotient`, `fission_cli quotient`).

This is not a decompiler-quality claim. `quotient` is a separate experimental
command, and nothing here changes decompiler output or DecBench results. The
question is the RFC's own (section 11):

- **H1.** On flattened, state-machine and protocol-parser code, the quotient
  cuts node/edge complexity by at least 50% against CFG structuring. It
  preserves behaviour on at least 99% of checks, with no spurious state.
- **H0.** The gain is not significant against constant propagation plus
  opaque-predicate removal. Or 60% or more of real binaries degenerate to
  `Unknown`/`PartialFrontier`.

## 1. What was measured

| Corpus | What | Size |
|---|---|---|
| (a)+(b) state machines | 10 machines from published descriptions (RFC 793 TCP, HTTP/1.x request line, RFC 3629 UTF-8, Telnet, SLIP, keypad lock, debouncer, coin acceptor, traffic light, Base64) and 1 large one (XOR-checksummed frame). Each is a step function `unsigned step(ctx *, unsigned in)`. | 11 functions |
| negatives | SHA-256 round, linked-list walk, `rdtsc`, a system call (RFC section 11's intentional negatives) | 4 functions |
| builds | {x86-64, AArch64} x {O0, O1, O1+flattening, O1+opaque-predicates+flattening} | 8 binaries, 120 runs |
| (c) real binaries | DecBench x86-64 ELF functions whose DWARF signature is `<int> f(<struct> *, <int>)`. Selected by signature only, so the body (calls, globals, loops) is not filtered. One copy per (project, opt). | 728 found, 300 sampled |

Tooling (all under `scripts/quality/wbcq/`):

- `obfuscator/WbcqObfuscator.cpp`: OLLVM-style `opt` plugin.
  - `wbcq-fla`: dispatcher switch on a stack state variable, with pseudo-random 32-bit case numbers.
  - `wbcq-bcf`: `(x*(x+1)) & 1 == 0` opaque predicates with junk blocks.
  - It runs on the -O1 IR after the optimizer, and only `llc` lowers it, so no IR pass can undo the obfuscation (OLLVM's order).
- `build_corpus.py`: builds every (ISA, variant), plus a native library.
- `ground_truth.py`: the minimal machine, computed without Fission.
  1. It calls the natively compiled source through `ctypes`.
  2. It explores the reachable contexts breadth-first over all 256 input bytes.
  3. Moore partition refinement then merges contexts no input word tells apart.
- `evaluate.py` drives `crates/fission-quotient/examples/p3_eval.rs`. For each function, it runs:
  - the white-box quotient;
  - 200 random words of length 24, replayed on the emulator and on the learned machine;
  - a W-method conformance suite (every state's access word, times every alphabet symbol, times every distinguishing suffix), replayed the same way;
  - a black-box ablation: the same L# learner and emulator oracle, but with a 16-value sampled alphabet, random-walk equivalence, and no P-code summaries or solver;
  - `fission_cli decomp`, measured as text (lines, gotos) as the CFG-structuring baseline.
- `select_decbench.py` picks the real-binary sample, and `evaluate.py decbench` runs it.

Reproduce (about 40 min for the corpus, 5 min for DecBench, on 14 cores):

```bash
python3 scripts/quality/wbcq/build_corpus.py --out ~/fission-benchmark/wbcq-p3/corpus
python3 scripts/quality/wbcq/ground_truth.py ~/fission-benchmark/wbcq-p3/corpus/manifest.json \
  --out ~/fission-benchmark/wbcq-p3/ground_truth.json
cargo build --release -p fission-cli -p fission-quotient --example p3_eval
python3 scripts/quality/wbcq/evaluate.py run --corpus ~/fission-benchmark/wbcq-p3/corpus \
  --out ~/fission-benchmark/wbcq-p3/results --eval target/release/examples/p3_eval \
  --cli target/release/fission_cli -j 8
python3 scripts/quality/wbcq/evaluate.py summarize --out ~/fission-benchmark/wbcq-p3/results \
  --truth ~/fission-benchmark/wbcq-p3/ground_truth.json
python3 scripts/quality/wbcq/select_decbench.py ~/fission-benchmark/decbench-data \
  --out ~/fission-benchmark/wbcq-p3/decbench_candidates.json
python3 scripts/quality/wbcq/evaluate.py decbench \
  --candidates ~/fission-benchmark/wbcq-p3/decbench_candidates.json \
  --out ~/fission-benchmark/wbcq-p3/decbench --eval target/release/examples/p3_eval -j 6
```

Results stay under `~/fission-benchmark/wbcq-p3/`, not in the repository.

## 2. Corpus: states learned against ground truth

`=` means the count equals the minimal machine of the natively compiled
source. Statuses:
- **P**: `proved_under_model`;
- **F**: `partial_frontier`;
- **U**: `unsupported_semantics`.

| function | truth | a64 O0 | a64 O1 | a64 fla | a64 bcf+fla | x64 O0 | x64 O1 | x64 fla | x64 bcf+fla |
|---|---|---|---|---|---|---|---|---|---|
| `tcp_step` | 11 | P 11= | P 11= | P 11= | P 11= | P 11= | P 11= | P 11= | P 11= |
| `http_reqline_step` | 22 | P 22= | P 22= | P 22= | P 22= | P 22= | P 22= | P 22= | P 22= |
| `utf8_step` | 9 | P 9= | P 9= | P 9= | P 9= | P 9= | P 9= | P 9= | P 9= |
| `telnet_step` | 5 | P 5= | P 5= | P 5= | P 5= | P 5= | P 5= | P 5= | P 5= |
| `slip_step` | 2 | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= |
| `pin_step` | 13 | P 13= | P 13= | P 13= | P 13= | P 13= | P 13= | P 13= | P 13= |
| `debounce_step` | 7 | P 7= | P 7= | P 7= | P 7= | P 7= | P 7= | P 7= | P 7= |
| `vending_step` | 5 | U | P 5= | P 5= | P 5= | U | U | U | U |
| `traffic_step` | 7 | P 7= | P 7= | P 7= | P 7= | P 7= | P 7= | P 7= | P 7= |
| `base64_step` | 4 | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= |
| `frame_step` | 2051 | F | F | F | F | F | F | F | F |
| `sha256_round_step` | - | U | F | F | F | U | F | F | F |
| `list_walk_step` | - | U | U | U | U | U | U | U | U |
| `clock_step` | - | U | U | U | U | U | U | U | U |
| `syscall_step` | - | U | U | U | U | U | U | U | U |

- **Correctness.** All 75 machines have exactly the ground-truth state count.
  The count is the same across optimization level, flattening, opaque
  predicates and ISA.
  - The solver proved every one (`proved_under_model`), to horizons of 16–54
    steps (median 39).
  - All apartness witnesses re-ran on the emulator.
  - Random replay agreed on 15,000/15,000 words, and W-method conformance on
    102,450/102,450 words. No spurious state and no wrong transition was
    observed.
- **No machine (fail-closed).** Every function without a machine is either
  outside the model or larger than the budget.
  - `vending_step`: the x86 build divides by 5 through a 128-bit multiply,
    and the AArch64 -O0 build uses a divide by a value the summary cannot
    prove non-zero.
  - `frame_step`: 2,051 states, past K_max = 32.
  - The four negatives: a call at -O0, then a state count past the budget,
    a data-computed address, and effects outside the P-code. Not one
    negative produced a machine.

## 3. Structural complexity (75 machines)

| build | median P-code CFG blocks | median HIR lines / gotos (`decomp`) | median quotient states / transitions |
|---|---|---|---|
| O0 | 30 | 76 / 2.5 | 7 / 24 |
| O1 | 17 | 52 / 0 | 7 / 24 |
| O1 + flattening | 30 | 156 / 14 | 7 / 24 |
| O1 + opaque + flattening | 66 | 257 / 32 | 7 / 24 |

- **Reduction.** Median node reduction is 80% against the P-code CFG and 80%
  against the pruned CFG. Median edge reduction is 59% and 56%.
- **The pruned CFG (H0 baseline)** is the CFG left after path-sensitive
  constant propagation and pruning of solver-refuted branch sides: the blocks
  and edges some feasible path uses. Opaque-predicate removal alone shrinks
  the obfuscated CFGs by 0–15% and leaves the dispatcher intact.
- **Sensitivity to obfuscation.** Obfuscation multiplies the CFG (O1 → bcf+fla:
  17 → 66 median blocks) and the decompiled text (52 → 257 lines, 0 → 32
  gotos). It leaves the quotient unchanged.
- **Where the reduction is negative.** For a counter machine, the reduction
  is negative: `debounce_step`, `traffic_step`, and `pin_step` at O0/O1 have
  more states than CFG blocks. Their state is data that the CFG does not show,
  so the CFG is not a smaller description of the behaviour; it describes less
  of it. The node-reduction figure compares two different things there, and
  is reported as measured.

## 4. Ablation: black-box learning (RFC baseline 4)

| function | truth | white box = truth | black box = truth | black box states (median) |
|---|---|---|---|---|
| `tcp_step` | 11 | 8/8 | 0/8 | 2 |
| `http_reqline_step` | 22 | 8/8 | 0/8 | 1 |
| `utf8_step` | 9 | 8/8 | 0/8 | 4 |
| `telnet_step` | 5 | 8/8 | 0/8 | 1 |
| `slip_step` | 2 | 8/8 | 0/8 | 1 |
| `pin_step` | 13 | 8/8 | 0/8 | 3 |
| `debounce_step` | 7 | 8/8 | 8/8 | 7 |
| `vending_step` | 5 | 3/8 | 8/8 | 5 |
| `traffic_step` | 7 | 8/8 | 8/8 | 7 |
| `base64_step` | 4 | 8/8 | 0/8 | 2 |

**Black-box results.**
- The black box misses every machine whose transitions hinge on specific
  input values: TCP events, protocol keywords, UTF-8 lead bytes. It recovers
  the three whose inputs are small and dense.
- It also reports "consistent" on machines it got wrong: random-walk
  equivalence cannot see a keyword it never sampled. The white box takes its
  alphabet from the branch guards and its counterexamples from the solver.
- **Cost.** Median membership queries were 55 for the white box and 263 for
  the black box.
- **Caveat:** a black box given all 256 byte values would do better on these
  byte machines. This is not true of 32-bit inputs, where a magic value is one
  in 2^32.

## 5. Real binaries (DecBench, 300 of 728 step-shaped functions)

| status | O0 | O2-noinline | O2 | all |
|---|---|---|---|---|
| `proved_under_model` | 3 | 3 | 4 | 10 (3%) |
| `unsupported_semantics` | 107 | 104 | 79 | 290 (97%) |
| `partial_frontier` / `empirically_consistent` | 0 | 0 | 0 | 0 (0%) |

The 10 machines are single-state (the result is a function of the input
alone). Every replay agreed.

Why there is no machine (290):

| Reason | Count | Share |
|---|---|---|
| a call (callee effects are outside the slice) | 123 | 42% |
| a load or store through an address computed from data (pointer fields, heap) | 87 | 30% |
| absolute memory (globals) | 38 | 13% |
| a branch on a pointer comparison, such as an `if (ctx == NULL)` guard | 20 | 7% |
| a jump outside the function, such as a tail call | 15 | 5% |
| floating point | 4 | 1% |
| other (context bounds, wide values) | 3 | 1% |

## 6. Verdict against the RFC's P3 criteria

| Criterion (RFC section 11) | Result |
|---|---|
| State complexity cut by at least 50% | **Met on the corpus**: median nodes -80%, edges -59%. The cut holds against the pruned-CFG baseline (-80% / -56%), so the H0 "no gain over constant propagation + opaque removal" branch is rejected *for this baseline*. |
| At least 99% behaviour preservation, no spurious state | **Met**: 75/75 state counts equal the ground truth; 117,450/117,450 replayed words agree; every witness reproduces. |
| No false positive on negative cases | **Met**: 0 of 32 negative runs produced a machine. |
| Better counterexample discovery than black-box learning | **Met**: 7 of 10 machines are found only by the white box. |
| Stop: at least 60% of real binaries `PartialFrontier` (or `Unknown`) | **Not triggered**: 0%. But 97% of step-shaped real functions are **outside the model** (`unsupported_semantics`), and the 3% inside it are trivial. |

**Conclusion.** The mechanism does what the RFC claims, inside its model.
Flattening and opaque predicates do not change its answer on either ISA, and
every claim it made was right. The model, however, does not reach real code.
Calls, pointer-carrying contexts and globals account for 86% of the
rejections.

## 7. Recommendation for P4

**Redesign, not adopt or reject.** P4 should be an ADR that keeps the engine
experimental and extends the model in the order the measurement ranks it,
re-running this evaluation after each step:

1. **Calls (42%).** Stub them as `verify --tier ground-truth` already does:
   the call becomes an observable event with its arguments, and its result is
   a fresh input.
2. **Pointer fields and data-computed addresses (30%).** Give context fields
   of pointer type their own regions, with an explicit no-alias assumption
   reported in `assumptions`.
3. **Globals (13%).** Treat read-only data as constants from the image, and
   writable globals as more context.
4. **`ctx == NULL` guards (7%).** The context pointer is non-null by
   construction of the boundary; compare it with 0 accordingly.
5. **Tail calls (5%), 128-bit multiply-high, floating point.**

The stop rule for the redesign stays the RFC's. If, after (1)-(3), more than
60% of the step-shaped DecBench sample is still outside the model or
degenerate, pivot to the runner-up (AEQ).

## 8. Threats to validity

- **The corpus machines were written for this evaluation.** They follow
  published specifications and were not tuned against Fission's output. But
  they are step functions in the supported subset by construction, so the
  corpus tests the mechanism, not its reach. Section 5 measures the reach.
- **The obfuscator is an OLLVM-style reimplementation**: flattening and
  opaque predicates only, with no MBA, virtualization or self-modification.
  A stronger obfuscator could defeat the summary.
- **Proofs are relative to the memory model and the horizon reached.** The
  Moore-bound deepening makes the horizon `n + K_max`, so the minimality claim
  assumes the true machine has at most K_max states.
- **Baselines.** The CFG-structuring baseline is Fission's own HIR, measured as
  text. Ghidra, angr and dedicated deflattening tools were not run (scope
  decision). The pruned-CFG baseline is weaker than a real deflattener.
- **Ground truth inputs are bytes.** All corpus machines read a byte or a
  small value. The engine's own inputs are 32-bit, and guards and solver
  proofs cover the full width.
- **Single ISA for real binaries.** The DecBench sample is x86-64 ELF only,
  and part (c) draws on the scored DecBench binaries. Nothing was tuned on
  them.

## 9. Engine fixes found by this evaluation

Each one was a wrong or refused answer on a compiled corpus function, and was
fixed before the final run (see the PR):

- a branch into the middle of a block (`cmov`);
- a context slot wider than a value (overlapping 8-byte stores). The engine's
  own summary-vs-emulator check caught it and failed closed;
- `xor r, r` on a register holding a pointer;
- `refuted` reported while the control machine was still incomplete;
- a bounded H = 4 certificate merging states only a long word separates. The
  fix is Moore-bound deepening, which took TCP from 6 to 11 states and the
  HTTP request line from 10 to 22.

## 10. P4 step 1: calls are effects, not refusals

Date: 2026-11-23. Status: **measured**. The first P4 step widens the model
where section 5 ranked the most rejections: calls (42% there) and tail calls
(5%). Nothing in the decompiler changes; only `fission-quotient`, the
`quotient` command's text output, its schema and these scripts do.

### 10.1 The model change

A call inside the slice is an observable effect, and its callee is never run.
Both sides stub it, the way `verify --tier ground-truth` does:

- **The P-code summary** stops at the `CALL`. It records the call's target
  and its argument registers. Then it continues at the fall-through, with the
  registers as the callee leaves them:
  - the convention's preserved registers keep their values;
  - the stack pointer has the return address popped, when the convention
    pushed one;
  - the return register holds the next **call-return input port** `retN`;
  - every other register is poison.
- **The emulator oracle** uses `EmulatorHarness::call_stubbed` with the
  function's `fission_dir::call_sites`. It stops at each call instruction,
  records the argument registers, and puts the symbol's `retN` value in the
  return register.

A call's return is therefore an input the environment chooses. A symbol is
the boundary's ports followed by one value per call of the step, in dynamic
order. Guards over `retN` come from the same summaries as guards over the
input, and the solver's bounded-equivalence proof covers every value of it.
So the quotient holds for every value a callee could return.

What a step shows is now `(exit, result, calls)`, and each call is
`(target, observed arguments)`. Which argument registers are observed, and
how wide, is decided once per call site from the summary. Both sides then
read it through the same `CallModel`.

**Callees are told apart by facts, never guessed:**

| Callee | Fact source | Treatment |
|---|---|---|
| proven no-return | `fission_dir::call_sites` (static no-return analysis) | The step ends there with exit `no_return`, and the machine halts. Every later step observes `halted`, so the quotient has a sink state. |
| library | a prototype in the signature library (`fission-signatures`, resolved through `PathConfig`), by symbol or PLT name | Exactly its declared parameters are observed. Each must be defined by the boundary at least as wide as its type, when the type is informative. A variadic prototype counts as unknown. |
| unknown | neither | Every argument register the boundary defines is observed, at the widest defined low part (64/32/16/8 bits). |

**A jump out of the slice** that `call_sites` lists as a tail jump is a call
whose return is the step's return.

**Fail closed.** The summary stops with a reason when:
- an unknown callee gets an address of the context or the frame in any
  argument register, or a library callee gets one in a declared parameter
  (the callee may write through it);
- a call is indirect;
- the call sites cannot all be listed;
- a step makes more than 8 calls;
- the convention names no caller-saved registers.

A register the summary poisoned and the slice then reads also stops it, as
before. Every assumption the quotient now depends on is in its
`assumptions`:
- what a call returns is chosen by the environment;
- a callee writes no memory the slice reads, and holds no alias of the
  context of its own;
- which registers a callee preserves;
- the library prototypes used;
- that an unknown callee's arity is not known, and that stack arguments are
  not observed;
- which callees are no-return;
- which jumps are tail calls.

The `quotient.v1` schema gains:
- `calls`, the call sites and how they are observed;
- `transitions[].calls` and `transitions[].exit`;
- `calls` on each observation, and the exit `no_return`.

These are additions. Existing fields keep their meaning.

### 10.2 Corpus: call-using machines and their controls

There are five new machines in `scripts/quality/wbcq/corpus/call_machines.c`.
Each calls out in a different way:

| function | the call | callee class | ground truth |
|---|---|---|---|
| `lex_step` | a lexer that classifies each byte with `isdigit`, then `isalpha` (up to 2 calls per step) | library | 3 |
| `door_step` | a garage door controller that logs each change of motion with `putchar` | library | 5 |
| `turnstile_step` | a coin turnstile with an application logger, `wbcq_log`, that no library describes | unknown | 2 |
| `hs_step` | a greeting handshake that calls `abort()` on a NUL byte | no-return | 4 (3 + halted) |
| `emit_step` | a parity reporter whose result is what `putchar` returned (`jmp putchar` at -O1) | library, tail call | 2 |

**Ground truth.** It is computed without Fission. `ground_truth.py` runs the
natively compiled source against `call_env_native.c`, which stands in for
every callee: it records the call (which function, which argument) and
returns a scripted value. `abort` records itself and unwinds out of the step.
- A symbol is an input byte plus one return value per call, drawn from
  {0, 1, 0xffffffff}.
- An observation is the result (or "did not return") plus the call trace.
- After a step that does not return, the machine is halted.
- Moore refinement then gives the minimal machine, as in section 2.

This is the semantics the engine uses: a callee's return is an input. The
ground truth observes the *declared* arguments. For the unknown callee, the
engine observes every defined argument register, so a match there also tests
that the over-observation did not split states.

**Positive controls.** Before this change, all 40 builds (5 machines x 8
builds) were `unsupported_semantics`:
- 34 were rejected as "a call: the callee's effects are outside the slice";
- 6 `emit_step` builds were rejected as "a branch outside the slice": the tail
  `jmp putchar`. At -O0 and on the flattened x86 builds the tail call is a
  plain call.

The control runs are kept in `~/fission-benchmark/wbcq-p4/controls-before/`.

**Unit fixtures** (`testdata/quotient_fixture.S`, assembled):
- `digits_step`: a classifier call with an environment-chosen return;
- `guarded_step`: `abort()` halts the machine;
- `tail_step`: `jmp isdigit`;
- `padded_select_step`: a `cmov` followed by a `nop`;
- `calling_step`, which now fails closed because an unknown callee is handed
  the context pointer.

### 10.3 The full P3 evaluation, re-run

This is the whole of section 1, re-run on the corpus with the five call
machines added: 20 functions x 8 builds = 160 runs. The binaries were rebuilt
from the same sources, and `call_env.c` is now linked in.

```bash
python3 scripts/quality/wbcq/build_corpus.py --out ~/fission-benchmark/wbcq-p4/corpus
python3 scripts/quality/wbcq/ground_truth.py ~/fission-benchmark/wbcq-p4/corpus/manifest.json \
  --out ~/fission-benchmark/wbcq-p4/ground_truth.json
cargo build --profile quick-release -p fission-quotient --example p3_eval
cargo build --profile quick-release -p fission-cli --bin fission_cli
python3 scripts/quality/wbcq/evaluate.py run --corpus ~/fission-benchmark/wbcq-p4/corpus \
  --out ~/fission-benchmark/wbcq-p4/results --eval <p3_eval> --cli <fission_cli> -j 2
python3 scripts/quality/wbcq/evaluate.py summarize --out ~/fission-benchmark/wbcq-p4/results \
  --truth ~/fission-benchmark/wbcq-p4/ground_truth.json
```

| function | truth | a64 O0 | a64 O1 | a64 fla | a64 bcf+fla | x64 O0 | x64 O1 | x64 fla | x64 bcf+fla |
|---|---|---|---|---|---|---|---|---|---|
| `lex_step` | 3 | P 3= | U* | U* | U* | P 3= | P 3= | P 3= | P 3= |
| `door_step` | 5 | P 5= | P 5= | P 5= | P 5= | P 5= | P 5= | P 5= | P 5= |
| `turnstile_step` | 2 | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= |
| `hs_step` | 4 | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= |
| `emit_step` | 2 | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= | P 2= |
| `sha256_round_step` (negative) | - | **F** (was U) | F | F | F | U | F | F | F |

The other 14 functions have the same row as in section 2.

| | P3 (#1102) | P4 step 1 |
|---|---|---|
| Machines equal to the ground truth | 75/75 | **112/112** (the 75 plus 37 of the 40 call-machine builds) |
| P3's 120 runs: status, states, transitions, witnesses, replay | - | **119 identical**. The one difference is a negative: `sha256_round_step` a64 O0 was refused at its call to `rotr` and now runs into the state budget (33 > K_max). It is still not a machine. |
| Witnesses re-run on the emulator | all | all (0 unverified among 112) |
| Random replay (200 words of length 24 per machine) | 15,000/15,000 | **22,400/22,400** |
| W-method conformance | 102,450/102,450 | **103,833/103,833** |
| Negatives producing a machine | 0/32 | **0/32**. x64 O0 `sha256` now fails closed because the unknown callee `rotr` finds a context address left in `rdx`. |
| Median node / edge reduction vs the P-code CFG | -80% / -59% | -80% / -62% |

\* **The three `U` cells for AArch64 `lex_step` are an emulator bug, caught by
the engine.**
- The engine refuses these builds because the P-code summary and the
  emulator disagree: "the P-code summary predicts control state [2] where the
  emulator left [0]".
- The emulator is wrong. The same disagreement reproduces without any stub,
  in plain `EmulatorHarness::call`.
- It also reproduces in a five-instruction function with no calls:
  `cmp w1,#2; mov w8,#2; csel w10,wzr,w8,eq; mov w0,w8; ret` returns 0 under
  `fission_cli call`, where hardware returns 2.
- The raw P-code is right (it never writes `x8`), so the summary is right.
- The repro is in `~/fission-benchmark/wbcq-p4/emulator-csel-repro/`. The
  owner is `fission-emulator`, which this step does not touch.

This is the summary-vs-emulator check doing its job: a disagreement is
refused, never learned. All 37 call-machine builds that are not affected
equal the ground truth.
- The library, no-return and tail-call paths are each exercised on both
  ISAs, at every optimisation level and with every obfuscation.
- The unknown-callee machine, `turnstile_step`, observes extra defined
  registers (`rsi`, and on the flattened x64 build `rcx`). It still equals the
  ground truth, which observes only the declared argument.

**Black box.** It is given the same port count, with sampled return values. It
finds `lex_step`, `turnstile_step` and `emit_step`, finds `door_step` 7 times
out of 8, and misses `hs_step` 8 times out of 8. That machine's halt sits
behind one input byte (`'H'`, then NUL).

### 10.4 Real-binary reach (dir-verify GNU programs)

**Policy deviation, recorded.** Section 5 ran the engine, and with it the
emulator, on DecBench binaries. That deviates from the policy that DecBench
and evalkit binaries are not emulated, and it is not repeated here. P4 runs
on the GNU programs in `~/fission-benchmark/dir-verify/binaries` instead.

But those are not independent of DecBench. 15 of the 16 are stripped
byte-identical copies of DecBench builds: the `.text` has the same address,
size and SHA-256. `unopt_*` are DecBench `O0` and `inlin_*` are DecBench `O2`.
`inlin_dash_dash.elf` has no match and is skipped. So emulating them is, in
substance, the same deviation. It is flagged here rather than hidden.
Nothing was tuned on them.

**Selection.** The binaries are stripped, so `select_dir_verify.py` reads the
DWARF of the byte-identical unstripped build. It uses P3's signature rule,
`<int> f(<struct> *, <int>)`, and does not filter by body. That gives 186
step-shaped functions in 15 binaries, all of them kept (no sampling). The
same 186 were run before (#1102's engine) and after this change, with the same
flags.

| outcome | before | after |
|---|---|---|
| `proved_under_model` | 8 (4%) | **10 (5%)** |
| data-computed address | 78 (42%) | 94 (51%) |
| globals (absolute memory) | 20 (11%) | 26 (14%) |
| a call (refused outright) | 56 (30%) | 0 |
| call: an unknown callee is given the context (`helper(ctx, ...)`) | - | 16 (9%) |
| call: a library callee is given an address of the frame (`setsockopt`) | - | 2 (1%) |
| call: indirect | - | 1 (1%) |
| call: more than 8 calls in one step | - | 1 (1%) |
| floating point | 7 (4%) | 15 (8%) |
| branch on a pointer (`ctx == NULL`) | 12 (6%) | 12 (6%) |
| wide values (> 8 bytes) | 4 (2%) | 6 (3%) |
| a jump out of the function | 1 (1%) | 0 |
| harness timeout (600 s) | 0 | 3 (2%) |

Where the 56 call rejections went:
- 16 to a data-computed address;
- 15 to an unknown callee given the context (one more came from the former
  out-of-function jump, a tail call to a helper with `ctx`);
- 8 to floating point;
- 6 to globals;
- 3 to the harness timeout (`tar_checksum` at both levels and
  `check_identifier` -O0; they now get past the call and then run out of the
  600 s budget);
- 2 to a frame address given to a library callee;
- 2 to wide values;
- 1 to an indirect call;
- 1 to the call budget;
- **2 became machines.**

No function was lost.

**Newly supported:**

| function | build | states / transitions | ports | calls | witnesses | replay | conformance |
|---|---|---|---|---|---|---|---|
| `re_node_set_alloc` (tar, gnulib regex) | O0 | 1 / 2 | 2 | `malloc` (library) | none needed (1 state) | 100/100 | 2/2 |
| `re_string_realloc_buffers` (tar, gnulib regex) | O2 | 1 / 1 | 4 | 3 x `realloc` (library) | none needed | 100/100 | 5/5 |

**Spot checks against the disassembly** (only two functions are newly
supported, so there are two):

- **`re_node_set_alloc`** (`unopt_tar_tar.elf` 0x65350). The machine code:
  - stores `size` to `set->alloc` and 0 to `set->nelem`;
  - calls `malloc(size << 3)` and stores the result to `set->elems`;
  - returns 12 (`REG_ESPACE`) when that is NULL, and 0 otherwise.

  The quotient has one state and two transitions, both
  `malloc((in << 3))`. One returns `0xc` when `ret0 == 0`, the other returns 0.
  Both update `ctx[0x0] := in`, `ctx[0x8] := 0` and `ctx[0x10] := ret0` as
  data registers. **Exact.**
- **`re_string_realloc_buffers`** (`inlin_tar_tar.elf` 0x50530). Two
  context fields guard the three `realloc` calls:
  - `cmpl $1, 0x90(%rdi); jle` (`mb_cur_max > 1`);
  - `cmpb $0, 0x8b(%rbx); jne`.

  The step never writes either field, so from the zeroed context no call is
  ever reached. The step only stores `in` to `0x40` (`bufs_len`) and returns
  0. The quotient is exactly that: one state, `if true / returns 0;
  ctx[0x40:8] := in`. **Correct, but degenerate.** Modelling calls is what
  let the summary run at all. The machine itself never exercises a call,
  because the boundary's initial context (zeroed) cannot reach one.

Both new machines are single-state, like all 8 earlier ones.

**What the 16 `helper(ctx, ...)` rejections are.** These are not leftovers
of unoptimised register use. Each passes the context pointer itself to an
in-binary helper in argument register 0, for example:
- `argp_fmtstream_set_{l,r,w}margin` → `__argp_fmtstream_update(fs)`;
- `rtnl_open` → `rtnl_open_byproto(rth, ...)`;
- `tuntap_filter_req` → `addattr_nest(nlh, ...)`.

The callee may write the context, so this is fail-closed by design.

### 10.5 Engine fix found by this step

A branch to an instruction without P-code of its own (a `nop`) was taken for
a jump out of the slice. The flattened x64 `door_step` places a `nop` right
after a `cmov`, and x86 lifts `cmov` as a branch to the next instruction. The
summary now follows such a branch to the next decoded instruction that has
P-code, using the instruction list `call_sites` decodes. The fixture is
`padded_select_step`.

### 10.6 Outcome and the next step

**The model change works, and the reach barely moves.**
- Calls, tail calls and no-return calls are now effects. On the corpus every
  answer is still right:
  - 112/112 machines equal the ground truth;
  - 126,233/126,233 replayed words agree (22,400 random plus 103,833 W-method);
  - no negative produces a machine.
- On the 186 step-shaped dir-verify functions, machines go from 8 (4%) to
  **10 (5%)**. Both new ones are single-state, and one is degenerate (its calls
  are unreachable from a zeroed context). 95% are still outside the model.

Calls were the *first* refusal, not the only one. Of the 56 functions they
blocked, 54 had another blocker behind them:
- 16 a data-computed address;
- 17 a context or frame address handed to a callee (15 unknown helpers, 2
  library);
- 8 floating point;
- 6 globals;
- 3 run out of time;
- 2 wide values;
- 2 more call limits (1 indirect call, 1 step over the call budget).

**Recommended next step: P4 step 2, data-computed addresses.** That means
pointer fields in the context as their own regions, under a recorded no-alias
assumption. It is now the first refusal for **94 of 186 (51%)**, up from 42%,
because it was hiding behind calls.

The second largest call-related remainder is **interprocedural**. 16 functions
(9%) hand the context itself to an in-binary helper (`helper(ctx, ...)`). The
fix is to compose the helper's own summary (its effect on the context) rather
than stub it. This is a natural step 2b, because it reuses this step's call
plumbing.

**Stop rule.** It is the RFC's: if after steps (1)-(3) more than 60% of the
step-shaped sample is still outside the model or degenerate, pivot to AEQ.
After step (1) that figure is 95%. Step 2 has to move the 51% bucket a lot for
the redesign to continue.

## 11. P4 step 2: data-computed addresses

Date: 2026-11-24. Status: **measured**. The second P4 step widens the model
where section 10.4 ranked the most first refusals: a load or store through
an address computed from data (94 of 186, 51%). As before, only
`fission-quotient`, the `quotient` command, its schema and these scripts
change; nothing in the decompiler does.

### 11.1 What the 94 were

Before changing anything, each of the 94 refusal sites was read in the
disassembly. **Half of the bucket was not a data pointer at all.**

| instruction at the refusal | count |
|---|---|
| `mov rax, qword ptr fs:[0x28]`: the stack protector reading its canary from thread-local storage | 47 |
| a load or store through a pointer the function computed (a context field, a call's result, a field of a field) | 47 |

The canary load's address is the thread pointer (`FS_OFFSET`, a register the
caller set up) plus a constant. Step 1 reported it with the same words as a
pointer field. This step gives it its own reason (11.2), and leaves it
unmodelled (11.6).

### 11.2 The model change

**Pointer fields are regions of their own.** A pointer the slice reads out of
the context and dereferences is a memory region with a symbolic base:
- The model never knows its address, only offsets within it, the same way it
  treats the context.
- Its bytes are hidden state like the context's. The context and every
  region are laid out in one byte space (`QuotientBoundary::layout`), so a
  slot of a region is a slot like any other: `ctx[0x8]->[0x10:1]`.
- The emulator oracle maps the context and the regions as one buffer and
  writes each region's real address into its pointer field.

**Where regions come from.** Either:
- the boundary declares them (`--region FIELD=HEX` or `FIELD:SIZE`), or
- the summary discovers them. A load or store whose address is a pointer
  read straight out of memory (one slot of pointer width, unchanged in the
  step) plus an offset names that field. The field gets a zeroed region and
  the summary runs again, until no new field turns up.

A discovered region starts at 64 bytes. It grows, up to 4096, to cover:
- every constant offset the slice reads;
- every place a bounded index can reach (the solver's maximum of the index
  under the path).

A pointer read out of a region is a nested region, at most 2 deep. At most 4
regions are discovered.

**Indexed access** (`buf[i]`, `i` computed from data) is an array access
within the region:
- the path splits where the index would leave the region;
- inside, the solver gives the range and stride of the places the index can
  take;
- a read is the value at each place, chosen by the index; a write updates
  each place unless the index chose another;
- at most 512 places per access.

The side of the split that leaves the region ends in `LeftRegion`. That is
not behaviour of the machine; the model has nothing to say about it. If any
input takes that side from a state the learner reached, the quotient is
`unsupported`, and the certificate's unrolling sees it as a difference from
every automaton. So `buf[head]` with an unmasked head is fine as long as no
reachable state lets the head out.

**A region's pointer is not null**: the boundary gives every region an
allocation. A null check on it folds; nothing else about its value is
known.

**Fail closed.** Each of these stops the summary with its reason:
- a store over a region's pointer field (the region would move);
- an address stored to memory (an alias the model cannot follow), such as
  `ctx->a = ctx->b`;
- an indexed read of a pointer (which region it names is data);
- a comparison between the addresses of two regions (whether they alias is
  what the model assumes, not what it knows);
- a region's pointer passed to a callee (step 1's rule, now naming the
  region);
- a region more pointers deep, or more regions, than the budget;
- a constant access outside a region's bytes;
- an indexed access with more places than the budget;
- an access at a constant offset from a register the caller set up
  (thread-local storage). This last one is a new reason, not a new
  refusal.

**Assumptions**, in `assumptions` whenever there is a region:
- each named pointer points to a region of its own that aliases nothing
  else: not the context, not the frame, not another region, and no callee
  holds an alias of it; the pointers are not null and never change;
- each region is exactly as long as its bytes, listed per region;
- a discovered region's bytes start zeroed, so the quotient holds for that
  initial memory only.

The certificate's `model_id` is then
`separated-registers-frame-context-regions.v1`; a quotient without regions
keeps `separated-registers-frame-context.v1`.

**Schema** (`quotient.v1`, additions only): `boundary.regions` (parent,
field, initial bytes, discovered), `slots[].region` (`offset` is then within
the region), and five budget fields. Transition text renders an indexed read
of a table as `lookup(key; 0x2e: 2, 0x30-0x39: 1; else 0)`.

**Engine fix found by this step.** A cell (a guard once the control state is
fixed) that the solver proves empty was refused ("a cell the guard folding
kept has no input"). Folding cannot always see such a guard is false: a table
read through a region folds to a lookup, not to a constant. The cell has no
input and adds no transition, so it is now skipped. The `-O0` keyword matcher
found it; no earlier run hit it.

### 11.3 Setup: step 1 re-run on main's emulator fixes

#1107's branch was merged with main (merge commit, no rebase). This brought
in the emulator fixes #1109-#1113, including #1109, the AArch64 `csel`
miscompile in the JIT that section 10.3 caught. The whole of 10.3 and 10.4
was then re-run with the merged engine before anything in this step
changed.

| | P4 step 1 (#1107) | step 1 on merged main |
|---|---|---|
| corpus runs (160) identical in status, states, transitions, witnesses, replay and conformance | - | **157** |
| AArch64 `lex_step` O1 / O1-fla / O1-bcf-fla | `U` (summary and emulator disagree) | **`P 3=`** each |
| machines equal to the ground truth | 112/112 | **115/115** |
| random replay | 22,400/22,400 | 23,000/23,000 |
| W-method conformance | 103,833/103,833 | 103,914/103,914 |
| negatives producing a machine | 0/32 | 0/32 |
| dir-verify functions identical (status, reason, states, replay) | - | **185/186** |
| dir-verify machines | 10 | 11 |

- **The three `lex_step` builds now match.** They have 3 states and 9
  transitions, equal to the ground truth. Replay agrees 200/200 and
  conformance 27/27 on each.
- **The one dir-verify difference is timing, not the model.**
  `check_identifier` (bash, -O0) hit the 600 s harness timeout under step 1.
  It now finishes in 588 s and is proved: 1 state, 4 transitions, replay
  100/100. It sits at the edge of the budget.

### 11.4 Corpus: machines behind a context pointer, and their controls

Three new machines are in `scripts/quality/wbcq/corpus/region_machines.c`.
Their tables come from `region_tables.py`. The engine gets them with
`--region FIELD=HEX`, and the ground truth gets the same bytes in buffers of
its own.

| function | through the pointer | index | ground truth |
|---|---|---|---|
| `tdfa_step` | a number lexer: a 256-byte class table at `ctx->cls` and a 5x4 transition table at `ctx->delta` (accept bit in bit 7) | the input byte; then `state * 4 + class` | 5 |
| `kw_step` | the keyword `HELO` at `ctx->word`, read at the match position | the machine's own state | 4 |
| `ring_step` | a 4-cell ring of bits at `ctx->ring`; each step returns the bit from four steps ago and writes the new one | the head, unmasked | 16 |

**Ground truth.** `ground_truth.py` explores each one natively. It uses all
256 input bytes, with the context and every buffer as the state, and Moore
refinement gives the minimal machine, as in section 2. It also checks that
no step moves a pointer.

**Positive controls.** The merged engine from 11.3, given no regions, ran on
all 24 builds (3 machines x 8). All 24 were `unsupported_semantics`: "a load
through an address computed from data". The runs are in
`~/fission-benchmark/wbcq-p4s2/controls-before/`.

**The full evaluation**: 23 functions x 8 builds = 184 runs, with the corpus
rebuilt from the same sources plus `region_machines.c`.

| function | truth | a64 O0 | a64 O1 | a64 fla | a64 bcf+fla | x64 O0 | x64 O1 | x64 fla | x64 bcf+fla |
|---|---|---|---|---|---|---|---|---|---|
| `tdfa_step` | 5 | P 5= | U* | U* | U* | P 5= | P 5= | P 5= | P 5= |
| `kw_step` | 4 | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= | P 4= |
| `ring_step` | 16 | P 16= | P 16= | P 16= | P 16= | P 16= | P 16= | P 16= | P 16= |
| `lex_step` | 3 | P 3= | P 3= | P 3= | P 3= | P 3= | P 3= | P 3= | P 3= |

The other 19 functions keep their rows.

| | step 1 on merged main | P4 step 2 |
|---|---|---|
| the 160 earlier runs: status, states, transitions, witnesses, replay, conformance | - | **160/160 identical** |
| machines equal to the ground truth | 115/115 | **136/136** (21 of the 24 new builds) |
| witnesses re-run on the emulator | all | all |
| random replay (200 words of length 24) | 23,000/23,000 | **27,200/27,200** (new machines: 4,200/4,200) |
| W-method conformance | 103,914/103,914 | **106,334/106,334** (new machines: 2,420/2,420) |
| negatives producing a machine | 0/32 | **0/32** |
| median node / edge reduction vs the P-code CFG | -80% / -62% | -77% / -51% |

\* **The three `U` cells fail closed on folding precision, not on the region
model.**
- The AArch64 -O1 builds index the class table with `and x10, x1, #0xff`, a
  64-bit mask of the 32-bit argument.
- The value is the input's low byte. But the folder keeps the mask over
  `concat(x1's undefined upper half, in)` rather than narrowing it. So the
  index still names the upper half, and the summary refuses: "an index
  depends on values outside the boundary: X1_upper_bits".
- The fix is a folding rule in `expr.rs`: a constant mask that clears a
  concatenation's high part narrows to the low part. It is left for a
  follow-up, because it touches every function and would need this whole
  evaluation re-run.

**Reductions.** The median reductions dropped because `tdfa_step` and
`ring_step` are one-block functions whose state lives in tables. Section 3
noted the same for counters: the CFG does not show that state, so the
"reduction" is negative, and it is reported as measured.

**Black box.** It misses `tdfa_step` and `kw_step` 8 of 8 times (median 1
state), and finds `ring_step` 8 of 8. The ring's input is one bit.

**Unit fixtures** (`testdata/quotient_fixture.S`):
- `table_step`: a declared class table, 3 states; the null check on the
  pointer folds.
- the same function undeclared: the region is discovered, zeroed and grown
  to 256 bytes, giving 1 state.
- `ring_step`: an indexed write at an unmasked head, 16 states.
- `nested_step`: two pointers deep; the depth budget refuses it at depth 1.
- Fail-closed fixtures, each with its reason:
  - `escape_step`: an unbounded index;
  - `retarget_step`: `ctx->a = ctx->b`;
  - `alias_step`: comparing two regions' addresses;
  - `tls_step`: thread-local storage.
- CLI tests: `--region`, schema conformance, and the text output.

### 11.5 Real-binary reach (dir-verify GNU programs)

The same 186 functions and flags as 10.4. The policy deviation recorded in
10.4 applies unchanged: 15 of the 16 binaries are stripped byte-identical
copies of DecBench builds, so this emulates DecBench code. Nothing was
tuned on them. "Before" is step 1 on merged main (11.3).

| first refusal | before | after |
|---|---|---|
| `proved_under_model` | 11 (6%) | **12 (6%)** |
| data-computed address | 94 (51%) | 16 (9%) |
| thread-local storage (stack protector canary), named in this step | (inside the 94) | 47 (25%) |
| globals (absolute memory) | 26 (14%) | 29 (16%) |
| floating point | 15 (8%) | 27 (15%) |
| call: unknown callee given an address of the context or frame | 16 (9%) | 18 (10%) |
| branch on a pointer (`ctx == NULL`) | 12 (6%) | 13 (7%) |
| wide values (> 8 bytes) | 6 (3%) | 6 (3%) |
| harness timeout (600 s) | 2 (1%) | 5 (3%) |
| budget (paths; region retries) | 0 | 4 (2%) |
| call: indirect | 1 (1%) | 3 (2%) |
| call: library callee given an address of the frame | 2 (1%) | 2 (1%) |
| call: more calls per step than ports | 1 (1%) | 1 (1%) |
| region: a pointer field overwritten | - | 1 (1%) |
| region: an index may leave its region | - | 1 (1%) |
| region: an indexed access with too many places | - | 1 (1%) |

**Where the 47 pointer-like refusals of 11.1 went:**
- 1 became a machine;
- 12 to floating point;
- 4 to the path or region-retry budget;
- 4 to the harness timeout (loops over a region array bounded by a context
  field: `search_cur_bkref_entry` x2, `re_node_set_contains`,
  `calc_state_hash`);
- 3 to globals;
- 2 to indirect calls;
- 1 to a context address given to a callee;
- 1 to a pointer branch;
- 3 to the new region refusals;
- 16 still refuse at a data-computed address. None of these pointers is held
  by a context field. Each is a call's result (`malloc`, `realloc`,
  `strcpy`, `__errno_location`), or a pointer the step loaded and then
  advanced or indexed (`*fs->p++`, an array of pointers).

No function was lost.

**All 27 floating-point refusals are one compiler artefact.** Every one is
an `fldz` in the `-fzero-call-used-regs` epilogue that clears the x87 stack
before `ret` (`fldz` x8, `fstp st(0)` x8). That code is dead to every
observation; the summary refuses it only because it refuses an unsupported
operation where it appears, not where its value is used.

**Newly supported:**

| function | build | states / transitions | ports | calls | regions | replay | conformance |
|---|---|---|---|---|---|---|---|
| `sshpkt_put_u64` (openssh) | O2 | 1 / 1 | 2 | tail call to `0x5d340` (unknown) | `ctx[0x0]`, discovered | 100/100 | 1/1 |

**Spot checks against the disassembly.** Only one function is newly
supported, so the other two checks are new region refusals.

- **`sshpkt_put_u64`** (`inlin_openssh-portable_sshd.elf` 0x81ea0):
  `mov rax, [rdi]; mov rdi, [rax+0x30]; jmp 0x5d340`. That is
  `sshbuf_put_u64(ssh->state->outgoing_packet, val)` as a tail call.
  - The field at 0 is discovered as a region because the step dereferences
    it.
  - The quotient is one state: `0 -> 0 if true / calls 0x5d340(0, in);
    returns ret0`.
  - The first argument is 0 because the discovered region starts zeroed.
  - **Exact, and degenerate.**
- **`re_string_wchar_at`** (`unopt_tar_tar.elf` 0x62911):
  `if (pstr->mb_cur_max == 1) return pstr->mbs[idx]; return pstr->wcs[idx];`
  - From a zeroed context the step takes the `wcs` side.
  - The index is the input itself, scaled by 4. Input 16 already reaches
    past the discovered 64-byte region.
  - Refused: "an index leaves its pointer region's bytes (state 0, input
    [16])".
  - **Correct.** The real bound is the string's length, a contract of the
    caller that the function never checks.
- **`re_node_set_insert_last`** (`unopt_tar_tar.elf` 0x662b9): when
  `alloc == nelem` it reallocates `set->elems` and stores the new pointer at
  0x6633c (`mov [rax+0x10], rdx`).
  - Refused: "a store over the pointer to the region ctx[0x10]: the region
    would move".
  - **Correct.** The region moves. The `realloc(set->elems, ...)` call before
    it would also be refused, because it hands a region's address to a
    callee.

### 11.6 What this step does not model, and why

**Thread-local storage (47 functions, 25%).** It is now named rather than
counted as a data-computed address.

Modelling it was tried and taken out, because it can never produce a
machine with the current harness:
- The emulator's call harness maps no thread-local block. A fixture's
  `fs:[0x28]` load faults there, and the summary and emulator then disagree.
- The canary check's reload follows the step's calls. A call leaves every
  register the convention does not name as poison, and the thread pointer
  is one of them.

The fix has two parts, both outside this step's crates:
- a thread-local block in `fission-dir`'s harness;
- the thread pointer as a callee-preserved register, a calling-convention
  fact.

**Step 2b, composing in-binary helpers (`helper(ctx, ...)`, 18 functions):
not done.** It does not compose naturally with step 1's plumbing:
- The emulator side stubs exactly the call sites `fission_dir::call_sites`
  lists for the entry function. Running a helper instead of stubbing it
  would run that helper's own calls unstubbed (into unresolved PLT stubs).
  That needs per-callee stub sets in `fission-dir`.
- The summary side would need the helper's P-code lifted and executed in
  the caller's frame.

It is left to a later step.

**Also seen:**
- Two functions exhausted the 64 summary retries while their regions grew
  ("the pointer regions did not settle"). Each growth jumps only to the
  offset that asked for it.
- Five functions hit the 600 s harness timeout. Three of them are new: once
  the region was modelled, their array loops became reachable.

### 11.7 Outcome and the recommendation

**The model change works, and the reach barely moves again.**
- On the corpus every answer is still right:
  - 136/136 machines equal the native ground truth, with no earlier run
    changed;
  - 133,534/133,534 replayed words agree;
  - no negative produces a machine;
  - every region refusal on a fixture is the one intended.
- On the 186 dir-verify functions, machines go from 11 to **12 (6%)**. The
  new one is single-state and degenerate, like all 11 before it.
- The "data-computed address" bucket shrank from 94 to 16. But it did not
  turn into machines:
  - half of it was never a pointer field (thread-local storage);
  - of the 47 that were, 46 had another blocker behind them, as calls did in
    step 1.

**The first refusals are now dominated by things that are not the slice's
semantics.**
- The stack protector's canary: 47 (25%).
- The `-fzero-call-used-regs` epilogue: 27 (15%).
- Together they are the first refusal for 74 of 186 (40%). Both are compiler
  hardening that no observation can see.
- Globals are 29 (16%) and helpers given the context are 18 (10%).

**Against the RFC's stop rule** (pivot to AEQ if, after steps 1-3, more than
60% of the sample is outside the model or degenerate):
- After step 2 the figure is **94% outside the model, and 100% of the
  machines are single-state.**
- Each of the last two steps removed its bucket and converted about 1% into
  machines. In step 1, 54 of 56 had a second blocker; here 46 of 47 did.
- Step 3 (globals, 16%) cannot bring the outside share under 60% even if
  every global function became a machine.

**Recommendation.**
1. Run step 3 as a bounded, last measurement. Widen it to globals plus the
   two hardening artefacts, because they are cheap and block 40% outright:
   - the dead `fldz` epilogue: an unsupported operation's result becomes
     poison, refused only where it is observed;
   - the canary: a thread-local block in the harness, and the thread pointer
     as a preserved register.

   Without them, the stop rule would judge the model on the compiler's
   hardening rather than on its semantics.
2. Do not build step 2b first. Its 18 functions would meet the same second
   blockers.
3. Unless step 3 moves real machines (more than single-state) into double
   digits, **pivot to AEQ** as the RFC's stop rule says. On current evidence,
   that is the expected outcome.

Reproduce (after section 10's corpus build; about 3 h on a loaded 14-core
host):

```bash
python3 scripts/quality/wbcq/build_corpus.py --out ~/fission-benchmark/wbcq-p4s2/corpus
python3 scripts/quality/wbcq/ground_truth.py ~/fission-benchmark/wbcq-p4s2/corpus/manifest.json \
  --out ~/fission-benchmark/wbcq-p4s2/ground_truth.json
python3 scripts/quality/wbcq/evaluate.py run --corpus ~/fission-benchmark/wbcq-p4s2/corpus \
  --out ~/fission-benchmark/wbcq-p4s2/results --eval <p3_eval> --cli <fission_cli> -j 3
# positive controls: the engine before this step, no regions declared
python3 scripts/quality/wbcq/evaluate.py run --corpus ~/fission-benchmark/wbcq-p4s2/corpus \
  --out ~/fission-benchmark/wbcq-p4s2/controls-before --eval <old p3_eval> --cli <old fission_cli> \
  --only tdfa_step kw_step ring_step --no-regions
python3 scripts/quality/wbcq/evaluate.py decbench \
  --candidates ~/fission-benchmark/wbcq-p4/dir_verify_candidates.json \
  --out ~/fission-benchmark/wbcq-p4s2/dirverify-step2 --eval <p3_eval> -j 2
python3 scripts/quality/wbcq/evaluate.py compare-reach \
  --before ~/fission-benchmark/wbcq-p4/dirverify-merged --after ~/fission-benchmark/wbcq-p4s2/dirverify-step2
```
