use super::super::analysis::defuse::DefinitionDependencyMap;
/// Use-driven backward type propagation pass.
///
/// `apply_type_inference_pass` (type_infer.rs) propagates types forward from
/// *definition* sites: if `x = (int32)...` then `x.ty = Int32`.  It cannot
/// infer types from *use* sites because `expr_type(PreHirExpr::Var(_)) = Unknown`.
///
/// This pass performs the complementary backward-direction inference:
///
/// 1. Walk every expression and statement to collect **use-site constraints**:
///    - `Load { ptr: Var(x), ty }` → x must be a pointer to ty
///    - `Deref { ptr: Var(x), ty }` (lvalue store destination) → same
///    - `Index { base: Var(x), elem_ty }` (array lvalue) → x is `Ptr(elem_ty)`
///    - `Binary { op: SLt|SLe, lhs: Var(x), ty }` → x is a signed integer
///    - `Binary { op: Lt|Le, lhs: Var(x), ty }` → x is an unsigned integer
///    - `Call { target: x }` → x must be an indirect-code pointer
///    - `Return(Var(x))` + known function return type → x gets the return type
///    - `Assign rhs = Cast(T, Var(x))` → x gets type T (the cast source)
///
/// 2. Merge all collected constraints into `PreHirBinding.ty` for locals and
///    params that are still `Unknown`.  Constraints are only *strengthened*
///    (Unknown → Ptr → Int with signedness), never weakened.
///
/// 3. Iterate until convergence (usually 1–2 rounds via the var-chain alias
///    mechanism).
///
/// This pass is binary-independent and heuristic-free.  It is placed right
/// after `apply_type_inference_pass` so that the def-driven types it computed
/// can serve as additional seeds for backward propagation.
use crate::prelude::*;
use crate::{HashMap, HashSet};

/// A type constraint derived from the context in which a variable is used.
#[derive(Debug, Clone, PartialEq, Eq)]
enum UseConstraint {
    /// Variable is used as a memory address (Load/Store/Deref); must be a pointer.
    Ptr(NirType),
    /// Variable is used in a signed comparison; must be a signed integer.
    Signed { bits: u32 },
    /// Variable is used in an unsigned comparison; must be an unsigned integer.
    Unsigned { bits: u32 },
    /// Variable is the lhs of a logical right-shift (INT_RIGHT / SHR). Stronger than
    /// generic `Unsigned`: may demote a signed param so C `>>` stays logical.
    LogicalShiftUnsigned { bits: u32 },
    /// Variable is used in a context that requires exactly this type.
    Exact(NirType),
}

#[derive(Default)]
struct BindingUseRole {
    address_use: bool,
    scalar_use: bool,
    pointer_value_definition: bool,
    non_pointer_value_definition: bool,
}

#[derive(Default)]
struct ByteIndexAccumulatorEvidence {
    def_count: usize,
    byte_seed_defs: usize,
    byte_update_defs: usize,
    byte_pointer_offset_uses: usize,
    disallowed_uses: usize,
}

type TypeStateSignature = (NirType, Vec<(String, NirType)>, Vec<(String, NirType)>);

fn type_state_signature(func: &PreHirFunction) -> TypeStateSignature {
    (
        func.return_type.clone(),
        func.params
            .iter()
            .map(|binding| (binding.name.clone(), binding.ty.clone()))
            .collect(),
        func.locals
            .iter()
            .map(|binding| (binding.name.clone(), binding.ty.clone()))
            .collect(),
    )
}

/// Accumulate use-site constraints for all named variables in `stmts`.
fn collect_constraints(
    stmts: &[PreHirStmt],
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    for stmt in stmts {
        collect_constraints_stmt(stmt, return_type, known_binding_types, out);
    }
}

