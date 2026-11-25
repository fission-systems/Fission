use crate::HashSet;
use crate::prelude::*;

/// Collect variable names that appear as the LHS of an assignment where the RHS
/// is a bitwise-integer-only binary operation (And, Or, Xor, Shl, Shr, Sar).
/// These variables must have an integer (not pointer) type to compile as valid C.
fn collect_bitop_lhs_vars_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_bitop_lhs_vars_stmt(stmt, out);
    }
}

fn collect_bitop_lhs_vars_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => {
            if rhs_is_integer_bitop(rhs) {
                out.insert(name.clone());
            }
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            collect_bitop_lhs_vars_stmts(body, out);
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_bitop_lhs_vars_stmts(then_body, out);
            collect_bitop_lhs_vars_stmts(else_body, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_bitop_lhs_vars_stmts(&case.body, out);
            }
            collect_bitop_lhs_vars_stmts(default, out);
        }
        _ => {}
    }
}

fn rhs_is_integer_bitop(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Binary { op, .. } => matches!(
            op,
            PreHirBinaryOp::And
                | PreHirBinaryOp::Or
                | PreHirBinaryOp::Xor
                | PreHirBinaryOp::Shl
                | PreHirBinaryOp::Shr
                | PreHirBinaryOp::Sar
        ),
        PreHirExpr::Cast { expr: inner, .. } => rhs_is_integer_bitop(inner),
        _ => false,
    }
}

/// Safety-net pass: if a local binding has `NirType::Ptr(_)` but is used as the
/// destination of a bitwise-integer-only operation, coerce its type to `ulonglong`
/// so that the generated C compiles cleanly.
///
/// This handles x86-64 idioms where a pointer difference is computed, stored in
/// a pointer-typed slot, and then bit-masked (e.g. `ptr_diff &= 4`).
pub fn coerce_ptr_typed_bitop_vars(func: &mut PreHirFunction) -> bool {
    // Collect all LHS names that receive a bitwise-integer RHS.
    let mut bitop_lhs: HashSet<String> = HashSet::default();
    collect_bitop_lhs_vars_stmts(&func.body, &mut bitop_lhs);
    if bitop_lhs.is_empty() {
        return false;
    }

    let int64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };

    let mut changed = false;
    for binding in &mut func.locals {
        if bitop_lhs.contains(&binding.name) && matches!(binding.ty, NirType::Ptr(_)) {
            binding.ty = int64_ty.clone();
            // Drop any pointer initializer so it doesn't conflict with the new integer type.
            if binding.initializer.is_some() {
                binding.initializer = None;
            }
            changed = true;
        }
    }
    changed
}
