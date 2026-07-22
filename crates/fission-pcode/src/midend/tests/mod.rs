use super::*;
use crate::pcode::{PcodeBasicBlock, PcodeOp};

/// Test-only helper: several `midend/tests` files build a `DirStmt` fixture
/// (the shape a normalize/structuring pass actually consumes) and want to
/// assert on its rendered text via the real printer, which is `HirStmt`-
/// typed. Converts via `dir_stmt_to_hir_stmt` rather than duplicating
/// `print_stmt` for `DirStmt`.
#[allow(dead_code)]
pub(super) fn print_dir_stmt(stmt: &fission_midend_dir::DirStmt) -> String {
    print_stmt(&fission_midend_dir::ir::dir_stmt_to_hir_stmt(stmt.clone()))
}

/// Same rationale as [`print_dir_stmt`], for whole-function fixtures.
#[allow(dead_code)]
pub(super) fn print_dir_function(func: &fission_midend_dir::DirFunction) -> String {
    let hir_body = fission_midend_dir::ir::dir_stmts_to_hir_stmts(func.body.clone());
    print_hir_function(&func.clone().into_hir_function(hir_body))
}

mod bootstrap_x86;
mod calling_convention;
mod entry_param_promotion;
mod normalize_arith;
mod normalize_defuse;
mod normalize_flag_recovery;
mod normalize_slots;
mod relative_branch_targets;
mod signum_struct;
mod clamp_dump;
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
    let mut options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: true,
        is_big_endian: false,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0x1400_0000,
        sections: vec![(0x1400_1000, 0x1400_2000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: CallingConvention::WindowsX64,
        ..Default::default()
    };
    apply_win64_cspec(&mut options);
    options
}

fn configure_options_for_abi(options: &mut MlilPreviewOptions, abi: CallingConvention) {
    options.calling_convention = abi;
    match abi {
        CallingConvention::SystemVAmd64 => {
            options.format = "ELF64".to_string();
            options.pe_x64_only = false;
            options.is_64bit = true;
            options.pointer_size = 8;
        }
        CallingConvention::AArch64 => {
            options.format = "ELF64".to_string();
            options.pe_x64_only = false;
            options.is_64bit = true;
            options.pointer_size = 8;
        }
        CallingConvention::Arm32 => {
            options.is_64bit = false;
            options.pointer_size = 4;
            options.format = "ELF".to_string();
            options.pe_x64_only = false;
        }
        CallingConvention::PowerPc32 => {
            options.is_64bit = false;
            options.pointer_size = 4;
            options.is_big_endian = true;
            options.format = "ELF".to_string();
            options.pe_x64_only = false;
        }
        CallingConvention::PowerPc64 => {
            options.is_64bit = true;
            options.pointer_size = 8;
            options.is_big_endian = true;
            options.format = "ELF64".to_string();
            options.pe_x64_only = false;
        }
        CallingConvention::LoongArch32 => {
            options.is_64bit = false;
            options.pointer_size = 4;
            options.format = "ELF".to_string();
            options.pe_x64_only = false;
        }
        CallingConvention::LoongArch64 => {
            options.is_64bit = true;
            options.pointer_size = 8;
            options.format = "ELF64".to_string();
            options.pe_x64_only = false;
        }
        CallingConvention::Mips32 => {
            options.is_64bit = false;
            options.pointer_size = 4;
            options.format = "ELF".to_string();
            options.pe_x64_only = false;
        }
        CallingConvention::Mips64 => {
            options.is_64bit = true;
            options.pointer_size = 8;
            options.format = "ELF64".to_string();
            options.pe_x64_only = false;
        }
        CallingConvention::X86_32 => {
            options.is_64bit = false;
            options.pointer_size = 4;
        }
        CallingConvention::WindowsX64 => {
            options.pe_x64_only = true;
            options.is_64bit = true;
            options.pointer_size = 8;
            options.format = "PE".to_string();
        }
    }
    apply_cspec_for_convention(options);
}

fn preview_options_for(abi: CallingConvention) -> MlilPreviewOptions {
    let mut options = preview_options();
    configure_options_for_abi(&mut options, abi);
    options
}

fn preview_options_win64() -> MlilPreviewOptions {
    let mut options = preview_options();
    apply_win64_cspec(&mut options);
    options
}

fn apply_win64_cspec(options: &mut MlilPreviewOptions) {
    crate::midend::cspec::test_maps::apply_preview_cspec(options);
}

fn apply_cspec_for_convention(options: &mut MlilPreviewOptions) {
    crate::midend::cspec::test_maps::apply_preview_cspec(options);
}

pub(super) fn int_params_for(abi: CallingConvention) -> Vec<u64> {
    preview_options_for(abi)
        .cspec_param_offsets
        .unwrap_or_default()
}

pub(super) fn abi_state_for(abi: CallingConvention, stack_frame_size: i64) -> AbiState {
    let options = preview_options_for(abi);
    AbiState::new_with_cspec(
        abi,
        options.is_64bit,
        options.pointer_size,
        stack_frame_size,
        options.cspec_param_offsets.clone(),
        options.cspec_stack_arg_base,
        options.cspec_extrapop,
    )
}

fn preview_options_x86() -> MlilPreviewOptions {
    let mut options = MlilPreviewOptions {
        pe_x64_only: true,
        is_64bit: false,
        is_big_endian: false,
        pointer_size: 4,
        format: "PE".to_string(),
        image_base: 0x400000,
        sections: vec![(0x401000, 0x402000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: CallingConvention::WindowsX64,
        ..Default::default()
    };
    apply_cspec_for_convention(&mut options);
    options
}

/// Shared preview context for tests that lower `CALL`/import sites at `0x14012c378` as `GetClientRect`
/// with `LPRECT`/`RECT` param rules — matches [`PreviewTypeContext::call_target_refs`] resolution in
/// [`crate::midend::builder::lower_expr`].
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
