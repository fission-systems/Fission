use crate::prelude::*;
use fission_midend_core::util_dir::expr_type;
use crate::HashMap;

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
pub fn apply_bit_consume_dead_code_pass(func: &mut DirFunction) -> bool {
    // --- Phase 1: collect definitions and multi-def variables ---
    let mut def_map: HashMap<String, DirExpr> = HashMap::default();
    let mut multi_def: crate::HashSet<String> = crate::HashSet::default();
    collect_definitions(&func.body, &mut def_map, &mut multi_def);

    // --- Phase 2: build initial consumed seeds from all use sites ---
    let mut consumed: HashMap<String, u64> = HashMap::default();
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
    stmts: &[DirStmt],
    def_map: &mut HashMap<String, DirExpr>,
    multi_def: &mut crate::HashSet<String>,
) {
    for stmt in stmts {
        collect_def_stmt(stmt, def_map, multi_def);
    }
}

fn collect_def_stmt(
    stmt: &DirStmt,
    def_map: &mut HashMap<String, DirExpr>,
    multi_def: &mut crate::HashSet<String>,
) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => {
            if def_map.contains_key(name.as_str()) {
                multi_def.insert(name.clone());
            } else {
                def_map.insert(name.clone(), rhs.clone());
            }
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            collect_definitions(body, def_map, multi_def);
        }
        DirStmt::For {
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
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_definitions(then_body, def_map, multi_def);
            collect_definitions(else_body, def_map, multi_def);
        }
        DirStmt::Switch { cases, default, .. } => {
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
    stmts: &[DirStmt],
    consumed: &mut HashMap<String, u64>,
    def_map: &HashMap<String, DirExpr>,
    multi_def: &crate::HashSet<String>,
) {
    for stmt in stmts {
        seed_stmt(stmt, consumed, def_map, multi_def);
    }
}

