//! ABI-aware return-type recovery for the intra-function type pass.
//!
//! This child module owns return-type consensus, narrow integer/ABI-width
//! recovery, and the corresponding body cast/constant cleanup.  It borrows
//! the parent pass's definition map and binding resolver rather than creating
//! a second type-flow representation.

use super::{DefEntry, infer_type_for_binding};
use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Re-derive the function's return type from its `return` statements.
///
/// The builder sets `return_type` to `expr_type(return_expr)`, but
/// `expr_type(Var(_)) = Unknown`.  This pass collects ALL non-Unknown return
/// expression types from the full body tree, then picks the consensus:
///
/// - If all non-Unknown candidates agree → use that type.
/// - If there are multiple distinct types, prefer the one that is NOT a Ptr
///   and not Bool (since integer return types are more common in practice).
/// - Fall back to the first candidate when no consensus can be found.
///
/// The function's declared return type is NEVER overwritten when it is already
/// known (non-Unknown) or when `surface_return_type_name` is set.
pub(super) fn rederive_return_type(
    return_type: &mut NirType,
    surface_return_type_name: &Option<String>,
    body: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) {
    if *return_type != NirType::Unknown || surface_return_type_name.is_some() {
        return;
    }
    // Collect ALL non-Unknown return candidates across the whole body.
    let mut candidates: Vec<NirType> = Vec::new();
    collect_return_types(body, defs, known_binding_types, &mut candidates);

    if candidates.is_empty() {
        return;
    }

    // Consensus: if all agree, use that type.
    if candidates.iter().all(|t| t == &candidates[0]) {
        *return_type = candidates[0].clone();
        return;
    }

    // Prefer integer types over Ptr/Bool for disagreement resolution.
    let int_candidates: Vec<_> = candidates
        .iter()
        .filter(|t| matches!(t, NirType::Int { .. }))
        .collect();
    if !int_candidates.is_empty() && int_candidates.iter().all(|t| *t == int_candidates[0]) {
        *return_type = int_candidates[0].clone();
        return;
    }

    // Fall back: use the first non-Unknown candidate.
    *return_type = candidates[0].clone();
}

/// Collect all non-Unknown return expression types from a statement list.
fn collect_return_types(
    stmts: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) {
    for stmt in stmts {
        collect_return_types_stmt(stmt, defs, known_binding_types, out);
    }
}

fn collect_return_types_stmt(
    stmt: &PreHirStmt,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) {
    match stmt {
        PreHirStmt::Return(Some(expr)) => {
            let ty = match expr {
                PreHirExpr::Var(name)
                | PreHirExpr::AddressOfGlobal(name)
                | PreHirExpr::AddressOfLocal(name) => {
                    let mut visited = HashSet::default();
                    infer_type_for_binding(name, defs, known_binding_types, &mut visited)
                }
                other => expr_type(other),
            };
            if ty != NirType::Unknown {
                out.push(ty);
            }
        }
        PreHirStmt::Block(stmts) => collect_return_types(stmts, defs, known_binding_types, out),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_return_types(then_body, defs, known_binding_types, out);
            collect_return_types(else_body, defs, known_binding_types, out);
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            collect_return_types(body, defs, known_binding_types, out);
        }
        PreHirStmt::For { body, .. } => collect_return_types(body, defs, known_binding_types, out),
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_return_types(&case.body, defs, known_binding_types, out);
            }
            collect_return_types(default, defs, known_binding_types, out);
        }
        _ => {}
    }
}

