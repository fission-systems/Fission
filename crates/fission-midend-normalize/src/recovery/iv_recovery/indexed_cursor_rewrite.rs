use crate::prelude::*;

fn rewrite_cursor_expr_to_index(expr: &mut PreHirExpr, cursor: &str, index_name: &str) -> bool {
    match expr {
        PreHirExpr::Var(name) => name != cursor,
        PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => true,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            rewrite_cursor_expr_to_index(expr, cursor, index_name)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite_cursor_expr_to_index(lhs, cursor, index_name)
                && rewrite_cursor_expr_to_index(rhs, cursor, index_name)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_cursor_expr_to_index(cond, cursor, index_name)
                && rewrite_cursor_expr_to_index(then_expr, cursor, index_name)
                && rewrite_cursor_expr_to_index(else_expr, cursor, index_name)
        }
        PreHirExpr::Call { args, .. } => args
            .iter_mut()
            .all(|arg| rewrite_cursor_expr_to_index(arg, cursor, index_name)),
        PreHirExpr::Load { ptr, ty } if direct_cursor_var(ptr, cursor) => {
            *expr = cursor_index_expr(cursor, index_name, ty.clone());
            true
        }
        PreHirExpr::Load { ptr, .. }
        | PreHirExpr::PtrOffset { base: ptr, .. }
        | PreHirExpr::FieldAccess { base: ptr, .. } => {
            rewrite_cursor_expr_to_index(ptr, cursor, index_name)
        }
        PreHirExpr::Index { base, index, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
                && rewrite_cursor_expr_to_index(index, cursor, index_name)
        }
    }
}

fn rewrite_cursor_lvalue_to_index(lhs: &mut PreHirLValue, cursor: &str, index_name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(name) => name != cursor,
        PreHirLValue::Deref { ptr, ty } if direct_cursor_var(ptr, cursor) => {
            *lhs = PreHirLValue::Index {
                base: Box::new(PreHirExpr::Var(cursor.to_string())),
                index: Box::new(index_var_expr(index_name)),
                elem_ty: ty.clone(),
            };
            true
        }
        PreHirLValue::Deref { ptr, .. } => rewrite_cursor_expr_to_index(ptr, cursor, index_name),
        PreHirLValue::Index { base, index, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
                && rewrite_cursor_expr_to_index(index, cursor, index_name)
        }
        PreHirLValue::FieldAccess { base, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
        }
    }
}

fn rewrite_cursor_stmt_to_index(stmt: &mut PreHirStmt, cursor: &str, index_name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            rewrite_cursor_lvalue_to_index(lhs, cursor, index_name)
                && rewrite_cursor_expr_to_index(rhs, cursor, index_name)
        }
        PreHirStmt::VaStart { va_list, .. } | PreHirStmt::Expr(va_list) => {
            rewrite_cursor_expr_to_index(va_list, cursor, index_name)
        }
        PreHirStmt::Return(Some(expr)) => rewrite_cursor_expr_to_index(expr, cursor, index_name),
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => rewrite_cursor_body_to_index(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            cursor,
            index_name,
        ),
        PreHirStmt::DoWhile { body, cond } => {
            rewrite_cursor_body_to_index(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                cursor,
                index_name,
            ) && rewrite_cursor_expr_to_index(cond, cursor, index_name)
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref_mut()
                .is_none_or(|stmt| rewrite_cursor_stmt_to_index(stmt, cursor, index_name))
                && cond
                    .as_mut()
                    .is_none_or(|expr| rewrite_cursor_expr_to_index(expr, cursor, index_name))
                && update
                    .as_deref_mut()
                    .is_none_or(|stmt| rewrite_cursor_stmt_to_index(stmt, cursor, index_name))
                && rewrite_cursor_body_to_index(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    cursor,
                    index_name,
                )
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_cursor_expr_to_index(cond, cursor, index_name)
                && rewrite_cursor_body_to_index(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    cursor,
                    index_name,
                )
                && rewrite_cursor_body_to_index(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    cursor,
                    index_name,
                )
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rewrite_cursor_expr_to_index(expr, cursor, index_name)
                && cases.iter_mut().all(|case| {
                    rewrite_cursor_body_to_index(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        cursor,
                        index_name,
                    )
                })
                && rewrite_cursor_body_to_index(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    cursor,
                    index_name,
                )
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => true,
    }
}

pub(super) fn rewrite_cursor_body_to_index(
    body: &mut [PreHirStmt],
    cursor: &str,
    index_name: &str,
) -> bool {
    body.iter_mut()
        .all(|stmt| rewrite_cursor_stmt_to_index(stmt, cursor, index_name))
}

pub(super) fn direct_cursor_var(expr: &PreHirExpr, cursor: &str) -> bool {
    matches!(strip_casts(expr), PreHirExpr::Var(name) if name == cursor)
}

fn index_var_expr(index_name: &str) -> PreHirExpr {
    PreHirExpr::Var(index_name.to_string())
}

fn cursor_index_expr(cursor: &str, index_name: &str, elem_ty: NirType) -> PreHirExpr {
    PreHirExpr::Index {
        base: Box::new(PreHirExpr::Var(cursor.to_string())),
        index: Box::new(index_var_expr(index_name)),
        elem_ty,
    }
}
