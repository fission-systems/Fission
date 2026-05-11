use super::*;

#[derive(Debug, Clone)]
struct InferredJumpTableTargets {
    unique_targets: Vec<u64>,
    recovered_cases: Vec<(i64, u64)>,
    selector_cardinality: usize,
    decode_mode: &'static str,
}

fn merge_inferred_branchind_targets(
    targets: &mut Vec<u64>,
    recovered_targets: InferredJumpTableTargets,
    recovered_case_map: &mut Option<Vec<(i64, u64)>>,
    recovered_selector_cardinality: &mut Option<usize>,
) {
    *recovered_selector_cardinality = Some(recovered_targets.selector_cardinality);
    *recovered_case_map = Some(recovered_targets.recovered_cases);
    for target in recovered_targets.unique_targets {
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
}

fn arm32_callable_target_expr(expr: &HirExpr) -> HirExpr {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if matches!(&**rhs, HirExpr::Const(0xffff_fffe, _) | HirExpr::Const(-2, _)) => {
            (**lhs).clone()
        }
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if matches!(&**lhs, HirExpr::Const(0xffff_fffe, _) | HirExpr::Const(-2, _)) => {
            (**rhs).clone()
        }
        _ => expr.clone(),
    }
}

fn is_arm32_callable_mask(value: i64) -> bool {
    value == 0xffff_fffe || value == -2
}

impl<'a> PreviewBuilder<'a> {
    fn recover_tail_call_expr_from_target_expr(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        target_expr: &HirExpr,
    ) -> Option<HirExpr> {
        let resolved_target = if let HirExpr::Var(target_name) = target_expr {
            self.resolve_address_like_call_target_name(target_name)
        } else {
            None
        };
        let target = resolved_target.or_else(|| {
            (self.options.calling_convention == CallingConvention::Arm32)
                .then(|| format!("((code *){})", print_expr(&arm32_callable_target_expr(target_expr))))
        })?;
        let args = if self.pcode.blocks.len() <= 2 {
            self.recover_tail_call_args(block_idx, block, term_idx)
        } else {
            Vec::new()
        };
        Some(HirExpr::Call {
            target,
            args,
            ty: NirType::Unknown,
        })
    }

    fn recover_tail_call_expr_from_branchind_target(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        switch_var: &Varnode,
        switch_expr: &HirExpr,
    ) -> Option<HirExpr> {
        let target_expr = self
            .recover_arm32_branchind_callable_target(switch_var)
            .unwrap_or_else(|| switch_expr.clone());
        self.recover_tail_call_expr_from_target_expr(block_idx, block, term_idx, &target_expr)
    }

    fn recover_arm32_branchind_callable_target(&mut self, switch_var: &Varnode) -> Option<HirExpr> {
        if self.options.calling_convention != CallingConvention::Arm32 {
            return None;
        }
        let (_, op) = self.lookup_def_site(switch_var)?;
        if op.opcode == PcodeOpcode::IntAnd && op.inputs.len() == 2 {
            let lhs_mask = const_offset(&op.inputs[0]).is_some_and(is_arm32_callable_mask);
            let rhs_mask = const_offset(&op.inputs[1]).is_some_and(is_arm32_callable_mask);
            let source = match (lhs_mask, rhs_mask) {
                (true, false) => op.inputs[1].clone(),
                (false, true) => op.inputs[0].clone(),
                _ => switch_var.clone(),
            };
            if let Some(expr) = self.recover_arm32_callable_source_expr(&source) {
                return Some(expr);
            }
            return self.lower_wrapped_varnode(&source, &mut HashSet::new()).ok();
        }
        self.recover_arm32_callable_source_expr(switch_var)
    }

    fn recover_arm32_callable_source_expr(&mut self, source: &Varnode) -> Option<HirExpr> {
        if let Some((_, op)) = self.lookup_def_site(source)
            && matches!(
                op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            )
            && let Some(input) = op.inputs.first().cloned()
            && is_register_varnode(&input)
            && let Some(param) = self.register_param(&input)
        {
            return Some(HirExpr::Var(param));
        }
        self.register_param(source).map(HirExpr::Var)
    }

    fn recover_known_external_tail_call_expr(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        target_vn: &Varnode,
    ) -> Option<HirExpr> {
        let target_addr = branch_target_address(target_vn)?;
        if self.address_to_index.contains_key(&target_addr) {
            return None;
        }
        let resolved_target = self
            .type_context
            .and_then(|ctx| ctx.call_target_refs.get(&target_addr))
            .map(|target_ref| target_ref.symbol.clone())?;
        let args = if self.pcode.blocks.len() <= 2 {
            self.recover_tail_call_args(block_idx, block, term_idx)
        } else {
            Vec::new()
        };
        Some(HirExpr::Call {
            target: resolved_target,
            args,
            ty: NirType::Unknown,
        })
    }

    fn recover_tail_call_args(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Vec<HirExpr> {
        if let Ok(Some(args)) = self.recover_tail_call_args_from_block(block, term_idx)
            && !args.is_empty()
        {
            return args;
        }
        let Some(preds) = self.predecessors.get(block_idx) else {
            return Vec::new();
        };
        let [pred_idx] = preds.as_slice() else {
            return Vec::new();
        };
        if !self
            .successors
            .get(*pred_idx)
            .is_some_and(|succs| succs.as_slice() == [block_idx])
        {
            return Vec::new();
        }
        let Some(pred_block) = self.pcode.blocks.get(*pred_idx).cloned() else {
            return Vec::new();
        };
        self.recover_tail_call_args_from_block(&pred_block, pred_block.ops.len())
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn last_primary_return_def_after_barrier(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Option<(usize, Varnode)> {
        let start = block
            .ops
            .iter()
            .enumerate()
            .take(term_idx)
            .rposition(|(idx, op)| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                ) && !self.call_is_return_target_artifact(block, idx)
            })
            .map_or(0, |idx| idx + 1);
        block
            .ops
            .iter()
            .enumerate()
            .take(term_idx)
            .skip(start)
            .rev()
            .find_map(|(op_idx, op)| {
                op.output
                    .as_ref()
                    .filter(|output| {
                        is_primary_return_register_for_abi(output, self.options.calling_convention)
                            && !self.is_return_target_copy(op, output)
                    })
                    .map(|output| (op_idx, output.clone()))
            })
    }

    fn uses_primary_return_registers(&self) -> bool {
        self.options.is_64bit || self.options.calling_convention == CallingConvention::Arm32
    }

    fn is_return_target_copy(&self, op: &PcodeOp, output: &Varnode) -> bool {
        op.opcode == PcodeOpcode::Copy
            && output.space_id == RUST_SLEIGH_REGISTER_SPACE_ID
            && output.offset == 0
            && op.inputs.first().is_some_and(|input| {
                is_return_target_register_for_abi(input, self.options.calling_convention)
            })
    }

    fn lower_primary_return_expr_from_block(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let Some((_ret_op_idx, ret_vn)) =
            self.last_primary_return_def_after_barrier(block, term_idx)
        else {
            return Ok(None);
        };
        self.with_lowering_site(
            LoweringSite {
                block_idx,
                op_idx: term_idx,
            },
            |this| this.lower_wrapped_varnode(&ret_vn, &mut HashSet::new()),
        )
        .map(Some)
    }

    fn block_has_primary_return_def_before_terminator(&self, idx: usize) -> bool {
        let pcode_idx = self.pcode_block_idx(idx);
        let Some(block) = self.pcode.blocks.get(pcode_idx) else {
            return false;
        };
        let term_idx = self
            .block_terminator_index(block)
            .unwrap_or(block.ops.len());
        self.last_primary_return_def_after_barrier(block, term_idx)
            .is_some()
    }

    fn is_pure_return_join_block(&self, idx: usize) -> bool {
        let pcode_idx = self.pcode_block_idx(idx);
        let Some(block) = self.pcode.blocks.get(pcode_idx) else {
            return false;
        };
        let Some(term_idx) = self.block_terminator_index(block) else {
            return false;
        };
        if block.ops[term_idx].opcode != PcodeOpcode::Return {
            return false;
        }
        block.ops.iter().take(term_idx).all(|op| {
            op.output
                .as_ref()
                .is_some_and(|output| self.is_return_target_copy(op, output))
        })
    }

    fn return_join_source_register(&self, return_idx: usize) -> Option<Varnode> {
        let pcode_idx = self.pcode_block_idx(return_idx);
        let block = self.pcode.blocks.get(pcode_idx)?;
        let term_idx = self.block_terminator_index(block)?;
        if block.ops[term_idx].opcode != PcodeOpcode::Return {
            return None;
        }
        let (mut cursor_idx, mut cursor_vn) =
            self.last_primary_return_def_after_barrier(block, term_idx)?;
        for (op_idx, op) in block.ops.iter().enumerate().take(cursor_idx).rev() {
            let Some(output) = op.output.as_ref() else {
                continue;
            };
            if !self.varnode_aliases_value(output, &cursor_vn) {
                continue;
            }
            let Some(input) = op.inputs.first() else {
                return None;
            };
            if op.inputs.len() != 1
                || !matches!(
                    op.opcode,
                    PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
                )
                || !is_register_space_id(input.space_id)
            {
                return None;
            }
            if !is_primary_return_register_for_abi(input, self.options.calling_convention) {
                return Some(input.clone());
            }
            cursor_idx = op_idx;
            cursor_vn = input.clone();
        }
        None
    }

    fn return_join_has_primary_return_evidence(&self, return_idx: usize) -> bool {
        self.predecessors.get(return_idx).is_some_and(|preds| {
            preds.iter().any(|pred| {
                *pred != return_idx && self.block_has_primary_return_def_before_terminator(*pred)
            })
        })
    }

    pub(in crate::nir) fn lower_return_join_expr_for_predecessor(
        &mut self,
        pred_idx: usize,
        return_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if self.options.is_64bit
            && let Some(source_vn) = self.return_join_source_register(return_idx)
        {
            let pred_pcode_idx = self.pcode_block_idx(pred_idx);
            let Some(pred_block) = self.pcode.blocks.get(pred_pcode_idx).cloned() else {
                return Ok(None);
            };
            let pred_term_idx = self
                .block_terminator_index(&pred_block)
                .unwrap_or(pred_block.ops.len());
            return self
                .with_lowering_site(
                    LoweringSite {
                        block_idx: pred_pcode_idx,
                        op_idx: pred_term_idx,
                    },
                    |this| this.lower_wrapped_varnode(&source_vn, &mut HashSet::new()),
                )
                .map(Some);
        }
        if !self.options.is_64bit
            || !self.is_pure_return_join_block(return_idx)
            || !self.return_join_has_primary_return_evidence(return_idx)
        {
            return Ok(None);
        }
        let pred_pcode_idx = self.pcode_block_idx(pred_idx);
        let Some(pred_block) = self.pcode.blocks.get(pred_pcode_idx).cloned() else {
            return Ok(None);
        };
        let pred_term_idx = self
            .block_terminator_index(&pred_block)
            .unwrap_or(pred_block.ops.len());
        if let Some(expr) =
            self.lower_primary_return_expr_from_block(pred_pcode_idx, &pred_block, pred_term_idx)?
        {
            return Ok(Some(expr));
        }
        let Some(ret_vn) =
            primary_return_registers(self.options.pointer_size, self.options.calling_convention)
                .into_iter()
                .next()
        else {
            return Ok(None);
        };
        self.with_lowering_site(
            LoweringSite {
                block_idx: pred_pcode_idx,
                op_idx: pred_term_idx,
            },
            |this| this.lower_wrapped_varnode(&ret_vn, &mut HashSet::new()),
        )
        .map(Some)
    }

    fn last_def_of_varnode_before(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        target: &Varnode,
    ) -> Option<(usize, Varnode)> {
        block
            .ops
            .iter()
            .enumerate()
            .take(term_idx)
            .rev()
            .find_map(|(op_idx, op)| {
                op.output
                    .as_ref()
                    .filter(|output| *output == target)
                    .map(|output| (op_idx, output.clone()))
            })
    }

    fn conditional_return_value_source(
        &self,
        return_block: &crate::pcode::PcodeBasicBlock,
        return_term_idx: usize,
        merge_vn: &Varnode,
    ) -> Option<(usize, Varnode)> {
        if is_primary_return_register_for_abi(merge_vn, self.options.calling_convention) {
            return Some((return_term_idx, merge_vn.clone()));
        }

        let start = return_block
            .ops
            .iter()
            .take(return_term_idx)
            .rposition(|op| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                        | PcodeOpcode::CallOther
                        | PcodeOpcode::Store
                )
            })
            .map_or(0, |idx| idx + 1);