fn collect_binding_use_roles(stmts: &[PreHirStmt], out: &mut HashMap<String, BindingUseRole>) {
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
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
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
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
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
        | PreHirExpr::AddressOfGlobal(_) => collect_binding_use_roles_expr(expr, out),
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
        PreHirExpr::Const(_, _) | PreHirExpr::AddressOfGlobal(_) => {}
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

fn collect_assignment_copy_constraints(
    lhs: &PreHirLValue,
    rhs: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match lhs {
        PreHirLValue::Var(lhs_name) => {
            // Reverse-propagate non-pointer legacy constraints only. Pointer
            // equality is owned by the operation-edge TypeFlow solver, which
            // carries evidence strength and lock boundaries.
            if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
                if let PreHirExpr::Var(rhs_name) = rhs {
                    match lhs_ty {
                        NirType::Ptr(_) => {}
                        other => {
                            out.entry(rhs_name.clone())
                                .or_default()
                                .push(copy_constraint_from_type(other));
                        }
                    }
                }
            }
            if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
                if matches!(lhs_ty, NirType::Ptr(_)) {
                    collect_pointer_assignment_base_constraints(
                        rhs,
                        lhs_ty,
                        known_binding_types,
                        out,
                    );
                }
            }

            if let PreHirExpr::Var(rhs_name) = rhs {
                if let Some(rhs_ty) = known_binding_types.get(rhs_name) {
                    out.entry(lhs_name.clone())
                        .or_default()
                        .push(copy_constraint_from_type(rhs_ty));
                }
            }

            if let PreHirExpr::AddressOfGlobal(_) = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let PreHirExpr::PtrOffset { .. } | PreHirExpr::FieldAccess { .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let PreHirExpr::Load { ty, .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }

            if let PreHirExpr::Index { elem_ty, .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(elem_ty.clone()));
            }

            if let PreHirExpr::Cast {
                ty: NirType::Ptr(pointee),
                ..
            } = rhs
            {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        PreHirLValue::Deref { ty, .. } => {
            if let PreHirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
        PreHirLValue::Index { base, elem_ty, .. } => {
            if let PreHirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(elem_ty.clone()));
            }
            if let PreHirExpr::Var(base_name) = base.as_ref() {
                if matches!(elem_ty, NirType::Float { .. }) {
                    out.entry(base_name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(elem_ty.clone()));
                }
            }
        }
        PreHirLValue::FieldAccess { ty, .. } => {
            if let PreHirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
    }
}

fn collect_pointer_assignment_base_constraints(
    rhs: &PreHirExpr,
    ptr_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let NirType::Ptr(pointee) = ptr_ty else {
        return;
    };
    match rhs {
        // Reverse a plain copy only while the source is unknown. A known source
        // can belong to an earlier scalar role while the destination acquired
        // its pointer type from a later register-reuse definition.
        PreHirExpr::Var(name)
            if matches!(known_binding_types.get(name), None | Some(NirType::Unknown)) =>
        {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        PreHirExpr::Var(_) => {}
        PreHirExpr::AddressOfGlobal(name) => {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        PreHirExpr::Cast { expr, .. } => {
            collect_pointer_assignment_base_constraints(expr, ptr_ty, known_binding_types, out);
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            // ptr + integer = pointer. Only promote a Var base when the other
            // side is a *numeric* offset (const / const arithmetic).
            // Do not treat an integer cast of a bare variable as an offset; it
            // can be an integerized pointer base.
            if expr_is_numeric_offset_with_types(rhs.as_ref(), known_binding_types) {
                if let PreHirExpr::Var(name) = strip_casts_unary(lhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
            if expr_is_numeric_offset_with_types(lhs.as_ref(), known_binding_types) {
                if let PreHirExpr::Var(name) = strip_casts_unary(rhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            ..
        } => {
            if let PreHirExpr::Var(name) = lhs.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        _ => {}
    }
}

fn strip_casts_unary(expr: &PreHirExpr) -> &PreHirExpr {
    let mut cur = expr;
    while let PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } = cur {
        cur = expr.as_ref();
    }
    cur
}

/// Integer offset in pointer arithmetic: const, index vars, scaled index.
/// An integer cast of a bare variable is not sufficient offset evidence; it can
/// be a pointer base forced through integer ALU.
fn expr_is_numeric_offset(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Const(_, _) => true,
        PreHirExpr::Var(_) => true,
        PreHirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            PreHirExpr::Const(_, _) => true,
            PreHirExpr::Binary { .. } => expr_is_numeric_offset(inner),
            // Bare var cast to int is ambiguous (often ptr-to-int for end calc).
            PreHirExpr::Var(_) => false,
            other => expr_is_numeric_offset(other),
        },
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => expr_is_numeric_offset(expr),
        PreHirExpr::Binary {
            op:
                PreHirBinaryOp::Add
                | PreHirBinaryOp::Sub
                | PreHirBinaryOp::Mul
                | PreHirBinaryOp::Shl
                | PreHirBinaryOp::Shr
                | PreHirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } => expr_is_numeric_offset(lhs) && expr_is_numeric_offset(rhs),
        _ => false,
    }
}

fn expr_is_numeric_offset_with_types(
    expr: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    match expr {
        PreHirExpr::Var(name) => !matches!(known_binding_types.get(name), Some(NirType::Ptr(_))),
        PreHirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            PreHirExpr::Var(name) => matches!(
                known_binding_types.get(name),
                Some(NirType::Int { .. } | NirType::Bool)
            ),
            _ => expr_is_numeric_offset_with_types(inner, known_binding_types),
        },
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_is_numeric_offset_with_types(expr, known_binding_types)
        }
        PreHirExpr::Binary {
            op:
                PreHirBinaryOp::Add
                | PreHirBinaryOp::Sub
                | PreHirBinaryOp::Mul
                | PreHirBinaryOp::Shl
                | PreHirBinaryOp::Shr
                | PreHirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } => {
            expr_is_numeric_offset_with_types(lhs, known_binding_types)
                && expr_is_numeric_offset_with_types(rhs, known_binding_types)
        }
        _ => expr_is_numeric_offset(expr),
    }
}

fn expr_is_pointer_offset_like(expr: &PreHirExpr) -> bool {
    expr_is_numeric_offset(expr)
}

fn copy_constraint_from_type(ty: &NirType) -> UseConstraint {
    match ty {
        NirType::Ptr(pointee) => UseConstraint::Ptr(pointee.as_ref().clone()),
        _ => UseConstraint::Exact(ty.clone()),
    }
}

fn collect_copy_alias_sources(stmts: &[PreHirStmt], out: &mut HashMap<String, HashSet<String>>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } => {
                let mut source = rhs;
                while let PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } = source {
                    source = expr.as_ref();
                }
                if let PreHirExpr::Var(source_name) = source {
                    out.entry(name.clone())
                        .or_default()
                        .insert(source_name.clone());
                }
            }
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
                collect_copy_alias_sources(body, out);
            }
            PreHirStmt::DoWhile { body, .. } => collect_copy_alias_sources(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_copy_alias_sources(then_body, out);
                collect_copy_alias_sources(else_body, out);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_copy_alias_sources(std::slice::from_ref(init), out);
                }
                if let Some(update) = update {
                    collect_copy_alias_sources(std::slice::from_ref(update), out);
                }
                collect_copy_alias_sources(body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_copy_alias_sources(&case.body, out);
                }
                collect_copy_alias_sources(default, out);
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn propagate_logical_shift_constraints_through_aliases(
    stmts: &[PreHirStmt],
    constraints: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let mut aliases = HashMap::default();
    collect_copy_alias_sources(stmts, &mut aliases);
    if aliases.is_empty() {
        return;
    }

    let mut work = Vec::new();
    for (name, items) in constraints.iter() {
        for item in items {
            if let UseConstraint::LogicalShiftUnsigned { bits } = item {
                work.push((name.clone(), *bits));
            }
        }
    }
    let mut seen = HashSet::default();
    while let Some((name, bits)) = work.pop() {
        if !seen.insert((name.clone(), bits)) {
            continue;
        }
        let Some(sources) = aliases.get(&name) else {
            continue;
        };
        for source in sources {
            constraints
                .entry(source.clone())
                .or_default()
                .push(UseConstraint::LogicalShiftUnsigned { bits });
            work.push((source.clone(), bits));
        }
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
            && matches!(op, PreHirBinaryOp::Add | PreHirBinaryOp::Sub | PreHirBinaryOp::Mul)
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
                PreHirBinaryOp::SLt | PreHirBinaryOp::SLe | PreHirBinaryOp::SGt | PreHirBinaryOp::SGe => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, true, out)
                }
                // Unsigned comparison → operands are unsigned integers.
                PreHirBinaryOp::Lt | PreHirBinaryOp::Le | PreHirBinaryOp::Gt | PreHirBinaryOp::Ge => {
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
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}

fn collect_compare_constraints(
    lhs: &PreHirExpr,
    rhs: &PreHirExpr,
    result_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    signed: bool,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let lhs_bits = expr_int_bits(lhs, known_binding_types)
        .or_else(|| expr_int_bits(rhs, known_binding_types))
        .or_else(|| nir_type_bits(result_ty));
    let rhs_bits = expr_int_bits(rhs, known_binding_types)
        .or_else(|| expr_int_bits(lhs, known_binding_types))
        .or_else(|| nir_type_bits(result_ty));

    if let (PreHirExpr::Var(name), Some(bits)) = (lhs, lhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
    if let (PreHirExpr::Var(name), Some(bits)) = (rhs, rhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
}

fn compare_constraint(bits: u32, signed: bool) -> UseConstraint {
    if signed {
        UseConstraint::Signed { bits }
    } else {
        UseConstraint::Unsigned { bits }
    }
}

fn collect_arithmetic_result_constraints(
    lhs: &PreHirExpr,
    rhs: &PreHirExpr,
    result_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    if let NirType::Float { bits } = result_ty {
        // FLOAT_MULT/ADD-class binary results force float operands and
        // float* bases for Index/Load that produced those operands.
        collect_float_operand_constraint(lhs, *bits, out);
        collect_float_operand_constraint(rhs, *bits, out);
        return;
    }
    let NirType::Int {
        bits: result_bits,
        signed,
    } = result_ty
    else {
        return;
    };
    collect_arithmetic_operand_constraint(lhs, *result_bits, *signed, known_binding_types, out);
    collect_arithmetic_operand_constraint(rhs, *result_bits, *signed, known_binding_types, out);
}

fn collect_float_operand_constraint(
    expr: &PreHirExpr,
    bits: u32,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let float_ty = NirType::Float { bits };
    match expr {
        PreHirExpr::Var(name) => {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Exact(float_ty));
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_float_operand_constraint(expr, bits, out);
        }
        PreHirExpr::Load { ptr, .. } => {
            if let PreHirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(float_ty));
            }
            collect_float_operand_constraint(ptr, bits, out);
        }
        PreHirExpr::Index { base, .. } => {
            if let PreHirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(float_ty));
            }
            collect_float_operand_constraint(base, bits, out);
        }
        _ => {}
    }
}

fn collect_arithmetic_operand_constraint(
    expr: &PreHirExpr,
    result_bits: u32,
    signed: bool,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let PreHirExpr::Var(name) = expr else {
        return;
    };
    if expr_int_bits(expr, known_binding_types) != Some(result_bits) {
        return;
    }
    out.entry(name.clone())
        .or_default()
        .push(compare_constraint(result_bits, signed));
}

fn is_byte_int_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { bits: 8, .. })
}

fn is_byte_pointer_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Ptr(pointee) if is_byte_int_type(pointee.as_ref()))
}

fn is_byte_expr(expr: &PreHirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).is_some_and(is_byte_int_type)
        }
        PreHirExpr::Const(value, ty) => is_byte_int_type(ty) || (0..=0xff).contains(value),
        PreHirExpr::Load { ty, .. }
        | PreHirExpr::Index { elem_ty: ty, .. }
        | PreHirExpr::FieldAccess { ty, .. } => is_byte_int_type(ty),
        PreHirExpr::Cast { ty, expr } => {
            is_byte_int_type(ty) || is_byte_expr(expr, known_binding_types)
        }
        _ => false,
    }
}

fn is_byte_pointer_expr(expr: &PreHirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => known_binding_types
            .get(name)
            .is_some_and(is_byte_pointer_type),
        PreHirExpr::Cast { ty, expr } => {
            is_byte_pointer_type(ty) || is_byte_pointer_expr(expr, known_binding_types)
        }
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            is_byte_pointer_expr(base, known_binding_types)
        }
        _ => false,
    }
}

fn expr_is_var(expr: &PreHirExpr, name: &str) -> bool {
    matches!(expr, PreHirExpr::Var(var_name) if var_name == name)
}

fn is_byte_accumulator_update(
    expr: &PreHirExpr,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    let PreHirExpr::Binary { op, lhs, rhs, .. } = expr else {
        return false;
    };
    matches!(
        op,
        PreHirBinaryOp::Add | PreHirBinaryOp::Sub | PreHirBinaryOp::Xor | PreHirBinaryOp::And | PreHirBinaryOp::Or
    ) && ((expr_is_var(lhs, name) && is_byte_expr(rhs, known_binding_types))
        || (expr_is_var(rhs, name) && is_byte_expr(lhs, known_binding_types)))
}

fn direct_var_name(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name.as_str()),
        _ => None,
    }
}

/// Collects byte-index-accumulator evidence for every name in `candidates`
/// simultaneously in one walk over `stmts`, instead of one full-body walk
/// per candidate -- `narrow_byte_index_accumulators` used to call this once
/// per qualifying local, making it O(locals * stmts) on functions with many
/// wide temp locals. `exclude`, when set, is the one local variable whose
/// own leaf references should NOT be recorded in the current expression
/// subtree (used for the single case where a candidate's own recognized
/// self-referential accumulator update, `x = x + 1`, must not also count as
/// a disallowed use of `x` in that same right-hand side -- other candidates
/// appearing in that same subtree are still recorded normally).
fn collect_byte_index_accumulator_evidence(
    stmts: &[PreHirStmt],
    candidates: &HashSet<String>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    for stmt in stmts {
        collect_byte_index_accumulator_evidence_stmt(stmt, candidates, known_binding_types, evidence);
    }
}

