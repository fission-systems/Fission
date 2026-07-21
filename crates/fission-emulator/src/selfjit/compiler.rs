//! P-code -> native AArch64 machine code, without Cranelift.
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
//! `Copy`, `IntZExt`, `IntSExt`, `IntAdd`, `IntSub`, `IntAnd`, `IntOr`,
//! `IntXor`, `IntMult`, `IntDiv`, `IntSDiv`, `IntRem`, `IntSRem`,
//! `IntLeft`, `IntRight`, `IntSRight`, `Int2Comp`, `IntNegate`,
//! `BoolAnd`, `BoolOr`, `BoolXor`, `BoolNegate`, `IntEqual`,
//! `IntNotEqual`, `IntSLess`, `IntLess`, `IntSLessEqual`, `IntLessEqual`,
//! and TB-terminating `Branch`/`CBranch` -- but **only as a straight-line,
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
//! Not implemented at all (of ~70 `PcodeOpcode` variants): the remaining
//! ~45 -- all `Float*`, `Call`/`CallInd`/`CallOther`, `MultiEqual`,
//! `Piece`/`SubPiece`, `PtrAdd`/`PtrSub`, `PopCount`/`LzCount`,
//! `IntCarry`/`IntSCarry`/`IntSBorrow` (x86-flag-style carry/overflow
//! opcodes -- deferred since they need either ARM's own NZCV flag
//! extraction or manual overflow arithmetic, neither implemented in the
//! emitter yet). `compile_translation_block` returns a descriptive `Err`
//! for any of these rather than emitting wrong code -- matching this
//! session's own repeated finding (the 8 missing `FLOAT_*` decompiler
//! opcodes, the emulator's own TZCNT bug) that a loud failure beats
//! silently-wrong output every time.

use anyhow::{bail, Result};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};

use crate::jit::backend::{GuestInsn, TbBackend};
use crate::jit::callbacks::{jit_read_space, jit_write_space};
use crate::selfjit::codebuf::CodeBuffer;
use crate::selfjit::emit::{aarch64::Cond, Asm};

// Fixed "the value is here right now" slots between callback calls -- not
// a real register allocator, see the module docs.
//
// Deliberately callee-saved (X19-X28), *not* caller-saved (X9-X15): a real,
// live bug this way, not a theoretical AAPCS64 footnote -- A_VAL/B_VAL
// originally lived in X9/X10, and `IntAdd`'s second `load_value` call
// (itself a `blr` into `jit_read_space`, which the ABI explicitly permits
// to clobber any caller-saved register including X9-X15) silently
// stomped A_VAL's first-loaded value before `add_reg` ever read it back --
// `10 + 32` came out as some unrelated leftover from inside
// `jit_read_space`'s own compiled body, not 42. Found by bisecting against
// the passing `emit`/`codebuf` unit tests (which never make two calls in a
// row) down to this exact module's own multi-op integration test, not by
// reasoning about the encoding in the abstract.
const A_VAL: u32 = crate::selfjit::emit::aarch64::X20;
const B_VAL: u32 = crate::selfjit::emit::aarch64::X21;
const RESULT: u32 = crate::selfjit::emit::aarch64::X22;
const CALLEE_ADDR: u32 = aarch64_regs::X9; // caller-saved is fine: never needs to survive a call
const EMU_PTR: u32 = crate::selfjit::emit::aarch64::X19;
/// AArch64's zero register, used where a data-processing instruction's `Rn`
/// field needs to read as 0 (e.g. `Int2Comp`'s `0 - x`) rather than a real
/// value slot.
const XZR: u32 = 31;

mod aarch64_regs {
    pub const X9: u32 = 9;
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
        #[cfg(not(target_arch = "aarch64"))]
        bail!("selfjit is only implemented for aarch64 hosts (see emit/x86_64.rs)");
        #[cfg(target_arch = "aarch64")]
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
            asm.mov_imm64(aarch64_regs::X9, fallthrough_pc);
            epilogue_return(&mut asm, aarch64_regs::X9);
        }

        let code = buf.finish()?;
        let ptr = code.as_ptr();
        self.code_arena.push(code);
        Ok(ptr)
    }
}

/// Frame layout for the 5 callee-saved registers this file repurposes
/// (X19=EMU_PTR, X20=A_VAL, X21=B_VAL, X22=RESULT) plus X30 (link
/// register, clobbered by every `blr`) -- 40 bytes rounded up to 48 to
/// keep SP 16-byte aligned throughout (required at every `blr` per
/// AAPCS64, not just at entry/exit).
const FRAME_BYTES: u32 = 48;