        return_block
            .ops
            .iter()
            .enumerate()
            .take(return_term_idx)
            .skip(start)
            .rev()
            .find_map(|(op_idx, op)| {
                let output = op.output.as_ref()?;
                if !is_primary_return_register_for_abi(output, self.options.calling_convention) {
                    return None;
                }
                op.inputs
                    .iter()
                    .any(|input| input == merge_vn)
                    .then(|| (op_idx, merge_vn.clone()))
            })
    }

    fn predecessor_primary_return_expr(
        &mut self,
        return_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let Some(preds) = self.predecessors.get(return_idx) else {
            return Ok(None);
        };
        let mut recovered: Vec<HirExpr> = Vec::new();
        for pred_idx in preds.clone() {
            if pred_idx == return_idx {
                continue;
            }
            let pred_pcode_idx = self.pcode_block_idx(pred_idx);
            let Some(pred_block) = self.pcode.blocks.get(pred_pcode_idx).cloned() else {
                continue;
            };
            let pred_term_idx = self
                .block_terminator_index(&pred_block)
                .unwrap_or(pred_block.ops.len());
            let Some(expr) = self.lower_primary_return_expr_from_block(
                pred_pcode_idx,
                &pred_block,
                pred_term_idx,
            )?
            else {
                return Ok(None);
            };
            recovered.push(expr);
        }
        let Some(first) = recovered.first().cloned() else {
            return Ok(None);
        };
        let canonical = strip_casts(&first);
        if recovered.iter().all(|expr| strip_casts(expr) == canonical) {
            Ok(Some(first))
        } else {
            Ok(None)
        }
    }

    fn lower_return_terminator(
        &mut self,
        idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if self.uses_primary_return_registers()
            && let Some((_ret_op_idx, ret_vn)) =
                self.last_primary_return_def_after_barrier(block, term_idx)
        {
            return self
                .lower_wrapped_varnode(&ret_vn, &mut HashSet::new())
                .map(Some);
        }
        if self.uses_primary_return_registers()
            && let Some(expr) = self.predecessor_primary_return_expr(idx)?
        {
            return Ok(Some(expr));
        }
        if self.uses_primary_return_registers()
            && let Some(input) = block.ops[term_idx].inputs.last()
            && !self.return_input_is_control_target(input)
        {
            return self
                .lower_wrapped_varnode(input, &mut HashSet::new())
                .map(Some);
        }
        if self.uses_primary_return_registers() && self.unsupported_indirect_control_count == 0 {
            return Ok(None);
        }

        let op = &block.ops[term_idx];
        op.inputs
            .last()
            .map(|input| self.lower_wrapped_varnode(input, &mut HashSet::new()))
            .transpose()
    }

    fn return_input_is_control_target(&self, input: &Varnode) -> bool {
        if self.return_input_is_stack_target(input) {
            return true;
        }
        if is_return_target_register_for_abi(input, self.options.calling_convention) {
            return true;
        }
        self.lookup_def_site(input).is_some_and(|(_, op)| {
            op.output.as_ref().is_some_and(|output| {
                output == input
                    && (self.is_return_target_copy(op, output)
                        || self.op_is_return_target_derivation(op))
            })
        })
    }

    fn op_is_return_target_derivation(&self, op: &PcodeOp) -> bool {
        matches!(
            op.opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::IntAnd
        ) && op.inputs.first().is_some_and(|input| {
            is_return_target_register_for_abi(input, self.options.calling_convention)
        })
    }

    pub(in crate::nir::builder) fn call_is_return_target_artifact(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if !matches!(
            op.opcode,
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
        ) {
            return false;
        }
        let Some(target) = op.inputs.first() else {
            return false;
        };
        let Some(term_idx) = self.block_terminator_index(block) else {
            return false;
        };
        if term_idx <= op_idx || block.ops[term_idx].opcode != PcodeOpcode::Return {
            return false;
        }
        if self.uses_primary_return_registers()
            && op.output.is_none()
            && op.address == block.ops[term_idx].address
        {
            return true;
        }
        if self.return_input_is_control_target(target) {
            return true;
        }
        block.ops[term_idx]
            .inputs
            .last()
            .is_some_and(|input| input == target)
    }

    pub(in crate::nir::builder) fn call_is_terminal_branchind_artifact(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if !matches!(
            op.opcode,
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
        ) || op.output.is_some()
        {
            return false;
        }
        let Some(term_idx) = self.block_terminator_index(block) else {
            return false;
        };
        term_idx > op_idx
            && block.ops[term_idx].opcode == PcodeOpcode::BranchInd
            && block.ops[term_idx].address == op.address
    }

    fn return_input_is_stack_target(&self, input: &Varnode) -> bool {
        let Some((_, op)) = self.lookup_def_site(input) else {
            return false;
        };
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return false;
        }
        self.stack_pointer_register_name(&op.inputs[1])
            .is_some_and(|name| matches!(name, "rsp" | "esp" | "sp"))
    }

    pub(in crate::nir) fn try_lower_intra_instruction_conditional_return(
        &mut self,
    ) -> Result<Option<Vec<HirStmt>>, MlilPreviewError> {
        if !self.options.is_64bit || self.pcode.blocks.len() != 3 {
            return Ok(None);
        }
        let branch_block_idx = 0usize;
        let branch_block = &self.pcode.blocks[branch_block_idx];
        let Some(branch_term_idx) = self.block_terminator_index(branch_block) else {
            return Ok(None);
        };
        let branch_op = &branch_block.ops[branch_term_idx];
        if branch_op.opcode != PcodeOpcode::CBranch || branch_op.inputs.len() < 2 {
            return Ok(None);
        }
        let successors = self
            .successors
            .get(branch_block_idx)
            .cloned()
            .unwrap_or_default();
        if successors.len() != 2 {
            return Ok(None);
        }
        let Some(target_idx) = resolve_branch_target_index(
            self.pcode,
            &self.address_to_index,
            branch_block_idx,
            branch_op,
            &branch_op.inputs[0],
        ) else {
            return Ok(None);
        };
        let Some(copy_idx) = successors.iter().copied().find(|idx| *idx != target_idx) else {
            return Ok(None);
        };
        if target_idx >= self.pcode.blocks.len() || copy_idx >= self.pcode.blocks.len() {
            return Ok(None);
        }
        let copy_block = &self.pcode.blocks[copy_idx];
        let return_block = &self.pcode.blocks[target_idx];
        if self.successors.get(copy_idx).map(Vec::as_slice) != Some(&[target_idx][..]) {
            return Ok(None);
        }
        if copy_block.ops.len() != 1 || copy_block.ops[0].opcode != PcodeOpcode::Copy {
            return Ok(None);
        }
        let Some(copy_output) = copy_block.ops[0].output.as_ref() else {
            return Ok(None);
        };
        let Some(return_term_idx) = self.block_terminator_index(return_block) else {
            return Ok(None);
        };
        if return_block.ops[return_term_idx].opcode != PcodeOpcode::Return {
            return Ok(None);
        }
        if self
            .conditional_return_value_source(return_block, return_term_idx, copy_output)
            .is_none()
        {
            return Ok(None);
        }
        let Some((default_op_idx, default_vn)) =
            self.last_def_of_varnode_before(branch_block, branch_term_idx, copy_output)
        else {
            return Ok(None);
        };

        let cond = self.with_lowering_site(
            LoweringSite {
                block_idx: branch_block_idx,
                op_idx: branch_term_idx,
            },
            |this| this.lower_wrapped_varnode(&branch_op.inputs[1], &mut HashSet::new()),
        )?;
        let default_expr = self.with_lowering_site(
            LoweringSite {
                block_idx: branch_block_idx,
                op_idx: default_op_idx,
            },
            |this| this.lower_def_op(&branch_block.ops[default_op_idx], &mut HashSet::new()),
        )?;
        let alt_expr = self.with_lowering_site(
            LoweringSite {
                block_idx: copy_idx,
                op_idx: 0,
            },
            |this| this.lower_def_op(&copy_block.ops[0], &mut HashSet::new()),
        )?;

        Ok(Some(vec![
            HirStmt::If {
                cond,
                then_body: vec![HirStmt::Return(Some(default_expr))],
                else_body: Vec::new(),
            },
            HirStmt::Return(Some(alt_expr)),
        ]))
    }

    pub(in crate::nir) fn try_lower_conditional_tailcall_after_return(
        &mut self,
    ) -> Result<Option<Vec<HirStmt>>, MlilPreviewError> {
        if !self.options.is_64bit || self.pcode.blocks.len() > 4 {
            return Ok(None);
        }
        let LoweredTerminator::Cond {
            cond,
            true_target,
            false_target: Some(false_target),
        } = self.lower_block_terminator(0)?
        else {
            return Ok(None);
        };

        let true_idx = self.address_to_index.get(&true_target).copied();
        let false_idx = self.address_to_index.get(&false_target).copied();
        let (return_on_true, return_idx, tail_idx) = match (true_idx, false_idx) {
            (Some(true_idx), Some(false_idx))
                if matches!(
                    self.lower_block_terminator(true_idx)?,
                    LoweredTerminator::Return(None)
                ) =>
            {
                (true, true_idx, false_idx)
            }
            (Some(true_idx), Some(false_idx))
                if matches!(
                    self.lower_block_terminator(false_idx)?,
                    LoweredTerminator::Return(None)
                ) =>
            {
                (false, false_idx, true_idx)
            }
            _ => return Ok(None),
        };
        if !self
            .lower_block_stmts(&self.pcode.blocks[return_idx].clone())?
            .is_empty()
        {
            return Ok(None);
        }

        let tail_block = self.pcode.blocks[tail_idx].clone();
        let mut tail_body = self.lower_block_stmts(&tail_block)?;
        if tail_body.len() > 3 {
            return Ok(None);
        }
        match self.lower_block_terminator(tail_idx)? {
            LoweredTerminator::Unsupported {
                evidence,
                target_expr,
            } if matches!(
                evidence.failure_family,
                UnsupportedControlFamily::ExternalTarget
            ) =>
            {
                tail_body.push(self.emit_unsupported_control_surface(evidence, target_expr));
            }
            LoweredTerminator::Goto(target) if self.address_to_index.get(&target).is_none() => {
                tail_body.push(HirStmt::Goto(block_label(target)));
            }
            LoweredTerminator::Fallthrough(None) | LoweredTerminator::Return(None) => {}
            _ => return Ok(None),
        }
        tail_body.push(HirStmt::Return(None));

        let return_cond = if return_on_true {
            cond
        } else {
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(cond),
                ty: NirType::Bool,
            }
        };
        Ok(Some(vec![
            HirStmt::If {
                cond: return_cond,
                then_body: vec![HirStmt::Return(None)],
                else_body: Vec::new(),
            },
            HirStmt::Block(tail_body),
        ]))
    }

    pub(in crate::nir) fn lower_block_terminator(
        &mut self,
        idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError> {
        if let Some(cached) = self.terminator_cache.get(&idx) {
            return Ok(cached.clone());
        }

        let pcode_idx = self.pcode_block_idx(idx);
        let block = &self.pcode.blocks[pcode_idx];
        let lowered = if let Some(term_idx) = self.block_terminator_index(block) {
            let op = &block.ops[term_idx];
            self.with_lowering_site(
                LoweringSite {
                    block_idx: pcode_idx,
                    op_idx: term_idx,
                },
                |this| {
                    let mut visiting = HashSet::new();
                    match op.opcode {
                        PcodeOpcode::Return => Ok(LoweredTerminator::Return(
                            this.lower_return_terminator(idx, block, term_idx)?,
                        )),
                        PcodeOpcode::Branch if op.inputs.len() == 1 => {
                            if let Some(target_vn) = op.inputs.first()
                                && let Some(tail_call_expr) = this
                                    .recover_known_external_tail_call_expr(
                                        idx, block, term_idx, target_vn,
                                    )
                            {
                                let evidence = UnsupportedControlEvidence {
                                    opcode: format!("{:?}", op.opcode),
                                    source_block: Some(block.start_address),
                                    target_expr: Some(print_expr(&tail_call_expr)),
                                    successor_targets: Vec::new(),
                                    failure_family: UnsupportedControlFamily::ExternalTarget,
                                    surface: IndirectControlSurface::BranchInd,
                                    confidence: 72,
                                };
                                return Ok(LoweredTerminator::Unsupported {
                                    evidence,
                                    target_expr: Some(tail_call_expr),
                                });
                            }
                            let target_idx = op.inputs.first().and_then(|input| {
                                this.resolve_branch_target_index_with_recovery(idx, op, input)
                            });
                            if let Some(target_idx) = target_idx {
                                return Ok(LoweredTerminator::Goto(
                                    this.block_target_key(target_idx),
                                ));
                            }
                            if let Some(target_vn) = op.inputs.first() {
                                let target_expr = this
                                    .lower_wrapped_varnode(target_vn, &mut HashSet::new())
                                    .ok();
                                let succ_addrs = block
                                    .successors
                                    .iter()
                                    .filter_map(|succ_idx| {
                                        this.pcode
                                            .blocks
                                            .get(*succ_idx as usize)
                                            .map(|succ| succ.start_address)
                                    })
                                    .collect::<Vec<_>>();
                                this.debug_branch_target_resolution_failure(
                                    "terminator_branch_target_resolve_fail",
                                    idx,
                                    block.start_address,
                                    op,
                                    target_vn,
                                    &succ_addrs,
                                );

                                if let Some(fallback_target) =
                                    this.infer_unconditional_branch_successor_target(idx)
                                {
                                    return Ok(LoweredTerminator::Goto(fallback_target));
                                }

                                // If the branch target points outside the current p-code slice,
                                // degrade to explicit unsupported marker instead of aborting render.
                                if branch_target_address(target_vn).is_some() {
                                    let tail_call_expr = target_expr.as_ref().and_then(|expr| {
                                        this.recover_tail_call_expr_from_target_expr(
                                            idx, block, term_idx, expr,
                                        )
                                    });
                                    let evidence = if tail_call_expr.is_some() {
                                        UnsupportedControlEvidence {
                                            opcode: format!("{:?}", op.opcode),
                                            source_block: Some(block.start_address),
                                            target_expr: tail_call_expr
                                                .as_ref()
                                                .or(target_expr.as_ref())
                                                .map(print_expr),
                                            successor_targets: succ_addrs,
                                            failure_family:
                                                UnsupportedControlFamily::ExternalTarget,
                                            surface: IndirectControlSurface::BranchInd,
                                            confidence: 48,
                                        }
                                    } else {
                                        this.build_unsupported_control_evidence(
                                            op.opcode,
                                            Some(block.start_address),
                                            target_expr.as_ref(),
                                            succ_addrs,
                                            UnsupportedControlFamily::ExternalTarget,
                                            IndirectControlSurface::BranchInd,
                                            48,
                                        )
                                    };
                                    return Ok(LoweredTerminator::Unsupported {
                                        evidence,
                                        target_expr: tail_call_expr.or(target_expr),
                                    });
                                }
                            }
                            Err(MlilPreviewError::UnsupportedCfgBranchTarget)
                        }
                        PcodeOpcode::CBranch | PcodeOpcode::Branch if op.inputs.len() >= 2 => {
                            let true_target = if let Some(true_target_idx) = this
                                .resolve_branch_target_index_with_recovery(idx, op, &op.inputs[0])
                            {
                                this.block_target_key(true_target_idx)
                            } else {
                                if let Some(target_vn) = op.inputs.first() {
                                    let target_expr = this
                                        .lower_wrapped_varnode(target_vn, &mut HashSet::new())
                                        .ok();
                                    let succ_addrs = block
                                        .successors
                                        .iter()
                                        .filter_map(|succ_idx| {
                                            this.pcode
                                                .blocks
                                                .get(*succ_idx as usize)
                                                .map(|succ| succ.start_address)
                                        })
                                        .collect::<Vec<_>>();
                                    this.debug_branch_target_resolution_failure(
                                        "terminator_cbranch_target_resolve_fail",
                                        idx,
                                        block.start_address,
                                        op,
                                        target_vn,
                                        &succ_addrs,
                                    );

                                    if let Some(fallback_target) =
                                        this.infer_cbranch_true_target_from_successors(idx)
                                    {
                                        // Keep conditional structure if CFG successors provide a unique
                                        // non-fallthrough edge even when direct target resolution fails.
                                        fallback_target
                                    } else if branch_target_address(target_vn).is_some() {
                                        // Same policy as Branch: keep rendering by degrading to explicit
                                        // unsupported marker when target resolution is external/unknown.
                                        let evidence = this.build_unsupported_control_evidence(
                                            op.opcode,
                                            Some(block.start_address),
                                            target_expr.as_ref(),
                                            succ_addrs,
                                            UnsupportedControlFamily::ExternalTarget,
                                            IndirectControlSurface::BranchInd,
                                            40,
                                        );
                                        return Ok(LoweredTerminator::Unsupported {
                                            evidence,
                                            target_expr,
                                        });
                                    } else {
                                        return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                                    }
                                } else {
                                    return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                                }
                            };
                            let recovered_cond = this
                                .try_recover_branch_condition(&op.inputs[1])?
                                .filter(|expr| !Self::branch_cond_too_complex(expr));
                            let cond = recovered_cond
                                .map(Ok)
                                .unwrap_or_else(|| {
                                    this.lower_wrapped_varnode(&op.inputs[1], &mut HashSet::new())
                                })
                                .map_err(|err| {
                                    this.debug_lowering_error(
                                        "terminator_cond",
                                        block.start_address,
                                        u64::from(op.seq_num),
                                        op.opcode,
                                        &err,
                                    );
                                    err
                                })?;
                            Ok(LoweredTerminator::Cond {
                                cond,
                                true_target,
                                false_target: this.next_block_address(idx),
                            })
                        }
                        PcodeOpcode::BranchInd => {
                            let switch_var = &op.inputs[0];
                            let switch_expr =
                                this.lower_branchind_switch_expr(idx, switch_var, &mut visiting)?;
                            if preview_builder_diag_enabled() {
                                eprintln!(
                                    "[DIAG] branchind_switch_expr block=0x{:x} seq=0x{:x} expr={}",
                                    block.start_address,
                                    op.seq_num,
                                    print_expr(&switch_expr)
                                );
                            }
                            let mut targets = Vec::new();
                            let had_successor_targets = !block.successors.is_empty();
                            for succ_idx in &block.successors {
                                let succ_idx = *succ_idx as usize;
                                if succ_idx < this.pcode.blocks.len() {
                                    targets.push(this.block_target_key(succ_idx));
                                }
                            }
                            let selector_alias =
                                this.recover_branchind_jump_table_selector_varnode(idx);
                            let mut inferred_single_input_target = false;
                            let mut recovered_case_map = None;
                            let mut recovered_selector_cardinality = None;
                            if let Some(recovered_targets) = this
                                .infer_branchind_targets_from_jump_table_expr(
                                    idx,
                                    &switch_expr,
                                    selector_alias.as_ref(),
                                )
                            {
                                merge_inferred_branchind_targets(
                                    &mut targets,
                                    recovered_targets,
                                    &mut recovered_case_map,
                                    &mut recovered_selector_cardinality,
                                );
                            }
                            if targets.is_empty()
                                && let Some(inferred_target) =
                                    this.infer_branchind_target_from_input(idx, op, switch_var)
                            {
                                inferred_single_input_target = true;
                                targets.push(inferred_target);
                            }
                            if targets.is_empty() {
                                let tail_call_expr = this.recover_tail_call_expr_from_branchind_target(
                                    idx,
                                    block,
                                    term_idx,
                                    switch_var,
                                    &switch_expr,
                                );
                                this.record_unsupported_inventory_event(
                                    "terminator_branchind_no_targets",
                                    Some(switch_var),
                                    Some(op),
                                    Some(op.opcode),
                                    Some(block.start_address),
                                    Some(u64::from(op.seq_num)),
                                    true,
                                    "branchind_targets_missing",
                                );
                                let evidence = this.build_unsupported_control_evidence(
                                    op.opcode,
                                    Some(block.start_address),
                                    tail_call_expr.as_ref().or(Some(&switch_expr)),
                                    Vec::new(),
                                    UnsupportedControlFamily::MissingTargets,
                                    IndirectControlSurface::BranchInd,
                                    32,
                                );
                                Ok(LoweredTerminator::Unsupported {
                                    evidence,
                                    target_expr: tail_call_expr.or(Some(switch_expr)),
                                })
                            } else {
                                if inferred_single_input_target
                                    && super::switch_table::has_jump_table_surface(
                                        &switch_expr,
                                        &this.options,
                                    )
                                {
                                    let rendered_target_expr = selector_alias
                                        .as_ref()
                                        .map(|alias| {
                                            this.recover_branchind_render_selector_expr(
                                                idx,
                                                alias,
                                                switch_expr.clone(),
                                                &mut visiting,
                                            )
                                        })
                                        .or_else(|| {
                                            this.recover_branchind_switch_expr_from_predecessors(
                                                idx,
                                                switch_var,
                                                &mut visiting,
                                            )
                                        })
                                        .unwrap_or_else(|| switch_expr.clone());
                                    this.indirect_target_set_refined_count += 1;
                                    this.dispatcher_shape_recovered_count += 1;
                                    let evidence = this.build_unsupported_control_evidence(
                                        op.opcode,
                                        Some(block.start_address),
                                        Some(&rendered_target_expr),
                                        targets,
                                        UnsupportedControlFamily::NonStructuralDispatcher,
                                        IndirectControlSurface::DispatcherLike,
                                        52,
                                    );
                                    return Ok(LoweredTerminator::Unsupported {
                                        evidence,
                                        target_expr: Some(rendered_target_expr),
                                    });
                                }
                                let default_target =
                                    this.infer_switch_default_target(idx, &targets);
                                // Attempt to recover a proof-bearing selector before we synthesize
                                // a switch. Single-target self-loop dispatcher shapes stay as
                                // explicit indirect surfaces instead of becoming degenerate switches.
                                let recovered_selector =
                                    super::switch_table::recover_switch_discriminant(
                                        &switch_expr,
                                        &this.options,
                                    );
                                let single_target_dispatcher =
                                    super::switch_table::proves_single_target_dispatcher_surface(
                                        &switch_expr,
                                        &targets,
                                        this.block_target_key(idx),
                                        &this.options,
                                    );
                                let dispatcher_recovered =
                                    recovered_selector.is_some() || single_target_dispatcher;
                                let (expr, min_val) = recovered_selector
                                    .as_ref()
                                    .map(|selector| {
                                        let render_expr = selector_alias
                                            .as_ref()
                                            .map(|alias| {
                                                this.recover_branchind_render_selector_expr(
                                                    idx,
                                                    alias,
                                                    selector.discriminant.clone(),
                                                    &mut visiting,
                                                )
                                            })
                                            .unwrap_or_else(|| selector.discriminant.clone());
                                        this.normalize_rendered_selector_expr(
                                            render_expr,
                                            selector.min_val,
                                        )
                                    })
                                    .unwrap_or_else(|| (switch_expr.clone(), 0));
                                let normalization = recovered_selector.as_ref().map(|selector| {
                                    this.selector_normalization_for_branchind(
                                        &expr,
                                        selector.min_val,
                                        selector.entry_size,
                                        recovered_case_map.as_deref(),
                                    )
                                });
                                let side_effect_free_selector =
                                    Self::selector_expr_is_side_effect_free(&expr);
                                let recovered_cases = recovered_case_map.unwrap_or_else(|| {
                                    targets
                                        .iter()
                                        .copied()
                                        .enumerate()
                                        .filter_map(|(ordinal, target)| {
                                            (Some(target) != default_target)
                                                .then_some((min_val + ordinal as i64, target))
                                        })
                                        .collect::<Vec<_>>()
                                });
                                let selector_cardinality =
                                    recovered_selector_cardinality.unwrap_or(recovered_cases.len());
                                let target_cardinality = recovered_cases
                                    .iter()
                                    .map(|(_, target)| *target)
                                    .collect::<std::collections::BTreeSet<_>>()
                                    .len();
                                let ordinal_domain_complete = selector_cardinality >= 2
                                    && !recovered_cases.is_empty()
                                    && recovered_cases.len() >= selector_cardinality;
                                let shared_tail_conflict = false;
                                let case_map_source = match (
                                    had_successor_targets,
                                    recovered_selector_cardinality.is_some(),
                                ) {
                                    (true, true) => DispatcherCaseMapSource::Merged,
                                    (false, true) => DispatcherCaseMapSource::JumpTableRecovered,
                                    (true, false) => DispatcherCaseMapSource::SuccessorOnly,
                                    (false, false) => DispatcherCaseMapSource::SuccessorOnly,
                                };
                                let mut guard_set = vec!["successor_bounded".to_string()];
                                if recovered_selector.is_some() {
                                    guard_set.push("selector_normalized".to_string());
                                }
                                if default_target.is_some() {
                                    guard_set.push("follow_candidate".to_string());
                                }
                                if ordinal_domain_complete {
                                    guard_set.push("ordinal_domain_complete".to_string());
                                }
                                let follow_or_bounded =
                                    default_target.is_some() || ordinal_domain_complete;
                                let proof_complete = follow_or_bounded
                                    && ordinal_domain_complete
                                    && side_effect_free_selector
                                    && !single_target_dispatcher
                                    && !shared_tail_conflict;
                                let failure_family = if proof_complete {
                                    None
                                } else if !side_effect_free_selector {
                                    Some(ProofFailureFamily::NonSideEffectFreeSelector)
                                } else if !ordinal_domain_complete {
                                    Some(ProofFailureFamily::MissingOrdinalCoverage)
                                } else if !follow_or_bounded {
                                    Some(ProofFailureFamily::MissingFollow)
                                } else if shared_tail_conflict {
                                    Some(ProofFailureFamily::SharedTailConflict)
                                } else {
                                    Some(ProofFailureFamily::AmbiguousTargetMap)
                                };
                                let legality_witness = Some(DispatcherLegality {
                                    follow_block: default_target,
                                    postdom_ok: follow_or_bounded,
                                    side_effect_free_selector,
                                    ordinal_domain_complete,
                                    shared_tail_conflict,
                                    valid: proof_complete,
                                });
                                let proof = Some(DispatcherProofUnit {
                                    selector_expr: print_expr(&expr),
                                    rendered_selector_expr: Some(print_expr(&expr)),
                                    candidate_targets: targets.clone(),
                                    recovered_cases,
                                    selector_cardinality,
                                    target_cardinality,
                                    case_map_source,
                                    default_target,
                                    guard_set,
                                    follow_block: default_target,
                                    normalization,
                                    legality_witness,
                                    proof_scope: DispatcherProofScope::TerminatorLocal,
                                    proof_complete,
                                    failure_family,
                                });
                                this.dispatcher_proof_unit_count += 1;
                                if proof_complete {
                                    this.dispatcher_proof_completed_count += 1;
                                } else {
                                    this.dispatcher_proof_failed_count += 1;
                                }
                                this.indirect_target_set_refined_count += 1;
                                if dispatcher_recovered {
                                    this.dispatcher_shape_recovered_count += 1;
                                }
                                if target_cardinality == 0 || single_target_dispatcher {
                                    let evidence = UnsupportedControlEvidence {
                                        opcode: format!("{:?}", op.opcode),
                                        source_block: Some(block.start_address),
                                        target_expr: Some(print_expr(&expr)),
                                        successor_targets: targets,
                                        failure_family:
                                            UnsupportedControlFamily::NonStructuralDispatcher,
                                        surface: IndirectControlSurface::DispatcherLike,
                                        confidence: if dispatcher_recovered { 60 } else { 40 },
                                    };
                                    return Ok(LoweredTerminator::Unsupported {
                                        evidence,
                                        target_expr: Some(expr),
                                    });
                                }
                                Ok(LoweredTerminator::Switch {
                                    expr,
                                    targets,
                                    default_target,
                                    min_val,
                                    proof,
                                })
                            }
                        }
                        _ => Ok(LoweredTerminator::Fallthrough(this.next_block_address(idx))),
                    }
                },
            )?
        } else {
            LoweredTerminator::Fallthrough(self.next_block_address(idx))
        };

        self.terminator_cache.insert(idx, lowered.clone());
        Ok(lowered)
    }

    fn try_recover_branch_condition(
        &mut self,
        vn: &Varnode,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if self.options.is_64bit {
            return Ok(None);
        }

        let recovery_budget = BRANCH_CONDITION_RECOVERY_BUDGET_MIN
            .max(self.pcode.blocks.len() * BRANCH_CONDITION_RECOVERY_BUDGET_PER_BLOCK)
            .min(BRANCH_CONDITION_RECOVERY_BUDGET_MAX);
        if self.x86_branch_recovery_attempts >= recovery_budget {
            return Ok(None);
        }
        self.x86_branch_recovery_attempts += 1;

        let peeled = self.peel_passthrough_varnode(vn);
        let Some((_, root_op)) = self.lookup_def_site(&peeled) else {
            return Ok(None);
        };
        if !matches!(
            root_op.opcode,
            PcodeOpcode::BoolNegate
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntSLess
        ) {
            return Ok(None);
        }

        let predicate = self
            .match_test_branch_predicate(&peeled)
            .or_else(|| self.match_cmp_branch_predicate(&peeled));
        predicate
            .map(|predicate| self.lower_x86_branch_predicate(predicate))
            .transpose()
    }

    fn lower_wrapped_varnode(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        match self.lower_varnode(vn, visiting) {
            Ok(expr) => Ok(expr),
            Err(err) => {
                let Some((_, op)) = self.lookup_def_site(vn) else {
                    return Err(err);
                };
                match op.opcode {
                    PcodeOpcode::Copy
                    | PcodeOpcode::Cast
                    | PcodeOpcode::IntZExt
                    | PcodeOpcode::IntSExt
                        if op.inputs.len() == 1 =>
                    {
                        self.lower_wrapped_varnode(&op.inputs[0], visiting)
                    }
                    PcodeOpcode::IntAdd | PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                        if const_offset(&op.inputs[0]) == Some(0) {
                            self.lower_wrapped_varnode(&op.inputs[1], visiting)
                        } else if const_offset(&op.inputs[1]) == Some(0) {
                            self.lower_wrapped_varnode(&op.inputs[0], visiting)
                        } else {
                            Err(err)
                        }
                    }
                    _ => Err(err),
                }
            }
        }
    }

    fn branch_cond_too_complex(expr: &HirExpr) -> bool {
        Self::expr_contains_call(expr) || Self::expr_node_count(expr) > 24
    }

    fn expr_contains_call(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call { .. } => true,
            HirExpr::Const(_, _) | HirExpr::Var(_) => false,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => Self::expr_contains_call(expr),
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_contains_call(lhs) || Self::expr_contains_call(rhs)
            }
            HirExpr::Index { base, index, .. } => {
                Self::expr_contains_call(base) || Self::expr_contains_call(index)
            }
        }
    }

    fn expr_node_count(expr: &HirExpr) -> usize {
        match expr {
            HirExpr::Const(_, _) | HirExpr::Var(_) => 1,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => 1 + Self::expr_node_count(expr),
            HirExpr::Binary { lhs, rhs, .. } => {
                1 + Self::expr_node_count(lhs) + Self::expr_node_count(rhs)
            }
            HirExpr::Call { args, .. } => 1 + args.iter().map(Self::expr_node_count).sum::<usize>(),
            HirExpr::Index { base, index, .. } => {
                1 + Self::expr_node_count(base) + Self::expr_node_count(index)
            }
        }
    }

    fn lower_x86_branch_predicate(
        &mut self,
        predicate: X86BranchPredicate,
    ) -> Result<HirExpr, MlilPreviewError> {
        let mut visiting = HashSet::new();
        let lower = |this: &mut Self, vn: &Varnode, visiting: &mut HashSet<VarnodeKey>| {
            this.lower_wrapped_varnode(vn, visiting)
        };
        Ok(match predicate {
            X86BranchPredicate::EqZero(value) => {
                let value = lower(self, &value, &mut visiting)?;
                bool_binary(HirBinaryOp::Eq, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::NeZero(value) => {
                let value = lower(self, &value, &mut visiting)?;
                bool_binary(HirBinaryOp::Ne, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::SLtZero(value) => {
                let value = lower(self, &value, &mut visiting)?;
                bool_binary(HirBinaryOp::SLt, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::SLeZero(value) => {
                let value = lower(self, &value, &mut visiting)?;
                bool_binary(HirBinaryOp::SLe, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::SGtZero(value) => {
                let value = lower(self, &value, &mut visiting)?;
                bool_binary(HirBinaryOp::SLt, zero_like(&value), value)
            }
            X86BranchPredicate::SGeZero(value) => {
                let value = lower(self, &value, &mut visiting)?;
                bool_binary(HirBinaryOp::SLe, zero_like(&value), value)
            }
            X86BranchPredicate::MaskEqZero { value, mask } => {
                let value = lower(self, &value, &mut visiting)?;
                let mask = lower(self, &mask, &mut visiting)?;
                let masked = HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(value.clone()),
                    rhs: Box::new(mask),
                    ty: expr_type(&value),
                };
                bool_binary(HirBinaryOp::Eq, masked.clone(), zero_like(&masked))
            }
            X86BranchPredicate::MaskNeZero { value, mask } => {
                let value = lower(self, &value, &mut visiting)?;
                let mask = lower(self, &mask, &mut visiting)?;
                let masked = HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(value.clone()),
                    rhs: Box::new(mask),
                    ty: expr_type(&value),
                };
                bool_binary(HirBinaryOp::Ne, masked.clone(), zero_like(&masked))
            }
            X86BranchPredicate::Eq(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::Eq, lhs, rhs)
            }
            X86BranchPredicate::Ne(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::Ne, lhs, rhs)
            }
            X86BranchPredicate::ULt(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::Lt, lhs, rhs)
            }
            X86BranchPredicate::ULe(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::Le, lhs, rhs)
            }
            X86BranchPredicate::UGt(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::Lt, rhs, lhs)
            }
            X86BranchPredicate::UGe(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::Le, rhs, lhs)
            }
            X86BranchPredicate::SLt(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::SLt, lhs, rhs)
            }
            X86BranchPredicate::SLe(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::SLe, lhs, rhs)
            }
            X86BranchPredicate::SGt(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::SLt, rhs, lhs)
            }
            X86BranchPredicate::SGe(lhs_vn, rhs_vn) => {
                let lhs = lower(self, &lhs_vn, &mut visiting)?;
                let rhs = lower(self, &rhs_vn, &mut visiting)?;
                bool_binary(HirBinaryOp::SLe, rhs, lhs)
            }
        })
    }

    fn match_test_branch_predicate(&self, vn: &Varnode) -> Option<X86BranchPredicate> {
        let peeled = self.peel_passthrough_varnode(vn);
        if let Some((value, mask)) = self.match_test_zero_flag(&peeled) {
            return Some(match mask {
                Some(mask) => X86BranchPredicate::MaskEqZero { value, mask },
                None => X86BranchPredicate::EqZero(value),
            });
        }
        if let Some(inner) = self.match_bool_negate_from_peeled(&peeled)
            && let Some((value, mask)) = self.match_test_zero_flag(&inner)
        {
            return Some(match mask {
                Some(mask) => X86BranchPredicate::MaskNeZero { value, mask },
                None => X86BranchPredicate::NeZero(value),
            });
        }
        if let Some(value) = self.match_test_sign_flag(&peeled) {
            return Some(X86BranchPredicate::SLtZero(value));
        }
        if let Some(inner) = self.match_bool_negate_from_peeled(&peeled)
            && let Some(value) = self.match_test_sign_flag(&inner)
        {
            return Some(X86BranchPredicate::SGeZero(value));
        }
        if let Some(value) = self.match_test_gt_zero(&peeled) {
            return Some(X86BranchPredicate::SGtZero(value));
        }
        if let Some(value) = self.match_test_le_zero(&peeled) {
            return Some(X86BranchPredicate::SLeZero(value));
        }
        None
    }

    fn infer_switch_default_target(&self, idx: usize, targets: &[u64]) -> Option<u64> {
        let fallthrough = self.next_block_address(idx)?;
        targets.contains(&fallthrough).then_some(fallthrough)
    }

    fn infer_unconditional_branch_successor_target(&self, idx: usize) -> Option<u64> {
        let block = self.pcode.blocks.get(idx)?;
        if block.successors.len() != 1 {
            return None;
        }
        let succ_idx = block.successors[0] as usize;
        (succ_idx < self.pcode.blocks.len()).then(|| self.block_target_key(succ_idx))
    }

    fn infer_cbranch_true_target_from_successors(&self, idx: usize) -> Option<u64> {
        let block = self.pcode.blocks.get(idx)?;
        let fallthrough = self.next_block_address(idx);
        let mut candidates = Vec::new();
        for succ_idx in &block.successors {
            let succ_idx = *succ_idx as usize;
            if succ_idx >= self.pcode.blocks.len() {
                continue;
            }
            let target = self.block_target_key(succ_idx);
            if Some(target) == fallthrough {
                continue;
            }
            if !candidates.contains(&target) {
                candidates.push(target);
            }
        }
        if candidates.len() == 1 {
            Some(candidates[0])
        } else {
            None
        }
    }

    fn infer_branchind_target_from_input(
        &self,
        idx: usize,
        op: &PcodeOp,
        switch_var: &Varnode,
    ) -> Option<u64> {
        self.resolve_branch_target_index_with_recovery(idx, op, switch_var)
            .or_else(|| self.infer_branchind_target_from_load_address(switch_var))
            .map(|target_idx| self.block_target_key(target_idx))
    }

    fn resolve_branch_target_index_with_recovery(
        &self,
        idx: usize,
        op: &PcodeOp,
        vn: &Varnode,
    ) -> Option<usize> {
        resolve_branch_target_index(self.pcode, &self.address_to_index, idx, op, vn).or_else(|| {
            let peeled = self.peel_passthrough_varnode(vn);
            if peeled != *vn {
                if let Some(target_idx) = resolve_branch_target_index(
                    self.pcode,
                    &self.address_to_index,
                    idx,
                    op,
                    &peeled,
                ) {
                    return Some(target_idx);
                }
            }

            let target_addr = self.infer_branch_target_address_one_step(vn)?;
            canonical_block_index_for_address(self.pcode, &self.address_to_index, target_addr)
        })
    }

    fn infer_branch_target_address_one_step(&self, vn: &Varnode) -> Option<u64> {
        if let Some(addr) = branch_target_address(vn) {
            return Some(addr);
        }

        let peeled = self.peel_passthrough_varnode(vn);
        if let Some(addr) = branch_target_address(&peeled) {
            return Some(addr);
        }

        let (_, def) = self.lookup_def_site(&peeled)?;
        match def.opcode {
            PcodeOpcode::IntAdd | PcodeOpcode::IntSub if def.inputs.len() == 2 => {
                self.eval_one_step_address_expr(def.opcode, &def.inputs[0], &def.inputs[1])
            }
            _ => None,
        }
    }

    fn eval_one_step_address_expr(
        &self,
        opcode: PcodeOpcode,
        lhs: &Varnode,
        rhs: &Varnode,
    ) -> Option<u64> {
        let lhs_const = const_offset(lhs);
        let rhs_const = const_offset(rhs);
        let (base_vn, delta) = match (lhs_const, rhs_const) {
            (Some(delta), None) => (rhs, delta),
            (None, Some(delta)) => (lhs, delta),
            _ => return None,
        };

        let base_addr = branch_target_address(&self.peel_passthrough_varnode(base_vn))?;
        let base = i128::from(base_addr);
        let delta = i128::from(delta);
        let value = match opcode {
            PcodeOpcode::IntAdd => base + delta,
            PcodeOpcode::IntSub => base - delta,
            _ => return None,
        };
        (0..=i128::from(u64::MAX))
            .contains(&value)
            .then_some(value as u64)
    }

    fn infer_branchind_target_from_load_address(&self, switch_var: &Varnode) -> Option<usize> {
        let peeled = self.peel_passthrough_varnode(switch_var);
        let (_, def) = self.lookup_def_site(&peeled)?;
        if def.opcode != PcodeOpcode::Load || def.inputs.len() < 2 {
            return None;
        }

        // For simple jump-table like forms, treat the computed LOAD address itself as
        // candidate target when it already lands inside the current CFG slice.
        let load_addr_vn = def.inputs.last()?;
        let load_addr = self.infer_branch_target_address_one_step(load_addr_vn)?;
        canonical_block_index_for_address(self.pcode, &self.address_to_index, load_addr)
    }

    fn lower_branchind_switch_expr(
        &mut self,
        idx: usize,
        switch_var: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let exact_expr = self.lower_wrapped_varnode(switch_var, visiting).ok();
        let alias_expr = self.lower_branchind_same_block_alias_expr(idx, switch_var, visiting);
        let predecessor_expr =
            self.recover_branchind_switch_expr_from_predecessors(idx, switch_var, visiting);

        let best_jump_table_expr = exact_expr
            .iter()
            .chain(alias_expr.iter())
            .chain(predecessor_expr.iter())
            .find(|expr| super::switch_table::has_jump_table_surface(expr, &self.options))
            .cloned();

        match (
            best_jump_table_expr,
            exact_expr,
            alias_expr,
            predecessor_expr,
        ) {
            (Some(expr), _, _, _) => Ok(expr),
            (None, Some(expr), _, _) => Ok(expr),
            (None, None, Some(alias), _) => Ok(alias),
            (None, None, None, Some(expr)) => Ok(expr),
            (None, None, None, None) => self.lower_wrapped_varnode(switch_var, visiting),
        }
    }

    fn lower_branchind_same_block_alias_expr(
        &mut self,
        idx: usize,
        switch_var: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<HirExpr> {
        let pcode_idx = self.pcode_block_idx(idx);
        let block = &self.pcode.blocks[pcode_idx];
        let term_idx = self.block_terminator_index(block)?;
        let key = VarnodeKey::from(switch_var);
        let exact_local = self
            .block_defs
            .get(pcode_idx)
            .and_then(|defs| defs.get(&key))
            .and_then(|indices| {
                indices
                    .iter()
                    .copied()
                    .rev()
                    .find(|def_idx| *def_idx < term_idx)
            });

        for def_idx in (0..term_idx).rev() {
            let op = &block.ops[def_idx];
            let Some(output) = op.output.as_ref() else {
                continue;
            };
            if output.is_constant
                || output.space_id != switch_var.space_id
                || output.offset != switch_var.offset
                || output.size < switch_var.size
            {
                continue;
            }
            if !is_safe_selector_provenance_opcode(op.opcode) {
                if exact_local == Some(def_idx) {
                    continue;
                }
                continue;
            }
            let site = LoweringSite {
                block_idx: pcode_idx,
                op_idx: def_idx,
            };
            if let Ok(expr) = self.with_lowering_site(site, |this| {
                this.lower_selector_source_expr(output, visiting)
            }) {
                return Some(expr);
            }
        }
        None
    }

    fn recover_branchind_render_selector_expr(
        &mut self,
        idx: usize,
        selector_alias: &Varnode,
        fallback: HirExpr,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> HirExpr {
        self.lower_branchind_same_block_alias_expr(idx, selector_alias, visiting)
            .or_else(|| self.recover_selector_expr_from_predecessors(idx, selector_alias, visiting))
            .unwrap_or(fallback)
    }

    fn recover_branchind_switch_expr_from_predecessors(
        &mut self,
        idx: usize,
        switch_var: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<HirExpr> {
        let predecessors = self.predecessors.get(idx)?.clone();
        if preview_builder_diag_enabled() {
            let pred_blocks = predecessors
                .iter()
                .map(|pred_idx| format!("0x{:x}", self.block_start_address(*pred_idx)))
                .collect::<Vec<_>>()
                .join(",");
            eprintln!(
                "[DIAG] branchind_pred_scan block=0x{:x} preds=[{}] switch_var=space={} off=0x{:x} size={}",
                self.block_start_address(idx),
                pred_blocks,
                switch_var.space_id,
                switch_var.offset,
                switch_var.size
            );
        }
        for pred_idx in predecessors {
            let pcode_idx = self.pcode_block_idx(pred_idx);
            let block = self.pcode.blocks.get(pcode_idx)?;
            let term_idx = self
                .block_terminator_index(block)
                .unwrap_or(block.ops.len());
            for op_idx in (0..term_idx).rev() {
                let op = &block.ops[op_idx];
                let Some(output) = op.output.as_ref() else {
                    continue;
                };
                if output.is_constant
                    || output.space_id != switch_var.space_id
                    || output.offset != switch_var.offset
                {
                    continue;
                }
                let site = LoweringSite {
                    block_idx: pcode_idx,
                    op_idx,
                };
                if let Ok(expr) = self
                    .with_lowering_site(site, |this| this.lower_wrapped_varnode(output, visiting))
                {
                    if preview_builder_diag_enabled() {
                        eprintln!(
                            "[DIAG] branchind_pred_expr block=0x{:x} pred=0x{:x} op_seq=0x{:x} expr={}",
                            self.block_start_address(idx),
                            block.start_address,
                            op.seq_num,
                            print_expr(&expr)
                        );
                    }
                    return Some(expr);
                }
            }
            if term_idx < block.ops.len() {
                let term_op = &block.ops[term_idx];
                if term_op.opcode == PcodeOpcode::BranchInd
                    && let Some(term_input) = term_op.inputs.first()
                {
                    let site = LoweringSite {
                        block_idx: pcode_idx,
                        op_idx: term_idx,
                    };
                    if let Ok(expr) = self.with_lowering_site(site, |this| {
                        this.lower_wrapped_varnode(term_input, visiting)
                    }) && super::switch_table::has_jump_table_surface(&expr, &self.options)
                    {
                        if preview_builder_diag_enabled() {
                            eprintln!(
                                "[DIAG] branchind_pred_term_expr block=0x{:x} pred=0x{:x} term_seq=0x{:x} expr={}",
                                self.block_start_address(idx),
                                block.start_address,
                                term_op.seq_num,
                                print_expr(&expr)
                            );
                        }
                        return Some(expr);
                    }
                }
            }
        }

        None
    }

    fn normalize_rendered_selector_expr(&self, expr: HirExpr, min_val: i64) -> (HirExpr, i64) {
        let Some((base_expr, offset)) = super::switch_table::split_selector_base_offset(&expr)
        else {
            return (expr, min_val);
        };
        let Some(next_min) = min_val.checked_add(offset) else {
            return (expr, min_val);
        };
        (base_expr, next_min)
    }

    fn selector_normalization_for_branchind(
        &self,
        expr: &HirExpr,
        min_val: i64,
        entry_size: u64,
        recovered_cases: Option<&[(i64, u64)]>,
    ) -> SelectorNormalization {
        let guard_bounds = recovered_cases
            .filter(|cases| !cases.is_empty())
            .map(|cases| {
                let min_case = cases.iter().map(|(value, _)| *value).min();
                let max_case = cases.iter().map(|(value, _)| *value).max();
                vec![(min_case, max_case)]
            })
            .unwrap_or_default();
        SelectorNormalization {
            base_subtract: (min_val != 0).then_some(min_val),
            mask: None,
            stride: (entry_size > 1).then_some(entry_size),
            width: Self::selector_expr_width(expr),
            address_space: None,
            guard_bounds,
        }
    }

    fn selector_expr_width(expr: &HirExpr) -> Option<u32> {
        match expr {
            HirExpr::Const(_, ty)
            | HirExpr::Load { ty, .. }
            | HirExpr::Cast { ty, .. }
            | HirExpr::Unary { ty, .. }
            | HirExpr::Binary { ty, .. } => Self::nir_type_width(ty),
            HirExpr::Var(_) => None,
            HirExpr::Call { ty, .. } => Self::nir_type_width(ty),
            HirExpr::PtrOffset { .. } => None,
            HirExpr::AggregateCopy { size, .. } => Some(*size * 8),
            HirExpr::Index { elem_ty, .. } => Self::nir_type_width(elem_ty),
        }
    }

    fn nir_type_width(ty: &NirType) -> Option<u32> {
        match ty {
            NirType::Bool => Some(1),
            NirType::Int { bits, .. } => Some(*bits),
            NirType::Ptr(_) => None,
            NirType::Aggregate { size, .. } => Some(*size * 8),
            NirType::Float { bits } => Some(*bits),
            NirType::Unknown => None,
        }
    }

    fn selector_expr_is_side_effect_free(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Const(_, _) | HirExpr::Var(_) => true,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => {
                Self::selector_expr_is_side_effect_free(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::selector_expr_is_side_effect_free(lhs)
                    && Self::selector_expr_is_side_effect_free(rhs)
            }
            HirExpr::Index { base, index, .. } => {
                Self::selector_expr_is_side_effect_free(base)
                    && Self::selector_expr_is_side_effect_free(index)
            }
            HirExpr::Call { .. } => false,
        }
    }

    fn infer_branchind_targets_from_jump_table_expr(
        &mut self,
        idx: usize,
        switch_expr: &HirExpr,
        selector_alias: Option<&Varnode>,
    ) -> Option<InferredJumpTableTargets> {
        const MAX_JUMP_TABLE_CASES: u64 = 256;

        let binary = self.binary?;
        let selector =
            super::switch_table::recover_switch_discriminant(switch_expr, &self.options)?;
        if preview_builder_diag_enabled() {
            eprintln!(
                "[DIAG] branchind_switch_selector block=0x{:x} expr={} discrim={} min={} table=0x{:x} target_base={:?} relative={} entry_size={}",
                self.block_start_address(idx),
                print_expr(switch_expr),
                print_expr(&selector.discriminant),
                selector.min_val,
                selector.table_base,
                selector.target_base.map(|addr| format!("0x{addr:x}")),
                selector.relative_entries,
                selector.entry_size
            );
        }
        let normalized_selector = selector_alias
            .and_then(|alias| {
                let mut selector_visiting = HashSet::new();
                self.recover_selector_expr_from_predecessors(idx, alias, &mut selector_visiting)
            })
            .unwrap_or_else(|| selector.discriminant.clone());
        let max_selector = self
            .infer_branchind_selector_upper_bound(idx, &normalized_selector, selector.min_val)
            .or_else(|| {
                selector_alias.and_then(|alias| {
                    self.infer_branchind_selector_upper_bound_from_alias_family(
                        idx,
                        alias,
                        selector.min_val,
                    )
                })
            })?;
        if preview_builder_diag_enabled() {
            eprintln!(
                "[DIAG] branchind_switch_bound block=0x{:x} normalized_selector={} max_selector={} min={}",
                self.block_start_address(idx),
                print_expr(&normalized_selector),
                max_selector,
                selector.min_val
            );
        }
        let case_count = max_selector.saturating_add(1).min(MAX_JUMP_TABLE_CASES);
        if case_count < 2 || selector.entry_size == 0 {
            return None;
        }

        let pointer_size = u64::from(self.options.pointer_size.max(1));
        let entry_width = selector.entry_size.min(pointer_size).max(4) as usize;
        let little_endian = !binary.arch_spec.contains(":BE:");
        let decode_modes = branchind_decode_modes(
            selector.relative_entries,
            selector.table_base,
            selector.target_base,
            self.options.image_base,
        );

        let mut best: Option<InferredJumpTableTargets> = None;
        for (decode_mode, relative_entries, relative_base) in decode_modes {
            let mut recovered_cases = Vec::new();
            let mut unique_targets = Vec::new();
            for ordinal in 0..case_count {
                let Some(entry_addr) = selector
                    .table_base
                    .checked_add(ordinal.saturating_mul(selector.entry_size))
                else {
                    break;
                };
                let Some(raw) = binary.get_bytes(entry_addr, entry_width) else {
                    break;
                };
                let Some(target_addr) =
                    decode_jump_table_target(&raw, little_endian, relative_entries, relative_base)
                else {
                    continue;
                };
                let Some(target_idx) = canonical_block_index_for_address(
                    self.pcode,
                    &self.address_to_index,
                    target_addr,
                ) else {
                    continue;
                };
                let target = self.block_target_key(target_idx);
                recovered_cases.push((selector.min_val + ordinal as i64, target));
                if !unique_targets.contains(&target) {
                    unique_targets.push(target);
                }
            }
            if unique_targets.len() < 2 || recovered_cases.len() < 2 {
                continue;
            }
            let candidate = InferredJumpTableTargets {
                unique_targets,
                recovered_cases,
                selector_cardinality: case_count as usize,
                decode_mode,
            };
            let replace = best.as_ref().is_none_or(|current| {
                candidate.recovered_cases.len() > current.recovered_cases.len()
                    || (candidate.recovered_cases.len() == current.recovered_cases.len()
                        && candidate.unique_targets.len() > current.unique_targets.len())
            });
            if replace {
                best = Some(candidate);
            }
        }

        if preview_builder_diag_enabled() {
            if let Some(best) = best.as_ref() {
                eprintln!(
                    "[DIAG] branchind_switch_targets block=0x{:x} mode={} targets={:?} cases={:?}",
                    self.block_start_address(idx),
                    best.decode_mode,
                    best.unique_targets,
                    best.recovered_cases
                );
            } else {
                eprintln!(
                    "[DIAG] branchind_switch_targets block=0x{:x} mode=none targets=[]",
                    self.block_start_address(idx)
                );
            }
        }

        best
    }

    fn recover_branchind_jump_table_selector_varnode(&self, idx: usize) -> Option<Varnode> {
        let pcode_idx = self.pcode_block_idx(idx);
        let block = self.pcode.blocks.get(pcode_idx)?;
        let term_idx = self.block_terminator_index(block)?;
        for op_idx in (0..term_idx).rev() {
            let op = &block.ops[op_idx];
            if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
                continue;
            }
            if let Some(selector) = self.extract_jump_table_selector_varnode(&op.inputs[1]) {
                return Some(selector);
            }
        }
        None
    }

    fn extract_jump_table_selector_varnode(&self, ptr: &Varnode) -> Option<Varnode> {
        let (_, op) = self.lookup_def_site(ptr)?;
        if op.opcode != PcodeOpcode::IntAdd || op.inputs.len() != 2 {
            return None;
        }
        self.extract_scaled_selector_varnode(&op.inputs[0])
            .or_else(|| self.extract_scaled_selector_varnode(&op.inputs[1]))
    }

    fn extract_scaled_selector_varnode(&self, vn: &Varnode) -> Option<Varnode> {
        let peeled = self.peel_passthrough_varnode(vn);
        if peeled.is_constant || self.materializes_const_address(&peeled) {
            return None;
        }
        let (_, op) = self.lookup_def_site(&peeled)?;
        match op.opcode {
            PcodeOpcode::IntLeft | PcodeOpcode::IntMult if op.inputs.len() == 2 => {
                if op.inputs[0].is_constant {
                    Some(op.inputs[1].clone())
                } else if op.inputs[1].is_constant {
                    Some(op.inputs[0].clone())
                } else {
                    None
                }
            }
            _ => Some(peeled),
        }
    }

    fn materializes_const_address(&self, vn: &Varnode) -> bool {
        let Some((_, op)) = self.lookup_def_site(vn) else {
            return false;
        };
        match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
                if op.inputs.len() == 1 =>
            {
                op.inputs[0].is_constant || self.materializes_const_address(&op.inputs[0])
            }
            PcodeOpcode::IntAdd | PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                op.inputs[0].is_constant && op.inputs[1].is_constant
            }
            _ => false,
        }
    }

    fn recover_selector_expr_from_predecessors(
        &mut self,
        idx: usize,
        selector_alias: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<HirExpr> {
        let cache_key = (idx, selector_alias.space_id, selector_alias.offset);
        if let Some(cached) = self.selector_representatives.get(&cache_key) {
            return Some(cached.clone());
        }

        let predecessors = self.predecessors.get(idx)?.clone();
        let selector_family = (selector_alias.space_id, selector_alias.offset);
        for pred_idx in predecessors {
            let pcode_idx = self.pcode_block_idx(pred_idx);
            let block = self.pcode.blocks.get(pcode_idx)?;
            let term_idx = self
                .block_terminator_index(block)
                .unwrap_or(block.ops.len());
            for op_idx in (0..term_idx).rev() {
                let op = &block.ops[op_idx];
                let Some(output) = op.output.as_ref() else {
                    continue;
                };
                if (output.space_id, output.offset) != selector_family {
                    continue;
                }
                let site = LoweringSite {
                    block_idx: pcode_idx,
                    op_idx,
                };
                if let Ok(expr) = self.with_lowering_site(site, |this| {
                    this.lower_selector_source_expr(output, visiting)
                }) {
                    self.selector_representatives
                        .insert(cache_key, expr.clone());
                    return Some(expr);
                }
            }
        }

        None
    }

    fn lower_selector_source_expr(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<HirExpr, MlilPreviewError> {
        let peeled = self.peel_passthrough_varnode(vn);
        if let Some((_, op)) = self.lookup_def_site(&peeled) {
            match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::SubPiece
                    if !op.inputs.is_empty() =>
                {
                    return self.lower_selector_source_expr(&op.inputs[0], visiting);
                }
                _ => {}
            }
        }
        self.lower_wrapped_varnode(&peeled, visiting)
    }

    fn infer_branchind_selector_upper_bound(
        &mut self,
        idx: usize,
        selector: &HirExpr,
        min_val: i64,
    ) -> Option<u64> {
        let normalized = strip_casts(selector);
        let mut best: Option<u64> = None;
        let predecessors = self.predecessors.get(idx)?.clone();

        for pred_idx in predecessors {
            let LoweredTerminator::Cond {
                cond,
                true_target,
                false_target,
            } = self.lower_block_terminator(pred_idx).ok()?
            else {
                continue;
            };
            let current_target = self.block_target_key(idx);
            let Some(bound) = (if true_target == current_target {
                extract_selector_upper_bound_from_cond(&cond, &normalized, true)
            } else if false_target == Some(current_target) {
                extract_selector_upper_bound_from_cond(&cond, &normalized, false)
            } else {
                None
            }) else {
                continue;
            };
            let normalized_bound = if min_val <= 0 {
                bound.checked_add((-min_val) as u64)?
            } else {
                bound.checked_sub(min_val as u64)?
            };
            best = Some(best.map_or(normalized_bound, |existing| existing.min(normalized_bound)));
        }

        best
    }

    fn infer_branchind_selector_upper_bound_from_alias_family(
        &mut self,
        idx: usize,
        selector_alias: &Varnode,
        min_val: i64,
    ) -> Option<u64> {
        let mut best: Option<u64> = None;
        let predecessors = self.predecessors.get(idx)?.clone();
        let selector_family = (selector_alias.space_id, selector_alias.offset);

        for pred_idx in predecessors {
            let current_target = self.block_target_key(idx);
            let LoweredTerminator::Cond {
                true_target,
                false_target,
                ..
            } = self.lower_block_terminator(pred_idx).ok()?
            else {
                continue;
            };
            let current_on_true = if true_target == current_target {
                true
            } else if false_target == Some(current_target) {
                false
            } else {
                continue;
            };
            let bound = self.extract_selector_upper_bound_from_predicate_family(
                pred_idx,
                selector_family,
                current_on_true,
            )?;
            let normalized_bound = if min_val <= 0 {
                bound.checked_add((-min_val) as u64)?
            } else {
                bound.checked_sub(min_val as u64)?
            };
            best = Some(best.map_or(normalized_bound, |existing| existing.min(normalized_bound)));
        }

        best
    }

    fn extract_selector_upper_bound_from_predicate_family(
        &self,
        pred_idx: usize,
        selector_family: (u64, u64),
        current_on_true: bool,
    ) -> Option<u64> {
        let pcode_idx = self.pcode_block_idx(pred_idx);
        let block = self.pcode.blocks.get(pcode_idx)?;
        let term_idx = self
            .block_terminator_index(block)
            .unwrap_or(block.ops.len());
        let mut less_than_bound: Option<u64> = None;
        let mut equality_bound: Option<u64> = None;

        for op in block.ops.iter().take(term_idx) {
            match op.opcode {
                PcodeOpcode::IntLess | PcodeOpcode::IntSLess if op.inputs.len() == 2 => {
                    if same_family_varnode(&op.inputs[0], selector_family)
                        && op.inputs[1].is_constant
                    {
                        less_than_bound = u64::try_from(op.inputs[1].constant_val).ok();
                    }
                }
                PcodeOpcode::IntEqual if op.inputs.len() == 2 => {
                    if op.inputs[1].is_zero()
                        && let Some((lhs, rhs)) = self.match_cmp_diff_from_peeled(&op.inputs[0])
                        && same_family_varnode(&lhs, selector_family)
                        && rhs.is_constant
                    {
                        equality_bound = u64::try_from(rhs.constant_val).ok();
                    } else if op.inputs[0].is_zero()
                        && let Some((lhs, rhs)) = self.match_cmp_diff_from_peeled(&op.inputs[1])
                        && same_family_varnode(&lhs, selector_family)
                        && rhs.is_constant
                    {
                        equality_bound = u64::try_from(rhs.constant_val).ok();
                    }
                }
                _ => {}
            }
        }

        match (current_on_true, less_than_bound, equality_bound) {
            (false, Some(less), Some(eq)) if less == eq => Some(less),
            (true, Some(less), _) => less.checked_sub(1),
            _ => None,
        }
    }

    fn match_cmp_branch_predicate(&self, vn: &Varnode) -> Option<X86BranchPredicate> {
        let peeled = self.peel_passthrough_varnode(vn);

        if let Some((lhs, rhs)) = self.match_cmp_zero_flag_from_peeled(&peeled) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::Eq(lhs, rhs));
            }
        }
        if let Some(inner) = self.match_bool_negate_from_peeled(&peeled)
            && let Some((lhs, rhs)) = self.match_cmp_zero_flag(&inner)
        {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::Ne(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_cmp_carry_flag_from_peeled(&peeled) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::ULt(lhs, rhs));
            }
        }
        if let Some(inner) = self.match_bool_negate_from_peeled(&peeled)
            && let Some((lhs, rhs)) = self.match_cmp_carry_flag(&inner)
        {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::UGe(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_unsigned_le(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::ULe(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_unsigned_gt(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::UGt(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_cmp_sign_overflow_ne(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::SLt(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_cmp_sign_overflow_eq(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::SGe(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_signed_gt(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::SGt(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_signed_le(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::SLe(lhs, rhs));
            }
        }
        None
    }

    fn peel_passthrough_varnode(&self, vn: &Varnode) -> Varnode {
        let scope = self.current_lowering_site;
        let start_key = VarnodeKey::from(vn);
        let cache_key = (scope, start_key.clone());
        let mut peel_cache = self.peel_cache.borrow_mut();
        if let Some(cached) = peel_cache.get(&cache_key).cloned() {
            return cached;
        }

        let mut current = vn.clone();
        let mut visited: Vec<VarnodeKey> = Vec::new();
        for _ in 0..PASSTHROUGH_PEEL_MAX_STEPS {
            let Some((_, op)) = self.lookup_def_site(&current) else {
                break;
            };
            let current_key = VarnodeKey::from(&current);
            if let Some(cached) = peel_cache.get(&(scope, current_key.clone())).cloned() {
                current = cached;
                break;
            }
            visited.push(current_key);

            match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                    if op.inputs.len() == 1 =>
                {
                    current = op.inputs[0].clone();
                }
                PcodeOpcode::IntAdd | PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                    if const_offset(&op.inputs[0]) == Some(0) {
                        current = op.inputs[1].clone();
                    } else if const_offset(&op.inputs[1]) == Some(0) {
                        current = op.inputs[0].clone();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        let final_value = current.clone();
        peel_cache.insert(cache_key, final_value.clone());
        for visited_key in visited {
            peel_cache.insert((scope, visited_key), final_value.clone());
        }

        current
    }

    fn match_bool_negate(&self, vn: &Varnode) -> Option<Varnode> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_bool_negate_from_peeled(&peeled)
    }

    fn match_bool_negate_from_peeled(&self, peeled: &Varnode) -> Option<Varnode> {
        let (_, op) = self.lookup_def_site(&peeled)?;
        (op.opcode == PcodeOpcode::BoolNegate && op.inputs.len() == 1).then(|| op.inputs[0].clone())
    }

    fn match_bool_binary(&self, vn: &Varnode, opcode: PcodeOpcode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_bool_binary_from_peeled(&peeled, opcode)
    }

    fn match_bool_binary_from_peeled(
        &self,
        peeled: &Varnode,
        opcode: PcodeOpcode,
    ) -> Option<(Varnode, Varnode)> {
        let (_, op) = self.lookup_def_site(&peeled)?;
        (op.opcode == opcode && op.inputs.len() == 2)
            .then(|| (op.inputs[0].clone(), op.inputs[1].clone()))
    }

    fn match_compare_pair(&self, vn: &Varnode, opcode: PcodeOpcode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_compare_pair_from_peeled(&peeled, opcode)
    }

    fn match_compare_pair_from_peeled(
        &self,
        peeled: &Varnode,
        opcode: PcodeOpcode,
    ) -> Option<(Varnode, Varnode)> {
        let (_, op) = self.lookup_def_site(&peeled)?;
        (op.opcode == opcode && op.inputs.len() == 2)
            .then(|| (op.inputs[0].clone(), op.inputs[1].clone()))
    }

    fn match_zero_compare_input(&self, vn: &Varnode) -> Option<Varnode> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_zero_compare_input_from_peeled(&peeled)
    }

    fn match_zero_compare_input_from_peeled(&self, peeled: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_compare_pair_from_peeled(peeled, PcodeOpcode::IntEqual)?;
        if lhs.is_zero() {
            return Some(rhs);
        }
        if rhs.is_zero() {
            return Some(lhs);
        }
        None
    }

    fn match_signed_less_than_zero_input(&self, vn: &Varnode) -> Option<Varnode> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_signed_less_than_zero_input_from_peeled(&peeled)
    }

    fn match_signed_less_than_zero_input_from_peeled(&self, peeled: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_compare_pair_from_peeled(peeled, PcodeOpcode::IntSLess)?;
        if rhs.is_zero() {
            return Some(lhs);
        }
        if lhs.is_zero() {
            return Some(rhs);
        }
        None
    }

    fn classify_test_input(&self, source: &Varnode) -> Option<(Varnode, Option<Varnode>)> {
        let peeled = self.peel_passthrough_varnode(source);
        let (_, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntAnd || op.inputs.len() != 2 {
            return None;
        }
        let lhs = self.peel_passthrough_varnode(&op.inputs[0]);
        let rhs = self.peel_passthrough_varnode(&op.inputs[1]);
        if lhs == rhs {
            return Some((lhs, None));
        }
        if rhs.is_constant {
            return Some((lhs, Some(rhs)));
        }
        if lhs.is_constant {
            return Some((rhs, Some(lhs)));
        }
        None
    }

    fn match_test_zero_flag(&self, vn: &Varnode) -> Option<(Varnode, Option<Varnode>)> {
        let source = self.match_zero_compare_input(vn)?;
        self.classify_test_input(&source)
    }

    fn match_test_sign_flag(&self, vn: &Varnode) -> Option<Varnode> {
        let source = self.match_signed_less_than_zero_input(vn)?;
        let (value, mask) = self.classify_test_input(&source)?;
        mask.is_none().then_some(value)
    }

    fn match_test_sign_eq_zero(&self, vn: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntEqual)?;
        if self.is_zero_valued_varnode(&lhs) {
            return self.match_test_sign_flag(&rhs);
        }
        if self.is_zero_valued_varnode(&rhs) {
            return self.match_test_sign_flag(&lhs);
        }
        None
    }

    fn match_test_sign_ne_zero(&self, vn: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntNotEqual)?;
        if self.is_zero_valued_varnode(&lhs) {
            return self.match_test_sign_flag(&rhs);
        }
        if self.is_zero_valued_varnode(&rhs) {
            return self.match_test_sign_flag(&lhs);
        }
        None
    }

    fn match_test_gt_zero(&self, vn: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolAnd)?;
        self.match_test_gt_zero_pair(&lhs, &rhs)
            .or_else(|| self.match_test_gt_zero_pair(&rhs, &lhs))
    }

    fn match_test_gt_zero_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<Varnode> {
        let not_zf = self.match_bool_negate(lhs)?;
        let zf_value = self.match_test_zero_flag(&not_zf)?.0;
        let sign_value = self.match_test_sign_eq_zero(rhs)?;
        (zf_value == sign_value).then_some(zf_value)
    }

    fn match_test_le_zero(&self, vn: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolOr)?;
        self.match_test_le_zero_pair(&lhs, &rhs)
            .or_else(|| self.match_test_le_zero_pair(&rhs, &lhs))
    }

    fn match_test_le_zero_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<Varnode> {
        let zf_value = self.match_test_zero_flag(lhs)?.0;
        let sign_value = self.match_test_sign_ne_zero(rhs)?;
        (zf_value == sign_value).then_some(zf_value)
    }

    fn match_cmp_diff(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_cmp_diff_from_peeled(&peeled)
    }

    fn match_cmp_diff_from_peeled(&self, peeled: &Varnode) -> Option<(Varnode, Varnode)> {
        let (_, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntSub || op.inputs.len() != 2 {
            return None;
        }
        Some((op.inputs[0].clone(), op.inputs[1].clone()))
    }

    fn match_cmp_zero_flag(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let source = self.match_zero_compare_input(vn)?;
        self.match_cmp_diff(&source)
    }

    fn match_cmp_zero_flag_from_peeled(&self, peeled: &Varnode) -> Option<(Varnode, Varnode)> {
        let source = self.match_zero_compare_input_from_peeled(peeled)?;
        self.match_cmp_diff(&source)
    }

    fn match_cmp_carry_flag(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_cmp_carry_flag_from_peeled(&peeled)
    }

    fn match_cmp_carry_flag_from_peeled(&self, peeled: &Varnode) -> Option<(Varnode, Varnode)> {
        let (_, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntLess || op.inputs.len() != 2 {
            return None;
        }
        Some((op.inputs[0].clone(), op.inputs[1].clone()))
    }

    fn match_cmp_sign_flag(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let source = self.match_signed_less_than_zero_input(vn)?;
        self.match_cmp_diff(&source)
    }

    fn match_cmp_overflow_flag(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
        let (_, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntSBorrow || op.inputs.len() != 2 {
            return None;
        }
        Some((op.inputs[0].clone(), op.inputs[1].clone()))
    }

    fn match_cmp_sign_overflow_ne(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntNotEqual)?;
        self.match_cmp_sign_overflow_pair(&lhs, &rhs)
            .or_else(|| self.match_cmp_sign_overflow_pair(&rhs, &lhs))
    }

    fn match_cmp_sign_overflow_eq(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntEqual)?;
        self.match_cmp_sign_overflow_pair(&lhs, &rhs)
            .or_else(|| self.match_cmp_sign_overflow_pair(&rhs, &lhs))
    }

    fn match_cmp_sign_overflow_pair(
        &self,
        lhs: &Varnode,
        rhs: &Varnode,
    ) -> Option<(Varnode, Varnode)> {
        let sign = self.match_cmp_sign_flag(lhs)?;
        let overflow = self.match_cmp_overflow_flag(rhs)?;
        same_cmp_pair(&sign, &overflow).then_some(sign)
    }

    fn match_unsigned_le(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolOr)?;
        self.match_unsigned_le_pair(&lhs, &rhs)
            .or_else(|| self.match_unsigned_le_pair(&rhs, &lhs))
    }

    fn match_unsigned_le_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<(Varnode, Varnode)> {
        let carry = self.match_cmp_carry_flag(lhs)?;
        let zero = self.match_cmp_zero_flag(rhs)?;
        same_cmp_pair(&carry, &zero).then_some(carry)
    }

    fn match_unsigned_gt(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolAnd)?;
        self.match_unsigned_gt_pair(&lhs, &rhs)
            .or_else(|| self.match_unsigned_gt_pair(&rhs, &lhs))
    }

    fn match_unsigned_gt_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<(Varnode, Varnode)> {
        let not_cf = self.match_bool_negate(lhs)?;
        let carry = self.match_cmp_carry_flag(&not_cf)?;
        let not_zf = self.match_bool_negate(rhs)?;
        let zero = self.match_cmp_zero_flag(&not_zf)?;
        same_cmp_pair(&carry, &zero).then_some(carry)
    }

    fn match_signed_gt(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolAnd)?;
        self.match_signed_gt_pair(&lhs, &rhs)
            .or_else(|| self.match_signed_gt_pair(&rhs, &lhs))
    }

    fn match_signed_gt_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<(Varnode, Varnode)> {
        let not_zf = self.match_bool_negate(lhs)?;
        let zero = self.match_cmp_zero_flag(&not_zf)?;
        let sign = self.match_cmp_sign_overflow_eq(rhs)?;
        same_cmp_pair(&zero, &sign).then_some(zero)
    }

    fn match_signed_le(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolOr)?;
        self.match_signed_le_pair(&lhs, &rhs)
            .or_else(|| self.match_signed_le_pair(&rhs, &lhs))
    }

    fn match_signed_le_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<(Varnode, Varnode)> {
        let zero = self.match_cmp_zero_flag(lhs)?;
        let sign = self.match_cmp_sign_overflow_ne(rhs)?;
        same_cmp_pair(&zero, &sign).then_some(zero)
    }

    fn is_zero_valued_varnode(&self, vn: &Varnode) -> bool {
        self.peel_passthrough_varnode(vn).is_zero()
    }

    fn is_simple_branch_value(&self, vn: &Varnode) -> bool {
        let peeled = self.peel_passthrough_varnode(vn);
        peeled.is_constant || peeled.space_id == REGISTER_SPACE_ID
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum X86BranchPredicate {
    EqZero(Varnode),
    NeZero(Varnode),
    SLtZero(Varnode),
    SLeZero(Varnode),
    SGtZero(Varnode),
    SGeZero(Varnode),
    MaskEqZero { value: Varnode, mask: Varnode },
    MaskNeZero { value: Varnode, mask: Varnode },
    Eq(Varnode, Varnode),
    Ne(Varnode, Varnode),
    ULt(Varnode, Varnode),
    ULe(Varnode, Varnode),
    UGt(Varnode, Varnode),
    UGe(Varnode, Varnode),
    SLt(Varnode, Varnode),
    SLe(Varnode, Varnode),
    SGt(Varnode, Varnode),
    SGe(Varnode, Varnode),
}

fn bool_binary(op: HirBinaryOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
    HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

fn zero_like(expr: &HirExpr) -> HirExpr {
    HirExpr::Const(0, expr_type(expr))
}

fn same_cmp_pair(lhs: &(Varnode, Varnode), rhs: &(Varnode, Varnode)) -> bool {
    lhs.0 == rhs.0 && lhs.1 == rhs.1
}

fn decode_jump_table_target(
    bytes: &[u8],
    little_endian: bool,
    relative_entries: bool,
    target_base: Option<u64>,
) -> Option<u64> {
    if relative_entries {
        let base = i128::from(target_base?);
        let displacement = match bytes.len() {
            4 => {
                let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
                i128::from(if little_endian {
                    i32::from_le_bytes(raw)
                } else {
                    i32::from_be_bytes(raw)
                })
            }
            8 => {
                let raw = [
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ];
                i128::from(if little_endian {
                    i64::from_le_bytes(raw)
                } else {
                    i64::from_be_bytes(raw)
                })
            }
            _ => return None,
        };
        let target = base + displacement;
        return (0..=i128::from(u64::MAX))
            .contains(&target)
            .then_some(target as u64);
    }

    match bytes.len() {
        4 => {
            let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Some(if little_endian {
                u32::from_le_bytes(raw) as u64
            } else {
                u32::from_be_bytes(raw) as u64
            })
        }
        8 => {
            let raw = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Some(if little_endian {
                u64::from_le_bytes(raw)
            } else {
                u64::from_be_bytes(raw)
            })
        }
        _ => None,
    }
}

fn branchind_decode_modes(
    relative_entries: bool,
    table_base: u64,
    target_base: Option<u64>,
    image_base: u64,
) -> Vec<(&'static str, bool, Option<u64>)> {
    if relative_entries {
        return vec![(
            "relative_target_base",
            true,
            target_base.or(Some(table_base)),
        )];
    }
    let mut modes = vec![
        ("absolute", false, None),
        ("relative_table_base", true, Some(table_base)),
    ];
    if image_base != 0 {
        modes.push(("image_base_relative", true, Some(image_base)));
    }
    modes
}

fn extract_selector_upper_bound_from_cond(
    cond: &HirExpr,
    selector: &HirExpr,
    current_on_true: bool,
) -> Option<u64> {
    let cond = strip_casts(cond);
    if let HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr,
        ..
    } = cond
    {
        return extract_selector_upper_bound_from_cond(&expr, selector, !current_on_true);
    }

    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };

    let lhs = strip_casts(&lhs);
    let rhs = strip_casts(&rhs);
    let selector_match = |expr: &HirExpr| strip_casts(expr) == *selector;
    let const_u64 = |expr: &HirExpr| match strip_casts(expr) {
        HirExpr::Const(value, _) if value >= 0 => Some(value as u64),
        _ => None,
    };

    match (op, selector_match(&lhs), selector_match(&rhs)) {
        (HirBinaryOp::Eq, true, false) if current_on_true => const_u64(&rhs),
        (HirBinaryOp::Eq, false, true) if current_on_true => const_u64(&lhs),
        (HirBinaryOp::Ne, true, false) if !current_on_true => const_u64(&rhs),
        (HirBinaryOp::Ne, false, true) if !current_on_true => const_u64(&lhs),
        (HirBinaryOp::Le | HirBinaryOp::SLe, true, false) if current_on_true => const_u64(&rhs),
        (HirBinaryOp::Lt | HirBinaryOp::SLt, true, false) if current_on_true => {
            const_u64(&rhs)?.checked_sub(1)
        }
        (HirBinaryOp::Le | HirBinaryOp::SLe, false, true) if !current_on_true => {
            const_u64(&lhs)?.checked_sub(1)
        }
        (HirBinaryOp::Lt | HirBinaryOp::SLt, false, true) if !current_on_true => const_u64(&lhs),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::nir::render_mlil_preview;
    use crate::nir::support::{CallingConvention, RUST_SLEIGH_REGISTER_SPACE_ID};
    use crate::nir::types::{MlilPreviewOptions, StructuringEngineKind};
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

    use super::{
        InferredJumpTableTargets, branchind_decode_modes, merge_inferred_branchind_targets,
    };

    #[test]
    fn branchind_decode_modes_include_image_base_relative_for_absolute_tables() {
        let modes = branchind_decode_modes(false, 0x1400_5000, None, 0x1400_0000);
        assert!(modes.contains(&("absolute", false, None)));
        assert!(modes.contains(&("relative_table_base", true, Some(0x1400_5000))));
        assert!(modes.contains(&("image_base_relative", true, Some(0x1400_0000))));
    }

    #[test]
    fn branchind_decode_modes_keep_relative_tables_target_based() {
        let modes = branchind_decode_modes(true, 0x1400_5000, Some(0x1400_7000), 0x1400_0000);
        assert_eq!(
            modes,
            vec![("relative_target_base", true, Some(0x1400_7000))]
        );
    }

    #[test]
    fn merge_inferred_branchind_targets_preserves_case_map_with_successors() {
        let mut targets = vec![0x2000];
        let mut recovered_case_map = None;
        let mut recovered_selector_cardinality = None;
        merge_inferred_branchind_targets(
            &mut targets,
            InferredJumpTableTargets {
                unique_targets: vec![0x2000, 0x3000, 0x4000],
                recovered_cases: vec![(0, 0x2000), (1, 0x3000), (2, 0x4000), (3, 0x3000)],
                selector_cardinality: 4,
                decode_mode: "absolute",
            },
            &mut recovered_case_map,
            &mut recovered_selector_cardinality,
        );

        assert_eq!(targets, vec![0x2000, 0x3000, 0x4000]);
        assert_eq!(recovered_selector_cardinality, Some(4));
        assert_eq!(
            recovered_case_map,
            Some(vec![(0, 0x2000), (1, 0x3000), (2, 0x4000), (3, 0x3000)])
        );
    }

    #[test]
    fn return_recovery_keeps_return_register_before_side_effect_store() {
        let w0 = Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            offset: 0x4000,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let x8 = Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            offset: 0x4040,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let lr = Varnode {
            space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
            offset: 0x40f0,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let constant = |value, size| Varnode::constant(value, size);
        let pcode = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1000,
                        output: Some(w0.clone()),
                        inputs: vec![constant(7, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x1004,
                        output: None,
                        inputs: vec![constant(0, 4), x8, w0],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Return,
                        address: 0x1008,
                        output: None,
                        inputs: vec![lr],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };
        let options = MlilPreviewOptions {
            pe_x64_only: false,
            is_64bit: true,
            pointer_size: 8,
            format: "ELF64".to_string(),
            image_base: 0,
            sections: vec![(0x1000, 0x2000)],
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            relocation_names: Default::default(),
            calling_convention: CallingConvention::AArch64,
        };
        let code = render_mlil_preview(&pcode, "store_then_return", 0x1000, &options)
            .expect("preview render");

        assert!(
            code.lines()
                .any(|line| line.contains(" = 7;") && !line.contains("return")),
            "{code}"
        );
        assert!(code.contains("return 7;"), "{code}");
    }
}

fn is_safe_selector_provenance_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
    )
}

fn same_family_varnode(vn: &Varnode, selector_family: (u64, u64)) -> bool {
    (vn.space_id, vn.offset) == selector_family
}
