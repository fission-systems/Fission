## Mandatory: Read AGENTS.md First

Before starting any task in this repository, you MUST read and internalize the nearest `AGENTS.md` file. The project structure, conventions, anti-patterns, and validation commands documented there are binding rules for all work. Do not bypass or override AGENTS.md guidance without explicit user direction.

---

Current priority order:

1. x86 / x86-64 decompilation correctness and readable pseudocode quality.
   SLEIGH lift coverage is now good enough that day-to-day quality work should focus on source-semantic correctness and human-readable pseudocode for x86/x86-64 sample binaries.
   The goal is not only mechanically correct p-code/NIR, but final output that reads like useful C pseudocode.

   Focus areas:
   - control-flow recovery
   - if / else / switch / loop / break / continue structuring
   - pointer, array, struct, and field-access expressions
   - calling convention, parameter, and local-variable recovery
   - removal of unnecessary temporaries
   - C-friendly recovery of pointer arithmetic into array/index/field forms
   - return-value, accumulator, and loop-induction-variable cleanup
   - function-level pseudocode readability compared with Ghidra

2. Type and data abstraction.
   Improve struct, pointer, array, field access, calling convention, parameter, and local recovery at the NIR/HIR semantic layer, not by output-only substitution.

3. Large and hard function structuring.
   Improve small sample functions first, then extend to complex x86/x86-64 functions using CFG, dominance, post-dominance, SCC, dataflow, and fixed-point analysis.

4. Maintain SLEIGH lift correctness and prevent regressions.
   Do not add manual mappings in the SLEIGH engine. Keep `.sla` ConstructTpl execution as the success source, and do not grow legacy token cursor, BoundOperand fallback, or compatibility-classifier debt.
   When SLEIGH changes are necessary, validate row-level raw p-code parity first, then the canonical gate, then benchmarks.

5. FID/name recovery.
   Gradually improve packed `.fidb`, exact hash inputs, and program seeker coverage relative to Ghidra Function ID / signature / symbol ecosystems.

6. Architecture / file-format breadth.
   Only after x86/x86-64 quality is solid, expand to ARM, MIPS, PPC, ELF, Mach-O advanced cases.

Required principles:
1. Bring sample-binary quality up before attacking real-world binaries.
2. Approach from a Ghidra 1:1 clean-room migration perspective.
3. Design and implement on a zero-dependency basis by default.
4. Prefer CFG, dominance, dataflow, fixed-point computation, and constraint-based inference over simple pattern matching or temporary heuristics.
5. Avoid overfitting to a specific ISA/compiler, but keep the current optimization target on x86/x86-64.
6. Only consider using a Rust library when a long-term bottleneck is confirmed that cannot be solved internally. C++ bindings are forbidden.
7. Prefer long-term maintainability and generalizable structural improvements over short-term output patches.
8. Proposals and implementations must include an architectural perspective, verifiability, and observability that remains valid after at least 2–3 cycles.
9. Do not use approximations or estimates. Base improvements strictly on measured values and verifiable analysis.
10. The ultimate success criterion is actual improvement in semantic accuracy and pseudocode quality in `benchmark/source_semantic_benchmark`.

Additional path / resource principles:
- `/Users/sjkim1127/Fission/utils` contains internal resources, type info, signatures, and benchmark support data that Fission can reuse.
- Avoid hardcoding or duplicate implementations; prefer leveraging existing data/helpers inside `utils` when applicable.
- `utils` usage must connect to a long-term maintainable structure, not a temporary semantic bypass.
- Resolve resource paths through `PathConfig`, resource roots, and existing loading paths; avoid hardcoded absolute paths.

Reference paths:
- `/Users/sjkim1127/Fission/vendor` is a reference-only external code path.
- In particular, `/Users/sjkim1127/Fission/vendor/ghidra/ghidra-Ghidra_12.0.4_build` can be referenced frequently from a Ghidra 1:1 clean-room migration perspective.
- Use `vendor` code only as a reference for design, algorithms, semantics, and edge-case understanding.
- Do not depend on `vendor` code directly or add runtime/build dependencies.
- C++ bindings are forbidden.

