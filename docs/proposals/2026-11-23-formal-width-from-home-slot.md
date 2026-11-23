# Decompiler Change Proposal: an integer formal is declared at the width of its home slot

## 1. Baseline Row Anchor

- Binaries (DecBench, stripped copies): `O0/bash/bash`, `O0/bzip2/bzip2`, `O0/openssh-portable/scp`, `O0/libselinux/libselinux.so`; and at `-O2`: `O2/betaflight/betaflight_STM32F405`, `O2/crazyflie/cf2`, `O2-noinline/cleanflight/cleanflight_DALRCF405`
- Functions: `invis_addc` (`char c, char face`, bash O0 `0x10e518`), `bsPutUChar` (`UChar c`, bzip2 O0 `0xb46f`), `sshbuf_put_u8` (`u_char val`, scp O0 `0x206b6`), `avc_claim_node` (`security_class_t tclass`); `-O2`: `spiWriteReg(dev, uint8_t reg, uint8_t data)` (betaflight `0x8022914`), `passthroughVcpTxSend(uint8_t c)` (crazyflie `0x802c408`), `VL53L1_WrByte(..., uint8_t VL53L1_PRIVATE_data)` (crazyflie `0x8014b68`), `i2cWrite` (cleanflight `0x800a888`)
- Corpus row or benchmark command: `~/fission-benchmark/decbench-full/sweep.py run --config full --workers 2`, baseline run `main-1f10b8c78`
- Current output summary: `void sub_10e518(int * ptr, uint param_2, uint param_3)` with `uchar param_2_sub8 = (uchar)param_2;` and `(uchar)param_3` at every read; `-O2` `int sub_802c408(uint param_1)` whose argument is spilled to a byte slot whose address is passed on.
- Failure category: type recovery -- an 8/16-bit formal declared at its 32/64-bit register width.
- Observations: per-variable decomposition of the full-dataset baseline (DecBench `TypeMatchMetric`, legacy-structured correspondence replayed exactly, 0 score mismatches on 86,937 rows): narrow ground-truth integers (`char`/`short`/`bool`) recovered as 32/64-bit formals are ~9,600 variables in 20+ projects, ~3,800 of them `-O0` arguments.

## 2. Owner Proof

- [x] Type/data recovery: `fission-midend-normalize` `types/use_type_infer` (use-driven type fixed point), next to `narrow_integer_params_from_wrapping_return_uses`, the existing owner of narrowing formals from their uses.

Evidence: the builder already lowers the spill at its true width (`local_28 = (uchar)param_4`, a `uchar` frame slot); only the formal's binding keeps the register width. No pass consumed the slot width as declaration evidence.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Home slot: a frame slot (StackOffset/HomeSlot integer binding) whose first
write in the straight-line entry is the formal itself, unmodified apart
from the truncation (`slot = formal` / `slot = (intN)formal`). Two homes of
one formal must agree. A slot that first receives a value derived from the
formal is an ordinary local.

Demanded bits: a consumer demands the low N bits through an explicit
integer cast to N, an N-bit store, or an assignment into an N-bit frame
slot. Demand flows back through + - & | ^, negation, complement, select arms
and assignments into the function's own locals. Every other consumer -- a
compare, a multiply, a shift, a call argument, an address, a return, a
write to a global (a Var lvalue that is not a local, or is in
global_storage_names) -- demands all bits.

