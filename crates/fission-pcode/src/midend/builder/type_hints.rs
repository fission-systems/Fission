use super::*;
use crate::midend::abstract_location::AbstractStackSlot;
use crate::midend::support::pcode_util::InputMetatype;
use fission_midend_core::util::var_rename::{
    rename_var_in_expr, rename_vars_in_stmts, rewrite_field_access_names_in_stmts,
};
use tracing::trace_span;

pub(super) struct StackAliasCollector {
    alias_boundaries: Vec<(AbstractStackSlot, u64)>,
}

#[derive(Debug, Clone)]
struct SurfaceArrayType {
    element: NirType,
    element_size: u32,
    count: u32,
}

#[derive(Debug, Clone)]
struct DebugArrayRegion {
    base_name: String,
    stack_offset: i64,
    size: u32,
    element: NirType,
    element_size: u32,
}

/// Apply trusted debug-info array shapes before normalize can discard the
/// physical stack extent.  A byte store at the end of a source array is a
/// separate stack view in raw PreHIR; once the normalizer has treated it as a
/// short-lived scalar temporary, the relation to the array is no longer
/// recoverable from the final HIR alone.
pub(super) fn apply_pre_hir_debug_array_hints(
    func: &mut PreHirFunction,
    context: &PreviewTypeContext,
    debug_cfa_stack_offset_bias: Option<i64>,
) -> usize {
    let Some(hints) = &context.function_hints else {
        return 0;
    };

    let mut reserved_names = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();
    let mut renames = Vec::new();
    let mut arrays = HashMap::default();
    let mut array_regions = Vec::new();
    let mut applied = 0;

    for binding in &mut func.locals {
        let Some((offset, is_derived)) = stack_origin_offset(binding.origin) else {
            continue;
        };
        if is_derived {
            continue;
        }
        let debug_offset = match hints.debug_stack_offset_base {
            NirStackOffsetBase::BuilderFrame => Some(offset),
            NirStackOffsetBase::CallFrameCfa => {
                debug_cfa_stack_offset_bias.and_then(|bias| offset.checked_sub(bias))
            }
        };
        let type_name = debug_offset
            .and_then(|offset| hints.debug_stack_local_type_names.get(&offset))
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                hints
                    .stack_local_type_names
                    .get(&offset)
                    .filter(|name| !name.trim().is_empty())
            });
        let Some(type_name) = type_name else {
            continue;
        };
        let Some(array) = parse_surface_array_type(type_name) else {
            continue;
        };
        let Some(size) = array.element_size.checked_mul(array.count) else {
            continue;
        };
        let fields = (0..array.count)
            .map(|index| StructField {
                offset: index.saturating_mul(array.element_size),
                ty: array.element.clone(),
                name: format!("element_{index}"),
            })
            .collect();
        binding.ty = NirType::Aggregate { size, fields };
        binding.surface_type_name = Some(type_name.trim().to_string());
        let base_name = binding.name.clone();
        arrays.insert(base_name.clone(), array.element.clone());
        array_regions.push(DebugArrayRegion {
            base_name,
            stack_offset: offset,
            size,
            element: array.element.clone(),
            element_size: array.element_size,
        });
        applied += 1;

        let new_name = debug_offset
            .and_then(|offset| hints.debug_stack_local_names.get(&offset))
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                hints
                    .stack_local_names
                    .get(&offset)
                    .filter(|name| !name.trim().is_empty())
            })
            .map(|name| name.trim().to_string());
        let Some(new_name) = new_name else {
            continue;
        };
        if new_name == binding.name || reserved_names.contains(&new_name) {
            continue;
        }
        reserved_names.remove(&binding.name);
        reserved_names.insert(new_name.clone());
        renames.push((binding.name.clone(), new_name.clone()));
        binding.name = new_name;
    }

    if applied == 0 {
        return 0;
    }

    // The machine store is still a scalar operation even though its stack
    // owner is now known to be an array.  Keep that operation explicit as a
    // typed memory write.
    rewrite_pre_hir_array_base_scalar_stores(&mut func.body, &arrays);

    // A compiler may materialize the last byte (or another element) through
    // a separate scalar stack view.  The normalizer normally converts such a
    // write-only view to a Temp before its stack merge pass runs.  Do this
    // rewrite while the builder still has the original stack provenance, and
    // use the normalize owner's traversal so the same rule handles calls,
    // loads, pointer escapes, and nested statements consistently.
    let array_base_names = array_regions
        .iter()
        .map(|region| region.base_name.as_str())
        .collect::<HashSet<_>>();
    let mut scalar_views = Vec::new();
    for binding in &func.locals {
        if array_base_names.contains(binding.name.as_str()) || binding.initializer.is_some() {
            continue;
        }
        let Some((scalar_offset, is_derived)) = stack_origin_offset(binding.origin) else {
            continue;
        };
        if is_derived {
            continue;
        }
        let Some(scalar_size) = binding_byte_size(&binding.ty) else {
            continue;
        };
        for region in &array_regions {
            if scalar_size != region.element_size {
                continue;
            }
            let Some(relative) = scalar_offset.checked_sub(region.stack_offset) else {
                continue;
            };
            if relative < 0
                || relative % i64::from(region.element_size) != 0
                || relative + i64::from(scalar_size) > i64::from(region.size)
            {
                continue;
            }
            scalar_views.push((
                binding.name.clone(),
                region.base_name.clone(),
                relative,
                region.element.clone(),
            ));
            break;
        }
    }

    let mut recovered_scalar_names = HashSet::default();
    for (scalar_name, base_name, offset, element_ty) in scalar_views {
        fission_midend_normalize::recovery::rewrite_stack_view_as_array_element(
            &mut func.body,
            &scalar_name,
            &base_name,
            offset,
            element_ty,
        );
        recovered_scalar_names.insert(scalar_name);
    }
    if !recovered_scalar_names.is_empty() {
        func.locals
            .retain(|binding| !recovered_scalar_names.contains(&binding.name));
    }

    if !renames.is_empty() {
        fission_midend_prehir::rename_vars_in_stmts(&mut func.body, &renames);
        for binding in &mut func.locals {
            if let Some(initializer) = binding.initializer.as_mut() {
                fission_midend_prehir::util::rename_vars_in_expr(initializer, &renames);
            }
        }
    }
    applied
}

