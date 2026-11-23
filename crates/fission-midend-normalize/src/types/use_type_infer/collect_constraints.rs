use super::constraints::UseConstraint;
use super::copy_constraints::collect_assignment_copy_constraints;
use super::expression_types::{expr_int_bits, indirect_call_target_binding_name, nir_type_bits};
use super::operator_constraints::{
    collect_arithmetic_result_constraints, collect_compare_constraints,
};
use crate::HashMap;
use crate::prelude::*;

/// Accumulate use-site constraints for all named variables in `stmts`.
pub(super) fn collect_constraints(
    stmts: &[PreHirStmt],
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    for stmt in stmts {
        collect_constraints_stmt(stmt, return_type, known_binding_types, out);
    }
}

fn collect_constraints_stmt(
    stmt: &PreHirStmt,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            // Use-site on the lhs: Deref/Index require the base to be a pointer.
            collect_constraints_lvalue(lhs, out);
            collect_assignment_copy_constraints(lhs, rhs, known_binding_types, out);
            // Use-site on the rhs: look for Cast(T, Var(x)) → x: T.
            collect_constraints_cast_source(rhs, known_binding_types, out);
            // Recurse into rhs for nested uses.
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        PreHirStmt::Expr(expr) => {
            collect_constraints_expr(expr, return_type, known_binding_types, out);
        }
        PreHirStmt::Block(body) => {
            collect_constraints(body, return_type, known_binding_types, out);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(then_body, return_type, known_binding_types, out);
            collect_constraints(else_body, return_type, known_binding_types, out);
        }
        PreHirStmt::While { cond, body } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(body, return_type, known_binding_types, out);
        }
        PreHirStmt::DoWhile { body, cond } => {
            collect_constraints(body, return_type, known_binding_types, out);
            collect_constraints_expr(cond, return_type, known_binding_types, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                collect_constraints_stmt(i, return_type, known_binding_types, out);
            }
            if let Some(c) = cond {
                collect_constraints_expr(c, return_type, known_binding_types, out);
            }
            if let Some(u) = update {
                collect_constraints_stmt(u, return_type, known_binding_types, out);
            }
            collect_constraints(body, return_type, known_binding_types, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_constraints_expr(expr, return_type, known_binding_types, out);
            for case in cases {
                collect_constraints(&case.body, return_type, known_binding_types, out);
            }
            collect_constraints(default, return_type, known_binding_types, out);
        }
        PreHirStmt::Return(Some(expr)) => {
            // If the function's return type is already known and the expression
            // is a bare variable, constrain that variable to the return type.
            if *return_type != NirType::Unknown {
                if let PreHirExpr::Var(name) = expr {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Exact(return_type.clone()));
                }
            }
            collect_constraints_expr(expr, return_type, known_binding_types, out);
        }
        _ => {}
    }
}

/// Collect pointer constraints from lvalue use sites.
fn collect_constraints_lvalue(lhs: &PreHirLValue, out: &mut HashMap<String, Vec<UseConstraint>>) {
    match lhs {
        PreHirLValue::Deref { ptr, ty } => {
            // Storing through *ptr → ptr must be Ptr(ty).
            if let PreHirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
        }
        PreHirLValue::Index { base, elem_ty, .. } => {
            // base[idx] → base is Ptr(elem_ty).
            if let PreHirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
        }
        PreHirLValue::Var(_) => {}
        PreHirLValue::FieldAccess { base, ty, .. } => {
            if let PreHirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
        }
    }
}

/// Collect `Cast(T, Var(x))` → x: T constraints.
fn collect_constraints_cast_source(
    expr: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    if let PreHirExpr::Cast { ty, expr: inner } = expr {
        if let PreHirExpr::Var(name) = inner.as_ref() {
            // The variable is being cast; constrain it to the source type of the
            // cast.
            match ty {
                NirType::Int { .. } | NirType::Bool => {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Exact(ty.clone()));
                }
                NirType::Ptr(pointee) => {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
                _ => {}
            }
        }
        if matches!(ty, NirType::Int { .. })
            && let PreHirExpr::Binary { op, lhs, rhs, .. } = inner.as_ref()
            && matches!(
                op,
                PreHirBinaryOp::Add | PreHirBinaryOp::Sub | PreHirBinaryOp::Mul
            )
        {
            collect_arithmetic_result_constraints(lhs, rhs, ty, known_binding_types, out);
        }
    }
}

