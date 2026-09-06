//! Pure HIR rewrite helpers used by guarded-tail promotion/execution.

use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};

pub fn expr_contains_var(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Var(var)
        | PreHirExpr::AddressOfGlobal(var)
        | PreHirExpr::AddressOfLocal(var) => var == name,
        PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, name) || expr_contains_var(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, name)),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var(cond, name)
                || expr_contains_var(then_expr, name)
                || expr_contains_var(else_expr, name)
        }
    }
}

pub fn lvalue_contains_var(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_contains_var(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_contains_var(base, name),
    }
}

/// Every variable name `expr_contains_var` would answer `true` for.
///
/// Mirrors that function arm for arm so the two cannot drift; it exists so a
/// caller asking about many names walks the expression once.
pub fn expr_var_names(expr: &PreHirExpr) -> std::collections::HashSet<&str> {
    let mut out = std::collections::HashSet::new();
    collect_expr_var_names(expr, &mut out);
    out
}

fn collect_expr_var_names<'a>(expr: &'a PreHirExpr, out: &mut std::collections::HashSet<&'a str>) {
    match expr {
        PreHirExpr::Var(var)
        | PreHirExpr::AddressOfGlobal(var)
        | PreHirExpr::AddressOfLocal(var) => {
            out.insert(var.as_str());
        }
        PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => collect_expr_var_names(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_expr_var_names(lhs, out);
            collect_expr_var_names(rhs, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_expr_var_names(arg, out);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_expr_var_names(base, out);
            collect_expr_var_names(index, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expr_var_names(cond, out);
            collect_expr_var_names(then_expr, out);
            collect_expr_var_names(else_expr, out);
        }
    }
}

/// Every variable name `lvalue_contains_var` would answer `true` for.
pub fn lvalue_var_names(lhs: &PreHirLValue) -> std::collections::HashSet<&str> {
    match lhs {
        PreHirLValue::Var(_) => std::collections::HashSet::new(),
        PreHirLValue::Deref { ptr, .. } => expr_var_names(ptr),
        PreHirLValue::Index { base, index, .. } => {
            let mut out = expr_var_names(base);
            out.extend(expr_var_names(index));
            out
        }
        PreHirLValue::FieldAccess { base, .. } => expr_var_names(base),
    }
}

pub fn replace_var_in_expr(expr: &mut PreHirExpr, name: &str, replacement: &PreHirExpr) {
    match expr {
        PreHirExpr::Var(var) if var == name => *expr = replacement.clone(),
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
            replace_var_in_expr(expr, name, replacement);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                replace_var_in_expr(arg, name, replacement);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            replace_var_in_expr(cond, name, replacement);
            replace_var_in_expr(then_expr, name, replacement);
            replace_var_in_expr(else_expr, name, replacement);
        }
    }
}

pub fn replace_var_in_lvalue(lhs: &mut PreHirLValue, name: &str, replacement: &PreHirExpr) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        PreHirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            replace_var_in_expr(base, name, replacement);
        }
    }
}

pub fn replace_var_in_stmt(stmt: &mut PreHirStmt, name: &str, replacement: &PreHirExpr) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        PreHirStmt::VaStart { va_list, .. } => replace_var_in_expr(va_list, name, replacement),
        PreHirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
        PreHirStmt::Block(stmts) => {
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::While { cond, body } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::DoWhile { body, cond } => {
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
            replace_var_in_expr(cond, name, replacement);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, name, replacement);
            for case in cases {
                for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body) {
                    replace_var_in_stmt(stmt, name, replacement);
                }
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init_stmt) = init {
                replace_var_in_stmt(init_stmt, name, replacement);
            }
            if let Some(cond) = cond {
                replace_var_in_expr(cond, name, replacement);
            }
            if let Some(update_stmt) = update {
                replace_var_in_stmt(update_stmt, name, replacement);
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

/// Every variable a statement assigns, with the count `count_var_defs_stmt`
/// would report -- one traversal for all names instead of one per name.
///
/// Mirrors that function arm for arm; only `Assign` to a plain `Var` counts,
/// and nested bodies sum.
pub fn collect_stmt_var_defs<'a>(stmt: &'a PreHirStmt, out: &mut crate::HashMap<&'a str, usize>) {
    // DAG, not a tree -- see [`crate::stmt_dag`]. Definition counts
    // accumulate, so adding a cached sub-count matches the inline walk.
    let mut memo: crate::stmt_dag::StmtMemo<crate::HashMap<&'a str, usize>> = Default::default();
    collect_stmt_var_defs_memo(stmt, out, &mut memo);
}

