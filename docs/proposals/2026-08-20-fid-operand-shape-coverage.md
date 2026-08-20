# FID hashing: operand shapes that refuse the whole function

## 1. Baseline Row Anchor

- Binaries: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/`,
  24 sampled across all three formats in the set.
- Command: release `fission_cli decomp <binary> --addr <addr>` with
  `FISSION_FID_DIAG=1`.
- Current measured output, before this change:

```text
format          discovered   decoded   hashed   FID-matched
ELF64-x86-64         6,117     3,322      168             0
PE32-i386            1,255       309       12             0
ELF32-ARM (8 bins)       8         0        0             0
```

`fid_hashes` declines roughly 95% of decoded candidates, and not one of the
hashes it does produce matches any of the 228 shipped databases. Every named
libc call in the x86-64 output comes from those binaries' 33,208 dynamic
symbols; FID contributes nothing here.

**Correction, measured later in the same cycle:** "matches nothing" is true of
this corpus and must not be generalised. Every sample-set x86-64 binary is
dynamically linked, so its library code is not in the image and zero matches
is correct behaviour rather than a defect. On a statically linked target --
`vendor/binaries/tests/x86_64/elf_with_static_libc_ubuntu_2004_stripped` --
the same build already matched three functions before any change here.

The decline reasons, measured:

```text
                       ARM (of 2,091)   x86-64 (of 59)
unknown operand shape           2,043               53
too few code units                 48                3
```

Two things this rules out. The ARM row's `discovered = 1` is not an ARM
defect: `--function-discovery-profile balanced` finds 2,501 functions in the
same binary, and CLI `decomp` merely defaults to `conservative`, which has no
seeds in a stripped, statically linked image. It is also not worth changing
for the score -- balanced was measured over the whole 58-binary ARM slice and
produced identical output (21 gotos either way, zero named library calls
either way) plus four new timeouts, because naming still needs FID.

## 2. Owner Proof

- [x] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

`compiled_table::fid_hash` owns it. The reference is vendored at
`vendor/ghidra/.../hash/MessageDigestFidHasher.java`, whose per-operand loop
walks `getOpObjects(ii)` and mixes three object kinds -- `Scalar`, `Register`,
`Address`. Anything else falls through all three arms and contributes only the
operand's base term; nothing aborts.

Fission derives the same objects from SLEIGH handles instead, and treats an
operand it cannot classify as fatal to the whole function's hash. That is the
right policy for a hash (a wrong hash is worse than none), but it means one
unclassified operand in one instruction discards a function of hundreds --
so a small per-operand gap becomes near-total at function scale.

Two gaps were found in that classification:

1. **A direct branch or call target.** `is_flow_target` required
   `debug_value == Some(Immediate)`, but `jz`/`jnz` on x86 and `beq`/`bvc` on
   ARM arrive with no resolved `BoundOperand` at all, leaving the target in
   register or unique space. They fell through to the memory-address tracer,
   which cannot describe a branch target. Ghidra reports these as `Address`.
2. **A handle that names a register and nothing more.** `cmovz RDX,RBX`'s
   destination carries a register-space offset and no `BoundOperand`. Ghidra
   reports it as `Register` and hashes `reg.getOffset()`.

## 3. Generality / Invariant Proof

```text
An operand is classified by what the handle proves about it, in the order the
reference classifies: a resolved BoundOperand first, then a direct branch or
call target as an Address, then a traced memory address, and only then a bare
register-space handle as a Register. Each arm reproduces one arm of
MessageDigestFidHasher's per-operand loop.
```

Order is load-bearing and was proved so by an existing golden vector. Reading
a register-space handle as a `Register` *before* the memory tracer broke
`fid_hashes_match_ghidra_exactly_for_aarch64_stp_ldp_prologue_epilogue`,
because AArch64 pre-index addressing leaves a computed memory address in a
register-space varnode. The register reading is therefore a fallback after
the tracer declines, never a shortcut past it.

ISA-agnostic check:

- [x] Uses only decoded instructions, SLEIGH handles and p-code.
- [x] No binary/function/address/compiler/ISA identity in production logic.
- [x] Ghidra is a read-only reference; no runtime or build dependency.

## 4. Risk And Ownership Check

- The all-or-nothing policy is kept. Neither arm guesses: each one mixes a
  value the handle proves, or declines as before.
- A hash that disagreed with Ghidra's would match nothing rather than match
  something wrong, but the golden vectors are what actually guard this, and
  they are unchanged and passing.
- `FISSION_FID_DIAG` now reports the decline reason and the handle that caused
  it. That is how the remaining gap below was localised, and it is how the
  next one will be.

## 5. Validation Matrix

- [x] Two targeted tests spelling Ghidra's `Address` arithmetic out from the
      Java, independent of this module's own constants.
- [x] `cargo nextest run -p fission-sleigh` (303 passed, 3 skipped), including
      the pre-existing AArch64 golden vector that caught the first attempt.
- [x] `cargo nextest run --workspace --no-fail-fast`: 2,582 passed, 7 failed,
      1 timed out -- the established `fission-emulator` baseline plus the
      pre-existing `fission-dir::phase2_corpus_ground_truth` timeout, both
      verified unrelated earlier in this cycle.
- [x] Re-measured on the sample-set binaries the anchor was taken from.

Measured result. Hashes produced, before this cycle's changes and after:

```text
binary                                       before   after
bin_000.elf (sample-set, dynamic)                 3      25
bin_002.elf (sample-set, dynamic)                14     335
bin_009.elf (sample-set, dynamic)                41     138
elf_with_static_libc_ubuntu_2004_stripped       192     377
elf_with_static_libc_ubuntu_2004                235     466
static                                          177     380
bin_001.elf (sample-set, ARM)                     0       0
```

Three operand shapes were closed, in the order the reference classifies:

1. a direct branch or call target, as `Address`;
2. the address a lone `LOAD`/`STORE` dereferences, when the handle names no
   varnode to trace from -- this is the one that moved the numbers, since a
   store destination such as `mov dword ptr [RBX],EAX` arrives with an
   entirely empty handle;
3. a bare register, read either off the handle or off a single-handle operand
   subtable, which is Ghidra's `getOpObjects` flattening.

**Match count is unchanged at three on each static binary, and no end-to-end
gain is claimed.** Hash production roughly doubled without converting into
matches, which leaves two possibilities this cycle did not separate: the newly
hashed functions are genuinely absent from the vendored databases (an Ubuntu
20.04 glibc against databases built from other distributions), or the new
hashes disagree with Ghidra's. Nothing regressed -- the three that matched
before still match, and the AArch64 golden vector still passes -- but the
next cycle should settle which, because the answer decides whether the
remaining work is more operand shapes or more databases.

ARM still produces no hashes at all. Its remaining declines are `stmda<cond>`
forms with an operand subtable carrying no handles, and those are what
arbitrary data decodes to under ARM -- a signal that `balanced` discovery is
claiming data regions on that target, which is a discovery question rather
than a hashing one.

## 6. AI Review / Prompt Firewall

- No external model was asked for implementation advice.
- The vendored Ghidra source was read to establish the reference arithmetic
  and the classification order; the tests restate that arithmetic from the
  Java rather than from the Rust.
- The sample-set was used as a measurement corpus. Production code contains no
  binary, function, or address identity from it.
