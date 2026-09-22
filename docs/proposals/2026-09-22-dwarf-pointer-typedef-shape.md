# Proposal: Preserve DWARF pointer typedef shape in project output

## 1. Baseline Row Anchor

- Binary: `advanced_patterns_gcc_O1.exe`
- Function: `_FindPESectionByName`
- Address: `0x140002490`
- Corpus row or benchmark command: local `fission_cli decomp --project` run on the
  PE dev binary; issue #80
- Current output summary: the project prelude emits
  `typedef unsigned long long PIMAGE_SECTION_HEADER;`, while functions whose
  DWARF return type is that alias are declared with the integer alias.
- Semantic cases passed / total: not a source-semantic scored row; this is a
  project-output declaration/compilability defect.
- Failure category: debug type/data recovery loses a pointer layer at the
  loader-to-NIR type-context boundary.
- Relevant benchmark/static/readability observations: `llvm-dwarfdump` reports
  `PIMAGE_SECTION_HEADER` as a typedef to `_IMAGE_SECTION_HEADER *`; the same
  alias is used as the return type of `_FindPESectionByName`,
  `_FindPESectionExec`, and `__mingw_GetSectionForAddress`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
DWARF: PIMAGE_SECTION_HEADER -> _IMAGE_SECTION_HEADER *
NIR preview before the change: return type is the pointer-width integer carrier
and surface_return_type_name is PIMAGE_SECTION_HEADER.
The layered printer therefore derives:
typedef unsigned long long PIMAGE_SECTION_HEADER;
```

The first wrong fact is the absent pointer shape in the type context, not the
printer's formatting of an already-resolved pointer.

## 3. Generality / Invariant Proof

Generalized rule:

```text
When debug metadata names a typedef whose target contains one or more ordinary
pointer DIEs, preserve the pointee name and pointer depth in the shared type
context. Apply that fact to a matching source-level return or binding type when
the recovered machine carrier is scalar or less informative. Do not infer
pointer-ness from alias spelling.
```

ISA-agnostic check (ADR 0009):

- [x] The rule is based on debug type shape, not an ISA, address, function name,
  or compiler tuple.
- [x] Target-specific representation remains in loader/DWARF metadata; the
  type-hint owner consumes a shared pointer-depth fact.
- [x] The synthetic test uses only a typedef target and a machine-width carrier.

Comparable coverage:

- Similar shape 1: any DWARF/PDB typedef whose target is `T *` and whose
  machine-level carrier is an integer-width value.
- Similar shape 2: nested ordinary pointer typedefs (`T **`) with preserved
  pointer depth.
- Synthetic invariant test: `preview_type_hints_restore_debug_pointer_typedef_shape`.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `apply_preview_type_hints`
  already applies debug function/parameter/return type names and aggregate
  promotions; the loader already owns DWARF type extraction.
- Shared analysis/substrate candidate:
  - [x] Type constraint / calling-convention fact
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: a small typed loader fact carries the
  missing shape to the existing type-hint stage, and the existing layered
  renderer already emits pointer aliases from resolved `NirType::Ptr` values.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no new pass is added; the helper extends the existing preview
  type-hint application.
- Possible interaction with existing normalize/structuring/materialize passes:
  only the recovered source type of debug-named bindings changes; p-code/NIR
  value semantics and control flow remain unchanged.
- New or changed owner-to-owner dependency:
  - [x] Existing migration debt only
- Telemetry impact, if any: none.
- Known cases that must not change: function-pointer typedefs remain owned by the
  existing callable-typedef path; aliases without a pointer target remain
  scalar/opaque; an already informative aggregate pointer is not replaced.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode preview_type_hints_restore_debug_pointer_typedef_shape`
  - Expected signal: resolved return/local types are `Ptr(Aggregate)`.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the three pre-existing failures
    recorded for this workspace.
- [x] Focused benchmark row:
  - Command: release `fission_cli decomp --project` on the anchored PE binary,
    with a fresh output directory.
  - Expected row-level improvement: `PIMAGE_SECTION_HEADER` is emitted as a
    pointer-preserving alias instead of an integer alias.
- [x] Smoke or automation sample:
  - Command: `cargo nextest run -p fission-decompiler`, `cargo check`, and a
    release CLI build.
  - Expected no-regression signal: all commands pass.
- [x] Optional related checks:
  - Command: `cargo fmt --all --check` and `git diff --check`.
  - Expected signal: clean formatting and patch.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass or dependency.
  - Expected signal: not applicable.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: not applicable.
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed: not applicable.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not run for this declaration-only fix.
  - Synthetic invariant test command/result: recorded after implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed; the measured claim is limited to the anchored project-output
    declaration defect.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the helper extends existing type-hint application.