fn infer_return_type_from_body(
    stmts: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> NirType {
    let mut candidates = Vec::new();
    collect_return_types(stmts, defs, known_binding_types, &mut candidates);
    candidates
        .into_iter()
        .find(|t| *t != NirType::Unknown)
        .unwrap_or(NirType::Unknown)
}

fn infer_return_type_stmt(
    stmt: &PreHirStmt,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    let mut out = Vec::new();
    collect_return_types_stmt(stmt, defs, known_binding_types, &mut out);
    out.into_iter().find(|t| *t != NirType::Unknown)
}

fn infer_return_type_stmts(
    stmts: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    for stmt in stmts.iter().rev() {
        if let Some(ty) = infer_return_type_stmt(stmt, defs, known_binding_types) {
            return Some(ty);
        }
    }
    None
}

fn zero_extended_return_candidate_type(
    expr: &PreHirExpr,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        PreHirExpr::Cast { ty, expr: inner } => {
            let NirType::Int {
                bits: outer_bits,
                signed: false,
            } = ty
            else {
                return None;
            };
            // Sub-64-bit unsigned cast: the outer type is itself a narrow return candidate.
            // (On x86-64, 32-bit values written to EAX implicitly zero-extend to RAX; the
            // ZExt to u64 may have been stripped by an earlier normalization pass.)
            if *outer_bits < 64 {
                return Some(ty.clone());
            }
            // 64-bit unsigned cast (explicit ZExt): recurse into the inner expression to
            // find the narrower source type.
            let inner_ty = match inner.as_ref() {
                PreHirExpr::Var(name)
                | PreHirExpr::AddressOfGlobal(name)
                | PreHirExpr::AddressOfLocal(name) => {
                    zero_extended_return_candidate_type_for_binding(
                        name,
                        defs,
                        known_binding_types,
                    )?
                }
                other => expr_type(other),
            };
            match inner_ty {
                NirType::Int {
                    bits: inner_bits, ..
                } if inner_bits < *outer_bits => Some(inner_ty),
                _ => None,
            }
        }
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            // Prefer multi-assign aggregation when available via defs map alone first;
            // full body scan is applied in `collect_zero_extended_return_candidates_stmt`.
            let ty =
                zero_extended_return_candidate_type_for_binding(name, defs, known_binding_types)?;
            match ty {
                NirType::Int { bits, .. } if bits < 64 => Some(ty),
                _ => None,
            }
        }
        PreHirExpr::Select {
            then_expr,
            else_expr,
            ..
        } => {
            // Nested ternary return values (signum-style): combine arm candidates.
            let then_ty =
                zero_extended_return_candidate_type(then_expr, defs, known_binding_types)?;
            let else_ty =
                zero_extended_return_candidate_type(else_expr, defs, known_binding_types)?;
            Some(prefer_narrow_return_candidate(Some(then_ty), else_ty))
        }
        other => {
            // 64-bit integer constant whose u64 value fits in 32 bits:
            // treat as a zero-extended (or sign-extended) 32-bit return candidate.
            if let PreHirExpr::Const(value, NirType::Int { bits: 64, .. }) = other {
                let v = *value as u64;
                if v <= 0xFFFF_FFFF {
                    let signed = v >= 0x8000_0000;
                    return Some(NirType::Int { bits: 32, signed });
                }
            }
            // Also accept unsigned-32 Const typed as u32 (printer still shows large decimals).
            if let PreHirExpr::Const(
                value,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ) = other
            {
                let v = *value as u64;
                if v <= 0xFFFF_FFFF {
                    let signed = v >= 0x8000_0000;
                    return Some(NirType::Int { bits: 32, signed });
                }
            }
            match expr_type(other) {
                ty @ NirType::Int { bits, .. } if bits < 64 => Some(ty),
                _ => None,
            }
        }
    }
}

