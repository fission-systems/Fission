# ADR 0009: ISA-agnostic semantic rules

**Status:** Accepted  
**Last verified:** 2026-07-10

## Context

Day-to-day quality work is correctly optimized for x86 / x86-64 sample binaries.
That optimization target must not become a license to copy semantic rules per
ISA. Fission is a Rust pipeline with explicit owners (builder/materialize,
normalize, structuring, type/data recovery). Shared mutable program state is
not available the way it is inside Ghidra's JVM, so each x86-32-only branch
tends to become permanent debt: the same CFG/dataflow failure reappears on x64
or ARM, and the pass graph grows without reducing rule count.

Recent quality fixes (loop-carried register updates, same-block conditional
moves, short-circuit diamond return arms, conditional copy merge barriers)
succeeded when restated as register/CFG/def-use invariants, and failed the
architecture test when phrased only as "m32 row" or "ebp+8" special cases.

Related ADRs:

- [0002](0002-fission-pcode-canonical-semantics.md) — `fission-pcode` owns semantics
- [0006](0006-decompiler-quality-change-gate.md) — quality change gate / invariants
- [0007](0007-ai-overfit-and-validation-firewall.md) — no row identity in production
- [0008](0008-nir-substrate-and-owner-boundaries.md) — substrate vs owner layers

## Decision

1. **Optimization target ≠ rule language.**  
   Prefer measuring and fixing x86 / x86-64 first. Production semantic logic must
   still be stated in ISA-agnostic terms: CFG, dominance, postdominance, SCC,
   def-use, register families, calling-convention *slots*, memory alias, and
   p-code op semantics.

2. **ISA differences live in models and tables, not in control-structure cores.**  
   Allowed ISA-specific surfaces:
   - SLEIGH / language id / `.sla` execution
   - cspec / register namer / primary return and param register *sets*
   - calling-convention stack-parameter classification
   - format loaders (PE/ELF/Mach-O) outside NIR semantics

   Not allowed as the primary condition for semantic behavior:
   - `if calling_convention == X86_32` (or similar) wrapping join/loop/cmov logic
   - hard-coded EBP/ESP offsets, register string names, or instruction mnemonics
     as the only guard for a meaning-changing transform
   - per-ISA copies of the same structuring or materialize rule

3. **Motivate with a row; implement a common invariant.**  
   A motivating binary may be gcc-m32, but the production condition must be the
   generalized fact (for example: a forward arm that defines the ABI primary
   return register is not a trivial empty forward into a return join).

4. **Tests follow the invariant, not the ISA label.**  
   Prefer synthetic CFG / p-code fixtures without function names, addresses, or
   compiler tuples in the assertion surface. An optional x86-shaped fixture is
   fine when it only supplies register offsets via the normal namer/cspec path.

5. **Default to extending shared helpers and owners.**  
   Grow helpers such as loop-carried update detection, same-block forward branch
   resolution, return-join live-in, and unconditional-copy merge barriers.
   Do not add a new normalize pass to paper over an ISA-local miss when the owner
   fact can be fixed once.

6. **Emergency end-of-pipeline cleanups are temporary debt.**  
   If a hoist/cleanup pass is added because an earlier stage dropped a pure
   param alias or similar, the proposal must mark the root owner and a removal
   path. Prefer fixing materialize/structuring over stacking ISA-shaped cleanups.

## Consequences

### Positive

- Rule count scales with **invariant families**, not ISA × compiler × row.
- Future ARM/MIPS/PPC work reuses register/loop/join cores and only supplies
  models (cspec, CC, SLEIGH).
- Reviews have a clear reject reason: "this is x86-32 copy-paste, restate as a
  common fact."

### Accepted costs

- Authors must spend a few sentences generalizing a motivating m32/x64 row.
- Some x86 encodings (absolute cmov targets, stack epilogue returns) still need
  *interpretation* helpers; those helpers decode encodings into common facts,
  they do not own separate semantic policies per ISA.

### Follow-up

- Keep [`Agents.md`](../../Agents.md) and NIR `AGENTS.md` aligned with this ADR.
- When touching quality proposals, the invariant section should name the common
  fact first and the motivating ISA second.

## Non-goals

- Pausing x86 / x86-64 quality work until other ISAs are equal.
- Forbidding all mention of x86 in comments, tests, or proposals.
- Requiring every helper to run on every language id on day one (admission and
  coverage may still be phased; the *rule statement* must still be general).
