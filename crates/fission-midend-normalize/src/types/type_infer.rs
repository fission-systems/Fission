use super::super::analysis::defuse::{DefinitionDependencyMap, collect_expr_vars};
/// Intra-function type inference pass.
///
/// Ghidra's `ActionInferTypes::propagateOneType` follows data-flow edges in the
/// full SSA graph. Here we approximate the same idea using Fission's already-
/// structured HIR: since the HIR is in near-SSA form (most variables are
/// single-assignment after normalization), we can reconstruct types by walking
/// the def map without a full data-flow framework.
///
/// Algorithm:
/// 1. `scan_def_types(body)` — build a `HashMap<name, DirExpr>` from the first
///    assignment to each variable anywhere in the body tree.
/// 2. `infer_type_for_binding(name, defs, visited)` — recursively derive the
///    type of a named binding.  If the definition is `Var(other)` we follow the
///    chain (cycle-protected with a `HashSet`); otherwise we call `expr_type`.
/// 3. `apply_type_inference_pass(func)` — for every `DirBinding` whose `ty` is
///    `Unknown` _and_ whose `surface_type_name` is unset, replace `ty` with the
///    inferred result.  Also re-derives `DirFunction.return_type` for the common
///    `return <var>;` pattern that previously always produced `undefined`.
///
/// This pass is binary-independent: it only propagates types
/// that are already embedded in typed sub-expressions (Const, Cast, Binary, …).
use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Collect the first assignment expression type for each named variable in the
/// body.  We store `(NirType, Option<String>)` where the Option carries the
/// target variable name when the RHS is a `Var` — so we can chain-resolve later.
///
/// Storing owned types (not references) avoids lifetime conflicts when we
/// later mutate `func` to apply the inferred types.
fn scan_def_types(stmts: &[DirStmt], defs: &mut HashMap<String, DefEntry>) {
    for stmt in stmts {
        scan_def_types_stmt(stmt, defs);
    }
}

/// Either a concrete type inferred from the expression, or the name of another
/// variable whose type we still need to chase (for `x = y` patterns).
enum DefEntry {
    Known(NirType),
    Alias(String),
    TypedAlias {
        source: String,
        ty: NirType,
    },
    Derived {
        sources: HashSet<String>,
        ty: NirType,
    },
}

fn scan_def_types_stmt(stmt: &DirStmt, defs: &mut HashMap<String, DefEntry>) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => {
            if defs.contains_key(name.as_str()) {
                // Only record the first definition (near-SSA assumption).
                return;
            }
            let entry = match rhs {
                DirExpr::Var(src) => DefEntry::Alias(src.clone()),
                DirExpr::Cast { ty, expr } if matches!(expr.as_ref(), DirExpr::Var(_)) => {
                    let DirExpr::Var(source) = expr.as_ref() else {
                        unreachable!();
                    };
                    DefEntry::TypedAlias {
                        source: source.clone(),
                        ty: ty.clone(),
                    }
                }
                other => {
                    let ty = expr_type(other);
                    let mut sources = HashSet::default();
                    collect_value_provenance_vars(other, &mut sources);
                    if sources.is_empty() {
                        DefEntry::Known(ty)
                    } else {
                        DefEntry::Derived { sources, ty }
                    }
                }
            };
            defs.insert(name.clone(), entry);
        }
        DirStmt::Block(stmts) => scan_def_types(stmts, defs),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            scan_def_types(then_body, defs);
            scan_def_types(else_body, defs);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => scan_def_types(body, defs),
        DirStmt::For {
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
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                scan_def_types(&case.body, defs);
            }
            scan_def_types(default, defs);
        }
        _ => {}
    }
}

fn collect_value_provenance_vars(expr: &DirExpr, out: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) => {
            out.insert(name.clone());
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => {
            collect_value_provenance_vars(expr, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_value_provenance_vars(lhs, out);
            collect_value_provenance_vars(rhs, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_value_provenance_vars(cond, out);
            collect_value_provenance_vars(then_expr, out);
            collect_value_provenance_vars(else_expr, out);
        }
        // The loaded value does not inherit the scalar role of its address.
        DirExpr::Load { .. }
        | DirExpr::Index { .. }
        | DirExpr::FieldAccess { .. }
        | DirExpr::Call { .. }
        | DirExpr::AddressOfGlobal(_)
        | DirExpr::Const(_, _) => {}
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
        Some(DefEntry::TypedAlias { ty, .. }) if *ty != NirType::Unknown => ty.clone(),
        Some(DefEntry::TypedAlias { source, .. }) => {
            let source = source.clone();
            infer_type_for_binding(&source, defs, known_binding_types, visited)
        }
        Some(DefEntry::Derived { ty, .. }) if *ty != NirType::Unknown => ty.clone(),
        Some(DefEntry::Derived { .. }) => known_binding_types
            .get(name)
            .cloned()
            .unwrap_or(NirType::Unknown),
    }
}

fn collect_known_binding_types(func: &DirFunction) -> HashMap<String, NirType> {
    let mut known = HashMap::default();
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

#[derive(Default)]
struct BindingUseRole {
    address_use: bool,
    strong_scalar_use: bool,
    address_pointee_type: Option<NirType>,
}

fn scalar_role_type_for_function(func: &DirFunction) -> NirType {
    NirType::Int {
        bits: if func.is_64bit { 64 } else { 32 },
        signed: false,
    }
}

fn apply_scalar_role_override_for_pointer_locals(func: &mut DirFunction) -> bool {
    let mut roles: HashMap<String, BindingUseRole> = HashMap::default();
    collect_binding_use_roles_stmts(&func.body, &mut roles);
    let scalar_ty = scalar_role_type_for_function(func);
    let mut changed = false;

    for binding in &mut func.locals {
        if binding.surface_type_name.is_some() || !matches!(binding.ty, NirType::Ptr(_)) {
            continue;
        }
        let Some(role) = roles.get(&binding.name) else {
            continue;
        };
        if role.strong_scalar_use && !role.address_use {
            binding.ty = scalar_ty.clone();
            changed = true;
        }
    }

    changed
}

fn default_address_pointee_type() -> NirType {
    NirType::Int {
        bits: 8,
        signed: false,
    }
}

fn apply_address_role_pointer_override_for_locals(func: &mut DirFunction) -> bool {
    let mut roles: HashMap<String, BindingUseRole> = HashMap::default();
    collect_binding_use_roles_stmts(&func.body, &mut roles);
    let mut changed = false;

    for binding in &mut func.locals {
        if binding.surface_type_name.is_some() || matches!(binding.ty, NirType::Ptr(_)) {
            continue;
        }
        let Some(role) = roles.get(&binding.name) else {
            continue;
        };
        if role.address_use {
            let pointee = role
                .address_pointee_type
                .clone()
                .filter(|ty| *ty != NirType::Unknown)
                .unwrap_or_else(default_address_pointee_type);
            binding.ty = NirType::Ptr(Box::new(pointee));
            changed = true;
        }
    }

    changed
}

pub(super) fn transitive_address_pointer_locals(func: &DirFunction) -> HashMap<String, NirType> {
    let pointer_roots: HashSet<String> = func
        .params
        .iter()
        .filter(|binding| matches!(binding.ty, NirType::Ptr(_)))
        .map(|binding| binding.name.clone())
        .collect();
    if pointer_roots.is_empty() {
        return HashMap::default();
    }
    let local_names: HashSet<&str> = func
        .locals
        .iter()
        .map(|binding| binding.name.as_str())
        .collect();
    let dependencies = DefinitionDependencyMap::build(&func.body);
    dependencies
        .address_contributors(&func.body, &pointer_roots)
        .into_iter()
        .filter(|(name, _)| local_names.contains(name.as_str()))
        .map(|(name, pointee)| (name, NirType::Ptr(Box::new(pointee))))
        .collect()
}

fn apply_transitive_address_pointer_override_for_locals(func: &mut DirFunction) -> bool {
    let contributors = transitive_address_pointer_locals(func);
    if contributors.is_empty() {
        return false;
    }
    let mut changed = false;
    for binding in &mut func.locals {
        if binding.surface_type_name.is_some() {
            continue;
        }
        if let Some(pointer_ty) = contributors.get(&binding.name)
            && binding.ty != *pointer_ty
        {
            binding.ty = pointer_ty.clone();
            changed = true;
        }
    }
    changed
}

/// When a local is equality-compared with a pointer-typed value, promote that
/// local to the same pointer type.
///
/// A register can be reused for a computed end pointer and later compared with
/// a cursor. Comparing with a known pointer is strong evidence that the peer is
/// also a pointer of the same machine-word width.
fn apply_pointer_compare_peer_override_for_locals(func: &mut DirFunction) -> bool {
    let promote = pointer_compare_peer_promotions(func);
    if promote.is_empty() {
        return false;
    }
    let mut changed = false;
    for binding in &mut func.locals {
        if binding.surface_type_name.is_some() || matches!(binding.ty, NirType::Ptr(_)) {
            continue;
        }
        if let Some(ptr_ty) = promote.get(&binding.name) {
            binding.ty = ptr_ty.clone();
            changed = true;
        }
    }
    changed
}

pub(super) fn pointer_compare_peer_promotions(func: &DirFunction) -> HashMap<String, NirType> {
    let types = collect_known_binding_types(func);
    let mut promote: HashMap<String, NirType> = HashMap::default();
    collect_pointer_compare_peer_promotions(&func.body, &types, &mut promote);
    promote
}

fn collect_pointer_compare_peer_promotions(
    stmts: &[DirStmt],
    types: &HashMap<String, NirType>,
    out: &mut HashMap<String, NirType>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } => {
                collect_pointer_compare_peer_promotions(body, types, out);
            }
            DirStmt::DoWhile { body, cond } => {
                collect_pointer_compare_peer_promotions(body, types, out);
                collect_pointer_compare_peer_promotions_expr(cond, types, out);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_pointer_compare_peer_promotions_expr(cond, types, out);
                collect_pointer_compare_peer_promotions(then_body, types, out);
                collect_pointer_compare_peer_promotions(else_body, types, out);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_pointer_compare_peer_promotions(std::slice::from_ref(init), types, out);
                }
                if let Some(cond) = cond {
                    collect_pointer_compare_peer_promotions_expr(cond, types, out);
                }
                if let Some(update) = update {
                    collect_pointer_compare_peer_promotions(
                        std::slice::from_ref(update),
                        types,
                        out,
                    );
                }
                collect_pointer_compare_peer_promotions(body, types, out);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_pointer_compare_peer_promotions_expr(expr, types, out);
                for case in cases {
                    collect_pointer_compare_peer_promotions(&case.body, types, out);
                }
                collect_pointer_compare_peer_promotions(default, types, out);
            }
            DirStmt::Assign { rhs, .. } => {
                collect_pointer_compare_peer_promotions_expr(rhs, types, out);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                collect_pointer_compare_peer_promotions_expr(expr, types, out);
            }
            _ => {}
        }
    }
}

