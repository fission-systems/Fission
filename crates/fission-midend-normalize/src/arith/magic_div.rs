//! Constant division by multiply-high, recovered across statements.
//!
//! A compiler lowers `x / d` for a constant `d` to a multiply by a magic
//! `M ~= 2^n / d`, a right shift, and (signed) a sign correction. After
//! register coalescing those pieces are not one expression: they are a
//! straight-line chain that reuses one variable,
//!
//! ```text
//! t = x;  t = t * M;  k = 33;  t = t >> k;  t = t - (x >> 31);
//! ```
//!
//! so the expression-level `recognize_magic_number_division` only ever sees the
//! bare multiply. This pass keeps a block-local view of each variable's current
//! definition (the same reaching-definition fact [`collapse_cdq_signed_mod_in_stmts`]
//! uses), resolves a statement's operands through it, and recognizes the
//! division on the resolved tree. The intermediate definitions are left for the
//! dead-assignment passes that follow; nothing here deletes a statement.
//!
//! # The invariant
//!
//! Every shape recognized below evaluates `floor(x * M / 2^n)` (plus the sign
//! fix-up for the signed shapes). With `d = ceil(2^n / M)` and
//! `e = M*d - 2^n > 0`, that equals `x / d` for every `x` in the range iff the
//! error term `x*e / 2^n` never carries the fractional part of `x/d` over one:
//!
//! * unsigned, `x` in `[0, 2^w)`:        `(2^w - 1) * e <  2^n`
//! * signed with correction, `x` in `[-2^(w-1), 2^(w-1))`: `2^(w-1) * e <= 2^n`
//!
//! A shape whose constants fail the bound is left alone: it is a multiply that
//! happens to resemble a division, not one.
//!
//! The extension kind of the multiplicand is *not* read from the cast that
//! wraps it: the lifter writes a sign extension as `cast<u32>(x)` into a wider
//! variable (`movsxd`), so the cast's signedness is unreliable. The idiom
//! decides: a sign-correction term means signed, a bare shift means unsigned.

use crate::analysis::defuse::collect_address_taken_locals;
use crate::prelude::*;
use fission_midend_prehir::util::expr_type;

/// Longest resolved expression we are willing to build (nodes). Keeps a long
/// chain of coalesced redefinitions from turning into an exponential tree.
const MAX_RESOLVED_NODES: usize = 64;

/// A magic multiplier is never small; this only gates the (cheap) attempt.
const MIN_MAGIC_MULTIPLIER: i128 = 1 << 8;

struct Tracked {
    /// The variable's current value, written over variables that are still
    /// unmodified (already resolved, so entries never chain).
    value: PreHirExpr,
    frees: crate::HashSet<String>,
}

struct Ctx {
    tys: crate::HashMap<String, NirType>,
    address_taken: crate::HashSet<String>,
}

pub fn recognize_magic_division_in_function(func: &mut PreHirFunction) -> bool {
    let mut tys = crate::HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        tys.insert(binding.name.clone(), binding.ty.clone());
    }
    let ctx = Ctx {
        address_taken: collect_address_taken_locals(&func.body),
        tys,
    };
    recognize_in_stmts(&mut func.body, &ctx)
}

fn recognize_in_stmts(stmts: &mut Vec<PreHirStmt>, ctx: &Ctx) -> bool {
    let mut live: crate::HashMap<String, Tracked> = crate::HashMap::default();
    recognize_in_stmts_with(stmts, ctx, &mut live)
}

