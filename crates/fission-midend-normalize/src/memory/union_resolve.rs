use crate::prelude::*;
use fission_midend_core::expr_type;

/// Access context describing the parent operation and siblings of a field access.
#[derive(Debug, Clone)]
enum AccessContext {
    Binary {
        op: HirBinaryOp,
        ty: NirType,
        is_float_op: bool,
    },
    Call {
        target: String,
        arg_idx: usize,
    },
    Cast {
        target_ty: NirType,
    },
    StoreRhs {
        rhs_ty: NirType,
    },
    LoadParent,
}

struct AccessInfo {
    access_ty: NirType,
    contexts: Vec<AccessContext>,
}

/// A Fission-native Union/Alias datatype resolution pass (equivalent to Ghidra's UnionResolve/ScoreUnionFields).
/// Identifies structure field offsets accessed as multiple overlapping types and applies
/// bidirectional context-sensitive scoring to resolve and establish the single most accurate type.
pub fn apply_union_resolve_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;

    // 1. Identify bindings with Ptr(Aggregate { .. }) type
    let mut struct_bindings = Vec::new();
    for binding in func.locals.iter().chain(func.params.iter()) {
        if let NirType::Ptr(inner) = &binding.ty {
            if let NirType::Aggregate { fields, .. } = inner.as_ref() {
                if !fields.is_empty() {
                    struct_bindings.push(binding.name.clone());
                }
            }
        }
    }

    if struct_bindings.is_empty() {
        return false;
    }

    // 2. For each structural binding, collect access info for each field offset
    for binding_name in &struct_bindings {
        // Retrieve current fields
        let mut fields = Vec::new();
        for binding in func.locals.iter().chain(func.params.iter()) {
            if binding.name == *binding_name {
                if let NirType::Ptr(inner) = &binding.ty {
                    if let NirType::Aggregate { fields: f, .. } = inner.as_ref() {
                        fields = f.clone();
                    }
                }
                break;
            }
        }

        let mut fields_updated = false;

        for field in &mut fields {
            let offset = field.offset;
            let mut accesses = Vec::new();

            // Collect accesses to binding_name at offset
            collect_accesses_in_stmts(&func.body, binding_name, offset as i64, &mut accesses);

            if accesses.is_empty() {
                continue;
            }

            // Gather candidate types. Seed with the current type and observed access types
            let mut candidates: Vec<NirType> = Vec::new();
            candidates.push(field.ty.clone());
            for access in &accesses {
                if access.access_ty != NirType::Unknown && !candidates.contains(&access.access_ty) {
                    candidates.push(access.access_ty.clone());
                }
            }

            if candidates.len() <= 1 {
                continue;
            }

            // Score each candidate type
            let mut best_ty = field.ty.clone();
            let mut best_score = i32::MIN;

            for candidate in &candidates {
                let score = score_candidate_type(candidate, &accesses);
                if score > best_score {
                    best_score = score;
                    best_ty = candidate.clone();
                }
            }

            if best_ty != field.ty {
                field.ty = best_ty;
                fields_updated = true;
                changed = true;
            }
        }

        if fields_updated {
            // Update the binding's aggregate fields
            for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
                if binding.name == *binding_name {
                    if let NirType::Ptr(inner) = &mut binding.ty {
                        if let NirType::Aggregate { fields: f, .. } = inner.as_mut() {
                            *f = fields;
                        }
                    }
                    break;
                }
            }
        }
    }

    changed
}

fn is_target_access(expr: &HirExpr, binding_name: &str, target_offset: i64) -> bool {
    match expr {
        HirExpr::Var(name) => name == binding_name && target_offset == 0,
        HirExpr::PtrOffset { base, offset } => {
            if let HirExpr::Var(name) = base.as_ref() {
                name == binding_name && *offset == target_offset
            } else {
                false
            }
        }
        HirExpr::Cast { expr, .. } => is_target_access(expr, binding_name, target_offset),
        _ => false,
    }
}

