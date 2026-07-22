use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Scans the structured HIR and propagates constant values within branch scopes
/// where they are constrained by conditions (e.g., `if (x == 5) { ... }`).
///
/// Also tracks relational branch constraints (`var CMP const`) as per-variable
/// signed/unsigned intervals. A nested `if` condition on the same, unmodified
/// variable that is fully decided by dominating conditions folds to a constant;
/// the cleanup family (`simplify_empty_and_constant_ifs`) then removes the dead
/// arm. This is what erases re-tests left behind by duplicated join tails.
pub fn apply_conditional_const_pass(func: &mut DirFunction) -> bool {
    let mut binding_types = HashMap::default();
    for local in &func.locals {
        binding_types.insert(local.name.clone(), local.ty.clone());
    }
    for param in &func.params {
        binding_types.insert(param.name.clone(), param.ty.clone());
    }

    let mut env = HashMap::default();
    let mut ranges = RangeEnv::default();
    visit_stmts(&mut func.body, &mut env, &mut ranges, &binding_types)
}

// ── Relational interval domain ───────────────────────────────────────────────

type RangeEnv = HashMap<String, VarRanges>;

/// Inclusive interval in i128 space (wide enough for u64 and i64 endpoints).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Interval {
    lo: i128,
    hi: i128,
}

/// Per-variable constraints, kept separately for the signed and unsigned
/// comparison families. Each slot records the operand bit-width the constraint
/// was stated at; a query only consults a slot of the same width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct VarRanges {
    signed: Option<(u32, Interval)>,
    unsigned: Option<(u32, Interval)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CmpKind {
    SLt,
    SLe,
    SGt,
    SGe,
    ULt,
    ULe,
    UGt,
    UGe,
    Eq,
    Ne,
}

fn cmp_kind(op: DirBinaryOp) -> Option<CmpKind> {
    Some(match op {
        DirBinaryOp::SLt => CmpKind::SLt,
        DirBinaryOp::SLe => CmpKind::SLe,
        DirBinaryOp::SGt => CmpKind::SGt,
        DirBinaryOp::SGe => CmpKind::SGe,
        DirBinaryOp::Lt => CmpKind::ULt,
        DirBinaryOp::Le => CmpKind::ULe,
        DirBinaryOp::Gt => CmpKind::UGt,
        DirBinaryOp::Ge => CmpKind::UGe,
        DirBinaryOp::Eq => CmpKind::Eq,
        DirBinaryOp::Ne => CmpKind::Ne,
        _ => return None,
    })
}

/// Mirror the comparison across sides: `const CMP var` → `var CMP' const`.
fn flip_cmp(kind: CmpKind) -> CmpKind {
    match kind {
        CmpKind::SLt => CmpKind::SGt,
        CmpKind::SLe => CmpKind::SGe,
        CmpKind::SGt => CmpKind::SLt,
        CmpKind::SGe => CmpKind::SLe,
        CmpKind::ULt => CmpKind::UGt,
        CmpKind::ULe => CmpKind::UGe,
        CmpKind::UGt => CmpKind::ULt,
        CmpKind::UGe => CmpKind::ULe,
        CmpKind::Eq => CmpKind::Eq,
        CmpKind::Ne => CmpKind::Ne,
    }
}

/// Logical negation for the else-branch constraint.
fn negate_cmp(kind: CmpKind) -> CmpKind {
    match kind {
        CmpKind::SLt => CmpKind::SGe,
        CmpKind::SLe => CmpKind::SGt,
        CmpKind::SGt => CmpKind::SLe,
        CmpKind::SGe => CmpKind::SLt,
        CmpKind::ULt => CmpKind::UGe,
        CmpKind::ULe => CmpKind::UGt,
        CmpKind::UGt => CmpKind::ULe,
        CmpKind::UGe => CmpKind::ULt,
        CmpKind::Eq => CmpKind::Ne,
        CmpKind::Ne => CmpKind::Eq,
    }
}

fn int_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

fn sext_const(value: i64, bits: u32) -> i128 {
    if bits == 0 || bits >= 64 {
        return value as i128;
    }
    let shift = 64 - bits;
    ((value << shift) >> shift) as i128
}

fn zext_const(value: i64, bits: u32) -> i128 {
    if bits == 0 || bits >= 64 {
        return (value as u64) as i128;
    }
    ((value as u64) & ((1u64 << bits) - 1)) as i128
}

fn signed_domain(bits: u32) -> Interval {
    let w = bits.min(64);
    if w == 0 || w >= 64 {
        return Interval {
            lo: i64::MIN as i128,
            hi: i64::MAX as i128,
        };
    }
    Interval {
        lo: -(1_i128 << (w - 1)),
        hi: (1_i128 << (w - 1)) - 1,
    }
}

