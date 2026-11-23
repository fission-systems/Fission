# ADR 0017: Ghidra is a reference, not an oracle

**Status:** Accepted
**Last verified:** 2026-11-23
**Amends:** [ADR 0005](0005-ghidra-reference-boundary.md) (its "indispensable
oracle for sleigh semantics" context) and [ADR 0007](0007-ai-overfit-and-validation-firewall.md)
("cleanroom reference for correctness")

## Context

Until now the project's guidance called Ghidra a correctness oracle: ADR 0005
described it as "an indispensable oracle for sleigh semantics", ADR 0007 as "a
cleanroom reference for correctness", and `AGENTS.md` let it be cited as "a
cleanroom correctness/reference oracle".

Ghidra's own SLEIGH specifications have been wrong, and Fission had shipped the
errors because it lifts from those specifications:

- **AArch64 FCVT** (#834): upstream gives every FCVT{A,M,N,P,Z}{S,U} the body
  `Rd = trunc(Rn)`, so `fcvtms`, `fcvtps`, `fcvtns` and `fcvtas` lift as
  `fcvtzs`, rounding toward zero whatever the instruction says.
- **AArch64 FRINT** (#838): upstream gives all seven FRINT{A,I,M,N,P,X,Z}
  modes the body `Rd = trunc(Rn)`. FRINT produces an integral *float*, but
  `trunc` is a float-to-integer conversion, so every mode wrote integer bits
  into a floating-point register.

The FCVT error surfaced through a bug report against another decompiler built
on the same SLEIGH specs. The FRINT error was found while fixing it. Both
were confirmed against the Arm architecture manual. Comparing with Ghidra
could not have found them: Fission compiles Ghidra's `.sla` files, so Fission and Ghidra
produce the same wrong p-code and agree. The same holds downstream. Fission's
emulator and decompiler share that p-code, which is why #992 adds native
execution as an observation independent of the shared lift.

An oracle has to be able to disagree with Fission when Fission is wrong. For any
error that comes from the shared SLEIGH input, Ghidra cannot.

## Decision

1. **Ghidra is not a correctness oracle for anything Fission produces**: lifted
   p-code, emulation results, decompiled code, types, function boundaries, or
   metadata. "Ghidra agrees" is not evidence that Fission is right. "Ghidra
   differs" is a lead to investigate, not a verdict that Fission is wrong.

2. **Correctness is decided by evidence that does not share Fission's inputs.**
   In order of strength:
   1. **Native execution**: the instruction or binary run on real hardware,
      e.g. #992's native replay and the GNU fixtures in `dir-verify`.
   2. **Architecture manuals**: Intel SDM, AMD APM, the Arm Architecture
      Reference Manual (its pseudocode), and the equivalent manuals for other ISAs.
   3. **Source-level ground truth**: DWARF, PDB, and the published source CFGs
      that are DecBench's answer key.
   4. **Independent implementations**: for example, z3 for the solver
      (`tests/differential_z3.rs`). The emulator's JIT-versus-interpreter
      comparison checks only execution; both engines run the same p-code.

   Ghidra stays a **reference**: for algorithms, file formats, invariants and
   edge cases worth testing, and as a comparison that points to where to look.

3. **Upstream SLEIGH specifications are input data that can be wrong.** Fixing
   one is allowed, and is done as the FCVT and FRINT fixes were:
   - Evidence from (2.1) or (2.2), cited in the commit message.
   - A re-apply script under `scripts/sleigh/` for the next Ghidra upgrade.
   - A control: the unmodified spec recompiles to the checked-in `.sla` byte
     for byte, so the patch is the only difference.
   - A test that fails with the upstream `.sla` restored.
   - An entry under "Local deviations from upstream" in `utils/sleigh-specs/README.md`.

4. **"Raw p-code parity" means parity with Fission's previous output.** When a
   SLEIGH change is validated by row-level p-code comparison, the comparison
   detects what moved. Every intended difference needs evidence from (2.1) or
   (2.2), whether or not it moves Fission toward Ghidra or away from it.

## Consequences

- Ghidra parity audits (`docs/architecture/GHIDRA_PARITY_GAP_AUDIT.md`,
  `scripts/audit/ghidra_parity_audit.py`) stay reporting-only. A gap they
  report is a candidate, not a defect.
- A review or AI prompt may cite Ghidra's behaviour as a hint. It cannot cite it
  as the reason a change is correct.
- Agreement checks between Fission paths (emulator against decompiler, PreHIR
  against HIR) share the lift. They need native execution before they can
  rule out a SLEIGH error (#992).
- The other upstream specifications have not been audited against the manuals.
  The FCVT/FRINT pattern, where one body is shared by modes that should
  differ, is a place to look first.
