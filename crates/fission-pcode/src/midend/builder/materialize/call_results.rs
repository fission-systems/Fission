//! ABI call-result observation and binding priming.
//!
//! This module owns the ABI-facing effect of calls: recognizing call
//! scaffolding and marker operations, proving that a primary return register
//! remains observed, and establishing stable call-result bindings before
//! ordinary materialization begins.

use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn is_call_return_scaffold_store(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
    ) -> bool {
        if op.inputs.len() < 3 || !op.inputs[2].is_constant {
            return false;
        }
        let Some((next_idx, next_call)) =
            block
                .ops
                .iter()
                .enumerate()
                .skip(op_idx + 1)
                .find(|(_, candidate)| {
                    matches!(
                        candidate.opcode,
                        PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                    )
                })
        else {
            return false;
        };
        if next_idx != op_idx + 1 {
            return false;
        }
        let ret_addr = op.inputs[2].constant_val as u64;
        ret_addr > next_call.address && ret_addr.saturating_sub(next_call.address) <= 0x10
    }

    pub(super) fn call_result_registers(&self) -> Vec<Varnode> {
        if !self.options.is_64bit
            && !matches!(
                self.options.calling_convention,
                CallingConvention::X86_32
                    | CallingConvention::Arm32
                    | CallingConvention::PowerPc32
                    | CallingConvention::LoongArch32
                    | CallingConvention::Mips32
            )
        {
            return Vec::new();
        }
        self.register_namer().return_registers()
    }

    pub(super) fn callother_is_same_instruction_call_marker(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        op.opcode == PcodeOpcode::CallOther
            && op.output.is_none()
            && op.inputs.len() == 1
            && block
                .ops
                .iter()
                .skip(op_idx + 1)
                .take_while(|candidate| candidate.address == op.address)
                .any(|candidate| {
                    matches!(candidate.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
                })
    }

    pub(super) fn callother_is_guarded_trap_marker(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if op.opcode != PcodeOpcode::CallOther || op.output.is_some() {
            return false;
        }
        let block_idx = self.lowering_block_index(block);
        let Some(preds) = self.predecessors.get(block_idx) else {
            return false;
        };
        preds.iter().any(|pred_idx| {
            let pred = self.pcode_block(*pred_idx);
            let Some(term_idx) = self.block_terminator_index(pred) else {
                return false;
            };
            let term = &pred.ops[term_idx];
            if term.opcode != PcodeOpcode::CBranch || term.address != op.address {
                return false;
            }
            let Some(target_seq) = term
                .inputs
                .first()
                .and_then(|target| instruction_local_branch_target_seq(term, target))
            else {
                return false;
            };
            block
                .ops
                .iter()
                .enumerate()
                .any(|(target_op_idx, candidate)| {
                    target_op_idx > op_idx && candidate.seq_num == target_seq
                })
        })
    }

    pub(super) fn call_result_is_observed(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let ret_regs = self.call_result_registers();
        if ret_regs.is_empty() {
            return false;
        }
        let mut redefined = false;
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if candidate.inputs.iter().any(|input| {
                ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, input))
            }) {
                return true;
            }
            if let Some(output) = candidate.output.as_ref()
                && ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, output))
            {
                redefined = true;
                break;
            }
        }
        if redefined {
            return false;
        }
        // CALL is frequently a CFG block terminator. The classic save pattern
        // (`call f; mov reg, eax`) then lives in a **successor** block. Without
        // this scan, the call lowers as a bare expression and the successor
        // read reuses the pre-call argument temp in the return register.
        if self.call_result_observed_in_successors(block, &ret_regs) {
            return true;
        }
        // Some ABIs expose the return carrier only through the function's
        // epilogue: the shared successor contains `Return` with a control
        // input, while another predecessor writes an alternate return value.
        // A call feeding that join is still observed when one successor path
        // reaches `Return` without redefining the carrier.  Require
        // function-wide primary-return evidence so a genuinely void call is
        // not turned into a value merely because it happens to precede RET.
        if self.call_result_reaches_return_in_successors(block, &ret_regs) {
            return true;
        }
        // CallInd often has no p-code use of RAX before an epilogue Return
        // (return address on the stack). The ABI still leaves the primary
        // return register live-out — materialize `ret = (*(fp))(…)` so return
        // recovery can read the call-result binding.
        matches!(
            block.ops.get(op_idx).map(|op| op.opcode),
            Some(PcodeOpcode::CallInd)
        )
    }

    /// True when a successor uses an ABI primary-return register (or alias)
    /// as an input before redefining it. Used when CALL is a block terminator
    /// (or the return value is otherwise live-out of the call block).
    fn call_result_observed_in_successors(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        ret_regs: &[Varnode],
    ) -> bool {
        for &succ_idx in &block.successors {
            if self.return_reg_used_before_redefinition_in_block(succ_idx as usize, ret_regs) {
                return true;
            }
        }
        false
    }

    fn call_result_reaches_return_in_successors(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        ret_regs: &[Varnode],
    ) -> bool {
        if !self.function_has_primary_return_def() {
            return false;
        }
        let mut visited = std::collections::BTreeSet::new();
        block.successors.iter().any(|successor| {
            self.call_result_reaches_return(*successor as usize, ret_regs, &mut visited)
        })
    }

    fn call_result_reaches_return(
        &self,
        block_idx: usize,
        ret_regs: &[Varnode],
        visited: &mut std::collections::BTreeSet<usize>,
    ) -> bool {
        if !visited.insert(block_idx) {
            return false;
        }
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return false;
        };
        for candidate in &block.ops {
            if candidate.inputs.iter().any(|input| {
                ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, input))
            }) {
                return true;
            }
            if let Some(output) = candidate.output.as_ref()
                && ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, output))
            {
                return false;
            }
            if candidate.opcode == PcodeOpcode::Return {
                return true;
            }
        }
        block.successors.iter().any(|successor| {
            self.call_result_reaches_return(*successor as usize, ret_regs, visited)
        })
    }

    fn return_reg_used_before_redefinition_in_block(
        &self,
        block_idx: usize,
        ret_regs: &[Varnode],
    ) -> bool {
        let Some(succ) = self.pcode.blocks.get(block_idx) else {
            return false;
        };
        for candidate in &succ.ops {
            if candidate.inputs.iter().any(|input| {
                ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, input))
            }) {
                return true;
            }
            if let Some(output) = candidate.output.as_ref()
                && ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, output))
            {
                return false;
            }
        }
        false
    }

    fn call_result_register_used_by_op(
        &self,
        op: &PcodeOp,
        ret_regs: &[Varnode],
    ) -> Option<Varnode> {
        let matches: Vec<Varnode> = ret_regs
            .iter()
            .filter(|ret_reg| {
                op.inputs
                    .iter()
                    .any(|input| self.varnode_aliases_value(ret_reg, input))
            })
            .cloned()
            .collect();
        if matches.is_empty() {
            return None;
        }
        // A floating operation is the strongest available local evidence that
        // the call consumed the floating ABI carrier rather than the integer
        // one. Copy/Cast and untyped stores intentionally fall back to the
        // stable integer-first order, preserving the existing integer-call
        // behavior when both caller-saved carrier families are present.
        if matches!(
            op.opcode,
            PcodeOpcode::FloatAdd
                | PcodeOpcode::FloatSub
                | PcodeOpcode::FloatMult
                | PcodeOpcode::FloatDiv
                | PcodeOpcode::FloatNeg
                | PcodeOpcode::FloatAbs
                | PcodeOpcode::FloatSqrt
                | PcodeOpcode::FloatCeil
                | PcodeOpcode::FloatFloor
                | PcodeOpcode::FloatRound
                | PcodeOpcode::FloatFloat2Float
                | PcodeOpcode::FloatTrunc
                | PcodeOpcode::FloatEqual
                | PcodeOpcode::FloatNotEqual
                | PcodeOpcode::FloatLess
                | PcodeOpcode::FloatLessEqual
                | PcodeOpcode::FloatNan
        ) {
            return matches
                .into_iter()
                .find(|ret_reg| self.register_namer().is_float_return_register(ret_reg));
        }
        matches
            .into_iter()
            .find(|ret_reg| !self.register_namer().is_float_return_register(ret_reg))
            .or_else(|| {
                ret_regs.iter().find_map(|ret_reg| {
                    op.inputs
                        .iter()
                        .any(|input| self.varnode_aliases_value(ret_reg, input))
                        .then(|| ret_reg.clone())
                })
            })
    }

    fn observed_call_result_register(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        ret_regs: &[Varnode],
    ) -> Option<Varnode> {
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if let Some(ret_reg) = self.call_result_register_used_by_op(candidate, ret_regs) {
                return Some(ret_reg);
            }
            if candidate.output.as_ref().is_some_and(|output| {
                ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, output))
            }) {
                break;
            }
        }
        for &succ_idx in &block.successors {
            let Some(successor) = self.pcode.blocks.get(succ_idx as usize) else {
                continue;
            };
            for candidate in &successor.ops {
                if let Some(ret_reg) = self.call_result_register_used_by_op(candidate, ret_regs) {
                    return Some(ret_reg);
                }
                if candidate.output.as_ref().is_some_and(|output| {
                    ret_regs
                        .iter()
                        .any(|ret_reg| self.varnode_aliases_value(ret_reg, output))
                }) {
                    break;
                }
            }
        }
        None
    }

    fn set_call_result_binding_type(&mut self, name: &str, ty: &NirType) {
        if let Some(binding) = self.temps.get_mut(name) {
            binding.ty = ty.clone();
        }
        for binding in self.params.values_mut() {
            if binding.name == name {
                binding.ty = ty.clone();
            }
        }
        for slot in self.locals.values_mut() {
            if slot.name == name {
                slot.ty = ty.clone();
            }
        }
    }

    pub(super) fn ensure_call_result_binding(
        &mut self,
        site: LoweringSite,
        op: &PcodeOp,
    ) -> String {
        if let Some(name) = self.call_result_bindings.get(&site) {
            return name.clone();
        }
        let ret_regs = self.call_result_registers();
        let Some(ret_reg) = self
            .observed_call_result_register(self.pcode_block(site.block_idx), site.op_idx, &ret_regs)
            .or_else(|| ret_regs.first().cloned())
        else {
            return self
                .ensure_temp_binding_for_output(
                    op,
                    &Varnode {
                        space_id: UNIQUE_SPACE_ID,
                        offset: u64::from(op.seq_num),
                        size: self.options.pointer_size,
                        is_constant: false,
                        constant_val: 0,
                    },
                    false,
                )
                .name;
        };
        let result_ty = if self.register_namer().is_float_return_register(&ret_reg) {
            float_type_from_size(ret_reg.size)
        } else {
            type_from_size(ret_reg.size, false)
        };
        self.call_result_types.insert(site, result_ty.clone());
        // Prefer the ABI return surface (rax / r3 / …) so epilogue recovery and
        // CallInd result share one name. Temps (`xVarN`) break `return` join and
        // force undeclared-symbol noise when the call is a function pointer.
        if let Some(name) = self.sla_hw_name(ret_reg.offset, ret_reg.size) {
            self.ensure_live_register_binding(&name, ret_reg.size);
            self.set_call_result_binding_type(&name, &result_ty);
            self.call_result_bindings.insert(site, name.clone());
            return name;
        }
        let name = self.next_unused_temp_binding_name(&result_ty);
        self.temps.insert(
            name.clone(),
            PreHirBinding {
                name: name.clone(),
                ty: result_ty,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
        );
        self.call_result_bindings.insert(site, name.clone());
        name
    }

    /// Registers every observed call's result binding in one whole-function,
    /// program-order forward pass, before any expression lowering begins.
    ///
    /// Expression lowering resolves a register's value on demand and
    /// reentrantly (`lower_varnode_inner` -> `lookup_def_site` -> possibly
    /// back into lowering an earlier op) -- so which call's result binding
    /// exists in `call_result_bindings` at any given moment depends on
    /// visitation order, not program order. A register defined as `Copy dst
    /// <- <call-return-register>` right after a call reads that call's
    /// result via `live_call_result_binding_for_return_register`, which
    /// only succeeds if `ensure_call_result_binding` already ran for that
    /// specific call; `lookup_def_site`'s reaching-definition search can't
    /// see `Call`/`CallInd` ops as definitions at all (they carry no
    /// `.output` varnode), so a lazy miss doesn't retry -- it silently
    /// falls through to whatever definition dominates the read, which can
    /// be a stale pre-call value. Two reads of the identical call result at
    /// different points in the lowering process could therefore resolve to
    /// two different values depending purely on visitation order.
    ///
    /// Mirrors Ghidra's Heritage pass (`heritage.cc`'s `guardCalls`),
    /// which materializes every call's effect on the return register into
    /// the SSA graph in one synchronous, whole-function sweep before any
    /// read gets resolved -- eliminating this class of bug by construction
    /// rather than by fixing individual read sites. `ensure_call_result_binding`
    /// is idempotent (keyed by `LoweringSite`, short-circuits on a cache
    /// hit), so calling it here ahead of the normal per-block lowering pass
    /// changes nothing about *when* a binding is first requested from the
    /// caller's perspective -- only guarantees it already exists.
    pub(in crate::midend::builder) fn prime_call_result_bindings(&mut self) {
        for block_idx in 0..self.pcode.blocks.len() {
            let op_count = self.pcode.blocks[block_idx].ops.len();
            for op_idx in 0..op_count {
                let block = &self.pcode.blocks[block_idx];
                let op = &block.ops[op_idx];
                if !matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
                    continue;
                }
                if op.output.is_some() {
                    continue;
                }
                if self.call_is_return_target_artifact(block, op_idx)
                    || self.call_is_terminal_branchind_artifact(block, op_idx)
                    || self.callother_is_same_instruction_call_marker(block, op_idx)
                    || self.callother_is_guarded_trap_marker(block, op_idx)
                {
                    continue;
                }
                if !self.call_result_is_observed(block, op_idx) {
                    continue;
                }
                let resolved_block_idx = self.lowering_block_index(block);
                let op_owned = op.clone();
                let site = LoweringSite {
                    block_idx: resolved_block_idx,
                    op_idx,
                };
                self.ensure_call_result_binding(site, &op_owned);
            }
        }
    }
}
