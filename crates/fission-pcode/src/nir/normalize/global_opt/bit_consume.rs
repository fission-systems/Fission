use super::super::*;
use crate::nir::support::expr_type;
use std::collections::HashMap;

/// Bit-level consumed-mask backward propagation pass.
///
/// This is the HIR-level analog of Ghidra's `ActionDeadCode` + `ActionNonzeroMask`.
///
/// ## Algorithm
///
/// 1. **Seed**: All variable reads that are not subject to mask narrowing are seeded
///    with a fully-consumed mask (all bits).  Reads inside comparisons, call arguments,
///    return values, and memory stores are treated as fully consuming.
///
/// 2. **Backward propagation** (worklist): For single-defined variables, propagate the
///    consumed mask backward through the defining expression using operator-specific
///    bit-mask transfer functions:
///    - `x = y & C`    → consumed[y] |= consumed[x] & C
///    - `x = y >> n`   → consumed[y] |= consumed[x] << n
///    - `x = y << n`   → consumed[y] |= consumed[x] >> n
///    - `x = (TN)y`    (narrow cast) → consumed[y] |= consumed[x] & narrow_mask(N)
///    - `x = y | z`    → consumed[y] |= consumed[x], consumed[z] |= consumed[x]
///    - `x = y + z`    → consumed[y] |= consumed[x] (conservative)
///    - `x = y`        → consumed[y] |= consumed[x]
///
/// 3. **Simplification**:
///    - `x = y | C` where `consumed[x] & C == 0` → `x = y`  (dead OR bits)
///    - `x = y | C` where `consumed[x] & C == consumed[x]` and `consumed[y] == 0`
///      → `x = C` (y bits dead, constant wins)
///    - `x = y & C` where `consumed[x] == 0` → assignment dead (handled by dead-assign)
///    - `x = ZEXT(y)` where `consumed[x] ≤ narrow_mask(y)` → `x = y` (ZEXT redundant)
///
/// ## Limitations
///
/// Variables with multiple definitions are treated conservatively (fully consumed).
/// Loop-carried variables are also treated conservatively.
pub(crate) fn apply_bit_consume_dead_code_pass(func: &mut HirFunction) -> bool {
    // --- Phase 1: collect definitions and multi-def variables ---
    let mut def_map: HashMap<String, HirExpr> = HashMap::new();
    let mut multi_def: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_definitions(&func.body, &mut def_map, &mut multi_def);

    // --- Phase 2: build initial consumed seeds from all use sites ---
    let mut consumed: HashMap<String, u64> = HashMap::new();
    collect_consumed_seeds(&func.body, &mut consumed, &def_map, &multi_def);

    // --- Phase 3: backward propagation worklist ---
    // Each iteration propagates consumed masks backward through def_map entries.
    // We iterate until convergence (no change in any consumed mask).
    let mut changed_masks = true;
    let mut iters = 0;
    while changed_masks && iters < 32 {
        changed_masks = false;
        iters += 1;
        for (var, expr) in &def_map {
            if multi_def.contains(var) {
                continue;
            }
            let out_consume = consumed.get(var).copied().unwrap_or(0);
            if out_consume == 0 {
                continue;
            }
            let propagations = backward_propagate(expr, out_consume);
            for (src_var, mask) in propagations {
                let entry = consumed.entry(src_var).or_insert(0);
                let old = *entry;
                *entry |= mask;
                if *entry != old {
                    changed_masks = true;
                }
            }
        }
    }

    // --- Phase 4: simplify the body ---
    let type_map = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| (binding.name.clone(), binding.ty.clone()))
        .collect();
    let mut any_changed = false;
    simplify_stmts(
        &mut func.body,
        &consumed,
        &multi_def,
        &type_map,
        &mut any_changed,
    );
    any_changed
}

// ── Phase 1: definition collection ───────────────────────────────────────────

