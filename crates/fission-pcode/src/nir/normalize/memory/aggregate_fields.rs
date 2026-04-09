/// Aggregate field layout recovery pass.
///
/// After pointer-arithmetic recovery (`ptr_arith.rs`) has converted raw
/// `IntAdd(ptr, k)` expressions into `PtrOffset { base: Var(x), offset: k }`
/// nodes, this pass examines every `PtrOffset` whose base variable has type
/// `Ptr(Aggregate { .. })` and accumulates the complete set of byte offsets
/// that are actually accessed.  It then:
///
/// 1. **Builds an offset → field-type map** for each aggregate variable by
///    scanning `Load { ptr: PtrOffset }` and store-lvalue `Deref { ptr:
///    PtrOffset }` sites.
/// 2. **Annotates** the `NirType::Aggregate` with a sorted `Vec<StructField>`,
///    giving each field the name `field_{offset:x}` (e.g. `field_8`).
/// 3. **Updates the printer** indirectly: `printer.rs` checks for a non-empty
///    `fields` vec and emits `base->field_8` instead of the raw byte-offset form.
///
/// ### Algorithm
///
/// The algorithm is purely data-flow / use-site driven:
///
/// - Only constant-offset `PtrOffset` nodes are considered (variable offsets
///   produce `Index` nodes, handled separately).
/// - When two accesses at the same offset have different type widths, the
///   wider type wins (conservative union-field model; no Rust-level union is
///   emitted, the smaller access simply becomes a nested cast at the use-site).
/// - The pass is monotone: it only *adds* fields to a previously-empty
///   `fields` vec.  Re-running is safe.
///
/// This pass is architecture-agnostic and has no binary-specific thresholds.
use super::super::*;
use std::collections::HashMap;

/// Map: variable name → (offset → best NirType for that field).
type OffsetMap = HashMap<String, HashMap<u32, NirType>>;

/// Return the byte width of a type (0 = unknown).
fn type_bits_bytes(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => bits / 8,
        NirType::Ptr(_) => 8,
        NirType::Float { bits } => bits / 8,
        NirType::Aggregate { size, .. } => *size,
        NirType::Unknown => 0,
    }
}

/// Merge a new candidate `NirType` into the best-so-far type for an offset.
/// The wider / more-concrete type wins.
fn merge_field_ty(current: &NirType, candidate: &NirType) -> NirType {
    if *current == NirType::Unknown {
        return candidate.clone();
    }
    if *candidate == NirType::Unknown {
        return current.clone();
    }
    let cur_w = type_bits_bytes(current);
    let cand_w = type_bits_bytes(candidate);
    if cand_w > cur_w {
        candidate.clone()
    } else {
        current.clone()
    }
}

/// Build a map from variable_name → { offset → best_field_type } by walking
/// all expressions and statements in the function body.
fn collect_offsets(
    stmts: &[HirStmt],
    agg_vars: &HashMap<String, NirType>, // name → Aggregate type
    out: &mut OffsetMap,
) {
    for stmt in stmts {
        collect_offsets_stmt(stmt, agg_vars, out);
    }
}

fn record_access(var_name: &str, offset: u32, access_ty: NirType, out: &mut OffsetMap) {
    let entry = out.entry(var_name.to_owned()).or_default();
    let best = entry.entry(offset).or_insert(NirType::Unknown);
    *best = merge_field_ty(best, &access_ty);
}

fn collect_offsets_expr(expr: &HirExpr, agg_vars: &HashMap<String, NirType>, out: &mut OffsetMap) {
    match expr {
        // Load through a PtrOffset: the base is a pointer-to-aggregate, the
        // load type tells us the field type.
        HirExpr::Load {
            ptr:
                box_expr @ _,
            ty: load_ty,
        } => {
            if let HirExpr::PtrOffset { base, offset } = box_expr.as_ref() {
                if let HirExpr::Var(name) = base.as_ref() {
                    if agg_vars.contains_key(name.as_str()) && *offset >= 0 {
                        record_access(name, *offset as u32, load_ty.clone(), out);
                    }
                }
            }
            collect_offsets_expr(box_expr.as_ref(), agg_vars, out);
        }
        HirExpr::PtrOffset { base, .. } => collect_offsets_expr(base, agg_vars, out),
        HirExpr::Cast { expr, .. } => collect_offsets_expr(expr, agg_vars, out),
        HirExpr::Unary { expr, .. } => collect_offsets_expr(expr, agg_vars, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_offsets_expr(lhs, agg_vars, out);
            collect_offsets_expr(rhs, agg_vars, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_offsets_expr(arg, agg_vars, out);
            }
        }
        HirExpr::Index { base, index, .. } => {
            collect_offsets_expr(base, agg_vars, out);
            collect_offsets_expr(index, agg_vars, out);
        }
        HirExpr::AggregateCopy { src, .. } => collect_offsets_expr(src, agg_vars, out),
        _ => {}
    }
}

