# SLEIGH Specs Snapshot

This directory is the canonical checked-in SLEIGH spec data root for Fission.

- Primary path: `utils/sleigh-specs/languages/`
- Compiled lift artifacts: `utils/sleigh-specs/compiled/` (Ghidra 12.2 `.sla` snapshots, taken from the `ghidra_12.2_PUBLIC_20261016` release)
- Manifest: `utils/sleigh-specs/ghidra_language_manifest.json`
- Integrity manifest: `utils/sleigh-specs/MANIFEST.sha256.json`

The `fission-sleigh` crate resolves specs in this order:

1. `FISSION_SLEIGH_SPEC_DIR`
2. repo-relative `utils/sleigh-specs`
3. legacy crate-local `crates/fission-sleigh/specs`

Production lift uses checked-in `.slaspec` sources plus the matching compiled `.sla`
ConstructTpl overlay from `compiled/<arch>/<entry>.sla`. Executable runtime frontends
require the overlay; slaspec-only lift is not supported for production paths. Vendor
Ghidra installs are not required at runtime.

The legacy crate-local path remains a temporary fallback during migration. New
tooling should use this directory.

## Local deviations from upstream Ghidra 12.2

Upstream specs are not ground truth ([ADR 0017](../../docs/adr/0017-ghidra-is-not-an-oracle.md)).
A semantic fix needs architecture-manual or native-execution evidence, a re-apply script,
a byte-for-byte recompile control, and a test that fails with the upstream `.sla`.
Recompile with the release's Java compiler, `support/sleigh` (run from the `.slaspec`'s
directory): it reproduces the checked-in AArch64 `.sla` byte for byte, while the native
`Ghidra/Features/Decompiler/os/*/sleigh` builds a different `AARCH64_AppleSilicon.sla`.

`languages/` is upstream `Ghidra/Processors/<P>/data/languages/` verbatim (the `.sla`
files go to `compiled/`), except for the following. Re-apply them when upgrading;
each is recorded in `MANIFEST.sha256.json` (`source_path`).

- `ARM/{ARMt,ARMtTHUMB,ARMt_v6,ARMt_v45,ARMCortex,ARM_v45}.pspec`: `ISAModeSwitch` and
  `TB` are marked `hidden="true"`, so decoder-only interworking state never surfaces as
  decompiled assignments.
- `ARM/ARMTHUMBinstructions.sinc` (and the twelve compiled `ARM*.sla` with Thumb): the 16-bit
  `stmia Rn!, {list}` (STM T1) always writes the base back, adding 4 per listed register, as
  the ARMv7-M pseudocode's `wback = TRUE` does, and displays `Rn!` in every form. Upstream
  gave it LDM T1's `Rn_exclaim`/`Rn_exclaim_WB` pair, which skips the writeback when `Rn` is
  in the list. Re-apply with `scripts/sleigh/apply_thumb_stm_writeback.py`.
  Upstream: NationalSecurityAgency/ghidra#10273.
  `ARM8_le.sla` and `ARM8m_le.sla` as released are reproduced by no 12.2 compiler (Java on
  JDK 17 or 21, or native): the root `instruction` decision tree splits on different bits.
  The 14 other ARM `.sla` reproduce byte for byte, and lifting every 16-bit Thumb halfword
  plus 300,000 random Thumb-2 and 365,536 ARM words with the released and the recompiled
  ARM8 files gives identical p-code, so the checked-in ARM8 files are plain recompiles.
  The same file sets C from the shifter in the Thumb-2 `ands`/`bics`/`movs`/`mvns` with a
  modified immediate and `ands`/`mvns` with a shifted register, as `ThumbExpandImm_C`/`Shift_C`
  and `APSR.C = carry` define; upstream built `thSBIT_ZN` there and set only N and Z. They now
  build a new flag table, `thSBIT_SCZN`, whose `s` form also does `CY = shift_carry`, so the
  forms without S keep upstream's p-code exactly. Re-apply with
  `scripts/sleigh/apply_thumb2_logical_carry.py`. Upstream: NationalSecurityAgency/ghidra#10274.
- `x86/x86-64-swift.cspec` and its `<compiler id="swift">` entry in `x86/x86.ldefs`:
  Ghidra 12.2 removed them (Swift moved to cspec extensions under
  `Ghidra/Features/Swift/data/extensions`). They are kept from 12.0.4 so Swift x86-64
  binaries still select a Swift calling-convention spec. Not yet migrated to the extension
  model.