fn parse_surface_array_type(surface: &str) -> Option<SurfaceArrayType> {
    let surface = surface.trim();
    let open = surface.rfind('[')?;
    let count = surface[open + 1..].strip_suffix(']')?.trim().parse().ok()?;
    if count == 0 || surface[..open].contains('[') || surface[..open].contains('*') {
        return None;
    }
    let base = surface_scalar_type(surface[..open].trim())?;
    let element_size = binding_byte_size(&base)?;
    Some(SurfaceArrayType {
        element: base,
        element_size,
        count,
    })
}

fn surface_scalar_type(surface: &str) -> Option<NirType> {
    let words = surface
        .split_whitespace()
        .filter(|word| !matches!(*word, "const" | "volatile" | "restrict"))
        .collect::<Vec<_>>();
    let name = words.join(" ");
    let (bits, signed) = match name.as_str() {
        "char" | "signed char" | "int8_t" => (8, true),
        "unsigned char" | "uint8_t" | "uchar" => (8, false),
        "short" | "signed short" | "signed short int" | "int16_t" => (16, true),
        "unsigned short" | "unsigned short int" | "uint16_t" | "ushort" => (16, false),
        "int" | "signed" | "signed int" | "int32_t" => (32, true),
        "unsigned" | "unsigned int" | "uint32_t" | "uint" => (32, false),
        "long long" | "signed long long" | "signed long long int" | "int64_t" => (64, true),
        "unsigned long long" | "unsigned long long int" | "uint64_t" | "ulong" => (64, false),
        "float" => return Some(NirType::Float { bits: 32 }),
        "double" => return Some(NirType::Float { bits: 64 }),
        _ => return None,
    };
    Some(NirType::Int { bits, signed })
}

fn rewrite_pre_hir_array_base_scalar_stores(
    body: &mut [PreHirStmt],
    arrays: &HashMap<String, NirType>,
) {
    for stmt in body {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } if arrays.contains_key(name)
                && !matches!(
                    fission_midend_prehir::expr_type(rhs),
                    NirType::Aggregate { .. }
                ) =>
            {
                let ty = fission_midend_prehir::expr_type(rhs);
                if matches!(
                    ty,
                    NirType::Int { .. } | NirType::Bool | NirType::Float { .. }
                ) {
                    let array_name = name.clone();
                    *stmt = PreHirStmt::Assign {
                        lhs: PreHirLValue::Deref {
                            ptr: Box::new(PreHirExpr::AddressOfLocal(array_name)),
                            ty,
                        },
                        rhs: rhs.clone(),
                    };
                }
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
            PreHirStmt::Block(stmts)
            | PreHirStmt::While { body: stmts, .. }
            | PreHirStmt::DoWhile { body: stmts, .. }
            | PreHirStmt::For { body: stmts, .. } => {
                let nested = std::rc::Rc::make_mut(stmts);
                rewrite_pre_hir_array_base_scalar_stores(nested.as_mut_slice(), arrays)
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    let nested = std::rc::Rc::make_mut(&mut case.body);
                    rewrite_pre_hir_array_base_scalar_stores(nested.as_mut_slice(), arrays);
                }
                let nested = std::rc::Rc::make_mut(default);
                rewrite_pre_hir_array_base_scalar_stores(nested.as_mut_slice(), arrays);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                let nested_then = std::rc::Rc::make_mut(then_body);
                rewrite_pre_hir_array_base_scalar_stores(nested_then.as_mut_slice(), arrays);
                let nested_else = std::rc::Rc::make_mut(else_body);
                rewrite_pre_hir_array_base_scalar_stores(nested_else.as_mut_slice(), arrays);
            }
        }
    }
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
    debug_cfa_stack_offset_bias: Option<i64>,
) -> PreviewHintStats {
    let _hints = trace_span!("preview_type_hints", fn_name = %func.name).entered();
    let mut stats =
        apply_function_name_hints(func, context, register_origins, debug_cfa_stack_offset_bias);
    apply_debug_pointer_type_aliases(func, context);
    stats.local_surface_hits += propagate_pointer_surface_aliases(func);
    preserve_surface_pointer_byte_offsets(func);
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

    decay_array_addresses_at_typed_calls(func, context);

    stats
}

/// Restore pointer semantics that are carried by a named debug typedef.
///
/// A machine-level return or binding can be represented as an integer with
/// pointer width even when DWARF says its source type is a typedef whose target
/// is a pointer. The alias spelling alone is not evidence (`DWORD_PTR` and
/// `PIMAGE_SECTION_HEADER` are both opaque names), so the loader transports the
/// typedef target and depth in `pointer_type_aliases`. Apply that fact to the
/// internal type before rendering; this keeps the source alias in the emitted
/// declaration while preventing the project prelude from defining it as an
/// integer.
fn apply_debug_pointer_type_aliases(func: &mut HirFunction, context: &PreviewTypeContext) {
    if context.pointer_type_aliases.is_empty() {
        return;
    }

    let return_type = func
        .surface_return_type_name
        .as_deref()
        .and_then(|surface| pointer_alias_for_surface(surface, context))
        .and_then(|alias| nir_type_for_pointer_alias(alias, context));
    if let Some(return_type) = return_type
        && pointer_alias_type_should_replace(&func.return_type, &return_type)
    {
        func.return_type = return_type;
    }

    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        let Some(surface) = binding.surface_type_name.as_deref() else {
            continue;
        };
        let Some(alias) = pointer_alias_for_surface(surface, context) else {
            continue;
        };
        let Some(recovered) = nir_type_for_pointer_alias(alias, context) else {
            continue;
        };
        if pointer_alias_type_should_replace(&binding.ty, &recovered) {
            binding.ty = recovered;
        }
    }
}

