use super::constraints::{UseConstraint, has_exact_scalar_constraint};
use crate::HashMap;
use crate::analysis::defuse::DefinitionDependencyMap;
use crate::prelude::*;
use crate::types::type_infer;

#[derive(Default)]
pub(super) struct BindingUseRole {
    pub(super) address_use: bool,
    pub(super) scalar_use: bool,
    pub(super) pointer_value_definition: bool,
    pub(super) non_pointer_value_definition: bool,
}

pub(super) fn collect_binding_use_roles(
    stmts: &[PreHirStmt],
    out: &mut HashMap<String, BindingUseRole>,
) {
    for stmt in stmts {
        collect_binding_use_roles_stmt(stmt, out);
    }
}

fn collect_binding_use_roles_stmt(stmt: &PreHirStmt, out: &mut HashMap<String, BindingUseRole>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            if let PreHirLValue::Var(name) = lhs {
                let role = out.entry(name.clone()).or_default();
                if matches!(expr_type(rhs), NirType::Ptr(_)) {
                    role.pointer_value_definition = true;
                } else {
                    role.non_pointer_value_definition = true;
                }
            }
            collect_binding_use_roles_lvalue(lhs, out);
            collect_binding_use_roles_expr(rhs, out);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            collect_binding_use_roles_expr(expr, out);
        }
        PreHirStmt::VaStart { va_list, .. } => collect_binding_use_roles_expr(va_list, out),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            collect_binding_use_roles(body, out);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_binding_use_roles_expr(cond, out);
            collect_binding_use_roles(then_body, out);
            collect_binding_use_roles(else_body, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_binding_use_roles_stmt(init, out);
            }
            if let Some(cond) = cond {
                collect_binding_use_roles_expr(cond, out);
            }
            if let Some(update) = update {
                collect_binding_use_roles_stmt(update, out);
            }
            collect_binding_use_roles(body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_binding_use_roles_expr(expr, out);
            for case in cases {
                collect_binding_use_roles(&case.body, out);
            }
            collect_binding_use_roles(default, out);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_binding_use_roles_lvalue(lhs: &PreHirLValue, out: &mut HashMap<String, BindingUseRole>) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => mark_address_role(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            mark_address_role(base, out);
            collect_binding_use_roles_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => mark_address_role(base, out),
    }
}

fn collect_binding_use_roles_expr(expr: &PreHirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { ty, expr } => {
            if matches!(ty, NirType::Int { .. } | NirType::Bool) {
                mark_scalar_role(expr, out);
            } else {
                collect_binding_use_roles_expr(expr, out);
            }
        }
        PreHirExpr::Unary { expr, .. } => mark_scalar_role(expr, out),
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            if role_scalar_op(*op) {
                mark_scalar_role(lhs, out);
                mark_scalar_role(rhs, out);
            } else {
                collect_binding_use_roles_expr(lhs, out);
                collect_binding_use_roles_expr(rhs, out);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_binding_use_roles_expr(cond, out);
            collect_binding_use_roles_expr(then_expr, out);
            collect_binding_use_roles_expr(else_expr, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_binding_use_roles_expr(arg, out);
            }
        }
        PreHirExpr::Load { ptr, .. } => mark_address_role(ptr, out),
        PreHirExpr::PtrOffset { base, .. }
        | PreHirExpr::FieldAccess { base, .. }
        | PreHirExpr::AggregateCopy { src: base, .. } => mark_address_role(base, out),
        PreHirExpr::Index { base, index, .. } => {
            mark_address_role(base, out);
            collect_binding_use_roles_expr(index, out);
        }
    }
}

fn mark_address_role(expr: &PreHirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        PreHirExpr::Var(name) => {
            out.entry(name.clone()).or_default().address_use = true;
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => mark_address_role(expr, out),
        PreHirExpr::Index { base, .. } => mark_address_role(base, out),
        PreHirExpr::Binary { .. }
        | PreHirExpr::Select { .. }
        | PreHirExpr::Call { .. }
        | PreHirExpr::Load { .. }
        | PreHirExpr::Const(_, _)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_) => collect_binding_use_roles_expr(expr, out),
    }
}

