use super::*;
use crate::midend::abstract_location::AbstractStackSlot;
use crate::midend::var_rename::{rename_vars_in_stmts, rewrite_field_access_names_in_stmts};
use tracing::trace_span;

pub(super) struct StackAliasCollector {
    alias_boundaries: Vec<(AbstractStackSlot, u64)>,
}

impl StackAliasCollector {
    pub(super) fn new(func: &HirFunction) -> Self {
        let mut boundaries = Vec::new();
        for local in &func.locals {
            if let Some(slot) = AbstractStackSlot::from_binding_origin(local.origin) {
                if let Some(size) = binding_byte_size(&local.ty) {
                    boundaries.push((slot, size as u64));
                }
            }
        }
        Self {
            alias_boundaries: boundaries,
        }
    }

    fn might_alias(&self, offset: i64, size: u32) -> bool {
        let probe = AbstractStackSlot(offset);
        let sz = size as u64;
        self.alias_boundaries
            .iter()
            .any(|&(slot, slot_sz)| probe.intervals_overlap(sz, slot, slot_sz))
    }
}

pub(super) fn apply_preview_type_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
    register_origins: &HashMap<String, (u64, u32)>,
) -> PreviewHintStats {
    let _hints = trace_span!("preview_type_hints", fn_name = %func.name).entered();
    let mut stats = apply_function_name_hints(func, context, register_origins);
    apply_debug_struct_promotions(func, context, &mut stats);
    apply_debug_struct_field_names(func, context, &mut stats);
    let alias_collector = StackAliasCollector::new(func);

    let mut pointer_hints: HashMap<String, PreviewCallParamRule> = HashMap::default();
    collect_call_type_hints(&func.body, context, &mut pointer_hints);

    for (var_name, hint) in &pointer_hints {
        if let Some(binding) = find_binding_mut(func, var_name)
            && binding.surface_type_name.is_none()
        {
            let should_apply = match stack_origin_offset(binding.origin) {
                Some((offset, is_derived)) => {
                    is_derived && alias_collector.might_alias(offset, hint.pointer_size)
                }
                // Keep synthetic/test bodies and non-stack params eligible.
                None => true,
            };
            if should_apply {
                binding.surface_type_name = Some(hint.pointer_alias.clone());
                stats.pointer_alias_hits += 1;
            }
        }
    }

    let mut local_hints: HashMap<String, String> = HashMap::default();
    collect_local_surface_hints(
        &func.body,
        &pointer_hints,
        func,
        &alias_collector,
        &mut local_hints,
    );

    for (var_name, surface_type_name) in local_hints {
        if let Some(binding) = func
            .locals
            .iter_mut()
            .find(|binding| binding.name == var_name)
            && binding.surface_type_name.is_none()
        {
            binding.surface_type_name = Some(surface_type_name);
            stats.local_surface_hits += 1;
        }
    }

    stats
}