fn pointer_alias_for_surface<'a>(
    surface: &str,
    context: &'a PreviewTypeContext,
) -> Option<&'a NirPointerTypeAlias> {
    let mut alias_name = None;
    for word in surface.split_whitespace() {
        if matches!(word, "const" | "volatile" | "restrict") {
            continue;
        }
        if word.contains('*') || alias_name.is_some() {
            return None;
        }
        alias_name = Some(word);
    }
    context.pointer_type_aliases.get(alias_name?)
}

fn nir_type_for_pointer_alias(
    alias: &NirPointerTypeAlias,
    context: &PreviewTypeContext,
) -> Option<NirType> {
    if alias.pointer_depth == 0 {
        return None;
    }
    let pointee = context
        .struct_types
        .get(&alias.pointee_name)
        .map(|hint| NirType::Aggregate {
            size: hint.size,
            fields: hint
                .fields
                .iter()
                .map(|field| StructField {
                    offset: field.offset,
                    ty: NirType::Unknown,
                    name: field.name.clone(),
                })
                .collect(),
        })
        .unwrap_or(NirType::Unknown);
    let mut recovered = pointee;
    for _ in 0..alias.pointer_depth {
        recovered = NirType::Ptr(Box::new(recovered));
    }
    Some(recovered)
}

fn pointer_alias_type_should_replace(existing: &NirType, recovered: &NirType) -> bool {
    match (existing, recovered) {
        (NirType::Ptr(existing_inner), NirType::Ptr(recovered_inner))
            if matches!(recovered_inner.as_ref(), NirType::Aggregate { .. }) =>
        {
            !matches!(existing_inner.as_ref(), NirType::Aggregate { .. })
        }
        (NirType::Ptr(_), NirType::Ptr(_)) => false,
        _ => true,
    }
}

fn decay_array_addresses_at_typed_calls(func: &mut HirFunction, context: &PreviewTypeContext) {
    let arrays = func
        .params
        .iter()
        .chain(func.locals.iter())
        .filter_map(|binding| {
            parse_surface_array_type(binding.surface_type_name.as_deref()?)
                .map(|array| (binding.name.clone(), array.element_size))
        })
        .collect::<HashMap<_, _>>();
    if arrays.is_empty() {
        return;
    }
    decay_array_addresses_in_stmts(&mut func.body, context, &arrays);
}

fn decay_array_addresses_in_stmts(
    body: &mut [HirStmt],
    context: &PreviewTypeContext,
    arrays: &HashMap<String, u32>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                decay_array_addresses_in_lvalue(lhs, context, arrays);
                decay_array_addresses_in_expr(rhs, context, arrays);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                decay_array_addresses_in_expr(expr, context, arrays)
            }
            HirStmt::VaStart { va_list, .. } => {
                decay_array_addresses_in_expr(va_list, context, arrays)
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => {
                decay_array_addresses_in_stmts(stmts, context, arrays)
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                decay_array_addresses_in_expr(expr, context, arrays);
                for case in cases {
                    decay_array_addresses_in_stmts(&mut case.body, context, arrays);
                }
                decay_array_addresses_in_stmts(default, context, arrays);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                decay_array_addresses_in_expr(cond, context, arrays);
                decay_array_addresses_in_stmts(then_body, context, arrays);
                decay_array_addresses_in_stmts(else_body, context, arrays);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn decay_array_addresses_in_lvalue(
    lvalue: &mut HirLValue,
    context: &PreviewTypeContext,
    arrays: &HashMap<String, u32>,
) {
    match lvalue {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => decay_array_addresses_in_expr(ptr, context, arrays),
        HirLValue::Index { base, index, .. } => {
            decay_array_addresses_in_expr(base, context, arrays);
            decay_array_addresses_in_expr(index, context, arrays);
        }
        HirLValue::FieldAccess { base, .. } => decay_array_addresses_in_expr(base, context, arrays),
    }
}

fn decay_array_addresses_in_expr(
    expr: &mut HirExpr,
    context: &PreviewTypeContext,
    arrays: &HashMap<String, u32>,
) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            for (index, arg) in args.iter_mut().enumerate() {
                decay_array_addresses_in_expr(arg, context, arrays);
                let Some(name) = (match arg {
                    HirExpr::AddressOfLocal(name) => Some(name.clone()),
                    _ => None,
                }) else {
                    continue;
                };
                let Some(array_size) = arrays.get(&name).copied() else {
                    continue;
                };
                if call_expects_pointer_to_element(context, target, index, array_size) {
                    *arg = HirExpr::Var(name);
                }
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            decay_array_addresses_in_expr(expr, context, arrays)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            decay_array_addresses_in_expr(lhs, context, arrays);
            decay_array_addresses_in_expr(rhs, context, arrays);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            decay_array_addresses_in_expr(cond, context, arrays);
            decay_array_addresses_in_expr(then_expr, context, arrays);
            decay_array_addresses_in_expr(else_expr, context, arrays);
        }
        HirExpr::Index { base, index, .. } => {
            decay_array_addresses_in_expr(base, context, arrays);
            decay_array_addresses_in_expr(index, context, arrays);
        }
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
    }
}