fn zero_extended_return_candidate_type_for_binding(
    name: &str,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    let mut current = name.to_owned();
    let mut visited = HashSet::default();
    let mut best = None;

    loop {
        if !visited.insert(current.clone()) {
            return best;
        }
        if let Some(ty @ NirType::Int { bits, .. }) = known_binding_types.get(&current) {
            if *bits < 64 {
                best = Some(prefer_narrow_return_candidate(best, ty.clone()));
            }
        }
        match defs.get(&current) {
            Some(DefEntry::Known(ty @ NirType::Int { bits, .. })) => {
                if *bits < 64 {
                    best = Some(prefer_narrow_return_candidate(best, ty.clone()));
                }
                // First-def may be a wide u64 (Select/const temp). Keep scanning alias only;
                // multi-assign aggregation happens in `aggregate_return_temp_candidates`.
                return best.or_else(|| {
                    // Wide Known(u64) alone is not a narrow candidate.
                    None
                });
            }
            Some(DefEntry::Known(_)) | None => return best,
            Some(DefEntry::Alias(src)) => {
                current = src.clone();
            }
            Some(DefEntry::TypedAlias { source, ty }) => {
                if let NirType::Int { bits, .. } = ty
                    && *bits < 64
                {
                    best = Some(prefer_narrow_return_candidate(best, ty.clone()));
                }
                current = source.clone();
            }
            Some(DefEntry::Derived { ty, .. }) => {
                if let NirType::Int { bits, .. } = ty
                    && *bits < 64
                {
                    best = Some(prefer_narrow_return_candidate(best, ty.clone()));
                }
                return best;
            }
        }
    }
}

/// Aggregate i32-compatible candidates across *all* assignments to a returned temp.
/// First-def-only maps miss later `x = INT_MIN` / `x = -1` arms (signum/saturating_add).
fn aggregate_return_temp_candidates(
    name: &str,
    stmts: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    aggregate_return_temp_candidates_guarded(
        name,
        stmts,
        defs,
        known_binding_types,
        &mut HashSet::default(),
    )
}

fn aggregate_return_temp_candidates_guarded(
    name: &str,
    stmts: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    visiting: &mut HashSet<String>,
) -> Option<NirType> {
    if !visiting.insert(name.to_owned()) {
        return None;
    }
    let mut rhss = Vec::new();
    collect_var_assign_rhs(stmts, name, &mut rhss);
    if rhss.is_empty() {
        return zero_extended_return_candidate_type_for_binding(name, defs, known_binding_types);
    }
    // Multi-assign aggregation is only for return-join patterns that carry
    // high-bit (signed) 32-bit constants or Select arms. Plain loop accumulators
    // (x=0; x=x+c) must keep their wide type.
    if !rhss.iter().any(|rhs| rhs_has_i32_sign_bit_evidence(rhs)) {
        return zero_extended_return_candidate_type_for_binding(name, defs, known_binding_types);
    }
    let mut best = None;
    for rhs in rhss {
        let ty = match rhs {
            PreHirExpr::Var(src)
            | PreHirExpr::AddressOfGlobal(src)
            | PreHirExpr::AddressOfLocal(src) => {
                aggregate_return_temp_candidates_guarded(
                    src,
                    stmts,
                    defs,
                    known_binding_types,
                    visiting,
                )
                .or_else(|| zero_extended_return_candidate_type(rhs, defs, known_binding_types))
                // Fall back: plain scalar temps assigned only small constants (e.g. local_4=0)
                // still contribute an unsigned i32 arm so signed join temps can narrow.
                .or_else(|| i32_compatible_const_leaf_type(rhs))
                .or_else(|| {
                    // Last resort for unsigned narrowable leaves without high-bit evidence.
                    Some(NirType::Int {
                        bits: 32,
                        signed: false,
                    })
                })?
            }
            other => zero_extended_return_candidate_type(other, defs, known_binding_types)
                .or_else(|| i32_compatible_const_leaf_type(other))?,
        };
        best = Some(prefer_narrow_return_candidate(best, ty));
    }
    best
}

fn i32_compatible_const_leaf_type(expr: &PreHirExpr) -> Option<NirType> {
    match expr {
        PreHirExpr::Const(value, NirType::Int { bits: 32 | 64, .. }) => {
            let v = *value as u64;
            if v <= 0xFFFF_FFFF {
                Some(NirType::Int {
                    bits: 32,
                    signed: v >= 0x8000_0000,
                })
            } else {
                None
            }
        }
        PreHirExpr::Cast { expr, .. } => i32_compatible_const_leaf_type(expr),
        _ => None,
    }
}