fn apply_function_name_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
    register_origins: &HashMap<String, (u64, u32)>,
) -> PreviewHintStats {
    let mut stats = PreviewHintStats::default();
    let Some(hints) = &context.function_hints else {
        return stats;
    };

    ensure_missing_hinted_params(func, hints, &mut stats);

    let mut renames = Vec::new();
    let mut reserved_names = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();

    for binding in &mut func.params {
        let Some(NirBindingOrigin::ParamIndex(index)) = binding.origin else {
            continue;
        };
        let Some(new_name) = hints.param_names.get(index) else {
            continue;
        };
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name == binding.name {
            continue;
        }
        if reserved_names.contains(new_name) {
            continue;
        }
        reserved_names.remove(&binding.name);
        reserved_names.insert(new_name.to_string());
        renames.push((binding.name.clone(), new_name.to_string()));
        binding.name = new_name.to_string();
        stats.explicit_param_name_hits += 1;
    }

    for binding in &mut func.locals {
        let Some(
            NirBindingOrigin::StackOffset(offset)
            | NirBindingOrigin::HomeSlot(offset)
            | NirBindingOrigin::OutgoingArgSlot(offset),
        ) = binding.origin
        else {
            continue;
        };
        let Some(new_name) = hints.stack_local_names.get(&offset) else {
            continue;
        };
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name == binding.name {
            continue;
        }
        if reserved_names.contains(new_name) {
            continue;
        }
        reserved_names.remove(&binding.name);
        reserved_names.insert(new_name.to_string());
        renames.push((binding.name.clone(), new_name.to_string()));
        binding.name = new_name.to_string();
        stats.explicit_local_name_hits += 1;
    }

    if !hints.register_local_names.is_empty() {
        // A register has no stable per-function identity the way a stack
        // slot's address does -- it gets reused for unrelated values
        // constantly, which is why `register_local_names` only ever contains
        // an entry when the DWARF location agrees on the *same* register
        // across every range of the variable's declared scope (see
        // `DwarfAnalyzer::parse_location_list` / `extract_location`) --
        // never a guess from a single range. Given that, an *assignment
        // count* gate here would be redundant in the wrong direction: the
        // dominant real case (a loop accumulator, `total = 0; ... total +=
        // x;`) is written more than once *by construction*, and
        // materialization already gives every write to the same physical
        // register the same one binding for the whole function -- multiple
        // assignments to it are normal read-modify-write on that one
        // variable, not evidence of the register being repurposed. The
        // residual risk this doesn't cover -- Fission reusing the same
        // binding name for an unrelated value *outside* the DWARF-declared
        // scope -- isn't something a body-wide assignment count can
        // distinguish from the accumulator case either, so it isn't gated
        // away here.
        for binding in &mut func.locals {
            if !matches!(
                binding.origin,
                Some(NirBindingOrigin::Temp | NirBindingOrigin::TempPreserved)
            ) {
                continue;
            }
            let Some((register_offset, _register_size)) = register_origins.get(&binding.name)
            else {
                continue;
            };
            let Some(new_name) = hints.register_local_names.get(register_offset) else {
                continue;
            };
            let new_name = new_name.trim();
            if new_name.is_empty() || new_name == binding.name {
                continue;
            }
            if reserved_names.contains(new_name) {
                continue;
            }
            reserved_names.remove(&binding.name);
            reserved_names.insert(new_name.to_string());
            renames.push((binding.name.clone(), new_name.to_string()));
            binding.name = new_name.to_string();
            stats.explicit_register_local_name_hits += 1;
        }
    }

    if !renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &renames);
    }

    for binding in &mut func.params {
        let Some(NirBindingOrigin::ParamIndex(index)) = binding.origin else {
            continue;
        };
        let Some(type_name) = hints.param_type_names.get(&index) else {
            continue;
        };
        let type_name = type_name.trim();
        if !type_name.is_empty() {
            binding.surface_type_name = Some(type_name.to_string());
            stats.explicit_param_type_hits += 1;
        }
    }

    for binding in &mut func.locals {
        let Some((offset, is_derived)) = stack_origin_offset(binding.origin) else {
            continue;
        };
        let Some(type_name) = hints.stack_local_type_names.get(&offset) else {
            continue;
        };
        let type_name = type_name.trim();
        if !type_name.is_empty() {
            binding.surface_type_name = Some(type_name.to_string());
            stats.explicit_local_type_hits += 1;
            if is_derived {
                stats.derived_origin_type_hits += 1;
            }
        }
    }

    if let Some(return_type_name) = hints
        .return_type_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        func.surface_return_type_name = Some(return_type_name.to_string());
        stats.explicit_return_type_hit += 1;
        if let Some(bits) = surface_integer_return_bits(return_type_name) {
            elide_surface_return_casts(&mut func.body, bits);
        }
    }

    stats
}