fn collect_accesses_in_stmts(
    stmts: &[HirStmt],
    binding_name: &str,
    target_offset: i64,
    out: &mut Vec<AccessInfo>,
) {
    for stmt in stmts {
        collect_accesses_in_stmt(stmt, binding_name, target_offset, out);
    }
}

fn collect_accesses_in_stmt(
    stmt: &HirStmt,
    binding_name: &str,
    target_offset: i64,
    out: &mut Vec<AccessInfo>,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            if let HirLValue::Deref { ptr, ty } = lhs {
                if is_target_access(ptr, binding_name, target_offset) {
                    let mut contexts = Vec::new();
                    contexts.push(AccessContext::StoreRhs {
                        rhs_ty: expr_type(rhs),
                    });
                    out.push(AccessInfo {
                        access_ty: ty.clone(),
                        contexts,
                    });
                }
                collect_accesses_in_expr(ptr, binding_name, target_offset, out, &[]);
            } else if let HirLValue::Index { base, index, .. } = lhs {
                collect_accesses_in_expr(base, binding_name, target_offset, out, &[]);
                collect_accesses_in_expr(index, binding_name, target_offset, out, &[]);
            }
            collect_accesses_in_expr(rhs, binding_name, target_offset, out, &[]);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            collect_accesses_in_expr(expr, binding_name, target_offset, out, &[]);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_accesses_in_expr(cond, binding_name, target_offset, out, &[]);
            collect_accesses_in_stmts(then_body, binding_name, target_offset, out);
            collect_accesses_in_stmts(else_body, binding_name, target_offset, out);
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            collect_accesses_in_expr(cond, binding_name, target_offset, out, &[]);
            collect_accesses_in_stmts(body, binding_name, target_offset, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_accesses_in_stmt(init, binding_name, target_offset, out);
            }
            if let Some(cond) = cond {
                collect_accesses_in_expr(cond, binding_name, target_offset, out, &[]);
            }
            if let Some(update) = update {
                collect_accesses_in_stmt(update, binding_name, target_offset, out);
            }
            collect_accesses_in_stmts(body, binding_name, target_offset, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_accesses_in_expr(expr, binding_name, target_offset, out, &[]);
            for case in cases {
                collect_accesses_in_stmts(&case.body, binding_name, target_offset, out);
            }
            collect_accesses_in_stmts(default, binding_name, target_offset, out);
        }
        HirStmt::Block(body) => {
            collect_accesses_in_stmts(body, binding_name, target_offset, out);
        }
        _ => {}
    }
}

fn collect_accesses_in_expr(
    expr: &HirExpr,
    binding_name: &str,
    target_offset: i64,
    out: &mut Vec<AccessInfo>,
    parent_contexts: &[AccessContext],
) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            if is_target_access(ptr, binding_name, target_offset) {
                let mut contexts = parent_contexts.to_vec();
                contexts.push(AccessContext::LoadParent);
                out.push(AccessInfo {
                    access_ty: ty.clone(),
                    contexts,
                });
            }
            collect_accesses_in_expr(ptr, binding_name, target_offset, out, &[]);
        }
        HirExpr::Cast { ty, expr: inner } => {
            let mut next_contexts = parent_contexts.to_vec();
            next_contexts.push(AccessContext::Cast {
                target_ty: ty.clone(),
            });
            collect_accesses_in_expr(inner, binding_name, target_offset, out, &next_contexts);
        }
        HirExpr::Unary { expr: inner, .. } => {
            collect_accesses_in_expr(inner, binding_name, target_offset, out, &[]);
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let is_float_op = matches!(ty, NirType::Float { .. });
            let mut next_contexts = parent_contexts.to_vec();
            next_contexts.push(AccessContext::Binary {
                op: *op,
                ty: ty.clone(),
                is_float_op,
            });
            collect_accesses_in_expr(lhs, binding_name, target_offset, out, &next_contexts);
            collect_accesses_in_expr(rhs, binding_name, target_offset, out, &next_contexts);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_accesses_in_expr(cond, binding_name, target_offset, out, &[]);
            collect_accesses_in_expr(then_expr, binding_name, target_offset, out, parent_contexts);
            collect_accesses_in_expr(else_expr, binding_name, target_offset, out, parent_contexts);
        }
        HirExpr::Call { target, args, .. } => {
            for (idx, arg) in args.iter().enumerate() {
                let mut next_contexts = parent_contexts.to_vec();
                next_contexts.push(AccessContext::Call {
                    target: target.clone(),
                    arg_idx: idx,
                });
                collect_accesses_in_expr(arg, binding_name, target_offset, out, &next_contexts);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            collect_accesses_in_expr(base, binding_name, target_offset, out, &[]);
        }
        HirExpr::Index { base, index, .. } => {
            collect_accesses_in_expr(base, binding_name, target_offset, out, &[]);
            collect_accesses_in_expr(index, binding_name, target_offset, out, &[]);
        }
        HirExpr::AggregateCopy { src, .. } => {
            collect_accesses_in_expr(src, binding_name, target_offset, out, &[]);
        }
        _ => {}
    }
}

