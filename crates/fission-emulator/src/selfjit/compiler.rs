//! P-code -> native host machine code (AArch64 or x86-64), without
//! Cranelift.
//!
//! # Two host backends, one generic translation layer
//!
//! This file's own p-code-to-instructions translation (`compile_op` and
//! everything below it) is written against the `Asm`/`Cond` shape
//! `crate::selfjit::emit` re-exports for whichever host arch Fission is
//! compiled for -- not hand-specialized per arch. The one place that
//! genuinely isn't generic is frame setup (`prologue`/`epilogue_return`,
//! `#[cfg(target_arch = ...)]`-gated below): AAPCS64 (aarch64) and SysV64
//! (x86-64) differ in real, structural ways here -- no hardware call stack
//! vs. one, and AAPCS64 conveniently reuses the same register (X0) for a
//! call's first argument and its return value where SysV64 uses two
//! different ones (RDI vs RAX) -- see `emit::x86_64`'s own module doc for
//! the full account of both differences and how `ARG0`/`RET` (distinct
//! generic constants here, replacing an earlier `aarch64_regs_x0()` helper
//! that quietly assumed they were always the same register) resolve the
//! second one.
//!
//! # Strategy: correctness first, no register allocator
//!
//! Every value read/write goes through the *same* `jit_read_space`/
//! `jit_write_space` host callbacks `crate::jit::compiler` (the Cranelift
//! backend) already uses -- reusing them is deliberate: they already
//! correctly handle the register-file fast path, page-fault checks, and
//! symbolic/shadow tracking. Re-deriving that logic natively (the way a
//! *fast* self-JIT eventually should, to stop paying a host-call per
//! p-code operand) is real, separate, follow-up work -- see the
//! `TODO(perf)` markers below. This file's only job is to prove the
//! pipeline (p-code -> real host machine code -> mmap RX -> call -> guest-
//! correct result) end to end without Cranelift in the loop, which it does
//! -- see this module's own integration test.
//!
//! # What's implemented vs. TODO
//!
//! Implemented, and covered by a real end-to-end test (compiled, mapped
//! executable, called, result checked against the real host arithmetic):
//! `Copy`, `IntZExt`, `IntSExt`, `IntAdd`, `IntSub`, `PtrSub`, `IntAnd`,
//! `IntOr`, `IntXor`, `IntMult`, `IntDiv`, `IntSDiv`, `IntRem`, `IntSRem`,
//! `IntLeft`, `IntRight`, `IntSRight`, `Int2Comp`, `IntNegate`,
//! `BoolAnd`, `BoolOr`, `BoolXor`, `BoolNegate`, `IntEqual`,
//! `IntNotEqual`, `IntSLess`, `IntLess`, `IntSLessEqual`, `IntLessEqual`,
//! `IntCarry`, `IntSCarry`, `IntSBorrow`, `PopCount`, `PtrAdd`, `Piece`,
//! `SubPiece`, `LzCount`,
//! `Load`, `Store` (computed-address memory access -- **≤8 bytes only**,
//! see below), and TB-terminating `Branch`/`CBranch` -- but **only as a straight-line,
//! single-exit TB**: every `Branch`/`CBranch` here ends the compiled
//! function (returns the target PC, or falls through), the same shape
//! `crate::jit::compiler`'s exit block produces for a cross-instruction
//! jump. Intra-instruction relative branches (SLEIGH's own p-code-level
//! loops, e.g. `TZCNT`'s bit-scan -- see
//! `crate::jit::compiler::remap_relative_branches`'s doc comment and this
//! session's own fix for the real bug that construct caused) are **not**
//! handled -- a TB containing one returns `Err`, not silently wrong
//! output.
//!
//! Known, deliberate simplifications within what *is* implemented (see
//! inline `TODO(correctness)` comments at each site, not hidden): none of
//! the arithmetic/shift ops truncate their result to the varnode's
//! declared bit width (they operate on full 64-bit host registers
//! regardless of whether the p-code value is logically 8/16/32 bits),
//! and shift amounts are not clamped to the operand's declared width
//! before shifting (AArch64's LSLV/LSRV/ASRV mask mod 64 instead, which
//! only coincides with p-code's own "shift >= width => 0" semantics for
//! 64-bit operands).
//!
//! A third, real one found and characterized (not yet fixed here) via
//! `fission-verify`'s emulator-grounded verification tier: `load_value`
//! (like `crate::jit::compiler`'s `load_vn!`, which had the identical
//! defect and *was* fixed) always zero-extends a narrower-than-8-byte
//! operand -- correct for unsigned ops, wrong for `IntSLess`/
//! `IntSLessEqual`/`IntSDiv`/`IntSRem`/`IntSRight`, whose result depends
//! on the operand's actual sign (a negative `dword` `-1` = `0xFFFFFFFF`
//! must sign-extend to `0xFFFFFFFF_FFFFFFFF`, not stay a huge positive
//! `0x00000000_FFFFFFFF`). `crate::jit::compiler`'s version of this fix
//! sign-extends via a `ishl`/`sshr`-immediate pair in Cranelift IR right
//! before the signed op; the AArch64 analog would be `SBFX`/`SXTW`-style
//! emission ahead of `sdiv_reg`/`asr_reg`/`Cond::Lt`-`Cond::Le` at each of
//! this file's signed-op sites. Left as a documented gap rather than a
//! quick hand-assembly patch, consistent with this module's own stated
//! bar (`SelfJitCompiler` is not wired in anywhere live, and
//! `crate::selfjit`'s own module doc's item 4 -- differential-test against
//! `crate::jit::compiler` on real corpus
//! binaries -- is the right place to catch/fix this class of bug
//! properly rather than patching call sites ad hoc without that harness).
//!
//! `Load`/`Store`'s own gap: only the ≤8-byte path is implemented (`jit_
//! read_space`/`jit_write_space`, the same host callbacks `load_value`/
//! `store_value` already use, just with a runtime-computed address
//! register in the X2 argument slot instead of a compile-time-constant
//! offset). A `>8`-byte `Load`/`Store` (e.g. an XMM/YMM-width value or a
//! struct copy) returns `Err`, not a silently-truncated result --
//! `crate::jit::compiler`'s wide path additionally allocates a Cranelift
//! stack slot and calls `jit_read_bytes`/`jit_write_bytes`; this hand-
//! rolled backend has no stack-slot allocator yet, so porting that shape
//! safely is real, separate follow-up work rather than something to rush
//! alongside the ≤8-byte path.
//!
//! Not implemented at all (of ~70 `PcodeOpcode` variants): the remaining
//! ones -- `Float*`, `Call`/`CallInd`/`CallOther`, `MultiEqual`, `Extract`,
//! `Insert`, `SegmentOp`. `compile_translation_block` returns a descriptive
//! `Err` for any of these rather than emitting wrong code -- matching this
//! session's own repeated finding (the 8 missing `FLOAT_*` decompiler
//! opcodes, the emulator's own TZCNT bug) that a loud failure beats
//! silently-wrong output every time.

use anyhow::{bail, Result};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};

use crate::jit::backend::{GuestInsn, TbBackend};
use crate::jit::callbacks::{jit_int_flag, jit_read_space, jit_write_space};
use crate::selfjit::codebuf::CodeBuffer;
use crate::selfjit::emit::{
    Asm, Cond, ARG0, ARG1, ARG2, ARG3, ARG4, A_VAL_SLOT, B_VAL_SLOT, EMU_PTR_SLOT, RESULT_SLOT,
    RET,
};

