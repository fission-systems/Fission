use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn should_preserve_materialized_expr(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(..) => false,
            HirExpr::Cast { expr, .. } => Self::should_preserve_materialized_expr(expr),
            HirExpr::Select { .. } => true,
            HirExpr::Unary { .. }
            | HirExpr::Binary { .. }
            | HirExpr::Call { .. }
            | HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. } => true,
        }
    }

    pub(super) fn expr_is_side_effectful_for_materialization_trace(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call { .. } => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(lhs)
                    || Self::expr_is_side_effectful_for_materialization_trace(rhs)
            }
            HirExpr::Load { ptr, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(ptr)
            }
            HirExpr::PtrOffset { base, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(base)
            }
            HirExpr::Index { base, index, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(base)
                    || Self::expr_is_side_effectful_for_materialization_trace(index)
            }
            HirExpr::AggregateCopy { src, .. } => {
                Self::expr_is_side_effectful_for_materialization_trace(src)
            }
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::expr_is_side_effectful_for_materialization_trace(cond)
                    || Self::expr_is_side_effectful_for_materialization_trace(then_expr)
                    || Self::expr_is_side_effectful_for_materialization_trace(else_expr)
            }
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(..) => false,
        }
    }

    pub(super) fn classify_terminator_sensitive_output_use(
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

    pub(super) fn replacement_read_requires_stable_representative(
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

    pub(super) fn same_block_replacement_requires_stable_representative(rhs: &HirExpr) -> bool {
        Self::should_preserve_materialized_expr(rhs)
    }

    fn classify_stable_representative_owner_reason(
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        overlaps_representative_root_attribution: bool,
        overlaps_temp_only_lifecycle: bool,
        overlaps_real_missing_merge: bool,
        downstream_opcode: Option<PcodeOpcode>,
    ) -> StableRepresentativeOwnerReason {
        if overlaps_representative_root_attribution {
            return StableRepresentativeOwnerReason::RootRepresentativeStableRequired;
        }
        if overlaps_temp_only_lifecycle {
            return StableRepresentativeOwnerReason::TempLifecycleStableRequired;
        }
        if overlaps_real_missing_merge {
            return StableRepresentativeOwnerReason::RealMergeStableRequired;
        }
        if matches!(
            consumer_kind,
            DisallowedSingleConsumerConsumerKind::Predicate
                | DisallowedSingleConsumerConsumerKind::BranchCondition
        ) || downstream_opcode == Some(PcodeOpcode::CBranch)
        {
            return StableRepresentativeOwnerReason::PredicateStableRequired;
        }
        if consumer_kind == DisallowedSingleConsumerConsumerKind::StoreValue {
            return StableRepresentativeOwnerReason::StoreValueStableRequired;
        }
        if matches!(
            consumer_kind,
            DisallowedSingleConsumerConsumerKind::OtherData
                | DisallowedSingleConsumerConsumerKind::CallArg
                | DisallowedSingleConsumerConsumerKind::LoadAddr
                | DisallowedSingleConsumerConsumerKind::StoreAddr
                | DisallowedSingleConsumerConsumerKind::PhiMerge
        ) || downstream_opcode.is_some()
        {
            return StableRepresentativeOwnerReason::AliasStableRequired;
        }
        StableRepresentativeOwnerReason::UnknownStableRepresentative
    }

    fn classify_alias_stable_required_family(
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
        downstream_opcode: Option<PcodeOpcode>,
    ) -> AliasStableRequiredFamily {
        if consumer_kind == DisallowedSingleConsumerConsumerKind::LoadAddr
            || downstream_opcode == Some(PcodeOpcode::Load)
        {
            return AliasStableRequiredFamily::LoadAddrStableRequired;
        }
        if consumer_kind == DisallowedSingleConsumerConsumerKind::StoreAddr
            || downstream_opcode == Some(PcodeOpcode::Store)
        {
            return AliasStableRequiredFamily::StoreAddrStableRequired;
        }
        if downstream_opcode == Some(PcodeOpcode::BranchInd) {
            return AliasStableRequiredFamily::BranchIndStableRequired;
        }
        if consumer_kind == DisallowedSingleConsumerConsumerKind::OtherData
            && rhs_kind == DisallowedSingleConsumerRhsKind::LoadLike
        {
            return AliasStableRequiredFamily::OtherDataLoadLikeStable;
        }
        if consumer_kind == DisallowedSingleConsumerConsumerKind::OtherData
            && rhs_kind == DisallowedSingleConsumerRhsKind::VarOrConst
            && downstream_opcode == Some(PcodeOpcode::Copy)
        {
            return AliasStableRequiredFamily::OtherDataCopyStable;
        }
        if consumer_kind == DisallowedSingleConsumerConsumerKind::OtherData
            && matches!(
                rhs_kind,
                DisallowedSingleConsumerRhsKind::Arithmetic
                    | DisallowedSingleConsumerRhsKind::BinaryBoolean
                    | DisallowedSingleConsumerRhsKind::UnaryBoolean
            )
        {
            return AliasStableRequiredFamily::ArithmeticStableRequired;
        }
        AliasStableRequiredFamily::UnknownAliasStable
    }

    fn classify_address_stable_required_base_kind(rhs: &HirExpr) -> AddressStableRequiredBaseKind {
        fn classify_var_name(name: &str) -> AddressStableRequiredBaseKind {
            if name.starts_with("stack_")
                || name.starts_with("local_")
                || name.starts_with("home_")
                || name.starts_with("arg_out_")
                || name.starts_with("ret_scaffold_")
                || matches!(name, "rsp" | "rbp" | "esp" | "ebp")
            {
                AddressStableRequiredBaseKind::StackRelative
            } else if name.starts_with("DAT_") {
                AddressStableRequiredBaseKind::GlobalRelative
            } else if name.starts_with("param_")
                || matches!(
                    name,
                    "rax"
                        | "rbx"
                        | "rcx"
                        | "rdx"
                        | "rsi"
                        | "rdi"
                        | "r8"
                        | "r9"
                        | "r10"
                        | "r11"
                        | "r12"
                        | "r13"
                        | "r14"
                        | "r15"
                        | "eax"
                        | "ebx"
                        | "ecx"
                        | "edx"
                        | "esi"
                        | "edi"
                        | "esp"
                        | "ebp"
                )
                || name.starts_with("tmp_")
                || name.starts_with("uVar")
                || name.starts_with("xVar")
                || name.starts_with("iVar")
                || name.starts_with("bVar")
                || name.starts_with("fVar")
                || name.starts_with("auVar")
                || name.starts_with("puVar")
                || name.starts_with("pp")
            {
                AddressStableRequiredBaseKind::RegisterBase
            } else {
                AddressStableRequiredBaseKind::UnknownBase
            }
        }

        fn merge_base_kinds(
            lhs: AddressStableRequiredBaseKind,
            rhs: AddressStableRequiredBaseKind,
        ) -> AddressStableRequiredBaseKind {
            if lhs == rhs {
                return lhs;
            }
            if lhs == AddressStableRequiredBaseKind::UnknownBase {
                return rhs;
            }
            if rhs == AddressStableRequiredBaseKind::UnknownBase {
                return lhs;
            }
            AddressStableRequiredBaseKind::UnknownBase
        }

        match rhs {
            HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => classify_var_name(name),
            HirExpr::Const(..) => AddressStableRequiredBaseKind::UnknownBase,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => {
                Self::classify_address_stable_required_base_kind(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => merge_base_kinds(
                Self::classify_address_stable_required_base_kind(lhs),
                Self::classify_address_stable_required_base_kind(rhs),
            ),
            HirExpr::Call { .. } => AddressStableRequiredBaseKind::UnknownBase,
            HirExpr::PtrOffset { base, .. } => {
                Self::classify_address_stable_required_base_kind(base)
            }
            HirExpr::Index { base, index, .. } => merge_base_kinds(
                Self::classify_address_stable_required_base_kind(base),
                Self::classify_address_stable_required_base_kind(index),
            ),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => merge_base_kinds(
                Self::classify_address_stable_required_base_kind(cond),
                merge_base_kinds(
                    Self::classify_address_stable_required_base_kind(then_expr),
                    Self::classify_address_stable_required_base_kind(else_expr),
                ),
            ),
        }
    }

    fn classify_address_stable_required_expr_kind(rhs: &HirExpr) -> AddressStableRequiredExprKind {
        fn merge_expr_kinds(
            lhs: AddressStableRequiredExprKind,
            rhs: AddressStableRequiredExprKind,
        ) -> AddressStableRequiredExprKind {
            use AddressStableRequiredExprKind::*;
            if lhs == HasCall || rhs == HasCall {
                return HasCall;
            }
            if lhs == HasLoad || rhs == HasLoad {
                return HasLoad;
            }
            if lhs == PureArithmetic && rhs == PureArithmetic {
                return PureArithmetic;
            }
            UnknownAddressExpr
        }

        match rhs {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(..) => {
                AddressStableRequiredExprKind::PureArithmetic
            }
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::classify_address_stable_required_expr_kind(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => merge_expr_kinds(
                Self::classify_address_stable_required_expr_kind(lhs),
                Self::classify_address_stable_required_expr_kind(rhs),
            ),
            HirExpr::Call { .. } => AddressStableRequiredExprKind::HasCall,
            HirExpr::Load { .. } | HirExpr::AggregateCopy { .. } => {
                AddressStableRequiredExprKind::HasLoad
            }
            HirExpr::PtrOffset { base, .. } => {
                Self::classify_address_stable_required_expr_kind(base)
            }
            HirExpr::Index { base, index, .. } => merge_expr_kinds(
                Self::classify_address_stable_required_expr_kind(base),
                Self::classify_address_stable_required_expr_kind(index),
            ),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => merge_expr_kinds(
                Self::classify_address_stable_required_expr_kind(cond),
                merge_expr_kinds(
                    Self::classify_address_stable_required_expr_kind(then_expr),
                    Self::classify_address_stable_required_expr_kind(else_expr),
                ),
            ),
        }
    }

    fn scan_intervening_address_stability_hazards(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        uses: &[(usize, &PcodeOp)],
    ) -> (bool, bool) {
        let Some(first_use_idx) = uses.first().map(|(idx, _)| *idx) else {
            return (false, false);
        };
        let mut has_intervening_store = false;
        let mut has_intervening_call = false;
        for candidate in block
            .ops
            .iter()
            .skip(op_idx + 1)
            .take(first_use_idx.saturating_sub(op_idx + 1))
        {
            match candidate.opcode {
                PcodeOpcode::Store => has_intervening_store = true,
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                    has_intervening_call = true;
                }
                _ => {}
            }
        }
        (has_intervening_store, has_intervening_call)
    }

    fn classify_address_stable_required_family(
        address_base_kind: AddressStableRequiredBaseKind,
        address_expr_kind: AddressStableRequiredExprKind,
        has_intervening_store: bool,
        has_intervening_call: bool,
    ) -> AddressStableRequiredFamily {
        if address_expr_kind == AddressStableRequiredExprKind::HasLoad {
            return AddressStableRequiredFamily::AddressExprHasLoad;
        }
        if address_expr_kind == AddressStableRequiredExprKind::HasCall {
            return AddressStableRequiredFamily::AddressExprHasCall;
        }
        if has_intervening_call {
            return AddressStableRequiredFamily::AddressExprCrossesCall;
        }
        if has_intervening_store {
            return AddressStableRequiredFamily::AddressExprCrossesStore;
        }
        if address_base_kind == AddressStableRequiredBaseKind::StackRelative {
            return AddressStableRequiredFamily::AddressExprStackRelative;
        }
        if address_base_kind == AddressStableRequiredBaseKind::GlobalRelative {
            return AddressStableRequiredFamily::AddressExprGlobalRelative;
        }
        if address_base_kind == AddressStableRequiredBaseKind::RegisterBase {
            return AddressStableRequiredFamily::AddressExprRegisterBase;
        }
        if address_expr_kind == AddressStableRequiredExprKind::PureArithmetic {
            return AddressStableRequiredFamily::AddressExprPureArithmetic;
        }
        AddressStableRequiredFamily::AddressExprUnknownBase
    }

    pub(super) fn classify_stack_address_base_reg(rhs: &HirExpr) -> StackAddressBaseReg {
        fn classify_var_name(name: &str) -> StackAddressBaseReg {
            match name {
                "rsp" => StackAddressBaseReg::Rsp,
                "rbp" => StackAddressBaseReg::Rbp,
                "esp" => StackAddressBaseReg::Esp,
                "ebp" => StackAddressBaseReg::Ebp,
                _ => StackAddressBaseReg::Unknown,
            }
        }

        fn merge_base_regs(
            lhs: StackAddressBaseReg,
            rhs: StackAddressBaseReg,
        ) -> StackAddressBaseReg {
            if lhs == rhs {
                return lhs;
            }
            if lhs == StackAddressBaseReg::Unknown {
                return rhs;
            }
            if rhs == StackAddressBaseReg::Unknown {
                return lhs;
            }
            StackAddressBaseReg::Unknown
        }

        match rhs {
            HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => classify_var_name(name),
            HirExpr::Const(..) | HirExpr::Call { .. } => StackAddressBaseReg::Unknown,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => {
                Self::classify_stack_address_base_reg(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => merge_base_regs(
                Self::classify_stack_address_base_reg(lhs),
                Self::classify_stack_address_base_reg(rhs),
            ),
            HirExpr::PtrOffset { base, .. } => Self::classify_stack_address_base_reg(base),
            HirExpr::Index { base, index, .. } => merge_base_regs(
                Self::classify_stack_address_base_reg(base),
                Self::classify_stack_address_base_reg(index),
            ),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => merge_base_regs(
                Self::classify_stack_address_base_reg(cond),
                merge_base_regs(
                    Self::classify_stack_address_base_reg(then_expr),
                    Self::classify_stack_address_base_reg(else_expr),
                ),
            ),
        }
    }

    pub(super) fn extract_stack_address_offset(rhs: &HirExpr) -> Option<i64> {
        fn is_stack_base(expr: &HirExpr) -> bool {
            PreviewBuilder::classify_stack_address_base_reg(expr) != StackAddressBaseReg::Unknown
        }

        match rhs {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => is_stack_base(rhs).then_some(0),
            HirExpr::Const(..)
            | HirExpr::Unary { .. }
            | HirExpr::Call { .. }
            | HirExpr::Load { .. }
            | HirExpr::Index { .. }
            | HirExpr::Select { .. }
            | HirExpr::AggregateCopy { .. } => None,
            HirExpr::Cast { expr, .. } => Self::extract_stack_address_offset(expr),
            HirExpr::PtrOffset { base, offset } => is_stack_base(base).then_some(*offset),
            HirExpr::Binary { op, lhs, rhs, .. } => match (op, lhs.as_ref(), rhs.as_ref()) {
                (HirBinaryOp::Add, base, HirExpr::Const(offset, _)) if is_stack_base(base) => {
                    Some(*offset)
                }
                (HirBinaryOp::Add, HirExpr::Const(offset, _), base) if is_stack_base(base) => {
                    Some(*offset)
                }
                (HirBinaryOp::Sub, base, HirExpr::Const(offset, _)) if is_stack_base(base) => {
                    Some(-*offset)
                }
                _ => None,
            },
        }
    }

    fn stack_address_frame_relative_candidate(rhs: &HirExpr) -> bool {
        fn is_simple_frame_relative_expr(expr: &HirExpr) -> bool {
            match expr {
                HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => true,
                HirExpr::Cast { expr, .. } => is_simple_frame_relative_expr(expr),
                HirExpr::PtrOffset { base, .. } => is_simple_frame_relative_expr(base),
                HirExpr::Binary { op, lhs, rhs, .. } => {
                    matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub)
                        && matches!(rhs.as_ref(), HirExpr::Const(..))
                        && is_simple_frame_relative_expr(lhs)
                }
                _ => false,
            }
        }

        Self::classify_address_stable_required_expr_kind(rhs)
            == AddressStableRequiredExprKind::PureArithmetic
            && Self::classify_address_stable_required_base_kind(rhs)
                == AddressStableRequiredBaseKind::StackRelative
            && is_simple_frame_relative_expr(rhs)
    }

    fn stack_base_reg_redefined_before_use(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        uses: &[(usize, &PcodeOp)],
        base_reg: StackAddressBaseReg,
    ) -> bool {
        fn output_base_reg(builder: &PreviewBuilder<'_>, output: &Varnode) -> StackAddressBaseReg {
            let name = match output.space_id {
                UNIQUE_SPACE_ID => unique_register_name(output.offset, output.size),
                space_id if is_register_space_id(space_id) && !builder.options.is_64bit => {
                    crate::nir::support::register_name_32(output.offset, output.size)
                }
                space_id if is_register_space_id(space_id) => Some(
                    crate::nir::support::register_name(output.offset, output.size),
                ),
                _ => None,
            };
            match name {
                Some("rsp") => StackAddressBaseReg::Rsp,
                Some("rbp") => StackAddressBaseReg::Rbp,
                Some("esp") => StackAddressBaseReg::Esp,
                Some("ebp") => StackAddressBaseReg::Ebp,
                _ => StackAddressBaseReg::Unknown,
            }
        }

        if base_reg == StackAddressBaseReg::Unknown {
            return false;
        }
        let Some(first_use_idx) = uses.first().map(|(idx, _)| *idx) else {
            return false;
        };
        for candidate in block
            .ops
            .iter()
            .skip(op_idx + 1)
            .take(first_use_idx.saturating_sub(op_idx + 1))
        {
            let Some(output) = candidate.output.as_ref() else {
                continue;
            };
            if output_base_reg(self, output) == base_reg {
                return true;
            }
        }
        false
    }

    fn classify_stack_address_stability_reason(
        same_block_use_count: usize,
        nonlocal_use_exists: bool,
        crosses_call: bool,
        crosses_store: bool,
        rsp_redefined_before_use: bool,
        frame_relative_candidate: bool,
        base_reg: StackAddressBaseReg,
    ) -> StackAddressStabilityReason {
        if crosses_call {
            return StackAddressStabilityReason::StackAddrCrossesCall;
        }
        if crosses_store {
            return StackAddressStabilityReason::StackAddrCrossesStore;
        }
        if rsp_redefined_before_use {
            return StackAddressStabilityReason::StackAddrRspMutatedBeforeUse;
        }
        if nonlocal_use_exists {
            return StackAddressStabilityReason::StackAddrEscapes;
        }
        if same_block_use_count > 1 {
            return StackAddressStabilityReason::StackAddrMultipleUse;
        }
        if frame_relative_candidate
            && same_block_use_count == 1
            && base_reg != StackAddressBaseReg::Unknown
        {
            return StackAddressStabilityReason::StackAddrFrameStable;
        }
        if same_block_use_count == 1 {
            return StackAddressStabilityReason::StackAddrSingleUse;
        }
        StackAddressStabilityReason::StackAddrUnknown
    }

    fn classify_stack_addr_frame_stable_trial_reason(
        proof: &StackAddressStabilityProof,
        nonlocal_use_exists: bool,
    ) -> Result<(), StackAddrFrameStableTrialReason> {
        if !matches!(
            proof.consumer_kind,
            DisallowedSingleConsumerConsumerKind::LoadAddr
                | DisallowedSingleConsumerConsumerKind::StoreAddr
        ) || !matches!(
            proof.downstream_opcode,
            Some(PcodeOpcode::Load | PcodeOpcode::Store)
        ) {
            return Err(StackAddrFrameStableTrialReason::RejectedConsumerKind);
        }
        if proof.crosses_call || proof.crosses_store {
            return Err(StackAddrFrameStableTrialReason::RejectedCrossesCallOrStore);
        }
        if proof.rsp_redefined_before_use {
            return Err(StackAddrFrameStableTrialReason::RejectedBaseMutation);
        }
        if nonlocal_use_exists || proof.reason == StackAddressStabilityReason::StackAddrEscapes {
            return Err(StackAddrFrameStableTrialReason::RejectedEscapes);
        }
        if proof.same_block_use_count > 1
            || proof.reason == StackAddressStabilityReason::StackAddrMultipleUse
        {
            return Err(StackAddrFrameStableTrialReason::RejectedMultipleUse);
        }
        if proof.reason != StackAddressStabilityReason::StackAddrFrameStable
            || proof.base_reg != StackAddressBaseReg::Rsp
            || !proof.frame_relative_candidate
        {
            return Err(StackAddrFrameStableTrialReason::RejectedNonFrameStable);
        }
        Ok(())
    }

    pub(super) fn describe_stable_representative_owner_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<StableRepresentativeOwnerProof> {
        let rhs_kind = Self::classify_disallowed_single_consumer_rhs_kind(rhs);
        let use_sites = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let output_key = VarnodeKey::from(output);
        let first_use = use_sites.first().map(|(_, op)| *op);
        let mut consumer_kind = DisallowedSingleConsumerConsumerKind::OtherData;
        let mut downstream_opcode = first_use.map(|op| op.opcode);

        let stable_required = if let Some(read_class) =
            self.classify_terminator_sensitive_output_use(block, op_idx, terminator_index, output)
        {
            if !Self::replacement_read_requires_stable_representative(read_class, rhs) {
                return None;
            }
            match read_class {
                ReplacementReadClass::PredicateSensitive => {
                    consumer_kind = DisallowedSingleConsumerConsumerKind::Predicate;
                    downstream_opcode = Some(PcodeOpcode::CBranch);
                }
                ReplacementReadClass::SelectorSensitive => {
                    consumer_kind = DisallowedSingleConsumerConsumerKind::OtherData;
                    downstream_opcode = Some(PcodeOpcode::BranchInd);
                }
                ReplacementReadClass::ReturnPath => {
                    consumer_kind = DisallowedSingleConsumerConsumerKind::OtherData;
                    downstream_opcode = Some(PcodeOpcode::Return);
                }
                ReplacementReadClass::SameBlockData | ReplacementReadClass::Merge => {}
            }
            true
        } else if self.output_replacement_is_complete(block, op_idx, output, rhs)
            && Self::same_block_replacement_requires_stable_representative(rhs)
        {
            if let Some(use_op) = first_use {
                let matched_inputs = use_op
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, input)| {
                        (VarnodeKey::from(input) == output_key).then_some(idx)
                    })
                    .collect::<Vec<_>>();
                consumer_kind =
                    Self::classify_disallowed_single_consumer_kind(use_op, &matched_inputs);
                downstream_opcode = Some(use_op.opcode);
            }
            true
        } else {
            false
        };

        if !stable_required {
            return None;
        }

        let nonlocal_reason = self
            .output_has_nonlocal_use(block, op_idx, output)
            .then(|| {
                self.classify_nonlocal_materialization_rejection_reason(block, op_idx, output, rhs)
            });
        let overlaps_representative_root_attribution =
            nonlocal_reason == Some(MaterializationRejectionReason::RepresentativeRootAttribution);
        let overlaps_temp_only_lifecycle = matches!(
            nonlocal_reason,
            Some(
                MaterializationRejectionReason::TempOnlyRepresentativeLifecycle
                    | MaterializationRejectionReason::DeadTempRepresentative
            )
        );
        let overlaps_real_missing_merge =
            nonlocal_reason == Some(MaterializationRejectionReason::MissingMergeBinding);
        let reason = Self::classify_stable_representative_owner_reason(
            consumer_kind,
            overlaps_representative_root_attribution,
            overlaps_temp_only_lifecycle,
            overlaps_real_missing_merge,
            downstream_opcode,
        );

        Some(StableRepresentativeOwnerProof {
            consumer_kind,
            rhs_kind,
            overlaps_representative_root_attribution,
            overlaps_temp_only_lifecycle,
            overlaps_real_missing_merge,
            downstream_opcode,
            reason,
        })
    }

    pub(super) fn describe_alias_stable_required_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<AliasStableRequiredProof> {
        let proof = self.describe_stable_representative_owner_proof(
            block,
            op_idx,
            terminator_index,
            output,
            rhs,
        )?;
        if proof.reason != StableRepresentativeOwnerReason::AliasStableRequired {
            return None;
        }
        let same_block_use_count =
            Self::collect_output_use_sites_in_block(block, op_idx, output).len();
        let rhs_has_load = proof.rhs_kind == DisallowedSingleConsumerRhsKind::LoadLike;
        let rhs_has_call = proof.rhs_kind == DisallowedSingleConsumerRhsKind::CallLike;
        let requires_preserved_expr = Self::should_preserve_materialized_expr(rhs);
        let reason = Self::classify_alias_stable_required_family(
            proof.consumer_kind,
            proof.rhs_kind,
            proof.downstream_opcode,
        );
        Some(AliasStableRequiredProof {
            consumer_kind: proof.consumer_kind,
            rhs_kind: proof.rhs_kind,
            downstream_opcode: proof.downstream_opcode,
            same_block_use_count,
            rhs_has_load,
            rhs_has_call,
            requires_preserved_expr,
            reason,
        })
    }

    pub(super) fn describe_address_stable_required_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<AddressStableRequiredProof> {
        let proof = self.describe_alias_stable_required_proof(
            block,
            op_idx,
            terminator_index,
            output,
            rhs,
        )?;
        if !matches!(
            proof.reason,
            AliasStableRequiredFamily::LoadAddrStableRequired
                | AliasStableRequiredFamily::StoreAddrStableRequired
        ) {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (has_intervening_store, has_intervening_call) =
            Self::scan_intervening_address_stability_hazards(block, op_idx, &uses);
        let address_base_kind = Self::classify_address_stable_required_base_kind(rhs);
        let address_expr_kind = Self::classify_address_stable_required_expr_kind(rhs);
        let reason = Self::classify_address_stable_required_family(
            address_base_kind,
            address_expr_kind,
            has_intervening_store,
            has_intervening_call,
        );
        Some(AddressStableRequiredProof {
            consumer_kind: proof.consumer_kind,
            rhs_kind: proof.rhs_kind,
            downstream_opcode: proof.downstream_opcode,
            same_block_use_count: proof.same_block_use_count,
            rhs_has_load: proof.rhs_has_load,
            rhs_has_call: proof.rhs_has_call,
            address_base_kind,
            address_expr_kind,
            has_intervening_store,
            has_intervening_call,
            reason,
        })
    }

    pub(super) fn describe_stack_address_stability_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<StackAddressStabilityProof> {
        let proof = self.describe_address_stable_required_proof(
            block,
            op_idx,
            terminator_index,
            output,
            rhs,
        )?;
        if proof.reason != AddressStableRequiredFamily::AddressExprStackRelative {
            return None;
        }
        if !matches!(
            proof.consumer_kind,
            DisallowedSingleConsumerConsumerKind::LoadAddr
                | DisallowedSingleConsumerConsumerKind::StoreAddr
        ) {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let same_block_use_count = uses.len();
        let nonlocal_use_exists = self.output_has_nonlocal_use(block, op_idx, output);
        let base_reg = Self::classify_stack_address_base_reg(rhs);
        let offset = Self::extract_stack_address_offset(rhs);
        let frame_relative_candidate = Self::stack_address_frame_relative_candidate(rhs);
        let rsp_redefined_before_use =
            self.stack_base_reg_redefined_before_use(block, op_idx, &uses, base_reg);
        let reason = Self::classify_stack_address_stability_reason(
            same_block_use_count,
            nonlocal_use_exists,
            proof.has_intervening_call,
            proof.has_intervening_store,
            rsp_redefined_before_use,
            frame_relative_candidate,
            base_reg,
        );
        Some(StackAddressStabilityProof {
            consumer_kind: proof.consumer_kind,
            downstream_opcode: proof.downstream_opcode,
            base_reg,
            offset,
            same_block_use_count,
            crosses_call: proof.has_intervening_call,
            crosses_store: proof.has_intervening_store,
            rsp_redefined_before_use,
            frame_relative_candidate,
            reason,
        })
    }

    pub(super) fn describe_stack_addr_frame_stable_trial(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Result<StackAddressStabilityProof, StackAddrFrameStableTrialReason> {
        let proof = self
            .describe_stack_address_stability_proof(block, op_idx, terminator_index, output, rhs)
            .ok_or(StackAddrFrameStableTrialReason::RejectedNonFrameStable)?;
        let nonlocal_use_exists = self.output_has_nonlocal_use(block, op_idx, output);
        Self::classify_stack_addr_frame_stable_trial_reason(&proof, nonlocal_use_exists)?;
        Ok(proof)
    }

    pub(super) fn output_has_nonlocal_use(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        let Some(block_idx) = self.address_to_index.get(&block.start_address).copied() else {
            return false;
        };
        for (candidate_block_idx, candidate_block) in self.pcode.blocks.iter().enumerate() {
            if candidate_block_idx == block_idx {
                continue;
            }
            if !self.block_can_reach(block_idx, candidate_block_idx, usize::MAX) {
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

    pub(super) fn classify_alias_unsafe_hazard(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> AliasUnsafeHazard {
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        if let Some(hazard) = Self::first_intervening_alias_unsafe_hazard(block, op_idx, &uses) {
            return hazard;
        }
        if uses.len() > 1 {
            let second_use = uses.get(1).or_else(|| uses.first());
            return AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::MultipleSameBlockConsumers,
                second_use.map(|(idx, _)| *idx),
                second_use.map(|(idx, _)| *idx),
                second_use.map(|(_, op)| op.opcode),
            );
        }
        if let Some((use_idx, use_op)) = uses.first().copied() {
            if terminator_index.is_some_and(|term_idx| use_idx > term_idx) {
                return AliasUnsafeHazard::new(
                    AliasUnsafeHazardKind::UnknownConsumerAfterTerminator,
                    Some(use_idx),
                    Some(use_idx),
                    Some(use_op.opcode),
                );
            }
            let passthrough_required = Self::expr_requires_passthrough_single_use_inline(rhs);
            let consumer_allows_inline = if passthrough_required {
                Self::use_opcode_allows_passthrough_single_use_builder_inline(use_op.opcode)
            } else {
                Self::use_opcode_allows_single_use_builder_inline(use_op.opcode)
            };
            if !Self::expr_is_low_cost_builder_inline_candidate(rhs) || !consumer_allows_inline {
                return AliasUnsafeHazard::new(
                    if consumer_allows_inline {
                        AliasUnsafeHazardKind::UnknownUnhandledConsumerKind
                    } else {
                        AliasUnsafeHazardKind::DisallowedSingleConsumer
                    },
                    Some(use_idx),
                    Some(use_idx),
                    Some(use_op.opcode),
                );
            }
        }
        if let Some((redef_idx, redef_op)) =
            Self::first_output_redefinition_in_block(block, op_idx, output)
        {
            return AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownMalformedDefUseWindow,
                None,
                Some(redef_idx),
                Some(redef_op.opcode),
            );
        }
        AliasUnsafeHazard::new(
            AliasUnsafeHazardKind::UnknownNoConsumerFound,
            None,
            None,
            None,
        )
    }

    fn materialize_expr_contains_load(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Load { .. } => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::materialize_expr_contains_load(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::materialize_expr_contains_load(lhs)
                    || Self::materialize_expr_contains_load(rhs)
            }
            HirExpr::PtrOffset { base, .. } => Self::materialize_expr_contains_load(base),
            HirExpr::Index { base, index, .. } => {
                Self::materialize_expr_contains_load(base)
                    || Self::materialize_expr_contains_load(index)
            }
            HirExpr::AggregateCopy { src, .. } => Self::materialize_expr_contains_load(src),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::materialize_expr_contains_load(cond)
                    || Self::materialize_expr_contains_load(then_expr)
                    || Self::materialize_expr_contains_load(else_expr)
            }
            HirExpr::Call { .. }
            | HirExpr::Var(_)
            | HirExpr::AddressOfGlobal(_)
            | HirExpr::Const(_, _) => false,
        }
    }

    fn materialize_expr_contains_call(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Call { .. } => true,
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::materialize_expr_contains_call(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                Self::materialize_expr_contains_call(lhs)
                    || Self::materialize_expr_contains_call(rhs)
            }
            HirExpr::Load { ptr, .. } => Self::materialize_expr_contains_call(ptr),
            HirExpr::PtrOffset { base, .. } => Self::materialize_expr_contains_call(base),
            HirExpr::Index { base, index, .. } => {
                Self::materialize_expr_contains_call(base)
                    || Self::materialize_expr_contains_call(index)
            }
            HirExpr::AggregateCopy { src, .. } => Self::materialize_expr_contains_call(src),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                Self::materialize_expr_contains_call(cond)
                    || Self::materialize_expr_contains_call(then_expr)
                    || Self::materialize_expr_contains_call(else_expr)
            }
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        }
    }

    fn first_call_expr_in_materialize_expr<'b>(
        expr: &'b HirExpr,
    ) -> Option<(&'b str, &'b [HirExpr])> {
        match expr {
            HirExpr::Call { target, args, .. } => Some((target.as_str(), args.as_slice())),
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::first_call_expr_in_materialize_expr(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => Self::first_call_expr_in_materialize_expr(lhs)
                .or_else(|| Self::first_call_expr_in_materialize_expr(rhs)),
            HirExpr::Load { ptr, .. } => Self::first_call_expr_in_materialize_expr(ptr),
            HirExpr::PtrOffset { base, .. } => Self::first_call_expr_in_materialize_expr(base),
            HirExpr::Index { base, index, .. } => Self::first_call_expr_in_materialize_expr(base)
                .or_else(|| Self::first_call_expr_in_materialize_expr(index)),
            HirExpr::AggregateCopy { src, .. } => Self::first_call_expr_in_materialize_expr(src),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => Self::first_call_expr_in_materialize_expr(cond)
                .or_else(|| Self::first_call_expr_in_materialize_expr(then_expr))
                .or_else(|| Self::first_call_expr_in_materialize_expr(else_expr)),
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => None,
        }
    }

    fn first_load_expr_in_materialize_expr<'b>(expr: &'b HirExpr) -> Option<&'b HirExpr> {
        match expr {
            HirExpr::Load { ptr, .. } => Some(ptr.as_ref()),
            HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
                Self::first_load_expr_in_materialize_expr(expr)
            }
            HirExpr::Binary { lhs, rhs, .. } => Self::first_load_expr_in_materialize_expr(lhs)
                .or_else(|| Self::first_load_expr_in_materialize_expr(rhs)),
            HirExpr::PtrOffset { base, .. } => Self::first_load_expr_in_materialize_expr(base),
            HirExpr::Index { base, index, .. } => Self::first_load_expr_in_materialize_expr(base)
                .or_else(|| Self::first_load_expr_in_materialize_expr(index)),
            HirExpr::AggregateCopy { src, .. } => Self::first_load_expr_in_materialize_expr(src),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => Self::first_load_expr_in_materialize_expr(cond)
                .or_else(|| Self::first_load_expr_in_materialize_expr(then_expr))
                .or_else(|| Self::first_load_expr_in_materialize_expr(else_expr)),
            HirExpr::Call { .. }
            | HirExpr::Var(_)
            | HirExpr::AddressOfGlobal(_)
            | HirExpr::Const(_, _) => None,
        }
    }

    fn materialize_call_target_is_known_pure_intrinsic(target: &str) -> bool {
        matches!(
            target,
            "__popcount" | "__lzcnt" | "__carry" | "__scarry" | "__sborrow"
        ) || target.starts_with("__pcodeop_")
    }

    fn materialize_call_target_is_carry_like_intrinsic(target: &str) -> bool {
        matches!(target, "__carry" | "__scarry")
    }

    pub(super) fn classify_disallowed_single_consumer_rhs_kind(
        rhs: &HirExpr,
    ) -> DisallowedSingleConsumerRhsKind {
        match rhs {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {
                DisallowedSingleConsumerRhsKind::VarOrConst
            }
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                ..
            } => DisallowedSingleConsumerRhsKind::UnaryBoolean,
            HirExpr::Binary { op, .. }
                if matches!(
                    op,
                    HirBinaryOp::LogicalAnd
                        | HirBinaryOp::LogicalOr
                        | HirBinaryOp::Eq
                        | HirBinaryOp::Ne
                        | HirBinaryOp::Lt
                        | HirBinaryOp::Le
                        | HirBinaryOp::SLt
                        | HirBinaryOp::SLe
                ) =>
            {
                DisallowedSingleConsumerRhsKind::BinaryBoolean
            }
            HirExpr::Binary { .. } => DisallowedSingleConsumerRhsKind::Arithmetic,
            HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. } => DisallowedSingleConsumerRhsKind::LoadLike,
            HirExpr::Call { .. } => DisallowedSingleConsumerRhsKind::CallLike,
            HirExpr::Cast { expr, .. } => Self::classify_disallowed_single_consumer_rhs_kind(expr),
            HirExpr::Unary { .. } => DisallowedSingleConsumerRhsKind::Other,
            HirExpr::Select { .. } => DisallowedSingleConsumerRhsKind::Other,
        }
    }

    pub(super) fn classify_disallowed_single_consumer_kind(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> DisallowedSingleConsumerConsumerKind {
        match use_op.opcode {
            PcodeOpcode::CBranch | PcodeOpcode::BranchInd => {
                DisallowedSingleConsumerConsumerKind::BranchCondition
            }
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                if matched_inputs.iter().any(|idx| *idx >= 1) {
                    DisallowedSingleConsumerConsumerKind::CallArg
                } else {
                    DisallowedSingleConsumerConsumerKind::UnknownConsumerKind
                }
            }
            PcodeOpcode::Store => {
                if matched_inputs.contains(&1) {
                    DisallowedSingleConsumerConsumerKind::StoreAddr
                } else if matched_inputs.contains(&2) {
                    DisallowedSingleConsumerConsumerKind::StoreValue
                } else {
                    DisallowedSingleConsumerConsumerKind::UnknownConsumerKind
                }
            }
            PcodeOpcode::Load => DisallowedSingleConsumerConsumerKind::LoadAddr,
            PcodeOpcode::MultiEqual => DisallowedSingleConsumerConsumerKind::PhiMerge,
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr => DisallowedSingleConsumerConsumerKind::Predicate,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::PopCount
            | PcodeOpcode::LzCount
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub => DisallowedSingleConsumerConsumerKind::OtherData,
            _ => DisallowedSingleConsumerConsumerKind::UnknownConsumerKind,
        }
    }

    fn classify_unknown_consumer_kind_reason(
        opcode: PcodeOpcode,
        matched_inputs: &[usize],
    ) -> UnknownConsumerKindReason {
        if matched_inputs.len() > 1 {
            return UnknownConsumerKindReason::ConsumerHasMultipleMatchedInputs;
        }
        match opcode {
            PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                UnknownConsumerKindReason::ConsumerIsIndirectUse
            }
            PcodeOpcode::Branch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::CBranch
            | PcodeOpcode::Return => UnknownConsumerKindReason::ConsumerIsControlLike,
            PcodeOpcode::PtrAdd | PcodeOpcode::PtrSub => {
                UnknownConsumerKindReason::ConsumerIsAddressComputation
            }
            PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => UnknownConsumerKindReason::ConsumerIsSubpieceOrCast,
            PcodeOpcode::Call | PcodeOpcode::Store => {
                UnknownConsumerKindReason::ConsumerInputRoleUnknown
            }
            PcodeOpcode::Copy
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::Load
            | PcodeOpcode::MultiEqual => UnknownConsumerKindReason::ConsumerOpcodeUnhandled,
            _ => UnknownConsumerKindReason::Unknown,
        }
    }

    fn classify_popcount_result_use_family(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> PopCountResultUseFamily {
        match use_op.opcode {
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual => {
                if use_op.inputs.len() != 2 {
                    return PopCountResultUseFamily::UnknownPopCountUse;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                let other_input = if lhs_matches && !rhs_matches {
                    use_op.inputs.get(1)
                } else if rhs_matches && !lhs_matches {
                    use_op.inputs.first()
                } else {
                    None
                };
                match other_input {
                    Some(input) if input.is_constant && input.constant_val == 0 => {
                        PopCountResultUseFamily::PopCountFeedsCompareZero
                    }
                    Some(input) if input.is_constant => {
                        PopCountResultUseFamily::PopCountFeedsCompareConst
                    }
                    Some(_) => PopCountResultUseFamily::PopCountFeedsPredicate,
                    None => PopCountResultUseFamily::UnknownPopCountUse,
                }
            }
            PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::CBranch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual => PopCountResultUseFamily::PopCountFeedsPredicate,
            PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Store
            | PcodeOpcode::Load => PopCountResultUseFamily::PopCountFeedsStoreOrCall,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::PopCount => PopCountResultUseFamily::PopCountFeedsArithmetic,
            _ => PopCountResultUseFamily::UnknownPopCountUse,
        }
    }

    fn classify_popcount_intand_mask_kind(mask: Option<u64>) -> PopCountIntAndMaskKind {
        match mask {
            Some(1) => PopCountIntAndMaskKind::AndOne,
            Some(0xff) => PopCountIntAndMaskKind::AndByteMask,
            Some(value) if value > 1 && (value + 1).is_power_of_two() => {
                PopCountIntAndMaskKind::AndPowerOfTwoMinusOne
            }
            Some(_) => PopCountIntAndMaskKind::AndNonPowerOfTwoMask,
            None => PopCountIntAndMaskKind::UnknownMask,
        }
    }

    fn classify_popcount_intand_downstream_use_family(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> PopCountIntAndDownstreamUseFamily {
        match use_op.opcode {
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual => {
                if use_op.inputs.len() != 2 {
                    return PopCountIntAndDownstreamUseFamily::FeedsUnknown;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                let other_input = if lhs_matches && !rhs_matches {
                    use_op.inputs.get(1)
                } else if rhs_matches && !lhs_matches {
                    use_op.inputs.first()
                } else {
                    None
                };
                match other_input {
                    Some(input) if input.is_constant && input.constant_val == 0 => {
                        PopCountIntAndDownstreamUseFamily::FeedsCompareZero
                    }
                    Some(input) if input.is_constant => {
                        PopCountIntAndDownstreamUseFamily::FeedsCompareConst
                    }
                    Some(_) => PopCountIntAndDownstreamUseFamily::FeedsPredicate,
                    None => PopCountIntAndDownstreamUseFamily::FeedsUnknown,
                }
            }
            PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::CBranch
            | PcodeOpcode::BranchInd
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual => PopCountIntAndDownstreamUseFamily::FeedsPredicate,
            PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Store
            | PcodeOpcode::Load => PopCountIntAndDownstreamUseFamily::FeedsStoreOrCall,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::PopCount
            | PcodeOpcode::MultiEqual => PopCountIntAndDownstreamUseFamily::FeedsArithmetic,
            _ => PopCountIntAndDownstreamUseFamily::FeedsUnknown,
        }
    }

    pub(super) fn parity_chain_materialization_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_PARITY_CHAIN_MATERIALIZATION"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    pub(super) fn stack_addr_frame_stable_replacement_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_STACK_ADDR_FRAME_STABLE_REPLACEMENT"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    fn classify_single_consumer_predicate_family(expr: &HirExpr) -> SingleConsumerPredicateFamily {
        match expr {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => {
                SingleConsumerPredicateFamily::DirectFlag
            }
            HirExpr::Cast { expr, .. } => Self::classify_single_consumer_predicate_family(expr),
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                ..
            } => SingleConsumerPredicateFamily::NegatedFlag,
            HirExpr::Unary { .. } => SingleConsumerPredicateFamily::UnknownPredicate,
            HirExpr::Binary { op, lhs, rhs, .. } => match op {
                HirBinaryOp::Eq => {
                    if matches!(&**lhs, HirExpr::Const(0, _))
                        || matches!(&**rhs, HirExpr::Const(0, _))
                    {
                        SingleConsumerPredicateFamily::CompareZero
                    } else if matches!(&**lhs, HirExpr::Const(_, _))
                        || matches!(&**rhs, HirExpr::Const(_, _))
                    {
                        SingleConsumerPredicateFamily::CompareConst
                    } else {
                        SingleConsumerPredicateFamily::CompareOtherVar
                    }
                }
                HirBinaryOp::Ne => {
                    if matches!(&**lhs, HirExpr::Const(0, _))
                        || matches!(&**rhs, HirExpr::Const(0, _))
                    {
                        SingleConsumerPredicateFamily::CompareNonZero
                    } else if matches!(&**lhs, HirExpr::Const(_, _))
                        || matches!(&**rhs, HirExpr::Const(_, _))
                    {
                        SingleConsumerPredicateFamily::CompareConst
                    } else {
                        SingleConsumerPredicateFamily::CompareOtherVar
                    }
                }
                HirBinaryOp::LogicalAnd
                | HirBinaryOp::LogicalOr
                | HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe => SingleConsumerPredicateFamily::ComposedPredicate,
                _ => SingleConsumerPredicateFamily::UnknownPredicate,
            },
            HirExpr::Call { .. }
            | HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. }
            | HirExpr::Select { .. }
            | HirExpr::Const(_, _) => SingleConsumerPredicateFamily::UnknownPredicate,
        }
    }

    fn classify_single_consumer_guard_family(
        use_op: &PcodeOp,
        matched_inputs: &[usize],
    ) -> SingleConsumerPredicateFamily {
        match use_op.opcode {
            PcodeOpcode::BoolNegate if matched_inputs == [0] => {
                SingleConsumerPredicateFamily::NegatedFlag
            }
            PcodeOpcode::IntEqual => {
                if use_op.inputs.len() != 2 {
                    return SingleConsumerPredicateFamily::UnknownPredicate;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                if lhs_matches && use_op.inputs[1].is_constant && use_op.inputs[1].constant_val == 0
                    || rhs_matches
                        && use_op.inputs[0].is_constant
                        && use_op.inputs[0].constant_val == 0
                {
                    SingleConsumerPredicateFamily::CompareZero
                } else if lhs_matches && use_op.inputs[1].is_constant
                    || rhs_matches && use_op.inputs[0].is_constant
                {
                    SingleConsumerPredicateFamily::CompareConst
                } else if lhs_matches || rhs_matches {
                    SingleConsumerPredicateFamily::CompareOtherVar
                } else {
                    SingleConsumerPredicateFamily::UnknownPredicate
                }
            }
            PcodeOpcode::IntNotEqual => {
                if use_op.inputs.len() != 2 {
                    return SingleConsumerPredicateFamily::UnknownPredicate;
                }
                let lhs_matches = matched_inputs.contains(&0);
                let rhs_matches = matched_inputs.contains(&1);
                if lhs_matches && use_op.inputs[1].is_constant && use_op.inputs[1].constant_val == 0
                    || rhs_matches
                        && use_op.inputs[0].is_constant
                        && use_op.inputs[0].constant_val == 0
                {
                    SingleConsumerPredicateFamily::CompareNonZero
                } else if lhs_matches && use_op.inputs[1].is_constant
                    || rhs_matches && use_op.inputs[0].is_constant
                {
                    SingleConsumerPredicateFamily::CompareConst
                } else if lhs_matches || rhs_matches {
                    SingleConsumerPredicateFamily::CompareOtherVar
                } else {
                    SingleConsumerPredicateFamily::UnknownPredicate
                }
            }
            PcodeOpcode::BoolAnd | PcodeOpcode::BoolOr | PcodeOpcode::BoolXor => matched_inputs
                .iter()
                .any(|idx| *idx < use_op.inputs.len())
                .then_some(SingleConsumerPredicateFamily::ComposedPredicate)
                .unwrap_or(SingleConsumerPredicateFamily::UnknownPredicate),
            PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual => matched_inputs
                .iter()
                .any(|idx| *idx < use_op.inputs.len())
                .then_some(SingleConsumerPredicateFamily::ComposedPredicate)
                .unwrap_or(SingleConsumerPredicateFamily::UnknownPredicate),
            _ => SingleConsumerPredicateFamily::UnknownPredicate,
        }
    }

    fn predicate_families_match(
        predicate_family: SingleConsumerPredicateFamily,
        guard_family: SingleConsumerPredicateFamily,
    ) -> bool {
        predicate_family == guard_family
    }

    fn classify_arithmetic_predicate_shape(
        expr: &HirExpr,
    ) -> (ArithmeticPredicateShape, Option<u64>) {
        match expr {
            HirExpr::Cast { expr, .. } => Self::classify_arithmetic_predicate_shape(expr),
            HirExpr::Binary { op, lhs, rhs, .. } if matches!(op, HirBinaryOp::And) => {
                let (value_expr, mask_value) = match (&**lhs, &**rhs) {
                    (HirExpr::Const(value, _), other) if *value >= 0 => {
                        (other, Some(*value as u64))
                    }
                    (other, HirExpr::Const(value, _)) if *value >= 0 => {
                        (other, Some(*value as u64))
                    }
                    _ => return (ArithmeticPredicateShape::UnknownArithmetic, None),
                };
                let Some(mask_value) = mask_value else {
                    return (ArithmeticPredicateShape::UnknownArithmetic, None);
                };
                let shape = if matches!(
                    value_expr,
                    HirExpr::Binary {
                        op: HirBinaryOp::Shr | HirBinaryOp::Sar | HirBinaryOp::Shl,
                        ..
                    }
                ) {
                    ArithmeticPredicateShape::ShiftAndMask
                } else if mask_value == 1 {
                    ArithmeticPredicateShape::LowBitAndOne
                } else if mask_value.is_power_of_two() {
                    ArithmeticPredicateShape::PowerOfTwoMask
                } else {
                    ArithmeticPredicateShape::NonPowerOfTwoMask
                };
                (shape, Some(mask_value))
            }
            _ => (ArithmeticPredicateShape::UnknownArithmetic, None),
        }
    }

    fn classify_arithmetic_predicate_stable_reason(
        proof: &SingleConsumerPredicateProof,
        shape: ArithmeticPredicateShape,
    ) -> Option<ArithmeticPredicateStableReason> {
        if !proof.requires_stable_representative {
            return None;
        }
        if shape != ArithmeticPredicateShape::UnknownArithmetic {
            return Some(ArithmeticPredicateStableReason::ArithmeticMask);
        }
        if !proof.same_guard_as_consumer {
            return Some(ArithmeticPredicateStableReason::NonCanonicalPredicate);
        }
        if matches!(
            proof.guard_family,
            SingleConsumerPredicateFamily::CompareZero
                | SingleConsumerPredicateFamily::CompareNonZero
                | SingleConsumerPredicateFamily::CompareConst
                | SingleConsumerPredicateFamily::CompareOtherVar
        ) {
            return Some(ArithmeticPredicateStableReason::ConsumerCompare);
        }
        Some(ArithmeticPredicateStableReason::PredicateSensitive)
    }

    fn low_bit_mask_input_expr<'b>(expr: &'b HirExpr) -> Option<&'b HirExpr> {
        match expr {
            HirExpr::Cast { expr, .. } => Self::low_bit_mask_input_expr(expr),
            HirExpr::Binary { op, lhs, rhs, .. } if matches!(op, HirBinaryOp::And) => {
                match (&**lhs, &**rhs) {
                    (HirExpr::Const(1, _), other) | (other, HirExpr::Const(1, _)) => Some(other),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn expr_boolean_like(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Const(_, ty) => {
                matches!(ty, NirType::Bool) || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
            }
            HirExpr::Cast { ty, expr } => match ty {
                NirType::Bool => true,
                NirType::Int { bits, .. } if *bits == 1 => true,
                _ => Self::expr_boolean_like(expr),
            },
            HirExpr::Unary { op, ty, expr } => match op {
                HirUnaryOp::Not => true,
                _ => {
                    matches!(ty, NirType::Bool)
                        || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
                        || Self::expr_boolean_like(expr)
                }
            },
            HirExpr::Binary { op, ty, .. } => {
                matches!(
                    op,
                    HirBinaryOp::LogicalAnd
                        | HirBinaryOp::LogicalOr
                        | HirBinaryOp::Eq
                        | HirBinaryOp::Ne
                        | HirBinaryOp::Lt
                        | HirBinaryOp::Le
                        | HirBinaryOp::SLt
                        | HirBinaryOp::SLe
                ) || matches!(ty, NirType::Bool)
                    || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
            }
            HirExpr::Call { ty, .. } | HirExpr::Load { ty, .. } => {
                matches!(ty, NirType::Bool) || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
            }
            HirExpr::Select { ty, .. } => {
                matches!(ty, NirType::Bool) || matches!(ty, NirType::Int { bits, .. } if *bits == 1)
            }
            HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. }
            | HirExpr::Var(_)
            | HirExpr::AddressOfGlobal(_) => false,
        }
    }

    fn classify_low_bit_mask_input_origin_kind(expr: &HirExpr) -> LowBitMaskInputOriginKind {
        match expr {
            HirExpr::Cast { expr, .. } => Self::classify_low_bit_mask_input_origin_kind(expr),
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                ..
            } => LowBitMaskInputOriginKind::BoolOp,
            HirExpr::Unary { .. } => LowBitMaskInputOriginKind::Unknown,
            HirExpr::Binary { op, .. } => match op {
                HirBinaryOp::Eq
                | HirBinaryOp::Ne
                | HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe => LowBitMaskInputOriginKind::Compare,
                HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr => {
                    LowBitMaskInputOriginKind::BoolOp
                }
                HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Xor
                | HirBinaryOp::Add
                | HirBinaryOp::Sub
                | HirBinaryOp::Mul
                | HirBinaryOp::Div
                | HirBinaryOp::Mod
                | HirBinaryOp::Shl
                | HirBinaryOp::Shr
                | HirBinaryOp::Sar => LowBitMaskInputOriginKind::Arithmetic,
            },
            HirExpr::Load { .. } | HirExpr::PtrOffset { .. } | HirExpr::Index { .. } => {
                LowBitMaskInputOriginKind::Load
            }
            HirExpr::Call { .. } => LowBitMaskInputOriginKind::Call,
            HirExpr::AggregateCopy { .. }
            | HirExpr::Select { .. }
            | HirExpr::Var(_)
            | HirExpr::AddressOfGlobal(_)
            | HirExpr::Const(_, _) => LowBitMaskInputOriginKind::Unknown,
        }
    }

    fn classify_low_bit_mask_predicate_family(
        input_origin_kind: LowBitMaskInputOriginKind,
        input_is_boolean_like: bool,
    ) -> LowBitMaskPredicateFamily {
        match (input_origin_kind, input_is_boolean_like) {
            (LowBitMaskInputOriginKind::Compare, _) => {
                LowBitMaskPredicateFamily::MaskFromCompareResult
            }
            (LowBitMaskInputOriginKind::BoolOp, true) => LowBitMaskPredicateFamily::BooleanFlagMask,
            (LowBitMaskInputOriginKind::Arithmetic, _) => {
                LowBitMaskPredicateFamily::MaskFromArithmeticValue
            }
            (LowBitMaskInputOriginKind::Load, false)
            | (LowBitMaskInputOriginKind::Call, false)
            | (LowBitMaskInputOriginKind::Unknown, false) => {
                LowBitMaskPredicateFamily::IntegerBitTest
            }
            _ => LowBitMaskPredicateFamily::UnknownLowBitMask,
        }
    }

    pub(super) fn describe_disallowed_single_consumer_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<DisallowedSingleConsumerProof> {
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (_, use_op) = *uses.first()?;
        if uses.len() != 1 {
            return None;
        }
        let output_key = VarnodeKey::from(output);
        let matched_inputs = use_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| (VarnodeKey::from(input) == output_key).then_some(idx))
            .collect::<Vec<_>>();
        let consumer_kind = Self::classify_disallowed_single_consumer_kind(use_op, &matched_inputs);
        let rhs_kind = Self::classify_disallowed_single_consumer_rhs_kind(rhs);
        let rhs_has_call = Self::materialize_expr_contains_call(rhs);
        let rhs_has_load = Self::materialize_expr_contains_load(rhs);
        let rhs_low_cost = Self::expr_is_low_cost_builder_inline_candidate(rhs);
        let reason = if rhs_has_call {
            DisallowedSingleConsumerReason::RhsHasCall
        } else if rhs_has_load {
            DisallowedSingleConsumerReason::RhsHasLoad
        } else if !rhs_low_cost {
            DisallowedSingleConsumerReason::RhsNotLowCost
        } else {
            match consumer_kind {
                DisallowedSingleConsumerConsumerKind::BranchCondition => {
                    DisallowedSingleConsumerReason::ConsumerIsBranchCondition
                }
                DisallowedSingleConsumerConsumerKind::Predicate => {
                    DisallowedSingleConsumerReason::ConsumerIsPredicate
                }
                DisallowedSingleConsumerConsumerKind::CallArg => {
                    DisallowedSingleConsumerReason::ConsumerIsCallArg
                }
                DisallowedSingleConsumerConsumerKind::StoreAddr => {
                    DisallowedSingleConsumerReason::ConsumerIsStoreAddr
                }
                DisallowedSingleConsumerConsumerKind::StoreValue => {
                    DisallowedSingleConsumerReason::ConsumerIsStoreValue
                }
                DisallowedSingleConsumerConsumerKind::LoadAddr => {
                    DisallowedSingleConsumerReason::ConsumerIsLoadAddr
                }
                DisallowedSingleConsumerConsumerKind::PhiMerge => {
                    DisallowedSingleConsumerReason::ConsumerIsPhiMerge
                }
                DisallowedSingleConsumerConsumerKind::OtherData
                | DisallowedSingleConsumerConsumerKind::UnknownConsumerKind => {
                    DisallowedSingleConsumerReason::UnknownConsumerKind
                }
            }
        };
        Some(DisallowedSingleConsumerProof {
            consumer_block_addr: block.start_address,
            consumer_op_seq: use_op.seq_num,
            consumer_opcode: use_op.opcode,
            matched_input_indices: matched_inputs,
            consumer_kind,
            rhs_kind,
            rhs_low_cost,
            rhs_has_load,
            rhs_has_call,
            reason,
        })
    }

    pub(super) fn describe_unknown_consumer_kind_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<UnknownConsumerKindProof> {
        let base = Self::describe_disallowed_single_consumer_proof(block, op_idx, output, rhs)?;
        if base.reason != DisallowedSingleConsumerReason::UnknownConsumerKind {
            return None;
        }
        Some(UnknownConsumerKindProof {
            consumer_block_addr: base.consumer_block_addr,
            consumer_op_seq: base.consumer_op_seq,
            consumer_opcode: base.consumer_opcode,
            matched_input_indices: base.matched_input_indices.clone(),
            rhs_kind: base.rhs_kind,
            reason: Self::classify_unknown_consumer_kind_reason(
                base.consumer_opcode,
                &base.matched_input_indices,
            ),
        })
    }

    pub(super) fn describe_single_consumer_call_rhs_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<SingleConsumerCallRhsProof> {
        let base = Self::describe_disallowed_single_consumer_proof(block, op_idx, output, rhs)?;
        if base.reason != DisallowedSingleConsumerReason::RhsHasCall {
            return None;
        }
        let (call_target, _args) = Self::first_call_expr_in_materialize_expr(rhs)?;
        let effect_summary = self
            .type_context
            .and_then(|ctx| ctx.call_effect_summaries.get(call_target));
        let effect_source = effect_summary.and_then(|summary| summary.source);
        let writes_memory = effect_summary.and_then(|summary| summary.writes_memory);
        let may_call_unknown = effect_summary.and_then(|summary| summary.may_call_unknown);
        let may_exit = effect_summary.and_then(|summary| summary.may_exit);
        let import_call = crate::nir::normalize::is_known_api_signature(call_target);
        let internal_target = crate::nir::types::parse_call_target_address(call_target).is_some();
        let defining_opcode = block.ops.get(op_idx).map(|op| op.opcode);
        let preview_unsafe = effect_source == Some(CallEffectSummarySource::PreviewCalleeAnalysis)
            && (writes_memory == Some(true)
                || may_call_unknown == Some(true)
                || may_exit == Some(true));
        let family = if Self::materialize_call_target_is_known_pure_intrinsic(call_target) {
            SingleConsumerCallRhsFamily::KnownPureIntrinsic
        } else if preview_unsafe {
            SingleConsumerCallRhsFamily::PreviewCalleeAnalysisUnsafe
        } else if import_call {
            SingleConsumerCallRhsFamily::ImportCall
        } else if defining_opcode == Some(PcodeOpcode::CallOther) {
            SingleConsumerCallRhsFamily::CallOther
        } else if defining_opcode == Some(PcodeOpcode::CallInd) {
            SingleConsumerCallRhsFamily::IndirectCall
        } else if internal_target {
            SingleConsumerCallRhsFamily::UnknownInternalCall
        } else {
            SingleConsumerCallRhsFamily::UnknownCall
        };

        Some(SingleConsumerCallRhsProof {
            consumer_block_addr: base.consumer_block_addr,
            consumer_op_seq: base.consumer_op_seq,
            consumer_opcode: base.consumer_opcode,
            consumer_kind: base.consumer_kind,
            call_target: call_target.to_string(),
            family,
            rhs_low_cost: base.rhs_low_cost,
            call_effect_source: effect_source,
            writes_memory,
            may_call_unknown,
            may_exit,
            return_used: true,
            downstream_opcode: Some(base.consumer_opcode),
        })
    }

    fn classify_carry_intrinsic_predicate_use_family(
        consumer_op: &PcodeOp,
    ) -> CarryIntrinsicPredicateUseFamily {
        match consumer_op.opcode {
            PcodeOpcode::BoolOr => CarryIntrinsicPredicateUseFamily::CarryFeedsBoolOr,
            PcodeOpcode::IntEqual => {
                let compare_const_zero = consumer_op
                    .inputs
                    .iter()
                    .any(|input| input.is_constant && input.constant_val == 0);
                if compare_const_zero {
                    CarryIntrinsicPredicateUseFamily::CarryFeedsCompareZero
                } else {
                    CarryIntrinsicPredicateUseFamily::CarryFeedsUnknown
                }
            }
            PcodeOpcode::IntNotEqual => {
                let compare_const_zero = consumer_op
                    .inputs
                    .iter()
                    .any(|input| input.is_constant && input.constant_val == 0);
                if compare_const_zero {
                    CarryIntrinsicPredicateUseFamily::CarryFeedsCompareNonZero
                } else {
                    CarryIntrinsicPredicateUseFamily::CarryFeedsUnknown
                }
            }
            PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor => CarryIntrinsicPredicateUseFamily::CarryFeedsArithmetic,
            _ => CarryIntrinsicPredicateUseFamily::CarryFeedsUnknown,
        }
    }

    fn classify_boolor_downstream_use_family(use_op: &PcodeOp) -> BoolOrDownstreamUseFamily {
        match use_op.opcode {
            PcodeOpcode::CBranch | PcodeOpcode::BranchInd => {
                BoolOrDownstreamUseFamily::BoolOrFeedsBranch
            }
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual => {
                BoolOrDownstreamUseFamily::BoolOrFeedsCompare
            }
            PcodeOpcode::BoolAnd | PcodeOpcode::BoolOr | PcodeOpcode::BoolXor => {
                BoolOrDownstreamUseFamily::BoolOrFeedsPredicate
            }
            PcodeOpcode::Store
            | PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther => BoolOrDownstreamUseFamily::BoolOrFeedsData,
            _ => BoolOrDownstreamUseFamily::UnknownBoolOrUse,
        }
    }

    pub(super) fn describe_carry_intrinsic_predicate_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<CarryIntrinsicPredicateProof> {
        let base = self.describe_single_consumer_call_rhs_proof(block, op_idx, output, rhs)?;
        if base.family != SingleConsumerCallRhsFamily::KnownPureIntrinsic
            || base.consumer_kind != DisallowedSingleConsumerConsumerKind::Predicate
            || !Self::materialize_call_target_is_carry_like_intrinsic(&base.call_target)
        {
            return None;
        }
        let (_, args) = Self::first_call_expr_in_materialize_expr(rhs)?;
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (consumer_idx, consumer_op) = *uses.first()?;
        let bool_chain_role = Self::classify_carry_intrinsic_predicate_use_family(consumer_op);
        let args_side_effect_free = args.iter().all(|arg| {
            !Self::materialize_expr_contains_call(arg)
                && !Self::materialize_expr_contains_load(arg)
                && !Self::expr_is_side_effectful_for_materialization_trace(arg)
        });
        let (final_predicate_context, boolor_downstream_use) =
            if consumer_op.opcode == PcodeOpcode::BoolOr {
                if let Some(boolor_output) = consumer_op.output.as_ref() {
                    let downstream_uses =
                        Self::collect_output_use_sites_in_block(block, consumer_idx, boolor_output);
                    let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
                        &self.pcode.blocks,
                        block.start_address,
                        boolor_output,
                    );
                    if downstream_uses.is_empty() && cross_block_consumers == 0 {
                        (CarryIntrinsicFinalPredicateContext::BoolOrOnly, None)
                    } else if downstream_uses.len() == 1 && cross_block_consumers == 0 {
                        let (_, downstream_op) = downstream_uses[0];
                        let family = Self::classify_boolor_downstream_use_family(downstream_op);
                        let context = match family {
                            BoolOrDownstreamUseFamily::BoolOrFeedsBranch => {
                                CarryIntrinsicFinalPredicateContext::BranchPredicate
                            }
                            BoolOrDownstreamUseFamily::BoolOrFeedsCompare => {
                                let compare_zero = downstream_op
                                    .inputs
                                    .iter()
                                    .any(|input| input.is_constant && input.constant_val == 0);
                                if compare_zero {
                                    CarryIntrinsicFinalPredicateContext::CompareZero
                                } else {
                                    CarryIntrinsicFinalPredicateContext::Unknown
                                }
                            }
                            BoolOrDownstreamUseFamily::BoolOrFeedsPredicate => {
                                CarryIntrinsicFinalPredicateContext::PredicateChain
                            }
                            BoolOrDownstreamUseFamily::BoolOrFeedsData
                            | BoolOrDownstreamUseFamily::UnknownBoolOrUse => {
                                CarryIntrinsicFinalPredicateContext::Unknown
                            }
                        };
                        (context, Some(family))
                    } else {
                        (CarryIntrinsicFinalPredicateContext::Unknown, None)
                    }
                } else {
                    (CarryIntrinsicFinalPredicateContext::Unknown, None)
                }
            } else {
                let context = match bool_chain_role {
                    CarryIntrinsicPredicateUseFamily::CarryFeedsCompareZero => {
                        CarryIntrinsicFinalPredicateContext::CompareZero
                    }
                    CarryIntrinsicPredicateUseFamily::CarryFeedsCompareNonZero => {
                        CarryIntrinsicFinalPredicateContext::CompareNonZero
                    }
                    _ => CarryIntrinsicFinalPredicateContext::Unknown,
                };
                (context, None)
            };

        Some(CarryIntrinsicPredicateProof {
            call_target: base.call_target,
            args: args.iter().map(|arg| format!("{arg:?}")).collect(),
            consumer_kind: base.consumer_kind,
            downstream_opcode: base.downstream_opcode.unwrap_or(base.consumer_opcode),
            bool_chain_role,
            rhs_low_cost: base.rhs_low_cost,
            args_side_effect_free,
            final_predicate_context,
            boolor_downstream_use,
        })
    }

    fn classify_intrinsic_compare_only_family(
        call_target: &str,
        compare_const: Option<i64>,
    ) -> IntrinsicCompareOnlyFamily {
        match (call_target, compare_const) {
            ("__sborrow" | "__borrow", Some(0)) => IntrinsicCompareOnlyFamily::BorrowCompareZero,
            ("__carry", Some(0)) => IntrinsicCompareOnlyFamily::CarryCompareZero,
            ("__scarry", Some(0)) => IntrinsicCompareOnlyFamily::SignedCarryCompareZero,
            ("__popcount", Some(0)) => IntrinsicCompareOnlyFamily::PopCountCompareZero,
            _ => IntrinsicCompareOnlyFamily::UnknownIntrinsicCompare,
        }
    }

    fn classify_intrinsic_compare_final_predicate_context(
        consumer_op: &PcodeOp,
    ) -> IntrinsicCompareFinalPredicateContext {
        let compare_const = consumer_op
            .inputs
            .iter()
            .find(|input| input.is_constant)
            .map(|input| input.constant_val);
        match (consumer_op.opcode, compare_const) {
            (PcodeOpcode::IntEqual, Some(0)) => IntrinsicCompareFinalPredicateContext::CompareZero,
            (PcodeOpcode::IntEqual, Some(1)) => IntrinsicCompareFinalPredicateContext::CompareOne,
            (PcodeOpcode::IntNotEqual, Some(0)) => {
                IntrinsicCompareFinalPredicateContext::CompareNonZero
            }
            _ => IntrinsicCompareFinalPredicateContext::Unknown,
        }
    }

    pub(super) fn describe_intrinsic_compare_only_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<IntrinsicCompareOnlyProof> {
        let base = self.describe_single_consumer_call_rhs_proof(block, op_idx, output, rhs)?;
        if base.family != SingleConsumerCallRhsFamily::KnownPureIntrinsic
            || !matches!(
                base.consumer_opcode,
                PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual
            )
        {
            return None;
        }
        let (_, args) = Self::first_call_expr_in_materialize_expr(rhs)?;
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (_, consumer_op) = *uses.first()?;
        let compare_const = consumer_op
            .inputs
            .iter()
            .find(|input| input.is_constant)
            .map(|input| input.constant_val);
        let args_side_effect_free = args.iter().all(|arg| {
            !Self::materialize_expr_contains_call(arg)
                && !Self::materialize_expr_contains_load(arg)
                && !Self::expr_is_side_effectful_for_materialization_trace(arg)
        });
        Some(IntrinsicCompareOnlyProof {
            call_target: base.call_target.clone(),
            args: args.iter().map(|arg| format!("{arg:?}")).collect(),
            downstream_opcode: consumer_op.opcode,
            compare_const,
            family: Self::classify_intrinsic_compare_only_family(&base.call_target, compare_const),
            rhs_low_cost: base.rhs_low_cost,
            args_side_effect_free,
            final_predicate_context: Self::classify_intrinsic_compare_final_predicate_context(
                consumer_op,
            ),
        })
    }

    fn classify_single_consumer_load_rhs_family(
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        consumer_opcode: PcodeOpcode,
    ) -> SingleConsumerLoadRhsFamily {
        match consumer_kind {
            DisallowedSingleConsumerConsumerKind::Predicate
            | DisallowedSingleConsumerConsumerKind::BranchCondition => {
                SingleConsumerLoadRhsFamily::LoadFeedsPredicate
            }
            DisallowedSingleConsumerConsumerKind::LoadAddr
            | DisallowedSingleConsumerConsumerKind::StoreAddr => {
                SingleConsumerLoadRhsFamily::LoadFeedsAddressComputation
            }
            DisallowedSingleConsumerConsumerKind::StoreValue
            | DisallowedSingleConsumerConsumerKind::CallArg => {
                SingleConsumerLoadRhsFamily::LoadFeedsStoreOrCall
            }
            DisallowedSingleConsumerConsumerKind::OtherData => match consumer_opcode {
                PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntMult
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual => SingleConsumerLoadRhsFamily::LoadFeedsArithmetic,
                _ => SingleConsumerLoadRhsFamily::LoadFeedsUnknown,
            },
            _ => SingleConsumerLoadRhsFamily::LoadFeedsUnknown,
        }
    }

    fn classify_single_consumer_load_alias_class(
        ptr: &HirExpr,
        same_block_store_before: bool,
        same_block_store_after: bool,
        same_block_call_before: bool,
        same_block_call_after: bool,
    ) -> SingleConsumerLoadAliasClass {
        if same_block_store_before || same_block_store_after {
            return SingleConsumerLoadAliasClass::MayAliasSameBlockStore;
        }
        if same_block_call_before || same_block_call_after {
            return SingleConsumerLoadAliasClass::MayAliasCall;
        }
        match ptr {
            HirExpr::Var(name)
                if name == "rsp" || name == "rbp" || name == "sp" || name == "bp" =>
            {
                SingleConsumerLoadAliasClass::ReadOnlyLocalLoad
            }
            HirExpr::PtrOffset { base, .. } => match base.as_ref() {
                HirExpr::Var(name)
                    if name == "rsp" || name == "rbp" || name == "sp" || name == "bp" =>
                {
                    SingleConsumerLoadAliasClass::ReadOnlyLocalLoad
                }
                HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => {
                    SingleConsumerLoadAliasClass::GlobalOrExternalLoad
                }
                HirExpr::Call { .. } | HirExpr::Load { .. } => {
                    SingleConsumerLoadAliasClass::VolatileOrUnknownLoad
                }
                _ => SingleConsumerLoadAliasClass::UnknownLoad,
            },
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => {
                SingleConsumerLoadAliasClass::GlobalOrExternalLoad
            }
            HirExpr::Call { .. } | HirExpr::Load { .. } => {
                SingleConsumerLoadAliasClass::VolatileOrUnknownLoad
            }
            _ => SingleConsumerLoadAliasClass::UnknownLoad,
        }
    }

    pub(super) fn describe_single_consumer_load_rhs_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<SingleConsumerLoadRhsProof> {
        let base = Self::describe_disallowed_single_consumer_proof(block, op_idx, output, rhs)?;
        if base.reason != DisallowedSingleConsumerReason::RhsHasLoad {
            return None;
        }
        let load_ptr = Self::first_load_expr_in_materialize_expr(rhs)?;
        let same_block_store_before = block.ops[..op_idx]
            .iter()
            .any(|op| matches!(op.opcode, PcodeOpcode::Store));
        let same_block_store_after = block.ops[op_idx + 1..]
            .iter()
            .any(|op| matches!(op.opcode, PcodeOpcode::Store));
        let same_block_call_before = block.ops[..op_idx].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
            )
        });
        let same_block_call_after = block.ops[op_idx + 1..].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
            )
        });
        Some(SingleConsumerLoadRhsProof {
            consumer_block_addr: base.consumer_block_addr,
            consumer_op_seq: base.consumer_op_seq,
            consumer_opcode: base.consumer_opcode,
            consumer_kind: base.consumer_kind,
            load_ptr: format!("{load_ptr:?}"),
            family: Self::classify_single_consumer_load_rhs_family(
                base.consumer_kind,
                base.consumer_opcode,
            ),
            alias_class: Self::classify_single_consumer_load_alias_class(
                load_ptr,
                same_block_store_before,
                same_block_store_after,
                same_block_call_before,
                same_block_call_after,
            ),
            same_block_store_before,
            same_block_store_after,
        })
    }

    pub(super) fn describe_popcount_consumer_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<PopCountConsumerProof> {
        let base = Self::describe_unknown_consumer_kind_proof(block, op_idx, output, rhs)?;
        if base.consumer_opcode != PcodeOpcode::PopCount {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (consumer_idx, consumer_op) = *uses.first()?;
        if uses.len() != 1 {
            return None;
        }
        let matched_input = *base.matched_input_indices.first()?;
        let input_width = consumer_op
            .inputs
            .get(matched_input)
            .map(|input| input.size)
            .unwrap_or(output.size);
        let output_width = consumer_op.output.as_ref().map(|vn| vn.size);
        let (popcount_result_used_by, downstream_consumer_opcode) =
            if let Some(popcount_output) = consumer_op.output.as_ref() {
                let downstream_uses =
                    Self::collect_output_use_sites_in_block(block, consumer_idx, popcount_output);
                let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
                    &self.pcode.blocks,
                    block.start_address,
                    popcount_output,
                );
                if downstream_uses.is_empty() && cross_block_consumers == 0 {
                    (PopCountResultUseFamily::PopCountResultUnused, None)
                } else if downstream_uses.len() == 1 && cross_block_consumers == 0 {
                    let (_, downstream_op) = downstream_uses[0];
                    let output_key = VarnodeKey::from(popcount_output);
                    let matched_inputs = downstream_op
                        .inputs
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, input)| {
                            (VarnodeKey::from(input) == output_key).then_some(idx)
                        })
                        .collect::<Vec<_>>();
                    (
                        Self::classify_popcount_result_use_family(downstream_op, &matched_inputs),
                        Some(downstream_op.opcode),
                    )
                } else {
                    (
                        PopCountResultUseFamily::UnknownPopCountUse,
                        downstream_uses.first().map(|(_, op)| op.opcode),
                    )
                }
            } else {
                (PopCountResultUseFamily::PopCountResultUnused, None)
            };
        Some(PopCountConsumerProof {
            consumer_op_seq: consumer_op.seq_num,
            input_width,
            output_width,
            rhs_kind: base.rhs_kind,
            rhs_low_cost: Self::expr_is_low_cost_builder_inline_candidate(rhs),
            rhs_has_call: Self::materialize_expr_contains_call(rhs),
            rhs_has_load: Self::materialize_expr_contains_load(rhs),
            popcount_result_used_by,
            downstream_consumer_opcode,
        })
    }

    pub(super) fn describe_popcount_intand_chain_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<PopCountIntAndChainProof> {
        let popcount = self.describe_popcount_consumer_proof(block, op_idx, output, rhs)?;
        if popcount.downstream_consumer_opcode != Some(PcodeOpcode::IntAnd) {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (consumer_idx, consumer_op) = *uses.first()?;
        let popcount_output = consumer_op.output.as_ref()?;
        let downstream_uses =
            Self::collect_output_use_sites_in_block(block, consumer_idx, popcount_output);
        let (intand_idx, intand_op) = *downstream_uses.first()?;
        if intand_op.opcode != PcodeOpcode::IntAnd {
            return None;
        }
        let popcount_output_key = VarnodeKey::from(popcount_output);
        let matched_inputs = intand_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| {
                (VarnodeKey::from(input) == popcount_output_key).then_some(idx)
            })
            .collect::<Vec<_>>();
        let intand_mask = if matched_inputs == [0] {
            intand_op
                .inputs
                .get(1)
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        } else if matched_inputs == [1] {
            intand_op
                .inputs
                .first()
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        } else {
            None
        };
        let intand_mask_kind = Self::classify_popcount_intand_mask_kind(intand_mask);
        let (intand_result_consumer, downstream_consumer_opcode) = if let Some(intand_output) =
            intand_op.output.as_ref()
        {
            let intand_output_uses =
                Self::collect_output_use_sites_in_block(block, intand_idx, intand_output);
            let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
                &self.pcode.blocks,
                block.start_address,
                intand_output,
            );
            if intand_output_uses.len() == 1 && cross_block_consumers == 0 {
                let (_, use_op) = intand_output_uses[0];
                let intand_output_key = VarnodeKey::from(intand_output);
                let matched_inputs = use_op
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, input)| {
                        (VarnodeKey::from(input) == intand_output_key).then_some(idx)
                    })
                    .collect::<Vec<_>>();
                (
                    Self::classify_popcount_intand_downstream_use_family(use_op, &matched_inputs),
                    Some(use_op.opcode),
                )
            } else {
                (
                    PopCountIntAndDownstreamUseFamily::FeedsUnknown,
                    intand_output_uses.first().map(|(_, op)| op.opcode),
                )
            }
        } else {
            (PopCountIntAndDownstreamUseFamily::FeedsUnknown, None)
        };
        Some(PopCountIntAndChainProof {
            popcount_consumer_op_seq: popcount.consumer_op_seq,
            intand_op_seq: intand_op.seq_num,
            popcount_result: format!(
                "space:{} off:0x{:x} size:{}",
                popcount_output.space_id, popcount_output.offset, popcount_output.size
            ),
            intand_mask,
            intand_mask_kind,
            intand_result_consumer,
            downstream_consumer_opcode,
            chain_low_cost: popcount.rhs_low_cost,
            chain_side_effect_free: !popcount.rhs_has_call && !popcount.rhs_has_load,
        })
    }

    fn extract_popcount_input_expr<'b>(rhs: &'b HirExpr) -> Option<&'b HirExpr> {
        let HirExpr::Call { target, args, .. } = rhs else {
            return None;
        };
        (target == "__popcount" && args.len() == 1).then_some(&args[0])
    }

    fn extract_intand_popcount_operand<'b>(rhs: &'b HirExpr) -> Option<(&'b HirExpr, Option<u64>)> {
        match rhs {
            HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                let lhs_popcount = Self::extract_popcount_input_expr(lhs);
                let rhs_popcount = Self::extract_popcount_input_expr(rhs);
                let lhs_const = match rhs.as_ref() {
                    HirExpr::Const(value, _) => Some(*value as u64),
                    _ => None,
                };
                let rhs_const = match lhs.as_ref() {
                    HirExpr::Const(value, _) => Some(*value as u64),
                    _ => None,
                };
                if let Some(popcount_input) = lhs_popcount {
                    Some((popcount_input, lhs_const))
                } else if let Some(popcount_input) = rhs_popcount {
                    Some((popcount_input, rhs_const))
                } else {
                    None
                }
            }
            _ => Self::extract_popcount_input_expr(rhs).map(|expr| (expr, None)),
        }
    }

    fn range_has_parity_chain_side_effect(
        block: &crate::pcode::PcodeBasicBlock,
        start_idx: usize,
        end_idx: usize,
    ) -> bool {
        if end_idx <= start_idx + 1 {
            return false;
        }
        block.ops[start_idx + 1..end_idx].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Store
                    | PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }

    fn classify_parity_chain_compare<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        producer_idx: usize,
        output: &Varnode,
    ) -> Result<(usize, &'b PcodeOp, u64), ParityChainKeepReason> {
        let uses = Self::collect_output_use_sites_in_block(block, producer_idx, output);
        let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
            &self.pcode.blocks,
            block.start_address,
            output,
        );
        if uses.len() != 1 || cross_block_consumers != 0 {
            return Err(ParityChainKeepReason::IntAndHasMultipleConsumers);
        }
        let (compare_idx, compare_op) = uses[0];
        if !matches!(
            compare_op.opcode,
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual
        ) {
            return Err(ParityChainKeepReason::FinalConsumerNotCompare);
        }
        let output_key = VarnodeKey::from(output);
        let matched_inputs = compare_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| (VarnodeKey::from(input) == output_key).then_some(idx))
            .collect::<Vec<_>>();
        if matched_inputs.len() != 1 {
            return Err(ParityChainKeepReason::FinalConsumerNotCompare);
        }
        let compare_const = if matched_inputs[0] == 0 {
            compare_op
                .inputs
                .get(1)
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        } else {
            compare_op
                .inputs
                .first()
                .and_then(|input| input.is_constant.then_some(input.constant_val as u64))
        };
        let Some(compare_const) = compare_const else {
            return Err(ParityChainKeepReason::CompareConstUnsupported);
        };
        if !matches!(compare_const, 0 | 1) {
            return Err(ParityChainKeepReason::CompareConstUnsupported);
        }
        Ok((compare_idx, compare_op, compare_const))
    }

    pub(super) fn describe_parity_chain_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<Result<ParityChainProof, ParityChainKeepReason>> {
        let current_op = block.ops.get(op_idx)?;
        match current_op.opcode {
            PcodeOpcode::PopCount => {
                let input_expr = Self::extract_popcount_input_expr(rhs)?;
                if !Self::expr_is_low_cost_builder_inline_candidate(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsNotLowCost));
                }
                if Self::materialize_expr_contains_load(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasLoad));
                }
                if Self::materialize_expr_contains_call(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasCall));
                }
                let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
                let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
                    &self.pcode.blocks,
                    block.start_address,
                    output,
                );
                if uses.len() != 1 || cross_block_consumers != 0 {
                    return Some(Err(ParityChainKeepReason::PopCountHasMultipleConsumers));
                }
                let (intand_idx, intand_op) = uses[0];
                let output_key = VarnodeKey::from(output);
                let matched_inputs = intand_op
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, input)| {
                        (VarnodeKey::from(input) == output_key).then_some(idx)
                    })
                    .collect::<Vec<_>>();
                let intand_mask =
                    if matched_inputs == [0] {
                        intand_op.inputs.get(1).and_then(|input| {
                            input.is_constant.then_some(input.constant_val as u64)
                        })
                    } else if matched_inputs == [1] {
                        intand_op.inputs.first().and_then(|input| {
                            input.is_constant.then_some(input.constant_val as u64)
                        })
                    } else {
                        None
                    };
                if intand_mask != Some(1) {
                    return Some(Err(ParityChainKeepReason::IntAndMaskNotOne));
                }
                if Self::range_has_parity_chain_side_effect(block, op_idx, intand_idx) {
                    return Some(Err(ParityChainKeepReason::InterveningSideEffect));
                }
                let intand_output = intand_op.output.as_ref()?;
                let (compare_idx, compare_op, compare_const) =
                    match self.classify_parity_chain_compare(block, intand_idx, intand_output) {
                        Ok(proof) => proof,
                        Err(reason) => return Some(Err(reason)),
                    };
                if Self::range_has_parity_chain_side_effect(block, intand_idx, compare_idx) {
                    return Some(Err(ParityChainKeepReason::InterveningSideEffect));
                }
                return Some(Ok(ParityChainProof {
                    role: ParityChainRole::PopCountResult,
                    popcount_op_seq: current_op.seq_num,
                    intand_op_seq: block.ops.get(intand_idx)?.seq_num,
                    compare_op_seq: compare_op.seq_num,
                    compare_opcode: compare_op.opcode,
                    compare_const,
                    chain_low_cost: true,
                    chain_side_effect_free: true,
                }));
            }
            PcodeOpcode::IntAnd => {
                let Some((input_expr, mask_value)) = Self::extract_intand_popcount_operand(rhs)
                else {
                    return None;
                };
                if mask_value != Some(1) {
                    return Some(Err(ParityChainKeepReason::IntAndMaskNotOne));
                }
                if !Self::expr_is_low_cost_builder_inline_candidate(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsNotLowCost));
                }
                if Self::materialize_expr_contains_load(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasLoad));
                }
                if Self::materialize_expr_contains_call(input_expr) {
                    return Some(Err(ParityChainKeepReason::RhsHasCall));
                }
                let (compare_idx, compare_op, compare_const) =
                    match self.classify_parity_chain_compare(block, op_idx, output) {
                        Ok(proof) => proof,
                        Err(reason) => return Some(Err(reason)),
                    };
                if Self::range_has_parity_chain_side_effect(block, op_idx, compare_idx) {
                    return Some(Err(ParityChainKeepReason::InterveningSideEffect));
                }
                let intand_op = block.ops.get(op_idx)?;
                let popcount_input = if intand_op
                    .inputs
                    .first()
                    .is_some_and(|input| input.is_constant)
                {
                    intand_op.inputs.get(1)?
                } else {
                    intand_op.inputs.first()?
                };
                let popcount_output_key = VarnodeKey::from(popcount_input);
                let popcount_idx = block
                    .ops
                    .iter()
                    .enumerate()
                    .take(op_idx)
                    .rev()
                    .find(|(_, candidate)| {
                        candidate.output.as_ref().map(VarnodeKey::from)
                            == Some(popcount_output_key.clone())
                    })
                    .map(|(idx, _)| idx)?;
                return Some(Ok(ParityChainProof {
                    role: ParityChainRole::IntAndResult,
                    popcount_op_seq: block.ops.get(popcount_idx)?.seq_num,
                    intand_op_seq: current_op.seq_num,
                    compare_op_seq: compare_op.seq_num,
                    compare_opcode: compare_op.opcode,
                    compare_const,
                    chain_low_cost: true,
                    chain_side_effect_free: true,
                }));
            }
            _ => {}
        }

        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
            &self.pcode.blocks,
            block.start_address,
            output,
        );
        let Some((popcount_idx, popcount_op)) = uses.first().copied() else {
            return None;
        };
        if popcount_op.opcode != PcodeOpcode::PopCount {
            return None;
        }
        if uses.len() != 1 || cross_block_consumers != 0 {
            return Some(Err(ParityChainKeepReason::PopCountHasMultipleConsumers));
        }
        if !Self::expr_is_low_cost_builder_inline_candidate(rhs) {
            return Some(Err(ParityChainKeepReason::RhsNotLowCost));
        }
        if Self::materialize_expr_contains_load(rhs) {
            return Some(Err(ParityChainKeepReason::RhsHasLoad));
        }
        if Self::materialize_expr_contains_call(rhs) {
            return Some(Err(ParityChainKeepReason::RhsHasCall));
        }
        if Self::range_has_parity_chain_side_effect(block, op_idx, popcount_idx) {
            return Some(Err(ParityChainKeepReason::InterveningSideEffect));
        }
        let chain = self.describe_popcount_intand_chain_proof(block, op_idx, output, rhs)?;
        if chain.intand_mask_kind != PopCountIntAndMaskKind::AndOne {
            return Some(Err(ParityChainKeepReason::IntAndMaskNotOne));
        }
        let popcount_output = popcount_op.output.as_ref()?;
        let uses = Self::collect_output_use_sites_in_block(block, popcount_idx, popcount_output);
        let (cross_block_consumers, _) = Self::collect_output_use_sites_outside_block(
            &self.pcode.blocks,
            block.start_address,
            popcount_output,
        );
        if uses.len() != 1 || cross_block_consumers != 0 {
            return Some(Err(ParityChainKeepReason::PopCountHasMultipleConsumers));
        }
        let (intand_idx, _) = uses[0];
        if Self::range_has_parity_chain_side_effect(block, popcount_idx, intand_idx) {
            return Some(Err(ParityChainKeepReason::InterveningSideEffect));
        }
        let intand_output = block.ops.get(intand_idx)?.output.as_ref()?;
        let (compare_idx, compare_op, compare_const) =
            match self.classify_parity_chain_compare(block, intand_idx, intand_output) {
                Ok(proof) => proof,
                Err(reason) => return Some(Err(reason)),
            };
        if Self::range_has_parity_chain_side_effect(block, intand_idx, compare_idx) {
            return Some(Err(ParityChainKeepReason::InterveningSideEffect));
        }
        Some(Ok(ParityChainProof {
            role: ParityChainRole::PopCountInput,
            popcount_op_seq: popcount_op.seq_num,
            intand_op_seq: block.ops.get(intand_idx)?.seq_num,
            compare_op_seq: compare_op.seq_num,
            compare_opcode: compare_op.opcode,
            compare_const,
            chain_low_cost: true,
            chain_side_effect_free: true,
        }))
    }

    pub(super) fn describe_parity_chain_final_hir_expr(
        rhs: &HirExpr,
        proof: &ParityChainProof,
    ) -> Option<String> {
        let input_expr = match proof.role {
            ParityChainRole::PopCountInput => rhs,
            ParityChainRole::PopCountResult => Self::extract_popcount_input_expr(rhs)?,
            ParityChainRole::IntAndResult => Self::extract_intand_popcount_operand(rhs)?.0,
        };
        let compare_op = match proof.compare_opcode {
            PcodeOpcode::IntEqual => "==",
            PcodeOpcode::IntNotEqual => "!=",
            _ => return None,
        };
        Some(format!(
            "((__popcount({input_expr:?}) & 1) {compare_op} {})",
            proof.compare_const
        ))
    }

    pub(super) fn describe_single_consumer_predicate_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<SingleConsumerPredicateProof> {
        let base = Self::describe_disallowed_single_consumer_proof(block, op_idx, output, rhs)?;
        if base.reason != DisallowedSingleConsumerReason::ConsumerIsPredicate {
            return None;
        }
        let uses = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let (_, use_op) = *uses.first()?;
        if uses.len() != 1 {
            return None;
        }
        let output_key = VarnodeKey::from(output);
        let matched_inputs = use_op
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| (VarnodeKey::from(input) == output_key).then_some(idx))
            .collect::<Vec<_>>();
        let predicate_family = Self::classify_single_consumer_predicate_family(rhs);
        let guard_family = Self::classify_single_consumer_guard_family(use_op, &matched_inputs);
        let low_cost_if_predicate = Self::expr_is_low_cost_builder_inline_candidate(rhs);
        let requires_stable_representative = Self::replacement_read_requires_stable_representative(
            ReplacementReadClass::PredicateSensitive,
            rhs,
        );
        Some(SingleConsumerPredicateProof {
            consumer_block_addr: base.consumer_block_addr,
            consumer_op_seq: base.consumer_op_seq,
            consumer_opcode: base.consumer_opcode,
            rhs_kind: base.rhs_kind,
            predicate_family,
            guard_family,
            same_guard_as_consumer: Self::predicate_families_match(predicate_family, guard_family),
            requires_stable_representative,
            low_cost_if_predicate,
            has_call: base.rhs_has_call,
            has_load: base.rhs_has_load,
        })
    }

    pub(super) fn describe_arithmetic_predicate_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<ArithmeticPredicateProof> {
        let predicate = Self::describe_single_consumer_predicate_proof(block, op_idx, output, rhs)?;
        if predicate.predicate_family != SingleConsumerPredicateFamily::UnknownPredicate {
            return None;
        }
        let (mask_kind, mask_value) = Self::classify_arithmetic_predicate_shape(rhs);
        let boolean_width = mask_value == Some(1) || output.size == 1;
        let stable_required_reason =
            Self::classify_arithmetic_predicate_stable_reason(&predicate, mask_kind);
        Some(ArithmeticPredicateProof {
            consumer_guard: predicate.guard_family,
            mask_kind,
            mask_value,
            boolean_width,
            low_cost: predicate.low_cost_if_predicate,
            stable_required: predicate.requires_stable_representative,
            stable_required_reason,
        })
    }

    pub(super) fn describe_low_bit_mask_predicate_proof(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<LowBitMaskPredicateProof> {
        let arithmetic = Self::describe_arithmetic_predicate_proof(block, op_idx, output, rhs)?;
        if arithmetic.mask_kind != ArithmeticPredicateShape::LowBitAndOne {
            return None;
        }
        let mask_input = Self::low_bit_mask_input_expr(rhs)?;
        let input_origin_kind = Self::classify_low_bit_mask_input_origin_kind(mask_input);
        let input_is_boolean_like = Self::expr_boolean_like(mask_input);
        let family =
            Self::classify_low_bit_mask_predicate_family(input_origin_kind, input_is_boolean_like);
        Some(LowBitMaskPredicateProof {
            family,
            mask_input: format!("{mask_input:?}"),
            consumer_guard: arithmetic.consumer_guard,
            feeds_only_predicate: true,
            input_is_boolean_like,
            input_origin_kind,
            stable_required_reason: arithmetic.stable_required_reason,
        })
    }

    pub(super) fn first_intervening_alias_unsafe_hazard(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        uses: &[(usize, &PcodeOp)],
    ) -> Option<AliasUnsafeHazard> {
        let first_use_idx = uses.first().map(|(idx, _)| *idx)?;
        let mut first_store: Option<(usize, PcodeOpcode)> = None;
        for (candidate_idx, candidate) in block
            .ops
            .iter()
            .enumerate()
            .skip(op_idx + 1)
            .take(first_use_idx.saturating_sub(op_idx + 1))
        {
            match candidate.opcode {
                PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                    return Some(AliasUnsafeHazard::new(
                        AliasUnsafeHazardKind::CallBetweenDefUse,
                        Some(first_use_idx),
                        Some(candidate_idx),
                        Some(candidate.opcode),
                    ));
                }
                PcodeOpcode::Load if first_store.is_some() => {
                    return Some(AliasUnsafeHazard::new(
                        AliasUnsafeHazardKind::LoadAfterStore,
                        Some(first_use_idx),
                        Some(candidate_idx),
                        Some(candidate.opcode),
                    ));
                }
                PcodeOpcode::Store => {
                    first_store.get_or_insert((candidate_idx, candidate.opcode));
                }
                _ => {}
            }
        }
        first_store.map(|(store_idx, store_opcode)| {
            AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::SameBlockStore,
                Some(first_use_idx),
                Some(store_idx),
                Some(store_opcode),
            )
        })
    }

    pub(super) fn classify_same_block_overwrite_rhs_kind(
        opcode: PcodeOpcode,
    ) -> SameBlockOverwriteRhsKind {
        match opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => SameBlockOverwriteRhsKind::CopyLike,
            PcodeOpcode::Load => SameBlockOverwriteRhsKind::Load,
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                SameBlockOverwriteRhsKind::Call
            }
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor => SameBlockOverwriteRhsKind::Predicate,
            PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr => SameBlockOverwriteRhsKind::Arithmetic,
            _ => SameBlockOverwriteRhsKind::Unknown,
        }
    }

    pub(super) fn classify_same_block_overwrite_shape(
        consumer_relation: CrossBlockConsumerRelation,
        redef_idx: usize,
        redef_opcode: PcodeOpcode,
        terminator_idx: Option<usize>,
    ) -> SameBlockOverwriteShapeKind {
        if consumer_relation == CrossBlockConsumerRelation::LoopBackedge {
            return SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate;
        }
        match redef_opcode {
            PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                return SameBlockOverwriteShapeKind::OverwriteAtCallResult;
            }
            PcodeOpcode::Load => return SameBlockOverwriteShapeKind::OverwriteAtLoadResult,
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::SubPiece
            | PcodeOpcode::Piece
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt => return SameBlockOverwriteShapeKind::OverwriteAtCopy,
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor => {
                return SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer;
            }
            _ => {}
        }
        if terminator_idx.is_some_and(|term_idx| redef_idx < term_idx) {
            SameBlockOverwriteShapeKind::OverwriteBeforeBranch
        } else {
            SameBlockOverwriteShapeKind::OverwriteUnknown
        }
    }

    pub(super) fn expr_is_low_cost_builder_inline_candidate(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => true,
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
            HirExpr::Select { .. } => false,
            HirExpr::Call { target, args, .. } => {
                Self::materialize_call_target_is_known_pure_intrinsic(target)
                    && args
                        .iter()
                        .all(Self::expr_is_low_cost_builder_inline_candidate)
            }
        }
    }

    pub(super) fn use_opcode_allows_single_use_builder_inline(opcode: PcodeOpcode) -> bool {
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
                | PcodeOpcode::PopCount
                | PcodeOpcode::LzCount
                | PcodeOpcode::Piece
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Cast
                | PcodeOpcode::PtrAdd
                | PcodeOpcode::PtrSub
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::BoolNegate
                | PcodeOpcode::BoolXor
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
        )
    }

    pub(super) fn use_opcode_allows_passthrough_single_use_builder_inline(
        opcode: PcodeOpcode,
    ) -> bool {
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

    pub(super) fn expr_requires_passthrough_single_use_inline(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
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
            HirExpr::Select { .. } => true,
            HirExpr::Call { .. } => true,
        }
    }

    pub(super) fn output_used_only_by_block_terminator(
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

    pub(super) fn output_use_sites_in_block<'b>(
        &self,
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        Self::collect_output_use_sites_in_block(block, op_idx, output)
    }

    pub(super) fn output_used_only_by_single_store(
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

    pub(super) fn output_used_only_by_passthrough_chain(
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
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    fn test_ptr() -> NirType {
        NirType::Ptr(Box::new(NirType::Unknown))
    }

    fn stack_addr_proof(
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        downstream_opcode: Option<PcodeOpcode>,
        base_reg: StackAddressBaseReg,
        same_block_use_count: usize,
        crosses_call: bool,
        crosses_store: bool,
        rsp_redefined_before_use: bool,
        frame_relative_candidate: bool,
        reason: StackAddressStabilityReason,
    ) -> StackAddressStabilityProof {
        StackAddressStabilityProof {
            consumer_kind,
            downstream_opcode,
            base_reg,
            offset: Some(0x20),
            same_block_use_count,
            crosses_call,
            crosses_store,
            rsp_redefined_before_use,
            frame_relative_candidate,
            reason,
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
    fn low_cost_builder_inline_accepts_pcodeop_intrinsics() {
        let expr = HirExpr::Call {
            target: "__pcodeop_294".to_string(),
            args: vec![HirExpr::Var("param_1".to_string())],
            ty: int(32),
        };

        assert!(PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
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
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::IntEqual
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::BoolOr
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::PopCount
        ));
        assert!(PreviewBuilder::use_opcode_allows_single_use_builder_inline(
            PcodeOpcode::LzCount
        ));
    }

    #[test]
    fn count_intrinsic_consumers_are_data_consumers_for_single_use_inline() {
        let popcount = op(
            1,
            PcodeOpcode::PopCount,
            Some(varnode(0x20)),
            vec![varnode(0x10)],
        );
        let lzcnt = op(
            2,
            PcodeOpcode::LzCount,
            Some(varnode(0x30)),
            vec![varnode(0x10)],
        );

        assert_eq!(
            PreviewBuilder::classify_disallowed_single_consumer_kind(&popcount, &[0]),
            DisallowedSingleConsumerConsumerKind::OtherData
        );
        assert_eq!(
            PreviewBuilder::classify_disallowed_single_consumer_kind(&lzcnt, &[0]),
            DisallowedSingleConsumerConsumerKind::OtherData
        );
    }

    #[test]
    fn lzcnt_intrinsic_rhs_is_low_cost_builder_inline_candidate() {
        let expr = HirExpr::Call {
            target: "__lzcnt".to_string(),
            args: vec![HirExpr::Var("tmp_1".to_string())],
            ty: int(32),
        };

        assert!(PreviewBuilder::expr_is_low_cost_builder_inline_candidate(
            &expr
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

    #[test]
    fn alias_unsafe_hazard_prefers_call_between_def_and_use() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                2,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::CallBetweenDefUse);
        assert_eq!(hazard.use_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Call));
    }

    #[test]
    fn alias_unsafe_hazard_detects_load_after_store_chain() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Store,
                None,
                vec![varnode(0x30), varnode(0x31), constant(0)],
            ),
            op(
                2,
                PcodeOpcode::Load,
                Some(varnode(0x40)),
                vec![varnode(0x30)],
            ),
            op(
                3,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::LoadAfterStore);
        assert_eq!(hazard.use_stmt_idx, Some(3));
        assert_eq!(hazard.hazard_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Load));
    }

    #[test]
    fn alias_unsafe_hazard_falls_back_to_multiple_consumers() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(varnode(0x30)),
                vec![output.clone(), constant(1)],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::MultipleSameBlockConsumers
        );
        assert_eq!(hazard.use_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::IntAdd));
    }

    #[test]
    fn alias_unsafe_hazard_marks_disallowed_single_consumer() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(1, PcodeOpcode::BranchInd, None, vec![output.clone()]),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::DisallowedSingleConsumer);
        assert_eq!(hazard.use_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::BranchInd));
    }

    #[test]
    fn disallowed_single_consumer_proof_marks_predicate_reason() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_disallowed_single_consumer_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("disallowed single consumer proof");

        assert_eq!(
            proof.consumer_kind,
            DisallowedSingleConsumerConsumerKind::Predicate
        );
        assert_eq!(proof.rhs_kind, DisallowedSingleConsumerRhsKind::VarOrConst);
        assert_eq!(
            proof.reason,
            DisallowedSingleConsumerReason::ConsumerIsPredicate
        );
        assert!(proof.rhs_low_cost);
        assert!(!proof.rhs_has_load);
        assert!(!proof.rhs_has_call);
    }

    #[test]
    fn disallowed_single_consumer_proof_marks_load_rhs_reason() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_disallowed_single_consumer_proof(
            &block,
            0,
            &output,
            &HirExpr::Load {
                ptr: Box::new(HirExpr::Var("ptr".to_string())),
                ty: int(32),
            },
        )
        .expect("disallowed single consumer proof");

        assert_eq!(proof.reason, DisallowedSingleConsumerReason::RhsHasLoad);
        assert_eq!(proof.rhs_kind, DisallowedSingleConsumerRhsKind::LoadLike);
        assert!(proof.rhs_has_load);
        assert!(!proof.rhs_has_call);
    }

    #[test]
    fn disallowed_single_consumer_proof_marks_call_arg_reason() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Call,
                None,
                vec![constant(0x2000), output.clone()],
            ),
        ]);

        let proof = PreviewBuilder::describe_disallowed_single_consumer_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("disallowed single consumer proof");

        assert_eq!(
            proof.consumer_kind,
            DisallowedSingleConsumerConsumerKind::CallArg
        );
        assert_eq!(
            proof.reason,
            DisallowedSingleConsumerReason::ConsumerIsCallArg
        );
        assert_eq!(proof.consumer_opcode, PcodeOpcode::Call);
    }

    #[test]
    fn single_consumer_call_rhs_proof_marks_known_pure_intrinsic() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::CallOther,
                Some(output.clone()),
                vec![constant(0x2000), constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntAnd,
                Some(varnode(0x20)),
                vec![output.clone(), constant(1)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "__popcount".to_string(),
            args: vec![HirExpr::Var("tmp_1".to_string())],
            ty: int(32),
        };

        let proof = builder
            .describe_single_consumer_call_rhs_proof(&block, 0, &output, &rhs)
            .expect("call rhs proof");

        assert_eq!(
            proof.family,
            SingleConsumerCallRhsFamily::KnownPureIntrinsic
        );
        assert_eq!(proof.call_target, "__popcount");
        assert_eq!(proof.call_effect_source, None);
        assert_eq!(proof.downstream_opcode, Some(PcodeOpcode::IntAnd));
        assert!(proof.return_used);
    }

    #[test]
    fn single_consumer_call_rhs_proof_marks_preview_unsafe_internal_call() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Call,
                Some(output.clone()),
                vec![constant(0x140043d30)],
            ),
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(varnode(0x20)),
                vec![output.clone(), constant(1)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let mut type_context = NirTypeContext::default();
        type_context.call_effect_summaries.insert(
            "FUN_0x140043d30".to_string(),
            NirCallEffectSummary {
                writes_memory: Some(true),
                may_call_unknown: Some(true),
                may_exit: Some(true),
                source: Some(CallEffectSummarySource::PreviewCalleeAnalysis),
                ..NirCallEffectSummary::default()
            },
        );
        let builder = PreviewBuilder::new(&pcode, &options, Some(&type_context));
        let rhs = HirExpr::Call {
            target: "FUN_0x140043d30".to_string(),
            args: vec![HirExpr::Var("arg_1".to_string())],
            ty: int(32),
        };

        let proof = builder
            .describe_single_consumer_call_rhs_proof(&block, 0, &output, &rhs)
            .expect("call rhs proof");

        assert_eq!(
            proof.family,
            SingleConsumerCallRhsFamily::PreviewCalleeAnalysisUnsafe
        );
        assert_eq!(
            proof.call_effect_source,
            Some(CallEffectSummarySource::PreviewCalleeAnalysis)
        );
        assert_eq!(proof.writes_memory, Some(true));
        assert_eq!(proof.may_call_unknown, Some(true));
        assert_eq!(proof.may_exit, Some(true));
        assert_eq!(proof.downstream_opcode, Some(PcodeOpcode::IntAdd));
    }

    #[test]
    fn carry_intrinsic_predicate_proof_marks_boolor_branch_chain() {
        let output = varnode(0x10);
        let boolor_output = varnode(0x20);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::CallOther,
                Some(output.clone()),
                vec![constant(0x2000), constant(1), constant(2)],
            ),
            op(
                1,
                PcodeOpcode::BoolOr,
                Some(boolor_output.clone()),
                vec![output.clone(), constant(1)],
            ),
            op(
                2,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x3000), boolor_output],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "__carry".to_string(),
            args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
            ty: int(1),
        };

        let proof = builder
            .describe_carry_intrinsic_predicate_proof(&block, 0, &output, &rhs)
            .expect("carry predicate proof");

        assert_eq!(
            proof.bool_chain_role,
            CarryIntrinsicPredicateUseFamily::CarryFeedsBoolOr
        );
        assert_eq!(
            proof.boolor_downstream_use,
            Some(BoolOrDownstreamUseFamily::BoolOrFeedsBranch)
        );
        assert_eq!(
            proof.final_predicate_context,
            CarryIntrinsicFinalPredicateContext::BranchPredicate
        );
        assert!(proof.args_side_effect_free);
    }

    #[test]
    fn carry_intrinsic_predicate_proof_marks_compare_zero_chain() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::CallOther,
                Some(output.clone()),
                vec![constant(0x2000), constant(1), constant(2)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "__scarry".to_string(),
            args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
            ty: int(1),
        };

        let proof = builder
            .describe_carry_intrinsic_predicate_proof(&block, 0, &output, &rhs)
            .expect("carry predicate proof");

        assert_eq!(
            proof.bool_chain_role,
            CarryIntrinsicPredicateUseFamily::CarryFeedsCompareZero
        );
        assert_eq!(proof.boolor_downstream_use, None);
        assert_eq!(
            proof.final_predicate_context,
            CarryIntrinsicFinalPredicateContext::CompareZero
        );
    }

    #[test]
    fn intrinsic_compare_only_proof_marks_sborrow_compare_zero() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::CallOther,
                Some(output.clone()),
                vec![constant(0x2000), constant(1), constant(2)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
            ty: int(1),
        };

        let proof = builder
            .describe_intrinsic_compare_only_proof(&block, 0, &output, &rhs)
            .expect("intrinsic compare-only proof");

        assert_eq!(proof.family, IntrinsicCompareOnlyFamily::BorrowCompareZero);
        assert_eq!(proof.compare_const, Some(0));
        assert_eq!(
            proof.final_predicate_context,
            IntrinsicCompareFinalPredicateContext::CompareZero
        );
        assert!(proof.args_side_effect_free);
    }

    #[test]
    fn intrinsic_compare_only_proof_marks_carry_compare_nonzero() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::CallOther,
                Some(output.clone()),
                vec![constant(0x2000), constant(1), constant(2)],
            ),
            op(
                1,
                PcodeOpcode::IntNotEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "__carry".to_string(),
            args: vec![HirExpr::Var("a".to_string()), HirExpr::Var("b".to_string())],
            ty: int(1),
        };

        let proof = builder
            .describe_intrinsic_compare_only_proof(&block, 0, &output, &rhs)
            .expect("intrinsic compare-only proof");

        assert_eq!(proof.family, IntrinsicCompareOnlyFamily::CarryCompareZero);
        assert_eq!(proof.compare_const, Some(0));
        assert_eq!(
            proof.final_predicate_context,
            IntrinsicCompareFinalPredicateContext::CompareNonZero
        );
    }

    #[test]
    fn unknown_consumer_kind_proof_marks_address_computation() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PtrAdd,
                Some(varnode(0x20)),
                vec![output.clone(), constant(4)],
            ),
        ]);

        let proof = PreviewBuilder::describe_unknown_consumer_kind_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("unknown consumer proof");

        assert_eq!(proof.consumer_opcode, PcodeOpcode::PtrAdd);
        assert_eq!(proof.matched_input_indices, vec![0]);
        assert_eq!(
            proof.reason,
            UnknownConsumerKindReason::ConsumerIsAddressComputation
        );
    }

    #[test]
    fn unknown_consumer_kind_proof_marks_multiple_matched_inputs() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(varnode(0x20)),
                vec![output.clone(), output.clone()],
            ),
        ]);

        let proof = PreviewBuilder::describe_unknown_consumer_kind_proof(
            &block,
            0,
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        )
        .expect("unknown consumer proof");

        assert_eq!(proof.consumer_opcode, PcodeOpcode::IntAdd);
        assert_eq!(proof.matched_input_indices, vec![0, 1]);
        assert_eq!(
            proof.reason,
            UnknownConsumerKindReason::ConsumerHasMultipleMatchedInputs
        );
    }

    #[test]
    fn popcount_consumer_proof_marks_compare_zero_downstream_use() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntEqual,
                Some(varnode(0x30)),
                vec![popcount_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_consumer_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount proof");

        assert_eq!(proof.input_width, 8);
        assert_eq!(proof.output_width, Some(8));
        assert_eq!(
            proof.popcount_result_used_by,
            PopCountResultUseFamily::PopCountFeedsCompareZero
        );
        assert_eq!(
            proof.downstream_consumer_opcode,
            Some(PcodeOpcode::IntEqual)
        );
        assert!(proof.rhs_low_cost);
        assert!(!proof.rhs_has_call);
        assert!(!proof.rhs_has_load);
    }

    #[test]
    fn popcount_consumer_proof_marks_unused_result() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output),
                vec![output.clone()],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_consumer_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount proof");

        assert_eq!(
            proof.popcount_result_used_by,
            PopCountResultUseFamily::PopCountResultUnused
        );
        assert_eq!(proof.downstream_consumer_opcode, None);
    }

    #[test]
    fn popcount_intand_chain_proof_marks_and_one_compare_zero() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_intand_chain_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount intand proof");

        assert_eq!(proof.popcount_consumer_op_seq, 1);
        assert_eq!(proof.intand_op_seq, 2);
        assert_eq!(proof.intand_mask, Some(1));
        assert_eq!(proof.intand_mask_kind, PopCountIntAndMaskKind::AndOne);
        assert_eq!(
            proof.intand_result_consumer,
            PopCountIntAndDownstreamUseFamily::FeedsCompareZero
        );
        assert_eq!(
            proof.downstream_consumer_opcode,
            Some(PcodeOpcode::IntEqual)
        );
        assert!(proof.chain_low_cost);
        assert!(proof.chain_side_effect_free);
    }

    #[test]
    fn popcount_intand_chain_proof_marks_byte_mask_arithmetic() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(0xff)],
            ),
            op(
                3,
                PcodeOpcode::IntAdd,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(1)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_popcount_intand_chain_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("popcount intand proof");

        assert_eq!(proof.intand_mask, Some(0xff));
        assert_eq!(proof.intand_mask_kind, PopCountIntAndMaskKind::AndByteMask);
        assert_eq!(
            proof.intand_result_consumer,
            PopCountIntAndDownstreamUseFamily::FeedsArithmetic
        );
        assert_eq!(proof.downstream_consumer_opcode, Some(PcodeOpcode::IntAdd));
    }

    #[test]
    fn parity_chain_proof_marks_popcount_input_role() {
        let output = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![output.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .describe_parity_chain_proof(&block, 0, &output, &HirExpr::Var("tmp_1".into()))
            .expect("parity chain result")
            .expect("parity chain proof");

        assert_eq!(proof.role, ParityChainRole::PopCountInput);
        assert_eq!(proof.compare_opcode, PcodeOpcode::IntEqual);
        assert_eq!(proof.compare_const, 0);
        assert!(proof.chain_low_cost);
        assert!(proof.chain_side_effect_free);
    }

    #[test]
    fn parity_chain_proof_marks_popcount_result_role() {
        let input = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(0, PcodeOpcode::Copy, Some(input.clone()), vec![constant(1)]),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![input.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntNotEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(1)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "__popcount".into(),
            args: vec![HirExpr::Var("tmp_1".into())],
            ty: int(32),
        };

        let proof = builder
            .describe_parity_chain_proof(&block, 1, &popcount_output, &rhs)
            .expect("parity chain result")
            .expect("parity chain proof");

        assert_eq!(proof.role, ParityChainRole::PopCountResult);
        assert_eq!(proof.compare_opcode, PcodeOpcode::IntNotEqual);
        assert_eq!(proof.compare_const, 1);
    }

    #[test]
    fn parity_chain_proof_marks_intand_result_role() {
        let input = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(0, PcodeOpcode::Copy, Some(input.clone()), vec![constant(1)]),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![input.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(HirExpr::Call {
                target: "__popcount".into(),
                args: vec![HirExpr::Var("tmp_1".into())],
                ty: int(32),
            }),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };

        let proof = builder
            .describe_parity_chain_proof(&block, 2, &and_output, &rhs)
            .expect("parity chain result")
            .expect("parity chain proof");

        assert_eq!(proof.role, ParityChainRole::IntAndResult);
        assert_eq!(proof.compare_const, 0);
    }

    #[test]
    fn parity_chain_proof_rejects_non_one_mask() {
        let input = varnode(0x10);
        let popcount_output = varnode(0x20);
        let and_output = varnode(0x30);
        let block = block(vec![
            op(0, PcodeOpcode::Copy, Some(input.clone()), vec![constant(1)]),
            op(
                1,
                PcodeOpcode::PopCount,
                Some(popcount_output.clone()),
                vec![input.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAnd,
                Some(and_output.clone()),
                vec![popcount_output.clone(), constant(3)],
            ),
            op(
                3,
                PcodeOpcode::IntEqual,
                Some(varnode(0x40)),
                vec![and_output.clone(), constant(0)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let result = builder
            .describe_parity_chain_proof(&block, 0, &input, &HirExpr::Var("tmp_1".into()))
            .expect("parity chain result");

        assert_eq!(result, Err(ParityChainKeepReason::IntAndMaskNotOne));
    }

    #[test]
    fn single_consumer_predicate_proof_marks_compare_zero_same_guard() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_single_consumer_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("tmp_1".to_string())),
                rhs: Box::new(HirExpr::Const(0, int(32))),
                ty: int(1),
            },
        )
        .expect("single consumer predicate proof");

        assert_eq!(
            proof.predicate_family,
            SingleConsumerPredicateFamily::CompareZero
        );
        assert_eq!(
            proof.guard_family,
            SingleConsumerPredicateFamily::CompareZero
        );
        assert!(proof.same_guard_as_consumer);
        assert!(proof.requires_stable_representative);
    }

    #[test]
    fn single_consumer_predicate_proof_marks_negated_flag_same_guard() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::BoolNegate,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let proof = PreviewBuilder::describe_single_consumer_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("tmp_1".to_string())),
                ty: int(1),
            },
        )
        .expect("single consumer predicate proof");

        assert_eq!(
            proof.predicate_family,
            SingleConsumerPredicateFamily::NegatedFlag
        );
        assert_eq!(
            proof.guard_family,
            SingleConsumerPredicateFamily::NegatedFlag
        );
        assert!(proof.same_guard_as_consumer);
    }

    #[test]
    fn single_consumer_predicate_proof_marks_compare_other_var_guard_mismatch() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_single_consumer_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("lhs".to_string())),
                rhs: Box::new(HirExpr::Var("rhs".to_string())),
                ty: int(1),
            },
        )
        .expect("single consumer predicate proof");

        assert_eq!(
            proof.predicate_family,
            SingleConsumerPredicateFamily::CompareOtherVar
        );
        assert_eq!(
            proof.guard_family,
            SingleConsumerPredicateFamily::CompareZero
        );
        assert!(!proof.same_guard_as_consumer);
        assert!(proof.low_cost_if_predicate);
    }

    #[test]
    fn arithmetic_predicate_proof_marks_low_bit_and_one() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_arithmetic_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Var("flag_bits".to_string())),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("arithmetic predicate proof");

        assert_eq!(proof.mask_kind, ArithmeticPredicateShape::LowBitAndOne);
        assert_eq!(proof.mask_value, Some(1));
        assert!(proof.boolean_width);
        assert!(proof.stable_required);
        assert_eq!(
            proof.stable_required_reason,
            Some(ArithmeticPredicateStableReason::ArithmeticMask)
        );
    }

    #[test]
    fn arithmetic_predicate_proof_marks_shift_and_mask() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_arithmetic_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Shr,
                    lhs: Box::new(HirExpr::Var("flags".to_string())),
                    rhs: Box::new(HirExpr::Const(3, int(32))),
                    ty: int(32),
                }),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("arithmetic predicate proof");

        assert_eq!(proof.mask_kind, ArithmeticPredicateShape::ShiftAndMask);
        assert_eq!(proof.mask_value, Some(1));
        assert!(proof.boolean_width);
    }

    #[test]
    fn low_bit_mask_predicate_proof_marks_compare_origin() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_low_bit_mask_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(0, int(32))),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("low-bit mask predicate proof");

        assert_eq!(
            proof.family,
            LowBitMaskPredicateFamily::MaskFromCompareResult
        );
        assert_eq!(proof.input_origin_kind, LowBitMaskInputOriginKind::Compare);
        assert!(proof.input_is_boolean_like);
        assert!(proof.feeds_only_predicate);
    }

    #[test]
    fn low_bit_mask_predicate_proof_marks_arithmetic_origin() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let proof = PreviewBuilder::describe_low_bit_mask_predicate_proof(
            &block,
            0,
            &output,
            &HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(1, int(32))),
                    ty: int(32),
                }),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        )
        .expect("low-bit mask predicate proof");

        assert_eq!(
            proof.family,
            LowBitMaskPredicateFamily::MaskFromArithmeticValue
        );
        assert_eq!(
            proof.input_origin_kind,
            LowBitMaskInputOriginKind::Arithmetic
        );
        assert!(!proof.input_is_boolean_like);
        assert_eq!(
            proof.stable_required_reason,
            Some(ArithmeticPredicateStableReason::ArithmeticMask)
        );
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_no_consumer_found() {
        let output = varnode(0x10);
        let block = block(vec![op(
            0,
            PcodeOpcode::Copy,
            Some(output.clone()),
            vec![constant(1)],
        )]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Const(1, int(32)),
        );

        assert_eq!(hazard.kind, AliasUnsafeHazardKind::UnknownNoConsumerFound);
        assert_eq!(hazard.use_stmt_idx, None);
        assert_eq!(hazard.hazard_stmt_idx, None);
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_redefinition_before_consumer() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(2)],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Const(1, int(32)),
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownMalformedDefUseWindow
        );
        assert_eq!(hazard.use_stmt_idx, None);
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Copy));
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_allowed_consumer_but_non_low_cost_rhs() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(varnode(0x20)),
                vec![output.clone()],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            None,
            &output,
            &HirExpr::Call {
                target: "helper".to_string(),
                args: vec![HirExpr::Var("tmp_1".to_string())],
                ty: int(32),
            },
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownUnhandledConsumerKind
        );
        assert_eq!(hazard.use_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_stmt_idx, Some(1));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::Copy));
    }

    #[test]
    fn alias_unsafe_unknown_subtyping_marks_after_terminator_single_consumer() {
        let output = varnode(0x10);
        let block = block(vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(output.clone()),
                vec![constant(1)],
            ),
            op(1, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
            op(
                2,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
        ]);

        let hazard = PreviewBuilder::classify_alias_unsafe_hazard(
            &block,
            0,
            Some(1),
            &output,
            &HirExpr::Var("tmp_1".to_string()),
        );

        assert_eq!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownConsumerAfterTerminator
        );
        assert_eq!(hazard.use_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_stmt_idx, Some(2));
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::IntEqual));
    }

    #[test]
    fn alias_stable_required_family_prefers_load_addr() {
        let family = PreviewBuilder::classify_alias_stable_required_family(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            DisallowedSingleConsumerRhsKind::Arithmetic,
            Some(PcodeOpcode::Copy),
        );
        assert_eq!(family, AliasStableRequiredFamily::LoadAddrStableRequired);
    }

    #[test]
    fn alias_stable_required_family_prefers_store_addr() {
        let family = PreviewBuilder::classify_alias_stable_required_family(
            DisallowedSingleConsumerConsumerKind::StoreAddr,
            DisallowedSingleConsumerRhsKind::VarOrConst,
            Some(PcodeOpcode::Store),
        );
        assert_eq!(family, AliasStableRequiredFamily::StoreAddrStableRequired);
    }

    #[test]
    fn alias_stable_required_family_prefers_branch_ind() {
        let family = PreviewBuilder::classify_alias_stable_required_family(
            DisallowedSingleConsumerConsumerKind::OtherData,
            DisallowedSingleConsumerRhsKind::VarOrConst,
            Some(PcodeOpcode::BranchInd),
        );
        assert_eq!(family, AliasStableRequiredFamily::BranchIndStableRequired);
    }

    #[test]
    fn alias_stable_required_family_prefers_otherdata_loadlike() {
        let family = PreviewBuilder::classify_alias_stable_required_family(
            DisallowedSingleConsumerConsumerKind::OtherData,
            DisallowedSingleConsumerRhsKind::LoadLike,
            Some(PcodeOpcode::Copy),
        );
        assert_eq!(family, AliasStableRequiredFamily::OtherDataLoadLikeStable);
    }

    #[test]
    fn alias_stable_required_family_prefers_otherdata_copy() {
        let family = PreviewBuilder::classify_alias_stable_required_family(
            DisallowedSingleConsumerConsumerKind::OtherData,
            DisallowedSingleConsumerRhsKind::VarOrConst,
            Some(PcodeOpcode::Copy),
        );
        assert_eq!(family, AliasStableRequiredFamily::OtherDataCopyStable);
    }

    #[test]
    fn alias_stable_required_family_prefers_arithmetic() {
        let family = PreviewBuilder::classify_alias_stable_required_family(
            DisallowedSingleConsumerConsumerKind::OtherData,
            DisallowedSingleConsumerRhsKind::Arithmetic,
            Some(PcodeOpcode::IntAdd),
        );
        assert_eq!(family, AliasStableRequiredFamily::ArithmeticStableRequired);
    }

    #[test]
    fn alias_stable_required_family_falls_back_to_unknown() {
        let family = PreviewBuilder::classify_alias_stable_required_family(
            DisallowedSingleConsumerConsumerKind::CallArg,
            DisallowedSingleConsumerRhsKind::Other,
            None,
        );
        assert_eq!(family, AliasStableRequiredFamily::UnknownAliasStable);
    }

    #[test]
    fn address_stable_required_family_prefers_expr_has_load() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::StackRelative,
            AddressStableRequiredExprKind::HasLoad,
            false,
            false,
        );
        assert_eq!(family, AddressStableRequiredFamily::AddressExprHasLoad);
    }

    #[test]
    fn address_stable_required_family_prefers_expr_has_call() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::GlobalRelative,
            AddressStableRequiredExprKind::HasCall,
            false,
            false,
        );
        assert_eq!(family, AddressStableRequiredFamily::AddressExprHasCall);
    }

    #[test]
    fn address_stable_required_family_prefers_crosses_call_over_base_kind() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::StackRelative,
            AddressStableRequiredExprKind::PureArithmetic,
            false,
            true,
        );
        assert_eq!(family, AddressStableRequiredFamily::AddressExprCrossesCall);
    }

    #[test]
    fn address_stable_required_family_prefers_crosses_store_over_base_kind() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::GlobalRelative,
            AddressStableRequiredExprKind::PureArithmetic,
            true,
            false,
        );
        assert_eq!(family, AddressStableRequiredFamily::AddressExprCrossesStore);
    }

    #[test]
    fn address_stable_required_family_classifies_stack_relative() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::StackRelative,
            AddressStableRequiredExprKind::PureArithmetic,
            false,
            false,
        );
        assert_eq!(
            family,
            AddressStableRequiredFamily::AddressExprStackRelative
        );
    }

    #[test]
    fn address_stable_required_family_classifies_global_relative() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::GlobalRelative,
            AddressStableRequiredExprKind::PureArithmetic,
            false,
            false,
        );
        assert_eq!(
            family,
            AddressStableRequiredFamily::AddressExprGlobalRelative
        );
    }

    #[test]
    fn address_stable_required_family_classifies_register_base() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::RegisterBase,
            AddressStableRequiredExprKind::PureArithmetic,
            false,
            false,
        );
        assert_eq!(family, AddressStableRequiredFamily::AddressExprRegisterBase);
    }

    #[test]
    fn address_stable_required_family_falls_back_to_unknown_base() {
        let family = PreviewBuilder::classify_address_stable_required_family(
            AddressStableRequiredBaseKind::UnknownBase,
            AddressStableRequiredExprKind::UnknownAddressExpr,
            false,
            false,
        );
        assert_eq!(family, AddressStableRequiredFamily::AddressExprUnknownBase);
    }

    #[test]
    fn address_stable_required_base_kind_recognizes_stack_like_names() {
        assert_eq!(
            PreviewBuilder::classify_address_stable_required_base_kind(&HirExpr::Var(
                "home_20".to_string()
            )),
            AddressStableRequiredBaseKind::StackRelative
        );
        assert_eq!(
            PreviewBuilder::classify_address_stable_required_base_kind(&HirExpr::Var(
                "rsp".to_string()
            )),
            AddressStableRequiredBaseKind::StackRelative
        );
    }

    #[test]
    fn address_stable_required_base_kind_recognizes_dat_global() {
        assert_eq!(
            PreviewBuilder::classify_address_stable_required_base_kind(&HirExpr::Var(
                "DAT_140008000".to_string()
            )),
            AddressStableRequiredBaseKind::GlobalRelative
        );
    }

    #[test]
    fn address_stable_required_expr_kind_recognizes_nested_load() {
        let expr = HirExpr::PtrOffset {
            base: Box::new(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("param_1".to_string())),
                ty: NirType::Unknown,
            }),
            offset: 8,
        };
        assert_eq!(
            PreviewBuilder::classify_address_stable_required_expr_kind(&expr),
            AddressStableRequiredExprKind::HasLoad
        );
    }

    #[test]
    fn address_stable_required_expr_kind_recognizes_nested_call() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Call {
                target: "sub_140001000".to_string(),
                args: vec![HirExpr::Var("param_1".to_string())],
                ty: NirType::Unknown,
            }),
            rhs: Box::new(HirExpr::Const(4, NirType::Unknown)),
            ty: NirType::Unknown,
        };
        assert_eq!(
            PreviewBuilder::classify_address_stable_required_expr_kind(&expr),
            AddressStableRequiredExprKind::HasCall
        );
    }

    #[test]
    fn address_stable_required_expr_kind_recognizes_pure_ptr_arithmetic() {
        let expr = HirExpr::PtrOffset {
            base: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("local_10".to_string())),
                rhs: Box::new(HirExpr::Const(8, NirType::Unknown)),
                ty: NirType::Unknown,
            }),
            offset: 4,
        };
        assert_eq!(
            PreviewBuilder::classify_address_stable_required_expr_kind(&expr),
            AddressStableRequiredExprKind::PureArithmetic
        );
    }

    #[test]
    fn stack_address_stability_reason_prefers_crosses_call() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            1,
            false,
            true,
            true,
            true,
            true,
            StackAddressBaseReg::Rsp,
        );
        assert_eq!(reason, StackAddressStabilityReason::StackAddrCrossesCall);
    }

    #[test]
    fn stack_address_stability_reason_prefers_crosses_store() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            1,
            false,
            false,
            true,
            true,
            true,
            StackAddressBaseReg::Rsp,
        );
        assert_eq!(reason, StackAddressStabilityReason::StackAddrCrossesStore);
    }

    #[test]
    fn stack_address_stability_reason_prefers_rsp_mutated_before_use() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            1,
            false,
            false,
            false,
            true,
            true,
            StackAddressBaseReg::Rsp,
        );
        assert_eq!(
            reason,
            StackAddressStabilityReason::StackAddrRspMutatedBeforeUse
        );
    }

    #[test]
    fn stack_address_stability_reason_prefers_escapes_over_single_use() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            1,
            true,
            false,
            false,
            false,
            true,
            StackAddressBaseReg::Rsp,
        );
        assert_eq!(reason, StackAddressStabilityReason::StackAddrEscapes);
    }

    #[test]
    fn stack_address_stability_reason_prefers_multiple_use() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            2,
            false,
            false,
            false,
            false,
            true,
            StackAddressBaseReg::Rsp,
        );
        assert_eq!(reason, StackAddressStabilityReason::StackAddrMultipleUse);
    }

    #[test]
    fn stack_address_stability_reason_classifies_frame_stable() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            1,
            false,
            false,
            false,
            false,
            true,
            StackAddressBaseReg::Rbp,
        );
        assert_eq!(reason, StackAddressStabilityReason::StackAddrFrameStable);
    }

    #[test]
    fn stack_address_stability_reason_classifies_single_use() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            1,
            false,
            false,
            false,
            false,
            false,
            StackAddressBaseReg::Unknown,
        );
        assert_eq!(reason, StackAddressStabilityReason::StackAddrSingleUse);
    }

    #[test]
    fn stack_address_stability_reason_falls_back_to_unknown() {
        let reason = PreviewBuilder::classify_stack_address_stability_reason(
            0,
            false,
            false,
            false,
            false,
            false,
            StackAddressBaseReg::Unknown,
        );
        assert_eq!(reason, StackAddressStabilityReason::StackAddrUnknown);
    }

    #[test]
    fn stack_address_base_reg_recognizes_rsp() {
        assert_eq!(
            PreviewBuilder::classify_stack_address_base_reg(&HirExpr::Var("rsp".to_string())),
            StackAddressBaseReg::Rsp
        );
    }

    #[test]
    fn stack_address_base_reg_recognizes_rbp() {
        let expr = HirExpr::PtrOffset {
            base: Box::new(HirExpr::Cast {
                ty: test_ptr(),
                expr: Box::new(HirExpr::Var("rbp".to_string())),
            }),
            offset: 0x20,
        };
        assert_eq!(
            PreviewBuilder::classify_stack_address_base_reg(&expr),
            StackAddressBaseReg::Rbp
        );
    }

    #[test]
    fn stack_address_offset_extracts_add_const() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("rsp".to_string())),
            rhs: Box::new(HirExpr::Const(0x28, int(64))),
            ty: test_ptr(),
        };
        assert_eq!(
            PreviewBuilder::extract_stack_address_offset(&expr),
            Some(0x28)
        );
    }

    #[test]
    fn stack_address_offset_extracts_sub_const() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(HirExpr::Var("rbp".to_string())),
            rhs: Box::new(HirExpr::Const(0x18, int(64))),
            ty: test_ptr(),
        };
        assert_eq!(
            PreviewBuilder::extract_stack_address_offset(&expr),
            Some(-0x18)
        );
    }

    #[test]
    fn stack_address_frame_relative_candidate_accepts_ptr_offset() {
        let expr = HirExpr::PtrOffset {
            base: Box::new(HirExpr::Cast {
                ty: test_ptr(),
                expr: Box::new(HirExpr::Var("rsp".to_string())),
            }),
            offset: 0x30,
        };
        assert!(PreviewBuilder::stack_address_frame_relative_candidate(
            &expr
        ));
    }

    #[test]
    fn stack_address_frame_relative_candidate_rejects_complex_binary() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("rsp".to_string())),
            rhs: Box::new(HirExpr::Var("xVar31".to_string())),
            ty: test_ptr(),
        };
        assert!(!PreviewBuilder::stack_address_frame_relative_candidate(
            &expr
        ));
    }

    #[test]
    fn stack_addr_frame_stable_trial_accepts_rsp_frame_stable_load_addr() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rsp,
            1,
            false,
            false,
            false,
            true,
            StackAddressStabilityReason::StackAddrFrameStable,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Ok(())
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_accepts_rsp_frame_stable_store_addr() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::StoreAddr,
            Some(PcodeOpcode::Store),
            StackAddressBaseReg::Rsp,
            1,
            false,
            false,
            false,
            true,
            StackAddressStabilityReason::StackAddrFrameStable,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Ok(())
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_non_frame_stable() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rsp,
            1,
            false,
            false,
            false,
            false,
            StackAddressStabilityReason::StackAddrSingleUse,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Err(StackAddrFrameStableTrialReason::RejectedNonFrameStable)
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_multiple_use() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rsp,
            2,
            false,
            false,
            false,
            true,
            StackAddressStabilityReason::StackAddrMultipleUse,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Err(StackAddrFrameStableTrialReason::RejectedMultipleUse)
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_escape() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rsp,
            1,
            false,
            false,
            false,
            true,
            StackAddressStabilityReason::StackAddrEscapes,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, true),
            Err(StackAddrFrameStableTrialReason::RejectedEscapes)
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_base_mutation() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rsp,
            1,
            false,
            false,
            true,
            true,
            StackAddressStabilityReason::StackAddrRspMutatedBeforeUse,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Err(StackAddrFrameStableTrialReason::RejectedBaseMutation)
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_crosses_call() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rsp,
            1,
            true,
            false,
            false,
            true,
            StackAddressStabilityReason::StackAddrCrossesCall,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Err(StackAddrFrameStableTrialReason::RejectedCrossesCallOrStore)
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_crosses_store() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rsp,
            1,
            false,
            true,
            false,
            true,
            StackAddressStabilityReason::StackAddrCrossesStore,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Err(StackAddrFrameStableTrialReason::RejectedCrossesCallOrStore)
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_non_rsp_base() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::LoadAddr,
            Some(PcodeOpcode::Load),
            StackAddressBaseReg::Rbp,
            1,
            false,
            false,
            false,
            true,
            StackAddressStabilityReason::StackAddrFrameStable,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Err(StackAddrFrameStableTrialReason::RejectedNonFrameStable)
        );
    }

    #[test]
    fn stack_addr_frame_stable_trial_rejects_non_memory_consumer() {
        let proof = stack_addr_proof(
            DisallowedSingleConsumerConsumerKind::OtherData,
            Some(PcodeOpcode::Copy),
            StackAddressBaseReg::Rsp,
            1,
            false,
            false,
            false,
            true,
            StackAddressStabilityReason::StackAddrFrameStable,
        );
        assert_eq!(
            PreviewBuilder::classify_stack_addr_frame_stable_trial_reason(&proof, false),
            Err(StackAddrFrameStableTrialReason::RejectedConsumerKind)
        );
    }
}