fn collect_pointer_compare_peer_promotions_expr(
    expr: &DirExpr,
    types: &HashMap<String, NirType>,
    out: &mut HashMap<String, NirType>,
) {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Eq | DirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => {
            let lhs_ptr = pointer_type_of_expr(lhs, types);
            let rhs_ptr = pointer_type_of_expr(rhs, types);
            if let (Some(ptr_ty), DirExpr::Var(name)) = (lhs_ptr.as_ref(), rhs.as_ref()) {
                out.entry(name.clone()).or_insert_with(|| ptr_ty.clone());
            }
            if let (Some(ptr_ty), DirExpr::Var(name)) = (rhs_ptr.as_ref(), lhs.as_ref()) {
                out.entry(name.clone()).or_insert_with(|| ptr_ty.clone());
            }
            collect_pointer_compare_peer_promotions_expr(lhs, types, out);
            collect_pointer_compare_peer_promotions_expr(rhs, types, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_pointer_compare_peer_promotions_expr(lhs, types, out);
            collect_pointer_compare_peer_promotions_expr(rhs, types, out);
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            collect_pointer_compare_peer_promotions_expr(expr, types, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_pointer_compare_peer_promotions_expr(cond, types, out);
            collect_pointer_compare_peer_promotions_expr(then_expr, types, out);
            collect_pointer_compare_peer_promotions_expr(else_expr, types, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_pointer_compare_peer_promotions_expr(arg, types, out);
            }
        }
        DirExpr::Load { ptr, .. }
        | DirExpr::PtrOffset { base: ptr, .. }
        | DirExpr::FieldAccess { base: ptr, .. }
        | DirExpr::AggregateCopy { src: ptr, .. } => {
            collect_pointer_compare_peer_promotions_expr(ptr, types, out);
        }
        DirExpr::Index { base, index, .. } => {
            collect_pointer_compare_peer_promotions_expr(base, types, out);
            collect_pointer_compare_peer_promotions_expr(index, types, out);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn zero_initializer_aliases(func: &DirFunction) -> HashSet<String> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter_map(|binding| match binding.initializer.as_ref() {
            Some(DirExpr::Const(0, _)) => Some(binding.name.clone()),
            _ => None,
        })
        .collect()
}

fn rewrite_scalar_zero_alias_assignments(func: &mut DirFunction) -> bool {
    let zero_aliases = zero_initializer_aliases(func);
    if zero_aliases.is_empty() {
        return false;
    }
    let binding_types = collect_known_binding_types(func);
    rewrite_scalar_zero_alias_stmts(&mut func.body, &binding_types, &zero_aliases)
}

fn rewrite_scalar_zero_alias_stmts(
    stmts: &mut [DirStmt],
    binding_types: &HashMap<String, NirType>,
    zero_aliases: &HashSet<String>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= rewrite_scalar_zero_alias_stmt(stmt, binding_types, zero_aliases);
    }
    changed
}

fn rewrite_scalar_zero_alias_stmt(
    stmt: &mut DirStmt,
    binding_types: &HashMap<String, NirType>,
    zero_aliases: &HashSet<String>,
) -> bool {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs),
            rhs,
        } => {
            let DirExpr::Var(src) = rhs else {
                return false;
            };
            let Some(lhs_ty) = binding_types.get(lhs.as_str()) else {
                return false;
            };
            if !matches!(lhs_ty, NirType::Int { .. }) || !zero_aliases.contains(src.as_str()) {
                return false;
            }
            *rhs = DirExpr::Const(0, lhs_ty.clone());
            true
        }
        DirStmt::Block(stmts) | DirStmt::While { body: stmts, .. } => {
            rewrite_scalar_zero_alias_stmts(stmts, binding_types, zero_aliases)
        }
        DirStmt::DoWhile { body, .. } => {
            rewrite_scalar_zero_alias_stmts(body, binding_types, zero_aliases)
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = false;
            if let Some(init) = init {
                changed |= rewrite_scalar_zero_alias_stmt(init, binding_types, zero_aliases);
            }
            if let Some(update) = update {
                changed |= rewrite_scalar_zero_alias_stmt(update, binding_types, zero_aliases);
            }
            changed | rewrite_scalar_zero_alias_stmts(body, binding_types, zero_aliases)
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            rewrite_scalar_zero_alias_stmts(then_body, binding_types, zero_aliases)
                | rewrite_scalar_zero_alias_stmts(else_body, binding_types, zero_aliases)
        }
        DirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |=
                    rewrite_scalar_zero_alias_stmts(&mut case.body, binding_types, zero_aliases);
            }
            changed | rewrite_scalar_zero_alias_stmts(default, binding_types, zero_aliases)
        }
        _ => false,
    }
}

fn param_name_set(func: &DirFunction) -> HashSet<String> {
    func.params
        .iter()
        .filter(|param| param.surface_type_name.is_none())
        .map(|param| param.name.clone())
        .collect()
}

struct ParamPointerRoleContext<'a> {
    defs: &'a HashMap<String, DefEntry>,
    binding_types: &'a HashMap<String, NirType>,
    params: &'a HashSet<String>,
    address_params: &'a HashSet<String>,
    strong_scalar_params: &'a HashSet<String>,
}

#[derive(Default)]
struct StrongScalarParamRoots {
    all: HashSet<String>,
    shifts: HashSet<String>,
}

fn extend_first_def_param_roots(
    names: impl IntoIterator<Item = String>,
    defs: &HashMap<String, DefEntry>,
    params: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    for name in names {
        collect_first_def_param_roots(&name, defs, params, &mut HashSet::default(), out);
    }
}

