//! Use-driven backward type propagation pass.
//!
//! `apply_type_inference_pass` (type_infer.rs) propagates types forward from
//! *definition* sites: if `x = (int32)...` then `x.ty = Int32`.  It cannot
//! infer types from *use* sites because `expr_type(PreHirExpr::Var(_)) = Unknown`.
//!
//! This pass performs the complementary backward-direction inference:
//!
//! 1. Walk every expression and statement to collect **use-site constraints**:
//!    - `Load { ptr: Var(x), ty }` → x must be a pointer to ty
//!    - `Deref { ptr: Var(x), ty }` (lvalue store destination) → same
//!    - `Index { base: Var(x), elem_ty }` (array lvalue) → x is `Ptr(elem_ty)`
//!    - `Binary { op: SLt|SLe, lhs: Var(x), ty }` → x is a signed integer
//!    - `Binary { op: Lt|Le, lhs: Var(x), ty }` → x is an unsigned integer
//!    - `Call { target: x }` → x must be an indirect-code pointer
//!    - `Return(Var(x))` + known function return type → x gets the return type
//!    - `Assign rhs = Cast(T, Var(x))` → x gets type T (the cast source)
//!
//! 2. Merge all collected constraints into `PreHirBinding.ty` for locals and
//!    params that are still `Unknown`.  Constraints are only *strengthened*
//!    (Unknown → Ptr → Int with signedness), never weakened.
//!
//! 3. Iterate until convergence (usually 1–2 rounds via the var-chain alias
//!    mechanism).
//!
//! This pass is binary-independent and heuristic-free.  It is placed right
//! after `apply_type_inference_pass` so that the def-driven types it computed
//! can serve as additional seeds for backward propagation.

mod alias_constraints;
mod binding_roles;
mod byte_index_accumulators;
mod collect_constraints;
mod constraints;
mod copy_constraints;
mod expression_types;
mod operator_constraints;
mod use_promotions;

#[cfg(test)]
mod tests;

use crate::HashMap;
use crate::analysis::defuse::DefinitionDependencyMap;
use crate::prelude::*;
use alias_constraints::propagate_logical_shift_constraints_through_aliases;
use binding_roles::{
    BindingUseRole, collect_binding_use_roles, restore_scalar_only_pointer_locals,
    should_skip_pointer_constraint_for_scalar_local,
};
use byte_index_accumulators::narrow_byte_index_accumulators;
use collect_constraints::collect_constraints;
use constraints::{UseConstraint, merge_constraint};
use expression_types::collect_known_binding_types;
use use_promotions::{
    narrow_integer_params_from_wrapping_return_uses, promote_return_signedness_from_returns,
    promote_store_value_only_aggregate_bindings, promote_store_value_only_unsigned_params,
    promote_unknown_call_return_type,
};

type TypeStateSignature = (NirType, Vec<(String, NirType)>, Vec<(String, NirType)>);

fn type_state_signature(func: &PreHirFunction) -> TypeStateSignature {
    (
        func.return_type.clone(),
        func.params
            .iter()
            .map(|binding| (binding.name.clone(), binding.ty.clone()))
            .collect(),
        func.locals
            .iter()
            .map(|binding| (binding.name.clone(), binding.ty.clone()))
            .collect(),
    )
}

/// Apply the use-driven backward type inference pass.
///
/// Iterates to convergence (typically 1–2 rounds).  Returns `true` if any
/// binding type changed.
pub fn apply_use_driven_type_infer_pass(func: &mut PreHirFunction) -> bool {
    let before = type_state_signature(func);
    let dependencies = DefinitionDependencyMap::build(&func.body);
    // `func.body`'s shape (as opposed to binding types) never changes across
    // this function's rounds below -- every sub-pass in the loop only
    // rewrites `.ty` fields, never statements/expressions -- so the use-role
    // classification (which reads only expression shape: Deref/Index/Load
    // context vs. arithmetic context) is round-invariant. Computing it once
    // here instead of on every round (and again inside
    // `restore_scalar_only_pointer_locals`, which used to rebuild its own
    // copy) turns up to 8 full-body walks per call into 1.
    let mut roles = HashMap::<String, BindingUseRole>::default();
    collect_binding_use_roles(&func.body, &mut roles);
    // A Var RHS has no expression type, but a proven operation-local COPY
    // still defines a pointer. Do not erase that value's type merely because
    // a null check or the reused carrier's scalar fact supplies a scalar use.
    for name in super::type_flow::pointer_copy_value_definitions(func) {
        let role = roles.entry(name).or_default();
        role.pointer_value_definition = true;
        role.non_pointer_value_definition = false;
    }
    let scalar_induction = super::type_infer::scalar_induction_evidence(func);
    let mut flow_changed = false;
    // Iterate to convergence (alias chains may require multiple rounds).
    for _ in 0..4 {
        let current_flow_changed = super::type_flow::apply_type_flow_pass(func);
        flow_changed |= current_flow_changed;
        let mut constraints: HashMap<String, Vec<UseConstraint>> = HashMap::default();
        let known_binding_types = collect_known_binding_types(func);
        let pointer_roots = func
            .params
            .iter()
            .filter_map(|binding| {
                matches!(binding.ty, NirType::Ptr(_)).then_some(binding.name.clone())
            })
            .collect();
        let address_contributors = dependencies.address_contributors(&func.body, &pointer_roots);
        collect_constraints(
            &func.body,
            &func.return_type,
            &known_binding_types,
            &mut constraints,
        );
        propagate_logical_shift_constraints_through_aliases(&func.body, &mut constraints);

        let pointer_bits = if func.is_64bit { 64 } else { 32 };
        let mut round_changed = current_flow_changed;
        for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
            if let Some(constraints_for) = constraints.get(&binding.name) {
                for constraint in constraints_for {
                    if should_skip_pointer_constraint_for_scalar_local(
                        binding,
                        constraint,
                        &roles,
                        &address_contributors,
                    ) {
                        continue;
                    }
                    round_changed |= merge_constraint(binding, constraint, pointer_bits);
                }
            }
        }
        round_changed |= promote_unknown_call_return_type(func);
        round_changed |= promote_return_signedness_from_returns(func);
        round_changed |= narrow_integer_params_from_wrapping_return_uses(func);
        round_changed |= promote_store_value_only_unsigned_params(func);
        round_changed |= promote_store_value_only_aggregate_bindings(func, &constraints, &roles);
        round_changed |= restore_scalar_only_pointer_locals(
            func,
            &constraints,
            &roles,
            &dependencies,
            &scalar_induction,
        );
        round_changed |= narrow_byte_index_accumulators(func);
        if !round_changed {
            break;
        }
    }
    flow_changed || type_state_signature(func) != before
}
