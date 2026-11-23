# Decompiler Change Proposal: an integer formal is declared at the width of its home slot

## 1. Baseline Row Anchor

- Binary: DecBench `O0/bash/bash` (stripped copy), also `O0/bzip2/bzip2`, `O0/bash/man2html`
- Function: `invis_addc` (`char c`, `char face`), `bsPutUChar` (`UChar c`), `mmed3` (`UChar a, b, c`), `trans_char` (`char s, t`)
- Address: `0x10e518` (bash O0), `0xb46f`, `0x19526` (bzip2 O0), `0x7246` (man2html O0)
- Corpus row or benchmark command: `~/fission-benchmark/decbench-full/sweep.py run --config full --workers 2`, baseline run `main-1f10b8c78`
- Current output summary: `void sub_10e518(int * ptr, uint param_2, uint param_3)` with `uchar param_2_sub8 = (uchar)param_2;` and `(uchar)param_3` at every read; `invis_adds` spills `local_28 = (uchar)param_4`.
- Failure category: type recovery -- an 8/16-bit formal declared at its 32/64-bit register width.
- Observations: per-variable decomposition of the full-dataset baseline (DecBench `TypeMatchMetric`, legacy-structured correspondence replayed exactly, 0 score mismatches on 86,937 rows): narrow ground-truth integers (`char`/`short`/`bool`) recovered as 32/64-bit formals are ~9,600 variables in 20+ projects, ~3,800 of them `-O0` arguments.

## 2. Owner Proof

- [x] Type/data recovery: `fission-midend-normalize` `types/use_type_infer` (use-driven type fixed point), next to `narrow_integer_params_from_wrapping_return_uses`, the existing owner of narrowing formals from their uses.

Evidence: the builder already lowers the spill at its true width (`local_28 = (uchar)param_4`, a `uchar` frame slot); only the formal's binding keeps the register width. No pass consumed the slot width as declaration evidence.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Demanded-bits analysis over the whole function: a consumer demands the low N
bits through an explicit integer cast to N, an N-bit store, or an assignment
into an N-bit frame slot (StackOffset/HomeSlot origin). Demand flows back
unchanged through + - * & | ^, negation, complement, select arms, and
assignments into other names; every other consumer demands all bits. A
formal that is never assigned, whose widest demand is N < its width, and
whose widest demand is witnessed by a frame slot it is stored into, is
declared N bits wide. Sound: no consumer reads more than N bits, so the
narrower declaration reads the same bits; the home slot is the
declaration-width witness.
```

Why the slot witness is required (measured): a use-only first version
narrowed `int c` stored as a byte, `int last` read as a halfword, and zlib's
`uLong crc1` read as 32 bits. On a 3,652-row slice (gzip, bzip2, zlib,
base-passwd, sed, dash; all three opt configs) it flipped 91 correct
variables to wrong and fixed 6. With the witness: 5 fixed, 0 flipped.

ISA-agnostic check:

- [x] Not gated on a calling convention or ISA: reads PreHIR casts, stores and binding origins only.
- [x] No ISA data involved.
- [x] Synthetic tests state the dataflow shape only.

Comparable coverage: bash O0 `invis_addc`/`invis_adds`, man2html O0 `trans_char`, bzip2 O0 `bsPutUChar`/`mmed3`/`BZ2_compressBlock`.

## 4. Risk And Ownership Check

- Existing owner: `use_promotions.rs` (`narrow_integer_params_from_wrapping_return_uses`). Extended in place; no new pass registration.
- Substrate: def-use / type constraint.
- Interaction: runs inside `apply_use_driven_type_infer_pass`, changes only binding `.ty` (the loop's contract). `promote_store_value_only_unsigned_params` only touches 32-bit formals and is unaffected.
- `-O2` formals live in registers with no home slot, so `-O2` output is unchanged by design (an `int` and a `char` used as a byte are the same machine code there).
- Telemetry: none.

## 5. Validation Matrix

- [x] Targeted tests: `cargo nextest run -p fission-midend-normalize use_type_infer` -- 8 new tests; the 3 positive ones fail with the call removed.
- [x] Crate gates: `cargo nextest run -p fission-midend-normalize`, `cargo nextest run -p fission-pcode`.
- [x] Full-dataset sweep before/after (see PR body).

## 6. AI Review / Prompt Firewall

- [x] No other model consulted.

## 7. Review Notes

- [x] No hardcoded binary/function/address/corpus guards.
- [x] No metric-only edits.
- [x] Extends the existing owner.