fn seed_stmt(
    stmt: &DirStmt,
    consumed: &mut HashMap<String, u64>,
    def_map: &HashMap<String, DirExpr>,
    multi_def: &crate::HashSet<String>,
) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => {
            // The RHS seeds the consumed mask for variables it reads.
            // This is the forward-seed pass: we look at how `rhs` reads variables.
            seed_expr_uses(rhs, consumed, true /*can_narrow*/);
        }
        DirStmt::Assign { lhs, rhs } => {
            // Memory write: all operands fully consumed.
            seed_lvalue_fully(lhs, consumed);
            seed_expr_fully(rhs, consumed);
        }
        DirStmt::Expr(expr) => seed_expr_fully(expr, consumed),
        DirStmt::Return(Some(expr)) => seed_expr_fully(expr, consumed),
        DirStmt::Return(None) => {}
        DirStmt::Break | DirStmt::Continue | DirStmt::Label(_) | DirStmt::Goto(_) => {}
        DirStmt::VaStart { va_list, .. } => seed_expr_fully(va_list, consumed),
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            collect_consumed_seeds(body, consumed, def_map, multi_def);
        }
        DirStmt::For {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            seed_expr_fully(cond, consumed);
            collect_consumed_seeds(then_body, consumed, def_map, multi_def);
            collect_consumed_seeds(else_body, consumed, def_map, multi_def);
        }
        DirStmt::Switch {
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
fn seed_expr_uses(expr: &DirExpr, consumed: &mut HashMap<String, u64>, can_narrow: bool) {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if can_narrow => {
            // `_ = lhs & const_mask`: only those bits of lhs are initially consumed.
            if let DirExpr::Const(mask, _) = rhs.as_ref() {
                seed_expr_with_mask(lhs, *mask as u64, consumed);
                // mask itself is a constant, no variable
                return;
            }
            if let DirExpr::Const(mask, _) = lhs.as_ref() {
                seed_expr_with_mask(rhs, *mask as u64, consumed);
                return;
            }
            // Non-const AND: treat fully
            seed_expr_fully(lhs, consumed);
            seed_expr_fully(rhs, consumed);
        }
        DirExpr::Cast { ty, expr: inner } if can_narrow => {
            // Narrow cast: only narrow bits of inner are consumed initially.
            let narrow_mask = type_bitmask(ty);
            seed_expr_with_mask(inner, narrow_mask, consumed);
        }
        DirExpr::Binary {
            op: DirBinaryOp::Shr,
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

fn seed_expr_with_mask(expr: &DirExpr, mask: u64, consumed: &mut HashMap<String, u64>) {
    match expr {
        DirExpr::Var(name) => {
            let entry = consumed.entry(name.clone()).or_insert(0);
            *entry |= mask;
        }
        // Non-variable: seed children fully (conservative).
        _ => seed_expr_fully(expr, consumed),
    }
}

fn seed_expr_fully(expr: &DirExpr, consumed: &mut HashMap<String, u64>) {
    match expr {
        DirExpr::Var(name) => {
            *consumed.entry(name.clone()).or_insert(0) = u64::MAX;
        }
        DirExpr::Const(_, _) => {}
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => seed_expr_fully(inner, consumed),
        DirExpr::Binary { lhs, rhs, .. } => {
            seed_expr_fully(lhs, consumed);
            seed_expr_fully(rhs, consumed);
        }
        DirExpr::Call { args, .. } => {
            for a in args {
                seed_expr_fully(a, consumed);
            }
        }
        DirExpr::Index { base, index, .. } => {
            seed_expr_fully(base, consumed);
            seed_expr_fully(index, consumed);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            seed_expr_fully(cond, consumed);
            seed_expr_fully(then_expr, consumed);
            seed_expr_fully(else_expr, consumed);
        }
        DirExpr::AddressOfGlobal(_) => {}
    }
}

fn seed_lvalue_fully(lhs: &DirLValue, consumed: &mut HashMap<String, u64>) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => seed_expr_fully(ptr, consumed),
        DirLValue::Index { base, index, .. } => {
            seed_expr_fully(base, consumed);
            seed_expr_fully(index, consumed);
        }
        DirLValue::FieldAccess { base, .. } => seed_expr_fully(base, consumed),
    }
}

// ── Phase 3: backward propagation ────────────────────────────────────────────

/// Given a definition `x = expr` and `out_consume` = consumed mask of `x`,
/// compute additional consumed contributions for variables appearing in `expr`.
fn backward_propagate(expr: &DirExpr, out_consume: u64) -> Vec<(String, u64)> {
    let mut result = Vec::new();
    backward_propagate_inner(expr, out_consume, &mut result);
    result
}

fn backward_propagate_inner(expr: &DirExpr, out_consume: u64, result: &mut Vec<(String, u64)>) {
    match expr {
        // x = y        → consumed[y] |= consumed[x]
        DirExpr::Var(name) => {
            result.push((name.clone(), out_consume));
        }

        // x = y & C    → consumed[y] |= consumed[x] & C
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Const(mask, _) = rhs.as_ref() {
                backward_propagate_inner(lhs, out_consume & (*mask as u64), result);
                // mask is a constant, no variable contribution
            } else if let DirExpr::Const(mask, _) = lhs.as_ref() {
                backward_propagate_inner(rhs, out_consume & (*mask as u64), result);
            } else {
                // Non-const AND: conservative
                backward_propagate_inner(lhs, out_consume, result);
                backward_propagate_inner(rhs, out_consume, result);
            }
        }

        // x = y | z   → consumed[y] |= consumed[x], consumed[z] |= consumed[x]
        // Special: x = y | C → consumed[y] |= consumed[x] & ~C (those bits not covered by C)
        DirExpr::Binary {
            op: DirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Const(c, _) = rhs.as_ref() {
                // Bits covered by constant: variable contributes if constant didn't already cover
                let var_mask = out_consume & !(*c as u64);
                backward_propagate_inner(lhs, var_mask, result);
            } else if let DirExpr::Const(c, _) = lhs.as_ref() {
                let var_mask = out_consume & !(*c as u64);
                backward_propagate_inner(rhs, var_mask, result);
            } else {
                backward_propagate_inner(lhs, out_consume, result);
                backward_propagate_inner(rhs, out_consume, result);
            }
        }

        // x = y ^ z   → consumed[y] |= consumed[x], consumed[z] |= consumed[x]
        DirExpr::Binary {
            op: DirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } => {
            backward_propagate_inner(lhs, out_consume, result);
            backward_propagate_inner(rhs, out_consume, result);
        }

        // x = y << n (const) → consumed[y] |= consumed[x] >> n
        DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs,
            rhs: shift,
            ..
        } => {
            if let DirExpr::Const(n, _) = shift.as_ref() {
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
        DirExpr::Binary {
            op: DirBinaryOp::Shr,
            lhs,
            rhs: shift,
            ..
        } => {
            if let DirExpr::Const(n, _) = shift.as_ref() {
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
        DirExpr::Binary {
            op: DirBinaryOp::Sar,
            lhs,
            rhs: shift,
            ..
        } => {
            if let DirExpr::Const(n, _) = shift.as_ref() {
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
        DirExpr::Cast { ty, expr: inner } => {
            let narrow_mask = type_bitmask(ty);
            let src_mask = out_consume & narrow_mask;
            backward_propagate_inner(inner, src_mask, result);
        }

        // x = -y  or  x = ~y → consumed[y] |= consumed[x]
        DirExpr::Unary { expr: inner, .. } => {
            backward_propagate_inner(inner, out_consume, result);
        }
        DirExpr::FieldAccess { base, .. } => {
            backward_propagate_inner(base, out_consume, result);
        }

        // x = y + z  → conservative (carry propagation makes it hard to be precise)
        DirExpr::Binary {
            op: DirBinaryOp::Add | DirBinaryOp::Sub | DirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            backward_propagate_inner(lhs, out_consume, result);
            backward_propagate_inner(rhs, out_consume, result);
        }

        // Comparisons, div, mod: treat all inputs as fully consumed
        DirExpr::Binary { lhs, rhs, .. } => {
            let full = if out_consume == 0 { 0 } else { u64::MAX };
            backward_propagate_inner(lhs, full, result);
            backward_propagate_inner(rhs, full, result);
        }

        // Loads, calls, constants: no variable propagation needed (no sub-variables)
        DirExpr::Const(_, _)
        | DirExpr::Load { .. }
        | DirExpr::Call { .. }
        | DirExpr::AddressOfGlobal(_)
        | DirExpr::PtrOffset { .. }
        | DirExpr::AggregateCopy { .. }
        | DirExpr::Index { .. }
        | DirExpr::Select { .. } => {}
    }
}

// ── Phase 4: simplification ───────────────────────────────────────────────────

fn simplify_stmts(
    stmts: &mut Vec<DirStmt>,
    consumed: &HashMap<String, u64>,
    multi_def: &crate::HashSet<String>,
    type_map: &HashMap<String, NirType>,
    any_changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        simplify_stmt(stmt, consumed, multi_def, type_map, any_changed);
    }
}

fn simplify_stmt(
    stmt: &mut DirStmt,
    consumed: &HashMap<String, u64>,
    multi_def: &crate::HashSet<String>,
    type_map: &HashMap<String, NirType>,
    any_changed: &mut bool,
) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
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
        DirStmt::Assign { lhs, rhs } => {
            simplify_lvalue(lhs, consumed, any_changed);
            simplify_expr(rhs, consumed, any_changed);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            simplify_expr(expr, consumed, any_changed);
        }
        DirStmt::VaStart { va_list, .. } => simplify_expr(va_list, consumed, any_changed),
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            simplify_stmts(body, consumed, multi_def, type_map, any_changed);
        }
        DirStmt::For {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            simplify_expr(cond, consumed, any_changed);
            simplify_stmts(then_body, consumed, multi_def, type_map, any_changed);
            simplify_stmts(else_body, consumed, multi_def, type_map, any_changed);
        }
        DirStmt::Switch {
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
    rhs: &mut DirExpr,
    out_consume: u64,
    consumed: &HashMap<String, u64>,
    type_map: &HashMap<String, NirType>,
    any_changed: &mut bool,
) {
    // Rule 1: `x = y | C` where no consumed bit overlaps with C → `x = y`
    // This removes dead OR-with-constant branches.
    if let DirExpr::Binary {
        op: DirBinaryOp::Or,
        lhs,
        rhs: rhs_inner,
        ty,
    } = rhs
    {
        if let DirExpr::Const(c, _) = rhs_inner.as_ref() {
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
        if let DirExpr::Const(c, _) = lhs.as_ref() {
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
    if let DirExpr::Cast { ty, expr: inner } = rhs {
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

fn simplify_expr(expr: &mut DirExpr, consumed: &HashMap<String, u64>, any_changed: &mut bool) {
    match expr {
        DirExpr::Binary { lhs, rhs, .. } => {
            simplify_expr(lhs, consumed, any_changed);
            simplify_expr(rhs, consumed, any_changed);
        }
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            simplify_expr(inner, consumed, any_changed);
        }
        DirExpr::Load { ptr, .. } | DirExpr::PtrOffset { base: ptr, .. } => {
            simplify_expr(ptr, consumed, any_changed);
        }
        DirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                simplify_expr(a, consumed, any_changed);
            }
        }
        DirExpr::Index { base, index, .. } => {
            simplify_expr(base, consumed, any_changed);
            simplify_expr(index, consumed, any_changed);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            simplify_expr(cond, consumed, any_changed);
            simplify_expr(then_expr, consumed, any_changed);
            simplify_expr(else_expr, consumed, any_changed);
        }
        DirExpr::AggregateCopy { src, .. } => simplify_expr(src, consumed, any_changed),
        _ => {}
    }
}

fn simplify_lvalue(lhs: &mut DirLValue, consumed: &HashMap<String, u64>, any_changed: &mut bool) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => simplify_expr(ptr, consumed, any_changed),
        DirLValue::Index { base, index, .. } => {
            simplify_expr(base, consumed, any_changed);
            simplify_expr(index, consumed, any_changed);
        }
        DirLValue::FieldAccess { base, .. } => simplify_expr(base, consumed, any_changed),
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

fn expr_type_with_bindings(expr: &DirExpr, type_map: &HashMap<String, NirType>) -> NirType {
    match expr {
        DirExpr::Var(name) => type_map.get(name).cloned().unwrap_or(NirType::Unknown),
        _ => expr_type(expr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    fn uint(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    #[test]
    fn preserves_narrowing_cast_from_wide_binding() {
        let mut func = DirFunction {
            name: "narrow_lane".into(),
            locals: vec![
                DirBinding {
                    name: "wide".into(),
                    ty: uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                DirBinding {
                    name: "narrowed".into(),
                    ty: uint(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            return_type: uint(32),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("narrowed".into()),
                    rhs: DirExpr::Cast {
                        ty: uint(8),
                        expr: Box::new(DirExpr::Var("wide".into())),
                    },
                },
                DirStmt::Return(Some(DirExpr::Var("narrowed".into()))),
            ],
            ..Default::default()
        };

        assert!(!apply_bit_consume_dead_code_pass(&mut func));
        assert!(matches!(
            &func.body[0],
            DirStmt::Assign {
                rhs: DirExpr::Cast {
                    ty: NirType::Int { bits: 8, .. },
                    ..
                },
                ..
            }
        ));
    }
}
