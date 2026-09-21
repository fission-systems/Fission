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
/// 1. `scan_def_types(body)` — build a `HashMap<name, PreHirExpr>` from the first
///    assignment to each variable anywhere in the body tree.
/// 2. `infer_type_for_binding(name, defs, visited)` — recursively derive the
///    type of a named binding.  If the definition is `Var(other)` we follow the
///    chain (cycle-protected with a `HashSet`); otherwise we call `expr_type`.
/// 3. `apply_type_inference_pass(func)` — for every `PreHirBinding` whose `ty` is
///    `Unknown` _and_ whose `surface_type_name` is unset, replace `ty` with the
///    inferred result.  Also re-derives `PreHirFunction.return_type` for the common
///    `return <var>;` pattern that previously always produced `undefined`.
///
/// This pass is binary-independent: it only propagates types
/// that are already embedded in typed sub-expressions (Const, Cast, Binary, …).
use crate::prelude::*;
use crate::{HashMap, HashSet};

mod return_type;

mod pointer_roles;
use pointer_roles::{
    apply_address_contributor_param_pointer_types, apply_address_role_pointer_override_for_locals,
    apply_pointer_compare_peer_override_for_locals, apply_scalar_role_override_for_pointer_locals,
    apply_transitive_address_pointer_override_for_locals,
    promote_signed_neutral_word_load_pointees, rewrite_scalar_zero_alias_assignments,
};
pub(super) use pointer_roles::{
    pointer_compare_peer_promotions, transitive_address_pointer_locals,
    transitive_address_pointer_locals_with_dependencies,
};
/// Collect the first assignment expression type for each named variable in the
/// body.  We store `(NirType, Option<String>)` where the Option carries the
/// target variable name when the RHS is a `Var` — so we can chain-resolve later.
///
/// Storing owned types (not references) avoids lifetime conflicts when we
/// later mutate `func` to apply the inferred types.
fn scan_def_types(stmts: &[PreHirStmt], defs: &mut HashMap<String, DefEntry>) {
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

fn scan_def_types_stmt(stmt: &PreHirStmt, defs: &mut HashMap<String, DefEntry>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => {
            if defs.contains_key(name.as_str()) {
                // Only record the first definition (near-SSA assumption).
                return;
            }
            let entry = match rhs {
                PreHirExpr::Var(src) => DefEntry::Alias(src.clone()),
                PreHirExpr::Cast { ty, expr } if matches!(expr.as_ref(), PreHirExpr::Var(_)) => {
                    let PreHirExpr::Var(source) = expr.as_ref() else {
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
        PreHirStmt::Block(stmts) => scan_def_types(stmts, defs),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            scan_def_types(then_body, defs);
            scan_def_types(else_body, defs);
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            scan_def_types(body, defs)
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                scan_def_types(&case.body, defs);
            }
            scan_def_types(default, defs);
        }
        _ => {}
    }
}

fn collect_value_provenance_vars(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            collect_value_provenance_vars(expr, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_value_provenance_vars(lhs, out);
            collect_value_provenance_vars(rhs, out);
        }
        PreHirExpr::Select {
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
        PreHirExpr::Load { .. }
        | PreHirExpr::Index { .. }
        | PreHirExpr::FieldAccess { .. }
        | PreHirExpr::Call { .. }
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
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

fn collect_known_binding_types(func: &PreHirFunction) -> HashMap<String, NirType> {
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

/// Apply the type inference pass to a function.
///
/// - Updates `PreHirBinding.ty` for all `locals` and `params` that have
///   `ty == Unknown` and no `surface_type_name` override.
/// - Re-derives `PreHirFunction.return_type` when it is `Unknown`.
///
/// Returns `true` when at least one binding/return type was strengthened.
pub fn apply_type_inference_pass(func: &mut PreHirFunction) -> bool {
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
    return_type::rederive_return_type(
        &mut func.return_type,
        &func.surface_return_type_name,
        &func.body,
        &defs,
        &known_binding_types,
    );
    changed |= func.return_type != prev_return_type;

    changed |= return_type::narrow_zero_extended_return_width(func, &defs, &known_binding_types);
    changed |= return_type::promote_sub32_abi_return_width(func, &defs, &known_binding_types);
    changed |= return_type::promote_narrow_returned_temps_for_abi_return(func);
    changed |= return_type::strip_zero_extended_casts_to_declared_return_width(func);
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
    changed |= apply_transitive_address_pointer_override_for_locals(func, &dependencies);
    changed |= promote_signed_neutral_word_load_pointees(func, &dependencies);

    changed
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    fn make_assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_owned()),
            rhs,
        }
    }

    fn make_binding(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_owned(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_param(name: &str, ty: NirType) -> PreHirBinding {
        PreHirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }
    }

    fn make_func(
        locals: Vec<PreHirBinding>,
        body: Vec<PreHirStmt>,
        return_type: NirType,
    ) -> PreHirFunction {
        PreHirFunction {
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
            PreHirExpr::Const(
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
            PreHirExpr::Const(
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
            make_assign("x", PreHirExpr::Const(1, NirType::Bool)),
            make_assign("y", PreHirExpr::Var("x".to_owned())),
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
            make_assign("a", PreHirExpr::Var("b".to_owned())),
            make_assign("b", PreHirExpr::Var("a".to_owned())),
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
                PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            ),
            PreHirStmt::Return(Some(PreHirExpr::Var("x".to_owned()))),
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
        let body = vec![PreHirStmt::Return(Some(PreHirExpr::Const(
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
            PreHirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(PreHirExpr::Var("y".to_owned())),
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
            PreHirExpr::Const(
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
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Mod,
                lhs: Box::new(PreHirExpr::Var("acc".to_owned())),
                rhs: Box::new(PreHirExpr::Const(
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
            PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("ptr".to_owned())),
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
        zero.initializer = Some(PreHirExpr::Const(0, zero.ty.clone()));
        let mut scalar = make_binding("acc");
        scalar.ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let mut func = make_func(
            vec![zero, scalar],
            vec![make_assign("acc", PreHirExpr::Var("rax".to_string()))],
            NirType::Unknown,
        );

        assert!(super::apply_type_inference_pass(&mut func));
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Const(0, NirType::Int { bits: 64, signed: false }),
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
        zero.initializer = Some(PreHirExpr::Const(0, zero.ty.clone()));
        let mut ptr = make_binding("ptr");
        ptr.ty = zero.ty.clone();
        let mut func = make_func(
            vec![zero, ptr],
            vec![make_assign("ptr", PreHirExpr::Var("rax".to_string()))],
            NirType::Unknown,
        );

        assert!(!super::rewrite_scalar_zero_alias_assignments(&mut func));
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Var(src),
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
            make_assign("buf", PreHirExpr::Var("param_1".to_string())),
            make_assign("len", PreHirExpr::Var("param_2".to_string())),
            make_assign(
                "end",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("buf".to_string())),
                    rhs: Box::new(PreHirExpr::Var("len".to_string())),
                    ty: ptr_ty.clone(),
                },
            ),
            make_assign(
                "byte",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("buf".to_string())),
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
            make_assign("p", PreHirExpr::Var("param_1".to_string())),
            make_assign(
                "byte",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("p".to_string())),
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
            make_assign("cursor_word", PreHirExpr::Var("buffer_param".to_string())),
            make_assign(
                "end_word",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("buffer_param".to_string())),
                    rhs: Box::new(PreHirExpr::Var("length_param".to_string())),
                    ty: u32_ty.clone(),
                },
            ),
            make_assign(
                "cursor",
                PreHirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("cursor_word".to_string())),
                },
            ),
            make_assign(
                "byte",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("cursor".to_string())),
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
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("buffer".to_string())),
                    rhs: Box::new(PreHirExpr::Var("length".to_string())),
                    ty: u32_ty.clone(),
                },
            ),
            make_assign("cursor", PreHirExpr::Var("buffer".to_string())),
            make_assign(
                "cursor_word",
                PreHirExpr::Cast {
                    ty: u32_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("cursor".to_string())),
                },
            ),
            make_assign(
                "cursor_word",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("cursor_word".to_string())),
                    rhs: Box::new(PreHirExpr::Const(1, u32_ty.clone())),
                    ty: u32_ty.clone(),
                },
            ),
            make_assign(
                "cursor",
                PreHirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("cursor_word".to_string())),
                },
            ),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("cursor".to_string())),
                    ty: u8_ty,
                },
                rhs: PreHirExpr::Const(0, u32_ty.clone()),
            },
            PreHirStmt::DoWhile {
                body: Vec::new().into(),
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Ne,
                    lhs: Box::new(PreHirExpr::Var("end_word".to_string())),
                    rhs: Box::new(PreHirExpr::Var("cursor_word".to_string())),
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
                PreHirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("state_param".to_string())),
                        ty: u8_ty.clone(),
                    }),
                },
            ),
            make_assign(
                "shared",
                PreHirExpr::Cast {
                    ty: ptr_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("buffer_param".to_string())),
                },
            ),
            make_assign(
                "byte",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("shared".to_string())),
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
            make_assign("base_alias", PreHirExpr::Var("base_param".into())),
            make_assign("cursor", PreHirExpr::Var("index".into())),
            make_assign(
                "cursor",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("cursor".into())),
                    rhs: Box::new(PreHirExpr::Var("base_alias".into())),
                    ty: u64_ty.clone(),
                },
            ),
            make_assign(
                "value",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("cursor".into())),
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
            make_assign("limit", PreHirExpr::Var("param_1".to_string())),
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Lt,
                    lhs: Box::new(PreHirExpr::Var("i".to_string())),
                    rhs: Box::new(PreHirExpr::Var("limit".to_string())),
                    ty: NirType::Bool,
                },
                then_body: Vec::new().into(),
                else_body: Vec::new().into(),
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
            )]
            .into_iter()
            .collect::<HashMap<_, _>>(),
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
        let mut func = PreHirFunction {
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
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_owned()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(PreHirExpr::Var("param_2".to_owned())),
                    }))]
                    .into(),
                    else_body: vec![].into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        let changed = super::apply_type_inference_pass(&mut func);
        assert!(changed);
        assert_eq!(func.return_type, u32_ty);
        let PreHirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("expected if");
        };
        assert!(matches!(
            &then_body[0],
            PreHirStmt::Return(Some(PreHirExpr::Var(name))) if name == "param_2"
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", u32_ty.clone())],
            locals: vec![],
            return_type: u32_ty,
            surface_return_type_name: None,
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: u64_ty,
                expr: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("param_1".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(
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
            PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", u32_ty)],
            locals: vec![],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Return(Some(PreHirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("param_1".to_owned())),
                })),
                PreHirStmt::Return(Some(PreHirExpr::Var("unknown_wide".to_owned()))),
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param("param_1", i32_ty.clone())],
            locals: vec![PreHirBinding {
                name: "tmp".to_owned(),
                ty: u32_ty,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            return_type: u64_ty,
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_owned()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var(
                        "param_1".to_owned(),
                    )))]
                    .into(),
                    else_body: vec![].into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("tmp".to_owned()))),
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
        let local = |name: &str, ty: NirType| PreHirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        let mut func = PreHirFunction {
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
                make_assign("rdi", PreHirExpr::Var("param_1".to_owned())),
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_owned()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("rdi".to_owned())))]
                        .into(),
                    else_body: vec![].into(),
                },
                make_assign("ret32", PreHirExpr::Var("wide_acc".to_owned())),
                make_assign("ret64", PreHirExpr::Var("ret32".to_owned())),
                PreHirStmt::Return(Some(PreHirExpr::Var("ret64".to_owned()))),
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
        let local = |name: &str, ty: NirType| PreHirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        };
        let mut func = PreHirFunction {
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
                make_assign("rdi", PreHirExpr::Var("param_1".to_owned())),
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_owned()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("rdi".to_owned())))]
                        .into(),
                    else_body: vec![].into(),
                },
                make_assign("ret32", PreHirExpr::Var("wide_acc".to_owned())),
                make_assign("ret64", PreHirExpr::Var("ret32".to_owned())),
                PreHirStmt::Return(Some(PreHirExpr::Var("ret64".to_owned()))),
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
        let mut func = PreHirFunction {
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
                PreHirStmt::If {
                    cond: PreHirExpr::Var("c1".to_owned()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        4294967295, // 0xFFFFFFFF = -1 as u32
                        u64_ty.clone(),
                    )))]
                    .into(),
                    else_body: vec![].into(),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Var("c2".to_owned()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        4294967294, // 0xFFFFFFFE = -2 as u32
                        u64_ty.clone(),
                    )))]
                    .into(),
                    else_body: vec![].into(),
                },
                // Simulate: return (ulonglong)(uint)(int)(param_1 + param_2)
                // The outer u64 ZExt cast is what the decompiler produces for x86-64.
                PreHirStmt::Return(Some(PreHirExpr::Cast {
                    ty: u64_ty.clone(),
                    expr: Box::new(PreHirExpr::Cast {
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                        expr: Box::new(PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(PreHirExpr::Var("param_1".to_owned())),
                            rhs: Box::new(PreHirExpr::Var("param_2".to_owned())),
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
        let PreHirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("expected if statement");
        };
        let PreHirStmt::Return(Some(PreHirExpr::Const(v, ty))) = &then_body[0] else {
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
        let mut func = PreHirFunction {
            name: "signum_setnz_neg".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param(
                "param_1",
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )],
            locals: vec![PreHirBinding {
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
                    PreHirExpr::Unary {
                        op: PreHirUnaryOp::Not,
                        expr: Box::new(PreHirExpr::Var("zf".into())),
                        ty: u8_ty.clone(),
                    },
                ),
                make_assign(
                    "uVar2",
                    PreHirExpr::Unary {
                        op: PreHirUnaryOp::Neg,
                        expr: Box::new(PreHirExpr::Var("uVar2".into())),
                        ty: i32_ty.clone(),
                    },
                ),
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".into()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("uVar2".into())))]
                        .into(),
                    else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
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
        let mut func = PreHirFunction {
            name: "signum_temp_width".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![make_param(
                "param_1",
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )],
            locals: vec![PreHirBinding {
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
                    PreHirExpr::Unary {
                        op: PreHirUnaryOp::Not,
                        expr: Box::new(PreHirExpr::Var("zf".into())),
                        ty: u8_ty.clone(),
                    },
                ),
                make_assign(
                    "xVar8",
                    PreHirExpr::Unary {
                        op: PreHirUnaryOp::Neg,
                        expr: Box::new(PreHirExpr::Var("xVar8".into())),
                        ty: i32_ty.clone(),
                    },
                ),
                PreHirStmt::Return(Some(PreHirExpr::Var("xVar8".into()))),
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
        let mut func = PreHirFunction {
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
                    PreHirExpr::Select {
                        cond: Box::new(PreHirExpr::Var("c1".into())),
                        then_expr: Box::new(PreHirExpr::Const(1, u64_ty.clone())),
                        else_expr: Box::new(PreHirExpr::Select {
                            cond: Box::new(PreHirExpr::Var("c2".into())),
                            then_expr: Box::new(PreHirExpr::Const(0, u64_ty.clone())),
                            else_expr: Box::new(PreHirExpr::Const(4294967295, u64_ty.clone())),
                            ty: u64_ty.clone(),
                        }),
                        ty: u64_ty.clone(),
                    },
                ),
                PreHirStmt::Return(Some(PreHirExpr::Var("xVar8".into()))),
            ],
            ..Default::default()
        };
        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.return_type, i32_ty);
        let PreHirStmt::Assign { rhs, .. } = &func.body[0] else {
            panic!("expected assign");
        };
        let printed = format!("{rhs:?}");
        assert!(
            printed.contains("-1")
                || matches!(rhs, PreHirExpr::Select { else_expr, .. }
                if matches!(else_expr.as_ref(), PreHirExpr::Select { else_expr: e2, .. }
                    if matches!(e2.as_ref(), PreHirExpr::Const(-1, _)))),
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
        let mut func = PreHirFunction {
            name: "saturating_like".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![make_binding("xVar39"), make_binding("local_4")],
            return_type: u64_ty.clone(),
            surface_return_type_name: None,
            body: vec![
                make_assign("local_4", PreHirExpr::Const(0, u64_ty.clone())),
                PreHirStmt::If {
                    cond: PreHirExpr::Var("overflow_pos".into()),
                    then_body: vec![make_assign(
                        "xVar39",
                        PreHirExpr::Const(2147483647, u64_ty.clone()),
                    )]
                    .into(),
                    else_body: vec![].into(),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Var("overflow_neg".into()),
                    then_body: vec![make_assign(
                        "xVar39",
                        PreHirExpr::Const(2147483648u64 as i64, u64_ty.clone()),
                    )]
                    .into(),
                    else_body: vec![].into(),
                },
                make_assign("xVar39", PreHirExpr::Var("local_4".into())),
                PreHirStmt::Return(Some(PreHirExpr::Var("xVar39".into()))),
            ],
            ..Default::default()
        };
        assert!(super::apply_type_inference_pass(&mut func));
        assert_eq!(func.return_type, i32_ty);
        // INT_MIN arm rewritten
        let PreHirStmt::If { then_body, .. } = &func.body[2] else {
            panic!("expected second if");
        };
        let PreHirStmt::Assign {
            rhs: PreHirExpr::Const(v, ty),
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
            make_assign("edx", PreHirExpr::Var("param_1".into())),
            make_assign("ecx", PreHirExpr::Var("param_2".into())),
            make_assign(
                "ecx",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("param_2".into())),
                    rhs: Box::new(PreHirExpr::Cast {
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                        expr: Box::new(PreHirExpr::Var("edx".into())),
                    }),
                    ty: u32_ty.clone(),
                },
            ),
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Ne,
                    lhs: Box::new(PreHirExpr::Var("ecx".into())),
                    rhs: Box::new(PreHirExpr::Var("edx".into())),
                    ty: NirType::Bool,
                },
                then_body: vec![].into(),
                else_body: vec![].into(),
            },
            make_assign(
                "byte",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("edx".into())),
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
            make_assign("alias", PreHirExpr::Var("input".into())),
            make_assign("cursor", PreHirExpr::Var("alias".into())),
            make_assign(
                "acc",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("acc".into())),
                    rhs: Box::new(PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("cursor".into())),
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
            make_assign("base_alias", PreHirExpr::Var("base".into())),
            make_assign("offset", PreHirExpr::Var("count".into())),
            make_assign(
                "half",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shr,
                    lhs: Box::new(PreHirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(PreHirExpr::Var("offset".into())),
                    }),
                    rhs: Box::new(PreHirExpr::Const(1, u64_ty.clone())),
                    ty: u64_ty.clone(),
                },
            ),
            make_assign(
                "offset",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: Box::new(PreHirExpr::Var("offset".into())),
                    rhs: Box::new(PreHirExpr::Var("index".into())),
                    ty: u64_ty.clone(),
                },
            ),
            make_assign(
                "address",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("base_alias".into())),
                    rhs: Box::new(PreHirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(PreHirExpr::Var("offset".into())),
                    }),
                    ty: ptr_ty.clone(),
                },
            ),
            make_assign(
                "value",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("address".into())),
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
            make_assign("reused", PreHirExpr::Var("input".into())),
            make_assign(
                "address",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Cast {
                        ty: ptr_ty.clone(),
                        expr: Box::new(PreHirExpr::Var("index".into())),
                    }),
                    rhs: Box::new(PreHirExpr::Var("reused".into())),
                    ty: ptr_ty.clone(),
                },
            ),
            make_assign(
                "reused",
                PreHirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("address".into())),
                        ty: u8_ty,
                    }),
                },
            ),
            make_assign(
                "acc",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mod,
                    lhs: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("acc".into())),
                        rhs: Box::new(PreHirExpr::Var("reused".into())),
                        ty: u64_ty.clone(),
                    }),
                    rhs: Box::new(PreHirExpr::Const(256, u64_ty.clone())),
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
            make_assign("cursor", PreHirExpr::Var("input".into())),
            make_assign(
                "loaded",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("cursor".into())),
                    ty: u8_ty,
                },
            ),
            make_assign(
                "shifted",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shr,
                    lhs: Box::new(PreHirExpr::Var("loaded".into())),
                    rhs: Box::new(PreHirExpr::Const(1, u32_ty.clone())),
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
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Lt,
                    lhs: Box::new(PreHirExpr::Var("induction".into())),
                    rhs: Box::new(PreHirExpr::Var("count".into())),
                    ty: NirType::Bool,
                },
                then_body: vec![].into(),
                else_body: vec![].into(),
            },
            make_assign(
                "value",
                PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("base".into())),
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
