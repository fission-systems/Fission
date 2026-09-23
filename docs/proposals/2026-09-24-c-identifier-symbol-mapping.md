# C Identifier Mapping for Linker Symbols

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O1.exe`
- Function: `__mingwthr_run_key_dtors.part.0`
- Address: `0x1400021a0`
- Corpus row or benchmark command: real-binary project render via `fission_cli decomp <binary> --project --no-header --no-warnings --no-db`
- Current output summary: the project prelude and caller use `__mingwthr_run_key_dtors_part_0`, while the function definition uses the raw linker spelling `__mingwthr_run_key_dtors.part.0`.
- Semantic cases passed / total: not applicable; this is a C translation-unit syntax/identity defect, not a semantic benchmark change.
- Failure category: render output contains a non-C function identifier and does not match the address-resolved call-target spelling.
- Relevant compiler observation: `clang -ferror-limit=0 -x c -std=gnu17 -fsyntax-only <project.c>` reports `expected ';' after top level declarator` at the dot in the function definition.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [x] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
The address-indexed CallTargetRef is the source of the caller's underscore
spelling. PreviewBuilder::build_hir currently assigns the function definition
the independent caller-provided loader name; that is the first owner of the
definition/call identity mismatch. The C printer then writes function and call
names verbatim, producing invalid C. In addition, sanitize_c_identifier
currently maps every forbidden character to `_` and explicitly permits
distinct symbols to collide.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Map each raw linker symbol exactly once when it enters C-facing HIR. The map
must be deterministic and injective; function definitions, calls, and
declarations for the same resolved symbol use the same mapped identifier.
Prepared-HIR renderers preserve mapped names, while standalone raw-HIR printer
helpers map their input once.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] The rule is not ISA-specific.
- [x] The rule is shared by all architectures and lives at the C-facing HIR boundary.
- [x] Synthetic tests cover identifier validity, collision resistance, and matching declarations/calls.

Comparable coverage:

- ELF versioned/imported symbols containing dots and `@`.
- COFF/compiler-local symbols containing dots or `$`.
- Synthetic pairs that previously collapsed to the same underscore spelling.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `sanitize_c_identifier` in `fission-midend-core` already maps global and declared symbols, but was lossy; `PreviewBuilder::build_hir` has the address-indexed `CallTargetRef` but did not use it for the definition name. Call-target and relocation paths also need to map raw symbols before they become HIR.
- Shared analysis/substrate candidate: none; this is a C output spelling rule plus address-based symbol identity already available in `CallTargetRef`.
- Why extending that owner is sufficient: make the existing helper injective, map raw symbol names as they enter C-facing HIR, and source the emitted definition name from the same address-indexed `CallTargetRef` used by calls. The integrated printer then preserves prepared names instead of applying the encoding a second time. No new semantic pass or cross-crate dependency is needed.
- Possible interaction with existing normalize/structuring/materialize passes: none; only the final function name presented to rendering and emitted C tokens change.
- New or changed owner-to-owner dependency: none.
- Telemetry impact: none.
- Known cases that must not change: ordinary valid C identifiers remain byte-for-byte unchanged; indirect-call expressions and synthetic p-code helper spellings retain their existing behavior.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: focused `fission-midend-core` identifier tests and `fission-pcode` render/builder tests.
  - Expected signal: invalid spellings become valid C identifiers; distinct raw spellings remain distinct; definition, call, and extern use one resolved spelling.
- [ ] Full crate-level gate:
  - Command: `cargo nextest run -p fission-pcode --no-fail-fast`.
  - Result: 1126 passed, 3 failed, 1 skipped. The failures are the recurring `diamond_join_lowers_copy_through_join_read_as_select`, `movzx_after_byte_add_zero_extends_unsigned`, and `x64_byte_add_movzx_does_not_double_add_load`; the first two were independently reproduced before this patch, while the third remains unverified against an untouched checkout.
- [x] Compile gates:
  - Commands: `cargo check -p fission-pcode`, `cargo check -p fission-decompiler`, `cargo fmt --all --check`, `git diff --check`.
  - Result: passed.
- [x] Focused real-binary check:
  - Command: re-run the project render for the baseline PE and inspect the target prototype, definition, and call; run focused Clang parsing of the target function surface.
  - Result: the same mapped identifier occurs in the prototype, definition, and both call sites (four occurrences); the raw dotted symbol and double-encoded spelling are absent. A declaration/definition/self-call excerpt using the emitted identifier passes `clang -std=c11 -Werror -fsyntax-only`. The full project still has unrelated compile diagnostics, so no whole-project compile claim is made.
- [x] Smoke or automation sample:
  - Command: focused project-render test and CLI release build.
  - Result: release `fission_cli` build passed and project render recognizes the same mapped definition/call identity without a duplicate target extern.
- [x] Optional related checks:
  - Command: `cargo fmt --all --check` and `git diff --check`.
  - Expected signal: formatting and whitespace gates pass.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Information exposed in the AI prompt:
  - [x] Not applicable
- Redaction confirmed:
  - [x] Not applicable
- Ghidra guidance confirmed:
  - [x] Not applicable
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not applicable; no semantic quality claim.
  - Synthetic invariant test: valid-identifier and injective-mapping cases.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed; this is an output-validity fix.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the existing C identifier helper is extended and used consistently.