fn collect_stmt_var_defs_memo<'a>(
    stmt: &'a PreHirStmt,
    out: &mut crate::HashMap<&'a str, usize>,
    memo: &mut crate::stmt_dag::StmtMemo<crate::HashMap<&'a str, usize>>,
) {
    let key = crate::stmt_dag::stmt_key(stmt);
    if let Some(cached) = memo.get(&key) {
        for (name, n) in cached {
            *out.entry(name).or_insert(0) += n;
        }
        return;
    }
    let mut local: crate::HashMap<&'a str, usize> = Default::default();
    collect_stmt_var_defs_uncached(stmt, &mut local, memo);
    for (name, n) in &local {
        *out.entry(name).or_insert(0) += n;
    }
    memo.insert(key, local);
}

fn collect_stmt_var_defs_uncached<'a>(
    stmt: &'a PreHirStmt,
    out: &mut crate::HashMap<&'a str, usize>,
    memo: &mut crate::stmt_dag::StmtMemo<crate::HashMap<&'a str, usize>>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, .. } => {
            if let PreHirLValue::Var(name) = lhs {
                *out.entry(name.as_str()).or_insert(0) += 1;
            }
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => {
            for stmt in stmts.iter() {
                collect_stmt_var_defs_memo(stmt, out, memo);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for stmt in cases
                .iter()
                .flat_map(|case| case.body.iter())
                .chain(default.iter())
            {
                collect_stmt_var_defs_memo(stmt, out, memo);
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body.iter().chain(else_body.iter()) {
                collect_stmt_var_defs_memo(stmt, out, memo);
            }
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            for stmt in init.iter() {
                collect_stmt_var_defs_memo(stmt, out, memo);
            }
            for stmt in update.iter() {
                collect_stmt_var_defs_memo(stmt, out, memo);
            }
            for stmt in body.iter() {
                collect_stmt_var_defs_memo(stmt, out, memo);
            }
        }
        _ => {}
    }
}

/// How many times `target` is assigned inside `stmt`.
///
/// The bodies are `Rc`-shared, so this is a DAG walk -- see
/// [`crate::stmt_dag`]. A shared subtree is emitted once per parent that names
/// it, so its definitions count once per parent: the memo caches the subtree's
/// total and adds that total on each visit, which is the sum the naive walk
/// produced, reached in DAG time instead of path time.
pub fn count_var_defs_stmt(stmt: &PreHirStmt, target: &str) -> usize {
    let mut memo: crate::stmt_dag::StmtMemo<usize> = Default::default();
    count_var_defs_stmt_memo(stmt, target, &mut memo)
}

fn count_var_defs_stmt_memo(
    stmt: &PreHirStmt,
    target: &str,
    memo: &mut crate::stmt_dag::StmtMemo<usize>,
) -> usize {
    let key = crate::stmt_dag::stmt_key(stmt);
    if let Some(cached) = memo.get(&key) {
        return *cached;
    }
    let answer = count_var_defs_stmt_uncached(stmt, target, memo);
    memo.insert(key, answer);
    answer
}

fn count_var_defs_stmt_uncached(
    stmt: &PreHirStmt,
    target: &str,
    memo: &mut crate::stmt_dag::StmtMemo<usize>,
) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs, .. } => {
            usize::from(matches!(lhs, PreHirLValue::Var(name) if name == target))
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => stmts
            .iter()
            .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
            .sum(),
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .map(|case| {
                    case.body
                        .iter()
                        .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
                        .sum::<usize>()
                })
                .sum::<usize>()
                + default
                    .iter()
                    .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
                    .sum::<usize>()
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body
                .iter()
                .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
                .sum::<usize>()
                + else_body
                    .iter()
                    .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
                    .sum::<usize>()
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            init.iter()
                .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
                .sum::<usize>()
                + update
                    .iter()
                    .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
                    .sum::<usize>()
                + body
                    .iter()
                    .map(|stmt| count_var_defs_stmt_memo(stmt, target, memo))
                    .sum::<usize>()
        }
        PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => 0,
    }
}

pub fn count_var_reads_expr(expr: &PreHirExpr, name: &str) -> usize {
    match expr {
        PreHirExpr::Var(var)
        | PreHirExpr::AddressOfGlobal(var)
        | PreHirExpr::AddressOfLocal(var) => usize::from(var == name),
        PreHirExpr::Const(_, _) => 0,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => count_var_reads_expr(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_reads_expr(lhs, name) + count_var_reads_expr(rhs, name)
        }
        PreHirExpr::Call { args, .. } => {
            args.iter().map(|arg| count_var_reads_expr(arg, name)).sum()
        }
        PreHirExpr::Index { base, index, .. } => {
            count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_reads_expr(cond, name)
                + count_var_reads_expr(then_expr, name)
                + count_var_reads_expr(else_expr, name)
        }
    }
}