fn collect_definitions(
    stmts: &[HirStmt],
    def_map: &mut HashMap<String, HirExpr>,
    multi_def: &mut std::collections::HashSet<String>,
) {
    for stmt in stmts {
        collect_def_stmt(stmt, def_map, multi_def);
    }
}

fn collect_def_stmt(
    stmt: &HirStmt,
    def_map: &mut HashMap<String, HirExpr>,
    multi_def: &mut std::collections::HashSet<String>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            if def_map.contains_key(name.as_str()) {
                multi_def.insert(name.clone());
            } else {
                def_map.insert(name.clone(), rhs.clone());
            }
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_definitions(body, def_map, multi_def);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                collect_def_stmt(i, def_map, multi_def);
            }
            if let Some(u) = update {
                collect_def_stmt(u, def_map, multi_def);
            }
            collect_definitions(body, def_map, multi_def);
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_definitions(then_body, def_map, multi_def);
            collect_definitions(else_body, def_map, multi_def);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_definitions(&case.body, def_map, multi_def);
            }
            collect_definitions(default, def_map, multi_def);
        }
        _ => {}
    }
}

// ── Phase 2: seed consumed masks from use sites ───────────────────────────────

fn collect_consumed_seeds(
    stmts: &[HirStmt],
    consumed: &mut HashMap<String, u64>,
    def_map: &HashMap<String, HirExpr>,
    multi_def: &std::collections::HashSet<String>,
) {
    for stmt in stmts {
        seed_stmt(stmt, consumed, def_map, multi_def);
    }
}

fn seed_stmt(
    stmt: &HirStmt,
    consumed: &mut HashMap<String, u64>,
    def_map: &HashMap<String, HirExpr>,
    multi_def: &std::collections::HashSet<String>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            // The RHS seeds the consumed mask for variables it reads.
            // This is the forward-seed pass: we look at how `rhs` reads variables.
            seed_expr_uses(rhs, consumed, true /*can_narrow*/);
        }
        HirStmt::Assign { lhs, rhs } => {
            // Memory write: all operands fully consumed.
            seed_lvalue_fully(lhs, consumed);
            seed_expr_fully(rhs, consumed);
        }
        HirStmt::Expr(expr) => seed_expr_fully(expr, consumed),
        HirStmt::Return(Some(expr)) => seed_expr_fully(expr, consumed),
        HirStmt::Return(None) => {}
        HirStmt::Break | HirStmt::Continue | HirStmt::Label(_) | HirStmt::Goto(_) => {}
        HirStmt::VaStart { va_list, .. } => seed_expr_fully(va_list, consumed),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_consumed_seeds(body, consumed, def_map, multi_def);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                seed_stmt(i, consumed, def_map, multi_def);
            }
            if let Some(c) = cond {
                seed_expr_fully(c, consumed);
            }
            if let Some(u) = update {
                seed_stmt(u, consumed, def_map, multi_def);
            }
            collect_consumed_seeds(body, consumed, def_map, multi_def);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            seed_expr_fully(cond, consumed);
            collect_consumed_seeds(then_body, consumed, def_map, multi_def);
            collect_consumed_seeds(else_body, consumed, def_map, multi_def);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            seed_expr_fully(expr, consumed);
            for case in cases {
                collect_consumed_seeds(&case.body, consumed, def_map, multi_def);
            }
            collect_consumed_seeds(default, consumed, def_map, multi_def);
        }
    }
}