A formal that is never assigned, whose widest demand is N < its width and
equals its home slot's width, is declared N bits wide -- unless N is 32 and
the formal feeds arithmetic (an `int` operand is not promoted, and the
machine's wider operation wrapped).
```

Why each condition (measured or reviewed):

- The home slot, not truncating reads: a use-only first version narrowed
  `int c` stored as a byte, `int last` read as a halfword, and zlib's `uLong
  crc1` read as 32 bits. On a 3,652-row slice (gzip, bzip2, zlib,
  base-passwd, sed, dash; all three opt configs) it flipped 91 correctly
  typed variables and fixed 6.
- The spill must be the formal itself (review): `ushort local =
  (ushort)(param + 1)` is a frame slot but not the formal's home.
- Globals demand all bits (review): a global write is a `Var` lvalue the
  function never reads, so it looked unobserved.
- No multiply (review): two `ushort` operands promote to signed `int` and
  can overflow where the unsigned 32-bit product wrapped.

ISA-agnostic check:

- [x] Not gated on a calling convention or ISA: reads PreHIR casts, stores and binding origins only. Fires on x86-64 (bash, bzip2, openssh, libselinux) and Thumb (crazyflie, betaflight, cleanflight).
- [x] No ISA data involved.
- [x] Synthetic tests state the dataflow shape only.

Comparable coverage (shape rule: >= 3 functions, >= 2 programs, mixed opt levels):

- `-O0`: every formal is spilled, so the shape is everywhere (1,619 changed rows; type-perfect +272).
- `-O2`/`-O2-noinline`: a formal is spilled only when its home is address-taken, which ARM firmware does for byte buffers handed to SPI/I2C/queue APIs. Measured changed rows: betaflight `spiWriteReg`, `spiWrite`, `spiReadReg`, `spiWriteRegBuf`, `i2cWrite`, `sx1280SetFifoAddr`, `timerNVICConfigure`; crazyflie `passthroughVcpTxSend`, `VL53L1_WrByte`, `registerWrite`, `xchgSpi`; cleanflight `i2cWrite`, `smartPortWriteFrameSerial` -- in both `O2` and `O2-noinline`. On x86-64 `-O2` arguments stay in registers, so the rule does not fire there; it cannot, because without a spill an `int` and a `char` read as a byte are the same machine code.

## 4. Risk And Ownership Check

- Existing owner: `use_promotions.rs`. Extended in place; no new pass registration.
- Substrate: def-use / type constraint.
- Interaction: runs inside `apply_use_driven_type_infer_pass`, changes only binding `.ty` (the loop's contract).
- Known divergences from the answer key (behavior-equivalent, not defects):
  - arm-none-eabi uses short enums: a 1-byte enum formal (`FunctionalState NewState`, `lighthouseBaseStationType_t`) now reads `uchar`, its real storage, while DWARF-to-`int` mapping in the key expects `int` (106 of the 109 rows that lost type score, all crazyflie; every flipped variable is an enum: `FunctionalState` 46, `lighthouseBaseStationType_t`, `supervisorState_t`, `eTaskState`, ...).
  - `memoutstream_putc(..., int ch)` in nuttx `-O2` copies `ch` into a `char` local whose address is passed on; at `-O2` that copy is the same shape as a spill (2 rows).
- One correctness gain seen in review: crazyflie `-O2` `passthroughVcpTxSend` lost its byte spill `local_7 = (uchar)param_1` before `&local_7` was passed to the queue; with the formal declared `uchar` the store is kept (`local_7 = param_1`).
- Telemetry: none.

## 5. Validation Matrix

- [x] Targeted tests: `cargo nextest run -p fission-midend-normalize home_slot_widths` -- 12 tests. With the rule disabled the positive ones fail; against the pre-review rule the derived-slot, global, multiply and 64->32-through-addition tests fail.
- [x] Crate gates: `cargo nextest run -p fission-midend-normalize`, `cargo nextest run -p fission-pcode`.
- [x] Full-dataset sweep before/after (PR body): baseline `main-1f10b8c78`, after `types-narrow-v2-on-1f10b8c78` (this rule applied to the same base commit).

## 6. AI Review / Prompt Firewall

- [x] No other model consulted for the implementation; the PR's automated review findings were addressed in place.

## 7. Review Notes

- [x] No hardcoded binary/function/address/corpus guards.
- [x] No metric-only edits.
- [x] Extends the existing owner.
