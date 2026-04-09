/// HIR-level def-use analysis and dataflow-based normalization passes.
///
/// These passes extend the existing name-pattern-based cleanup in `cleanup.rs`
/// with proper graph-theoretic analysis:
///
/// - [`DefUseMap`] counts every read of a named variable across the ENTIRE
///   function body (all nesting levels), without any name-pattern assumption.
/// - [`constant_folding_pass`] evaluates binary and unary expressions whose
///   operands are both compile-time constants.  Pure algebra, binary-independent.
/// - [`defuse_dead_assignment_pass`] removes flat-level assignments to any
///   variable whose use count is zero in the whole function body and whose
///   RHS has no observable side effects.
use super::cleanup::{expr_has_side_effects, prune_unused_temp_bindings};
use super::*;
use std::collections::HashMap;

// ── DefUseMap ─────────────────────────────────────────────────────────────────

/// Function-level use-count map for named HIR variables.
///
/// Counts every `Var(name)` occurrence that is used as an *rvalue* anywhere in
/// the function body.  LHS variable names in direct Assign statements
/// (`Assign { lhs: Var(_), .. }`) are NOT counted — they are definition sites.
pub(super) struct DefUseMap {
    /// Number of rvalue uses of each variable name across the whole body.
    pub(super) use_count: HashMap<String, usize>,
}

impl DefUseMap {
    pub(super) fn build(stmts: &[HirStmt]) -> Self {
        let mut map = Self {
            use_count: HashMap::new(),
        };
        for stmt in stmts {
            map.count_stmt(stmt);
        }
        map
    }

    fn count_stmt(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                self.count_lvalue(lhs);
                self.count_expr(rhs);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => self.count_expr(expr),
            HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue
            | HirStmt::Label(_)
            | HirStmt::Goto(_) => {}
            HirStmt::Block(stmts) => {
                for s in stmts {
                    self.count_stmt(s);
                }
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.count_expr(cond);
                for s in then_body {
                    self.count_stmt(s);
                }
                for s in else_body {
                    self.count_stmt(s);
                }
            }
            HirStmt::While { cond, body } => {
                self.count_expr(cond);
                for s in body {
                    self.count_stmt(s);
                }
            }
            HirStmt::DoWhile { body, cond } => {
                for s in body {
                    self.count_stmt(s);
                }
                self.count_expr(cond);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.count_stmt(i);
                }
                if let Some(c) = cond {
                    self.count_expr(c);
                }
                if let Some(u) = update {
                    self.count_stmt(u);
                }
                for s in body {
                    self.count_stmt(s);
                }
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.count_expr(expr);
                for case in cases {
                    for s in &case.body {
                        self.count_stmt(s);
                    }
                }
                for s in default {
                    self.count_stmt(s);
                }
            }
        }
    }

    fn count_lvalue(&mut self, lhs: &HirLValue) {
        match lhs {
            // The defined name is a write site — not an rvalue use.
            HirLValue::Var(_) => {}
            HirLValue::Deref { ptr, .. } => self.count_expr(ptr),
            HirLValue::Index { base, index, .. } => {
                self.count_expr(base);
                self.count_expr(index);
            }
        }
    }

    fn count_expr(&mut self, expr: &HirExpr) {
        match expr {
            HirExpr::Var(name) => {
                *self.use_count.entry(name.clone()).or_default() += 1;
            }
            HirExpr::Const(_, _) => {}
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => self.count_expr(expr),
            HirExpr::Binary { lhs, rhs, .. } => {
                self.count_expr(lhs);
                self.count_expr(rhs);
            }
            HirExpr::Call { args, .. } => {
                for a in args {
                    self.count_expr(a);
                }
            }
            HirExpr::Index { base, index, .. } => {
                self.count_expr(base);
                self.count_expr(index);
            }
        }
    }
}

// ── Constant folding ──────────────────────────────────────────────────────────

/// Evaluate binary/unary/cast expressions whose operands are compile-time
/// constants.  Returns `true` if any rewrite was made.
///
/// Rules (all binary-independent, pure algebra):
/// - `Binary(op, Const(a), Const(b)) → Const(eval(op,a,b))`
/// - `Unary(Neg, Const(a)) → Const(-a)`, `Unary(Not|BitNot, Const(a)) → Const(~a)`
/// - `Cast(IntN, Const(a)) → Const(a & mask_N)`
///
/// Overflow uses wrapping arithmetic to match x86 semantics.
pub(super) fn constant_folding_pass(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= fold_stmt(stmt);
    }
    changed
}

