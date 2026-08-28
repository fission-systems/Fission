//! Dual-layer print orchestration and global declaration stubs.
//!
//! Owns `render_layered_pseudocode` and print-time global/aggregate decls.
//! Global pointer recovery lives in [`super::globals`].

use super::globals::is_c_identifier;
use super::layer::{LayeredPseudocode, PrintProfile};
use super::presentation::apply_hir_presentation_with_globals;
use super::{
    HirExpr, HirFunction, HirLValue, HirStmt, MlilPreviewOptions, NirBinding, NirBindingOrigin,
    NirType, expr_type, print_hir_function, print_hir_function_with_global_names,
    print_hir_function_with_profile, print_type,
};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) fn render_hir_function_with_global_decls(
    hir: &HirFunction,
    options: &MlilPreviewOptions,
) -> String {
    render_hir_function_with_profile(hir, options, PrintProfile::Nir)
}

/// Dual NIR/HIR presentation from one structured function tree.
pub(crate) fn render_layered_pseudocode(
    hir: &HirFunction,
    options: &MlilPreviewOptions,
) -> LayeredPseudocode {
    let nir = render_hir_function_with_profile(hir, options, PrintProfile::Nir);
    let mut hir_tree = hir.clone();
    // Presentation must know which names are file-scope, or its
    // dead-assignment pass deletes stores to globals -- the write is the
    // observable effect even when this function never reads it back.
    let mut global_names = options
        .global_names
        .values()
        .filter(|name| is_c_identifier(name))
        .cloned()
        .collect::<HashSet<_>>();
    // A symbol name this function also declares as a param or local is that
    // binding, not the global it shadows.
    global_names.retain(|name| {
        !hir.params.iter().any(|binding| binding.name == *name)
            && !hir.locals.iter().any(|binding| binding.name == *name)
    });
    // A global the binary carries no symbol for still prints under its address
    // (`tmp_1a004`, `DAT_1a004`), and it is no less a global for being
    // unnamed. Added after the shadowing filter, not before: the builder
    // declares these among the locals, and dropping them for that would put
    // the write back in reach of the dead-assignment pass -- which is the
    // whole reason the set exists.
    collect_address_named_globals(hir, options, &mut global_names);
    apply_hir_presentation_with_globals(&mut hir_tree, &global_names);
    let hir_code = render_hir_function_with_profile(&hir_tree, options, PrintProfile::Hir);
    LayeredPseudocode { nir, hir: hir_code }
}

#[cfg(test)]
mod layered_tests {
    use super::*;
    use crate::midend::{HirExpr, HirStmt, NirBinding, NirBindingOrigin, NirType};