fn call_expects_pointer_to_element(
    context: &PreviewTypeContext,
    target: &str,
    arg_index: usize,
    array_size: u32,
) -> bool {
    if let Some(summary) = context.call_prototype_summaries.get(target) {
        if let Some(surface) = summary
            .param_surface_type_names
            .get(arg_index)
            .and_then(Option::as_deref)
            && surface.contains('*')
            && surface_pointee_byte_size(surface) == Some(array_size)
        {
            return true;
        }
        if let Some(Some(NirCallPointerPointee::Int { bits, .. })) =
            summary.param_pointer_pointees.get(arg_index)
            && bits / 8 == array_size
        {
            return true;
        }
    }
    context.call_param_rules.iter().any(|rule| {
        rule.callee_name == target
            && rule.arg_index == arg_index
            && rule.pointee_sizes.contains(&array_size)
    })
}

fn apply_function_name_hints(
    func: &mut HirFunction,
    context: &PreviewTypeContext,
    register_origins: &HashMap<String, (u64, u32)>,
    debug_cfa_stack_offset_bias: Option<i64>,
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
        let debug_offset = match hints.debug_stack_offset_base {
            NirStackOffsetBase::BuilderFrame => Some(offset),
            NirStackOffsetBase::CallFrameCfa => {
                debug_cfa_stack_offset_bias.and_then(|bias| offset.checked_sub(bias))
            }
        };
        let new_name = debug_offset
            .and_then(|debug_offset| hints.debug_stack_local_names.get(&debug_offset))
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                hints
                    .stack_local_names
                    .get(&offset)
                    .filter(|name| !name.trim().is_empty())
            });
        let Some(new_name) = new_name else {
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

    if !hints.register_local_names.is_empty() || !hints.register_local_type_names.is_empty() {
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

            if binding.surface_type_name.is_none()
                && let Some(type_name) = hints.register_local_type_names.get(register_offset)
            {
                let type_name = type_name.trim();
                if !type_name.is_empty() {
                    binding.surface_type_name = Some(type_name.to_string());
                    stats.explicit_local_type_hits += 1;
                }
            }

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
        // A local's initializer is printed as part of its declaration and is
        // not reached by a walk of the body. Renaming a parameter without it
        // left the old spelling in the one place the new name is never
        // declared: `int *slot_3c = (uint *)(param_1 + 60);` in a function
        // whose parameter is now `pImageBase`.
        for binding in &mut func.locals {
            if let Some(initializer) = binding.initializer.as_mut() {
                rename_var_in_expr(initializer, &renames);
            }
        }
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
        let debug_offset = match hints.debug_stack_offset_base {
            NirStackOffsetBase::BuilderFrame => Some(offset),
            NirStackOffsetBase::CallFrameCfa => {
                debug_cfa_stack_offset_bias.and_then(|bias| offset.checked_sub(bias))
            }
        };
        let type_name = debug_offset
            .and_then(|debug_offset| hints.debug_stack_local_type_names.get(&debug_offset))
            .filter(|type_name| !type_name.trim().is_empty())
            .or_else(|| {
                hints
                    .stack_local_type_names
                    .get(&offset)
                    .filter(|type_name| !type_name.trim().is_empty())
            });
        let Some(type_name) = type_name else {
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
        if return_type_name == "void" {
            drop_return_values(&mut func.body);
        } else {
            recover_tail_call_return(&mut func.body);
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
                init, update, body, ..
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

/// Propagate a trusted pointer declaration through address-preserving local
/// aliases without changing the observed internal value type.
///
/// Packed loads are allowed to make a cursor's internal type wider than the
/// element type named by its source declaration.  That internal type is still
/// needed by the printer for an explicit wide load, but it must not make a
/// copied/offset cursor itself print as a wide-element pointer.  This helper
/// therefore overlays only `surface_type_name` and accepts only expressions
/// whose pointer provenance is unambiguous.
///
/// A local may be assigned more than once when every assignment preserves the
/// same pointer source.  This matters for loop cursors such as
/// `cursor = arr; cursor = cursor + 4;`: the self-referential update is safe
/// once the initial source assignment establishes the surface type.  Unknown
/// pointer sources, memory loads, pointer/ptr arithmetic, and non-pointer
/// assignments keep the local unannotated.
fn propagate_pointer_surface_aliases(func: &mut HirFunction) -> usize {
    let mut assignments: HashMap<String, Vec<HirExpr>> = HashMap::default();
    collect_pointer_assignments_in_stmts(&func.body, &mut assignments);
    if assignments.is_empty() {
        return 0;
    }

    let mut propagated = 0;
    for _ in 0..=func.locals.len() {
        let known_surfaces = pointer_surface_bindings(func);
        let pointer_bindings = pointer_binding_names(func);
        let mut updates = Vec::new();

        for (name, rhses) in &assignments {
            let Some(binding) = func.locals.iter().find(|binding| binding.name == *name) else {
                continue;
            };
            if binding.surface_type_name.is_some() || !is_pointer_type(&binding.ty) {
                continue;
            }
            let Some(surface_type_name) =
                infer_pointer_alias_surface(name, rhses, &known_surfaces, &pointer_bindings)
            else {
                continue;
            };
            updates.push((name.clone(), surface_type_name));
        }

        if updates.is_empty() {
            break;
        }
        for (name, surface_type_name) in updates {
            if let Some(binding) = func.locals.iter_mut().find(|binding| binding.name == name)
                && binding.surface_type_name.is_none()
            {
                binding.surface_type_name = Some(surface_type_name);
                propagated += 1;
            }
        }
    }
    propagated
}

fn collect_pointer_assignments_in_stmts(
    body: &[HirStmt],
    assignments: &mut HashMap<String, Vec<HirExpr>>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } => {
                assignments
                    .entry(name.clone())
                    .or_default()
                    .push(rhs.clone());
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
            HirStmt::Block(stmts) => collect_pointer_assignments_in_stmts(stmts, assignments),
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_pointer_assignments_in_stmts(body, assignments)
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    collect_pointer_assignments_in_stmts(
                        std::slice::from_ref(init_stmt.as_ref()),
                        assignments,
                    );
                }
                if let Some(update_stmt) = update {
                    collect_pointer_assignments_in_stmts(
                        std::slice::from_ref(update_stmt.as_ref()),
                        assignments,
                    );
                }
                collect_pointer_assignments_in_stmts(body, assignments);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_pointer_assignments_in_stmts(&case.body, assignments);
                }
                collect_pointer_assignments_in_stmts(default, assignments);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_pointer_assignments_in_stmts(then_body, assignments);
                collect_pointer_assignments_in_stmts(else_body, assignments);
            }
        }
    }
}

