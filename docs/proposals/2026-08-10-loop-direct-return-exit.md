# Loop Direct-Return Exit Preservation

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/.cache/decbench-local-eval/x86_O2/math.elf`
- Function: `binary_search`
- Address: `0x4008b0`
- Corpus row or benchmark command: DecBench local x86 O2, caches disabled; focused `fission_cli decomp <binary> --addr 0x4008b0 --profile quality --layer both --prehir --json --debug-decomp`
- Current behavior: GED is 13.0. The match edge at `0x4008d6` branches directly to the RET block at `0x4008e6` with `EAX=mid`, but Fission emits `if (uVar24 == target) break;` followed by the shared `return -1;`. The successful path therefore loses its edge-specific return value.
- Readability baseline: one residual goto, nesting depth 3, and an infinite loop with three break exits.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
The machine CFG has two distinct loop exits:

match:    0x4008d6 -> 0x4008e6 RET, EAX already contains mid
notfound: 0x4008df -> 0x4008e1 (EAX=-1) -> 0x4008e6 RET

`lower_loop_body_subgraph` builds `break_addrs` from every natural-loop exit
and emits `Break` for either conditional arm before asking the existing
`lower_return_join_expr_for_predecessor` hook whether an exit edge is a
value-bearing direct return. Linear multiblock lowering already uses that
hook and preserves the edge-specific return expression. Focused diagnostics
then showed a second owner-contract mismatch: the common RET block is accepted
as a side-effect-free epilogue join by final return lowering, but predecessor
edge recovery requires a pure join on x64 and returns `None`. The direct match
predecessor itself defines EAX, so that stricter rejection discards proven live
return evidence.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
An edge leaving a natural loop directly for a shared RET block is a terminal
return edge when the return-join owner can prove a predecessor-specific live
ABI return expression. It must be emitted as Return(expr), not collapsed to
Break. Other loop exits, including an exit through a block that assigns a
sentinel before the RET, remain Break and reach the common post-loop tail.

On x64, a non-pure shared RET is eligible only when it satisfies the existing
side-effect-free epilogue-join proof and the specific predecessor defines the
primary return register before its terminator.
```

ISA-agnostic check:

- [x] The structuring rule is CFG- and host-proof-based, not x86-specific.
- [x] ABI/register differences remain in the existing return-join host hook.
- [x] No function, address, compiler, or corpus identity enters production code.

Comparable coverage:

- Search loops returning an index directly on a match and a sentinel through a separate exit block.
- Loop-carried accumulators that branch directly to a shared terminal return on one exit.
- Negative case: an ordinary nonterminal loop exit remains `Break`.

## 4. Risk And Ownership Check

- Existing owners: `fission-midend-structuring::loops::lower_loop_body_subgraph` and builder return-join edge recovery.
- Shared substrate: natural-loop exit set plus the existing `StructuringHost::lower_return_join_expr_for_predecessor` proof.
- No new pass, telemetry payload, dependency, or ISA-local rule is required.
- Risk: incorrectly turning an ordinary exit into a return. Guard by requiring the existing return-join hook to produce a proven expression; `None` retains current break behavior.

## 5. Validation Matrix

- [x] Positive targeted p-code/CFG test: direct loop exit to shared RET becomes a value-bearing return while the sentinel exit remains post-loop.
- [x] Negative boundary in the same test: the exit that assigns a sentinel first remains the post-loop return path.
- [x] `cargo nextest run -p fission-midend-structuring` (115/115 pass)
- [x] `cargo nextest run -p fission-pcode` (988 pass, 1 skipped)
- [x] `cargo check -p fission-pcode`
- [x] Focused real binary rerun at `0x4008b0` with caches disabled: output now contains `if (uVar24 == target) return mid;`; the sentinel path remains `return -1;`, and loop multi-exit break count changes from 3 to 2.
- [x] Focused DecBench x86 O2 GED rerun: 13.0 -> 6.0; type match remains 0.8333 and byte match remains 0.0.
- [x] Broader cache-disabled DecBench core profile rerun: 165/165 targets produced, zero errors. x86 O2 remains weak overall (`GED0=2/18`, `Type1=0/18`), so this is a focused row improvement rather than a general O2 solution.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation review was requested.
- Production invariant is expressed only in CFG exit and proven return-join terms.

## 7. Review Notes

- [x] No hardcoded row identity.
- [x] Existing CFG/ABI proof is reused.
- [x] Focused quality improvement is backed by the same-row real-binary and DecBench measurement; the broader core run has no production/robustness regression but still confirms substantial O2 quality debt.