    #[test]
    fn layered_pseudocode_hir_drops_unused_home_local() {
        let func = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![
                NirBinding {
                    name: "home_0".into(),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                NirBinding {
                    name: "x".into(),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            return_type: NirType::Int {
                bits: 32,
                signed: true,
            },
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".into())))],
            ..Default::default()
        };
        let options = MlilPreviewOptions::default();
        let layered = render_layered_pseudocode(&func, &options);
        assert!(
            layered.nir.contains("home_0"),
            "NIR should keep mechanical locals:\n{}",
            layered.nir
        );
        assert!(
            !layered.hir.contains("home_0"),
            "HIR should drop unused home scaffold:\n{}",
            layered.hir
        );
        assert!(layered.hir.contains("x"));
    }
}

fn render_hir_function_with_profile(
    hir: &HirFunction,
    options: &MlilPreviewOptions,
    profile: PrintProfile,
) -> String {
    let decls = collect_referenced_global_decls(hir, options);
    let aggregate_typedefs = collect_referenced_aggregate_typedefs(hir, decls.values());
    let opaque_pcodeop_stubs = collect_opaque_pcodeop_stubs(hir);
    if decls.is_empty() && aggregate_typedefs.is_empty() && opaque_pcodeop_stubs.is_empty() {
        return print_hir_function_with_profile(hir, Some(&options.global_names), profile);
    }

    let mut rendered = String::new();
    for (size, fields) in aggregate_typedefs {
        rendered.push_str(&render_aggregate_typedef(size, &fields));
    }
    for (target, return_ty) in opaque_pcodeop_stubs {
        rendered.push_str(&render_opaque_pcodeop_stub(&target, &return_ty));
    }
    for (name, ty) in decls {
        rendered.push_str(&format!(
            "{} {};\n",
            print_global_decl_type(&ty, options),
            name
        ));
    }
    rendered.push('\n');
    rendered.push_str(&print_hir_function_with_profile(
        hir,
        Some(&options.global_names),
        profile,
    ));
    rendered
}

fn collect_opaque_pcodeop_stubs(hir: &HirFunction) -> BTreeMap<String, NirType> {
    let mut stubs = BTreeMap::new();
    collect_opaque_pcodeop_stubs_from_stmts(&hir.body, &mut stubs);
    stubs
}

fn collect_opaque_pcodeop_stubs_from_stmts(
    stmts: &[HirStmt],
    stubs: &mut BTreeMap<String, NirType>,
) {
    for stmt in stmts {
        collect_opaque_pcodeop_stubs_from_stmt(stmt, stubs);
    }
}

fn collect_opaque_pcodeop_stubs_from_stmt(stmt: &HirStmt, stubs: &mut BTreeMap<String, NirType>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_opaque_pcodeop_stubs_from_lvalue(lhs, stubs);
            collect_opaque_pcodeop_stubs_from_expr(rhs, stubs);
        }
        HirStmt::VaStart { va_list, .. }
        | HirStmt::Expr(va_list)
        | HirStmt::Return(Some(va_list)) => {
            collect_opaque_pcodeop_stubs_from_expr(va_list, stubs);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_opaque_pcodeop_stubs_from_stmts(body, stubs);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_opaque_pcodeop_stubs_from_expr(cond, stubs);
            collect_opaque_pcodeop_stubs_from_stmts(then_body, stubs);
            collect_opaque_pcodeop_stubs_from_stmts(else_body, stubs);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_opaque_pcodeop_stubs_from_stmt(init, stubs);
            }
            if let Some(cond) = cond {
                collect_opaque_pcodeop_stubs_from_expr(cond, stubs);
            }
            if let Some(update) = update {
                collect_opaque_pcodeop_stubs_from_stmt(update, stubs);
            }
            collect_opaque_pcodeop_stubs_from_stmts(body, stubs);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_opaque_pcodeop_stubs_from_expr(expr, stubs);
            for case in cases {
                collect_opaque_pcodeop_stubs_from_stmts(&case.body, stubs);
            }
            collect_opaque_pcodeop_stubs_from_stmts(default, stubs);
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_opaque_pcodeop_stubs_from_lvalue(
    lhs: &HirLValue,
    stubs: &mut BTreeMap<String, NirType>,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => collect_opaque_pcodeop_stubs_from_expr(ptr, stubs),
        HirLValue::Index { base, index, .. } => {
            collect_opaque_pcodeop_stubs_from_expr(base, stubs);
            collect_opaque_pcodeop_stubs_from_expr(index, stubs);
        }
        HirLValue::FieldAccess { base, .. } => {
            collect_opaque_pcodeop_stubs_from_expr(base, stubs);
        }
    }
}

fn collect_opaque_pcodeop_stubs_from_expr(expr: &HirExpr, stubs: &mut BTreeMap<String, NirType>) {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => {
            collect_opaque_pcodeop_stubs_from_expr(expr, stubs);
        }
        HirExpr::Binary { lhs, rhs, .. }
        | HirExpr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            collect_opaque_pcodeop_stubs_from_expr(lhs, stubs);
            collect_opaque_pcodeop_stubs_from_expr(rhs, stubs);
        }
        HirExpr::Call { target, args, ty } => {
            if target.starts_with("__pcodeop_") {
                let entry = stubs.entry(target.clone()).or_insert_with(|| ty.clone());
                *entry = merge_opaque_pcodeop_return_type(entry, ty);
            }
            for arg in args {
                collect_opaque_pcodeop_stubs_from_expr(arg, stubs);
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_opaque_pcodeop_stubs_from_expr(cond, stubs);
            collect_opaque_pcodeop_stubs_from_expr(then_expr, stubs);
            collect_opaque_pcodeop_stubs_from_expr(else_expr, stubs);
        }
    }
}

fn merge_opaque_pcodeop_return_type(existing: &NirType, next: &NirType) -> NirType {
    if existing == next || *next == NirType::Unknown {
        return existing.clone();
    }
    if *existing == NirType::Unknown {
        return next.clone();
    }
    match (existing, next) {
        (NirType::Aggregate { .. }, _) | (_, NirType::Aggregate { .. }) => existing.clone(),
        (NirType::Ptr(_), _) | (_, NirType::Ptr(_)) => existing.clone(),
        (
            NirType::Int {
                bits: existing_bits,
                signed: existing_signed,
            },
            NirType::Int {
                bits: next_bits,
                signed: next_signed,
            },
        ) => NirType::Int {
            bits: (*existing_bits).max(*next_bits),
            signed: *existing_signed || *next_signed,
        },
        (
            NirType::Float {
                bits: existing_bits,
            },
            NirType::Float { bits: next_bits },
        ) => NirType::Float {
            bits: (*existing_bits).max(*next_bits),
        },
        _ => existing.clone(),
    }
}

fn render_opaque_pcodeop_stub(target: &str, return_ty: &NirType) -> String {
    let return_type = opaque_pcodeop_return_type_name(return_ty);
    let return_stmt = opaque_pcodeop_default_return(return_ty);
    format!("static inline {return_type} {target}() {{ {return_stmt} }}\n")
}

