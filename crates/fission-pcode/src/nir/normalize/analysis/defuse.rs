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
use super::super::cleanup::{expr_has_side_effects, prune_unused_temp_bindings};
use super::super::*;
use crate::nir::normalize::analysis::expr_key::pure_expr_key;
use crate::nir::normalize::pipeline::normalize_expr;
use crate::nir::normalize::wave_stats;
use crate::nir::support::{expr_type, next_temp_name};
use std::collections::HashMap;

const WIDE_DEAD_ASSIGNMENT_RERUN_STMT_LIMIT: usize = 220;
const WIDE_DEAD_ASSIGNMENT_RERUN_LOCAL_LIMIT: usize = 160;

// ── DefUseMap ─────────────────────────────────────────────────────────────────

/// Function-level use-count map for named HIR variables.
///
/// Counts every `Var(name)` occurrence that is used as an *rvalue* anywhere in
/// the function body.  LHS variable names in direct Assign statements
/// (`Assign { lhs: Var(_), .. }`) are NOT counted — they are definition sites.
pub(crate) struct DefUseMap {
    /// Number of rvalue uses of each variable name across the whole body.
    pub(crate) use_count: HashMap<String, usize>,
}

impl DefUseMap {
    pub(crate) fn build(stmts: &[HirStmt]) -> Self {
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
            HirStmt::VaStart { va_list, .. } => self.count_expr(va_list),
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
            HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
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
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.count_expr(cond);
                self.count_expr(then_expr);
                self.count_expr(else_expr);
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
pub(crate) fn constant_folding_pass(stmts: &mut Vec<HirStmt>) -> bool {
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
        HirStmt::VaStart { va_list, .. } => changed |= fold_expr(va_list),
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
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= fold_expr(cond);
            changed |= fold_expr(then_expr);
            changed |= fold_expr(else_expr);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
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
pub(crate) fn fold_expr_hir(expr: &mut HirExpr) -> bool {
    fold_expr(expr)
}

/// Evaluate `expr` to a compile-time integer/bool constant using `env` for
/// `Var` bindings.  Returns `None` for `Load`/`Call`/non-constant leaves.
pub(crate) fn eval_hir_expr_with_const_env(
    expr: &HirExpr,
    env: &HashMap<String, (i64, NirType)>,
) -> Option<(i64, NirType)> {
    match expr {
        HirExpr::Const(v, ty) => Some((*v, ty.clone())),
        HirExpr::Var(name) => env.get(name).map(|(v, t)| (*v, t.clone())),
        HirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => {
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
        HirExpr::AddressOfGlobal(_)
        | HirExpr::Load { .. }
        | HirExpr::Call { .. }
        | HirExpr::PtrOffset { .. }
        | HirExpr::Index { .. }
        | HirExpr::Select { .. }
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
        HirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => {
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
        HirBinaryOp::Gt => i64::from((a as u64) > (b as u64)),
        HirBinaryOp::Ge => i64::from((a as u64) >= (b as u64)),
        HirBinaryOp::SLt => i64::from(a < b),
        HirBinaryOp::SLe => i64::from(a <= b),
        HirBinaryOp::SGt => i64::from(a > b),
        HirBinaryOp::SGe => i64::from(a >= b),
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
/// (those with a temp-like origin).  Stack slots and
/// other memory-backed locals must NOT be removed even when their name is never
/// read, because the write itself may be observable through aliased pointers.
pub(crate) fn defuse_dead_assignment_pass(func: &mut HirFunction) -> bool {
    // Collect pure-temp variable names (including builder-preserved temps).
    let mut temp_names: std::collections::HashSet<String> = func
        .locals
        .iter()
        .filter(|b| b.is_temp_like())
        .map(|b| b.name.clone())
        .collect();
    collect_temp_like_assignment_names(&func.body, &mut temp_names);
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
pub(crate) fn apply_wide_dead_assignment_pass(func: &mut HirFunction) -> bool {
    let first_changed = defuse_dead_assignment_pass(func);
    if !first_changed {
        return false;
    }
    if !wide_dead_assignment_rerun_admitted(func) {
        if wide_dead_assignment_rerun_admission_enabled() {
            wave_stats::add_wide_dead_assignment_rerun_skipped_by_admission(1);
        }
        return true;
    }
    if wide_dead_assignment_rerun_admission_enabled() {
        wave_stats::add_wide_dead_assignment_rerun_admitted(1);
    }
    for _ in 1..6 {
        if !defuse_dead_assignment_pass(func) {
            break;
        }
    }
    true
}

fn wide_dead_assignment_rerun_admission_enabled() -> bool {
    std::env::var("FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION")
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes" || normalized == "on"
        })
        .unwrap_or(false)
}

fn wide_dead_assignment_rerun_admitted(func: &HirFunction) -> bool {
    if !wide_dead_assignment_rerun_admission_enabled() {
        return true;
    }
    count_hir_stmts_for_wide_dead_assignment(&func.body) <= WIDE_DEAD_ASSIGNMENT_RERUN_STMT_LIMIT
        && func.locals.len() <= WIDE_DEAD_ASSIGNMENT_RERUN_LOCAL_LIMIT
}

fn count_hir_stmts_for_wide_dead_assignment(stmts: &[HirStmt]) -> usize {
    fn count_stmt(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => {
                1 + count_hir_stmts_for_wide_dead_assignment(stmts)
            }
            HirStmt::Switch { cases, default, .. } => {
                1 + cases
                    .iter()
                    .map(|case| count_hir_stmts_for_wide_dead_assignment(&case.body))
                    .sum::<usize>()
                    + count_hir_stmts_for_wide_dead_assignment(default)
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                1 + count_hir_stmts_for_wide_dead_assignment(then_body)
                    + count_hir_stmts_for_wide_dead_assignment(else_body)
            }
            _ => 1,
        }
    }

    stmts.iter().map(count_stmt).sum()
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
        HirStmt::For {
            init, update, body, ..
        } => {
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

fn collect_temp_like_assignment_names(
    stmts: &[HirStmt],
    names: &mut std::collections::HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } => {
                if is_temp_like_assignment_name(name) {
                    names.insert(name.clone());
                }
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_temp_like_assignment_names(body, names);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_temp_like_assignment_names(std::slice::from_ref(init.as_ref()), names);
                }
                if let Some(update) = update {
                    collect_temp_like_assignment_names(
                        std::slice::from_ref(update.as_ref()),
                        names,
                    );
                }
                collect_temp_like_assignment_names(body, names);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_temp_like_assignment_names(then_body, names);
                collect_temp_like_assignment_names(else_body, names);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_temp_like_assignment_names(&case.body, names);
                }
                collect_temp_like_assignment_names(default, names);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn is_temp_like_assignment_name(name: &str) -> bool {
    name == "result" || name == "retval" || temp_name_suffix(name).is_some()
}

// ── Forward-scan fix helper (used by cleanup.rs callers) ─────────────────────

/// Returns `true` if the forward scan for a single-use temp may skip `stmt`
/// when the variable `name` has ZERO uses inside `stmt`.
///
/// This extends the existing `stmt_allows_forward_scan` logic to pass through
/// loops, switches, and blocks that simply don't mention the variable.
pub(crate) fn can_skip_stmt_for_var(stmt: &HirStmt, name: &str) -> bool {
    count_any_mention_in_stmt(stmt, name) == 0
}

/// Count all occurrences of `name` in a statement (both reads and the LHS).
fn count_any_mention_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_mention_lhs(lhs, name) + count_mention_expr(rhs, name)
        }
        HirStmt::VaStart { va_list, .. } => count_mention_expr(va_list, name),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// Process-wide env is shared across parallel tests; serialize mutations that toggle admission.
    static WIDE_DEAD_RERUN_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct WideDeadRerunAdmissionEnvGuard(MutexGuard<'static, ()>);

    impl WideDeadRerunAdmissionEnvGuard {
        fn set_enabled() -> Self {
            let guard = WIDE_DEAD_RERUN_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            unsafe {
                std::env::set_var("FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION", "1");
            }
            Self(guard)
        }
    }

    impl Drop for WideDeadRerunAdmissionEnvGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("FISSION_ENABLE_WIDE_DEAD_ASSIGNMENT_RERUN_ADMISSION");
            }
        }
    }

    fn temp_binding(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn assign_dead_temp(name: &str, value: i64) -> HirStmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name.to_string()),
            rhs: HirExpr::Const(
                value,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        }
    }

    fn test_func(stmt_count: usize, local_count: usize) -> HirFunction {
        let body = (0..stmt_count)
            .map(|idx| assign_dead_temp(&format!("xVar{idx}"), idx as i64))
            .collect();
        let locals = (0..local_count)
            .map(|idx| temp_binding(&format!("xVar{idx}")))
            .collect();
        HirFunction {
            name: "wide_dead_assignment_test".to_string(),
            params: Vec::new(),
            locals,
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        }
    }

    #[test]
    fn wide_dead_assignment_rerun_admission_allows_small_function() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let func = test_func(10, 10);
        assert!(wide_dead_assignment_rerun_admitted(&func));
    }