- `AARCH64/AARCH64neon.sinc` (and the three compiled `AARCH64*.sla`): upstream lifts every
  FCVT{A,M,N,P,Z}{S,U} rounding mode to a bare `trunc`, so `fcvtms`/`fcvtps`/`fcvtns`/`fcvtas`
  behave like `fcvtzs`. Each mode now has its own display table and constructors:
  `trunc(floor(x))` for M, `trunc(ceil(x))` for P, `trunc(round(x))` for N and A, `trunc(x)`
  for Z. N and A are approximations (SLEIGH has only half-up `round`, no ties-to-even or
  ties-away), and saturation is unchanged. Re-apply with
  `scripts/sleigh/apply_aarch64_fcvt_rounding.py` (needs a `sleigh` compiler of the same
  Ghidra release), then recompile the three `.sla` and refresh `MANIFEST.sha256.json`.
  The same file also gives each FRINT{A,I,M,N,P,X,Z} mode its own semantics: upstream lifts
  all seven to `trunc`, a float-to-integer conversion, although FRINT yields a float. Now
  `floor` for M, `ceil` for P, `round` for A/I/N/X (approximations: no ties-to-even or
  ties-away, FPCR mode not modelled) and `int2float(trunc(x))` for Z (loses precision for
  magnitudes at or above 2^63). Re-apply with `scripts/sleigh/apply_aarch64_frint_rounding.py`.
  Then `scripts/sleigh/apply_aarch64_frint_exact.py` makes N, I, X and Z exact (#1064):
  ties to even for N and for I/X (FPCR.RMode = round to nearest, the default; other modes are
  not modelled), built from `floor`, the exact fraction `x - floor(x)` and an evenness test,
  and toward zero as `ceil` for negative and `floor` for positive operands, so -0.0 and
  magnitudes at or above 2^63 survive. A stays `round` (ties away from zero). It also gives
  FRINT32Z/FRINT32X/FRINT64Z/FRINT64X their FPRoundIntN semantics (the same roundings, and
  the most negative N-bit integer for a NaN, an infinity or an out-of-range result), with
  the rest of the register zeroed; upstream lifted them as `trunc`, integer bits (#1070).
  Evidence: an Apple M4 run natively (the native differential).
  Then `scripts/sleigh/apply_aarch64_fcvt_saturate.py` gives every FCVT{A,M,N,P,Z}{S,U} the
  Arm ARM FPToFixed semantics (#1056): each mode table is split into one table per mnemonic
  (`fcvt_vmnemonic_ms`, `..._mu`, ...) so S and U get their own constructors; the operand is
  rounded exactly (M `floor`, P `ceil`, Z by `trunc` itself, N ties to even and A ties away
  built from `floor` and the exact fraction), then saturated to the N-bit signed or unsigned
  range, with 0 for a NaN; the unsigned forms convert values at or above 2^(N-1) as
  `trunc(r - 2^(N-1)) ^ 2^(N-1)`. Half precision is widened to single first. The same script
  rewrites the fixed-point `fcvtz{s,u} Wd/Xd, Hn/Sn/Dn, #fbits` (scaled by 2^fbits as two
  exact power-of-two multiplications; upstream's half-precision FCVTZU multiplied by the
  integer fbits) and the scalar SIMD `fcvtz{s,u} Hd/Sd/Dd, #fbits` (upstream lifted `fcvtzs`
  as the user op `NEON_fcvtzs`, and the H and D forms of `fcvtzu` read the immediate through
  `Imm_shr_imm32`: wrong fbits and disassembly). The vector fixed-point forms, still the user
  ops `NEON_fcvtzs`/`NEON_fcvtzu`, are unchanged. Evidence: an Apple M4 run natively.
- `AARCH64/AARCH64neon.sinc` (and the three compiled `AARCH64*.sla`): the saturating and
  rounding NEON forms upstream lifts as plain p-code wrap and truncate. `SUQADD`, `SQNEG`,
  `SQDMULL`/`SQDMULL2`, `SQDMLAL`/`SQDMLAL2`, `SQDMLSL`/`SQDMLSL2` and the by-element
  `SQDMULH`/`SQRDMULH` now saturate as SatQ does (one saturation for the doubled product and
  one for the accumulation), `SQRDMULH` adds the rounding constant, and `SRSRA`/`URSRA` add
  bit `shift-1` of the operand (the rounding constant, without widening, so a shift of the
  full element width is right too). All sources are read before Vd is written (#1068, native
  evidence). Forms upstream lifts as `NEON_*` user ops are unchanged. Re-apply with
  `scripts/sleigh/apply_aarch64_neon_saturate.py`.
- `AARCH64/AARCH64neon.sinc` and `AARCH64/AARCH64instructions.sinc` (and the three compiled
  `AARCH64*.sla`): `FMLAL`/`FMLAL2`/`FMLSL`/`FMLSL2` accumulated with the integer `+`/`-`
  (adding float bit patterns) after a half-precision multiply; each lane is now
  `acc f+ (float2float(a) f* float2float(b))`, exact because a half-precision product fits in
  single precision, so it rounds once as FPMulAdd does (FMLSL negates the first element). Their
  by-element forms decoded the index as H:L instead of H:L:M (the `b_2223=2` rules of
  `vIndexHLM` and `Re_VPR128Lo.H.sel`, used by no other instruction). `SCVTF`/`UCVTF Hd, Rn,
  #fbits` converted to half precision (infinity above 65504) and divided by 2^fbits as a
  half-precision constant (infinity for fbits >= 16), giving NaN; they now convert through
  double and round once (a 64-bit source with more than 53 significant bits rounds twice).
  Evidence: an Apple M4 run natively; the native differential's `~fp16` entry hid these.
  The vector `FMLAL2`/`FMLSL2 .4S` forms also read the second source from Vm<63:0> instead of
  Vm<127:64> (upstream); the same script fixes that.
  Re-apply with `scripts/sleigh/apply_aarch64_fmlal_fp16_convert.py`.
- `AARCH64/AARCH64neon.sinc` (and the three compiled `AARCH64*.sla`): the scalar
  `FMSUB`/`FNMADD`/`FNMSUB` subtracted where FPMulAdd negates an operand (FPNeg), so a
  propagated NaN had the wrong sign; they and `FMADD` are now `addend f+ (element1 f*
  element2)` with the operand negated first. `FADDP .4H` put Vm's pairwise sums in the low
  lanes; Vn's come first. Evidence: an Apple M4 run natively (nightly size). Not modelled:
  the single rounding (#1072) and the signalling-NaN priority of FPProcessNaNs3 (#1397).
  Re-apply with `scripts/sleigh/apply_aarch64_fpneg_faddp.py`.
- `AARCH64/AARCH64neon.sinc` (and the three compiled `AARCH64*.sla`): the scalar
  `FMAX`/`FMIN`/`FMAXNM`/`FMINNM` (H, S, D) follow FPMax/FPMin/FPMaxNum/FPMinNum. Upstream
  copied Rn into Rd before comparing, so with Rd = Rm (`fmax d0, d1, d0`) the result was
  always Rn; it ignored a NaN in Rn (FMAX/FMIN must return a NaN) and returned a lone quiet
  NaN from FMAXNM (it must return the number), and FMINNM was the userop `NEON_fminnm`. They
  now copy both operands first, process NaNs as FPProcessNaNs does (signalling first, then
  op1, quieted), and combine the signs of two zeros (#1063, native evidence). Re-apply with
  `scripts/sleigh/apply_aarch64_fmax_fmin_scalar.py`.
- `AARCH64/AARCH64base.sinc` (and the three compiled `AARCH64*.sla`): the sign-extending byte
  and halfword loads into a `W` register (`ldrsb`/`ldrsh` in every addressing form,
  `ldursb`/`ldursh`, `ldtrsb`/`ldtrsh`) sign-extend to 32 bits and zero bits 63:32, as the
  Arm ARM's `X[t, 32] = SignExtend(data, 32)` does and an Apple M4 does when run natively.
  Upstream sign-extended them into all 64 bits, and zero-extended the `ldursb`/`ldtrsb`
  byte. Each of the eight `Rt_GPR32` constructors now lifts as
  `local tmp:4 = sext(*:N addr); Rt_GPR64 = zext(tmp);`; the `X` forms, `ldrsw` and
  `ldapursb`/`ldapursh` were already right. Re-apply with
  `scripts/sleigh/apply_aarch64_w_signed_loads.py`. Upstream: NationalSecurityAgency/ghidra#6471.
- `AARCH64/AARCH64base.sinc` (and the three compiled `AARCH64*.sla`): writes to a `W`
  register zero bits 63:32 of `X` (Arm ARM: the `X[]` setter stores `ZeroExtend(value, 64)`)
  in the forms upstream wrote as a bare `W` assignment, which kept the old upper half: the
  LSE atomics `LD{ADD,CLR,EOR,SET,SMAX,SMIN,UMAX,UMIN}` and `SWP` (byte, halfword and word,
  every ordering), `LDLARB`/`LDLARH`/`LDLAR`, `LDAPRB`/`LDAPRH`/`LDAPR` and `CAS`/`CASP`
  with `W` registers. Each now assigns `zext(...)` to the `X` register. Evidence: an Apple
  M4 run natively (the native differential, fission-systems/Fission#1067 and #1057).
  Re-apply with `scripts/sleigh/apply_aarch64_w_dest_zero_extend.py`.
  The same file compares the byte and halfword `LDSMAX`/`LDSMIN`/`LDUMAX`/`LDUMIN` at the
  element width (`Max(SInt(data), SInt(value))` over 8 or 16 bits); upstream compared the
  loaded element with the whole 32-bit `Ws`, so the stored maximum could be the register's
  low byte (#1069). Re-apply with `scripts/sleigh/apply_aarch64_lse_minmax_width.py`.
- `AARCH64/AARCH64ldst.sinc` (and the three compiled `AARCH64*.sla`): `LD1`..`LD4` and
  `LD1R`..`LD4R` with a 64-bit arrangement (`.8B/.4H/.2S/.1D`) zero bits 127:64 (and the SVE
  upper bits) of every destination, as `V[t, 64] = rval` does; upstream left them, marked
  `# neglected zexts`. The 41 constructors now call `zext_zd`, as the spec's other 64-bit
  SIMD results do (#1067, native evidence). Re-apply with
  `scripts/sleigh/apply_aarch64_ld_vpr64_zero_upper.py`.
- `x86/lockable.sinc` (and the compiled `x86.sla`, `x86-64.sla`): `XADD m, r` stores the
  sum before it writes the register, and reads the memory operand once (SDM: `TEMP := SRC +
  DEST; SRC := DEST; DEST := TEMP` with the address formed before execution). Upstream wrote
  the register first, so `xadd byte ptr [rbx], bh` stored through the updated RBX (#1053).
  `XCHG m32, r32` zero-extends the register into 64 bits (`check_Reg32_dest`), as every
  32-bit register write does in 64-bit mode (#1060). Re-apply with
  `scripts/sleigh/apply_x86_xadd_xchg_memory_forms.py`.
- `x86/ia.sinc` (and the compiled `x86.sla`, `x86-64.sla`): `0x90` is a NOP, and `F3 90`
  PAUSE, only when REX.B is 0; with REX.B it is `xchg r8d, eax` (`90+rd`), and REX.W/R/X
  alone no longer turn it into a 32-bit `xchg eax, eax` that clears RAX[63:32] (#1059). A
  repeated `0x66` prefix keeps the operand-size override instead of toggling it back
  (`66 66 03 e8` is `add bp, ax`, #1061): the first one sets `prefix_66` and later ones
  match a constructor that changes nothing. `SHR` and `SHRD` leave OF alone when the masked
  count is 0 and set it per the SDM for a count of 1 (#1051); upstream's `shrflags`/
  `shrdflags` had the two cases swapped. `PUSHF`/`PUSHFD`/`PUSHFQ` store reserved bit 1 as 1
  (#1052). Evidence: an x86-64 host run natively (the native differential) and the SDM
  sections cited in the script. Re-apply with `scripts/sleigh/apply_x86_prefix_and_flag_fixes.py`.
- `x86/ia.sinc`, `x86/lockable.sinc` (and the compiled `x86.sla`, `x86-64.sla`): AF is set
  by every add/subtract form, as the carry or borrow out of bit 3 (SDM Vol. 1, 3.4.3.1);
  upstream never wrote it (#1048). `addflags`, `subflags`, `addCarryFlags`,
  `subCarryFlags` and `negflags` compute `AF = ((a ^ b ^ result) & 0x10) != 0` from their
  operands, and INC/DEC set it from the low nibble, so ADD, ADC, SUB, SBB, CMP, CMPS,
  SCAS, NEG, INC, DEC, XADD, CMPXCHG and CMPccXADD all write it. The SSE
  float-to-integer conversions `CVTSD2SI`/`CVTSS2SI`/`CVTPD2DQ`/`CVTPS2DQ` round to
  nearest with ties to even (MXCSR.RC = 00, the default; other modes are not modelled)
  instead of upstream's ties-away `round` or, for the packed forms, no rounding (#1049),
  and they and `CVTTSD2SI`/`CVTTSS2SI`/`CVTTPD2DQ`/`CVTTPS2DQ` return the integer
  indefinite (`0x80000000`, `0x8000000000000000`) for a NaN, an infinity or an
  out-of-range result (#1050). The rounding is built exactly from `floor`, as
  `apply_aarch64_frint_exact.py` does. The VEX/EVEX `VCVT*` and MMX `CVT*PI` forms are
  unchanged. Evidence: an x86-64 host run natively (the native differential) and the SDM
  sections cited in the script. Re-apply with `scripts/sleigh/apply_x86_af_and_cvt.py`.
- `Toy/` is from the Ghidra source tree (it has no `data/languages` in the release) and
  has no compiled `.sla`.

Regenerate `ghidra_language_manifest.json` with
`cargo run --release -p fission-sleigh --example generate_sleigh_frontends`.
