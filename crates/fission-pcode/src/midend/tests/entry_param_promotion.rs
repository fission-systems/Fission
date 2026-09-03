//! Tests for entry-block `tmp = <param reg>` → `param_k` promotion.

use super::*;
use crate::midend::ir::{NirBindingOrigin, NirType};
use crate::midend::normalize::normalize_hir_function;
use fission_core::CallingConvention;
use fission_midend_prehir::{PreHirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};

fn binding_temp(name: &str) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty: NirType::Int {
            bits: 64,
            signed: true,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

#[test]
fn entry_spill_sysv_rsi_becomes_param_2() {
    // System V AMD64: rsi is the second integer parameter register.
    let mut func = PreHirFunction {
        name: "spill".into(),
        int_param_offsets: int_params_for(CallingConvention::SystemVAmd64),
        params: vec![],
        locals: vec![binding_temp("tmp_x")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("tmp_x".into()),
                rhs: PreHirExpr::Var("rsi".into()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("tmp_x".into()))),
        ],
        calling_convention: CallingConvention::SystemVAmd64,
        is_64bit: true,
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(
        rendered.contains("param_2"),
        "expected param_2 promotion, got:\n{rendered}"
    );
}

#[test]
fn entry_spill_win64_ecx_alias_becomes_param_1() {
    let mut func = PreHirFunction {
        name: "spill".into(),
        int_param_offsets: int_params_for(CallingConvention::WindowsX64),
        params: vec![],
        locals: vec![binding_temp("saved_n")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("saved_n".into()),
                rhs: PreHirExpr::Var("ecx".into()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("saved_n".into()))),
        ],
        calling_convention: CallingConvention::WindowsX64,
        is_64bit: true,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(
        rendered.contains("param_1"),
        "expected Win64 ecx alias spill to promote to param_1, got:\n{rendered}"
    );
    assert!(
        !rendered.contains("ecx"),
        "expected ecx alias to be replaced by param_1, got:\n{rendered}"
    );
}

#[test]
fn win64_variadic_shape_trims_unused_tail_params() {
    let int64 = NirType::Int {
        bits: 64,
        signed: true,
    };
    let mut func = PreHirFunction {
        name: "variadic".into(),
        int_param_offsets: int_params_for(CallingConvention::WindowsX64),
        params: (0..4)
            .map(|slot| PreHirBinding {
                name: format!("param_{}", slot + 1),
                ty: int64.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(slot)),
                initializer: None,
            })
            .collect(),
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![PreHirStmt::Expr(PreHirExpr::Call {
            target: "sub_1400c05e8".into(),
            args: vec![
                PreHirExpr::Var("param_1".into()),
                PreHirExpr::Var("param_2".into()),
                PreHirExpr::Const(-1, int64.clone()),
                PreHirExpr::Var("param_2".into()),
                PreHirExpr::Const(0, int64.clone()),
                PreHirExpr::Var("va_cursor".into()),
            ],
            ty: NirType::Unknown,
        })],
        calling_convention: CallingConvention::WindowsX64,
        is_64bit: true,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    assert_eq!(
        func.params.len(),
        2,
        "expected variadic Win64 wrapper to keep two fixed params, got:\n{}",
        print_prehir_function(&func)
    );
}

#[test]
fn loongarch32_existing_param_local_becomes_function_param_before_self_call_prune() {
    let int32 = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = PreHirFunction {
        name: "recursive_fib".into(),
        int_param_offsets: int_params_for(CallingConvention::LoongArch32),
        params: vec![],
        locals: vec![PreHirBinding {
            name: "param_1".into(),
            ty: int32.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }],
        return_type: int32.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Call {
            target: "recursive_fib".into(),
            args: vec![PreHirExpr::Var("param_1".into())],
            ty: int32,
        }))],
        calling_convention: CallingConvention::LoongArch32,
        is_64bit: false,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(
        rendered.contains("recursive_fib(int param_1)"),
        "expected LoongArch32 param_1 to become a function parameter:\n{rendered}"
    );
    assert!(
        rendered.contains("recursive_fib(param_1)"),
        "expected self-call argument to survive arity pruning:\n{rendered}"
    );
}

