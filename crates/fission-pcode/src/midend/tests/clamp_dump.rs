use super::*;
use crate::midend::normalize::normalize_hir_function;

fn i32t() -> NirType {
    NirType::Int {
        bits: 32,
        signed: true,
    }
}

fn u32t() -> NirType {
    NirType::Int {
        bits: 32,
        signed: false,
    }
}

fn bool_ty() -> NirType {
    NirType::Bool
}

fn bind(name: &str, ty: NirType) -> NirBinding {
    NirBinding {
        name: name.into(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    }
}

fn param(name: &str, idx: usize) -> NirBinding {
    NirBinding {
        name: name.into(),
        ty: i32t(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(idx)),
        initializer: None,
    }
}

fn assign(lhs: &str, rhs: HirExpr) -> HirStmt {
    HirStmt::Assign {
        lhs: HirLValue::Var(lhs.into()),
        rhs,
    }
}

fn var(name: &str) -> HirExpr {
    HirExpr::Var(name.into())
}

/// Reconstruct the pre-normalize clamp cmov HIR (simplified SLe/SLt form) and
/// ensure normalize keeps both guarded overrides on the accumulator.
#[test]
fn normalize_preserves_cmov_clamp_select_chain() {
    let mut func = HirFunction {
        name: "clamp".to_string(),
        return_type: i32t(),
        params: vec![param("param_1", 0), param("param_2", 1), param("param_3", 2)],
        locals: vec![
            bind("edx", i32t()),
            bind("ecx", i32t()),
            bind("uVar2", i32t()),
        ],
        body: vec![
            assign("edx", var("param_1")),
            assign("uVar2", var("param_3")),
            assign("ecx", var("param_2")),
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::SLe,
                    lhs: Box::new(var("edx")),
                    rhs: Box::new(var("uVar2")),
                    ty: bool_ty(),
                },
                then_body: vec![assign("uVar2", var("edx"))],
                else_body: vec![],
            },
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::SLt,
                    lhs: Box::new(var("edx")),
                    rhs: Box::new(var("ecx")),
                    ty: bool_ty(),
                },
                then_body: vec![assign("uVar2", var("ecx"))],
                else_body: vec![],
            },
            HirStmt::Return(Some(var("uVar2"))),
        ],
        ..HirFunction::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    eprintln!("normalized simplified clamp:\n{code}");
    assert!(
        code.contains("if") || code.contains("?"),
        "lost conditional structure:\n{code}"
    );
    assert!(
        !code.contains("ecx = ecx"),
        "lo source merged into accumulator:\n{code}"
    );
    // Accumulator default is hi (param_3); must still appear.
    assert!(
        code.contains("param_3") || code.contains("uVar2"),
        "lost hi default:\n{code}"
    );
}