fn opaque_pcodeop_return_type_name(return_ty: &NirType) -> String {
    match return_ty {
        NirType::Unknown => "unsigned long long".to_string(),
        _ => print_type(return_ty),
    }
}

fn opaque_pcodeop_default_return(return_ty: &NirType) -> String {
    match return_ty {
        NirType::Aggregate { size, .. } => {
            format!("fission_agg{size} out = {{0}}; return out;")
        }
        NirType::Ptr(_) => format!("return ({})0;", print_type(return_ty)),
        NirType::Float { .. } => "return 0.0;".to_string(),
        NirType::Bool => "return false;".to_string(),
        NirType::Unknown | NirType::Int { .. } => "return 0;".to_string(),
    }
}

/// size → (offset → (field_name, field_ty)). Fields are merged across all
/// references so `Index`/`FieldAccess` recovery can emit named members that
/// compile (not just `unsigned char bytes[N]` placeholders).
fn collect_referenced_aggregate_typedefs<'a>(
    hir: &'a HirFunction,
    global_decl_types: impl IntoIterator<Item = &'a NirType>,
) -> BTreeMap<u32, BTreeMap<u32, (String, NirType)>> {
    let mut aggs: BTreeMap<u32, BTreeMap<u32, (String, NirType)>> = BTreeMap::new();
    collect_aggregate_typedefs_from_type(&hir.return_type, &mut aggs);
    for binding in hir.params.iter().chain(hir.locals.iter()) {
        collect_aggregate_typedefs_from_type(&binding.ty, &mut aggs);
    }
    for ty in global_decl_types {
        collect_aggregate_typedefs_from_type(ty, &mut aggs);
    }
    collect_aggregate_typedefs_from_stmts(&hir.body, &mut aggs);
    aggs
}

fn merge_aggregate_fields(
    aggs: &mut BTreeMap<u32, BTreeMap<u32, (String, NirType)>>,
    size: u32,
    fields: &[(u32, String, NirType)],
) {
    let entry = aggs.entry(size).or_default();
    for (offset, name, ty) in fields {
        entry
            .entry(*offset)
            .or_insert_with(|| (name.clone(), ty.clone()));
    }
}

fn collect_aggregate_typedefs_from_type(
    ty: &NirType,
    aggs: &mut BTreeMap<u32, BTreeMap<u32, (String, NirType)>>,
) {
    match ty {
        NirType::Ptr(inner) => collect_aggregate_typedefs_from_type(inner, aggs),
        NirType::Aggregate { size, fields } => {
            let mapped: Vec<(u32, String, NirType)> = fields
                .iter()
                .map(|f| (f.offset, f.name.clone(), f.ty.clone()))
                .collect();
            merge_aggregate_fields(aggs, *size, &mapped);
        }
        NirType::Unknown | NirType::Bool | NirType::Int { .. } | NirType::Float { .. } => {}
    }
}

fn collect_aggregate_typedefs_from_stmts(
    stmts: &[HirStmt],
    aggs: &mut BTreeMap<u32, BTreeMap<u32, (String, NirType)>>,
) {
    for stmt in stmts {
        collect_aggregate_typedefs_from_stmt(stmt, aggs);
    }
}

fn collect_aggregate_typedefs_from_stmt(
    stmt: &HirStmt,
    aggs: &mut BTreeMap<u32, BTreeMap<u32, (String, NirType)>>,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_aggregate_typedefs_from_lvalue(lhs, aggs);
            collect_aggregate_typedefs_from_expr(rhs, aggs);
        }
        HirStmt::VaStart { va_list, .. }
        | HirStmt::Expr(va_list)
        | HirStmt::Return(Some(va_list)) => {
            collect_aggregate_typedefs_from_expr(va_list, aggs);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_aggregate_typedefs_from_stmts(body, aggs);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_aggregate_typedefs_from_expr(cond, aggs);
            collect_aggregate_typedefs_from_stmts(then_body, aggs);
            collect_aggregate_typedefs_from_stmts(else_body, aggs);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_aggregate_typedefs_from_stmt(init, aggs);
            }
            if let Some(cond) = cond {
                collect_aggregate_typedefs_from_expr(cond, aggs);
            }
            if let Some(update) = update {
                collect_aggregate_typedefs_from_stmt(update, aggs);
            }
            collect_aggregate_typedefs_from_stmts(body, aggs);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_aggregate_typedefs_from_expr(expr, aggs);
            for case in cases {
                collect_aggregate_typedefs_from_stmts(&case.body, aggs);
            }
            collect_aggregate_typedefs_from_stmts(default, aggs);
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_aggregate_typedefs_from_lvalue(
    lhs: &HirLValue,
    aggs: &mut BTreeMap<u32, BTreeMap<u32, (String, NirType)>>,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, ty } => {
            collect_aggregate_typedefs_from_type(ty, aggs);
            collect_aggregate_typedefs_from_expr(ptr, aggs);
        }
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            collect_aggregate_typedefs_from_type(elem_ty, aggs);
            collect_aggregate_typedefs_from_expr(base, aggs);
            collect_aggregate_typedefs_from_expr(index, aggs);
        }
        HirLValue::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => {
            collect_aggregate_typedefs_from_type(ty, aggs);
            collect_field_access_into_aggregate(base, field_name, *offset, ty, aggs);
            collect_aggregate_typedefs_from_expr(base, aggs);
        }
    }
}

