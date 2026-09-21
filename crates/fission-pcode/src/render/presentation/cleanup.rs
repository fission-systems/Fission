//! Final HIR presentation cleanup: cast simplification, dead writes, and unused locals.
//!
//! These passes run after the structural presentation fixed point. They own
//! only presentation-local cleanup; semantic recovery remains in the
//! normalization and structuring layers.

use super::*;

/// ```text
/// goto Lcond;
/// Lbody:

/// Peel redundant integer casts in the presentation tree (HIR only).
pub(super) fn simplify_presentation_casts(func: &mut HirFunction) {
    let mut var_types: HashMap<String, NirType> = HashMap::new();
    for b in func.params.iter().chain(func.locals.iter()) {
        var_types.insert(b.name.clone(), b.ty.clone());
    }
    simplify_casts_in_stmts(&mut func.body, &var_types);
}

fn simplify_casts_in_stmts(stmts: &mut [HirStmt], var_types: &HashMap<String, NirType>) {
    for stmt in stmts.iter_mut() {
        simplify_casts_in_stmt(stmt, var_types);
    }
}

fn simplify_casts_in_stmt(stmt: &mut HirStmt, var_types: &HashMap<String, NirType>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            simplify_casts_in_lvalue(lhs, var_types);
            simplify_casts_in_expr(rhs, var_types);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            simplify_casts_in_expr(e, var_types)
        }
        HirStmt::Block(b) => simplify_casts_in_stmts(b, var_types),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            simplify_casts_in_expr(cond, var_types);
            simplify_casts_in_stmts(body, var_types);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            simplify_casts_in_expr(cond, var_types);
            simplify_casts_in_stmts(then_body, var_types);
            simplify_casts_in_stmts(else_body, var_types);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                simplify_casts_in_stmt(i, var_types);
            }
            if let Some(c) = cond {
                simplify_casts_in_expr(c, var_types);
            }
            if let Some(u) = update {
                simplify_casts_in_stmt(u, var_types);
            }
            simplify_casts_in_stmts(body, var_types);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            simplify_casts_in_expr(expr, var_types);
            for case in cases {
                simplify_casts_in_stmts(&mut case.body, var_types);
            }
            simplify_casts_in_stmts(default, var_types);
        }
        _ => {}
    }
}

fn simplify_casts_in_lvalue(lhs: &mut HirLValue, var_types: &HashMap<String, NirType>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => simplify_casts_in_expr(ptr, var_types),
        HirLValue::Index { base, index, .. } => {
            simplify_casts_in_expr(base, var_types);
            simplify_casts_in_expr(index, var_types);
        }
        HirLValue::FieldAccess { base, .. } => simplify_casts_in_expr(base, var_types),
    }
}