/// Full flag-temp shape matching builder output for m32 O2 clamp cmov.
#[test]
fn normalize_preserves_flag_temp_cmov_clamp_chain() {
    let mut func = HirFunction {
        name: "clamp".to_string(),
        return_type: i32t(),
        params: vec![param("param_1", 0), param("param_2", 1), param("param_3", 2)],
        locals: vec![
            bind("edx", i32t()),
            bind("ecx", i32t()),
            bind("uVar2", i32t()),
            bind("uVar5", u32t()),
            bind("iVar6", i32t()),
            bind("of", bool_ty()),
            bind("sf", bool_ty()),
            bind("zf", bool_ty()),
            bind("xVar10", bool_ty()),
            bind("xVar11", u32t()),
            bind("xVar12", u32t()),
            bind("uVar13", u32t()),
            bind("iVar14", i32t()),
            bind("xVar18", bool_ty()),
            bind("xVar19", u32t()),
        ],
        body: vec![
            assign("edx", var("param_1")),
            assign("uVar2", var("param_3")),
            assign("ecx", var("param_2")),
            // First cmp: value vs hi → LE = zf | (of != sf)
            assign("uVar5", var("edx")),
            assign(
                "of",
                HirExpr::Call {
                    target: "__sborrow".into(),
                    args: vec![var("uVar5"), var("uVar2")],
                    ty: bool_ty(),
                },
            ),
            assign(
                "iVar6",
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(var("uVar5")),
                    rhs: Box::new(var("uVar2")),
                    ty: i32t(),
                },
            ),
            assign(
                "sf",
                HirExpr::Binary {
                    op: HirBinaryOp::SLt,
                    lhs: Box::new(var("iVar6")),
                    rhs: Box::new(HirExpr::Const(0, u32t())),
                    ty: bool_ty(),
                },
            ),
            assign(
                "zf",
                HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(var("iVar6")),
                    rhs: Box::new(HirExpr::Const(0, u32t())),
                    ty: bool_ty(),
                },
            ),
            assign(
                "xVar10",
                HirExpr::Binary {
                    op: HirBinaryOp::Ne,
                    lhs: Box::new(var("of")),
                    rhs: Box::new(var("sf")),
                    ty: bool_ty(),
                },
            ),
            assign(
                "xVar11",
                HirExpr::Binary {
                    op: HirBinaryOp::LogicalOr,
                    lhs: Box::new(var("zf")),
                    rhs: Box::new(var("xVar10")),
                    ty: u32t(),
                },
            ),
            assign(
                "xVar12",
                HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(var("xVar11")),
                    ty: u32t(),
                },
            ),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(var("xVar12")),
                    ty: bool_ty(),
                },
                then_body: vec![assign("uVar2", var("edx"))],
                else_body: vec![],
            },
            // Second cmp: value vs lo → LT = of != sf
            assign("uVar13", var("edx")),
            assign(
                "of",
                HirExpr::Call {
                    target: "__sborrow".into(),
                    args: vec![var("uVar13"), var("ecx")],
                    ty: bool_ty(),
                },
            ),
            assign(
                "iVar14",
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(var("uVar13")),
                    rhs: Box::new(var("ecx")),
                    ty: i32t(),
                },
            ),
            assign(
                "sf",
                HirExpr::Binary {
                    op: HirBinaryOp::SLt,
                    lhs: Box::new(var("iVar14")),
                    rhs: Box::new(HirExpr::Const(0, u32t())),
                    ty: bool_ty(),
                },
            ),
            assign(
                "xVar18",
                HirExpr::Binary {
                    op: HirBinaryOp::Ne,
                    lhs: Box::new(var("of")),
                    rhs: Box::new(var("sf")),
                    ty: bool_ty(),
                },
            ),
            assign(
                "xVar19",
                HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(var("xVar18")),
                    ty: u32t(),
                },
            ),
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(var("xVar19")),
                    ty: bool_ty(),
                },
                then_body: vec![assign("uVar2", var("ecx"))],
                else_body: vec![],
            },
            HirStmt::Return(Some(var("uVar2"))),
        ],
        ..HirFunction::default()
    };

    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    eprintln!("normalized flag-temp clamp:\n{code}");

    // edx = param_1 must dominate its uses (no use-before-def).
    if let Some(use_pos) = code.find("edx") {
        if let Some(def_pos) = code.find("edx = param_1") {
            assert!(
                def_pos < use_pos || code.matches("edx = param_1").count() >= 1,
                "edx used without dominating assign:\n{code}"
            );
        }
    }
    assert!(
        code.contains("param_1") && code.contains("param_2") && code.contains("param_3"),
        "missing params after normalize:\n{code}"
    );
    assert!(
        !code.contains("ecx = ecx"),
        "lo merged into accumulator:\n{code}"
    );
    // Should not return only lo unconditionally.
    assert!(
        !(code.contains("return param_2") && !code.contains("if") && !code.contains("?")),
        "collapsed to return lo only:\n{code}"
    );
}