/// `live` flows through the list: a plain `Block` only groups statements that
/// run in sequence, so facts established before it hold inside it and the
/// facts it leaves hold after it.
fn recognize_in_stmts_with(
    stmts: &mut Vec<PreHirStmt>,
    ctx: &Ctx,
    live: &mut crate::HashMap<String, Tracked>,
) -> bool {
    let mut changed = false;

    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body) => {
                changed |= recognize_in_stmts_with(std::rc::Rc::make_mut(body), ctx, live);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                changed |= recognize_in_stmts(std::rc::Rc::make_mut(body), ctx);
                live.clear();
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= recognize_in_stmts(std::rc::Rc::make_mut(then_body), ctx);
                changed |= recognize_in_stmts(std::rc::Rc::make_mut(else_body), ctx);
                live.clear();
            }
            PreHirStmt::For { body, .. } => {
                changed |= recognize_in_stmts(std::rc::Rc::make_mut(body), ctx);
                live.clear();
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= recognize_in_stmts(std::rc::Rc::make_mut(&mut case.body), ctx);
                }
                changed |= recognize_in_stmts(std::rc::Rc::make_mut(default), ctx);
                live.clear();
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => live.clear(),
            PreHirStmt::Return(Some(expr)) => {
                changed |= rewrite_expr(expr, live, ctx);
                live.clear();
            }
            PreHirStmt::Return(None) => live.clear(),
            PreHirStmt::Expr(expr) => {
                changed |= rewrite_expr(expr, live, ctx);
            }
            PreHirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_expr(rhs, live, ctx);
                if let PreHirLValue::Var(name) = lhs {
                    let resolved = resolve(rhs, live, ctx);
                    // The definition replaces every earlier fact about `name`
                    // and invalidates facts built on its previous value.
                    live.retain(|_, tracked| !tracked.frees.contains(name));
                    live.remove(name);
                    if trackable(name, ctx) {
                        if let Some(value) = resolved {
                            let mut frees = crate::HashSet::default();
                            collect_vars(&value, &mut frees);
                            if !frees.contains(name) && frees.iter().all(|v| trackable(v, ctx)) {
                                live.insert(name.clone(), Tracked { value, frees });
                            }
                        }
                    }
                }
            }
            PreHirStmt::VaStart { .. } => live.clear(),
        }
    }
    changed
}

/// Only local scalars whose address never escapes are tracked: a call or a
/// store through a pointer cannot change them behind the scan's back.
fn trackable(name: &str, ctx: &Ctx) -> bool {
    ctx.tys.contains_key(name) && !ctx.address_taken.contains(name)
}

/// `expr` with every tracked variable replaced by its current value, or `None`
/// when the result is not a pure value expression or is too large.
fn resolve(
    expr: &PreHirExpr,
    live: &crate::HashMap<String, Tracked>,
    ctx: &Ctx,
) -> Option<PreHirExpr> {
    let mut out = expr.clone();
    let mut budget = MAX_RESOLVED_NODES;
    if !substitute(&mut out, live, ctx, &mut budget) {
        return None;
    }
    Some(out)
}

fn substitute(
    expr: &mut PreHirExpr,
    live: &crate::HashMap<String, Tracked>,
    ctx: &Ctx,
    budget: &mut usize,
) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    match expr {
        PreHirExpr::Var(name) => {
            if !trackable(name, ctx) {
                // Not a tracked local (a global or a memory-backed name): its
                // value is not ours to reason about, and the expression is not
                // a pure function of locals.
                return false;
            }
            if let Some(tracked) = live.get(name.as_str()) {
                let node_count = count_nodes(&tracked.value);
                if node_count > *budget {
                    return false;
                }
                *budget -= node_count;
                *expr = tracked.value.clone();
            }
            true
        }
        PreHirExpr::Const(_, _) => true,
        PreHirExpr::Cast { expr: inner, .. } | PreHirExpr::Unary { expr: inner, .. } => {
            substitute(inner, live, ctx, budget)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            substitute(lhs, live, ctx, budget) && substitute(rhs, live, ctx, budget)
        }
        // Memory, calls, selects and addresses: not pure functions of the
        // tracked locals, so no reaching-definition fact is built on them.
        _ => false,
    }
}

fn count_nodes(expr: &PreHirExpr) -> usize {
    match expr {
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => 1 + count_nodes(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => 1 + count_nodes(lhs) + count_nodes(rhs),
        _ => 1,
    }
}

fn collect_vars(expr: &PreHirExpr, out: &mut crate::HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => collect_vars(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_vars(lhs, out);
            collect_vars(rhs, out);
        }
        _ => {}
    }
}

fn contains_magic_multiplier(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            let big = |e: &PreHirExpr| matches!(e, PreHirExpr::Const(v, _) if i128::from(*v).abs() >= MIN_MAGIC_MULTIPLIER);
            big(lhs) || big(rhs) || contains_magic_multiplier(lhs) || contains_magic_multiplier(rhs)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            contains_magic_multiplier(lhs) || contains_magic_multiplier(rhs)
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            contains_magic_multiplier(expr)
        }
        _ => false,
    }
}