fn rhs_has_i32_sign_bit_evidence(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Const(value, NirType::Int { bits: 32 | 64, .. }) => {
            let v = *value as u64;
            v <= 0xFFFF_FFFF && v >= 0x8000_0000
        }
        // `neg` of setnz (signum ≤0 path) yields -1 / 0 in full EAX — signed i32.
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Neg,
            ..
        } => true,
        PreHirExpr::Select {
            then_expr,
            else_expr,
            ..
        } => rhs_has_i32_sign_bit_evidence(then_expr) || rhs_has_i32_sign_bit_evidence(else_expr),
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            rhs_has_i32_sign_bit_evidence(expr)
        }
        _ => false,
    }
}

fn collect_var_assign_rhs<'a>(stmts: &'a [PreHirStmt], name: &str, out: &mut Vec<&'a PreHirExpr>) {
    for stmt in stmts {
        collect_var_assign_rhs_stmt(stmt, name, out);
    }
}

fn collect_var_assign_rhs_stmt<'a>(
    stmt: &'a PreHirStmt,
    name: &str,
    out: &mut Vec<&'a PreHirExpr>,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(n),
            rhs,
        } if n == name => out.push(rhs),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => collect_var_assign_rhs(body, name, out),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_var_assign_rhs(then_body, name, out);
            collect_var_assign_rhs(else_body, name, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_var_assign_rhs(&case.body, name, out);
            }
            collect_var_assign_rhs(default, name, out);
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                collect_var_assign_rhs_stmt(i, name, out);
            }
            if let Some(u) = update {
                collect_var_assign_rhs_stmt(u, name, out);
            }
            collect_var_assign_rhs(body, name, out);
        }
        _ => {}
    }
}

fn prefer_narrow_return_candidate(current: Option<NirType>, candidate: NirType) -> NirType {
    /// ABI integer returns live in a full machine register (32-bit on x86-32,
    /// low 32 of RAX on x64). Prefer 32-bit over both 64-bit zext wrappers and
    /// 8/16-bit setcc lanes.
    fn abi_int_rank(bits: u32) -> u8 {
        match bits {
            32 => 0,
            16 | 8 => 1,
            64 => 2,
            _ => 3,
        }
    }
    match (current, candidate) {
        (
            Some(NirType::Int {
                bits: current_bits,
                signed: current_signed,
            }),
            NirType::Int {
                bits: candidate_bits,
                signed: candidate_signed,
            },
        ) if current_bits == candidate_bits => NirType::Int {
            bits: current_bits,
            signed: current_signed || candidate_signed,
        },
        (
            Some(NirType::Int {
                bits: current_bits,
                signed: current_signed,
            }),
            NirType::Int {
                bits: candidate_bits,
                signed: candidate_signed,
            },
        ) => {
            let signed = current_signed || candidate_signed;
            if abi_int_rank(candidate_bits) < abi_int_rank(current_bits) {
                NirType::Int {
                    bits: candidate_bits.max(32),
                    signed: signed || candidate_bits < 32,
                }
            } else {
                NirType::Int {
                    bits: current_bits.max(if current_bits < 32 { 32 } else { current_bits }),
                    signed: signed || current_bits < 32,
                }
            }
        }
        (Some(current), _) => current,
        (None, NirType::Int { bits, signed }) if bits < 32 => NirType::Int {
            bits: 32,
            signed: signed || bits <= 8,
        },
        (None, candidate) => candidate,
    }
}

fn collect_zero_extended_return_candidates(
    stmts: &[PreHirStmt],
    root_body: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    let mut value_return_count = 0;
    for stmt in stmts {
        value_return_count += collect_zero_extended_return_candidates_stmt(
            stmt,
            root_body,
            defs,
            known_binding_types,
            out,
        );
    }
    value_return_count
}

