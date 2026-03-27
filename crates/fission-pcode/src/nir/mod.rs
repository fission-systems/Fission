use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

mod builder;
mod cfg;
mod normalize;
mod piece;
mod printer;
mod structuring;
#[cfg(test)]
mod tests;
mod types;

pub use self::types::*;
use self::{builder::*, cfg::*, normalize::*, printer::*, structuring::*};

thread_local! {
    static LAST_PREVIEW_BUILD_STATS: RefCell<Option<PreviewBuildStats>> = const { RefCell::new(None) };
    static LAST_PREVIEW_HINT_STATS: RefCell<Option<PreviewHintStats>> = const { RefCell::new(None) };
}

const UNIQUE_SPACE_ID: u64 = 3;
const REGISTER_SPACE_ID: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackBase {
    Rsp,
    Rbp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StackSlot {
    id: StackSlotId,
    name: String,
    ty: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VarnodeKey {
    space_id: u64,
    offset: u64,
    size: u32,
    is_constant: bool,
    constant_val: i64,
}

impl From<&Varnode> for VarnodeKey {
    fn from(value: &Varnode) -> Self {
        Self {
            space_id: value.space_id,
            offset: value.offset,
            size: value.size,
            is_constant: value.is_constant,
            constant_val: value.constant_val,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MaterializedVarnodeKey {
    varnode: VarnodeKey,
    def_addr: u64,
    def_seq: u32,
}

impl MaterializedVarnodeKey {
    fn new(vn: &Varnode, op: &PcodeOp) -> Self {
        Self {
            varnode: VarnodeKey::from(vn),
            def_addr: op.address,
            def_seq: op.seq_num,
        }
    }
}

#[derive(Debug)]
struct PreviewBuilder<'a> {
    pcode: &'a PcodeFunction,
    options: &'a MlilPreviewOptions,
    type_context: Option<&'a PreviewTypeContext>,
    defs: HashMap<VarnodeKey, DefSite<'a>>,
    address_to_index: HashMap<u64, usize>,
    block_target_keys: Vec<u64>,
    target_key_to_index: HashMap<u64, usize>,
    layout_fallthrough: Vec<Option<usize>>,
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    params: BTreeMap<usize, NirBinding>,
    locals: BTreeMap<i64, StackSlot>,
    locals_next_id: StackSlotId,
    temps: BTreeMap<String, NirBinding>,
    temp_next_id: u32,
    materialized_vns: HashMap<MaterializedVarnodeKey, String>,
    current_lowering_site: Option<LoweringSite>,
    register_param_aliases: HashMap<u64, usize>,
    stack_frame_size: i64,
    linear_exit_cache: HashMap<usize, Option<LinearExit>>,
    linear_body_cache: HashMap<LinearBodyCacheKey, LinearBodyCachedOutcome>,
    active_linear_body_keys: HashSet<LinearBodyCacheKey>,
    active_conditional_tail_keys: HashSet<ConditionalTailKey>,
    jump_targets_cache: Option<HashSet<u64>>,
    active_trace_id: Option<u64>,
    last_trace_id: Option<u64>,
    next_trace_id: u64,
    lowering_site_depth: usize,
    forced_linear_structuring_count: usize,
    region_linearize_structuring_count: usize,
    region_linearize_heuristic_exit_count: usize,
    region_linearize_rejected_non_structuring_failure_count: usize,
    region_linearize_rejected_no_exit_count: usize,
    region_linearize_rejected_body_lowering_failed_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count:
        usize,
    region_linearize_rejected_body_lowering_successor_inline_rejected_count: usize,
    region_linearize_rejected_body_lowering_revisit_cycle_count: usize,
    region_linearize_rejected_body_lowering_unsupported_terminator_count: usize,
    region_linearize_rejected_non_advancing_count: usize,
    region_linearize_rejected_irreducible_cfg_count: usize,
    structuring_scc_component_count: usize,
    structuring_irreducible_scc_count: usize,
    structuring_irreducible_header_count: usize,
    loop_control_explicit_reducer_count: usize,
    loop_control_rewrite_break_count: usize,
    loop_control_rewrite_continue_count: usize,
    loop_control_rewrite_skipped_nested_scope_count: usize,
    promotion_candidate_count: usize,
    promoted_region_count: usize,
    promotion_rejected_by_shape_count: usize,
    promotion_rejected_by_shape_missing_terminal_join_target_count: usize,
    promotion_rejected_by_shape_empty_nonterminal_tail_count: usize,
    promotion_rejected_by_gate_count: usize,
    discovery_seen_guarded_tail_like_shape_count: usize,
    discovery_rejected_noncanonical_layout_count: usize,
    canonicalized_guarded_tail_shape_count: usize,
    canonicalization_failed_multiple_payload_entries: usize,
    canonicalization_failed_interleaved_join_uses: usize,
    canonicalization_failed_nonterminal_join_label: usize,
    canonicalization_failed_nested_tail_escape: usize,
    canonicalized_interleaved_join_use_count: usize,
    canonicalized_local_nonfallthrough_alias_count: usize,
    canonicalization_failed_alias_not_fallthrough_count: usize,
    canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: usize,
    canonicalization_failed_alias_not_fallthrough_nested_after_label_count: usize,
    canonicalization_failed_alias_has_multiple_internal_predecessors_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_external_before_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: usize,
    canonicalization_failed_alias_body_not_trivial_count: usize,
    canonicalization_failed_join_has_external_ref_count: usize,
    canonicalization_failed_payload_crosses_join_count: usize,
    rejected_must_emit_label: usize,
    rejected_must_emit_label_surviving_middle_ref: usize,
    rejected_must_emit_label_surviving_external_ref: usize,
    rejected_must_emit_label_owner_conflict: usize,
    rejected_not_single_pred_succ: usize,
    rejected_external_entry: usize,
    rejected_loop_or_switch_target: usize,
}

#[derive(Debug, Clone, Copy)]
struct DefSite<'a> {
    block_idx: usize,
    op_idx: usize,
    op: &'a PcodeOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LoweringSite {
    block_idx: usize,
    op_idx: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoweredTerminator {
    Fallthrough(Option<u64>),
    Goto(u64),
    Cond {
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
    },
    Return(Option<HirExpr>),
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LinearExit {
    Join(usize),
    Return,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LinearBodyCacheKey {
    start_idx: usize,
    exit: LinearExit,
    region_recovery: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ConditionalTailKey {
    true_idx: usize,
    false_idx: usize,
    exit: LinearExit,
    region_recovery: bool,
}

#[derive(Debug)]
struct IfLoweringBudget {
    enabled: bool,
    start: Instant,
    subcalls: usize,
    tripped: bool,
    idx: usize,
    block_addr: u64,
    label: &'static str,
}

const X86_TRY_LOWER_IF_BUDGET_MS: f64 = 10.0;
const X86_TRY_LOWER_IF_SUBCALL_LIMIT: usize = 512;

#[derive(Debug, Clone)]
struct SubpieceOrigin {
    base: VarnodeKey,
    base_vn: Varnode,
    base_size: u32,
    byte_offset: i64,
    piece_size: u32,
}

pub fn render_mlil_preview(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_context(pcode, name, address, options, None)
}

pub fn render_nir(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview(pcode, name, address, options)
}

pub fn render_mlil_preview_with_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Result<String, MlilPreviewError> {
    LAST_PREVIEW_BUILD_STATS.with(|slot| {
        *slot.borrow_mut() = None;
    });
    LAST_PREVIEW_HINT_STATS.with(|slot| {
        *slot.borrow_mut() = None;
    });
    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    let debug_log = |stage: &str| {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(format!("/tmp/fission_preview_{address:x}.log"))
                .and_then(|mut f| {
                    std::io::Write::write_all(
                        &mut f,
                        format!("[mlil-preview] stage={stage}\n").as_bytes(),
                    )
                });
        }
    };
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::remove_file(format!("/tmp/fission_preview_{address:x}_unsupported.json"));
    }
    if options.pe_x64_only && !options.is_supported_pe() {
        return Err(MlilPreviewError::UnsupportedArchitectureDetailed);
    }

    let build_start = Instant::now();
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=build_hir start fn=0x{address:x}");
    }
    debug_log("build_hir_start");
    let mut builder = PreviewBuilder::new(pcode, options, type_context);
    let mut hir = builder.build_hir(name, address).map_err(|err| {
        LAST_PREVIEW_BUILD_STATS.with(|slot| {
            *slot.borrow_mut() = Some(builder.preview_build_stats());
        });
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=build_hir error fn=0x{address:x} err={err}");
        }
        if matches!(err, MlilPreviewError::UnsupportedPattern("opcode")) {
            builder.record_unsupported_inventory_event(
                "build_hir_error",
                None,
                None,
                None,
                Some(address),
                None,
                true,
                "render_mlil_preview_with_context",
            );
        }
        debug_log("build_hir_error");
        err
    })?;
    let mut build_stats = builder.preview_build_stats();
    if diag {
        eprintln!(
            "[DIAG] build_hir done: fn=0x{address:x} elapsed={:.3}s body_stmts={} locals={}",
            build_start.elapsed().as_secs_f64(),
            hir.body.len(),
            hir.locals.len()
        );
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=normalize start fn=0x{address:x}");
    }
    debug_log("normalize_start");
    let normalize_start = Instant::now();
    normalize_hir_function(&mut hir);
    let normalized_discovery_stats = discover_guarded_tail_candidates_for_stats(&hir.body);
    build_stats.promotion_candidate_count += normalized_discovery_stats.promotion_candidate_count;
    build_stats.promoted_region_count += normalized_discovery_stats.promoted_region_count;
    build_stats.promotion_rejected_by_shape_count +=
        normalized_discovery_stats.promotion_rejected_by_shape_count;
    build_stats.promotion_rejected_by_gate_count +=
        normalized_discovery_stats.promotion_rejected_by_gate_count;
    build_stats.discovery_seen_guarded_tail_like_shape_count +=
        normalized_discovery_stats.discovery_seen_guarded_tail_like_shape_count;
    build_stats.discovery_rejected_noncanonical_layout_count +=
        normalized_discovery_stats.discovery_rejected_noncanonical_layout_count;
    build_stats.canonicalized_guarded_tail_shape_count +=
        normalized_discovery_stats.canonicalized_guarded_tail_shape_count;
    build_stats.canonicalization_failed_multiple_payload_entries +=
        normalized_discovery_stats.canonicalization_failed_multiple_payload_entries;
    build_stats.canonicalization_failed_interleaved_join_uses +=
        normalized_discovery_stats.canonicalization_failed_interleaved_join_uses;
    build_stats.canonicalization_failed_nonterminal_join_label +=
        normalized_discovery_stats.canonicalization_failed_nonterminal_join_label;
    build_stats.canonicalization_failed_nested_tail_escape +=
        normalized_discovery_stats.canonicalization_failed_nested_tail_escape;
    build_stats.canonicalized_interleaved_join_use_count +=
        normalized_discovery_stats.canonicalized_interleaved_join_use_count;
    build_stats.canonicalized_local_nonfallthrough_alias_count +=
        normalized_discovery_stats.canonicalized_local_nonfallthrough_alias_count;
    build_stats.canonicalization_failed_alias_not_fallthrough_count +=
        normalized_discovery_stats.canonicalization_failed_alias_not_fallthrough_count;
    build_stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count += normalized_discovery_stats
        .canonicalization_failed_alias_not_fallthrough_top_level_after_label_count;
    build_stats.canonicalization_failed_alias_not_fallthrough_nested_after_label_count += normalized_discovery_stats
        .canonicalization_failed_alias_not_fallthrough_nested_after_label_count;
    build_stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count +=
        normalized_discovery_stats
            .canonicalization_failed_alias_has_multiple_internal_predecessors_count;
    build_stats.canonicalization_failed_alias_has_nonlocal_ref_count +=
        normalized_discovery_stats.canonicalization_failed_alias_has_nonlocal_ref_count;
    build_stats.canonicalization_failed_alias_body_not_trivial_count +=
        normalized_discovery_stats.canonicalization_failed_alias_body_not_trivial_count;
    build_stats.canonicalization_failed_join_has_external_ref_count +=
        normalized_discovery_stats.canonicalization_failed_join_has_external_ref_count;
    build_stats.canonicalization_failed_payload_crosses_join_count +=
        normalized_discovery_stats.canonicalization_failed_payload_crosses_join_count;
    build_stats.rejected_must_emit_label += normalized_discovery_stats.rejected_must_emit_label;
    build_stats.rejected_not_single_pred_succ +=
        normalized_discovery_stats.rejected_not_single_pred_succ;
    build_stats.rejected_external_entry += normalized_discovery_stats.rejected_external_entry;
    build_stats.rejected_loop_or_switch_target +=
        normalized_discovery_stats.rejected_loop_or_switch_target;
    LAST_PREVIEW_BUILD_STATS.with(|slot| {
        *slot.borrow_mut() = Some(build_stats);
    });
    if diag {
        eprintln!(
            "[DIAG] normalize stage done: fn=0x{address:x} elapsed={:.3}s body_stmts={} locals={}",
            normalize_start.elapsed().as_secs_f64(),
            hir.body.len(),
            hir.locals.len()
        );
    }
    debug_log("normalize_done");
    if let Some(context) = type_context {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=type_hints start fn=0x{address:x}");
        }
        debug_log("type_hints_start");
        let type_hints_start = Instant::now();
        let hint_stats = apply_preview_type_hints(&mut hir, context);
        LAST_PREVIEW_HINT_STATS.with(|slot| {
            *slot.borrow_mut() = Some(hint_stats);
        });
        if diag {
            eprintln!(
                "[DIAG] type_hints done: fn=0x{address:x} elapsed={:.3}s",
                type_hints_start.elapsed().as_secs_f64()
            );
        }
        debug_log("type_hints_done");
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=print start fn=0x{address:x}");
    }
    debug_log("print_start");
    let print_start = Instant::now();
    let rendered = print_hir_function(&hir);
    if diag {
        eprintln!(
            "[DIAG] print done: fn=0x{address:x} elapsed={:.3}s",
            print_start.elapsed().as_secs_f64()
        );
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=print done fn=0x{address:x}");
    }
    debug_log("print_done");
    Ok(rendered)
}

pub fn render_nir_with_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
    type_context: Option<&NirTypeContext>,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_context(pcode, name, address, options, type_context)
}

pub fn take_last_preview_build_stats() -> Option<PreviewBuildStats> {
    LAST_PREVIEW_BUILD_STATS.with(|slot| slot.borrow_mut().take())
}

pub fn take_last_preview_hint_stats() -> Option<PreviewHintStats> {
    LAST_PREVIEW_HINT_STATS.with(|slot| slot.borrow_mut().take())
}

pub fn take_last_nir_build_stats() -> Option<NirBuildStats> {
    take_last_preview_build_stats()
}

pub fn take_last_nir_hint_stats() -> Option<NirHintStats> {
    take_last_preview_hint_stats()
}

fn is_comparison(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
    )
}