Operational rules:
- No manual mapping in the SLEIGH engine.
- Commit and push to GitHub regularly.
- After changes, run related targeted tests, crate checks/tests, and source-semantic benchmarks within feasible scope.
- Commit and push on the `main` branch only.

## Layer Ownership Decision Tree

When a decompilation quality bug is observed, prove the owner before writing code. Fix behavior at the canonical owner; do not patch downstream.

| Symptom | Likely Owner | Proof needed |
|---|---|---|
| Wrong instruction decode, missing ops, bad operand sizes | SLEIGH / raw p-code | `fission_cli raw-pcode` output does not match expected machine-code semantics |
| Wrong variable names, extra temp bindings, param re-binding, loop-carried alias mismatch | Builder (`nir/builder/`) | HIR after materialization already shows the defect |
| Wrong types, missing cast elision, dead assignments not removed | Normalize (`nir/normalize/`) | Def-use chains, type inference, or cleanup passes produce wrong result |
| Unstructured gotos, wrong loop shape, bad if/else nesting | Structuring (`nir/structuring/`) | CFG dominator/post-dominator facts do not match expected structure |
| Ugly formatting, wrong parentheses, missing type decorations | Printer (`nir/printer.rs`) | HIR itself is correct; only rendering is off |
| Benchmark row mis-scored, stale cache, false positive/negative | Benchmark / automation (`benchmark/`, `fission-automation/`) | Oracle logic or artifact diff is the source of discrepancy |

If the bug spans multiple layers, fix the **earliest** layer that produces wrong facts. Do not add a normalize workaround for a builder bug.

## Synthetic Test Writing Rules

Every real-binary bug must leave behind a synthetic regression test. Follow these rules:

1. **Invariant-based, not sample-specific:** The test must not contain hardcoded function names, addresses, binary paths, or compiler-specific patterns. It should test a structural invariant (e.g., "GPR32 update must reuse the prior wide alias, not rebind a param").
2. **Deterministic output:** Tests that inspect printed HIR or pseudocode must use stable ordering and fully specified `NirBinding` fields. Avoid tests that pass only because `HashMap` iteration happens to be stable.
3. **Minimal surface:** Include only the statements, bindings, and types needed to trigger the bug. Do not copy an entire function body.
4. **Two-bar test:** A good decompiler test checks both "did not crash" and "produced the correct semantic structure. A test that only asserts "no panic" is insufficient.
5. **Builder tests over end-to-end when possible:** If the bug is in materialization, add a builder-level unit test (`nir/builder/.../tests.rs`) rather than a full binary decompilation test. It runs faster and isolates the layer.

## Heuristic vs Algorithmic Threshold

Prefer algorithmic, invariant-based solutions. Temporary heuristics are technical debt.

| If the problem can be solved by ... | Do this | Avoid this |
|---|---|---|
| CFG, dominance, post-dominance, SCC | Write the algorithmic pass | A pattern matcher on specific opcode sequences |
| Dataflow, fixed-point propagation | Add a new dataflow fact or propagate through existing passes | A one-off scan for known variable names |
| Type-system rules (subtyping, width) | Extend the type inference pass with a proper rule | Output-only regex substitution in the printer |
| Def-use chain analysis | Use `defuse` helpers in `nir/normalize/analysis/` | Manual dead-code elimination by statement text match |

If you introduce a heuristic, it must be guarded by an invariant check and have a ticket referencing the planned algorithmic replacement. Heuristics without a removal plan are not allowed.

## Performance & Complexity Guardrails

Passes in the decompiler pipeline run on every function. A slow pass hurts all functions, not just the hard ones.

1. **Pass budgets are binding:** The `normalize_hir_function` pipeline already uses `PassBudget { stmt_limit, block_limit, round_limit }`. Do not silently raise these limits to make a test pass. If a function legitimately exceeds the budget, the pass must early-exit and leave the function unchanged rather than run unbounded.
2. **Complexity warning gate:** Before merging an algorithm that is worse than O(n log n) in the number of basic blocks or statements, add a comment justifying the complexity and a targeted benchmark on the worst-case sample (e.g., a function with >200 blocks). If no such benchmark exists, the change is blocked.
3. **No unbounded recursion:** The HIR tree can be deeply nested. Any recursive walk must have an explicit depth limit or be converted to an iterative walk with an explicit stack.
4. **Allocations are not free:** Do not clone the entire function body inside a cleanup pass. Mutate in place where possible, and use `std::mem::take` or `std::mem::replace` for local swaps rather than deep clones.
5. **Profile before optimizing:** If a pass is suspected to be slow, add `tracing` spans or `Instant::elapsed()` measurements. Do not optimize based on guesswork. Use `cargo flamegraph` or equivalent when available.