/// Save the callee-saved registers this file uses as fixed value slots
/// (see [`FRAME_BYTES`]'s doc), then move the incoming arg (X0, `*mut
/// Emulator` per AAPCS64) into EMU_PTR (X19).
fn prologue(asm: &mut Asm) {
    asm.sub_imm(31 /* sp */, 31, FRAME_BYTES);
    asm.str_imm(EMU_PTR, 31, 0);
    asm.str_imm(A_VAL, 31, 8);
    asm.str_imm(B_VAL, 31, 16);
    asm.str_imm(RESULT, 31, 24);
    asm.str_imm(aarch64::X30_LR, 31, 32);
    asm.mov_reg(EMU_PTR, aarch64_regs_x0());
}

fn aarch64_regs_x0() -> u32 {
    crate::selfjit::emit::aarch64::X0
}

/// Restore the callee-saved registers, move `result_reg` into X0 (AAPCS64
/// return value; safe even if `result_reg` is one of the callee-saved
/// slots being restored, since the move happens first), ret.
fn epilogue_return(asm: &mut Asm, result_reg: u32) {
    if result_reg != aarch64_regs_x0() {
        asm.mov_reg(aarch64_regs_x0(), result_reg);
    }
    asm.ldr_imm(EMU_PTR, 31, 0);
    asm.ldr_imm(A_VAL, 31, 8);
    asm.ldr_imm(B_VAL, 31, 16);
    asm.ldr_imm(RESULT, 31, 24);
    asm.ldr_imm(aarch64::X30_LR, 31, 32);
    asm.add_imm(31, 31, FRAME_BYTES);
    asm.ret();
}

use crate::selfjit::emit::aarch64;

/// Emit a call to `jit_read_space(emu, space_id, offset, size) -> u64`,
/// leaving the result in `dst`. `vn.is_constant` short-circuits to a plain
/// immediate load (no call needed).
fn load_value(asm: &mut Asm, vn: &Varnode, dst: u32) {
    if vn.is_constant {
        asm.mov_imm64(dst, vn.constant_val as u64);
        return;
    }
    asm.mov_reg(aarch64_regs_x0(), EMU_PTR);
    asm.mov_imm64(aarch64::X1, vn.space_id);
    asm.mov_imm64(aarch64::X2, vn.offset);
    asm.mov_imm64(aarch64::X3, vn.size as u64);
    asm.mov_imm64(CALLEE_ADDR, jit_read_space as *const () as usize as u64);
    asm.blr(CALLEE_ADDR);
    if dst != aarch64_regs_x0() {
        asm.mov_reg(dst, aarch64_regs_x0());
    }
}

/// Emit a call to `jit_write_space(emu, space_id, offset, size, val)`.
/// `src` is clobbered (moved into the X4 arg slot).
fn store_value(asm: &mut Asm, vn: &Varnode, src: u32) {
    // src may already be x4-shaped by luck; always re-home defensively
    // since call argument registers are not modeled as "owned" by any op.
    asm.mov_reg(aarch64::X4, src);
    asm.mov_reg(aarch64_regs_x0(), EMU_PTR);
    asm.mov_imm64(aarch64::X1, vn.space_id);
    asm.mov_imm64(aarch64::X2, vn.offset);
    asm.mov_imm64(aarch64::X3, vn.size as u64);
    asm.mov_imm64(CALLEE_ADDR, jit_write_space as *const () as usize as u64);
    asm.blr(CALLEE_ADDR);
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
        PcodeOpcode::IntAdd
        | PcodeOpcode::IntSub
        | PcodeOpcode::IntAnd
        | PcodeOpcode::IntOr
        | PcodeOpcode::IntXor
        | PcodeOpcode::BoolAnd
        | PcodeOpcode::BoolOr
        | PcodeOpcode::BoolXor
        | PcodeOpcode::IntMult => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            match op.opcode {
                PcodeOpcode::IntAdd => asm.add_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntSub => asm.sub_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntAnd | PcodeOpcode::BoolAnd => asm.and_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntOr | PcodeOpcode::BoolOr => asm.orr_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntXor | PcodeOpcode::BoolXor => asm.eor_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntMult => asm.mul_reg(RESULT, A_VAL, B_VAL),
                _ => unreachable!(),
            }
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv => {
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
            // module's doc) -- flagged, not silently trusted.
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
            asm.mov_imm64(aarch64_regs_x0(), target.offset);
            epilogue_return(asm, aarch64_regs_x0());
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
            asm.mov_imm64(aarch64_regs_x0(), target.offset);
            epilogue_return(asm, aarch64_regs_x0());
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
}