fn collect_aggregate_typedefs_from_expr(
    expr: &HirExpr,
    aggs: &mut BTreeMap<u32, BTreeMap<u32, (String, NirType)>>,
) {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { ty, expr }
        | HirExpr::Unary { ty, expr, .. }
        | HirExpr::Load { ty, ptr: expr } => {
            collect_aggregate_typedefs_from_type(ty, aggs);
            collect_aggregate_typedefs_from_expr(expr, aggs);
        }
        HirExpr::Binary { lhs, rhs, ty, .. } => {
            collect_aggregate_typedefs_from_type(ty, aggs);
            collect_aggregate_typedefs_from_expr(lhs, aggs);
            collect_aggregate_typedefs_from_expr(rhs, aggs);
        }
        HirExpr::Call { args, ty, .. } => {
            collect_aggregate_typedefs_from_type(ty, aggs);
            for arg in args {
                collect_aggregate_typedefs_from_expr(arg, aggs);
            }
        }
        HirExpr::PtrOffset { base, .. } => collect_aggregate_typedefs_from_expr(base, aggs),
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            collect_aggregate_typedefs_from_type(elem_ty, aggs);
            collect_aggregate_typedefs_from_expr(base, aggs);
            collect_aggregate_typedefs_from_expr(index, aggs);
        }
        HirExpr::AggregateCopy { src, size } => {
            merge_aggregate_fields(aggs, *size, &[]);
            collect_aggregate_typedefs_from_expr(src, aggs);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => {
            collect_aggregate_typedefs_from_type(ty, aggs);
            collect_aggregate_typedefs_from_expr(cond, aggs);
            collect_aggregate_typedefs_from_expr(then_expr, aggs);
            collect_aggregate_typedefs_from_expr(else_expr, aggs);
        }
        HirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => {
            collect_aggregate_typedefs_from_type(ty, aggs);
            collect_field_access_into_aggregate(base, field_name, *offset, ty, aggs);
            collect_aggregate_typedefs_from_expr(base, aggs);
        }
    }
}

/// When `base[i].field` is recovered, attach `field` to the Index element
/// aggregate so the typedef has a real member (not only `bytes[N]`).
fn collect_field_access_into_aggregate(
    base: &HirExpr,
    field_name: &str,
    offset: u32,
    field_ty: &NirType,
    aggs: &mut BTreeMap<u32, BTreeMap<u32, (String, NirType)>>,
) {
    let size = match base {
        HirExpr::Index {
            elem_ty: NirType::Aggregate { size, fields },
            ..
        } => {
            let mapped: Vec<(u32, String, NirType)> = fields
                .iter()
                .map(|f| (f.offset, f.name.clone(), f.ty.clone()))
                .collect();
            merge_aggregate_fields(aggs, *size, &mapped);
            Some(*size)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => {
            // Size unknown from var alone; skip (binding type collection covers it).
            None
        }
        _ => None,
    };
    if let Some(size) = size {
        merge_aggregate_fields(
            aggs,
            size,
            &[(offset, field_name.to_string(), field_ty.clone())],
        );
    }
}

fn nir_type_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } | NirType::Float { bits } if *bits > 0 && bits % 8 == 0 => {
            Some(bits / 8)
        }
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        _ => None,
    }
}

/// Whether a member of `field_ty` would make `fission_agg{size}` contain itself.
///
/// Aggregates are named by their byte size, so a field whose own type is an
/// aggregate of the same size is spelled with the name of the struct being
/// defined -- `struct fission_agg16 { fission_agg16 field_0; }`. That type has
/// no finite size and no compiler accepts it, which takes the whole
/// translation unit with it.
fn field_would_be_self_referential(size: u32, field_ty: &NirType) -> bool {
    matches!(field_ty, NirType::Aggregate { size: inner, .. } if *inner == size)
}