fn pointer_surface_bindings(func: &HirFunction) -> HashMap<String, String> {
    func.params
        .iter()
        .chain(func.locals.iter())
        .filter_map(|binding| {
            is_pointer_type(&binding.ty)
                .then(|| binding.surface_type_name.as_ref())
                .flatten()
                .map(|surface_type_name| (binding.name.clone(), surface_type_name.clone()))
        })
        .collect()
}

fn pointer_binding_names(func: &HirFunction) -> HashSet<String> {
    func.params
        .iter()
        .chain(func.locals.iter())
        .filter(|binding| is_pointer_type(&binding.ty))
        .map(|binding| binding.name.clone())
        .collect()
}

fn infer_pointer_alias_surface(
    destination: &str,
    rhses: &[HirExpr],
    known_surfaces: &HashMap<String, String>,
    pointer_bindings: &HashSet<String>,
) -> Option<String> {
    let mut candidate: Option<&str> = None;
    for rhs in rhses {
        if let Some(surface_type_name) = pointer_surface_from_expr(rhs, known_surfaces) {
            if let Some(previous) = candidate
                && previous != surface_type_name
            {
                return None;
            }
            candidate = Some(surface_type_name);
        }
        if !is_pointer_preserving_alias_expr(rhs, destination, known_surfaces, pointer_bindings) {
            return None;
        }
    }
    candidate.map(str::to_owned)
}

fn pointer_surface_from_expr<'a>(
    expr: &HirExpr,
    known_surfaces: &'a HashMap<String, String>,
) -> Option<&'a str> {
    match expr {
        HirExpr::Var(name) => known_surfaces.get(name).map(String::as_str),
        HirExpr::Cast { ty, expr } if is_pointer_type(ty) => {
            pointer_surface_from_expr(expr, known_surfaces)
        }
        HirExpr::PtrOffset { base, .. } => pointer_surface_from_expr(base, known_surfaces),
        HirExpr::Binary {
            op: HirBinaryOp::Add | HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            let lhs_surface = pointer_surface_from_expr(lhs, known_surfaces);
            let rhs_surface = pointer_surface_from_expr(rhs, known_surfaces);
            match (lhs_surface, rhs_surface) {
                (Some(surface), None) => Some(surface),
                (None, Some(surface)) => Some(surface),
                _ => None,
            }
        }
        HirExpr::Index { base, .. } => pointer_surface_from_expr(base, known_surfaces),
        _ => None,
    }
}

fn is_pointer_preserving_alias_expr(
    expr: &HirExpr,
    destination: &str,
    known_surfaces: &HashMap<String, String>,
    pointer_bindings: &HashSet<String>,
) -> bool {
    match expr {
        HirExpr::Var(name) => name == destination || known_surfaces.contains_key(name),
        HirExpr::Cast { ty, expr } if is_pointer_type(ty) => {
            is_pointer_preserving_alias_expr(expr, destination, known_surfaces, pointer_bindings)
        }
        HirExpr::PtrOffset { base, .. } => {
            is_pointer_preserving_alias_expr(base, destination, known_surfaces, pointer_bindings)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add | HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            let lhs_pointer = is_definitely_pointer_expr(lhs, destination, pointer_bindings);
            let rhs_pointer = is_definitely_pointer_expr(rhs, destination, pointer_bindings);
            lhs_pointer != rhs_pointer
                && if lhs_pointer {
                    is_pointer_preserving_alias_expr(
                        lhs,
                        destination,
                        known_surfaces,
                        pointer_bindings,
                    )
                } else {
                    is_pointer_preserving_alias_expr(
                        rhs,
                        destination,
                        known_surfaces,
                        pointer_bindings,
                    )
                }
        }
        HirExpr::Index { base, index, .. } => {
            !is_definitely_pointer_expr(index, destination, pointer_bindings)
                && is_pointer_preserving_alias_expr(
                    base,
                    destination,
                    known_surfaces,
                    pointer_bindings,
                )
        }
        _ => false,
    }
}

fn is_definitely_pointer_expr(
    expr: &HirExpr,
    destination: &str,
    pointer_bindings: &HashSet<String>,
) -> bool {
    match expr {
        HirExpr::Var(name) => name == destination || pointer_bindings.contains(name),
        HirExpr::AddressOfGlobal(_) | HirExpr::AddressOfLocal(_) => true,
        HirExpr::Cast { ty, .. } => is_pointer_type(ty),
        HirExpr::PtrOffset { .. } | HirExpr::Index { .. } => true,
        HirExpr::Binary {
            op: HirBinaryOp::Add | HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            is_definitely_pointer_expr(lhs, destination, pointer_bindings)
                != is_definitely_pointer_expr(rhs, destination, pointer_bindings)
        }
        _ => false,
    }
}