fn map_binary_op(opcode: PcodeOpcode) -> Result<HirBinaryOp, MlilPreviewError> {
    match opcode {
        PcodeOpcode::IntAdd => Ok(HirBinaryOp::Add),
        PcodeOpcode::IntSub => Ok(HirBinaryOp::Sub),
        PcodeOpcode::IntMult => Ok(HirBinaryOp::Mul),
        PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv => Ok(HirBinaryOp::Div),
        PcodeOpcode::IntRem | PcodeOpcode::IntSRem => Ok(HirBinaryOp::Mod),
        PcodeOpcode::IntAnd => Ok(HirBinaryOp::And),
        PcodeOpcode::BoolAnd => Ok(HirBinaryOp::LogicalAnd),
        PcodeOpcode::IntOr => Ok(HirBinaryOp::Or),
        PcodeOpcode::BoolOr => Ok(HirBinaryOp::LogicalOr),
        PcodeOpcode::IntXor | PcodeOpcode::BoolXor => Ok(HirBinaryOp::Xor),
        PcodeOpcode::IntLeft => Ok(HirBinaryOp::Shl),
        PcodeOpcode::IntRight => Ok(HirBinaryOp::Shr),
        PcodeOpcode::IntSRight => Ok(HirBinaryOp::Sar),
        PcodeOpcode::IntEqual => Ok(HirBinaryOp::Eq),
        PcodeOpcode::IntNotEqual => Ok(HirBinaryOp::Ne),
        PcodeOpcode::IntLess => Ok(HirBinaryOp::Lt),
        PcodeOpcode::IntLessEqual => Ok(HirBinaryOp::Le),
        PcodeOpcode::IntSLess => Ok(HirBinaryOp::SLt),
        PcodeOpcode::IntSLessEqual => Ok(HirBinaryOp::SLe),
        _ => Err(MlilPreviewError::UnsupportedPattern("binary op")),
    }
}