fn unsigned_domain(bits: u32) -> Interval {
    let w = bits.min(64);
    if w == 0 || w >= 64 {
        return Interval {
            lo: 0,
            hi: u64::MAX as i128,
        };
    }
    Interval {
        lo: 0,
        hi: (1_i128 << w) - 1,
    }
}

/// Decompose `var CMP const` (either operand order) into a var-on-left form.
fn split_var_const_cmp(cond: &DirExpr) -> Option<(&str, CmpKind, i64, u32)> {
    let DirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    let kind = cmp_kind(*op)?;
    match (lhs.as_ref(), rhs.as_ref()) {
        (DirExpr::Var(name), DirExpr::Const(val, cty)) => {
            Some((name.as_str(), kind, *val, int_bits(cty)?))
        }
        (DirExpr::Const(val, cty), DirExpr::Var(name)) => {
            Some((name.as_str(), flip_cmp(kind), *val, int_bits(cty)?))
        }
        _ => None,
    }
}

fn apply_range_constraint(ranges: &mut RangeEnv, name: &str, kind: CmpKind, val: i64, bits: u32) {
    let entry = ranges.entry(name.to_string()).or_default();
    let sc = sext_const(val, bits);
    let uc = zext_const(val, bits);

    let mut constrain_signed = |lo_bound: Option<i128>, hi_bound: Option<i128>| {
        let mut iv = match entry.signed {
            Some((b, iv)) if b == bits => iv,
            _ => signed_domain(bits),
        };
        if let Some(lo) = lo_bound {
            iv.lo = iv.lo.max(lo);
        }
        if let Some(hi) = hi_bound {
            iv.hi = iv.hi.min(hi);
        }
        entry.signed = if iv.lo <= iv.hi { Some((bits, iv)) } else { None };
    };

    match kind {
        CmpKind::SLt => constrain_signed(None, Some(sc - 1)),
        CmpKind::SLe => constrain_signed(None, Some(sc)),
        CmpKind::SGt => constrain_signed(Some(sc + 1), None),
        CmpKind::SGe => constrain_signed(Some(sc), None),
        CmpKind::Eq => constrain_signed(Some(sc), Some(sc)),
        CmpKind::Ne => {}
        CmpKind::ULt | CmpKind::ULe | CmpKind::UGt | CmpKind::UGe => {}
    }

    let mut constrain_unsigned = |lo_bound: Option<i128>, hi_bound: Option<i128>| {
        let mut iv = match entry.unsigned {
            Some((b, iv)) if b == bits => iv,
            _ => unsigned_domain(bits),
        };
        if let Some(lo) = lo_bound {
            iv.lo = iv.lo.max(lo);
        }
        if let Some(hi) = hi_bound {
            iv.hi = iv.hi.min(hi);
        }
        entry.unsigned = if iv.lo <= iv.hi { Some((bits, iv)) } else { None };
    };

    match kind {
        CmpKind::ULt => constrain_unsigned(None, Some(uc - 1)),
        CmpKind::ULe => constrain_unsigned(None, Some(uc)),
        CmpKind::UGt => constrain_unsigned(Some(uc + 1), None),
        CmpKind::UGe => constrain_unsigned(Some(uc), None),
        CmpKind::Eq => constrain_unsigned(Some(uc), Some(uc)),
        CmpKind::Ne => {}
        CmpKind::SLt | CmpKind::SLe | CmpKind::SGt | CmpKind::SGe => {}
    }
}

/// Collect relational constraints implied by `cond` holding (`is_then_branch`)
/// or failing (else). Follows the same polarity rules as `extract_constraints`.
fn extract_range_constraints(cond: &DirExpr, is_then_branch: bool, ranges: &mut RangeEnv) {
    match cond {
        DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ..
        } => {
            if is_then_branch {
                extract_range_constraints(lhs, true, ranges);
                extract_range_constraints(rhs, true, ranges);
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ..
        } => {
            if !is_then_branch {
                extract_range_constraints(lhs, false, ranges);
                extract_range_constraints(rhs, false, ranges);
            }
        }
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } => {
            extract_range_constraints(expr, !is_then_branch, ranges);
        }
        _ => {
            if let Some((name, kind, val, bits)) = split_var_const_cmp(cond) {
                let eff = if is_then_branch { kind } else { negate_cmp(kind) };
                apply_range_constraint(ranges, name, eff, val, bits);
            }
        }
    }
}