/// Overlay real field names from debug-info struct/union layouts onto
/// already-recovered `NirType::Aggregate` fields.
///
/// Does not decide which variables become aggregates, and does not touch
/// field offsets or types: `aggregate_fields.rs` (an earlier normalize
/// pass) already derived those from actual observed load/store access
/// widths, which is grounded in real pcode and safer to trust than a
/// naively re-parsed debug-info type string. This only renames a field
/// whose offset matches a field in a debug-info type named by the
/// binding's `surface_type_name` -- from a synthetic `field_{offset:x}`
/// to its real declared name.
/// Promote a param/local straight to `NirType::Ptr(Aggregate)` from a
/// debug-info struct/union layout, for bindings `aggregate_fields.rs`'s own
/// heuristic never touches.
///
/// `aggregate_fields.rs` only promotes from `Ptr(Unknown | Int{8|16})` --
/// deliberately excluding wider integer pointers (`Ptr(Int{32|64})`) to
/// avoid misclassifying a genuine `int*`/`long*` array as a fake struct
/// when there's no other evidence. That exclusion is exactly right without
/// debug info, but it also means a struct whose first field is `int` or
/// wider (the common case) never gets promoted at all -- confirmed with a
/// real `-O0` build of `struct Point { int x, y; }; int f(Point *p) {
/// return p->x + p->y; }`, where `p`'s type lands on `Ptr(Int{32})` (from
/// the first dereference) and never advances. With DWARF/PDB proof that
/// the type really is a struct, there's no more ambiguity, so this widens
/// the promotion to any pointer type not already an aggregate.
///
/// Deliberately narrow in a different way instead: only rewrites the two
/// simplest, single-expression access shapes --
/// `Load{ptr: Var(name)}`/`Deref{ptr: Var(name)}` (field at offset 0) and
/// `Load{ptr: PtrOffset{base: Var(name), offset}}`/matching `Deref` (field
/// at a nonzero constant offset) -- plus one level of direct-copy alias
/// (`local_8 = p;` where `local_8` is assigned exactly once in the whole
/// function): real -O0 output confirmed this is not an edge case but the
/// *dominant* shape, since compilers commonly spill a param into a local
/// "shadow" before its first use, so without following it this pass would
/// almost never fire in practice. Does not follow pointer values through
/// non-copy intermediate assignments (`t = p + 1; ... *t ...`) or aliases
/// assigned more than once; reaching those would need real cross-statement
/// def-use/reaching-definitions tracking this pass doesn't have. A binding
/// whose accesses are all past this pass's reach keeps its existing
/// (non-aggregate) type -- silently doing less, never wrongly promoting a
/// field access it can't actually verify the offset of.
fn apply_debug_struct_promotions(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
    stats: &mut PreviewHintStats,
) {
    if context.struct_types.is_empty() {
        return;
    }
    let mut eligible: HashMap<String, &NirStructTypeHint> = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        let Some(surface_name) = binding.surface_type_name.as_deref() else {
            continue;
        };
        let Some(struct_name) = struct_base_name_for_single_pointer(surface_name) else {
            continue;
        };
        let Some(struct_hint) = context.struct_types.get(struct_name) else {
            continue;
        };
        let already_aggregate = matches!(
            &binding.ty,
            NirType::Ptr(inner) if matches!(inner.as_ref(), NirType::Aggregate { .. })
        );
        if already_aggregate || struct_hint.fields.is_empty() {
            continue;
        }
        eligible.insert(binding.name.clone(), struct_hint);
    }
    if eligible.is_empty() {
        return;
    }
    extend_with_copy_aliases(&func.body, &mut eligible);

    let mut promoted_names: HashSet<String> = HashSet::default();
    promote_field_access_in_stmts(&mut func.body, &eligible, &mut promoted_names);
    if promoted_names.is_empty() {
        return;
    }

    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        let Some(struct_hint) = promoted_names
            .contains(&binding.name)
            .then(|| eligible.get(&binding.name))
            .flatten()
        else {
            continue;
        };
        binding.ty = NirType::Ptr(Box::new(NirType::Aggregate {
            size: struct_hint.size,
            fields: struct_hint
                .fields
                .iter()
                .map(|f| StructField {
                    offset: f.offset,
                    ty: NirType::Unknown,
                    name: f.name.clone(),
                })
                .collect(),
        }));
        stats.debug_struct_promotions += 1;
    }
}

