# Decompiler Change Proposal: Preserve partial-register zero extension

## 1. Baseline Row Anchor

- Binary: local dev-corpus 32-bit optimized control-flow sample
- Function: `checksum`
- Address: resolved from the corpus manifest at run time
- Corpus row or benchmark command: focused local Docker run with the Fission adapter
- Current output summary: a byte-lane sum widened into another register becomes an
  identity copy; generated C can use the untruncated wide sum as an index.
- Semantic cases passed / total: 3 / 5
- Failure category: runtime error
- Relevant observation: two synthetic register-transfer shapes already reproduce
  the same loss without a corpus binary or compiler tuple.

## 2. Owner Proof

- [ ] Builder/materialize
- [ ] SLEIGH/raw p-code
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
P-code: INT_ADD wide_dst, ...; INT_ZEXT other_wide_dst <- low_byte(wide_dst)
Expected HIR: other_wide_dst = (wide_dst & 0xff)
Current HIR/C: other_wide_dst = wide_dst

Builder materialization initially emits `dst = ((u8)src) & 0xff`. The
`bit_consume` normalize pass then rewrites the remaining narrowing cast to an
identity copy because it obtains `Unknown` from `expr_type(Var)` and treats that
as a full-width source mask. The first wrong fact is created in normalize.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
INT_ZEXT copies exactly the unsigned bit-vector represented by its input
varnode. When that input is a strict subrange of a wider register family, the
source-width projection must be represented before widening, regardless of
whether the destination aliases the same hardware register family or another
one.
```

ISA-agnostic check:

- [x] The condition is based on varnode space/range/width and opcode semantics.
- [x] No compiler tuple, function identity, address, or corpus identity is used.
- [x] Synthetic tests describe only p-code dataflow and register-lane shape.

Comparable coverage:

- Similar shape 1: low byte of an arithmetic result zero-extended into a
  different 32-bit register, then returned.
- Similar shape 2: the same result widened again and used in pointer arithmetic.
- Synthetic invariant tests:
  `movzx_al_into_edx_after_int_add_preserves_low_byte_truncation` and
  `rc4_keystream_movzx_sequence_truncates_index`.

## 4. Risk And Ownership Check

- Existing owner: normalize global bit-consumption analysis.
- Shared analysis/substrate candidate:
  - [x] P-code semantic contract
- Why extending that owner is sufficient: parameter/local binding types already
  provide the source width needed to distinguish narrowing casts from removable
  widening casts; no new recovery pass is required.
- New pass/helper: none planned.
- Possible interaction: cast/mask normalization must not erase a semantically
  required source-lane projection merely because the destination is wider.
- New owner-to-owner dependency:
  - [x] None
- Telemetry impact: none.
- Known cases that must not change: ordinary full-register widening, signed
  extension, and same-width copies.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: `cargo nextest run -p fission-pcode -E 'test(movzx_al_into_edx_after_int_add_preserves_low_byte_truncation) | test(rc4_keystream_movzx_sequence_truncates_index)'`
  - Expected signal: destination retains low-byte truncation.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode --no-fail-fast`
  - Result: 1,156 passed, 5 known failures, 9 skipped; no new failure. The two
    low-byte synthetic failures in the prior seven-failure baseline now pass.
- [x] Focused benchmark row:
  - Command: local Docker Fission-only dev run.
  - Result: 32-bit optimized row returns all 5 / 5 semantic cases.
- [x] Smoke or automation sample:
  - Command: `cargo check -p fission-pcode`
  - Result: successful check.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Result: 5 existing violations and 47 migration findings; no new dependency
    violation from this change.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] Yes; the operator supplied row identity in the existing task context.
- Information used to derive the production condition:
  - [x] Structural failure pattern only
  - [x] Owner evidence only
  - [x] Invariant candidates only
  - [x] Validation matrix only
- Redaction note: row identity is retained only in this required baseline anchor;
  production code and synthetic tests do not branch on it.
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Synthetic tests listed above; patch validation pool remains a later gate.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; no new pass/helper is proposed
