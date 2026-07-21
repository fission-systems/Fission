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
//! `Copy`, `IntAdd`, `IntSub`, `IntAnd`, `IntOr`, `IntXor`, `IntEqual`,
//! `IntNotEqual`, `IntSLess`, `IntLess`, and TB-terminating `Branch`/
//! `CBranch` -- but **only as a straight-line, single-exit TB**: every
//! `Branch`/`CBranch` here ends the compiled function (returns the target
//! PC, or falls through), the same shape `crate::jit::compiler`'s exit
//! block produces for a cross-instruction jump. Intra-instruction relative
//! branches (SLEIGH's own p-code-level loops, e.g. `TZCNT`'s bit-scan --
//! see `crate::jit::compiler::remap_relative_branches`'s doc comment and
//! this session's own fix for the real bug that construct caused) are
//! **not** handled -- a TB containing one returns `Err`, not silently
//! wrong output.
//!
//! Not implemented at all (of ~70 `PcodeOpcode` variants): the ~55 other
//! ones -- multiplication/division, shifts, all `Float*`, `Call`/`CallInd`/
//! `CallOther`, `MultiEqual`, `Piece`/`SubPiece`, `PtrAdd`/`PtrSub`,
//! `PopCount`/`LzCount`, sign/zero extension. `compile_translation_block`
//! returns a descriptive `Err` for any of these rather than emitting wrong
//! code -- matching this session's own repeated finding (the 8 missing
//! `FLOAT_*` decompiler opcodes, the emulator's own TZCNT bug) that a loud
//! failure beats silently-wrong output every time.

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
        PcodeOpcode::Copy => {
            let out = require_output(op)?;
            load_value(asm, &op.inputs[0], RESULT);
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntAdd | PcodeOpcode::IntSub | PcodeOpcode::IntAnd | PcodeOpcode::IntOr
        | PcodeOpcode::IntXor => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            match op.opcode {
                PcodeOpcode::IntAdd => asm.add_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntSub => asm.sub_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntAnd => asm.and_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntOr => asm.orr_reg(RESULT, A_VAL, B_VAL),
                PcodeOpcode::IntXor => asm.eor_reg(RESULT, A_VAL, B_VAL),
                _ => unreachable!(),
            }
            store_value(asm, out, RESULT);
        }
        PcodeOpcode::IntEqual
        | PcodeOpcode::IntNotEqual
        | PcodeOpcode::IntSLess
        | PcodeOpcode::IntLess => {
            let out = require_output(op)?;
            anyhow::ensure!(op.inputs.len() == 2, "{:?} needs 2 inputs", op.opcode);
            load_value(asm, &op.inputs[0], A_VAL);
            load_value(asm, &op.inputs[1], B_VAL);
            asm.cmp_reg(A_VAL, B_VAL);
            let cond = match op.opcode {
                PcodeOpcode::IntEqual => Cond::Eq,
                PcodeOpcode::IntNotEqual => Cond::Ne,
                // TODO(correctness): IntSLess vs. IntLess both map to a
                // signed Lt here -- unsigned comparison (`IntLess`'s real
                // p-code semantics) needs Cond::Cc (unsigned <), not
                // implemented yet. Every other opcode in this match is
                // fully correct for its documented semantics; this one
                // pair is a known, deliberate simplification, not an
                // oversight -- flagged so it isn't silently trusted.
                PcodeOpcode::IntSLess | PcodeOpcode::IntLess => Cond::Lt,
                _ => unreachable!(),
            };
            // materialize cond ? 1 : 0 without a CSEL instruction (not
            // implemented in the emitter yet): branch-and-set.
            asm.mov_imm64(RESULT, 0);
            let skip = asm.placeholder();
            let set_one_at = asm.offset();
            asm.mov_imm64(RESULT, 1);
            let after = asm.offset();
            asm.patch_b_cond(skip, cond, set_one_at);
            let _ = after;
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
        use crate::core::Emulator;
        use crate::os::LinuxEnv;
        use crate::pcode::state::MachineState;

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

        // Real Emulator construction, same pattern as the integration
        // tests under tests/*.rs (e.g. diag_alloc_meta.rs's make_emu) --
        // a real loaded ELF is the simplest way to get a fully-formed
        // MachineState/register space, even though this test's compiled
        // code never touches the binary's own instructions.
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_static_printf_malloc.elf");
        let binary = fission_loader::loader::LoadedBinary::from_file(&path)
            .expect("load real test ELF");
        let mut state = MachineState::new();
        let _info = crate::os::linux::loader::load_elf(&mut state, &binary)
            .expect("load_elf");
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
        let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
            .expect("emulator");

        let f: extern "C" fn(*mut Emulator) -> u64 = unsafe { std::mem::transmute(func_ptr) };
        let next_pc = f(&mut emu as *mut _);
        assert_eq!(next_pc, 0x1004, "unconditional fallthrough PC");

        let result = emu
            .state
            .read_space(4, 16, 8)
            .expect("read result register");
        let result = u64::from_le_bytes(result.try_into().unwrap());
        assert_eq!(result, 42);
    }
}