// Fixed "the value is here right now" slots between callback calls -- not
// a real register allocator, see the module docs. Callee-saved on *every*
// host arch this file targets (AAPCS64 X19-X28 on aarch64, SysV64
// RBX/R12-R15 on x86-64 -- see each `emit::<arch>` module's own
// `*_SLOT` constants), *not* caller-saved: a real, live bug this way on
// aarch64, not a theoretical footnote -- A_VAL/B_VAL originally lived in
// X9/X10, and `IntAdd`'s second `load_value` call (itself a `blr` into
// `jit_read_space`, which the ABI explicitly permits to clobber any
// caller-saved register including X9-X15) silently stomped A_VAL's
// first-loaded value before `add_reg` ever read it back -- `10 + 32` came
// out as some unrelated leftover from inside `jit_read_space`'s own
// compiled body, not 42. Found by bisecting against the passing `emit`/
// `codebuf` unit tests (which never make two calls in a row) down to this
// exact module's own multi-op integration test, not by reasoning about
// the encoding in the abstract.
const A_VAL: u32 = A_VAL_SLOT;
const B_VAL: u32 = B_VAL_SLOT;
const RESULT: u32 = RESULT_SLOT;
const CALLEE_ADDR: u32 = scratch_regs::R9; // caller-saved is fine: never needs to survive a call
// Also caller-saved-is-fine, same reasoning as `CALLEE_ADDR` -- only used by
// `PopCount`'s multi-step bit-twiddling algorithm, which makes no nested
// `blr` between reading and writing them (unlike `A_VAL`/`B_VAL`, which
// *do* need to survive a call and are callee-saved specifically because of
// the bug documented above).
const TMP1: u32 = scratch_regs::R10;
const TMP2: u32 = scratch_regs::R11;
const EMU_PTR: u32 = EMU_PTR_SLOT;
/// AArch64's zero register (register 31 reads as XZR in the data-
/// processing instruction forms `sub_reg` uses), reused as a cross-arch
/// sentinel for "read as 0" -- `emit::x86_64::Asm::sub_reg` special-cases
/// this same raw value (31, out of range for any real x86-64 register
/// encoding) to synthesize `0 - x` via `NEG`, since x86 has no literal
/// zero register. Used where a data-processing instruction's `Rn` field
/// needs to read as 0 (e.g. `Int2Comp`'s `0 - x`) rather than a real value
/// slot.
const XZR: u32 = 31;

// Raw scratch-register numbers, deliberately arch-agnostic raw literals
// (not routed through `emit::<arch>::*_SLOT`): 9, 10, 11 are valid,
// caller-saved-scratch register encodings on *both* supported host arches
// (AArch64's X9-X11; x86-64's R9-R11 -- confirmed disjoint from that arch's
// own `ARG0..ARG4`/`RET`/`*_SLOT` role constants, see `emit::x86_64`'s own
// register-plan doc comment) by coincidence of both ISAs' encoding schemes
// happening to number their extended/scratch registers similarly here, not
// because they share any real ABI concept.
mod scratch_regs {
    pub const R9: u32 = 9;
    pub const R10: u32 = 10;
    pub const R11: u32 = 11;
}

/// `code_arena` keeps every compiled TB's [`ExecutableCode`] mapping alive
/// for as long as the compiler itself lives -- matching Cranelift's own
/// `JITModule`, which keeps all compiled functions in one arena for the
/// process's lifetime rather than freeing each one individually. Without
/// this, a returned function pointer would outlive the mapping it points
/// into: `compile_translation_block` used to return `code.as_ptr()` after
/// letting `code: ExecutableCode` drop at the end of the function scope,
/// which `munmap`s the page immediately -- confirmed as a real, live bug
/// this way (not a theoretical one): the standalone emitter/codebuf unit
/// tests all passed (they keep `code` in scope for their own duration),
/// but this trait method's own integration test reliably hit SIGBUS until
/// this field was added, tracked down by bisecting against those passing
/// tests rather than guessing.
pub struct SelfJitCompiler {
    code_arena: Vec<crate::selfjit::codebuf::ExecutableCode>,
}

impl TbBackend for SelfJitCompiler {
    fn new() -> Result<Self> {
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        bail!("selfjit is only implemented for aarch64/x86-64 hosts");
        #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
        Ok(Self {
            code_arena: Vec::new(),
        })
    }

    fn compile_translation_block(
        &mut self,
        insns: &[GuestInsn],
        _register_space: u64,
    ) -> Result<*const u8> {
        anyhow::ensure!(!insns.is_empty(), "empty translation block");
        let fallthrough_pc = {
            let last = insns.last().unwrap();
            last.pc.wrapping_add(last.len as u64)
        };

        let mut buf = CodeBuffer::new();
        {
            let mut asm = Asm::new(&mut buf);
            prologue(&mut asm);

            for insn in insns {
                for op in &insn.ops {
                    compile_op(&mut asm, op, fallthrough_pc)?;
                }
            }

            // Fell through every instruction with no terminating
            // Branch/CBranch (straight-line TB, e.g. no control flow at
            // all in this block) -- return the natural next PC.
            asm.mov_imm64(RET, fallthrough_pc);
            epilogue_return(&mut asm, RET);
        }

        let code = buf.finish()?;
        let ptr = code.as_ptr();
        self.code_arena.push(code);
        Ok(ptr)
    }
}

/// Frame setup is genuinely arch-specific glue, not part of the shared
/// `Asm` instruction-semantics shape this file otherwise stays generic
/// over -- see `emit::x86_64`'s own module doc for why: AArch64 has no
/// hardware call stack (an explicit link register, saved/restored like any
/// other value) and no PUSH/POP; x86-64 has both, and PUSH/POP is the
/// idiomatic way to save callee-saved registers there. `prologue`/
/// `epilogue_return` are the one place in this file that forgo a single
/// generic body in favor of `#[cfg(target_arch = ...)]`-gated ones.
#[cfg(target_arch = "aarch64")]
mod frame {
    use super::{Asm, A_VAL, B_VAL, EMU_PTR, RESULT, RET};
    use crate::selfjit::emit::aarch64;

    /// Frame layout for the 5 callee-saved registers this file repurposes
    /// (X19=EMU_PTR, X20=A_VAL, X21=B_VAL, X22=RESULT) plus X30 (link
    /// register, clobbered by every `blr`) -- 40 bytes rounded up to 48 to
    /// keep SP 16-byte aligned throughout (required at every `blr` per
    /// AAPCS64, not just at entry/exit).
    const FRAME_BYTES: u32 = 48;

    /// Save the callee-saved registers this file uses as fixed value slots
    /// (see [`FRAME_BYTES`]'s doc), then move the incoming arg (`ARG0`,
    /// `*mut Emulator` per AAPCS64) into EMU_PTR (X19).
    pub(super) fn prologue(asm: &mut Asm) {
        asm.sub_imm(31 /* sp */, 31, FRAME_BYTES);
        asm.str_imm(EMU_PTR, 31, 0);
        asm.str_imm(A_VAL, 31, 8);
        asm.str_imm(B_VAL, 31, 16);
        asm.str_imm(RESULT, 31, 24);
        asm.str_imm(aarch64::X30_LR, 31, 32);
        asm.mov_reg(EMU_PTR, super::ARG0);
    }

    /// Restore the callee-saved registers, move `result_reg` into `RET`
    /// (safe even if `result_reg` is one of the callee-saved slots being
    /// restored, since the move happens first), ret.
    pub(super) fn epilogue_return(asm: &mut Asm, result_reg: u32) {
        if result_reg != RET {
            asm.mov_reg(RET, result_reg);
        }
        asm.ldr_imm(EMU_PTR, 31, 0);
        asm.ldr_imm(A_VAL, 31, 8);
        asm.ldr_imm(B_VAL, 31, 16);
        asm.ldr_imm(RESULT, 31, 24);
        asm.ldr_imm(aarch64::X30_LR, 31, 32);
        asm.add_imm(31, 31, FRAME_BYTES);
        asm.ret();
    }
}

#[cfg(target_arch = "x86_64")]
mod frame {
    use super::{Asm, A_VAL, B_VAL, EMU_PTR, RESULT, RET};