/// Seed variables in `expr` as their bits in context determine.
/// `can_narrow = true` means we track the operator-level narrowing (AND/cast).
fn seed_expr_uses(expr: &HirExpr, consumed: &mut HashMap<String, u64>, can_narrow: bool) {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if can_narrow => {
            // `_ = lhs & const_mask`: only those bits of lhs are initially consumed.
            if let HirExpr::Const(mask, _) = rhs.as_ref() {
                seed_expr_with_mask(lhs, *mask as u64, consumed);
                // mask itself is a constant, no variable
                return;
            }
            if let HirExpr::Const(mask, _) = lhs.as_ref() {
                seed_expr_with_mask(rhs, *mask as u64, consumed);
                return;
            }
            // Non-const AND: treat fully
            seed_expr_fully(lhs, consumed);
            seed_expr_fully(rhs, consumed);
        }
        HirExpr::Cast { ty, expr: inner } if can_narrow => {
            // Narrow cast: only narrow bits of inner are consumed initially.
            let narrow_mask = type_bitmask(ty);
            seed_expr_with_mask(inner, narrow_mask, consumed);
        }
        HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs,
            rhs: shift,
            ..
        } if can_narrow => {
            // `_ = lhs >> n`: not narrowing – treat lhs fully, shift fully.
            seed_expr_fully(lhs, consumed);
            seed_expr_fully(shift, consumed);
        }
        // Everything else: fully consumed.
        _ => seed_expr_fully(expr, consumed),
    }
}

fn seed_expr_with_mask(expr: &HirExpr, mask: u64, consumed: &mut HashMap<String, u64>) {
    match expr {
        HirExpr::Var(name) => {
            let entry = consumed.entry(name.clone()).or_insert(0);
            *entry |= mask;
        }
        // Non-variable: seed children fully (conservative).
        _ => seed_expr_fully(expr, consumed),
    }
}

