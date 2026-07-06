---
name: "fission-decompiler-quality"
description: "Portable AI skill for improving Fission decompiler quality without benchmark overfit. Use for semantic fixes, NIR/HIR recovery, structuring, type/data recovery, and pseudocode quality work."
---

# Fission Decompiler Quality Skill

Use this skill when improving Fission's Rust-native decompiler pipeline. The goal is better semantic correctness and readable C-like pseudocode without row-specific, benchmark-specific, or AI-prompt overfit.

## Priority Order

1. Improve x86/x86-64 source-semantic correctness and readable pseudocode quality.
2. Fix type/data abstraction: pointers, arrays, structs, fields, calling convention, parameters, locals.
3. Improve hard control-flow recovery: if/else, switch, loops, break/continue, irreducible cases.
4. Preserve SLEIGH/p-code lift correctness; do not add manual SLEIGH mappings or legacy fallback debt.
5. Improve name/FID recovery only after the semantic owner is clear.
6. Expand architecture/file-format breadth after x86/x86-64 quality is stable.

## Architecture Contract

`fission-pcode` owns shipped decompiler semantics, but it is not a dumping ground for every fix. Treat it as:

- **Substrate:** IR/HIR types, p-code/NIR contracts, telemetry, action pipeline, shared CFG facts, def-use facts, type constraints, calling-convention facts, and alias facts.
- **Owner layers:** builder/materialize, normalize, type/data recovery, structuring, and render/printer.
- **Future split candidates:** `fission-nir-analysis`, `fission-nir-normalize`, `fission-structuring`.

Before adding code, translate the observed failure into an owner-native invariant. Prefer reusable dataflow, def-use, type-constraint, calling-convention, CFG, or alias analysis over another narrow pass/helper.

Forbidden production shortcuts:

- Function-name, address, binary-path, corpus-row, or compiler-tuple branches.
- Printer/UI/dashboard-only semantic fixes.
- Structuring code that repairs expression semantics.
- Normalize code that recovers builder-owned stack/parameter facts.
- Builder code that calls normalize, structuring promotion, or render policy as a shortcut.
- Ghidra output-style mimicry when a clearer equivalent preserves correctness.
- Runtime/build dependency on `vendor/` reference code or C++ bindings.

## Improvement Workflow

1. Record the baseline: motivating row/sample, current output, semantic case count, failure category, and smallest p-code/NIR/HIR/CFG evidence.
2. Prove the owner: SLEIGH/raw p-code, builder/materialize, normalize, type/data recovery, structuring, printer, or benchmark/automation.
3. State the invariant using ABI/ISA, p-code semantics, CFG/dominance/postdominance, def-use, type constraints, calling convention, or alias facts.
4. Check whether the invariant belongs in shared substrate analysis before adding a pass/helper.
5. Extend the existing owner/pass by default. Add new owners only with explicit proof that no existing owner covers the invariant.
6. Add targeted invariant coverage, including negative cases when broadening acceptance.
7. Validate with focused tests, crate-level tests/checks, focused benchmark row, and smoke/automation no-regression evidence.
8. Report quality truthfully: distinguish mechanical code changes, telemetry-only changes, benchmark harness changes, and actual semantic improvement.

## AI Prompt Firewall

When asking another AI model for help, provide only structural information:

- structural failure pattern,
- owner evidence,
- invariant candidates,
- forbidden shortcuts,
- validation matrix.

Redact benchmark function names, concrete addresses, binary paths, corpus row ids, and compiler tuples. Ghidra may be used as a cleanroom reference for algorithms and invariants, not as an output-style target.

Use `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md` for cross-model review and `docs/templates/DECOMPILER_CHANGE_PROPOSAL.md` before production semantic changes.

## Validation Commands

Run the smallest relevant checks first, then broaden:

```bash
cargo nextest run -p fission-pcode --filter <targeted_test>
cargo nextest run -p fission-pcode
cargo check -p fission-pcode
cargo check -p fission-decompiler
python3 scripts/test/no_benchmark_fixture_refs_in_crates.py
python3 scripts/audit/benchmark_smell_scan.py --root . --mode fast --fail-on high
python3 scripts/audit/ai_prompt_leak_scan.py --root .
python3 scripts/audit/nir_boundary_scan.py --root .
```

For semantic claims, also rerun the focused source-semantic benchmark row with stale decompilation and behavior caches disabled. Compare behavior status, case progress, stderr/stdout, pseudocode size, and feature gaps, not just aggregate scores.

## Review Rejection Rules

Reject a proposed fix if it:

- improves only a dashboard/metric without moving semantic evidence,
- adds a sample-specific guard,
- creates a new narrow pass where shared analysis would fit,
- increases owner-to-owner dependency debt without justification,
- claims success from one targeted test only,
- copies reference implementation code instead of cleanroom reasoning.
