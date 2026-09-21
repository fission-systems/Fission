//! Pointer/scalar role recovery and address-contributor inference.

use super::*;

#[derive(Default)]
struct BindingUseRole {
    address_use: bool,
    strong_scalar_use: bool,
    address_pointee_type: Option<NirType>,
}

fn scalar_role_type_for_function(func: &PreHirFunction) -> NirType {
    NirType::Int {
        bits: if func.is_64bit { 64 } else { 32 },
        signed: false,
    }
}

pub(super) fn apply_scalar_role_override_for_pointer_locals(func: &mut PreHirFunction) -> bool {
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

pub(super) fn apply_address_role_pointer_override_for_locals(func: &mut PreHirFunction) -> bool {
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

pub(crate) fn transitive_address_pointer_locals(func: &PreHirFunction) -> HashMap<String, NirType> {
    let dependencies = DefinitionDependencyMap::build(&func.body);
    transitive_address_pointer_locals_with_dependencies(func, &dependencies)
}

/// Same as [`transitive_address_pointer_locals`], but takes an
/// already-built [`DefinitionDependencyMap`] instead of rebuilding one from
/// `func.body` -- the dependency graph reflects the HIR's *shape*, not
/// current binding types, so it stays valid across a caller's repeated
/// type-refinement rounds and only needs to be built once per body.
pub(crate) fn transitive_address_pointer_locals_with_dependencies(
    func: &PreHirFunction,
    dependencies: &DefinitionDependencyMap,
) -> HashMap<String, NirType> {
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
    dependencies
        .address_contributors(&func.body, &pointer_roots)
        .into_iter()
        .filter(|(name, _)| local_names.contains(name.as_str()))
        .map(|(name, pointee)| (name, NirType::Ptr(Box::new(pointee))))
        .collect()
}

pub(super) fn apply_transitive_address_pointer_override_for_locals(
    func: &mut PreHirFunction,
    dependencies: &DefinitionDependencyMap,
) -> bool {
    let contributors = transitive_address_pointer_locals_with_dependencies(func, dependencies);
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
pub(super) fn apply_pointer_compare_peer_override_for_locals(func: &mut PreHirFunction) -> bool {
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

pub(crate) fn pointer_compare_peer_promotions(func: &PreHirFunction) -> HashMap<String, NirType> {
    let types = collect_known_binding_types(func);
    let mut promote: HashMap<String, NirType> = HashMap::default();
    collect_pointer_compare_peer_promotions(&func.body, &types, &mut promote);
    promote
}

fn collect_pointer_compare_peer_promotions(
    stmts: &[PreHirStmt],
    types: &HashMap<String, NirType>,
    out: &mut HashMap<String, NirType>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
                collect_pointer_compare_peer_promotions(body, types, out);
            }
            PreHirStmt::DoWhile { body, cond } => {
                collect_pointer_compare_peer_promotions(body, types, out);
                collect_pointer_compare_peer_promotions_expr(cond, types, out);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_pointer_compare_peer_promotions_expr(cond, types, out);
                collect_pointer_compare_peer_promotions(then_body, types, out);
                collect_pointer_compare_peer_promotions(else_body, types, out);
            }
            PreHirStmt::For {
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
            PreHirStmt::Switch {
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
            PreHirStmt::Assign { rhs, .. } => {
                collect_pointer_compare_peer_promotions_expr(rhs, types, out);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_pointer_compare_peer_promotions_expr(expr, types, out);
            }
            _ => {}
        }
    }
}

fn collect_pointer_compare_peer_promotions_expr(
    expr: &PreHirExpr,
    types: &HashMap<String, NirType>,
    out: &mut HashMap<String, NirType>,
) {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq | PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => {
            let lhs_ptr = pointer_type_of_expr(lhs, types);
            let rhs_ptr = pointer_type_of_expr(rhs, types);
            if let (Some(ptr_ty), PreHirExpr::Var(name)) = (lhs_ptr.as_ref(), rhs.as_ref()) {
                out.entry(name.clone()).or_insert_with(|| ptr_ty.clone());
            }
            if let (Some(ptr_ty), PreHirExpr::Var(name)) = (rhs_ptr.as_ref(), lhs.as_ref()) {
                out.entry(name.clone()).or_insert_with(|| ptr_ty.clone());
            }
            collect_pointer_compare_peer_promotions_expr(lhs, types, out);
            collect_pointer_compare_peer_promotions_expr(rhs, types, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_pointer_compare_peer_promotions_expr(lhs, types, out);
            collect_pointer_compare_peer_promotions_expr(rhs, types, out);
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_pointer_compare_peer_promotions_expr(expr, types, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_pointer_compare_peer_promotions_expr(cond, types, out);
            collect_pointer_compare_peer_promotions_expr(then_expr, types, out);
            collect_pointer_compare_peer_promotions_expr(else_expr, types, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_pointer_compare_peer_promotions_expr(arg, types, out);
            }
        }
        PreHirExpr::Load { ptr, .. }
        | PreHirExpr::PtrOffset { base: ptr, .. }
        | PreHirExpr::FieldAccess { base: ptr, .. }
        | PreHirExpr::AggregateCopy { src: ptr, .. } => {
            collect_pointer_compare_peer_promotions_expr(ptr, types, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_pointer_compare_peer_promotions_expr(base, types, out);
            collect_pointer_compare_peer_promotions_expr(index, types, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn zero_initializer_aliases(func: &PreHirFunction) -> HashSet<String> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter_map(|binding| match binding.initializer.as_ref() {
            Some(PreHirExpr::Const(0, _)) => Some(binding.name.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn rewrite_scalar_zero_alias_assignments(func: &mut PreHirFunction) -> bool {
    let zero_aliases = zero_initializer_aliases(func);
    if zero_aliases.is_empty() {
        return false;
    }
    let binding_types = collect_known_binding_types(func);
    rewrite_scalar_zero_alias_stmts(&mut func.body, &binding_types, &zero_aliases)
}

fn rewrite_scalar_zero_alias_stmts(
    stmts: &mut [PreHirStmt],
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
    stmt: &mut PreHirStmt,
    binding_types: &HashMap<String, NirType>,
    zero_aliases: &HashSet<String>,
) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
            rhs,
        } => {
            let PreHirExpr::Var(src) = rhs else {
                return false;
            };
            let Some(lhs_ty) = binding_types.get(lhs.as_str()) else {
                return false;
            };
            if !matches!(lhs_ty, NirType::Int { .. }) || !zero_aliases.contains(src.as_str()) {
                return false;
            }
            *rhs = PreHirExpr::Const(0, lhs_ty.clone());
            true
        }
        PreHirStmt::Block(stmts) | PreHirStmt::While { body: stmts, .. } => {
            rewrite_scalar_zero_alias_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
                binding_types,
                zero_aliases,
            )
        }
        PreHirStmt::DoWhile { body, .. } => rewrite_scalar_zero_alias_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            binding_types,
            zero_aliases,
        ),
        PreHirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = false;
            if let Some(init) = init {
                changed |= rewrite_scalar_zero_alias_stmt(init, binding_types, zero_aliases);
            }
            if let Some(update) = update {
                changed |= rewrite_scalar_zero_alias_stmt(update, binding_types, zero_aliases);
            }
            changed
                | rewrite_scalar_zero_alias_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    binding_types,
                    zero_aliases,
                )
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            rewrite_scalar_zero_alias_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                binding_types,
                zero_aliases,
            ) | rewrite_scalar_zero_alias_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                binding_types,
                zero_aliases,
            )
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |= rewrite_scalar_zero_alias_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    binding_types,
                    zero_aliases,
                );
            }
            changed
                | rewrite_scalar_zero_alias_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    binding_types,
                    zero_aliases,
                )
        }
        _ => false,
    }
}

