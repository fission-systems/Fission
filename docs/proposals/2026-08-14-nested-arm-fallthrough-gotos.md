# Remove nested-arm gotos to the immediately following label

## 1. Baseline Row Anchors

The current 224-binary / 250-function NIR corpus emits 1013 gotos. A rendered
label/target census resolves 518 of them and finds 12 forward transfers whose
target label is two source lines later. Direct inspection shows the recurring
AST shape:

```text
if (cond) {
    statements;
    goto L;
}
L:
```

Measured examples include:

- `bin_069.elf`: 14 gotos, 221 lines, 6,953 bytes; the arm ending at rendered
  line 103 jumps to the parent sequence's label at line 105.
- `bin_117.elf`: 17 gotos, 246 lines, 7,513 bytes; two separate nested arms
  end in transfers to their immediately following parent labels.
- The same shape is present in `bin_008`, `bin_009`, `bin_033`, `bin_039`,
  `bin_073`, and `bin_179`.

The existing structuring finalizer already knows this invariant, but it runs
before the late `invert_forward_guard_gotos` layout rewrite. That rewrite can
create the nested arm only after the finalizer has passed, so the final output
retains the otherwise redundant transfer.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Structuring post-layout cleanup
- [ ] Normalize control-flow cleanup
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

The CFG has already been represented faithfully. The redundant transfer is a
property of the final structured AST's parent/child layout. The existing
`eliminate_redundant_gotos` finalizer owns exactly this canonicalization; the
defect is orchestration order, not a missing normalization rule.

## 3. General Invariant

After all post-layout rewrites, run the idempotent structuring finalizer once
more. For an `If` at parent sequence index `i`, let `L` be the label at
`i + 1`. When either arm's final statement is `Goto(L)`, its existing
parent-successor-aware rule removes only that final statement: completion of
the arm falls through to the same parent label. Statements before the transfer
retain their order and evaluation count.

This applies independently to both arms. The label is not removed and may
remain the target of other scopes. No address, function, compiler, or ISA fact
is used.

## 4. Safety Contract

- Only the final statement of an immediate `If` arm may be removed.
- The target must equal the label immediately following the parent `If`; no
  search past intervening statements is allowed.
- Nested loops and switches are not traversed by this individual rewrite;
  their own sequence invocation decides their fallthrough.
- The existing rule does not discard the `If` condition when an arm becomes
  empty; the normal cleanup pipeline retains its established side-effect
  contract.
- Defined labels and all other goto references remain untouched.

## 5. Validation Matrix

- [x] Existing positive then-arm, else-arm, loop-boundary, and switch-boundary
  coverage for the parent-successor rule.
- [x] Integration coverage for guard inversion creating the shape before late
  finalization removes it.
- [x] Focused NIR/HIR reruns for the measured rows.
- [x] `cargo nextest run -p fission-midend-structuring -p fission-pcode`.
- [x] `cargo check -p fission-pcode -p fission-decompiler` and release CLI.
- [x] DecBench NIR/HIR full-corpus and per-file comparison.
- [x] Phase-2 corpus ground truth with zero divergence.
- [x] Owner-boundary and benchmark-smell scans.
- [x] External local Docker benchmark, non-published.

## 6. Quality Claim Gate

The rewrite is mechanically justified by structured fallthrough, but a quality
claim requires measured NIR/HIR reduction with no per-file or semantic
regression.

## 7. Measured Results

- Focused rows moved identically in NIR and HIR: `bin_069` 14→13,
  `bin_117` 17→15, `bin_008` 28→27, `bin_009` 26→25, `bin_033` 35→34,
  `bin_039` 68→66, `bin_073` 27→24, and `bin_179` 14→13.
- Full 224-binary / 250-function corpus: NIR 1013→996 and HIR 1005→988.
  Twelve files improved, 212 tied, and zero regressed in each layer.
- On the 204-file Fission/Ghidra/angr intersection: NIR 955, HIR 947,
  Ghidra 691, angr 420.
- Structuring tests: 238/238; pcode tests: 1000/1000; phase-2 real-machine
  differential: 1/1; owner boundary scan passed. The full workspace run was
  2498 passed, 7 failed, and 5 skipped; all seven failures are the pre-existing
  emulator `addr64 DecodeNoMatch` set at `0x1007c8e`.
- External local Docker smoke: 10/10 backend clean, 10/10 output clean,
  matrix valid, artifact valid, and overall local validity true. This result
  is intentionally non-publishable and was not promoted.
- The repository-wide smell scan still reports its pre-existing broad
  findings; this change adds no binary address, function name, compiler, or
  ISA-specific condition to production code.
