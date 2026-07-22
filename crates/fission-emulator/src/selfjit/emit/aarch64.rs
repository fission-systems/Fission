//! Minimal AArch64 host-code assembler.
//!
//! Hand-encodes a small, real subset of A64 -- enough to prove the whole
//! `selfjit` pipeline (p-code -> machine code -> mmap RX -> call -> correct
//! result) end to end, not a complete assembler. Every encoding below is
//! checked against the *executed* result in `compiler.rs`'s integration
//! test, not just "looks right" -- see that test for what's actually
//! verified.
//!
//! Register numbering matches AAPCS64: X0-X7 argument/scratch, X9-X15
//! caller-saved scratch, X19-X28 callee-saved, X29 frame pointer, X30 link
//! register, SP/XZR is 31 (context-dependent, not modeled separately here
//! since nothing below needs the SP-vs-XZR distinction).
//!
//! Reference: ARM Architecture Reference Manual for A-profile architecture,
//! section C4 (A64 base instruction set encoding).

use super::Label;
use crate::selfjit::codebuf::CodeBuffer;

pub const X0: u32 = 0;
pub const X1: u32 = 1;
pub const X2: u32 = 2;
pub const X3: u32 = 3;
pub const X4: u32 = 4;
/// Callee-saved -- used to hold `*mut Emulator` across the whole TB body,
/// including calls out to `jit_read_space`/`jit_write_space`-style
/// callbacks (which per AAPCS64 may clobber X0-X18 but must preserve
/// X19-X28). `compiler.rs` also uses X20-X22 as its fixed value slots for
/// the same reason -- see that file's own doc comment on why they must be
/// callee-saved, not X9-X15.
pub const X19: u32 = 19;
pub const X20: u32 = 20;
pub const X21: u32 = 21;
pub const X22: u32 = 22;
pub const X30_LR: u32 = 30;

// Generic cross-arch role constants `compiler.rs` imports without naming a
// concrete arch module (see `selfjit::emit::x86_64`'s own doc comment on
// why `ARG0` and `RET` must be genuinely distinct constants on that arch,
// even though they're both just `X0` here -- AAPCS64 uses the same
// register for a call's first argument and its return value; SysV64 does
// not).
pub const ARG0: u32 = X0;
pub const ARG1: u32 = X1;
pub const ARG2: u32 = X2;
pub const ARG3: u32 = X3;
pub const ARG4: u32 = X4;
pub const RET: u32 = X0;
pub const EMU_PTR_SLOT: u32 = X19;
pub const A_VAL_SLOT: u32 = X20;
pub const B_VAL_SLOT: u32 = X21;
pub const RESULT_SLOT: u32 = X22;
/// Stack pointer, for `add_imm`/`sub_imm`-based scratch-buffer address
/// computation (`compiler.rs`'s `scratch_buf_addr`). Register 31 is
/// context-dependent on AArch64: `ADD`/`SUB` *immediate* forms (what
/// `add_imm`/`sub_imm` emit) treat it as SP, but most other encodings
/// (e.g. `mov_reg`'s `ORR`-based alias) treat it as the zero register
/// (XZR) instead -- this constant is only ever meant to be passed through
/// `add_imm`/`sub_imm`, never `mov_reg`, or it would silently compute
/// from 0 instead of SP.
pub const SP: u32 = 31;

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
    fn bits(self) -> u32 {
        match self {
            Cond::Eq => 0b0000,
            Cond::Ne => 0b0001,
            Cond::Cs => 0b0010,
            Cond::Cc => 0b0011,
            Cond::Lt => 0b1011,
            Cond::Le => 0b1101,
            Cond::Gt => 0b1100,
            Cond::Ge => 0b1010,
            Cond::Ls => 0b1001,
            Cond::Hi => 0b1000,
        }
    }

    /// The logical negation of this condition (A64 pairs conditions by
    /// flipping the low bit of their 4-bit encoding, AL/NV excepted --
    /// irrelevant here since neither is ever constructed).
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

impl<'a> Asm<'a> {
    pub fn new(buf: &'a mut CodeBuffer) -> Self {
        Self { buf }
    }

    pub fn offset(&self) -> usize {
        self.buf.offset()
    }

    /// Load a 64-bit immediate into `rd` via up to 4 MOVZ/MOVK instructions
    /// (one per non-zero 16-bit chunk; always emits at least a MOVZ).
    pub fn mov_imm64(&mut self, rd: u32, imm: u64) {
        let chunks = [
            (imm & 0xFFFF) as u32,
            ((imm >> 16) & 0xFFFF) as u32,
            ((imm >> 32) & 0xFFFF) as u32,
            ((imm >> 48) & 0xFFFF) as u32,
        ];
        let mut first = true;
        for (hw, &chunk) in chunks.iter().enumerate() {
            if chunk == 0 && !(first && hw == 3) {
                continue;
            }
            if first {
                self.movz(rd, chunk, hw as u32);
                first = false;
            } else {
                self.movk(rd, chunk, hw as u32);
            }
        }
        if first {
            // imm == 0: MOVZ Xd, #0 wasn't emitted by the loop above.
            self.movz(rd, 0, 0);
        }
    }