/// Recurse into an expression and collect use-site constraints.
fn collect_constraints_expr(
    expr: &PreHirExpr,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match expr {
        PreHirExpr::Load { ptr, ty } => {
            // Loading through *ptr → ptr is Ptr(ty).
            if let PreHirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
            // Recurse into the pointer expression itself.
            collect_constraints_expr(ptr, return_type, known_binding_types, out);
        }
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            match op {
                // Signed comparison → operands are signed integers.  The
                // comparison expression itself is Bool, so operand width must
                // come from an actual operand or an existing binding type.
                PreHirBinaryOp::SLt
                | PreHirBinaryOp::SLe
                | PreHirBinaryOp::SGt
                | PreHirBinaryOp::SGe => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, true, out)
                }
                // Unsigned comparison → operands are unsigned integers.
                PreHirBinaryOp::Lt
                | PreHirBinaryOp::Le
                | PreHirBinaryOp::Gt
                | PreHirBinaryOp::Ge => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, false, out)
                }
                // Arithmetic right-shift: the left operand must be a signed integer.
                // `x >> k` where `>>` is Sar (arithmetic) means x is signed.
                PreHirBinaryOp::Sar => {
                    let bits = nir_type_bits(ty)
                        .or_else(|| expr_int_bits(lhs.as_ref(), known_binding_types))
                        .unwrap_or(32);
                    if let PreHirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::Signed { bits });
                    }
                }
                // Logical right-shift (p-code INT_RIGHT / x86 SHR): lhs must be unsigned
                // so C `>>` does not become an arithmetic shift on a signed `int`.
                // Example: `count_bits(unsigned)` with `x >>= 1` must stay logical for
                // `0xFFFFFFFF` (case: 32 ones), otherwise signed `int` loops forever.
                //
                // Do not also run `collect_arithmetic_result_constraints` with a signed
                // result ty — that would re-push Signed and cancel the demotion.
                PreHirBinaryOp::Shr => {
                    let bits = nir_type_bits(ty)
                        .or_else(|| expr_int_bits(lhs.as_ref(), known_binding_types))
                        .unwrap_or(32);
                    if let PreHirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::LogicalShiftUnsigned { bits });
                    }
                }
                PreHirBinaryOp::Add
                | PreHirBinaryOp::Sub
                | PreHirBinaryOp::Mul
                | PreHirBinaryOp::Div
                | PreHirBinaryOp::Mod
                | PreHirBinaryOp::And
                | PreHirBinaryOp::Or
                | PreHirBinaryOp::Xor
                | PreHirBinaryOp::Shl => {
                    collect_arithmetic_result_constraints(lhs, rhs, ty, known_binding_types, out);
                }
                _ => {}
            }
            collect_constraints_expr(lhs, return_type, known_binding_types, out);
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        PreHirExpr::Unary { expr: inner, .. } => {
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        PreHirExpr::Cast { expr: inner, .. } => {
            collect_constraints_cast_source(expr, known_binding_types, out);
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        PreHirExpr::Call { target, args, .. } => {
            if let Some(name) = indirect_call_target_binding_name(target) {
                out.entry(name.to_owned())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }
            for arg in args {
                collect_constraints_expr(arg, return_type, known_binding_types, out);
            }
        }
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            if let PreHirExpr::Var(base_name) = base.as_ref() {
                out.entry(base_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }
            collect_constraints_expr(base, return_type, known_binding_types, out);
        }
        PreHirExpr::AggregateCopy { src: base, .. } => {
            collect_constraints_expr(base, return_type, known_binding_types, out);
        }
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            // base[index] → base is Ptr(elem_ty).
            if let PreHirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
            if let PreHirExpr::Var(name) = index.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(NirType::Int {
                        bits: 32,
                        signed: false,
                    }));
            }
            collect_constraints_expr(base, return_type, known_binding_types, out);
            collect_constraints_expr(index, return_type, known_binding_types, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints_expr(then_expr, return_type, known_binding_types, out);
            collect_constraints_expr(else_expr, return_type, known_binding_types, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}
