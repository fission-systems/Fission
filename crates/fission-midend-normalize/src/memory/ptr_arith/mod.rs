//! Pointer arithmetic HIR recovery pass.
//!
//! After use-driven type propagation (`use_type_infer.rs`) has marked pointer
//! variables, many remaining `IntAdd`/`IntSub` expressions of the form
//!
//! ```text
//! IntAdd(Var(ptr), Const(k))
//! IntAdd(Var(ptr), Mul(idx, Const(stride)))
//! ```
//!
//! still have purely-integer semantics in the HIR even though `ptr` is now
//! known to be a pointer.  This pass converts them to the higher-level
//! pointer-valued HIR expressions, or to `Index` when the arithmetic is the
//! address operand of a load/store. The printer renders these as:
//!
//! ```c
//! ptr + k          (PtrOffset with stride 1)
//! ptr + idx        (pointer-valued scaled add)
//! ptr[idx]         (Index load/store)
//! ```
//!
//! rather than the verbose `(uint8_t *)(ptr) + k` produced by the generic
//! integer-arithmetic printer.
//!
//! ### Algorithm
//!
//! For every expression tree:
//!
//! 1. `Add(Var(x), Mul(idx, Const(stride)))` where `x.ty == Ptr(T)` and
//!    `stride == sizeof(T)` (or T is Unknown/byte) → pointer-valued
//!    `Add(Var(x), idx)`. The C printer's pointer arithmetic supplies the
//!    element scaling.
//!
//! 2. `Add(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: k }`
//!
//! 3. `Load/Deref(Add(Var(x), Mul(idx, Const(stride))))` where the stride
//!    matches the accessed type → `Index { base, index, elem_ty }`.
//!
//! 4. `Cast { ty: Ptr(Int8), expr: Add(Var(x), ...) }` where `x` is already
//!    known to be a pointer → strip the `Ptr(Int8)` cast (it was added as a
//!    `uint8_t *` cast by the builder; now that the base is typed we don't need it).
//!
//! 5. `Sub(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: -k }`
//!
//! All transformations are conservative: we only act when the pointer type is
//! concretely known (`Ptr(_)`), never for `Unknown`.
//!
//! Reference: Ghidra `RulePtrArith` (ruleaction.cc:6496),
//!            RetDec `DerefToArrayIndexOptimizer`.

mod additive_trees;
mod address_terms;
mod comparisons;
mod field_access;
mod index_access;
mod pointer_offsets;
mod pointer_types;
mod rewrite;
mod zero_index;

#[cfg(test)]
mod tests;

use super::typed_facts;
use crate::prelude::*;
use pointer_types::{
    build_binding_type_map, infer_pointee_type_from_patterns, propagate_pointer_assignment_types,
};
use rewrite::recover_in_stmts;
use zero_index::normalize_zero_index_stmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PointerLayout {
    bytes: u64,
    bits: u32,
}

impl PointerLayout {
    fn from_function(func: &PreHirFunction) -> Self {
        if func.is_64bit {
            Self { bytes: 8, bits: 64 }
        } else {
            Self { bytes: 4, bits: 32 }
        }
    }
}

/// Apply the pointer arithmetic recovery pass to a function.
///
/// Returns `true` if any expression was rewritten.
pub fn apply_ptr_arith_recovery_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    let pointer_layout = PointerLayout::from_function(func);

    // Pre-pass: refine local and parameter pointer types using Scale-Invariant Access Pattern Scorer
    let inventory = typed_facts::collect_typed_fact_inventory(func, false);
    for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
        if matches!(binding.ty, NirType::Ptr(_)) {
            let refined =
                infer_pointee_type_from_patterns(&binding.name, &binding.ty, &inventory, None);
            if refined != NirType::Unknown {
                let new_ptr_ty = NirType::Ptr(Box::new(refined));
                if binding.ty != new_ptr_ty {
                    binding.ty = new_ptr_ty;
                    changed = true;
                }
            }
        }
    }

    for _ in 0..4 {
        let propagated = propagate_pointer_assignment_types(func);
        changed |= propagated;
        let binding_types = build_binding_type_map(func);
        if binding_types.is_empty() {
            break;
        }
        let recovered = recover_in_stmts(&mut func.body, &binding_types, pointer_layout);
        changed |= recovered;
        if !propagated && !recovered {
            break;
        }
    }
    changed
}

pub fn apply_zero_index_deref_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= normalize_zero_index_stmt(stmt);
    }
    changed
}