fn simplify_casts_in_expr(expr: &mut HirExpr, var_types: &HashMap<String, NirType>) {
    match expr {
        HirExpr::Cast { ty, expr: inner } => {
            simplify_casts_in_expr(inner, var_types);
            // Peel (T)(T)x
            if let HirExpr::Cast {
                ty: inner_ty,
                expr: deeper,
            } = inner.as_ref()
            {
                if inner_ty == ty {
                    *inner = deeper.clone();
                    simplify_casts_in_expr(expr, var_types);
                    return;
                }
                // Peel outer wider unsigned over inner unsigned int cast family:
                // (ulonglong)(uint)x → (ulonglong)x when only width sugar.
                if let (
                    NirType::Int {
                        bits: outer_bits,
                        signed: false,
                    },
                    NirType::Int {
                        bits: inner_bits,
                        signed: false,
                    },
                ) = (&*ty, inner_ty)
                {
                    if outer_bits >= inner_bits {
                        *inner = deeper.clone();
                    }
                }
            }
            // (T)v when v is declared as T → v
            if let HirExpr::Var(name) = inner.as_ref() {
                if var_types.get(name.as_str()).is_some_and(|vt| vt == &*ty) {
                    *expr = HirExpr::Var(name.clone());
                    return;
                }
            }
            // (T)const when const already carries T
            if let HirExpr::Const(v, cty) = inner.as_ref() {
                if cty == &*ty {
                    *expr = HirExpr::Const(*v, ty.clone());
                }
            }
        }
        HirExpr::Unary { expr: e, .. } => simplify_casts_in_expr(e, var_types),
        HirExpr::Binary { lhs, rhs, .. } => {
            simplify_casts_in_expr(lhs, var_types);
            simplify_casts_in_expr(rhs, var_types);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            simplify_casts_in_expr(cond, var_types);
            simplify_casts_in_expr(then_expr, var_types);
            simplify_casts_in_expr(else_expr, var_types);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                simplify_casts_in_expr(a, var_types);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => simplify_casts_in_expr(ptr, var_types),
        HirExpr::Index { base, index, .. } => {
            simplify_casts_in_expr(base, var_types);
            simplify_casts_in_expr(index, var_types);
        }
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
    }
}

/// Whether `name` is one the builder mints for a stack slot.
///
/// The origin alone is not enough by the time presentation runs: a binding
/// pruned by a name-based liveness pass and then put back by
/// `rescue_undeclared_bindings` comes back as `Temp`, losing the one field
/// that said it was a frame slot. `local_2` -- the second half of a six-byte
/// string `main` passes to two callees -- arrives here exactly that way. The
/// prefixes are the same ones `partition.rs` and `slots.rs` already key on.
fn name_is_stack_slot(name: &str) -> bool {
    name.starts_with("local_")
        || name.starts_with("stack_")
        || name.starts_with("home_")
        || name.starts_with("arg_out_")
        || name.starts_with("ret_scaffold_")
}

pub(super) fn eliminate_pure_dead_assigns(
    func: &mut HirFunction,
    globals: &HashSet<String>,
) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    // A stack slot's store is observable through a pointer into the frame, so
    // an unread *name* is not evidence the write is dead. `main` builds a
    // six-byte string across `local_6` and `local_2` and passes `&local_6` to
    // its callees: neither name is ever read, and dropping either write hands
    // the callees a frame that was never filled in. This is the same rule
    // `eliminate_dead_local_clobber_assigns` holds in normalize.
    let stack_backed: HashSet<&str> = func
        .locals
        .iter()
        .filter(|binding| {
            matches!(
                binding.origin,
                Some(
                    NirBindingOrigin::StackOffset(_)
                        | NirBindingOrigin::DerivedFromStackOffset(_)
                        | NirBindingOrigin::HomeSlot(_)
                        | NirBindingOrigin::OutgoingArgSlot(_)
                )
            ) || name_is_stack_slot(&binding.name)
        })
        .map(|binding| binding.name.as_str())
        .collect();
    // Whole-function use counts only. Nested subtree-local counts incorrectly
    // treat `if { x = e; } return x;` as dead `x` (use lives outside the if body).
    let mut any = false;
    for _ in 0..16 {
        let mut defs = HashMap::new();
        count_defs_in_stmts(&func.body, &mut defs);
        let use_counts: HashMap<String, usize> = defs
            .keys()
            .map(|n| (n.clone(), count_uses_in_stmts(&func.body, n)))
            .collect();
        let changed = eliminate_pure_dead_in_stmts(
            &mut func.body,
            &formal,
            &stack_backed,
            &use_counts,
            globals,
        );
        if !changed {
            break;
        }
        any = true;
    }
    any
}

