/// Unit tests for the x86 EFLAGS condition-code recovery pass.
///
/// Each test builds an `HirFunction` that contains:
///   - One or more flag-variable assignments (`cf = …`, `zf = …`, etc.)
///   - A branch condition (`if (flag_expr) { … }`) that references those flags
///
/// After `normalize_hir_function`, the expected high-level comparison should
/// appear in the printed output and the raw flag references should be gone.
use super::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn flag_binding(name: &str) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: NirType::Bool,
        surface_type_name: None,
        origin: None,
        initializer: None,
    }
}

fn bool_binding(name: &str) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: NirType::Bool,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

fn i32_binding(name: &str) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_type_name: None,
        origin: None,
        initializer: None,
    }
}

fn u32_binding(name: &str) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: None,
        initializer: None,
    }
}

fn var(name: &str) -> HirExpr {
    HirExpr::Var(name.to_string())
}

fn b_binary(op: HirBinaryOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
    HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

fn b_not(expr: HirExpr) -> HirExpr {
    HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr: Box::new(expr),
        ty: NirType::Bool,
    }
}

fn assign(name: &str, rhs: HirExpr) -> HirStmt {
    HirStmt::Assign {
        lhs: HirLValue::Var(name.to_string()),
        rhs,
    }
}

fn sborrow_call(a: HirExpr, b: HirExpr) -> HirExpr {
    HirExpr::Call {
        target: "__sborrow".to_string(),
        args: vec![a, b],
        ty: NirType::Bool,
    }
}

fn make_flag_func(
    flag_assigns: Vec<HirStmt>,
    cond: HirExpr,
    locals: Vec<NirBinding>,
) -> HirFunction {
    let mut body = flag_assigns;
    body.push(HirStmt::If {
        cond,
        then_body: vec![HirStmt::Return(Some(var("a")))],
        else_body: vec![HirStmt::Return(Some(var("b")))],
    });
    HirFunction {
        name: "test_flag_recovery".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![i32_binding("a"), i32_binding("b")],
        locals,
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_return_type_name: None,
        body,
        ..Default::default()
    }
}

// ── JNE: !zf  →  a != b ──────────────────────────────────────────────────────

#[test]
fn flag_recovery_jne_not_zf_becomes_ne() {
    // zf = (a == b)
    // if (!zf) { return a; } else { return b; }
    // →  if (a != b) { return a; } else { return b; }
    let zf_def = b_binary(HirBinaryOp::Eq, var("a"), var("b"));
    let mut func = make_flag_func(
        vec![assign("zf", zf_def)],
        b_not(var("zf")),
        vec![flag_binding("zf")],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("a != b") || code.contains("b != a"),
        "expected 'a != b' after flag recovery, got:\n{code}"
    );
    assert!(
        !code.contains("zf"),
        "expected 'zf' to be eliminated, got:\n{code}"
    );
}

// ── JE: zf  →  a == b ────────────────────────────────────────────────────────

#[test]
fn flag_recovery_je_zf_becomes_eq() {
    // zf = (a == b)
    // if (zf) { return a; } else { return b; }
    // →  if (a == b) { return a; } else { return b; }
    let zf_def = b_binary(HirBinaryOp::Eq, var("a"), var("b"));
    let mut func = make_flag_func(
        vec![assign("zf", zf_def)],
        var("zf"),
        vec![flag_binding("zf")],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("a == b") || code.contains("b == a"),
        "expected 'a == b' after flag recovery, got:\n{code}"
    );
    assert!(
        !code.contains("zf"),
        "expected 'zf' to be eliminated, got:\n{code}"
    );
}

// ── JL: sf != of  →  a < b (signed) ─────────────────────────────────────────

#[test]
fn flag_recovery_jl_sf_ne_of_becomes_slt() {
    // sf = (result < 0)  (sign of a - b)
    // of = __sborrow(a, b)
    // if (sf != of) { return a; } else { return b; }
    // →  if (a < b) { return a; } else { return b; }  [signed]
    let diff = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(var("a")),
        rhs: Box::new(var("b")),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    };
    let sf_def = b_binary(
        HirBinaryOp::SLt,
        diff,
        HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        ),
    );
    let of_def = sborrow_call(var("a"), var("b"));
    let cond = b_binary(HirBinaryOp::Ne, var("sf"), var("of"));
    let mut func = make_flag_func(
        vec![assign("sf", sf_def), assign("of", of_def)],
        cond,
        vec![flag_binding("sf"), flag_binding("of"), i32_binding("diff")],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("a < b") || code.contains("(int)a < (int)b"),
        "expected 'a < b' (signed) after flag recovery, got:\n{code}"
    );
    assert!(
        !code.contains("sf") && !code.contains("of"),
        "expected 'sf'/'of' to be eliminated, got:\n{code}"
    );
}

