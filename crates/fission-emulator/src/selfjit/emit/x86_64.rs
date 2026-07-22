//! x86-64 host emitter.
//!
//! Same idea as [`super::aarch64`]: hand-encode a small, real subset of
//! SysV64 machine code, verified against *executed* results (see this
//! module's own unit tests, and `compiler.rs`'s integration test), not
//! derived by inspection alone. Unlike `aarch64.rs`, this was written and
//! verified on an Apple Silicon dev machine via Rosetta 2 (`rustup target
//! add x86_64-apple-darwin`, then `cargo test --target x86_64-apple-darwin`
//! -- Rosetta transparently translates a process's *own* runtime-generated
//! machine code, not just its statically-linked instructions, so an mmap'd
//! RX page written by this emitter executes correctly there), not real
//! x86-64 silicon -- see this module's own doc note on that gap.
//!
//! Encoding reference: Intel SDM Vol. 2, and
//! <https://www.felixcloutier.com/x86/> (much easier going for a
//! hand-rolled emitter than the SDM itself).
//!
//! # Register roles differ from AArch64 in one structural way `compiler.rs`
//! # had to be generalized for
//!
//! AAPCS64 (aarch64) uses X0 for *both* the first argument and the return
//! value of a call -- `compiler.rs` used to lean on that coincidence
//! directly (a single `aarch64_regs_x0()` helper for both roles). SysV64
//! (x86-64) does not: the first argument is RDI, the return value is RAX,
//! different registers. `compiler.rs` now spells these as two distinct
//! generic roles, `ARG0` and `RET` (identical on aarch64, genuinely
//! different here), rather than conflating them -- see this module's own
//! `ARG0`/`RET` constants below and `compiler.rs`'s doc comment on the
//! split.
//!
//! # Frame setup is arch-specific glue, not part of the shared `Asm` shape
//!
//! AArch64 has no hardware call stack for return addresses (an explicit
//! link register, X30, saved/restored like any other value) and no
//! PUSH/POP instructions; x86-64 has both (`call`/`ret` push/pop the
//! return address implicitly, and PUSH/POP r64 are the idiomatic way to
//! save callee-saved registers). Rather than force one shape to cover
//! both (which is what silently didn't generalize before this backend
//! existed), `compiler.rs`'s `prologue`/`epilogue_return` are
//! `#[cfg(target_arch = ...)]`-gated: aarch64's existing, already-verified
//! frame layout is untouched; x86-64 gets its own PUSH/POP-based version
//! using primitives defined only here (`push_reg`/`pop_reg`), not part of
//! the interface aarch64.rs exposes. `str_imm`/`ldr_imm` below are
//! consequently unused on this arch (aarch64's frame needs them;
//! x86-64's PUSH/POP-based one doesn't) -- left as `todo!()` stubs rather
//! than implemented-but-dead code. `sub_imm`/`add_imm` are real,
//! implemented primitives here too, unlike those two -- `compiler.rs`'s
//! `LzCount` arm calls them directly (not just aarch64's frame code).

use super::Label;
use crate::selfjit::codebuf::CodeBuffer;

/// SysV64 argument-register roles, by argument *position* (not by a
/// borrowed AArch64 register name like the old `X0`..`X4` -- see this
/// module's own doc on why aarch64's "X0 is also the return register"
/// shortcut doesn't hold here). `compiler.rs` imports these generically
/// via `crate::selfjit::emit::{ARG0, ARG1, ...}`.
pub const ARG0: u32 = RDI;
pub const ARG1: u32 = RSI;
pub const ARG2: u32 = RDX;
pub const ARG3: u32 = RCX;
pub const ARG4: u32 = R8;
/// SysV64 return-value register -- genuinely different from `ARG0` here,
/// unlike aarch64 where both are X0.
pub const RET: u32 = RAX;

