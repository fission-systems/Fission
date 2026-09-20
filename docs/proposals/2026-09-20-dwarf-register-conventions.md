# Target-Aware DWARF Stack Registers

## Baseline / issue anchor

- Issue: #43
- Owner: `crates/fission-loader/src/loader/dwarf/`
- Current defect: location expressions classify DWARF register numbers using a
  single x86-64/AArch64 set, misclassifying x86-32, ARM32, and RISC-V stack
  locations and treating valid function address `0` as absent.
- Observable invariant: `RegisterOffset` is stack-relative only when its base
  register belongs to the target language's DWARF stack/frame convention.

## Owner proof

`DwarfAnalyzer::analyze_functions_inner` and the location-expression parser
create the incorrect facts before decompiler consumers see `DwarfLocation`.
The loader already carries the selected architecture descriptor and language
ID, so the loader DWARF owner can choose the register convention without
coupling to a downstream p-code or printer layer.

## Generalized rule

Select the stack/frame DWARF register set from the binary's target language and
architecture descriptor: x86-32 `(4,5)`, x86-64 `(6,7)`, ARM `(11,13)` with
Thumb's additional `r7`, AArch64 `(29,31)`, RISC-V `(2,8)`, and MIPS `(29,30)`.
Unknown targets return an empty set and remain conservative. A subprogram with
an explicit `DW_AT_low_pc` keeps address `0`; only a missing low-PC attribute
is treated as declaration-only.

No function, address, binary, or compiler-specific guard is used.

## Validation matrix

- Architecture mapping regression covers x86-32, x86-64, ARM, Thumb, AArch64,
  and RISC-V, including rejection of x86-64 numbers on 32-bit targets.
- Existing DWARF analyzer and loader tests remain green.
- `cargo nextest run -p fission-loader`, downstream decompiler/CLI checks,
  format, and diff checks.
