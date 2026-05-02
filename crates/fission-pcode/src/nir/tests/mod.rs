use super::*;
use crate::pcode::{PcodeBasicBlock, PcodeOp};

mod bootstrap_x86;
mod calling_convention;
mod entry_param_promotion;
mod normalize_arith;
mod normalize_bitstream;
mod normalize_defuse;
mod normalize_flag_recovery;
mod normalize_slots;
mod relative_branch_targets;
mod snapshot_printer;
mod structuring_conditionals;
mod structuring_guarded_tail;
mod structuring_linear;
mod structuring_loops;
mod structuring_misc;
mod structuring_switch;
mod type_hints_aggregates;
mod type_hints_aliases;
mod type_hints_function_hints;
mod type_hints_imports;
mod type_hints_stack_slots;
mod unique_x86_regs;

fn reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn uniq(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn cst(value: i64, size: u32) -> Varnode {
    Varnode::constant(value, size)
}

fn preview_options() -> MlilPreviewOptions {
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

fn preview_options_x86() -> MlilPreviewOptions {
    MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: false,
        pointer_size: 4,
        format: "PE".to_string(),
        image_base: 0x400000,
        sections: vec![(0x401000, 0x402000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        calling_convention: Default::default(),
    }
}

/// Shared preview context for tests that lower `CALL`/import sites at `0x14012c378` as `GetClientRect`
/// with `LPRECT`/`RECT` param rules — matches [`PreviewTypeContext::call_target_refs`] resolution in
/// [`crate::nir::builder::lower_expr`].
fn get_client_rect_preview_type_context() -> PreviewTypeContext {
    let mut context = PreviewTypeContext::default();
    let addr = 0x14012c378_u64;
    context
        .call_targets
        .insert(addr, "GetClientRect".to_string());
    context.call_target_refs.insert(
        addr,
        CallTargetRef {
            address: Some(addr),
            symbol: "GetClientRect".to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 100,
        },
    );
    context.call_param_rules.push(PreviewCallParamRule {
        callee_address: Some(addr),
        callee_name: "GetClientRect".to_string(),
        arg_index: 1,
        pointer_alias: "LPRECT".to_string(),
        pointee_alias: "RECT".to_string(),
        pointer_size: 8,
        pointee_sizes: vec![16],
    });
    context
}

#[test]
fn target_profile_unifies_pe_x64_auto_gate() {
    let options = preview_options();
    let profile = options.target_profile();
    let facts = NirAdmissionFacts {
        block_count: 12,
        op_count: 600,
        max_multiequal_fanin: 4,
    };

    assert_eq!(profile.format_family, FormatFamily::Pe);
    assert_eq!(profile.admission_class, AdmissionClass::PeX64Auto);
    assert!(profile.preview_eligible);
    assert!(profile.worker_eligible);
    assert!(profile.auto_admission_eligible(facts));
    assert!(!profile.if_lowering_budget_enabled());
}

#[test]
fn target_profile_unifies_pe_x86_budget_without_auto_gate() {
    let options = preview_options_x86();
    let profile = options.target_profile();
    let facts = NirAdmissionFacts {
        block_count: 4,
        op_count: 32,
        max_multiequal_fanin: 1,
    };

    assert_eq!(profile.format_family, FormatFamily::Pe);
    assert_eq!(profile.admission_class, AdmissionClass::PeX86PreviewOnly);
    assert!(profile.preview_eligible);
    assert!(!profile.worker_eligible);
    assert!(!profile.auto_admission_eligible(facts));
    assert!(profile.if_lowering_budget_enabled());
}