/// Extend `eligible` with locals that are direct, single-assignment copies
/// of an already-eligible binding (`local_8 = p;`, where `local_8` is
/// assigned exactly once in the whole function). One level only -- does
/// not chase `local_9 = local_8;` chains.
fn extend_with_copy_aliases<'a>(
    body: &[HirStmt],
    eligible: &mut HashMap<String, &'a NirStructTypeHint>,
) {
    let mut assign_counts: HashMap<String, u32> = HashMap::default();
    let mut direct_copies: HashMap<String, String> = HashMap::default();
    collect_assign_stats_in_stmts(body, &mut assign_counts, &mut direct_copies);

    let new_entries: Vec<(String, &'a NirStructTypeHint)> = direct_copies
        .into_iter()
        .filter(|(name, _)| assign_counts.get(name).copied().unwrap_or(0) == 1)
        .filter_map(|(name, source)| eligible.get(&source).map(|hint| (name, *hint)))
        .collect();
    for (name, hint) in new_entries {
        eligible.entry(name).or_insert(hint);
    }
}

fn collect_assign_stats_in_stmts(
    body: &[HirStmt],
    assign_counts: &mut HashMap<String, u32>,
    direct_copies: &mut HashMap<String, String>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } => {
                *assign_counts.entry(name.clone()).or_insert(0) += 1;
                if let HirExpr::Var(source) = rhs {
                    direct_copies.insert(name.clone(), source.clone());
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
            HirStmt::Block(stmts) => {
                collect_assign_stats_in_stmts(stmts, assign_counts, direct_copies)
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_assign_stats_in_stmts(body, assign_counts, direct_copies)
            }
            HirStmt::For {
                init,
                update,
                body,
                ..
            } => {
                if let Some(init_stmt) = init {
                    collect_assign_stats_in_stmts(
                        std::slice::from_ref(init_stmt.as_ref()),
                        assign_counts,
                        direct_copies,
                    );
                }
                if let Some(update_stmt) = update {
                    collect_assign_stats_in_stmts(
                        std::slice::from_ref(update_stmt.as_ref()),
                        assign_counts,
                        direct_copies,
                    );
                }
                collect_assign_stats_in_stmts(body, assign_counts, direct_copies);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_assign_stats_in_stmts(&case.body, assign_counts, direct_copies);
                }
                collect_assign_stats_in_stmts(default, assign_counts, direct_copies);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_assign_stats_in_stmts(then_body, assign_counts, direct_copies);
                collect_assign_stats_in_stmts(else_body, assign_counts, direct_copies);
            }
        }
    }
}

fn promote_field_access_in_stmts(
    body: &mut [HirStmt],
    eligible: &HashMap<String, &NirStructTypeHint>,
    promoted: &mut HashSet<String>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                promote_field_access_in_lvalue(lhs, eligible, promoted);
                promote_field_access_in_expr(rhs, eligible, promoted);
            }
            HirStmt::VaStart { va_list, .. } => {
                promote_field_access_in_expr(va_list, eligible, promoted)
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                promote_field_access_in_expr(expr, eligible, promoted)
            }
            HirStmt::Block(stmts) => promote_field_access_in_stmts(stmts, eligible, promoted),
            HirStmt::While { cond, body } => {
                promote_field_access_in_expr(cond, eligible, promoted);
                promote_field_access_in_stmts(body, eligible, promoted);
            }
            HirStmt::DoWhile { body, cond } => {
                promote_field_access_in_stmts(body, eligible, promoted);
                promote_field_access_in_expr(cond, eligible, promoted);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    promote_field_access_in_stmts(
                        std::slice::from_mut(init_stmt.as_mut()),
                        eligible,
                        promoted,
                    );
                }
                if let Some(cond_expr) = cond {
                    promote_field_access_in_expr(cond_expr, eligible, promoted);
                }
                if let Some(update_stmt) = update {
                    promote_field_access_in_stmts(
                        std::slice::from_mut(update_stmt.as_mut()),
                        eligible,
                        promoted,
                    );
                }
                promote_field_access_in_stmts(body, eligible, promoted);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                promote_field_access_in_expr(expr, eligible, promoted);
                for case in cases {
                    promote_field_access_in_stmts(&mut case.body, eligible, promoted);
                }
                promote_field_access_in_stmts(default, eligible, promoted);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                promote_field_access_in_expr(cond, eligible, promoted);
                promote_field_access_in_stmts(then_body, eligible, promoted);
                promote_field_access_in_stmts(else_body, eligible, promoted);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn promote_field_access_in_lvalue(
    lvalue: &mut HirLValue,
    eligible: &HashMap<String, &NirStructTypeHint>,
    promoted: &mut HashSet<String>,
) {
    match lvalue {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, ty } => {
            if let Some((base, field_name, offset)) = base_field_at_offset(ptr, ty, eligible) {
                *lvalue = HirLValue::FieldAccess {
                    base: Box::new(base),
                    field_name: field_name.clone(),
                    offset,
                    ty: ty.clone(),
                };
                if let HirLValue::FieldAccess { base, .. } = lvalue
                    && let HirExpr::Var(name) = base.as_ref()
                {
                    promoted.insert(name.clone());
                }
                return;
            }
            promote_field_access_in_expr(ptr, eligible, promoted);
        }
        HirLValue::Index { base, index, .. } => {
            promote_field_access_in_expr(base, eligible, promoted);
            promote_field_access_in_expr(index, eligible, promoted);
        }
        HirLValue::FieldAccess { base, .. } => {
            promote_field_access_in_expr(base, eligible, promoted)
        }
    }
}

