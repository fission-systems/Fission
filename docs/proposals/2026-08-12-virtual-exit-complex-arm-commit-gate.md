# Virtual-exit complex-arm commit gate

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_073.elf`
- Function: `sub_6780`
- Address: `0x6780`
- Command: release `fission_cli decomp .../bin_073.elf --layer nir --json --no-header --no-warnings --addr 0x6780`
- Current output: 559 lines, 16,501 bytes, and 42 `goto` statements; entry emits `if (!rax) { goto block_6f60; }`.
- Semantic cases: unavailable in this DecBench sample-set lane.
- Failure category: complex conditional arms with distinct real returns have only a virtual common postdominator, and recursive arm lowering is unsafe while it runs inside an uncommitted candidate.
- Measurement history: a reverted recursive-arm experiment changed the 224-binary total from 2,135 to 2,296 gotos and changed unrelated whole-function types. No part of that experiment is retained.

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
ImmPostDomTree::compute constructs a virtual super-exit for multiple returns,
then truncates it and remaps every real node whose idom is the virtual exit to
self. nearest_common_postdominator therefore cannot distinguish "no common
postdominator" from "common postdominator is the virtual function exit".

Temporary field fingerprints on bin_073 idx=0 also proved that failed collapse
rules mutate locals, stack-slot ownership, temps, materialized_vns, and lowering
caches. A broad rollback prototype kept 42 gotos but substantially rewrote
whole-function register/temp bindings, proving that the current pipeline also
depends on pre-lowering side effects. It was reverted.
```

## 3. Generality / Invariant Proof

Generalized rules:

```text
1. In a CFG with multiple real exits, the common postdominator of arms that end
   at different exits is a typed VirtualExit fact, not an absent fact and not a
   fabricated real block index.
2. Recursive complex-arm structuring may execute only after shape and admission
   have established that its enclosing stable-first conditional will commit.
   Failed probes must not recursively lower arbitrary blocks.
```

ISA-agnostic check:

- [x] Both rules use CFG/postdominance and dispatch facts only.
- [x] No ISA or calling-convention gate is added.
- [x] Synthetic tests use only graph shape and candidate commit behavior.

Comparable coverage:

- Multiple exits: two conditional arms reach distinct returns.
- Mixed arm: one arm is linear and the other contains a loop/nested conditional before a distinct return.
- Negative: an arm trapped in a non-terminating SCC has no exit-reachable postdominator.

## 4. Risk And Ownership Check

- Existing owners: `cfg_analysis::ImmPostDomTree`, `conditionals::if_else`, and `sese_driver` stable-first dispatch.
- Shared substrate: CFG / immediate postdominance.
- Why extension is sufficient: retain the already-computed virtual root and expose it through a typed query; do not add a pass or vendor dependency.
- Interaction risk: existing real-block follow callers must keep receiving `Option<usize>` and must never treat the virtual exit as an index. Recursive lowering is a separate measured slice after the substrate lands.
- Telemetry: none for the virtual-exit substrate. A later recursive slice may reuse existing rejection telemetry.
- Known cases that must not change: single-exit diamonds, real join follows, loops disconnected from every exit, current NIR/HIR text and goto totals for the substrate-only commit.

## 5. Validation Matrix

- [x] Targeted invariant tests
  - Command: `cargo nextest run -p fission-midend-structuring virtual_exit`
  - Expected: distinct-return arms report `VirtualExit`; legacy real-follow query remains `None`; real diamonds still report their join.
- [x] Crate gates
  - Command: `cargo nextest run -p fission-midend-structuring && cargo nextest run -p fission-pcode`
- [x] Focused real row
  - Command: rebuild release CLI and rerun `bin_073.elf@0x6780`.
  - Expected for substrate-only slice: byte-identical NIR/HIR, 42 gotos.
- [ ] External sample-set smoke
  - Command: `vendor/decbench-evalkit/decbench-evalkit-sample-set/run_fission.py`
  - Expected for substrate-only slice: no output/goto regression.
- [x] Related checks
  - Command: `cargo check -p fission-pcode && cargo check -p fission-decompiler`
  - Result: both checks passed; `nir_boundary_scan.py` also reported zero findings.

## 6. AI Review / Prompt Firewall

- Another AI model was not asked for implementation advice.
- No external prompt or vendor implementation is used.
- Production conditions contain no binary, function, address, corpus, compiler, or ISA identity.
- A later recursive-arm implementation needs both this focused row and an unseen/synthetic regression signal before any quality claim.

## 7. Review Notes

- [x] No hardcoded row identity in production.
- [x] No benchmark/printer-only semantic claim.
- [x] Existing postdom and conditional owners are extended; no duplicate pass.