fn seed_expr_fully(expr: &HirExpr, consumed: &mut HashMap<String, u64>) {
    match expr {
        HirExpr::Var(name) => {
            *consumed.entry(name.clone()).or_insert(0) = u64::MAX;
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => seed_expr_fully(inner, consumed),
        HirExpr::Binary { lhs, rhs, .. } => {
            seed_expr_fully(lhs, consumed);
            seed_expr_fully(rhs, consumed);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                seed_expr_fully(a, consumed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            seed_expr_fully(base, consumed);
            seed_expr_fully(index, consumed);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            seed_expr_fully(cond, consumed);
            seed_expr_fully(then_expr, consumed);
            seed_expr_fully(else_expr, consumed);
        }
        HirExpr::AddressOfGlobal(_) => {}
    }
}

fn seed_lvalue_fully(lhs: &HirLValue, consumed: &mut HashMap<String, u64>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => seed_expr_fully(ptr, consumed),
        HirLValue::Index { base, index, .. } => {
            seed_expr_fully(base, consumed);
            seed_expr_fully(index, consumed);
        }
        HirLValue::FieldAccess { base, .. } => seed_expr_fully(base, consumed),
    }
}

// ── Phase 3: backward propagation ────────────────────────────────────────────

/// Given a definition `x = expr` and `out_consume` = consumed mask of `x`,
/// compute additional consumed contributions for variables appearing in `expr`.
fn backward_propagate(expr: &HirExpr, out_consume: u64) -> Vec<(String, u64)> {
    let mut result = Vec::new();
    backward_propagate_inner(expr, out_consume, &mut result);
    result
}

fn backward_propagate_inner(expr: &HirExpr, out_consume: u64, result: &mut Vec<(String, u64)>) {
    match expr {
        // x = y        → consumed[y] |= consumed[x]
        HirExpr::Var(name) => {
            result.push((name.clone(), out_consume));
        }

        // x = y & C    → consumed[y] |= consumed[x] & C
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(mask, _) = rhs.as_ref() {
                backward_propagate_inner(lhs, out_consume & (*mask as u64), result);
                // mask is a constant, no variable contribution
            } else if let HirExpr::Const(mask, _) = lhs.as_ref() {
                backward_propagate_inner(rhs, out_consume & (*mask as u64), result);
            } else {
                // Non-const AND: conservative
                backward_propagate_inner(lhs, out_consume, result);
                backward_propagate_inner(rhs, out_consume, result);
            }
        }

        // x = y | z   → consumed[y] |= consumed[x], consumed[z] |= consumed[x]
        // Special: x = y | C → consumed[y] |= consumed[x] & ~C (those bits not covered by C)
        HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(c, _) = rhs.as_ref() {
                // Bits covered by constant: variable contributes if constant didn't already cover
                let var_mask = out_consume & !(*c as u64);
                backward_propagate_inner(lhs, var_mask, result);
            } else if let HirExpr::Const(c, _) = lhs.as_ref() {
                let var_mask = out_consume & !(*c as u64);
                backward_propagate_inner(rhs, var_mask, result);
            } else {
                backward_propagate_inner(lhs, out_consume, result);
                backward_propagate_inner(rhs, out_consume, result);
            }
        }

        // x = y ^ z   → consumed[y] |= consumed[x], consumed[z] |= consumed[x]
        HirExpr::Binary {
            op: HirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } => {
            backward_propagate_inner(lhs, out_consume, result);
            backward_propagate_inner(rhs, out_consume, result);
        }

        // x = y << n (const) → consumed[y] |= consumed[x] >> n
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs: shift,
            ..
        } => {
            if let HirExpr::Const(n, _) = shift.as_ref() {
                let n = (*n).clamp(0, 63) as u32;
                let src_mask = out_consume >> n;
                if src_mask != 0 {
                    backward_propagate_inner(lhs, src_mask, result);
                }
            } else {
                // Variable shift: conservative
                let full = if out_consume == 0 { 0 } else { u64::MAX };
                backward_propagate_inner(lhs, full, result);
                backward_propagate_inner(shift, full, result);
            }
        }

        // x = y >> n (const, logical) → consumed[y] |= consumed[x] << n
        HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs,
            rhs: shift,
            ..
        } => {
            if let HirExpr::Const(n, _) = shift.as_ref() {
                let n = (*n).clamp(0, 63) as u32;
                let src_mask = out_consume << n;
                backward_propagate_inner(lhs, src_mask, result);
            } else {
                let full = if out_consume == 0 { 0 } else { u64::MAX };
                backward_propagate_inner(lhs, full, result);
                backward_propagate_inner(shift, full, result);
            }
        }

        // x = y >>s n (const, arithmetic) → same as logical for consumed-mask purposes
        HirExpr::Binary {
            op: HirBinaryOp::Sar,
            lhs,
            rhs: shift,
            ..
        } => {
            if let HirExpr::Const(n, _) = shift.as_ref() {
                let n = (*n).clamp(0, 63) as u32;
                // Arithmetic shift: sign-bit may replicate; treat upper bits as consumed too
                let src_mask = (out_consume << n)
                    | (if out_consume >> 63 != 0 {
                        u64::MAX << (64 - n.min(63))
                    } else {
                        0
                    });
                backward_propagate_inner(lhs, src_mask, result);
            } else {
                let full = if out_consume == 0 { 0 } else { u64::MAX };
                backward_propagate_inner(lhs, full, result);
                backward_propagate_inner(shift, full, result);
            }
        }

        // x = (NarrowType)y → consumed[y] |= consumed[x] & narrow_mask
        HirExpr::Cast { ty, expr: inner } => {
            let narrow_mask = type_bitmask(ty);
            let src_mask = out_consume & narrow_mask;
            backward_propagate_inner(inner, src_mask, result);
        }

        // x = -y  or  x = ~y → consumed[y] |= consumed[x]
        HirExpr::Unary { expr: inner, .. } => {
            backward_propagate_inner(inner, out_consume, result);
        }
        HirExpr::FieldAccess { base, .. } => {
            backward_propagate_inner(base, out_consume, result);
        }

        // x = y + z  → conservative (carry propagation makes it hard to be precise)
        HirExpr::Binary {
            op: HirBinaryOp::Add | HirBinaryOp::Sub | HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            backward_propagate_inner(lhs, out_consume, result);
            backward_propagate_inner(rhs, out_consume, result);
        }

        // Comparisons, div, mod: treat all inputs as fully consumed
        HirExpr::Binary { lhs, rhs, .. } => {
            let full = if out_consume == 0 { 0 } else { u64::MAX };
            backward_propagate_inner(lhs, full, result);
            backward_propagate_inner(rhs, full, result);
        }

        // Loads, calls, constants: no variable propagation needed (no sub-variables)
        HirExpr::Const(_, _)
        | HirExpr::Load { .. }
        | HirExpr::Call { .. }
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::PtrOffset { .. }
        | HirExpr::AggregateCopy { .. }
        | HirExpr::Index { .. }
        | HirExpr::Select { .. } => {}
    }
}