fn collect_byte_index_accumulator_evidence_stmt(
    stmt: &PreHirStmt,
    candidates: &HashSet<String>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            let self_def_name = match lhs {
                PreHirLValue::Var(lhs_name) if candidates.contains(lhs_name) => {
                    Some(lhs_name.as_str())
                }
                _ => None,
            };
            if let Some(name) = self_def_name {
                let is_accum_update = is_byte_accumulator_update(rhs, name, known_binding_types);
                let ev = evidence.entry(name.to_owned()).or_default();
                ev.def_count += 1;
                if is_byte_expr(rhs, known_binding_types) {
                    ev.byte_seed_defs += 1;
                } else if is_accum_update {
                    ev.byte_update_defs += 1;
                } else {
                    ev.disallowed_uses += 1;
                }
                let exclude = if is_accum_update { Some(name) } else { None };
                collect_byte_index_accumulator_evidence_expr(
                    rhs,
                    candidates,
                    exclude,
                    known_binding_types,
                    evidence,
                );
            } else {
                collect_byte_index_accumulator_evidence_lvalue(
                    lhs,
                    candidates,
                    None,
                    known_binding_types,
                    evidence,
                );
                collect_byte_index_accumulator_evidence_expr(
                    rhs,
                    candidates,
                    None,
                    known_binding_types,
                    evidence,
                );
            }
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            collect_byte_index_accumulator_evidence_expr(
                expr,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            collect_byte_index_accumulator_evidence(body, candidates, known_binding_types, evidence);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_byte_index_accumulator_evidence_expr(
                cond,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence(then_body, candidates, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence(else_body, candidates, known_binding_types, evidence);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_byte_index_accumulator_evidence_stmt(init, candidates, known_binding_types, evidence);
            }
            if let Some(cond) = cond {
                collect_byte_index_accumulator_evidence_expr(
                    cond,
                    candidates,
                    None,
                    known_binding_types,
                    evidence,
                );
            }
            if let Some(update) = update {
                collect_byte_index_accumulator_evidence_stmt(update, candidates, known_binding_types, evidence);
            }
            collect_byte_index_accumulator_evidence(body, candidates, known_binding_types, evidence);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_byte_index_accumulator_evidence_expr(
                expr,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
            for case in cases {
                collect_byte_index_accumulator_evidence(&case.body, candidates, known_binding_types, evidence);
            }
            collect_byte_index_accumulator_evidence(default, candidates, known_binding_types, evidence);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                va_list,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_byte_index_accumulator_evidence_lvalue(
    lhs: &PreHirLValue,
    candidates: &HashSet<String>,
    exclude: Option<&str>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } | PreHirLValue::FieldAccess { base: ptr, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                ptr,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirLValue::Index { base, index, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                base,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                index,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
    }
}

fn collect_byte_index_accumulator_evidence_expr(
    expr: &PreHirExpr,
    candidates: &HashSet<String>,
    exclude: Option<&str>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    match expr {
        PreHirExpr::Var(var_name) | PreHirExpr::AddressOfGlobal(var_name)
            if candidates.contains(var_name) && Some(var_name.as_str()) != exclude =>
        {
            evidence.entry(var_name.clone()).or_default().disallowed_uses += 1;
        }
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                inner,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if direct_var_name(lhs)
            .is_some_and(|v| candidates.contains(v) && Some(v) != exclude)
            && is_byte_pointer_expr(rhs, known_binding_types) =>
        {
            let v = direct_var_name(lhs).expect("guard matched Some");
            evidence.entry(v.to_owned()).or_default().byte_pointer_offset_uses += 1;
            // `lhs` (the identified candidate) is fully accounted for above;
            // `rhs` (the pointer operand) may still reference *other*
            // candidates and still needs its own scan.
            collect_byte_index_accumulator_evidence_expr(
                rhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if direct_var_name(rhs)
            .is_some_and(|v| candidates.contains(v) && Some(v) != exclude)
            && is_byte_pointer_expr(lhs, known_binding_types) =>
        {
            let v = direct_var_name(rhs).expect("guard matched Some");
            evidence.entry(v.to_owned()).or_default().byte_pointer_offset_uses += 1;
            collect_byte_index_accumulator_evidence_expr(
                lhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                lhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                rhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_byte_index_accumulator_evidence_expr(
                    arg,
                    candidates,
                    exclude,
                    known_binding_types,
                    evidence,
                );
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                base,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                index,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_byte_index_accumulator_evidence_expr(
                cond,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                then_expr,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                else_expr,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}

fn narrow_byte_index_accumulators(func: &mut PreHirFunction) -> bool {
    let known_binding_types = collect_known_binding_types(func);
    let candidates: HashSet<String> = func
        .locals
        .iter()
        .filter(|binding| {
            binding.surface_type_name.is_none()
                && matches!(
                    binding.origin,
                    Some(NirBindingOrigin::Temp | NirBindingOrigin::TempPreserved)
                )
                && matches!(binding.ty, NirType::Int { bits, .. } if bits > 8)
        })
        .map(|binding| binding.name.clone())
        .collect();
    if candidates.is_empty() {
        return false;
    }

    let mut evidence_by_name: HashMap<String, ByteIndexAccumulatorEvidence> = HashMap::default();
    collect_byte_index_accumulator_evidence(
        &func.body,
        &candidates,
        &known_binding_types,
        &mut evidence_by_name,
    );

    let mut changed = false;
    for binding in &mut func.locals {
        if !candidates.contains(&binding.name) {
            continue;
        }
        let Some(evidence) = evidence_by_name.get(&binding.name) else {
            continue;
        };
        if evidence.def_count > 0
            && evidence.disallowed_uses == 0
            && evidence.byte_seed_defs > 0
            && evidence.byte_update_defs > 0
            && evidence.byte_pointer_offset_uses > 0
        {
            binding.ty = NirType::Int {
                bits: 8,
                signed: false,
            };
            changed = true;
        }
    }
    changed
}

fn expr_int_bits(expr: &PreHirExpr, known_binding_types: &HashMap<String, NirType>) -> Option<u32> {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).and_then(nir_type_bits)
        }
        PreHirExpr::Const(_, ty)
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Call { ty, .. }
        | PreHirExpr::Load { ty, .. }
        | PreHirExpr::Index { elem_ty: ty, .. }
        | PreHirExpr::Cast { ty, .. }
        | PreHirExpr::Select { ty, .. }
        | PreHirExpr::FieldAccess { ty, .. } => nir_type_bits(ty),
        PreHirExpr::Binary { ty, .. } => nir_type_bits(ty),
        PreHirExpr::PtrOffset { .. } | PreHirExpr::AggregateCopy { .. } => None,
    }
}

fn indirect_call_target_binding_name(target: &str) -> Option<&str> {
    if is_binding_name(target) {
        return Some(target);
    }
    target
        .strip_prefix("((code *)")
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|name| is_binding_name(name))
}

fn is_binding_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Extract the bit-width of an integer/bool NirType, if applicable.
fn nir_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Int { bits, .. } => Some(*bits),
        NirType::Bool => None,
        _ => None,
    }
}

fn collect_known_binding_types(func: &PreHirFunction) -> HashMap<String, NirType> {
    let mut known = HashMap::default();
    for binding in func.locals.iter().chain(func.params.iter()) {
        if binding.ty != NirType::Unknown {
            known.insert(binding.name.clone(), binding.ty.clone());
        }
    }
    known
}

fn return_expr_type(
    expr: &PreHirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).cloned()
        }
        other => {
            let ty = expr_type(other);
            (ty != NirType::Unknown).then_some(ty)
        }
    }
}

fn collect_value_return_types(
    stmts: &[PreHirStmt],
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    let mut value_return_count = 0usize;
    for stmt in stmts {
        value_return_count += collect_value_return_types_stmt(stmt, known_binding_types, out);
    }
    value_return_count
}

fn collect_value_return_types_stmt(
    stmt: &PreHirStmt,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        PreHirStmt::Return(Some(expr)) => {
            if let Some(ty) = return_expr_type(expr, known_binding_types) {
                out.push(ty);
            }
            1
        }
        PreHirStmt::Return(None) => 0,
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            collect_value_return_types(stmts, known_binding_types, out)
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_value_return_types(then_body, known_binding_types, out)
                + collect_value_return_types(else_body, known_binding_types, out)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut value_return_count = 0usize;
            for case in cases {
                value_return_count +=
                    collect_value_return_types(&case.body, known_binding_types, out);
            }
            value_return_count + collect_value_return_types(default, known_binding_types, out)
        }
        _ => 0,
    }
}

