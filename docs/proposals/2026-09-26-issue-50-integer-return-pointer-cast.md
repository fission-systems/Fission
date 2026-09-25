# Issue #50: integer return keeps an incompatible pointer cast

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O1.exe` (SHA-256 `55c57ee4705adcd8d470029b6cc0aab2d3d99bcf599f0c43b822a0f57bf95a03`)
- Function: `main`
- Address: `0x140001664`
- Corpus row or benchmark command: focused real-binary repro with `/tmp/fission-50-target/release/fission_cli decomp <binary> --addr 0x140001664 --layer {nir,hir} --no-header --no-warnings`; source is `fission-benchmark/corpus/dev/source/c/advanced_patterns.c:main`.
- Current output summary: baseline NIR and HIR declare `int main(void)` and emit `return (void *)((uchar)(rax ^ rbx));`. The source returns `r & 0xff`. The same pointer cast is already present in the builder's PreHIR output.
- Semantic cases passed / total: not applicable. This checkout has no `benchmark/source_semantic_benchmark` directory or source-semantic row manifest; the focused issue binary is in the external dev corpus.
- Failure category: incompatible return expression and stale internal return type. The actual emitted `main` body, with the separate malformed `table = 1;` statement removed and test-only declarations supplied, fails C11 compilation with both `-Wint-to-void-pointer-cast` and `-Wint-conversion` at the return statement.
- Relevant benchmark/static/readability observations: raw p-code performs a 32-bit `IntXor`, zero-extends the result, and returns it; the corresponding disassembly ends in `xorl %ebx,%eax`, `movzbl %al,%eax`, `retq`. No readability score is claimed.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence: the raw p-code and machine instructions return an integer. The builder's PreHIR already contains `return (void *)((uchar)xVar5);`, before final NIR/HIR printing. The existing `apply_preview_type_hints` path receives the explicit surface return type `int`, but currently updates only the printed surface name; the internal `HirFunction.return_type` remains a pointer. Its cast-elision helper also leaves the outer pointer cast in place. A targeted regression first caught that removing the expression cast alone still makes the printer re-wrap the return using the stale internal pointer type. The canonical owner is the builder's function-hint reconciliation, which must align the internal return type and expression with the trusted integer signature.

```text
PreHIR: void * main(void) { ... return (void *)((uchar)xVar5); }
NIR/HIR: int main(void) { ... return (void *)((uchar)(rax ^ rbx)); }
Raw p-code: IntXor (4 bytes) -> IntZExt (8 bytes) -> Return
C11: cast from uchar to void *; incompatible pointer-to-int return
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When an explicit integer function-return hint is available, align the internal
HIR return type with that integer type. An outer pointer cast around an
expression whose recorded type is integer cannot satisfy the return contract:
remove only that incompatible outer pointer cast and preserve any inner
integer-width or signedness casts. Do not remove a pointer cast when its
operand is pointer-typed or its type is unknown, and do not run this rule for
pointer-returning signatures.
```

ISA-agnostic check (ADR 0009):

- [x] The condition uses the shared expression type, not an ISA, register, or ABI enum.
- [x] No architecture-specific copy of the return rule is introduced.
- [x] The synthetic test describes a typed integer temporary beneath a pointer cast.

Comparable coverage:

- Similar shape 1: an integer return with a narrowing integer cast beneath an incompatible outer pointer cast; preserve the narrowing cast.
- Similar shape 2: a pointer return under an explicit pointer signature; preserve its pointer type and cast behavior from the pointer-return contract in `docs/proposals/2026-09-21-typed-pointer-return-contract.md`.
- Synthetic invariant test: an integer function returns an integer temporary through a pointer-shaped outer cast; a control case keeps a pointer-valued return under a pointer signature.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `builder/type_hints.rs::elide_surface_return_casts`, which already reconciles explicit integer return hints with outer scalar casts.
- Shared analysis/substrate candidate: existing typed `HirExpr` facts; no new fact or map is needed.
- Why extending that owner is sufficient: the authoritative return hint is applied there, and the immediate operand type distinguishes an integer conversion from a pointer-valued result. The bug is visible before rendering, and both output layers share the same HIR.
- Possible interaction with existing normalize/structuring/materialize passes: normalization runs before the surface function hint is applied. The change remains local to the existing hint reconciliation helper and does not alter CFG or dataflow behavior.
- New or changed owner-to-owner dependency: none.
- Telemetry impact: none.
- Known cases that must not change: pointer-return signatures; pointer-typed or unknown-typed operands; inner scalar casts that preserve the integer's width or signedness; the existing behavior for pointer values returned through representation casts.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode preview_type_hints_elide_incompatible_pointer_return_cast`
  - Result: passed; internal return type becomes signed 32-bit integer, the outer pointer cast is removed, the inner 8-bit cast remains, and the pointer-return control preserves its cast.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode --no-fail-fast`
  - Result: 1133 passed, 3 failed, 1 skipped. The same three `lower_expr` tests failed on the unchanged base commit (1132 passed, 3 failed, 1 skipped), so this change adds no crate-level regression.
- [x] Focused benchmark row:
  - Command: release CLI decompilation of the binary above for both `nir` and `hir`, followed by C11 syntax checking of the emitted `main` body with only the unrelated `table = 1;` statement omitted from the compile probe.
  - Result: both layers emit `return (uchar)(rax ^ rbx);`; both extracted C11 probes pass with `-Werror=int-to-pointer-cast -Werror=int-conversion`. The baseline probe emitted both diagnostics.
- [x] Smoke or automation sample:
  - Command: compile the same C source as a local x86-64 ELF and decompile its `main` through the release CLI.
  - Result: the fresh ELF decompiles in NIR and HIR with an integer-compatible return.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode`, `cargo check -p fission-decompiler`, `cargo fmt --all --check`.
-  - Result: all pass. The release CLI build and `git diff --check` also pass.
- [ ] Boundary audit, if a new pass/helper/dependency was added: not applicable; no new pass, helper, or dependency is planned.

The user removed Docker, so the external Docker benchmark runner is unavailable. No external DecBench score or broad quality claim is planned; validation for this return-type defect is the actual binary output plus the focused generated-C compiler check.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed.
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the change extends existing return-hint reconciliation.
