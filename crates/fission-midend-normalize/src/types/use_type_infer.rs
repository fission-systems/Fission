use super::super::analysis::defuse::DefinitionDependencyMap;
/// Use-driven backward type propagation pass.
///
/// `apply_type_inference_pass` (type_infer.rs) propagates types forward from
/// *definition* sites: if `x = (int32)...` then `x.ty = Int32`.  It cannot
/// infer types from *use* sites because `expr_type(DirExpr::Var(_)) = Unknown`.
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
/// 2. Merge all collected constraints into `DirBinding.ty` for locals and
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

fn type_state_signature(func: &DirFunction) -> TypeStateSignature {
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
    stmts: &[DirStmt],
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    for stmt in stmts {
        collect_constraints_stmt(stmt, return_type, known_binding_types, out);
    }
}

fn collect_binding_use_roles(stmts: &[DirStmt], out: &mut HashMap<String, BindingUseRole>) {
    for stmt in stmts {
        collect_binding_use_roles_stmt(stmt, out);
    }
}

fn collect_binding_use_roles_stmt(stmt: &DirStmt, out: &mut HashMap<String, BindingUseRole>) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            collect_binding_use_roles_lvalue(lhs, out);
            collect_binding_use_roles_expr(rhs, out);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            collect_binding_use_roles_expr(expr, out);
        }
        DirStmt::VaStart { va_list, .. } => collect_binding_use_roles_expr(va_list, out),
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            collect_binding_use_roles(body, out);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_binding_use_roles_expr(cond, out);
            collect_binding_use_roles(then_body, out);
            collect_binding_use_roles(else_body, out);
        }
        DirStmt::For {
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
        DirStmt::Switch {
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
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn collect_binding_use_roles_lvalue(lhs: &DirLValue, out: &mut HashMap<String, BindingUseRole>) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => mark_address_role(ptr, out),
        DirLValue::Index { base, index, .. } => {
            mark_address_role(base, out);
            collect_binding_use_roles_expr(index, out);
        }
        DirLValue::FieldAccess { base, .. } => mark_address_role(base, out),
    }
}

fn collect_binding_use_roles_expr(expr: &DirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
        DirExpr::Cast { ty, expr } => {
            if matches!(ty, NirType::Int { .. } | NirType::Bool) {
                mark_scalar_role(expr, out);
            } else {
                collect_binding_use_roles_expr(expr, out);
            }
        }
        DirExpr::Unary { expr, .. } => mark_scalar_role(expr, out),
        DirExpr::Binary { op, lhs, rhs, .. } => {
            if role_scalar_op(*op) {
                mark_scalar_role(lhs, out);
                mark_scalar_role(rhs, out);
            } else {
                collect_binding_use_roles_expr(lhs, out);
                collect_binding_use_roles_expr(rhs, out);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_binding_use_roles_expr(cond, out);
            collect_binding_use_roles_expr(then_expr, out);
            collect_binding_use_roles_expr(else_expr, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_binding_use_roles_expr(arg, out);
            }
        }
        DirExpr::Load { ptr, .. } => mark_address_role(ptr, out),
        DirExpr::PtrOffset { base, .. }
        | DirExpr::FieldAccess { base, .. }
        | DirExpr::AggregateCopy { src: base, .. } => mark_address_role(base, out),
        DirExpr::Index { base, index, .. } => {
            mark_address_role(base, out);
            collect_binding_use_roles_expr(index, out);
        }
    }
}

fn mark_address_role(expr: &DirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        DirExpr::Var(name) => {
            out.entry(name.clone()).or_default().address_use = true;
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => mark_address_role(expr, out),
        DirExpr::Index { base, .. } => mark_address_role(base, out),
        DirExpr::Binary { .. }
        | DirExpr::Select { .. }
        | DirExpr::Call { .. }
        | DirExpr::Load { .. }
        | DirExpr::Const(_, _)
        | DirExpr::AddressOfGlobal(_) => collect_binding_use_roles_expr(expr, out),
    }
}