    #[test]
    fn wide_dead_assignment_rerun_admission_skips_large_stmt_budget() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let func = test_func(221, 10);
        assert!(!wide_dead_assignment_rerun_admitted(&func));
    }

    #[test]
    fn wide_dead_assignment_rerun_admission_skips_large_local_budget() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let func = test_func(10, 161);
        assert!(!wide_dead_assignment_rerun_admitted(&func));
    }

    #[test]
    fn wide_dead_assignment_first_pass_still_runs_when_admission_skips() {
        let _env = WideDeadRerunAdmissionEnvGuard::set_enabled();
        let mut func = test_func(221, 221);
        assert!(apply_wide_dead_assignment_pass(&mut func));
        assert!(func.body.is_empty());
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
        HirExpr::Var(n) | HirExpr::AddressOfGlobal(n) => usize::from(n.as_str() == name),
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
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_mention_expr(cond, name)
                + count_mention_expr(then_expr, name)
                + count_mention_expr(else_expr, name)
        }
    }
}

pub(crate) fn stabilize_repeated_pure_exprs(func: &mut HirFunction) -> usize {
    let mut next_temp_id = next_temp_name_seed(&func.locals);
    stabilize_repeated_pure_exprs_in_stmts(&mut func.body, &mut func.locals, &mut next_temp_id)
}

