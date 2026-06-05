/// Intra-function type inference pass.
///
/// Ghidra's `ActionInferTypes::propagateOneType` follows data-flow edges in the
/// full SSA graph. Here we approximate the same idea using Fission's already-
/// structured HIR: since the HIR is in near-SSA form (most variables are
/// single-assignment after normalization), we can reconstruct types by walking
/// the def map without a full data-flow framework.
///
/// Algorithm:
/// 1. `scan_def_types(body)` — build a `HashMap<name, HirExpr>` from the first
///    assignment to each variable anywhere in the body tree.
/// 2. `infer_type_for_binding(name, defs, visited)` — recursively derive the
///    type of a named binding.  If the definition is `Var(other)` we follow the
///    chain (cycle-protected with a `HashSet`); otherwise we call `expr_type`.
/// 3. `apply_type_inference_pass(func)` — for every `NirBinding` whose `ty` is
///    `Unknown` _and_ whose `surface_type_name` is unset, replace `ty` with the
///    inferred result.  Also re-derives `HirFunction.return_type` for the common
///    `return <var>;` pattern that previously always produced `undefined`.
///
/// This pass is binary-independent: it only propagates types
/// that are already embedded in typed sub-expressions (Const, Cast, Binary, …).
use super::super::*;
use std::collections::{HashMap, HashSet};

/// Collect the first assignment expression type for each named variable in the
/// body.  We store `(NirType, Option<String>)` where the Option carries the
/// target variable name when the RHS is a `Var` — so we can chain-resolve later.
///
/// Storing owned types (not references) avoids lifetime conflicts when we
/// later mutate `func` to apply the inferred types.
fn scan_def_types(stmts: &[HirStmt], defs: &mut HashMap<String, DefEntry>) {
    for stmt in stmts {
        scan_def_types_stmt(stmt, defs);
    }
}

/// Either a concrete type inferred from the expression, or the name of another
/// variable whose type we still need to chase (for `x = y` patterns).
enum DefEntry {
    Known(NirType),
    Alias(String),
}

fn scan_def_types_stmt(stmt: &HirStmt, defs: &mut HashMap<String, DefEntry>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            if defs.contains_key(name.as_str()) {
                // Only record the first definition (near-SSA assumption).
                return;
            }
            let entry = match rhs {
                HirExpr::Var(src) => DefEntry::Alias(src.clone()),
                other => {
                    let ty = expr_type(other);
                    DefEntry::Known(ty)
                }
            };
            defs.insert(name.clone(), entry);
        }
        HirStmt::Block(stmts) => scan_def_types(stmts, defs),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            scan_def_types(then_body, defs);
            scan_def_types(else_body, defs);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => scan_def_types(body, defs),
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                scan_def_types_stmt(i, defs);
            }
            if let Some(u) = update {
                scan_def_types_stmt(u, defs);
            }
            scan_def_types(body, defs);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                scan_def_types(&case.body, defs);
            }
            scan_def_types(default, defs);
        }
        _ => {}
    }
}

/// Infer the type of a named binding by following its definition chain.
///
/// Returns `NirType::Unknown` when:
/// - the name has no definition in `defs`
/// - the definition's type is `Unknown` (e.g. another unresolved Var)
/// - a cycle is detected in the Var-chain
fn infer_type_for_binding(
    name: &str,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    visited: &mut HashSet<String>,
) -> NirType {
    if !visited.insert(name.to_owned()) {
        return NirType::Unknown;
    }
    match defs.get(name) {
        None => known_binding_types
            .get(name)
            .cloned()
            .unwrap_or(NirType::Unknown),
        Some(DefEntry::Known(ty)) if *ty != NirType::Unknown => ty.clone(),
        Some(DefEntry::Known(_)) => known_binding_types
            .get(name)
            .cloned()
            .unwrap_or(NirType::Unknown),
        Some(DefEntry::Alias(src)) => {
            let src = src.clone();
            infer_type_for_binding(&src, defs, known_binding_types, visited)
        }
    }
}

