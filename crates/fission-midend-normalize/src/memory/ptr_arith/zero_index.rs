use crate::prelude::*;

pub(super) fn normalize_zero_index_stmt(stmt: &mut PreHirStmt) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            changed |= normalize_zero_index_lvalue(lhs);
            changed |= normalize_zero_index_expr(rhs);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            changed |= normalize_zero_index_expr(expr);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= normalize_zero_index_expr(cond);
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body) {
                changed |= normalize_zero_index_stmt(stmt);
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body) {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        PreHirStmt::While { cond, body } => {
            changed |= normalize_zero_index_expr(cond);
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        PreHirStmt::DoWhile { body, cond } => {
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                changed |= normalize_zero_index_stmt(stmt);
            }
            changed |= normalize_zero_index_expr(cond);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                changed |= normalize_zero_index_stmt(init);
            }
            if let Some(cond) = cond {
                changed |= normalize_zero_index_expr(cond);
            }
            if let Some(update) = update {
                changed |= normalize_zero_index_stmt(update);
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= normalize_zero_index_expr(expr);
            for case in cases {
                for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body) {
                    changed |= normalize_zero_index_stmt(stmt);
                }
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default) {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        PreHirStmt::Block(body) => {
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        PreHirStmt::VaStart { va_list, .. } => {
            changed |= normalize_zero_index_expr(va_list);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
    changed
}

fn normalize_zero_index_lvalue(lhs: &mut PreHirLValue) -> bool {
    match lhs {
        PreHirLValue::Deref { ptr, .. } => normalize_zero_index_expr(ptr),
        PreHirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let mut changed = normalize_zero_index_expr(base) | normalize_zero_index_expr(index);
            if is_zero_index(index) {
                let ptr = std::mem::replace(base, Box::new(PreHirExpr::Const(0, NirType::Unknown)));
                let ty = elem_ty.clone();
                *lhs = PreHirLValue::Deref { ptr, ty };
                changed = true;
            }
            changed
        }
        PreHirLValue::Var(_) => false,
        PreHirLValue::FieldAccess { base, .. } => normalize_zero_index_expr(base),
    }
}

fn normalize_zero_index_expr(expr: &mut PreHirExpr) -> bool {
    let mut changed = false;
    match expr {
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= normalize_zero_index_expr(lhs);
            changed |= normalize_zero_index_expr(rhs);
        }
        PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            changed |= normalize_zero_index_expr(expr);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= normalize_zero_index_expr(arg);
            }
        }
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            changed |= normalize_zero_index_expr(base);
            changed |= normalize_zero_index_expr(index);
            if is_zero_index(index) {
                let ptr = std::mem::replace(base, Box::new(PreHirExpr::Const(0, NirType::Unknown)));
                let ty = elem_ty.clone();
                *expr = PreHirExpr::Load { ptr, ty };
                changed = true;
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= normalize_zero_index_expr(cond);
            changed |= normalize_zero_index_expr(then_expr);
            changed |= normalize_zero_index_expr(else_expr);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    changed
}

fn is_zero_index(index: &PreHirExpr) -> bool {
    matches!(index, PreHirExpr::Const(0, _))
}