fn stabilize_repeated_pure_exprs_in_stmts(
    stmts: &mut Vec<HirStmt>,
    locals: &mut Vec<NirBinding>,
    next_temp_id: &mut u32,
) -> usize {
    let mut changed = 0usize;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for mut stmt in stmts.drain(..) {
        match &mut stmt {
            HirStmt::Block(body) => {
                changed += stabilize_repeated_pure_exprs_in_stmts(body, locals, next_temp_id);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed += stabilize_repeated_pure_exprs_in_stmts(then_body, locals, next_temp_id);
                changed += stabilize_repeated_pure_exprs_in_stmts(else_body, locals, next_temp_id);
                if let Some((temp_stmt, stabilized_cond)) =
                    stabilize_expr_with_temp(cond, locals, next_temp_id)
                {
                    rewritten.push(temp_stmt);
                    *cond = stabilized_cond;
                    changed += 1;
                }
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                for case in cases.iter_mut() {
                    changed += stabilize_repeated_pure_exprs_in_stmts(
                        &mut case.body,
                        locals,
                        next_temp_id,
                    );
                }
                changed += stabilize_repeated_pure_exprs_in_stmts(default, locals, next_temp_id);
                if let Some((temp_stmt, stabilized_expr)) =
                    stabilize_expr_with_temp(expr, locals, next_temp_id)
                {
                    rewritten.push(temp_stmt);
                    *expr = stabilized_expr;
                    changed += 1;
                }
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                if let Some((temp_stmt, stabilized_expr)) =
                    stabilize_expr_with_temp(expr, locals, next_temp_id)
                {
                    rewritten.push(temp_stmt);
                    *expr = stabilized_expr;
                    changed += 1;
                }
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed += stabilize_repeated_pure_exprs_in_stmts(body, locals, next_temp_id);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init.as_mut() {
                    let mut nested = vec![(*init.clone())];
                    changed +=
                        stabilize_repeated_pure_exprs_in_stmts(&mut nested, locals, next_temp_id);
                    if let Some(updated) = nested.into_iter().next() {
                        *init = Box::new(updated);
                    }
                }
                if let Some(update) = update.as_mut() {
                    let mut nested = vec![(*update.clone())];
                    changed +=
                        stabilize_repeated_pure_exprs_in_stmts(&mut nested, locals, next_temp_id);
                    if let Some(updated) = nested.into_iter().next() {
                        *update = Box::new(updated);
                    }
                }
                changed += stabilize_repeated_pure_exprs_in_stmts(body, locals, next_temp_id);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue
            | HirStmt::Label(_)
            | HirStmt::Goto(_) => {}
        }
        rewritten.push(stmt);
    }

    *stmts = rewritten;
    changed
}

fn stabilize_expr_with_temp(
    expr: &HirExpr,
    locals: &mut Vec<NirBinding>,
    next_temp_id: &mut u32,
) -> Option<(HirStmt, HirExpr)> {
    let best = best_repeated_pure_expr(expr)?;
    let temp_ty = expr_type(&best);
    let temp_name = next_temp_name(&temp_ty, next_temp_id);
    locals.push(NirBinding {
        name: temp_name.clone(),
        ty: temp_ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    });
    let mut temp_rhs = best.clone();
    normalize_expr(&mut temp_rhs);
    let replacement = HirExpr::Var(temp_name.clone());
    let mut stabilized_expr = replace_matching_pure_expr(expr, &best, &replacement);
    normalize_expr(&mut stabilized_expr);
    Some((
        HirStmt::Assign {
            lhs: HirLValue::Var(temp_name),
            rhs: temp_rhs,
        },
        stabilized_expr,
    ))
}

fn best_repeated_pure_expr(expr: &HirExpr) -> Option<HirExpr> {
    let mut counts: HashMap<String, (usize, usize, HirExpr)> = HashMap::new();
    collect_repeated_pure_exprs(expr, &mut counts);
    let mut candidates = counts
        .into_values()
        .filter(|(count, nodes, repr)| {
            *count > 1
                && *nodes >= 3
                && count_nonconst_leaf_inputs(repr) >= 2
                && is_stabilization_candidate_expr(repr)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.0.cmp(&a.0))
            .then_with(|| print_expr(&a.2).cmp(&print_expr(&b.2)))
    });
    candidates.into_iter().next().map(|(_, _, expr)| expr)
}

