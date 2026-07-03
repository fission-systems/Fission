# ADR 0006: Decompiler quality change gate

**Status:** Accepted
**Last verified:** 2026-07-03

## Context

Fission quality work often starts from a concrete benchmark row: a binary,
function, address, and observed decompiler failure. That row is useful evidence,
but it is also the easiest path to overfitting. A semantic fix that checks a
function name, address, binary id, corpus row, or compiler artifact can improve a
dashboard cell while weakening the decompiler architecture.

The first guarded scope is the core semantic layer: builder, materialize,
normalize, structuring, and type/data recovery. UI, printer-only, benchmark, and
dashboard changes can report quality, but they do not count as semantic fixes.

## Decision

Every semantic decompiler-quality change must pass a pre-implementation proposal
gate before production code is added. Use
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md)
and record the chain:

1. **Row anchor:** the binary, function, address, current output, semantic case
   count, and failure category that motivated the work.
2. **Owner proof:** evidence that the bug belongs to SLEIGH/raw p-code,
   builder/materialize, normalize, structuring, type/data recovery, printer, or
   benchmark/automation.
3. **Invariant proof:** the generalized condition that should hold beyond the
   motivating row.
4. **Validation matrix:** targeted invariant test, `cargo nextest run -p
   fission-pcode`, focused benchmark row, and smoke/automation sample.

Production semantic code must not branch directly on:

- function names,
- addresses,
- binary ids or paths,
- corpus row ids,
- compiler artifacts that are only row-identifying labels.

Allowed production conditions are invariant-backed facts such as:

- ABI or ISA rules,
- p-code operation semantics,
- CFG, dominance, postdominance, and SCC facts,
- def-use and reaching-definition facts,
- type constraints,
- calling-convention facts,
- memory alias facts,
- documented binary-format semantics.

The default implementation path is to extend the existing owner/pass. Add a new
pass, helper, or metric only when the proposal shows that no existing owner
already covers the invariant. A targeted test is required, but it is never enough
to claim success by itself.

This ADR intentionally does not add CI enforcement yet. The gate starts as ADR +
checklist process, and automated enforcement can be added after the policy is
stable.

## Consequences

- Concrete benchmark rows remain the starting evidence, not the production
  condition.
- Reviewers can reject semantic-fix claims that only change the dashboard,
  benchmark harness, printer surface, or row-specific behavior.
- Reviews have a single place to check whether a change is generalized before
  reading the implementation.
- Small quality fixes require a little more written proof, but the proof should
  be short and reusable in PR descriptions and release notes.