fn promote_return_signedness_from_returns(func: &mut PreHirFunction) -> bool {
    if func.surface_return_type_name.is_some() {
        return false;
    }
    let NirType::Int {
        bits: return_bits,
        signed: false,
    } = &func.return_type
    else {
        return false;
    };
    let return_bits = *return_bits;

    let known_binding_types = collect_known_binding_types(func);
    let mut candidates = Vec::new();
    let value_return_count =
        collect_value_return_types(&func.body, &known_binding_types, &mut candidates);
    if value_return_count == 0 || candidates.len() != value_return_count {
        return false;
    }
    if candidates.iter().all(|ty| {
        matches!(
            ty,
            NirType::Int {
                bits,
                signed: true
            } if *bits == return_bits
        )
    }) {
        func.return_type = NirType::Int {
            bits: return_bits,
            signed: true,
        };
        true
    } else {
        false
    }
}

fn promote_unknown_call_return_type(func: &mut PreHirFunction) -> bool {
    if func.surface_return_type_name.is_some() || func.return_type != NirType::Unknown {
        return false;
    }
    let mut value_return_count = 0usize;
    let mut unknown_call_return_count = 0usize;
    collect_unknown_call_returns(
        &func.body,
        &mut value_return_count,
        &mut unknown_call_return_count,
    );
    if value_return_count == 0 || value_return_count != unknown_call_return_count {
        return false;
    }
    func.return_type = native_unsigned_word_type(func);
    true
}

fn native_unsigned_word_type(func: &PreHirFunction) -> NirType {
    NirType::Int {
        bits: if func.is_64bit { 64 } else { 32 },
        signed: false,
    }
}

fn collect_unknown_call_returns(
    stmts: &[PreHirStmt],
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    for stmt in stmts {
        collect_unknown_call_returns_stmt(stmt, value_return_count, unknown_call_return_count);
    }
}

fn collect_unknown_call_returns_stmt(
    stmt: &PreHirStmt,
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    match stmt {
        PreHirStmt::Return(Some(expr)) => {
            *value_return_count += 1;
            if is_unknown_call_result(expr) {
                *unknown_call_return_count += 1;
            }
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            collect_unknown_call_returns(stmts, value_return_count, unknown_call_return_count);
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_unknown_call_returns(then_body, value_return_count, unknown_call_return_count);
            collect_unknown_call_returns(else_body, value_return_count, unknown_call_return_count);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_unknown_call_returns(
                    &case.body,
                    value_return_count,
                    unknown_call_return_count,
                );
            }
            collect_unknown_call_returns(default, value_return_count, unknown_call_return_count);
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn is_unknown_call_result(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { ty, .. } => *ty == NirType::Unknown,
        PreHirExpr::Cast { expr, ty } if *ty == NirType::Unknown => is_unknown_call_result(expr),
        _ => false,
    }
}

fn count_var_uses_expr(expr: &PreHirExpr, out: &mut HashMap<String, usize>) {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => count_var_uses_expr(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_uses_expr(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                count_var_uses_expr(arg, out);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_uses_expr(cond, out);
            count_var_uses_expr(then_expr, out);
            count_var_uses_expr(else_expr, out);
        }
    }
}

fn count_var_uses_lvalue(lhs: &PreHirLValue, out: &mut HashMap<String, usize>) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => count_var_uses_expr(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            count_var_uses_expr(base, out);
        }
    }
}

