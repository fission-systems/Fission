//! Tests for entry-block `tmp = <param reg>` → `param_k` promotion.

use super::*;
use crate::midend::normalize::normalize_hir_function;
use crate::midend::ir::{
    DirExpr, DirFunction, DirLValue, DirStmt, DirBinding, NirBindingOrigin, NirType,
};
use fission_core::CallingConvention;

fn binding_temp(name: &str) -> DirBinding {
    DirBinding {
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
    let mut func = DirFunction {
        name: "spill".into(),
        int_param_offsets: int_params_for(CallingConvention::SystemVAmd64),
        params: vec![],
        locals: vec![binding_temp("tmp_x")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("tmp_x".into()),
                rhs: DirExpr::Var("rsi".into()),
            },
            DirStmt::Return(Some(DirExpr::Var("tmp_x".into()))),
        ],
        calling_convention: CallingConvention::SystemVAmd64,
        is_64bit: true,
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let rendered = print_dir_function(&func);
    assert!(
        rendered.contains("param_2"),
        "expected param_2 promotion, got:\n{rendered}"
    );
}

#[test]
fn entry_spill_win64_ecx_alias_becomes_param_1() {
    let mut func = DirFunction {
        name: "spill".into(),
        int_param_offsets: int_params_for(CallingConvention::WindowsX64),
        params: vec![],
        locals: vec![binding_temp("saved_n")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("saved_n".into()),
                rhs: DirExpr::Var("ecx".into()),
            },
            DirStmt::Return(Some(DirExpr::Var("saved_n".into()))),
        ],
        calling_convention: CallingConvention::WindowsX64,
        is_64bit: true,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_dir_function(&func);
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
    let mut func = DirFunction {
        name: "variadic".into(),
        int_param_offsets: int_params_for(CallingConvention::WindowsX64),
        params: (0..4)
            .map(|slot| DirBinding {
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
        body: vec![DirStmt::Expr(DirExpr::Call {
            target: "sub_1400c05e8".into(),
            args: vec![
                DirExpr::Var("param_1".into()),
                DirExpr::Var("param_2".into()),
                DirExpr::Const(-1, int64.clone()),
                DirExpr::Var("param_2".into()),
                DirExpr::Const(0, int64.clone()),
                DirExpr::Var("va_cursor".into()),
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
        print_dir_function(&func)
    );
}

#[test]
fn loongarch32_existing_param_local_becomes_function_param_before_self_call_prune() {
    let int32 = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = DirFunction {
        name: "recursive_fib".into(),
        int_param_offsets: int_params_for(CallingConvention::LoongArch32),
        params: vec![],
        locals: vec![DirBinding {
            name: "param_1".into(),
            ty: int32.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }],
        return_type: int32.clone(),
        surface_return_type_name: None,
        body: vec![DirStmt::Return(Some(DirExpr::Call {
            target: "recursive_fib".into(),
            args: vec![DirExpr::Var("param_1".into())],
            ty: int32,
        }))],
        calling_convention: CallingConvention::LoongArch32,
        is_64bit: false,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_dir_function(&func);
    assert!(
        rendered.contains("recursive_fib(int param_1)"),
        "expected LoongArch32 param_1 to become a function parameter:\n{rendered}"
    );
    assert!(
        rendered.contains("recursive_fib(param_1)"),
        "expected self-call argument to survive arity pruning:\n{rendered}"
    );
}