fn collect_repeated_pure_exprs(
    expr: &HirExpr,
    counts: &mut HashMap<String, (usize, usize, HirExpr)>,
) {
    if let Some(key) = pure_expr_key(expr) {
        let nodes = expr_node_count(expr);
        let entry = counts
            .entry(key)
            .or_insert_with(|| (0, nodes, expr.clone()));
        entry.0 += 1;
        if nodes > entry.1 {
            entry.1 = nodes;
            entry.2 = expr.clone();
        }
    }

    match expr {
        HirExpr::Const(_, _) | HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => collect_repeated_pure_exprs(expr, counts),
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_repeated_pure_exprs(lhs, counts);
            collect_repeated_pure_exprs(rhs, counts);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_repeated_pure_exprs(arg, counts);
            }
        }
        HirExpr::Index { base, index, .. } => {
            collect_repeated_pure_exprs(base, counts);
            collect_repeated_pure_exprs(index, counts);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_repeated_pure_exprs(cond, counts);
            collect_repeated_pure_exprs(then_expr, counts);
            collect_repeated_pure_exprs(else_expr, counts);
        }
    }
}

fn replace_matching_pure_expr(expr: &HirExpr, needle: &HirExpr, replacement: &HirExpr) -> HirExpr {
    if pure_expr_key(expr) == pure_expr_key(needle) {
        return replacement.clone();
    }

    match expr {
        HirExpr::Const(_, _) | HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => expr.clone(),
        HirExpr::Cast { ty, expr: inner } => HirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(replace_matching_pure_expr(inner, needle, replacement)),
        },
        HirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => HirExpr::Unary {
            op: *op,
            expr: Box::new(replace_matching_pure_expr(inner, needle, replacement)),
            ty: ty.clone(),
        },
        HirExpr::Binary { op, lhs, rhs, ty } => HirExpr::Binary {
            op: *op,
            lhs: Box::new(replace_matching_pure_expr(lhs, needle, replacement)),
            rhs: Box::new(replace_matching_pure_expr(rhs, needle, replacement)),
            ty: ty.clone(),
        },
        HirExpr::Call { target, args, ty } => HirExpr::Call {
            target: target.clone(),
            args: args
                .iter()
                .map(|arg| replace_matching_pure_expr(arg, needle, replacement))
                .collect(),
            ty: ty.clone(),
        },
        HirExpr::Load { ptr, ty } => HirExpr::Load {
            ptr: Box::new(replace_matching_pure_expr(ptr, needle, replacement)),
            ty: ty.clone(),
        },
        HirExpr::PtrOffset { base, offset } => HirExpr::PtrOffset {
            base: Box::new(replace_matching_pure_expr(base, needle, replacement)),
            offset: *offset,
        },
        HirExpr::AggregateCopy { src, size } => HirExpr::AggregateCopy {
            src: Box::new(replace_matching_pure_expr(src, needle, replacement)),
            size: *size,
        },
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => HirExpr::Index {
            base: Box::new(replace_matching_pure_expr(base, needle, replacement)),
            index: Box::new(replace_matching_pure_expr(index, needle, replacement)),
            elem_ty: elem_ty.clone(),
        },
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => HirExpr::Select {
            cond: Box::new(replace_matching_pure_expr(cond, needle, replacement)),
            then_expr: Box::new(replace_matching_pure_expr(then_expr, needle, replacement)),
            else_expr: Box::new(replace_matching_pure_expr(else_expr, needle, replacement)),
            ty: ty.clone(),
        },
    }
}