fn count_var_uses_stmt(stmt: &PreHirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            count_var_uses_lvalue(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        PreHirStmt::VaStart { va_list, .. } | PreHirStmt::Expr(va_list) => {
            count_var_uses_expr(va_list, out);
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => count_var_uses_stmts(stmts, out),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses_expr(cond, out);
            count_var_uses_stmts(then_body, out);
            count_var_uses_stmts(else_body, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                count_var_uses_stmt(init, out);
            }
            if let Some(cond) = cond {
                count_var_uses_expr(cond, out);
            }
            if let Some(update) = update {
                count_var_uses_stmt(update, out);
            }
            count_var_uses_stmts(body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_uses_expr(expr, out);
            for case in cases {
                count_var_uses_stmts(&case.body, out);
            }
            count_var_uses_stmts(default, out);
        }
        PreHirStmt::Return(Some(expr)) => count_var_uses_expr(expr, out),
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn count_var_uses_stmts(stmts: &[PreHirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_var_uses_stmt(stmt, out);
    }
}

fn store_value_var_name(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name.as_str()),
        PreHirExpr::Cast { expr, .. } => store_value_var_name(expr),
        _ => None,
    }
}

fn count_store_value_uses_stmt(stmt: &PreHirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { .. } | PreHirLValue::Index { .. },
            rhs,
        } => {
            if let Some(name) = store_value_var_name(rhs) {
                *out.entry(name.to_owned()).or_default() += 1;
            }
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => count_store_value_uses_stmts(stmts, out),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            count_store_value_uses_stmts(then_body, out);
            count_store_value_uses_stmts(else_body, out);
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                count_store_value_uses_stmt(init, out);
            }
            if let Some(update) = update {
                count_store_value_uses_stmt(update, out);
            }
            count_store_value_uses_stmts(body, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_store_value_uses_stmts(&case.body, out);
            }
            count_store_value_uses_stmts(default, out);
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn count_store_value_uses_stmts(stmts: &[PreHirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_store_value_uses_stmt(stmt, out);
    }
}

fn promote_store_value_only_unsigned_params(func: &mut PreHirFunction) -> bool {
    let mut all_uses = HashMap::default();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut store_value_uses = HashMap::default();
    count_store_value_uses_stmts(&func.body, &mut store_value_uses);

    let mut changed = false;
    for binding in &mut func.params {
        if binding.surface_type_name.is_some()
            || !matches!(binding.origin, Some(NirBindingOrigin::ParamIndex(_)))
        {
            continue;
        }
        let NirType::Int {
            bits: 32,
            signed: false,
        } = binding.ty
        else {
            continue;
        };
        let all = all_uses.get(&binding.name).copied().unwrap_or(0);
        let stores = store_value_uses.get(&binding.name).copied().unwrap_or(0);
        if all > 0 && all == stores {
            binding.ty = NirType::Int {
                bits: 32,
                signed: true,
            };
            changed = true;
        }
    }
    changed
}

fn wrapping_narrow_op(op: PreHirBinaryOp) -> bool {
    matches!(
        op,
        PreHirBinaryOp::Add
            | PreHirBinaryOp::Sub
            | PreHirBinaryOp::Mul
            | PreHirBinaryOp::And
            | PreHirBinaryOp::Or
            | PreHirBinaryOp::Xor
    )
}

fn collect_wrapping_narrow_return_vars(
    expr: &PreHirExpr,
    context_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        PreHirExpr::Cast { ty, expr } => {
            let bits = nir_type_bits(ty).unwrap_or(context_bits).min(context_bits);
            collect_wrapping_narrow_return_vars(expr, bits, out);
        }
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Neg,
            expr,
            ..
        } => collect_wrapping_narrow_return_vars(expr, context_bits, out),
        PreHirExpr::Binary { op, lhs, rhs, .. } if wrapping_narrow_op(*op) => {
            collect_wrapping_narrow_return_vars(lhs, context_bits, out);
            collect_wrapping_narrow_return_vars(rhs, context_bits, out);
        }
        PreHirExpr::Const(_, _)
        | PreHirExpr::Unary { .. }
        | PreHirExpr::Binary { .. }
        | PreHirExpr::Call { .. }
        | PreHirExpr::Load { .. }
        | PreHirExpr::PtrOffset { .. }
        | PreHirExpr::Index { .. }
        | PreHirExpr::Select { .. }
        | PreHirExpr::FieldAccess { .. }
        | PreHirExpr::AggregateCopy { .. } => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmt(
    stmt: &PreHirStmt,
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match stmt {
        PreHirStmt::Return(Some(expr)) => collect_wrapping_narrow_return_vars(expr, return_bits, out),
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => {
            collect_wrapping_narrow_return_vars_stmts(stmts, return_bits, out)
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_wrapping_narrow_return_vars_stmts(then_body, return_bits, out);
            collect_wrapping_narrow_return_vars_stmts(else_body, return_bits, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_wrapping_narrow_return_vars_stmts(&case.body, return_bits, out);
            }
            collect_wrapping_narrow_return_vars_stmts(default, return_bits, out);
        }
        _ => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmts(
    stmts: &[PreHirStmt],
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    for stmt in stmts {
        collect_wrapping_narrow_return_vars_stmt(stmt, return_bits, out);
    }
}

fn narrow_integer_params_from_wrapping_return_uses(func: &mut PreHirFunction) -> bool {
    let NirType::Int {
        bits: return_bits,
        signed: return_signed,
    } = &func.return_type
    else {
        return false;
    };
    let return_bits = *return_bits;
    let return_signed = *return_signed;
    if return_bits >= 64 {
        return false;
    }

    let mut all_uses = HashMap::default();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut constrained_uses = HashMap::default();
    collect_wrapping_narrow_return_vars_stmts(&func.body, return_bits, &mut constrained_uses);

    let mut changed = false;
    for binding in &mut func.params {
        if binding.surface_type_name.is_some() {
            continue;
        }
        if !matches!(binding.origin, Some(NirBindingOrigin::ParamIndex(_))) {
            continue;
        }
        let NirType::Int { bits, .. } = binding.ty else {
            continue;
        };
        if bits <= return_bits {
            continue;
        }
        let all = all_uses.get(&binding.name).copied().unwrap_or(0);
        let constrained = constrained_uses.get(&binding.name).copied().unwrap_or(0);
        if all > 0 && all == constrained {
            binding.ty = NirType::Int {
                bits: return_bits,
                signed: return_signed,
            };
            changed = true;
        }
    }
    changed
}

/// Merge a `UseConstraint` into a binding, returning `true` if the type changed.
///
/// The merge is monotone: types only move from weaker to stronger:
/// `Unknown < Int(unsigned) < Int(signed) < Ptr(Unknown) < Ptr(known)`.
/// An existing `Known` type is NEVER overwritten.
fn merge_constraint(binding: &mut PreHirBinding, constraint: &UseConstraint) -> bool {
    if binding.surface_type_name.is_some() {
        return false;
    }
    match (&binding.ty, constraint) {
        // Already has a strong type — don't overwrite (except float upgrades).
        (NirType::Float { .. }, _) => false,
        (NirType::Bool, _) => false,
        // Full XMM vector bindings start as Aggregate(16) from PXOR zeroing;
        // FLOAT_ADD/MULT use evidence upgrades them to a float lane type.
        (
            NirType::Aggregate { .. },
            UseConstraint::Exact(NirType::Float { bits }),
        ) => {
            binding.ty = NirType::Float { bits: *bits };
            true
        }
        (NirType::Aggregate { .. }, _) => false,

        // Same-width int pointee → float pointee when float ops prove element type.
        // Size-4 loads default to `uint*`; FLOAT_MULT use upgrades to `float*`.
        (
            NirType::Ptr(cur_pointee),
            UseConstraint::Ptr(NirType::Float { bits: float_bits }),
        ) => match cur_pointee.as_ref() {
            NirType::Int { bits, .. } if bits == float_bits => {
                binding.ty = NirType::Ptr(Box::new(NirType::Float {
                    bits: *float_bits,
                }));
                true
            }
            NirType::Unknown => {
                binding.ty = NirType::Ptr(Box::new(NirType::Float {
                    bits: *float_bits,
                }));
                true
            }
            _ => false,
        },
        (NirType::Ptr(_), _) => false,

        // Pointer constraint — always upgrade when current is Unknown or Int.
        (_, UseConstraint::Ptr(pointee)) => {
            let new_ty = NirType::Ptr(Box::new(pointee.clone()));
            if binding.ty != new_ty {
                binding.ty = new_ty;
                true
            } else {
                false
            }
        }

        // Exact type from Cast context — only upgrade Unknown.
        (NirType::Unknown, UseConstraint::Exact(ty)) => {
            binding.ty = ty.clone();
            true
        }
        // Same-width int → float when float arithmetic uses the value.
        (
            NirType::Int { bits, .. },
            UseConstraint::Exact(NirType::Float {
                bits: float_bits,
            }),
        ) if bits == float_bits => {
            binding.ty = NirType::Float {
                bits: *float_bits,
            };
            true
        }
        (
            NirType::Int { .. },
            UseConstraint::Exact(NirType::Int {
                bits: new_bits,
                signed: new_signed,
            }),
        ) => {
            // Only change signedness if currently unsigned → promote to signed.
            if let NirType::Int {
                signed: cur_signed,
                bits: cur_bits,
            } = &binding.ty
            {
                if !*cur_signed && *new_signed && cur_bits == new_bits {
                    binding.ty = NirType::Int {
                        bits: *new_bits,
                        signed: true,
                    };
                    return true;
                }
            }
            false
        }
        (_, UseConstraint::Exact(_)) => false,

        // Signed/unsigned constraint — apply if Unknown or conflicting.
        (NirType::Unknown, UseConstraint::Signed { bits }) => {
            binding.ty = NirType::Int {
                bits: *bits,
                signed: true,
            };
            true
        }
        (NirType::Unknown, UseConstraint::Unsigned { bits })
        | (NirType::Unknown, UseConstraint::LogicalShiftUnsigned { bits }) => {
            binding.ty = NirType::Int {
                bits: *bits,
                signed: false,
            };
            true
        }
        (
            NirType::Int {
                signed: false,
                bits: cur_bits,
            },
            UseConstraint::Signed { bits: new_bits },
        ) if cur_bits == new_bits => {
            // Promote from unsigned to signed.
            binding.ty = NirType::Int {
                bits: *new_bits,
                signed: true,
            };
            true
        }
        (
            NirType::Int {
                signed: true,
                bits: cur_bits,
            },
            UseConstraint::LogicalShiftUnsigned { bits: new_bits },
        ) if cur_bits == new_bits => {
            // Demote signed scalars → unsigned only for logical SHR (INT_RIGHT).
            // Generic `Unsigned` must not undo signed promotion from signed
            // comparisons / casted arithmetic (see signed_casted_arithmetic test).
            binding.ty = NirType::Int {
                bits: *new_bits,
                signed: false,
            };
            true
        }
        _ => false,
    }
}

fn has_exact_scalar_constraint(constraints: Option<&Vec<UseConstraint>>) -> bool {
    constraints.is_some_and(|constraints| {
        constraints.iter().any(|constraint| {
            matches!(
                constraint,
                UseConstraint::Exact(NirType::Int { .. } | NirType::Bool)
            )
        })
    })
}

fn restore_scalar_only_pointer_locals(
    func: &mut PreHirFunction,
    constraints: &HashMap<String, Vec<UseConstraint>>,
    roles: &HashMap<String, BindingUseRole>,
    dependencies: &DefinitionDependencyMap,
) -> bool {
    let pointer_compare_peers = super::type_infer::pointer_compare_peer_promotions(func);
    let transitive_address_locals =
        super::type_infer::transitive_address_pointer_locals_with_dependencies(func, dependencies);
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

fn should_skip_pointer_constraint_for_scalar_local(
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

/// Apply the use-driven backward type inference pass.
///
/// Iterates to convergence (typically 1–2 rounds).  Returns `true` if any
/// binding type changed.
pub fn apply_use_driven_type_infer_pass(func: &mut PreHirFunction) -> bool {
    let before = type_state_signature(func);
    let dependencies = DefinitionDependencyMap::build(&func.body);
    // `func.body`'s shape (as opposed to binding types) never changes across
    // this function's rounds below -- every sub-pass in the loop only
    // rewrites `.ty` fields, never statements/expressions -- so the use-role
    // classification (which reads only expression shape: Deref/Index/Load
    // context vs. arithmetic context) is round-invariant. Computing it once
    // here instead of on every round (and again inside
    // `restore_scalar_only_pointer_locals`, which used to rebuild its own
    // copy) turns up to 8 full-body walks per call into 1.
    let mut roles = HashMap::<String, BindingUseRole>::default();
    collect_binding_use_roles(&func.body, &mut roles);
    let mut flow_changed = false;
    // Iterate to convergence (alias chains may require multiple rounds).
    for _ in 0..4 {
        let current_flow_changed = super::type_flow::apply_type_flow_pass(func);
        flow_changed |= current_flow_changed;
        let mut constraints: HashMap<String, Vec<UseConstraint>> = HashMap::default();
        let known_binding_types = collect_known_binding_types(func);
        let pointer_roots = func
            .params
            .iter()
            .filter_map(|binding| {
                matches!(binding.ty, NirType::Ptr(_)).then_some(binding.name.clone())
            })
            .collect();
        let address_contributors = dependencies.address_contributors(&func.body, &pointer_roots);
        collect_constraints(
            &func.body,
            &func.return_type,
            &known_binding_types,
            &mut constraints,
        );
        propagate_logical_shift_constraints_through_aliases(&func.body, &mut constraints);

        let mut round_changed = current_flow_changed;
        for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
            if let Some(constraints_for) = constraints.get(&binding.name) {
                for constraint in constraints_for {
                    if should_skip_pointer_constraint_for_scalar_local(
                        binding,
                        constraint,
                        &roles,
                        &address_contributors,
                    ) {
                        continue;
                    }
                    round_changed |= merge_constraint(binding, constraint);
                }
            }
        }
        round_changed |= promote_unknown_call_return_type(func);
        round_changed |= promote_return_signedness_from_returns(func);
        round_changed |= narrow_integer_params_from_wrapping_return_uses(func);
        round_changed |= promote_store_value_only_unsigned_params(func);
        round_changed |= restore_scalar_only_pointer_locals(func, &constraints, &roles, &dependencies);
        round_changed |= narrow_byte_index_accumulators(func);
        if !round_changed {
            break;
        }
    }
    flow_changed || type_state_signature(func) != before
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    fn make_binding(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_owned(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_typed_binding(name: &str, ty: NirType, origin: NirBindingOrigin) -> PreHirBinding {
        PreHirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<PreHirBinding>, body: Vec<PreHirStmt>, return_type: NirType) -> PreHirFunction {
        PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals,
            return_type,
            surface_return_type_name: None,
            body,
            ..Default::default()
        }
    }

    /// `x = a * b` with float result upgrades int temps and `uint*` bases.
    #[test]
    fn float_mul_promotes_int_operands_and_uint_pointer_base() {
        let float_ty = NirType::Float { bits: 32 };
        let uint_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("a".to_owned()),
                rhs: PreHirExpr::Index {
                    base: Box::new(PreHirExpr::Var("p".to_owned())),
                    index: Box::new(PreHirExpr::Const(0, uint_ty.clone())),
                    elem_ty: uint_ty.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("prod".to_owned()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(PreHirExpr::Var("a".to_owned())),
                    rhs: Box::new(PreHirExpr::Var("b".to_owned())),
                    ty: float_ty.clone(),
                },
            },
        ];
        let mut func = make_func(
            vec![
                make_typed_binding(
                    "p",
                    NirType::Ptr(Box::new(uint_ty.clone())),
                    NirBindingOrigin::ParamIndex(0),
                ),
                make_typed_binding("a", uint_ty.clone(), NirBindingOrigin::Temp),
                make_typed_binding("b", uint_ty.clone(), NirBindingOrigin::Temp),
                make_typed_binding("prod", uint_ty, NirBindingOrigin::Temp),
            ],
            body,
            NirType::Unknown,
        );
        // Move `p` to params so ParamIndex origin matches production shape.
        let p = func.locals.remove(0);
        func.params.push(p);

        let changed = super::apply_use_driven_type_infer_pass(&mut func);
        assert!(changed, "float mul must promote operand/base types");
        assert_eq!(
            func.params[0].ty,
            NirType::Ptr(Box::new(float_ty.clone())),
            "uint* base of float mul load must become float*"
        );
        assert_eq!(
            func.locals.iter().find(|b| b.name == "a").map(|b| &b.ty),
            Some(&float_ty),
            "float mul lhs operand must become float"
        );
        assert_eq!(
            func.locals.iter().find(|b| b.name == "b").map(|b| &b.ty),
            Some(&float_ty),
            "float mul rhs operand must become float"
        );
    }

    /// A float store through a default uint pointer refines the complete alias
    /// chain back to the original parameter.
    #[test]
    fn float_deref_store_refines_pointer_alias_chain() {
        let float_ty = NirType::Float { bits: 32 };
        let uint_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let uint_ptr = NirType::Ptr(Box::new(uint_ty.clone()));
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("param_home".to_owned()),
                rhs: PreHirExpr::Var("out".to_owned()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("store_ptr".to_owned()),
                rhs: PreHirExpr::Var("param_home".to_owned()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("store_ptr".to_owned())),
                    ty: uint_ty,
                },
                rhs: PreHirExpr::Var("value".to_owned()),
            },
        ];
        let mut func = make_func(
            vec![
                make_typed_binding("param_home", uint_ptr.clone(), NirBindingOrigin::Temp),
                make_typed_binding("store_ptr", uint_ptr.clone(), NirBindingOrigin::Temp),
                make_typed_binding("value", float_ty.clone(), NirBindingOrigin::Temp),
            ],
            body,
            NirType::Unknown,
        );
        func.params.push(make_typed_binding(
            "out",
            uint_ptr,
            NirBindingOrigin::ParamIndex(2),
        ));

        let changed = super::apply_use_driven_type_infer_pass(&mut func);
        assert!(changed, "float store must refine pointer aliases");
        let float_ptr = NirType::Ptr(Box::new(float_ty));
        assert_eq!(func.params[0].ty, float_ptr);
        assert_eq!(
            func.locals
                .iter()
                .find(|binding| binding.name == "param_home")
                .map(|binding| &binding.ty),
            Some(&float_ptr)
        );
        assert_eq!(
            func.locals
                .iter()
                .find(|binding| binding.name == "store_ptr")
                .map(|binding| &binding.ty),
            Some(&float_ptr)
        );
    }

    /// Load { ptr: Var("p"), ty: uint32 } → p: Ptr(uint32)
    #[test]
    fn infers_ptr_from_load() {
        let body = vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_owned()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(
            vec![make_binding("p"), make_binding("x")],
            body,
            NirType::Unknown,
        );
        let changed = super::apply_use_driven_type_infer_pass(&mut func);
        assert!(changed);
        assert_eq!(
            func.locals[0].ty,
            NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false
            }))
        );
    }

    #[test]
    fn narrows_byte_accumulator_used_as_byte_pointer_offset() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("idx".to_owned()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                    ty: u8_ty.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("idx".to_owned()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("idx".to_owned())),
                    rhs: Box::new(PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("q".to_owned())),
                        ty: u8_ty.clone(),
                    }),
                    ty: u32_ty.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("cursor".to_owned()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                    rhs: Box::new(PreHirExpr::Var("idx".to_owned())),
                    ty: byte_ptr_ty.clone(),
                },
            },
        ];
        let mut func = make_func(
            vec![
                make_typed_binding("p", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
                make_typed_binding("q", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
                make_typed_binding("cursor", byte_ptr_ty, NirBindingOrigin::TempPreserved),
                make_typed_binding("idx", u32_ty, NirBindingOrigin::TempPreserved),
            ],
            body,
            NirType::Unknown,
        );

        let changed = super::apply_use_driven_type_infer_pass(&mut func);
        assert!(changed);
        let idx = func
            .locals
            .iter()
            .find(|local| local.name == "idx")
            .unwrap();
        assert_eq!(idx.ty, u8_ty);
    }

    #[test]
    fn does_not_narrow_plain_large_pointer_offset() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
        let body = vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("cursor".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("p".to_owned())),
                rhs: Box::new(PreHirExpr::Var("idx".to_owned())),
                ty: byte_ptr_ty.clone(),
            },
        }];
        let mut func = make_func(
            vec![
                make_typed_binding("p", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
                make_typed_binding("cursor", byte_ptr_ty, NirBindingOrigin::TempPreserved),
                make_typed_binding("idx", u32_ty.clone(), NirBindingOrigin::TempPreserved),
            ],
            body,
            NirType::Unknown,
        );

        super::apply_use_driven_type_infer_pass(&mut func);
        let idx = func
            .locals
            .iter()
            .find(|local| local.name == "idx")
            .unwrap();
        assert_eq!(idx.ty, u32_ty);
    }

    #[test]
    fn pointer_result_does_not_promote_known_integer_offset_param() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u32_ty));
        let body = vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("end".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("base".to_owned())),
                rhs: Box::new(PreHirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("count".to_owned())),
                }),
                ty: ptr_ty.clone(),
            },
        }];
        let mut func = make_func(
            vec![make_typed_binding(
                "end",
                ptr_ty.clone(),
                NirBindingOrigin::TempPreserved,
            )],
            body,
            NirType::Unknown,
        );
        func.params = vec![
            make_typed_binding("base", ptr_ty.clone(), NirBindingOrigin::ParamIndex(0)),
            make_typed_binding("count", u64_ty.clone(), NirBindingOrigin::ParamIndex(1)),
        ];

        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(func.params[0].ty, ptr_ty);
        assert_eq!(func.params[1].ty, u64_ty);
    }

    #[test]
    fn transitive_address_evidence_blocks_stale_scalar_copy_constraint() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("alias".to_owned()),
                rhs: PreHirExpr::Var("input".to_owned()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("cursor".to_owned()),
                rhs: PreHirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("alias".to_owned())),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("value".to_owned()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("cursor".to_owned())),
                    ty: u8_ty,
                },
            },
        ];
        let mut func = make_func(
            vec![
                make_typed_binding("alias", u64_ty, NirBindingOrigin::TempPreserved),
                make_typed_binding("cursor", ptr_ty.clone(), NirBindingOrigin::TempPreserved),
                make_binding("value"),
            ],
            body,
            NirType::Unknown,
        );
        func.params = vec![make_typed_binding(
            "input",
            ptr_ty.clone(),
            NirBindingOrigin::ParamIndex(0),
        )];

        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(func.params[0].ty, ptr_ty);
    }

    #[test]
    fn scalar_only_local_pointer_constraint_converges_once() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p".to_owned()),
                rhs: PreHirExpr::Cast {
                    ty: byte_ptr_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("x".to_owned())),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("sum".to_owned()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("x".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(1, u64_ty.clone())),
                    ty: u64_ty.clone(),
                },
            },
        ];
        let mut func = make_func(
            vec![
                make_typed_binding("x", byte_ptr_ty.clone(), NirBindingOrigin::TempPreserved),
                make_typed_binding("p", byte_ptr_ty, NirBindingOrigin::TempPreserved),
                make_typed_binding("sum", u64_ty.clone(), NirBindingOrigin::TempPreserved),
            ],
            body,
            NirType::Unknown,
        );
        func.is_64bit = true;

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        let x = func.locals.iter().find(|local| local.name == "x").unwrap();
        assert_eq!(x.ty, u64_ty);
        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
    }

    #[test]
    fn pointer_defined_local_with_scalar_address_arithmetic_stays_pointer() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("cursor_base".to_owned()),
                rhs: PreHirExpr::PtrOffset {
                    base: Box::new(PreHirExpr::Var("input".to_owned())),
                    offset: 4,
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("end".to_owned()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("cursor_base".to_owned())),
                    rhs: Box::new(PreHirExpr::Var("span".to_owned())),
                    ty: u64_ty.clone(),
                },
            },
        ];
        let mut func = make_func(
            vec![
                make_typed_binding(
                    "cursor_base",
                    byte_ptr_ty.clone(),
                    NirBindingOrigin::TempPreserved,
                ),
                make_typed_binding("span", u64_ty.clone(), NirBindingOrigin::TempPreserved),
                make_typed_binding("end", u64_ty, NirBindingOrigin::TempPreserved),
            ],
            body,
            NirType::Unknown,
        );
        func.params = vec![make_typed_binding(
            "input",
            byte_ptr_ty.clone(),
            NirBindingOrigin::ParamIndex(0),
        )];
        func.is_64bit = true;

        super::apply_use_driven_type_infer_pass(&mut func);
        let cursor_base = func
            .locals
            .iter()
            .find(|local| local.name == "cursor_base")
            .unwrap();
        assert!(matches!(cursor_base.ty, NirType::Ptr(_)));
        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
    }

    #[test]
    fn exact_scalar_return_constraint_demotes_non_address_pointer_local() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let byte_ptr_ty = NirType::Ptr(Box::new(u8_ty));
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Var("acc".to_owned())))];
        let mut func = make_func(
            vec![make_typed_binding(
                "acc",
                byte_ptr_ty,
                NirBindingOrigin::StackOffset(-4),
            )],
            body,
            u64_ty.clone(),
        );

        super::apply_use_driven_type_infer_pass(&mut func);

        assert_eq!(func.locals[0].ty, u64_ty);
    }

    /// Deref store lhs: *p = val → p: Ptr(val_ty)
    #[test]
    fn infers_ptr_from_deref_store() {
        let body = vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
            rhs: PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
        }];
        let mut func = make_func(vec![make_binding("p")], body, NirType::Unknown);
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(
            func.locals[0].ty,
            NirType::Ptr(Box::new(NirType::Int {
                bits: 64,
                signed: false
            }))
        );
    }

    /// SLt comparison → operand is signed int
    #[test]
    fn infers_signed_from_slt() {
        let body = vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SLt,
                lhs: Box::new(PreHirExpr::Var("a".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![].into(),
            else_body: vec![].into(),
        }];
        let mut func = make_func(vec![make_binding("a")], body, NirType::Unknown);
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 32,
                signed: true
            }
        );
    }

    #[test]
    fn unknown_call_only_value_returns_promote_native_word_return_type() {
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Call {
            target: "param_1".to_owned(),
            args: vec![
                PreHirExpr::Var("param_2".to_owned()),
                PreHirExpr::Var("param_3".to_owned()),
            ],
            ty: NirType::Unknown,
        }))];
        let mut func = make_func(Vec::new(), body, NirType::Unknown);
        func.is_64bit = false;

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(
            func.return_type,
            NirType::Int {
                bits: 32,
                signed: false
            }
        );
    }

    #[test]
    fn call_target_use_promotes_binding_to_pointer() {
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Call {
            target: "((code *)param_1)".to_owned(),
            args: vec![
                PreHirExpr::Var("param_2".to_owned()),
                PreHirExpr::Var("param_3".to_owned()),
            ],
            ty: NirType::Unknown,
        }))];
        let mut func = make_func(
            vec![
                make_typed_binding(
                    "param_1",
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    NirBindingOrigin::ParamIndex(0),
                ),
                make_typed_binding(
                    "param_2",
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    NirBindingOrigin::ParamIndex(1),
                ),
                make_typed_binding(
                    "param_3",
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    NirBindingOrigin::ParamIndex(2),
                ),
            ],
            body,
            NirType::Unknown,
        );
        func.is_64bit = false;

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.locals[0].ty, NirType::Ptr(Box::new(NirType::Unknown)));
    }

    #[test]
    fn unknown_call_return_promotion_requires_all_value_returns_to_be_calls() {
        let body = vec![PreHirStmt::If {
            cond: PreHirExpr::Var("flag".to_owned()),
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Call {
                target: "param_1".to_owned(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }))].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("fallback".to_owned())))].into(),
        }];
        let mut func = make_func(Vec::new(), body, NirType::Unknown);
        func.is_64bit = false;

        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.return_type, NirType::Unknown);
    }

    #[test]
    fn signed_compare_promotes_unsigned_params_and_return() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SLt,
                lhs: Box::new(PreHirExpr::Var("a".to_owned())),
                rhs: Box::new(PreHirExpr::Var("b".to_owned())),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("b".to_owned())))].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("a".to_owned())))].into(),
        }];
        let mut func = PreHirFunction {
            name: "signed_max".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding("a", u32_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("b", u32_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: u32_ty,
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        let signed_i32 = NirType::Int {
            bits: 32,
            signed: true,
        };
        assert_eq!(func.params[0].ty, signed_i32);
        assert_eq!(func.params[1].ty, signed_i32);
        assert_eq!(func.return_type, signed_i32);
    }

    #[test]
    fn logical_shr_demotes_signed_param_to_unsigned() {
        // count_bits-style: signed stack param used with INT_RIGHT must become uint
        // so C `>>` is logical and `0xFFFFFFFF` terminates.
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let body = vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("x".to_owned()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Shr,
                lhs: Box::new(PreHirExpr::Var("x".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: i32_ty.clone(),
            },
        }];
        let mut func = PreHirFunction {
            name: "count_bits".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_typed_binding(
                "x",
                i32_ty,
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![],
            return_type: NirType::Int {
                bits: 32,
                signed: true,
            },
            surface_return_type_name: None,
            body,
            ..Default::default()
        };
        func.is_64bit = false;

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(
            func.params[0].ty,
            NirType::Int {
                bits: 32,
                signed: false,
            },
            "logical SHR must force unsigned param typing"
        );
    }

    #[test]
    fn logical_shr_unsigned_constraint_reaches_param_through_copy_alias() {
        let i64_ty = NirType::Int {
            bits: 64,
            signed: true,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("shifted".into()),
                rhs: PreHirExpr::Var("count".into()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("shifted".into()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shr,
                    lhs: Box::new(PreHirExpr::Var("shifted".into())),
                    rhs: Box::new(PreHirExpr::Const(1, u64_ty.clone())),
                    ty: i64_ty.clone(),
                },
            },
        ];
        let mut func = PreHirFunction {
            name: "test".into(),
            params: vec![make_typed_binding(
                "count",
                i64_ty.clone(),
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![make_typed_binding(
                "shifted",
                i64_ty,
                NirBindingOrigin::Temp,
            )],
            return_type: NirType::Unknown,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[0].ty, u64_ty);
        assert_eq!(func.locals[0].ty, func.params[0].ty);
    }

    #[test]
    fn signed_neutral_arithmetic_result_promotes_operand_signedness() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("a".to_owned())),
            rhs: Box::new(PreHirExpr::Var("b".to_owned())),
            ty: i32_ty.clone(),
        }))];
        let mut func = PreHirFunction {
            name: "add".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding("a", u32_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("b", u32_ty, NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: i32_ty.clone(),
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[0].ty, i32_ty);
        assert_eq!(func.params[1].ty, i32_ty);
    }

    #[test]
    fn signed_casted_arithmetic_result_promotes_operand_signedness() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
            ty: i32_ty.clone(),
            expr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("a".to_owned())),
                rhs: Box::new(PreHirExpr::Var("b".to_owned())),
                ty: u32_ty.clone(),
            }),
        }))];
        let mut func = PreHirFunction {
            name: "add".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding("a", u32_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("b", u32_ty, NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: i32_ty.clone(),
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[0].ty, i32_ty);
        assert_eq!(func.params[1].ty, i32_ty);
    }

    #[test]
    fn wrapping_return_use_narrows_wide_integer_params() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = PreHirFunction {
            name: "add32".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding("param_1", u64_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("param_2", u64_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: u32_ty.clone(),
            surface_return_type_name: None,
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("param_1".to_owned())),
                rhs: Box::new(PreHirExpr::Var("param_2".to_owned())),
                ty: u64_ty,
            }))],
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[0].ty, u32_ty);
        assert_eq!(func.params[1].ty, u32_ty);
    }

    #[test]
    fn wrapping_return_use_does_not_narrow_param_with_unconstrained_use() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = PreHirFunction {
            name: "add32_with_call".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_typed_binding(
                "param_1",
                u64_ty.clone(),
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![],
            return_type: u32_ty,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "observe64".to_owned(),
                    args: vec![PreHirExpr::Var("param_1".to_owned())],
                    ty: NirType::Unknown,
                }),
                PreHirStmt::Return(Some(PreHirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[0].ty, u64_ty);
    }

    #[test]
    fn store_value_only_unsigned_param_defaults_to_signed_int() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = PreHirFunction {
            name: "fill".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding(
                    "param_1",
                    NirType::Ptr(Box::new(u32_ty.clone())),
                    NirBindingOrigin::ParamIndex(0),
                ),
                make_typed_binding("param_2", u32_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("param_1".to_owned())),
                    ty: u32_ty.clone(),
                },
                rhs: PreHirExpr::Var("param_2".to_owned()),
            }],
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[1].ty, i32_ty);
    }

    #[test]
    fn store_value_param_keeps_unsigned_when_used_in_unsigned_comparison() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = PreHirFunction {
            name: "fill_guarded".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding(
                    "param_1",
                    NirType::Ptr(Box::new(u32_ty.clone())),
                    NirBindingOrigin::ParamIndex(0),
                ),
                make_typed_binding("param_2", u32_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Lt,
                        lhs: Box::new(PreHirExpr::Var("param_2".to_owned())),
                        rhs: Box::new(PreHirExpr::Const(10, u32_ty.clone())),
                        ty: NirType::Bool,
                    },
                    then_body: Vec::new().into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::Var("param_1".to_owned())),
                        ty: u32_ty.clone(),
                    },
                    rhs: PreHirExpr::Var("param_2".to_owned()),
                },
            ],
            ..Default::default()
        };

        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[1].ty, u32_ty);
    }

    #[test]
    fn signed_compare_without_width_evidence_does_not_invent_type() {
        let body = vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SLt,
                lhs: Box::new(PreHirExpr::Var("a".to_owned())),
                rhs: Box::new(PreHirExpr::Var("b".to_owned())),
                ty: NirType::Bool,
            },
            then_body: vec![].into(),
            else_body: vec![].into(),
        }];
        let mut func = make_func(
            vec![make_binding("a"), make_binding("b")],
            body,
            NirType::Unknown,
        );
        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.locals[0].ty, NirType::Unknown);
        assert_eq!(func.locals[1].ty, NirType::Unknown);
    }

    #[test]
    fn signed_compare_uses_constant_width_evidence() {
        let body = vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SLt,
                lhs: Box::new(PreHirExpr::Var("a".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 16,
                        signed: true,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![].into(),
            else_body: vec![].into(),
        }];
        let mut func = make_func(vec![make_binding("a")], body, NirType::Unknown);
        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 16,
                signed: true
            }
        );
    }

    /// Return(Var("r")) + known return_type → r gets return_type
    #[test]
    fn infers_type_from_return_context() {
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Var("r".to_owned())))];
        let ret_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = make_func(vec![make_binding("r")], body, ret_ty.clone());
        super::apply_use_driven_type_infer_pass(&mut func);
        assert_eq!(func.locals[0].ty, ret_ty);
    }

    #[test]
    fn propagates_pointer_use_back_through_copy_edge() {
        let uint_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p".to_owned()),
                rhs: PreHirExpr::Var("param_1".to_owned()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: PreHirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = PreHirFunction {
            name: "copy_ptr".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_typed_binding(
                "param_1",
                NirType::Unknown,
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![make_binding("p")],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        let expected = NirType::Ptr(Box::new(uint_ty));
        assert_eq!(func.locals[0].ty, expected);
        assert_eq!(func.params[0].ty, func.locals[0].ty);
    }

    #[test]
    fn propagates_pointer_use_back_through_scaled_pointer_assignment() {
        let uint_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p".to_owned()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("param_1".to_owned())),
                    rhs: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Mul,
                        lhs: Box::new(PreHirExpr::Var("idx".to_owned())),
                        rhs: Box::new(PreHirExpr::Const(4, u64_ty.clone())),
                        ty: u64_ty.clone(),
                    }),
                    ty: u64_ty,
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: PreHirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = PreHirFunction {
            name: "scaled_ptr".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_typed_binding(
                "param_1",
                NirType::Unknown,
                NirBindingOrigin::ParamIndex(0),
            )],
            locals: vec![make_binding("p"), make_binding("idx")],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            ..Default::default()
        };

        assert!(super::apply_use_driven_type_infer_pass(&mut func));
        let expected = NirType::Ptr(Box::new(uint_ty));
        assert_eq!(func.locals[0].ty, expected);
        assert_eq!(func.params[0].ty, func.locals[0].ty);
        assert_eq!(func.locals[1].ty, NirType::Unknown);
    }

    // ── narrow_byte_index_accumulators (multi-candidate single-pass walk) ───

    fn byte_ptr_ty() -> NirType {
        NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }))
    }

    fn wide_int_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_owned()),
            rhs,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_owned())
    }

    fn byte_const(v: i64) -> PreHirExpr {
        PreHirExpr::Const(
            v,
            NirType::Int {
                bits: 8,
                signed: false,
            },
        )
    }

    fn qualifying_accumulator_body(name: &str, ptr_name: &str) -> Vec<PreHirStmt> {
        vec![
            // byte_seed_defs
            assign(name, byte_const(0)),
            // byte_update_defs (`x = x + <byte const>` recognized as an
            // accumulator update)
            assign(
                name,
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(var(name)),
                    rhs: Box::new(byte_const(1)),
                    ty: wide_int_ty(),
                },
            ),
            // byte_pointer_offset_uses
            PreHirStmt::Expr(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(var(ptr_name)),
                rhs: Box::new(var(name)),
                ty: byte_ptr_ty(),
            }),
        ]
    }

    #[test]
    fn narrow_byte_index_accumulators_narrows_multiple_candidates_in_one_pass() {
        let mut body = qualifying_accumulator_body("x", "bptr");
        body.extend(qualifying_accumulator_body("z", "bptr"));
        let mut func = make_func(
            vec![
                make_typed_binding("bptr", byte_ptr_ty(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("x", wide_int_ty(), NirBindingOrigin::Temp),
                make_typed_binding("z", wide_int_ty(), NirBindingOrigin::Temp),
            ],
            body,
            NirType::Unknown,
        );

        assert!(super::narrow_byte_index_accumulators(&mut func));
        let expected = NirType::Int {
            bits: 8,
            signed: false,
        };
        assert_eq!(func.locals[1].ty, expected, "x should narrow to byte");
        assert_eq!(func.locals[2].ty, expected, "z should narrow to byte");
    }

    #[test]
    fn narrow_byte_index_accumulators_skips_candidate_with_disallowed_use() {
        let mut body = qualifying_accumulator_body("x", "bptr");
        // A plain, unrecognized use of `x` elsewhere disqualifies it.
        body.push(PreHirStmt::Expr(PreHirExpr::Call {
            target: "sink".to_owned(),
            args: vec![var("x")],
            ty: NirType::Unknown,
        }));
        let mut func = make_func(
            vec![
                make_typed_binding("bptr", byte_ptr_ty(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("x", wide_int_ty(), NirBindingOrigin::Temp),
            ],
            body,
            NirType::Unknown,
        );

        assert!(!super::narrow_byte_index_accumulators(&mut func));
        assert_eq!(func.locals[1].ty, wide_int_ty());
    }

    #[test]
    fn byte_index_accumulator_evidence_excludes_only_the_self_referential_candidate() {
        // `x = x + (byte)z`: recognized as an accumulator update for `x`, so
        // `x`'s own reference inside that rhs must NOT also count as a
        // disallowed use of `x` -- but `z`, a *different* candidate embedded
        // in the same rhs (via the cast), must still be recorded normally,
        // since from `z`'s perspective this is just an ordinary use, not a
        // self-update.
        let candidates: HashSet<String> = ["x".to_owned(), "z".to_owned()].into_iter().collect();
        let known_binding_types: HashMap<String, NirType> = [
            ("x".to_owned(), wide_int_ty()),
            ("z".to_owned(), wide_int_ty()),
        ]
        .into_iter()
        .collect();
        let body = vec![assign(
            "x",
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(var("x")),
                rhs: Box::new(PreHirExpr::Cast {
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                    expr: Box::new(var("z")),
                }),
                ty: wide_int_ty(),
            },
        )];

        let mut evidence = HashMap::default();
        super::collect_byte_index_accumulator_evidence(
            &body,
            &candidates,
            &known_binding_types,
            &mut evidence,
        );

        let x_evidence = evidence.get("x").expect("x should have evidence");
        assert_eq!(x_evidence.def_count, 1);
        assert_eq!(x_evidence.byte_update_defs, 1);
        assert_eq!(
            x_evidence.disallowed_uses, 0,
            "x's own reference inside its recognized self-update must not double-count"
        );

        let z_evidence = evidence.get("z").expect("z should have evidence");
        assert_eq!(
            z_evidence.disallowed_uses, 1,
            "z, embedded in x's update rhs, is still an ordinary (disallowed) use from z's own perspective"
        );
        assert_eq!(z_evidence.def_count, 0);
    }
}