/// Decide `var CMP const` from inherited intervals: Some(true/false) only when
/// every value in the interval agrees. Width must match the constraint slot.
fn decide_cmp(ranges: &RangeEnv, cond: &DirExpr) -> Option<bool> {
    let (name, kind, val, bits) = split_var_const_cmp(cond)?;
    let entry = ranges.get(name)?;

    let signed_iv = match entry.signed {
        Some((b, iv)) if b == bits => Some(iv),
        _ => None,
    };
    let unsigned_iv = match entry.unsigned {
        Some((b, iv)) if b == bits => Some(iv),
        _ => None,
    };

    let decide_interval = |iv: Interval, c: i128, k: CmpKind| -> Option<bool> {
        match k {
            CmpKind::SLt | CmpKind::ULt => {
                if iv.hi < c {
                    Some(true)
                } else if iv.lo >= c {
                    Some(false)
                } else {
                    None
                }
            }
            CmpKind::SLe | CmpKind::ULe => {
                if iv.hi <= c {
                    Some(true)
                } else if iv.lo > c {
                    Some(false)
                } else {
                    None
                }
            }
            CmpKind::SGt | CmpKind::UGt => {
                if iv.lo > c {
                    Some(true)
                } else if iv.hi <= c {
                    Some(false)
                } else {
                    None
                }
            }
            CmpKind::SGe | CmpKind::UGe => {
                if iv.lo >= c {
                    Some(true)
                } else if iv.hi < c {
                    Some(false)
                } else {
                    None
                }
            }
            CmpKind::Eq => {
                if iv.lo == c && iv.hi == c {
                    Some(true)
                } else if c < iv.lo || c > iv.hi {
                    Some(false)
                } else {
                    None
                }
            }
            CmpKind::Ne => decide_interval_ne(iv, c),
        }
    };

    match kind {
        CmpKind::SLt | CmpKind::SLe | CmpKind::SGt | CmpKind::SGe => {
            decide_interval(signed_iv?, sext_const(val, bits), kind)
        }
        CmpKind::ULt | CmpKind::ULe | CmpKind::UGt | CmpKind::UGe => {
            decide_interval(unsigned_iv?, zext_const(val, bits), kind)
        }
        CmpKind::Eq | CmpKind::Ne => {
            let via_signed = signed_iv.and_then(|iv| decide_interval(iv, sext_const(val, bits), kind));
            if via_signed.is_some() {
                return via_signed;
            }
            unsigned_iv.and_then(|iv| decide_interval(iv, zext_const(val, bits), kind))
        }
    }
}

fn decide_interval_ne(iv: Interval, c: i128) -> Option<bool> {
    if c < iv.lo || c > iv.hi {
        Some(true)
    } else if iv.lo == c && iv.hi == c {
        Some(false)
    } else {
        None
    }
}

/// Labels are jump targets; a decided-dead arm containing one cannot be
/// discarded safely, so the fold is skipped in that case.
fn stmts_contain_label(stmts: &[DirStmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        DirStmt::Label(_) => true,
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. } => stmts_contain_label(body),
        DirStmt::For {
            init, update, body, ..
        } => {
            init.as_deref()
                .is_some_and(|s| stmts_contain_label(std::slice::from_ref(s)))
                || update
                    .as_deref()
                    .is_some_and(|s| stmts_contain_label(std::slice::from_ref(s)))
                || stmts_contain_label(body)
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => stmts_contain_label(then_body) || stmts_contain_label(else_body),
        DirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmts_contain_label(&c.body)) || stmts_contain_label(default)
        }
        _ => false,
    })
}

fn visit_stmts(
    stmts: &mut [DirStmt],
    env: &mut HashMap<String, DirExpr>,
    ranges: &mut RangeEnv,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= visit_stmt(stmt, env, ranges, binding_types);
    }
    changed
}