/// Persistent value-slot roles (see `compiler.rs`'s own doc comment on
/// why these must be callee-saved), mapped to real SysV64 callee-saved
/// GPRs. `compiler.rs` imports these generically as `EMU_PTR_SLOT`/
/// `A_VAL_SLOT`/`B_VAL_SLOT`/`RESULT_SLOT`.
pub const EMU_PTR_SLOT: u32 = RBX;
pub const A_VAL_SLOT: u32 = R12;
pub const B_VAL_SLOT: u32 = R13;
pub const RESULT_SLOT: u32 = R14;
/// Stack pointer, for `add_imm`-based scratch-buffer address computation
/// (`compiler.rs`'s `scratch_buf_addr`). Unlike aarch64's SP, RSP is a
/// perfectly ordinary GPR encoding in register-to-register contexts (no
/// XZR-style ambiguity) -- safe to pass through `add_imm` (which does
/// `mov_reg(dst, RSP)` internally when `dst != RSP`, an entirely normal
/// register-register MOV, not a memory operand, so RSP's usual "needs a
/// SIB byte as a memory base" quirk doesn't apply here).
pub const SP: u32 = RSP;

// Raw SysV64 register encodings (the 4-bit number ModRM/REX/opcode-plus-reg
// fields ultimately need -- 0-7 need no REX extension bit, 8-15 do).
const RAX: u32 = 0;
const RCX: u32 = 1;
const RDX: u32 = 2;
const RBX: u32 = 3;
const RSP: u32 = 4;
#[allow(dead_code)]
const RBP: u32 = 5;
const RSI: u32 = 6;
const RDI: u32 = 7;
const R8: u32 = 8;
#[allow(dead_code)]
const R9: u32 = 9;
#[allow(dead_code)]
const R10: u32 = 10;
#[allow(dead_code)]
const R11: u32 = 11;
const R12: u32 = 12;
const R13: u32 = 13;
const R14: u32 = 14;
/// Dedicated internal scratch register for this emitter's own use only
/// (never a role `compiler.rs` is aware of or can pass in) -- needed to
/// correctly handle destination/source register aliasing in the
/// destructive 2-operand `rd = rn OP rm` forms x86 ALU instructions use
/// (see [`Asm::binop_general`]'s doc). Confirmed disjoint from every
/// role above (`ARG0..ARG4`, `RET`, `EMU_PTR_SLOT`/`A_VAL_SLOT`/
/// `B_VAL_SLOT`/`RESULT_SLOT`, and `compiler.rs`'s own raw-literal
/// `CALLEE_ADDR`/`TMP1`/`TMP2` = 9/10/11).
const SCRATCH: u32 = 15;

/// `compiler.rs`'s AArch64-flavored "register 31 means the zero register"
/// convention (`XZR`), reused verbatim as a cross-arch sentinel -- 31 is
/// out of range for any real x86-64 register encoding (0-15), so it can't
/// collide with a real operand by accident. [`Asm::sub_reg`] special-cases
/// it (there is no x86 zero register; `0 - rm` is synthesized via `NEG`).
const XZR_SENTINEL: u32 = 31;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    /// Unsigned >=
    Cs,
    /// Unsigned <
    Cc,
    /// Signed <
    Lt,
    /// Signed <=
    Le,
    /// Signed >
    Gt,
    /// Signed >=
    Ge,
    /// Unsigned <=
    Ls,
    /// Unsigned >
    Hi,
}

impl Cond {
    /// The low nibble of a `Jcc` opcode's second byte (`0F 8<cc>`) --
    /// chosen by *meaning* (what `compiler.rs`'s callers use each variant
    /// to express, e.g. `Cc` = "unsigned less-than"), not by trying to
    /// replicate AArch64's raw carry-flag polarity, which is inverted
    /// relative to x86's for subtraction (ARM's C=1 after `SUBS a,b` means
    /// no borrow, i.e. `a >= b` unsigned; x86's CF=1 after `SUB a,b` means
    /// a borrow occurred, i.e. `a < b` unsigned) -- using x86's own named
    /// mnemonics (`JB`/`JAE`/`JBE`/`JA`) sidesteps that polarity
    /// difference entirely since each already encodes its own meaning.
    fn cc_bits(self) -> u8 {
        match self {
            Cond::Eq => 0x4,
            Cond::Ne => 0x5,
            Cond::Cc => 0x2, // JB:  unsigned <
            Cond::Cs => 0x3, // JAE: unsigned >=
            Cond::Lt => 0xC, // JL:  signed <
            Cond::Ge => 0xD, // JGE: signed >=
            Cond::Le => 0xE, // JLE: signed <=
            Cond::Gt => 0xF, // JG:  signed >
            Cond::Ls => 0x6, // JBE: unsigned <=
            Cond::Hi => 0x7, // JA:  unsigned >
        }
    }