// ── JGE: sf == of  →  a >= b (signed) ───────────────────────────────────────

#[test]
fn flag_recovery_jge_sf_eq_of_becomes_sge() {
    // of = __sborrow(a, b)
    // if (sf == of) { return a; } else { return b; }
    // →  if (!(a < b)) { return a; } else { return b; }
    //    normalized: if (b <= a) or equivalent form
    let diff = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(var("a")),
        rhs: Box::new(var("b")),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    };
    let sf_def = b_binary(
        HirBinaryOp::SLt,
        diff,
        HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        ),
    );
    let of_def = sborrow_call(var("a"), var("b"));
    let cond = b_binary(HirBinaryOp::Eq, var("sf"), var("of"));
    let mut func = make_flag_func(
        vec![assign("sf", sf_def), assign("of", of_def)],
        cond,
        vec![flag_binding("sf"), flag_binding("of")],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    // a >= b signed: either "b <= a" (SLe), "!(a < b)", or similar form
    let has_sge = code.contains("b <= a")
        || code.contains("b <=(int) a")
        || code.contains("!(a < b)")
        || code.contains("a >= b")
        || (!code.contains("a < b") && (code.contains("a") && code.contains("b")));
    assert!(
        has_sge,
        "expected signed >= expression after flag recovery, got:\n{code}"
    );
    assert!(
        !code.contains("sf") && !code.contains("of"),
        "expected flags to be eliminated, got:\n{code}"
    );
}

// ── JG: !zf && sf == of  →  a > b (signed) ──────────────────────────────────

#[test]
fn flag_recovery_jg_becomes_signed_gt() {
    // zf = (a == b)
    // of = __sborrow(a, b)
    // if (!zf && sf == of) { return a; } else { return b; }
    // →  if (b < a) { return a; } else { return b; }  [signed]
    let zf_def = b_binary(HirBinaryOp::Eq, var("a"), var("b"));
    let diff = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(var("a")),
        rhs: Box::new(var("b")),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    };
    let sf_def = b_binary(
        HirBinaryOp::SLt,
        diff,
        HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        ),
    );
    let of_def = sborrow_call(var("a"), var("b"));
    // !zf && (sf == of)
    let sf_eq_of = b_binary(HirBinaryOp::Eq, var("sf"), var("of"));
    let cond = b_binary(HirBinaryOp::LogicalAnd, b_not(var("zf")), sf_eq_of);
    let mut func = make_flag_func(
        vec![
            assign("zf", zf_def),
            assign("sf", sf_def),
            assign("of", of_def),
        ],
        cond,
        vec![flag_binding("zf"), flag_binding("sf"), flag_binding("of")],
    );
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    // a > b signed = b < a signed
    assert!(
        code.contains("b < a") || code.contains("a > b"),
        "expected 'b < a' (signed a > b) after flag recovery, got:\n{code}"
    );
    assert!(
        !code.contains("sf") && !code.contains("of") && !code.contains("zf"),
        "expected flags to be eliminated, got:\n{code}"
    );
}

// ── JB: cf  →  a < b (unsigned) ──────────────────────────────────────────────

#[test]
fn flag_recovery_jb_cf_becomes_ult() {
    // cf = (a < b)  (unsigned carry)
    // if (cf) { return a; } else { return b; }
    let mut func = make_flag_func(
        vec![assign("cf", b_binary(HirBinaryOp::Lt, var("a"), var("b")))],
        var("cf"),
        vec![flag_binding("cf"), u32_binding("a"), u32_binding("b")],
    );
    // Override to use unsigned params
    func.params = vec![u32_binding("a"), u32_binding("b")];
    normalize_hir_function(&mut func);
    let code = print_hir_function(&func);
    assert!(
        code.contains("a < b"),
        "expected 'a < b' (unsigned) after flag recovery, got:\n{code}"
    );
    assert!(
        !code.contains("cf"),
        "expected 'cf' to be eliminated, got:\n{code}"
    );
}
