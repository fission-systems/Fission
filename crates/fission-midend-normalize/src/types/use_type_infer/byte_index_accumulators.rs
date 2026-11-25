use super::expression_types::collect_known_binding_types;
use crate::prelude::*;
use crate::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct ByteIndexAccumulatorEvidence {
    pub(super) def_count: usize,
    byte_seed_defs: usize,
    pub(super) byte_update_defs: usize,
    byte_pointer_offset_uses: usize,
    pub(super) disallowed_uses: usize,
}

fn is_byte_int_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { bits: 8, .. })
}

fn is_byte_pointer_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Ptr(pointee) if is_byte_int_type(pointee.as_ref()))
}

fn is_byte_expr(expr: &PreHirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            known_binding_types.get(name).is_some_and(is_byte_int_type)
        }
        PreHirExpr::Const(value, ty) => is_byte_int_type(ty) || (0..=0xff).contains(value),
        PreHirExpr::Load { ty, .. }
        | PreHirExpr::Index { elem_ty: ty, .. }
        | PreHirExpr::FieldAccess { ty, .. } => is_byte_int_type(ty),
        PreHirExpr::Cast { ty, expr } => {
            is_byte_int_type(ty) || is_byte_expr(expr, known_binding_types)
        }
        _ => false,
    }
}

fn is_byte_pointer_expr(expr: &PreHirExpr, known_binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => known_binding_types
            .get(name)
            .is_some_and(is_byte_pointer_type),
        PreHirExpr::Cast { ty, expr } => {
            is_byte_pointer_type(ty) || is_byte_pointer_expr(expr, known_binding_types)
        }
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            is_byte_pointer_expr(base, known_binding_types)
        }
        _ => false,
    }
}

fn expr_is_var(expr: &PreHirExpr, name: &str) -> bool {
    matches!(expr, PreHirExpr::Var(var_name) if var_name == name)
}

fn is_byte_accumulator_update(
    expr: &PreHirExpr,
    name: &str,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    let PreHirExpr::Binary { op, lhs, rhs, .. } = expr else {
        return false;
    };
    matches!(
        op,
        PreHirBinaryOp::Add
            | PreHirBinaryOp::Sub
            | PreHirBinaryOp::Xor
            | PreHirBinaryOp::And
            | PreHirBinaryOp::Or
    ) && ((expr_is_var(lhs, name) && is_byte_expr(rhs, known_binding_types))
        || (expr_is_var(rhs, name) && is_byte_expr(lhs, known_binding_types)))
}

fn direct_var_name(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name.as_str()),
        _ => None,
    }
}

/// Collects byte-index-accumulator evidence for every name in `candidates`
/// simultaneously in one walk over `stmts`, instead of one full-body walk
/// per candidate -- `narrow_byte_index_accumulators` used to call this once
/// per qualifying local, making it O(locals * stmts) on functions with many
/// wide temp locals. `exclude`, when set, is the one local variable whose
/// own leaf references should NOT be recorded in the current expression
/// subtree (used for the single case where a candidate's own recognized
/// self-referential accumulator update, `x = x + 1`, must not also count as
/// a disallowed use of `x` in that same right-hand side -- other candidates
/// appearing in that same subtree are still recorded normally).
pub(super) fn collect_byte_index_accumulator_evidence(
    stmts: &[PreHirStmt],
    candidates: &HashSet<String>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    for stmt in stmts {
        collect_byte_index_accumulator_evidence_stmt(
            stmt,
            candidates,
            known_binding_types,
            evidence,
        );
    }
}