fn mark_scalar_role(expr: &DirExpr, out: &mut HashMap<String, BindingUseRole>) {
    match expr {
        DirExpr::Var(name) => {
            out.entry(name.clone()).or_default().scalar_use = true;
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => mark_scalar_role(expr, out),
        DirExpr::Binary { lhs, rhs, .. } => {
            mark_scalar_role(lhs, out);
            mark_scalar_role(rhs, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            mark_scalar_role(cond, out);
            mark_scalar_role(then_expr, out);
            mark_scalar_role(else_expr, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                mark_scalar_role(arg, out);
            }
        }
        DirExpr::Index { base, index, .. } => {
            mark_scalar_role(base, out);
            mark_scalar_role(index, out);
        }
        DirExpr::Load { ptr, .. } => mark_address_role(ptr, out),
        DirExpr::Const(_, _) | DirExpr::AddressOfGlobal(_) => {}
    }
}

fn role_scalar_op(op: DirBinaryOp) -> bool {
    matches!(
        op,
        DirBinaryOp::Add
            | DirBinaryOp::Sub
            | DirBinaryOp::Mul
            | DirBinaryOp::Div
            | DirBinaryOp::Mod
            | DirBinaryOp::And
            | DirBinaryOp::Or
            | DirBinaryOp::Xor
            | DirBinaryOp::Shl
            | DirBinaryOp::Shr
            | DirBinaryOp::Sar
            | DirBinaryOp::Eq
            | DirBinaryOp::Ne
            | DirBinaryOp::Lt
            | DirBinaryOp::Le
            | DirBinaryOp::Gt
            | DirBinaryOp::Ge
            | DirBinaryOp::SLt
            | DirBinaryOp::SLe
            | DirBinaryOp::SGt
            | DirBinaryOp::SGe
    )
}

fn collect_constraints_stmt(
    stmt: &DirStmt,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            // Use-site on the lhs: Deref/Index require the base to be a pointer.
            collect_constraints_lvalue(lhs, out);
            collect_assignment_copy_constraints(lhs, rhs, known_binding_types, out);
            // Use-site on the rhs: look for Cast(T, Var(x)) → x: T.
            collect_constraints_cast_source(rhs, known_binding_types, out);
            // Recurse into rhs for nested uses.
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        DirStmt::Expr(expr) => {
            collect_constraints_expr(expr, return_type, known_binding_types, out);
        }
        DirStmt::Block(body) => {
            collect_constraints(body, return_type, known_binding_types, out);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(then_body, return_type, known_binding_types, out);
            collect_constraints(else_body, return_type, known_binding_types, out);
        }
        DirStmt::While { cond, body } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints(body, return_type, known_binding_types, out);
        }
        DirStmt::DoWhile { body, cond } => {
            collect_constraints(body, return_type, known_binding_types, out);
            collect_constraints_expr(cond, return_type, known_binding_types, out);
        }
        DirStmt::For {
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
        DirStmt::Switch {
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
        DirStmt::Return(Some(expr)) => {
            // If the function's return type is already known and the expression
            // is a bare variable, constrain that variable to the return type.
            if *return_type != NirType::Unknown {
                if let DirExpr::Var(name) = expr {
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
    lhs: &DirLValue,
    rhs: &DirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match lhs {
        DirLValue::Var(lhs_name) => {
            // Reverse-propagate non-pointer types only. Propagating Ptr through
            // a simple copy is unsafe when a register is later reused for an
            // address value.
            if let Some(lhs_ty) = known_binding_types.get(lhs_name) {
                if let DirExpr::Var(rhs_name) = rhs {
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

            if let DirExpr::Var(rhs_name) = rhs {
                if let Some(rhs_ty) = known_binding_types.get(rhs_name) {
                    out.entry(lhs_name.clone())
                        .or_default()
                        .push(copy_constraint_from_type(rhs_ty));
                }
            }

            if let DirExpr::AddressOfGlobal(_) = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let DirExpr::PtrOffset { .. } | DirExpr::FieldAccess { .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }

            if let DirExpr::Load { ty, .. } = rhs {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }

            if let DirExpr::Cast {
                ty: NirType::Ptr(pointee),
                ..
            } = rhs
            {
                out.entry(lhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        DirLValue::Deref { ty, .. } => {
            if let DirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
        DirLValue::Index { elem_ty, .. } => {
            if let DirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(elem_ty.clone()));
            }
        }
        DirLValue::FieldAccess { ty, .. } => {
            if let DirExpr::Var(rhs_name) = rhs {
                out.entry(rhs_name.clone())
                    .or_default()
                    .push(UseConstraint::Exact(ty.clone()));
            }
        }
    }
}

fn collect_pointer_assignment_base_constraints(
    rhs: &DirExpr,
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
        DirExpr::Var(name)
            if matches!(known_binding_types.get(name), None | Some(NirType::Unknown)) =>
        {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        DirExpr::Var(_) => {}
        DirExpr::AddressOfGlobal(name) => {
            out.entry(name.clone())
                .or_default()
                .push(UseConstraint::Ptr(pointee.as_ref().clone()));
        }
        DirExpr::Cast { expr, .. } => {
            collect_pointer_assignment_base_constraints(expr, ptr_ty, known_binding_types, out);
        }
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            // ptr + integer = pointer. Only promote a Var base when the other
            // side is a *numeric* offset (const / const arithmetic).
            // Do not treat an integer cast of a bare variable as an offset; it
            // can be an integerized pointer base.
            if expr_is_numeric_offset_with_types(rhs.as_ref(), known_binding_types) {
                if let DirExpr::Var(name) = strip_casts_unary(lhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
            if expr_is_numeric_offset_with_types(lhs.as_ref(), known_binding_types) {
                if let DirExpr::Var(name) = strip_casts_unary(rhs.as_ref()) {
                    out.entry(name.clone())
                        .or_default()
                        .push(UseConstraint::Ptr(pointee.as_ref().clone()));
                }
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs,
            ..
        } => {
            if let DirExpr::Var(name) = lhs.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(pointee.as_ref().clone()));
            }
        }
        _ => {}
    }
}

fn strip_casts_unary(expr: &DirExpr) -> &DirExpr {
    let mut cur = expr;
    while let DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } = cur {
        cur = expr.as_ref();
    }
    cur
}

/// Integer offset in pointer arithmetic: const, index vars, scaled index.
/// An integer cast of a bare variable is not sufficient offset evidence; it can
/// be a pointer base forced through integer ALU.
fn expr_is_numeric_offset(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Const(_, _) => true,
        DirExpr::Var(_) => true,
        DirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            DirExpr::Const(_, _) => true,
            DirExpr::Binary { .. } => expr_is_numeric_offset(inner),
            // Bare var cast to int is ambiguous (often ptr-to-int for end calc).
            DirExpr::Var(_) => false,
            other => expr_is_numeric_offset(other),
        },
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => expr_is_numeric_offset(expr),
        DirExpr::Binary {
            op:
                DirBinaryOp::Add
                | DirBinaryOp::Sub
                | DirBinaryOp::Mul
                | DirBinaryOp::Shl
                | DirBinaryOp::Shr
                | DirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } => expr_is_numeric_offset(lhs) && expr_is_numeric_offset(rhs),
        _ => false,
    }
}

fn expr_is_numeric_offset_with_types(
    expr: &DirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    match expr {
        DirExpr::Var(name) => !matches!(known_binding_types.get(name), Some(NirType::Ptr(_))),
        DirExpr::Cast {
            ty: NirType::Int { .. },
            expr: inner,
        } => match inner.as_ref() {
            DirExpr::Var(name) => matches!(
                known_binding_types.get(name),
                Some(NirType::Int { .. } | NirType::Bool)
            ),
            _ => expr_is_numeric_offset_with_types(inner, known_binding_types),
        },
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            expr_is_numeric_offset_with_types(expr, known_binding_types)
        }
        DirExpr::Binary {
            op:
                DirBinaryOp::Add
                | DirBinaryOp::Sub
                | DirBinaryOp::Mul
                | DirBinaryOp::Shl
                | DirBinaryOp::Shr
                | DirBinaryOp::Sar,
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

fn expr_is_pointer_offset_like(expr: &DirExpr) -> bool {
    expr_is_numeric_offset(expr)
}

fn copy_constraint_from_type(ty: &NirType) -> UseConstraint {
    match ty {
        NirType::Ptr(pointee) => UseConstraint::Ptr(pointee.as_ref().clone()),
        _ => UseConstraint::Exact(ty.clone()),
    }
}

fn collect_copy_alias_sources(stmts: &[DirStmt], out: &mut HashMap<String, HashSet<String>>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } => {
                let mut source = rhs;
                while let DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } = source {
                    source = expr.as_ref();
                }
                if let DirExpr::Var(source_name) = source {
                    out.entry(name.clone())
                        .or_default()
                        .insert(source_name.clone());
                }
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } => {
                collect_copy_alias_sources(body, out);
            }
            DirStmt::DoWhile { body, .. } => collect_copy_alias_sources(body, out),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_copy_alias_sources(then_body, out);
                collect_copy_alias_sources(else_body, out);
            }
            DirStmt::For {
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
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_copy_alias_sources(&case.body, out);
                }
                collect_copy_alias_sources(default, out);
            }
            DirStmt::Assign { .. }
            | DirStmt::Expr(_)
            | DirStmt::Return(_)
            | DirStmt::VaStart { .. }
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn propagate_logical_shift_constraints_through_aliases(
    stmts: &[DirStmt],
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
fn collect_constraints_lvalue(lhs: &DirLValue, out: &mut HashMap<String, Vec<UseConstraint>>) {
    match lhs {
        DirLValue::Deref { ptr, ty } => {
            // Storing through *ptr → ptr must be Ptr(ty).
            if let DirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
        }
        DirLValue::Index { base, elem_ty, .. } => {
            // base[idx] → base is Ptr(elem_ty).
            if let DirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
        }
        DirLValue::Var(_) => {}
        DirLValue::FieldAccess { base, ty, .. } => {
            if let DirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
        }
    }
}

/// Collect `Cast(T, Var(x))` → x: T constraints.
fn collect_constraints_cast_source(
    expr: &DirExpr,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    if let DirExpr::Cast { ty, expr: inner } = expr {
        if let DirExpr::Var(name) = inner.as_ref() {
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
            && let DirExpr::Binary { op, lhs, rhs, .. } = inner.as_ref()
            && matches!(op, DirBinaryOp::Add | DirBinaryOp::Sub | DirBinaryOp::Mul)
        {
            collect_arithmetic_result_constraints(lhs, rhs, ty, known_binding_types, out);
        }
    }
}

/// Recurse into an expression and collect use-site constraints.
fn collect_constraints_expr(
    expr: &DirExpr,
    return_type: &NirType,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    match expr {
        DirExpr::Load { ptr, ty } => {
            // Loading through *ptr → ptr is Ptr(ty).
            if let DirExpr::Var(name) = ptr.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(ty.clone()));
            }
            // Recurse into the pointer expression itself.
            collect_constraints_expr(ptr, return_type, known_binding_types, out);
        }
        DirExpr::Binary { op, lhs, rhs, ty } => {
            match op {
                // Signed comparison → operands are signed integers.  The
                // comparison expression itself is Bool, so operand width must
                // come from an actual operand or an existing binding type.
                DirBinaryOp::SLt | DirBinaryOp::SLe | DirBinaryOp::SGt | DirBinaryOp::SGe => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, true, out)
                }
                // Unsigned comparison → operands are unsigned integers.
                DirBinaryOp::Lt | DirBinaryOp::Le | DirBinaryOp::Gt | DirBinaryOp::Ge => {
                    collect_compare_constraints(lhs, rhs, ty, known_binding_types, false, out)
                }
                // Arithmetic right-shift: the left operand must be a signed integer.
                // `x >> k` where `>>` is Sar (arithmetic) means x is signed.
                DirBinaryOp::Sar => {
                    let bits = nir_type_bits(ty)
                        .or_else(|| expr_int_bits(lhs.as_ref(), known_binding_types))
                        .unwrap_or(32);
                    if let DirExpr::Var(name) = lhs.as_ref() {
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
                DirBinaryOp::Shr => {
                    let bits = nir_type_bits(ty)
                        .or_else(|| expr_int_bits(lhs.as_ref(), known_binding_types))
                        .unwrap_or(32);
                    if let DirExpr::Var(name) = lhs.as_ref() {
                        out.entry(name.clone())
                            .or_default()
                            .push(UseConstraint::LogicalShiftUnsigned { bits });
                    }
                }
                DirBinaryOp::Add
                | DirBinaryOp::Sub
                | DirBinaryOp::Mul
                | DirBinaryOp::Div
                | DirBinaryOp::Mod
                | DirBinaryOp::And
                | DirBinaryOp::Or
                | DirBinaryOp::Xor
                | DirBinaryOp::Shl => {
                    collect_arithmetic_result_constraints(lhs, rhs, ty, known_binding_types, out);
                }
                _ => {}
            }
            collect_constraints_expr(lhs, return_type, known_binding_types, out);
            collect_constraints_expr(rhs, return_type, known_binding_types, out);
        }
        DirExpr::Unary { expr: inner, .. } => {
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        DirExpr::Cast { expr: inner, .. } => {
            collect_constraints_cast_source(expr, known_binding_types, out);
            collect_constraints_expr(inner, return_type, known_binding_types, out);
        }
        DirExpr::Call { target, args, .. } => {
            if let Some(name) = indirect_call_target_binding_name(target) {
                out.entry(name.to_owned())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }
            for arg in args {
                collect_constraints_expr(arg, return_type, known_binding_types, out);
            }
        }
        DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => {
            if let DirExpr::Var(base_name) = base.as_ref() {
                out.entry(base_name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(NirType::Unknown));
            }
            collect_constraints_expr(base, return_type, known_binding_types, out);
        }
        DirExpr::AggregateCopy { src: base, .. } => {
            collect_constraints_expr(base, return_type, known_binding_types, out);
        }
        DirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            // base[index] → base is Ptr(elem_ty).
            if let DirExpr::Var(name) = base.as_ref() {
                out.entry(name.clone())
                    .or_default()
                    .push(UseConstraint::Ptr(elem_ty.clone()));
            }
            if let DirExpr::Var(name) = index.as_ref() {
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
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_constraints_expr(cond, return_type, known_binding_types, out);
            collect_constraints_expr(then_expr, return_type, known_binding_types, out);
            collect_constraints_expr(else_expr, return_type, known_binding_types, out);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn collect_compare_constraints(
    lhs: &DirExpr,
    rhs: &DirExpr,
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

    if let (DirExpr::Var(name), Some(bits)) = (lhs, lhs_bits) {
        out.entry(name.clone())
            .or_default()
            .push(compare_constraint(bits, signed));
    }
    if let (DirExpr::Var(name), Some(bits)) = (rhs, rhs_bits) {
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
    lhs: &DirExpr,
    rhs: &DirExpr,
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
    expr: &DirExpr,
    result_bits: u32,
    signed: bool,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut HashMap<String, Vec<UseConstraint>>,
) {
    let DirExpr::Var(name) = expr else {
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

fn is_byte_expr(expr: &DirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).is_some_and(is_byte_int_type)
        }
        DirExpr::Const(value, ty) => is_byte_int_type(ty) || (0..=0xff).contains(value),
        DirExpr::Load { ty, .. }
        | DirExpr::Index { elem_ty: ty, .. }
        | DirExpr::FieldAccess { ty, .. } => is_byte_int_type(ty),
        DirExpr::Cast { ty, expr } => {
            is_byte_int_type(ty) || is_byte_expr(expr, known_binding_types)
        }
        _ => false,
    }
}

fn is_byte_pointer_expr(expr: &DirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => known_binding_types
            .get(name)
            .is_some_and(is_byte_pointer_type),
        DirExpr::Cast { ty, expr } => {
            is_byte_pointer_type(ty) || is_byte_pointer_expr(expr, known_binding_types)
        }
        DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => {
            is_byte_pointer_expr(base, known_binding_types)
        }
        _ => false,
    }
}

fn expr_is_var(expr: &DirExpr, name: &str) -> bool {
    matches!(expr, DirExpr::Var(var_name) if var_name == name)
}

fn is_byte_accumulator_update(
    expr: &DirExpr,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    let DirExpr::Binary { op, lhs, rhs, .. } = expr else {
        return false;
    };
    matches!(
        op,
        DirBinaryOp::Add | DirBinaryOp::Sub | DirBinaryOp::Xor | DirBinaryOp::And | DirBinaryOp::Or
    ) && ((expr_is_var(lhs, name) && is_byte_expr(rhs, known_binding_types))
        || (expr_is_var(rhs, name) && is_byte_expr(lhs, known_binding_types)))
}

fn collect_byte_index_accumulator_evidence(
    stmts: &[DirStmt],
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    for stmt in stmts {
        collect_byte_index_accumulator_evidence_stmt(stmt, name, known_binding_types, evidence);
    }
}

fn collect_byte_index_accumulator_evidence_stmt(
    stmt: &DirStmt,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            let self_def = matches!(lhs, DirLValue::Var(lhs_name) if lhs_name == name);
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
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            collect_byte_index_accumulator_evidence_expr(expr, name, known_binding_types, evidence);
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            collect_byte_index_accumulator_evidence(body, name, known_binding_types, evidence);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_byte_index_accumulator_evidence_expr(cond, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence(then_body, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence(else_body, name, known_binding_types, evidence);
        }
        DirStmt::For {
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
        DirStmt::Switch {
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
        DirStmt::VaStart { va_list, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                va_list,
                name,
                known_binding_types,
                evidence,
            );
        }
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn collect_byte_index_accumulator_evidence_lvalue(
    lhs: &DirLValue,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } | DirLValue::FieldAccess { base: ptr, .. } => {
            collect_byte_index_accumulator_evidence_expr(ptr, name, known_binding_types, evidence);
        }
        DirLValue::Index { base, index, .. } => {
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
    expr: &DirExpr,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut ByteIndexAccumulatorEvidence,
) {
    match expr {
        DirExpr::Var(var_name) | DirExpr::AddressOfGlobal(var_name) if var_name == name => {
            evidence.disallowed_uses += 1;
        }
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                inner,
                name,
                known_binding_types,
                evidence,
            );
        }
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if expr_is_var(lhs, name) && is_byte_pointer_expr(rhs, known_binding_types) => {
            evidence.byte_pointer_offset_uses += 1;
        }
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if expr_is_var(rhs, name) && is_byte_pointer_expr(lhs, known_binding_types) => {
            evidence.byte_pointer_offset_uses += 1;
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_byte_index_accumulator_evidence_expr(lhs, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence_expr(rhs, name, known_binding_types, evidence);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_byte_index_accumulator_evidence_expr(
                    arg,
                    name,
                    known_binding_types,
                    evidence,
                );
            }
        }
        DirExpr::Index { base, index, .. } => {
            collect_byte_index_accumulator_evidence_expr(base, name, known_binding_types, evidence);
            collect_byte_index_accumulator_evidence_expr(
                index,
                name,
                known_binding_types,
                evidence,
            );
        }
        DirExpr::Select {
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
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn narrow_byte_index_accumulators(func: &mut DirFunction) -> bool {
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

fn expr_int_bits(expr: &DirExpr, known_binding_types: &HashMap<String, NirType>) -> Option<u32> {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).and_then(nir_type_bits)
        }
        DirExpr::Const(_, ty)
        | DirExpr::Unary { ty, .. }
        | DirExpr::Call { ty, .. }
        | DirExpr::Load { ty, .. }
        | DirExpr::Index { elem_ty: ty, .. }
        | DirExpr::Cast { ty, .. }
        | DirExpr::Select { ty, .. }
        | DirExpr::FieldAccess { ty, .. } => nir_type_bits(ty),
        DirExpr::Binary { ty, .. } => nir_type_bits(ty),
        DirExpr::PtrOffset { .. } | DirExpr::AggregateCopy { .. } => None,
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

fn collect_known_binding_types(func: &DirFunction) -> HashMap<String, NirType> {
    let mut known = HashMap::default();
    for binding in func.locals.iter().chain(func.params.iter()) {
        if binding.ty != NirType::Unknown {
            known.insert(binding.name.clone(), binding.ty.clone());
        }
    }
    known
}

fn return_expr_type(
    expr: &DirExpr,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            known_binding_types.get(name).cloned()
        }
        other => {
            let ty = expr_type(other);
            (ty != NirType::Unknown).then_some(ty)
        }
    }
}

fn collect_value_return_types(
    stmts: &[DirStmt],
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
    stmt: &DirStmt,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        DirStmt::Return(Some(expr)) => {
            if let Some(ty) = return_expr_type(expr, known_binding_types) {
                out.push(ty);
            }
            1
        }
        DirStmt::Return(None) => 0,
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. }
        | DirStmt::For { body: stmts, .. } => {
            collect_value_return_types(stmts, known_binding_types, out)
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_value_return_types(then_body, known_binding_types, out)
                + collect_value_return_types(else_body, known_binding_types, out)
        }
        DirStmt::Switch { cases, default, .. } => {
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

fn promote_return_signedness_from_returns(func: &mut DirFunction) -> bool {
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

fn promote_unknown_call_return_type(func: &mut DirFunction) -> bool {
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

fn native_unsigned_word_type(func: &DirFunction) -> NirType {
    NirType::Int {
        bits: if func.is_64bit { 64 } else { 32 },
        signed: false,
    }
}

fn collect_unknown_call_returns(
    stmts: &[DirStmt],
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    for stmt in stmts {
        collect_unknown_call_returns_stmt(stmt, value_return_count, unknown_call_return_count);
    }
}

fn collect_unknown_call_returns_stmt(
    stmt: &DirStmt,
    value_return_count: &mut usize,
    unknown_call_return_count: &mut usize,
) {
    match stmt {
        DirStmt::Return(Some(expr)) => {
            *value_return_count += 1;
            if is_unknown_call_result(expr) {
                *unknown_call_return_count += 1;
            }
        }
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. }
        | DirStmt::For { body: stmts, .. } => {
            collect_unknown_call_returns(stmts, value_return_count, unknown_call_return_count);
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_unknown_call_returns(then_body, value_return_count, unknown_call_return_count);
            collect_unknown_call_returns(else_body, value_return_count, unknown_call_return_count);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_unknown_call_returns(
                    &case.body,
                    value_return_count,
                    unknown_call_return_count,
                );
            }
            collect_unknown_call_returns(default, value_return_count, unknown_call_return_count);
        }
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn is_unknown_call_result(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Call { ty, .. } => *ty == NirType::Unknown,
        DirExpr::Cast { expr, ty } if *ty == NirType::Unknown => is_unknown_call_result(expr),
        _ => false,
    }
}

fn count_var_uses_expr(expr: &DirExpr, out: &mut HashMap<String, usize>) {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        DirExpr::Const(_, _) => {}
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => count_var_uses_expr(expr, out),
        DirExpr::Binary { lhs, rhs, .. } => {
            count_var_uses_expr(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                count_var_uses_expr(arg, out);
            }
        }
        DirExpr::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        DirExpr::Select {
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

fn count_var_uses_lvalue(lhs: &DirLValue, out: &mut HashMap<String, usize>) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => count_var_uses_expr(ptr, out),
        DirLValue::Index { base, index, .. } => {
            count_var_uses_expr(base, out);
            count_var_uses_expr(index, out);
        }
        DirLValue::FieldAccess { base, .. } => {
            count_var_uses_expr(base, out);
        }
    }
}

fn count_var_uses_stmt(stmt: &DirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            count_var_uses_lvalue(lhs, out);
            count_var_uses_expr(rhs, out);
        }
        DirStmt::VaStart { va_list, .. } | DirStmt::Expr(va_list) => {
            count_var_uses_expr(va_list, out);
        }
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. } => count_var_uses_stmts(stmts, out),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses_expr(cond, out);
            count_var_uses_stmts(then_body, out);
            count_var_uses_stmts(else_body, out);
        }
        DirStmt::For {
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
        DirStmt::Switch {
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
        DirStmt::Return(Some(expr)) => count_var_uses_expr(expr, out),
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn count_var_uses_stmts(stmts: &[DirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_var_uses_stmt(stmt, out);
    }
}

fn store_value_var_name(expr: &DirExpr) -> Option<&str> {
    match expr {
        DirExpr::Var(name) => Some(name.as_str()),
        DirExpr::Cast { expr, .. } => store_value_var_name(expr),
        _ => None,
    }
}

fn count_store_value_uses_stmt(stmt: &DirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Deref { .. } | DirLValue::Index { .. },
            rhs,
        } => {
            if let Some(name) = store_value_var_name(rhs) {
                *out.entry(name.to_owned()).or_default() += 1;
            }
        }
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. } => count_store_value_uses_stmts(stmts, out),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            count_store_value_uses_stmts(then_body, out);
            count_store_value_uses_stmts(else_body, out);
        }
        DirStmt::For {
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
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_store_value_uses_stmts(&case.body, out);
            }
            count_store_value_uses_stmts(default, out);
        }
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Return(_)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn count_store_value_uses_stmts(stmts: &[DirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_store_value_uses_stmt(stmt, out);
    }
}

fn promote_store_value_only_unsigned_params(func: &mut DirFunction) -> bool {
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

fn wrapping_narrow_op(op: DirBinaryOp) -> bool {
    matches!(
        op,
        DirBinaryOp::Add
            | DirBinaryOp::Sub
            | DirBinaryOp::Mul
            | DirBinaryOp::And
            | DirBinaryOp::Or
            | DirBinaryOp::Xor
    )
}

fn collect_wrapping_narrow_return_vars(
    expr: &DirExpr,
    context_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            *out.entry(name.clone()).or_default() += 1;
        }
        DirExpr::Cast { ty, expr } => {
            let bits = nir_type_bits(ty).unwrap_or(context_bits).min(context_bits);
            collect_wrapping_narrow_return_vars(expr, bits, out);
        }
        DirExpr::Unary {
            op: DirUnaryOp::Neg,
            expr,
            ..
        } => collect_wrapping_narrow_return_vars(expr, context_bits, out),
        DirExpr::Binary { op, lhs, rhs, .. } if wrapping_narrow_op(*op) => {
            collect_wrapping_narrow_return_vars(lhs, context_bits, out);
            collect_wrapping_narrow_return_vars(rhs, context_bits, out);
        }
        DirExpr::Const(_, _)
        | DirExpr::Unary { .. }
        | DirExpr::Binary { .. }
        | DirExpr::Call { .. }
        | DirExpr::Load { .. }
        | DirExpr::PtrOffset { .. }
        | DirExpr::Index { .. }
        | DirExpr::Select { .. }
        | DirExpr::FieldAccess { .. }
        | DirExpr::AggregateCopy { .. } => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmt(
    stmt: &DirStmt,
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    match stmt {
        DirStmt::Return(Some(expr)) => collect_wrapping_narrow_return_vars(expr, return_bits, out),
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. }
        | DirStmt::For { body: stmts, .. } => {
            collect_wrapping_narrow_return_vars_stmts(stmts, return_bits, out)
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_wrapping_narrow_return_vars_stmts(then_body, return_bits, out);
            collect_wrapping_narrow_return_vars_stmts(else_body, return_bits, out);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_wrapping_narrow_return_vars_stmts(&case.body, return_bits, out);
            }
            collect_wrapping_narrow_return_vars_stmts(default, return_bits, out);
        }
        _ => {}
    }
}

fn collect_wrapping_narrow_return_vars_stmts(
    stmts: &[DirStmt],
    return_bits: u32,
    out: &mut HashMap<String, usize>,
) {
    for stmt in stmts {
        collect_wrapping_narrow_return_vars_stmt(stmt, return_bits, out);
    }
}

fn narrow_integer_params_from_wrapping_return_uses(func: &mut DirFunction) -> bool {
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
fn merge_constraint(binding: &mut DirBinding, constraint: &UseConstraint) -> bool {
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
    func: &mut DirFunction,
    constraints: &HashMap<String, Vec<UseConstraint>>,
) -> bool {
    let mut roles = HashMap::<String, BindingUseRole>::default();
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
    binding: &DirBinding,
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
pub fn apply_use_driven_type_infer_pass(func: &mut DirFunction) -> bool {
    let before = type_state_signature(func);
    let dependencies = DefinitionDependencyMap::build(&func.body);
    // Iterate to convergence (alias chains may require multiple rounds).
    for _ in 0..4 {
        let mut constraints: HashMap<String, Vec<UseConstraint>> = HashMap::default();
        let mut roles = HashMap::<String, BindingUseRole>::default();
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
    use crate::prelude::*;

    fn make_binding(name: &str) -> DirBinding {
        DirBinding {
            name: name.to_owned(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_typed_binding(name: &str, ty: NirType, origin: NirBindingOrigin) -> DirBinding {
        DirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<DirBinding>, body: Vec<DirStmt>, return_type: NirType) -> DirFunction {
        DirFunction {
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
        let body = vec![DirStmt::Assign {
            lhs: DirLValue::Var("x".to_owned()),
            rhs: DirExpr::Load {
                ptr: Box::new(DirExpr::Var("p".to_owned())),
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
            DirStmt::Assign {
                lhs: DirLValue::Var("idx".to_owned()),
                rhs: DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("p".to_owned())),
                    ty: u8_ty.clone(),
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("idx".to_owned()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("idx".to_owned())),
                    rhs: Box::new(DirExpr::Load {
                        ptr: Box::new(DirExpr::Var("q".to_owned())),
                        ty: u8_ty.clone(),
                    }),
                    ty: u32_ty.clone(),
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".to_owned()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("p".to_owned())),
                    rhs: Box::new(DirExpr::Var("idx".to_owned())),
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
        let body = vec![DirStmt::Assign {
            lhs: DirLValue::Var("cursor".to_owned()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("p".to_owned())),
                rhs: Box::new(DirExpr::Var("idx".to_owned())),
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
        let body = vec![DirStmt::Assign {
            lhs: DirLValue::Var("end".to_owned()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("base".to_owned())),
                rhs: Box::new(DirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(DirExpr::Var("count".to_owned())),
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
            DirStmt::Assign {
                lhs: DirLValue::Var("alias".to_owned()),
                rhs: DirExpr::Var("input".to_owned()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".to_owned()),
                rhs: DirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(DirExpr::Var("alias".to_owned())),
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("value".to_owned()),
                rhs: DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("cursor".to_owned())),
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
            DirStmt::Assign {
                lhs: DirLValue::Var("p".to_owned()),
                rhs: DirExpr::Cast {
                    ty: byte_ptr_ty.clone(),
                    expr: Box::new(DirExpr::Var("x".to_owned())),
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("sum".to_owned()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("x".to_owned())),
                    rhs: Box::new(DirExpr::Const(1, u64_ty.clone())),
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
        let body = vec![DirStmt::Return(Some(DirExpr::Var("acc".to_owned())))];
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
        let body = vec![DirStmt::Assign {
            lhs: DirLValue::Deref {
                ptr: Box::new(DirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
            rhs: DirExpr::Const(
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
        let body = vec![DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::SLt,
                lhs: Box::new(DirExpr::Var("a".to_owned())),
                rhs: Box::new(DirExpr::Const(
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
        let body = vec![DirStmt::Return(Some(DirExpr::Call {
            target: "param_1".to_owned(),
            args: vec![
                DirExpr::Var("param_2".to_owned()),
                DirExpr::Var("param_3".to_owned()),
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
        let body = vec![DirStmt::Return(Some(DirExpr::Call {
            target: "((code *)param_1)".to_owned(),
            args: vec![
                DirExpr::Var("param_2".to_owned()),
                DirExpr::Var("param_3".to_owned()),
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
        let body = vec![DirStmt::If {
            cond: DirExpr::Var("flag".to_owned()),
            then_body: vec![DirStmt::Return(Some(DirExpr::Call {
                target: "param_1".to_owned(),
                args: Vec::new(),
                ty: NirType::Unknown,
            }))],
            else_body: vec![DirStmt::Return(Some(DirExpr::Var("fallback".to_owned())))],
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
        let body = vec![DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::SLt,
                lhs: Box::new(DirExpr::Var("a".to_owned())),
                rhs: Box::new(DirExpr::Var("b".to_owned())),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Return(Some(DirExpr::Var("b".to_owned())))],
            else_body: vec![DirStmt::Return(Some(DirExpr::Var("a".to_owned())))],
        }];
        let mut func = DirFunction {
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
        let body = vec![DirStmt::Assign {
            lhs: DirLValue::Var("x".to_owned()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Shr,
                lhs: Box::new(DirExpr::Var("x".to_owned())),
                rhs: Box::new(DirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: i32_ty.clone(),
            },
        }];
        let mut func = DirFunction {
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
            DirStmt::Assign {
                lhs: DirLValue::Var("shifted".into()),
                rhs: DirExpr::Var("count".into()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("shifted".into()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Shr,
                    lhs: Box::new(DirExpr::Var("shifted".into())),
                    rhs: Box::new(DirExpr::Const(1, u64_ty.clone())),
                    ty: i64_ty.clone(),
                },
            },
        ];
        let mut func = DirFunction {
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
        let body = vec![DirStmt::Return(Some(DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var("a".to_owned())),
            rhs: Box::new(DirExpr::Var("b".to_owned())),
            ty: i32_ty.clone(),
        }))];
        let mut func = DirFunction {
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
        let body = vec![DirStmt::Return(Some(DirExpr::Cast {
            ty: i32_ty.clone(),
            expr: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("a".to_owned())),
                rhs: Box::new(DirExpr::Var("b".to_owned())),
                ty: u32_ty.clone(),
            }),
        }))];
        let mut func = DirFunction {
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
        let mut func = DirFunction {
            name: "add32".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_typed_binding("param_1", u64_ty.clone(), NirBindingOrigin::ParamIndex(0)),
                make_typed_binding("param_2", u64_ty.clone(), NirBindingOrigin::ParamIndex(1)),
            ],
            locals: vec![],
            return_type: u32_ty.clone(),
            surface_return_type_name: None,
            body: vec![DirStmt::Return(Some(DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var("param_1".to_owned())),
                rhs: Box::new(DirExpr::Var("param_2".to_owned())),
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
        let mut func = DirFunction {
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
                DirStmt::Expr(DirExpr::Call {
                    target: "observe64".to_owned(),
                    args: vec![DirExpr::Var("param_1".to_owned())],
                    ty: NirType::Unknown,
                }),
                DirStmt::Return(Some(DirExpr::Var("param_1".to_owned()))),
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
        let mut func = DirFunction {
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
            body: vec![DirStmt::Assign {
                lhs: DirLValue::Deref {
                    ptr: Box::new(DirExpr::Var("param_1".to_owned())),
                    ty: u32_ty.clone(),
                },
                rhs: DirExpr::Var("param_2".to_owned()),
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
        let mut func = DirFunction {
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
                DirStmt::If {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::Lt,
                        lhs: Box::new(DirExpr::Var("param_2".to_owned())),
                        rhs: Box::new(DirExpr::Const(10, u32_ty.clone())),
                        ty: NirType::Bool,
                    },
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::Var("param_1".to_owned())),
                        ty: u32_ty.clone(),
                    },
                    rhs: DirExpr::Var("param_2".to_owned()),
                },
            ],
            ..Default::default()
        };

        assert!(!super::apply_use_driven_type_infer_pass(&mut func));
        assert_eq!(func.params[1].ty, u32_ty);
    }

    #[test]
    fn signed_compare_without_width_evidence_does_not_invent_type() {
        let body = vec![DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::SLt,
                lhs: Box::new(DirExpr::Var("a".to_owned())),
                rhs: Box::new(DirExpr::Var("b".to_owned())),
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
        let body = vec![DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::SLt,
                lhs: Box::new(DirExpr::Var("a".to_owned())),
                rhs: Box::new(DirExpr::Const(
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
        let body = vec![DirStmt::Return(Some(DirExpr::Var("r".to_owned())))];
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
            DirStmt::Assign {
                lhs: DirLValue::Var("p".to_owned()),
                rhs: DirExpr::Var("param_1".to_owned()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Deref {
                    ptr: Box::new(DirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: DirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = DirFunction {
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
            DirStmt::Assign {
                lhs: DirLValue::Var("p".to_owned()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("param_1".to_owned())),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Mul,
                        lhs: Box::new(DirExpr::Var("idx".to_owned())),
                        rhs: Box::new(DirExpr::Const(4, u64_ty.clone())),
                        ty: u64_ty.clone(),
                    }),
                    ty: u64_ty,
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Deref {
                    ptr: Box::new(DirExpr::Var("p".to_owned())),
                    ty: uint_ty.clone(),
                },
                rhs: DirExpr::Const(7, uint_ty.clone()),
            },
        ];
        let mut func = DirFunction {
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
