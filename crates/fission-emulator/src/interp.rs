//! Interpreting a translation block, when compiling it is not an option.
//!
//! # Why there is a second engine at all
//!
//! The JIT was the only one, and `jit::callbacks`' own doc said so: "There is
//! no interpreter fallback path." That is a fine stance for a benchmark
//! harness and a bad one for a product. It means an opcode Cranelift cannot
//! lower, or a host Cranelift does not target, is not slow -- it is a hard
//! error with nothing behind it.
//!
//! The semantics were already written. [`crate::pcode::eval::Evaluator`]
//! implements 73 of 73 p-code opcodes against the same [`MachineState`] the
//! JIT mutates; it had simply never been wired to the run loop. This module is
//! that wiring, not a new engine.
//!
//! # Sharing what has to be shared
//!
//! Both engines go through the same [`crate::jit::callbacks::jit_call_other`]
//! for userops, the same `MachineState`, and the same relative-branch
//! remapping ([`crate::jit::compiler::remap_relative_branches`]). That last one
//! is not an optimisation: a `goto <label>` inside one instruction's semantics
//! is a *signed* op distance, so reading it as an unsigned index would send a
//! backward branch off the end. Sharing the remap is also what makes the two
//! engines comparable -- a differential that let them disagree about what an
//! offset means would only ever be testing the disagreement.

use anyhow::{Result, bail};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};

use crate::core::Emulator;
use crate::jit::compiler::{GuestInsn, remap_relative_branches};
use crate::pcode::eval::{Evaluator, StepResult};

/// How a block stopped.
pub enum InterpExit {
    /// Control left the block for this address.
    Branch(u64),
    /// The block ran off its end.
    FallThrough(u64),
    /// The guest asked to stop (`exit` HLE, halt).
    Halt,
}

impl Emulator {
    /// Run one translation block by interpreting its p-code.
    ///
    /// Advances `inst_count` at each guest-instruction boundary and notifies
    /// observers there too, so a block that falls back is indistinguishable
    /// from a compiled one in the metrics and in anything watching.
    /// One varnode's value, for reporting a memory access. Reporting is
    /// best-effort: a varnode that cannot be read is reported as zero rather
    /// than failing the run, because an observer must never change what the
    /// guest does.
    fn varnode_value(&mut self, vn: &Varnode) -> u64 {
        let mut evaluator = Evaluator::new(&mut self.state, &mut self.solver);
        evaluator.read_varnode_u64(vn).unwrap_or(0)
    }

