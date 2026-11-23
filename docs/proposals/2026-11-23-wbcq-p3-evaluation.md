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
