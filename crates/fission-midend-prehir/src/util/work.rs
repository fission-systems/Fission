use crate::ir::{PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};

enum Node<'a> {
    Stmt(&'a PreHirStmt),
    Expr(&'a PreHirExpr),
    LValue(&'a PreHirLValue),
}

/// Return whether a body contains at most `budget` expression nodes.
///
/// The walk is iterative so the admission check is safe for the same deep
/// expression trees it is intended to bound.
pub fn pre_hir_body_expr_nodes_fit_budget(body: &[PreHirStmt], budget: usize) -> bool {
    pre_hir_expr_nodes_fit_budget(body.iter().map(Node::Stmt), budget)
}

/// Return whether a function body and its binding initializers contain at
/// most `budget` expression nodes.
pub fn pre_hir_function_expr_nodes_fit_budget(func: &PreHirFunction, budget: usize) -> bool {
    let nodes = func.body.iter().map(Node::Stmt).chain(
        func.params
            .iter()
            .chain(func.locals.iter())
            .filter_map(|binding| binding.initializer.as_ref())
            .map(Node::Expr),
    );
    pre_hir_expr_nodes_fit_budget(nodes, budget)
}

fn pre_hir_expr_nodes_fit_budget<'a>(
    nodes: impl IntoIterator<Item = Node<'a>>,
    budget: usize,
) -> bool {
    let mut stack: Vec<Node<'a>> = nodes.into_iter().collect();
    let mut expr_nodes = 0usize;

    while let Some(node) = stack.pop() {
        match node {
            Node::Stmt(stmt) => match stmt {
                PreHirStmt::Assign { lhs, rhs } => {
                    stack.push(Node::LValue(lhs));
                    stack.push(Node::Expr(rhs));
                }
                PreHirStmt::VaStart { va_list, .. } => stack.push(Node::Expr(va_list)),
                PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                    stack.push(Node::Expr(expr));
                }
                PreHirStmt::Block(body)
                | PreHirStmt::While { body, .. }
                | PreHirStmt::DoWhile { body, .. } => {
                    stack.extend(body.iter().map(Node::Stmt));
                    match stmt {
                        PreHirStmt::While { cond, .. } | PreHirStmt::DoWhile { cond, .. } => {
                            stack.push(Node::Expr(cond));
                        }
                        _ => {}
                    }
                }
                PreHirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    stack.extend(body.iter().map(Node::Stmt));
                    if let Some(init) = init {
                        stack.push(Node::Stmt(init));
                    }
                    if let Some(cond) = cond {
                        stack.push(Node::Expr(cond));
                    }
                    if let Some(update) = update {
                        stack.push(Node::Stmt(update));
                    }
                }
                PreHirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    stack.push(Node::Expr(cond));
                    stack.extend(then_body.iter().map(Node::Stmt));
                    stack.extend(else_body.iter().map(Node::Stmt));
                }
                PreHirStmt::Switch {
                    expr,
                    cases,
                    default,
                } => {
                    stack.push(Node::Expr(expr));
                    for case in cases {
                        stack.extend(case.body.iter().map(Node::Stmt));
                    }
                    stack.extend(default.iter().map(Node::Stmt));
                }
                PreHirStmt::Return(None)
                | PreHirStmt::Break
                | PreHirStmt::Continue
                | PreHirStmt::Label(_)
                | PreHirStmt::Goto(_) => {}
            },
            Node::LValue(lhs) => match lhs {
                PreHirLValue::Var(_) => {}
                PreHirLValue::Deref { ptr, .. } => stack.push(Node::Expr(ptr)),
                PreHirLValue::Index { base, index, .. } => {
                    stack.push(Node::Expr(base));
                    stack.push(Node::Expr(index));
                }
                PreHirLValue::FieldAccess { base, .. } => stack.push(Node::Expr(base)),
            },
            Node::Expr(expr) => {
                expr_nodes += 1;
                if expr_nodes > budget {
                    return false;
                }
                match expr {
                    PreHirExpr::Var(_)
                    | PreHirExpr::AddressOfGlobal(_)
                    | PreHirExpr::AddressOfLocal(_)
                    | PreHirExpr::Const(_, _) => {}
                    PreHirExpr::Cast { expr, .. }
                    | PreHirExpr::Unary { expr, .. }
                    | PreHirExpr::Load { ptr: expr, .. }
                    | PreHirExpr::PtrOffset { base: expr, .. }
                    | PreHirExpr::FieldAccess { base: expr, .. }
                    | PreHirExpr::AggregateCopy { src: expr, .. } => {
                        stack.push(Node::Expr(expr));
                    }
                    PreHirExpr::Binary { lhs, rhs, .. }
                    | PreHirExpr::Index {
                        base: lhs,
                        index: rhs,
                        ..
                    } => {
                        stack.push(Node::Expr(lhs));
                        stack.push(Node::Expr(rhs));
                    }
                    PreHirExpr::Call { args, .. } => {
                        stack.extend(args.iter().map(Node::Expr));
                    }
                    PreHirExpr::Select {
                        cond,
                        then_expr,
                        else_expr,
                        ..
                    } => {
                        stack.push(Node::Expr(cond));
                        stack.push(Node::Expr(then_expr));
                        stack.push(Node::Expr(else_expr));
                    }
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{NirType, PreHirBinaryOp};

    #[test]
    fn expression_budget_counts_nested_nodes() {
        let ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Const(1, ty.clone())),
            rhs: Box::new(PreHirExpr::Const(2, ty.clone())),
            ty,
        }))];

        assert!(pre_hir_body_expr_nodes_fit_budget(&body, 3));
        assert!(!pre_hir_body_expr_nodes_fit_budget(&body, 2));
    }
}
