# Decompiler Change Proposal: PE Image-Base API Signatures

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O1.exe`
- Function: `_ValidateImageBase`, `_FindPESection`
- Address: `0x140002410`, `0x140002440`
- Corpus row or benchmark command: Real binary, focused `fission_cli decomp ... --layer hir` runs; these CRT helper functions are not in the source-semantic source manifest, so no per-function DecBench score or semantic case count is available.
- Current output summary: Both `--db` and `--no-db` render `_ValidateImageBase(int pImageBase)` and `_FindPESection(int pImageBase, int rva)`. Their bodies dereference/index `pImageBase`; pre-HIR reports pointer-like parameters (`ushort *` and `int *` respectively), so the scalar surface type is introduced later by an authoritative function hint. Correcting only the hint to `PBYTE` makes the renderer synthesize conflicting per-function `PBYTE` definitions from those different access widths; the project unit then selects one width for both functions, changing the other's address arithmetic.
- Semantic cases passed / total: N/A — no source-ground-truth cases for these runtime-generated helpers.
- Failure category: Incorrect authoritative API signature data / type recovery.
- Relevant benchmark/static/readability observations: The binary is x86-64 COFF and has no debug directory. Disassembly reads PE headers through RCX as a base pointer; `_FindPESection` uses RDX as an RVA. `--no-db` does not change the rendered types. The existing packed WinAPI table and its local source text agree exactly and both contain scalar prototypes. Microsoft CRT declarations give `_ValidateImageBase(PBYTE) -> BOOL` and `_FindPESection(PBYTE, DWORD_PTR) -> PIMAGE_SECTION_HEADER`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [x] Type/data recovery:
- [x] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
utils/source/typeinfo/win_api_signatures.txt:
_FindPESection|int|pImageBase:int,rva:int
_ValidateImageBase|int|pImageBase:int

The packed table is byte-for-byte sourced from this local text table. Function
hint diagnostics report explicit parameter and return type hits for both
functions. Pre-HIR has pointer-like parameter types; the signature-derived
surface types are authoritative and replace those with `int`. Replacing the
scalar rows with exact `PBYTE` spellings alone is insufficient: the current
project printer fabricates `PBYTE` from the inferred pointee, and the two
functions infer different widths.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
An authoritative API signature must express the callee's documented ABI
contract. For these CRT helpers, the image base is a byte pointer, the section
lookup result is a section-header pointer, and the RVA is pointer-width
unsigned. Represent the image-base surface as portable `unsigned char *` (the
underlying type of `PBYTE`) so one function's access width cannot redefine a
shared alias. When a byte-pointer declaration carries a wider recovered load or
subscript, render an explicit cast to that access type so the machine byte
offset and load width remain unchanged. Existing type-hint and callsite
propagation then carries one stable pointer contract without a binary- or
address-specific inference rule.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production conditions depend only on declared pointee width versus recovered access width.
- [x] No ISA-specific control-structure or type-inference rule is added.
- [x] Focused resource assertions validate the signature records independently of a compiler tuple.

Comparable coverage:

- Similar shape 1: CRT code passes `PBYTE` image bases to `_ValidateImageBase`.
- Similar shape 2: CRT code passes `PBYTE` plus `DWORD_PTR` RVA to `_FindPESection` and consumes a `PIMAGE_SECTION_HEADER` result.
- Synthetic invariant test: Assert both complete API records, `unsigned char *` to NIR pointer mapping, and explicit-width casts for byte-pointer subscripts.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: WinAPI/CRT API signature resource supplies the incorrect scalar contract. The printer already handles pointee-width mismatches for loads/dereferences, but not scalar subscripts; extend that existing access-width contract rather than adding a pass.
- Shared analysis/substrate candidate:
  - [x] Type constraint / calling-convention fact
- Why extending that owner is sufficient: Correct API records restore a byte-pointer contract. The printer's existing scalar access-width check already names the invariant; applying it to subscripts prevents C pointer scale from changing recovered byte addresses.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: N/A; extend an existing printer access-width check, with no new pass/helper/metric.
- Possible interaction with existing normalize/structuring/materialize passes: Callsite type propagation gains an unsigned-byte pointer type. NIR control flow and instruction semantics must remain unchanged; HIR must cast wider accesses when necessary.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: None.
- Known cases that must not change: Unrelated API signatures and body-derived types; no inference or address-specific fallback should be introduced.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: `cargo nextest run -p fission-signatures msvc_pe_image_helpers_keep_their_pointer_contracts`; `cargo nextest run -p fission-midend-normalize unsigned_char_pointer_type`; `cargo nextest run -p fission-pcode byte_pointer_subscripts_preserve_wide_access_width`
  - Expected signal: The shipped records expose the corrected prototypes, the byte-pointer spelling maps to the expected 8-bit pointee, and wider indexed accesses retain their machine stride.
- [ ] Crate-level gate (not fully green; see observed result):
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1,118 passed, 3 failed, 1 skipped. The failures are three unchanged
    `midend::builder::expr::lower_expr` tests; two were reproduced on the clean
    pre-change base commit (`diamond_join_lowers_copy_through_join_read_as_select`
    and `movzx_after_byte_add_zero_extends_unsigned`). The third,
    `x64_byte_add_movzx_does_not_double_add_load`, is in the same untouched
    builder test module but was not independently rerun on the base commit.
  - Expected signal: No failures attributable to API type mapping or HIR access-width rendering.
- [x] Focused benchmark row:
  - Command: Focused real-binary decompilation of both addresses before and after; no source-semantic row exists for these CRT helpers.
  - Expected row-level improvement: Both function surfaces and shared declarations/callers use pointer-correct prototypes; no DecBench score is claimed.
- [x] Smoke or automation sample:
  - Command: Release CLI decompilation of both functions and `--project` output; compare issue-related clang diagnostics with the baseline whole-project compile.
  - Result: Both prototypes and definitions agree on the byte-pointer contract;
    wider indexed accesses retain explicit typed casts. The whole-TU syntax
    check still reports unrelated existing failures, including an aggregate
    field reconstruction error in `_FindPESection`; the issue's integer
    indirection/subscript diagnostics are absent. Whole-project compilation is
    not claimed fixed.
- [x] Optional related checks:
  - Command: `cargo nextest run -p fission-signatures`, `cargo check -p fission-pcode`, `cargo check -p fission-decompiler`, `cargo build -p fission-cli --release`, `cargo fmt --all --check`, `git diff --check`.
  - Result: fission-signatures 88/88 and fission-midend-normalize 429/429;
    both cargo checks, release CLI build, formatting, and diff checks pass.
  - Expected signal: Resource, type-flow, CLI, and formatting gates pass.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Information exposed in the AI prompt: N/A.
- Redaction confirmed: N/A.
- Ghidra guidance confirmed: N/A.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: Not applicable to correcting documented CRT API records.
  - Synthetic invariant test: Resource, type-conversion, and printer assertions cover the API/access-width contracts.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed; the change is limited to canonical type facts, generic type conversion, and a width-consistency rule in the existing printer owner.
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed; it will be reported as focused real-binary type-correctness evidence, with no DecBench score claim.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] No new metric/pass/helper; the existing scalar access-width check is reused for subscript emission.
