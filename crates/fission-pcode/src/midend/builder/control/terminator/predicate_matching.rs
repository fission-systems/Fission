//! x86 flag-expression recognition used by conditional branch lowering.
//!
//! The surrounding terminator code decides how a terminator is rendered and
//! how its targets are recovered.  This module only recognizes the boolean
//! expression families produced from x86 `test`/`cmp` flag p-code and returns
//! the canonical predicate description consumed by that lowering.

use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn match_cmp_branch_predicate(&self, vn: &Varnode) -> Option<X86BranchPredicate> {
        let peeled = self.peel_passthrough_varnode(vn);

        if let Some(operands) = self.match_cmp_zero_flag_from_peeled(&peeled) {
            if let Some(result) = operands.destructive_result() {
                return Some(X86BranchPredicate::EqZero(result.clone()));
            }
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::Eq(operands));
            }
        }
        if let Some(inner) = self.match_bool_negate_from_peeled(&peeled)
            && let Some(operands) = self.match_cmp_zero_flag(&inner)
        {
            if let Some(result) = operands.destructive_result() {
                return Some(X86BranchPredicate::NeZero(result.clone()));
            }
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::Ne(operands));
            }
        }
        if let Some(operands) = self.match_cmp_carry_flag_from_peeled(&peeled) {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::ULt(operands));
            }
        }
        if let Some(inner) = self.match_bool_negate_from_peeled(&peeled)
            && let Some(operands) = self.match_cmp_carry_flag(&inner)
        {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::UGe(operands));
            }
        }
        if let Some(operands) = self.match_unsigned_le(vn) {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::ULe(operands));
            }
        }
        if let Some(operands) = self.match_unsigned_gt(vn) {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::UGt(operands));
            }
        }
        if let Some(operands) = self.match_cmp_sign_overflow_ne(vn) {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::SLt(operands));
            }
        }
        if let Some(operands) = self.match_cmp_sign_overflow_eq(vn) {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::SGe(operands));
            }
        }
        if let Some(operands) = self.match_signed_gt(vn) {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::SGt(operands));
            }
        }
        if let Some(operands) = self.match_signed_le(vn) {
            if self.is_simple_branch_value(&operands.lhs)
                && self.is_simple_branch_value(&operands.rhs)
            {
                return Some(X86BranchPredicate::SLe(operands));
            }
        }
        None
    }

    pub(super) fn peel_passthrough_varnode(&self, vn: &Varnode) -> Varnode {
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

    pub(super) fn match_bool_negate_from_peeled(&self, peeled: &Varnode) -> Option<Varnode> {
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

    /// Peel unique/temp Copy chains but stop at the first register.
    ///
    /// Full `peel_passthrough` would continue `rax ← rcx` and then read a
    /// later redefinition of `rcx` when lowering `test rax` after
    /// `mov rax,rcx; mov ecx,edx`.
    fn peel_to_register_or_value(&self, vn: &Varnode) -> Varnode {
        let mut current = vn.clone();
        for _ in 0..PASSTHROUGH_PEEL_MAX_STEPS {
            if is_register_varnode(&current) {
                return current;
            }
            let Some((_, op)) = self.lookup_def_site(&current) else {
                break;
            };
            match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
                    if op.inputs.len() == 1 =>
                {
                    current = op.inputs[0].clone();
                }
                _ => break,
            }
        }
        current
    }

    fn classify_test_input(&self, source: &Varnode) -> Option<(Varnode, Option<Varnode>)> {
        let peeled = self.peel_passthrough_varnode(source);
        let (_, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntAnd || op.inputs.len() != 2 {
            return None;
        }
        let lhs = self.peel_to_register_or_value(&op.inputs[0]);
        let rhs = self.peel_to_register_or_value(&op.inputs[1]);
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

    pub(super) fn match_test_zero_flag(&self, vn: &Varnode) -> Option<(Varnode, Option<Varnode>)> {
        let source = self.match_zero_compare_input(vn)?;
        self.classify_test_input(&source)
    }

    pub(super) fn match_test_sign_flag(&self, vn: &Varnode) -> Option<Varnode> {
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

    pub(super) fn match_test_gt_zero(&self, vn: &Varnode) -> Option<Varnode> {
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

    pub(super) fn match_test_le_zero(&self, vn: &Varnode) -> Option<Varnode> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolOr)?;
        self.match_test_le_zero_pair(&lhs, &rhs)
            .or_else(|| self.match_test_le_zero_pair(&rhs, &lhs))
    }

    fn match_test_le_zero_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<Varnode> {
        let zf_value = self.match_test_zero_flag(lhs)?.0;
        let sign_value = self.match_test_sign_ne_zero(rhs)?;
        (zf_value == sign_value).then_some(zf_value)
    }

    fn match_cmp_diff(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_cmp_diff_from_peeled(&peeled)
    }

    pub(super) fn match_cmp_diff_from_peeled(
        &self,
        peeled: &Varnode,
    ) -> Option<X86CompareOperands> {
        let (site, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntSub || op.inputs.len() != 2 {
            return None;
        }
        Some(X86CompareOperands {
            lhs: op.inputs[0].clone(),
            rhs: op.inputs[1].clone(),
            site,
            result: Some(peeled.clone()),
        })
    }

    fn match_cmp_zero_flag(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let source = self.match_zero_compare_input(vn)?;
        self.match_cmp_diff(&source)
    }

    fn match_cmp_zero_flag_from_peeled(&self, peeled: &Varnode) -> Option<X86CompareOperands> {
        let source = self.match_zero_compare_input_from_peeled(peeled)?;
        self.match_cmp_diff(&source)
    }

    fn match_cmp_carry_flag(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let peeled = self.peel_passthrough_varnode(vn);
        self.match_cmp_carry_flag_from_peeled(&peeled)
    }

    fn match_cmp_carry_flag_from_peeled(&self, peeled: &Varnode) -> Option<X86CompareOperands> {
        let (site, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntLess || op.inputs.len() != 2 {
            return None;
        }
        Some(X86CompareOperands {
            lhs: op.inputs[0].clone(),
            rhs: op.inputs[1].clone(),
            site,
            result: None,
        })
    }

    fn match_cmp_sign_flag(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let source = self.match_signed_less_than_zero_input(vn)?;
        self.match_cmp_diff(&source)
    }

    fn match_cmp_overflow_flag(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let peeled = self.peel_passthrough_varnode(vn);
        let (site, op) = self.lookup_def_site(&peeled)?;
        if op.opcode != PcodeOpcode::IntSBorrow || op.inputs.len() != 2 {
            return None;
        }
        Some(X86CompareOperands {
            lhs: op.inputs[0].clone(),
            rhs: op.inputs[1].clone(),
            site,
            result: None,
        })
    }

    fn match_cmp_sign_overflow_ne(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntNotEqual)?;
        self.match_cmp_sign_overflow_pair(&lhs, &rhs)
            .or_else(|| self.match_cmp_sign_overflow_pair(&rhs, &lhs))
    }

    fn match_cmp_sign_overflow_eq(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let (lhs, rhs) = self.match_compare_pair(vn, PcodeOpcode::IntEqual)?;
        self.match_cmp_sign_overflow_pair(&lhs, &rhs)
            .or_else(|| self.match_cmp_sign_overflow_pair(&rhs, &lhs))
    }

    fn match_cmp_sign_overflow_pair(
        &self,
        lhs: &Varnode,
        rhs: &Varnode,
    ) -> Option<X86CompareOperands> {
        let sign = self.match_cmp_sign_flag(lhs)?;
        let overflow = self.match_cmp_overflow_flag(rhs)?;
        same_cmp_pair(&sign, &overflow).then_some(sign)
    }

    fn match_unsigned_le(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolOr)?;
        self.match_unsigned_le_pair(&lhs, &rhs)
            .or_else(|| self.match_unsigned_le_pair(&rhs, &lhs))
    }

    fn match_unsigned_le_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<X86CompareOperands> {
        let carry = self.match_cmp_carry_flag(lhs)?;
        let zero = self.match_cmp_zero_flag(rhs)?;
        same_cmp_pair(&carry, &zero).then_some(carry)
    }

    fn match_unsigned_gt(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        if let Some((lhs, rhs)) = self.match_bool_binary(vn, PcodeOpcode::BoolAnd) {
            if let Some(res) = self
                .match_unsigned_gt_pair(&lhs, &rhs)
                .or_else(|| self.match_unsigned_gt_pair(&rhs, &lhs))
            {
                return Some(res);
            }
        }
        if let Some(inner) = self.match_bool_negate(vn) {
            if let Some((lhs, rhs)) = self.match_bool_binary(&inner, PcodeOpcode::BoolOr) {
                if let Some(res) = self
                    .match_unsigned_le_pair(&lhs, &rhs)
                    .or_else(|| self.match_unsigned_le_pair(&rhs, &lhs))
                {
                    return Some(res);
                }
            }
        }
        None
    }

    fn match_unsigned_gt_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<X86CompareOperands> {
        let not_cf = self.match_bool_negate(lhs)?;
        let carry = self.match_cmp_carry_flag(&not_cf)?;
        let not_zf = self.match_bool_negate(rhs)?;
        let zero = self.match_cmp_zero_flag(&not_zf)?;
        same_cmp_pair(&carry, &zero).then_some(carry)
    }

    fn match_signed_gt(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolAnd)?;
        self.match_signed_gt_pair(&lhs, &rhs)
            .or_else(|| self.match_signed_gt_pair(&rhs, &lhs))
    }

    fn match_signed_gt_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<X86CompareOperands> {
        let not_zf = self.match_bool_negate(lhs)?;
        let zero = self.match_cmp_zero_flag(&not_zf)?;
        let sign = self.match_cmp_sign_overflow_eq(rhs)?;
        same_cmp_pair(&zero, &sign).then_some(zero)
    }

    fn match_signed_le(&self, vn: &Varnode) -> Option<X86CompareOperands> {
        let (lhs, rhs) = self.match_bool_binary(vn, PcodeOpcode::BoolOr)?;
        self.match_signed_le_pair(&lhs, &rhs)
            .or_else(|| self.match_signed_le_pair(&rhs, &lhs))
    }

    fn match_signed_le_pair(&self, lhs: &Varnode, rhs: &Varnode) -> Option<X86CompareOperands> {
        let zero = self.match_cmp_zero_flag(lhs)?;
        let sign = self.match_cmp_sign_overflow_ne(rhs)?;
        same_cmp_pair(&zero, &sign).then_some(zero)
    }

    fn is_zero_valued_varnode(&self, vn: &Varnode) -> bool {
        self.peel_passthrough_varnode(vn).is_zero()
    }

    fn is_simple_branch_value(&self, vn: &Varnode) -> bool {
        let peeled = self.peel_passthrough_varnode(vn);
        peeled.is_constant
            || is_register_space_id(peeled.space_id)
            || is_unique_space_id(peeled.space_id)
    }
}