    pub fn interpret_translation_block(&mut self, insns: &[GuestInsn]) -> Result<InterpExit> {
        if insns.is_empty() {
            bail!("interpreter: empty translation block");
        }
        let entry_pc = insns[0].pc;
        let fallthrough = {
            let last = &insns[insns.len() - 1];
            last.pc.wrapping_add(u64::from(last.len))
        };

        // Same flattening the JIT does, so relative branch targets mean the
        // same thing in both engines.
        let mut flat: Vec<PcodeOp> = Vec::new();
        // Op index → the guest PCs of every instruction starting there.
        //
        // A list per index, not one PC: an instruction can lift to no p-code
        // (x86 `nop dword ptr [rax]`), and then it shares its start index with
        // the next one. Taking just the first, or just the last, loses a real
        // instruction from the count and from anything watching.
        let mut starts_at: Vec<Vec<u64>> = Vec::new();
        for insn in insns {
            let base = flat.len();
            while starts_at.len() <= base {
                starts_at.push(Vec::new());
            }
            starts_at[base].push(insn.pc);
            for (local_i, op) in insn.ops.iter().enumerate() {
                let mut op = op.clone();
                remap_relative_branches(&mut op, base, local_i, insn.ops.len());
                flat.push(op);
            }
        }
        while starts_at.len() <= flat.len() {
            starts_at.push(Vec::new());
        }

        if self.observe.block {
            self.notify_block(entry_pc);
        }

        let mut idx = 0usize;
        // Guard against a p-code-level loop that never leaves the block. The
        // JIT has `jit_count_pcode` for the same hazard; this is its twin.
        let mut ops_run: u64 = 0;
        let op_ceiling = self
            .max_inst
            .map(crate::jit::callbacks::pcode_budget)
            .unwrap_or(u64::MAX);

        while idx < flat.len() {
            for pc in starts_at[idx].clone() {
                self.pc = pc;
                self.inst_count = self.inst_count.saturating_add(1);
                if let Some(limit) = self.max_inst {
                    if self.inst_count >= limit {
                        if self.metrics.exit_reason.is_none() {
                            self.metrics.exit_reason = Some("max_inst".into());
                        }
                        return Ok(InterpExit::Branch(pc));
                    }
                }
                if self.observe.insn {
                    self.notify_insn(pc);
                }
            }

            ops_run = ops_run.saturating_add(1);
            self.pcode_ops = self.pcode_ops.saturating_add(1);
            if ops_run >= op_ceiling {
                if self.metrics.exit_reason.is_none() {
                    self.metrics.exit_reason = Some("pcode_budget".into());
                }
                self.pcode_budget_pc.get_or_insert(entry_pc);
                bail!(
                    "interpreter: p-code budget ({op_ceiling}) exhausted in the block at \
                     0x{entry_pc:X} -- one instruction is looping"
                );
            }

            let op = &flat[idx];

            // A memory access, for anything watching. The JIT reports these
            // from its read/write callbacks; the interpreter has to read the
            // operands itself, because the evaluator holds the state but not
            // the observers.
            //
            // The address is read *before* the op, since a load may write its
            // output over the varnode holding its own address (`RAX = *RAX`);
            // the loaded value has to be read after.
            let pending_mem = if self.observe.mem {
                match op.opcode {
                    PcodeOpcode::Load => Some((
                        self.varnode_value(&op.inputs[1]),
                        op.output.as_ref().map_or(0, |o| o.size),
                        false,
                        0,
                    )),
                    PcodeOpcode::Store => Some((
                        self.varnode_value(&op.inputs[1]),
                        op.inputs[2].size,
                        true,
                        self.varnode_value(&op.inputs[2]),
                    )),
                    _ => None,
                }
            } else {
                None
            };

            let (step, unimplemented) = {
                let mut evaluator = Evaluator::new(&mut self.state, &mut self.solver);
                let step = evaluator.step(op)?;
                (step, evaluator.unimplemented)
            };
            if let Some(opcode) = unimplemented {
                self.metrics.note_unimplemented(opcode);
            }

            if let Some((addr, size, write, stored)) = pending_mem {
                let value = if write {
                    stored
                } else {
                    op.output.as_ref().map_or(0, |o| self.varnode_value(o))
                };
                self.notify_mem(addr, size, write, value);
            }

            match step {
                StepResult::Next => idx += 1,
                StepResult::BranchRel(target) => {
                    if target >= flat.len() {
                        // A relative target past the block's end is a branch
                        // to the next instruction, not a defect: SLEIGH's
                        // `goto inst_next` compiles that way.
                        return Ok(InterpExit::FallThrough(fallthrough));
                    }
                    idx = target;
                }
                StepResult::Branch(addr) => {
                    if self.halt_requested {
                        return Ok(InterpExit::Halt);
                    }
                    return Ok(InterpExit::Branch(addr));
                }
                StepResult::CBranch {
                    condition_val,
                    true_rel_idx,
                    true_addr,
                    ..
                } => {
                    if !condition_val {
                        idx += 1;
                    } else if let Some(rel) = true_rel_idx {
                        if rel >= flat.len() {
                            return Ok(InterpExit::FallThrough(fallthrough));
                        }
                        idx = rel;
                    } else if let Some(addr) = true_addr {
                        return Ok(InterpExit::Branch(addr));
                    } else {
                        bail!(
                            "interpreter: CBRANCH with no destination at 0x{:X}",
                            self.pc
                        );
                    }
                }
                StepResult::CallOther {
                    userop_id,
                    input_vals,
                    output_size,
                } => {
                    // Same function the compiled path calls, for the same
                    // reason: two implementations of one vector instruction
                    // are two chances to be wrong differently.
                    if crate::arch::vector::is_wide_userop(&flat[idx]) {
                        let op = flat[idx].clone();
                        if !crate::arch::vector::answer_vector_userop(self, &op) {
                            let name = self
                                .userop_map
                                .get(&userop_id)
                                .cloned()
                                .unwrap_or_else(|| format!("userop_{userop_id}"));
                            self.metrics.note_unhandled_userop(&name);
                        }
                        idx += 1;
                        continue;
                    }
                    let _control = crate::jit::callbacks::jit_call_other(
                        self as *mut _,
                        userop_id,
                        input_vals.as_ptr(),
                        input_vals.len() as u64,
                        u64::from(output_size),
                    );
                    if let Some(out) = flat[idx].output.clone() {
                        // `jit_call_other` returns the control result (halt,
                        // jump, or continue), not the userop's data result.
                        // The latter is stored in the same slot the JIT reads
                        // after the callback. Using the control result here
                        // made every continuing CallOther with an output look
                        // like zero to the interpreter. AArch64's
                        // ExclusiveMonitorPass returns Continue while its
                        // data result is one, so this changed the predicate
                        // that controls the exclusive-store path.
                        let result = crate::jit::callbacks::jit_callother_result(self as *mut _);
                        let mut evaluator = Evaluator::new(&mut self.state, &mut self.solver);
                        evaluator.write_varnode_u64(&out, result)?;
                    }
                    if self.halt_requested {
                        return Ok(InterpExit::Halt);
                    }
                    if let Some(pc) = self.pc_override.take() {
                        return Ok(InterpExit::Branch(pc));
                    }
                    idx += 1;
                }
            }
        }

        // Trailing instructions that lifted to nothing: reached only by
        // falling out of the block, same as in the compiled path.
        for pc in starts_at[flat.len()].clone() {
            self.pc = pc;
            self.inst_count = self.inst_count.saturating_add(1);
            if self.observe.insn {
                self.notify_insn(pc);
            }
        }

        Ok(InterpExit::FallThrough(fallthrough))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::ArchInfo;
    use crate::core::Emulator;
    use crate::os::{HleResult, LinuxEnv, OsEnvironment};
    use crate::pcode::state::MachineState;
    use fission_loader::loader::LoadedBinary;
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::path::PathBuf;

    fn test_emulator() -> Emulator {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
        let binary = LoadedBinary::from_file(&path).expect("load test ELF");
        let mut state = MachineState::new();
        crate::os::linux::loader::load_elf(&mut state, &binary).expect("load ELF");
        let load_spec = binary.load_spec().expect("load spec").clone();
        let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
            .expect("frontend")
            .into_iter()
            .next()
            .expect("frontend candidate");
        let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
            .expect("architecture");
        Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emulator")
    }

    fn constant(value: i64, size: u32) -> Varnode {
        Varnode {
            space_id: 0,
            offset: value as u64,
            size,
            is_constant: true,
            constant_val: value,
        }
    }

    fn register(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 4,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    struct JumpToEnv;

    impl OsEnvironment for JumpToEnv {
        fn patch_imports(&self, _state: &mut MachineState, _binary: &LoadedBinary) -> Result<()> {
            Ok(())
        }

        fn resolve_stub(&self, _binary: &LoadedBinary, _magic_addr: u64) -> Option<String> {
            None
        }

        fn dispatch_hle(&self, _emu: &mut Emulator, _func_name: &str) -> Result<HleResult> {
            Ok(HleResult::Continue)
        }

        fn dispatch_userop(
            &self,
            _emu: &mut Emulator,
            _userop_name: &str,
            _input_vals: &[u64],
            _output_size: u32,
        ) -> Result<HleResult> {
            Ok(HleResult::JumpTo(0x2000))
        }
    }

    #[test]
    fn interpreter_writes_callother_data_result_not_control_result() {
        let mut emu = test_emulator();
        let userop_id = 0xD00D;
        emu.userop_map.insert(userop_id, "segment_fs".to_string());
        emu.fs_base = 0x7FFF_0000_0000;

        let insns = [GuestInsn {
            pc: 0x1000,
            len: 4,
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::CallOther,
                address: 0x1000,
                output: Some(register(0, 8)),
                inputs: vec![constant(userop_id as i64, 4), constant(0x2A, 8)],
                asm_mnemonic: Some("CALLOTHER".to_string()),
            }],
        }];

        let exit = emu
            .interpret_translation_block(&insns)
            .expect("interpret CallOther");
        assert!(matches!(exit, InterpExit::FallThrough(0x1004)));
        let value = emu
            .state
            .read_space(emu.state.register_space(), 0, 8)
            .expect("read result");
        assert_eq!(
            u64::from_le_bytes(value.try_into().unwrap()),
            emu.fs_base + 0x2A
        );
    }

    #[test]
    fn interpreter_hle_jump_to_exits_translation_block() {
        let mut emu = test_emulator();
        let userop_id = 0xD00E;
        emu.os = Box::new(JumpToEnv);
        emu.userop_map.insert(userop_id, "jump_to_test".to_string());

        let insns = [GuestInsn {
            pc: 0x1000,
            len: 4,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CallOther,
                    address: 0x1000,
                    output: None,
                    inputs: vec![constant(userop_id as i64, 4)],
                    asm_mnemonic: Some("CALLOTHER".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(register(0, 8)),
                    inputs: vec![constant(0xBAD, 8)],
                    asm_mnemonic: Some("COPY after JumpTo".to_string()),
                },
            ],
        }];

        let exit = emu
            .interpret_translation_block(&insns)
            .expect("interpret JumpTo CallOther");
        assert!(matches!(exit, InterpExit::Branch(0x2000)));

        let value = emu
            .state
            .read_space(emu.state.register_space(), 0, 8)
            .expect("read untouched register");
        assert_eq!(u64::from_le_bytes(value.try_into().unwrap()), 0);
    }
}