fn is_stabilization_candidate_expr(expr: &HirExpr) -> bool {
    matches!(
        expr,
        HirExpr::Binary {
            op: HirBinaryOp::Add
                | HirBinaryOp::Sub
                | HirBinaryOp::Mul
                | HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Xor
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
                | HirBinaryOp::Shl
                | HirBinaryOp::Shr
                | HirBinaryOp::Sar,
            ..
        } | HirExpr::Unary { .. }
            | HirExpr::Cast { .. }
    )
}
fn count_nonconst_leaf_inputs(expr: &HirExpr) -> usize {
    match expr {
        HirExpr::Const(_, _) => 0,
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => 1,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => count_nonconst_leaf_inputs(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_nonconst_leaf_inputs(lhs) + count_nonconst_leaf_inputs(rhs)
        }
        HirExpr::Call { args, .. } => args.iter().map(count_nonconst_leaf_inputs).sum(),
        HirExpr::Index { base, index, .. } => {
            count_nonconst_leaf_inputs(base) + count_nonconst_leaf_inputs(index)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_nonconst_leaf_inputs(cond)
                + count_nonconst_leaf_inputs(then_expr)
                + count_nonconst_leaf_inputs(else_expr)
        }
    }
}

fn expr_node_count(expr: &HirExpr) -> usize {
    match expr {
        HirExpr::Const(_, _) | HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => 1,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => 1 + expr_node_count(expr),
        HirExpr::Binary { lhs, rhs, .. } => 1 + expr_node_count(lhs) + expr_node_count(rhs),
        HirExpr::Call { args, .. } => 1 + args.iter().map(expr_node_count).sum::<usize>(),
        HirExpr::Index { base, index, .. } => 1 + expr_node_count(base) + expr_node_count(index),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => 1 + expr_node_count(cond) + expr_node_count(then_expr) + expr_node_count(else_expr),
    }
}

fn next_temp_name_seed(locals: &[NirBinding]) -> u32 {
    locals
        .iter()
        .filter_map(|binding| temp_name_suffix(&binding.name))
        .max()
        .map_or(0, |suffix| suffix.saturating_add(1))
}

fn temp_name_suffix(name: &str) -> Option<u32> {
    let digit_start = name.find(|ch: char| ch.is_ascii_digit())?;
    let prefix = &name[..digit_start];
    matches!(prefix, "bVar" | "iVar" | "uVar" | "xVar")
        .then(|| name[digit_start..].parse::<u32>().ok())
        .flatten()
}