/// Win64 `<group>` makes `XMM1_Qa` the *second* parameter, not the second
/// float: it is the alternative storage for the slot `RDX` also serves.
#[test]
fn win64_xmm_slot_becomes_the_param_at_its_group_index() {
    let (float_offsets, shares) = float_params_for(CallingConvention::WindowsX64);
    let mut func = PreHirFunction {
        name: "scaled".into(),
        int_param_offsets: int_params_for(CallingConvention::WindowsX64),
        float_param_offsets: float_offsets,
        float_shares_int_slots: shares,
        params: vec![],
        locals: vec![binding_temp("prod")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("prod".into()),
                rhs: PreHirExpr::Var("xmm1_qa".into()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("prod".into()))),
        ],
        calling_convention: CallingConvention::WindowsX64,
        is_64bit: true,
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(
        rendered.contains("param_2"),
        "expected XMM1_Qa to promote to slot 2's param, got:\n{rendered}"
    );
    assert!(
        !rendered.contains("xmm1_qa"),
        "expected xmm1_qa to be replaced by its param, got:\n{rendered}"
    );
}

/// SysV groups nothing, so a float register's index is not its C parameter
/// position and there is no slot to promote it into.
#[test]
fn sysv_xmm_read_is_not_claimed_as_a_param_slot() {
    let (float_offsets, shares) = float_params_for(CallingConvention::SystemVAmd64);
    assert!(
        !shares,
        "SysV must not report shared slots; its classes are independent"
    );
    let mut func = PreHirFunction {
        name: "sse".into(),
        int_param_offsets: int_params_for(CallingConvention::SystemVAmd64),
        float_param_offsets: float_offsets,
        float_shares_int_slots: shares,
        params: vec![],
        locals: vec![binding_temp("v")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("v".into()),
                rhs: PreHirExpr::Var("xmm1_qa".into()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("v".into()))),
        ],
        calling_convention: CallingConvention::SystemVAmd64,
        is_64bit: true,
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(
        !rendered.contains("param_"),
        "SysV XMM1 is not slot 2; expected no param promotion, got:\n{rendered}"
    );
}

/// The ARM32 `push {rN, lr}` padding idiom: a register spilled once to the
/// *frame* and never read back is alignment padding, not an argument.
#[test]
fn arm32_frame_spilled_register_is_still_not_a_param() {
    let mut func = PreHirFunction {
        name: "pad".into(),
        int_param_offsets: int_params_for(CallingConvention::Arm32),
        params: vec![],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("sp".into())),
                    ty: NirType::Unknown,
                },
                rhs: PreHirExpr::Var("r3".into()),
            },
            PreHirStmt::Return(None),
        ],
        calling_convention: CallingConvention::Arm32,
        is_64bit: false,
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(
        !rendered.contains("param_"),
        "frame-spilled padding must not promote, got:\n{rendered}"
    );
}

/// ... but the same one-store-no-read shape through an *incoming pointer* is
/// `void store(T *out, T v)`, whose stored value is a genuine argument.
#[test]
fn arm32_store_through_param_pointer_is_a_param() {
    let mut func = PreHirFunction {
        name: "out".into(),
        int_param_offsets: int_params_for(CallingConvention::Arm32),
        params: vec![],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("r0".into())),
                    ty: NirType::Unknown,
                },
                rhs: PreHirExpr::Var("r1".into()),
            },
            PreHirStmt::Return(None),
        ],
        calling_convention: CallingConvention::Arm32,
        is_64bit: false,
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(
        rendered.contains("param_2"),
        "value stored through an incoming pointer is a param, got:\n{rendered}"
    );
}