    pub fn invert(self) -> Cond {
        match self {
            Cond::Eq => Cond::Ne,
            Cond::Ne => Cond::Eq,
            Cond::Cs => Cond::Cc,
            Cond::Cc => Cond::Cs,
            Cond::Lt => Cond::Ge,
            Cond::Ge => Cond::Lt,
            Cond::Le => Cond::Gt,
            Cond::Gt => Cond::Le,
            Cond::Ls => Cond::Hi,
            Cond::Hi => Cond::Ls,
        }
    }
}

pub struct Asm<'a> {
    buf: &'a mut CodeBuffer,
}

fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

impl<'a> Asm<'a> {
    pub fn new(buf: &'a mut CodeBuffer) -> Self {
        Self { buf }
    }

    pub fn offset(&self) -> usize {
        self.buf.offset()
    }

    /// `movabs rd, imm64` -- always the full 10-byte form regardless of
    /// the immediate's magnitude (unlike aarch64's variable MOVZ/MOVK
    /// chain); simpler and not worth optimizing for this emitter's goals.
    pub fn mov_imm64(&mut self, rd: u32, imm: u64) {
        let rex = rex(true, false, false, rd >= 8);
        let mut bytes = vec![rex, 0xB8 + (rd & 7) as u8];
        bytes.extend_from_slice(&imm.to_le_bytes());
        self.buf.emit_bytes(&bytes);
    }

    /// `mov rd, rn` (`MOV r/m64, r64`, opcode 0x89: dst is the ModRM.rm
    /// field, src is ModRM.reg).
    pub fn mov_reg(&mut self, rd: u32, rn: u32) {
        if rd == rn {
            return;
        }
        self.emit_rr(0x89, rd, rn);
    }

    /// `op r/m64, reg` (dst = ModRM.rm, src = ModRM.reg) -- the shape
    /// `MOV`/`ADD`/`SUB`/`AND`/`OR`/`XOR`/`CMP` all share for their
    /// register-register forms.
    fn emit_rr(&mut self, opcode: u8, dst_rm: u32, src_reg: u32) {
        let rex = rex(true, src_reg >= 8, false, dst_rm >= 8);
        let modrm = 0xC0 | (((src_reg & 7) as u8) << 3) | ((dst_rm & 7) as u8);
        self.buf.emit_bytes(&[rex, opcode, modrm]);
    }

    /// `op reg, r/m64` (dst = ModRM.reg, src = ModRM.rm) -- the *reversed*
    /// field assignment `IMUL r64,r/m64` and `BSR r64,r/m64` use (two-byte
    /// `0F xx` opcodes).
    fn emit_rm(&mut self, opcode2: u8, dst_reg: u32, src_rm: u32) {
        let rex = rex(true, dst_reg >= 8, false, src_rm >= 8);
        let modrm = 0xC0 | (((dst_reg & 7) as u8) << 3) | ((src_rm & 7) as u8);
        self.buf.emit_bytes(&[rex, 0x0F, opcode2, modrm]);
    }

    /// Single r/m64 operand with an opcode-extension digit in ModRM.reg
    /// (`/digit` forms: `NEG`/`DIV`/`IDIV` = 0xF7, `CALL` = 0xFF, `SHL`/
    /// `SHR`/`SAR` by CL = 0xD3).
    fn emit_m_ext(&mut self, opcode: u8, ext: u8, rm: u32) {
        let rex = rex(true, false, false, rm >= 8);
        let modrm = 0xC0 | (ext << 3) | ((rm & 7) as u8);
        self.buf.emit_bytes(&[rex, opcode, modrm]);
    }

