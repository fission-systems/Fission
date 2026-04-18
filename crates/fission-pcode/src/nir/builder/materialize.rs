use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplacementReadClass {
    SameBlockData,
    PredicateSensitive,
    SelectorSensitive,
    ReturnPath,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaterializationRejectionReason {
    AliasUnsafe,
    MissingMergeBinding,
    ConsumerRequiresStableRepresentative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplacementCompleteness {
    Complete,
    Incomplete(MaterializationRejectionReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReplacementValuePlan {
    dominant_read: ReplacementReadClass,
    completeness: ReplacementCompleteness,
}

impl ReplacementValuePlan {
    fn complete(dominant_read: ReplacementReadClass) -> Self {
        Self {
            dominant_read,
            completeness: ReplacementCompleteness::Complete,
        }
    }

    fn incomplete(
        dominant_read: ReplacementReadClass,
        reason: MaterializationRejectionReason,
    ) -> Self {
        Self {
            dominant_read,
            completeness: ReplacementCompleteness::Incomplete(reason),
        }
    }

    fn is_complete(self) -> bool {
        matches!(self.completeness, ReplacementCompleteness::Complete)
    }
}

impl<'a> PreviewBuilder<'a> {
    fn should_preserve_materialized_expr(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::Const(..) => false,
            HirExpr::Cast { expr, .. } => Self::should_preserve_materialized_expr(expr),
            HirExpr::Unary { .. }
            | HirExpr::Binary { .. }
            | HirExpr::Call { .. }
            | HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. } => true,
        }
    }

    fn is_callee_saved_push_store(&self, op: &PcodeOp) -> bool {
        let Some(asm) = op.asm_mnemonic.as_deref() else {
            return false;
        };
        let asm = asm.trim().to_ascii_uppercase();
        asm.starts_with("PUSH RSI")
            || asm.starts_with("PUSH RDI")
            || asm.starts_with("PUSH RBX")
            || asm.starts_with("PUSH RBP")
            || asm.starts_with("PUSH R12")
            || asm.starts_with("PUSH R13")
            || asm.starts_with("PUSH R14")
            || asm.starts_with("PUSH R15")
    }

    fn is_call_return_scaffold_store(
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

    fn call_result_registers(&self) -> Vec<Varnode> {
        if !self.options.is_64bit {
            return Vec::new();
        }
        vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x00,
                size: self.options.pointer_size,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: crate::arch::x86::X86_REG_BASE,
                size: self.options.pointer_size,
                is_constant: false,
                constant_val: 0,
            },
        ]
    }

    fn call_result_is_observed(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let ret_regs = self.call_result_registers();
        if ret_regs.is_empty() {
            return false;
        }
        let keys = ret_regs.iter().map(VarnodeKey::from).collect::<Vec<_>>();
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if candidate
                .inputs
                .iter()
                .any(|input| keys.iter().any(|key| VarnodeKey::from(input) == *key))
            {
                return true;
            }
            if let Some(output) = candidate.output.as_ref()
                && keys.iter().any(|key| VarnodeKey::from(output) == *key)
            {
                return false;
            }
        }
        false
    }

    fn ensure_call_result_binding(&mut self, site: LoweringSite, op: &PcodeOp) -> String {
        if let Some(name) = self.call_result_bindings.get(&site) {
            return name.clone();
        }
        let ret_regs = self.call_result_registers();
        let Some(ret_reg) = ret_regs.first() else {
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
        let name = next_temp_name(&type_from_size(ret_reg.size, false), &mut self.temp_next_id);
        self.temps.insert(
            name.clone(),
            NirBinding {
                name: name.clone(),
                ty: type_from_size(ret_reg.size, false),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
        );
        self.call_result_bindings.insert(site, name.clone());
        name
    }

    pub(in crate::nir) fn lower_block_stmts(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let terminator_index = self.block_terminator_index(block);
        let block_idx = self
            .address_to_index
            .get(&block.start_address)
            .copied()
            .unwrap_or(0);
        for (op_idx, op) in block.ops.iter().enumerate() {
            if Some(op_idx) == terminator_index {
                continue;
            }
            let site = LoweringSite { block_idx, op_idx };
            let maybe_stmt = self.with_lowering_site(
                site,
                |this| -> Result<Option<HirStmt>, MlilPreviewError> {
                    let mut visiting = HashSet::new();
                    match op.opcode {
                        PcodeOpcode::Store => {
                            if op.inputs.len() < 3 {
                                this.debug_lowering_error(
                                    "store_malformed_skip",
                                    block.start_address,
                                    u64::from(op.seq_num),
                                    op.opcode,
                                    &MlilPreviewError::UnsupportedExprMemoryBackedVarnode,
                                );
                                return Ok(None);
                            }
                            if this.is_callee_saved_push_store(op)
                                || this.is_call_return_scaffold_store(block, op_idx, op)
                            {
                                return Ok(None);
                            }
                            let lhs = if let Some((slot_name, _slot_ty)) = this
                                .try_stack_slot_lvalue_for_memory_op(
                                    op,
                                    &op.inputs[1],
                                    type_from_size(op.inputs[2].size, false),
                                ) {
                                HirLValue::Var(slot_name)
                            } else {
                                HirLValue::Deref {
                                    ptr: Box::new(
                                        this.lower_varnode(&op.inputs[1], &mut HashSet::new())
                                            .map_err(|err| {
                                                this.debug_lowering_error(
                                                    "store_ptr",
                                                    block.start_address,
                                                    u64::from(op.seq_num),
                                                    op.opcode,
                                                    &err,
                                                );
                                                err
                                            })?,
                                    ),
                                    ty: type_from_size(op.inputs[2].size, false),
                                }
                            };
                            let rhs = if let Some(expr) = this
                                .recover_aggregate_store_rhs_from_block(
                                    block,
                                    op_idx,
                                    &op.inputs[2],
                                )? {
                                expr
                            } else {
                                this.lower_varnode(&op.inputs[2], &mut HashSet::new())
                                    .map_err(|err| {
                                        this.debug_lowering_error(
                                            "store_rhs",
                                            block.start_address,
                                            u64::from(op.seq_num),
                                            op.opcode,
                                            &err,
                                        );
                                        err
                                    })?
                            };
                            Ok(Some(HirStmt::Assign { lhs, rhs }))
                        }
                        PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                            if op.output.is_none() {
                                let recovered_args = if op.inputs.len() > 1 {
                                    None
                                } else {
                                    this.recover_call_args_from_block(block, op_idx)?
                                };
                                let expr = this
                                    .lower_call(op, recovered_args, &mut visiting)
                                    .map_err(|err| {
                                        this.debug_lowering_error(
                                            "call_expr",
                                            block.start_address,
                                            u64::from(op.seq_num),
                                            op.opcode,
                                            &err,
                                        );
                                        err
                                    })?;
                                if this.call_result_is_observed(block, op_idx) {
                                    let lhs =
                                        HirLValue::Var(this.ensure_call_result_binding(site, op));
                                    Ok(Some(HirStmt::Assign { lhs, rhs: expr }))
                                } else {
                                    Ok(Some(HirStmt::Expr(expr)))
                                }
                            } else {
                                this.maybe_materialize_output_stmt(
                                    block.start_address,
                                    block,
                                    op_idx,
                                    terminator_index,
                                    op,
                                )
                            }
                        }
                        _ => this.maybe_materialize_output_stmt(
                            block.start_address,
                            block,
                            op_idx,
                            terminator_index,
                            op,
                        ),
                    }
                },
            )?;
            if let Some(stmt) = maybe_stmt {
                body.push(stmt);
            }
        }
        Ok(body)
    }

    fn maybe_materialize_output_stmt(
        &mut self,
        block_addr: u64,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
    ) -> Result<Option<HirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if self.output_used_only_by_single_store(block, op_idx, output)
            || self.output_used_only_by_passthrough_chain(block, op_idx, output)
        {
            return Ok(None);
        }
        let Some(rhs) = self.try_lower_materialized_output_rhs(block_addr, op)? else {
            return Ok(None);
        };
        let legacy_inline_candidate =
            self.output_replacement_is_complete(block, op_idx, output, &rhs);
        let replacement_plan =
            self.build_replacement_value_plan(block, op_idx, terminator_index, output, &rhs);
        if replacement_plan.is_complete() {
            self.representative_downgrade_count += 1;
            return Ok(None);
        }
        if legacy_inline_candidate {
            self.materialization_inline_suppressed_count += 1;
        }
        let preserve_materialization = Self::should_preserve_materialized_expr(&rhs);
        let lhs = HirLValue::Var(
            self.ensure_temp_binding_for_output(op, output, preserve_materialization)
                .name,
        );
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }

    fn try_lower_materialized_output_rhs(
        &mut self,
        block_addr: u64,
        op: &PcodeOp,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if !is_materializable_output_opcode(op.opcode) {
            return Ok(None);
        }
        let rhs = match self.lower_def_op(op, &mut HashSet::new()) {
            Ok(rhs) => rhs,
            Err(err)
                if matches!(
                    err,
                    MlilPreviewError::LoweringFailed
                        | MlilPreviewError::UnsupportedExprVarnodeLowering
                        | MlilPreviewError::UnsupportedExprAddressMaterialization
                        | MlilPreviewError::UnsupportedExprIndirectValueSource
                        | MlilPreviewError::UnsupportedExprPieceShape
                        | MlilPreviewError::UnsupportedExprPtrArithmetic
                        | MlilPreviewError::UnsupportedExprMemoryBackedVarnode
                        | MlilPreviewError::UnsupportedExprMultiequal
                ) =>
            {
                self.debug_lowering_error(
                    "materialize_output_skip",
                    block_addr,
                    u64::from(op.seq_num),
                    op.opcode,
                    &err,
                );
                return Ok(None);
            }
            Err(err) => {
                self.debug_lowering_error(
                    "materialize_output",
                    block_addr,
                    u64::from(op.seq_num),
                    op.opcode,
                    &err,
                );
                return Err(err);
            }
        };
        let _ = output;
        Ok(Some(rhs))
    }

    fn output_replacement_is_complete(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> bool {
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        uses.len() == 1
            && Self::expr_is_low_cost_builder_inline_candidate(rhs)
            && if Self::expr_requires_passthrough_single_use_inline(rhs) {
                Self::use_opcode_allows_passthrough_single_use_builder_inline(uses[0].1.opcode)
            } else {
                Self::use_opcode_allows_single_use_builder_inline(uses[0].1.opcode)
            }
    }

    fn build_replacement_value_plan(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> ReplacementValuePlan {
        self.replacement_plan_candidate_count += 1;
        if self.output_has_nonlocal_use(block, op_idx, output) {
            self.replacement_plan_rejected_missing_merge_count += 1;
            return ReplacementValuePlan::incomplete(
                ReplacementReadClass::Merge,
                MaterializationRejectionReason::MissingMergeBinding,
            );
        }
        if let Some(read_class) =
            self.classify_terminator_sensitive_output_use(block, op_idx, terminator_index, output)
        {
            if Self::replacement_read_requires_stable_representative(read_class, rhs) {
                self.replacement_plan_rejected_alias_unsafe_count += 1;
                return ReplacementValuePlan::incomplete(
                    read_class,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
            }
            self.replacement_plan_completed_count += 1;
            return ReplacementValuePlan::complete(read_class);
        }
        if self.output_replacement_is_complete(block, op_idx, output, rhs) {
            if Self::same_block_replacement_requires_stable_representative(rhs) {
                self.replacement_plan_rejected_alias_unsafe_count += 1;
                return ReplacementValuePlan::incomplete(
                    ReplacementReadClass::SameBlockData,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
            }
            self.replacement_plan_completed_count += 1;
            return ReplacementValuePlan::complete(ReplacementReadClass::SameBlockData);
        }
        self.replacement_plan_rejected_alias_unsafe_count += 1;
        ReplacementValuePlan::incomplete(
            ReplacementReadClass::SameBlockData,
            MaterializationRejectionReason::AliasUnsafe,
        )
    }

    fn classify_terminator_sensitive_output_use(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<ReplacementReadClass> {
        let Some(terminator_index) = terminator_index else {
            return None;
        };
        let use_sites = self.output_use_sites_in_block(block, op_idx, output);
        if use_sites.len() != 1 || use_sites[0].0 != terminator_index {
            return None;
        }
        let terminator = &block.ops[terminator_index];
        Some(match terminator.opcode {
            PcodeOpcode::CBranch => ReplacementReadClass::PredicateSensitive,
            PcodeOpcode::BranchInd => ReplacementReadClass::SelectorSensitive,
            PcodeOpcode::Return => ReplacementReadClass::ReturnPath,
            _ => ReplacementReadClass::SameBlockData,
        })
    }

    fn replacement_read_requires_stable_representative(
        read_class: ReplacementReadClass,
        rhs: &HirExpr,
    ) -> bool {
        matches!(
            read_class,
            ReplacementReadClass::PredicateSensitive
                | ReplacementReadClass::SelectorSensitive
                | ReplacementReadClass::ReturnPath
        ) && (Self::should_preserve_materialized_expr(rhs)
            || !Self::expr_is_low_cost_builder_inline_candidate(rhs))
    }

    fn same_block_replacement_requires_stable_representative(rhs: &HirExpr) -> bool {
        Self::should_preserve_materialized_expr(rhs)
    }

    fn output_has_nonlocal_use(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let block_idx = self
            .address_to_index
            .get(&block.start_address)
            .copied()
            .unwrap_or(usize::MAX);
        for (candidate_block_idx, candidate_block) in self.pcode.blocks.iter().enumerate() {
            if candidate_block_idx == block_idx {
                continue;
            }
            for candidate in &candidate_block.ops {
                if candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
                {
                    return true;
                }
                if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                    break;
                }
            }
        }
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                break;
            }
            if candidate
                .inputs
                .iter()
                .any(|input| VarnodeKey::from(input) == key)
            {
                return false;
            }
        }
        false
    }

    fn expr_is_low_cost_builder_inline_candidate(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::Const(_, _) => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::expr_is_low_cost_builder_inline_candidate(expr)
            }
            HirExpr::Load { ptr, .. }
            | HirExpr::PtrOffset { base: ptr, .. }
            | HirExpr::AggregateCopy { src: ptr, .. } => {
                Self::expr_is_low_cost_builder_inline_candidate(ptr)
            }
            HirExpr::Index { base, index, .. } => {
                Self::expr_is_low_cost_builder_inline_candidate(base)
                    && Self::expr_is_low_cost_builder_inline_candidate(index)
            }
            HirExpr::Binary { op, lhs, rhs, .. } => {
                matches!(
                    op,
                    HirBinaryOp::Eq
                        | HirBinaryOp::Ne
                        | HirBinaryOp::Lt
                        | HirBinaryOp::Le
                        | HirBinaryOp::SLt
                        | HirBinaryOp::SLe
                        | HirBinaryOp::And
                        | HirBinaryOp::Or
                        | HirBinaryOp::Xor
                        | HirBinaryOp::Add
                        | HirBinaryOp::Sub
                        | HirBinaryOp::Shl
                        | HirBinaryOp::Shr
                        | HirBinaryOp::Sar
                        | HirBinaryOp::Mul
                ) && Self::expr_is_low_cost_builder_inline_candidate(lhs)
                    && Self::expr_is_low_cost_builder_inline_candidate(rhs)
            }
            HirExpr::Call { .. } => false,
        }
    }

    fn use_opcode_allows_single_use_builder_inline(opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Load
                | PcodeOpcode::Store
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntXor
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntLeft
                | PcodeOpcode::IntRight
                | PcodeOpcode::IntSRight
                | PcodeOpcode::IntMult
                | PcodeOpcode::Piece
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Cast
                | PcodeOpcode::PtrAdd
                | PcodeOpcode::PtrSub
        )
    }

    fn use_opcode_allows_passthrough_single_use_builder_inline(opcode: PcodeOpcode) -> bool {
        matches!(
            opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                | PcodeOpcode::Piece
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Cast
        )
    }

    fn expr_requires_passthrough_single_use_inline(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::Const(_, _) => false,
            HirExpr::Cast { expr, .. } => Self::expr_requires_passthrough_single_use_inline(expr),
            HirExpr::Unary { op, expr, .. } => {
                matches!(op, HirUnaryOp::Not)
                    || Self::expr_requires_passthrough_single_use_inline(expr)
            }
            HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. } => true,
            HirExpr::Binary { op, .. } => matches!(
                op,
                HirBinaryOp::LogicalAnd
                    | HirBinaryOp::LogicalOr
                    | HirBinaryOp::Eq
                    | HirBinaryOp::Ne
                    | HirBinaryOp::Lt
                    | HirBinaryOp::Le
                    | HirBinaryOp::SLt
                    | HirBinaryOp::SLe
            ),
            HirExpr::Call { .. } => true,
        }
    }

    fn output_used_only_by_block_terminator(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let mut use_sites = block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .filter(|(_, candidate)| {
                candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
            })
            .map(|(idx, _)| idx);

        let Some(first_use) = use_sites.next() else {
            return false;
        };
        if use_sites.next().is_some() {
            return false;
        }
        Some(first_use) == terminator_index
    }

    fn output_use_sites_in_block<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        let mut uses = Vec::new();
        for (idx, candidate) in block.ops.iter().enumerate().skip(op_idx + 1) {
            if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                break;
            }
            if candidate
                .inputs
                .iter()
                .any(|input| VarnodeKey::from(input) == key)
            {
                uses.push((idx, candidate));
            }
        }
        uses
    }

    fn output_used_only_by_single_store(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if output.size < 16 {
            return false;
        }
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        uses.len() == 1 && uses[0].1.opcode == PcodeOpcode::Store
    }

    fn output_used_only_by_passthrough_chain(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if output.size < 16 {
            return false;
        }
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        !uses.is_empty()
            && uses.iter().all(|(_, op)| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Copy
                        | PcodeOpcode::Cast
                        | PcodeOpcode::IntZExt
                        | PcodeOpcode::IntSExt
                        | PcodeOpcode::SubPiece
                )
            })
    }

    pub(in crate::nir::builder) fn block_terminator_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Option<usize> {
        block.ops.iter().rposition(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    #[test]
    fn low_cost_builder_inline_accepts_single_use_load_chain() {
        let expr = HirExpr::Load {
            ptr: Box::new(HirExpr::PtrOffset {
                base: Box::new(HirExpr::Var("param_1".to_string())),
                offset: 0x20,
            }),
            ty: int(64),
        };

        assert!(PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
            &expr
        ));
    }

    #[test]
    fn low_cost_builder_inline_rejects_calls() {
        let expr = HirExpr::Call {
            target: "helper".to_string(),
            args: vec![HirExpr::Var("param_1".to_string())],
            ty: int(32),
        };

        assert!(!PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
            &expr
        ));
    }

    #[test]
    fn single_use_builder_inline_blocks_call_like_consumers() {
        assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::Call));
        assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CallInd));
        assert!(
            !PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CallOther)
        );
        assert!(!PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::CBranch));
        assert!(
            !PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::BranchInd)
        );
        assert!(
            !PreviewBuilder::use_opcode_allows_single_use_builder_inline(PcodeOpcode::IntEqual)
        );
    }

    #[test]
    fn single_use_builder_inline_keeps_dataflow_consumers() {
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::Copy
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::Load
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::IntAdd
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::PtrAdd
        ));
    }

    #[test]
    fn memory_backed_single_use_inline_requires_passthrough_consumer() {
        let expr = HirExpr::Load {
            ptr: Box::new(HirExpr::Var("param_1".to_string())),
            ty: int(64),
        };

        assert!(PreviewBuilder::expr_requires_passthrough_single_use_inline(
            &expr
        ));
        assert!(
            PreviewBuilder::use_opcode_allows_passthrough_single_use_builder_inline(
                PcodeOpcode::Copy
            )
        );
        assert!(
            !PreviewBuilder::use_opcode_allows_passthrough_single_use_builder_inline(
                PcodeOpcode::IntAdd
            )
        );
    }

    #[test]
    fn plain_leaf_single_use_inline_can_flow_into_data_consumer() {
        let expr = HirExpr::Var("tmp_1".to_string());
        assert!(!PreviewBuilder::expr_requires_passthrough_single_use_inline(&expr));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::IntAdd
        ));
    }

    #[test]
    fn arithmetic_single_use_inline_can_flow_into_data_consumer() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("x".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };

        assert!(!PreviewBuilder::expr_requires_passthrough_single_use_inline(&expr));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::IntAdd
        ));
    }

    #[test]
    fn predicate_single_use_inline_requires_passthrough_consumer() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(HirExpr::Var("x".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: NirType::Bool,
        };

        assert!(PreviewBuilder::expr_requires_passthrough_single_use_inline(
            &expr
        ));
        assert!(
            !PreviewBuilder::use_opcode_allows_passthrough_single_use_builder_inline(
                PcodeOpcode::IntAdd
            )
        );
    }

    #[test]
    fn predicate_sensitive_reads_require_stable_representative_for_nontrivial_rhs() {
        let expr = HirExpr::Load {
            ptr: Box::new(HirExpr::Var("param_1".to_string())),
            ty: int(64),
        };
        assert!(
            PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::PredicateSensitive,
                &expr
            )
        );
        assert!(
            PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::SelectorSensitive,
                &expr
            )
        );
    }

    #[test]
    fn predicate_sensitive_reads_allow_direct_leaf_replacement() {
        let expr = HirExpr::Var("tmp_1".to_string());
        assert!(
            !PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::PredicateSensitive,
                &expr
            )
        );
        assert!(
            !PreviewBuilder::replacement_read_requires_stable_representative(
                ReplacementReadClass::ReturnPath,
                &expr
            )
        );
    }

    #[test]
    fn same_block_replacement_keeps_nonleaf_representatives() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("x".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };

        assert!(PreviewBuilder::same_block_replacement_requires_stable_representative(&expr));
        assert!(
            !PreviewBuilder::same_block_replacement_requires_stable_representative(&HirExpr::Var(
                "tmp_1".to_string()
            ))
        );
    }
}