    fn movz(&mut self, rd: u32, imm16: u32, hw: u32) {
        self.buf
            .emit_u32_le(0xD2800000 | (hw << 21) | (imm16 << 5) | rd);
    }

    fn movk(&mut self, rd: u32, imm16: u32, hw: u32) {
        self.buf
            .emit_u32_le(0xF2800000 | (hw << 21) | (imm16 << 5) | rd);
    }

    /// `mov xd, xn` (alias for `orr xd, xzr, xn`).
    pub fn mov_reg(&mut self, rd: u32, rn: u32) {
        self.buf.emit_u32_le(0xAA0003E0 | (rn << 16) | rd);
    }

    pub fn add_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf.emit_u32_le(0x8B000000 | (rm << 16) | (rn << 5) | rd);
    }

    pub fn sub_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf.emit_u32_le(0xCB000000 | (rm << 16) | (rn << 5) | rd);
    }

    pub fn and_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf.emit_u32_le(0x8A000000 | (rm << 16) | (rn << 5) | rd);
    }

    pub fn orr_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf.emit_u32_le(0xAA000000 | (rm << 16) | (rn << 5) | rd);
    }

    pub fn eor_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf.emit_u32_le(0xCA000000 | (rm << 16) | (rn << 5) | rd);
    }

    /// `mul xd, xn, xm` (alias for `madd xd, xn, xm, xzr`).
    pub fn mul_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf
            .emit_u32_le(0x9B007C00 | (rm << 16) | (rn << 5) | rd);
    }

    /// `udiv xd, xn, xm` (unsigned integer division, truncating).
    pub fn udiv_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf
            .emit_u32_le(0x9AC00800 | (rm << 16) | (rn << 5) | rd);
    }

    /// `sdiv xd, xn, xm` (signed integer division, truncating toward 0).
    pub fn sdiv_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf
            .emit_u32_le(0x9AC00C00 | (rm << 16) | (rn << 5) | rd);
    }

    /// `msub xd, xn, xm, xa` (xd = xa - xn*xm) -- used to compute a
    /// remainder as `a - (a/b)*b` after a preceding `udiv_reg`/`sdiv_reg`
    /// (AArch64 has no direct remainder instruction).
    pub fn msub_reg(&mut self, rd: u32, rn: u32, rm: u32, ra: u32) {
        self.buf
            .emit_u32_le(0x9B008000 | (rm << 16) | (ra << 10) | (rn << 5) | rd);
    }

    /// `lslv xd, xn, xm` (logical shift left by a register-held amount).
    pub fn lsl_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf
            .emit_u32_le(0x9AC02000 | (rm << 16) | (rn << 5) | rd);
    }

    /// `lsrv xd, xn, xm` (logical shift right by a register-held amount).
    pub fn lsr_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf
            .emit_u32_le(0x9AC02400 | (rm << 16) | (rn << 5) | rd);
    }

    /// `asrv xd, xn, xm` (arithmetic shift right by a register-held
    /// amount).
    pub fn asr_reg(&mut self, rd: u32, rn: u32, rm: u32) {
        self.buf
            .emit_u32_le(0x9AC02800 | (rm << 16) | (rn << 5) | rd);
    }

    /// `clz xd, xn` (count leading zero bits, 64-bit form). Well-defined
    /// for a zero input -- `clz(0) == 64`, unlike x86's BSR -- so callers
    /// needing a p-code-width-relative count can compute
    /// `clz_reg(x) - (64 - width)` directly without special-casing zero.
    pub fn clz_reg(&mut self, rd: u32, rn: u32) {
        self.buf.emit_u32_le(0xDAC01000 | (rn << 5) | rd);
    }

    /// `sub xd, xn, #imm12` (imm12 unsigned, 0..=4095, unscaled).
    pub fn sub_imm(&mut self, rd: u32, rn: u32, imm12: u32) {
        debug_assert!(imm12 <= 0xFFF);
        self.buf.emit_u32_le(0xD1000000 | (imm12 << 10) | (rn << 5) | rd);
    }

    /// `add xd, xn, #imm12` (imm12 unsigned, 0..=4095, unscaled).
    pub fn add_imm(&mut self, rd: u32, rn: u32, imm12: u32) {
        debug_assert!(imm12 <= 0xFFF);
        self.buf.emit_u32_le(0x91000000 | (imm12 << 10) | (rn << 5) | rd);
    }

    /// `cmp xn, xm` (alias for `subs xzr, xn, xm`) -- sets NZCV for a
    /// following `Cond`-conditioned branch/select.
    pub fn cmp_reg(&mut self, rn: u32, rm: u32) {
        self.buf.emit_u32_le(0xEB00001F | (rm << 16) | (rn << 5));
    }

    /// `ldr xt, [xn, #imm]` (imm unsigned, 0..=32760, must be 8-aligned).
    pub fn ldr_imm(&mut self, rt: u32, rn: u32, imm: u32) {
        debug_assert_eq!(imm % 8, 0);
        self.buf
            .emit_u32_le(0xF9400000 | ((imm / 8) << 10) | (rn << 5) | rt);
    }

    /// `str xt, [xn, #imm]` (imm unsigned, 0..=32760, must be 8-aligned).
    pub fn str_imm(&mut self, rt: u32, rn: u32, imm: u32) {
        debug_assert_eq!(imm % 8, 0);
        self.buf
            .emit_u32_le(0xF9000000 | ((imm / 8) << 10) | (rn << 5) | rt);
    }

    /// `blr xn` -- call through a register (used to reach a fixed host
    /// callback address loaded via `mov_imm64`).
    pub fn blr(&mut self, rn: u32) {
        self.buf.emit_u32_le(0xD63F0000 | (rn << 5));
    }

    pub fn ret(&mut self) {
        self.buf.emit_u32_le(0xD65F03C0);
    }

    /// Reserve one instruction slot for a not-yet-known branch target and
    /// return a [`Label`] to [`Self::bind`] later, or resolve immediately
    /// via [`Self::patch_b_cond`]/[`Self::patch_b`] once the target offset
    /// is known (backward branches: target already emitted, so callers
    /// that already have the target offset should skip the label dance and
    /// emit the real instruction directly).
    pub fn placeholder(&mut self) -> Label {
        let at = self.buf.offset();
        self.buf.emit_u32_le(0); // overwritten by a later patch_* call
        Label(at)
    }

    /// `b.cond <target>` -- `target` is an absolute buffer offset (already
    /// known: backward branch, or resolved via [`Self::placeholder`] +
    /// patch for forward branches).
    pub fn b_cond(&mut self, cond: Cond, target: usize) {
        let at = self.buf.offset();
        let word = Self::encode_b_cond(cond, at, target);
        self.buf.emit_u32_le(word);
    }

    pub fn patch_b_cond(&mut self, label: Label, cond: Cond, target: usize) {
        let word = Self::encode_b_cond(cond, label.0, target);
        self.buf.patch_u32_le(label.0, word);
    }

    fn encode_b_cond(cond: Cond, at: usize, target: usize) -> u32 {
        let imm19 = Self::pc_rel_imm19(at, target);
        0x54000000 | ((imm19 & 0x7FFFF) << 5) | cond.bits()
    }

    pub fn b(&mut self, target: usize) {
        let at = self.buf.offset();
        let imm26 = Self::pc_rel_imm26(at, target);
        self.buf.emit_u32_le(0x14000000 | (imm26 & 0x3FFFFFF));
    }

    pub fn patch_b(&mut self, label: Label, target: usize) {
        let imm26 = Self::pc_rel_imm26(label.0, target);
        self.buf.patch_u32_le(label.0, 0x14000000 | (imm26 & 0x3FFFFFF));
    }

    fn pc_rel_imm19(at: usize, target: usize) -> u32 {
        let delta = target as i64 - at as i64;
        debug_assert_eq!(delta % 4, 0, "branch target must be 4-byte aligned");
        ((delta / 4) as i32 as u32) & 0x7FFFF
    }

    fn pc_rel_imm26(at: usize, target: usize) -> u32 {
        let delta = target as i64 - at as i64;
        debug_assert_eq!(delta % 4, 0, "branch target must be 4-byte aligned");
        ((delta / 4) as i32 as u32) & 0x3FFFFFF
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
            asm.mov_imm64(X0, 0x1234_5678_9ABC_DEF0);
            asm.mov_imm64(X1, 1);
            asm.add_reg(X0, X0, X1);
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 0x1234_5678_9ABC_DEF1);
    }

    #[test]
    fn cmp_and_conditional_branch() {
        // if (x0 == x1) return 111; else return 222;
        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            asm.mov_imm64(X0, 7);
            asm.mov_imm64(X1, 7);
            asm.cmp_reg(X0, X1);
            let branch_to_else = asm.placeholder();
            // taken when NOT equal -> jump to else
            let else_target_unknown_yet = branch_to_else;
            // then-arm: EQ falls through here
            asm.mov_imm64(X0, 111);
            let branch_to_end = asm.placeholder();
            let else_start = asm.offset();
            asm.patch_b_cond(else_target_unknown_yet, Cond::Ne, else_start);
            asm.mov_imm64(X0, 222);
            let end = asm.offset();
            asm.patch_b(branch_to_end, end);
            asm.ret();
        }
        let code = buf.finish().expect("finish");
        let f: extern "C" fn() -> u64 = unsafe { std::mem::transmute(code.as_ptr()) };
        assert_eq!(f(), 111);
    }
}
