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
use super::super::analysis::expr_key::{PureExprMap, invalidate_pure_map, pure_expr_key};
use super::super::*;
use std::collections::{HashMap, HashSet};

/// Apply CSE to the function body.  Returns `true` if any substitution was made.
pub(crate) fn apply_cse_pass(func: &mut HirFunction) -> bool {
    let mut map: PureExprMap = HashMap::new();
    let non_value_representatives = collect_non_value_representatives(func);
    cse_stmts(&mut func.body, &mut map, &non_value_representatives)
}

fn collect_non_value_representatives(func: &HirFunction) -> HashSet<String> {
    func.locals
        .iter()
        .filter(|binding| {
            matches!(
                binding.origin,
                Some(
                    NirBindingOrigin::StackOffset(_)
                        | NirBindingOrigin::HomeSlot(_)
                        | NirBindingOrigin::OutgoingArgSlot(_)
                        | NirBindingOrigin::ReturnScaffold
                        | NirBindingOrigin::DerivedFromStackOffset(_)
                )
            )
        })
        .map(|binding| binding.name.clone())
        .collect()
}

fn is_cse_representative_name(name: &str, non_value_representatives: &HashSet<String>) -> bool {
    if non_value_representatives.contains(name) {
        return false;
    }
    !(name.starts_with("home_")
        || name.starts_with("local_")
        || name.starts_with("arg_out_")
        || name.starts_with("ret_scaffold_"))
}

/// Process a statement list with CSE.  `map` accumulates known expressions.
/// Returns `true` if any substitution was made.
fn cse_stmts(
    stmts: &mut Vec<HirStmt>,
    map: &mut PureExprMap,
    non_value_representatives: &HashSet<String>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        if cse_stmt(stmt, map, non_value_representatives) {
            changed = true;
        }
    }
    changed
}

fn cse_stmt(
    stmt: &mut HirStmt,
    map: &mut PureExprMap,
    non_value_representatives: &HashSet<String>,
) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            // Try to substitute rhs with a known equivalent variable.
            let mut changed = false;
            if let HirLValue::Var(target) = lhs {
                if let Some(key) = pure_expr_key(rhs) {
                    if let Some(existing) = map.get(&key) {
                        // Replace rhs with Var(existing).
                        let existing_name = existing.clone();
                        if existing_name != *target {
                            *rhs = HirExpr::Var(existing_name);
                            changed = true;
                        }
                    } else if is_cse_representative_name(target, non_value_representatives) {
                        // Record this expression → variable mapping.
                        map.insert(key, target.clone());
                    }
                }
                // Invalidate any cached expression that uses this variable.
                invalidate_pure_map(map, target.as_str());
            } else {
                // Memory write — invalidate everything conservatively
                // (we can't know what a store through a pointer might alias).
                map.clear();
            }
            changed
        }
        // For branches: recurse with a fresh map clone (no propagation across arms).
        HirStmt::If {
            cond: _,
            then_body,
            else_body,
        } => {
            let mut then_map = map.clone();
            let mut else_map = map.clone();
            let mut changed = cse_stmts(then_body, &mut then_map, non_value_representatives);
            if cse_stmts(else_body, &mut else_map, non_value_representatives) {
                changed = true;
            }
            // After the if, the map is cleared (join point — values may differ).
            map.clear();
            changed
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            // Loop body: fresh map (loop may execute 0 or many times).
            let mut loop_map = HashMap::new();
            let changed = cse_stmts(body, &mut loop_map, non_value_representatives);
            // After the loop, the outer map is unchanged (loop didn't run = no defs).
            changed
        }
        HirStmt::For {
            init, body, update, ..
        } => {
            let mut changed = false;
            if let Some(s) = init {
                if cse_stmt(s, map, non_value_representatives) {
                    changed = true;
                }
            }
            let mut loop_map = HashMap::new();
            if cse_stmts(body, &mut loop_map, non_value_representatives) {
                changed = true;
            }
            if let Some(s) = update {
                let mut u_map = HashMap::new();
                if cse_stmt(s, &mut u_map, non_value_representatives) {
                    changed = true;
                }
            }
            changed
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases.iter_mut() {
                let mut arm_map = map.clone();
                if cse_stmts(&mut case.body, &mut arm_map, non_value_representatives) {
                    changed = true;
                }
            }
            let mut def_map = map.clone();
            if cse_stmts(default, &mut def_map, non_value_representatives) {
                changed = true;
            }
            map.clear();
            changed
        }
        HirStmt::Block(body) => cse_stmts(body, map, non_value_representatives),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn assign(lhs: &str, rhs: HirExpr) -> HirStmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs.to_owned()),
            rhs,
        }
    }

    fn local(name: &str, origin: NirBindingOrigin) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty: u32_ty(),
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        }
    }

    #[test]
    fn stack_slots_do_not_become_cse_value_representatives() {
        let mut func = HirFunction {
            name: "cse_stack_slot_rep".to_owned(),
            locals: vec![local("saved_param", NirBindingOrigin::HomeSlot(0))],
            body: vec![
                assign("saved_param", HirExpr::Var("param_1".to_owned())),
                assign("uVar19", HirExpr::Var("param_1".to_owned())),
            ],
            ..Default::default()
        };

        assert!(!apply_cse_pass(&mut func));
        let HirStmt::Assign { rhs, .. } = &func.body[1] else {
            panic!("expected second assignment");
        };
        assert!(matches!(rhs, HirExpr::Var(name) if name == "param_1"));
    }

    #[test]
    fn temp_representatives_still_drive_local_cse() {
        let mut func = HirFunction {
            name: "cse_temp_rep".to_owned(),
            body: vec![
                assign(
                    "uVar1",
                    HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                        rhs: Box::new(HirExpr::Const(1, u32_ty())),
                        ty: u32_ty(),
                    },
                ),
                assign(
                    "uVar2",
                    HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                        rhs: Box::new(HirExpr::Const(1, u32_ty())),
                        ty: u32_ty(),
                    },
                ),
            ],
            ..Default::default()
        };

        assert!(apply_cse_pass(&mut func));
        let HirStmt::Assign { rhs, .. } = &func.body[1] else {
            panic!("expected second assignment");
        };
        assert!(matches!(rhs, HirExpr::Var(name) if name == "uVar1"));
    }
}