fn is_pointer_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Ptr(_))
}

/// Keep raw machine-address arithmetic byte-scaled after a surface pointer
/// alias has been recovered.
///
/// The normalize pointer-arithmetic pass deliberately leaves
/// `Add(pointer, index * stride)` alone when the observed internal pointee
/// width does not match `stride`.  That is the correct choice for a packed
/// load: changing the internal pointer type would make the load itself
/// narrower.  Once a trusted surface declaration is available, however, C
/// pointer arithmetic would apply the declaration's element scale to the
/// same expression.  Cast only the pointer operand to a byte pointer in that
/// unresolved, scaled form so the emitted C preserves the p-code address
/// calculation while the binding keeps its observed wide load type.
///
/// This is intentionally narrower than a general pointer-arithmetic rewrite:
/// normalized `Index`/element-pointer forms are untouched, constant
/// `PtrOffset` nodes already carry byte units, and unscaled arithmetic is not
/// guessed.  The rule is driven by surface pointer provenance and a visible
/// integer multiplication, never by an ISA, function, or address.
fn preserve_surface_pointer_byte_offsets(func: &mut HirFunction) {
    let known_surfaces = pointer_surface_bindings(func);
    if known_surfaces.is_empty() {
        return;
    }
    preserve_surface_pointer_byte_offsets_in_stmts(&mut func.body, &known_surfaces);
}

fn preserve_surface_pointer_byte_offsets_in_stmts(
    body: &mut [HirStmt],
    known_surfaces: &HashMap<String, String>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                preserve_surface_pointer_byte_offsets_in_lvalue(lhs, known_surfaces);
                preserve_surface_pointer_byte_offsets_in_expr(rhs, known_surfaces);
            }
            HirStmt::VaStart { va_list, .. } => {
                preserve_surface_pointer_byte_offsets_in_expr(va_list, known_surfaces)
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                preserve_surface_pointer_byte_offsets_in_expr(expr, known_surfaces)
            }
            HirStmt::Block(stmts) | HirStmt::While { body: stmts, .. } => {
                preserve_surface_pointer_byte_offsets_in_stmts(stmts, known_surfaces)
            }
            HirStmt::DoWhile { body, cond } => {
                preserve_surface_pointer_byte_offsets_in_stmts(body, known_surfaces);
                preserve_surface_pointer_byte_offsets_in_expr(cond, known_surfaces);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    preserve_surface_pointer_byte_offsets_in_stmts(
                        std::slice::from_mut(init.as_mut()),
                        known_surfaces,
                    );
                }
                if let Some(cond) = cond {
                    preserve_surface_pointer_byte_offsets_in_expr(cond, known_surfaces);
                }
                if let Some(update) = update {
                    preserve_surface_pointer_byte_offsets_in_stmts(
                        std::slice::from_mut(update.as_mut()),
                        known_surfaces,
                    );
                }
                preserve_surface_pointer_byte_offsets_in_stmts(body, known_surfaces);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                preserve_surface_pointer_byte_offsets_in_expr(expr, known_surfaces);
                for case in cases {
                    preserve_surface_pointer_byte_offsets_in_stmts(&mut case.body, known_surfaces);
                }
                preserve_surface_pointer_byte_offsets_in_stmts(default, known_surfaces);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                preserve_surface_pointer_byte_offsets_in_expr(cond, known_surfaces);
                preserve_surface_pointer_byte_offsets_in_stmts(then_body, known_surfaces);
                preserve_surface_pointer_byte_offsets_in_stmts(else_body, known_surfaces);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn preserve_surface_pointer_byte_offsets_in_lvalue(
    lvalue: &mut HirLValue,
    known_surfaces: &HashMap<String, String>,
) {
    match lvalue {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(ptr, known_surfaces)
        }
        HirLValue::Index { base, index, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(base, known_surfaces);
            preserve_surface_pointer_byte_offsets_in_expr(index, known_surfaces);
        }
        HirLValue::FieldAccess { base, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(base, known_surfaces)
        }
    }
}

fn preserve_surface_pointer_byte_offsets_in_expr(
    expr: &mut HirExpr,
    known_surfaces: &HashMap<String, String>,
) {
    match expr {
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(expr, known_surfaces)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(lhs, known_surfaces);
            preserve_surface_pointer_byte_offsets_in_expr(rhs, known_surfaces);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            preserve_surface_pointer_byte_offsets_in_expr(cond, known_surfaces);
            preserve_surface_pointer_byte_offsets_in_expr(then_expr, known_surfaces);
            preserve_surface_pointer_byte_offsets_in_expr(else_expr, known_surfaces);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                preserve_surface_pointer_byte_offsets_in_expr(arg, known_surfaces);
            }
        }
        HirExpr::Load { ptr, .. } | HirExpr::PtrOffset { base: ptr, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(ptr, known_surfaces)
        }
        HirExpr::Index { base, index, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(base, known_surfaces);
            preserve_surface_pointer_byte_offsets_in_expr(index, known_surfaces);
        }
        HirExpr::FieldAccess { base, .. } => {
            preserve_surface_pointer_byte_offsets_in_expr(base, known_surfaces)
        }
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
    }

    let mut cast_lhs = false;
    let mut cast_rhs = false;
    if let HirExpr::Binary { op, lhs, rhs, .. } = expr {
        let lhs_surface = pointer_surface_from_expr(lhs, known_surfaces);
        let rhs_surface = pointer_surface_from_expr(rhs, known_surfaces);
        match op {
            HirBinaryOp::Add => {
                if lhs_surface.is_some()
                    && rhs_surface.is_none()
                    && surface_pointer_needs_byte_address(lhs_surface, rhs)
                {
                    cast_lhs = true;
                } else if rhs_surface.is_some()
                    && lhs_surface.is_none()
                    && surface_pointer_needs_byte_address(rhs_surface, lhs)
                {
                    cast_rhs = true;
                }
            }
            HirBinaryOp::Sub => {
                if lhs_surface.is_some()
                    && rhs_surface.is_none()
                    && surface_pointer_needs_byte_address(lhs_surface, rhs)
                {
                    cast_lhs = true;
                }
            }
            _ => {}
        }
    }

    if cast_lhs {
        if let HirExpr::Binary { lhs, .. } = expr {
            *lhs = Box::new(byte_pointer_cast((**lhs).clone()));
        }
    } else if cast_rhs {
        if let HirExpr::Binary { rhs, .. } = expr {
            *rhs = Box::new(byte_pointer_cast((**rhs).clone()));
        }
    }
}