fn param_name_set(func: &PreHirFunction) -> HashSet<String> {
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
    stmts: &[PreHirStmt],
    dependencies: &HashMap<String, DefEntry>,
    binding_types: &HashMap<String, NirType>,
    params: &HashSet<String>,
    out: &mut StrongScalarParamRoots,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                collect_strong_scalar_param_roots_expr(
                    rhs,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
                collect_strong_scalar_param_roots_stmts(
                    body,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            PreHirStmt::DoWhile { body, cond } => {
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
            PreHirStmt::If {
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
            PreHirStmt::For {
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
            PreHirStmt::Switch {
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
            PreHirStmt::VaStart { va_list, .. } => {
                collect_strong_scalar_param_roots_expr(
                    va_list,
                    dependencies,
                    binding_types,
                    params,
                    out,
                );
            }
            PreHirStmt::Return(None)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn collect_strong_scalar_param_roots_expr(
    expr: &PreHirExpr,
    dependencies: &HashMap<String, DefEntry>,
    binding_types: &HashMap<String, NirType>,
    params: &HashSet<String>,
    out: &mut StrongScalarParamRoots,
) {
    match expr {
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            if matches!(
                op,
                PreHirBinaryOp::Shl | PreHirBinaryOp::Shr | PreHirBinaryOp::Sar
            ) {
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
                PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::Gt
                    | PreHirBinaryOp::Ge
                    | PreHirBinaryOp::SLt
                    | PreHirBinaryOp::SLe
                    | PreHirBinaryOp::SGt
                    | PreHirBinaryOp::SGe
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
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            collect_strong_scalar_param_roots_expr(expr, dependencies, binding_types, params, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_strong_scalar_param_roots_expr(base, dependencies, binding_types, params, out);
            collect_strong_scalar_param_roots_expr(index, dependencies, binding_types, params, out);
        }
        PreHirExpr::Select {
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
        PreHirExpr::Call { args, .. } => {
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
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

/// Collect parameters that appear as the integer-offset side of a pointer add.
///
/// The pointer-base side must have independent address-use evidence. This keeps
/// the classification fail-closed when both operands are merely pointer-sized.
fn collect_param_pointer_offset_params_stmts(
    stmts: &[PreHirStmt],
    context: &ParamPointerRoleContext<'_>,
    out: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                collect_param_pointer_offset_params_expr(rhs, context, out);
            }
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
                collect_param_pointer_offset_params_stmts(body, context, out);
            }
            PreHirStmt::DoWhile { body, cond } => {
                collect_param_pointer_offset_params_stmts(body, context, out);
                collect_param_pointer_offset_params_expr(cond, context, out);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_param_pointer_offset_params_expr(cond, context, out);
                collect_param_pointer_offset_params_stmts(then_body, context, out);
                collect_param_pointer_offset_params_stmts(else_body, context, out);
            }
            PreHirStmt::For {
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
            PreHirStmt::Switch {
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
    expr: &PreHirExpr,
    context: &ParamPointerRoleContext<'_>,
    out: &mut HashSet<String>,
) {
    if expr_has_first_def_pointer_type(expr, context.defs) {
        return;
    }
    let mut cur = expr;
    while let PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } = cur {
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

fn expr_is_pointer_base(expr: &PreHirExpr, context: &ParamPointerRoleContext<'_>) -> bool {
    let mut cur = expr;
    while let PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } = cur {
        cur = expr.as_ref();
    }
    let PreHirExpr::Var(name) = cur else {
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
    expr: &PreHirExpr,
    context: &ParamPointerRoleContext<'_>,
    out: &mut HashSet<String>,
) {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
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
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_param_pointer_offset_params_expr(lhs, context, out);
            collect_param_pointer_offset_params_expr(rhs, context, out);
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_param_pointer_offset_params_expr(expr, context, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_param_pointer_offset_params_expr(cond, context, out);
            collect_param_pointer_offset_params_expr(then_expr, context, out);
            collect_param_pointer_offset_params_expr(else_expr, context, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_param_pointer_offset_params_expr(arg, context, out);
            }
        }
        PreHirExpr::Load { ptr, .. }
        | PreHirExpr::PtrOffset { base: ptr, .. }
        | PreHirExpr::FieldAccess { base: ptr, .. }
        | PreHirExpr::AggregateCopy { src: ptr, .. } => {
            collect_param_pointer_offset_params_expr(ptr, context, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_param_pointer_offset_params_expr(base, context, out);
            collect_param_pointer_offset_params_expr(index, context, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
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

fn expr_has_first_def_pointer_type(expr: &PreHirExpr, defs: &HashMap<String, DefEntry>) -> bool {
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
        PreHirExpr::Var(name) => binding_has_pointer_type(name, defs, &mut HashSet::default()),
        PreHirExpr::Cast {
            ty: NirType::Ptr(_),
            ..
        } => true,
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
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
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<NirType> {
    match expr {
        PreHirExpr::Var(name) => binding_types.get(name).and_then(|ty| match ty {
            NirType::Ptr(_) => Some(ty.clone()),
            _ => None,
        }),
        PreHirExpr::Cast {
            ty: NirType::Ptr(_),
            ..
        } => Some(expr_type(expr)),
        PreHirExpr::PtrOffset { base, .. }
        | PreHirExpr::Load { ptr: base, .. }
        | PreHirExpr::FieldAccess { base, .. }
        | PreHirExpr::AggregateCopy { src: base, .. } => pointer_type_of_expr(base, binding_types),
        PreHirExpr::Index { base, .. } => pointer_type_of_expr(base, binding_types),
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
    expr: &PreHirExpr,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
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
        PreHirExpr::Binary { lhs, rhs, .. } => {
            param_pointer_candidates_from_expr(lhs, context, out);
            param_pointer_candidates_from_expr(rhs, context, out);
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            param_pointer_candidates_from_expr(expr, context, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            param_pointer_candidates_from_expr(cond, context, out);
            param_pointer_candidates_from_expr(then_expr, context, out);
            param_pointer_candidates_from_expr(else_expr, context, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                param_pointer_candidates_from_expr(arg, context, out);
            }
        }
        PreHirExpr::Load { ptr, ty } => {
            // Param used as a load address is a pointer to the loaded type.
            record_param_pointer_from_address_expr(ptr, ty, context, out);
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        PreHirExpr::PtrOffset { base: ptr, .. }
        | PreHirExpr::FieldAccess { base: ptr, .. }
        | PreHirExpr::AggregateCopy { src: ptr, .. } => {
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            param_pointer_candidates_from_expr(base, context, out);
            param_pointer_candidates_from_expr(index, context, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn record_param_pointer_from_address_expr(
    addr: &PreHirExpr,
    pointee: &NirType,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match addr {
        PreHirExpr::Var(name) => {
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
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
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
                if matches!(rhs.as_ref(), PreHirExpr::Var(n) if !context.params.contains(n.as_str()))
                {
                    record_param_pointer_from_address_expr(lhs, pointee, context, out);
                }
                if matches!(lhs.as_ref(), PreHirExpr::Var(n) if !context.params.contains(n.as_str()))
                {
                    record_param_pointer_from_address_expr(rhs, pointee, context, out);
                }
            }
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            record_param_pointer_from_address_expr(expr, pointee, context, out);
        }
        _ => {}
    }
}

fn expr_looks_integer_offset(expr: &PreHirExpr, binding_types: &HashMap<String, NirType>) -> bool {
    match expr {
        PreHirExpr::Const(_, _) => true,
        PreHirExpr::Var(name) => matches!(
            binding_types.get(name.as_str()),
            Some(NirType::Int { .. } | NirType::Bool)
        ),
        PreHirExpr::Cast {
            ty: NirType::Int { .. },
            ..
        } => true,
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_looks_integer_offset(expr, binding_types)
        }
        PreHirExpr::Binary {
            op:
                PreHirBinaryOp::Add | PreHirBinaryOp::Sub | PreHirBinaryOp::Mul | PreHirBinaryOp::Shl,
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
    lhs: &PreHirLValue,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, ty } => {
            record_param_pointer_from_address_expr(ptr, ty, context, out);
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        PreHirLValue::FieldAccess { base: ptr, .. } => {
            param_pointer_candidates_from_expr(ptr, context, out);
        }
        PreHirLValue::Index { base, index, .. } => {
            param_pointer_candidates_from_expr(base, context, out);
            param_pointer_candidates_from_expr(index, context, out);
        }
    }
}

fn collect_param_pointer_candidates_stmts(
    stmts: &[PreHirStmt],
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    for stmt in stmts {
        collect_param_pointer_candidates_stmt(stmt, context, out);
    }
}

fn collect_param_pointer_candidates_stmt(
    stmt: &PreHirStmt,
    context: &ParamPointerCandidateContext<'_>,
    out: &mut HashMap<String, NirType>,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            param_pointer_candidates_from_lvalue(lhs, context, out);
            param_pointer_candidates_from_expr(rhs, context, out);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            param_pointer_candidates_from_expr(expr, context, out);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            param_pointer_candidates_from_expr(va_list, context, out);
        }
        PreHirStmt::Block(stmts) | PreHirStmt::While { body: stmts, .. } => {
            collect_param_pointer_candidates_stmts(stmts, context, out);
        }
        PreHirStmt::DoWhile { body, cond } => {
            collect_param_pointer_candidates_stmts(body, context, out);
            param_pointer_candidates_from_expr(cond, context, out);
        }
        PreHirStmt::For {
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
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            param_pointer_candidates_from_expr(cond, context, out);
            collect_param_pointer_candidates_stmts(then_body, context, out);
            collect_param_pointer_candidates_stmts(else_body, context, out);
        }
        PreHirStmt::Switch {
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
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

pub(super) fn apply_address_contributor_param_pointer_types(
    func: &mut PreHirFunction,
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
fn demote_pointer_offset_params(func: &mut PreHirFunction) -> bool {
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

fn collect_word_load_pointer_names(expr: &PreHirExpr, out: &mut HashMap<String, u32>) {
    match expr {
        PreHirExpr::Load {
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
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_word_load_pointer_names(expr, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_word_load_pointer_names(lhs, out);
            collect_word_load_pointer_names(rhs, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_word_load_pointer_names(cond, out);
            collect_word_load_pointer_names(then_expr, out);
            collect_word_load_pointer_names(else_expr, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_word_load_pointer_names(arg, out);
            }
        }
        PreHirExpr::Load { ptr, .. }
        | PreHirExpr::PtrOffset { base: ptr, .. }
        | PreHirExpr::FieldAccess { base: ptr, .. }
        | PreHirExpr::AggregateCopy { src: ptr, .. } => {
            collect_word_load_pointer_names(ptr, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_word_load_pointer_names(base, out);
            collect_word_load_pointer_names(index, out);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn collect_signed_neutral_load_contexts_stmts(
    stmts: &[PreHirStmt],
    candidates: &mut HashMap<String, u32>,
    blockers: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                collect_signed_neutral_load_contexts_expr(rhs, candidates, blockers);
            }
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
                collect_signed_neutral_load_contexts_stmts(body, candidates, blockers);
            }
            PreHirStmt::DoWhile { body, cond } => {
                collect_signed_neutral_load_contexts_stmts(body, candidates, blockers);
                collect_signed_neutral_load_contexts_expr(cond, candidates, blockers);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_signed_neutral_load_contexts_expr(cond, candidates, blockers);
                collect_signed_neutral_load_contexts_stmts(then_body, candidates, blockers);
                collect_signed_neutral_load_contexts_stmts(else_body, candidates, blockers);
            }
            PreHirStmt::For {
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
            PreHirStmt::Switch {
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
            PreHirStmt::VaStart { va_list, .. } => {
                collect_signed_neutral_load_contexts_expr(va_list, candidates, blockers);
            }
            PreHirStmt::Return(None)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn collect_signed_neutral_load_contexts_expr(
    expr: &PreHirExpr,
    candidates: &mut HashMap<String, u32>,
    blockers: &mut HashSet<String>,
) {
    match expr {
        PreHirExpr::Binary { op, lhs, rhs, ty } => {
            if matches!(
                (op, ty),
                (
                    PreHirBinaryOp::Add | PreHirBinaryOp::Sub | PreHirBinaryOp::Mul,
                    NirType::Int { signed: true, .. }
                )
            ) {
                collect_word_load_pointer_names(lhs, candidates);
                collect_word_load_pointer_names(rhs, candidates);
            }
            if matches!(
                op,
                PreHirBinaryOp::Div
                    | PreHirBinaryOp::Mod
                    | PreHirBinaryOp::And
                    | PreHirBinaryOp::Or
                    | PreHirBinaryOp::Xor
                    | PreHirBinaryOp::Shr
                    | PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::Gt
                    | PreHirBinaryOp::Ge
            ) {
                let mut unsigned_loads = HashMap::default();
                collect_word_load_pointer_names(lhs, &mut unsigned_loads);
                collect_word_load_pointer_names(rhs, &mut unsigned_loads);
                blockers.extend(unsigned_loads.into_keys());
            }
            collect_signed_neutral_load_contexts_expr(lhs, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(rhs, candidates, blockers);
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_signed_neutral_load_contexts_expr(expr, candidates, blockers);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_signed_neutral_load_contexts_expr(cond, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(then_expr, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(else_expr, candidates, blockers);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_signed_neutral_load_contexts_expr(arg, candidates, blockers);
            }
        }
        PreHirExpr::Load { ptr, .. }
        | PreHirExpr::PtrOffset { base: ptr, .. }
        | PreHirExpr::FieldAccess { base: ptr, .. }
        | PreHirExpr::AggregateCopy { src: ptr, .. } => {
            collect_signed_neutral_load_contexts_expr(ptr, candidates, blockers);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_signed_neutral_load_contexts_expr(base, candidates, blockers);
            collect_signed_neutral_load_contexts_expr(index, candidates, blockers);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

pub(super) fn promote_signed_neutral_word_load_pointees(
    func: &mut PreHirFunction,
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
    expr: &PreHirExpr,
    pointee_ty: Option<&NirType>,
    roles: &mut HashMap<String, BindingUseRole>,
) {
    fn mark_root(
        expr: &PreHirExpr,
        pointee_ty: Option<&NirType>,
        roles: &mut HashMap<String, BindingUseRole>,
    ) {
        match expr {
            PreHirExpr::Var(name) => {
                let role = roles.entry(name.clone()).or_default();
                role.address_use = true;
                if let Some(ty) = pointee_ty
                    && *ty != NirType::Unknown
                {
                    role.address_pointee_type.get_or_insert_with(|| ty.clone());
                }
            }
            PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
                mark_root(expr, pointee_ty, roles);
            }
            PreHirExpr::PtrOffset { base, .. }
            | PreHirExpr::FieldAccess { base, .. }
            | PreHirExpr::AggregateCopy { src: base, .. } => {
                mark_root(base, pointee_ty, roles);
            }
            PreHirExpr::Index { base, .. } => mark_root(base, pointee_ty, roles),
            PreHirExpr::Binary { .. }
            | PreHirExpr::Select { .. }
            | PreHirExpr::Call { .. }
            | PreHirExpr::Load { .. }
            | PreHirExpr::Const(_, _)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_) => {}
        }
    }

    mark_root(expr, pointee_ty, roles);
    collect_binding_use_roles_expr(expr, roles);
}

fn mark_strong_scalar_use(expr: &PreHirExpr, roles: &mut HashMap<String, BindingUseRole>) {
    if let PreHirExpr::Var(name) = expr {
        roles.entry(name.clone()).or_default().strong_scalar_use = true;
    }
    collect_binding_use_roles_expr(expr, roles);
}

fn scalar_role_op(op: PreHirBinaryOp) -> bool {
    matches!(
        op,
        PreHirBinaryOp::Mod
            | PreHirBinaryOp::And
            | PreHirBinaryOp::Or
            | PreHirBinaryOp::Xor
            | PreHirBinaryOp::Shl
            | PreHirBinaryOp::Shr
            | PreHirBinaryOp::Sar
    )
}

fn collect_binding_use_roles_stmts(
    stmts: &[PreHirStmt],
    roles: &mut HashMap<String, BindingUseRole>,
) {
    for stmt in stmts {
        collect_binding_use_roles_stmt(stmt, roles);
    }
}

fn collect_binding_use_roles_stmt(stmt: &PreHirStmt, roles: &mut HashMap<String, BindingUseRole>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_binding_use_roles_lvalue(lhs, roles);
            collect_binding_use_roles_expr(rhs, roles);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            collect_binding_use_roles_expr(expr, roles);
        }
        PreHirStmt::VaStart { va_list, .. } => collect_binding_use_roles_expr(va_list, roles),
        PreHirStmt::Block(stmts) => collect_binding_use_roles_stmts(stmts, roles),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_binding_use_roles_expr(cond, roles);
            collect_binding_use_roles_stmts(then_body, roles);
            collect_binding_use_roles_stmts(else_body, roles);
        }
        PreHirStmt::While { cond, body } => {
            collect_binding_use_roles_expr(cond, roles);
            collect_binding_use_roles_stmts(body, roles);
        }
        PreHirStmt::DoWhile { body, cond } => {
            collect_binding_use_roles_stmts(body, roles);
            collect_binding_use_roles_expr(cond, roles);
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch {
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
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_binding_use_roles_lvalue(
    lhs: &PreHirLValue,
    roles: &mut HashMap<String, BindingUseRole>,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, ty } => mark_address_use(ptr, Some(ty), roles),
        PreHirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            mark_address_use(base, Some(elem_ty), roles);
            collect_binding_use_roles_expr(index, roles);
        }
        PreHirLValue::FieldAccess { base, .. } => mark_address_use(base, None, roles),
    }
}

fn collect_binding_use_roles_expr(expr: &PreHirExpr, roles: &mut HashMap<String, BindingUseRole>) {
    match expr {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_binding_use_roles_expr(expr, roles);
        }
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            if scalar_role_op(*op) {
                mark_strong_scalar_use(lhs, roles);
                mark_strong_scalar_use(rhs, roles);
            } else {
                collect_binding_use_roles_expr(lhs, roles);
                collect_binding_use_roles_expr(rhs, roles);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_binding_use_roles_expr(cond, roles);
            collect_binding_use_roles_expr(then_expr, roles);
            collect_binding_use_roles_expr(else_expr, roles);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_binding_use_roles_expr(arg, roles);
            }
        }
        PreHirExpr::Load { ptr, ty } => mark_address_use(ptr, Some(ty), roles),
        PreHirExpr::PtrOffset { base, .. } => mark_address_use(base, None, roles),
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            mark_address_use(base, Some(elem_ty), roles);
            collect_binding_use_roles_expr(index, roles);
        }
        PreHirExpr::FieldAccess { base, .. } => mark_address_use(base, None, roles),
        PreHirExpr::AggregateCopy { src, .. } => mark_address_use(src, None, roles),
    }
}
