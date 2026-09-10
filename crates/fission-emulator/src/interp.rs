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
use fission_pcode::ir::PcodeOp;

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
            let step = {
                let mut evaluator = Evaluator::new(&mut self.state, &mut self.solver);
                evaluator.step(op)?
            };

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
                    let result = crate::jit::callbacks::jit_call_other(
                        self as *mut _,
                        userop_id,
                        input_vals.as_ptr(),
                        input_vals.len() as u64,
                        u64::from(output_size),
                    );
                    if let Some(out) = flat[idx].output.clone() {
                        let mut evaluator = Evaluator::new(&mut self.state, &mut self.solver);
                        evaluator.write_varnode_u64(&out, result)?;
                    }
                    if self.halt_requested {
                        return Ok(InterpExit::Halt);
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