fn render_aggregate_typedef(size: u32, fields: &BTreeMap<u32, (String, NirType)>) -> String {
    let fields: BTreeMap<u32, (String, NirType)> = fields
        .iter()
        .filter(|(_, (_, ty))| !field_would_be_self_referential(size, ty))
        .map(|(offset, member)| (*offset, member.clone()))
        .collect();
    let fields = &fields;
    if fields.is_empty() {
        return format!(
            "typedef struct fission_agg{size} {{ unsigned char bytes[{size}]; }} fission_agg{size};\n"
        );
    }
    let mut members = String::new();
    let mut cursor = 0u32;
    for (offset, (name, ty)) in fields {
        if *offset > cursor {
            members.push_str(&format!(
                "    unsigned char _pad_{cursor}[{}];\n",
                offset - cursor
            ));
            cursor = *offset;
        } else if *offset < cursor {
            // Overlapping/out-of-order residual — still emit the named field.
        }
        let fty = print_type(ty);
        let fsize = nir_type_byte_size(ty).unwrap_or(1);
        // Sanitize field names that might not be valid C identifiers.
        let fname = if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            name.clone()
        } else {
            format!("field_{offset:x}")
        };
        members.push_str(&format!("    {fty} {fname};\n"));
        cursor = cursor.max(*offset + fsize);
    }
    if cursor < size {
        members.push_str(&format!(
            "    unsigned char _pad_{cursor}[{}];\n",
            size - cursor
        ));
    }
    format!("typedef struct fission_agg{size} {{\n{members}}} fission_agg{size};\n")
}

/// Names spelled as an address (`tmp_1a004`, `DAT_1a004`) that the loader
/// mapped, added to the global set.
///
/// A stripped binary carries no symbol for most of its globals, so the
/// printer falls back to the address. Presentation still has to know these
/// name memory: without it, a store to one is deleted for having no reader.
fn collect_address_named_globals(
    hir: &HirFunction,
    options: &MlilPreviewOptions,
    globals: &mut HashSet<String>,
) {
    let mut consider = |name: &str| {
        let Some(rest) = name
            .strip_prefix("tmp_")
            .or_else(|| name.strip_prefix("DAT_"))
        else {
            return;
        };
        if let Ok(address) = u64::from_str_radix(rest, 16)
            && options.is_mapped_global(address)
        {
            globals.insert(name.to_string());
        }
    };
    for binding in hir.locals.iter().chain(hir.params.iter()) {
        consider(&binding.name);
    }
    collect_assigned_names(&hir.body, &mut consider);
}

fn collect_assigned_names(stmts: &[HirStmt], consider: &mut impl FnMut(&str)) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } => consider(name),
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_assigned_names(body, consider);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_assigned_names(then_body, consider);
                collect_assigned_names(else_body, consider);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_assigned_names(std::slice::from_ref(init.as_ref()), consider);
                }
                if let Some(update) = update {
                    collect_assigned_names(std::slice::from_ref(update.as_ref()), consider);
                }
                collect_assigned_names(body, consider);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_assigned_names(&case.body, consider);
                }
                collect_assigned_names(default, consider);
            }
            _ => {}
        }
    }
}

fn collect_referenced_global_decls(
    hir: &HirFunction,
    options: &MlilPreviewOptions,
) -> BTreeMap<String, NirType> {
    let global_names = options
        .global_names
        .values()
        .filter(|name| is_c_identifier(name))
        .filter(|name| {
            name.as_str() != hir.name
                && !hir.params.iter().any(|binding| binding.name == **name)
                && !hir.locals.iter().any(|binding| binding.name == **name)
        })
        .cloned()
        .collect::<HashSet<_>>();
    if global_names.is_empty() {
        return BTreeMap::new();
    }

    // Two file-scope statics in different translation units may share a
    // name, and the emitted C has one declaration for the name -- so one
    // entry wins. `options.global_names` is a `HashMap<u64, _>`, whose
    // iteration order changes from process to process, so collecting
    // straight into a name-keyed map let the winner change run to run:
    // `gzip` has `bitbuf` at 0xe13c0 sized 2 and at 0xe1670 sized 8, and six
    // of its functions alternated between `ushort bitbuf` and
    // `ulonglong bitbuf` across runs of one unchanged binary.
    //
    // Visiting in address order makes the choice stable. It is still a
    // choice between two real globals, but it is the same one every time.
    let mut by_address: Vec<(&u64, &String)> = options.global_names.iter().collect();
    by_address.sort_unstable_by_key(|(addr, _)| **addr);
    let global_decl_types = by_address
        .into_iter()
        .filter_map(|(addr, name)| {
            options
                .global_sizes
                .get(addr)
                .and_then(|size| global_decl_type_from_size(*size))
                .map(|ty| (name.clone(), ty))
        })
        .collect::<HashMap<_, _>>();
    let binding_types = hir
        .params
        .iter()
        .chain(hir.locals.iter())
        .map(|binding| (binding.name.clone(), binding.ty.clone()))
        .collect::<HashMap<_, _>>();
    let mut decls = BTreeMap::new();
    collect_global_decls_from_stmts(
        &hir.body,
        &global_names,
        &global_decl_types,
        &binding_types,
        &mut decls,
    );
    decls
}