fn eliminate_pure_dead_in_stmts(
    stmts: &mut Vec<HirStmt>,
    formal: &HashSet<&str>,
    stack_backed: &HashSet<&str>,
    use_counts: &HashMap<String, usize>,
    globals: &HashSet<String>,
) -> bool {
    let mut changed = false;
    let before = stmts.len();
    stmts.retain(|stmt| match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } if !formal.contains(name.as_str())
            && !stack_backed.contains(name.as_str())
            && !globals.contains(name.as_str())
            && use_counts.get(name.as_str()).copied().unwrap_or(0) == 0
            && expr_is_presentation_pure(rhs) =>
        {
            // Includes flag (zf/sf/…) and pure-intrinsic temps once unused.
            changed = true;
            false
        }
        _ => true,
    });
    if stmts.len() != before {
        changed = true;
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |=
                    eliminate_pure_dead_in_stmts(body, formal, stack_backed, use_counts, globals);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_pure_dead_in_stmts(
                    then_body,
                    formal,
                    stack_backed,
                    use_counts,
                    globals,
                );
                changed |= eliminate_pure_dead_in_stmts(
                    else_body,
                    formal,
                    stack_backed,
                    use_counts,
                    globals,
                );
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= eliminate_pure_dead_in_stmts(
                            b,
                            formal,
                            stack_backed,
                            use_counts,
                            globals,
                        );
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= eliminate_pure_dead_in_stmts(
                            b,
                            formal,
                            stack_backed,
                            use_counts,
                            globals,
                        );
                    }
                }
                changed |=
                    eliminate_pure_dead_in_stmts(body, formal, stack_backed, use_counts, globals);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= eliminate_pure_dead_in_stmts(
                        &mut case.body,
                        formal,
                        stack_backed,
                        use_counts,
                        globals,
                    );
                }
                changed |= eliminate_pure_dead_in_stmts(
                    default,
                    formal,
                    stack_backed,
                    use_counts,
                    globals,
                );
            }
            _ => {}
        }
    }
    changed
}

pub(super) fn drop_unused_presentation_locals(func: &mut HirFunction) {
    let mut used = HashSet::new();
    collect_used_names_stmts(&func.body, &mut used);
    for p in &func.params {
        used.insert(p.name.clone());
    }
    // Drop any never-referenced local for HIR presentation (including home
    // scaffold and temps whose assigns were folded away).
    func.locals.retain(|b| used.contains(&b.name));
}

fn collect_used_names_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        collect_used_names_stmt(s, out);
    }
}

fn collect_used_names_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_used_names_lvalue(lhs, out);
            collect_used_names_expr(rhs, out);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => collect_used_names_expr(e, out),
        HirStmt::Return(None) => {}
        HirStmt::Block(body) => collect_used_names_stmts(body, out),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            collect_used_names_expr(cond, out);
            collect_used_names_stmts(body, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_used_names_expr(cond, out);
            collect_used_names_stmts(then_body, out);
            collect_used_names_stmts(else_body, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                collect_used_names_stmt(i, out);
            }
            if let Some(c) = cond {
                collect_used_names_expr(c, out);
            }
            if let Some(u) = update {
                collect_used_names_stmt(u, out);
            }
            collect_used_names_stmts(body, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_used_names_expr(expr, out);
            for case in cases {
                collect_used_names_stmts(&case.body, out);
            }
            collect_used_names_stmts(default, out);
        }
        HirStmt::VaStart { va_list, .. } => collect_used_names_expr(va_list, out),
        _ => {}
    }
}

fn collect_used_names_lvalue(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        // Assigned vars still need a declaration while the assign remains.
        HirLValue::Var(n) => {
            out.insert(n.clone());
        }
        HirLValue::Deref { ptr, .. } => collect_used_names_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            collect_used_names_expr(base, out);
            collect_used_names_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => collect_used_names_expr(base, out),
    }
}

fn collect_used_names_expr(expr: &HirExpr, out: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(n) | HirExpr::AddressOfGlobal(n) | HirExpr::AddressOfLocal(n) => {
            out.insert(n.clone());
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => {
            collect_used_names_expr(expr, out)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_used_names_expr(lhs, out);
            collect_used_names_expr(rhs, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_used_names_expr(cond, out);
            collect_used_names_expr(then_expr, out);
            collect_used_names_expr(else_expr, out);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                collect_used_names_expr(a, out);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => collect_used_names_expr(ptr, out),
        HirExpr::Index { base, index, .. } => {
            collect_used_names_expr(base, out);
            collect_used_names_expr(index, out);
        }
    }
}