fn type_from_size(size: u32, signed: bool) -> NirType {
    match size {
        1 => NirType::Int { bits: 8, signed },
        2 => NirType::Int { bits: 16, signed },
        4 => NirType::Int { bits: 32, signed },
        8 => NirType::Int { bits: 64, signed },
        16 | 24 | 32 => NirType::Aggregate { size },
        _ => NirType::Unknown,
    }
}

fn is_materializable_output_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::Load
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow
            | PcodeOpcode::PopCount
            | PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Piece
            | PcodeOpcode::SubPiece
            | PcodeOpcode::MultiEqual
            | PcodeOpcode::Indirect
    )
}

fn next_temp_name(ty: &NirType, next_id: &mut u32) -> String {
    let prefix = match ty {
        NirType::Bool => "bVar",
        NirType::Int {
            bits: 32,
            signed: true,
        } => "iVar",
        NirType::Int {
            bits: 32,
            signed: false,
        } => "uVar",
        _ => "xVar",
    };
    let name = format!("{prefix}{}", *next_id);
    *next_id += 1;
    name
}

fn register_name_with_param(offset: u64, _size: u32) -> Option<(&'static str, Option<usize>)> {
    match offset {
        0x08 => Some(("param_1", Some(0))),
        0x10 => Some(("param_2", Some(1))),
        0x80 => Some(("param_3", Some(2))),
        0x88 => Some(("param_4", Some(3))),
        0x00 => Some(("rax", None)),
        0x18 => Some(("rbx", None)),
        0x20 => Some(("rsp", None)),
        0x28 => Some(("rbp", None)),
        0x30 => Some(("rsi", None)),
        0x38 => Some(("rdi", None)),
        0x90 => Some(("r10", None)),
        0x98 => Some(("r11", None)),
        0xa0 => Some(("r12", None)),
        0xa8 => Some(("r13", None)),
        0xb0 => Some(("r14", None)),
        0xb8 => Some(("r15", None)),
        _ => None,
    }
}

fn register_name(offset: u64, size: u32) -> &'static str {
    register_name_with_param(offset, size)
        .map(|(name, _)| name)
        .unwrap_or("reg")
}

fn x86_register_name(offset: u64, size: u32) -> Option<&'static str> {
    match (offset, size) {
        (0x00, 4) => Some("eax"),
        (0x04, 4) => Some("ecx"),
        (0x08, 4) => Some("edx"),
        (0x0c, 4) => Some("ebx"),
        (0x10, 4) => Some("esp"),
        (0x14, 4) => Some("ebp"),
        (0x18, 4) => Some("esi"),
        (0x1c, 4) => Some("edi"),
        _ => None,
    }
}

fn expr_type(expr: &HirExpr) -> NirType {
    match expr {
        HirExpr::Var(_) => NirType::Unknown,
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        HirExpr::Cast { ty, .. } => ty.clone(),
        HirExpr::PtrOffset { .. } => NirType::Ptr(Box::new(NirType::Unknown)),
        HirExpr::AggregateCopy { size, .. } => NirType::Aggregate { size: *size },
    }
}