fn surface_pointer_needs_byte_address(surface_type_name: Option<&str>, offset: &HirExpr) -> bool {
    surface_type_name
        .and_then(surface_pointee_byte_size)
        .is_some_and(|size| size > 1)
        && contains_scaled_integer_offset(offset)
}

fn contains_scaled_integer_offset(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } if matches!(lhs.as_ref(), HirExpr::Const(_, _))
            || matches!(rhs.as_ref(), HirExpr::Const(_, _)) =>
        {
            true
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => contains_scaled_integer_offset(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            contains_scaled_integer_offset(lhs) || contains_scaled_integer_offset(rhs)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            contains_scaled_integer_offset(cond)
                || contains_scaled_integer_offset(then_expr)
                || contains_scaled_integer_offset(else_expr)
        }
        HirExpr::Index { base, index, .. } => {
            contains_scaled_integer_offset(base) || contains_scaled_integer_offset(index)
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. } => contains_scaled_integer_offset(ptr),
        HirExpr::Call { .. }
        | HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => false,
    }
}

fn byte_pointer_cast(expr: HirExpr) -> HirExpr {
    HirExpr::Cast {
        ty: NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        })),
        expr: Box::new(expr),
    }
}

fn surface_pointee_byte_size(declaration: &str) -> Option<u32> {
    let (base, stars) = declaration.split_once('*')?;
    if stars.contains('*') {
        return Some(8);
    }
    let base = base
        .split_whitespace()
        .filter(|word| !matches!(*word, "const" | "volatile"))
        .collect::<Vec<_>>()
        .join(" ");
    match base.as_str() {
        "char" | "signed char" | "unsigned char" | "int8_t" | "uint8_t" | "uchar" => Some(1),
        "short" | "signed short" | "signed short int" | "unsigned short" | "unsigned short int"
        | "int16_t" | "uint16_t" | "ushort" => Some(2),
        "int" | "signed" | "signed int" | "unsigned" | "unsigned int" | "int32_t" | "uint32_t"
        | "uint" => Some(4),
        "long long"
        | "signed long long"
        | "signed long long int"
        | "unsigned long long"
        | "unsigned long long int"
        | "int64_t"
        | "uint64_t"
        | "ulong" => Some(8),
        "float" => Some(4),
        "double" => Some(8),
        _ => None,
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
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let index_expr = HirExpr::Index {
                base: base.clone(),
                index: index.clone(),
                elem_ty: elem_ty.clone(),
            };
            if let Some((field_base, field_name, offset)) =
                base_field_at_offset(&index_expr, elem_ty, eligible)
            {
                *lvalue = HirLValue::FieldAccess {
                    base: Box::new(field_base),
                    field_name: field_name.clone(),
                    offset,
                    ty: elem_ty.clone(),
                };
                if let HirLValue::FieldAccess { base, .. } = lvalue
                    && let HirExpr::Var(name) = base.as_ref()
                {
                    promoted.insert(name.clone());
                }
                return;
            }
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
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
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
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            let index_expr = HirExpr::Index {
                base: base.clone(),
                index: index.clone(),
                elem_ty: elem_ty.clone(),
            };
            if let Some((field_base, field_name, offset)) =
                base_field_at_offset(&index_expr, elem_ty, eligible)
            {
                if let HirExpr::Var(name) = &field_base {
                    promoted.insert(name.clone());
                }
                *expr = HirExpr::FieldAccess {
                    base: Box::new(field_base),
                    field_name: field_name.clone(),
                    offset,
                    ty: elem_ty.clone(),
                };
                return;
            }
            promote_field_access_in_expr(base, eligible, promoted);
            promote_field_access_in_expr(index, eligible, promoted);
        }
        HirExpr::FieldAccess { base, .. } => promote_field_access_in_expr(base, eligible, promoted),
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
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            // Normalization can represent a scalar load at byte offset 4 as
            // `Index(p, 1)` while `p` is still typed as `uint *`.  Once a
            // debug hint proves that `p` is a record, keep the old scalar
            // element width for this one constant index; using the newly
            // promoted record stride would incorrectly mean `p[1]`.
            if matches!(elem_ty, NirType::Aggregate { .. }) {
                return None;
            }
            let HirExpr::Const(index, _) = index.as_ref() else {
                return None;
            };
            let elem_size = i64::from(binding_byte_size(elem_ty)?);
            if *index < 0 {
                return None;
            }
            let offset = index.checked_mul(elem_size)?;
            (base.as_ref().clone(), offset)
        }
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
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
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
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) | HirExpr::AddressOfLocal(name) => {
            Some(name)
        }
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
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) | HirExpr::AddressOfLocal(name) => {
            Some(name)
        }
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