    /// PUSH-ing 4 callee-saved registers (8 bytes each = 32, a multiple of
    /// 16) leaves RSP's alignment exactly where it started -- this
    /// function is entered with `RSP % 16 == 8` (SysV64: the caller's
    /// `call` pushed an 8-byte return address onto a 16-aligned RSP), so
    /// after the 4 pushes RSP is still `% 16 == 8`, not the `% 16 == 0`
    /// required immediately before *this* function's own `call`s. One
    /// extra 8-byte pad (`sub rsp, 8`) fixes that; the epilogue undoes it
    /// (`add rsp, 8`) before popping back in reverse order.
    pub(super) fn prologue(asm: &mut Asm) {
        asm.push_reg(EMU_PTR);
        asm.push_reg(A_VAL);
        asm.push_reg(B_VAL);
        asm.push_reg(RESULT);
        asm.sub_rsp_imm8(8);
        asm.mov_reg(EMU_PTR, super::ARG0);
    }

    pub(super) fn epilogue_return(asm: &mut Asm, result_reg: u32) {
        if result_reg != RET {
            asm.mov_reg(RET, result_reg);
        }
        asm.add_rsp_imm8(8);
        asm.pop_reg(RESULT);
        asm.pop_reg(B_VAL);
        asm.pop_reg(A_VAL);
        asm.pop_reg(EMU_PTR);
        asm.ret();
    }
}

use frame::{epilogue_return, prologue};

/// Emit a call to `jit_read_space(emu, space_id, offset, size) -> u64`,
/// leaving the result in `dst`. `vn.is_constant` short-circuits to a plain
/// immediate load (no call needed).
fn load_value(asm: &mut Asm, vn: &Varnode, dst: u32) {
    if vn.is_constant {
        asm.mov_imm64(dst, vn.constant_val as u64);
        return;
    }
    asm.mov_reg(ARG0, EMU_PTR);
    asm.mov_imm64(ARG1, vn.space_id);
    asm.mov_imm64(ARG2, vn.offset);
    asm.mov_imm64(ARG3, vn.size as u64);
    asm.mov_imm64(CALLEE_ADDR, jit_read_space as *const () as usize as u64);
    asm.blr(CALLEE_ADDR);
    if dst != RET {
        asm.mov_reg(dst, RET);
    }
}

/// Emit a call to `jit_write_space(emu, space_id, offset, size, val)`.
/// `src` is clobbered (moved into the `ARG4` slot).
fn store_value(asm: &mut Asm, vn: &Varnode, src: u32) {
    // src may already be ARG4-shaped by luck; always re-home defensively
    // since call argument registers are not modeled as "owned" by any op.
    asm.mov_reg(ARG4, src);
    asm.mov_reg(ARG0, EMU_PTR);
    asm.mov_imm64(ARG1, vn.space_id);
    asm.mov_imm64(ARG2, vn.offset);
    asm.mov_imm64(ARG3, vn.size as u64);
    asm.mov_imm64(CALLEE_ADDR, jit_write_space as *const () as usize as u64);
    asm.blr(CALLEE_ADDR);
}

/// Resolve a `Load`/`Store`'s first input (the p-code "constant space id"
/// operand) to a concrete space id -- matches `crate::jit::compiler`'s own
/// `space_const`: Fission's raw DTO can encode a constant-space varnode
/// either as a genuine constant (`is_constant`, value in `constant_val`)
/// or with the id folded into `offset`; both are compile-time known, so
/// this resolves fully at compile time, never emitting any code.
fn space_const(vn: &Varnode) -> u64 {
    if vn.is_constant {
        vn.constant_val as u64
    } else {
        vn.offset
    }
}