fn fold_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            fold_lvalue(lhs);
            changed |= fold_expr(rhs);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => changed |= fold_expr(expr),
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => {}
        HirStmt::Block(stmts) => changed |= constant_folding_pass(stmts),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= fold_expr(cond);
            changed |= constant_folding_pass(then_body);
            changed |= constant_folding_pass(else_body);
        }
        HirStmt::While { cond, body } => {
            changed |= fold_expr(cond);
            changed |= constant_folding_pass(body);
        }
        HirStmt::DoWhile { body, cond } => {
            changed |= constant_folding_pass(body);
            changed |= fold_expr(cond);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= fold_stmt(i);
            }
            if let Some(c) = cond {
                changed |= fold_expr(c);
            }
            if let Some(u) = update {
                changed |= fold_stmt(u);
            }
            changed |= constant_folding_pass(body);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= fold_expr(expr);
            for case in cases.iter_mut() {
                changed |= constant_folding_pass(&mut case.body);
            }
            changed |= constant_folding_pass(default);
        }
    }
    changed
}

fn fold_lvalue(lhs: &mut HirLValue) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => {
            fold_expr(ptr);
        }
        HirLValue::Index { base, index, .. } => {
            fold_expr(base);
            fold_expr(index);
        }
    }
}

/// Recursively fold constant sub-expressions bottom-up.
fn fold_expr(expr: &mut HirExpr) -> bool {
    // Fold children first.
    let mut changed = false;
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= fold_expr(lhs);
            changed |= fold_expr(rhs);
        }
        HirExpr::Unary { expr: inner, .. } | HirExpr::Cast { expr: inner, .. } => {
            changed |= fold_expr(inner);
        }
        HirExpr::Load { ptr, .. } | HirExpr::PtrOffset { base: ptr, .. } => {
            changed |= fold_expr(ptr);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= fold_expr(base);
            changed |= fold_expr(index);
        }
        HirExpr::AggregateCopy { src, .. } => {
            changed |= fold_expr(src);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                changed |= fold_expr(a);
            }
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
    // Try to fold this node.
    if let Some(folded) = try_fold(expr) {
        *expr = folded;
        changed = true;
    }
    changed
}

/// Expose bottom-up constant folding for passes that rewrite expressions in place
/// (e.g. SCCP after substituting known variables).
pub(super) fn fold_expr_hir(expr: &mut HirExpr) -> bool {
    fold_expr(expr)
}

/// Evaluate `expr` to a compile-time integer/bool constant using `env` for
/// `Var` bindings.  Returns `None` for `Load`/`Call`/non-constant leaves.
pub(super) fn eval_hir_expr_with_const_env(
    expr: &HirExpr,
    env: &HashMap<String, (i64, NirType)>,
) -> Option<(i64, NirType)> {
    match expr {
        HirExpr::Const(v, ty) => Some((*v, ty.clone())),
        HirExpr::Var(name) => env.get(name).map(|(v, t)| (*v, t.clone())),
        HirExpr::Unary { op, expr: inner, ty } => {
            let (a, _) = eval_hir_expr_with_const_env(inner, env)?;
            let result = eval_unary(*op, a, ty)?;
            Some((result, ty.clone()))
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let (a, _) = eval_hir_expr_with_const_env(lhs, env)?;
            let (b, _) = eval_hir_expr_with_const_env(rhs, env)?;
            let result = eval_binary(*op, a, b, ty)?;
            Some((result, ty.clone()))
        }
        HirExpr::Cast { ty, expr: inner } => {
            let (a, _) = eval_hir_expr_with_const_env(inner, env)?;
            let result = truncate_const(a, ty)?;
            Some((result, ty.clone()))
        }
        HirExpr::Load { .. }
        | HirExpr::Call { .. }
        | HirExpr::PtrOffset { .. }
        | HirExpr::Index { .. }
        | HirExpr::AggregateCopy { .. } => None,
    }
}

fn try_fold(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let HirExpr::Const(a, _) = lhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(b, _) = rhs.as_ref() else {
                return None;
            };
            let result = eval_binary(*op, *a, *b, ty)?;
            Some(HirExpr::Const(result, ty.clone()))
        }
        HirExpr::Unary { op, expr: inner, ty } => {
            let HirExpr::Const(a, _) = inner.as_ref() else {
                return None;
            };
            let result = eval_unary(*op, *a, ty)?;
            Some(HirExpr::Const(result, ty.clone()))
        }
        HirExpr::Cast { ty, expr: inner } => {
            let HirExpr::Const(a, _) = inner.as_ref() else {
                return None;
            };
            let result = truncate_const(*a, ty)?;
            Some(HirExpr::Const(result, ty.clone()))
        }
        _ => None,
    }
}

