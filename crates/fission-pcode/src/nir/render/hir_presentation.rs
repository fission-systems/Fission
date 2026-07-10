//! HIR presentation pass — readability-only tree polish before HIR print.
//!
//! Must not change control-flow structure or expression meaning beyond
//! presentation (unused local drop for printing, sugar). Semantic recovery
//! stays in normalize/.

use super::super::*;
use std::collections::HashSet;

/// Apply HIR-facing presentation polish in place.
pub(crate) fn apply_hir_presentation(func: &mut HirFunction) {
    drop_unused_noise_locals(func);
}

fn drop_unused_noise_locals(func: &mut HirFunction) {
    let mut used = HashSet::new();
    collect_used_names_stmts(&func.body, &mut used);
    for p in &func.params {
        used.insert(p.name.clone());
    }
    // Drop never-referenced stack/home scaffold locals only.
    func.locals.retain(|b| {
        if used.contains(&b.name) {
            return true;
        }
        let noise = b.name == "home_0"
            || b.name == "home_1"
            || b.name.starts_with("home_")
            || matches!(b.name.as_str(), "rsp" | "rbp" | "esp" | "ebp");
        !noise
    });
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
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_used_names_stmts(body, out)
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
        _ => {}
    }
}

fn collect_used_names_lvalue(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        // Assigned vars still need a declaration.
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
        HirExpr::Var(n) | HirExpr::AddressOfGlobal(n) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hir_presentation_drops_unused_home_local() {
        let mut func = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![
                NirBinding {
                    name: "home_0".into(),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                NirBinding {
                    name: "x".into(),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            return_type: NirType::Int {
                bits: 32,
                signed: true,
            },
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".into())))],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        assert!(func.locals.iter().all(|b| b.name != "home_0"));
        assert!(func.locals.iter().any(|b| b.name == "x"));
    }
}