fn collect_known_binding_types(func: &HirFunction) -> HashMap<String, NirType> {
    let mut known = HashMap::new();
    for b in &func.params {
        if b.ty != NirType::Unknown {
            known.insert(b.name.clone(), b.ty.clone());
        }
    }
    for b in &func.locals {
        if b.ty != NirType::Unknown {
            known.insert(b.name.clone(), b.ty.clone());
        }
    }
    known
}

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
fn rederive_return_type(
    return_type: &mut NirType,
    surface_return_type_name: &Option<String>,
    body: &[HirStmt],
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
    stmts: &[HirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) {
    for stmt in stmts {
        collect_return_types_stmt(stmt, defs, known_binding_types, out);
    }
}

fn collect_return_types_stmt(
    stmt: &HirStmt,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) {
    match stmt {
        HirStmt::Return(Some(expr)) => {
            let ty = match expr {
                HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
                    let mut visited = HashSet::new();
                    infer_type_for_binding(name, defs, known_binding_types, &mut visited)
                }
                other => expr_type(other),
            };
            if ty != NirType::Unknown {
                out.push(ty);
            }
        }
        HirStmt::Block(stmts) => collect_return_types(stmts, defs, known_binding_types, out),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_return_types(then_body, defs, known_binding_types, out);
            collect_return_types(else_body, defs, known_binding_types, out);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_return_types(body, defs, known_binding_types, out);
        }
        HirStmt::For { body, .. } => collect_return_types(body, defs, known_binding_types, out),
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_return_types(&case.body, defs, known_binding_types, out);
            }
            collect_return_types(default, defs, known_binding_types, out);
        }
        _ => {}
    }
}

fn infer_return_type_from_body(
    stmts: &[HirStmt],
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
    stmt: &HirStmt,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    let mut out = Vec::new();
    collect_return_types_stmt(stmt, defs, known_binding_types, &mut out);
    out.into_iter().find(|t| *t != NirType::Unknown)
}

fn infer_return_type_stmts(
    stmts: &[HirStmt],
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
    expr: &HirExpr,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        HirExpr::Cast { ty, expr: inner } => {
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
                HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
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
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            let ty =
                zero_extended_return_candidate_type_for_binding(name, defs, known_binding_types)?;
            match ty {
                NirType::Int { bits, .. } if bits < 64 => Some(ty),
                _ => None,
            }
        }
        other => {
            // 64-bit integer constant whose u64 value fits in 32 bits:
            // treat as a zero-extended (or sign-extended) 32-bit return candidate.
            if let HirExpr::Const(value, NirType::Int { bits: 64, .. }) = other {
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
    let mut visited = HashSet::new();
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
                return best;
            }
            Some(DefEntry::Known(_)) | None => return best,
            Some(DefEntry::Alias(src)) => {
                current = src.clone();
            }
        }
    }
}

fn prefer_narrow_return_candidate(current: Option<NirType>, candidate: NirType) -> NirType {
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
            Some(current @ NirType::Int {
                bits: current_bits, ..
            }),
            candidate @ NirType::Int {
                bits: candidate_bits, ..
            },
        ) => {
            if candidate_bits < current_bits {
                candidate
            } else {
                current
            }
        }
        (Some(current), _) => current,
        (None, candidate) => candidate,
    }
}

fn collect_zero_extended_return_candidates(
    stmts: &[HirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    let mut value_return_count = 0;
    for stmt in stmts {
        value_return_count +=
            collect_zero_extended_return_candidates_stmt(stmt, defs, known_binding_types, out);
    }
    value_return_count
}

fn collect_zero_extended_return_candidates_stmt(
    stmt: &HirStmt,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        HirStmt::Return(Some(expr)) => {
            if let Some(ty) = zero_extended_return_candidate_type(expr, defs, known_binding_types) {
                out.push(ty);
            }
            1
        }
        HirStmt::Return(None) => 0,
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            collect_zero_extended_return_candidates(stmts, defs, known_binding_types, out)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let then_count =
                collect_zero_extended_return_candidates(then_body, defs, known_binding_types, out);
            let else_count =
                collect_zero_extended_return_candidates(else_body, defs, known_binding_types, out);
            then_count + else_count
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut value_return_count = 0;
            for case in cases {
                value_return_count += collect_zero_extended_return_candidates(
                    &case.body,
                    defs,
                    known_binding_types,
                    out,
                );
            }
            value_return_count
                + collect_zero_extended_return_candidates(default, defs, known_binding_types, out)
        }
        _ => 0,
    }
}