fn rewrite_expr(expr: &mut PreHirExpr, live: &crate::HashMap<String, Tracked>, ctx: &Ctx) -> bool {
    let Some(resolved) = resolve(expr, live, ctx) else {
        return false;
    };
    if !contains_magic_multiplier(&resolved) {
        return false;
    }
    match recognize_resolved(&resolved, ctx) {
        Some(new_expr) if new_expr != *expr => {
            *expr = new_expr;
            true
        }
        _ => false,
    }
}

// ── Recognition on a resolved tree ───────────────────────────────────────────

/// Recognize a whole constant division at the root of `expr` (looking through
/// the casts that wrap it) and return `x / d`.
fn recognize_resolved(expr: &PreHirExpr, ctx_tys: &Ctx) -> Option<PreHirExpr> {
    let (outer_cast, core) = match expr {
        PreHirExpr::Cast { ty, expr: inner } => (Some(ty.clone()), inner.as_ref()),
        other => (None, other),
    };
    let core = strip_casts(core);
    let division = recognize_signed(core, ctx_tys).or_else(|| recognize_unsigned(core, ctx_tys))?;
    Some(match outer_cast {
        Some(ty) if int_bits(&ty).is_some() => PreHirExpr::Cast {
            ty,
            expr: Box::new(division),
        },
        _ => division,
    })
}

fn strip_casts(mut expr: &PreHirExpr) -> &PreHirExpr {
    while let PreHirExpr::Cast { expr: inner, .. } = expr {
        expr = inner;
    }
    expr
}

/// `strip_casts`, also reporting the narrowest integer width any stripped cast
/// has (`u32::MAX` when there was none): a shifted-out high half only survives
/// a cast that is wide enough to hold it.
fn strip_casts_tracking_width(mut expr: &PreHirExpr) -> (&PreHirExpr, u32) {
    let mut narrowest = u32::MAX;
    while let PreHirExpr::Cast { ty, expr: inner } = expr {
        if let Some(bits) = int_bits(ty) {
            narrowest = narrowest.min(bits);
        }
        expr = inner;
    }
    (expr, narrowest)
}

/// Strip casts that only widen or re-sign (never narrow), so the remaining
/// expression is the value that was extended.
fn strip_extensions<'a>(mut expr: &'a PreHirExpr, ctx: &Ctx) -> &'a PreHirExpr {
    while let PreHirExpr::Cast { ty, expr: inner } = expr {
        match (int_bits(ty), value_bits(inner, ctx)) {
            (Some(to), Some(from)) if to >= from => expr = inner,
            _ => break,
        }
    }
    expr
}

fn int_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

/// Bit width of the integer value `expr` yields, when the IR states it.
fn value_bits(expr: &PreHirExpr, ctx: &Ctx) -> Option<u32> {
    match expr {
        PreHirExpr::Var(name) => ctx.tys.get(name).and_then(int_bits),
        PreHirExpr::Cast { ty, .. }
        | PreHirExpr::Binary { ty, .. }
        | PreHirExpr::Unary { ty, .. }
        | PreHirExpr::Const(_, ty) => int_bits(ty),
        _ => None,
    }
}

fn const_value(expr: &PreHirExpr) -> Option<i128> {
    match strip_casts(expr) {
        PreHirExpr::Const(v, _) => Some(i128::from(*v)),
        _ => None,
    }
}

/// `x` as a value of the integer type `signed`/`bits`, without a cast when it
/// already has that type.
fn as_int(expr: &PreHirExpr, bits: u32, signed: bool, ctx: &Ctx) -> PreHirExpr {
    let ty = NirType::Int { bits, signed };
    if let PreHirExpr::Var(name) = expr {
        if ctx.tys.get(name) == Some(&ty) {
            return expr.clone();
        }
    }
    if let PreHirExpr::Cast {
        ty: cast_ty,
        expr: inner,
    } = expr
    {
        if int_bits(cast_ty) == Some(bits) {
            return PreHirExpr::Cast {
                ty,
                expr: inner.clone(),
            };
        }
    }
    PreHirExpr::Cast {
        ty,
        expr: Box::new(expr.clone()),
    }
}