fn collect_byte_index_accumulator_evidence_stmt(
    stmt: &PreHirStmt,
    candidates: &HashSet<String>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            let self_def_name = match lhs {
                PreHirLValue::Var(lhs_name) if candidates.contains(lhs_name) => {
                    Some(lhs_name.as_str())
                }
                _ => None,
            };
            if let Some(name) = self_def_name {
                let is_accum_update = is_byte_accumulator_update(rhs, name, known_binding_types);
                let ev = evidence.entry(name.to_owned()).or_default();
                ev.def_count += 1;
                if is_byte_expr(rhs, known_binding_types) {
                    ev.byte_seed_defs += 1;
                } else if is_accum_update {
                    ev.byte_update_defs += 1;
                } else {
                    ev.disallowed_uses += 1;
                }
                let exclude = if is_accum_update { Some(name) } else { None };
                collect_byte_index_accumulator_evidence_expr(
                    rhs,
                    candidates,
                    exclude,
                    known_binding_types,
                    evidence,
                );
            } else {
                collect_byte_index_accumulator_evidence_lvalue(
                    lhs,
                    candidates,
                    None,
                    known_binding_types,
                    evidence,
                );
                collect_byte_index_accumulator_evidence_expr(
                    rhs,
                    candidates,
                    None,
                    known_binding_types,
                    evidence,
                );
            }
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            collect_byte_index_accumulator_evidence_expr(
                expr,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            collect_byte_index_accumulator_evidence(
                body,
                candidates,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_byte_index_accumulator_evidence_expr(
                cond,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence(
                then_body,
                candidates,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence(
                else_body,
                candidates,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_byte_index_accumulator_evidence_stmt(
                    init,
                    candidates,
                    known_binding_types,
                    evidence,
                );
            }
            if let Some(cond) = cond {
                collect_byte_index_accumulator_evidence_expr(
                    cond,
                    candidates,
                    None,
                    known_binding_types,
                    evidence,
                );
            }
            if let Some(update) = update {
                collect_byte_index_accumulator_evidence_stmt(
                    update,
                    candidates,
                    known_binding_types,
                    evidence,
                );
            }
            collect_byte_index_accumulator_evidence(
                body,
                candidates,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_byte_index_accumulator_evidence_expr(
                expr,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
            for case in cases {
                collect_byte_index_accumulator_evidence(
                    &case.body,
                    candidates,
                    known_binding_types,
                    evidence,
                );
            }
            collect_byte_index_accumulator_evidence(
                default,
                candidates,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::VaStart { va_list, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                va_list,
                candidates,
                None,
                known_binding_types,
                evidence,
            );
        }
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_byte_index_accumulator_evidence_lvalue(
    lhs: &PreHirLValue,
    candidates: &HashSet<String>,
    exclude: Option<&str>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } | PreHirLValue::FieldAccess { base: ptr, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                ptr,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirLValue::Index { base, index, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                base,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                index,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
    }
}

fn collect_byte_index_accumulator_evidence_expr(
    expr: &PreHirExpr,
    candidates: &HashSet<String>,
    exclude: Option<&str>,
    known_binding_types: &HashMap<String, NirType>,
    evidence: &mut HashMap<String, ByteIndexAccumulatorEvidence>,
) {
    match expr {
        PreHirExpr::Var(var_name)
        | PreHirExpr::AddressOfGlobal(var_name)
        | PreHirExpr::AddressOfLocal(var_name)
            if candidates.contains(var_name) && Some(var_name.as_str()) != exclude =>
        {
            evidence
                .entry(var_name.clone())
                .or_default()
                .disallowed_uses += 1;
        }
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                inner,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if direct_var_name(lhs).is_some_and(|v| candidates.contains(v) && Some(v) != exclude)
            && is_byte_pointer_expr(rhs, known_binding_types) =>
        {
            let v = direct_var_name(lhs).expect("guard matched Some");
            evidence
                .entry(v.to_owned())
                .or_default()
                .byte_pointer_offset_uses += 1;
            // `lhs` (the identified candidate) is fully accounted for above;
            // `rhs` (the pointer operand) may still reference *other*
            // candidates and still needs its own scan.
            collect_byte_index_accumulator_evidence_expr(
                rhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if direct_var_name(rhs).is_some_and(|v| candidates.contains(v) && Some(v) != exclude)
            && is_byte_pointer_expr(lhs, known_binding_types) =>
        {
            let v = direct_var_name(rhs).expect("guard matched Some");
            evidence
                .entry(v.to_owned())
                .or_default()
                .byte_pointer_offset_uses += 1;
            collect_byte_index_accumulator_evidence_expr(
                lhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                lhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                rhs,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_byte_index_accumulator_evidence_expr(
                    arg,
                    candidates,
                    exclude,
                    known_binding_types,
                    evidence,
                );
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_byte_index_accumulator_evidence_expr(
                base,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                index,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_byte_index_accumulator_evidence_expr(
                cond,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                then_expr,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
            collect_byte_index_accumulator_evidence_expr(
                else_expr,
                candidates,
                exclude,
                known_binding_types,
                evidence,
            );
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

pub(super) fn narrow_byte_index_accumulators(func: &mut PreHirFunction) -> bool {
    let known_binding_types = collect_known_binding_types(func);
    let candidates: HashSet<String> = func
        .locals
        .iter()
        .filter(|binding| {
            binding.surface_type_name.is_none()
                && matches!(
                    binding.origin,
                    Some(NirBindingOrigin::Temp | NirBindingOrigin::TempPreserved)
                )
                && matches!(binding.ty, NirType::Int { bits, .. } if bits > 8)
        })
        .map(|binding| binding.name.clone())
        .collect();
    if candidates.is_empty() {
        return false;
    }

    let mut evidence_by_name: HashMap<String, ByteIndexAccumulatorEvidence> = HashMap::default();
    collect_byte_index_accumulator_evidence(
        &func.body,
        &candidates,
        &known_binding_types,
        &mut evidence_by_name,
    );

    let mut changed = false;
    for binding in &mut func.locals {
        if !candidates.contains(&binding.name) {
            continue;
        }
        let Some(evidence) = evidence_by_name.get(&binding.name) else {
            continue;
        };
        if evidence.def_count > 0
            && evidence.disallowed_uses == 0
            && evidence.byte_seed_defs > 0
            && evidence.byte_update_defs > 0
            && evidence.byte_pointer_offset_uses > 0
        {
            binding.ty = NirType::Int {
                bits: 8,
                signed: false,
            };
            changed = true;
        }
    }
    changed
}