fn mark_scalar_role(expr: &PreHirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        PreHirExpr::Var(name) => {
            out.entry(name.clone()).or_default().scalar_use = true;
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => mark_scalar_role(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            mark_scalar_role(lhs, out);
            mark_scalar_role(rhs, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            mark_scalar_role(cond, out);
            mark_scalar_role(then_expr, out);
            mark_scalar_role(else_expr, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                mark_scalar_role(arg, out);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            mark_scalar_role(base, out);
            mark_scalar_role(index, out);
        }
        PreHirExpr::Load { ptr, .. } => mark_address_role(ptr, out),
        PreHirExpr::Const(_, _)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_) => {}
    }
}

fn role_scalar_op(op: PreHirBinaryOp) -> bool {
    matches!(
        op,
        PreHirBinaryOp::Add
            | PreHirBinaryOp::Sub
            | PreHirBinaryOp::Mul
            | PreHirBinaryOp::Div
            | PreHirBinaryOp::Mod
            | PreHirBinaryOp::And
            | PreHirBinaryOp::Or
            | PreHirBinaryOp::Xor
            | PreHirBinaryOp::Shl
            | PreHirBinaryOp::Shr
            | PreHirBinaryOp::Sar
            | PreHirBinaryOp::Eq
            | PreHirBinaryOp::Ne
            | PreHirBinaryOp::Lt
            | PreHirBinaryOp::Le
            | PreHirBinaryOp::Gt
            | PreHirBinaryOp::Ge
            | PreHirBinaryOp::SLt
            | PreHirBinaryOp::SLe
            | PreHirBinaryOp::SGt
            | PreHirBinaryOp::SGe
    )
}

pub(super) fn restore_scalar_only_pointer_locals(
    func: &mut PreHirFunction,
    constraints: &HashMap<String, Vec<UseConstraint>>,
    roles: &HashMap<String, BindingUseRole>,
    dependencies: &DefinitionDependencyMap,
    scalar_induction: &type_infer::ScalarInductionEvidence,
) -> bool {
    let pointer_compare_peers =
        type_infer::pointer_compare_peer_promotions_with_evidence(func, scalar_induction);
    let transitive_address_locals = type_infer::transitive_address_pointer_locals_with_evidence(
        func,
        dependencies,
        scalar_induction,
    );
    let scalar_ty = NirType::Int {
        bits: if func.is_64bit { 64 } else { 32 },
        signed: false,
    };
    let mut changed = false;
    for binding in &mut func.locals {
        if !matches!(binding.ty, NirType::Ptr(_)) {
            continue;
        }
        let role = roles.get(&binding.name);
        if binding.surface_type_name.is_some() {
            continue;
        }
        let scalar_evidence = role.is_some_and(|role| role.scalar_use)
            || has_exact_scalar_constraint(constraints.get(&binding.name));
        let address_use = role.is_some_and(|role| role.address_use);
        let exclusively_pointer_defined = role.is_some_and(|role| {
            role.pointer_value_definition && !role.non_pointer_value_definition
        });
        if scalar_evidence
            && !address_use
            && !exclusively_pointer_defined
            && !pointer_compare_peers.contains_key(&binding.name)
            && !transitive_address_locals.contains_key(&binding.name)
        {
            binding.ty = scalar_ty.clone();
            changed = true;
        }
    }
    changed
}

pub(super) fn should_skip_pointer_constraint_for_scalar_local(
    binding: &PreHirBinding,
    constraint: &UseConstraint,
    roles: &HashMap<String, BindingUseRole>,
    address_contributors: &HashMap<String, NirType>,
) -> bool {
    if address_contributors.contains_key(&binding.name)
        && matches!(
            constraint,
            UseConstraint::Signed { .. }
                | UseConstraint::Unsigned { .. }
                | UseConstraint::LogicalShiftUnsigned { .. }
                | UseConstraint::Exact(NirType::Int { .. } | NirType::Bool)
        )
    {
        return true;
    }
    if !matches!(constraint, UseConstraint::Ptr(_))
        || matches!(binding.origin, Some(NirBindingOrigin::ParamIndex(_)))
        || binding.surface_type_name.is_some()
    {
        return false;
    }

    roles
        .get(&binding.name)
        .is_some_and(|role| role.scalar_use && !role.address_use)
}