fn eval_binary(op: HirBinaryOp, a: i64, b: i64, ty: &NirType) -> Option<i64> {
    let bits = int_or_bool_bits(ty)?;
    let result: i64 = match op {
        HirBinaryOp::Add => a.wrapping_add(b),
        HirBinaryOp::Sub => a.wrapping_sub(b),
        HirBinaryOp::Mul => a.wrapping_mul(b),
        HirBinaryOp::And => a & b,
        HirBinaryOp::Or => a | b,
        HirBinaryOp::Xor => a ^ b,
        HirBinaryOp::LogicalAnd => i64::from((a != 0) && (b != 0)),
        HirBinaryOp::LogicalOr => i64::from((a != 0) || (b != 0)),
        HirBinaryOp::Shl => {
            if b < 0 || b >= 64 {
                return None;
            }
            a.wrapping_shl(b as u32)
        }
        HirBinaryOp::Shr => {
            if b < 0 || b >= 64 {
                return None;
            }
            ((a as u64).wrapping_shr(b as u32)) as i64
        }
        HirBinaryOp::Sar => {
            if b < 0 || b >= 64 {
                return None;
            }
            a.wrapping_shr(b as u32)
        }
        HirBinaryOp::Eq => i64::from(a == b),
        HirBinaryOp::Ne => i64::from(a != b),
        HirBinaryOp::Lt => i64::from((a as u64) < (b as u64)),
        HirBinaryOp::Le => i64::from((a as u64) <= (b as u64)),
        HirBinaryOp::SLt => i64::from(a < b),
        HirBinaryOp::SLe => i64::from(a <= b),
        HirBinaryOp::Div => {
            let bu = b as u64;
            if bu == 0 {
                return None;
            }
            ((a as u64).wrapping_div(bu)) as i64
        }
        HirBinaryOp::Mod => {
            let bu = b as u64;
            if bu == 0 {
                return None;
            }
            ((a as u64).wrapping_rem(bu)) as i64
        }
    };
    Some(mask_to_bits(result, bits))
}

fn eval_unary(op: HirUnaryOp, a: i64, ty: &NirType) -> Option<i64> {
    let bits = int_or_bool_bits(ty)?;
    let result = match op {
        HirUnaryOp::Neg => a.wrapping_neg(),
        HirUnaryOp::Not => i64::from(a == 0),
        HirUnaryOp::BitNot => !a,
    };
    Some(mask_to_bits(result, bits))
}

fn truncate_const(a: i64, ty: &NirType) -> Option<i64> {
    let bits = int_or_bool_bits(ty)?;
    Some(mask_to_bits(a, bits))
}

fn int_or_bool_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

/// Truncate an i64 to the lower `bits` bits using the i64 sign-bit convention
/// used throughout the HIR constant representation.
fn mask_to_bits(value: i64, bits: u32) -> i64 {
    if bits == 0 || bits > 63 {
        return value;
    }
    let mask = (1_i64 << bits).wrapping_sub(1);
    value & mask
}

// ── Dead assignment pass ──────────────────────────────────────────────────────

/// Remove assignments `name = rhs` at any level of the body where
/// `use_count[name] == 0` (never read anywhere in the whole function) and the
/// RHS has no side effects.
///
/// This generalises [`super::cleanup::eliminate_dead_temp_assigns`] to ALL
/// variable names — not only trivially-named temps — using a function-level
/// traversal instead of a flat per-stmt-list scan.
///
/// Safety restriction: only removes assignments to **pure temporary** bindings
/// (those with `origin == Some(NirBindingOrigin::Temp)`).  Stack slots and
/// other memory-backed locals must NOT be removed even when their name is never
/// read, because the write itself may be observable through aliased pointers.
pub(super) fn defuse_dead_assignment_pass(func: &mut HirFunction) -> bool {
    // Collect pure-temp variable names (Temp origin only).
    let temp_names: std::collections::HashSet<String> = func
        .locals
        .iter()
        .filter(|b| matches!(b.origin, Some(NirBindingOrigin::Temp)))
        .map(|b| b.name.clone())
        .collect();
    if temp_names.is_empty() {
        return false;
    }

    let map = DefUseMap::build(&func.body);
    let mut changed = false;
    remove_dead_in_stmts(&mut func.body, &map, &temp_names, &mut changed);
    if changed {
        // Remove temp bindings that became unreferenced.
        prune_unused_temp_bindings(func);
    }
    changed
}