fn promote_field_access_in_expr(
    expr: &mut HirExpr,
    eligible: &HashMap<String, &NirStructTypeHint>,
    promoted: &mut HashSet<String>,
) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            if let Some((base, field_name, offset)) = base_field_at_offset(ptr, ty, eligible) {
                if let HirExpr::Var(name) = &base {
                    promoted.insert(name.clone());
                }
                *expr = HirExpr::FieldAccess {
                    base: Box::new(base),
                    field_name: field_name.clone(),
                    offset,
                    ty: ty.clone(),
                };
                return;
            }
            promote_field_access_in_expr(ptr, eligible, promoted);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            promote_field_access_in_expr(expr, eligible, promoted)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            promote_field_access_in_expr(lhs, eligible, promoted);
            promote_field_access_in_expr(rhs, eligible, promoted);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            promote_field_access_in_expr(cond, eligible, promoted);
            promote_field_access_in_expr(then_expr, eligible, promoted);
            promote_field_access_in_expr(else_expr, eligible, promoted);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                promote_field_access_in_expr(arg, eligible, promoted);
            }
        }
        HirExpr::PtrOffset { base, .. } => promote_field_access_in_expr(base, eligible, promoted),
        HirExpr::Index { base, index, .. } => {
            promote_field_access_in_expr(base, eligible, promoted);
            promote_field_access_in_expr(index, eligible, promoted);
        }
        HirExpr::FieldAccess { base, .. } => {
            promote_field_access_in_expr(base, eligible, promoted)
        }
    }
}

/// If `ptr` is `Var(name)` (offset 0) or `PtrOffset{base: Var(name),
/// offset}`, `name` is in `eligible`, there's a debug-info field at that
/// exact offset, and `access_ty`'s byte size does not exceed that field's
/// declared size, return `(base_expr, field_name, offset)`.
///
/// The size check matters: without it, a wider read starting at a field's
/// offset (e.g. an 8-byte read of a `long` that actually spans two 4-byte
/// `int` fields packed together) would get mis-rendered as reading just
/// the first field. Unknown sizes (either side) are treated as
/// incompatible -- safer to under-promote than to guess.
fn base_field_at_offset<'a>(
    ptr: &HirExpr,
    access_ty: &NirType,
    eligible: &'a HashMap<String, &'a NirStructTypeHint>,
) -> Option<(HirExpr, &'a String, u32)> {
    let (base, offset) = match ptr {
        HirExpr::Var(_) => (ptr.clone(), 0i64),
        HirExpr::PtrOffset { base, offset } => (base.as_ref().clone(), *offset),
        _ => return None,
    };
    let HirExpr::Var(name) = &base else {
        return None;
    };
    let struct_hint = eligible.get(name)?;
    if offset < 0 {
        return None;
    }
    let offset = offset as u32;
    let field = struct_hint.fields.iter().find(|f| f.offset == offset)?;
    if field.name.is_empty() {
        return None;
    }
    let access_size = binding_byte_size(access_ty)?;
    if field.size != 0 && access_size > field.size {
        return None;
    }
    Some((base, &field.name, offset))
}