fn collect_zero_extended_return_candidates_stmt(
    stmt: &PreHirStmt,
    root_body: &[PreHirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        PreHirStmt::Return(Some(expr)) => {
            let ty = match expr {
                PreHirExpr::Var(name)
                | PreHirExpr::AddressOfGlobal(name)
                | PreHirExpr::AddressOfLocal(name) => {
                    aggregate_return_temp_candidates(name, root_body, defs, known_binding_types)
                        .or_else(|| {
                            zero_extended_return_candidate_type(expr, defs, known_binding_types)
                        })
                }
                _ => zero_extended_return_candidate_type(expr, defs, known_binding_types),
            };
            if let Some(ty) = ty {
                out.push(ty);
            }
            1
        }
        PreHirStmt::Return(None) => 0,
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => collect_zero_extended_return_candidates(
            stmts,
            root_body,
            defs,
            known_binding_types,
            out,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let then_count = collect_zero_extended_return_candidates(
                then_body,
                root_body,
                defs,
                known_binding_types,
                out,
            );
            let else_count = collect_zero_extended_return_candidates(
                else_body,
                root_body,
                defs,
                known_binding_types,
                out,
            );
            then_count + else_count
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut value_return_count = 0;
            for case in cases {
                value_return_count += collect_zero_extended_return_candidates(
                    &case.body,
                    root_body,
                    defs,
                    known_binding_types,
                    out,
                );
            }
            value_return_count
                + collect_zero_extended_return_candidates(
                    default,
                    root_body,
                    defs,
                    known_binding_types,
                    out,
                )
        }
        _ => 0,
    }
}

fn strip_zero_extended_return_casts(stmts: &mut [PreHirStmt], narrowed_ty: &NirType) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= strip_zero_extended_return_casts_stmt(stmt, narrowed_ty);
    }
    changed
}

fn strip_zero_extended_return_casts_stmt(stmt: &mut PreHirStmt, narrowed_ty: &NirType) -> bool {
    match stmt {
        // Rewrite 64-bit integer constants to their narrowed 32-bit equivalent.
        PreHirStmt::Return(Some(PreHirExpr::Const(value, const_ty))) => {
            let NirType::Int { bits: 64, .. } = const_ty else {
                return false;
            };
            let NirType::Int {
                bits: 32,
                signed: narrow_signed,
            } = narrowed_ty
            else {
                return false;
            };
            let v = *value as u64;
            if v <= 0xFFFF_FFFF {
                let u32_val = v as u32;
                *value = if *narrow_signed {
                    (u32_val as i32) as i64
                } else {
                    u32_val as i64
                };
                *const_ty = narrowed_ty.clone();
                true
            } else {
                false
            }
        }
        PreHirStmt::Return(Some(PreHirExpr::Cast { ty, expr })) => {
            let should_strip = matches!(
                (ty, narrowed_ty),
                (
                    NirType::Int {
                        bits: outer_bits,
                        signed: false,
                    },
                    NirType::Int {
                        bits: inner_bits,
                        ..
                    },
                ) if inner_bits < outer_bits
            );
            if should_strip {
                let inner = (**expr).clone();
                *stmt = PreHirStmt::Return(Some(inner));
                true
            } else {
                false
            }
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => strip_zero_extended_return_casts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            narrowed_ty,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            strip_zero_extended_return_casts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                narrowed_ty,
            ) | strip_zero_extended_return_casts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                narrowed_ty,
            )
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |= strip_zero_extended_return_casts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    narrowed_ty,
                );
            }
            changed
                | strip_zero_extended_return_casts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    narrowed_ty,
                )
        }
        _ => false,
    }
}

