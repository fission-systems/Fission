# ARM function symbols carry a state bit, not an odd address

## 1. Baseline Row Anchor

- Binaries: `fission-benchmark/corpus/scale/binaries/O0/betaflight/betaflight_STM32F405.elf`
  and `.../cleanflight/cleanflight_DALRCF405.elf`
- Command: release `fission_cli decomp <binary> --layer nir --json --addr <addr>`
  over the first twenty functions each manifest names.
- Current measured output: **not one function carries its own name.**

```text
program                 manifest name == emitted name
betaflight (ARM)                              0 / 20
cleanflight (ARM)                             0 / 20
bash (x86-64)                                20 / 20
bzip2 (x86-64)                               20 / 20
coreutils (x86-64)                           19 / 19
```

The failure is a shift, not noise. Every function is named after its
neighbour:

```text
manifest A7105Config     -> we emit A7105Strobe
manifest A7105ReadFIFO   -> we emit A7105Init
manifest ADC_ClearFlag   -> we emit ADC_GetFlagStatus
```

A wrong name is worse than no name: `sub_806ecf8` says "unknown", while
`A7105Strobe` on `A7105Config`'s body asserts something false about code the
reader is trying to understand.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [ ] Normalize · [ ] Structuring
- [ ] Type/data recovery · [ ] Printer · [x] Loader

`elf::parse_symbols_32` took `st_value` as the address. The ARM ELF ABI
encodes Thumb state in bit 0 of a *function* symbol's value; the instruction
is at the even address. Left set, every Thumb function registers one byte past
where it begins, an address lookup finds no exact match and falls back to the
nearest preceding symbol -- the previous function. That is the shift.

Not a structuring or naming-policy concern: the address is wrong before
anything downstream sees it.

## 3. Generality / Invariant Proof

```text
A function symbol's address is its value with the architecture's state bit
cleared. A data symbol's value is its address unchanged.
```

Scoped to ARM and to `STT_FUNC`. Data keeps bit 0 because there it is address,
not state -- an odd datum is genuinely at an odd address. Applied after the
relocatable section-base addition, since `st_value` carries the state bit in
both linked and relocatable objects.

ISA-agnostic check:

- [x] Keyed on `e_machine`, the ELF header's own statement of architecture,
      not on a binary or file identity.
- [x] No binary/function/address/compiler identity in production logic.

## 4. Risk And Ownership Check

Only ELF32 needs this: ARM32 is always ELF32, and AArch64 does not use the
bit. `parse_symbols_64` is untouched.

The rule is extracted as `elf32_symbol_entry_address` rather than inlined, so
the invariant is testable without an ELF fixture builder, which this crate
does not have.

## 5. Validation Matrix

- [x] Targeted test over all four cases: odd ARM function, even ARM function,
      odd ARM datum, odd non-ARM function.
- [x] `cargo nextest run -p fission-loader -p fission-static` (214 passed).
- [x] Re-measured the anchor: both ARM programs 0/20 -> **20/20**.
- [x] x86-64 unchanged at 20/20, 20/20, 19/19 -- the mask is architecture-gated
      and must not touch them.
- [x] DecBench sample-set rerun: 250/250 still decompile, three outputs
      changed, none of them ARM.

Measured result:

```text
program                 before    after
betaflight (ARM)         0 / 20   20 / 20
cleanflight (ARM)        0 / 20   20 / 20
bash (x86-64)           20 / 20   20 / 20
bzip2 (x86-64)          20 / 20   20 / 20
```

**No sample-set movement, and none is expected.** That kit's ARM binaries are
statically linked and fully stripped -- measured earlier this cycle at zero
dynamic symbols against 33,208 on the x86-64 ones -- so they carry no symbol
for this to attribute correctly. The fix pays on ARM images that have symbols,
which is most of what a reverse engineer actually opens, and not on the scored
corpus.

### It also invalidated a measurement

The structure-distance investigation compares a decompiled CFG to a published
source CFG **by function name**. On ARM every name was its neighbour's, so
those rows compared one function's source against another function's
decompilation. Of 259 rows in the wide run, 76 (29%) were ARM and are void.
Corrected, that run reads 183 rows, 68 exact (37%), median 8 -- against the
42% and median 5 reported before the names were checked.

## 6. AI Review / Prompt Firewall

- No external model was consulted.
- Found by checking whether the harness's name-keyed matching was sound, not
  from a benchmark row. Production code names no binary or function.
