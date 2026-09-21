# Preserve Local Register Definitions Before Loop-Carried Read Recovery

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/crypto_gcc_O2.exe`
- Function: `crc32`
- Address: `0x1400016e0`
- Corpus row or benchmark command: external DecBench local evaluation, `dev`
  corpus, focused on `crc32`, Fission local endpoint,
  `FISSION_BENCHMARK_NO_CACHE=1`; baseline artifact
  `results/issue103_before_723aecd96_recreated.json`.
- Current output summary: the raw builder preserves `xVar2 = data + length`
  and the byte load, but renders the CRC update as `rax = (uint)rax ^ param_2`
  instead of XORing the loaded byte. The loop condition is therefore faithful
  about the end pointer but the input-byte dataflow is wrong.
- Semantic cases passed / total: the nine `crc32` variants pass `29/54` cases
  in aggregate (`0.4259` mean pass rate); `gcc -O2` is `0/6` with a timeout,
  `clang -O2` is `1/6`, `gcc-m32 -O2` is `3/6`, and the two O0 rows pass `6/6`.
- Failure category: timeout for `gcc -O2`; assertion failures for `clang -O2`,
  `gcc-m32 -O2`, and `gcc -Os`; the remaining rows retain their independent
  runtime/compile/clean statuses in the before/after matrix.
- Relevant benchmark/static/readability observations: baseline `gcc -O2`
  source similarity is `0.2108`, GED `7`, type match `0.2857`, recompilation
  `0.16`, and `0/6` semantic cases. The decompiled function has an
  uninitialized end-pointer carrier in the normalized output and uses the
  length parameter as the CRC byte.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
Raw p-code already contains the complete dataflow:

  block_1: r9 = rcx + rdx
  block_2: unique_byte = Load(space=3, rcx, size=1)
            edx = ZExt(unique_byte)
            rdx = ZExt(edx)
            eax = eax ^ edx
  block_4: rcx = rcx + 1
            compare rcx != r9

The builder's raw PreHir keeps the end pointer and load:

  xVar1 = param_1 + param_2;
  xVar2 = xVar1;
  xVar11 = *(uchar *)(param_1);

but emits the first CRC update as:

  rax = (uint)rax ^ param_2;

The incorrect value is introduced by `lower_varnode_inner`: before lowering
the reaching `IntZExt` definition for the register read, it calls
`loop_body_carried_register_read_name`. That helper sees a later decrement of
the same physical register in the nested loop and returns the entry binding
`param_2`, even though the current block already has a reaching byte-load
definition for that register. The load and pointer end facts are therefore
not lost by raw lifting or normalize; the builder's loop-carried fallback
overrides a closer local definition.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Loop-carried register naming may provide a binding for a bare register read
only when the current use has no prior same-block reaching definition. A
same-block write, including a width-changing register alias such as
ZExt(byte_load) -> edx, is the nearest semantic definition and must be lowered
first. The loop-carried proof remains responsible for naming the update and
for reads that genuinely cross a block/iteration boundary.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is based on CFG/reaching-definition order and
      register alias coverage, not a function, address, binary, compiler, or
      ISA guard.
- [x] Register-width differences remain in the existing register-key alias
      model; the rule is shared by all register spaces.
- [x] Synthetic coverage describes only a loop, a same-block load/write, and a
      later self-update that must not shadow the local definition.

Comparable coverage:

- Similar shape 1: a loop loads a byte into a parameter register before an
  arithmetic consumer, then decrements the register as an inner-loop counter.
- Similar shape 2: a partial-register `Copy`, `ZExt`, or `SExt` write precedes
  a use while the full register also has a loop-carried update.
- Synthetic invariant test: the value consumed by same-block XOR is the local
  loaded byte, while the loop-carried name is used only for a later iteration
  read/update.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `loop_body_carried_register_read_name` in
  `crates/fission-pcode/src/midend/builder/materialize/loop_carried/mod.rs`,
  called from `lower_varnode_inner` in `expr/lower_expr.rs`.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [x] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed:
  the existing `has_prior_local_def_for_varnode` already computes the required
  same-block reaching-definition fact with the builder's register alias rules.
  The fix is a narrow precedence check at the existing fallback call; no new
  pass or representation is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot
  express the invariant: no new pass/helper/metric is planned.
- Possible interaction with existing normalize/structuring/materialize passes:
  loop-carried accumulator naming, pointer-scan cursor recovery, inner-loop
  counters, and partial-register alias tests must retain their current output.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none expected.
- Known cases that must not change: a bare loop-header register read with no
  local definition, true loop-carried self-updates, pointer cursors whose
  load address precedes the update, and ABI parameter naming when the entry
  register is the actual reaching value.

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command: add a builder regression for a same-block byte load/width-alias
    definition followed by a loop-carried register update.
  - Expected signal: the old code returns the ABI parameter for the local
    consumer; the fixed code lowers the load-derived value.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: the new regression and existing builder/materialize tests
    pass; any pre-existing failures are reported separately.
- [x] Focused benchmark row:
  - Command: cache-disabled local DecBench `dev --function crc32 --decompilers
    fission` before/after matrix.
  - Expected row-level improvement: CRC updates consume `data[i]`, preserve
    the end pointer, and improve semantic execution without a row-specific
    rule.
  - Measured baseline: `results/issue103_before_723aecd96_recreated.json`,
    nine rows, `29/54` cases, mean semantic score `0.4259`.
- [ ] Smoke or automation sample:
  - Command: cache-disabled dev smoke after the fix.
  - Expected no-regression signal: clean requested-function outputs and no
    adapter/boundary regressions.
- [ ] Optional related checks:
  - Command: `cargo check --workspace`, `cargo fmt --all --check`,
    `git diff --check`, and release CLI build.
  - Expected signal: clean compilation and formatting.
- [ ] Boundary audit, if a new pass/helper/dependency was added: no new pass or
  dependency is planned.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Information exposed in an AI prompt: none.
- Redaction confirmed: not applicable; no external/cross-model prompt was sent.
- Ghidra guidance confirmed: reference/correctness use only; no output-style
  mimicry request.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending after the fix.
  - Synthetic invariant test command/result: pending after the fix.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed for the proposed condition.
- The change does not claim semantic improvement from dashboard or benchmark-
  only edits:
  - [x] Confirmed.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; no new owner is introduced.
