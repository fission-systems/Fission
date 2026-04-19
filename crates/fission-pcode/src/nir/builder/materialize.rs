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
enum AliasUnsafeHazardKind {
    MultipleSameBlockConsumers,
    DisallowedSingleConsumer,
    CallBetweenDefUse,
    LoadAfterStore,
    SameBlockStore,
    UnknownNoConsumerFound,
    UnknownConsumerAfterTerminator,
    UnknownUnhandledConsumerKind,
    UnknownMalformedDefUseWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MalformedDefUseWindowRelation {
    DefAfterTerminator,
    ConsumerBeforeDef,
    ConsumerAfterTerminator,
    ConsumerInDifferentBlock,
    TerminatorMissing,
    OpIndexMissing,
    BlockMismatch,
    RedefinitionBeforeConsumer,
    UnknownWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MalformedDefUseWindowDetail {
    relation: MalformedDefUseWindowRelation,
    def_op_idx: usize,
    terminator_idx: Option<usize>,
    consumer_count: usize,
    first_consumer_block: Option<u64>,
    first_consumer_idx: Option<usize>,
    first_consumer_op_seq: Option<u32>,
    rhs_kind: NoConsumerSuppressionRhsKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrossBlockConsumerRelation {
    SuccessorBlock,
    JoinBlock,
    LoopBackedge,
    PostDominatorBlock,
    UnreachableOrUnclassified,
    MergePhiConsumer,
    OrdinaryDataConsumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CrossBlockConsumerProvenance {
    relation: CrossBlockConsumerRelation,
    consumer_opcode: Option<PcodeOpcode>,
    consumer_is_multiequal: bool,
    immediate_successor: bool,
    consumer_is_join: bool,
    redefined_before_consumer: bool,
    def_successor_count: usize,
    consumer_predecessor_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CrossBlockReplacementProof {
    relation: CrossBlockConsumerRelation,
    dominates_consumer: bool,
    rhs_low_cost: bool,
    preserve_materialization: bool,
    no_redefinition_before_consumer: bool,
    merge_phi: bool,
    def_successor_count: usize,
    consumer_predecessor_count: usize,
    narrow_candidate: bool,
    consumer_opcode: Option<PcodeOpcode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrossBlockRedefinitionRelation {
    RedefinedInDefBlockAfterDef,
    RedefinedOnEdge,
    RedefinedInConsumerBlockBeforeUse,
    RedefinedInSiblingPredecessor,
    PhiRedefinition,
    LoopCarriedRedefinition,
    UnknownRedefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CrossBlockRedefinitionDetail {
    relation: CrossBlockRedefinitionRelation,
    redef_block_addr: u64,
    redef_op_idx: usize,
    redef_op_seq: u32,
    redef_opcode: PcodeOpcode,
    redef_rhs_kind: SameBlockOverwriteRhsKind,
    overwrite_shape: SameBlockOverwriteShapeKind,
    def_to_redef_gap: usize,
    redef_to_terminator_gap: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CopyOverwriteRestartProof {
    consumer_relation: CrossBlockConsumerRelation,
    redef_op_seq: u32,
    redef_rhs: String,
    same_value: bool,
    redef_dominates_consumer: bool,
    old_def_has_pre_redef_use: bool,
    consumer_block_addr: u64,
    consumer_op_seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PredicateOverwriteRefreshProof {
    consumer_relation: CrossBlockConsumerRelation,
    redef_op_seq: u32,
    redef_rhs: String,
    predicate_consumer_block_addr: u64,
    predicate_consumer_op_seq: u32,
    predicate_rhs: String,
    same_guard_family: bool,
    old_def_has_pre_redef_use: bool,
    redef_dominates_predicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopCarriedValueKind {
    BooleanFlag,
    CounterIncrement,
    PointerAdvance,
    Accumulator,
    UnknownLoopCarried,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoopCarriedOverwriteProvenance {
    loop_header: u64,
    backedge_block: u64,
    consumer_block: u64,
    consumer_op_seq: u32,
    redef_op_seq: u32,
    redef_rhs: String,
    has_multiequal: bool,
    phi_input_count: usize,
    induction_like: bool,
    carried_value_kind: LoopCarriedValueKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SameBlockOverwriteShapeKind {
    OverwriteBeforeBranch,
    OverwriteAtPredicateProducer,
    OverwriteAtLoopUpdate,
    OverwriteAtCallResult,
    OverwriteAtLoadResult,
    OverwriteAtCopy,
    OverwriteUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SameBlockOverwriteRhsKind {
    CopyLike,
    Predicate,
    Arithmetic,
    Load,
    Call,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AliasUnsafeHazard {
    kind: AliasUnsafeHazardKind,
    use_stmt_idx: Option<usize>,
    hazard_stmt_idx: Option<usize>,
    hazard_opcode: Option<PcodeOpcode>,
}

impl AliasUnsafeHazard {
    fn new(
        kind: AliasUnsafeHazardKind,
        use_stmt_idx: Option<usize>,
        hazard_stmt_idx: Option<usize>,
        hazard_opcode: Option<PcodeOpcode>,
    ) -> Self {
        Self {
            kind,
            use_stmt_idx,
            hazard_stmt_idx,
            hazard_opcode,
        }
    }
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

    fn rejection_reason(self) -> Option<MaterializationRejectionReason> {
        match self.completeness {
            ReplacementCompleteness::Complete => None,
            ReplacementCompleteness::Incomplete(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NoConsumerMaterializationProfile {
    same_block_consumers: usize,
    cross_block_consumers: usize,
    has_later_block_use: bool,
    has_phi_merge_use: bool,
    has_debug_use: bool,
    rhs_side_effectful: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoConsumerMaterializationDecision {
    Suppress,
    Keep(NoConsumerMaterializationKeepReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoConsumerMaterializationKeepReason {
    NotUnknownNoConsumerFound,
    SuppressionDisabled,
    StateVisibleOutput,
    SameBlockConsumerPresent,
    CrossBlockConsumerPresent,
    LaterBlockUsePresent,
    PhiMergeUsePresent,
    DebugUsePresent,
    LegacyInlineCandidate,
    PreserveMaterialization,
    RhsSideEffectful,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoConsumerSuppressionRhsKind {
    Var,
    Const,
    Cast,
    Unary,
    Binary,
    Load,
    Call,
    Aggregate,
    PtrOffset,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoConsumerSuppressionBlockPosition {
    Local,
    PreBranch,
    PredicateAdjacent,
    ReturnAdjacent,
    MergeAdjacent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoConsumerSuppressionOutputKind {
    TempOnly,
    RegisterVisible,
    MemoryDerived,
}

impl<'a> PreviewBuilder<'a> {
    fn trace_materialization_plan(
        &self,
        block_addr: u64,
        op: &PcodeOp,
        output: &Varnode,
        rhs: &HirExpr,
        plan: ReplacementValuePlan,
        event: &str,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let reason = match plan.completeness {
            ReplacementCompleteness::Complete => "Complete".to_string(),
            ReplacementCompleteness::Incomplete(reason) => format!("{reason:?}"),
        };
        self.emit_ready_trace(format!(
            "materialization_drift event={} block=0x{:x} op_seq={} output=space:{} off:0x{:x} size:{} dominant_read={:?} reason={} rhs={:?}",
            event,
            block_addr,
            op.seq_num,
            output.space_id,
            output.offset,
            output.size,
            plan.dominant_read,
            reason,
            rhs,
        ));
    }

    fn trace_alias_unsafe_hazard(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
        hazard: AliasUnsafeHazard,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let use_stmt_idx = hazard
            .use_stmt_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let hazard_stmt_idx = hazard
            .hazard_stmt_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let hazard_op = hazard
            .hazard_opcode
            .map(|opcode| format!("{opcode:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "alias-unsafe-shape output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} use_block=0x{:x} first_alias_hazard={:?} use_stmt_idx={} hazard_stmt={} hazard_op={}",
            output.space_id,
            output.offset,
            output.size,
            block_addr,
            op_seq,
            block_addr,
            hazard.kind,
            use_stmt_idx,
            hazard_stmt_idx,
            hazard_op,
        ));
        if matches!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownNoConsumerFound
                | AliasUnsafeHazardKind::UnknownConsumerAfterTerminator
                | AliasUnsafeHazardKind::UnknownUnhandledConsumerKind
                | AliasUnsafeHazardKind::UnknownMalformedDefUseWindow
        ) {
            self.trace_alias_unsafe_unknown_shape(block_addr, op_seq, output, rhs, hazard);
        }
    }

    fn trace_alias_unsafe_unknown_shape(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
        hazard: AliasUnsafeHazard,
    ) {
        let Some(block) = self
            .pcode
            .blocks
            .iter()
            .find(|block| block.start_address == block_addr)
        else {
            return;
        };
        let Some(op_idx) = block.ops.iter().position(|op| op.seq_num == op_seq) else {
            return;
        };
        let terminator_index = self.block_terminator_index(block);
        let same_block_consumers = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let consumer_count = same_block_consumers.len();
        let first_consumer = same_block_consumers.first().copied();
        let first_consumer_stmt = first_consumer
            .map(|(idx, _)| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_op = first_consumer
            .map(|(_, op)| format!("{:?}", op.opcode))
            .unwrap_or_else(|| "None".to_string());
        let first_consumer_relation = match (first_consumer, terminator_index) {
            (Some((idx, _)), Some(term_idx)) if idx > term_idx => "AfterTerminator",
            (Some(_), _) => "BetweenDefAndTerminator",
            (None, Some(term_idx)) if op_idx > term_idx => "BeforeDef",
            (None, _) => "None",
        };
        let terminator_idx = terminator_index
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "alias-unsafe-unknown-shape output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} terminator_idx={} consumer_count={} same_block_consumers={} first_consumer_stmt={} first_consumer_op={} first_consumer_relation={} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block_addr,
            op_seq,
            terminator_idx,
            consumer_count,
            consumer_count,
            first_consumer_stmt,
            first_consumer_op,
            first_consumer_relation,
            hazard.kind,
        ));
        if hazard.kind == AliasUnsafeHazardKind::UnknownMalformedDefUseWindow {
            self.trace_malformed_def_use_window(block, op_idx, output, rhs);
        }
    }

    fn trace_no_consumer_materialization(
        &self,
        block_addr: u64,
        op_seq: u32,
        event: &str,
        output: &Varnode,
        rhs: &HirExpr,
        preserve_materialization: bool,
        legacy_inline_candidate: bool,
        profile: NoConsumerMaterializationProfile,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "no-consumer-materialization output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} rhs={:?} materialization_event={} preserve_materialization={} legacy_inline_candidate={} has_later_block_use={} has_phi_merge_use={} has_debug_use={} same_block_consumers={} cross_block_consumers={} rhs_side_effectful={}",
            output.space_id,
            output.offset,
            output.size,
            block_addr,
            op_seq,
            rhs,
            event,
            preserve_materialization,
            legacy_inline_candidate,
            profile.has_later_block_use,
            profile.has_phi_merge_use,
            profile.has_debug_use,
            profile.same_block_consumers,
            profile.cross_block_consumers,
            profile.rhs_side_effectful,
        ));
    }

    fn trace_malformed_def_use_window(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let detail = self.describe_malformed_def_use_window(block, op_idx, output, rhs);
        let terminator_idx = detail
            .terminator_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_block = detail
            .first_consumer_block
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_idx = detail
            .first_consumer_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_op_seq = detail
            .first_consumer_op_seq
            .map(|seq| seq.to_string())
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "malformed-def-use-window output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} def_op_idx={} terminator_idx={} consumer_count={} first_consumer_block={} first_consumer_idx={} first_consumer_op_seq={} relation={:?} rhs_kind={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops
                .get(op_idx)
                .map(|op| op.seq_num.to_string())
                .unwrap_or_else(|| "none".to_string()),
            detail.def_op_idx,
            terminator_idx,
            detail.consumer_count,
            first_consumer_block,
            first_consumer_idx,
            first_consumer_op_seq,
            detail.relation,
            detail.rhs_kind,
        ));
        if detail.relation == MalformedDefUseWindowRelation::ConsumerInDifferentBlock {
            self.trace_cross_block_consumer_provenance(block, op_idx, output, rhs);
        }
    }

    fn trace_copy_overwrite_restart_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_copy_overwrite_restart_proof(block, op_idx, output, redef)
        else {
            return;
        };
        self.emit_ready_trace(format!(
            "overwrite-copy-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} redef_rhs={} consumer_block=0x{:x} consumer_op_seq={} same_value={} redef_dominates_consumer={} old_def_has_pre_redef_use={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops[op_idx].seq_num,
            redef.redef_op_seq,
            proof.redef_rhs,
            proof.consumer_block_addr,
            proof.consumer_op_seq,
            proof.same_value,
            proof.redef_dominates_consumer,
            proof.old_def_has_pre_redef_use,
        ));
    }

    fn trace_predicate_overwrite_refresh_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_relation: CrossBlockConsumerRelation,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_predicate_overwrite_refresh_proof(
            block,
            op_idx,
            output,
            redef,
            consumer_relation,
        ) else {
            return;
        };
        self.emit_ready_trace(format!(
            "predicate-overwrite-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} redef_rhs={} predicate_consumer_block=0x{:x} predicate_consumer_op_seq={} predicate_rhs={} same_guard_family={} old_def_has_pre_redef_use={} redef_dominates_predicate={} consumer_relation={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops[op_idx].seq_num,
            proof.redef_op_seq,
            proof.redef_rhs,
            proof.predicate_consumer_block_addr,
            proof.predicate_consumer_op_seq,
            proof.predicate_rhs,
            proof.same_guard_family,
            proof.old_def_has_pre_redef_use,
            proof.redef_dominates_predicate,
            proof.consumer_relation,
        ));
    }

    fn trace_loop_carried_overwrite_provenance(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(provenance) = self.describe_loop_carried_overwrite_provenance(
            block,
            output,
            redef,
            consumer_block_addr,
            consumer_op_seq,
        ) else {
            return;
        };
        self.emit_ready_trace(format!(
            "loop-carried-overwrite output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} redef_rhs={} loop_header=0x{:x} backedge_block=0x{:x} consumer_block=0x{:x} consumer_op_seq={} has_multiequal={} phi_input_count={} induction_like={} carried_value_kind={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            provenance.redef_op_seq,
            provenance.redef_rhs,
            provenance.loop_header,
            provenance.backedge_block,
            provenance.consumer_block,
            provenance.consumer_op_seq,
            provenance.has_multiequal,
            provenance.phi_input_count,
            provenance.induction_like,
            provenance.carried_value_kind,
        ));
    }

    fn trace_no_consumer_suppressed(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "no-consumer-suppressed output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} rhs={:?}",
            output.space_id, output.offset, output.size, block_addr, op_seq, rhs,
        ));
    }

    fn trace_no_consumer_kept(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
        reason: NoConsumerMaterializationKeepReason,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "no-consumer-kept output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} rhs={:?} reason={:?}",
            output.space_id, output.offset, output.size, block_addr, op_seq, rhs, reason,
        ));
    }

    fn trace_no_consumer_suppression_detail(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
        applied: bool,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let rhs_kind = Self::classify_no_consumer_suppression_rhs_kind(rhs);
        let output_kind = Self::classify_no_consumer_suppression_output_kind(output);
        let block_position = self.classify_no_consumer_suppression_block_position(block, op_idx);
        self.emit_ready_trace(format!(
            "no-consumer-suppression-detail output=space:{} off:0x{:x} size:{} rhs={:?} rhs_kind={:?} block=0x{:x} op_seq={} block_position={:?} output_kind={:?} applied={} preserve=false unique={}",
            output.space_id,
            output.offset,
            output.size,
            rhs,
            rhs_kind,
            block.start_address,
            block.ops[op_idx].seq_num,
            block_position,
            output_kind,
            applied,
            output.space_id == UNIQUE_SPACE_ID && !output.is_constant,
        ));
    }

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

    fn expr_is_side_effectful_for_materialization_trace(expr: &HirExpr) -> bool {
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
            HirExpr::Var(_) | HirExpr::Const(..) => false,
        }
    }

    fn no_consumer_suppression_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_NO_CONSUMER_SUPPRESSION"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    fn copy_overwrite_restart_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_COPY_OVERWRITE_RESTART"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
    }

    fn predicate_refresh_restart_enabled() -> bool {
        matches!(
            std::env::var("FISSION_ENABLE_PREDICATE_REFRESH_RESTART"),
            Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")
        )
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
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "representative_downgrade",
            );
            self.representative_downgrade_count += 1;
            return Ok(None);
        }
        let no_consumer_profile =
            self.analyze_no_consumer_materialization_profile(block, op_idx, output, &rhs);
        let no_consumer_hazard = if replacement_plan.rejection_reason()
            == Some(MaterializationRejectionReason::AliasUnsafe)
        {
            Some(Self::classify_alias_unsafe_hazard(
                block,
                op_idx,
                terminator_index,
                output,
                &rhs,
            ))
        } else {
            None
        };
        match Self::classify_no_consumer_materialization_decision(
            output,
            &rhs,
            legacy_inline_candidate,
            replacement_plan,
            no_consumer_hazard,
            no_consumer_profile,
        ) {
            NoConsumerMaterializationDecision::Suppress => {
                let suppression_enabled = Self::no_consumer_suppression_enabled();
                self.trace_no_consumer_materialization(
                    block_addr,
                    op.seq_num,
                    if suppression_enabled {
                        "suppressed"
                    } else {
                        "suppression_candidate"
                    },
                    output,
                    &rhs,
                    Self::should_preserve_materialized_expr(&rhs),
                    legacy_inline_candidate,
                    no_consumer_profile,
                );
                self.trace_no_consumer_suppression_detail(
                    block,
                    op_idx,
                    output,
                    &rhs,
                    suppression_enabled,
                );
                if suppression_enabled {
                    self.trace_no_consumer_suppressed(block_addr, op.seq_num, output, &rhs);
                    return Ok(None);
                }
                self.trace_no_consumer_kept(
                    block_addr,
                    op.seq_num,
                    output,
                    &rhs,
                    NoConsumerMaterializationKeepReason::SuppressionDisabled,
                );
            }
            NoConsumerMaterializationDecision::Keep(reason) => {
                if reason != NoConsumerMaterializationKeepReason::NotUnknownNoConsumerFound {
                    self.trace_no_consumer_materialization(
                        block_addr,
                        op.seq_num,
                        "kept",
                        output,
                        &rhs,
                        Self::should_preserve_materialized_expr(&rhs),
                        legacy_inline_candidate,
                        no_consumer_profile,
                    );
                    self.trace_no_consumer_kept(block_addr, op.seq_num, output, &rhs, reason);
                }
            }
        }
        if legacy_inline_candidate {
            self.materialization_inline_suppressed_count += 1;
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "inline_suppressed",
            );
        } else {
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "materialized_binding",
            );
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
        if Self::copy_overwrite_restart_enabled() {
            if let Some(proof) = self.can_restart_def_window_at_copy_overwrite(
                block,
                op_idx,
                terminator_index,
                output,
            ) {
                self.emit_ready_trace(format!(
                    "def-window-restarted-at-copy-overwrite output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} consumer_block=0x{:x} consumer_op_seq={} relation={:?} redef_rhs={} same_value={} redef_dominates_consumer={} old_def_has_pre_redef_use={}",
                    output.space_id,
                    output.offset,
                    output.size,
                    block.start_address,
                    block.ops[op_idx].seq_num,
                    proof.redef_op_seq,
                    proof.consumer_block_addr,
                    proof.consumer_op_seq,
                    proof.consumer_relation,
                    proof.redef_rhs,
                    proof.same_value,
                    proof.redef_dominates_consumer,
                    proof.old_def_has_pre_redef_use,
                ));
                self.replacement_plan_completed_count += 1;
                return ReplacementValuePlan::complete(ReplacementReadClass::SameBlockData);
            }
        }
        if Self::predicate_refresh_restart_enabled() {
            if let Some(proof) = self.can_restart_def_window_at_predicate_refresh(
                block,
                op_idx,
                terminator_index,
                output,
            ) {
                self.emit_ready_trace(format!(
                    "def-window-restarted-at-predicate-refresh output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} predicate_consumer_block=0x{:x} predicate_consumer_op_seq={} relation={:?} redef_rhs={} predicate_rhs={} same_guard_family={} old_def_has_pre_redef_use={} redef_dominates_predicate={}",
                    output.space_id,
                    output.offset,
                    output.size,
                    block.start_address,
                    block.ops[op_idx].seq_num,
                    proof.redef_op_seq,
                    proof.predicate_consumer_block_addr,
                    proof.predicate_consumer_op_seq,
                    proof.consumer_relation,
                    proof.redef_rhs,
                    proof.predicate_rhs,
                    proof.same_guard_family,
                    proof.old_def_has_pre_redef_use,
                    proof.redef_dominates_predicate,
                ));
                self.replacement_plan_completed_count += 1;
                return ReplacementValuePlan::complete(ReplacementReadClass::PredicateSensitive);
            }
        }
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
        let hazard =
            Self::classify_alias_unsafe_hazard(block, op_idx, terminator_index, output, rhs);
        self.trace_alias_unsafe_hazard(
            block.start_address,
            block.ops[op_idx].seq_num,
            output,
            rhs,
            hazard,
        );
        ReplacementValuePlan::incomplete(
            ReplacementReadClass::SameBlockData,
            MaterializationRejectionReason::AliasUnsafe,
        )
    }

    fn can_restart_def_window_at_copy_overwrite(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<CopyOverwriteRestartProof> {
        let (consumer_block_addr, _consumer_op_seq, provenance) =
            self.describe_cross_block_consumer_provenance(block, op_idx, output)?;
        if !matches!(
            provenance.relation,
            CrossBlockConsumerRelation::SuccessorBlock
                | CrossBlockConsumerRelation::PostDominatorBlock
        ) || provenance.consumer_is_multiequal
            || provenance.relation == CrossBlockConsumerRelation::LoopBackedge
        {
            return None;
        }
        let redef = self.describe_cross_block_redefinition_detail(
            block,
            op_idx,
            output,
            consumer_block_addr,
        )?;
        let proof = self.describe_copy_overwrite_restart_proof(block, op_idx, output, &redef)?;
        if !proof.same_value || !proof.redef_dominates_consumer || proof.old_def_has_pre_redef_use {
            return None;
        }
        if !Self::copy_overwrite_rhs_is_pure_restart_candidate(&redef) {
            return None;
        }
        if !Self::no_alias_hazard_between_redef_and_terminator(
            block,
            redef.redef_op_idx,
            terminator_index,
        ) {
            return None;
        }
        Some(CopyOverwriteRestartProof {
            consumer_relation: provenance.relation,
            ..proof
        })
    }

    fn can_restart_def_window_at_predicate_refresh(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
    ) -> Option<PredicateOverwriteRefreshProof> {
        let (consumer_block_addr, _consumer_op_seq, provenance) =
            self.describe_cross_block_consumer_provenance(block, op_idx, output)?;
        if provenance.relation != CrossBlockConsumerRelation::PostDominatorBlock
            || provenance.consumer_is_multiequal
            || provenance.relation == CrossBlockConsumerRelation::LoopBackedge
        {
            return None;
        }
        let redef = self.describe_cross_block_redefinition_detail(
            block,
            op_idx,
            output,
            consumer_block_addr,
        )?;
        if !Self::predicate_refresh_rhs_is_restart_candidate(&redef) {
            return None;
        }
        let proof = self.describe_predicate_overwrite_refresh_proof(
            block,
            op_idx,
            output,
            &redef,
            provenance.relation,
        )?;
        if !proof.same_guard_family
            || proof.old_def_has_pre_redef_use
            || !proof.redef_dominates_predicate
        {
            return None;
        }
        let consumer_block_idx = self
            .address_to_index
            .get(&proof.predicate_consumer_block_addr)
            .copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let consumer_op = consumer_block
            .ops
            .iter()
            .find(|candidate| candidate.seq_num == proof.predicate_consumer_op_seq)?;
        if consumer_op.opcode != PcodeOpcode::BoolNegate {
            return None;
        }
        if !Self::no_alias_hazard_between_redef_and_terminator(
            block,
            redef.redef_op_idx,
            terminator_index,
        ) {
            return None;
        }
        Some(proof)
    }

    fn copy_overwrite_rhs_is_pure_restart_candidate(redef: &CrossBlockRedefinitionDetail) -> bool {
        matches!(redef.redef_rhs_kind, SameBlockOverwriteRhsKind::CopyLike)
            && matches!(
                redef.overwrite_shape,
                SameBlockOverwriteShapeKind::OverwriteAtCopy
            )
    }

    fn predicate_refresh_rhs_is_restart_candidate(redef: &CrossBlockRedefinitionDetail) -> bool {
        matches!(redef.redef_rhs_kind, SameBlockOverwriteRhsKind::Predicate)
            && matches!(
                redef.overwrite_shape,
                SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer
            )
            && matches!(
                redef.redef_opcode,
                PcodeOpcode::IntEqual
                    | PcodeOpcode::IntNotEqual
                    | PcodeOpcode::BoolNegate
                    | PcodeOpcode::BoolXor
            )
    }

    fn no_alias_hazard_between_redef_and_terminator(
        block: &crate::pcode::PcodeBasicBlock,
        redef_idx: usize,
        terminator_index: Option<usize>,
    ) -> bool {
        let Some(term_idx) = terminator_index else {
            return false;
        };
        if redef_idx >= term_idx {
            return false;
        }
        !block.ops[redef_idx + 1..term_idx].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
                    | PcodeOpcode::Store
                    | PcodeOpcode::Load
            )
        })
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

    fn analyze_no_consumer_materialization_profile(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> NoConsumerMaterializationProfile {
        let same_block_consumers =
            Self::collect_output_use_sites_in_block(block, op_idx, output).len();
        let (cross_block_consumers, has_phi_merge_use) =
            Self::collect_output_use_sites_outside_block(
                &self.pcode.blocks,
                block.start_address,
                output,
            );
        NoConsumerMaterializationProfile {
            same_block_consumers,
            cross_block_consumers,
            has_later_block_use: cross_block_consumers > 0,
            has_phi_merge_use,
            has_debug_use: false,
            rhs_side_effectful: Self::expr_is_side_effectful_for_materialization_trace(rhs),
        }
    }

    fn classify_no_consumer_materialization_decision(
        output: &Varnode,
        rhs: &HirExpr,
        legacy_inline_candidate: bool,
        plan: ReplacementValuePlan,
        hazard: Option<AliasUnsafeHazard>,
        profile: NoConsumerMaterializationProfile,
    ) -> NoConsumerMaterializationDecision {
        if plan.rejection_reason() != Some(MaterializationRejectionReason::AliasUnsafe) {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::NotUnknownNoConsumerFound,
            );
        }
        if hazard.map(|hazard| hazard.kind) != Some(AliasUnsafeHazardKind::UnknownNoConsumerFound) {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::NotUnknownNoConsumerFound,
            );
        }
        if profile.same_block_consumers != 0 {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::SameBlockConsumerPresent,
            );
        }
        if profile.cross_block_consumers != 0 {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::CrossBlockConsumerPresent,
            );
        }
        if profile.has_later_block_use {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::LaterBlockUsePresent,
            );
        }
        if profile.has_phi_merge_use {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::PhiMergeUsePresent,
            );
        }
        if profile.has_debug_use {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::DebugUsePresent,
            );
        }
        if legacy_inline_candidate {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::LegacyInlineCandidate,
            );
        }
        if Self::should_preserve_materialized_expr(rhs) {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::PreserveMaterialization,
            );
        }
        if profile.rhs_side_effectful {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::RhsSideEffectful,
            );
        }
        if output.space_id != UNIQUE_SPACE_ID || output.is_constant {
            return NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::StateVisibleOutput,
            );
        }
        NoConsumerMaterializationDecision::Suppress
    }

    fn classify_no_consumer_suppression_rhs_kind(rhs: &HirExpr) -> NoConsumerSuppressionRhsKind {
        match rhs {
            HirExpr::Var(_) => NoConsumerSuppressionRhsKind::Var,
            HirExpr::Const(..) => NoConsumerSuppressionRhsKind::Const,
            HirExpr::Cast { .. } => NoConsumerSuppressionRhsKind::Cast,
            HirExpr::Unary { .. } => NoConsumerSuppressionRhsKind::Unary,
            HirExpr::Binary { .. } => NoConsumerSuppressionRhsKind::Binary,
            HirExpr::Load { .. } => NoConsumerSuppressionRhsKind::Load,
            HirExpr::Call { .. } => NoConsumerSuppressionRhsKind::Call,
            HirExpr::AggregateCopy { .. } => NoConsumerSuppressionRhsKind::Aggregate,
            HirExpr::PtrOffset { .. } => NoConsumerSuppressionRhsKind::PtrOffset,
            HirExpr::Index { .. } => NoConsumerSuppressionRhsKind::Index,
        }
    }

    fn classify_no_consumer_suppression_output_kind(
        output: &Varnode,
    ) -> NoConsumerSuppressionOutputKind {
        if output.space_id == UNIQUE_SPACE_ID && !output.is_constant {
            NoConsumerSuppressionOutputKind::TempOnly
        } else if output.space_id == REGISTER_SPACE_ID {
            NoConsumerSuppressionOutputKind::RegisterVisible
        } else {
            NoConsumerSuppressionOutputKind::MemoryDerived
        }
    }

    fn classify_no_consumer_suppression_block_position(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> NoConsumerSuppressionBlockPosition {
        if let Some(term_idx) = self.block_terminator_index(block) {
            let term = &block.ops[term_idx];
            if op_idx + 1 == term_idx {
                return match term.opcode {
                    PcodeOpcode::CBranch => NoConsumerSuppressionBlockPosition::PredicateAdjacent,
                    PcodeOpcode::Return => NoConsumerSuppressionBlockPosition::ReturnAdjacent,
                    _ => NoConsumerSuppressionBlockPosition::PreBranch,
                };
            }
            if op_idx < term_idx {
                return NoConsumerSuppressionBlockPosition::PreBranch;
            }
        }
        let Some(block_idx) = self.address_to_index.get(&block.start_address).copied() else {
            return NoConsumerSuppressionBlockPosition::Local;
        };
        if self.successors.get(block_idx).is_some_and(|succs| {
            succs.iter().any(|succ| {
                self.predecessors
                    .get(*succ)
                    .is_some_and(|preds| preds.len() > 1)
            })
        }) {
            return NoConsumerSuppressionBlockPosition::MergeAdjacent;
        }
        NoConsumerSuppressionBlockPosition::Local
    }

    fn classify_alias_unsafe_hazard(
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
            let passthrough_required = Self::expr_requires_passthrough_single_use_inline(rhs);
            let consumer_allows_inline = if passthrough_required {
                Self::use_opcode_allows_passthrough_single_use_builder_inline(use_op.opcode)
            } else {
                Self::use_opcode_allows_single_use_builder_inline(use_op.opcode)
            };
            if !Self::expr_is_low_cost_builder_inline_candidate(rhs) || !consumer_allows_inline {
                return AliasUnsafeHazard::new(
                    if terminator_index.is_some_and(|term_idx| use_idx > term_idx) {
                        AliasUnsafeHazardKind::UnknownConsumerAfterTerminator
                    } else if consumer_allows_inline {
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

    fn first_intervening_alias_unsafe_hazard(
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

    fn collect_output_use_sites_in_block<'b>(
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

    fn first_output_redefinition_in_block<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, &'b PcodeOp)> {
        Self::first_output_redefinition_in_block_from(block, op_idx + 1, output)
    }

    fn first_output_redefinition_in_block_from<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        start_idx: usize,
        output: &Varnode,
    ) -> Option<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        block
            .ops
            .iter()
            .enumerate()
            .skip(start_idx)
            .find(|(_, candidate)| {
                candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone())
            })
    }

    fn collect_output_use_sites_in_block_unbounded<'b>(
        block: &'b crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Vec<(usize, &'b PcodeOp)> {
        let key = VarnodeKey::from(output);
        block
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
            .collect()
    }

    fn first_output_use_site_outside_block(
        &self,
        current_block_addr: u64,
        output: &Varnode,
    ) -> Option<(u64, usize, u32)> {
        let key = VarnodeKey::from(output);
        self.pcode
            .blocks
            .iter()
            .filter(|block| block.start_address != current_block_addr)
            .find_map(|block| {
                block
                    .ops
                    .iter()
                    .enumerate()
                    .find(|(_, candidate)| {
                        candidate
                            .inputs
                            .iter()
                            .any(|input| VarnodeKey::from(input) == key)
                    })
                    .map(|(idx, op)| (block.start_address, idx, op.seq_num))
            })
    }

    fn classify_malformed_def_use_window_relation(
        def_op_idx: usize,
        terminator_idx: Option<usize>,
        first_same_block_consumer_idx: Option<usize>,
        first_cross_block_consumer: Option<(u64, usize, u32)>,
        block_index_present: bool,
        has_redefinition: bool,
    ) -> MalformedDefUseWindowRelation {
        if !block_index_present {
            return MalformedDefUseWindowRelation::BlockMismatch;
        }
        let Some(terminator_idx) = terminator_idx else {
            return MalformedDefUseWindowRelation::TerminatorMissing;
        };
        if def_op_idx > terminator_idx {
            return MalformedDefUseWindowRelation::DefAfterTerminator;
        }
        if let Some(consumer_idx) = first_same_block_consumer_idx {
            if consumer_idx < def_op_idx {
                return MalformedDefUseWindowRelation::ConsumerBeforeDef;
            }
            if consumer_idx > terminator_idx {
                return MalformedDefUseWindowRelation::ConsumerAfterTerminator;
            }
        }
        if first_cross_block_consumer.is_some() {
            return MalformedDefUseWindowRelation::ConsumerInDifferentBlock;
        }
        if has_redefinition {
            return MalformedDefUseWindowRelation::RedefinitionBeforeConsumer;
        }
        MalformedDefUseWindowRelation::UnknownWindow
    }

    fn describe_malformed_def_use_window(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> MalformedDefUseWindowDetail {
        let rhs_kind = Self::classify_no_consumer_suppression_rhs_kind(rhs);
        let terminator_idx = self.block_terminator_index(block);
        let block_index_present = self.address_to_index.contains_key(&block.start_address);
        if op_idx >= block.ops.len() {
            return MalformedDefUseWindowDetail {
                relation: MalformedDefUseWindowRelation::OpIndexMissing,
                def_op_idx: op_idx,
                terminator_idx,
                consumer_count: 0,
                first_consumer_block: None,
                first_consumer_idx: None,
                first_consumer_op_seq: None,
                rhs_kind,
            };
        }
        let same_block_consumers =
            Self::collect_output_use_sites_in_block_unbounded(block, op_idx, output);
        let first_same_block_consumer = same_block_consumers.first().copied();
        let first_cross_block_consumer =
            self.first_output_use_site_outside_block(block.start_address, output);
        let relation = Self::classify_malformed_def_use_window_relation(
            op_idx,
            terminator_idx,
            first_same_block_consumer.map(|(idx, _)| idx),
            first_cross_block_consumer,
            block_index_present,
            Self::first_output_redefinition_in_block(block, op_idx, output).is_some(),
        );
        let consumer_count =
            same_block_consumers.len() + usize::from(first_cross_block_consumer.is_some());
        let (first_consumer_block, first_consumer_idx, first_consumer_op_seq) =
            if let Some((idx, op)) = first_same_block_consumer {
                (Some(block.start_address), Some(idx), Some(op.seq_num))
            } else if let Some((consumer_block, consumer_idx, consumer_op_seq)) =
                first_cross_block_consumer
            {
                (
                    Some(consumer_block),
                    Some(consumer_idx),
                    Some(consumer_op_seq),
                )
            } else {
                (None, None, None)
            };
        MalformedDefUseWindowDetail {
            relation,
            def_op_idx: op_idx,
            terminator_idx,
            consumer_count,
            first_consumer_block,
            first_consumer_idx,
            first_consumer_op_seq,
            rhs_kind,
        }
    }

    fn trace_cross_block_consumer_provenance(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(provenance) = self.describe_cross_block_consumer_provenance(block, op_idx, output)
        else {
            return;
        };
        let def_successors = self
            .address_to_index
            .get(&block.start_address)
            .and_then(|idx| self.successors.get(*idx))
            .map(|succs| {
                succs
                    .iter()
                    .filter_map(|succ| {
                        self.pcode
                            .blocks
                            .get(*succ)
                            .map(|block| format!("0x{:x}", block.start_address))
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "none".to_string());
        let consumer_block = provenance
            .0
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let consumer_op_seq = provenance
            .1
            .map(|seq| seq.to_string())
            .unwrap_or_else(|| "none".to_string());
        let consumer_opcode = provenance
            .2
            .consumer_opcode
            .map(|opcode| format!("{opcode:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "cross-block-consumer output=space:{} off:0x{:x} size:{} def_block=0x{:x} consumer_block={} consumer_op_seq={} consumer_opcode={} relation={:?} def_successors=[{}] def_successor_count={} consumer_predecessors={} consumer_is_multiequal={} immediate_successor={} consumer_is_join={} redefined_before_consumer={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            consumer_block,
            consumer_op_seq,
            consumer_opcode,
            provenance.2.relation,
            def_successors,
            provenance.2.def_successor_count,
            provenance.2.consumer_predecessor_count,
            provenance.2.consumer_is_multiequal,
            provenance.2.immediate_successor,
            provenance.2.consumer_is_join,
            provenance.2.redefined_before_consumer,
        ));
        if let Some(proof) = self.describe_cross_block_replacement_proof(block, op_idx, output, rhs)
        {
            let consumer_block = provenance
                .0
                .map(|addr| format!("0x{addr:x}"))
                .unwrap_or_else(|| "none".to_string());
            let consumer_opcode = proof
                .consumer_opcode
                .map(|opcode| format!("{opcode:?}"))
                .unwrap_or_else(|| "None".to_string());
            self.emit_ready_trace(format!(
                "cross-block-replacement-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} consumer_block={} relation={:?} def_successor_count={} consumer_predecessor_count={} dominates_consumer={} consumer_opcode={} rhs_low_cost={} preserve_materialization={} no_redefinition_before_consumer={} merge_phi={} narrow_candidate={}",
                output.space_id,
                output.offset,
                output.size,
                block.start_address,
                consumer_block,
                proof.relation,
                proof.def_successor_count,
                proof.consumer_predecessor_count,
                proof.dominates_consumer,
                consumer_opcode,
                proof.rhs_low_cost,
                proof.preserve_materialization,
                proof.no_redefinition_before_consumer,
                proof.merge_phi,
                proof.narrow_candidate,
            ));
            if let Some(redef) =
                self.describe_cross_block_redefinition_detail(block, op_idx, output, provenance.0)
            {
                self.emit_ready_trace(format!(
                    "cross-block-redefinition output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_block={} relation={:?} redef_block=0x{:x} redef_op_seq={} redef_opcode={:?} redef_rhs_kind={:?} overwrite_shape={:?} redef_relation={:?} consumer_op_seq={} terminator_idx={} def_to_redef_gap={} redef_to_terminator_gap={}",
                    output.space_id,
                    output.offset,
                    output.size,
                    block.start_address,
                    block.ops[op_idx].seq_num,
                    consumer_block,
                    proof.relation,
                    redef.redef_block_addr,
                    redef.redef_op_seq,
                    redef.redef_opcode,
                    redef.redef_rhs_kind,
                    redef.overwrite_shape,
                    redef.relation,
                    consumer_op_seq,
                    self.block_terminator_index(block)
                        .map(|idx| idx.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    redef.def_to_redef_gap,
                    redef
                        .redef_to_terminator_gap
                        .map(|gap| gap.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                ));
                if redef.overwrite_shape == SameBlockOverwriteShapeKind::OverwriteAtCopy {
                    self.trace_copy_overwrite_restart_proof(block, op_idx, output, &redef);
                } else if redef.overwrite_shape
                    == SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer
                {
                    self.trace_predicate_overwrite_refresh_proof(
                        block,
                        op_idx,
                        output,
                        &redef,
                        proof.relation,
                    );
                } else if redef.overwrite_shape
                    == SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate
                {
                    if let (Some(consumer_block_addr), Some(consumer_op_seq), _) = provenance {
                        self.trace_loop_carried_overwrite_provenance(
                            block,
                            op_idx,
                            output,
                            &redef,
                            consumer_block_addr,
                            consumer_op_seq,
                        );
                    }
                }
            }
        }
    }

    fn describe_cross_block_consumer_provenance(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<(Option<u64>, Option<u32>, CrossBlockConsumerProvenance)> {
        let (consumer_block_addr, consumer_idx, consumer_op_seq) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let consumer_op = consumer_block.ops.get(consumer_idx)?;
        let consumer_is_multiequal = consumer_op.opcode == PcodeOpcode::MultiEqual;
        let immediate_successor = self
            .successors
            .get(def_block_idx)
            .is_some_and(|succs| succs.contains(&consumer_block_idx));
        let consumer_predecessor_count = self
            .predecessors
            .get(consumer_block_idx)
            .map_or(0, Vec::len);
        let consumer_is_join = consumer_predecessor_count > 1;
        let redefined_before_consumer =
            Self::first_output_redefinition_in_block(block, op_idx, output).is_some();
        let consumer_dominates_def = self.dom_tree.dominates(consumer_block_idx, def_block_idx);
        let consumer_postdominates_def = self
            .cfg_facts
            .postdominators()
            .postdominators()
            .get(&def_block_idx)
            .is_some_and(|set| set.contains(&consumer_block_idx));
        let relation = if consumer_is_multiequal {
            CrossBlockConsumerRelation::MergePhiConsumer
        } else if consumer_dominates_def && !consumer_postdominates_def {
            CrossBlockConsumerRelation::LoopBackedge
        } else if immediate_successor && !consumer_is_join {
            CrossBlockConsumerRelation::SuccessorBlock
        } else if consumer_is_join {
            CrossBlockConsumerRelation::JoinBlock
        } else if consumer_postdominates_def {
            CrossBlockConsumerRelation::PostDominatorBlock
        } else if immediate_successor {
            CrossBlockConsumerRelation::SuccessorBlock
        } else if self.address_to_index.contains_key(&consumer_block_addr) {
            CrossBlockConsumerRelation::OrdinaryDataConsumer
        } else {
            CrossBlockConsumerRelation::UnreachableOrUnclassified
        };
        Some((
            Some(consumer_block_addr),
            Some(consumer_op_seq),
            CrossBlockConsumerProvenance {
                relation,
                consumer_opcode: Some(consumer_op.opcode),
                consumer_is_multiequal,
                immediate_successor,
                consumer_is_join,
                redefined_before_consumer,
                def_successor_count: self.successors.get(def_block_idx).map_or(0, Vec::len),
                consumer_predecessor_count,
            },
        ))
    }

    fn describe_cross_block_replacement_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<CrossBlockReplacementProof> {
        let (_, _, provenance) =
            self.describe_cross_block_consumer_provenance(block, op_idx, output)?;
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let (consumer_block_addr, _, _) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let dominates_consumer = self.dom_tree.dominates(def_block_idx, consumer_block_idx);
        let rhs_low_cost = Self::expr_is_low_cost_builder_inline_candidate(rhs);
        let preserve_materialization = Self::should_preserve_materialized_expr(rhs);
        let no_redefinition_before_consumer =
            Self::first_output_redefinition_in_block(block, op_idx, output).is_none();
        let narrow_candidate = matches!(
            provenance.relation,
            CrossBlockConsumerRelation::SuccessorBlock
                | CrossBlockConsumerRelation::PostDominatorBlock
                | CrossBlockConsumerRelation::OrdinaryDataConsumer
        ) && provenance.def_successor_count == 1
            && !provenance.consumer_is_multiequal
            && rhs_low_cost
            && !preserve_materialization
            && no_redefinition_before_consumer
            && dominates_consumer;
        Some(CrossBlockReplacementProof {
            relation: provenance.relation,
            dominates_consumer,
            rhs_low_cost,
            preserve_materialization,
            no_redefinition_before_consumer,
            merge_phi: provenance.consumer_is_multiequal,
            def_successor_count: provenance.def_successor_count,
            consumer_predecessor_count: provenance.consumer_predecessor_count,
            narrow_candidate,
            consumer_opcode: provenance.consumer_opcode,
        })
    }

    fn describe_cross_block_redefinition_detail(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        consumer_block_addr: Option<u64>,
    ) -> Option<CrossBlockRedefinitionDetail> {
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let consumer_block_addr = consumer_block_addr?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let (consumer_idx, consumer_op) =
            consumer_block
                .ops
                .iter()
                .enumerate()
                .find(|(_, candidate)| {
                    candidate
                        .inputs
                        .iter()
                        .any(|input| VarnodeKey::from(input) == VarnodeKey::from(output))
                })?;
        let consumer_relation = self
            .describe_cross_block_consumer_provenance(block, op_idx, output)
            .map(|(_, _, provenance)| provenance.relation)
            .unwrap_or(CrossBlockConsumerRelation::UnreachableOrUnclassified);

        let terminator_idx = self.block_terminator_index(block);

        if let Some((redef_idx, redef_op)) =
            Self::first_output_redefinition_in_block(block, op_idx, output)
        {
            return Some(CrossBlockRedefinitionDetail {
                relation: CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef,
                redef_block_addr: block.start_address,
                redef_op_idx: redef_idx,
                redef_op_seq: redef_op.seq_num,
                redef_opcode: redef_op.opcode,
                redef_rhs_kind: Self::classify_same_block_overwrite_rhs_kind(redef_op.opcode),
                overwrite_shape: Self::classify_same_block_overwrite_shape(
                    consumer_relation,
                    redef_idx,
                    redef_op.opcode,
                    terminator_idx,
                ),
                def_to_redef_gap: redef_idx.saturating_sub(op_idx),
                redef_to_terminator_gap: terminator_idx.map(|term| term.saturating_sub(redef_idx)),
            });
        }

        if let Some((redef_idx, redef_op)) =
            Self::first_output_redefinition_in_block_from(consumer_block, 0, output)
        {
            if redef_idx < consumer_idx {
                return Some(CrossBlockRedefinitionDetail {
                    relation: CrossBlockRedefinitionRelation::RedefinedInConsumerBlockBeforeUse,
                    redef_block_addr: consumer_block.start_address,
                    redef_op_idx: redef_idx,
                    redef_op_seq: redef_op.seq_num,
                    redef_opcode: redef_op.opcode,
                    redef_rhs_kind: Self::classify_same_block_overwrite_rhs_kind(redef_op.opcode),
                    overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                    def_to_redef_gap: redef_idx,
                    redef_to_terminator_gap: self
                        .block_terminator_index(consumer_block)
                        .map(|term| term.saturating_sub(redef_idx)),
                });
            }
        }

        if consumer_op.opcode == PcodeOpcode::MultiEqual {
            if let Some((pred_block_addr, redef_op_seq)) = self
                .predecessors
                .get(consumer_block_idx)
                .into_iter()
                .flat_map(|preds| preds.iter())
                .filter(|pred_idx| **pred_idx != def_block_idx)
                .find_map(|pred_idx| {
                    self.pcode.blocks.get(*pred_idx).and_then(|pred_block| {
                        Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                            .map(|(_, op)| (pred_block.start_address, op.seq_num))
                    })
                })
            {
                return Some(CrossBlockRedefinitionDetail {
                    relation: CrossBlockRedefinitionRelation::PhiRedefinition,
                    redef_block_addr: pred_block_addr,
                    redef_op_idx: 0,
                    redef_op_seq,
                    redef_opcode: PcodeOpcode::MultiEqual,
                    redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                    overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                    def_to_redef_gap: 0,
                    redef_to_terminator_gap: None,
                });
            }
        }

        if self.dom_tree.dominates(consumer_block_idx, def_block_idx) {
            if let Some((redef_block_addr, redef_op_seq)) = self
                .predecessors
                .get(consumer_block_idx)
                .into_iter()
                .flat_map(|preds| preds.iter())
                .find_map(|pred_idx| {
                    self.pcode.blocks.get(*pred_idx).and_then(|pred_block| {
                        Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                            .map(|(_, op)| (pred_block.start_address, op.seq_num))
                    })
                })
            {
                return Some(CrossBlockRedefinitionDetail {
                    relation: CrossBlockRedefinitionRelation::LoopCarriedRedefinition,
                    redef_block_addr,
                    redef_op_idx: 0,
                    redef_op_seq,
                    redef_opcode: PcodeOpcode::MultiEqual,
                    redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                    overwrite_shape: SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate,
                    def_to_redef_gap: 0,
                    redef_to_terminator_gap: None,
                });
            }
        }

        if let Some((edge_block_addr, redef_op_seq)) = self
            .successors
            .get(def_block_idx)
            .into_iter()
            .flat_map(|succs| succs.iter())
            .filter(|succ_idx| **succ_idx != consumer_block_idx)
            .find_map(|succ_idx| {
                self.pcode.blocks.get(*succ_idx).and_then(|succ_block| {
                    Self::first_output_redefinition_in_block_from(succ_block, 0, output)
                        .map(|(_, op)| (succ_block.start_address, op.seq_num))
                })
            })
        {
            return Some(CrossBlockRedefinitionDetail {
                relation: CrossBlockRedefinitionRelation::RedefinedOnEdge,
                redef_block_addr: edge_block_addr,
                redef_op_idx: 0,
                redef_op_seq,
                redef_opcode: PcodeOpcode::Copy,
                redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                def_to_redef_gap: 0,
                redef_to_terminator_gap: None,
            });
        }

        if let Some((pred_block_addr, redef_op_seq)) = self
            .predecessors
            .get(consumer_block_idx)
            .into_iter()
            .flat_map(|preds| preds.iter())
            .filter(|pred_idx| **pred_idx != def_block_idx)
            .find_map(|pred_idx| {
                self.pcode.blocks.get(*pred_idx).and_then(|pred_block| {
                    Self::first_output_redefinition_in_block_from(pred_block, 0, output)
                        .map(|(_, op)| (pred_block.start_address, op.seq_num))
                })
            })
        {
            return Some(CrossBlockRedefinitionDetail {
                relation: CrossBlockRedefinitionRelation::RedefinedInSiblingPredecessor,
                redef_block_addr: pred_block_addr,
                redef_op_idx: 0,
                redef_op_seq,
                redef_opcode: PcodeOpcode::Copy,
                redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                def_to_redef_gap: 0,
                redef_to_terminator_gap: None,
            });
        }

        self.pcode
            .blocks
            .iter()
            .filter(|candidate| {
                candidate.start_address != block.start_address
                    && candidate.start_address != consumer_block_addr
            })
            .find_map(|candidate| {
                Self::first_output_redefinition_in_block_from(candidate, 0, output).map(
                    |(_, op)| CrossBlockRedefinitionDetail {
                        relation: CrossBlockRedefinitionRelation::UnknownRedefinition,
                        redef_block_addr: candidate.start_address,
                        redef_op_idx: 0,
                        redef_op_seq: op.seq_num,
                        redef_opcode: op.opcode,
                        redef_rhs_kind: SameBlockOverwriteRhsKind::Unknown,
                        overwrite_shape: SameBlockOverwriteShapeKind::OverwriteUnknown,
                        def_to_redef_gap: 0,
                        redef_to_terminator_gap: None,
                    },
                )
            })
    }

    fn describe_copy_overwrite_restart_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
    ) -> Option<CopyOverwriteRestartProof> {
        if redef.relation != CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef
            || redef.overwrite_shape != SameBlockOverwriteShapeKind::OverwriteAtCopy
        {
            return None;
        }
        let redef_op = block.ops.get(redef.redef_op_idx)?;
        let (consumer_block_addr, _, _) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let (_consumer_idx, consumer_op) =
            consumer_block
                .ops
                .iter()
                .enumerate()
                .find(|(_, candidate)| {
                    candidate
                        .inputs
                        .iter()
                        .any(|input| VarnodeKey::from(input) == VarnodeKey::from(output))
                })?;
        let def_op = block.ops.get(op_idx)?;
        let old_def_has_pre_redef_use =
            !Self::collect_output_use_sites_in_block(block, op_idx, output).is_empty();
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let redef_dominates_consumer = self.dom_tree.dominates(def_block_idx, consumer_block_idx)
            && self
                .block_terminator_index(block)
                .is_some_and(|term_idx| redef.redef_op_idx < term_idx);
        Some(CopyOverwriteRestartProof {
            consumer_relation: CrossBlockConsumerRelation::UnreachableOrUnclassified,
            redef_op_seq: redef.redef_op_seq,
            redef_rhs: Self::format_copy_overwrite_inputs(&redef_op.inputs),
            same_value: Self::ops_share_copylike_value(def_op, redef_op),
            redef_dominates_consumer,
            old_def_has_pre_redef_use,
            consumer_block_addr,
            consumer_op_seq: consumer_op.seq_num,
        })
    }

    fn describe_predicate_overwrite_refresh_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_relation: CrossBlockConsumerRelation,
    ) -> Option<PredicateOverwriteRefreshProof> {
        if redef.relation != CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef
            || redef.overwrite_shape != SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer
        {
            return None;
        }
        let redef_op = block.ops.get(redef.redef_op_idx)?;
        let (consumer_block_addr, _, _) =
            self.first_output_use_site_outside_block(block.start_address, output)?;
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let (_consumer_idx, consumer_op) =
            consumer_block
                .ops
                .iter()
                .enumerate()
                .find(|(_, candidate)| {
                    candidate
                        .inputs
                        .iter()
                        .any(|input| VarnodeKey::from(input) == VarnodeKey::from(output))
                })?;
        let old_def_has_pre_redef_use =
            !Self::collect_output_use_sites_in_block(block, op_idx, output).is_empty();
        let def_block_idx = self.address_to_index.get(&block.start_address).copied()?;
        let redef_dominates_predicate = self.dom_tree.dominates(def_block_idx, consumer_block_idx)
            && self
                .block_terminator_index(block)
                .is_some_and(|term_idx| redef.redef_op_idx < term_idx);
        Some(PredicateOverwriteRefreshProof {
            consumer_relation,
            redef_op_seq: redef.redef_op_seq,
            redef_rhs: Self::format_copy_overwrite_inputs(&redef_op.inputs),
            predicate_consumer_block_addr: consumer_block_addr,
            predicate_consumer_op_seq: consumer_op.seq_num,
            predicate_rhs: Self::format_copy_overwrite_inputs(&consumer_op.inputs),
            same_guard_family: Self::predicate_consumer_matches_output_guard_family(
                consumer_op,
                output,
            ),
            old_def_has_pre_redef_use,
            redef_dominates_predicate,
        })
    }

    fn describe_loop_carried_overwrite_provenance(
        &self,
        _block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
    ) -> Option<LoopCarriedOverwriteProvenance> {
        if redef.overwrite_shape != SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate {
            return None;
        }
        let consumer_block_idx = self.address_to_index.get(&consumer_block_addr).copied()?;
        let consumer_block = self.pcode.blocks.get(consumer_block_idx)?;
        let redef_block_idx = self
            .address_to_index
            .get(&redef.redef_block_addr)
            .copied()?;
        let redef_block = self.pcode.blocks.get(redef_block_idx)?;
        let loop_header = consumer_block_addr;
        let backedge_block = redef.redef_block_addr;
        let (has_multiequal, phi_input_count) = consumer_block
            .ops
            .iter()
            .filter(|op| op.opcode == PcodeOpcode::MultiEqual)
            .fold((false, 0usize), |(_, max_inputs), op| {
                (true, max_inputs.max(op.inputs.len()))
            });
        let redef_op = redef_block.ops.get(redef.redef_op_idx)?;
        let redef_rhs = Self::format_copy_overwrite_inputs(&redef_op.inputs);
        let carried_value_kind =
            Self::classify_loop_carried_value_kind(output, redef_op, self.options.pointer_size);
        let induction_like = matches!(
            carried_value_kind,
            LoopCarriedValueKind::CounterIncrement | LoopCarriedValueKind::PointerAdvance
        );
        Some(LoopCarriedOverwriteProvenance {
            loop_header,
            backedge_block,
            consumer_block: consumer_block_addr,
            consumer_op_seq,
            redef_op_seq: redef.redef_op_seq,
            redef_rhs,
            has_multiequal,
            phi_input_count,
            induction_like,
            carried_value_kind,
        })
    }

    fn classify_loop_carried_value_kind(
        output: &Varnode,
        redef_op: &PcodeOp,
        pointer_size: u32,
    ) -> LoopCarriedValueKind {
        match redef_op.opcode {
            PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::BoolXor
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow => LoopCarriedValueKind::BooleanFlag,
            PcodeOpcode::IntAdd | PcodeOpcode::IntSub => {
                let self_carried = redef_op
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == VarnodeKey::from(output));
                let has_const = redef_op.inputs.iter().any(|input| input.is_constant);
                if self_carried && has_const && output.size == pointer_size {
                    LoopCarriedValueKind::PointerAdvance
                } else if self_carried && has_const {
                    LoopCarriedValueKind::CounterIncrement
                } else if self_carried {
                    LoopCarriedValueKind::Accumulator
                } else {
                    LoopCarriedValueKind::UnknownLoopCarried
                }
            }
            PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr => LoopCarriedValueKind::Accumulator,
            _ => LoopCarriedValueKind::UnknownLoopCarried,
        }
    }

    fn predicate_consumer_matches_output_guard_family(
        consumer_op: &PcodeOp,
        output: &Varnode,
    ) -> bool {
        let key = VarnodeKey::from(output);
        match consumer_op.opcode {
            PcodeOpcode::BoolNegate => consumer_op
                .inputs
                .first()
                .is_some_and(|input| VarnodeKey::from(input) == key),
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual | PcodeOpcode::BoolXor => {
                if consumer_op.inputs.len() != 2 {
                    return false;
                }
                let lhs_matches = VarnodeKey::from(&consumer_op.inputs[0]) == key
                    && consumer_op.inputs[1].is_constant
                    && consumer_op.inputs[1].constant_val <= 1;
                let rhs_matches = VarnodeKey::from(&consumer_op.inputs[1]) == key
                    && consumer_op.inputs[0].is_constant
                    && consumer_op.inputs[0].constant_val <= 1;
                lhs_matches || rhs_matches
            }
            PcodeOpcode::CBranch => consumer_op
                .inputs
                .get(1)
                .is_some_and(|input| VarnodeKey::from(input) == key),
            _ => false,
        }
    }

    fn ops_share_copylike_value(def_op: &PcodeOp, redef_op: &PcodeOp) -> bool {
        matches!(
            redef_op.opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::SubPiece
                | PcodeOpcode::Piece
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt
        ) && def_op.opcode == redef_op.opcode
            && def_op.inputs == redef_op.inputs
    }

    fn format_copy_overwrite_inputs(inputs: &[Varnode]) -> String {
        let formatted = inputs
            .iter()
            .map(Self::format_copy_overwrite_varnode)
            .collect::<Vec<_>>()
            .join(",");
        format!("[{formatted}]")
    }

    fn format_copy_overwrite_varnode(vn: &Varnode) -> String {
        if vn.is_constant {
            format!("const(0x{:x}:s{})", vn.offset, vn.size)
        } else {
            format!("space:{}:0x{:x}:s{}", vn.space_id, vn.offset, vn.size)
        }
    }

    fn classify_same_block_overwrite_rhs_kind(opcode: PcodeOpcode) -> SameBlockOverwriteRhsKind {
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

    fn classify_same_block_overwrite_shape(
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

    fn collect_output_use_sites_outside_block(
        blocks: &[crate::pcode::PcodeBasicBlock],
        current_block_addr: u64,
        output: &Varnode,
    ) -> (usize, bool) {
        let key = VarnodeKey::from(output);
        let mut consumer_count = 0usize;
        let mut has_phi_merge_use = false;
        for block in blocks {
            if block.start_address == current_block_addr {
                continue;
            }
            for candidate in &block.ops {
                if candidate.output.as_ref().map(VarnodeKey::from) == Some(key.clone()) {
                    break;
                }
                if candidate
                    .inputs
                    .iter()
                    .any(|input| VarnodeKey::from(input) == key)
                {
                    consumer_count += 1;
                    has_phi_merge_use |= candidate.opcode == PcodeOpcode::MultiEqual;
                }
            }
        }
        (consumer_count, has_phi_merge_use)
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
        Self::collect_output_use_sites_in_block(block, op_idx, output)
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

    fn varnode(offset: u64) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size: 8,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn constant(value: i64) -> Varnode {
        Varnode::constant(value, 8)
    }

    fn op(
        seq_num: u32,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address: 0x1000 + u64::from(seq_num),
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    fn block(ops: Vec<PcodeOp>) -> crate::pcode::PcodeBasicBlock {
        crate::pcode::PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: Vec::new(),
            ops,
        }
    }

    fn block_at(
        start_address: u64,
        index: u32,
        ops: Vec<PcodeOp>,
    ) -> crate::pcode::PcodeBasicBlock {
        crate::pcode::PcodeBasicBlock {
            index,
            start_address,
            successors: Vec::new(),
            ops,
        }
    }

    fn pcode_function(blocks: Vec<crate::pcode::PcodeBasicBlock>) -> crate::pcode::PcodeFunction {
        crate::pcode::PcodeFunction { blocks }
    }

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0x1400_0000,
            sections: vec![(0x1400_1000, 0x1400_2000)],
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            calling_convention: Default::default(),
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
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(varnode(0x20)),
                vec![output.clone(), constant(0)],
            ),
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
        assert_eq!(hazard.hazard_opcode, Some(PcodeOpcode::IntEqual));
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
    fn malformed_def_use_window_relation_marks_terminator_missing() {
        let relation = PreviewBuilder::classify_malformed_def_use_window_relation(
            0, None, None, None, true, true,
        );

        assert_eq!(relation, MalformedDefUseWindowRelation::TerminatorMissing);
    }

    #[test]
    fn malformed_def_use_window_relation_marks_cross_block_consumer() {
        let relation = PreviewBuilder::classify_malformed_def_use_window_relation(
            0,
            Some(3),
            None,
            Some((0x2000, 1, 7)),
            true,
            true,
        );

        assert_eq!(
            relation,
            MalformedDefUseWindowRelation::ConsumerInDifferentBlock
        );
    }

    #[test]
    fn malformed_def_use_window_relation_marks_redefinition_before_consumer() {
        let relation = PreviewBuilder::classify_malformed_def_use_window_relation(
            0,
            Some(3),
            None,
            None,
            true,
            true,
        );

        assert_eq!(
            relation,
            MalformedDefUseWindowRelation::RedefinitionBeforeConsumer
        );
    }

    #[test]
    fn cross_block_consumer_provenance_prefers_merge_phi_consumer() {
        let output = varnode(0x10);
        let rhs = HirExpr::Const(1, int(32));
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    1,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![constant(2)],
                )],
            ),
            block_at(
                0x1020,
                2,
                vec![op(
                    2,
                    PcodeOpcode::MultiEqual,
                    Some(varnode(0x30)),
                    vec![output.clone(), varnode(0x20)],
                )],
            ),
        ];
        blocks[0].successors = vec![2];
        blocks[1].successors = vec![2];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let provenance = builder
            .describe_cross_block_consumer_provenance(&blocks[0], 0, &output)
            .expect("cross-block provenance");

        assert_eq!(
            provenance.2.relation,
            CrossBlockConsumerRelation::MergePhiConsumer
        );
        assert!(provenance.2.consumer_is_multiequal);
        let proof = builder
            .describe_cross_block_replacement_proof(&blocks[0], 0, &output, &rhs)
            .expect("cross-block proof");
        assert!(!proof.narrow_candidate);
        assert!(proof.merge_phi);
    }

    #[test]
    fn cross_block_consumer_provenance_marks_single_successor_data_consumer() {
        let output = varnode(0x10);
        let rhs = HirExpr::Const(1, int(32));
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    1,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let provenance = builder
            .describe_cross_block_consumer_provenance(&blocks[0], 0, &output)
            .expect("cross-block provenance");

        assert_eq!(
            provenance.2.relation,
            CrossBlockConsumerRelation::SuccessorBlock
        );
        assert!(!provenance.2.consumer_is_multiequal);
        assert!(provenance.2.immediate_successor);
        assert!(!provenance.2.consumer_is_join);
        let proof = builder
            .describe_cross_block_replacement_proof(&blocks[0], 0, &output, &rhs)
            .expect("cross-block proof");
        assert!(proof.narrow_candidate);
        assert!(proof.dominates_consumer);
        assert!(proof.rhs_low_cost);
        assert!(proof.no_redefinition_before_consumer);
    }

    #[test]
    fn cross_block_redefinition_marks_def_block_after_def() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
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
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    2,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("cross-block redefinition");

        assert_eq!(
            redef.relation,
            CrossBlockRedefinitionRelation::RedefinedInDefBlockAfterDef
        );
        assert_eq!(redef.redef_block_addr, 0x1000);
        assert_eq!(redef.redef_op_seq, 1);
    }

    #[test]
    fn cross_block_redefinition_marks_consumer_block_before_use() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(2)],
                    ),
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(varnode(0x20)),
                        vec![output.clone()],
                    ),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("cross-block redefinition");

        assert_eq!(
            redef.relation,
            CrossBlockRedefinitionRelation::RedefinedInConsumerBlockBeforeUse
        );
        assert_eq!(redef.redef_block_addr, 0x1010);
        assert_eq!(redef.redef_op_seq, 1);
    }

    #[test]
    fn copy_overwrite_restart_proof_marks_same_value_and_no_pre_redef_use() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
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
                        vec![constant(1)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("cross-block redefinition");
        assert_eq!(
            redef.overwrite_shape,
            SameBlockOverwriteShapeKind::OverwriteAtCopy
        );

        let proof = builder
            .describe_copy_overwrite_restart_proof(&blocks[0], 0, &output, &redef)
            .expect("copy overwrite proof");

        assert!(proof.same_value);
        assert!(proof.redef_dominates_consumer);
        assert!(!proof.old_def_has_pre_redef_use);
        assert_eq!(proof.consumer_block_addr, 0x1010);
        assert_eq!(proof.consumer_op_seq, 3);
        assert_eq!(proof.redef_rhs, "[const(0x1:s8)]");
    }

    #[test]
    fn def_window_restart_proves_copy_overwrite_successor_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
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
                        vec![constant(1)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(2), &output)
            .expect("restart proof");

        assert_eq!(
            proof.consumer_relation,
            CrossBlockConsumerRelation::SuccessorBlock
        );
        assert!(proof.same_value);
        assert!(proof.redef_dominates_consumer);
        assert!(!proof.old_def_has_pre_redef_use);
    }

    #[test]
    fn def_window_restart_applies_for_copy_overwrite_postdominator_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
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
                        vec![constant(1)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1008)]),
                ],
            ),
            block_at(
                0x1008,
                1,
                vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
            ),
            block_at(
                0x1010,
                2,
                vec![op(
                    4,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![2];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(2), &output)
            .expect("restart proof");

        assert_eq!(
            proof.consumer_relation,
            CrossBlockConsumerRelation::PostDominatorBlock
        );
        assert!(proof.same_value);
        assert!(proof.redef_dominates_consumer);
        assert!(!proof.old_def_has_pre_redef_use);
    }

    #[test]
    fn def_window_restart_rejects_pre_redef_use() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
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
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    4,
                    PcodeOpcode::Copy,
                    Some(varnode(0x30)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(3), &output)
                .is_none()
        );
    }

    #[test]
    fn def_window_restart_rejects_impure_copy_overwrite_rhs() {
        let output = varnode(0x10);
        let ptr = varnode(0x11);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(output.clone()),
                        vec![constant(1)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Load,
                        Some(output.clone()),
                        vec![constant(0), ptr.clone()],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(2), &output)
                .is_none()
        );
    }

    #[test]
    fn def_window_restart_rejects_alias_hazard_after_redef() {
        let output = varnode(0x10);
        let ptr = varnode(0x11);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
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
                        vec![constant(1)],
                    ),
                    op(
                        2,
                        PcodeOpcode::Load,
                        Some(varnode(0x20)),
                        vec![constant(0), ptr.clone()],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    4,
                    PcodeOpcode::Copy,
                    Some(varnode(0x30)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_copy_overwrite(&blocks[0], 0, Some(3), &output)
                .is_none()
        );
    }

    #[test]
    fn predicate_overwrite_refresh_proof_marks_same_guard_family_for_bool_negate_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x12), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::BoolNegate,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("redef");

        let proof = builder
            .describe_predicate_overwrite_refresh_proof(
                &blocks[0],
                0,
                &output,
                &redef,
                CrossBlockConsumerRelation::PostDominatorBlock,
            )
            .expect("predicate proof");

        assert!(proof.same_guard_family);
        assert!(proof.redef_dominates_predicate);
        assert!(!proof.old_def_has_pre_redef_use);
    }

    #[test]
    fn predicate_overwrite_refresh_proof_marks_non_guard_family_for_plain_copy_consumer() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x12), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::Copy,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[0], 0, &output, Some(0x1010))
            .expect("redef");

        let proof = builder
            .describe_predicate_overwrite_refresh_proof(
                &blocks[0],
                0,
                &output,
                &redef,
                CrossBlockConsumerRelation::SuccessorBlock,
            )
            .expect("predicate proof");

        assert!(!proof.same_guard_family);
        assert!(proof.redef_dominates_predicate);
    }

    #[test]
    fn predicate_refresh_restart_proves_same_guard_postdom_boolnegate() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x12), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1008)]),
                ],
            ),
            block_at(
                0x1008,
                1,
                vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
            ),
            block_at(
                0x1010,
                2,
                vec![op(
                    4,
                    PcodeOpcode::BoolNegate,
                    Some(varnode(0x20)),
                    vec![output.clone()],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![2];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let proof = builder
            .can_restart_def_window_at_predicate_refresh(&blocks[0], 0, Some(2), &output)
            .expect("predicate refresh restart proof");

        assert_eq!(
            proof.consumer_relation,
            CrossBlockConsumerRelation::PostDominatorBlock
        );
        assert!(proof.same_guard_family);
        assert!(!proof.old_def_has_pre_redef_use);
        assert!(proof.redef_dominates_predicate);
    }

    #[test]
    fn predicate_refresh_restart_rejects_successor_int_notequal_composition() {
        let output = varnode(0x10);
        let other = varnode(0x12);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::IntEqual,
                        Some(output.clone()),
                        vec![varnode(0x11), constant(0)],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntSLess,
                        Some(output.clone()),
                        vec![varnode(0x13), constant(0)],
                    ),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::IntNotEqual,
                    Some(varnode(0x20)),
                    vec![output.clone(), other],
                )],
            ),
        ];
        blocks[0].successors = vec![1];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .can_restart_def_window_at_predicate_refresh(&blocks[0], 0, Some(2), &output)
                .is_none()
        );
    }

    #[test]
    fn loop_carried_overwrite_provenance_marks_boolean_flag_without_multiequal() {
        let output = varnode(0x10);
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::Copy,
                        Some(varnode(0x14)),
                        vec![output.clone()],
                    ),
                    op(1, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntSCarry,
                        Some(output.clone()),
                        vec![varnode(0x12), varnode(0x13)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let detail = builder
            .describe_loop_carried_overwrite_provenance(&blocks[1], &output, &redef, 0x1000, 0)
            .expect("loop provenance");

        assert_eq!(detail.loop_header, 0x1000);
        assert_eq!(detail.backedge_block, 0x1010);
        assert!(!detail.has_multiequal);
        assert_eq!(detail.phi_input_count, 0);
        assert!(!detail.induction_like);
        assert_eq!(detail.carried_value_kind, LoopCarriedValueKind::BooleanFlag);
    }

    #[test]
    fn loop_carried_overwrite_provenance_marks_pointer_advance_with_multiequal() {
        let output = Varnode {
            space_id: REGISTER_SPACE_ID,
            offset: 0x20,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let mut blocks = vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(
                        0,
                        PcodeOpcode::MultiEqual,
                        Some(varnode(0x50)),
                        vec![varnode(0x30), varnode(0x31)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(varnode(0x40)),
                        vec![output.clone()],
                    ),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::IntAdd,
                        Some(output.clone()),
                        vec![output.clone(), constant(8)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1000)]),
                ],
            ),
        ];
        blocks[0].successors = vec![1];
        blocks[1].successors = vec![0];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let redef = builder
            .describe_cross_block_redefinition_detail(&blocks[1], 0, &output, Some(0x1000))
            .expect("redef");
        let detail = builder
            .describe_loop_carried_overwrite_provenance(&blocks[1], &output, &redef, 0x1000, 1)
            .expect("loop provenance");

        assert_eq!(detail.loop_header, 0x1000);
        assert_eq!(detail.backedge_block, 0x1010);
        assert!(detail.has_multiequal);
        assert_eq!(detail.phi_input_count, 2);
        assert!(detail.induction_like);
        assert_eq!(
            detail.carried_value_kind,
            LoopCarriedValueKind::PointerAdvance
        );
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
    fn no_consumer_materialization_profile_marks_deadish_local_output() {
        let output = varnode(0x10);
        let blocks = vec![block(vec![op(
            0,
            PcodeOpcode::Copy,
            Some(output.clone()),
            vec![constant(1)],
        )])];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let profile = builder.analyze_no_consumer_materialization_profile(
            &blocks[0],
            0,
            &output,
            &HirExpr::Const(1, int(32)),
        );

        assert_eq!(profile.same_block_consumers, 0);
        assert_eq!(profile.cross_block_consumers, 0);
        assert!(!profile.has_later_block_use);
        assert!(!profile.has_phi_merge_use);
        assert!(!profile.has_debug_use);
        assert!(!profile.rhs_side_effectful);
    }

    #[test]
    fn no_consumer_materialization_profile_detects_cross_block_multiequal_use() {
        let output = varnode(0x10);
        let blocks = vec![
            block_at(
                0x1000,
                0,
                vec![op(
                    0,
                    PcodeOpcode::Copy,
                    Some(output.clone()),
                    vec![constant(1)],
                )],
            ),
            block_at(
                0x2000,
                1,
                vec![op(
                    0,
                    PcodeOpcode::MultiEqual,
                    Some(varnode(0x20)),
                    vec![output.clone(), constant(2)],
                )],
            ),
        ];
        let pcode = pcode_function(blocks.clone());
        let options = test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        let profile = builder.analyze_no_consumer_materialization_profile(
            &blocks[0],
            0,
            &output,
            &HirExpr::Const(1, int(32)),
        );

        assert_eq!(profile.same_block_consumers, 0);
        assert_eq!(profile.cross_block_consumers, 1);
        assert!(profile.has_later_block_use);
        assert!(profile.has_phi_merge_use);
        assert!(!profile.has_debug_use);
    }

    #[test]
    fn no_consumer_materialization_decision_suppresses_dead_unique_const() {
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &varnode(0x10),
            &HirExpr::Const(1, int(32)),
            false,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::AliasUnsafe,
            ),
            Some(AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownNoConsumerFound,
                None,
                None,
                None,
            )),
            NoConsumerMaterializationProfile {
                same_block_consumers: 0,
                cross_block_consumers: 0,
                has_later_block_use: false,
                has_phi_merge_use: false,
                has_debug_use: false,
                rhs_side_effectful: false,
            },
        );

        assert_eq!(decision, NoConsumerMaterializationDecision::Suppress);
    }

    #[test]
    fn no_consumer_materialization_decision_keeps_preserved_rhs() {
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &varnode(0x10),
            &HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("x".to_string())),
                rhs: Box::new(HirExpr::Const(0, int(32))),
                ty: NirType::Bool,
            },
            false,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::AliasUnsafe,
            ),
            Some(AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownNoConsumerFound,
                None,
                None,
                None,
            )),
            NoConsumerMaterializationProfile {
                same_block_consumers: 0,
                cross_block_consumers: 0,
                has_later_block_use: false,
                has_phi_merge_use: false,
                has_debug_use: false,
                rhs_side_effectful: false,
            },
        );

        assert_eq!(
            decision,
            NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::PreserveMaterialization
            )
        );
    }

    #[test]
    fn no_consumer_materialization_decision_keeps_non_unique_outputs() {
        let mut output = varnode(0x10);
        output.space_id = REGISTER_SPACE_ID;
        let decision = PreviewBuilder::classify_no_consumer_materialization_decision(
            &output,
            &HirExpr::Const(1, int(32)),
            false,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::AliasUnsafe,
            ),
            Some(AliasUnsafeHazard::new(
                AliasUnsafeHazardKind::UnknownNoConsumerFound,
                None,
                None,
                None,
            )),
            NoConsumerMaterializationProfile {
                same_block_consumers: 0,
                cross_block_consumers: 0,
                has_later_block_use: false,
                has_phi_merge_use: false,
                has_debug_use: false,
                rhs_side_effectful: false,
            },
        );

        assert_eq!(
            decision,
            NoConsumerMaterializationDecision::Keep(
                NoConsumerMaterializationKeepReason::StateVisibleOutput
            )
        );
    }
}