fn apply_debug_struct_field_names(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
    stats: &mut PreviewHintStats,
) {
    if context.struct_types.is_empty() {
        return;
    }
    // (base binding name, byte offset) -> real field name, collected while
    // overlaying the type-level `StructField`s below. Applied to the body
    // afterward: `FieldAccess` AST nodes carry their own `field_name`
    // string, baked in once by normalize's pointer-arithmetic recovery
    // (`ptr_arith.rs`) -- renaming only the `StructField` annotation here
    // would be invisible to the printer, which reads `field_name` straight
    // off the AST node, not the binding's type.
    let mut ast_renames: std::collections::HashMap<(String, u32), String> =
        std::collections::HashMap::new();

    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        let Some(surface_name) = binding.surface_type_name.as_deref() else {
            continue;
        };
        let Some(struct_name) = struct_base_name_for_single_pointer(surface_name) else {
            continue;
        };
        let Some(struct_hint) = context.struct_types.get(struct_name) else {
            continue;
        };
        let NirType::Ptr(inner) = &mut binding.ty else {
            continue;
        };
        let NirType::Aggregate { fields, .. } = inner.as_mut() else {
            continue;
        };
        for field in fields.iter_mut() {
            let Some(hint_field) = struct_hint
                .fields
                .iter()
                .find(|candidate| candidate.offset == field.offset)
            else {
                continue;
            };
            if hint_field.name.is_empty() || hint_field.name == field.name {
                continue;
            }
            ast_renames.insert(
                (binding.name.clone(), field.offset),
                hint_field.name.clone(),
            );
            field.name = hint_field.name.clone();
            stats.debug_struct_field_hits += 1;
        }
    }

    if !ast_renames.is_empty() {
        rewrite_field_access_names_in_stmts(&mut func.body, &ast_renames);
    }
}

/// Strip a debug-info type name down to a bare struct/union/class base name,
/// for exactly one level of pointer indirection (`Foo*`, `const Foo*`).
///
/// Multi-level pointers (`Foo**`) are deliberately rejected: the aggregate
/// whose fields we'd be naming belongs to `**binding`, not `*binding`, so
/// applying the struct's field layout at this binding's own offset set
/// would be a semantic mismatch.
fn struct_base_name_for_single_pointer(type_name: &str) -> Option<&str> {
    let mut name = type_name.trim();
    loop {
        if let Some(rest) = name.strip_prefix("const ") {
            name = rest.trim_start();
        } else if let Some(rest) = name.strip_prefix("volatile ") {
            name = rest.trim_start();
        } else {
            break;
        }
    }
    let inner = name.strip_suffix('*')?;
    if inner.is_empty() || inner.ends_with('*') {
        return None;
    }
    let inner = inner.trim();
    let inner = inner
        .strip_prefix("struct ")
        .or_else(|| inner.strip_prefix("union "))
        .or_else(|| inner.strip_prefix("class "))
        .unwrap_or(inner)
        .trim();
    if inner.is_empty() { None } else { Some(inner) }
}

fn surface_integer_return_bits(type_name: &str) -> Option<u32> {
    let normalized = type_name
        .trim()
        .trim_start_matches("const ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.contains('*') {
        return None;
    }
    match normalized.as_str() {
        "int" | "signed int" | "unsigned int" | "uint" | "dword" | "undefined4" => Some(32),
        "short" | "signed short" | "unsigned short" | "word" | "undefined2" => Some(16),
        "char" | "signed char" | "unsigned char" | "byte" | "undefined1" => Some(8),
        _ => None,
    }
}

