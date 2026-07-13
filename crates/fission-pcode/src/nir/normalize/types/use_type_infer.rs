use super::super::analysis::defuse::DefinitionDependencyMap;
/// Use-driven backward type propagation pass.
///
/// `apply_type_inference_pass` (type_infer.rs) propagates types forward from
/// *definition* sites: if `x = (int32)...` then `x.ty = Int32`.  It cannot
/// infer types from *use* sites because `expr_type(HirExpr::Var(_)) = Unknown`.
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
/// 2. Merge all collected constraints into `NirBinding.ty` for locals and
///    params that are still `Unknown`.  Constraints are only *strengthened*
///    (Unknown → Ptr → Int with signedness), never weakened.
///
/// 3. Iterate until convergence (usually 1–2 rounds via the var-chain alias
///    mechanism).
///
/// This pass is binary-independent and heuristic-free.  It is placed right
/// after `apply_type_inference_pass` so that the def-driven types it computed
/// can serve as additional seeds for backward propagation.
use super::super::*;
use std::collections::{HashMap, HashSet};

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

fn type_state_signature(func: &HirFunction) -> TypeStateSignature {
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
    stmts: &[HirStmt],
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    for stmt in stmts {
        collect_constraints_stmt(stmt, return_type, known_binding_types, out);
    }
}

fn collect_binding_use_roles(stmts: &[HirStmt], out: &mut HashMap<String, BindingUseRole>) {
    for stmt in stmts {
        collect_binding_use_roles_stmt(stmt, out);
    }
}

