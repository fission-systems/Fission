use fission_loader::loader::LoadedBinary;
use std::collections::HashMap;
use thiserror::Error;

pub type NirValueId = u32;
pub type StackSlotId = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirType {
    Unknown,
    Bool,
    Int { bits: u32, signed: bool },
    Ptr(Box<NirType>),
    Aggregate { size: u32 },
    Float { bits: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBinding {
    pub name: String,
    pub ty: NirType,
    pub surface_type_name: Option<String>,
    pub origin: Option<NirBindingOrigin>,
    pub initializer: Option<HirExpr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NirBindingOrigin {
    ParamIndex(usize),
    StackOffset(i64),
    DerivedFromStackOffset(i64),
    Temp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirFunction {
    pub name: String,
    pub address: u64,
    pub blocks: Vec<NirBlock>,
    pub locals: Vec<NirBinding>,
    pub params: Vec<NirBinding>,
    pub return_type: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBlock {
    pub id: u32,
    pub phis: Vec<String>,
    pub stmts: Vec<HirStmt>,
    pub terminator: NirTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirTerminator {
    Fallthrough(Option<u32>),
    Goto(u32),
    Branch {
        cond: HirExpr,
        true_target: u32,
        false_target: Option<u32>,
    },
    Return(Option<HirExpr>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<NirBinding>,
    pub locals: Vec<NirBinding>,
    pub return_type: NirType,
    pub surface_return_type_name: Option<String>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirBuildStats {
    #[serde(default)]
    pub forced_linear_structuring_count: usize,
    #[serde(default)]
    pub region_linearize_structuring_count: usize,
    #[serde(default)]
    pub region_linearize_heuristic_exit_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_non_structuring_failure_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_no_exit_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_failed_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_successor_inline_rejected_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_revisit_cycle_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_unsupported_terminator_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_non_advancing_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_irreducible_cfg_count: usize,
    #[serde(default)]
    pub structuring_scc_component_count: usize,
    #[serde(default)]
    pub structuring_irreducible_scc_count: usize,
    #[serde(default)]
    pub structuring_irreducible_header_count: usize,
    #[serde(default)]
    pub loop_control_explicit_reducer_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_break_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_continue_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_skipped_nested_scope_count: usize,
    #[serde(default)]
    pub promotion_candidate_count: usize,
    #[serde(default)]
    pub promoted_region_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_missing_terminal_join_target_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_empty_nonterminal_tail_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_gate_count: usize,
    #[serde(default)]
    pub discovery_seen_guarded_tail_like_shape_count: usize,
    #[serde(default)]
    pub discovery_rejected_noncanonical_layout_count: usize,
    #[serde(default)]
    pub canonicalized_guarded_tail_shape_count: usize,
    #[serde(default)]
    pub canonicalization_failed_multiple_payload_entries: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses_no_next_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: usize,
    #[serde(default)]
    pub canonicalization_failed_nonterminal_join_label: usize,
    #[serde(default)]
    pub canonicalization_failed_nested_tail_escape: usize,
    #[serde(default)]
    pub canonicalized_interleaved_join_use_count: usize,
    #[serde(default)]
    pub canonicalized_local_nonfallthrough_alias_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_nested_after_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_multiple_internal_predecessors_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_external_before_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_body_not_trivial_count: usize,
    #[serde(default)]
    pub canonicalization_failed_join_has_external_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_payload_crosses_join_count: usize,
    #[serde(default)]
    pub rejected_must_emit_label: usize,
    #[serde(default)]
    pub rejected_must_emit_label_surviving_middle_ref: usize,
    #[serde(default)]
    pub rejected_must_emit_label_surviving_external_ref: usize,
    #[serde(default)]
    pub rejected_must_emit_label_owner_conflict: usize,
    #[serde(default)]
    pub rejected_not_single_pred_succ: usize,
    #[serde(default)]
    pub rejected_external_entry: usize,
    #[serde(default)]
    pub rejected_loop_or_switch_target: usize,
}

impl NirBuildStats {
    pub fn merge_assign(&mut self, other: &Self) {
        self.forced_linear_structuring_count += other.forced_linear_structuring_count;
        self.region_linearize_structuring_count += other.region_linearize_structuring_count;
        self.region_linearize_heuristic_exit_count += other.region_linearize_heuristic_exit_count;
        self.region_linearize_rejected_non_structuring_failure_count +=
            other.region_linearize_rejected_non_structuring_failure_count;
        self.region_linearize_rejected_no_exit_count +=
            other.region_linearize_rejected_no_exit_count;
        self.region_linearize_rejected_body_lowering_failed_count +=
            other.region_linearize_rejected_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count +=
            other.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count +=
            other
                .region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count +=
            other.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count +=
            other.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count;
        self.region_linearize_rejected_body_lowering_successor_inline_rejected_count +=
            other.region_linearize_rejected_body_lowering_successor_inline_rejected_count;
        self.region_linearize_rejected_body_lowering_revisit_cycle_count +=
            other.region_linearize_rejected_body_lowering_revisit_cycle_count;
        self.region_linearize_rejected_body_lowering_unsupported_terminator_count +=
            other.region_linearize_rejected_body_lowering_unsupported_terminator_count;
        self.region_linearize_rejected_non_advancing_count +=
            other.region_linearize_rejected_non_advancing_count;
        self.region_linearize_rejected_irreducible_cfg_count +=
            other.region_linearize_rejected_irreducible_cfg_count;
        self.structuring_scc_component_count += other.structuring_scc_component_count;
        self.structuring_irreducible_scc_count += other.structuring_irreducible_scc_count;
        self.structuring_irreducible_header_count += other.structuring_irreducible_header_count;
        self.loop_control_explicit_reducer_count += other.loop_control_explicit_reducer_count;
        self.loop_control_rewrite_break_count += other.loop_control_rewrite_break_count;
        self.loop_control_rewrite_continue_count += other.loop_control_rewrite_continue_count;
        self.loop_control_rewrite_skipped_nested_scope_count +=
            other.loop_control_rewrite_skipped_nested_scope_count;
        self.promotion_candidate_count += other.promotion_candidate_count;
        self.promoted_region_count += other.promoted_region_count;
        self.promotion_rejected_by_shape_count += other.promotion_rejected_by_shape_count;
        self.promotion_rejected_by_shape_missing_terminal_join_target_count +=
            other.promotion_rejected_by_shape_missing_terminal_join_target_count;
        self.promotion_rejected_by_shape_empty_nonterminal_tail_count +=
            other.promotion_rejected_by_shape_empty_nonterminal_tail_count;
        self.promotion_rejected_by_gate_count += other.promotion_rejected_by_gate_count;
        self.discovery_seen_guarded_tail_like_shape_count +=
            other.discovery_seen_guarded_tail_like_shape_count;
        self.discovery_rejected_noncanonical_layout_count +=
            other.discovery_rejected_noncanonical_layout_count;
        self.canonicalized_guarded_tail_shape_count += other.canonicalized_guarded_tail_shape_count;
        self.canonicalization_failed_multiple_payload_entries +=
            other.canonicalization_failed_multiple_payload_entries;
        self.canonicalization_failed_interleaved_join_uses +=
            other.canonicalization_failed_interleaved_join_uses;
        self.canonicalization_failed_interleaved_join_uses_no_next_label_count +=
            other.canonicalization_failed_interleaved_join_uses_no_next_label_count;
        self.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count +=
            other.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count;
        self.canonicalization_failed_nonterminal_join_label +=
            other.canonicalization_failed_nonterminal_join_label;
        self.canonicalization_failed_nested_tail_escape +=
            other.canonicalization_failed_nested_tail_escape;
        self.canonicalized_interleaved_join_use_count +=
            other.canonicalized_interleaved_join_use_count;
        self.canonicalized_local_nonfallthrough_alias_count +=
            other.canonicalized_local_nonfallthrough_alias_count;
        self.canonicalization_failed_alias_not_fallthrough_count +=
            other.canonicalization_failed_alias_not_fallthrough_count;
        self.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count +=
            other.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count;
        self.canonicalization_failed_alias_not_fallthrough_nested_after_label_count +=
            other.canonicalization_failed_alias_not_fallthrough_nested_after_label_count;
        self.canonicalization_failed_alias_has_multiple_internal_predecessors_count +=
            other.canonicalization_failed_alias_has_multiple_internal_predecessors_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_external_before_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_external_before_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count;
        self.canonicalization_failed_alias_body_not_trivial_count +=
            other.canonicalization_failed_alias_body_not_trivial_count;
        self.canonicalization_failed_join_has_external_ref_count +=
            other.canonicalization_failed_join_has_external_ref_count;
        self.canonicalization_failed_payload_crosses_join_count +=
            other.canonicalization_failed_payload_crosses_join_count;
        self.rejected_must_emit_label += other.rejected_must_emit_label;
        self.rejected_must_emit_label_surviving_middle_ref +=
            other.rejected_must_emit_label_surviving_middle_ref;
        self.rejected_must_emit_label_surviving_external_ref +=
            other.rejected_must_emit_label_surviving_external_ref;
        self.rejected_must_emit_label_owner_conflict +=
            other.rejected_must_emit_label_owner_conflict;
        self.rejected_not_single_pred_succ += other.rejected_not_single_pred_succ;
        self.rejected_external_entry += other.rejected_external_entry;
        self.rejected_loop_or_switch_target += other.rejected_loop_or_switch_target;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmt {
    Assign {
        lhs: HirLValue,
        rhs: HirExpr,
    },
    Expr(HirExpr),
    Block(Vec<HirStmt>),
    Switch {
        expr: HirExpr,
        cases: Vec<HirSwitchCase>,
        default: Vec<HirStmt>,
    },
    If {
        cond: HirExpr,
        then_body: Vec<HirStmt>,
        else_body: Vec<HirStmt>,
    },
    While {
        cond: HirExpr,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        cond: HirExpr,
    },
    Label(String),
    Goto(String),
    Return(Option<HirExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirSwitchCase {
    pub values: Vec<i64>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirLValue {
    Var(String),
    Deref {
        ptr: Box<HirExpr>,
        ty: NirType,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        elem_ty: NirType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExpr {
    Var(String),
    Const(i64, NirType),
    Cast {
        ty: NirType,
        expr: Box<HirExpr>,
    },
    Unary {
        op: HirUnaryOp,
        expr: Box<HirExpr>,
        ty: NirType,
    },
    Binary {
        op: HirBinaryOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
        ty: NirType,
    },
    Call {
        target: String,
        args: Vec<HirExpr>,
        ty: NirType,
    },
    Load {
        ptr: Box<HirExpr>,
        ty: NirType,
    },
    PtrOffset {
        base: Box<HirExpr>,
        offset: i64,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        elem_ty: NirType,
    },
    AggregateCopy {
        src: Box<HirExpr>,
        size: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LogicalAnd,
    LogicalOr,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Sar,
    Eq,
    Ne,
    Lt,
    Le,
    SLt,
    SLe,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirRenderOptions {
    pub pe_x64_only: bool,
    pub is_64bit: bool,
    pub pointer_size: u32,
    pub format: String,
    pub image_base: u64,
    pub sections: Vec<(u64, u64)>,
    pub region_linearize_structuring: bool,
    pub force_linear_structuring: bool,
    #[serde(default)]
    pub conservative_irreducible_fallback: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirTypeContext {
    pub call_targets: HashMap<u64, String>,
    pub call_param_rules: Vec<NirCallParamRule>,
    pub function_hints: Option<NirFunctionHints>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirHintStats {
    pub explicit_param_name_hits: usize,
    pub explicit_local_name_hits: usize,
    pub explicit_param_type_hits: usize,
    pub explicit_local_type_hits: usize,
    pub explicit_return_type_hit: usize,
    pub heuristic_pointer_alias_hits: usize,
    pub heuristic_local_surface_hits: usize,
    pub derived_origin_type_hits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirCallParamRule {
    pub callee_name: String,
    pub arg_index: usize,
    pub pointer_alias: String,
    pub pointee_alias: String,
    pub pointer_size: u32,
    pub pointee_sizes: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirFunctionHints {
    pub param_names: Vec<String>,
    pub param_type_names: HashMap<usize, String>,
    pub stack_local_names: HashMap<i64, String>,
    pub stack_local_type_names: HashMap<i64, String>,
    pub return_type_name: Option<String>,
}

impl NirRenderOptions {
    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let sections = binary
            .inner()
            .sections
            .iter()
            .map(|section| {
                (
                    section.virtual_address,
                    section.virtual_address + section.virtual_size as u64,
                )
            })
            .collect();
        Self {
            pe_x64_only: true,
            is_64bit: binary.is_64bit,
            pointer_size: if binary.is_64bit { 8 } else { 4 },
            format: binary.format.clone(),
            image_base: binary.inner().image_base,
            sections,
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
        }
    }

    pub(super) fn is_supported_pe(&self) -> bool {
        self.format.to_ascii_uppercase().starts_with("PE")
    }

    pub(super) fn is_mapped_global(&self, address: u64) -> bool {
        self.sections
            .iter()
            .any(|(start, end)| address >= *start && address < *end)
    }
}

pub type PreviewBuildStats = NirBuildStats;
pub type MlilPreviewOptions = NirRenderOptions;
pub type PreviewTypeContext = NirTypeContext;
pub type PreviewHintStats = NirHintStats;
pub type PreviewCallParamRule = NirCallParamRule;
pub type PreviewFunctionHints = NirFunctionHints;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuringFailureKind {
    RegionShape,
    PhiJoin,
    IndirectCallRegion,
}

impl StructuringFailureKind {
    pub const fn preview_block_signature(self) -> &'static str {
        match self {
            StructuringFailureKind::RegionShape => "unsupported_cfg_region_shape",
            StructuringFailureKind::PhiJoin => "unsupported_cfg_phi_join",
            StructuringFailureKind::IndirectCallRegion => "unsupported_cfg_indirect_call_region",
        }
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MlilPreviewError {
    #[error("mlil-preview currently supports PE x64 only")]
    UnsupportedArchitecture,
    #[error("unsupported architecture in mlil-preview")]
    UnsupportedArchitectureDetailed,
    #[error("unsupported control flow in mlil-preview")]
    UnsupportedControlFlow,
    #[error("unsupported branch target in mlil-preview")]
    UnsupportedCfgBranchTarget,
    #[error("unsupported region shape in mlil-preview")]
    UnsupportedCfgRegionShape,
    #[error("unsupported phi join in mlil-preview")]
    UnsupportedCfgPhiJoin,
    #[error("unsupported indirect call region in mlil-preview")]
    UnsupportedCfgIndirectCallRegion,
    #[error("unsupported pcode pattern: {0}")]
    UnsupportedPattern(&'static str),
    #[error("value lowering failed")]
    LoweringFailed,
    #[error("value lowering failed on multiequal")]
    UnsupportedExprMultiequal,
    #[error("value lowering failed on varnode")]
    UnsupportedExprVarnodeLowering,
    #[error("value lowering failed on varnode: unsupported address materialization")]
    UnsupportedExprAddressMaterialization,
    #[error("value lowering failed on varnode: unsupported indirect value source")]
    UnsupportedExprIndirectValueSource,
    #[error("value lowering failed on varnode: unsupported piece/subpiece shape")]
    UnsupportedExprPieceShape,
    #[error("value lowering failed on varnode: unsupported ptr arithmetic shape")]
    UnsupportedExprPtrArithmetic,
    #[error("value lowering failed on varnode: unsupported memory-backed varnode")]
    UnsupportedExprMemoryBackedVarnode,
}

impl MlilPreviewError {
    pub const fn structuring_failure_kind(self) -> Option<StructuringFailureKind> {
        match self {
            MlilPreviewError::UnsupportedCfgRegionShape => {
                Some(StructuringFailureKind::RegionShape)
            }
            MlilPreviewError::UnsupportedCfgPhiJoin => Some(StructuringFailureKind::PhiJoin),
            MlilPreviewError::UnsupportedCfgIndirectCallRegion => {
                Some(StructuringFailureKind::IndirectCallRegion)
            }
            _ => None,
        }
    }
}