pub(super) fn narrow_zero_extended_return_width(
    func: &mut PreHirFunction,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    if func.surface_return_type_name.is_some() {
        return false;
    }
    let NirType::Int {
        bits: return_bits,
        signed: false,
    } = &func.return_type
    else {
        return false;
    };
    let mut candidates = Vec::new();
    let value_return_count = collect_zero_extended_return_candidates(
        &func.body,
        &func.body,
        defs,
        known_binding_types,
        &mut candidates,
    );
    if value_return_count == 0 || candidates.len() != value_return_count {
        return false;
    }
    let NirType::Int {
        bits: candidate_bits,
        ..
    } = candidates[0].clone()
    else {
        return false;
    };
    let candidate_signed = candidates
        .iter()
        .any(|ty| matches!(ty, NirType::Int { signed: true, .. }));
    // setcc/movzx alone can look like an 8-bit return, but x86 ABI integer
    // returns stay in EAX. signum: `setnz al; movzx eax,al; neg eax` must not
    // become `uchar` or `-1` recompiles as `255`.
    // Only allow narrowing 64→32 (implicit EAX zext); never shrink below 32.
    let effective_bits = candidate_bits.max(32);
    if effective_bits > *return_bits
        || candidates.iter().any(|ty| {
            !matches!(
                ty,
                NirType::Int { bits, .. } if *bits == candidate_bits
            )
        })
        || (effective_bits == *return_bits && !candidate_signed && candidate_bits >= 32)
    {
        return false;
    }
    // Sub-32 evidence still contributes signedness, but the ABI width is 32.
    let candidate = NirType::Int {
        bits: effective_bits,
        signed: candidate_signed || candidate_bits < 32,
    };
    func.return_type = candidate.clone();
    strip_zero_extended_return_casts(&mut func.body, &candidate);
    // Rewrite join-temp constants only for signed narrow (signum/INT_MIN paths).
    // Unsigned zext narrow must not rewrite body temps (breaks loop-carried casts).
    if candidate_signed {
        rewrite_i32_compatible_constants_in_body(&mut func.body, &candidate);
        narrow_returned_temp_bindings(func, &candidate);
    }
    true
}

fn narrow_returned_temp_bindings(func: &mut PreHirFunction, narrowed_ty: &NirType) {
    let mut returned = HashSet::default();
    collect_returned_var_names(&func.body, &mut returned);
    for binding in &mut func.locals {
        if returned.contains(&binding.name) {
            binding.ty = narrowed_ty.clone();
        }
    }
}