fn visit_stmt(
    stmt: &mut DirStmt,
    env: &mut HashMap<String, DirExpr>,
    ranges: &mut RangeEnv,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            // 1. Substitute in RHS
            changed |= substitute_expr(rhs, env);
            // 2. Substitute in LHS (indices / dereferences)
            changed |= substitute_lvalue(lhs, env);
            // 3. Invalidate written variable in env
            if let DirLValue::Var(name) = lhs {
                env.remove(name);
                ranges.remove(name);
            }
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            changed |= substitute_expr(expr, env);
        }
        DirStmt::Block(body) => {
            changed |= visit_stmts(body, env, ranges, binding_types);
        }
        DirStmt::While { cond, body } | DirStmt::DoWhile { cond, body } => {
            changed |= substitute_expr(cond, env);

            let mut loop_env = env.clone();
            let mut loop_ranges = ranges.clone();
            let mut written = HashSet::default();
            collect_written_vars(body, &mut written);
            for v in &written {
                loop_env.remove(v);
                loop_ranges.remove(v);
            }
            changed |= visit_stmts(body, &mut loop_env, &mut loop_ranges, binding_types);
            // Facts about variables the loop writes do not survive the loop.
            for v in &written {
                env.remove(v);
                ranges.remove(v);
            }
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let mut loop_env = env.clone();
            let mut loop_ranges = ranges.clone();
            let mut written = HashSet::default();
            if let Some(i) = init {
                collect_written_vars(std::slice::from_ref(i.as_ref()), &mut written);
            }
            if let Some(u) = update {
                collect_written_vars(std::slice::from_ref(u.as_ref()), &mut written);
            }
            collect_written_vars(body, &mut written);
            for v in &written {
                loop_env.remove(v);
                loop_ranges.remove(v);
            }

            if let Some(i) = init {
                changed |= visit_stmt(i.as_mut(), &mut loop_env, &mut loop_ranges, binding_types);
            }
            if let Some(c) = cond {
                changed |= substitute_expr(c, &loop_env);
            }
            if let Some(u) = update {
                changed |= visit_stmt(u.as_mut(), &mut loop_env, &mut loop_ranges, binding_types);
            }
            changed |= visit_stmts(body, &mut loop_env, &mut loop_ranges, binding_types);
            for v in &written {
                env.remove(v);
                ranges.remove(v);
            }
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= substitute_expr(cond, env);

            let mut then_env = env.clone();
            let mut else_env = env.clone();
            let mut then_ranges = ranges.clone();
            let mut else_ranges = ranges.clone();

            extract_constraints(cond, true, &mut then_env);
            extract_constraints(cond, false, &mut else_env);
            extract_range_constraints(cond, true, &mut then_ranges);
            extract_range_constraints(cond, false, &mut else_ranges);

            // Fold a condition fully decided by dominating constraints; the
            // constant-if cleanup drops the dead arm afterwards.
            if let Some(decided) = decide_cmp(ranges, cond) {
                let discarded: &[DirStmt] = if decided { else_body } else { then_body };
                if !stmts_contain_label(discarded) {
                    let cond_ty = match cond {
                        DirExpr::Binary { ty, .. } => ty.clone(),
                        _ => NirType::Bool,
                    };
                    *cond = DirExpr::Const(i64::from(decided), cond_ty);
                    changed = true;
                }
            }

            changed |= visit_stmts(then_body, &mut then_env, &mut then_ranges, binding_types);
            changed |= visit_stmts(else_body, &mut else_env, &mut else_ranges, binding_types);

            // Post-if state: facts about variables written in either arm no
            // longer hold on the joined path.
            let mut written = HashSet::default();
            collect_written_vars(then_body, &mut written);
            collect_written_vars(else_body, &mut written);
            for v in &written {
                env.remove(v);
                ranges.remove(v);
            }
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= substitute_expr(expr, env);
            for case in &mut *cases {
                let mut case_env = env.clone();
                let mut case_ranges = ranges.clone();
                if let DirExpr::Var(x) = expr {
                    if case.values.len() == 1 {
                        let val = case.values[0];
                        if let Some(ty) = binding_types.get(x) {
                            case_env.insert(x.clone(), DirExpr::Const(val, ty.clone()));
                            if let Some(bits) = int_bits(ty) {
                                apply_range_constraint(
                                    &mut case_ranges,
                                    x,
                                    CmpKind::Eq,
                                    val,
                                    bits,
                                );
                            }
                        }
                    }
                }
                changed |= visit_stmts(&mut case.body, &mut case_env, &mut case_ranges, binding_types);
            }
            changed |= visit_stmts(default, env, ranges, binding_types);
            let mut written = HashSet::default();
            for case in cases.iter() {
                collect_written_vars(&case.body, &mut written);
            }
            collect_written_vars(default, &mut written);
            for v in &written {
                env.remove(v);
                ranges.remove(v);
            }
        }
        DirStmt::VaStart { va_list, .. } => {
            changed |= substitute_expr(va_list, env);
        }
        _ => {}
    }
    changed
}