fn collect_global_decls_from_stmts(
    stmts: &[HirStmt],
    global_names: &HashSet<String>,
    global_decl_types: &HashMap<String, NirType>,
    binding_types: &HashMap<String, NirType>,
    decls: &mut BTreeMap<String, NirType>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                collect_global_decls_from_lvalue(
                    lhs,
                    Some(infer_global_decl_expr_type(rhs, binding_types)),
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
                collect_global_decls_from_expr(
                    rhs,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                collect_global_decls_from_expr(
                    expr,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_global_decls_from_expr(
                    va_list,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::Block(body) => {
                collect_global_decls_from_stmts(
                    body,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::While { cond, body } => {
                collect_global_decls_from_expr(
                    cond,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
                collect_global_decls_from_stmts(
                    body,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::DoWhile { body, cond } => {
                collect_global_decls_from_stmts(
                    body,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
                collect_global_decls_from_expr(
                    cond,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_global_decls_from_expr(
                    expr,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
                for case in cases {
                    collect_global_decls_from_stmts(
                        &case.body,
                        global_names,
                        global_decl_types,
                        binding_types,
                        decls,
                    );
                }
                collect_global_decls_from_stmts(
                    default,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_global_decls_from_expr(
                    cond,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
                collect_global_decls_from_stmts(
                    then_body,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
                collect_global_decls_from_stmts(
                    else_body,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
            HirStmt::For {
                init, cond, update, ..
            } => {
                if let Some(init) = init {
                    collect_global_decls_from_stmts(
                        std::slice::from_ref(init.as_ref()),
                        global_names,
                        global_decl_types,
                        binding_types,
                        decls,
                    );
                }
                if let Some(cond) = cond {
                    collect_global_decls_from_expr(
                        cond,
                        global_names,
                        global_decl_types,
                        binding_types,
                        decls,
                    );
                }
                if let Some(update) = update {
                    collect_global_decls_from_stmts(
                        std::slice::from_ref(update.as_ref()),
                        global_names,
                        global_decl_types,
                        binding_types,
                        decls,
                    );
                }
            }
            HirStmt::Return(None)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn collect_global_decls_from_lvalue(
    lhs: &HirLValue,
    assigned_ty: Option<NirType>,
    global_names: &HashSet<String>,
    global_decl_types: &HashMap<String, NirType>,
    binding_types: &HashMap<String, NirType>,
    decls: &mut BTreeMap<String, NirType>,
) {
    match lhs {
        HirLValue::Var(name) if global_names.contains(name) => {
            let ty = global_decl_types
                .get(name)
                .cloned()
                .or(assigned_ty)
                .unwrap_or(NirType::Unknown);
            merge_global_decl_type(decls, name, ty);
        }
        HirLValue::Deref { ptr, .. } => collect_global_decls_from_expr(
            ptr,
            global_names,
            global_decl_types,
            binding_types,
            decls,
        ),
        HirLValue::Index { base, index, .. } => {
            collect_global_decls_from_expr(
                base,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
            collect_global_decls_from_expr(
                index,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
        }
        HirLValue::Var(_) => {}
        HirLValue::FieldAccess { base, .. } => {
            collect_global_decls_from_expr(
                base,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
        }
    }
}

fn collect_global_decls_from_expr(
    expr: &HirExpr,
    global_names: &HashSet<String>,
    global_decl_types: &HashMap<String, NirType>,
    binding_types: &HashMap<String, NirType>,
    decls: &mut BTreeMap<String, NirType>,
) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) if global_names.contains(name) => {
            let ty = global_decl_types.get(name).cloned().unwrap_or_else(|| {
                if matches!(expr, HirExpr::AddressOfGlobal(_)) {
                    NirType::Unknown
                } else {
                    infer_global_decl_expr_type(expr, binding_types)
                }
            });
            merge_global_decl_type(decls, name, ty);
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. } => {
            collect_global_decls_from_expr(
                expr,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
        }
        HirExpr::Binary { lhs, rhs, .. }
        | HirExpr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            collect_global_decls_from_expr(
                lhs,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
            collect_global_decls_from_expr(
                rhs,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_global_decls_from_expr(
                cond,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
            collect_global_decls_from_expr(
                then_expr,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
            collect_global_decls_from_expr(
                else_expr,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_global_decls_from_expr(
                    arg,
                    global_names,
                    global_decl_types,
                    binding_types,
                    decls,
                );
            }
        }
        HirExpr::PtrOffset { base, .. }
        | HirExpr::AggregateCopy { src: base, .. }
        | HirExpr::FieldAccess { base, .. } => {
            collect_global_decls_from_expr(
                base,
                global_names,
                global_decl_types,
                binding_types,
                decls,
            );
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

fn infer_global_decl_expr_type(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> NirType {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => binding_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| expr_type(expr)),
        HirExpr::Cast { ty, .. }
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Select { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. } => ty.clone(),
        HirExpr::Index { elem_ty, .. } => elem_ty.clone(),
        HirExpr::Const(_, ty) => ty.clone(),
        HirExpr::PtrOffset { .. } => expr_type(expr),
        HirExpr::FieldAccess { ty, .. } => ty.clone(),
        HirExpr::AggregateCopy { size, .. } => NirType::Aggregate {
            size: *size,
            fields: Vec::new(),
        },
    }
}

fn global_decl_type_from_size(size: u64) -> Option<NirType> {
    match size {
        1 => Some(NirType::Int {
            bits: 8,
            signed: false,
        }),
        2 => Some(NirType::Int {
            bits: 16,
            signed: false,
        }),
        4 => Some(NirType::Int {
            bits: 32,
            signed: false,
        }),
        8 => Some(NirType::Int {
            bits: 64,
            signed: false,
        }),
        size if size <= u64::from(u32::MAX) => Some(NirType::Aggregate {
            size: size as u32,
            fields: Vec::new(),
        }),
        _ => None,
    }
}

/// How an undetermined global is spelled once nothing more can be learned.
///
/// `NirType::Unknown` is [`merge_global_decl_type`]'s "not known yet" marker --
/// a real type may still replace it, which is why the merge cannot commit and
/// this does. Rendering is the last point at which the answer is still open.
///
/// Printed as `undefined`, the spelling states no width at all, and the
/// benchmark's uniform recompilation fixup resolves it to `unsigned char`. So
/// a global we go on to dereference eight bytes wide gets defined as one, and
/// the codegen that follows is not the program's. `stdout` is the case that
/// showed it: declared `undefined`, read as `ulonglong *`.
///
/// Pointer width is the default because it is never *narrower* than the access
/// that made us name the global, which is the failure that matters here. It
/// remains a default rather than a recovery -- when the size table or the
/// assignment context knows better, `merge_global_decl_type` has already taken
/// that answer and this never runs.
fn print_global_decl_type(ty: &NirType, options: &MlilPreviewOptions) -> String {
    if *ty != NirType::Unknown {
        return print_type(ty);
    }
    let bits = u32::try_from(options.pointer_size)
        .unwrap_or(8)
        .saturating_mul(8);
    print_type(&NirType::Int {
        bits: if bits == 0 { 64 } else { bits },
        signed: false,
    })
}

fn merge_global_decl_type(decls: &mut BTreeMap<String, NirType>, name: &str, ty: NirType) {
    if ty == NirType::Unknown {
        decls.entry(name.to_string()).or_insert(NirType::Unknown);
        return;
    }
    match decls.get(name) {
        Some(existing) if *existing != NirType::Unknown => {}
        _ => {
            decls.insert(name.to_string(), ty);
        }
    }
}

#[cfg(test)]
mod global_decl_tests {
    use super::*;
    use crate::midend::{CallingConvention, StructuringEngineKind};

    #[test]
    fn layered_keeps_a_write_to_an_address_named_global_declared_as_a_local() {
        // The builder declares an unnamed global among the locals -- it has no
        // symbol to name it by -- so filtering the global set against `locals`
        // (right for deciding what to *declare*, since a local shadows a
        // global) put the write back in reach of presentation's
        // dead-assignment pass: it reached NIR and vanished from the HIR
        // surface that ships. 0x2000 is inside this fixture's mapped section.
        let options = preview_options_with_global("unused");
        let int32 = NirType::Int {
            bits: 32,
            signed: true,
        };
        let hir = HirFunction {
            name: "f".to_string(),
            locals: vec![NirBinding {
                name: "tmp_2000".to_string(),
                ty: int32.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Var("tmp_2000".to_string()),
                rhs: HirExpr::Const(1, int32),
            }],
            ..HirFunction::default()
        };
        let layered = render_layered_pseudocode(&hir, &options);
        assert!(layered.nir.contains("tmp_2000 = 1;"), "{}", layered.nir);
        assert!(layered.hir.contains("tmp_2000 = 1;"), "{}", layered.hir);
    }

    fn preview_options_with_global(name: &str) -> MlilPreviewOptions {
        let mut global_names = HashMap::new();
        global_names.insert(0x2000, name.to_string());
        MlilPreviewOptions {
            pe_x64_only: false,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "ELF64".to_string(),
            image_base: 0,
            sections: vec![(0x1000, 0x3000)],
            region_linearize_structuring: false,
            force_linear_structuring: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            conservative_irreducible_fallback: false,
            global_names,
            global_sizes: HashMap::new(),
            relocation_names: HashMap::new(),
            calling_convention: CallingConvention::AArch64,
            ..Default::default()
        }
    }

    #[test]
    fn render_hir_declares_referenced_loader_global() {
        let hir = HirFunction {
            name: "store_global".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            return_type: NirType::Unknown,
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Var("math_sink".to_string()),
                rhs: HirExpr::Var("param_1".to_string()),
            }],
            ..HirFunction::default()
        };
        let rendered =
            render_hir_function_with_global_decls(&hir, &preview_options_with_global("math_sink"));

        assert!(rendered.starts_with("uint math_sink;\n\n"), "{rendered}");
        assert!(rendered.contains("math_sink = param_1;"), "{rendered}");
    }

    #[test]
    fn recover_global_symbol_accesses_follows_constant_pointer_alias() {
        let mut hir = HirFunction {
            name: "store_global_alias".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![NirBinding {
                name: "uVar1".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 32,
                    signed: false,
                })),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::TempPreserved),
                initializer: None,
            }],
            return_type: NirType::Unknown,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar1".to_string()),
                    rhs: HirExpr::Const(
                        0x2000,
                        NirType::Ptr(Box::new(NirType::Int {
                            bits: 32,
                            signed: false,
                        })),
                    ),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("uVar1".to_string())),
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    },
                    rhs: HirExpr::Var("param_1".to_string()),
                },
            ],
            ..HirFunction::default()
        };
        let options = preview_options_with_global("math_sink");

        crate::render::recover_global_symbol_accesses(&mut hir, &options);
        let rendered = render_hir_function_with_global_decls(&hir, &options);

        assert!(rendered.starts_with("uint math_sink;\n\n"), "{rendered}");
        assert!(rendered.contains("math_sink = param_1;"), "{rendered}");
        assert!(!rendered.contains("*uVar1 = param_1;"), "{rendered}");
    }

    #[test]
    fn render_hir_skips_non_identifier_global_names() {
        let hir = HirFunction {
            name: "string_user".to_string(),
            int_param_offsets: Vec::new(),
            return_type: NirType::Unknown,
            body: vec![HirStmt::Expr(HirExpr::Var("\"hello\"".to_string()))],
            ..HirFunction::default()
        };
        let rendered =
            render_hir_function_with_global_decls(&hir, &preview_options_with_global("\"hello\""));

        assert!(!rendered.starts_with("undefined \"hello\";"), "{rendered}");
    }

    #[test]
    fn render_hir_declares_referenced_aggregate_placeholder_types() {
        let hir = HirFunction {
            name: "aggregate_user".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Aggregate {
                    size: 16,
                    fields: Vec::new(),
                })),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            return_type: NirType::Unknown,
            body: vec![HirStmt::Return(None)],
            ..HirFunction::default()
        };
        let rendered =
            render_hir_function_with_global_decls(&hir, &preview_options_with_global("unused"));

        assert!(
            rendered.starts_with(
                "typedef struct fission_agg16 { unsigned char bytes[16]; } fission_agg16;\n\n"
            ),
            "{rendered}"
        );
        assert!(rendered.contains("fission_agg16 * param_1"), "{rendered}");
    }

    #[test]
    fn render_hir_emits_named_fields_for_recovered_aggregate() {
        use crate::midend::StructField;
        let hir = HirFunction {
            name: "pair_user".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 32,
                    signed: false,
                })),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            return_type: NirType::Int {
                bits: 32,
                signed: true,
            },
            body: vec![HirStmt::Return(Some(HirExpr::FieldAccess {
                base: Box::new(HirExpr::Index {
                    base: Box::new(HirExpr::Var("param_1".to_string())),
                    index: Box::new(HirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    elem_ty: NirType::Aggregate {
                        size: 8,
                        fields: vec![StructField {
                            offset: 0,
                            name: "field_0".to_string(),
                            ty: NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        }],
                    },
                }),
                field_name: "field_0".to_string(),
                offset: 0,
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }))],
            ..HirFunction::default()
        };
        let rendered =
            render_hir_function_with_global_decls(&hir, &preview_options_with_global("unused"));
        assert!(
            rendered.contains("uint field_0;"),
            "typedef must declare field_0:\n{rendered}"
        );
        assert!(rendered.contains("field_0"), "{rendered}");
        assert!(
            !rendered.contains("unsigned char bytes[8]"),
            "named-field aggregates should not use bytes-only placeholder:\n{rendered}"
        );
    }

    #[test]
    fn render_hir_declares_opaque_pcodeop_stub_for_aggregate_return() {
        let hir = HirFunction {
            name: "userop_aggregate".to_string(),
            int_param_offsets: Vec::new(),
            locals: vec![NirBinding {
                name: "xVar30".to_string(),
                ty: NirType::Aggregate {
                    size: 16,
                    fields: Vec::new(),
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::TempPreserved),
                initializer: None,
            }],
            return_type: NirType::Unknown,
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Var("xVar30".to_string()),
                rhs: HirExpr::Call {
                    target: "__pcodeop_294".to_string(),
                    args: Vec::new(),
                    ty: NirType::Aggregate {
                        size: 16,
                        fields: Vec::new(),
                    },
                },
            }],
            ..HirFunction::default()
        };
        let rendered =
            render_hir_function_with_global_decls(&hir, &preview_options_with_global("unused"));

        assert!(
            rendered.starts_with(
                "typedef struct fission_agg16 { unsigned char bytes[16]; } fission_agg16;\n"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "static inline fission_agg16 __pcodeop_294() { fission_agg16 out = {0}; return out; }"
            ),
            "{rendered}"
        );
        assert!(rendered.contains("xVar30 = __pcodeop_294();"), "{rendered}");
    }
}