fn division(x: PreHirExpr, d: i128, bits: u32, signed: bool) -> PreHirExpr {
    let ty = NirType::Int { bits, signed };
    PreHirExpr::Binary {
        op: PreHirBinaryOp::Div,
        lhs: Box::new(x),
        rhs: Box::new(PreHirExpr::Const(d as i64, ty.clone())),
        ty,
    }
}

/// `d = ceil(2^n / M)` and `e = M*d - 2^n`, for a multiplier that is not a
/// power of two.
fn magic_parameters(m: i128, n: u32) -> Option<(i128, i128)> {
    if m <= 1 || n >= 100 || m.count_ones() == 1 {
        return None;
    }
    let power = 1i128 << n;
    let d = power / m + 1;
    let e = m * d - power;
    (d >= 2 && e > 0).then_some((d, e))
}

/// Unsigned: `(x * M) >> n` (or `/ 2^n` in an unsigned type) over the full
/// unsigned `w`-bit range.
fn recognize_unsigned(core: &PreHirExpr, ctx: &Ctx) -> Option<PreHirExpr> {
    let PreHirExpr::Binary { op, lhs, rhs, ty } = core else {
        return None;
    };
    let n = match op {
        PreHirBinaryOp::Shr => u32::try_from(const_value(rhs)?).ok()?,
        PreHirBinaryOp::Div if matches!(ty, NirType::Int { signed: false, .. }) => {
            let divisor = const_value(rhs)?;
            if divisor <= 1 || divisor.count_ones() != 1 {
                return None;
            }
            divisor.trailing_zeros()
        }
        _ => return None,
    };
    // The shift may be split across a cast: `((x * M) >> a) >> b` is a shift by
    // `a + b` when the cast between them can hold the `a`-shifted product.
    let (shifted, narrowest) = strip_casts_tracking_width(lhs);
    let (product, n, inner_shift) = match shifted {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr,
            lhs: inner,
            rhs: inner_amount,
            ..
        } if const_value(inner_amount).is_some() => {
            let a = u32::try_from(const_value(inner_amount)?).ok()?;
            (strip_casts(inner), n.checked_add(a)?, Some((a, narrowest)))
        }
        other => (other, n, None),
    };
    let (x, m, mul_bits) = split_mul_by_const(product, ctx)?;
    let w = value_bits(x, ctx).filter(|w| (8..=32).contains(w))?;
    if mul_bits < 2 * w || n < w {
        return None;
    }
    let (d, e) = magic_parameters(m, n)?;
    let max_x = (1i128 << w) - 1;
    if m <= 0 || max_x * e >= (1i128 << n) || max_x * m >= (1i128 << mul_bits) {
        return None;
    }
    if let Some((a, narrowest)) = inner_shift {
        if narrowest < 128 && (max_x * m) >> a >= (1i128 << narrowest) {
            return None;
        }
    }
    Some(division(as_int(x, w, false, ctx), d, w, false))
}

/// `Mul(extension of x, constant)` -> (`x`, constant, bits of the multiply).
fn split_mul_by_const<'a>(expr: &'a PreHirExpr, ctx: &Ctx) -> Option<(&'a PreHirExpr, i128, u32)> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Mul,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let (other, m) = match (const_value(lhs), const_value(rhs)) {
        (None, Some(m)) => (lhs.as_ref(), m),
        (Some(m), None) => (rhs.as_ref(), m),
        _ => return None,
    };
    Some((strip_extensions(other, ctx), m, int_bits(ty)?))
}

/// Signed with a sign correction:
///
/// * `(sext(x) * M >> n) - (x >> (w-1))`
/// * `((hi32(sext(x) * (M - 2^w)) + x) >> k) - (x >> (w-1))`, the form used
///   when `M` does not fit a signed `w`-bit immediate (`n = w + k`).
fn recognize_signed(core: &PreHirExpr, ctx: &Ctx) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
        lhs: quotient,
        rhs: sign,
        ty: sub_ty,
    } = core
    else {
        return None;
    };

    // Sign mask: x >>s (w-1).
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Sar,
        lhs: mask_operand,
        rhs: mask_shift,
        ..
    } = strip_casts(sign)
    else {
        return None;
    };
    let x = strip_extensions(mask_operand, ctx);
    let w = value_bits(x, ctx).filter(|w| (8..=32).contains(w))?;
    // The difference is a `w`-bit value; that truncation is what makes a
    // logical high-half shift below equal to the arithmetic one.
    if int_bits(sub_ty) != Some(w) {
        return None;
    }
    if const_value(mask_shift)? != i128::from(w - 1) {
        return None;
    }

    let (m, n, mul_bits) = signed_quotient_parts(strip_casts(quotient), x, w, ctx)?;
    if mul_bits < 2 * w {
        return None;
    }
    let (d, e) = magic_parameters(m, n)?;
    if (1i128 << (w - 1)) * e > (1i128 << n) {
        return None;
    }
    Some(division(as_int(x, w, true, ctx), d, w, true))
}