fn score_candidate_type(candidate: &NirType, accesses: &[AccessInfo]) -> i32 {
    let mut score = 0;

    for access in accesses {
        if access.access_ty == *candidate {
            score += 2;
        }

        for ctx in &access.contexts {
            match ctx {
                AccessContext::Binary {
                    op, is_float_op, ..
                } => match candidate {
                    NirType::Float { .. } => {
                        if *is_float_op {
                            score += 10;
                        } else {
                            score -= 10;
                        }
                    }
                    NirType::Int { signed, .. } => {
                        if *is_float_op {
                            score -= 10;
                        } else {
                            score += 3;
                            if matches!(
                                op,
                                HirBinaryOp::SLt
                                    | HirBinaryOp::SLe
                                    | HirBinaryOp::SGt
                                    | HirBinaryOp::SGe
                            ) {
                                if *signed {
                                    score += 5;
                                } else {
                                    score -= 2;
                                }
                            }
                            if matches!(
                                op,
                                HirBinaryOp::Lt
                                    | HirBinaryOp::Le
                                    | HirBinaryOp::Gt
                                    | HirBinaryOp::Ge
                            ) {
                                if !*signed {
                                    score += 5;
                                } else {
                                    score -= 2;
                                }
                            }
                            if matches!(
                                op,
                                HirBinaryOp::And
                                    | HirBinaryOp::Or
                                    | HirBinaryOp::Xor
                                    | HirBinaryOp::Shl
                                    | HirBinaryOp::Shr
                                    | HirBinaryOp::Sar
                            ) {
                                score += 5;
                            }
                        }
                    }
                    NirType::Ptr(_) => {
                        if matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub) {
                            score += 5;
                        } else {
                            score -= 8;
                        }
                    }
                    _ => {}
                },
                AccessContext::Call { target, .. } => {
                    let target_lower = target.to_lowercase();
                    let is_float_fn = target_lower.contains("sqrt")
                        || target_lower.contains("sin")
                        || target_lower.contains("cos")
                        || target_lower.contains("tan")
                        || target_lower.contains("pow")
                        || target_lower.contains("floor")
                        || target_lower.contains("ceil")
                        || target_lower.contains("abs");

                    match candidate {
                        NirType::Float { .. } => {
                            if is_float_fn {
                                score += 12;
                            }
                        }
                        NirType::Int { .. } => {
                            if is_float_fn {
                                score -= 10;
                            }
                        }
                        _ => {}
                    }
                }
                AccessContext::Cast { target_ty } => match (candidate, target_ty) {
                    (NirType::Float { .. }, NirType::Float { .. }) => score += 8,
                    (NirType::Int { .. }, NirType::Int { .. }) => score += 8,
                    (NirType::Ptr(_), NirType::Ptr(_)) => score += 8,
                    _ => {
                        score -= 3;
                    }
                },
                AccessContext::StoreRhs { rhs_ty } => {
                    if rhs_ty == candidate {
                        score += 8;
                    }
                }
                AccessContext::LoadParent => {}
            }
        }
    }

    score
}