fn strip_zero_extended_return_casts(stmts: &mut [HirStmt], narrowed_ty: &NirType) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= strip_zero_extended_return_casts_stmt(stmt, narrowed_ty);
    }
    changed
}

fn strip_zero_extended_return_casts_stmt(stmt: &mut HirStmt, narrowed_ty: &NirType) -> bool {
    match stmt {
        // Rewrite 64-bit integer constants to their narrowed 32-bit equivalent.
        HirStmt::Return(Some(HirExpr::Const(value, const_ty))) => {
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
        HirStmt::Return(Some(HirExpr::Cast { ty, expr })) => {
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
                *stmt = HirStmt::Return(Some(inner));
                true
            } else {
                false
            }
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => strip_zero_extended_return_casts(stmts, narrowed_ty),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            strip_zero_extended_return_casts(then_body, narrowed_ty)
                | strip_zero_extended_return_casts(else_body, narrowed_ty)
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |= strip_zero_extended_return_casts(&mut case.body, narrowed_ty);
            }
            changed | strip_zero_extended_return_casts(default, narrowed_ty)
        }
        _ => false,
    }
}

fn narrow_zero_extended_return_width(
    func: &mut HirFunction,
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
    let candidate_signed = candidates.iter().any(|ty| {
        matches!(
            ty,
            NirType::Int {
                signed: true,
                ..
            }
        )
    });
    if candidate_bits > *return_bits
        || candidates.iter().any(|ty| {
            !matches!(
                ty,
                NirType::Int { bits, .. } if *bits == candidate_bits
            )
        })
        || (candidate_bits == *return_bits && !candidate_signed)
    {
        return false;
    }

    let candidate = NirType::Int {
        bits: candidate_bits,
        signed: candidate_signed,
    };
    func.return_type = candidate.clone();
    strip_zero_extended_return_casts(&mut func.body, &candidate);
    true
}