/// The multiplier and total shift of the signed quotient term, requiring its
/// dividend to be the same value `x` as the sign mask.
fn signed_quotient_parts(
    quotient: &PreHirExpr,
    x: &PreHirExpr,
    w: u32,
    ctx: &Ctx,
) -> Option<(i128, u32, u32)> {
    let same_x = |candidate: &PreHirExpr| strip_extensions(candidate, ctx) == x;
    let (shifted, extra, narrowest) = match quotient {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } => {
            let (inner, narrowest) = strip_casts_tracking_width(lhs);
            (inner, u32::try_from(const_value(rhs)?).ok()?, narrowest)
        }
        other => (other, 0, u32::MAX),
    };
    // The high half of the product, `(x * M) >> w`, may be shifted again:
    // `(hi >> k)` is one shift by `w + k`. A logical `>> w` equals the
    // arithmetic one only once truncated to exactly `w` bits.
    let (shifted, extra) = match shifted {
        PreHirExpr::Binary {
            op: op @ (PreHirBinaryOp::Shr | PreHirBinaryOp::Sar),
            lhs: product,
            rhs: amount,
            ..
        } if matches!(
            strip_casts(product),
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                ..
            }
        ) && const_value(amount) == Some(i128::from(w))
            && if *op == PreHirBinaryOp::Shr {
                narrowest == w || extra == 0
            } else {
                narrowest >= w
            } =>
        {
            (strip_casts(product), extra + w)
        }
        other => (other, extra),
    };

    // Plain: sar(x * M, n).
    if let Some((operand, m, mul_bits)) = split_mul_by_const(shifted, ctx) {
        if extra >= w && same_x(operand) && m > 0 {
            return Some((m, extra, mul_bits));
        }
    }

    // High-half form: sar(hi + x, k) where hi = (x * Mneg) >> w.
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = shifted
    else {
        return None;
    };
    for (high, addend) in [(lhs, rhs), (rhs, lhs)] {
        if !same_x(addend) {
            continue;
        }
        let PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
            lhs: product,
            rhs: high_shift,
            ..
        } = strip_casts(high)
        else {
            continue;
        };
        if const_value(high_shift) != Some(i128::from(w)) {
            continue;
        }
        let Some((operand, m_signed, mul_bits)) = split_mul_by_const(strip_casts(product), ctx)
        else {
            continue;
        };
        if !same_x(operand) || m_signed >= 0 {
            continue;
        }
        let m = m_signed + (1i128 << w);
        return Some((m, w + extra, mul_bits));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn konst(v: i64, ty: NirType) -> PreHirExpr {
        PreHirExpr::Const(v, ty)
    }

    fn bin(op: PreHirBinaryOp, lhs: PreHirExpr, rhs: PreHirExpr, ty: NirType) -> PreHirExpr {
        PreHirExpr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty,
        }
    }

    fn cast(ty: NirType, e: PreHirExpr) -> PreHirExpr {
        PreHirExpr::Cast {
            ty,
            expr: Box::new(e),
        }
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs,
        }
    }

    fn binding(name: &str, ty: NirType) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    fn function(locals: Vec<(&str, NirType)>, body: Vec<PreHirStmt>) -> PreHirFunction {
        PreHirFunction {
            locals: locals.into_iter().map(|(n, t)| binding(n, t)).collect(),
            body,
            ..Default::default()
        }
    }

    /// `t = sext(x); t = t * M; k = n; t = t >> k; t = (u32)t - (u32)(x >> 31)`
    /// -- the coalesced chain seen in compiled output, with the shift amount in
    /// a variable that is reused for the sign shift.
    fn signed_chain(m: i64, n: i64) -> PreHirFunction {
        let w32 = i(32, true);
        let w64 = i(64, true);
        function(
            vec![
                ("x", w32.clone()),
                ("t", w64.clone()),
                ("k", i(32, false)),
                ("s", w32.clone()),
            ],
            vec![
                assign("t", cast(i(32, false), var("x"))),
                assign("k", konst(31, i(32, false))),
                assign(
                    "s",
                    bin(
                        PreHirBinaryOp::Sar,
                        cast(i(32, false), var("x")),
                        var("k"),
                        w32.clone(),
                    ),
                ),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Mul,
                        var("t"),
                        konst(m, w64.clone()),
                        w64.clone(),
                    ),
                ),
                assign("k", konst(n, i(32, false))),
                assign(
                    "t",
                    bin(PreHirBinaryOp::Sar, var("t"), var("k"), w64.clone()),
                ),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Sub,
                        cast(i(32, false), var("t")),
                        cast(i(32, false), var("s")),
                        w32.clone(),
                    ),
                ),
                PreHirStmt::Return(Some(var("t"))),
            ],
        )
    }

    fn last_assign_rhs(func: &PreHirFunction, name: &str) -> PreHirExpr {
        func.body
            .iter()
            .rev()
            .find_map(|s| match s {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(n),
                    rhs,
                } if n == name => Some(rhs.clone()),
                _ => None,
            })
            .expect("assignment")
    }

    fn assert_is_div(expr: &PreHirExpr, divisor: i64, signed: bool) {
        let core = strip_casts(expr);
        let PreHirExpr::Binary {
            op: PreHirBinaryOp::Div,
            lhs,
            rhs,
            ty,
        } = core
        else {
            panic!("expected a division, got {expr:?}");
        };
        assert_eq!(**rhs, konst(divisor, i(32, signed)));
        assert_eq!(*ty, i(32, signed));
        assert_eq!(strip_casts(lhs), &var("x"));
    }

    #[test]
    fn signed_division_by_five_across_coalesced_chain() {
        let mut func = signed_chain(1717986919, 33);
        assert!(recognize_magic_division_in_function(&mut func));
        assert_is_div(&last_assign_rhs(&func, "t"), 5, true);
    }

    #[test]
    fn signed_division_by_ten_and_hundred() {
        let mut func = signed_chain(1717986919, 34);
        assert!(recognize_magic_division_in_function(&mut func));
        assert_is_div(&last_assign_rhs(&func, "t"), 10, true);

        let mut func = signed_chain(1374389535, 37);
        assert!(recognize_magic_division_in_function(&mut func));
        assert_is_div(&last_assign_rhs(&func, "t"), 100, true);
    }

    #[test]
    fn multiplier_that_is_not_a_division_is_left_alone() {
        // One off from the /5 multiplier: e = 8, and 2^31 * 8 > 2^33, so some
        // negative dividend would round the wrong way. Not a division.
        let mut func = signed_chain(1717986920, 33);
        assert!(!recognize_magic_division_in_function(&mut func));
    }

    #[test]
    fn wrong_shift_amount_is_left_alone() {
        // The sign shift must be w-1 for the sign correction to be one.
        let mut func = signed_chain(1717986919, 33);
        func.body[1] = assign("k", konst(30, i(32, false)));
        assert!(!recognize_magic_division_in_function(&mut func));
    }

    #[test]
    fn redefined_dividend_blocks_recognition() {
        let mut func = signed_chain(1717986919, 33);
        // x changes between the multiply and the sign mask use.
        func.body.insert(
            4,
            assign(
                "x",
                bin(
                    PreHirBinaryOp::Add,
                    var("x"),
                    konst(1, i(32, true)),
                    i(32, true),
                ),
            ),
        );
        assert!(!recognize_magic_division_in_function(&mut func));
    }

    #[test]
    fn unsigned_division_through_a_power_of_two_div() {
        // t = x; t = t * 3435973837; return t / 2^35;   (unsigned /10)
        let w32 = i(32, true);
        let w64 = i(64, true);
        let mut func = function(
            vec![("x", w32.clone()), ("t", w32.clone())],
            vec![
                assign("t", var("x")),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Mul,
                        var("t"),
                        konst(3435973837, i(64, false)),
                        w64.clone(),
                    ),
                ),
                PreHirStmt::Return(Some(bin(
                    PreHirBinaryOp::Div,
                    var("t"),
                    konst(1 << 35, i(64, false)),
                    i(64, false),
                ))),
            ],
        );
        assert!(recognize_magic_division_in_function(&mut func));
        let PreHirStmt::Return(Some(expr)) = func.body.last().unwrap() else {
            panic!()
        };
        assert_is_div(expr, 10, false);
    }

    #[test]
    fn address_taken_dividend_is_not_tracked() {
        let mut func = signed_chain(1717986919, 33);
        func.body.insert(
            0,
            PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "f".into(),
                args: vec![PreHirExpr::AddressOfLocal("t".into())],
                ty: i(32, true),
            }),
        );
        assert!(!recognize_magic_division_in_function(&mut func));
    }

    /// `t = sext(x) * M; t = t >> 32; t = (u32)t >>s k; t = t - (x >> 31)` --
    /// the shift split across a truncating cast.
    fn signed_split_shift_chain(m: i64, k: i64, inner_op: PreHirBinaryOp) -> PreHirFunction {
        let w32 = i(32, true);
        let w64 = i(64, true);
        function(
            vec![("x", w32.clone()), ("t", w64.clone()), ("s", w32.clone())],
            vec![
                assign("t", cast(w64.clone(), var("x"))),
                assign(
                    "s",
                    bin(
                        PreHirBinaryOp::Sar,
                        var("x"),
                        konst(31, w32.clone()),
                        w32.clone(),
                    ),
                ),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Mul,
                        var("t"),
                        konst(m, w64.clone()),
                        w64.clone(),
                    ),
                ),
                assign(
                    "t",
                    bin(inner_op, var("t"), konst(32, w32.clone()), w64.clone()),
                ),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Sar,
                        cast(i(32, false), var("t")),
                        konst(k, w32.clone()),
                        w32.clone(),
                    ),
                ),
                assign(
                    "t",
                    bin(PreHirBinaryOp::Sub, var("t"), var("s"), w32.clone()),
                ),
                PreHirStmt::Return(Some(var("t"))),
            ],
        )
    }

    #[test]
    fn signed_division_with_shift_split_across_a_cast() {
        // 1717986919 >> (32 + 1) is /5; 715827883 >> (32 + 0) is /6.
        for (m, k, d) in [(1717986919, 1, 5), (1374389535, 5, 100)] {
            for op in [PreHirBinaryOp::Shr, PreHirBinaryOp::Sar] {
                let mut func = signed_split_shift_chain(m, k, op);
                assert!(
                    recognize_magic_division_in_function(&mut func),
                    "{m} {op:?}"
                );
                assert_is_div(&last_assign_rhs(&func, "t"), d, true);
            }
        }
    }

    #[test]
    fn split_shift_with_wrong_total_is_left_alone() {
        // 1717986919 with a total shift of 32 is not a division.
        let mut func = signed_split_shift_chain(1717986919, 0, PreHirBinaryOp::Sar);
        assert!(!recognize_magic_division_in_function(&mut func));
    }

    #[test]
    fn unsigned_division_with_shift_split_across_a_cast() {
        // t = x * 1374389535; t = t >> 32; t = (u32)t >> 5 -> x / 100 (n = 37)
        let w32 = i(32, false);
        let w64 = i(64, false);
        let mut func = function(
            vec![("x", w32.clone()), ("t", w64.clone())],
            vec![
                assign("t", cast(w64.clone(), var("x"))),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Mul,
                        var("t"),
                        konst(1374389535, w64.clone()),
                        w64.clone(),
                    ),
                ),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Shr,
                        var("t"),
                        konst(32, w32.clone()),
                        w64.clone(),
                    ),
                ),
                assign(
                    "t",
                    bin(
                        PreHirBinaryOp::Shr,
                        cast(w32.clone(), var("t")),
                        konst(5, w32.clone()),
                        w32.clone(),
                    ),
                ),
                PreHirStmt::Return(Some(var("t"))),
            ],
        );
        assert!(recognize_magic_division_in_function(&mut func));
        assert_is_div(&last_assign_rhs(&func, "t"), 100, false);
    }

    // ── The arithmetic claim, checked against brute force ────────────────────

    /// For every width small enough to enumerate, every divisor and shift: if
    /// the bound accepts `(M, n)`, the shape equals `x / d` for ALL `x`.
    #[test]
    fn unsigned_bound_is_sound_exhaustively() {
        for w in 8..=14u32 {
            let max_x = (1i128 << w) - 1;
            for d in 2..=200i128 {
                for n in w..=w + 12 {
                    let m = ((1i128 << n) + d - 1) / d; // ceil(2^n / d)
                    let Some((found_d, e)) = magic_parameters(m, n) else {
                        continue;
                    };
                    if found_d != d || max_x * e >= (1i128 << n) {
                        continue;
                    }
                    for x in 0..=max_x {
                        assert_eq!((x * m) >> n, x / d, "w={w} d={d} n={n} x={x}");
                    }
                }
            }
        }
    }

    #[test]
    fn signed_bound_is_sound_exhaustively() {
        for w in 8..=14u32 {
            let half = 1i128 << (w - 1);
            for d in 2..=200i128 {
                for n in w..=w + 12 {
                    let m = ((1i128 << n) + d - 1) / d;
                    let Some((found_d, e)) = magic_parameters(m, n) else {
                        continue;
                    };
                    if found_d != d || half * e > (1i128 << n) {
                        continue;
                    }
                    for x in -half..half {
                        let q = ((x * m) >> n) - (x >> (w - 1));
                        assert_eq!(q, x.abs() / d * x.signum(), "w={w} d={d} n={n} x={x}");
                    }
                }
            }
        }
    }

    /// The constants seen in compiled 32-bit code, at the edges of the range.
    #[test]
    fn thirty_two_bit_constants_hold_at_range_edges() {
        let cases: &[(i128, u32, i128)] = &[
            (1717986919, 33, 5),
            (1717986919, 34, 10),
            (1374389535, 37, 100),
            (715827883, 32, 6),
            (954437177, 33, 9),
        ];
        for &(m, n, d) in cases {
            let (found_d, e) = magic_parameters(m, n).expect("parameters");
            assert_eq!(found_d, d);
            assert!((1i128 << 31) * e <= (1i128 << n), "bound holds for {m}/{n}");
            let edges = [
                -(1i128 << 31),
                -(1i128 << 31) + 1,
                -d - 1,
                -d,
                -d + 1,
                -1,
                0,
                1,
                d - 1,
                d,
                d + 1,
                (1i128 << 31) - 1,
                (1i128 << 31) - 1 - ((1i128 << 31) - 1) % d, // top multiple
                (1i128 << 31) - 1 - ((1i128 << 31) - 1) % d - 1, // top remainder d-1
            ];
            for x in edges {
                let q = ((x * m) >> n) - (x >> 31);
                assert_eq!(q, x.abs() / d * x.signum(), "m={m} n={n} d={d} x={x}");
            }
        }
        // Unsigned: 0xAAAAAAAB >> 33 is x/3 over the whole 32-bit range.
        let (d, e) = magic_parameters(2863311531, 33).unwrap();
        assert_eq!(d, 3);
        assert!(((1i128 << 32) - 1) * e < (1i128 << 33));
        for x in [0i128, 1, 2, 3, (1 << 32) - 1, (1 << 32) - 2, (1 << 32) - 3] {
            assert_eq!((x * 2863311531) >> 33, x / 3);
        }
    }

    #[test]
    fn signed_plus_x_form_matches_division_by_seven() {
        // hi = (sext(x) * (M - 2^32)) >> 32 ; q = ((hi + x) >> 2) - (x >> 31)
        let m_signed = -1840700269i128;
        let m = m_signed + (1i128 << 32);
        let (d, e) = magic_parameters(m, 34).unwrap();
        assert_eq!(d, 7);
        assert!((1i128 << 31) * e <= (1i128 << 34));
        for x in [-(1i128 << 31), -7, -6, -1, 0, 1, 6, 7, (1 << 31) - 1] {
            let hi = (x * m_signed) >> 32;
            let q = ((hi + x) >> 2) - (x >> 31);
            assert_eq!(q, x.abs() / 7 * x.signum(), "x={x}");
        }
    }
}
