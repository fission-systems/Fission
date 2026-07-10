# Decompiler Change Proposal: Keep recovered low-byte masks unsigned

## 1. Baseline Row Anchor

- Binary: local dev-corpus 32-bit optimized control-flow sample
- Function: `checksum`
- Address: resolved from the corpus manifest at run time
- Current output summary: a recovered `0xff` truncation mask inherits an i8
  expression type and prints as `-1`, turning truncation into an identity.
- Semantic cases passed / total: 3 / 5
- Failure category: runtime error

## 2. Owner Proof

- [x] Normalize cleanup: `byte_sum_index_trunc`

Evidence:

```text
Before pass: eax = al
After pass:  eax = al & Const(255, i8)
Printed C:   eax = al & -1
```

## 3. Generality / Invariant Proof

```text
A low-bit retention mask is an unsigned bit-vector operation. Its HIR constant
and result type must be unsigned even when the producer expression has a signed
type of the same width.
```

- [x] Rule depends only on integer width and bitmask semantics.
- [x] No function, address, binary, compiler, or ISA condition.
- Synthetic coverage: signed byte producer copied into an unsigned destination.

## 4. Risk And Ownership Check

- Existing owner: the pass that introduces the mask.
- New pass/helper: none; one owner-local type-selection helper.
- Risk: changing mask width can alter later inference; preserve destination width
  when known and only force unsigned signedness.
- New owner dependency: none.

## 5. Validation Matrix

- [x] Existing byte-sum truncation test.
- [x] Signed-producer/unsigned-mask invariant test.
- [x] Partial-register zero-extension synthetic tests.
- [x] `cargo check -p fission-pcode`.
- [x] Full crate test baseline comparison: 1,156 passed, 5 known failures,
  9 skipped; no new failure.
- [x] Focused local Docker semantic verification: 32-bit optimized row
  improved from 3 / 5 to 5 / 5 and prints `& 255`.

## 6. AI Review / Prompt Firewall

- Row identity exists only in this required baseline anchor.
- Production rule uses bit-vector type semantics only.
- No reference-output style is copied.

## 7. Review Notes

- [x] No hardcoded row identity in production code.
- [x] Existing owner is extended; no duplicate pass.
- [x] Semantic claim requires execution verification.