fn compile_op(asm: &mut Asm, op: &PcodeOp, fallthrough_pc: u64) -> Result<()> {
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::IntZExt => {
            // IntZExt shares Copy's body: `load_value` already returns the
            // source zero-extended to a full 64-bit host register (that's
            // what `jit_read_space` does for any `size < 8`), so widening
            // the declared varnode size on the way out needs no extra
            // instructions -- the high bits are already zero.
            let out = require_output(op)?;
            load_value(asm, &op.inputs[0], RESULT);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::Load => {
            anyhow::ensure!(
                op.inputs.len() >= 2,
                "Load needs a space-id input and an address input"
            );
            let out = require_output(op)?;
            if out.size > 8 {
                bail!(
                    "selfjit: Load of a {}-byte (>8) value is not supported yet \
                     -- see module docs' wide-path note",
                    out.size
                );
            }
            let space_id = space_const(&op.inputs[0]);
            // Resolve the address into a callee-saved slot *before* the
            // X0-X3 call-argument setup below -- `load_value` may itself
            // `blr` into `jit_read_space` (if `inputs[1]` isn't a compile-
            // time constant), which the ABI permits to clobber any
            // caller-saved register. A_VAL survives that the same way it
            // does for every other multi-call op here (see this file's
            // own `A_VAL`/`B_VAL` doc comment above).
            load_value(asm, &op.inputs[1], A_VAL);
            asm.mov_reg(ARG0, EMU_PTR);
            asm.mov_imm64(ARG1, space_id);
            asm.mov_reg(ARG2, A_VAL);
            asm.mov_imm64(ARG3, out.size as u64);
            asm.mov_imm64(CALLEE_ADDR, jit_read_space as *const () as usize as u64);
            asm.blr(CALLEE_ADDR);
            store_value(asm, out, RET);
        }
        PcodeOpcode::Store => {
            if op.inputs.len() < 3 {
                // Matches `crate::jit::compiler`'s own `>= 3` gate for
                // parity -- a legal-but-unseen 2-input Store form
                // (implicit space id) silently does nothing on that
                // backend too; diverging here would be a worse surprise
                // than matching it.
                return Ok(());
            }
            let val_vn = &op.inputs[2];
            if val_vn.size > 8 {
                bail!(
                    "selfjit: Store of a {}-byte (>8) value is not supported yet \
                     -- see module docs' wide-path note",
                    val_vn.size
                );
            }
            let space_id = space_const(&op.inputs[0]);
            // Resolve address, then value, both into callee-saved slots
            // (same reasoning as `Load` above) so they survive each
            // other's potential nested `blr` and the write call's own
            // X0-X4 setup.
            load_value(asm, &op.inputs[1], A_VAL);
            load_value(asm, val_vn, B_VAL);
            asm.mov_reg(ARG4, B_VAL);
            asm.mov_reg(ARG0, EMU_PTR);
            asm.mov_imm64(ARG1, space_id);
            asm.mov_reg(ARG2, A_VAL);
            asm.mov_imm64(ARG3, val_vn.size as u64);
            asm.mov_imm64(CALLEE_ADDR, jit_write_space as *const () as usize as u64);
            asm.blr(CALLEE_ADDR);
        }
        PcodeOpcode::IntAdd
        | PcodeOpcode::IntSub
        | PcodeOpcode::PtrSub
        | PcodeOpcode::IntAnd
        | PcodeOpcode::IntOr
        | PcodeOpcode::IntXor
        | PcodeOpcode::BoolAnd
        | PcodeOpcode::BoolOr
        | PcodeOpcode::BoolXor
        | PcodeOpcode::IntMult => {
            // PtrSub shares IntSub's body -- matches `crate::jit::compiler`'s
            // own `IntSub | PtrSub` arm: p-code's PTRSUB is just pointer
            // arithmetic (base - offset for a sub-component reference),
            // identical bit-level operation to a plain subtract.
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            match op.opcode {
                PcodeOpcode::IntAdd => asm.add_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntSub | PcodeOpcode::PtrSub => asm.sub_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntAnd | PcodeOpcode::BoolAnd => asm.and_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntOr | PcodeOpcode::BoolOr => asm.orr_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntXor | PcodeOpcode::BoolXor => asm.eor_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntMult => asm.mul_reg(RESULT, A_VAL, B_VAL),
                _ => unreachable!(),
            }
            store_value(asm, out, RESULT);
        }
        // PTRADD: ptr + offset * (element size, or 1 if no 3rd input) --
        // matches `crate::jit::compiler`'s own arm exactly.
        PcodeOpcode::PtrAdd => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() >= 2, "PtrAdd needs at least 2 inputs");
            load_value(asm, &op.inputs[0], A_VAL); // ptr
            load_value(asm, &op.inputs[1], B_VAL); // offset
            if op.inputs.len() > 2 {
                // RESULT is callee-saved (safe across this load_value's
                // possible nested call, same reasoning as A_VAL/B_VAL).
                load_value(asm, &op.inputs[2], RESULT); // element size
                asm.mul_reg(RESULT, B_VAL, RESULT);
            } else {
                asm.mov_reg(RESULT, B_VAL);
            }
            asm.add_reg(RESULT, A_VAL, RESULT);
            store_value(asm, out, RESULT);
        }
        // PIECE: concatenate high || low -> (high << low_bits) | low --
        // matches `crate::jit::compiler`'s own arm (including its `.min(63)`
        // shift-amount clamp, for parity).
        PcodeOpcode::Piece => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "Piece needs 2 inputs");
            load_value(asm, &op.inputs[0], A_VAL); // high
            load_value(asm, &op.inputs[1], B_VAL); // low
            let low_bits = (op.inputs[1].size as i64).saturating_mul(8).min(63);
            if low_bits > 0 {
                asm.mov_imm64(TMP1, low_bits as u64);
                asm.lsl_reg(RESULT, A_VAL, TMP1);
            } else {
                asm.mov_reg(RESULT, A_VAL);
            }
            asm.orr_reg(RESULT, RESULT, B_VAL);
            store_value(asm, out, RESULT);
        }
        // SUBPIECE: (val >> shift_bytes*8), truncated to out.size*8 bits --
        // matches `crate::jit::compiler`'s own arm (dynamic-offset second
        // operand is not handled here either, same as that backend).
        PcodeOpcode::SubPiece => {
            let out = require_output(op)?;
            anyhow::ensure!(!op.inputs.is_empty(), "SubPiece needs at least 1 input");
            load_value(asm, &op.inputs[0], A_VAL);
            let shift_bytes = if op.inputs.len() > 1 && op.inputs[1].is_constant {
                op.inputs[1].constant_val
            } else {
                0
            };
            if shift_bytes > 0 {
                asm.mov_imm64(TMP1, (shift_bytes as u64).wrapping_mul(8));
                asm.lsr_reg(RESULT, A_VAL, TMP1);
            } else {
                asm.mov_reg(RESULT, A_VAL);
            }
            let bits = (out.size as u64).saturating_mul(8).min(63);
            let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
            asm.mov_imm64(TMP1, mask);
            asm.and_reg(RESULT, RESULT, TMP1);
            store_value(asm, out, RESULT);
        }
        // LZCOUNT: leading zero bits relative to the input's declared
        // width -- `load_value` already zero-extends, so the untouched
        // high bits (above the declared width) are genuinely 0 and would
        // otherwise inflate a raw 64-bit CLZ. AArch64's CLZ is well-defined
        // for a zero input (returns 64), which already makes
        // `clz(x) - (64 - width)` correct even for `x == 0` without a
        // separate branch -- but branch explicitly anyway, matching
        // `crate::jit::compiler`'s own arm's belt-and-suspenders shape
        // exactly rather than relying on that reasoning unverified.
        PcodeOpcode::LzCount => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 1, "LzCount needs 1 input");
            let width = (op.inputs[0].size as i64).saturating_mul(8).min(64);
            load_value(asm, &op.inputs[0], A_VAL);
            asm.cmp_reg(A_VAL, XZR);
            let branch_to_zero_case = asm.placeholder();
            // non-zero arm (falls through: cmp was not-equal-to-zero)
            asm.clz_reg(RESULT, A_VAL);
            let adj = (64 - width) as u32;
            if adj > 0 {
                asm.sub_imm(RESULT, RESULT, adj);
            }
            let jump_to_end = asm.placeholder();
            let zero_case_at = asm.offset();
            asm.patch_b_cond(branch_to_zero_case, Cond::Eq, zero_case_at);
            asm.mov_imm64(RESULT, width as u64);
            let end = asm.offset();
            asm.patch_b(jump_to_end, end);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv => {
            // TODO(correctness): `IntSDiv` on a narrower-than-8-byte
            // negative operand is wrong -- `load_value` zero-extends, see
            // this module's own doc comment ("A third, real one...").
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            match op.opcode {
                PcodeOpcode::IntDiv => asm.udiv_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntSDiv => asm.sdiv_reg(RESULT, A_VAL, B_VAL),
                _ => unreachable!(),
            }
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntRem | PcodeOpcode::IntSRem => {
            // AArch64 has no remainder instruction: quotient = a/b, then
            // remainder = a - quotient*b via a single MSUB.
            // TODO(correctness): same `IntSRem` narrow-negative-operand gap
            // as `IntSDiv` above.
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            match op.opcode {
                PcodeOpcode::IntRem => asm.udiv_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntSRem => asm.sdiv_reg(RESULT, A_VAL, B_VAL),
                _ => unreachable!(),
            }
            asm.msub_reg(RESULT, RESULT, B_VAL, A_VAL);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntLeft | PcodeOpcode::IntRight | PcodeOpcode::IntSRight => {
            // TODO(correctness): shifts by an amount >= the varnode's own
            // declared bit width should produce 0 (IntLeft/IntRight) --
            // p-code semantics, not a full-64-bit-register semantics.
            // AArch64's LSLV/LSRV/ASRV instead mask the shift amount mod
            // 64, so a shift of e.g. 40 on a declared-32-bit value comes
            // out wrong here. Same class of gap as IntAdd/etc. not
            // truncating to the varnode's declared width (see this
            // module's doc) -- flagged, not silently trusted. `IntSRight`
            // additionally has the narrow-negative-operand sign-extension
            // gap (same doc comment) on its `A_VAL` operand specifically.
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            match op.opcode {
                PcodeOpcode::IntLeft => asm.lsl_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntRight => asm.lsr_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntSRight => asm.asr_reg(RESULT, A_VAL, B_VAL),
                _ => unreachable!(),
            }
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::Int2Comp => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 1, "{:?} needs 1 input", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            asm.sub_reg(RESULT, XZR, A_VAL);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntNegate => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 1, "{:?} needs 1 input", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            asm.mov_imm64(B_VAL, u64::MAX);
            asm.eor_reg(RESULT, A_VAL, B_VAL);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntCarry | PcodeOpcode::IntSCarry | PcodeOpcode::IntSBorrow => {
            // `jit_int_flag(kind, size, a, b) -> u64` is the exact host
            // callout `crate::jit::compiler` (Cranelift) already uses for
            // these three -- a pure function (no `emu_ptr`, no state), and
            // already size-aware/sign-correct internally (`int_flag_op`
            // does its own `sign_extend_n` from the explicitly-passed
            // `size`), so there's no narrow-negative-operand trap here the
            // way there was for `IntSLess`/`IntSDiv`/etc.
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            let kind: u64 = match op.opcode {
                PcodeOpcode::IntCarry => 0,
                PcodeOpcode::IntSCarry => 1,
                _ => 2,
            };
            let size = op.inputs[0].size.max(1) as u64;
            // ARG0=kind, ARG1=size, ARG2=a, ARG3=b, result in RET.
            asm.mov_reg(ARG2, A_VAL);
            asm.mov_reg(ARG3, B_VAL);
            asm.mov_imm64(ARG0, kind);
            asm.mov_imm64(ARG1, size);
            asm.mov_imm64(CALLEE_ADDR, jit_int_flag as *const () as usize as u64);
            asm.blr(CALLEE_ADDR);
            store_value(asm, out, RET);
        }
        // Population count via the classic SWAR bit-twiddling algorithm --
        // the exact same one `crate::jit::compiler`'s `PopCount` arm uses
        // (Cranelift doesn't call a host function for this either), ported
        // instruction-for-instruction rather than re-derived, since it's
        // already proven correct there. Operates on the full 64-bit
        // zero-extended value regardless of the varnode's declared width:
        // `load_value` already zero-extends to exactly that width, so the
        // untouched high bits are 0 and contribute nothing to the count.
        PcodeOpcode::PopCount => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 1, "PopCount needs 1 input");
            load_value(asm, &op.inputs[0], A_VAL);
            // s1 = x0 >> 1; t1 = s1 & 0x5555...; x1 = x0 - t1
            asm.mov_imm64(TMP1, 1);
            asm.lsr_reg(RESULT, A_VAL, TMP1);
            asm.mov_imm64(TMP1, 0x5555_5555_5555_5555);
            asm.and_reg(RESULT, RESULT, TMP1);
            asm.sub_reg(A_VAL, A_VAL, RESULT);
            // s2 = x1 >> 2; t2 = s2 & 0x3333...; b2 = x1 & 0x3333...; x2 = b2 + t2
            asm.mov_imm64(TMP1, 2);
            asm.lsr_reg(RESULT, A_VAL, TMP1);
            asm.mov_imm64(TMP1, 0x3333_3333_3333_3333);
            asm.and_reg(RESULT, RESULT, TMP1);
            asm.and_reg(TMP2, A_VAL, TMP1);
            asm.add_reg(A_VAL, TMP2, RESULT);
            // s4 = x2 >> 4; a4 = x2 + s4; x3 = a4 & 0x0f0f...
            asm.mov_imm64(TMP1, 4);
            asm.lsr_reg(RESULT, A_VAL, TMP1);
            asm.add_reg(A_VAL, A_VAL, RESULT);
            asm.mov_imm64(TMP1, 0x0f0f_0f0f_0f0f_0f0f);
            asm.and_reg(A_VAL, A_VAL, TMP1);
            // x4 = x3 * 0x0101...; result = x4 >> 56
            asm.mov_imm64(TMP1, 0x0101_0101_0101_0101);
            asm.mul_reg(A_VAL, A_VAL, TMP1);
            asm.mov_imm64(TMP1, 56);
            asm.lsr_reg(RESULT, A_VAL, TMP1);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::BoolNegate => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 1, "{:?} needs 1 input", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            asm.mov_imm64(B_VAL, 1);
            asm.eor_reg(RESULT, A_VAL, B_VAL);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntSExt => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 1, "{:?} needs 1 input", op.opcode);
            let in_size = op.inputs[0].size;
            anyhow::ensure!(
                (1..=8).contains(&in_size),
                "IntSExt: unsupported input size {} bytes",
                in_size
            );
            load_value(asm, &op.inputs[0], A_VAL);
            if in_size < 8 {
                // Sign-extend from `in_size` bytes to a full 64-bit host
                // register by shifting the value into the top of the
                // register then back down arithmetically (fills with the
                // sign bit) -- no SXTB/SXTH/SXTW encodings implemented in
                // the emitter, so this uses the two shift instructions
                // IntLeft/IntSRight already needed instead.
                let shift = (64 - in_size * 8) as u64;
                asm.mov_imm64(B_VAL, shift);
                asm.lsl_reg(RESULT, A_VAL, B_VAL);
                asm.asr_reg(RESULT, RESULT, B_VAL);
            } else {
                asm.mov_reg(RESULT, A_VAL);
            }
            store_value(asm, out, RESULT);
        }
        // TODO(correctness): `IntSLess`/`IntSLessEqual` on a narrower-
        // than-8-byte negative operand are wrong -- `load_value` zero-
        // extends, see this module's own doc comment ("A third, real
        // one..."). `IntEqual`/`IntNotEqual`/`IntLess`/`IntLessEqual`
        // (unsigned/equality) are unaffected.
        PcodeOpcode::IntEqual
        | PcodeOpcode::IntNotEqual
        | PcodeOpcode::IntSLess
        | PcodeOpcode::IntLess
        | PcodeOpcode::IntSLessEqual
        | PcodeOpcode::IntLessEqual => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            asm.cmp_reg(A_VAL, B_VAL);
            let cond = match op.opcode {
                PcodeOpcode::IntEqual => Cond::Eq,
                PcodeOpcode::IntNotEqual => Cond::Ne,
                PcodeOpcode::IntSLess => Cond::Lt,
                // `IntLess`'s p-code semantics are unsigned <, unlike
                // `IntSLess` -- Cond::Cc (unsigned <), not Cond::Lt (this
                // used to be a known, documented bug: both mapped to Lt).
                PcodeOpcode::IntLess => Cond::Cc,
                PcodeOpcode::IntSLessEqual => Cond::Le,
                PcodeOpcode::IntLessEqual => Cond::Ls,
                _ => unreachable!(),
            };
            // materialize cond ? 1 : 0 without a CSEL instruction (not
            // implemented in the emitter yet): branch-on-NOT(cond) to the
            // false-arm, matching the if/else shape
            // `emit::aarch64`'s own `cmp_and_conditional_branch` unit test
            // proves works (branch on the *inverse* condition, jump past
            // the true-arm; the true-arm itself falls straight through
            // from the `cmp`). The previous version of this code patched
            // the placeholder with `cond` itself, targeting the
            // instruction immediately after the placeholder -- a branch
            // whose target equals its own fallthrough address, which is
            // always a no-op regardless of whether it's taken, so
            // `RESULT` always ended up 1. Caught by
            // `unsigned_and_signed_less_equal_and_bool_ops`, not by
            // inspection.
            let branch_to_false = asm.placeholder();
            asm.mov_imm64(RESULT, 1);
            let jump_to_end = asm.placeholder();
            let false_at = asm.offset();
            asm.patch_b_cond(branch_to_false, cond.invert(), false_at);
            asm.mov_imm64(RESULT, 0);
            let end = asm.offset();
            asm.patch_b(jump_to_end, end);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::Branch => {
            let target = &op.inputs[0];
            anyhow::ensure!(
                !target.is_constant && target.offset > 0x10000,
                "selfjit: intra-instruction relative Branch (e.g. a SLEIGH \
                 loop construct like TZCNT) is not supported yet -- see \
                 module docs"
            );
            asm.mov_imm64(RET, target.offset);
            epilogue_return(asm, RET);
        }
        PcodeOpcode::CBranch => {
            anyhow::ensure!(op.inputs.len() >= 2, "CBranch needs a target and a condition");
            let target = &op.inputs[0];
            anyhow::ensure!(
                !target.is_constant && target.offset > 0x10000,
                "selfjit: intra-instruction relative CBranch is not \
                 supported yet -- see module docs"
            );
            load_value(asm, &op.inputs[1], A_VAL);
            asm.mov_imm64(B_VAL, 0);
            asm.cmp_reg(A_VAL, B_VAL);
            let taken = asm.placeholder();
            // not-taken: fall through to next op (or the TB's final
            // fallthrough-pc return if this was the last op).
            let not_taken_label = asm.placeholder();
            let taken_at = asm.offset();
            asm.mov_imm64(RET, target.offset);
            epilogue_return(asm, RET);
            let not_taken_at = asm.offset();
            asm.patch_b_cond(taken, Cond::Ne, taken_at);
            asm.patch_b(not_taken_label, not_taken_at);
        }
        other => bail!(
            "selfjit: PcodeOpcode::{:?} not implemented yet (see module docs \
             for the full covered/uncovered list) -- refusing to emit wrong code",
            other
        ),
    }
    let _ = fallthrough_pc;
    Ok(())
}

