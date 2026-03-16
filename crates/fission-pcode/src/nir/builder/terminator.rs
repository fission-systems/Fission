use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir) fn lower_block_terminator(
        &mut self,
        idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError> {
        let block = &self.pcode.blocks[idx];
        let Some(term_idx) = self.block_terminator_index(block) else {
            return Ok(LoweredTerminator::Fallthrough(self.next_block_address(idx)));
        };
        let op = &block.ops[term_idx];
        self.with_lowering_site(
            LoweringSite {
                block_idx: idx,
                op_idx: term_idx,
            },
            |this| match op.opcode {
                PcodeOpcode::Return => {
                    let expr = op
                        .inputs
                        .last()
                        .map(|input| this.lower_wrapped_varnode(input, &mut HashSet::new()))
                        .transpose()?;
                    Ok(LoweredTerminator::Return(expr))
                }
                PcodeOpcode::Branch if op.inputs.len() == 1 => {
                    let Some(target) = op.inputs.first().and_then(branch_target_address) else {
                        return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                    };
                    Ok(LoweredTerminator::Goto(target))
                }
                PcodeOpcode::CBranch | PcodeOpcode::Branch if op.inputs.len() >= 2 => {
                    let Some(true_target) = branch_target_address(&op.inputs[0]) else {
                        return Err(MlilPreviewError::UnsupportedCfgBranchTarget);
                    };
                    let cond = this
                        .try_recover_x86_branch_condition(&op.inputs[1])?
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
                PcodeOpcode::BranchInd => Ok(LoweredTerminator::Unsupported),
                _ => Ok(LoweredTerminator::Fallthrough(this.next_block_address(idx))),
            },
        )
    }

    fn try_recover_x86_branch_condition(
        &mut self,
        vn: &Varnode,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        if self.options.is_64bit {
            return Ok(None);
        }
        let predicate = self
            .match_test_branch_predicate(vn)
            .or_else(|| self.match_cmp_branch_predicate(vn));
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
        if let Some((value, mask)) = self.match_test_zero_flag(vn) {
            return Some(match mask {
                Some(mask) => X86BranchPredicate::MaskEqZero { value, mask },
                None => X86BranchPredicate::EqZero(value),
            });
        }
        if let Some(inner) = self.match_bool_negate(vn)
            && let Some((value, mask)) = self.match_test_zero_flag(&inner)
        {
            return Some(match mask {
                Some(mask) => X86BranchPredicate::MaskNeZero { value, mask },
                None => X86BranchPredicate::NeZero(value),
            });
        }
        if let Some(value) = self.match_test_sign_flag(vn) {
            return Some(X86BranchPredicate::SLtZero(value));
        }
        if let Some(inner) = self.match_bool_negate(vn)
            && let Some(value) = self.match_test_sign_flag(&inner)
        {
            return Some(X86BranchPredicate::SGeZero(value));
        }
        if let Some(value) = self.match_test_gt_zero(vn) {
            return Some(X86BranchPredicate::SGtZero(value));
        }
        if let Some(value) = self.match_test_le_zero(vn) {
            return Some(X86BranchPredicate::SLeZero(value));
        }
        None
    }

    fn match_cmp_branch_predicate(&self, vn: &Varnode) -> Option<X86BranchPredicate> {
        if let Some((lhs, rhs)) = self.match_cmp_zero_flag(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::Eq(lhs, rhs));
            }
        }
        if let Some(inner) = self.match_bool_negate(vn)
            && let Some((lhs, rhs)) = self.match_cmp_zero_flag(&inner)
        {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::Ne(lhs, rhs));
            }
        }
        if let Some((lhs, rhs)) = self.match_cmp_carry_flag(vn) {
            if self.is_simple_branch_value(&lhs) && self.is_simple_branch_value(&rhs) {
                return Some(X86BranchPredicate::ULt(lhs, rhs));
            }
        }
        if let Some(inner) = self.match_bool_negate(vn)
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
        let mut current = vn.clone();
        while let Some((_, op)) = self.lookup_def_site(&current) {
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
        current
    }

    fn match_bool_negate(&self, vn: &Varnode) -> Option<Varnode> {
        let peeled = self.peel_passthrough_varnode(vn);
        let (_, op) = self.lookup_def_site(&peeled)?;
        (op.opcode == PcodeOpcode::BoolNegate && op.inputs.len() == 1).then(|| op.inputs[0].clone())
    }

    fn match_bool_binary(&self, vn: &Varnode, opcode: PcodeOpcode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
        let (_, op) = self.lookup_def_site(&peeled)?;
        (op.opcode == opcode && op.inputs.len() == 2)
            .then(|| (op.inputs[0].clone(), op.inputs[1].clone()))
    }

    fn match_compare_pair(&self, vn: &Varnode, opcode: PcodeOpcode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
        let (_, op) = self.lookup_def_site(&peeled)?;
        (op.opcode == opcode && op.inputs.len() == 2)
            .then(|| (op.inputs[0].clone(), op.inputs[1].clone()))
    }

    fn match_zero_compare_input(&self, vn: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntEqual)?;
        if lhs.is_zero() {
            return Some(rhs);
        }
        if rhs.is_zero() {
            return Some(lhs);
        }
        None
    }

    fn match_signed_less_than_zero_input(&self, vn: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntSLess)?;
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

    fn match_cmp_carry_flag(&self, vn: &Varnode) -> Option<(Varnode, Varnode)> {
        let peeled = self.peel_passthrough_varnode(vn);
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