// ── Phase 4: simplification ───────────────────────────────────────────────────

fn simplify_stmts(
    stmts: &mut Vec<HirStmt>,
    consumed: &HashMap<String, u64>,
    multi_def: &std::collections::HashSet<String>,
    type_map: &HashMap<String, NirType>,
    any_changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        simplify_stmt(stmt, consumed, multi_def, type_map, any_changed);
    }
}

fn simplify_stmt(
    stmt: &mut HirStmt,
    consumed: &HashMap<String, u64>,
    multi_def: &std::collections::HashSet<String>,
    type_map: &HashMap<String, NirType>,
    any_changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            // Only simplify single-defined, non-multi-def variables.
            if multi_def.contains(name.as_str()) {
                simplify_expr(rhs, consumed, any_changed);
                return;
            }
            let out_consume = consumed.get(name.as_str()).copied().unwrap_or(0);
            simplify_assign_rhs(rhs, out_consume, consumed, type_map, any_changed);
        }
        HirStmt::Assign { lhs, rhs } => {
            simplify_lvalue(lhs, consumed, any_changed);
            simplify_expr(rhs, consumed, any_changed);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            simplify_expr(expr, consumed, any_changed);
        }
        HirStmt::VaStart { va_list, .. } => simplify_expr(va_list, consumed, any_changed),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            simplify_stmts(body, consumed, multi_def, type_map, any_changed);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                simplify_stmt(i, consumed, multi_def, type_map, any_changed);
            }
            if let Some(c) = cond {
                simplify_expr(c, consumed, any_changed);
            }
            if let Some(u) = update {
                simplify_stmt(u, consumed, multi_def, type_map, any_changed);
            }
            simplify_stmts(body, consumed, multi_def, type_map, any_changed);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            simplify_expr(cond, consumed, any_changed);
            simplify_stmts(then_body, consumed, multi_def, type_map, any_changed);
            simplify_stmts(else_body, consumed, multi_def, type_map, any_changed);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            simplify_expr(expr, consumed, any_changed);
            for case in cases.iter_mut() {
                simplify_stmts(&mut case.body, consumed, multi_def, type_map, any_changed);
            }
            simplify_stmts(default, consumed, multi_def, type_map, any_changed);
        }
        _ => {}
    }
}

/// Simplify the RHS of `name = rhs` given the consumed mask of `name`.
fn simplify_assign_rhs(
    rhs: &mut HirExpr,
    out_consume: u64,
    consumed: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
    any_changed: &mut bool,
) {
    // Rule 1: `x = y | C` where no consumed bit overlaps with C → `x = y`
    // This removes dead OR-with-constant branches.
    if let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs: rhs_inner,
        ty,
    } = rhs
    {
        if let HirExpr::Const(c, _) = rhs_inner.as_ref() {
            let dead_bits = *c as u64 & !out_consume;
            if dead_bits == *c as u64 && *c != 0 {
                // All bits of C are never consumed → strip the OR
                let inner = *lhs.clone();
                *rhs = inner;
                *any_changed = true;
                // Recurse into the new simpler expr
                simplify_expr(rhs, consumed, any_changed);
                return;
            }
        }
        if let HirExpr::Const(c, _) = lhs.as_ref() {
            let dead_bits = *c as u64 & !out_consume;
            if dead_bits == *c as u64 && *c != 0 {
                let inner = *rhs_inner.clone();
                *rhs = inner;
                *any_changed = true;
                simplify_expr(rhs, consumed, any_changed);
                return;
            }
        }
    }

    // Rule 2: `x = y & C` where C covers more than consumed[x] AND the inner
    //  type of y is already narrow enough → we can replace with cast.
    // (Safe but conservative; subvar_flow handles the full case.)

    // Rule 3: a widening cast is redundant when consumers only need bits that
    // already exist in the source. A narrowing cast always changes the value.
    if let HirExpr::Cast { ty, expr: inner } = rhs {
        let source_ty = expr_type_with_bindings(inner, type_map);
        let can_remove = type_width(ty)
            .zip(type_width(&source_ty))
            .is_some_and(|(target_bits, source_bits)| target_bits >= source_bits);
        let source_mask = type_bitmask(&source_ty);
        if can_remove && source_mask != 0 && (out_consume & !source_mask) == 0 {
            // Consumer only needs bits within inner's natural width → remove cast
            let new_expr = *inner.clone();
            *rhs = new_expr;
            *any_changed = true;
            simplify_expr(rhs, consumed, any_changed);
            return;
        }
    }

    // Default: recurse into the RHS children.
    simplify_expr(rhs, consumed, any_changed);
}