fn collect_strong_scalar_param_roots_stmts(
    stmts: &[DirStmt],
    dependencies: &HashMap<String, DefEntry>,
    binding_types: &HashMap<String, NirType>,
    params: &HashSet<String>,
    out: &mut StrongScalarParamRoots,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { rhs, .. } | DirStmt::Expr(rhs) | DirStmt::Return(Some(rhs)) => {
                collect_strong_scalar_param_roots_expr(
                    rhs,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } => {
                collect_strong_scalar_param_roots_stmts(
                    body,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            DirStmt::DoWhile { body, cond } => {
                collect_strong_scalar_param_roots_stmts(
                    body,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
                collect_strong_scalar_param_roots_expr(
                    cond,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_strong_scalar_param_roots_expr(
                    cond,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
                collect_strong_scalar_param_roots_stmts(
                    then_body,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
                collect_strong_scalar_param_roots_stmts(
                    else_body,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_strong_scalar_param_roots_stmts(
                        std::slice::from_ref(init),
                        dependencies,
                        binding_types,
                        params,
                        out,
                    );
                }
                if let Some(cond) = cond {
                    collect_strong_scalar_param_roots_expr(
                        cond,
                        dependencies,
                        binding_types,
                        params,
                        out,
                    );
                }
                if let Some(update) = update {
                    collect_strong_scalar_param_roots_stmts(
                        std::slice::from_ref(update),
                        dependencies,
                        binding_types,
                        params,
                        out,
                    );
                }
                collect_strong_scalar_param_roots_stmts(
                    body,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_strong_scalar_param_roots_expr(
                    expr,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
                for case in cases {
                    collect_strong_scalar_param_roots_stmts(
                        &case.body,
                        dependencies,
                        binding_types,
                        params,
                        out,
                    );
                }
                collect_strong_scalar_param_roots_stmts(
                    default,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            DirStmt::VaStart { va_list, .. } => {
                collect_strong_scalar_param_roots_expr(
                    va_list,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            DirStmt::Return(None)
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn collect_strong_scalar_param_roots_expr(
    expr: &DirExpr,
    dependencies: &HashMap<String, DefEntry>,
    binding_types: &HashMap<String, NirType>,
    params: &HashSet<String>,
    out: &mut StrongScalarParamRoots,
) {
    match expr {
        DirExpr::Binary { op, lhs, rhs, .. } => {
            if matches!(op, DirBinaryOp::Shl | DirBinaryOp::Shr | DirBinaryOp::Sar) {
                let mut names = HashSet::default();
                if !expr_has_first_def_pointer_type(lhs, dependencies) {
                    collect_expr_vars(lhs, &mut names);
                }
                if !expr_has_first_def_pointer_type(rhs, dependencies) {
                    collect_expr_vars(rhs, &mut names);
                }
                let mut roots = HashSet::default();
                extend_first_def_param_roots(names, dependencies, params, &mut roots);
                out.all.extend(roots.iter().cloned());
                out.shifts.extend(roots);
            }
            if matches!(
                op,
                DirBinaryOp::Lt
                    | DirBinaryOp::Le
                    | DirBinaryOp::Gt
                    | DirBinaryOp::Ge
                    | DirBinaryOp::SLt
                    | DirBinaryOp::SLe
                    | DirBinaryOp::SGt
                    | DirBinaryOp::SGe
            ) {
                if expr_looks_integer_offset(lhs, binding_types)
                    && !expr_has_first_def_pointer_type(rhs, dependencies)
                {
                    let mut names = HashSet::default();
                    collect_expr_vars(rhs, &mut names);
                    extend_first_def_param_roots(names, dependencies, params, &mut out.all);
                }
                if expr_looks_integer_offset(rhs, binding_types)
                    && !expr_has_first_def_pointer_type(lhs, dependencies)
                {
                    let mut names = HashSet::default();
                    collect_expr_vars(lhs, &mut names);
                    extend_first_def_param_roots(names, dependencies, params, &mut out.all);
                }
            }
            collect_strong_scalar_param_roots_expr(lhs, dependencies, binding_types, params, out);
            collect_strong_scalar_param_roots_expr(rhs, dependencies, binding_types, params, out);
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => {
            collect_strong_scalar_param_roots_expr(expr, dependencies, binding_types, params, out);
        }
        DirExpr::Index { base, index, .. } => {
            collect_strong_scalar_param_roots_expr(base, dependencies, binding_types, params, out);
            collect_strong_scalar_param_roots_expr(index, dependencies, binding_types, params, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_strong_scalar_param_roots_expr(cond, dependencies, binding_types, params, out);
            collect_strong_scalar_param_roots_expr(
                then_expr,
                dependencies,
                binding_types,
                params,
                out,
            );
            collect_strong_scalar_param_roots_expr(
                else_expr,
                dependencies,
                binding_types,
                params,
                out,
            );
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_strong_scalar_param_roots_expr(
                    arg,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

/// Collect parameters that appear as the integer-offset side of a pointer add.
///
/// The pointer-base side must have independent address-use evidence. This keeps
/// the classification fail-closed when both operands are merely pointer-sized.
fn collect_param_pointer_offset_params_stmts(
    stmts: &[DirStmt],
    context: &ParamPointerRoleContext<'_>,
    out: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { rhs, .. } | DirStmt::Expr(rhs) | DirStmt::Return(Some(rhs)) => {
                collect_param_pointer_offset_params_expr(rhs, context, out);
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } => {
                collect_param_pointer_offset_params_stmts(body, context, out);
            }
            DirStmt::DoWhile { body, cond } => {
                collect_param_pointer_offset_params_stmts(body, context, out);
                collect_param_pointer_offset_params_expr(cond, context, out);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_param_pointer_offset_params_expr(cond, context, out);
                collect_param_pointer_offset_params_stmts(then_body, context, out);
                collect_param_pointer_offset_params_stmts(else_body, context, out);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_param_pointer_offset_params_stmts(
                        std::slice::from_ref(init),
                        context,
                        out,
                    );
                }
                if let Some(cond) = cond {
                    collect_param_pointer_offset_params_expr(cond, context, out);
                }
                if let Some(update) = update {
                    collect_param_pointer_offset_params_stmts(
                        std::slice::from_ref(update),
                        context,
                        out,
                    );
                }
                collect_param_pointer_offset_params_stmts(body, context, out);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_param_pointer_offset_params_expr(expr, context, out);
                for case in cases {
                    collect_param_pointer_offset_params_stmts(&case.body, context, out);
                }
                collect_param_pointer_offset_params_stmts(default, context, out);
            }
            _ => {}
        }
    }
}

fn record_offset_param_from_expr(
    expr: &DirExpr,
    context: &ParamPointerRoleContext<'_>,
    out: &mut HashSet<String>,
) {
    if expr_has_first_def_pointer_type(expr, context.defs) {
        return;
    }
    let mut cur = expr;
    while let DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } = cur {
        cur = expr.as_ref();
    }
    let mut names = HashSet::default();
    collect_expr_vars(cur, &mut names);
    for name in names {
        let mut roots = HashSet::default();
        collect_first_def_param_roots(
            &name,
            context.defs,
            context.params,
            &mut HashSet::default(),
            &mut roots,
        );
        for param in roots {
            if !context.address_params.contains(&param)
                || context.strong_scalar_params.contains(&param)
            {
                out.insert(param);
            }
        }
    }
}

fn expr_is_pointer_base(expr: &DirExpr, context: &ParamPointerRoleContext<'_>) -> bool {
    let mut cur = expr;
    while let DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } = cur {
        cur = expr.as_ref();
    }
    let DirExpr::Var(name) = cur else {
        return false;
    };
    if !context.params.contains(name.as_str())
        && matches!(
            context.binding_types.get(name.as_str()),
            Some(NirType::Ptr(_))
        )
    {
        return true;
    }
    resolve_alias_to_param(name, context.defs, context.params, &mut HashSet::default())
        .is_some_and(|param| context.address_params.contains(&param))
}

fn collect_param_pointer_offset_params_expr(
    expr: &DirExpr,
    context: &ParamPointerRoleContext<'_>,
    out: &mut HashSet<String>,
) {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if expr_is_pointer_base(lhs, context) {
                record_offset_param_from_expr(rhs, context, out);
            }
            if expr_is_pointer_base(rhs, context) {
                record_offset_param_from_expr(lhs, context, out);
            }
            collect_param_pointer_offset_params_expr(lhs, context, out);
            collect_param_pointer_offset_params_expr(rhs, context, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_param_pointer_offset_params_expr(lhs, context, out);
            collect_param_pointer_offset_params_expr(rhs, context, out);
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            collect_param_pointer_offset_params_expr(expr, context, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_param_pointer_offset_params_expr(cond, context, out);
            collect_param_pointer_offset_params_expr(then_expr, context, out);
            collect_param_pointer_offset_params_expr(else_expr, context, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_param_pointer_offset_params_expr(arg, context, out);
            }
        }
        DirExpr::Load { ptr, .. }
        | DirExpr::PtrOffset { base: ptr, .. }
        | DirExpr::FieldAccess { base: ptr, .. }
        | DirExpr::AggregateCopy { src: ptr, .. } => {
            collect_param_pointer_offset_params_expr(ptr, context, out);
        }
        DirExpr::Index { base, index, .. } => {
            collect_param_pointer_offset_params_expr(base, context, out);
            collect_param_pointer_offset_params_expr(index, context, out);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn resolve_alias_to_param(
    name: &str,
    defs: &HashMap<String, DefEntry>,
    params: &HashSet<String>,
    visited: &mut HashSet<String>,
) -> Option<String> {
    if !visited.insert(name.to_string()) {
        return None;
    }
    if params.contains(name) {
        return Some(name.to_string());
    }
    match defs.get(name) {
        Some(DefEntry::Alias(src)) => resolve_alias_to_param(src, defs, params, visited),
        Some(DefEntry::TypedAlias { source, .. }) => {
            resolve_alias_to_param(source, defs, params, visited)
        }
        _ => None,
    }
}

fn expr_has_first_def_pointer_type(expr: &DirExpr, defs: &HashMap<String, DefEntry>) -> bool {
    fn binding_has_pointer_type(
        name: &str,
        defs: &HashMap<String, DefEntry>,
        visited: &mut HashSet<String>,
    ) -> bool {
        if !visited.insert(name.to_string()) {
            return false;
        }
        match defs.get(name) {
            Some(DefEntry::Known(NirType::Ptr(_)))
            | Some(DefEntry::TypedAlias {
                ty: NirType::Ptr(_),
                ..
            })
            | Some(DefEntry::Derived {
                ty: NirType::Ptr(_),
                ..
            }) => true,
            Some(DefEntry::Alias(source)) | Some(DefEntry::TypedAlias { source, .. }) => {
                binding_has_pointer_type(source, defs, visited)
            }
            _ => false,
        }
    }

    match expr {
        DirExpr::Var(name) => binding_has_pointer_type(name, defs, &mut HashSet::default()),
        DirExpr::Cast {
            ty: NirType::Ptr(_),
            ..
        } => true,
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            expr_has_first_def_pointer_type(expr, defs)
        }
        _ => false,
    }
}

fn collect_first_def_param_roots(
    name: &str,
    defs: &HashMap<String, DefEntry>,
    params: &HashSet<String>,
    visited: &mut HashSet<String>,
    out: &mut HashSet<String>,
) {
    if !visited.insert(name.to_string()) {
        return;
    }
    if params.contains(name) {
        out.insert(name.to_string());
        return;
    }
    match defs.get(name) {
        Some(DefEntry::Alias(source)) | Some(DefEntry::TypedAlias { source, .. }) => {
            collect_first_def_param_roots(source, defs, params, visited, out);
        }
        Some(DefEntry::Derived { sources, .. }) => {
            for source in sources {
                collect_first_def_param_roots(source, defs, params, visited, out);
            }
        }
        Some(DefEntry::Known(_)) | None => {}
    }
}

fn pointer_type_of_expr(
    expr: &DirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        DirExpr::Var(name) => binding_types.get(name).and_then(|ty| match ty {
            NirType::Ptr(_) => Some(ty.clone()),
            _ => None,
        }),
        DirExpr::Cast {
            ty: NirType::Ptr(_),
            ..
        } => Some(expr_type(expr)),
        DirExpr::PtrOffset { base, .. }
        | DirExpr::Load { ptr: base, .. }
        | DirExpr::FieldAccess { base, .. }
        | DirExpr::AggregateCopy { src: base, .. } => pointer_type_of_expr(base, binding_types),
        DirExpr::Index { base, .. } => pointer_type_of_expr(base, binding_types),
        _ => None,
    }
}

struct ParamPointerCandidateContext<'a> {
    defs: &'a HashMap<String, DefEntry>,
    dependencies: &'a DefinitionDependencyMap,
    binding_types: &'a HashMap<String, NirType>,
    params: &'a HashSet<String>,
}

fn param_pointer_candidates_from_expr(
    expr: &DirExpr,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            // Pointer plus integer yields a pointer. Do not promote the peer
            // operand merely because the other side has pointer evidence;
            // recurse so nested address uses still contribute.
            //
            // Previously both sides were typed as the pointer when either side
            // was pointer-typed, which forced `len: uchar *` and broke callers
            // that pass an integer length.
            param_pointer_candidates_from_expr(lhs, context, out);
            param_pointer_candidates_from_expr(rhs, context, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            param_pointer_candidates_from_expr(lhs, context, out);
            param_pointer_candidates_from_expr(rhs, context, out);
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            param_pointer_candidates_from_expr(expr, context, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            param_pointer_candidates_from_expr(cond, context, out);
            param_pointer_candidates_from_expr(then_expr, context, out);
            param_pointer_candidates_from_expr(else_expr, context, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                param_pointer_candidates_from_expr(arg, context, out);
            }
        }
        DirExpr::Load { ptr, ty } => {
            // Param used as a load address is a pointer to the loaded type.
            record_param_pointer_from_address_expr(ptr, ty, context, out);
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        DirExpr::PtrOffset { base: ptr, .. }
        | DirExpr::FieldAccess { base: ptr, .. }
        | DirExpr::AggregateCopy { src: ptr, .. } => {
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        DirExpr::Index { base, index, .. } => {
            param_pointer_candidates_from_expr(base, context, out);
            param_pointer_candidates_from_expr(index, context, out);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn record_param_pointer_from_address_expr(
    addr: &DirExpr,
    pointee: &NirType,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match addr {
        DirExpr::Var(name) => {
            let mut roots = HashSet::default();
            collect_first_def_param_roots(
                name,
                context.defs,
                context.params,
                &mut HashSet::default(),
                &mut roots,
            );
            if roots.is_empty() {
                let fallback = context
                    .dependencies
                    .address_roots_reaching(name, context.params);
                if fallback.len() == 1 {
                    roots.extend(fallback);
                }
            }
            for param in roots {
                out.entry(param)
                    .or_insert_with(|| NirType::Ptr(Box::new(pointee.clone())));
            }
        }
        // Load *(base + index): the integer-offset side stays scalar; the other
        // side (often a stack param buffer) is the pointer base.
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            let lhs_ptr = pointer_type_of_expr(lhs, context.binding_types).is_some();
            let rhs_ptr = pointer_type_of_expr(rhs, context.binding_types).is_some();
            let lhs_int = expr_looks_integer_offset(lhs, context.binding_types);
            let rhs_int = expr_looks_integer_offset(rhs, context.binding_types);
            if rhs_int && !lhs_ptr {
                record_param_pointer_from_address_expr(lhs, pointee, context, out);
            }
            if lhs_int && !rhs_ptr {
                record_param_pointer_from_address_expr(rhs, pointee, context, out);
            }
            // Neither side known: prefer Var that aliases a param when the other
            // is a non-param local (common: buf + i).
            if !lhs_int && !rhs_int {
                if matches!(rhs.as_ref(), DirExpr::Var(n) if !context.params.contains(n.as_str())) {
                    record_param_pointer_from_address_expr(lhs, pointee, context, out);
                }
                if matches!(lhs.as_ref(), DirExpr::Var(n) if !context.params.contains(n.as_str())) {
                    record_param_pointer_from_address_expr(rhs, pointee, context, out);
                }
            }
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            record_param_pointer_from_address_expr(expr, pointee, context, out);
        }
        _ => {}
    }
}

fn expr_looks_integer_offset(expr: &DirExpr, binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        DirExpr::Const(_, _) => true,
        DirExpr::Var(name) => matches!(
            binding_types.get(name.as_str()),
            Some(NirType::Int { .. } | NirType::Bool)
        ),
        DirExpr::Cast {
            ty: NirType::Int { .. },
            ..
        } => true,
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            expr_looks_integer_offset(expr, binding_types)
        }
        DirExpr::Binary {
            op: DirBinaryOp::Add | DirBinaryOp::Sub | DirBinaryOp::Mul | DirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            expr_looks_integer_offset(lhs, binding_types)
                || expr_looks_integer_offset(rhs, binding_types)
        }
        _ => false,
    }
}

fn param_pointer_candidates_from_lvalue(
    lhs: &DirLValue,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, ty } => {
            record_param_pointer_from_address_expr(ptr, ty, context, out);
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        DirLValue::FieldAccess { base: ptr, .. } => {
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        DirLValue::Index { base, index, .. } => {
            param_pointer_candidates_from_expr(base, context, out);
            param_pointer_candidates_from_expr(index, context, out);
        }
    }
}

fn collect_param_pointer_candidates_stmts(
    stmts: &[DirStmt],
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    for stmt in stmts {
        collect_param_pointer_candidates_stmt(stmt, context, out);
    }
}

fn collect_param_pointer_candidates_stmt(
    stmt: &DirStmt,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            param_pointer_candidates_from_lvalue(lhs, context, out);
            param_pointer_candidates_from_expr(rhs, context, out);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            param_pointer_candidates_from_expr(expr, context, out);
        }
        DirStmt::VaStart { va_list, .. } => {
            param_pointer_candidates_from_expr(va_list, context, out);
        }
        DirStmt::Block(stmts) | DirStmt::While { body: stmts, .. } => {
            collect_param_pointer_candidates_stmts(stmts, context, out);
        }
        DirStmt::DoWhile { body, cond } => {
            collect_param_pointer_candidates_stmts(body, context, out);
            param_pointer_candidates_from_expr(cond, context, out);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_param_pointer_candidates_stmt(init, context, out);
            }
            if let Some(cond) = cond {
                param_pointer_candidates_from_expr(cond, context, out);
            }
            if let Some(update) = update {
                collect_param_pointer_candidates_stmt(update, context, out);
            }
            collect_param_pointer_candidates_stmts(body, context, out);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            param_pointer_candidates_from_expr(cond, context, out);
            collect_param_pointer_candidates_stmts(then_body, context, out);
            collect_param_pointer_candidates_stmts(else_body, context, out);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            param_pointer_candidates_from_expr(expr, context, out);
            for case in cases {
                collect_param_pointer_candidates_stmts(&case.body, context, out);
            }
            collect_param_pointer_candidates_stmts(default, context, out);
        }
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn apply_address_contributor_param_pointer_types(
    func: &mut DirFunction,
    defs: &HashMap<String, DefEntry>,
    dependencies: &DefinitionDependencyMap,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    let params = param_name_set(func);
    if params.is_empty() {
        return false;
    }
    let mut candidates = HashMap::default();
    let candidate_context = ParamPointerCandidateContext {
        defs,
        dependencies,
        binding_types,
        params: &params,
    };
    collect_param_pointer_candidates_stmts(&func.body, &candidate_context, &mut candidates);
    let address_params: HashSet<String> = candidates.keys().cloned().collect();
    let mut strong_scalar_roots = StrongScalarParamRoots::default();
    collect_strong_scalar_param_roots_stmts(
        &func.body,
        defs,
        binding_types,
        &params,
        &mut strong_scalar_roots,
    );
    let strong_scalar_params = strong_scalar_roots.all;
    let shift_scalar_params = strong_scalar_roots.shifts;
    let role_context = ParamPointerRoleContext {
        defs,
        binding_types,
        params: &params,
        address_params: &address_params,
        strong_scalar_params: &strong_scalar_params,
    };
    // Parameters used as the integer side of pointer arithmetic stay scalar
    // even when weaker propagation would otherwise classify them as pointers.
    let mut offset_params = HashSet::default();
    collect_param_pointer_offset_params_stmts(&func.body, &role_context, &mut offset_params);
    let mut scalar_params = strong_scalar_params.clone();
    scalar_params.extend(offset_params.iter().cloned());
    for address_param in &address_params {
        if !shift_scalar_params.contains(address_param) {
            scalar_params.remove(address_param);
        }
    }
    static DIAG_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *DIAG_ENABLED.get_or_init(|| std::env::var_os("FISSION_PREVIEW_DIAG").is_some()) {
        let mut address_params: Vec<_> = address_params.iter().cloned().collect();
        let mut scalar_params: Vec<_> = scalar_params.iter().cloned().collect();
        address_params.sort_unstable();
        scalar_params.sort_unstable();
        eprintln!(
            "[DIAG] param_pointer_roles fn={} address={address_params:?} scalar={scalar_params:?}",
            func.name
        );
    }
    let mut changed = false;
    for param in &mut func.params {
        if scalar_params.contains(&param.name) {
            continue;
        }
        let Some(ptr_ty) = candidates.get(&param.name) else {
            continue;
        };
        if param.surface_type_name.is_none() && !matches!(param.ty, NirType::Ptr(_)) {
            param.ty = ptr_ty.clone();
            changed = true;
        }
    }
    changed |= demote_pointer_offset_params(func);
    changed
}

/// Demote formal params that are used as integer offsets in pointer adds.
///
/// Runs as a late cleanup so later type passes cannot leave an offset parameter
/// pointer-typed after register reuse.
fn demote_pointer_offset_params(func: &mut DirFunction) -> bool {
    let params = param_name_set(func);
    if params.is_empty() {
        return false;
    }
    let binding_types = collect_known_binding_types(func);
    let mut defs: HashMap<String, DefEntry> = HashMap::default();
    scan_def_types(&func.body, &mut defs);
    let dependencies = DefinitionDependencyMap::build(&func.body);
    let mut candidates = HashMap::default();
    let candidate_context = ParamPointerCandidateContext {
        defs: &defs,
        dependencies: &dependencies,
        binding_types: &binding_types,
        params: &params,
    };
    collect_param_pointer_candidates_stmts(&func.body, &candidate_context, &mut candidates);
    let address_params: HashSet<String> = candidates.keys().cloned().collect();
    let mut strong_scalar_roots = StrongScalarParamRoots::default();
    collect_strong_scalar_param_roots_stmts(
        &func.body,
        &defs,
        &binding_types,
        &params,
        &mut strong_scalar_roots,
    );
    let strong_scalar_params = strong_scalar_roots.all;
    let shift_scalar_params = strong_scalar_roots.shifts;
    let role_context = ParamPointerRoleContext {
        defs: &defs,
        binding_types: &binding_types,
        params: &params,
        address_params: &address_params,
        strong_scalar_params: &strong_scalar_params,
    };
    let mut offset_params = HashSet::default();
    collect_param_pointer_offset_params_stmts(&func.body, &role_context, &mut offset_params);
    let mut scalar_params = strong_scalar_params.clone();
    scalar_params.extend(offset_params);
    for address_param in &address_params {
        if !shift_scalar_params.contains(address_param) {
            scalar_params.remove(address_param);
        }
    }
    if scalar_params.is_empty() {
        return false;
    }
    let scalar_bits = if func.is_64bit { 64 } else { 32 };
    let mut changed = false;
    for param in &mut func.params {
        if !scalar_params.contains(&param.name) {
            continue;
        }
        if param.surface_type_name.is_none() && matches!(param.ty, NirType::Ptr(_)) {
            param.ty = NirType::Int {
                bits: scalar_bits,
                signed: false,
            };
            changed = true;
        }
    }
    changed
}

fn collect_word_load_pointer_names(expr: &DirExpr, out: &mut HashMap<String, u32>) {
    match expr {
        DirExpr::Load {
            ptr,
            ty: NirType::Int {
                bits,
                signed: false,
            },
        } if *bits > 8 => {
            let mut names = HashSet::default();
            collect_expr_vars(ptr, &mut names);
            for name in names {
                out.entry(name).or_insert(*bits);
            }
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            collect_word_load_pointer_names(expr, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_word_load_pointer_names(lhs, out);
            collect_word_load_pointer_names(rhs, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_word_load_pointer_names(cond, out);
            collect_word_load_pointer_names(then_expr, out);
            collect_word_load_pointer_names(else_expr, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_word_load_pointer_names(arg, out);
            }
        }
        DirExpr::Load { ptr, .. }
        | DirExpr::PtrOffset { base: ptr, .. }
        | DirExpr::FieldAccess { base: ptr, .. }
        | DirExpr::AggregateCopy { src: ptr, .. } => {
            collect_word_load_pointer_names(ptr, out);
        }
        DirExpr::Index { base, index, .. } => {
            collect_word_load_pointer_names(base, out);
            collect_word_load_pointer_names(index, out);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn collect_signed_neutral_load_contexts_stmts(
    stmts: &[DirStmt],
    candidates: &mut HashMap<String, u32>,
    blockers: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { rhs, .. } | DirStmt::Expr(rhs) | DirStmt::Return(Some(rhs)) => {
                collect_signed_neutral_load_contexts_expr(rhs, candidates, blockers);
            }
            DirStmt::Block(body) | DirStmt::While { body, .. } => {
                collect_signed_neutral_load_contexts_stmts(body, candidates, blockers);
            }
            DirStmt::DoWhile { body, cond } => {
                collect_signed_neutral_load_contexts_stmts(body, candidates, blockers);
                collect_signed_neutral_load_contexts_expr(cond, candidates, blockers);
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_signed_neutral_load_contexts_expr(cond, candidates, blockers);
                collect_signed_neutral_load_contexts_stmts(then_body, candidates, blockers);
                collect_signed_neutral_load_contexts_stmts(else_body, candidates, blockers);
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_signed_neutral_load_contexts_stmts(
                        std::slice::from_ref(init),
                        candidates,
                        blockers,
                    );
                }
                if let Some(cond) = cond {
                    collect_signed_neutral_load_contexts_expr(cond, candidates, blockers);
                }
                if let Some(update) = update {
                    collect_signed_neutral_load_contexts_stmts(
                        std::slice::from_ref(update),
                        candidates,
                        blockers,
                    );
                }
                collect_signed_neutral_load_contexts_stmts(body, candidates, blockers);
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_signed_neutral_load_contexts_expr(expr, candidates, blockers);
                for case in cases {
                    collect_signed_neutral_load_contexts_stmts(&case.body, candidates, blockers);
                }
                collect_signed_neutral_load_contexts_stmts(default, candidates, blockers);
            }
            DirStmt::VaStart { va_list, .. } => {
                collect_signed_neutral_load_contexts_expr(va_list, candidates, blockers);
            }
            DirStmt::Return(None)
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn collect_signed_neutral_load_contexts_expr(
    expr: &DirExpr,
    candidates: &mut HashMap<String, u32>,
    blockers: &mut HashSet<String>,
) {
    match expr {
        DirExpr::Binary { op, lhs, rhs, ty } => {
            if matches!(
                (op, ty),
                (
                    DirBinaryOp::Add | DirBinaryOp::Sub | DirBinaryOp::Mul,
                    NirType::Int { signed: true, .. }
                )
            ) {
                collect_word_load_pointer_names(lhs, candidates);
                collect_word_load_pointer_names(rhs, candidates);
            }
            if matches!(
                op,
                DirBinaryOp::Div
                    | DirBinaryOp::Mod
                    | DirBinaryOp::And
                    | DirBinaryOp::Or
                    | DirBinaryOp::Xor
                    | DirBinaryOp::Shr
                    | DirBinaryOp::Lt
                    | DirBinaryOp::Le
                    | DirBinaryOp::Gt
                    | DirBinaryOp::Ge
            ) {
                let mut unsigned_loads = HashMap::default();
                collect_word_load_pointer_names(lhs, &mut unsigned_loads);
                collect_word_load_pointer_names(rhs, &mut unsigned_loads);
                blockers.extend(unsigned_loads.into_keys());
            }
            collect_signed_neutral_load_contexts_expr(lhs, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(rhs, candidates, blockers);
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            collect_signed_neutral_load_contexts_expr(expr, candidates, blockers);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_signed_neutral_load_contexts_expr(cond, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(then_expr, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(else_expr, candidates, blockers);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_signed_neutral_load_contexts_expr(arg, candidates, blockers);
            }
        }
        DirExpr::Load { ptr, .. }
        | DirExpr::PtrOffset { base: ptr, .. }
        | DirExpr::FieldAccess { base: ptr, .. }
        | DirExpr::AggregateCopy { src: ptr, .. } => {
            collect_signed_neutral_load_contexts_expr(ptr, candidates, blockers);
        }
        DirExpr::Index { base, index, .. } => {
            collect_signed_neutral_load_contexts_expr(base, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(index, candidates, blockers);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
}

fn promote_signed_neutral_word_load_pointees(
    func: &mut DirFunction,
    dependencies: &DefinitionDependencyMap,
) -> bool {
    let mut candidates = HashMap::default();
    let mut blockers = HashSet::default();
    collect_signed_neutral_load_contexts_stmts(&func.body, &mut candidates, &mut blockers);
    if candidates.is_empty() {
        return false;
    }

    let params: HashSet<String> = func.params.iter().map(|param| param.name.clone()).collect();
    let mut promoted = HashMap::default();
    for (name, bits) in candidates {
        if blockers.contains(&name) {
            continue;
        }
        promoted.entry(name.clone()).or_insert(bits);
        for path_name in dependencies.nodes_reaching_roots(&name, &params) {
            if !blockers.contains(&path_name) {
                promoted.entry(path_name).or_insert(bits);
            }
        }
    }

    let mut changed = false;
    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        let Some(bits) = promoted.get(&binding.name) else {
            continue;
        };
        if binding.surface_type_name.is_none()
            && matches!(
                binding.ty,
                NirType::Ptr(ref pointee)
                    if matches!(
                        pointee.as_ref(),
                        NirType::Int {
                            bits: pointee_bits,
                            signed: false,
                        } if pointee_bits == bits
                    )
            )
        {
            binding.ty = NirType::Ptr(Box::new(NirType::Int {
                bits: *bits,
                signed: true,
            }));
            changed = true;
        }
    }
    changed
}

fn mark_address_use(
    expr: &DirExpr,
    pointee_ty: Option<&NirType>,
    roles: &mut HashMap<String, BindingUseRole>,
) {
    fn mark_root(
        expr: &DirExpr,
        pointee_ty: Option<&NirType>,
        roles: &mut HashMap<String, BindingUseRole>,
    ) {
        match expr {
            DirExpr::Var(name) => {
                let role = roles.entry(name.clone()).or_default();
                role.address_use = true;
                if let Some(ty) = pointee_ty
                    && *ty != NirType::Unknown
                {
                    role.address_pointee_type.get_or_insert_with(|| ty.clone());
                }
            }
            DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
                mark_root(expr, pointee_ty, roles);
            }
            DirExpr::PtrOffset { base, .. }
            | DirExpr::FieldAccess { base, .. }
            | DirExpr::AggregateCopy { src: base, .. } => {
                mark_root(base, pointee_ty, roles);
            }
            DirExpr::Index { base, .. } => mark_root(base, pointee_ty, roles),
            DirExpr::Binary { .. }
            | DirExpr::Select { .. }
            | DirExpr::Call { .. }
            | DirExpr::Load { .. }
            | DirExpr::Const(_, _)
            | DirExpr::AddressOfGlobal(_) => {}
        }
    }

    mark_root(expr, pointee_ty, roles);
    collect_binding_use_roles_expr(expr, roles);
}

fn mark_strong_scalar_use(expr: &DirExpr, roles: &mut HashMap<String, BindingUseRole>) {
    if let DirExpr::Var(name) = expr {
        roles.entry(name.clone()).or_default().strong_scalar_use = true;
    }
    collect_binding_use_roles_expr(expr, roles);
}

fn scalar_role_op(op: DirBinaryOp) -> bool {
    matches!(
        op,
        DirBinaryOp::Mod
            | DirBinaryOp::And
            | DirBinaryOp::Or
            | DirBinaryOp::Xor
            | DirBinaryOp::Shl
            | DirBinaryOp::Shr
            | DirBinaryOp::Sar
    )
}

fn collect_binding_use_roles_stmts(stmts: &[DirStmt], roles: &mut HashMap<String, BindingUseRole>) {
    for stmt in stmts {
        collect_binding_use_roles_stmt(stmt, roles);
    }
}

fn collect_binding_use_roles_stmt(stmt: &DirStmt, roles: &mut HashMap<String, BindingUseRole>) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            collect_binding_use_roles_lvalue(lhs, roles);
            collect_binding_use_roles_expr(rhs, roles);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            collect_binding_use_roles_expr(expr, roles);
        }
        DirStmt::VaStart { va_list, .. } => collect_binding_use_roles_expr(va_list, roles),
        DirStmt::Block(stmts) => collect_binding_use_roles_stmts(stmts, roles),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_binding_use_roles_expr(cond, roles);
            collect_binding_use_roles_stmts(then_body, roles);
            collect_binding_use_roles_stmts(else_body, roles);
        }
        DirStmt::While { cond, body } => {
            collect_binding_use_roles_expr(cond, roles);
            collect_binding_use_roles_stmts(body, roles);
        }
        DirStmt::DoWhile { body, cond } => {
            collect_binding_use_roles_stmts(body, roles);
            collect_binding_use_roles_expr(cond, roles);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_binding_use_roles_stmt(init, roles);
            }
            if let Some(cond) = cond {
                collect_binding_use_roles_expr(cond, roles);
            }
            if let Some(update) = update {
                collect_binding_use_roles_stmt(update, roles);
            }
            collect_binding_use_roles_stmts(body, roles);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_binding_use_roles_expr(expr, roles);
            for case in cases {
                collect_binding_use_roles_stmts(&case.body, roles);
            }
            collect_binding_use_roles_stmts(default, roles);
        }
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn collect_binding_use_roles_lvalue(lhs: &DirLValue, roles: &mut HashMap<String, BindingUseRole>) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, ty } => mark_address_use(ptr, Some(ty), roles),
        DirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            mark_address_use(base, Some(elem_ty), roles);
            collect_binding_use_roles_expr(index, roles);
        }
        DirLValue::FieldAccess { base, .. } => mark_address_use(base, None, roles),
    }
}

fn collect_binding_use_roles_expr(expr: &DirExpr, roles: &mut HashMap<String, BindingUseRole>) {
    match expr {
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            collect_binding_use_roles_expr(expr, roles);
        }
        DirExpr::Binary { op, lhs, rhs, .. } => {
            if scalar_role_op(*op) {
                mark_strong_scalar_use(lhs, roles);
                mark_strong_scalar_use(rhs, roles);
            } else {
                collect_binding_use_roles_expr(lhs, roles);
                collect_binding_use_roles_expr(rhs, roles);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_binding_use_roles_expr(cond, roles);
            collect_binding_use_roles_expr(then_expr, roles);
            collect_binding_use_roles_expr(else_expr, roles);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_binding_use_roles_expr(arg, roles);
            }
        }
        DirExpr::Load { ptr, ty } => mark_address_use(ptr, Some(ty), roles),
        DirExpr::PtrOffset { base, .. } => mark_address_use(base, None, roles),
        DirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            mark_address_use(base, Some(elem_ty), roles);
            collect_binding_use_roles_expr(index, roles);
        }
        DirExpr::FieldAccess { base, .. } => mark_address_use(base, None, roles),
        DirExpr::AggregateCopy { src, .. } => mark_address_use(src, None, roles),
    }
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
    body: &[DirStmt],
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
    stmts: &[DirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) {
    for stmt in stmts {
        collect_return_types_stmt(stmt, defs, known_binding_types, out);
    }
}

fn collect_return_types_stmt(
    stmt: &DirStmt,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) {
    match stmt {
        DirStmt::Return(Some(expr)) => {
            let ty = match expr {
                DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
                    let mut visited = HashSet::default();
                    infer_type_for_binding(name, defs, known_binding_types, &mut visited)
                }
                other => expr_type(other),
            };
            if ty != NirType::Unknown {
                out.push(ty);
            }
        }
        DirStmt::Block(stmts) => collect_return_types(stmts, defs, known_binding_types, out),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_return_types(then_body, defs, known_binding_types, out);
            collect_return_types(else_body, defs, known_binding_types, out);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            collect_return_types(body, defs, known_binding_types, out);
        }
        DirStmt::For { body, .. } => collect_return_types(body, defs, known_binding_types, out),
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_return_types(&case.body, defs, known_binding_types, out);
            }
            collect_return_types(default, defs, known_binding_types, out);
        }
        _ => {}
    }
}

fn infer_return_type_from_body(
    stmts: &[DirStmt],
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
    stmt: &DirStmt,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    let mut out = Vec::new();
    collect_return_types_stmt(stmt, defs, known_binding_types, &mut out);
    out.into_iter().find(|t| *t != NirType::Unknown)
}

fn infer_return_type_stmts(
    stmts: &[DirStmt],
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
    expr: &DirExpr,
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        DirExpr::Cast { ty, expr: inner } => {
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
                DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
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
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            // Prefer multi-assign aggregation when available via defs map alone first;
            // full body scan is applied in `collect_zero_extended_return_candidates_stmt`.
            let ty =
                zero_extended_return_candidate_type_for_binding(name, defs, known_binding_types)?;
            match ty {
                NirType::Int { bits, .. } if bits < 64 => Some(ty),
                _ => None,
            }
        }
        DirExpr::Select {
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
            if let DirExpr::Const(value, NirType::Int { bits: 64, .. }) = other {
                let v = *value as u64;
                if v <= 0xFFFF_FFFF {
                    let signed = v >= 0x8000_0000;
                    return Some(NirType::Int { bits: 32, signed });
                }
            }
            // Also accept unsigned-32 Const typed as u32 (printer still shows large decimals).
            if let DirExpr::Const(
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
    stmts: &[DirStmt],
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
    stmts: &[DirStmt],
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
            DirExpr::Var(src) | DirExpr::AddressOfGlobal(src) => {
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

fn i32_compatible_const_leaf_type(expr: &DirExpr) -> Option<NirType> {
    match expr {
        DirExpr::Const(value, NirType::Int { bits: 32 | 64, .. }) => {
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
        DirExpr::Cast { expr, .. } => i32_compatible_const_leaf_type(expr),
        _ => None,
    }
}

fn rhs_has_i32_sign_bit_evidence(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Const(value, NirType::Int { bits: 32 | 64, .. }) => {
            let v = *value as u64;
            v <= 0xFFFF_FFFF && v >= 0x8000_0000
        }
        // `neg` of setnz (signum ≤0 path) yields -1 / 0 in full EAX — signed i32.
        DirExpr::Unary {
            op: DirUnaryOp::Neg,
            ..
        } => true,
        DirExpr::Select {
            then_expr,
            else_expr,
            ..
        } => rhs_has_i32_sign_bit_evidence(then_expr) || rhs_has_i32_sign_bit_evidence(else_expr),
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            rhs_has_i32_sign_bit_evidence(expr)
        }
        _ => false,
    }
}

fn collect_var_assign_rhs<'a>(stmts: &'a [DirStmt], name: &str, out: &mut Vec<&'a DirExpr>) {
    for stmt in stmts {
        collect_var_assign_rhs_stmt(stmt, name, out);
    }
}

fn collect_var_assign_rhs_stmt<'a>(stmt: &'a DirStmt, name: &str, out: &mut Vec<&'a DirExpr>) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(n),
            rhs,
        } if n == name => out.push(rhs),
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            collect_var_assign_rhs(body, name, out)
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_var_assign_rhs(then_body, name, out);
            collect_var_assign_rhs(else_body, name, out);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_var_assign_rhs(&case.body, name, out);
            }
            collect_var_assign_rhs(default, name, out);
        }
        DirStmt::For {
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
    stmts: &[DirStmt],
    root_body: &[DirStmt],
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
    stmt: &DirStmt,
    root_body: &[DirStmt],
    defs: &HashMap<String, DefEntry>,
    known_binding_types: &HashMap<String, NirType>,
    out: &mut Vec<NirType>,
) -> usize {
    match stmt {
        DirStmt::Return(Some(expr)) => {
            let ty = match expr {
                DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
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
        DirStmt::Return(None) => 0,
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. }
        | DirStmt::For { body: stmts, .. } => collect_zero_extended_return_candidates(
            stmts,
            root_body,
            defs,
            known_binding_types,
            out,
        ),
        DirStmt::If {
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
        DirStmt::Switch { cases, default, .. } => {
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

fn strip_zero_extended_return_casts(stmts: &mut [DirStmt], narrowed_ty: &NirType) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= strip_zero_extended_return_casts_stmt(stmt, narrowed_ty);
    }
    changed
}

fn strip_zero_extended_return_casts_stmt(stmt: &mut DirStmt, narrowed_ty: &NirType) -> bool {
    match stmt {
        // Rewrite 64-bit integer constants to their narrowed 32-bit equivalent.
        DirStmt::Return(Some(DirExpr::Const(value, const_ty))) => {
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
        DirStmt::Return(Some(DirExpr::Cast { ty, expr })) => {
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
                *stmt = DirStmt::Return(Some(inner));
                true
            } else {
                false
            }
        }
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. }
        | DirStmt::For { body: stmts, .. } => strip_zero_extended_return_casts(stmts, narrowed_ty),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            strip_zero_extended_return_casts(then_body, narrowed_ty)
                | strip_zero_extended_return_casts(else_body, narrowed_ty)
        }
        DirStmt::Switch { cases, default, .. } => {
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
    func: &mut DirFunction,
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

fn narrow_returned_temp_bindings(func: &mut DirFunction, narrowed_ty: &NirType) {
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
fn promote_sub32_abi_return_width(
    func: &mut DirFunction,
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
            DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
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
fn promote_narrow_returned_temps_for_abi_return(func: &mut DirFunction) -> bool {
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

fn collect_all_return_exprs<'a>(stmts: &'a [DirStmt], out: &mut Vec<&'a DirExpr>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Return(Some(expr)) => out.push(expr),
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => collect_all_return_exprs(body, out),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_all_return_exprs(then_body, out);
                collect_all_return_exprs(else_body, out);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_all_return_exprs(&case.body, out);
                }
                collect_all_return_exprs(default, out);
            }
            _ => {}
        }
    }
}

fn collect_returned_var_names(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Return(Some(DirExpr::Var(n) | DirExpr::AddressOfGlobal(n))) => {
                out.insert(n.clone());
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => collect_returned_var_names(body, out),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_returned_var_names(then_body, out);
                collect_returned_var_names(else_body, out);
            }
            DirStmt::Switch { cases, default, .. } => {
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
fn rewrite_i32_compatible_constants_in_body(stmts: &mut [DirStmt], narrowed_ty: &NirType) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= rewrite_i32_compatible_constants_in_stmt(stmt, narrowed_ty);
    }
    changed
}

fn rewrite_i32_compatible_constants_in_stmt(stmt: &mut DirStmt, narrowed_ty: &NirType) -> bool {
    match stmt {
        DirStmt::Assign { rhs, .. } => rewrite_i32_compatible_constants_in_expr(rhs, narrowed_ty),
        DirStmt::Return(Some(expr)) => rewrite_i32_compatible_constants_in_expr(expr, narrowed_ty),
        DirStmt::Expr(expr) => rewrite_i32_compatible_constants_in_expr(expr, narrowed_ty),
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            rewrite_i32_compatible_constants_in_body(body, narrowed_ty)
        }
        DirStmt::If {
            then_body,
            else_body,
            cond,
            ..
        } => {
            rewrite_i32_compatible_constants_in_expr(cond, narrowed_ty)
                | rewrite_i32_compatible_constants_in_body(then_body, narrowed_ty)
                | rewrite_i32_compatible_constants_in_body(else_body, narrowed_ty)
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
            ..
        } => {
            let mut changed = rewrite_i32_compatible_constants_in_expr(expr, narrowed_ty);
            for case in cases {
                changed |= rewrite_i32_compatible_constants_in_body(&mut case.body, narrowed_ty);
            }
            changed | rewrite_i32_compatible_constants_in_body(default, narrowed_ty)
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = false;
            if let Some(i) = init {
                changed |= rewrite_i32_compatible_constants_in_stmt(i, narrowed_ty);
            }
            if let Some(u) = update {
                changed |= rewrite_i32_compatible_constants_in_stmt(u, narrowed_ty);
            }
            changed | rewrite_i32_compatible_constants_in_body(body, narrowed_ty)
        }
        _ => false,
    }
}

fn rewrite_i32_compatible_constants_in_expr(expr: &mut DirExpr, narrowed_ty: &NirType) -> bool {
    let NirType::Int {
        bits: 32,
        signed: narrow_signed,
    } = narrowed_ty
    else {
        return false;
    };
    match expr {
        DirExpr::Const(value, ty) => {
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
        DirExpr::Select {
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
        DirExpr::Cast { expr: inner, ty } => {
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
        DirExpr::Unary { expr: inner, .. } => {
            rewrite_i32_compatible_constants_in_expr(inner, narrowed_ty)
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            rewrite_i32_compatible_constants_in_expr(lhs, narrowed_ty)
                | rewrite_i32_compatible_constants_in_expr(rhs, narrowed_ty)
        }
        _ => false,
    }
}

fn strip_zero_extended_casts_to_declared_return_width(func: &mut DirFunction) -> bool {
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
/// - Updates `DirBinding.ty` for all `locals` and `params` that have
///   `ty == Unknown` and no `surface_type_name` override.
/// - Re-derives `DirFunction.return_type` when it is `Unknown`.
///
/// Returns `true` when at least one binding/return type was strengthened.
pub fn apply_type_inference_pass(func: &mut DirFunction) -> bool {
    // Build the owned def map (no lifetime ties to func).
    let mut defs: HashMap<String, DefEntry> = HashMap::default();
    scan_def_types(&func.body, &mut defs);
    let dependencies = DefinitionDependencyMap::build(&func.body);
    let mut known_binding_types = collect_known_binding_types(func);
    let mut changed = false;

    // Infer types for locals whose ty is Unknown.
    for binding in func.locals.iter_mut() {
        if binding.ty != NirType::Unknown || binding.surface_type_name.is_some() {
            continue;
        }
        let mut visited = HashSet::default();
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
        let mut visited = HashSet::default();
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
    changed |= promote_sub32_abi_return_width(func, &defs, &known_binding_types);
    changed |= promote_narrow_returned_temps_for_abi_return(func);
    changed |= strip_zero_extended_casts_to_declared_return_width(func);
    changed |= apply_scalar_role_override_for_pointer_locals(func);
    changed |= apply_address_role_pointer_override_for_locals(func);
    changed |= apply_pointer_compare_peer_override_for_locals(func);
    changed |= rewrite_scalar_zero_alias_assignments(func);
    let address_binding_types = collect_known_binding_types(func);
    changed |= apply_address_contributor_param_pointer_types(
        func,
        &defs,
        &dependencies,
        &address_binding_types,
    );
    changed |= apply_transitive_address_pointer_override_for_locals(func);
    changed |= promote_signed_neutral_word_load_pointees(func, &dependencies);

    changed
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    fn make_assign(name: &str, rhs: DirExpr) -> DirStmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name.to_owned()),
            rhs,
        }
    }

    fn make_binding(name: &str) -> DirBinding {
        DirBinding {
            name: name.to_owned(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_param(name: &str, ty: NirType) -> DirBinding {
        DirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<DirBinding>, body: Vec<DirStmt>, return_type: NirType) -> DirFunction {
        DirFunction {
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
            DirExpr::Const(
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
            DirExpr::Const(
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
            make_assign("x", DirExpr::Const(1, NirType::Bool)),
            make_assign("y", DirExpr::Var("x".to_owned())),
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
            make_assign("a", DirExpr::Var("b".to_owned())),
            make_assign("b", DirExpr::Var("a".to_owned())),
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
                DirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            ),
            DirStmt::Return(Some(DirExpr::Var("x".to_owned()))),
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
        let body = vec![DirStmt::Return(Some(DirExpr::Const(
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
            DirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(DirExpr::Var("y".to_owned())),
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
            DirExpr::Const(
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
    fn scalar_role_demotes_pointer_local_without_address_use() {
        let mut local = make_binding("acc");
        local.ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let body = vec![make_assign(
            "acc",
            DirExpr::Binary {
                op: DirBinaryOp::Mod,
                lhs: Box::new(DirExpr::Var("acc".to_owned())),
                rhs: Box::new(DirExpr::Const(
                    256,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        )];
        let mut func = make_func(vec![local], body, NirType::Unknown);

        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(
            func.locals[0].ty,
            NirType::Int {
                bits: 64,
                signed: false
            }
        );
    }

    #[test]
    fn scalar_role_keeps_pointer_local_with_address_use() {
        let mut local = make_binding("ptr");
        local.ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let body = vec![make_assign(
            "tmp",
            DirExpr::Load {
                ptr: Box::new(DirExpr::Var("ptr".to_owned())),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            },
        )];
        let mut func = make_func(vec![local, make_binding("tmp")], body, NirType::Unknown);

        assert!(super::apply_type_inference_pass(&mut func));
        assert!(matches!(func.locals[0].ty, NirType::Ptr(_)));
    }

    #[test]
    fn scalar_zero_alias_assignment_rewrites_pointer_zero_to_scalar_zero() {
        let mut zero = make_binding("rax");
        zero.ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        zero.initializer = Some(DirExpr::Const(0, zero.ty.clone()));
        let mut scalar = make_binding("acc");
        scalar.ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = make_func(
            vec![zero, scalar],
            vec![make_assign("acc", DirExpr::Var("rax".to_string()))],
            NirType::Unknown,
        );

        assert!(super::apply_type_inference_pass(&mut func));
        assert!(matches!(
            &func.body[0],
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs: DirExpr::Const(0, NirType::Int { bits: 64, signed: false }),
            } if name == "acc"
        ));
    }

    #[test]
    fn scalar_zero_alias_assignment_keeps_pointer_destination() {
        let mut zero = make_binding("rax");
        zero.ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        zero.initializer = Some(DirExpr::Const(0, zero.ty.clone()));
        let mut ptr = make_binding("ptr");
        ptr.ty = zero.ty.clone();
        let mut func = make_func(
            vec![zero, ptr],
            vec![make_assign("ptr", DirExpr::Var("rax".to_string()))],
            NirType::Unknown,
        );

        assert!(!super::rewrite_scalar_zero_alias_assignments(&mut func));
        assert!(matches!(
            &func.body[0],
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs: DirExpr::Var(src),
            } if name == "ptr" && src == "rax"
        ));
    }

    #[test]
    fn pointer_add_offset_param_stays_integer_not_pointer() {
        // An offset parameter must remain integer even when the sum result is
        // pointer-typed.
        let ptr_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut buf = make_binding("buf");
        buf.ty = ptr_ty.clone();
        let mut end = make_binding("end");
        end.ty = ptr_ty.clone();
        let mut len = make_binding("len");
        len.ty = u32_ty.clone();
        let body = vec![
            make_assign("buf", DirExpr::Var("param_1".to_string())),
            make_assign("len", DirExpr::Var("param_2".to_string())),
            make_assign(
                "end",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("buf".to_string())),
                    rhs: Box::new(DirExpr::Var("len".to_string())),
                    ty: ptr_ty.clone(),
                },
            ),
            make_assign(
                "byte",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("buf".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            ),
        ];
        let mut func = make_func(
            vec![buf, end, len, make_binding("byte")],
            body,
            NirType::Unknown,
        );
        func.params = vec![
            make_param("param_1", u32_ty.clone()),
            make_param("param_2", u32_ty.clone()),
        ];

        let _ = super::apply_type_inference_pass(&mut func);
        // buf param may become pointer via load of buf alias.
        // len must not be promoted to pointer via the Add.
        assert!(
            !matches!(func.params[1].ty, NirType::Ptr(_)),
            "len/param_2 must stay integer, got {:?}",
            func.params[1].ty
        );
    }

    #[test]
    fn load_through_param_alias_promotes_param_to_pointer() {
        let ptr_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![
            make_assign("p", DirExpr::Var("param_1".to_string())),
            make_assign(
                "byte",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("p".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            ),
        ];
        let mut func = make_func(
            vec![make_binding("p"), make_binding("byte")],
            body,
            NirType::Unknown,
        );
        func.params = vec![make_param("param_1", u32_ty)];

        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.params[0].ty, ptr_ty);
    }

    #[test]
    fn casted_cursor_load_keeps_parameter_pointer_despite_end_pointer_add() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let body = vec![
            make_assign("cursor_word", DirExpr::Var("buffer_param".to_string())),
            make_assign(
                "end_word",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("buffer_param".to_string())),
                    rhs: Box::new(DirExpr::Var("length_param".to_string())),
                    ty: u32_ty.clone(),
                },
            ),
            make_assign(
                "cursor",
                DirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(DirExpr::Var("cursor_word".to_string())),
                },
            ),
            make_assign(
                "byte",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("cursor".to_string())),
                    ty: u8_ty,
                },
            ),
        ];
        let mut cursor = make_binding("cursor");
        cursor.ty = ptr_ty.clone();
        let mut func = make_func(
            vec![
                make_binding("cursor_word"),
                make_binding("end_word"),
                cursor,
                make_binding("byte"),
            ],
            body,
            NirType::Unknown,
        );
        func.params = vec![
            make_param("buffer_param", u32_ty.clone()),
            make_param("length_param", u32_ty.clone()),
        ];

        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.params[0].ty, ptr_ty);
        assert_eq!(func.params[1].ty, u32_ty);
    }

    #[test]
    fn pointer_word_roundtrip_preserves_cursor_and_end_sentinel_roles() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let body = vec![
            make_assign(
                "end_word",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("buffer".to_string())),
                    rhs: Box::new(DirExpr::Var("length".to_string())),
                    ty: u32_ty.clone(),
                },
            ),
            make_assign("cursor", DirExpr::Var("buffer".to_string())),
            make_assign(
                "cursor_word",
                DirExpr::Cast {
                    ty: u32_ty.clone(),
                    expr: Box::new(DirExpr::Var("cursor".to_string())),
                },
            ),
            make_assign(
                "cursor_word",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("cursor_word".to_string())),
                    rhs: Box::new(DirExpr::Const(1, u32_ty.clone())),
                    ty: u32_ty.clone(),
                },
            ),
            make_assign(
                "cursor",
                DirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(DirExpr::Var("cursor_word".to_string())),
                },
            ),
            DirStmt::Assign {
                lhs: DirLValue::Deref {
                    ptr: Box::new(DirExpr::Var("cursor".to_string())),
                    ty: u8_ty,
                },
                rhs: DirExpr::Const(0, u32_ty.clone()),
            },
            DirStmt::DoWhile {
                body: Vec::new(),
                cond: DirExpr::Binary {
                    op: DirBinaryOp::Ne,
                    lhs: Box::new(DirExpr::Var("end_word".to_string())),
                    rhs: Box::new(DirExpr::Var("cursor_word".to_string())),
                    ty: NirType::Bool,
                },
            },
        ];
        let mut cursor = make_binding("cursor");
        cursor.ty = ptr_ty.clone();
        let mut cursor_word = make_binding("cursor_word");
        cursor_word.ty = u32_ty.clone();
        let mut end_word = make_binding("end_word");
        end_word.ty = u32_ty.clone();
        let mut func = make_func(vec![cursor, cursor_word, end_word], body, NirType::Unknown);
        func.params = vec![
            make_param("buffer", ptr_ty.clone()),
            make_param("length", u32_ty),
        ];

        for _ in 0..3 {
            super::apply_type_inference_pass(&mut func);
        }

        assert_eq!(func.locals[0].ty, ptr_ty);
        assert!(matches!(func.locals[1].ty, NirType::Ptr(_)));
        assert!(matches!(func.locals[2].ty, NirType::Ptr(_)));
    }

    #[test]
    fn reused_load_and_cursor_binding_keeps_definition_scoped_address_root() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let body = vec![
            make_assign(
                "shared",
                DirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(DirExpr::Load {
                        ptr: Box::new(DirExpr::Var("state_param".to_string())),
                        ty: u8_ty.clone(),
                    }),
                },
            ),
            make_assign(
                "shared",
                DirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(DirExpr::Var("buffer_param".to_string())),
                },
            ),
            make_assign(
                "byte",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("shared".to_string())),
                    ty: u8_ty,
                },
            ),
        ];
        let mut shared = make_binding("shared");
        shared.ty = ptr_ty.clone();
        let mut func = make_func(vec![shared, make_binding("byte")], body, NirType::Unknown);
        func.params = vec![
            make_param("state_param", u32_ty.clone()),
            make_param("buffer_param", u32_ty),
        ];

        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.params[0].ty, ptr_ty);
        assert!(matches!(func.params[1].ty, NirType::Ptr(_)));
    }

    #[test]
    fn load_after_cursor_redefinition_promotes_base_parameter_only() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let body = vec![
            make_assign("base_alias", DirExpr::Var("base_param".into())),
            make_assign("cursor", DirExpr::Var("index".into())),
            make_assign(
                "cursor",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("cursor".into())),
                    rhs: Box::new(DirExpr::Var("base_alias".into())),
                    ty: u64_ty.clone(),
                },
            ),
            make_assign(
                "value",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("cursor".into())),
                    ty: u8_ty.clone(),
                },
            ),
        ];
        let mut cursor = make_binding("cursor");
        cursor.ty = u64_ty.clone();
        let mut index = make_binding("index");
        index.ty = u64_ty.clone();
        let mut func = make_func(
            vec![
                make_binding("base_alias"),
                cursor,
                index,
                make_binding("value"),
            ],
            body,
            NirType::Unknown,
        );
        func.params = vec![
            make_param("base_param", u64_ty.clone()),
            make_param("limit_param", u64_ty.clone()),
        ];

        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.params[0].ty, NirType::Ptr(Box::new(u8_ty)));
        assert_eq!(func.params[1].ty, u64_ty);
        assert!(matches!(func.locals[0].ty, NirType::Ptr(_)));
        assert!(matches!(func.locals[1].ty, NirType::Ptr(_)));
        assert!(matches!(func.locals[2].ty, NirType::Int { .. }));
    }

    #[test]
    fn scalar_comparison_alias_does_not_promote_param_to_pointer() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let body = vec![
            make_assign("limit", DirExpr::Var("param_1".to_string())),
            DirStmt::If {
                cond: DirExpr::Binary {
                    op: DirBinaryOp::Lt,
                    lhs: Box::new(DirExpr::Var("i".to_string())),
                    rhs: Box::new(DirExpr::Var("limit".to_string())),
                    ty: NirType::Bool,
                },
                then_body: Vec::new(),
                else_body: Vec::new(),
            },
        ];
        let mut limit = make_binding("limit");
        limit.ty = u64_ty.clone();
        let mut idx = make_binding("i");
        idx.ty = u64_ty.clone();
        let mut func = make_func(vec![limit, idx], body, NirType::Unknown);
        func.params = vec![make_param("param_1", u64_ty.clone())];
        let binding_types = super::collect_known_binding_types(&func);
        let dependencies = super::DefinitionDependencyMap::build(&func.body);
        assert!(!super::apply_address_contributor_param_pointer_types(
            &mut func,
            &[(
                "limit".to_string(),
                super::DefEntry::Alias("param_1".to_string())
            )].into_iter().collect::<HashMap<_, _>>(),
            &dependencies,
            &binding_types,
        ));
        assert_eq!(func.params[0].ty, u64_ty);
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
        let mut func = DirFunction {
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
                DirStmt::If {
                    cond: DirExpr::Var("cond".to_owned()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(DirExpr::Var("param_2".to_owned())),
                    }))],
                    else_body: vec![],
                },
                DirStmt::Return(Some(DirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);
        assert!(changed);
        assert_eq!(func.return_type, u32_ty);
        let DirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("expected if");
        };
        assert!(matches!(
            &then_body[0],
            DirStmt::Return(Some(DirExpr::Var(name))) if name == "param_2"
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
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", u32_ty.clone())],
            locals: vec![],
            return_type: u32_ty,
            surface_return_type_name: None,
            body: vec![DirStmt::Return(Some(DirExpr::Cast {
                ty: u64_ty,
                expr: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("param_1".to_owned())),
                    rhs: Box::new(DirExpr::Const(
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
            DirStmt::Return(Some(DirExpr::Binary {
                op: DirBinaryOp::Add,
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
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", u32_ty)],
            locals: vec![],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                DirStmt::Return(Some(DirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(DirExpr::Var("param_1".to_owned())),
                })),
                DirStmt::Return(Some(DirExpr::Var("unknown_wide".to_owned()))),
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
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", i32_ty.clone())],
            locals: vec![DirBinding {
                name: "tmp".to_owned(),
                ty: u32_ty,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            return_type: u64_ty,
            surface_return_type_name: None,
            body: vec![
                DirStmt::If {
                    cond: DirExpr::Var("cond".to_owned()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Var("param_1".to_owned())))],
                    else_body: vec![],
                },
                DirStmt::Return(Some(DirExpr::Var("tmp".to_owned()))),
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
        let local = |name: &str, ty: NirType| DirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        let mut func = DirFunction {
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
                make_assign("rdi", DirExpr::Var("param_1".to_owned())),
                DirStmt::If {
                    cond: DirExpr::Var("cond".to_owned()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Var("rdi".to_owned())))],
                    else_body: vec![],
                },
                make_assign("ret32", DirExpr::Var("wide_acc".to_owned())),
                make_assign("ret64", DirExpr::Var("ret32".to_owned())),
                DirStmt::Return(Some(DirExpr::Var("ret64".to_owned()))),
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
        let local = |name: &str, ty: NirType| DirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        let mut func = DirFunction {
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
                make_assign("rdi", DirExpr::Var("param_1".to_owned())),
                DirStmt::If {
                    cond: DirExpr::Var("cond".to_owned()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Var("rdi".to_owned())))],
                    else_body: vec![],
                },
                make_assign("ret32", DirExpr::Var("wide_acc".to_owned())),
                make_assign("ret64", DirExpr::Var("ret32".to_owned())),
                DirStmt::Return(Some(DirExpr::Var("ret64".to_owned()))),
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
        let mut func = DirFunction {
            name: "validate_input".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![
                make_param(
                    "param_1",
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
                make_param(
                    "param_2",
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            ],
            locals: vec![],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                DirStmt::If {
                    cond: DirExpr::Var("c1".to_owned()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Const(
                        4294967295, // 0xFFFFFFFF = -1 as u32
                        u64_ty.clone(),
                    )))],
                    else_body: vec![],
                },
                DirStmt::If {
                    cond: DirExpr::Var("c2".to_owned()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Const(
                        4294967294, // 0xFFFFFFFE = -2 as u32
                        u64_ty.clone(),
                    )))],
                    else_body: vec![],
                },
                // Simulate: return (ulonglong)(uint)(int)(param_1 + param_2)
                // The outer u64 ZExt cast is what the decompiler produces for x86-64.
                DirStmt::Return(Some(DirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(DirExpr::Cast {
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                        expr: Box::new(DirExpr::Binary {
                            op: DirBinaryOp::Add,
                            lhs: Box::new(DirExpr::Var("param_1".to_owned())),
                            rhs: Box::new(DirExpr::Var("param_2".to_owned())),
                            ty: NirType::Int {
                                bits: 32,
                                signed: true,
                            },
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
        let DirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("expected if statement");
        };
        let DirStmt::Return(Some(DirExpr::Const(v, ty))) = &then_body[0] else {
            panic!("expected return const");
        };
        assert_eq!(*v, -1i64, "0xFFFFFFFF should become -1");
        assert_eq!(*ty, i32_ty);
    }

    /// signum-style: single `return xVar` after `xVar = cond ? 1 : (cond2 ? 0 : 0xffffffff)`.
    /// signum O2: setnz→neg returns -1/0; must not declare `uchar` return or
    /// recompilation truncates `-1` to `255`.
    #[test]
    fn promotes_uchar_return_after_setnz_neg_to_signed_i32() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = DirFunction {
            name: "signum_setnz_neg".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param(
                "param_1",
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )],
            locals: vec![DirBinding {
                name: "uVar2".to_owned(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            // Wrong narrow from setcc lane.
            return_type: u8_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                make_assign(
                    "uVar2",
                    DirExpr::Unary {
                        op: DirUnaryOp::Not,
                        expr: Box::new(DirExpr::Var("zf".into())),
                        ty: u8_ty.clone(),
                    },
                ),
                make_assign(
                    "uVar2",
                    DirExpr::Unary {
                        op: DirUnaryOp::Neg,
                        expr: Box::new(DirExpr::Var("uVar2".into())),
                        ty: i32_ty.clone(),
                    },
                ),
                DirStmt::If {
                    cond: DirExpr::Var("cond".into()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Var("uVar2".into())))],
                    else_body: vec![DirStmt::Return(Some(DirExpr::Const(1, i32_ty.clone())))],
                },
            ],
            ..Default::default()
        };
        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(
            func.return_type, i32_ty,
            "setnz+neg return must stay signed int, not uchar"
        );
        assert_eq!(
            func.locals[0].ty, i32_ty,
            "returned setnz+neg temp must widen with the ABI return"
        );
    }

    /// Return type already i32, but join temp stayed uchar after setnz+neg
    /// (x64 signum O2 after partial-reg compose). Recompilation truncates -1.
    #[test]
    fn promotes_uchar_return_temp_when_return_already_i32() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = DirFunction {
            name: "signum_temp_width".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param(
                "param_1",
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )],
            locals: vec![DirBinding {
                name: "xVar8".to_owned(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            return_type: i32_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                make_assign(
                    "xVar8",
                    DirExpr::Unary {
                        op: DirUnaryOp::Not,
                        expr: Box::new(DirExpr::Var("zf".into())),
                        ty: u8_ty.clone(),
                    },
                ),
                make_assign(
                    "xVar8",
                    DirExpr::Unary {
                        op: DirUnaryOp::Neg,
                        expr: Box::new(DirExpr::Var("xVar8".into())),
                        ty: i32_ty.clone(),
                    },
                ),
                DirStmt::Return(Some(DirExpr::Var("xVar8".into()))),
            ],
            ..Default::default()
        };
        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.return_type, i32_ty);
        assert_eq!(
            func.locals[0].ty, i32_ty,
            "uchar join temp with Neg must widen to match int return"
        );
    }

    #[test]
    fn narrows_select_join_temp_return_to_signed_i32() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = DirFunction {
            name: "signum_like".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param(
                "param_1",
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )],
            locals: vec![make_binding("xVar8")],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                make_assign(
                    "xVar8",
                    DirExpr::Select {
                        cond: Box::new(DirExpr::Var("c1".into())),
                        then_expr: Box::new(DirExpr::Const(1, u64_ty.clone())),
                        else_expr: Box::new(DirExpr::Select {
                            cond: Box::new(DirExpr::Var("c2".into())),
                            then_expr: Box::new(DirExpr::Const(0, u64_ty.clone())),
                            else_expr: Box::new(DirExpr::Const(4294967295, u64_ty.clone())),
                            ty: u64_ty.clone(),
                        }),
                        ty: u64_ty.clone(),
                    },
                ),
                DirStmt::Return(Some(DirExpr::Var("xVar8".into()))),
            ],
            ..Default::default()
        };
        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.return_type, i32_ty);
        let DirStmt::Assign { rhs, .. } = &func.body[0] else {
            panic!("expected assign");
        };
        let printed = format!("{rhs:?}");
        assert!(
            printed.contains("-1")
                || matches!(rhs, DirExpr::Select { else_expr, .. }
                if matches!(else_expr.as_ref(), DirExpr::Select { else_expr: e2, .. }
                    if matches!(e2.as_ref(), DirExpr::Const(-1, _)))),
            "expected -1 const in select arms, got {rhs:?}"
        );
    }

    /// saturating_add-style: multi-assign join temp with INT_MIN bit pattern.
    #[test]
    fn narrows_multi_assign_return_temp_with_int_min() {
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut func = DirFunction {
            name: "saturating_like".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![make_binding("xVar39"), make_binding("local_4")],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                make_assign("local_4", DirExpr::Const(0, u64_ty.clone())),
                DirStmt::If {
                    cond: DirExpr::Var("overflow_pos".into()),
                    then_body: vec![make_assign(
                        "xVar39",
                        DirExpr::Const(2147483647, u64_ty.clone()),
                    )],
                    else_body: vec![],
                },
                DirStmt::If {
                    cond: DirExpr::Var("overflow_neg".into()),
                    then_body: vec![make_assign(
                        "xVar39",
                        DirExpr::Const(2147483648u64 as i64, u64_ty.clone()),
                    )],
                    else_body: vec![],
                },
                make_assign("xVar39", DirExpr::Var("local_4".into())),
                DirStmt::Return(Some(DirExpr::Var("xVar39".into()))),
            ],
            ..Default::default()
        };
        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.return_type, i32_ty);
        // INT_MIN arm rewritten
        let DirStmt::If { then_body, .. } = &func.body[2] else {
            panic!("expected second if");
        };
        let DirStmt::Assign {
            rhs: DirExpr::Const(v, ty),
            ..
        } = &then_body[0]
        else {
            panic!("expected const assign, got {:?}", then_body[0]);
        };
        assert_eq!(*v, i32::MIN as i64);
        assert_eq!(*ty, i32_ty);
    }
    #[test]
    fn demotes_len_param_used_as_pointer_add_offset() {
        let ptr_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        // Start with a mistaken pointer type on the offset parameter.
        let mut buf = make_binding("edx");
        buf.ty = ptr_ty.clone();
        let mut end = make_binding("ecx");
        end.ty = u32_ty.clone();
        let body = vec![
            make_assign("edx", DirExpr::Var("param_1".into())),
            make_assign("ecx", DirExpr::Var("param_2".into())),
            make_assign(
                "ecx",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("param_2".into())),
                    rhs: Box::new(DirExpr::Cast {
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                        expr: Box::new(DirExpr::Var("edx".into())),
                    }),
                    ty: u32_ty.clone(),
                },
            ),
            DirStmt::If {
                cond: DirExpr::Binary {
                    op: DirBinaryOp::Ne,
                    lhs: Box::new(DirExpr::Var("ecx".into())),
                    rhs: Box::new(DirExpr::Var("edx".into())),
                    ty: NirType::Bool,
                },
                then_body: vec![],
                else_body: vec![],
            },
            make_assign(
                "byte",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("edx".into())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
            ),
        ];
        let mut func = make_func(vec![buf, end, make_binding("byte")], body, NirType::Unknown);
        func.is_64bit = false;
        func.params = vec![
            make_param("param_1", ptr_ty.clone()),
            make_param("param_2", ptr_ty.clone()), // mistaken
        ];
        let _ = super::apply_type_inference_pass(&mut func);
        assert!(
            !matches!(func.params[1].ty, NirType::Ptr(_)),
            "param_2/len demoted, got {:?}",
            func.params[1].ty
        );
        assert!(matches!(func.params[0].ty, NirType::Ptr(_)));
    }

    #[test]
    fn promotes_signed_neutral_word_load_pointee_through_pointer_aliases() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let i64_ty = NirType::Int {
            bits: 64,
            signed: true,
        };
        let ptr_ty = NirType::Ptr(Box::new(u32_ty.clone()));
        let mut alias = make_binding("alias");
        alias.ty = ptr_ty.clone();
        let mut cursor = make_binding("cursor");
        cursor.ty = ptr_ty.clone();
        let mut acc = make_binding("acc");
        acc.ty = i64_ty.clone();
        let body = vec![
            make_assign("alias", DirExpr::Var("input".into())),
            make_assign("cursor", DirExpr::Var("alias".into())),
            make_assign(
                "acc",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("acc".into())),
                    rhs: Box::new(DirExpr::Load {
                        ptr: Box::new(DirExpr::Var("cursor".into())),
                        ty: u32_ty,
                    }),
                    ty: i64_ty.clone(),
                },
            ),
        ];
        let mut func = make_func(vec![alias, cursor, acc], body, i64_ty);
        func.params = vec![make_param("input", ptr_ty)];

        assert!(super::apply_type_inference_pass(&mut func));

        for binding in func.params.iter().chain(func.locals.iter().take(2)) {
            assert!(
                matches!(
                    binding.ty,
                    NirType::Ptr(ref pointee)
                        if matches!(
                            pointee.as_ref(),
                            NirType::Int {
                                bits: 32,
                                signed: true,
                            }
                        )
                ),
                "{} should be pointer-to-signed-word, got {:?}",
                binding.name,
                binding.ty
            );
        }
    }

    #[test]
    fn demotes_affine_scalar_param_through_shifted_alias_chain() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u32_ty.clone()));
        let typed_local = |name: &str, ty: NirType| {
            let mut binding = make_binding(name);
            binding.ty = ty;
            binding
        };
        let body = vec![
            make_assign("base_alias", DirExpr::Var("base".into())),
            make_assign("offset", DirExpr::Var("count".into())),
            make_assign(
                "half",
                DirExpr::Binary {
                    op: DirBinaryOp::Shr,
                    lhs: Box::new(DirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(DirExpr::Var("offset".into())),
                    }),
                    rhs: Box::new(DirExpr::Const(1, u64_ty.clone())),
                    ty: u64_ty.clone(),
                },
            ),
            make_assign(
                "offset",
                DirExpr::Binary {
                    op: DirBinaryOp::Sub,
                    lhs: Box::new(DirExpr::Var("offset".into())),
                    rhs: Box::new(DirExpr::Var("index".into())),
                    ty: u64_ty.clone(),
                },
            ),
            make_assign(
                "address",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("base_alias".into())),
                    rhs: Box::new(DirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(DirExpr::Var("offset".into())),
                    }),
                    ty: ptr_ty.clone(),
                },
            ),
            make_assign(
                "value",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("address".into())),
                    ty: u32_ty,
                },
            ),
        ];
        let mut func = make_func(
            vec![
                typed_local("base_alias", ptr_ty.clone()),
                typed_local("offset", ptr_ty.clone()),
                typed_local("address", ptr_ty.clone()),
                typed_local("half", u64_ty.clone()),
                typed_local("index", u64_ty.clone()),
                make_binding("value"),
            ],
            body,
            NirType::Unknown,
        );
        func.is_64bit = true;
        func.params = vec![
            make_param("base", ptr_ty.clone()),
            make_param("count", ptr_ty),
        ];

        assert!(super::apply_type_inference_pass(&mut func));
        assert!(matches!(func.params[0].ty, NirType::Ptr(_)));
        assert_eq!(func.params[1].ty, u64_ty);
    }

    #[test]
    fn keeps_pointer_param_when_reused_alias_later_holds_masked_load_value() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let typed_local = |name: &str, ty: NirType| {
            let mut binding = make_binding(name);
            binding.ty = ty;
            binding
        };
        let body = vec![
            make_assign("reused", DirExpr::Var("input".into())),
            make_assign(
                "address",
                DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Cast {
                        ty: ptr_ty.clone(),
                        expr: Box::new(DirExpr::Var("index".into())),
                    }),
                    rhs: Box::new(DirExpr::Var("reused".into())),
                    ty: ptr_ty.clone(),
                },
            ),
            make_assign(
                "reused",
                DirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(DirExpr::Load {
                        ptr: Box::new(DirExpr::Var("address".into())),
                        ty: u8_ty,
                    }),
                },
            ),
            make_assign(
                "acc",
                DirExpr::Binary {
                    op: DirBinaryOp::Mod,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("acc".into())),
                        rhs: Box::new(DirExpr::Var("reused".into())),
                        ty: u64_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Const(256, u64_ty.clone())),
                    ty: u64_ty.clone(),
                },
            ),
        ];
        let mut func = make_func(
            vec![
                typed_local("reused", ptr_ty.clone()),
                typed_local("address", ptr_ty.clone()),
                typed_local("index", u64_ty.clone()),
                typed_local("acc", u64_ty),
            ],
            body,
            NirType::Unknown,
        );
        func.params = vec![make_param("input", ptr_ty)];

        let _ = super::apply_type_inference_pass(&mut func);
        assert!(matches!(func.params[0].ty, NirType::Ptr(_)));
    }

    #[test]
    fn loaded_scalar_shift_does_not_demote_address_parameter() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let typed_local = |name: &str, ty: NirType| {
            let mut binding = make_binding(name);
            binding.ty = ty;
            binding
        };
        let body = vec![
            make_assign("cursor", DirExpr::Var("input".into())),
            make_assign(
                "loaded",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("cursor".into())),
                    ty: u8_ty,
                },
            ),
            make_assign(
                "shifted",
                DirExpr::Binary {
                    op: DirBinaryOp::Shr,
                    lhs: Box::new(DirExpr::Var("loaded".into())),
                    rhs: Box::new(DirExpr::Const(1, u32_ty.clone())),
                    ty: u32_ty.clone(),
                },
            ),
        ];
        let mut func = make_func(
            vec![
                typed_local("cursor", ptr_ty.clone()),
                typed_local("loaded", u32_ty.clone()),
                typed_local("shifted", u32_ty),
            ],
            body,
            NirType::Unknown,
        );
        func.params = vec![make_param("input", ptr_ty)];

        let _ = super::apply_type_inference_pass(&mut func);
        assert!(matches!(func.params[0].ty, NirType::Ptr(_)));
    }

    #[test]
    fn demotes_affine_param_compared_with_scalar_induction_value() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u32_ty));
        let mut induction = make_binding("induction");
        induction.ty = u64_ty.clone();
        let body = vec![
            DirStmt::If {
                cond: DirExpr::Binary {
                    op: DirBinaryOp::Lt,
                    lhs: Box::new(DirExpr::Var("induction".into())),
                    rhs: Box::new(DirExpr::Var("count".into())),
                    ty: NirType::Bool,
                },
                then_body: vec![],
                else_body: vec![],
            },
            make_assign(
                "value",
                DirExpr::Load {
                    ptr: Box::new(DirExpr::Var("base".into())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            ),
        ];
        let mut func = make_func(
            vec![induction, make_binding("value")],
            body,
            NirType::Unknown,
        );
        func.is_64bit = true;
        func.params = vec![
            make_param("base", ptr_ty.clone()),
            make_param("count", ptr_ty),
        ];

        assert!(super::apply_type_inference_pass(&mut func));
        assert!(matches!(func.params[0].ty, NirType::Ptr(_)));
        assert_eq!(func.params[1].ty, u64_ty);
    }
}