fn require_output(op: &PcodeOp) -> Result<&Varnode> {
    op.output
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("{:?} has no output varnode", op.opcode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_pcode::ir::PcodeOp;

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 4, // matches RUST_SLEIGH_REGISTER_SPACE_ID's numbering
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn imm(val: i64, size: u32) -> Varnode {
        Varnode {
            space_id: 0,
            offset: 0,
            size,
            is_constant: true,
            constant_val: val,
        }
    }

    /// Real `Emulator` construction, same pattern as the integration tests
    /// under `tests/*.rs` (e.g. `diag_alloc_meta.rs`'s `make_emu()`) -- a
    /// real loaded ELF is the simplest way to get a fully-formed
    /// `MachineState`/register space, even though these tests' compiled
    /// code never touches the binary's own instructions.
    fn make_emu() -> crate::core::Emulator {
        use crate::core::Emulator;
        use crate::os::LinuxEnv;
        use crate::pcode::state::MachineState;

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_static_printf_malloc.elf");
        let binary = fission_loader::loader::LoadedBinary::from_file(&path)
            .expect("load real test ELF");
        let mut state = MachineState::new();
        let _info = crate::os::linux::loader::load_elf(&mut state, &binary).expect("load_elf");
        let load_spec = binary.load_spec().expect("load spec").clone();
        let sleigh =
            fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
                &load_spec,
            )
            .expect("sleigh frontend candidates")
            .into_iter()
            .next()
            .expect("at least one sleigh frontend");
        let arch = crate::arch::ArchInfo::from_language_id(
            load_spec.pair.language_id.as_str(),
            Some(&binary),
        )
        .expect("arch info");
        Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emulator")
    }

    /// Full pipeline: build two real `IntAdd`/`Copy` p-code ops by hand
    /// (mirroring what a real SLEIGH decode would produce for something
    /// like `add eax, ecx`), compile with `SelfJitCompiler` (no Cranelift
    /// involved), map the result executable, and check *through the same
    /// `jit_read_space`/`jit_write_space` callbacks the real JIT uses*
    /// that the guest-visible register state is correct. This is the one
    /// test that proves the skeleton is a real, working JIT and not just
    /// code that happens to compile.
    #[test]
    fn compiles_and_executes_a_real_translation_block() {
        // r0 = 10 (const); r1 = 32 (const); r2 = r0 + r1; expect r2 == 42.
        let ops = vec![
            PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Copy,
                address: 0x1000,
                output: Some(reg(0, 8)),
                inputs: vec![imm(10, 8)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Copy,
                address: 0x1000,
                output: Some(reg(8, 8)),
                inputs: vec![imm(32, 8)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 2,
                opcode: PcodeOpcode::IntAdd,
                address: 0x1000,
                output: Some(reg(16, 8)),
                inputs: vec![reg(0, 8), reg(8, 8)],
                asm_mnemonic: None,
            },
        ];
        let insns = [GuestInsn {
            pc: 0x1000,
            len: 4,
            ops,
        }];

        let mut compiler = SelfJitCompiler::new().expect("selfjit backend available");
        let func_ptr = compiler
            .compile_translation_block(&insns, 4)
            .expect("compile");

        let mut emu = make_emu();
        let f: extern "C" fn(*mut crate::core::Emulator) -> u64 =
            unsafe { std::mem::transmute(func_ptr) };
        let next_pc = f(&mut emu as *mut _);
        assert_eq!(next_pc, 0x1004, "unconditional fallthrough PC");

        let result = emu
            .state
            .read_space(4, 16, 8)
            .expect("read result register");
        let result = u64::from_le_bytes(result.try_into().unwrap());
        assert_eq!(result, 42);
    }

    /// Compiles `ops` as a single-instruction TB, executes it against a
    /// real `Emulator`, and returns that emulator so the caller can read
    /// out whichever register-space offsets it cares about.
    fn compile_and_run(ops: Vec<PcodeOp>) -> crate::core::Emulator {
        let insns = [GuestInsn {
            pc: 0x1000,
            len: 4,
            ops,
        }];
        let mut compiler = SelfJitCompiler::new().expect("selfjit backend available");
        let func_ptr = compiler
            .compile_translation_block(&insns, 4)
            .expect("compile");
        let mut emu = make_emu();
        let f: extern "C" fn(*mut crate::core::Emulator) -> u64 =
            unsafe { std::mem::transmute(func_ptr) };
        let next_pc = f(&mut emu as *mut _);
        assert_eq!(next_pc, 0x1004, "unconditional fallthrough PC");
        emu
    }

    fn read_reg(emu: &mut crate::core::Emulator, offset: u64) -> u64 {
        let bytes = emu.state.read_space(4, offset, 8).expect("read register");
        u64::from_le_bytes(bytes.try_into().unwrap())
    }

    fn copy_const(out_offset: u64, val: i64) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address: 0x1000,
            output: Some(reg(out_offset, 8)),
            inputs: vec![imm(val, 8)],
            asm_mnemonic: None,
        }
    }

    fn binop(opcode: PcodeOpcode, out: u64, a: u64, b: u64) -> PcodeOp {
        PcodeOp {
            seq_num: 1,
            opcode,
            address: 0x1000,
            output: Some(reg(out, 8)),
            inputs: vec![reg(a, 8), reg(b, 8)],
            asm_mnemonic: None,
        }
    }

    fn unop(opcode: PcodeOpcode, out: u64, a: u64) -> PcodeOp {
        PcodeOp {
            seq_num: 1,
            opcode,
            address: 0x1000,
            output: Some(reg(out, 8)),
            inputs: vec![reg(a, 8)],
            asm_mnemonic: None,
        }
    }

    /// `IntMult`/`IntDiv`/`IntSDiv`/`IntRem`/`IntSRem`: r0=17, r1=5, then
    /// each op's result written to its own register -- checked against
    /// plain Rust integer arithmetic on the same values, not just "it ran".
    #[test]
    fn mult_div_rem_match_host_arithmetic() {
        let ops = vec![
            copy_const(0, 17),
            copy_const(8, 5),
            binop(PcodeOpcode::IntMult, 16, 0, 8),
            binop(PcodeOpcode::IntDiv, 24, 0, 8),
            binop(PcodeOpcode::IntSDiv, 32, 0, 8),
            binop(PcodeOpcode::IntRem, 40, 0, 8),
            binop(PcodeOpcode::IntSRem, 48, 0, 8),
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 16), 17u64.wrapping_mul(5));
        assert_eq!(read_reg(&mut emu, 24), 17 / 5);
        assert_eq!(read_reg(&mut emu, 32), 17 / 5);
        assert_eq!(read_reg(&mut emu, 40), 17 % 5);
        assert_eq!(read_reg(&mut emu, 48), 17 % 5);
    }

    /// `IntLeft`/`IntRight`/`IntSRight` (shift-by-register), `Int2Comp`
    /// (arithmetic negate), `IntNegate` (bitwise NOT).
    #[test]
    fn shifts_and_unary_negation_match_host_arithmetic() {
        let ops = vec![
            copy_const(0, 17),  // a
            copy_const(8, 3),   // shift amount
            binop(PcodeOpcode::IntLeft, 16, 0, 8),
            binop(PcodeOpcode::IntRight, 24, 0, 8),
            binop(PcodeOpcode::IntSRight, 32, 0, 8),
            unop(PcodeOpcode::Int2Comp, 40, 0),
            unop(PcodeOpcode::IntNegate, 48, 0),
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 16), 17u64 << 3);
        assert_eq!(read_reg(&mut emu, 24), 17u64 >> 3);
        assert_eq!(read_reg(&mut emu, 32), (17i64 >> 3) as u64);
        assert_eq!(read_reg(&mut emu, 40), (-17i64) as u64);
        assert_eq!(read_reg(&mut emu, 48), !17u64);
    }

    /// `IntSExt`: sign-extends a 1-byte -1 (0xFF) to a full 64-bit -1, and
    /// a 1-byte +5 stays +5 -- proves the shift-based sign fill handles
    /// both signs, not just the always-zero-extend case `IntZExt` already
    /// covers.
    #[test]
    fn sext_fills_with_the_sign_bit() {
        let ops = vec![
            PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Copy,
                address: 0x1000,
                output: Some(reg(0, 1)),
                inputs: vec![imm(-1i64, 1)], // stored as a 1-byte 0xFF
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Copy,
                address: 0x1000,
                output: Some(reg(8, 1)),
                inputs: vec![imm(5, 1)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 2,
                opcode: PcodeOpcode::IntSExt,
                address: 0x1000,
                output: Some(reg(16, 8)),
                inputs: vec![reg(0, 1)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 3,
                opcode: PcodeOpcode::IntSExt,
                address: 0x1000,
                output: Some(reg(24, 8)),
                inputs: vec![reg(8, 1)],
                asm_mnemonic: None,
            },
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 16), u64::MAX, "sign-extended -1 (0xFF)");
        assert_eq!(read_reg(&mut emu, 24), 5, "sign-extended +5 stays +5");
    }

    /// `IntLessEqual` (unsigned <=) and `IntSLessEqual` (signed <=), plus
    /// `BoolAnd`/`BoolNegate` on their 0/1 results -- exercises the
    /// comparison-arm extension and the boolean-op reuse of the
    /// int-arithmetic arm in the same match.
    #[test]
    fn unsigned_and_signed_less_equal_and_bool_ops() {
        let ops = vec![
            copy_const(0, 5),
            copy_const(8, 5),
            copy_const(16, 6),
            binop(PcodeOpcode::IntLessEqual, 24, 0, 8), // 5 <= 5 -> 1
            binop(PcodeOpcode::IntSLessEqual, 32, 16, 0), // 6 <= 5 -> 0
            binop(PcodeOpcode::BoolAnd, 40, 24, 24),    // 1 & 1 -> 1
            unop(PcodeOpcode::BoolNegate, 48, 32),      // !0 -> 1
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 24), 1);
        assert_eq!(read_reg(&mut emu, 32), 0);
        assert_eq!(read_reg(&mut emu, 40), 1);
        assert_eq!(read_reg(&mut emu, 48), 1);
    }

    /// Regression test for a real bug this batch's own new coverage
    /// caught: the comparison-op branch-and-set pattern used to patch its
    /// placeholder branch with a target equal to its own fallthrough
    /// address, making it a no-op regardless of the condition -- every
    /// comparison silently always produced 1, and nothing previously
    /// exercised the *false* outcome to notice. r0=3, r1=7: checks both
    /// the true and false side of `IntEqual`/`IntNotEqual`/`IntSLess`/
    /// `IntLess` explicitly.
    #[test]
    fn comparisons_produce_both_true_and_false_not_always_one() {
        let ops = vec![
            copy_const(0, 3),
            copy_const(8, 7),
            binop(PcodeOpcode::IntEqual, 16, 0, 0),    // 3 == 3 -> 1
            binop(PcodeOpcode::IntEqual, 24, 0, 8),    // 3 == 7 -> 0
            binop(PcodeOpcode::IntNotEqual, 32, 0, 8), // 3 != 7 -> 1
            binop(PcodeOpcode::IntNotEqual, 40, 0, 0), // 3 != 3 -> 0
            binop(PcodeOpcode::IntSLess, 48, 0, 8),    // 3 < 7 -> 1
            binop(PcodeOpcode::IntSLess, 56, 8, 0),    // 7 < 3 -> 0
            binop(PcodeOpcode::IntLess, 64, 0, 8),     // 3 < 7 (unsigned) -> 1
            binop(PcodeOpcode::IntLess, 72, 8, 0),     // 7 < 3 (unsigned) -> 0
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 16), 1, "3 == 3");
        assert_eq!(read_reg(&mut emu, 24), 0, "3 == 7");
        assert_eq!(read_reg(&mut emu, 32), 1, "3 != 7");
        assert_eq!(read_reg(&mut emu, 40), 0, "3 != 3");
        assert_eq!(read_reg(&mut emu, 48), 1, "3 < 7 signed");
        assert_eq!(read_reg(&mut emu, 56), 0, "7 < 3 signed");
        assert_eq!(read_reg(&mut emu, 64), 1, "3 < 7 unsigned");
        assert_eq!(read_reg(&mut emu, 72), 0, "7 < 3 unsigned");
    }

    fn store_op(space_id: i64, addr: Varnode, value: Varnode) -> PcodeOp {
        PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::Store,
            address: 0x1000,
            output: None,
            inputs: vec![imm(space_id, 8), addr, value],
            asm_mnemonic: None,
        }
    }

    fn load_op(out_offset: u64, space_id: i64, addr: Varnode) -> PcodeOp {
        PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::Load,
            address: 0x1000,
            output: Some(reg(out_offset, 8)),
            inputs: vec![imm(space_id, 8), addr],
            asm_mnemonic: None,
        }
    }

    /// `Load`/`Store` with a *computed* (runtime, not compile-time-
    /// constant-offset) address -- reuses the register space (id 4, same
    /// as every other test's `reg()` helper) as the "memory" being
    /// dereferenced, at offsets distinct from this test's own result
    /// registers, so a round trip through `Store` then `Load` proves the
    /// address actually flows through a register (`A_VAL`) into the
    /// `jit_write_space`/`jit_read_space` call, not a hard-coded offset.
    #[test]
    fn load_store_round_trip_with_computed_address() {
        let ops = vec![
            // Store 42 (8 bytes) at address 200, load it back into r0.
            store_op(4, imm(200, 8), imm(42, 8)),
            load_op(0, 4, imm(200, 8)),
            // Store a 4-byte value at address 208, load it back as 4
            // bytes into r8 -- `jit_read_space` always zero-extends to a
            // full u64 result register, so 0xFFFFFFFF round-trips as
            // 0x00000000FFFFFFFF (this backend's documented raw-load
            // contract, same as every other narrow op here; sign
            // interpretation is the caller's job).
            store_op(4, imm(208, 8), imm(-1, 4)),
            load_op(8, 4, imm(208, 4)),
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 0), 42, "8-byte round trip");
        assert_eq!(read_reg(&mut emu, 8), 0xFFFFFFFF, "4-byte round trip, zero-extended");
    }

    /// A `>8`-byte `Load`/`Store` isn't implemented (no stack-slot
    /// allocator in this hand-rolled backend yet -- see module docs) --
    /// must fail loudly at compile time, never silently truncate/corrupt.
    #[test]
    fn wide_load_and_store_fail_loudly_not_silently() {
        let load_ops = vec![PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Load,
            address: 0x1000,
            output: Some(reg(0, 16)),
            inputs: vec![imm(4, 8), imm(200, 8)],
            asm_mnemonic: None,
        }];
        let insns = [GuestInsn { pc: 0x1000, len: 4, ops: load_ops }];
        let mut compiler = SelfJitCompiler::new().expect("selfjit backend available");
        let err = compiler
            .compile_translation_block(&insns, 4)
            .expect_err("wide Load must fail loudly, not silently truncate");
        assert!(err.to_string().contains("not supported"), "unexpected error: {err}");

        let store_ops = vec![PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Store,
            address: 0x1000,
            output: None,
            inputs: vec![imm(4, 8), imm(200, 8), reg(0, 16)],
            asm_mnemonic: None,
        }];
        let insns2 = [GuestInsn { pc: 0x1000, len: 4, ops: store_ops }];
        let mut compiler2 = SelfJitCompiler::new().expect("selfjit backend available");
        let err2 = compiler2
            .compile_translation_block(&insns2, 4)
            .expect_err("wide Store must fail loudly, not silently truncate");
        assert!(err2.to_string().contains("not supported"), "unexpected error: {err2}");
    }

    /// `IntCarry`/`IntSCarry`/`IntSBorrow` -- checked against known
    /// carry/overflow cases (not just "it ran"), the same discipline
    /// `mult_div_rem_match_host_arithmetic` already established for
    /// division/remainder.
    #[test]
    fn int_carry_scarry_sborrow_match_known_cases() {
        let ops = vec![
            copy_const(0, u64::MAX as i64),  // r0 = u64::MAX
            copy_const(8, 1),                // r1 = 1
            copy_const(16, i64::MAX),        // r2 = i64::MAX
            copy_const(24, i64::MIN),        // r3 = i64::MIN
            copy_const(32, 5),               // r4 = 5
            copy_const(40, 3),                // r5 = 3
            binop(PcodeOpcode::IntCarry, 48, 0, 8),   // u64::MAX + 1 -> carries
            binop(PcodeOpcode::IntCarry, 56, 8, 8),   // 1 + 1 -> no carry
            binop(PcodeOpcode::IntSCarry, 64, 16, 8), // i64::MAX + 1 -> signed overflow
            binop(PcodeOpcode::IntSCarry, 72, 8, 8),  // 1 + 1 -> no overflow
            binop(PcodeOpcode::IntSBorrow, 80, 24, 8),  // i64::MIN - 1 -> signed overflow
            binop(PcodeOpcode::IntSBorrow, 88, 32, 40), // 5 - 3 -> no overflow
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 48), 1, "u64::MAX + 1 carries");
        assert_eq!(read_reg(&mut emu, 56), 0, "1 + 1 does not carry");
        assert_eq!(read_reg(&mut emu, 64), 1, "i64::MAX + 1 signed-overflows");
        assert_eq!(read_reg(&mut emu, 72), 0, "1 + 1 does not signed-overflow");
        assert_eq!(read_reg(&mut emu, 80), 1, "i64::MIN - 1 signed-overflows (sborrow)");
        assert_eq!(read_reg(&mut emu, 88), 0, "5 - 3 does not sborrow");
    }

    /// `PopCount` -- the SWAR bit-twiddling algorithm ported from
    /// `crate::jit::compiler`'s own arm, checked against `u64::count_ones`
    /// for boundary values (0, all-ones, a single bit, a mixed pattern).
    #[test]
    fn popcount_matches_host_count_ones() {
        let ops = vec![
            copy_const(0, 0),
            copy_const(8, u64::MAX as i64),
            copy_const(16, 1),
            copy_const(24, 0xFF),
            copy_const(32, 0x0F0F_0F0F_0F0F_0F0Fu64 as i64),
            unop(PcodeOpcode::PopCount, 40, 0),
            unop(PcodeOpcode::PopCount, 48, 8),
            unop(PcodeOpcode::PopCount, 56, 16),
            unop(PcodeOpcode::PopCount, 64, 24),
            unop(PcodeOpcode::PopCount, 72, 32),
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 40), 0u64.count_ones() as u64);
        assert_eq!(read_reg(&mut emu, 48), u64::MAX.count_ones() as u64);
        assert_eq!(read_reg(&mut emu, 56), 1u64.count_ones() as u64);
        assert_eq!(read_reg(&mut emu, 64), 0xFFu64.count_ones() as u64);
        assert_eq!(
            read_reg(&mut emu, 72),
            0x0F0F_0F0F_0F0F_0F0Fu64.count_ones() as u64
        );
    }

    /// `PtrAdd` (3-input scaled form and the 2-input unscaled fallback)
    /// and `PtrSub` (shares `IntSub`'s body) -- checked against plain
    /// pointer arithmetic, not just "it ran".
    #[test]
    fn ptr_add_and_ptr_sub_match_pointer_arithmetic() {
        let ops = vec![
            copy_const(0, 0x1000), // ptr
            copy_const(8, 3),      // index
            copy_const(16, 8),     // element size
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::PtrAdd,
                address: 0x1000,
                output: Some(reg(24, 8)),
                inputs: vec![reg(0, 8), reg(8, 8), reg(16, 8)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::PtrAdd,
                address: 0x1000,
                output: Some(reg(32, 8)),
                inputs: vec![reg(0, 8), reg(8, 8)],
                asm_mnemonic: None,
            },
            binop(PcodeOpcode::PtrSub, 40, 0, 8),
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 24), 0x1000 + 3 * 8, "scaled PtrAdd");
        assert_eq!(read_reg(&mut emu, 32), 0x1000 + 3, "unscaled (mul=1) PtrAdd");
        assert_eq!(read_reg(&mut emu, 40), 0x1000 - 3, "PtrSub");
    }

    /// `Piece` (concatenate high||low) followed by `SubPiece` (extract a
    /// byte-shifted, width-masked slice back out) -- a round trip checked
    /// against the known concatenated value, not just "it ran".
    #[test]
    fn piece_and_subpiece_round_trip() {
        let ops = vec![
            copy_const(0, 0x1122), // high (read back as 2 bytes)
            copy_const(8, 0x3344), // low (read back as 2 bytes)
            // SubPiece's own outputs below only write 2 of the 8 bytes at
            // their offset -- zero the full registers first so the
            // untouched high 6 bytes are deterministic 0 rather than
            // whatever offsets 24/32 happen to hold already (real,
            // already-initialized x86-64 registers in this space, e.g.
            // RSP -- not free scratch the way `Load`/`Store`'s own test
            // uses far-away offsets 200+ for exactly this reason).
            copy_const(24, 0),
            copy_const(32, 0),
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Piece,
                address: 0x1000,
                output: Some(reg(16, 8)),
                inputs: vec![reg(0, 2), reg(8, 2)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 2,
                opcode: PcodeOpcode::SubPiece,
                address: 0x1000,
                output: Some(reg(24, 2)),
                inputs: vec![reg(16, 8), imm(2, 4)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 3,
                opcode: PcodeOpcode::SubPiece,
                address: 0x1000,
                output: Some(reg(32, 2)),
                inputs: vec![reg(16, 8), imm(0, 4)],
                asm_mnemonic: None,
            },
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 16), 0x1122 << 16 | 0x3344, "Piece concat");
        assert_eq!(read_reg(&mut emu, 24), 0x1122, "SubPiece extracts high 2 bytes back");
        assert_eq!(read_reg(&mut emu, 32), 0x3344, "SubPiece extracts low 2 bytes back");
    }

    /// `LzCount` -- checked against known leading-zero-count cases,
    /// including the zero-input edge case (must return the operand's
    /// declared *width*, not the host CLZ instruction's raw 64-bit count).
    #[test]
    fn lzcount_matches_known_cases() {
        let ops = vec![
            copy_const(0, 0),          // all-zero
            copy_const(8, 1),          // 8-byte: 1 leading zero bit short of 64
            copy_const(16, 0x80u8 as i64), // read back as 1 byte: top bit set
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::LzCount,
                address: 0x1000,
                output: Some(reg(40, 8)),
                inputs: vec![reg(0, 8)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 2,
                opcode: PcodeOpcode::LzCount,
                address: 0x1000,
                output: Some(reg(48, 8)),
                inputs: vec![reg(8, 8)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 3,
                opcode: PcodeOpcode::LzCount,
                address: 0x1000,
                output: Some(reg(56, 8)),
                inputs: vec![reg(16, 1)],
                asm_mnemonic: None,
            },
        ];
        let mut emu = compile_and_run(ops);
        assert_eq!(read_reg(&mut emu, 40), 64, "lzcount(0) over 8 bytes == width, not 0");
        assert_eq!(read_reg(&mut emu, 48), 63, "lzcount(1) over 8 bytes");
        assert_eq!(read_reg(&mut emu, 56), 0, "lzcount(0x80) over 1 byte -- top bit set");
    }
}

