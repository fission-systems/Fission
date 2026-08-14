# Post-finalize terminal-tail duplication

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_146.elf`
- Function: `sub_fad0`
- Address: `0xfad0`
- Command: release `fission_cli decomp` in both `nir` and `hir`, followed by
  the 224-binary / 250-function DecBench sample-set runner.
- Current NIR: 31 gotos, 5,880 bytes, 241 lines.
- Current HIR: 31 gotos, 5,627 bytes, 234 lines.
- Semantic cases: not available for this DecBench readability row; semantic
  safety is covered by the existing terminal-tail proof, focused invariant
  tests, and the corpus execution differential.
- Failure category: cleanup ordering leaves a short return tail undiscoverable
  until the finalizer removes the residual transfers and labels around it.

Measured owner trace:

```text
before finalization:
  block_fb00 refs=8, next shapes=[stmt, stmt, stmt, label]
  terminal-tail candidate: no

after finalization:
  block_fb00 refs=7, region=[assign, call, call, return]
  terminal-tail candidate: yes
```

A temporary second bounded duplication pass changes the same real function to
23 gotos in both layers. It grows NIR to 6,727 bytes / 262 lines and HIR to
6,442 bytes / 255 lines, so aggregate measurement and per-file regression
inspection remain required before any readability claim.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring cleanup
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

The existing `duplicate_terminal_tails` owner already proves that replacing a
goto by a bounded, label-free tail ending in `Return` is semantics-preserving.
The missed tail is not admissible at its first call because it is not terminal
yet. `finalize_structured_body` then removes redundant control transfers and
labels, exposing exactly the shape the existing owner accepts. The defect is
therefore orchestration order/fixpoint completeness, not a missing CFG fact or
a printer concern.

## 3. Generality / Invariant Proof

Generalized rule:

```text
After layout and finalization change label references or expose a short
terminal return region, rerun the existing bounded terminal-tail duplicator
once and finalize once more. Admit only tails that pass the unchanged
terminality, label uniqueness, loop-control, protected-label, reference-count,
and growth proofs.
```

- [x] No binary, function, address, compiler, mnemonic, or ISA predicate is
  used in production.
- [x] The rule is ISA-agnostic and works only on structured AST control facts.
- Comparable shape 1: finalization removes a residual jump and exposes a
  shared return tail.
- Comparable shape 2: a non-terminal or protected region remains rejected on
  the second pass exactly as on the first.
- Synthetic invariant test: a pre-final body whose first duplication declines,
  whose finalization exposes a terminal tail, and whose second duplication
  removes the goto.

## 4. Risk And Ownership Check

- Existing owner: `fission-midend-structuring::cleanup::duplicate_terminal_tails`.
- Shared substrate: none; the required facts already exist in the cleanup AST.
- New pass/helper/metric: none. This is one additional invocation of the
  existing bounded, deterministic, builder-free rewrite. The two invocations
  each retain the existing 160-statement copy budget, so the orchestration-level
  worst-case ceiling becomes 320 copied statements; aggregate line/byte growth
  is therefore an explicit go/stop measurement.
- Interaction: the second finalization is required to remove labels made dead
  by the second duplication. Both cleanup operations are already idempotent.
- Owner dependency: none; orchestration already depends on both functions.
- Telemetry: unchanged.
- Must not change: non-terminal regions, regions with nested labels/gotos,
  `Break`/`Continue`, ambiguous label definitions, LSDA landing pads, and tails
  beyond the existing per-invocation 6-statement / 8-reference /
  160-copied-statement bounds.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-structuring post_finalize_terminal_tail`
  - Expected: first pass declines, finalization exposes the tail, second pass
    removes the goto; protected/non-terminal controls remain unchanged.
- [x] Crate gates:
  - Command: `cargo nextest run -p fission-midend-structuring -p fission-pcode`
  - Expected: no new failures.
- [x] Focused benchmark row:
  - Command: release NIR/HIR decomp of `bin_146.elf` at `0xfad0`.
  - Expected: 31 to 23 gotos in both layers, with identical function signature
    and preserved ordered error-handler statements.
- [x] Full sample set:
  - Command: `run_fission.py`, NIR and HIR, 224 binaries / 250 functions.
  - Expected: lower aggregate goto count and no per-file goto regressions;
    inspect byte/line growth separately.
- [x] Ground truth / smoke:
  - Command: phase-2 corpus execution differential and external local Docker
    benchmark.
  - Expected: zero semantic divergence and adapter-valid output.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected: pass.

## 6. AI Review / Prompt Firewall

- [x] No external model was asked for implementation advice.
- [x] Reference decompilers are used only as cleanroom algorithm controls.
- [x] Production code contains no benchmark identity or compiler tuple.
- [x] The measured row is restated as an owner-native AST invariant.

## 7. Review Notes

- [x] Production condition has no hardcoded binary/function/address/corpus
  guard.
- [x] The change is in the semantic structuring owner, not the printer.
- [x] No new metric, pass, dependency, or vendor coupling is introduced.
- Quality status: validated below as a bounded control-flow readability gain;
  code-size growth is reported separately.

## 8. Measured Result

Release CLI, 224 binaries / 250 functions, with the committed parent outputs
preserved for file-by-file comparison:

| Layer | Gotos before | Gotos after | Delta | Lines before | Lines after |
|---|---:|---:|---:|---:|---:|
| NIR | 993 | 985 | **-8** | 40,343 | 40,364 (+21) |
| HIR | 985 | 977 | **-8** | 38,356 | 38,377 (+21) |

Only `bin_146` changes goto or line count in either layer, moving 31 to 23
gotos. No file regresses in goto count. Its NIR grows from 5,880 to 6,727
bytes and its HIR from 5,627 to 6,442 bytes because seven short abort tails
and one cleanup-return tail become direct terminal guards. The statements in
each duplicated tail retain their original evaluation order.

On the 204-function Fission/Ghidra/angr intersection, totals are now 944 NIR
and 936 HIR, versus Ghidra 691 and angr 420. The five prioritized rows are:

| File | NIR | HIR | Ghidra | angr |
|---|---:|---:|---:|---:|
| `bin_209` | 72 | 72 | 83 | 16 |
| `bin_039` | 66 | 66 | 32 | 30 |
| `bin_033` | 34 | 34 | 12 | 2 |
| `bin_146` | 23 | 23 | 5 | 2 |
| `bin_186` | 29 | 29 | 9 | 0 |

Validation results:

- targeted invariant: 1 passed;
- structuring + pcode: 1,243 passed, 1 skipped;
- phase-2 corpus execution differential: 1 passed;
- `cargo check` for pcode + decompiler: passed;
- NIR boundary scan: zero findings;
- full workspace: 2,503 passed, 5 skipped, with only the same seven
  pre-existing `fission-emulator` failures at the SLEIGH `addr64` decode gap;
- external local Docker run, cache disabled: 102 attempted variant rows,
  101 adapter-clean, one preflight-classified whole-program output,
  `valid=true`, `adapter_output_valid=true`, and
  `semantic_result_valid=true`. It is intentionally non-publishable; the
  local oracle ABI/provenance metadata remains unverified/unknown.

This is a measured control-flow readability improvement with an explicit
21-line duplication cost, not a source-size improvement claim.