fn collect_offsets_lvalue(lhs: &HirLValue, agg_vars: &HashMap<String, NirType>, out: &mut OffsetMap) {
    match lhs {
        HirLValue::Deref { ptr, ty: store_ty } => {
            if let HirExpr::PtrOffset { base, offset } = ptr.as_ref() {
                if let HirExpr::Var(name) = base.as_ref() {
                    if agg_vars.contains_key(name.as_str()) && *offset >= 0 {
                        record_access(name, *offset as u32, store_ty.clone(), out);
                    }
                }
            }
            collect_offsets_expr(ptr, agg_vars, out);
        }
        HirLValue::Index { base, index, .. } => {
            collect_offsets_expr(base, agg_vars, out);
            collect_offsets_expr(index, agg_vars, out);
        }
        HirLValue::Var(_) => {}
    }
}

fn collect_offsets_stmt(stmt: &HirStmt, agg_vars: &HashMap<String, NirType>, out: &mut OffsetMap) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_offsets_lvalue(lhs, agg_vars, out);
            collect_offsets_expr(rhs, agg_vars, out);
        }
        HirStmt::Expr(expr) => collect_offsets_expr(expr, agg_vars, out),
        HirStmt::Return(Some(expr)) => collect_offsets_expr(expr, agg_vars, out),
        HirStmt::Block(body) => collect_offsets(body, agg_vars, out),
        HirStmt::If { cond, then_body, else_body } => {
            collect_offsets_expr(cond, agg_vars, out);
            collect_offsets(then_body, agg_vars, out);
            collect_offsets(else_body, agg_vars, out);
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            collect_offsets_expr(cond, agg_vars, out);
            collect_offsets(body, agg_vars, out);
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init { collect_offsets_stmt(i, agg_vars, out); }
            if let Some(c) = cond { collect_offsets_expr(c, agg_vars, out); }
            if let Some(u) = update { collect_offsets_stmt(u, agg_vars, out); }
            collect_offsets(body, agg_vars, out);
        }
        HirStmt::Switch { expr, cases, default } => {
            collect_offsets_expr(expr, agg_vars, out);
            for case in cases {
                collect_offsets(&case.body, agg_vars, out);
            }
            collect_offsets(default, agg_vars, out);
        }
        _ => {}
    }
}

/// Collect all variables (locals + params) whose type is
/// `Ptr(Aggregate { .. })`.  Returns a map from the variable name to the inner
/// `Aggregate` type.
fn collect_agg_ptr_vars(func: &HirFunction) -> HashMap<String, NirType> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter_map(|b| match &b.ty {
            NirType::Ptr(inner) => match inner.as_ref() {
                NirType::Aggregate { .. } => Some((b.name.clone(), *inner.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Apply aggregate field layout recovery to a function.
///
/// Returns `true` if any `NirType::Aggregate` had fields added to it.
pub(crate) fn apply_aggregate_fields_pass(func: &mut HirFunction) -> bool {
    let agg_ptr_vars = collect_agg_ptr_vars(func);
    if agg_ptr_vars.is_empty() {
        return false;
    }

    let mut offset_map: OffsetMap = HashMap::new();
    collect_offsets(&func.body, &agg_ptr_vars, &mut offset_map);

    if offset_map.is_empty() {
        return false;
    }

    let mut changed = false;

    // Update each NirBinding that is Ptr(Aggregate { .. }) with discovered fields.
    let update_binding = |binding: &mut NirBinding, offset_map: &OffsetMap| -> bool {
        let NirType::Ptr(inner) = &mut binding.ty else { return false; };
        let NirType::Aggregate { fields, .. } = inner.as_mut() else { return false; };
        if !fields.is_empty() {
            return false; // already populated
        }
        let Some(offsets) = offset_map.get(&binding.name) else { return false; };
        if offsets.is_empty() { return false; }

        let mut new_fields: Vec<StructField> = offsets
            .iter()
            .map(|(&offset, ty)| StructField {
                offset,
                ty: ty.clone(),
                name: format!("field_{offset:x}"),
            })
            .collect();
        new_fields.sort_by_key(|f| f.offset);
        *fields = new_fields;
        true
    };

    for binding in func.locals.iter_mut() {
        changed |= update_binding(binding, &offset_map);
    }
    for binding in func.params.iter_mut() {
        changed |= update_binding(binding, &offset_map);
    }

    changed
}