    /// Correctly computes `rd = rn OP rm` for x86's destructive 2-operand
    /// ALU instructions, handling every register-aliasing case a caller
    /// might pass (this emitter's own callers, `compiler.rs`'s fixed
    /// register-role slots, do alias in at least one real call site --
    /// `PtrAdd`'s `add_reg(RESULT, A_VAL, RESULT)` has `rd == rm`):
    /// - `rd == rn`: emit `OP rd, rm` directly.
    /// - `rd == rm` and `commutative`: swap to `OP rd, rn` (`rd` already
    ///   holds what was logically `rm`; commutativity makes this equal to
    ///   the requested `rn OP rm`).
    /// - `rd == rm`, not commutative (only `sub_reg` reaches this, since
    ///   every other caller of this helper is commutative): route through
    ///   [`SCRATCH`] so `rn`'s value survives long enough to be read.
    /// - otherwise: `mov rd,rn; OP rd,rm`.
    fn binop_general(&mut self, opcode: u8, rd: u32, rn: u32, rm: u32, commutative: bool) {
        if rd == rn {
            self.emit_rr(opcode, rd, rm);
        } else if rd == rm && commutative {
            self.emit_rr(opcode, rd, rn);
        } else if rd == rm {
            self.mov_reg(SCRATCH, rn);
            self.emit_rr(opcode, SCRATCH, rm);
            self.mov_reg(rd, SCRATCH);
        } else {
            self.mov_reg(rd, rn);
            self.emit_rr(opcode, rd, rm);
        }
    }