## Scope Control / When to Stop

The decompiler is never "done." Knowing when to cut a PR is as important as knowing what to build.

1. **One concern per PR:** A PR should fix one bug, add one pass, or add one invariant. Do not bundle a builder fix, a new normalize pass, and a benchmark script update in the same PR unless they are inseparable.
2. **Stop at the first green bar:** If the targeted test passes, the smoke row improves, and no regressions appear, stop. Do not continue refactoring "while you're here" unless the refactoring is necessary for the fix to be correct.
3. **The 80/20 rule for generality:** If a fix solves 80% of observed cases with an invariant-based algorithm, ship it. Do not delay the PR to chase the remaining 20% with a 10x more complex solution. File a ticket for the edge case and move on.
4. **No speculative abstractions:** Do not introduce a trait, a plugin system, or a generic framework for a problem that currently has only one instance. Abstract after the second or third concrete use case, not before.
5. **Tests are the contract:** If the tests capture the invariant, the implementation is allowed to be simple and even slightly ugly. Do not rewrite a working pass just to make it aesthetically pleasing unless the rewrite also fixes a real bug.
6. **Call out known-failures explicitly:** If a pre-existing test fails on `main` and your change does not fix it, mention it in the PR description. Do not silently ignore it.

## Regression Prevention / Source-Semantic Quality Workflow

Use this workflow for every decompiler-quality change, especially when a concrete row/function motivated the fix.

1. **Anchor the row:** Record the source file, binary, address, function name, current behavior status, case pass count, semantic/static scores, and the top missing/extra features.
2. **Find the owner:** Prove whether the bug belongs to SLEIGH/raw p-code, NIR materialization, type recovery, structuring, cleanup, printer, or benchmark/automation. Fix behavior at that owner.
3. **Add focused coverage:** Add or update the smallest targeted Rust/Python test that captures the invariant. Synthetic tests are necessary but not sufficient for decompiler-quality claims.
4. **Make the scoped change:** Keep production changes invariant-based, not function/address/sample-specific. Do not add runtime/build dependencies on `vendor/` reference tools.
5. **Run local checks:** Run the targeted test first, then the relevant crate checks/builds from the Build/Test section. If a known unrelated test is already failing, call it out explicitly.
6. **Run the focused benchmark:** Rerun the exact source-semantic row with no stale decompilation or behavior cache when measuring a semantic fix. Compare behavior status, case progress, stdout/stderr, line/byte size, and static feature gaps.
7. **Check regressions:** After a focused improvement, run the broader smoke manifest or automation lane. Existing pass rows must not regress, and weighted semantic/static scores should not drop without an explicit tradeoff.
8. **Report both bars:** Distinguish “mechanically changed” from “quality improved.” A merged test-only or telemetry-only change is not a semantic fix unless the row-level oracle moves.

### Source-Semantic Benchmark Commands

```bash
# Canonical benchmark runner
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py --help

# Run a specific row (example)
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py \
  --binary benchmark/binary/x86-64/window/small/binary/c/test_functions.exe \
  --function sum_array

# Smoke / quick validation
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py --smoke
```

### Pre-merge Quality Gate Checklist

- [ ] Targeted unit test passes (`cargo nextest run --filter ...`)
- [ ] Crate-level unit tests pass (pre-existing failures explicitly called out)
- [ ] Smoke benchmark row for the fixed sample shows improvement with no stale cache
- [ ] No regression in other smoke rows (diff artifacts)
- [ ] `cargo check -p fission-pcode` and `cargo build -p fission-cli --release` are clean (modulo existing warnings)
- [ ] The change is invariant-based, not function/address/sample-specific
- [ ] The change fixes behavior at the canonical owner, not downstream UI/surface layers