/// Refine one type with operand-side metatype evidence, or `None` to leave it.
///
/// This is the `inputTypeLocal` half of Ghidra's type seeding. The output half
/// (`pcode_output_type_from_size`) types the *result* of an op; this types the
/// values it *reads*, which is where parameters and locals actually appear:
/// `INT_SLESS` says its operands are signed, `FLOAT_ADD` says its operands are
/// floating point, and none of that used to reach them.
///
/// Deliberately conservative about what it will overwrite:
///
/// * A pointer, aggregate, array or float type came from stronger evidence (a
///   memory access shape, a library prototype), so operand metatype never
///   touches it -- an `INT_AND` on a pointer is a masked pointer, not an
///   integer.
/// * Signedness is refined only on an integer of the same width; changing the
///   width from a metatype would be guessing.
/// * `Bool` only where the storage is one byte. Ghidra's `BOOL_*` ops are
///   defined on any width, but a wider operand is a flag word and printing it
///   `bool` loses that.
pub(super) fn refine_with_operand_metatype(ty: &NirType, meta: InputMetatype) -> Option<NirType> {
    match (ty, meta) {
        (NirType::Int { bits, signed }, InputMetatype::Signed) if !*signed => Some(NirType::Int {
            bits: *bits,
            signed: true,
        }),
        (NirType::Int { bits, signed }, InputMetatype::Unsigned) if *signed => Some(NirType::Int {
            bits: *bits,
            signed: false,
        }),
        (NirType::Int { bits, .. }, InputMetatype::Float) if matches!(bits, 32 | 64 | 80) => {
            Some(NirType::Float { bits: *bits })
        }
        (NirType::Int { bits: 8, .. }, InputMetatype::Bool) => Some(NirType::Bool),
        (NirType::Unknown, InputMetatype::Signed) => Some(NirType::Int {
            bits: 32,
            signed: true,
        }),
        (NirType::Unknown, InputMetatype::Unsigned) => Some(NirType::Int {
            bits: 32,
            signed: false,
        }),
        (NirType::Unknown, InputMetatype::Bool) => Some(NirType::Bool),
        _ => None,
    }
}

/// Turn a trailing `f(x); return;` into `return f(x);`.
///
/// `return find_substring(str, "ol");` compiles to `call find_substring` and
/// then straight to `ret`: nothing writes the return register afterwards,
/// because the call already left the value there. Return-value recovery scans
/// *after* the last call -- correctly, since a call clobbers that register --
/// finds nothing, and emits a bare `return`. The value is dropped.
///
/// It cannot be recovered at that layer, because `void f() { g(); }` compiles
/// to the same instructions. Only the declared return type separates them, and
/// that arrives here with the debug info. So this runs only when the function
/// is known to return something, and only on the statement immediately before
/// the return.
fn recover_tail_call_return(body: &mut Vec<HirStmt>) {
    // Nested tails (a return inside an if arm) reach the same shape through
    // their own block, so recurse rather than only looking at the top level.
    for stmt in body.iter_mut() {
        match stmt {
            HirStmt::Block(inner)
            | HirStmt::While { body: inner, .. }
            | HirStmt::DoWhile { body: inner, .. } => recover_tail_call_return(inner),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                recover_tail_call_return(then_body);
                recover_tail_call_return(else_body);
            }
            _ => {}
        }
    }
    let Some(return_idx) = body
        .iter()
        .rposition(|stmt| matches!(stmt, HirStmt::Return(None)))
    else {
        return;
    };
    let Some(call_idx) = return_idx.checked_sub(1) else {
        return;
    };
    // Only a call whose result nothing else consumes: an expression statement.
    // A call already assigned to something has its value accounted for.
    let HirStmt::Expr(HirExpr::Call { .. }) = &body[call_idx] else {
        return;
    };
    let HirStmt::Expr(call) = body.remove(call_idx) else {
        unreachable!("checked immediately above")
    };
    body[call_idx] = HirStmt::Return(Some(call));
}

/// Strip the value from every `return` in a function that returns nothing.
///
/// Return-value recovery reads the exit register, which a void function leaves
/// holding whatever it last computed -- `void reverse_string(char*, size_t)`
/// came out returning an `rax` that one path never writes. Once debug info
/// says the function is void, that value is not a result and saying so is
/// wrong in both directions: it invents a return, and it makes the emitted C
/// invalid.
///
/// A call keeps its place as a statement. Dropping it would drop its effects,
/// which is a different and worse mistake than printing its value.
fn drop_return_values(body: &mut Vec<HirStmt>) {
    for stmt in body.iter_mut() {
        match stmt {
            HirStmt::Block(inner)
            | HirStmt::While { body: inner, .. }
            | HirStmt::DoWhile { body: inner, .. } => drop_return_values(inner),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                drop_return_values(then_body);
                drop_return_values(else_body);
            }
            _ => {}
        }
    }
    let mut index = 0;
    while index < body.len() {
        let HirStmt::Return(Some(expr)) = &body[index] else {
            index += 1;
            continue;
        };
        if matches!(expr, HirExpr::Call { .. }) {
            let HirStmt::Return(Some(call)) =
                std::mem::replace(&mut body[index], HirStmt::Return(None))
            else {
                unreachable!("checked immediately above")
            };
            body.insert(index, HirStmt::Expr(call));
            index += 2;
            continue;
        }
        body[index] = HirStmt::Return(None);
        index += 1;
    }
}