pub fn count_var_reads_lvalue(lhs: &PreHirLValue, name: &str) -> usize {
    match lhs {
        PreHirLValue::Var(_) => 0,
        PreHirLValue::Deref { ptr, .. } => count_var_reads_expr(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => count_var_reads_expr(base, name),
    }
}

/// How many times `name` is read inside `stmt`.
///
/// Memoised over the statement DAG on the same terms as
/// [`count_var_defs_stmt`]; `name` is fixed for the whole walk, so a
/// statement's address identifies its answer.
pub fn count_var_reads_stmt(stmt: &PreHirStmt, name: &str) -> usize {
    let mut memo: crate::stmt_dag::StmtMemo<usize> = Default::default();
    count_var_reads_stmt_memo(stmt, name, &mut memo)
}

fn count_var_reads_stmt_memo(
    stmt: &PreHirStmt,
    name: &str,
    memo: &mut crate::stmt_dag::StmtMemo<usize>,
) -> usize {
    let key = crate::stmt_dag::stmt_key(stmt);
    if let Some(cached) = memo.get(&key) {
        return *cached;
    }
    let answer = count_var_reads_stmt_uncached(stmt, name, memo);
    memo.insert(key, answer);
    answer
}

fn count_var_reads_stmt_uncached(
    stmt: &PreHirStmt,
    name: &str,
    memo: &mut crate::stmt_dag::StmtMemo<usize>,
) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            count_var_reads_lvalue(lhs, name) + count_var_reads_expr(rhs, name)
        }
        PreHirStmt::VaStart { va_list, .. } => count_var_reads_expr(va_list, name),
        PreHirStmt::Expr(expr) => count_var_reads_expr(expr, name),
        PreHirStmt::Block(stmts) | PreHirStmt::While { body: stmts, .. } => stmts
            .iter()
            .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
            .sum(),
        PreHirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                .sum::<usize>()
                + count_var_reads_expr(cond, name)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_reads_expr(expr, name)
                + cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                    .sum::<usize>()
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_reads_expr(cond, name)
                + then_body
                    .iter()
                    .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                    .sum::<usize>()
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.iter()
                .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                .sum::<usize>()
                + cond
                    .as_ref()
                    .map(|expr| count_var_reads_expr(expr, name))
                    .unwrap_or(0)
                + update
                    .iter()
                    .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                    .sum::<usize>()
                + body
                    .iter()
                    .map(|stmt| count_var_reads_stmt_memo(stmt, name, memo))
                    .sum::<usize>()
        }
        PreHirStmt::Return(Some(expr)) => count_var_reads_expr(expr, name),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => 0,
    }
}

#[cfg(test)]
mod dag_walk_tests {
    use super::*;
    use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};
    use std::rc::Rc;

    /// `levels` nested `if`s whose two arms name one shared body -- the shape a
    /// collapse tier produces when it gives both branches the same tail.
    ///
    /// Every walk below returns the answer for the *printed* program, in which
    /// the innermost statement appears `2^levels` times. Twenty levels is a
    /// million appearances stored in twenty-one statements, so a walker that
    /// recurses per path rather than per node does not return from any of
    /// these tests -- which is the regression they exist to catch.
    fn both_arms_share_one_body(levels: usize, leaf: PreHirStmt) -> Vec<PreHirStmt> {
        let mut body = vec![leaf];
        for _ in 0..levels {
            let shared = Rc::new(body);
            body = vec![PreHirStmt::If {
                cond: PreHirExpr::Var("c".to_string()),
                then_body: Rc::clone(&shared),
                else_body: shared,
            }];
        }
        body
    }

    fn assign(lhs: &str, rhs: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs.to_string()),
            rhs: PreHirExpr::Var(rhs.to_string()),
        }
    }

    #[test]
    fn definitions_in_a_shared_body_count_once_per_parent() {
        let body = both_arms_share_one_body(20, assign("x", "y"));
        assert_eq!(count_var_defs_stmt(&body[0], "x"), 1 << 20);
        assert_eq!(count_var_defs_stmt(&body[0], "y"), 0);
    }

    #[test]
    fn reads_in_a_shared_body_count_once_per_parent() {
        let body = both_arms_share_one_body(20, assign("x", "y"));
        assert_eq!(count_var_reads_stmt(&body[0], "y"), 1 << 20);
        // `c` is read by every guard on the way down, and each guard is itself
        // shared, so this is the whole tree of conditions rather than 20.
        assert_eq!(count_var_reads_stmt(&body[0], "c"), (1 << 20) - 1);
    }

    #[test]
    fn a_shared_bodys_definitions_are_indexed_once_per_parent() {
        let body = both_arms_share_one_body(20, assign("x", "y"));
        let mut defs = crate::HashMap::default();
        collect_stmt_var_defs(&body[0], &mut defs);
        assert_eq!(defs.get("x").copied(), Some(1 << 20));
    }
}