    pub fn add_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.binop_general(0x01, rd, rn, rm, true);
    }

    /// `rd = rn - rm`. `rn == `[`XZR_SENTINEL`] (compiler.rs's
    /// AArch64-flavored "zero register" convention, reused as a cross-arch
    /// sentinel -- see that constant's doc) synthesizes `0 - rm` via `NEG`,
    /// since x86 has no literal zero register to read from directly.
    pub fn sub_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        if rn == XZR_SENTINEL {
            self.mov_reg(rd, rm);
            self.emit_m_ext(0xF7, 3, rd); // NEG rd
            return;
        }
        self.binop_general(0x29, rd, rn, rm, false);
    }

    pub fn and_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.binop_general(0x21, rd, rn, rm, true);
    }

    pub fn orr_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.binop_general(0x09, rd, rn, rm, true);
    }

    pub fn eor_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.binop_general(0x31, rd, rn, rm, true);
    }

    /// `rd = rn * rm` (`IMUL r64,r/m64`, two-operand form -- commutative,
    /// so the `rd == rm` case is a plain operand swap, no [`SCRATCH`]
    /// needed unlike [`Self::binop_general`]'s non-commutative path).
    pub fn mul_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        if rd == rn {
            self.emit_rm(0xAF, rd, rm);
        } else if rd == rm {
            self.emit_rm(0xAF, rd, rn);
        } else {
            self.mov_reg(rd, rn);
            self.emit_rm(0xAF, rd, rm);
        }
    }

    /// `rd = rn / rm` (unsigned). x86 has no 3-operand DIV: dividend is
    /// the fixed `RDX:RAX` pair, quotient lands in RAX, remainder in RDX.
    /// Always clobbers RAX/RDX as scratch -- safe for every real caller in
    /// `compiler.rs`, whose `rd`/`rn`/`rm` are always its named
    /// callee-saved slots (`A_VAL`/`B_VAL`/`RESULT`, mapped to R12/R13/R14
    /// here), never RAX/RDX.
    pub fn udiv_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.mov_reg(RAX, rn);
        // xor edx,edx (32-bit form auto-zero-extends to 64-bit RDX; zeroes
        // the dividend's high half for unsigned division).
        self.buf.emit_bytes(&[0x31, 0xD2]);
        self.emit_m_ext(0xF7, 6, rm); // DIV rm
        self.mov_reg(rd, RAX);
    }

    /// `rd = rn / rm` (signed, truncating toward 0). Same RDX:RAX shape as
    /// [`Self::udiv_reg`], but sign-extends via `CQO` instead of zeroing.
    pub fn sdiv_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.mov_reg(RAX, rn);
        self.buf.emit_bytes(&[0x48, 0x99]); // CQO: sign-extend RAX into RDX:RAX
        self.emit_m_ext(0xF7, 7, rm); // IDIV rm
        self.mov_reg(rd, RAX);
    }

    /// `rd = ra - rn*rm` (used to compute a remainder as `a - (a/b)*b`
    /// after a preceding `udiv_reg`/`sdiv_reg`, same as aarch64's `MSUB` --
    /// x86 has no single instruction for this). Computes `rn*rm` into RAX
    /// *before* touching `rd`, so this is correct even when `rd` aliases
    /// `rn` (the real call shape: `msub_reg(RESULT, RESULT, B_VAL, A_VAL)`)
    /// -- `rm`/`ra` are read directly (IMUL's r/m operand, then a plain
    /// mov), never through `rd`, so no aliasing hazard there either.
    pub fn msub_reg(&mut self, rd: u32, rn: u32, rm: u32, ra: u32) {
        self.mov_reg(RAX, rn);
        self.emit_rm(0xAF, RAX, rm); // RAX = rn * rm
        self.mov_reg(rd, ra);
        self.emit_rr(0x29, rd, RAX); // rd -= RAX  =>  rd = ra - rn*rm
    }

    /// Shared shape for `SHL`/`SHR`/`SAR r/m64, CL` (x86's register-count
    /// shift forms only ever read the count from CL, unlike AArch64's
    /// LSLV/LSRV/ASRV which take any register) -- safe to unconditionally
    /// clobber RCX here since none of `compiler.rs`'s named persistent
    /// slots (`EMU_PTR`/`A_VAL`/`B_VAL`/`RESULT`/`CALLEE_ADDR`/`TMP1`/
    /// `TMP2`) are ever mapped to RCX, and shifts never execute
    /// mid-call-argument-setup (the only other place RCX is live, as
    /// `ARG3`).
    fn shift_by_reg(&mut self, ext: u8, rd: u32, rn: u32, rm: u32) {
        if rd != rn {
            self.mov_reg(rd, rn);
        }
        self.mov_reg(RCX, rm);
        self.emit_m_ext(0xD3, ext, rd);
    }

    pub fn lsl_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.shift_by_reg(4, rd, rn, rm); // SHL
    }

    pub fn lsr_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.shift_by_reg(5, rd, rn, rm); // SHR
    }

    pub fn asr_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.shift_by_reg(7, rd, rn, rm); // SAR
    }

    /// Count leading zero bits, 64-bit. **Only valid for a non-zero
    /// input** -- unlike aarch64's hardware `CLZ` (well-defined at 0),
    /// this is built on `BSR` (bit-scan-reverse), whose result is
    /// undefined for a zero operand per the Intel SDM. Deliberately not
    /// using `LZCNT` (which *is* well-defined at 0) despite that being a
    /// better match for aarch64's `clz_reg`: `LZCNT` requires the BMI1
    /// feature, not guaranteed present on every x86-64 CPU (a real
    /// portability difference from `BSR`, part of the base ISA since the
    /// original x86-64 spec), and `selfjit::compiler`'s own `LzCount` arm
    /// already guards every call site with an explicit zero-check branch
    /// before trusting this result (see that arm's doc comment) -- so the
    /// stricter, always-available primitive is the right choice here, not
    /// a limitation this call site actually hits.
    /// `clz64(x) = 63 - bsr(x)` for `x != 0`.
    pub fn clz_reg(&mut self, rd: u32, rn: u32) {
        self.emit_rm(0xBD, rd, rn); // BSR rd, rn
        self.emit_m_ext(0xF7, 3, rd); // NEG rd
        self.add_imm(rd, rd, 63); // ADD rd, 63  =>  rd = 63 - bsr(rn)
    }

    /// `rd = rn - imm` (`SUB r/m64, imm32`, opcode 0x81 /5, sign-extended
    /// -- covers `imm12`'s full range and then some; not worth a separate
    /// imm8-only fast path for this emitter's call volume). Used directly
    /// by `compiler.rs`'s `LzCount` arm (not just aarch64's frame setup,
    /// unlike `ldr_imm`/`str_imm` below, which really are aarch64-frame-
    /// only) -- must be a real implementation, not a stub.
    pub fn sub_imm(&mut self, rd: u32, rn: u32, imm12: u32) {
        if rd != rn {
            self.mov_reg(rd, rn);
        }
        self.emit_alu_imm32(5, rd, imm12); // /5 = SUB
    }

    pub fn add_imm(&mut self, rd: u32, rn: u32, imm12: u32) {
        if rd != rn {
            self.mov_reg(rd, rn);
        }
        self.emit_alu_imm32(0, rd, imm12); // /0 = ADD
    }

    fn emit_alu_imm32(&mut self, ext: u8, rd: u32, imm32: u32) {
        let rex = rex(true, false, false, rd >= 8);
        let modrm = 0xC0 | (ext << 3) | ((rd & 7) as u8);
        let mut bytes = vec![rex, 0x81, modrm];
        bytes.extend_from_slice(&imm32.to_le_bytes());
        self.buf.emit_bytes(&bytes);
    }

    /// `cmp rn, rm` (`CMP r/m64,r64`, opcode 0x39: computes `r/m - reg`
    /// and discards the result, only setting flags) -- `r/m = rn`, `reg =
    /// rm`, so this sets flags for `rn - rm`, matching aarch64's
    /// `cmp_reg(rn,rm)` direction exactly. No destination register is
    /// written, so unlike [`Self::binop_general`] there is no aliasing
    /// hazard to handle here.
    ///
    /// `rm == `[`XZR_SENTINEL`] (compiler.rs's `CBranch`/`LzCount` arms
    /// both compare against it) needs the same special-casing
    /// [`Self::sub_reg`] gives it -- there is no real x86 register 31 to
    /// read, so this emits `CMP rn, 0` (immediate form) instead of trying
    /// to encode a nonexistent register operand, which would silently
    /// compare against whatever real register (15/R15) register-31's low
    /// 3 bits happened to alias -- confirmed as a real bug this way, not
    /// theoretical: `LzCount`'s own zero-input branch took the wrong path
    /// under this exact scenario before this fix.
    pub fn cmp_reg(&mut self, rn: u32, rm: u32) {
        if rm == XZR_SENTINEL {
            let rex = rex(true, false, false, rn >= 8);
            let modrm = 0xC0 | (7 << 3) | ((rn & 7) as u8); // /7 = CMP
            self.buf.emit_bytes(&[rex, 0x83, modrm, 0]);
            return;
        }
        self.emit_rr(0x39, rn, rm);
    }

    pub fn ldr_imm(&mut self, _rt: u32, _rn: u32, _imm: u32) {
        todo!("unused on x86-64 -- see module doc on prologue/epilogue")
    }

    pub fn str_imm(&mut self, _rt: u32, _rn: u32, _imm: u32) {
        todo!("unused on x86-64 -- see module doc on prologue/epilogue")
    }

    /// `push rn` (SysV64 frame setup only -- see module doc).
    pub fn push_reg(&mut self, rn: u32) {
        let mut bytes = Vec::new();
        if rn >= 8 {
            bytes.push(rex(false, false, false, true));
        }
        bytes.push(0x50 + (rn & 7) as u8);
        self.buf.emit_bytes(&bytes);
    }

    /// `pop rn`.
    pub fn pop_reg(&mut self, rn: u32) {
        let mut bytes = Vec::new();
        if rn >= 8 {
            bytes.push(rex(false, false, false, true));
        }
        bytes.push(0x58 + (rn & 7) as u8);
        self.buf.emit_bytes(&bytes);
    }

    /// `sub rsp, #imm8` -- stack-alignment padding in the x86-64 prologue
    /// (see module doc: PUSH-only callee-saved saves leave RSP
    /// misaligned for subsequent `call`s by exactly 8 bytes).
    pub fn sub_rsp_imm8(&mut self, imm8: u8) {
        let rex = rex(true, false, false, false);
        self.buf.emit_bytes(&[rex, 0x83, 0xEC, imm8]); // SUB RSP, imm8 (/5)
    }

    pub fn add_rsp_imm8(&mut self, imm8: u8) {
        let rex = rex(true, false, false, false);
        self.buf.emit_bytes(&[rex, 0x83, 0xC4, imm8]); // ADD RSP, imm8 (/0)
    }

    /// `call rn` (`CALL r/m64`, opcode 0xFF /2 -- near indirect call
    /// through a register; operand size is forced to 64 bits in long mode
    /// regardless of REX.W, but including REX.W anyway is harmless and
    /// keeps this emitter's REX computation uniform).
    pub fn blr(&mut self, rn: u32) {
        self.emit_m_ext(0xFF, 2, rn);
    }

    pub fn ret(&mut self) {
        self.buf.emit_bytes(&[0xC3]);
    }

    /// Reserves a fixed 6-byte slot (the width of a `Jcc rel32`, the
    /// larger of the two forms this can become) regardless of which of
    /// [`Self::patch_b_cond`]/[`Self::patch_b`] eventually resolves it --
    /// x86 instructions are variable-length (5-byte `JMP rel32` vs 6-byte
    /// `Jcc rel32`), unlike aarch64's uniform 4-byte instructions, so a
    /// single reserved width has to be picked up front; [`Self::patch_b`]
    /// pads its shorter 5-byte `JMP` with a trailing 1-byte `NOP` to fill
    /// the same 6 bytes, keeping every offset computed after this call
    /// consistent with what's actually encoded here once patched.
    pub fn placeholder(&mut self) -> Label {
        let at = self.buf.offset();
        self.buf.emit_bytes(&[0u8; 6]);
        Label(at)
    }

    pub fn b_cond(&mut self, cond: Cond, target: usize) {
        let at = self.buf.offset();
        let bytes = Self::encode_jcc(cond, at, target);
        self.buf.emit_bytes(&bytes);
    }

    pub fn patch_b_cond(&mut self, label: Label, cond: Cond, target: usize) {
        let bytes = Self::encode_jcc(cond, label.0, target);
        self.buf.patch_bytes(label.0, &bytes);
    }

    /// `Jcc rel32` -- 6 bytes: `0F 8<cc>` + a 4-byte little-endian
    /// displacement relative to the address of the *next* instruction
    /// (`at + 6`), not `at` itself -- x86's rel32 convention, unlike
    /// aarch64's imm19-relative-to-the-branch's-own-address (divided by
    /// 4).
    fn encode_jcc(cond: Cond, at: usize, target: usize) -> [u8; 6] {
        let next_insn = at + 6;
        let rel32 = (target as i64 - next_insn as i64) as i32 as u32;
        let mut bytes = [0u8; 6];
        bytes[0] = 0x0F;
        bytes[1] = 0x80 | cond.cc_bits();
        bytes[2..6].copy_from_slice(&rel32.to_le_bytes());
        bytes
    }

    pub fn b(&mut self, target: usize) {
        let at = self.buf.offset();
        let bytes = Self::encode_jmp(at, target);
        self.buf.emit_bytes(&bytes);
    }

    /// Resolves a [`Self::placeholder`] slot as an unconditional jump: a
    /// 5-byte `JMP rel32` followed by a 1-byte `NOP` padding out to the
    /// reserved 6-byte width (see [`Self::placeholder`]'s doc).
    pub fn patch_b(&mut self, label: Label, target: usize) {
        let jmp = Self::encode_jmp(label.0, target);
        let mut bytes = [0u8; 6];
        bytes[..5].copy_from_slice(&jmp);
        bytes[5] = 0x90; // NOP
        self.buf.patch_bytes(label.0, &bytes);
    }

    /// `JMP rel32` -- 5 bytes: `E9` + a 4-byte displacement relative to
    /// `at + 5` (the next instruction's address).
    fn encode_jmp(at: usize, target: usize) -> [u8; 5] {
        let next_insn = at + 5;
        let rel32 = (target as i64 - next_insn as i64) as i32 as u32;
        let mut bytes = [0u8; 5];
        bytes[0] = 0xE9;
        bytes[1..5].copy_from_slice(&rel32.to_le_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selfjit::codebuf::CodeBuffer;

    #[test]
    fn mov_imm64_add_and_ret_roundtrip() {
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(RET, 0x1234_5678_9ABC_DEF0);
            asm.mov_imm64(R12, 1);
            asm.add_reg(RET, RET, R12);
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 0x1234_5678_9ABC_DEF1);
    }

    #[test]
    fn cmp_and_conditional_branch() {
        // if (x == y) return 111; else return 222; -- same shape and same
        // real bug class aarch64.rs's own version of this test caught
        // (a placeholder patched to branch to its own fallthrough, a
        // silent always-taken-the-same-way no-op) -- exercises both arms.
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(RET, 7);
            asm.mov_imm64(R12, 7);
            asm.cmp_reg(RET, R12);
            let branch_to_else = asm.placeholder();
            asm.mov_imm64(RET, 111);
            let branch_to_end = asm.placeholder();
            let else_start = asm.offset();
            asm.patch_b_cond(branch_to_else, Cond::Ne, else_start);
            asm.mov_imm64(RET, 222);
            let end = asm.offset();
            asm.patch_b(branch_to_end, end);
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 111);
    }

    #[test]
    fn cmp_and_conditional_branch_false_arm() {
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(RET, 3);
            asm.mov_imm64(R12, 7);
            asm.cmp_reg(RET, R12);
            let branch_to_else = asm.placeholder();
            asm.mov_imm64(RET, 111);
            let branch_to_end = asm.placeholder();
            let else_start = asm.offset();
            asm.patch_b_cond(branch_to_else, Cond::Ne, else_start);
            asm.mov_imm64(RET, 222);
            let end = asm.offset();
            asm.patch_b(branch_to_end, end);
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 222);
    }

    /// Register-aliasing case [`Asm::binop_general`]'s doc calls out by
    /// name (`rd == rm`, non-commutative op) -- `sub_reg(RET, R12, RET)`
    /// computes `12 - 5 = 7`, not `5 - 12` or some corrupted value from
    /// clobbering `rm` before it's read.
    #[test]
    fn sub_reg_handles_rd_aliasing_rm() {
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(R12, 12);
            asm.mov_imm64(RET, 5);
            asm.sub_reg(RET, R12, RET); // RET = R12 - RET = 12 - 5
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 7);
    }

    /// `udiv_reg`, then `lsl_reg` with the shift-amount register aliasing
    /// the value register (`rn == rm`) -- both real shapes exercised
    /// elsewhere in this file's ALU/shift helpers.
    #[test]
    fn udiv_then_shift_with_aliased_operands() {
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(R12, 17);
            asm.mov_imm64(R13, 5);
            asm.udiv_reg(R14, R12, R13); // R14 = 17/5 = 3
            asm.lsl_reg(RET, R14, R14); // RET = 3 << 3 = 24
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 24);
    }

    /// `sdiv_reg` (signed, truncating toward 0) and `msub_reg` (remainder
    /// via multiply-subtract, matching aarch64's lack of a native
    /// remainder instruction) with the exact aliasing shape
    /// `compiler.rs`'s `IntSRem` arm actually uses:
    /// `msub_reg(RESULT, RESULT, B_VAL, A_VAL)` (`rd == rn`).
    #[test]
    fn sdiv_and_msub_remainder() {
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(R12, (-17i64) as u64); // a = -17
            asm.mov_imm64(R13, 5); // b = 5
            asm.sdiv_reg(R14, R12, R13); // R14 = -17/5 = -3 (trunc toward 0)
            asm.msub_reg(R14, R14, R13, R12); // R14 = a - quotient*b = -17 - (-3*5) = -2
            asm.mov_reg(RET, R14);
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f() as i64, -17i64 % 5);
    }

    /// `clz_reg` (BSR-based) against known non-zero cases -- the one
    /// primitive this file's own doc flags as invalid at 0, so this
    /// deliberately never passes a zero operand (matching how
    /// `compiler.rs`'s `LzCount` arm actually calls it, always behind an
    /// explicit zero-check branch).
    #[test]
    fn clz_matches_known_nonzero_cases() {
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(R12, 1);
            asm.clz_reg(RET, R12); // clz(1) == 63
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 63);

        let mut buf2 = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf2);
            asm.mov_imm64(R12, 0x80);
            asm.clz_reg(RET, R12); // clz(0x80) == 56
            asm.ret();
        }
        let code2 = buf2.finish().expect("finish");
        let f2: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code2.as_ptr()) };
        assert_eq!(f2(), 56);
    }

    /// `push_reg`/`pop_reg`/`sub_rsp_imm8`/`add_rsp_imm8` -- the x86-64
    /// prologue/epilogue's own primitives (see module doc), round-tripped
    /// directly: save two callee-saved-role registers to the stack,
    /// clobber them, restore, verify the restored values.
    #[test]
    fn push_pop_roundtrip_with_rsp_padding() {
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(R12, 0xAAAA);
            asm.mov_imm64(R13, 0xBBBB);
            asm.push_reg(R12);
            asm.push_reg(R13);
            asm.sub_rsp_imm8(8);
            // Clobber both -- proves the restore below reads real saved
            // values back, not leftover register contents.
            asm.mov_imm64(R12, 0);
            asm.mov_imm64(R13, 0);
            asm.add_rsp_imm8(8);
            asm.pop_reg(R13);
            asm.pop_reg(R12);
            asm.add_reg(RET, R12, R13);
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 0xAAAA + 0xBBBB);
    }
}