/// Fixed-point dead temp removal: run [`defuse_dead_assignment_pass`] until it
/// quiesces or the iteration budget is hit.  Intended after SCCP exposes temps
/// whose only uses were folded away across the function.
pub(super) fn apply_wide_dead_assignment_pass(func: &mut HirFunction) -> bool {
    let mut any = false;
    for _ in 0..6 {
        if !defuse_dead_assignment_pass(func) {
            break;
        }
        any = true;
    }
    any
}

fn remove_dead_in_stmts(
    stmts: &mut Vec<HirStmt>,
    map: &DefUseMap,
    temp_names: &std::collections::HashSet<String>,
    changed: &mut bool,
) {
    // First recurse into nested bodies.
    for stmt in stmts.iter_mut() {
        remove_dead_in_stmt_nested(stmt, map, temp_names, changed);
    }

    // Then remove flat-level dead assignments to pure temps.
    stmts.retain(|stmt| {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = stmt
        {
            if temp_names.contains(name.as_str()) {
                let uses = map.use_count.get(name.as_str()).copied().unwrap_or(0);
                if uses == 0 && !expr_has_side_effects(rhs) {
                    *changed = true;
                    return false;
                }
            }
        }
        true
    });
}

fn remove_dead_in_stmt_nested(
    stmt: &mut HirStmt,
    map: &DefUseMap,
    temp_names: &std::collections::HashSet<String>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Block(stmts) => remove_dead_in_stmts(stmts, map, temp_names, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_dead_in_stmts(then_body, map, temp_names, changed);
            remove_dead_in_stmts(else_body, map, temp_names, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            remove_dead_in_stmts(body, map, temp_names, changed);
        }
        HirStmt::For { init, update, body, .. } => {
            if let Some(i) = init {
                remove_dead_in_stmt_nested(i, map, temp_names, changed);
            }
            if let Some(u) = update {
                remove_dead_in_stmt_nested(u, map, temp_names, changed);
            }
            remove_dead_in_stmts(body, map, temp_names, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_dead_in_stmts(&mut case.body, map, temp_names, changed);
            }
            remove_dead_in_stmts(default, map, temp_names, changed);
        }
        _ => {}
    }
}

// ── Forward-scan fix helper (used by cleanup.rs callers) ─────────────────────

/// Returns `true` if the forward scan for a single-use temp may skip `stmt`
/// when the variable `name` has ZERO uses inside `stmt`.
///
/// This extends the existing `stmt_allows_forward_scan` logic to pass through
/// loops, switches, and blocks that simply don't mention the variable.
pub(super) fn can_skip_stmt_for_var(stmt: &HirStmt, name: &str) -> bool {
    count_any_mention_in_stmt(stmt, name) == 0
}

/// Count all occurrences of `name` in a statement (both reads and the LHS).
fn count_any_mention_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_mention_lhs(lhs, name) + count_mention_expr(rhs, name)
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => count_mention_expr(expr, name),
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => 0,
        HirStmt::Block(stmts) => stmts
            .iter()
            .map(|s| count_any_mention_in_stmt(s, name))
            .sum(),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_mention_expr(cond, name)
                + then_body
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
        }
        HirStmt::While { cond, body } => {
            count_mention_expr(cond, name)
                + body
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
        }
        HirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|s| count_any_mention_in_stmt(s, name))
                .sum::<usize>()
                + count_mention_expr(cond, name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let mut total = 0;
            if let Some(i) = init {
                total += count_any_mention_in_stmt(i, name);
            }
            if let Some(c) = cond {
                total += count_mention_expr(c, name);
            }
            if let Some(u) = update {
                total += count_any_mention_in_stmt(u, name);
            }
            total += body
                .iter()
                .map(|s| count_any_mention_in_stmt(s, name))
                .sum::<usize>();
            total
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_mention_expr(expr, name)
                + cases
                    .iter()
                    .map(|c| {
                        c.body
                            .iter()
                            .map(|s| count_any_mention_in_stmt(s, name))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|s| count_any_mention_in_stmt(s, name))
                    .sum::<usize>()
        }
    }
}

fn count_mention_lhs(lhs: &HirLValue, name: &str) -> usize {
    match lhs {
        // The direct write to name counts as a mention (redefinition guard).
        HirLValue::Var(n) => usize::from(n == name),
        HirLValue::Deref { ptr, .. } => count_mention_expr(ptr, name),
        HirLValue::Index { base, index, .. } => {
            count_mention_expr(base, name) + count_mention_expr(index, name)
        }
    }
}

fn count_mention_expr(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(n) => usize::from(n.as_str() == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => count_mention_expr(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_mention_expr(lhs, name) + count_mention_expr(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_mention_expr(a, name)).sum(),
        HirExpr::Index { base, index, .. } => {
            count_mention_expr(base, name) + count_mention_expr(index, name)
        }
    }
}