fn elide_surface_return_casts(stmts: &mut [HirStmt], return_bits: u32) {
    for stmt in stmts {
        match stmt {
            HirStmt::Return(Some(expr)) => {
                if return_cast_is_surface_implied(expr, return_bits) {
                    let HirExpr::Cast { expr: inner, .. } = expr else {
                        continue;
                    };
                    *expr = (**inner).clone();
                }
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                elide_surface_return_casts(body, return_bits);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                elide_surface_return_casts(then_body, return_bits);
                elide_surface_return_casts(else_body, return_bits);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    elide_surface_return_casts(std::slice::from_mut(init.as_mut()), return_bits);
                }
                if let Some(update) = update {
                    elide_surface_return_casts(std::slice::from_mut(update.as_mut()), return_bits);
                }
                elide_surface_return_casts(body, return_bits);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    elide_surface_return_casts(&mut case.body, return_bits);
                }
                elide_surface_return_casts(default, return_bits);
            }
            _ => {}
        }
    }
}

fn return_cast_is_surface_implied(expr: &HirExpr, return_bits: u32) -> bool {
    let HirExpr::Cast { ty, .. } = expr else {
        return false;
    };
    matches!(ty, NirType::Int { bits, .. } if *bits == return_bits)
}

fn ensure_missing_hinted_params(
    func: &mut HirFunction,
    hints: &PreviewFunctionHints,
    stats: &mut PreviewHintStats,
) {
    let max_param = hints.param_names.len().max(
        hints
            .param_type_names
            .keys()
            .map(|index| index + 1)
            .max()
            .unwrap_or(0),
    );
    let mut added = false;
    for index in 0..max_param {
        if func
            .params
            .iter()
            .any(|p| p.origin == Some(NirBindingOrigin::ParamIndex(index)))
        {
            continue;
        }
        let default_name = format!("param_{}", index + 1);
        let name = hints
            .param_names
            .get(index)
            .map(String::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(default_name.as_str())
            .to_string();
        let surface_type_name = hints
            .param_type_names
            .get(&index)
            .map(String::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned);
        if name != default_name {
            stats.explicit_param_name_hits += 1;
        }
        if surface_type_name.is_some() {
            stats.explicit_param_type_hits += 1;
        }
        func.params.push(NirBinding {
            name,
            ty: NirType::Unknown,
            surface_type_name,
            origin: Some(NirBindingOrigin::ParamIndex(index)),
            initializer: None,
        });
        added = true;
    }
    if added {
        func.params.sort_by_key(|b| match b.origin {
            Some(NirBindingOrigin::ParamIndex(idx)) => idx,
            _ => 999,
        });
    }
}

fn stack_origin_offset(origin: Option<NirBindingOrigin>) -> Option<(i64, bool)> {
    match origin {
        Some(NirBindingOrigin::StackOffset(offset)) => Some((offset, false)),
        Some(NirBindingOrigin::HomeSlot(offset))
        | Some(NirBindingOrigin::OutgoingArgSlot(offset)) => Some((offset, false)),
        Some(NirBindingOrigin::DerivedFromStackOffset(offset)) => Some((offset, true)),
        _ => None,
    }
}

fn collect_call_type_hints(
    body: &[HirStmt],
    context: &PreviewTypeContext,
    pointer_hints: &mut HashMap<String, PreviewCallParamRule>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { rhs, .. } | HirStmt::Expr(rhs) => {
                collect_call_hints_from_expr(rhs, context, pointer_hints);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_call_hints_from_expr(va_list, context, pointer_hints);
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                collect_call_type_hints(stmts, context, pointer_hints);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_call_type_hints(&case.body, context, pointer_hints);
                }
                collect_call_type_hints(default, context, pointer_hints);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_call_hints_from_expr(cond, context, pointer_hints);
                collect_call_type_hints(then_body, context, pointer_hints);
                collect_call_type_hints(else_body, context, pointer_hints);
            }
            HirStmt::Return(Some(expr)) => {
                collect_call_hints_from_expr(expr, context, pointer_hints);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn collect_call_hints_from_expr(
    expr: &HirExpr,
    context: &PreviewTypeContext,
    pointer_hints: &mut HashMap<String, PreviewCallParamRule>,
) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            let target_addr = parse_call_target_address(target);
            for rule in &context.call_param_rules {
                if rule.callee_name != *target
                    && !matches!(rule.callee_address, Some(address) if Some(address) == target_addr)
                {
                    continue;
                }
                let Some(var_name) = args
                    .get(rule.arg_index)
                    .and_then(peel_surface_var_name_from_expr)
                else {
                    continue;
                };
                pointer_hints
                    .entry(var_name.to_string())
                    .or_insert_with(|| rule.clone());
            }
            for arg in args {
                collect_call_hints_from_expr(arg, context, pointer_hints);
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            collect_call_hints_from_expr(expr, context, pointer_hints);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_call_hints_from_expr(lhs, context, pointer_hints);
            collect_call_hints_from_expr(rhs, context, pointer_hints);
        }
        HirExpr::Index { base, index, .. } => {
            collect_call_hints_from_expr(base, context, pointer_hints);
            collect_call_hints_from_expr(index, context, pointer_hints);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_call_hints_from_expr(cond, context, pointer_hints);
            collect_call_hints_from_expr(then_expr, context, pointer_hints);
            collect_call_hints_from_expr(else_expr, context, pointer_hints);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

pub(super) fn collect_local_surface_hints(
    body: &[HirStmt],
    pointer_hints: &HashMap<String, PreviewCallParamRule>,
    func: &HirFunction,
    alias_collector: &StackAliasCollector,
    local_hints: &mut HashMap<String, String>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Deref {
                    ptr,
                    ty: NirType::Aggregate { .. } | NirType::Unknown | NirType::Ptr(_),
                } = lhs
                    && let Some(param_name) = peel_surface_var_name_from_expr(ptr)
                    && let Some(local_name) = peel_local_surface_name(rhs)
                    && let Some(rule) = pointer_hints.get(param_name)
                    && let Some(local_binding) = func
                        .locals
                        .iter()
                        .find(|binding| binding.name == local_name)
                {
                    let should_apply = match stack_origin_offset(local_binding.origin) {
                        Some((offset, _)) => rule
                            .pointee_sizes
                            .iter()
                            .any(|&size| alias_collector.might_alias(offset, size)),
                        // Synthetic/test locals may not carry stack-origin metadata.
                        None => binding_byte_size(&local_binding.ty)
                            .map(|size| rule.pointee_sizes.iter().any(|&expected| expected == size))
                            .unwrap_or(false),
                    };
                    if should_apply {
                        local_hints
                            .entry(local_name.to_string())
                            .or_insert_with(|| rule.pointee_alias.clone());
                    }
                }
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                collect_local_surface_hints(
                    stmts,
                    pointer_hints,
                    func,
                    alias_collector,
                    local_hints,
                );
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_local_surface_hints(
                        &case.body,
                        pointer_hints,
                        func,
                        alias_collector,
                        local_hints,
                    );
                }
                collect_local_surface_hints(
                    default,
                    pointer_hints,
                    func,
                    alias_collector,
                    local_hints,
                );
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_local_surface_hints(
                    then_body,
                    pointer_hints,
                    func,
                    alias_collector,
                    local_hints,
                );
                collect_local_surface_hints(
                    else_body,
                    pointer_hints,
                    func,
                    alias_collector,
                    local_hints,
                );
            }
            HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn peel_surface_var_name_from_expr(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => Some(name),
        HirExpr::Cast { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => peel_surface_var_name_from_expr(expr),
        HirExpr::PtrOffset { base, offset } if *offset == 0 => {
            peel_surface_var_name_from_expr(base)
        }
        HirExpr::FieldAccess { base, offset, .. } if *offset == 0 => {
            peel_surface_var_name_from_expr(base)
        }
        HirExpr::Index { base, index, .. } if matches!(index.as_ref(), HirExpr::Const(0, _)) => {
            peel_surface_var_name_from_expr(base)
        }
        _ => None,
    }
}

fn peel_local_surface_name(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => Some(name),
        HirExpr::Cast { expr, .. } | HirExpr::AggregateCopy { src: expr, .. } => {
            peel_local_surface_name(expr)
        }
        _ => None,
    }
}

fn find_binding_mut<'a>(func: &'a mut HirFunction, name: &str) -> Option<&'a mut NirBinding> {
    if let Some(param) = func.params.iter_mut().find(|binding| binding.name == name) {
        return Some(param);
    }
    func.locals.iter_mut().find(|binding| binding.name == name)
}

fn binding_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}