fn collect_binding_use_roles_stmt(stmt: &HirStmt, out: &mut HashMap<String, BindingUseRole>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_binding_use_roles_lvalue(lhs, out);
            collect_binding_use_roles_expr(rhs, out);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            collect_binding_use_roles_expr(expr, out);
        }
        HirStmt::VaStart { va_list, .. } => collect_binding_use_roles_expr(va_list, out),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_binding_use_roles(body, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_binding_use_roles_expr(cond, out);
            collect_binding_use_roles(then_body, out);
            collect_binding_use_roles(else_body, out);
        }
        HirStmt::For {
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
        HirStmt::Switch {
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
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_binding_use_roles_lvalue(lhs: &HirLValue, out: &mut HashMap<String, BindingUseRole>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => mark_address_role(ptr, out),
        HirLValue::Index { base, index, .. } => {
            mark_address_role(base, out);
            collect_binding_use_roles_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => mark_address_role(base, out),
    }
}

fn collect_binding_use_roles_expr(expr: &HirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { ty, expr } => {
            if matches!(ty, NirType::Int { .. } | NirType::Bool) {
                mark_scalar_role(expr, out);
            } else {
                collect_binding_use_roles_expr(expr, out);
            }
        }
        HirExpr::Unary { expr, .. } => mark_scalar_role(expr, out),
        HirExpr::Binary { op, lhs, rhs, .. } => {
            if role_scalar_op(*op) {
                mark_scalar_role(lhs, out);
                mark_scalar_role(rhs, out);
            } else {
                collect_binding_use_roles_expr(lhs, out);
                collect_binding_use_roles_expr(rhs, out);
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_binding_use_roles_expr(cond, out);
            collect_binding_use_roles_expr(then_expr, out);
            collect_binding_use_roles_expr(else_expr, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_binding_use_roles_expr(arg, out);
            }
        }
        HirExpr::Load { ptr, .. } => mark_address_role(ptr, out),
        HirExpr::PtrOffset { base, .. }
        | HirExpr::FieldAccess { base, .. }
        | HirExpr::AggregateCopy { src: base, .. } => mark_address_role(base, out),
        HirExpr::Index { base, index, .. } => {
            mark_address_role(base, out);
            collect_binding_use_roles_expr(index, out);
        }
    }
}

fn mark_address_role(expr: &HirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        HirExpr::Var(name) => {
            out.entry(name.clone()).or_default().address_use = true;
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => mark_address_role(expr, out),
        HirExpr::Index { base, .. } => mark_address_role(base, out),
        HirExpr::Binary { .. }
        | HirExpr::Select { .. }
        | HirExpr::Call { .. }
        | HirExpr::Load { .. }
        | HirExpr::Const(_, _)
        | HirExpr::AddressOfGlobal(_) => collect_binding_use_roles_expr(expr, out),
    }
}

fn mark_scalar_role(expr: &HirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        HirExpr::Var(name) => {
            out.entry(name.clone()).or_default().scalar_use = true;
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => mark_scalar_role(expr, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            mark_scalar_role(lhs, out);
            mark_scalar_role(rhs, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            mark_scalar_role(cond, out);
            mark_scalar_role(then_expr, out);
            mark_scalar_role(else_expr, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                mark_scalar_role(arg, out);
            }
        }
        HirExpr::Index { base, index, .. } => {
            mark_scalar_role(base, out);
            mark_scalar_role(index, out);
        }
        HirExpr::Load { ptr, .. } => mark_address_role(ptr, out),
        HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => {}
    }
}

fn role_scalar_op(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::Add
            | HirBinaryOp::Sub
            | HirBinaryOp::Mul
            | HirBinaryOp::Div
            | HirBinaryOp::Mod
            | HirBinaryOp::And
            | HirBinaryOp::Or
            | HirBinaryOp::Xor
            | HirBinaryOp::Shl
            | HirBinaryOp::Shr
            | HirBinaryOp::Sar
            | HirBinaryOp::Eq
            | HirBinaryOp::Ne
            | HirBinaryOp::Lt
            | HirBinaryOp::Le
            | HirBinaryOp::Gt
            | HirBinaryOp::Ge
            | HirBinaryOp::SLt
            | HirBinaryOp::SLe
            | HirBinaryOp::SGt
            | HirBinaryOp::SGe
    )
}

fn collect_constraints_stmt(
    stmt: &HirStmt,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            // Use-site on the lhs: Deref/Index require the base to be a pointer.
            collect_constraints_lvalue(lhs, out);
            collect_assignment_copy_constraints(lhs, rhs, known_binding_types, out);
            // Use-site on the rhs: look for Cast(T, Var(x)) → x: T.
            collect_constraints_cast_source(rhs, known_binding_types, out);
            // Recurse into rhs for nested uses.
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        HirStmt::Expr(expr) => {
            collect_constraints_expr(expr, return_type, known_binding_types, out);
        }
        HirStmt::Block(body) => {
            collect_constraints(body, return_type, known_binding_types, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(then_body, return_type, known_binding_types, out);
            collect_constraints(else_body, return_type, known_binding_types, out);
        }
        HirStmt::While { cond, body } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(body, return_type, known_binding_types, out);
        }
        HirStmt::DoWhile { body, cond } => {
            collect_constraints(body, return_type, known_binding_types, out);
            collect_constraints_expr(cond, return_type, known_binding_types, out);
        }
        HirStmt::For {
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
        HirStmt::Switch {
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
        HirStmt::Return(Some(expr)) => {
            // If the function's return type is already known and the expression
            // is a bare variable, constrain that variable to the return type.
            if *return_type != NirType::Unknown {
                if let HirExpr::Var(name) = expr {
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
    lhs: &HirLValue,
    rhs: &HirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match lhs {
        HirLValue::Var(lhs_name) => {
            // Reverse-propagate non-pointer types only. Propagating Ptr through
            // a simple copy is unsafe when a register is later reused for an
            // address value.
            if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
                if let HirExpr::Var(rhs_name) = rhs {
                    if !matches!(lhs_ty, NirType::Ptr(_)) {
                        out.entry(rhs_name.clone())
                            .or_default()
                            .push(copy_constraint_from_type(lhs_ty));
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

            if let HirExpr::Var(rhs_name) = rhs {
                if let Some(rhs_ty) = known_binding_types.get(rhs_name) {
                    out.entry(lhs_name.clone())
                        .or_default()
                        .push(copy_constraint_from_type(rhs_ty));
                }
            }

            if let HirExpr::AddressOfGlobal(_) = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let HirExpr::PtrOffset { .. } | HirExpr::FieldAccess { .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let HirExpr::Load { ty, .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }

            if let HirExpr::Cast {
                ty: NirType::Ptr(pointee),
                ..
            } = rhs
            {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        HirLValue::Deref { ty, .. } => {
            if let HirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
        HirLValue::Index { elem_ty, .. } => {
            if let HirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(elem_ty.clone()));
            }
        }
        HirLValue::FieldAccess { ty, .. } => {
            if let HirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
    }
}

fn collect_pointer_assignment_base_constraints(
    rhs: &HirExpr,
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
        HirExpr::Var(name)
            if matches!(known_binding_types.get(name), None | Some(NirType::Unknown)) =>
        {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        HirExpr::Var(_) => {}
        HirExpr::AddressOfGlobal(name) => {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        HirExpr::Cast { expr, .. } => {
            collect_pointer_assignment_base_constraints(expr, ptr_ty, known_binding_types, out);
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            // ptr + integer = pointer. Only promote a Var base when the other
            // side is a *numeric* offset (const / const arithmetic).
            // Do not treat an integer cast of a bare variable as an offset; it
            // can be an integerized pointer base.
            if expr_is_numeric_offset_with_types(rhs.as_ref(), known_binding_types) {
                if let HirExpr::Var(name) = strip_casts_unary(lhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
            if expr_is_numeric_offset_with_types(lhs.as_ref(), known_binding_types) {
                if let HirExpr::Var(name) = strip_casts_unary(rhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            ..
        } => {
            if let HirExpr::Var(name) = lhs.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        _ => {}
    }
}

fn strip_casts_unary(expr: &HirExpr) -> &HirExpr {
    let mut cur = expr;
    while let HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } = cur {
        cur = expr.as_ref();
    }
    cur
}

/// Integer offset in pointer arithmetic: const, index vars, scaled index.
/// An integer cast of a bare variable is not sufficient offset evidence; it can
/// be a pointer base forced through integer ALU.
fn expr_is_numeric_offset(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Const(_, _) => true,
        HirExpr::Var(_) => true,
        HirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            HirExpr::Const(_, _) => true,
            HirExpr::Binary { .. } => expr_is_numeric_offset(inner),
            // Bare var cast to int is ambiguous (often ptr-to-int for end calc).
            HirExpr::Var(_) => false,
            other => expr_is_numeric_offset(other),
        },
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => expr_is_numeric_offset(expr),
        HirExpr::Binary {
            op:
                HirBinaryOp::Add
                | HirBinaryOp::Sub
                | HirBinaryOp::Mul
                | HirBinaryOp::Shl
                | HirBinaryOp::Shr
                | HirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } => expr_is_numeric_offset(lhs) && expr_is_numeric_offset(rhs),
        _ => false,
    }
}

fn expr_is_numeric_offset_with_types(
    expr: &HirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    match expr {
        HirExpr::Var(name) => !matches!(known_binding_types.get(name), Some(NirType::Ptr(_))),
        HirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            HirExpr::Var(name) => matches!(
                known_binding_types.get(name),
                Some(NirType::Int { .. } | NirType::Bool)
            ),
            _ => expr_is_numeric_offset_with_types(inner, known_binding_types),
        },
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            expr_is_numeric_offset_with_types(expr, known_binding_types)
        }
        HirExpr::Binary {
            op:
                HirBinaryOp::Add
                | HirBinaryOp::Sub
                | HirBinaryOp::Mul
                | HirBinaryOp::Shl
                | HirBinaryOp::Shr
                | HirBinaryOp::Sar,
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

fn expr_is_pointer_offset_like(expr: &HirExpr) -> bool {
    expr_is_numeric_offset(expr)
}

fn copy_constraint_from_type(ty: &NirType) -> UseConstraint {
    match ty {
        NirType::Ptr(pointee) => UseConstraint::Ptr(pointee.as_ref().clone()),
        _ => UseConstraint::Exact(ty.clone()),
    }
}

fn collect_copy_alias_sources(stmts: &[HirStmt], out: &mut HashMap<String, HashSet<String>>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } => {
                let mut source = rhs;
                while let HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } = source {
                    source = expr.as_ref();
                }
                if let HirExpr::Var(source_name) = source {
                    out.entry(name.clone())
                        .or_default()
                        .insert(source_name.clone());
                }
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } => {
                collect_copy_alias_sources(body, out);
            }
            HirStmt::DoWhile { body, .. } => collect_copy_alias_sources(body, out),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_copy_alias_sources(then_body, out);
                collect_copy_alias_sources(else_body, out);
            }
            HirStmt::For {
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
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_copy_alias_sources(&case.body, out);
                }
                collect_copy_alias_sources(default, out);
            }
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::Return(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn propagate_logical_shift_constraints_through_aliases(
    stmts: &[HirStmt],
    constraints: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let mut aliases = HashMap::new();
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
    let mut seen = HashSet::new();
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
fn collect_constraints_lvalue(lhs: &HirLValue, out: &mut HashMap<String, Vec<UseConstraint>>) {
    match lhs {
        HirLValue::Deref { ptr, ty } => {
            // Storing through *ptr → ptr must be Ptr(ty).
            if let HirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
        }
        HirLValue::Index { base, elem_ty, .. } => {
            // base[idx] → base is Ptr(elem_ty).
            if let HirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
        }
        HirLValue::Var(_) => {}
        HirLValue::FieldAccess { base, ty, .. } => {
            if let HirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
        }
    }
}

/// Collect `Cast(T, Var(x))` → x: T constraints.
fn collect_constraints_cast_source(
    expr: &HirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    if let HirExpr::Cast { ty, expr: inner } = expr {
        if let HirExpr::Var(name) = inner.as_ref() {
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
            && let HirExpr::Binary { op, lhs, rhs, .. } = inner.as_ref()
            && matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub | HirBinaryOp::Mul)
        {
            collect_arithmetic_result_constraints(lhs, rhs, ty, known_binding_types, out);
        }
    }
}

/// Recurse into an expression and collect use-site constraints.
fn collect_constraints_expr(
    expr: &HirExpr,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            // Loading through *ptr → ptr is Ptr(ty).
            if let HirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
            // Recurse into the pointer expression itself.
            collect_constraints_expr(ptr, return_type, known_binding_types, out);
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            match op {
                // Signed comparison → operands are signed integers.  The
                // comparison expression itself is Bool, so operand width must
                // come from an actual operand or an existing binding type.
                HirBinaryOp::SLt | HirBinaryOp::SLe | HirBinaryOp::SGt | HirBinaryOp::SGe => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, true, out)
                }
                // Unsigned comparison → operands are unsigned integers.
                HirBinaryOp::Lt | HirBinaryOp::Le | HirBinaryOp::Gt | HirBinaryOp::Ge => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, false, out)
                }
                // Arithmetic right-shift: the left operand must be a signed integer.
                // `x >> k` where `>>` is Sar (arithmetic) means x is signed.
                HirBinaryOp::Sar => {
                    let bits = nir_type_bits(ty)
                        .or_else(|| expr_int_bits(lhs.as_ref(), known_binding_types))
                        .unwrap_or(32);
                    if let HirExpr::Var(name) = lhs.as_ref() {
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
                HirBinaryOp::Shr => {
                    let bits = nir_type_bits(ty)
                        .or_else(|| expr_int_bits(lhs.as_ref(), known_binding_types))
                        .unwrap_or(32);
                    if let HirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::LogicalShiftUnsigned { bits });
                    }
                }
                HirBinaryOp::Add
                | HirBinaryOp::Sub
                | HirBinaryOp::Mul
                | HirBinaryOp::Div
                | HirBinaryOp::Mod
                | HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Xor
                | HirBinaryOp::Shl => {
                    collect_arithmetic_result_constraints(lhs, rhs, ty, known_binding_types, out);
                }
                _ => {}
            }
            collect_constraints_expr(lhs, return_type, known_binding_types, out);
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        HirExpr::Unary { expr: inner, .. } => {
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        HirExpr::Cast { expr: inner, .. } => {
            collect_constraints_cast_source(expr, known_binding_types, out);
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        HirExpr::Call { target, args, .. } => {
            if let Some(name) = indirect_call_target_binding_name(target) {
                out.entry(name.to_owned())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }
            for arg in args {
                collect_constraints_expr(arg, return_type, known_binding_types, out);
            }
        }
        HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => {
            if let HirExpr::Var(base_name) = base.as_ref() {
                out.entry(base_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }
            collect_constraints_expr(base, return_type, known_binding_types, out);
        }
        HirExpr::AggregateCopy { src: base, .. } => {
            collect_constraints_expr(base, return_type, known_binding_types, out);
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            // base[index] → base is Ptr(elem_ty).
            if let HirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
            if let HirExpr::Var(name) = index.as_ref() {
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
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints_expr(then_expr, return_type, known_binding_types, out);
            collect_constraints_expr(else_expr, return_type, known_binding_types, out);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

fn collect_compare_constraints(
    lhs: &HirExpr,
    rhs: &HirExpr,
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

    if let (HirExpr::Var(name), Some(bits)) = (lhs, lhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
    if let (HirExpr::Var(name), Some(bits)) = (rhs, rhs_bits) {
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
    lhs: &HirExpr,
    rhs: &HirExpr,
    result_ty: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
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

fn collect_arithmetic_operand_constraint(
    expr: &HirExpr,
    result_bits: u32,
    signed: bool,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let HirExpr::Var(name) = expr else {
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

fn is_byte_expr(expr: &HirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).is_some_and(is_byte_int_type)
        }
        HirExpr::Const(value, ty) => is_byte_int_type(ty) || (0..=0xff).contains(value),
        HirExpr::Load { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. }
        | HirExpr::FieldAccess { ty, .. } => is_byte_int_type(ty),
        HirExpr::Cast { ty, expr } => {
            is_byte_int_type(ty) || is_byte_expr(expr, known_binding_types)
        }
        _ => false,
    }
}

fn is_byte_pointer_expr(expr: &HirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => known_binding_types
            .get(name)
            .is_some_and(is_byte_pointer_type),
        HirExpr::Cast { ty, expr } => {
            is_byte_pointer_type(ty) || is_byte_pointer_expr(expr, known_binding_types)
        }
        HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => {
            is_byte_pointer_expr(base, known_binding_types)
        }
        _ => false,
    }
}

fn expr_is_var(expr: &HirExpr, name: &str) -> bool {
    matches!(expr, HirExpr::Var(var_name) if var_name == name)
}

fn is_byte_accumulator_update(
    expr: &HirExpr,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    let HirExpr::Binary { op, lhs, rhs, .. } = expr else {
        return false;
    };
    matches!(
        op,
        HirBinaryOp::Add | HirBinaryOp::Sub | HirBinaryOp::Xor | HirBinaryOp::And | HirBinaryOp::Or
    ) && ((expr_is_var(lhs, name) && is_byte_expr(rhs, known_binding_types))
        || (expr_is_var(rhs, name) && is_byte_expr(lhs, known_binding_types)))
}

fn collect_byte_index_accumulator_evidence(
    stmts: &[HirStmt],
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    for stmt in stmts {
        collect_byte_index_accumulator_evidence_stmt(stmt, name, known_binding_types, evidence);
    }
}

fn collect_byte_index_accumulator_evidence_stmt(
    stmt: &HirStmt,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            let self_def = matches!(lhs, HirLValue::Var(lhs_name) if lhs_name == name);
            if self_def {
                evidence.def_count += 1;
                if is_byte_expr(rhs, known_binding_types) {
                    evidence.byte_seed_defs += 1;
                } else if is_byte_accumulator_update(rhs, name, known_binding_types) {
                    evidence.byte_update_defs += 1;
                } else {
                    evidence.disallowed_uses += 1;
                }
                if !is_byte_accumulator_update(rhs, name, known_binding_types) {
                    collect_byte_index_accumulator_evidence_expr(
                        rhs,
                        name,
                        known_binding_types,
                        evidence,
                    );
                }
            } else {
                collect_byte_index_accumulator_evidence_lvalue(
                    lhs,
                    name,
                    known_binding_types,
                    evidence,
                );
                collect_byte_index_accumulator_evidence_expr(
                    rhs,
                    name,
                    known_binding_types,
                    evidence,
                );
            }
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            collect_byte_index_accumulator_evidence_expr(expr, name, known_binding_types, evidence);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_byte_index_accumulator_evidence(body, name, known_binding_types, evidence);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_byte_index_accumulator_evidence_expr(cond, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence(then_body, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence(else_body, name, known_binding_types, evidence);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_byte_index_accumulator_evidence_stmt(
                    init,
                    name,
                    known_binding_types,
                    evidence,
                );
            }
            if let Some(cond) = cond {
                collect_byte_index_accumulator_evidence_expr(
                    cond,
                    name,
                    known_binding_types,
                    evidence,
                );
            }
            if let Some(update) = update {
                collect_byte_index_accumulator_evidence_stmt(
                    update,
                    name,
                    known_binding_types,
                    evidence,
                );
            }
            collect_byte_index_accumulator_evidence(body, name, known_binding_types, evidence);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_byte_index_accumulator_evidence_expr(expr, name, known_binding_types, evidence);
            for case in cases {
                collect_byte_index_accumulator_evidence(
                    &case.body,
                    name,
                    known_binding_types,
                    evidence,
                );
            }
            collect_byte_index_accumulator_evidence(default, name, known_binding_types, evidence);
        }
        HirStmt::VaStart { va_list, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                va_list,
                name,
                known_binding_types,
                evidence,
            );
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_byte_index_accumulator_evidence_lvalue(
    lhs: &HirLValue,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } | HirLValue::FieldAccess { base: ptr, .. } => {
            collect_byte_index_accumulator_evidence_expr(ptr, name, known_binding_types, evidence);
        }
        HirLValue::Index { base, index, .. } => {
            collect_byte_index_accumulator_evidence_expr(base, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence_expr(
                index,
                name,
                known_binding_types,
                evidence,
            );
        }
    }
}

fn collect_byte_index_accumulator_evidence_expr(
    expr: &HirExpr,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    match expr {
        HirExpr::Var(var_name) | HirExpr::AddressOfGlobal(var_name) if var_name == name => {
            evidence.disallowed_uses += 1;
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                inner,
                name,
                known_binding_types,
                evidence,
            );
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if expr_is_var(lhs, name) && is_byte_pointer_expr(rhs, known_binding_types) => {
            evidence.byte_pointer_offset_uses += 1;
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if expr_is_var(rhs, name) && is_byte_pointer_expr(lhs, known_binding_types) => {
            evidence.byte_pointer_offset_uses += 1;
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_byte_index_accumulator_evidence_expr(lhs, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence_expr(rhs, name, known_binding_types, evidence);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_byte_index_accumulator_evidence_expr(
                    arg,
                    name,
                    known_binding_types,
                    evidence,
                );
            }
        }
        HirExpr::Index { base, index, .. } => {
            collect_byte_index_accumulator_evidence_expr(base, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence_expr(
                index,
                name,
                known_binding_types,
                evidence,
            );
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_byte_index_accumulator_evidence_expr(cond, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence_expr(
                then_expr,
                name,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                else_expr,
                name,
                known_binding_types,
                evidence,
            );
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

fn narrow_byte_index_accumulators(func: &mut HirFunction) -> bool {
    let known_binding_types = collect_known_binding_types(func);
    let mut changed = false;
    for binding in &mut func.locals {
        if binding.surface_type_name.is_some() {
            continue;
        }
        if !matches!(
            binding.origin,
            Some(NirBindingOrigin::Temp | NirBindingOrigin::TempPreserved)
        ) {
            continue;
        }
        let NirType::Int { bits, .. } = binding.ty else {
            continue;
        };
        if bits <= 8 {
            continue;
        }

        let mut evidence = ByteIndexAccumulatorEvidence::default();
        collect_byte_index_accumulator_evidence(
            &func.body,
            &binding.name,
            &known_binding_types,
            &mut evidence,
        );
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

fn expr_int_bits(expr: &HirExpr, known_binding_types: &HashMap<String, NirType>) -> Option<u32> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).and_then(nir_type_bits)
        }
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. }
        | HirExpr::Cast { ty, .. }
        | HirExpr::Select { ty, .. }
        | HirExpr::FieldAccess { ty, .. } => nir_type_bits(ty),
        HirExpr::Binary { ty, .. } => nir_type_bits(ty),
        HirExpr::PtrOffset { .. } | HirExpr::AggregateCopy { .. } => None,
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

fn collect_known_binding_types(func: &HirFunction) -> HashMap<String, NirType> {
    let mut known = HashMap::new();
    for binding in func.locals.iter().chain(func.params.iter()) {
        if binding.ty != NirType::Unknown {
            known.insert(binding.name.clone(), binding.ty.clone());
        }
    }
    known
}

fn return_expr_type(
    expr: &HirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).cloned()
        }
        other => {
            let ty = expr_type(other);
            (ty != NirType::Unknown).then_some(ty)
        }
    }
}

fn collect_value_return_types(
    stmts: &[HirStmt],
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
    stmt: &HirStmt,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        HirStmt::Return(Some(expr)) => {
            if let Some(ty) = return_expr_type(expr, known_binding_types) {
                out.push(ty);
            }
            1
        }
        HirStmt::Return(None) => 0,
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            collect_value_return_types(stmts, known_binding_types, out)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_value_return_types(then_body, known_binding_types, out)
                + collect_value_return_types(else_body, known_binding_types, out)
        }
        HirStmt::Switch { cases, default, .. } => {
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

fn promote_return_signedness_from_returns(func: &mut HirFunction) -> bool {
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

fn promote_unknown_call_return_type(func: &mut HirFunction) -> bool {
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

fn native_unsigned_word_type(func: &HirFunction) -> NirType {
    NirType::Int {
        bits: if func.is_64bit { 64 } else { 32 },
        signed: false,
    }
}

fn collect_unknown_call_returns(
    stmts: &[HirStmt],
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    for stmt in stmts {
        collect_unknown_call_returns_stmt(stmt, value_return_count, unknown_call_return_count);
    }
}

fn collect_unknown_call_returns_stmt(
    stmt: &HirStmt,
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    match stmt {
        HirStmt::Return(Some(expr)) => {
            *value_return_count += 1;
            if is_unknown_call_result(expr) {
                *unknown_call_return_count += 1;
            }
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            collect_unknown_call_returns(stmts, value_return_count, unknown_call_return_count);
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_unknown_call_returns(then_body, value_return_count, unknown_call_return_count);
            collect_unknown_call_returns(else_body, value_return_count, unknown_call_return_count);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_unknown_call_returns(
                    &case.body,
                    value_return_count,
                    unknown_call_return_count,
                );
            }
            collect_unknown_call_returns(default, value_return_count, unknown_call_return_count);
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn is_unknown_call_result(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Call { ty, .. } => *ty == NirType::Unknown,
        HirExpr::Cast { expr, ty } if *ty == NirType::Unknown => is_unknown_call_result(expr),
        _ => false,
    }
}

fn count_var_uses_expr(expr: &HirExpr, out: &mut HashMap<String, usize>) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => count_var_uses_expr(expr, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_var_uses_expr(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                count_var_uses_expr(arg, out);
            }
        }
        HirExpr::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        HirExpr::Select {
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

fn count_var_uses_lvalue(lhs: &HirLValue, out: &mut HashMap<String, usize>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => count_var_uses_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => {
            count_var_uses_expr(base, out);
        }
    }
}

fn count_var_uses_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_var_uses_lvalue(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => {
            count_var_uses_expr(va_list, out);
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. } => count_var_uses_stmts(stmts, out),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses_expr(cond, out);
            count_var_uses_stmts(then_body, out);
            count_var_uses_stmts(else_body, out);
        }
        HirStmt::For {
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
        HirStmt::Switch {
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
        HirStmt::Return(Some(expr)) => count_var_uses_expr(expr, out),
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn count_var_uses_stmts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_var_uses_stmt(stmt, out);
    }
}

fn store_value_var_name(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name.as_str()),
        HirExpr::Cast { expr, .. } => store_value_var_name(expr),
        _ => None,
    }
}

fn count_store_value_uses_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Deref { .. } | HirLValue::Index { .. },
            rhs,
        } => {
            if let Some(name) = store_value_var_name(rhs) {
                *out.entry(name.to_owned()).or_default() += 1;
            }
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. } => count_store_value_uses_stmts(stmts, out),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            count_store_value_uses_stmts(then_body, out);
            count_store_value_uses_stmts(else_body, out);
        }
        HirStmt::For {
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
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_store_value_uses_stmts(&case.body, out);
            }
            count_store_value_uses_stmts(default, out);
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn count_store_value_uses_stmts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_store_value_uses_stmt(stmt, out);
    }
}

fn promote_store_value_only_unsigned_params(func: &mut HirFunction) -> bool {
    let mut all_uses = HashMap::new();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut store_value_uses = HashMap::new();
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

fn wrapping_narrow_op(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::Add
            | HirBinaryOp::Sub
            | HirBinaryOp::Mul
            | HirBinaryOp::And
            | HirBinaryOp::Or
            | HirBinaryOp::Xor
    )
}

fn collect_wrapping_narrow_return_vars(
    expr: &HirExpr,
    context_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        HirExpr::Cast { ty, expr } => {
            let bits = nir_type_bits(ty).unwrap_or(context_bits).min(context_bits);
            collect_wrapping_narrow_return_vars(expr, bits, out);
        }
        HirExpr::Unary {
            op: HirUnaryOp::Neg,
            expr,
            ..
        } => collect_wrapping_narrow_return_vars(expr, context_bits, out),
        HirExpr::Binary { op, lhs, rhs, .. } if wrapping_narrow_op(*op) => {
            collect_wrapping_narrow_return_vars(lhs, context_bits, out);
            collect_wrapping_narrow_return_vars(rhs, context_bits, out);
        }
        HirExpr::Const(_, _)
        | HirExpr::Unary { .. }
        | HirExpr::Binary { .. }
        | HirExpr::Call { .. }
        | HirExpr::Load { .. }
        | HirExpr::PtrOffset { .. }
        | HirExpr::Index { .. }
        | HirExpr::Select { .. }
        | HirExpr::FieldAccess { .. }
        | HirExpr::AggregateCopy { .. } => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmt(
    stmt: &HirStmt,
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match stmt {
        HirStmt::Return(Some(expr)) => collect_wrapping_narrow_return_vars(expr, return_bits, out),
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            collect_wrapping_narrow_return_vars_stmts(stmts, return_bits, out)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_wrapping_narrow_return_vars_stmts(then_body, return_bits, out);
            collect_wrapping_narrow_return_vars_stmts(else_body, return_bits, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_wrapping_narrow_return_vars_stmts(&case.body, return_bits, out);
            }
            collect_wrapping_narrow_return_vars_stmts(default, return_bits, out);
        }
        _ => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmts(
    stmts: &[HirStmt],
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    for stmt in stmts {
        collect_wrapping_narrow_return_vars_stmt(stmt, return_bits, out);
    }
}

fn narrow_integer_params_from_wrapping_return_uses(func: &mut HirFunction) -> bool {
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

    let mut all_uses = HashMap::new();
    count_var_uses_stmts(&func.body, &mut all_uses);
    let mut constrained_uses = HashMap::new();
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
fn merge_constraint(binding: &mut NirBinding, constraint: &UseConstraint) -> bool {
    if binding.surface_type_name.is_some() {
        return false;
    }
    match (&binding.ty, constraint) {
        // Already has a non-Unknown type — don't overwrite.
        (NirType::Ptr(_), _) => false,
        (NirType::Float { .. }, _) => false,
        (NirType::Aggregate { .. }, _) => false,
        (NirType::Bool, _) => false,

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
    func: &mut HirFunction,
    constraints: &HashMap<String, Vec<UseConstraint>>,
) -> bool {
    let mut roles = HashMap::<String, BindingUseRole>::new();
    collect_binding_use_roles(&func.body, &mut roles);
    let pointer_compare_peers = super::type_infer::pointer_compare_peer_promotions(func);
    let transitive_address_locals = super::type_infer::transitive_address_pointer_locals(func);
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
        if scalar_evidence
            && !address_use
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
    binding: &NirBinding,
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
pub(crate) fn apply_use_driven_type_infer_pass(func: &mut HirFunction) -> bool {
    let before = type_state_signature(func);
    let dependencies = DefinitionDependencyMap::build(&func.body);
    // Iterate to convergence (alias chains may require multiple rounds).
    for _ in 0..4 {
        let mut constraints: HashMap<String, Vec<UseConstraint>> = HashMap::new();
        let mut roles = HashMap::<String, BindingUseRole>::new();
        let known_binding_types = collect_known_binding_types(func);
        let pointer_roots = func
            .params
            .iter()
            .filter_map(|binding| {
                matches!(binding.ty, NirType::Ptr(_)).then_some(binding.name.clone())
            })
            .collect();
        let address_contributors = dependencies.address_contributors(&func.body, &pointer_roots);
        collect_binding_use_roles(&func.body, &mut roles);
        collect_constraints(
            &func.body,
            &func.return_type,
            &known_binding_types,
            &mut constraints,
        );
        propagate_logical_shift_constraints_through_aliases(&func.body, &mut constraints);

        let mut round_changed = false;
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
        round_changed |= restore_scalar_only_pointer_locals(func, &constraints);
        round_changed |= narrow_byte_index_accumulators(func);
        if !round_changed {
            break;
        }
    }
    type_state_signature(func) != before
}

#[cfg(test)]
mod tests {
    use super::super::super::*;

    fn make_binding(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_typed_binding(name: &str, ty: NirType, origin: NirBindingOrigin) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<NirBinding>, body: Vec<HirStmt>, return_type: NirType) -> HirFunction {
        HirFunction {
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

    /// Load { ptr: Var("p"), ty: uint32 } → p: Ptr(uint32)
    #[test]
    fn infers_ptr_from_load() {
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("x".to_owned()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Var("p".to_owned())),
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
            HirStmt::Assign {
                lhs: HirLValue::Var("idx".to_owned()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("p".to_owned())),
                    ty: u8_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("idx".to_owned()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("idx".to_owned())),
                    rhs: Box::new(HirExpr::Load {
                        ptr: Box::new(HirExpr::Var("q".to_owned())),
                        ty: u8_ty.clone(),
                    }),
                    ty: u32_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("cursor".to_owned()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("p".to_owned())),
                    rhs: Box::new(HirExpr::Var("idx".to_owned())),
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
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("cursor".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Var("idx".to_owned())),
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
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("end".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("base".to_owned())),
                rhs: Box::new(HirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(HirExpr::Var("count".to_owned())),
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
            HirStmt::Assign {
                lhs: HirLValue::Var("alias".to_owned()),
                rhs: HirExpr::Var("input".to_owned()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("cursor".to_owned()),
                rhs: HirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(HirExpr::Var("alias".to_owned())),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("value".to_owned()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("cursor".to_owned())),
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
            HirStmt::Assign {
                lhs: HirLValue::Var("p".to_owned()),
                rhs: HirExpr::Cast {
                    ty: byte_ptr_ty.clone(),
                    expr: Box::new(HirExpr::Var("x".to_owned())),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("sum".to_owned()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("x".to_owned())),
                    rhs: Box::new(HirExpr::Const(1, u64_ty.clone())),
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
        let body = vec![HirStmt::Return(Some(HirExpr::Var("acc".to_owned())))];
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
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
            rhs: HirExpr::Const(
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
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![],
            else_body: vec![],
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
        let body = vec![HirStmt::Return(Some(HirExpr::Call {
            target: "param_1".to_owned(),
            args: vec![
                HirExpr::Var("param_2".to_owned()),
                HirExpr::Var("param_3".to_owned()),
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
        let body = vec![HirStmt::Return(Some(HirExpr::Call {
            target: "((code *)param_1)".to_owned(),
            args: vec![
                HirExpr::Var("param_2".to_owned()),
                HirExpr::Var("param_3".to_owned()),
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
        let body = vec![HirStmt::If {
            cond: HirExpr::Var("flag".to_owned()),
            then_body: vec![HirStmt::Return(Some(HirExpr::Call {
                target: "param_1".to_owned(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }))],
            else_body: vec![HirStmt::Return(Some(HirExpr::Var("fallback".to_owned())))],
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
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Var("b".to_owned())),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Return(Some(HirExpr::Var("b".to_owned())))],
            else_body: vec![HirStmt::Return(Some(HirExpr::Var("a".to_owned())))],
        }];
        let mut func = HirFunction {
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
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("x".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: Box::new(HirExpr::Var("x".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: i32_ty.clone(),
            },
        }];
        let mut func = HirFunction {
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
            HirStmt::Assign {
                lhs: HirLValue::Var("shifted".into()),
                rhs: HirExpr::Var("count".into()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("shifted".into()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Shr,
                    lhs: Box::new(HirExpr::Var("shifted".into())),
                    rhs: Box::new(HirExpr::Const(1, u64_ty.clone())),
                    ty: i64_ty.clone(),
                },
            },
        ];
        let mut func = HirFunction {
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
        let body = vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("a".to_owned())),
            rhs: Box::new(HirExpr::Var("b".to_owned())),
            ty: i32_ty.clone(),
        }))];
        let mut func = HirFunction {
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
        let body = vec![HirStmt::Return(Some(HirExpr::Cast {
            ty: i32_ty.clone(),
            expr: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Var("b".to_owned())),
                ty: u32_ty.clone(),
            }),
        }))];
        let mut func = HirFunction {
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
        let mut func = HirFunction {
            name: "add32".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding("param_1", u64_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("param_2", u64_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: u32_ty.clone(),
            surface_return_type_name: None,
            body: vec![HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                rhs: Box::new(HirExpr::Var("param_2".to_owned())),
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
        let mut func = HirFunction {
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
                HirStmt::Expr(HirExpr::Call {
                    target: "observe64".to_owned(),
                    args: vec![HirExpr::Var("param_1".to_owned())],
                    ty: NirType::Unknown,
                }),
                HirStmt::Return(Some(HirExpr::Var("param_1".to_owned()))),
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
        let mut func = HirFunction {
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
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("param_1".to_owned())),
                    ty: u32_ty.clone(),
                },
                rhs: HirExpr::Var("param_2".to_owned()),
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
        let mut func = HirFunction {
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
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::Lt,
                        lhs: Box::new(HirExpr::Var("param_2".to_owned())),
                        rhs: Box::new(HirExpr::Const(10, u32_ty.clone())),
                        ty: NirType::Bool,
                    },
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("param_1".to_owned())),
                        ty: u32_ty.clone(),
                    },
                    rhs: HirExpr::Var("param_2".to_owned()),
                },
            ],
            ..Default::default()
        };

        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[1].ty, u32_ty);
    }

    #[test]
    fn signed_compare_without_width_evidence_does_not_invent_type() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Var("b".to_owned())),
                ty: NirType::Bool,
            },
            then_body: vec![],
            else_body: vec![],
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
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs: Box::new(HirExpr::Var("a".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 16,
                        signed: true,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![],
            else_body: vec![],
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
        let body = vec![HirStmt::Return(Some(HirExpr::Var("r".to_owned())))];
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
            HirStmt::Assign {
                lhs: HirLValue::Var("p".to_owned()),
                rhs: HirExpr::Var("param_1".to_owned()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: HirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = HirFunction {
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
            HirStmt::Assign {
                lhs: HirLValue::Var("p".to_owned()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Mul,
                        lhs: Box::new(HirExpr::Var("idx".to_owned())),
                        rhs: Box::new(HirExpr::Const(4, u64_ty.clone())),
                        ty: u64_ty.clone(),
                    }),
                    ty: u64_ty,
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: HirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = HirFunction {
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
}
