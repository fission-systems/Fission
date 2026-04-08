/// Local Common Subexpression Elimination (CSE) for HIR.
///
/// Within each **linear statement list** (before any control-flow branch),
/// identifies identical pure sub-expressions that are computed more than once
/// and replaces later occurrences with the variable that holds the first result.
///
/// ## Algorithm (Local GVN, single basic-block scope)
///
/// ```text
/// apply_cse_pass(func):
///   1. For each linear prefix of the function body (up to the first branch):
///      - Maintain expr_map: canonical ExprKey → binding name already holding
///        that expression.
///      - For each Assign { lhs: Var(y), rhs: E }:
///          a. If E is pure and ExprKey(E) is in expr_map → replace rhs with
///             Var(existing).  This makes y = existing which copy_propagation
///             will later inline.
///          b. If not in map → insert (ExprKey(E), y).
///      - When a Var(z) is defined (assigned), invalidate all map entries whose
///        expression contains z.
///   2. Recurse into if/while/for/switch arms with a fresh map each time (do
///      not hoist across branches — that requires global GVN with SSA).
///   3. After the pass, run copy_propagation to clean up y = existing chains.
/// ```
///
/// ## Soundness
///
/// - Only `Load`-free, `Call`-free expressions are eligible (pure).
/// - Map entries are invalidated when any of their operands is re-defined.
/// - Branches start with a fresh map (conservative: no value propagation
///   across join points).
///
/// ## References
///
/// - Ghidra `ActionMultiCse` (coreaction.cc): local CSE concept
/// - LLVM `GVN.cpp`: global value numbering (superset of this)
/// - Cooper & Torczon "Engineering a Compiler" §8.4
use super::*;
use std::collections::HashMap;

/// Apply CSE to the function body.  Returns `true` if any substitution was made.
pub(super) fn apply_cse_pass(func: &mut HirFunction) -> bool {
    let mut map: ExprMap = HashMap::new();
    cse_stmts(&mut func.body, &mut map)
}

/// Map from canonical expression key → the variable name that first computed it.
type ExprMap = HashMap<ExprKey, String>;

/// A canonical, hashable representation of a pure HIR expression.
///
/// We represent it as a `String` key built from the expression tree.  This is
/// simple, allocation-heavy but correct.  A typed arena would be more efficient
/// but adds complexity not warranted here.
type ExprKey = String;

/// Compute the canonical key for an expression, or `None` if not eligible
/// (contains Load, Call, AggregateCopy).
fn expr_key(expr: &HirExpr) -> Option<ExprKey> {
    match expr {
        HirExpr::Const(v, ty) => {
            Some(format!("K({},{})", v, type_key(ty)))
        }
        HirExpr::Var(name) => Some(format!("V({})", name)),
        HirExpr::Cast { ty, expr: inner } => {
            let ik = expr_key(inner)?;
            Some(format!("C({},{})", type_key(ty), ik))
        }
        HirExpr::Unary { op, expr: inner, ty } => {
            let ik = expr_key(inner)?;
            Some(format!("U({:?},{},{})", op, type_key(ty), ik))
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let lk = expr_key(lhs)?;
            let rk = expr_key(rhs)?;
            // For commutative ops, canonicalise order to eliminate duplicates.
            let (lk, rk) = if is_commutative(*op) && lk > rk {
                (rk, lk)
            } else {
                (lk, rk)
            };
            Some(format!("B({:?},{},{},{})", op, type_key(ty), lk, rk))
        }
        HirExpr::PtrOffset { base, offset } => {
            let bk = expr_key(base)?;
            Some(format!("P({},{})", offset, bk))
        }
        // Impure / complex — not eligible.
        HirExpr::Load { .. } | HirExpr::Call { .. } | HirExpr::AggregateCopy { .. }
        | HirExpr::Index { .. } => None,
    }
}

fn type_key(ty: &NirType) -> String {
    match ty {
        NirType::Unknown => "?".to_string(),
        NirType::Bool => "b".to_string(),
        NirType::Int { bits, signed } => format!("i{}s{}", bits, if *signed { 1 } else { 0 }),
        NirType::Ptr(_) => "p".to_string(),
        NirType::Aggregate { size, .. } => format!("a{}", size),
        NirType::Float { bits } => format!("f{}", bits),
    }
}

fn is_commutative(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::Add
            | HirBinaryOp::Mul
            | HirBinaryOp::And
            | HirBinaryOp::Or
            | HirBinaryOp::Xor
            | HirBinaryOp::Eq
            | HirBinaryOp::Ne
            | HirBinaryOp::LogicalAnd
            | HirBinaryOp::LogicalOr
    )
}

/// Invalidate map entries that depend on `defined_var`.
fn invalidate(map: &mut ExprMap, defined_var: &str) {
    // Remove entries where the key string contains V(defined_var) as a component.
    let marker = format!("V({})", defined_var);
    map.retain(|k, _| !k.contains(&marker));
}

/// Process a statement list with CSE.  `map` accumulates known expressions.
/// Returns `true` if any substitution was made.
fn cse_stmts(stmts: &mut Vec<HirStmt>, map: &mut ExprMap) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        if cse_stmt(stmt, map) {
            changed = true;
        }
    }
    changed
}

fn cse_stmt(stmt: &mut HirStmt, map: &mut ExprMap) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            // Try to substitute rhs with a known equivalent variable.
            let mut changed = false;
            if let HirLValue::Var(target) = lhs {
                if let Some(key) = expr_key(rhs) {
                    if let Some(existing) = map.get(&key) {
                        // Replace rhs with Var(existing).
                        let existing_name = existing.clone();
                        if existing_name != *target {
                            *rhs = HirExpr::Var(existing_name);
                            changed = true;
                        }
                    } else {
                        // Record this expression → variable mapping.
                        map.insert(key, target.clone());
                    }
                }
                // Invalidate any cached expression that uses this variable.
                invalidate(map, target.as_str());
            } else {
                // Memory write — invalidate everything conservatively
                // (we can't know what a store through a pointer might alias).
                map.clear();
            }
            changed
        }
        // For branches: recurse with a fresh map clone (no propagation across arms).
        HirStmt::If { cond: _, then_body, else_body } => {
            let mut then_map = map.clone();
            let mut else_map = map.clone();
            let mut changed = cse_stmts(then_body, &mut then_map);
            if cse_stmts(else_body, &mut else_map) { changed = true; }
            // After the if, the map is cleared (join point — values may differ).
            map.clear();
            changed
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            // Loop body: fresh map (loop may execute 0 or many times).
            let mut loop_map = HashMap::new();
            let changed = cse_stmts(body, &mut loop_map);
            // After the loop, the outer map is unchanged (loop didn't run = no defs).
            changed
        }
        HirStmt::For { init, body, update, .. } => {
            let mut changed = false;
            if let Some(s) = init { if cse_stmt(s, map) { changed = true; } }
            let mut loop_map = HashMap::new();
            if cse_stmts(body, &mut loop_map) { changed = true; }
            if let Some(s) = update {
                let mut u_map = HashMap::new();
                if cse_stmt(s, &mut u_map) { changed = true; }
            }
            changed
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases.iter_mut() {
                let mut arm_map = map.clone();
                if cse_stmts(&mut case.body, &mut arm_map) { changed = true; }
            }
            let mut def_map = map.clone();
            if cse_stmts(default, &mut def_map) { changed = true; }
            map.clear();
            changed
        }
        HirStmt::Block(body) => cse_stmts(body, map),
        _ => false,
    }
}