fn substitute_expr(expr: &mut DirExpr, env: &HashMap<String, DirExpr>) -> bool {
    let mut changed = false;
    match expr {
        DirExpr::Var(name) => {
            if let Some(cst) = env.get(name) {
                *expr = cst.clone();
                changed = true;
            }
        }
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            changed |= substitute_expr(inner, env);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= substitute_expr(lhs, env);
            changed |= substitute_expr(rhs, env);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= substitute_expr(arg, env);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= substitute_expr(cond, env);
            changed |= substitute_expr(then_expr, env);
            changed |= substitute_expr(else_expr, env);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= substitute_expr(base, env);
            changed |= substitute_expr(index, env);
        }
        DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    changed
}

fn substitute_lvalue(lval: &mut DirLValue, env: &HashMap<String, DirExpr>) -> bool {
    let mut changed = false;
    match lval {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => {
            changed |= substitute_expr(ptr, env);
        }
        DirLValue::Index { base, index, .. } => {
            changed |= substitute_expr(base, env);
            changed |= substitute_expr(index, env);
        }
        DirLValue::FieldAccess { base, .. } => {
            changed |= substitute_expr(base, env);
        }
    }
    changed
}

fn extract_constraints(cond: &DirExpr, is_then_branch: bool, env: &mut HashMap<String, DirExpr>) {
    match cond {
        DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } => {
            if is_then_branch {
                match (lhs.as_ref(), rhs.as_ref()) {
                    (DirExpr::Var(name), DirExpr::Const(val, ty)) => {
                        env.insert(name.clone(), DirExpr::Const(*val, ty.clone()));
                    }
                    (DirExpr::Const(val, ty), DirExpr::Var(name)) => {
                        env.insert(name.clone(), DirExpr::Const(*val, ty.clone()));
                    }
                    _ => {}
                }
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => {
            if !is_then_branch {
                match (lhs.as_ref(), rhs.as_ref()) {
                    (DirExpr::Var(name), DirExpr::Const(val, ty)) => {
                        env.insert(name.clone(), DirExpr::Const(*val, ty.clone()));
                    }
                    (DirExpr::Const(val, ty), DirExpr::Var(name)) => {
                        env.insert(name.clone(), DirExpr::Const(*val, ty.clone()));
                    }
                    _ => {}
                }
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ..
        } => {
            if is_then_branch {
                extract_constraints(lhs, is_then_branch, env);
                extract_constraints(rhs, is_then_branch, env);
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ..
        } => {
            if !is_then_branch {
                extract_constraints(lhs, is_then_branch, env);
                extract_constraints(rhs, is_then_branch, env);
            }
        }
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } => {
            extract_constraints(expr, !is_then_branch, env);
        }
        _ => {}
    }
}

fn collect_written_vars(stmts: &[DirStmt], written: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                collect_written_vars_lvalue(lhs, written);
                collect_written_vars_expr(rhs, written);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                collect_written_vars_expr(expr, written);
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_written_vars(body, written);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    collect_written_vars(std::slice::from_ref(i.as_ref()), written);
                }
                if let Some(c) = cond {
                    collect_written_vars_expr(c, written);
                }
                if let Some(u) = update {
                    collect_written_vars(std::slice::from_ref(u.as_ref()), written);
                }
                collect_written_vars(body, written);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_written_vars_expr(cond, written);
                collect_written_vars(then_body, written);
                collect_written_vars(else_body, written);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_written_vars_expr(expr, written);
                for case in cases {
                    collect_written_vars(&case.body, written);
                }
                collect_written_vars(default, written);
            }
            DirStmt::VaStart { va_list, .. } => {
                collect_written_vars_expr(va_list, written);
            }
            _ => {}
        }
    }
}

fn collect_written_vars_lvalue(lval: &DirLValue, written: &mut HashSet<String>) {
    match lval {
        DirLValue::Var(name) => {
            written.insert(name.clone());
        }
        DirLValue::Deref { ptr, .. } => {
            collect_written_vars_expr(ptr, written);
        }
        DirLValue::Index { base, index, .. } => {
            collect_written_vars_expr(base, written);
            collect_written_vars_expr(index, written);
        }
        DirLValue::FieldAccess { base, .. } => {
            collect_written_vars_expr(base, written);
        }
    }
}

fn collect_written_vars_expr(expr: &DirExpr, written: &mut HashSet<String>) {
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            collect_written_vars_expr(inner, written);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_written_vars_expr(lhs, written);
            collect_written_vars_expr(rhs, written);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_written_vars_expr(arg, written);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_written_vars_expr(cond, written);
            collect_written_vars_expr(then_expr, written);
            collect_written_vars_expr(else_expr, written);
        }
        DirExpr::Index { base, index, .. } => {
            collect_written_vars_expr(base, written);
            collect_written_vars_expr(index, written);
        }
        _ => {}
    }
}
