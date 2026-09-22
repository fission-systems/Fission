use super::*;

mod predicate_matching;
mod switch_recovery;

pub(super) use super::jump_table::{
    InferredJumpTableTargets, branchind_decode_modes, decode_jump_table_target,
    extract_selector_upper_bound_from_cond, is_safe_selector_provenance_opcode,
    merge_inferred_branchind_targets, same_family_varnode,
};

fn arm32_callable_target_expr(expr: &PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if matches!(
            &**rhs,
            PreHirExpr::Const(0xffff_fffe, _) | PreHirExpr::Const(-2, _)
        ) =>
        {
            (**lhs).clone()
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if matches!(
            &**lhs,
            PreHirExpr::Const(0xffff_fffe, _) | PreHirExpr::Const(-2, _)
        ) =>
        {
            (**rhs).clone()
        }
        _ => expr.clone(),
    }
}

fn is_arm32_callable_mask(value: i64) -> bool {
    value == 0xffff_fffe || value == -2
}

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn call_result_is_proven_non_source(
        &self,
        target: &str,
    ) -> bool {
        self.type_context
            .and_then(|context| context.call_result_is_source_value.get(target))
            .is_some_and(|is_source_value| !is_source_value)
    }

    fn final_call_result_is_proven_non_source(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> bool {
        let Some((call_idx, call)) =
            block
                .ops
                .iter()
                .enumerate()
                .take(term_idx)
                .rev()
                .find(|(idx, op)| {
                    matches!(
                        op.opcode,
                        PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                    ) && !self.call_is_return_target_artifact(block, *idx)
                })
        else {
            return false;
        };
        if block
            .ops
            .iter()
            .take(term_idx)
            .skip(call_idx + 1)
            .any(|op| {
                op.output.as_ref().is_some_and(|output| {
                    self.register_namer().is_primary_return_register(output)
                        && !self.is_return_target_copy(op, output)
                })
            })
        {
            return false;
        }
        super::super::resolve_lifted_direct_call_target(call, self.options, self.type_context)
            .is_some_and(|target| self.call_result_is_proven_non_source(&target))
    }

    fn recover_tail_call_expr_from_target_expr(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        target_expr: &PreHirExpr,
    ) -> Option<PreHirExpr> {
        let resolved_target = if let PreHirExpr::Var(target_name) = target_expr {
            self.resolve_address_like_call_target_name(target_name)
        } else {
            None
        };
        let target = resolved_target.or_else(|| {
            (self.options.calling_convention == CallingConvention::Arm32).then(|| {
                format!(
                    "((code *){})",
                    print_prehir_expr(&arm32_callable_target_expr(target_expr))
                )
            })
        })?;
        let args = if self.pcode.blocks.len() <= 2 {
            self.recover_tail_call_args(block_idx, block, term_idx)
        } else {
            Vec::new()
        };
        Some(PreHirExpr::Call {
            target,
            args,
            ty: NirType::Unknown,
        })
    }

    fn recover_tail_call_expr_from_callable_target_expr(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        target_expr: &PreHirExpr,
    ) -> PreHirExpr {
        // Prefer opaque CallInd-style rendering for register/param fps so the
        // printer emits a cast callable. Known symbols keep direct names.
        let resolved_symbol = if let PreHirExpr::Var(target_name) = target_expr {
            self.resolve_address_like_call_target_name(target_name)
        } else {
            None
        };
        // Always recover args from the BranchInd block (and single-pred chain);
        // multi-block diamonds still stage args in the arm that holds BranchInd
        // (x64 O2 apply_binop: 3 blocks, args live in the tail-call arm).
        let mut args = self.recover_tail_call_args(block_idx, block, term_idx);
        if let Some(target) = resolved_symbol {
            return PreHirExpr::Call {
                target,
                args,
                ty: NirType::Unknown,
            };
        }
        // Unresolved fp: opaque form — printer casts to callable.
        args.insert(0, target_expr.clone());
        PreHirExpr::Call {
            target: "__fission_callind_opaque".to_string(),
            args,
            ty: NirType::Unknown,
        }
    }

    fn recover_tail_call_expr_from_branchind_target(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        switch_var: &Varnode,
        switch_expr: &PreHirExpr,
    ) -> Option<PreHirExpr> {
        if let Some(target_expr) =
            self.recover_branchind_callable_target(block.index as usize, term_idx, switch_var)
        {
            return Some(self.recover_tail_call_expr_from_callable_target_expr(
                block_idx,
                block,
                term_idx,
                &target_expr,
            ));
        }
        self.recover_tail_call_expr_from_target_expr(block_idx, block, term_idx, switch_expr)
    }

    fn recover_branchind_callable_target(
        &mut self,
        block_idx: usize,
        term_idx: usize,
        switch_var: &Varnode,
    ) -> Option<PreHirExpr> {
        // x86/x64: `jmp rax` / `jmp r8` tail-calls through a register-held
        // function pointer (gcc -O2 apply_binop). ARM keeps the existing mask
        // and source recovery; x86 uses the same copy-chain → param/reg path.
        let allow_x86_fp_tail = matches!(
            self.options.calling_convention,
            CallingConvention::WindowsX64
                | CallingConvention::SystemVAmd64
                | CallingConvention::X86_32
        );
        let allow_arm = matches!(
            self.options.calling_convention,
            CallingConvention::AArch64 | CallingConvention::Arm32
        );
        if !allow_x86_fp_tail && !allow_arm {
            return None;
        }
        let (_, op) = self.lookup_def_site(switch_var)?;
        if self.options.calling_convention == CallingConvention::Arm32
            && op.opcode == PcodeOpcode::IntAnd
            && op.inputs.len() == 2
        {
            let lhs_mask = const_offset(&op.inputs[0]).is_some_and(is_arm32_callable_mask);
            let rhs_mask = const_offset(&op.inputs[1]).is_some_and(is_arm32_callable_mask);
            let source = match (lhs_mask, rhs_mask) {
                (true, false) => op.inputs[1].clone(),
                (false, true) => op.inputs[0].clone(),
                _ => switch_var.clone(),
            };
            if let Some(expr) =
                self.recover_branchind_callable_source_expr(block_idx, term_idx, &source, 0)
            {
                return Some(expr);
            }
            return self
                .lower_wrapped_varnode(&source, &mut HashSet::default())
                .ok();
        }
        if let Some(expr) =
            self.recover_branchind_callable_source_expr(block_idx, term_idx, switch_var, 0)
        {
            return Some(expr);
        }
        // x86: register target with no further param alias — still a callable fp.
        if allow_x86_fp_tail && is_register_varnode(switch_var) {
            return self
                .lower_wrapped_varnode(switch_var, &mut HashSet::default())
                .ok();
        }
        None
    }

    fn recover_branchind_callable_source_expr(
        &mut self,
        block_idx: usize,
        before_op_idx: usize,
        source: &Varnode,
        depth: usize,
    ) -> Option<PreHirExpr> {
        if depth > 8 {
            return None;
        }
        if let Some((site, op)) = self.lookup_def_site(source)
            && site.block_idx == block_idx
            && site.op_idx < before_op_idx
            && matches!(
                op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            )
            && let Some(input) = op.inputs.first().cloned()
            && is_register_varnode(&input)
        {
            if let Some(expr) = self.recover_branchind_callable_source_expr(
                block_idx,
                site.op_idx,
                &input,
                depth + 1,
            ) {
                return Some(expr);
            }
        }
        self.register_param(source).map(PreHirExpr::Var)
    }

    fn recover_known_external_tail_call_expr(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
        target_vn: &Varnode,
    ) -> Option<PreHirExpr> {
        let target_addr = branch_target_address(target_vn)?;
        if self.address_to_index.contains_key(&target_addr) {
            return None;
        }
        let resolved_target = self
            .type_context
            .and_then(|ctx| ctx.call_target_refs.get(&target_addr))
            .map(|target_ref| target_ref.symbol.clone())?;
        // The tail call's arguments are set up before the epilogue, in this
        // block or in the single predecessor that falls into it, and
        // `recover_tail_call_args` checks for exactly that. It used to run
        // only when the whole function had two blocks or fewer, which is not
        // a property of the call site at all: `__do_global_ctors` ends in
        // `jmp atexit` and has a loop, so it printed `return atexit()` with
        // the argument dropped.
        let args = self.recover_tail_call_args(block_idx, block, term_idx);
        Some(PreHirExpr::Call {
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
    ) -> Vec<PreHirExpr> {
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
                        self.register_namer().is_primary_return_register(output)
                            && !self.is_return_target_copy(op, output)
                    })
                    .map(|output| (op_idx, output.clone()))
            })
    }

    fn arm32_return_pair_def_after_barrier(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Option<((usize, Varnode), (usize, Varnode))> {
        if self.options.calling_convention != CallingConvention::Arm32 {
            return None;
        }
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
        let mut low = None;
        let mut high = None;
        for (op_idx, op) in block
            .ops
            .iter()
            .enumerate()
            .take(term_idx)
            .skip(start)
            .rev()
        {
            let Some(output) = op.output.as_ref() else {
                continue;
            };
            if !is_register_space_id(output.space_id) || output.size != 4 {
                continue;
            }
            match output.offset {
                0x20 if low.is_none() && !self.is_return_target_copy(op, output) => {
                    if self.arm32_return_pair_def_materializes_address(op) {
                        return None;
                    }
                    low = Some((op_idx, output.clone()));
                }
                0x24 if high.is_none() => {
                    if self.arm32_return_pair_def_materializes_address(op) {
                        return None;
                    }
                    high = Some((op_idx, output.clone()));
                }
                _ => {}
            }
            if low.is_some() && high.is_some() {
                break;
            }
        }
        Some((low?, high?))
    }

    fn arm32_return_pair_def_materializes_address(&self, op: &PcodeOp) -> bool {
        if self.options.relocation_names.contains_key(&op.address) {
            return true;
        }
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return false;
        }
        self.resolve_global_address(&op.inputs[1], 8)
            .is_some_and(|address| {
                self.options.relocation_names.contains_key(&address)
                    || self.options.global_names.contains_key(&address)
            })
    }

    fn compose_arm32_return_pair(&self, r0: PreHirExpr, r1: PreHirExpr) -> PreHirExpr {
        let (low, high) = if self.options.is_big_endian {
            (r1, r0)
        } else {
            (r0, r1)
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let shifted_high = PreHirExpr::Binary {
            op: PreHirBinaryOp::Shl,
            lhs: Box::new(PreHirExpr::Cast {
                ty: u64_ty.clone(),
                expr: Box::new(high),
            }),
            rhs: Box::new(PreHirExpr::Const(32, u64_ty.clone())),
            ty: u64_ty.clone(),
        };
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Or,
            lhs: Box::new(shifted_high),
            rhs: Box::new(PreHirExpr::Cast {
                ty: u64_ty.clone(),
                expr: Box::new(low),
            }),
            ty: u64_ty,
        }
    }

    fn arm32_return_pair_part_is_address_like(&self, expr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::PtrOffset { .. }
            | PreHirExpr::Index { .. } => true,
            PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
                self.arm32_return_pair_part_is_address_like(expr)
            }
            PreHirExpr::FieldAccess { base, ty, .. } => {
                matches!(ty, NirType::Ptr(_)) || self.arm32_return_pair_part_is_address_like(base)
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                self.arm32_return_pair_part_is_address_like(lhs)
                    || self.arm32_return_pair_part_is_address_like(rhs)
            }
            PreHirExpr::Load { ptr, ty } => {
                matches!(ty, NirType::Ptr(_)) || self.arm32_return_pair_part_is_address_like(ptr)
            }
            PreHirExpr::Call { ty, .. } => matches!(ty, NirType::Ptr(_)),
            PreHirExpr::AggregateCopy { src, .. } => {
                self.arm32_return_pair_part_is_address_like(src)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.arm32_return_pair_part_is_address_like(cond)
                    || self.arm32_return_pair_part_is_address_like(then_expr)
                    || self.arm32_return_pair_part_is_address_like(else_expr)
            }
            PreHirExpr::Var(name) => {
                self.options
                    .global_names
                    .values()
                    .any(|global| global == name)
                    || self
                        .options
                        .relocation_names
                        .values()
                        .any(|global| global == name)
            }
            PreHirExpr::Const(_, _) => false,
        }
    }

    fn lower_arm32_return_pair_expr_from_block(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        let Some(((_low_op_idx, low_vn), (_high_op_idx, high_vn))) =
            self.arm32_return_pair_def_after_barrier(block, term_idx)
        else {
            return Ok(None);
        };
        self.with_lowering_site(
            LoweringSite {
                block_idx,
                op_idx: term_idx,
            },
            |this| {
                let low = this.lower_wrapped_varnode(&low_vn, &mut HashSet::default())?;
                let high = this.lower_wrapped_varnode(&high_vn, &mut HashSet::default())?;
                if this.arm32_return_pair_part_is_address_like(&high) {
                    return Ok(None);
                }
                if this.arm32_return_pair_part_is_address_like(&low) {
                    return Ok(None);
                }
                Ok(Some(this.compose_arm32_return_pair(low, high)))
            },
        )
    }

    fn uses_primary_return_registers(&self) -> bool {
        self.options.is_64bit
            || (!self.options.is_64bit
                && self.options.pointer_size == 4
                && matches!(
                    self.options.calling_convention,
                    CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
                ))
            || matches!(
                self.options.calling_convention,
                CallingConvention::Arm32
                    | CallingConvention::X86_32
                    | CallingConvention::PowerPc32
                    | CallingConvention::LoongArch32
                    | CallingConvention::Mips32
            )
    }

    fn is_return_target_copy(&self, op: &PcodeOp, output: &Varnode) -> bool {
        op.opcode == PcodeOpcode::Copy
            && is_register_space_id(output.space_id)
            && output.offset == 0
            && op
                .inputs
                .first()
                .is_some_and(|input| self.register_namer().is_return_target_register(input))
    }

    fn lower_primary_return_expr_from_block(
        &mut self,
        block_idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if let Some(expr) =
            self.lower_arm32_return_pair_expr_from_block(block_idx, block, term_idx)?
        {
            return Ok(Some(expr));
        }
        let Some((ret_op_idx, ret_vn)) =
            self.last_primary_return_def_after_barrier(block, term_idx)
        else {
            return Ok(None);
        };
        let ret_vn = self
            .narrow_zero_extended_primary_return_source(block, ret_op_idx, &ret_vn)
            .unwrap_or(ret_vn);
        self.with_lowering_site(
            LoweringSite {
                block_idx,
                op_idx: term_idx,
            },
            |this| this.lower_wrapped_varnode(&ret_vn, &mut HashSet::default()),
        )
        .map(Some)
    }

    fn narrow_zero_extended_primary_return_source(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        ret_op_idx: usize,
        ret_vn: &Varnode,
    ) -> Option<Varnode> {
        if !self.options.is_64bit {
            return None;
        }
        let op = block.ops.get(ret_op_idx)?;
        if op.opcode != PcodeOpcode::IntZExt || op.output.as_ref() != Some(ret_vn) {
            return None;
        }
        let input = op.inputs.first()?;
        if input.size >= ret_vn.size || !is_register_space_id(input.space_id) {
            return None;
        }
        self.register_namer()
            .is_primary_return_register(input)
            .then_some(input.clone())
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

    /// x86-32 shared RET block after a diamond: epilogue may restore the frame
    /// (pop ebp / leave) without defining the primary return register. The
    /// return value lives in predecessor arms (mov eax, imm / mov eax, reg).
    fn is_epilogue_style_return_join_block(&self, idx: usize) -> bool {
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
        // Join must not redefine the primary return register — value comes from preds.
        if self
            .last_primary_return_def_after_barrier(block, term_idx)
            .is_some()
        {
            return false;
        }
        // At least two predecessors (diamond / multi-arm join).
        let pred_count = self
            .predecessors
            .get(idx)
            .map(|p| p.iter().filter(|p| **p != idx).count())
            .unwrap_or(0);
        if pred_count < 2 {
            return false;
        }
        // Ops before RET must not store through the return register (real work).
        !self.side_effect_consumes_primary_return_register_before(block, term_idx)
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
        loop {
            let op = block.ops.get(cursor_idx)?;
            let output = op.output.as_ref()?;
            if !self.varnode_aliases_value(output, &cursor_vn)
                || op.inputs.len() != 1
                || !matches!(
                    op.opcode,
                    PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
                )
            {
                return None;
            }
            let input = op.inputs.first()?;
            if !is_register_space_id(input.space_id) {
                return None;
            }
            if !self.register_namer().is_primary_return_register(input) {
                return Some(input.clone());
            }

            // Continue through a primary-return-register alias chain (for
            // example RAX <- ZExt(EAX)) by finding the preceding definition of
            // the narrower register. The current primary-return definition must
            // be examined too: a direct `RAX <- RDX` copy is the common return
            // join shape and was previously skipped by starting at `cursor_idx`.
            let (prior_idx, _) =
                block
                    .ops
                    .iter()
                    .enumerate()
                    .take(cursor_idx)
                    .rev()
                    .find(|(_, candidate)| {
                        candidate.output.as_ref().is_some_and(|candidate_output| {
                            self.varnode_aliases_value(candidate_output, input)
                        })
                    })?;
            cursor_idx = prior_idx;
            cursor_vn = input.clone();
        }
    }

    pub(in crate::midend::builder) fn return_join_has_primary_return_evidence(
        &self,
        return_idx: usize,
    ) -> bool {
        self.predecessors.get(return_idx).is_some_and(|preds| {
            preds.iter().any(|pred| {
                *pred != return_idx && self.block_has_primary_return_def_before_terminator(*pred)
            })
        })
    }

    /// True when any p-code block defines the ABI primary return register
    /// before its terminator (including loop-carried index updates). Used to
    /// distinguish void-like functions from value-returning ones when RET's
    /// p-code input is only the return address on the stack.
    pub(in crate::midend::builder) fn function_has_primary_return_def(&self) -> bool {
        (0..self.pcode.blocks.len())
            .any(|block_idx| self.block_has_primary_return_def_before_terminator(block_idx))
    }

    fn side_effect_consumes_primary_return_register_before(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> bool {
        let ret_regs = self.register_namer().primary_return_registers();
        if ret_regs.is_empty() {
            return false;
        }
        block.ops.iter().take(term_idx).any(|op| {
            matches!(op.opcode, PcodeOpcode::Store)
                && op.inputs.iter().skip(1).any(|input| {
                    ret_regs.iter().any(|ret_reg| {
                        self.varnode_aliases_value(ret_reg, input)
                            || self.varnode_aliases_value(input, ret_reg)
                    })
                })
        })
    }

    fn side_effect_consumes_exact_primary_return_register_before(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> bool {
        let ret_regs = self.register_namer().primary_return_registers();
        if ret_regs.is_empty() {
            return false;
        }
        block.ops.iter().take(term_idx).any(|op| {
            matches!(op.opcode, PcodeOpcode::Store)
                && op
                    .inputs
                    .iter()
                    .skip(1)
                    .any(|input| ret_regs.iter().any(|ret_reg| input == ret_reg))
        })
    }

    fn primary_return_value_flows_to_later_store(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        ret_op_idx: usize,
        term_idx: usize,
        ret_vn: &Varnode,
    ) -> bool {
        block
            .ops
            .iter()
            .enumerate()
            .take(term_idx)
            .skip(ret_op_idx + 1)
            .any(|(op_idx, op)| {
                matches!(op.opcode, PcodeOpcode::Store)
                    && op.inputs.iter().skip(1).any(|input| {
                        self.varnode_derives_from_varnode_before(block, op_idx, input, ret_vn, 0)
                    })
            })
    }

    fn varnode_derives_from_varnode_before(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        value: &Varnode,
        source: &Varnode,
        depth: usize,
    ) -> bool {
        if depth > 6 {
            return false;
        }
        if self.varnode_aliases_value(source, value) || self.varnode_aliases_value(value, source) {
            return true;
        }
        let Some((_def_idx, def)) =
            block
                .ops
                .iter()
                .enumerate()
                .take(before_idx)
                .rev()
                .find(|(_, op)| {
                    op.output
                        .as_ref()
                        .is_some_and(|output| self.varnode_aliases_value(output, value))
                })
        else {
            return false;
        };
        if !matches!(
            def.opcode,
            PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
        ) {
            return false;
        }
        def.inputs.first().is_some_and(|input| {
            self.varnode_derives_from_varnode_before(block, before_idx, input, source, depth + 1)
        })
    }

    pub(in crate::midend) fn lower_return_join_expr_for_predecessor(
        &mut self,
        pred_idx: usize,
        return_idx: usize,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
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
                    |this| this.lower_wrapped_varnode(&source_vn, &mut HashSet::default()),
                )
                .map(Some);
        }
        // Multi-pred return join: predecessors define the primary return register
        // (EAX on x86-32 / RAX on x64) and a shared exit block ends in RET.
        //
        // - 64-bit: keep the pure-join requirement (existing behavior).
        // - 32-bit: pure join OR epilogue-style join (pop/leave noise). Do not open
        //   join recovery for arbitrary impure exits — that eats natural while
        //   loops (count_bits) by treating the loop exit as a return diamond.
        if !self.return_join_has_primary_return_evidence(return_idx) {
            return Ok(None);
        }
        if self.options.is_64bit {
            // A shared x64 RET may contain epilogue/return-target mechanics
            // while leaving the primary return register untouched. Accept
            // that non-pure join only for an edge whose predecessor itself
            // defines the ABI return register; this preserves direct
            // value-bearing loop exits without opening arbitrary impure loop
            // exits as returns.
            if !self.is_pure_return_join_block(return_idx)
                && (!self.is_epilogue_style_return_join_block(return_idx)
                    || !self.block_has_primary_return_def_before_terminator(pred_idx))
            {
                return Ok(None);
            }
        } else if !self.is_pure_return_join_block(return_idx)
            && !self.is_epilogue_style_return_join_block(return_idx)
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
        let return_pcode_idx = self.pcode_block_idx(return_idx);
        if let Some(return_block) = self.pcode.blocks.get(return_pcode_idx)
            && let Some(return_term_idx) = self.block_terminator_index(return_block)
            && self
                .side_effect_consumes_primary_return_register_before(return_block, return_term_idx)
            && self
                .last_primary_return_def_after_barrier(return_block, return_term_idx)
                .is_none()
        {
            return Ok(None);
        }
        if let Some(expr) =
            self.lower_primary_return_expr_from_block(pred_pcode_idx, &pred_block, pred_term_idx)?
        {
            return Ok(Some(expr));
        }
        let Some(ret_vn) = self
            .register_namer()
            .primary_return_registers()
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
            |this| this.lower_wrapped_varnode(&ret_vn, &mut HashSet::default()),
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
        if self.register_namer().is_primary_return_register(merge_vn) {
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
                if !self.register_namer().is_primary_return_register(output) {
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
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        let Some(preds) = self.predecessors.get(return_idx) else {
            return Ok(None);
        };
        let mut recovered: Vec<PreHirExpr> = Vec::new();
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
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        self.lower_return_terminator_impl(idx, block, term_idx)
    }

    fn lower_return_terminator_impl(
        &mut self,
        idx: usize,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if self.uses_primary_return_registers()
            && let Some(expr) =
                self.lower_arm32_return_pair_expr_from_block(idx, block, term_idx)?
        {
            return Ok(Some(expr));
        }
        if self.uses_primary_return_registers()
            && let Some((ret_op_idx, ret_vn)) =
                self.last_primary_return_def_after_barrier(block, term_idx)
        {
            if matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            ) && self
                .primary_return_value_flows_to_later_store(block, ret_op_idx, term_idx, &ret_vn)
            {
                return Ok(None);
            }
            let ret_vn = self
                .narrow_zero_extended_primary_return_source(block, ret_op_idx, &ret_vn)
                .unwrap_or(ret_vn);
            // x86 cmov: last def of EAX may be a guarded Copy inside a same-block
            // CBranch skip. Inlining that Copy's RHS always is wrong (it only runs
            // on the taken path). Prefer the live register *name* so prior
            // default + guarded overrides compose as:
            //   eax = hi; if (le) eax = value; if (lt) eax = lo; return eax;
            if self.primary_return_def_is_same_block_cmov_body(block, ret_op_idx)
                && let Some(expr) =
                    self.cmov_live_primary_return_register_var(block, &ret_vn, ret_op_idx)
            {
                if preview_builder_diag_enabled() {
                    eprintln!(
                        "[DIAG] return recovery: block={} path=cmov_live_register op_idx={} expr={:?}",
                        idx, ret_op_idx, expr
                    );
                }
                return Ok(Some(expr));
            }
            let expr = self
                .with_lowering_site(
                    LoweringSite {
                        block_idx: idx,
                        op_idx: term_idx,
                    },
                    |this| this.lower_wrapped_varnode(&ret_vn, &mut HashSet::default()),
                )
                .map(Some)?;
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] return recovery: block={} path=local_primary_def op_idx={} source={:?} expr={:?}",
                    idx, ret_op_idx, ret_vn, expr
                );
            }
            return Ok(expr);
        }
        // A declared-void call still clobbers the ABI result slot. With no
        // definition of that slot after the call, an earlier scratch value in
        // the same register cannot be the function's source return value.
        if self.uses_primary_return_registers()
            && self.final_call_result_is_proven_non_source(block, term_idx)
        {
            return Ok(None);
        }
        if self.uses_primary_return_registers()
            && (!self.side_effect_consumes_primary_return_register_before(block, term_idx)
                || self.side_effect_consumes_exact_primary_return_register_before(block, term_idx))
            && let Some(expr) = self.predecessor_primary_return_expr(idx)?
        {
            return Ok(Some(expr));
        }
        // Epilogue-style multi-pred RET (e.g. x86-32 pop/leave + ret): Return's
        // p-code input is the return address, not the ABI return register.
        // Predecessor arms may write *different* values into the primary return
        // register (sum vs INT_MIN cmov). Do not require equal lowered pred
        // exprs — emit the live primary return binding at the join.
        if self.uses_primary_return_registers() && self.is_epilogue_style_return_join_block(idx) {
            if let Some(expr) = self.live_primary_return_register_expr(block, term_idx)? {
                if preview_builder_diag_enabled() {
                    eprintln!(
                        "[DIAG] return recovery: block={} path=epilogue_join_live_primary expr={:?}",
                        idx, expr
                    );
                }
                return Ok(Some(expr));
            }
        }
        if self.uses_primary_return_registers()
            && !self.side_effect_consumes_primary_return_register_before(block, term_idx)
            && self
                .predecessors
                .get(idx)
                .is_some_and(|preds| !preds.is_empty())
            && let Some(input) = block.ops[term_idx].inputs.last()
            && self.return_input_is_control_target(input)
        {
            // Stack/return-address RET input is control-only. Prefer the live
            // ABI primary return register when this function ever defines that
            // register (loop index / found-path early RET). Only emit bare
            // `return` when there is no function-wide primary-return evidence
            // (void-like). Previously `!return_join_has_primary_return_evidence`
            // alone forced bare return on match-path blocks whose predecessors
            // end in CBranch without a local EAX/RAX def, dropping a live
            // loop-carried return index (`linear_search`-class).
            if self.return_input_is_stack_target(input)
                && !self.return_join_has_primary_return_evidence(idx)
                && !self.block_has_primary_return_def_before_terminator(idx)
                && !self.function_has_primary_return_def()
            {
                return Ok(None);
            }
            return self.live_primary_return_register_expr(block, term_idx);
        }
        if self.uses_primary_return_registers()
            && let Some(input) = block.ops[term_idx].inputs.last()
            && !self.return_input_is_control_target(input)
        {
            return self
                .lower_wrapped_varnode(input, &mut HashSet::default())
                .map(Some);
        }
        // Last resort: control-target RET with no pred map still recovers the
        // live ABI return register when the function wrote it -- but only if
        // that write wasn't already consumed by an intervening side effect
        // (e.g. stored to a global) before this RET, same guard the more
        // specific branch above respects. Without this, a function that
        // writes the primary return register for an unrelated reason (a
        // value later stored elsewhere, not actually returned) then returns
        // void via the link register got a return value synthesized for it
        // that was never live (`aarch64_return_join_with_terminal_store_
        // does_not_synthesize_live_return`-class).
        if self.uses_primary_return_registers()
            && self.function_has_primary_return_def()
            && !self.side_effect_consumes_primary_return_register_before(block, term_idx)
            && let Some(expr) = self.live_primary_return_register_expr(block, term_idx)?
        {
            if preview_builder_diag_enabled() {
                eprintln!(
                    "[DIAG] return recovery: block={} path=function_wide_live_primary expr={:?}",
                    idx, expr
                );
            }
            return Ok(Some(expr));
        }
        if self.uses_primary_return_registers()
            && self
                .telemetry
                .indirect_control
                .unsupported_indirect_control_count
                == 0
        {
            return Ok(None);
        }

        let op = &block.ops[term_idx];
        op.inputs
            .last()
            .map(|input| self.lower_wrapped_varnode(input, &mut HashSet::default()))
            .transpose()
    }

    fn live_primary_return_register_expr(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        term_idx: usize,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        let Some(ret_vn) = self
            .register_namer()
            .primary_return_registers()
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let Some(block_idx) = self.address_to_index.get(&block.start_address).copied() else {
            return Ok(None);
        };
        self.with_lowering_site(
            LoweringSite {
                block_idx,
                op_idx: term_idx,
            },
            |this| this.lower_wrapped_varnode(&ret_vn, &mut HashSet::default()),
        )
        .map(Some)
    }

    /// True when `ret_op_idx` is a Copy that a same-block CBranch can skip
    /// (SLEIGH cmov body). Absolute or relative forward targets both count.
    fn primary_return_def_is_same_block_cmov_body(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        ret_op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(ret_op_idx) else {
            return false;
        };
        if op.opcode != PcodeOpcode::Copy || op.output.is_none() {
            return false;
        }
        // Walk micro-ops of the same machine instruction for a guarding CBranch.
        for i in (0..ret_op_idx).rev() {
            let cand = &block.ops[i];
            if cand.address != op.address {
                break;
            }
            if cand.opcode != PcodeOpcode::CBranch || cand.inputs.is_empty() {
                continue;
            }
            if let Some(target_op_idx) = crate::midend::cfg::same_block_forward_branch_target_op_idx(
                block,
                i,
                block.ops.len(),
                cand,
                &cand.inputs[0],
            ) {
                if target_op_idx > ret_op_idx {
                    return true;
                }
            }
        }
        // Adjacent pattern: CBranch immediately before the Copy.
        if ret_op_idx > 0 {
            let prev = &block.ops[ret_op_idx - 1];
            if prev.opcode == PcodeOpcode::CBranch && !prev.inputs.is_empty() {
                if let Some(target_op_idx) =
                    crate::midend::cfg::same_block_forward_branch_target_op_idx(
                        block,
                        ret_op_idx - 1,
                        block.ops.len(),
                        prev,
                        &prev.inputs[0],
                    )
                {
                    return target_op_idx > ret_op_idx;
                }
            }
        }
        false
    }

    /// Prefer a stable register/temp binding for a cmov-updated return register
    /// without inlining the last guarded Copy RHS.
    fn cmov_live_primary_return_register_var(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        ret_vn: &Varnode,
        ret_op_idx: usize,
    ) -> Option<PreHirExpr> {
        // Prefer the name chosen when the guarded Copy was materialized.
        if let Some(op) = block.ops.get(ret_op_idx)
            && let Some(output) = op.output.as_ref()
        {
            let key = MaterializedVarnodeKey::new(output, op);
            if let Some(name) = self.materialized_vns.get(&key) {
                return Some(PreHirExpr::Var(name.clone()));
            }
        }
        // Fall back to the hardware return-register name.
        if let Some(name) = self.sla_hw_name(ret_vn.offset, ret_vn.size) {
            return Some(PreHirExpr::Var(name));
        }
        None
    }

    fn return_input_is_control_target(&self, input: &Varnode) -> bool {
        self.return_input_derives_from_control_target(input, 0)
    }

    fn return_input_derives_from_control_target(&self, input: &Varnode, depth: usize) -> bool {
        if depth > 6 {
            return false;
        }
        if self.return_input_is_stack_target(input) {
            return true;
        }
        if self.register_namer().is_return_target_register(input) {
            return true;
        }
        self.lookup_def_site(input).is_some_and(|(_, op)| {
            op.output.as_ref().is_some_and(|output| {
                output == input && self.op_derives_return_target(op, depth + 1)
            })
        })
    }

    fn op_derives_return_target(&self, op: &PcodeOp, depth: usize) -> bool {
        match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                op.inputs.first().is_some_and(|input| {
                    self.return_input_derives_from_control_target(input, depth)
                })
            }
            PcodeOpcode::IntAnd => {
                let [lhs, rhs] = op.inputs.as_slice() else {
                    return false;
                };
                (const_offset(lhs).is_some_and(is_arm32_callable_mask)
                    && self.return_input_derives_from_control_target(rhs, depth))
                    || (const_offset(rhs).is_some_and(is_arm32_callable_mask)
                        && self.return_input_derives_from_control_target(lhs, depth))
            }
            _ => false,
        }
    }

    pub(in crate::midend::builder) fn call_is_return_target_artifact(
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

    pub(in crate::midend::builder) fn call_is_terminal_branchind_artifact(
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

    pub(in crate::midend::builder) fn op_is_terminal_branchind_target_artifact(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if !matches!(
            op.opcode,
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
        ) {
            return false;
        }
        let Some(output) = op.output.as_ref() else {
            return false;
        };
        let Some(term_idx) = self.block_terminator_index(block) else {
            return false;
        };
        term_idx > op_idx
            && block.ops[term_idx].opcode == PcodeOpcode::BranchInd
            && block.ops[term_idx]
                .inputs
                .first()
                .is_some_and(|input| input == output)
    }

    fn return_input_is_stack_target(&self, input: &Varnode) -> bool {
        let Some((_, op)) = self.lookup_def_site(input) else {
            return false;
        };
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return false;
        }
        self.stack_pointer_register_name(&op.inputs[1])
            .is_some_and(|name| matches!(name.as_str(), "rsp" | "esp" | "sp"))
    }

    pub(in crate::midend) fn try_lower_intra_instruction_conditional_return(
        &mut self,
    ) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
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
            |this| this.lower_wrapped_varnode(&branch_op.inputs[1], &mut HashSet::default()),
        )?;
        let default_expr = self.with_lowering_site(
            LoweringSite {
                block_idx: branch_block_idx,
                op_idx: default_op_idx,
            },
            |this| this.lower_def_op(&branch_block.ops[default_op_idx], &mut HashSet::default()),
        )?;
        let alt_expr = self.with_lowering_site(
            LoweringSite {
                block_idx: copy_idx,
                op_idx: 0,
            },
            |this| this.lower_def_op(&copy_block.ops[0], &mut HashSet::default()),
        )?;

        Ok(Some(vec![
            PreHirStmt::If {
                cond,
                then_body: vec![PreHirStmt::Return(Some(default_expr))].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(alt_expr)),
        ]))
    }

    pub(in crate::midend) fn try_lower_conditional_tailcall_after_return(
        &mut self,
    ) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
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
                tail_body.push(PreHirStmt::Goto(block_label(target)));
            }
            LoweredTerminator::Fallthrough(None) | LoweredTerminator::Return(None) => {}
            _ => return Ok(None),
        }
        tail_body.push(PreHirStmt::Return(None));

        let return_cond = if return_on_true {
            cond
        } else {
            PreHirExpr::Unary {
                op: PreHirUnaryOp::Not,
                expr: Box::new(cond),
                ty: NirType::Bool,
            }
        };
        Ok(Some(vec![
            PreHirStmt::If {
                cond: return_cond,
                then_body: vec![PreHirStmt::Return(None)].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Block(tail_body.into()),
        ]))
    }

    pub(in crate::midend) fn lower_block_terminator(
        &mut self,
        idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError> {
        if let Some(cached) = self.terminator_cache.get(&idx) {
            return Ok(cached.clone());
        }

        let pcode_idx = self.pcode_block_idx(idx);
        let block = &self.pcode.blocks[pcode_idx];
        let lowered = if crate::midend::cfg::block_ends_in_proven_noreturn_call(
            block,
            self.options,
            self.type_context,
        ) {
            LoweredTerminator::Return(None)
        } else if let Some(term_idx) = self.block_terminator_index(block) {
            let op = &block.ops[term_idx];
            self.with_lowering_site(
                LoweringSite {
                    block_idx: pcode_idx,
                    op_idx: term_idx,
                },
                |this| {
                    let mut visiting = HashSet::default();
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
                                    target_expr: Some(print_prehir_expr(&tail_call_expr)),
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
                                    .lower_wrapped_varnode(target_vn, &mut HashSet::default())
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
                                                .map(print_prehir_expr),
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
                            if this.options.is_data_ref_origin {
                                return Err(MlilPreviewError::NotAFunctionOrphanBlock);
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
                                        .lower_wrapped_varnode(target_vn, &mut HashSet::default())
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
                                        if this.options.is_data_ref_origin {
                                            return Err(MlilPreviewError::NotAFunctionOrphanBlock);
                                        }
                                        return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                                    }
                                } else {
                                    if this.options.is_data_ref_origin {
                                        return Err(MlilPreviewError::NotAFunctionOrphanBlock);
                                    }
                                    return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                                }
                            };
                            let recovered_cond = this
                                .try_recover_branch_condition(&op.inputs[1])?
                                .filter(|expr| !Self::branch_cond_too_complex(expr));
                            let cond = recovered_cond
                                .map(Ok)
                                .unwrap_or_else(|| {
                                    this.lower_wrapped_varnode(&op.inputs[1], &mut HashSet::default())
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
                            let false_target = {
                                let mut f_target = this.next_block_address(idx);
                                if let Some(succs) = this.successors.get(idx) {
                                    for succ_idx in succs {
                                        let succ_addr = this.block_target_key(*succ_idx);
                                        if succ_addr != true_target {
                                            f_target = Some(succ_addr);
                                            break;
                                        }
                                    }
                                }
                                f_target
                            };
                            Ok(LoweredTerminator::Cond {
                                cond,
                                true_target,
                                false_target,
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
                                    print_prehir_expr(&switch_expr)
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
                            } else if let Some(recovered_targets) = this
                                .emulate_branchind_targets_with_emulator(
                                    idx,
                                    op,
                                    switch_var,
                                )
                            {
                                merge_inferred_branchind_targets(
                                    &mut targets,
                                    recovered_targets,
                                    &mut recovered_case_map,
                                    &mut recovered_selector_cardinality,
                                );
                            }
                            let mut recovered_missing_target_selector =
                                super::switch_table::recover_switch_discriminant(
                                    &switch_expr,
                                    &this.options,
                                );
                            if recovered_missing_target_selector.is_none()
                                && let Some(recovered_expr) = this
                                    .recover_branchind_switch_expr_from_predecessors(
                                        idx,
                                        switch_var,
                                        &mut visiting,
                                    )
                            {
                                recovered_missing_target_selector =
                                    super::switch_table::recover_switch_discriminant(
                                        &recovered_expr,
                                        &this.options,
                                    );
                            }
                            if targets.is_empty()
                                && let Some(recovered_selector) =
                                    recovered_missing_target_selector.clone()
                                && let Some(ret_expr) = this
                                    .live_primary_return_register_expr(block, term_idx)?
                                    .or_else(|| {
                                        this.predecessor_primary_return_expr(idx).ok().flatten()
                                    })
                                    .or(Some(recovered_selector.discriminant))
                            {
                                this.record_unsupported_inventory_event(
                                    "terminator_branchind_jump_table_targets_missing",
                                    Some(switch_var),
                                    Some(op),
                                    Some(op.opcode),
                                    Some(block.start_address),
                                    Some(u64::from(op.seq_num)),
                                    true,
                                    "jump_table_targets_missing_return_register_fallback",
                                );
                                return Ok(LoweredTerminator::Return(Some(ret_expr)));
                            }
                            if targets.is_empty()
                                && this.options.calling_convention == CallingConvention::Arm32
                                && this
                                    .predecessors
                                    .get(idx)
                                    .is_some_and(|preds| !preds.is_empty())
                                && matches!(switch_expr, PreHirExpr::Var(_))
                            {
                                this.record_unsupported_inventory_event(
                                    "terminator_branchind_arm32_targets_missing",
                                    Some(switch_var),
                                    Some(op),
                                    Some(op.opcode),
                                    Some(block.start_address),
                                    Some(u64::from(op.seq_num)),
                                    true,
                                    "arm32_branchind_targets_missing_value_fallback",
                                );
                                return Ok(LoweredTerminator::Return(Some(PreHirExpr::Const(
                                    0,
                                    NirType::Int {
                                        bits: 32,
                                        signed: false,
                                    },
                                ))));
                            }
                            if targets.is_empty() {
                                let tail_call_expr = this
                                    .recover_tail_call_expr_from_branchind_target(
                                        idx,
                                        block,
                                        term_idx,
                                        switch_var,
                                        &switch_expr,
                                    );
                                if tail_call_expr.is_some() {
                                    this.record_unsupported_inventory_event(
                                        "terminator_branchind_tail_call",
                                        Some(switch_var),
                                        Some(op),
                                        Some(op.opcode),
                                        Some(block.start_address),
                                        Some(u64::from(op.seq_num)),
                                        true,
                                        "branchind_tail_call_recovered",
                                    );
                                    let evidence = this.build_unsupported_control_evidence(
                                        op.opcode,
                                        Some(block.start_address),
                                        tail_call_expr.as_ref(),
                                        Vec::new(),
                                        UnsupportedControlFamily::MissingTargets,
                                        IndirectControlSurface::BranchInd,
                                        32,
                                    );
                                    return Ok(LoweredTerminator::Unsupported {
                                        evidence,
                                        target_expr: tail_call_expr,
                                    });
                                }
                            }
                            if targets.is_empty()
                                && let Some(inferred_target) =
                                    this.infer_branchind_target_from_input(idx, op, switch_var)
                            {
                                inferred_single_input_target = true;
                                targets.push(inferred_target);
                            }
                            if targets.is_empty() {
                                let tail_call_expr = this
                                    .recover_tail_call_expr_from_branchind_target(
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
                                    this.telemetry.indirect_control.indirect_target_set_refined_count += 1;
                                    this.telemetry.indirect_control.dispatcher_shape_recovered_count += 1;
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
                                let (expr, min_val) =
                                    recovered_selector
                                        .as_ref()
                                        .map(|selector| {
                                            let render_expr =
                                                if Self::selector_expr_is_side_effect_free(
                                                    &selector.discriminant,
                                                ) {
                                                    selector.discriminant.clone()
                                                } else {
                                                    selector_alias
                                                        .as_ref()
                                                        .map(|alias| {
                                                            this.recover_branchind_render_selector_expr(
                                                                idx,
                                                                alias,
                                                                selector.discriminant.clone(),
                                                                &mut visiting,
                                                            )
                                                        })
                                                        .unwrap_or_else(|| {
                                                            selector.discriminant.clone()
                                                        })
                                                };
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
                                    selector_expr: print_prehir_expr(&expr),
                                    rendered_selector_expr: Some(print_prehir_expr(&expr)),
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
                                this.telemetry.dispatcher.dispatcher_proof_unit_count += 1;
                                if proof_complete {
                                    this.telemetry.dispatcher.dispatcher_proof_completed_count += 1;
                                } else {
                                    this.telemetry.dispatcher.dispatcher_proof_failed_count += 1;
                                }
                                this.telemetry.indirect_control.indirect_target_set_refined_count += 1;
                                if dispatcher_recovered {
                                    this.telemetry.indirect_control.dispatcher_shape_recovered_count += 1;
                                }
                                if target_cardinality == 0 || single_target_dispatcher {
                                    let evidence = UnsupportedControlEvidence {
                                        opcode: format!("{:?}", op.opcode),
                                        source_block: Some(block.start_address),
                                        target_expr: Some(print_prehir_expr(&expr)),
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

    pub(in crate::midend::builder) fn lower_cbranch_condition_for_block(
        &mut self,
        idx: usize,
    ) -> Option<(u64, PreHirExpr)> {
        if let Some(cached) = self.terminator_cache.get(&idx) {
            if let LoweredTerminator::Cond {
                cond, true_target, ..
            } = cached
            {
                return Some((*true_target, cond.clone()));
            }
        }
        let block = self.pcode.blocks.get(idx)?;
        let term_idx = self.block_terminator_index(block)?;
        let op = block.ops.get(term_idx)?;
        if op.opcode != PcodeOpcode::CBranch || op.inputs.len() < 2 {
            return None;
        }
        let true_target = self
            .resolve_branch_target_index_with_recovery(idx, op, &op.inputs[0])
            .map(|target_idx| self.block_target_key(target_idx))
            .or_else(|| self.infer_cbranch_true_target_from_successors(idx))?;
        let cond_input = op.inputs[1].clone();
        let cond = self
            .with_lowering_site(
                LoweringSite {
                    block_idx: idx,
                    op_idx: term_idx,
                },
                |this| {
                    let recovered = this
                        .try_recover_branch_condition(&cond_input)?
                        .filter(|expr| !Self::branch_cond_too_complex(expr));
                    recovered.map(Ok).unwrap_or_else(|| {
                        this.lower_wrapped_varnode(&cond_input, &mut HashSet::default())
                    })
                },
            )
            .ok()?;
        Some((true_target, cond))
    }

    fn try_recover_branch_condition(
        &mut self,
        vn: &Varnode,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        if self.options.is_64bit
            && !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
        {
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
    ) -> Result<PreHirExpr, MlilPreviewError> {
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

    fn branch_cond_too_complex(expr: &PreHirExpr) -> bool {
        Self::expr_contains_call(expr) || Self::expr_node_count(expr) > 24
    }

    fn expr_contains_call(expr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Call { .. } => true,
            PreHirExpr::Const(_, _)
            | PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_) => false,
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => Self::expr_contains_call(expr),
            PreHirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_contains_call(lhs) || Self::expr_contains_call(rhs)
            }
            PreHirExpr::Index { base, index, .. } => {
                Self::expr_contains_call(base) || Self::expr_contains_call(index)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_contains_call(cond)
                    || Self::expr_contains_call(then_expr)
                    || Self::expr_contains_call(else_expr)
            }
        }
    }

    fn expr_node_count(expr: &PreHirExpr) -> usize {
        match expr {
            PreHirExpr::Const(_, _)
            | PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_) => 1,
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => 1 + Self::expr_node_count(expr),
            PreHirExpr::Binary { lhs, rhs, .. } => {
                1 + Self::expr_node_count(lhs) + Self::expr_node_count(rhs)
            }
            PreHirExpr::Call { args, .. } => {
                1 + args.iter().map(Self::expr_node_count).sum::<usize>()
            }
            PreHirExpr::Index { base, index, .. } => {
                1 + Self::expr_node_count(base) + Self::expr_node_count(index)
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                1 + Self::expr_node_count(cond)
                    + Self::expr_node_count(then_expr)
                    + Self::expr_node_count(else_expr)
            }
        }
    }

    /// Lower a register used as a `test`/`cmp`-class flag input, freezing
    /// Copy/Cast/ZExt sources at their defining site.
    ///
    /// Without this, `mov rax, rcx; mov ecx, edx; test rax, rax` peels RAX→RCX
    /// and then reads the **redefined** RCX (param_2) instead of the snapshot
    /// captured into RAX (param_1).
    fn lower_flag_tested_value(
        &mut self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        if is_register_varnode(vn)
            && let Some((site, op)) = self.lookup_def_site(vn)
            && matches!(
                op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
            )
            && op.inputs.len() == 1
        {
            // A register copy is a value snapshot.  Lowering its source at the
            // defining site is only sound while that source name remains live;
            // x86 commonly overwrites the source register before the later
            // `test`/`cmp` that consumes the destination.  Prefer the binding
            // established for the destination definition so the predicate reads
            // the copied value rather than a reused hardware-register name.
            if let Some(output) = op.output.as_ref()
                && VarnodeKey::from(output) == VarnodeKey::from(vn)
            {
                let key = MaterializedVarnodeKey::new(output, op);
                if let Some(name) = self.materialized_vns.get(&key) {
                    return Ok(PreHirExpr::Var(name.clone()));
                }
                if let Some(name) = self.sla_hw_name(output.offset, output.size) {
                    let name = self.ensure_live_register_binding(&name, output.size);
                    return Ok(PreHirExpr::Var(name));
                }
            }
            let src = op.inputs[0].clone();
            return self
                .with_lowering_site(site, |this| this.lower_wrapped_varnode(&src, visiting));
        }
        self.lower_wrapped_varnode(vn, visiting)
    }

    fn lower_x86_branch_predicate(
        &mut self,
        predicate: X86BranchPredicate,
    ) -> Result<PreHirExpr, MlilPreviewError> {
        let mut visiting = HashSet::default();
        // Tested values (EqZero/NeZero/…) freeze Copy sources; compare operands
        // still use ordinary lowering so both sides stay live-SSA consistent.
        let lower_tested = |this: &mut Self, vn: &Varnode, visiting: &mut HashSet<VarnodeKey>| {
            this.lower_flag_tested_value(vn, visiting)
        };
        let lower = |this: &mut Self, vn: &Varnode, visiting: &mut HashSet<VarnodeKey>| {
            this.lower_wrapped_varnode(vn, visiting)
        };
        Ok(match predicate {
            X86BranchPredicate::EqZero(value) => {
                let value = lower_tested(self, &value, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Eq, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::NeZero(value) => {
                let value = lower_tested(self, &value, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Ne, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::SLtZero(value) => {
                let value = lower_tested(self, &value, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLt, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::SLeZero(value) => {
                let value = lower_tested(self, &value, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLe, value.clone(), zero_like(&value))
            }
            X86BranchPredicate::SGtZero(value) => {
                let value = lower_tested(self, &value, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLt, zero_like(&value), value)
            }
            X86BranchPredicate::SGeZero(value) => {
                let value = lower_tested(self, &value, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLe, zero_like(&value), value)
            }
            X86BranchPredicate::MaskEqZero { value, mask } => {
                let value = lower_tested(self, &value, &mut visiting)?;
                let mask = lower(self, &mask, &mut visiting)?;
                let masked = PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(value.clone()),
                    rhs: Box::new(mask),
                    ty: expr_type(&value),
                };
                bool_binary(PreHirBinaryOp::Eq, masked.clone(), zero_like(&masked))
            }
            X86BranchPredicate::MaskNeZero { value, mask } => {
                let value = lower_tested(self, &value, &mut visiting)?;
                let mask = lower(self, &mask, &mut visiting)?;
                let masked = PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(value.clone()),
                    rhs: Box::new(mask),
                    ty: expr_type(&value),
                };
                bool_binary(PreHirBinaryOp::Ne, masked.clone(), zero_like(&masked))
            }
            X86BranchPredicate::Eq(operands) => {
                let (lhs, rhs) = self.lower_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Eq, lhs, rhs)
            }
            X86BranchPredicate::Ne(operands) => {
                let (lhs, rhs) = self.lower_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Ne, lhs, rhs)
            }
            X86BranchPredicate::ULt(operands) => {
                let (lhs, rhs) = self.lower_unsigned_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Lt, lhs, rhs)
            }
            X86BranchPredicate::ULe(operands) => {
                let (lhs, rhs) = self.lower_unsigned_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Le, lhs, rhs)
            }
            X86BranchPredicate::UGt(operands) => {
                let (lhs, rhs) = self.lower_unsigned_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Lt, rhs, lhs)
            }
            X86BranchPredicate::UGe(operands) => {
                let (lhs, rhs) = self.lower_unsigned_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::Le, rhs, lhs)
            }
            X86BranchPredicate::SLt(operands) => {
                let (lhs, rhs) = self.lower_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLt, lhs, rhs)
            }
            X86BranchPredicate::SLe(operands) => {
                let (lhs, rhs) = self.lower_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLe, lhs, rhs)
            }
            X86BranchPredicate::SGt(operands) => {
                let (lhs, rhs) = self.lower_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLt, rhs, lhs)
            }
            X86BranchPredicate::SGe(operands) => {
                let (lhs, rhs) = self.lower_compare_operands(&operands, &mut visiting)?;
                bool_binary(PreHirBinaryOp::SLe, rhs, lhs)
            }
        })
    }

    /// Comparison inputs are values at the flag-producing operation, not at
    /// the later branch. This matters for two-address p-code such as
    /// `IntSub rax, rhs -> rax`: lowering `rax` at the CBranch observes the
    /// subtraction result, while lowering it at `IntSub` observes the input
    /// value whose flags the branch actually tests.
    fn lower_compare_operands(
        &mut self,
        operands: &X86CompareOperands,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<(PreHirExpr, PreHirExpr), MlilPreviewError> {
        self.with_lowering_site(operands.site, |this| {
            let lhs = this.lower_wrapped_varnode(&operands.lhs, visiting)?;
            let rhs = this.lower_wrapped_varnode(&operands.rhs, visiting)?;
            Ok((lhs, rhs))
        })
    }

    fn lower_unsigned_compare_operands(
        &mut self,
        operands: &X86CompareOperands,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Result<(PreHirExpr, PreHirExpr), MlilPreviewError> {
        let (lhs, rhs) = self.lower_compare_operands(operands, visiting)?;
        let bits = operands.lhs.size.saturating_mul(8);
        Ok((
            self.coerce_unsigned_compare_operand(lhs, bits),
            self.coerce_unsigned_compare_operand(rhs, bits),
        ))
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
    Eq(X86CompareOperands),
    Ne(X86CompareOperands),
    ULt(X86CompareOperands),
    ULe(X86CompareOperands),
    UGt(X86CompareOperands),
    UGe(X86CompareOperands),
    SLt(X86CompareOperands),
    SLe(X86CompareOperands),
    SGt(X86CompareOperands),
    SGe(X86CompareOperands),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct X86CompareOperands {
    lhs: Varnode,
    rhs: Varnode,
    /// Operation whose flags describe `lhs op rhs`. The operands must be
    /// lowered here so destructive outputs cannot replace their input values.
    site: LoweringSite,
    /// Arithmetic result tested for equality with zero. When it aliases an
    /// input, equality predicates must test this result directly: a later
    /// materialized high variable may represent the post-operation value even
    /// when queried under the producer site.
    result: Option<Varnode>,
}

impl X86CompareOperands {
    fn destructive_result(&self) -> Option<&Varnode> {
        self.result
            .as_ref()
            .filter(|result| **result == self.lhs || **result == self.rhs)
    }
}

fn bool_binary(op: PreHirBinaryOp, lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

fn zero_like(expr: &PreHirExpr) -> PreHirExpr {
    PreHirExpr::Const(0, expr_type(expr))
}

fn same_cmp_pair(lhs: &X86CompareOperands, rhs: &X86CompareOperands) -> bool {
    lhs.lhs == rhs.lhs && lhs.rhs == rhs.rhs
}

#[cfg(test)]
#[path = "terminator_tests.rs"]
mod tests;