fn simplify_expr(expr: &mut HirExpr, consumed: &HashMap<String, u64>, any_changed: &mut bool) {
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            simplify_expr(lhs, consumed, any_changed);
            simplify_expr(rhs, consumed, any_changed);
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            simplify_expr(inner, consumed, any_changed);
        }
        HirExpr::Load { ptr, .. } | HirExpr::PtrOffset { base: ptr, .. } => {
            simplify_expr(ptr, consumed, any_changed);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                simplify_expr(a, consumed, any_changed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            simplify_expr(base, consumed, any_changed);
            simplify_expr(index, consumed, any_changed);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            simplify_expr(cond, consumed, any_changed);
            simplify_expr(then_expr, consumed, any_changed);
            simplify_expr(else_expr, consumed, any_changed);
        }
        HirExpr::AggregateCopy { src, .. } => simplify_expr(src, consumed, any_changed),
        _ => {}
    }
}

fn simplify_lvalue(lhs: &mut HirLValue, consumed: &HashMap<String, u64>, any_changed: &mut bool) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => simplify_expr(ptr, consumed, any_changed),
        HirLValue::Index { base, index, .. } => {
            simplify_expr(base, consumed, any_changed);
            simplify_expr(index, consumed, any_changed);
        }
        HirLValue::FieldAccess { base, .. } => simplify_expr(base, consumed, any_changed),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return the bitmask for bits that are valid in `ty`.
/// Returns `u64::MAX` for unknown / pointer types (conservative).
fn type_bitmask(ty: &NirType) -> u64 {
    match ty {
        NirType::Bool => 0x1,
        NirType::Int { bits, .. } => {
            if *bits == 0 {
                return u64::MAX;
            }
            if *bits >= 64 {
                return u64::MAX;
            }
            (1u64 << bits) - 1
        }
        _ => u64::MAX,
    }
}

fn type_width(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } if *bits > 0 => Some(*bits),
        _ => None,
    }
}

fn expr_type_with_bindings(expr: &HirExpr, type_map: &HashMap<String, NirType>) -> NirType {
    match expr {
        HirExpr::Var(name) => type_map.get(name).cloned().unwrap_or(NirType::Unknown),
        _ => expr_type(expr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uint(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    #[test]
    fn preserves_narrowing_cast_from_wide_binding() {
        let mut func = HirFunction {
            name: "narrow_lane".into(),
            locals: vec![
                NirBinding {
                    name: "wide".into(),
                    ty: uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                NirBinding {
                    name: "narrowed".into(),
                    ty: uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            return_type: uint(32),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("narrowed".into()),
                    rhs: HirExpr::Cast {
                        ty: uint(8),
                        expr: Box::new(HirExpr::Var("wide".into())),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("narrowed".into()))),
            ],
            ..Default::default()
        };

        assert!(!apply_bit_consume_dead_code_pass(&mut func));
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                rhs: HirExpr::Cast {
                    ty: NirType::Int { bits: 8, .. },
                    ..
                },
                ..
            }
        ));
    }
}
