# Destructive Compare Operand Snapshot

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_207.elf`
- Function: `write_stamp` (`sub_7000` in the stripped output)
- Address: `0x7000`
- Corpus row or benchmark command: `fission_cli decomp .../bin_207.elf --addr 0x7000 --layer both --prehir`
- Current output summary: the epilogue `sub rax, fs:0x28; jne __stack_chk_fail`
  becomes `xVar30 = local_20 - xVar34; if (xVar30 == xVar34)`, comparing the
  subtraction result with the right operand instead of testing the result for
  zero or comparing the two pre-subtraction values.
- Semantic cases passed / total: no executable cases are provided by the
  evalkit; raw p-code and machine instruction semantics disagree with PreHIR.
- Failure category: destructive compare input is read after its defining
  register has been overwritten.
- Relevant benchmark/static/readability observations: the same malformed
  predicate is present in 65 current sample-set files across 23 projects:
  O0 9, O2 15, and O2-noinline 41. NIR has 1,119 gotos and HIR has 1,116 before
  this change; no goto reduction is expected from the semantic repair alone.

Comparable measured rows:

| binary | project / optimization | function / address | current malformed predicate |
| --- | --- | --- | --- |
| `bin_011.elf` | sysvinit / O2 | `newtoold` / `0x1740` | `(saved - guard) == guard` |
| `bin_135.so` | libedit / O0 | `rl_message` / `0x3045f` | `(saved - guard) != guard` |

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
0x7074  IntSub     rax, guard -> rax
0x7074  IntEqual   rax, 0     -> zf
0x707d  BoolNegate zf         -> branch_cond
0x707d  CBranch    fail, branch_cond

Machine/p-code meaning: branch to fail when pre_sub_rax != guard.
Current PreHIR: xVar30 = local_20 - xVar34; if (xVar30 == xVar34) success.
```

`match_cmp_diff_from_peeled` correctly identifies the two `IntSub` inputs, but
`lower_x86_branch_predicate` lowers them at the later `CBranch` site. Since the
`IntSub` output aliases its first input register, reaching-definition lookup
then resolves that input to the `IntSub` itself. The first operand must instead
be lowered at the flag-producing comparison site, where the prior definition is
still in scope.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Operands recovered from a comparison flag definition are snapshots at that
definition. Lower each operand using the flag/difference producer as the use
site. Never re-read a two-address input at the later branch site when the
arithmetic output may alias that input.
```

ISA-agnostic check:

- [x] Production condition is expressed as p-code def-use/reaching-definition
  semantics, not a function, address, compiler, or ABI special case.
- [x] No ISA-specific table or copied control-structure implementation is added.
- [x] The synthetic test states only the destructive p-code dataflow shape.

Comparable coverage:

- Similar shape 1: sysvinit/O2 `newtoold`, distinct project and binary.
- Similar shape 2: libedit/O0 `rl_message`, distinct project and optimization.
- Synthetic invariant test: an `IntSub` whose output aliases its first input,
  followed by `IntEqual(result, 0)`, `BoolNegate`, and `CBranch`.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `PreviewBuilder::try_recover_branch_condition` and
  `lower_x86_branch_predicate`.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] P-code semantic contract
- Why extending that owner is sufficient: the matcher already recovers the
  comparison and the builder already supports lowering under an explicit
  `LoweringSite`; only the producer site is currently discarded.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no pass or metric is added.
- Possible interaction: all recovered equality, ordering, carry, sign, and
  overflow predicates will read comparison operands at their flag-producing
  site. Non-destructive unique temporaries should remain byte-identical.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change: existing compare recovery where the
  `IntSub` result is a distinct unique varnode; TEST-derived zero comparisons;
  raw flag fallback when a comparison cannot be proven.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode destructive_cmp_branch_tests_result_against_zero`
  - Expected signal: the destructive result is compared with zero and is never
    compared with one of its own operands.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: all fission-pcode tests pass.
- [x] Focused benchmark rows:
  - Command: release CLI decompilation of `bin_207@0x7000`, `bin_011@0x1740`,
    and `bin_135@0x3045f`, NIR and HIR.
  - Measured result: all three rows now test the destructive subtraction result
    against zero. `bin_207` and `bin_011` take the normal return only when the
    result is zero; `bin_135` calls the failure function only when it is nonzero.
    No goto-count claim is made.
- [x] Sample-set regression:
  - Command: regenerate all 250 evalkit functions in NIR and HIR and compare
    per-file output/goto/short-circuit counts with the recorded baseline.
  - Measured result: NIR and HIR both completed 250/250 functions. NIR remained
    at 1,119 gotos and HIR at 1,116, with zero per-file goto regressions. NIR
    lost 151 lines and HIR lost 130 lines through dead comparison temporaries;
    NIR/HIR short-circuit counts were unchanged.
- [ ] Non-scored validation split:
  - Command: search and decompile matching destructive comparison shapes from
    `fission-benchmark/corpus/dev` if present.
  - Expected signal: the same p-code invariant holds outside the scored rows.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Measured result: 0 findings, 0 violations, 0 migration debt.

The mandatory Docker benchmark cannot be used in this session because the
Docker client cannot connect to the configured OrbStack daemon socket. The
checked-in evalkit and focused raw-p-code evidence are the available local
validation paths. No official leaderboard claim will be made.

## 6. AI Review / Prompt Firewall

- Was another AI model asked for implementation advice?
  - [x] No
- Information exposed in an external AI prompt: none.
- Ghidra/Glaurung guidance: reference inspection is limited to invariants; no
  runtime dependency or output-style copying is introduced.
- Unseen or synthetic validation evidence: the destructive-input synthetic test
  plus non-scored corpus/dev search above.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