/// Lift sub-32 integer return types to ABI-width 32-bit integers.
///
/// setcc/movzx evidence can leave `return_type = uchar`, which makes `return -1`
/// recompile as `255` (signum ≤0 path: setnz; movzx; neg).
pub(super) fn promote_sub32_abi_return_width(
    func: &mut PreHirFunction,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> bool {
    if func.surface_return_type_name.is_some() {
        return false;
    }
    let NirType::Int {
        bits,
        signed: was_signed,
    } = &func.return_type
    else {
        return false;
    };
    if *bits >= 32 {
        return false;
    }
    let mut rhss = Vec::new();
    collect_all_return_exprs(&func.body, &mut rhss);
    let signed_evidence = rhss.iter().any(|e| rhs_has_i32_sign_bit_evidence(e))
        || rhss.iter().any(|e| match e {
            PreHirExpr::Var(name)
            | PreHirExpr::AddressOfGlobal(name)
            | PreHirExpr::AddressOfLocal(name) => {
                let mut assign_rhss = Vec::new();
                collect_var_assign_rhs(&func.body, name, &mut assign_rhss);
                assign_rhss
                    .iter()
                    .any(|rhs| rhs_has_i32_sign_bit_evidence(rhs))
            }
            _ => false,
        });
    let _ = (defs, known_binding_types);
    // setcc-derived uchar is almost always a zero/sign-extended machine-word
    // return; promote to signed i32 when Neg/-1 evidence exists, else unsigned i32.
    let promoted = NirType::Int {
        bits: 32,
        signed: *was_signed || signed_evidence || *bits <= 8,
    };
    if func.return_type == promoted {
        return false;
    }
    func.return_type = promoted.clone();
    // Keep returned temps at the promoted width so `return x` is not truncated.
    narrow_returned_temp_bindings(func, &promoted);
    true
}

/// When the ABI return is already ≥32-bit but a returned join temp stayed as
/// setcc/uchar (e.g. `int f(){ uchar x = !zf; x = -x; return x; }`), widen those
/// temps so recompilation does not truncate `-1` to `255`.
pub(super) fn promote_narrow_returned_temps_for_abi_return(func: &mut PreHirFunction) -> bool {
    if func.surface_return_type_name.is_some() {
        return false;
    }
    let NirType::Int {
        bits: ret_bits,
        signed: _,
    } = func.return_type.clone()
    else {
        return false;
    };
    if ret_bits < 32 {
        return false;
    }
    let mut returned = HashSet::default();
    collect_returned_var_names(&func.body, &mut returned);
    if returned.is_empty() {
        return false;
    }
    let mut rhss = Vec::new();
    collect_all_return_exprs(&func.body, &mut rhss);
    let mut sign_ev = rhss.iter().any(|e| rhs_has_i32_sign_bit_evidence(e));
    if !sign_ev {
        for name in &returned {
            let mut assign_rhss = Vec::new();
            collect_var_assign_rhs(&func.body, name, &mut assign_rhss);
            if assign_rhss
                .iter()
                .any(|rhs| rhs_has_i32_sign_bit_evidence(rhs))
            {
                sign_ev = true;
                break;
            }
        }
    }
    if !sign_ev {
        return false;
    }
    // Sign-bit evidence ⇒ signed machine-word temp (matches ABI return width).
    let promoted = NirType::Int {
        bits: ret_bits,
        signed: true,
    };
    let mut changed = false;
    for binding in &mut func.locals {
        if !returned.contains(&binding.name) {
            continue;
        }
        let NirType::Int { bits, .. } = &binding.ty else {
            continue;
        };
        if *bits >= ret_bits {
            continue;
        }
        binding.ty = promoted.clone();
        changed = true;
    }
    changed
}

fn collect_all_return_exprs<'a>(stmts: &'a [PreHirStmt], out: &mut Vec<&'a PreHirExpr>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Return(Some(expr)) => out.push(expr),
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_all_return_exprs(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_all_return_exprs(then_body, out);
                collect_all_return_exprs(else_body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_all_return_exprs(&case.body, out);
                }
                collect_all_return_exprs(default, out);
            }
            _ => {}
        }
    }
}

fn collect_returned_var_names(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Return(Some(
                PreHirExpr::Var(n) | PreHirExpr::AddressOfGlobal(n) | PreHirExpr::AddressOfLocal(n),
            )) => {
                out.insert(n.clone());
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_returned_var_names(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_returned_var_names(then_body, out);
                collect_returned_var_names(else_body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_returned_var_names(&case.body, out);
                }
                collect_returned_var_names(default, out);
            }
            _ => {}
        }
    }
}

/// Rewrite 32-bit-compatible wide constants in expressions (Select/assign RHS)
/// after the function return type was narrowed to signed/unsigned i32.
fn rewrite_i32_compatible_constants_in_body(
    stmts: &mut [PreHirStmt],
    narrowed_ty: &NirType,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= rewrite_i32_compatible_constants_in_stmt(stmt, narrowed_ty);
    }
    changed
}