fn strip_zero_extended_casts_to_declared_return_width(func: &mut HirFunction) -> bool {
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

/// Apply the type inference pass to a function.
///
/// - Updates `NirBinding.ty` for all `locals` and `params` that have
///   `ty == Unknown` and no `surface_type_name` override.
/// - Re-derives `HirFunction.return_type` when it is `Unknown`.
///
/// Returns `true` when at least one binding/return type was strengthened.
pub(crate) fn apply_type_inference_pass(func: &mut HirFunction) -> bool {
    // Build the owned def map (no lifetime ties to func).
    let mut defs: HashMap<String, DefEntry> = HashMap::new();
    scan_def_types(&func.body, &mut defs);
    let mut known_binding_types = collect_known_binding_types(func);
    let mut changed = false;

    // Infer types for locals whose ty is Unknown.
    for binding in func.locals.iter_mut() {
        if binding.ty != NirType::Unknown || binding.surface_type_name.is_some() {
            continue;
        }
        let mut visited = HashSet::new();
        let inferred =
            infer_type_for_binding(&binding.name, &defs, &known_binding_types, &mut visited);
        if inferred != NirType::Unknown && binding.ty != inferred {
            binding.ty = inferred;
            known_binding_types.insert(binding.name.clone(), binding.ty.clone());
            changed = true;
        }
    }

    // Also update params (some params start as Unknown when they aren't
    // explicitly typed by hints).
    for binding in func.params.iter_mut() {
        if binding.ty != NirType::Unknown || binding.surface_type_name.is_some() {
            continue;
        }
        let mut visited = HashSet::new();
        let inferred =
            infer_type_for_binding(&binding.name, &defs, &known_binding_types, &mut visited);
        if inferred != NirType::Unknown && binding.ty != inferred {
            binding.ty = inferred;
            known_binding_types.insert(binding.name.clone(), binding.ty.clone());
            changed = true;
        }
    }

    // Re-derive the return type (no lifetime conflict — defs owns its data).
    let prev_return_type = func.return_type.clone();
    rederive_return_type(
        &mut func.return_type,
        &func.surface_return_type_name,
        &func.body,
        &defs,
        &known_binding_types,
    );
    changed |= func.return_type != prev_return_type;

    changed |= narrow_zero_extended_return_width(func, &defs, &known_binding_types);
    changed |= strip_zero_extended_casts_to_declared_return_width(func);

    changed
}

#[cfg(test)]
mod tests {
    use super::super::super::*;

    fn make_assign(name: &str, rhs: HirExpr) -> HirStmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name.to_owned()),
            rhs,
        }
    }

    fn make_binding(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_param(name: &str, ty: NirType) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<NirBinding>, body: Vec<HirStmt>, return_type: NirType) -> HirFunction {
        HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals,
            return_type,
            surface_return_type_name: None,
            body,
            ..Default::default()
        }
    }

    /// `x = Const(42, uint)` → x.ty inferred as `uint`
    #[test]
    fn infers_type_from_const_assign() {
        let body = vec![make_assign(
            "x",
            HirExpr::Const(
                42,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        )];
        let mut func = make_func(vec![make_binding("x")], body, NirType::Unknown);
        super::apply_type_inference_pass(&mut func);
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 32,
                signed: false
            }
        );
    }

    #[test]
    fn reports_change_and_reaches_fixpoint() {
        let body = vec![make_assign(
            "x",
            HirExpr::Const(
                42,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        )];
        let mut func = make_func(vec![make_binding("x")], body, NirType::Unknown);
        assert!(super::apply_type_inference_pass(&mut func));
        assert!(!super::apply_type_inference_pass(&mut func));
    }

    /// Chain: `y = x`, `x = Const(1, bool)` → y.ty inferred as `bool`
    #[test]
    fn infers_type_through_var_chain() {
        let body = vec![
            make_assign("x", HirExpr::Const(1, NirType::Bool)),
            make_assign("y", HirExpr::Var("x".to_owned())),
        ];
        let mut func = make_func(
            vec![make_binding("x"), make_binding("y")],
            body,
            NirType::Unknown,
        );
        super::apply_type_inference_pass(&mut func);
        assert_eq!(func.locals[1].ty, NirType::Bool);
    }

    /// Cycle: `a = b`, `b = a` → should not panic, both remain Unknown
    #[test]
    fn cycle_protection_does_not_panic() {
        let body = vec![
            make_assign("a", HirExpr::Var("b".to_owned())),
            make_assign("b", HirExpr::Var("a".to_owned())),
        ];
        let mut func = make_func(
            vec![make_binding("a"), make_binding("b")],
            body,
            NirType::Unknown,
        );
        super::apply_type_inference_pass(&mut func); // must not panic
        assert_eq!(func.locals[0].ty, NirType::Unknown);
        assert_eq!(func.locals[1].ty, NirType::Unknown);
    }

    /// `return x` where `x = Const(0, int)` → return_type inferred as `int`
    #[test]
    fn rederives_return_type_from_var() {
        let body = vec![
            make_assign(
                "x",
                HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            ),
            HirStmt::Return(Some(HirExpr::Var("x".to_owned()))),
        ];
        let mut func = make_func(vec![make_binding("x")], body, NirType::Unknown);
        super::apply_type_inference_pass(&mut func);
        assert_eq!(
            func.return_type,
            NirType::Int {
                bits: 32,
                signed: true
            }
        );
    }

    /// If return_type is already known, do not overwrite it.
    #[test]
    fn does_not_overwrite_known_return_type() {
        let body = vec![HirStmt::Return(Some(HirExpr::Const(
            1,
            NirType::Int {
                bits: 64,
                signed: false,
            },
        )))];
        let existing_type = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = make_func(vec![], body, existing_type.clone());
        super::apply_type_inference_pass(&mut func);
        // return_type was non-Unknown going in — should NOT be changed by the pass
        // (the pass only updates when return_type is Unknown)
        assert_eq!(func.return_type, existing_type);
    }

    /// Cast expression: `x = (ulonglong)y` → x.ty inferred as `ulonglong`
    #[test]
    fn infers_type_from_cast_rhs() {
        let body = vec![make_assign(
            "x",
            HirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(HirExpr::Var("y".to_owned())),
            },
        )];
        let mut func = make_func(vec![make_binding("x")], body, NirType::Unknown);
        super::apply_type_inference_pass(&mut func);
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 64,
                signed: false
            }
        );
    }

    /// surface_type_name set → ty must NOT be overwritten by inference.
    #[test]
    fn respects_surface_type_name_override() {
        let body = vec![make_assign(
            "x",
            HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        )];
        let mut binding = make_binding("x");
        binding.surface_type_name = Some("DWORD".to_owned());
        let mut func = make_func(vec![binding], body, NirType::Unknown);
        super::apply_type_inference_pass(&mut func);
        // ty must remain Unknown — only surface_type_name is authoritative
        assert_eq!(func.locals[0].ty, NirType::Unknown);
    }

    #[test]
    fn narrows_zero_extended_return_width_from_all_arms() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_param("param_1", u32_ty.clone()),
                make_param("param_2", u32_ty.clone()),
            ],
            locals: vec![],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_owned()),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(HirExpr::Var("param_2".to_owned())),
                    }))],
                    else_body: vec![],
                },
                HirStmt::Return(Some(HirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);
        assert!(changed);
        assert_eq!(func.return_type, u32_ty);
        let HirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("expected if");
        };
        assert!(matches!(
            &then_body[0],
            HirStmt::Return(Some(HirExpr::Var(name))) if name == "param_2"
        ));
    }

    #[test]
    fn strips_zero_extended_return_cast_when_return_width_is_already_narrow() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", u32_ty.clone())],
            locals: vec![],
            return_type: u32_ty,
            surface_return_type_name: None,
            body: vec![HirStmt::Return(Some(HirExpr::Cast {
                ty: u64_ty,
                expr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                    rhs: Box::new(HirExpr::Const(
                        10,
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                }),
            }))],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);

        assert!(changed);
        assert!(matches!(
            &func.body[0],
            HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                ..
            }))
        ));
    }

    #[test]
    fn keeps_wide_return_when_any_arm_lacks_narrow_evidence() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", u32_ty)],
            locals: vec![],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                HirStmt::Return(Some(HirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(HirExpr::Var("param_1".to_owned())),
                })),
                HirStmt::Return(Some(HirExpr::Var("unknown_wide".to_owned()))),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);
        assert!(!changed);
        assert_eq!(func.return_type, u64_ty);
    }

    #[test]
    fn narrows_mixed_zero_extended_return_candidates_to_signed_width() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", i32_ty.clone())],
            locals: vec![NirBinding {
                name: "tmp".to_owned(),
                ty: u32_ty,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            return_type: u64_ty,
            surface_return_type_name: None,
            body: vec![
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_owned()),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Var(
                        "param_1".to_owned(),
                    )))],
                    else_body: vec![],
                },
                HirStmt::Return(Some(HirExpr::Var("tmp".to_owned()))),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);

        assert!(changed);
        assert_eq!(func.return_type, i32_ty);
    }

    #[test]
    fn narrows_zero_extended_return_through_typed_alias_slot() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let i64_ty = NirType::Int {
            bits: 64,
            signed: true,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let local = |name: &str, ty: NirType| NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        let mut func = HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", i32_ty.clone())],
            locals: vec![
                local("rdi", i64_ty.clone()),
                local("wide_acc", i64_ty),
                local("ret32", u32_ty),
                local("ret64", u64_ty.clone()),
            ],
            return_type: u64_ty,
            surface_return_type_name: None,
            body: vec![
                make_assign("rdi", HirExpr::Var("param_1".to_owned())),
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_owned()),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Var("rdi".to_owned())))],
                    else_body: vec![],
                },
                make_assign("ret32", HirExpr::Var("wide_acc".to_owned())),
                make_assign("ret64", HirExpr::Var("ret32".to_owned())),
                HirStmt::Return(Some(HirExpr::Var("ret64".to_owned()))),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);

        assert!(changed);
        assert_eq!(func.return_type, i32_ty);
    }

    #[test]
    fn promotes_same_width_zero_extended_return_signedness_through_alias_slot() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let i64_ty = NirType::Int {
            bits: 64,
            signed: true,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let local = |name: &str, ty: NirType| NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        let mut func = HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", i32_ty.clone())],
            locals: vec![
                local("rdi", i64_ty.clone()),
                local("wide_acc", i64_ty),
                local("ret32", u32_ty.clone()),
                local("ret64", u64_ty),
            ],
            return_type: u32_ty,
            surface_return_type_name: None,
            body: vec![
                make_assign("rdi", HirExpr::Var("param_1".to_owned())),
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_owned()),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Var("rdi".to_owned())))],
                    else_body: vec![],
                },
                make_assign("ret32", HirExpr::Var("wide_acc".to_owned())),
                make_assign("ret64", HirExpr::Var("ret32".to_owned())),
                HirStmt::Return(Some(HirExpr::Var("ret64".to_owned()))),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);

        assert!(changed);
        assert_eq!(func.return_type, i32_ty);
    }

    /// `validate_input`-style: function with ulonglong return type where all return
    /// expressions are 64-bit constants whose values fit in 32 bits and have bit 31
    /// set (i.e., negative signed ints).  Expected: return type narrows to `int` and
    /// constants are rewritten to their signed 32-bit equivalents.
    #[test]
    fn narrows_u64_constant_returns_to_signed_i32() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        // Simulates: return -1; return -2; return param1 + param2;
        // After narrowing, constants should become -1, -2 and return type int.
        let mut func = HirFunction {
            name: "validate_input".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_param("param_1", NirType::Int { bits: 32, signed: true }),
                make_param("param_2", NirType::Int { bits: 32, signed: true }),
            ],
            locals: vec![],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                HirStmt::If {
                    cond: HirExpr::Var("c1".to_owned()),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(
                        4294967295, // 0xFFFFFFFF = -1 as u32
                        u64_ty.clone(),
                    )))],
                    else_body: vec![],
                },
                HirStmt::If {
                    cond: HirExpr::Var("c2".to_owned()),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(
                        4294967294, // 0xFFFFFFFE = -2 as u32
                        u64_ty.clone(),
                    )))],
                    else_body: vec![],
                },
                // Simulate: return (ulonglong)(uint)(int)(param_1 + param_2)
                // The outer u64 ZExt cast is what the decompiler produces for x86-64.
                HirStmt::Return(Some(HirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(HirExpr::Cast {
                        ty: NirType::Int { bits: 32, signed: false },
                        expr: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Var("param_1".to_owned())),
                            rhs: Box::new(HirExpr::Var("param_2".to_owned())),
                            ty: NirType::Int { bits: 32, signed: true },
                        }),
                    }),
                })),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);

        assert!(changed, "pass should change something");
        assert_eq!(func.return_type, i32_ty, "return type should narrow to int");

        // Verify constants were rewritten to their signed 32-bit values.
        let HirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("expected if statement");
        };
        let HirStmt::Return(Some(HirExpr::Const(v, ty))) = &then_body[0] else {
            panic!("expected return const");
        };
        assert_eq!(*v, -1i64, "0xFFFFFFFF should become -1");
        assert_eq!(*ty, i32_ty);
    }
}
