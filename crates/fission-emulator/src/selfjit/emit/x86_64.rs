//! x86-64 host emitter -- **stub**, not implemented.
//!
//! Mirrors [`super::aarch64::Asm`]'s shape (same method names/signatures)
//! so `compiler.rs` doesn't need `#[cfg]` branches for the two hosts, but
//! every method here panics. Filling this in is real follow-up work for
//! whoever picks this up on an x86-64 dev machine, where it can actually
//! be tested the same way `aarch64.rs`'s own unit tests validate
//! encodings today (mmap the generated code, call it, check the result --
//! not just "looks right").
//!
//! Encoding reference for when this gets implemented: Intel SDM Vol. 2,
//! or (much easier going for a hand-rolled emitter) the opcode tables in
//! <https://www.felixcloutier.com/x86/>. Cranelift's own
//! `cranelift-codegen/src/isa/x64/inst/emit.rs` is also a real, working
//! reference for exactly this kind of REX-prefix/ModRM encoding, though
//! it's solving a much bigger problem (full register allocation) than
//! this crate's fixed-register, no-allocator approach needs.

use super::Label;
use crate::selfjit::codebuf::CodeBuffer;

pub const X0: u32 = 0; // rax
pub const X1: u32 = 1; // rcx (arg placement TODO: doesn't match SysV yet)
pub const X2: u32 = 2;
pub const X3: u32 = 3;
pub const X4: u32 = 4;
pub const X19: u32 = 19; // TODO: map to a real callee-saved GPR (e.g. rbx/r12)
pub const X30_LR: u32 = 30;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Cs,
    Cc,
    Lt,
    Le,
    Gt,
    Ge,
}

pub struct Asm<'a> {
    _buf: &'a mut CodeBuffer,
}

impl<'a> Asm<'a> {
    pub fn new(buf: &'a mut CodeBuffer) -> Self {
        Self { _buf: buf }
    }

    pub fn offset(&self) -> usize {
        todo!("x86_64 emitter not implemented -- see module docs")
    }
    pub fn mov_imm64(&mut self, _rd: u32, _imm: u64) {
        todo!("x86_64 emitter not implemented -- see module docs")
    }
    pub fn mov_reg(&mut self, _rd: u32, _rn: u32) {
        todo!()
    }
    pub fn add_reg(&mut self, _rd: u32, _rn: u32, _rm: u32) {
        todo!()
    }
    pub fn sub_reg(&mut self, _rd: u32, _rn: u32, _rm: u32) {
        todo!()
    }
    pub fn and_reg(&mut self, _rd: u32, _rn: u32, _rm: u32) {
        todo!()
    }
    pub fn orr_reg(&mut self, _rd: u32, _rn: u32, _rm: u32) {
        todo!()
    }
    pub fn eor_reg(&mut self, _rd: u32, _rn: u32, _rm: u32) {
        todo!()
    }
    pub fn sub_imm(&mut self, _rd: u32, _rn: u32, _imm12: u32) {
        todo!()
    }
    pub fn add_imm(&mut self, _rd: u32, _rn: u32, _imm12: u32) {
        todo!()
    }
    pub fn cmp_reg(&mut self, _rn: u32, _rm: u32) {
        todo!()
    }
    pub fn ldr_imm(&mut self, _rt: u32, _rn: u32, _imm: u32) {
        todo!()
    }
    pub fn str_imm(&mut self, _rt: u32, _rn: u32, _imm: u32) {
        todo!()
    }
    pub fn blr(&mut self, _rn: u32) {
        todo!()
    }
    pub fn ret(&mut self) {
        todo!()
    }
    pub fn placeholder(&mut self) -> Label {
        todo!()
    }
    pub fn b_cond(&mut self, _cond: Cond, _target: usize) {
        todo!()
    }
    pub fn patch_b_cond(&mut self, _label: Label, _cond: Cond, _target: usize) {
        todo!()
    }
    pub fn b(&mut self, _target: usize) {
        todo!()
    }
    pub fn patch_b(&mut self, _label: Label, _target: usize) {
        todo!()
    }
}