fn rewrite_i32_compatible_constants_in_stmt(stmt: &mut PreHirStmt, narrowed_ty: &NirType) -> bool {
    match stmt {
        PreHirStmt::Assign { rhs, .. } => {
            rewrite_i32_compatible_constants_in_expr(rhs, narrowed_ty)
        }
        PreHirStmt::Return(Some(expr)) => {
            rewrite_i32_compatible_constants_in_expr(expr, narrowed_ty)
        }
        PreHirStmt::Expr(expr) => rewrite_i32_compatible_constants_in_expr(expr, narrowed_ty),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => rewrite_i32_compatible_constants_in_body(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            narrowed_ty,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            cond,
            ..
        } => {
            rewrite_i32_compatible_constants_in_expr(cond, narrowed_ty)
                | rewrite_i32_compatible_constants_in_body(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    narrowed_ty,
                )
                | rewrite_i32_compatible_constants_in_body(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    narrowed_ty,
                )
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
            ..
        } => {
            let mut changed = rewrite_i32_compatible_constants_in_expr(expr, narrowed_ty);
            for case in cases {
                changed |= rewrite_i32_compatible_constants_in_body(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    narrowed_ty,
                );
            }
            changed
                | rewrite_i32_compatible_constants_in_body(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    narrowed_ty,
                )
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = false;
            if let Some(i) = init {
                changed |= rewrite_i32_compatible_constants_in_stmt(i, narrowed_ty);
            }
            if let Some(u) = update {
                changed |= rewrite_i32_compatible_constants_in_stmt(u, narrowed_ty);
            }
            changed
                | rewrite_i32_compatible_constants_in_body(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    narrowed_ty,
                )
        }
        _ => false,
    }
}

fn rewrite_i32_compatible_constants_in_expr(expr: &mut PreHirExpr, narrowed_ty: &NirType) -> bool {
    let NirType::Int {
        bits: 32,
        signed: narrow_signed,
    } = narrowed_ty
    else {
        return false;
    };
    match expr {
        PreHirExpr::Const(value, ty) => {
            let width_ok = matches!(
                ty,
                NirType::Int {
                    bits: 64 | 32,
                    signed: false
                }
            );
            if !width_ok {
                return false;
            }
            let v = *value as u64;
            if v > 0xFFFF_FFFF {
                return false;
            }
            let u32_val = v as u32;
            let new_v = if *narrow_signed {
                (u32_val as i32) as i64
            } else {
                u32_val as i64
            };
            if *value == new_v && ty == narrowed_ty {
                return false;
            }
            *value = new_v;
            *ty = narrowed_ty.clone();
            true
        }
        PreHirExpr::Select {
            then_expr,
            else_expr,
            ty,
            ..
        } => {
            let mut changed = rewrite_i32_compatible_constants_in_expr(then_expr, narrowed_ty)
                | rewrite_i32_compatible_constants_in_expr(else_expr, narrowed_ty);
            if matches!(
                ty,
                NirType::Int {
                    bits: 64,
                    signed: false
                }
            ) {
                *ty = narrowed_ty.clone();
                changed = true;
            }
            changed
        }
        PreHirExpr::Cast { expr: inner, ty } => {
            let mut changed = rewrite_i32_compatible_constants_in_expr(inner, narrowed_ty);
            if matches!(
                ty,
                NirType::Int {
                    bits: 64,
                    signed: false
                }
            ) {
                // Prefer dropping the outer zext by rewriting type; caller may strip later.
                *ty = narrowed_ty.clone();
                changed = true;
            }
            changed
        }
        PreHirExpr::Unary { expr: inner, .. } => {
            rewrite_i32_compatible_constants_in_expr(inner, narrowed_ty)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite_i32_compatible_constants_in_expr(lhs, narrowed_ty)
                | rewrite_i32_compatible_constants_in_expr(rhs, narrowed_ty)
        }
        _ => false,
    }
}

pub(super) fn strip_zero_extended_casts_to_declared_return_width(
    func: &mut PreHirFunction,
) -> bool {
    if func.surface_return_type_name.is_some() {
        return false;
    }
    let NirType::Int {
        bits: return_bits, ..
    } = &func.return_type
    else {
        return false;
    };
    if *return_bits >= 64 {
        return false;
    }
    let return_type = func.return_type.clone();
    strip_zero_extended_return_casts(&mut func.body, &return_type)
}
