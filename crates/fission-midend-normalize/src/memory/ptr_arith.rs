/// Pointer arithmetic HIR recovery pass.
///
/// After use-driven type propagation (`use_type_infer.rs`) has marked pointer
/// variables, many remaining `IntAdd`/`IntSub` expressions of the form
///
/// ```text
/// IntAdd(Var(ptr), Const(k))
/// IntAdd(Var(ptr), Mul(idx, Const(stride)))
/// ```
///
/// still have purely-integer semantics in the HIR even though `ptr` is now
/// known to be a pointer.  This pass converts them to the higher-level
/// pointer-valued HIR expressions, or to `Index` when the arithmetic is the
/// address operand of a load/store. The printer renders these as:
///
/// ```c
/// ptr + k          (PtrOffset with stride 1)
/// ptr + idx        (pointer-valued scaled add)
/// ptr[idx]         (Index load/store)
/// ```
///
/// rather than the verbose `(uint8_t *)(ptr) + k` produced by the generic
/// integer-arithmetic printer.
///
/// ### Algorithm
///
/// For every expression tree:
///
/// 1. `Add(Var(x), Mul(idx, Const(stride)))` where `x.ty == Ptr(T)` and
///    `stride == sizeof(T)` (or T is Unknown/byte) → pointer-valued
///    `Add(Var(x), idx)`. The C printer's pointer arithmetic supplies the
///    element scaling.
///
/// 2. `Add(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: k }`
///
/// 3. `Load/Deref(Add(Var(x), Mul(idx, Const(stride))))` where the stride
///    matches the accessed type → `Index { base, index, elem_ty }`.
///
/// 4. `Cast { ty: Ptr(Int8), expr: Add(Var(x), ...) }` where `x` is already
///    known to be a pointer → strip the `Ptr(Int8)` cast (it was added as a
///    `uint8_t *` cast by the builder; now that the base is typed we don't need it).
///
/// 5. `Sub(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: -k }`
///
/// All transformations are conservative: we only act when the pointer type is
/// concretely known (`Ptr(_)`), never for `Unknown`.
///
/// Reference: Ghidra `RulePtrArith` (ruleaction.cc:6496),
///            RetDec `DerefToArrayIndexOptimizer`.
use crate::prelude::*;
use crate::HashMap;

/// Build a map from variable name → NirType for all locals and params.
fn build_binding_type_map(func: &HirFunction) -> HashMap<String, NirType> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter(|b| b.ty != NirType::Unknown)
        .map(|b| (b.name.clone(), b.ty.clone()))
        .collect()
}

/// Return the pointee type if `ty` is `Ptr(pointee)`.
fn pointee_ty(ty: &NirType) -> Option<&NirType> {
    match ty {
        NirType::Ptr(inner) => Some(inner),
        _ => None,
    }
}

/// Return the byte size of a type, if known.
fn type_byte_size(ty: &NirType) -> Option<u64> {
    match ty {
        NirType::Int { bits, .. } | NirType::Float { bits } => Some(u64::from(*bits / 8)),
        NirType::Bool => Some(1),
        NirType::Ptr(_) => Some(8), // assume 64-bit
        _ => None,
    }
}

/// Try to recognise `Mul(idx, Const(stride))` or `Mul(Const(stride), idx)`,
/// returning `(idx_expr, stride)`.
fn try_extract_index_mul(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    let HirExpr::Binary {
        op: HirBinaryOp::Mul,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    match (lhs.as_ref(), rhs.as_ref()) {
        (_, HirExpr::Const(stride, _)) => Some((*lhs.clone(), *stride)),
        (HirExpr::Const(stride, _), _) => Some((*rhs.clone(), *stride)),
        _ => None,
    }
}

/// Return a typed pointer base expression.  The builder may preserve byte-level
/// address arithmetic as `Cast(Ptr(Int8), p)` even after use-driven inference
/// has proven that `p` is a more specific pointer type.  Keep track of that
/// byte cast so constant byte offsets can be rescaled to element offsets.
fn typed_pointer_base(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<(HirExpr, NirType, bool)> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            binding_types.get(name.as_str()).and_then(|ty| {
                if matches!(ty, NirType::Ptr(_)) {
                    Some((expr.clone(), ty.clone(), false))
                } else {
                    None
                }
            })
        }
        HirExpr::Cast {
            ty: NirType::Ptr(pointee),
            expr: inner,
        } if matches!(pointee.as_ref(), NirType::Int { bits: 8, .. }) => {
            let (base, ty, _) = typed_pointer_base(inner, binding_types)?;
            Some((base, ty, true))
        }
        _ => {
            let ty = expr_type(expr);
            if matches!(ty, NirType::Ptr(_)) {
                Some((expr.clone(), ty, false))
            } else {
                None
            }
        }
    }
}

fn pointer_const_expr(value: i64, sample: &HirExpr) -> HirExpr {
    match sample {
        HirExpr::Const(_, ty) => HirExpr::Const(value, ty.clone()),
        _ => HirExpr::Const(
            value,
            NirType::Int {
                bits: 64,
                signed: value < 0,
            },
        ),
    }
}

fn pointer_sized_uint_ty() -> NirType {
    NirType::Int {
        bits: 64,
        signed: false,
    }
}

fn cast_pointer_operand_to_uint(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    typed_pointer_base(expr, binding_types)?;
    Some(HirExpr::Cast {
        ty: pointer_sized_uint_ty(),
        expr: Box::new(expr.clone()),
    })
}

fn cast_pointer_operands_for_integer_arith(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary { op, lhs, rhs, ty } = expr else {
        return None;
    };
    if !matches!(ty, NirType::Int { .. }) || !matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub) {
        return None;
    }
    let new_lhs = cast_pointer_operand_to_uint(lhs, binding_types);
    let new_rhs = cast_pointer_operand_to_uint(rhs, binding_types);
    if new_lhs.is_none() && new_rhs.is_none() {
        return None;
    }
    Some(HirExpr::Binary {
        op: *op,
        lhs: Box::new(new_lhs.unwrap_or_else(|| lhs.as_ref().clone())),
        rhs: Box::new(new_rhs.unwrap_or_else(|| rhs.as_ref().clone())),
        ty: ty.clone(),
    })
}

fn recover_const_offset_as_typed_pointer_add(
    ptr_expr: &HirExpr,
    ptr_ty: &NirType,
    elem_ty: &NirType,
    offset: i64,
    rhs_expr: &HirExpr,
) -> Option<HirExpr> {
    if matches!(elem_ty, NirType::Unknown | NirType::Aggregate { .. }) {
        return None;
    }
    let elem_size = type_byte_size(elem_ty)?;
    if elem_size == 0 {
        return None;
    }
    let elem_size = i64::try_from(elem_size).ok()?;
    if offset % elem_size != 0 {
        return None;
    }
    let elem_offset = offset / elem_size;
    if elem_offset == 0 {
        return Some(ptr_expr.clone());
    }
    let op = if elem_offset > 0 {
        HirBinaryOp::Add
    } else {
        HirBinaryOp::Sub
    };
    Some(HirExpr::Binary {
        op,
        lhs: Box::new(ptr_expr.clone()),
        rhs: Box::new(pointer_const_expr(elem_offset.abs(), rhs_expr)),
        ty: ptr_ty.clone(),
    })
}

fn condition_pointer_operand(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    match expr {
        HirExpr::Cast { ty, expr } if matches!(ty, NirType::Int { .. }) => {
            typed_pointer_base(expr, binding_types).map(|(base, _, _)| base)
        }
        _ => typed_pointer_base(expr, binding_types).map(|(base, _, _)| base),
    }
}

fn recover_pointer_difference_condition(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let lhs = condition_pointer_operand(lhs, binding_types)?;
    let rhs = condition_pointer_operand(rhs, binding_types)?;
    Some(HirExpr::Binary {
        op: HirBinaryOp::Ne,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    })
}

fn recover_pointer_difference_zero_compare(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: op @ (HirBinaryOp::Eq | HirBinaryOp::Ne),
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if matches!(rhs.as_ref(), HirExpr::Const(0, _)) {
        let HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: diff_lhs,
            rhs: diff_rhs,
            ..
        } = lhs.as_ref()
        else {
            return None;
        };
        let diff_lhs = condition_pointer_operand(diff_lhs, binding_types)?;
        let diff_rhs = condition_pointer_operand(diff_rhs, binding_types)?;
        return Some(HirExpr::Binary {
            op: *op,
            lhs: Box::new(diff_lhs),
            rhs: Box::new(diff_rhs),
            ty: NirType::Bool,
        });
    }
    if matches!(lhs.as_ref(), HirExpr::Const(0, _)) {
        let HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: diff_lhs,
            rhs: diff_rhs,
            ..
        } = rhs.as_ref()
        else {
            return None;
        };
        let diff_lhs = condition_pointer_operand(diff_lhs, binding_types)?;
        let diff_rhs = condition_pointer_operand(diff_rhs, binding_types)?;
        return Some(HirExpr::Binary {
            op: *op,
            lhs: Box::new(diff_lhs),
            rhs: Box::new(diff_rhs),
            ty: NirType::Bool,
        });
    }
    None
}

fn recover_negated_pointer_difference_condition(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr,
        ..
    } = expr
    else {
        return None;
    };
    recover_pointer_difference_zero_compare(expr, binding_types).map(negate_expr)
}

fn recover_condition_expr(expr: &mut HirExpr, binding_types: &HashMap<String, NirType>) -> bool {
    let mut changed = recover_in_expr(expr, binding_types);
    if let Some(new_expr) = recover_pointer_difference_condition(expr, binding_types)
        .or_else(|| recover_pointer_difference_zero_compare(expr, binding_types))
        .or_else(|| recover_negated_pointer_difference_condition(expr, binding_types))
    {
        *expr = new_expr;
        changed = true;
    }
    changed
}

// ── PtrOffset chaining / folding ──────────────────────────────────────────────

/// Fold `PtrOffset(PtrOffset(base, a), b)` → `PtrOffset(base, a+b)`.
/// Also folds `PtrOffset(base, 0)` → `base`.
/// Returns `true` if any rewrite occurred.
fn fold_ptr_offset_chains(expr: &mut HirExpr) -> bool {
    let mut changed = false;
    // Recurse children first.
    match expr {
        HirExpr::PtrOffset { base, .. } => {
            changed |= fold_ptr_offset_chains(base);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= fold_ptr_offset_chains(lhs);
            changed |= fold_ptr_offset_chains(rhs);
        }
        HirExpr::Cast { expr: inner, .. } | HirExpr::Unary { expr: inner, .. } => {
            changed |= fold_ptr_offset_chains(inner);
        }
        HirExpr::Load { ptr, .. } => {
            changed |= fold_ptr_offset_chains(ptr);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= fold_ptr_offset_chains(base);
            changed |= fold_ptr_offset_chains(index);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                changed |= fold_ptr_offset_chains(a);
            }
        }
        _ => {}
    }
    // Now try to fold this node.
    if let HirExpr::PtrOffset { base, offset } = expr {
        // PtrOffset(base, 0) → base
        if *offset == 0 {
            let inner = *base.clone();
            *expr = inner;
            return true;
        }
        // PtrOffset(PtrOffset(inner, a), b) → PtrOffset(inner, a+b)
        if let HirExpr::PtrOffset {
            base: inner,
            offset: inner_offset,
        } = base.as_mut()
        {
            let combined = inner_offset.saturating_add(*offset);
            let new_inner = *inner.clone();
            *expr = HirExpr::PtrOffset {
                base: Box::new(new_inner),
                offset: combined,
            };
            changed = true;
        }
    }
    changed
}

// ── Multi-level ADD tree flattening (Ghidra AddTreeState::spanAddTree analog) ─

struct AddTreeState {
    ptr_expr: HirExpr,
    ptr_ty: NirType,
    elem_size: i64,
    multiples: Vec<(HirExpr, i64)>,
    mult_const: i64,
    non_multiples: Vec<(HirExpr, i64)>,
    non_mult_const: i64,
    other_terms: Vec<HirExpr>,
    valid: bool,
}

impl AddTreeState {
    fn new(ptr_expr: HirExpr, ptr_ty: NirType) -> Self {
        let elem_ty = pointee_ty(&ptr_ty).cloned().unwrap_or(NirType::Unknown);
        let elem_size = type_byte_size(&elem_ty).unwrap_or(1) as i64;
        let elem_size = if elem_size <= 0 { 1 } else { elem_size };
        Self {
            ptr_expr,
            ptr_ty,
            elem_size,
            multiples: Vec::new(),
            mult_const: 0,
            non_multiples: Vec::new(),
            non_mult_const: 0,
            other_terms: Vec::new(),
            valid: true,
        }
    }

    fn span_add_tree(&mut self, expr: &HirExpr, coeff: i64) {
        if !self.valid {
            return;
        }
        match expr {
            HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs,
                rhs,
                ..
            } => {
                self.span_add_tree(lhs, coeff);
                self.span_add_tree(rhs, coeff);
            }
            HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs,
                rhs,
                ..
            } => {
                self.span_add_tree(lhs, coeff);
                self.span_add_tree(rhs, -coeff);
            }
            HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs,
                rhs,
                ..
            } => {
                if let Some((idx, stride)) = try_extract_index_mul(expr) {
                    let total_stride = stride.wrapping_mul(coeff);
                    if matches!(
                        idx,
                        HirExpr::Binary {
                            op: HirBinaryOp::Add | HirBinaryOp::Sub,
                            ..
                        }
                    ) {
                        self.span_add_tree(&idx, total_stride);
                    } else {
                        self.add_index_term(idx, total_stride);
                    }
                } else {
                    self.add_other_term(expr.clone(), coeff);
                }
            }
            HirExpr::Const(k, _) => {
                let val = k.wrapping_mul(coeff);
                self.add_const_term(val);
            }
            other => {
                if other == &self.ptr_expr {
                    return;
                }
                self.add_other_term(other.clone(), coeff);
            }
        }
    }

    fn add_const_term(&mut self, val: i64) {
        if val % self.elem_size == 0 {
            self.mult_const = self.mult_const.wrapping_add(val);
        } else {
            self.non_mult_const = self.non_mult_const.wrapping_add(val);
        }
    }

    fn add_index_term(&mut self, idx: HirExpr, stride: i64) {
        if stride % self.elem_size == 0 {
            self.multiples.push((idx, stride));
        } else {
            self.non_multiples.push((idx, stride));
        }
    }

    fn add_other_term(&mut self, expr: HirExpr, coeff: i64) {
        if coeff == 1 {
            self.other_terms.push(expr);
        } else if coeff == -1 {
            self.other_terms.push(HirExpr::Unary {
                op: HirUnaryOp::Neg,
                expr: Box::new(expr),
                ty: NirType::Unknown,
            });
        } else {
            if coeff % self.elem_size == 0 {
                self.multiples.push((expr, coeff));
            } else {
                self.other_terms.push(HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    lhs: Box::new(expr),
                    rhs: Box::new(HirExpr::Const(coeff, NirType::Unknown)),
                    ty: NirType::Unknown,
                });
            }
        }
    }

    fn build_tree(&self) -> Option<HirExpr> {
        if !self.valid {
            return None;
        }

        let mut index_terms: Vec<HirExpr> = Vec::new();

        for (idx, stride) in &self.multiples {
            let mult = stride / self.elem_size;
            if mult == 1 {
                index_terms.push(idx.clone());
            } else if mult == -1 {
                index_terms.push(HirExpr::Unary {
                    op: HirUnaryOp::Neg,
                    expr: Box::new(idx.clone()),
                    ty: NirType::Unknown,
                });
            } else {
                index_terms.push(HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    lhs: Box::new(idx.clone()),
                    rhs: Box::new(HirExpr::Const(mult, NirType::Unknown)),
                    ty: NirType::Unknown,
                });
            }
        }

        let mult_const_elements = self.mult_const / self.elem_size;
        if mult_const_elements != 0 {
            index_terms.push(HirExpr::Const(
                mult_const_elements,
                NirType::Int {
                    bits: 64,
                    signed: mult_const_elements < 0,
                },
            ));
        }

        let mut index_sum: Option<HirExpr> = None;
        for term in index_terms {
            if let Some(sum) = index_sum {
                index_sum = Some(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(sum),
                    rhs: Box::new(term),
                    ty: NirType::Unknown,
                });
            } else {
                index_sum = Some(term);
            }
        }

        let mut base = self.ptr_expr.clone();
        if let Some(idx_expr) = index_sum {
            base = HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(base),
                rhs: Box::new(idx_expr),
                ty: self.ptr_ty.clone(),
            };
        }

        for (idx, stride) in &self.non_multiples {
            let term = HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: Box::new(idx.clone()),
                rhs: Box::new(HirExpr::Const(*stride, NirType::Unknown)),
                ty: NirType::Unknown,
            };
            base = HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(base),
                rhs: Box::new(term),
                ty: self.ptr_ty.clone(),
            };
        }

        for other in &self.other_terms {
            base = HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(base),
                rhs: Box::new(other.clone()),
                ty: self.ptr_ty.clone(),
            };
        }

        if self.non_mult_const != 0 {
            base = HirExpr::PtrOffset {
                base: Box::new(base),
                offset: self.non_mult_const,
            };
        }

        if base == self.ptr_expr {
            None
        } else {
            Some(base)
        }
    }
}

/// Extension to `try_recover_ptr_arith` that handles multi-level ADD trees
/// (e.g. `ptr + idx*4 + 8` via flattening).
fn try_recover_ptr_arith_tree(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    // Only applies to Add/Sub trees.
    if !matches!(
        expr,
        HirExpr::Binary {
            op: HirBinaryOp::Add | HirBinaryOp::Sub,
            ..
        }
    ) {
        return None;
    }

    // Find the pointer base (must be a leaf or Cast(ptr8, leaf)).
    // We walk the left-most Add chain looking for a pointer.
    let mut ptr_side: Option<(HirExpr, NirType)> = None;
    let mut non_ptr_accum: Vec<HirExpr> = Vec::new();
    collect_add_terms_with_ptr(
        expr,
        false,
        binding_types,
        &mut ptr_side,
        &mut non_ptr_accum,
    );

    let (ptr_expr, ptr_ty) = ptr_side?;
    if non_ptr_accum.is_empty() {
        return None;
    }

    // Only proceed if there are at least 2 non-ptr terms (otherwise single-level handles it).
    if non_ptr_accum.len() < 2 {
        return None;
    }

    let mut state = AddTreeState::new(ptr_expr, ptr_ty);
    for term in non_ptr_accum {
        state.span_add_tree(&term, 1);
    }

    state.build_tree()
}

/// Collect additive terms from a possibly-nested ADD tree, separating out the
/// pointer base from the non-pointer terms.
fn collect_add_terms_with_ptr(
    expr: &HirExpr,
    neg: bool,
    binding_types: &HashMap<String, NirType>,
    ptr_side: &mut Option<(HirExpr, NirType)>,
    non_ptr: &mut Vec<HirExpr>,
) {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            collect_add_terms_with_ptr(lhs, neg, binding_types, ptr_side, non_ptr);
            collect_add_terms_with_ptr(rhs, neg, binding_types, ptr_side, non_ptr);
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            collect_add_terms_with_ptr(lhs, neg, binding_types, ptr_side, non_ptr);
            collect_add_terms_with_ptr(rhs, !neg, binding_types, ptr_side, non_ptr);
        }
        other => {
            // Check if this is the pointer base.
            if ptr_side.is_none() && !neg {
                if let Some((base, ty, _)) = typed_pointer_base(other, binding_types) {
                    *ptr_side = Some((base, ty));
                    return;
                }
            }
            // Otherwise it's an additive non-pointer term.
            if neg {
                // Wrap in Neg so the term stays signed.
                non_ptr.push(HirExpr::Unary {
                    op: HirUnaryOp::Neg,
                    expr: Box::new(other.clone()),
                    ty: NirType::Unknown,
                });
            } else {
                non_ptr.push(other.clone());
            }
        }
    }
}

/// Structural equality helper used to avoid pointless rewrites.
trait StructuralEq {
    fn structural_eq(&self, other: &Self) -> bool;
}

impl StructuralEq for HirExpr {
    fn structural_eq(&self, other: &Self) -> bool {
        // Simple pointer-identity check is fine here; this only avoids a no-op path.
        std::ptr::eq(self as *const _, other as *const _)
    }
}

/// Attempt to convert a pointer-arithmetic expression to a PtrOffset or Index
/// node.  Returns `Some(new_expr)` on success; `None` means leave unchanged.
fn try_recover_ptr_arith(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary { op, lhs, rhs, ty } = expr else {
        return None;
    };

    let (ptr_expr, rhs_expr, neg) = match op {
        HirBinaryOp::Add => (lhs.as_ref(), rhs.as_ref(), false),
        HirBinaryOp::Sub => (lhs.as_ref(), rhs.as_ref(), true),
        _ => return None,
    };

    let (typed_ptr_expr, ptr_ty, from_byte_cast) = typed_pointer_base(ptr_expr, binding_types)?;
    let mut elem_ty = pointee_ty(&ptr_ty).cloned().unwrap_or(NirType::Unknown);

    // Dynamic variable index scale recovery: extract dynamic stride if present
    let mut stride_opt = None;
    if !neg {
        if let Some((_, stride)) = try_extract_index_mul(rhs_expr) {
            stride_opt = Some(stride);
        }
    }

    // On-the-fly pointee type refinement using stride
    if matches!(elem_ty, NirType::Unknown | NirType::Int { bits: 8, .. }) {
        if let Some(stride) = stride_opt {
            if stride > 0 {
                elem_ty = match stride {
                    2 => NirType::Int {
                        bits: 16,
                        signed: false,
                    },
                    4 => NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    8 => NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    _ => NirType::Int {
                        bits: (stride * 8) as u32,
                        signed: false,
                    },
                };
            }
        }
    }

    let pointer_typed_const_byte_offset = matches!(ty, NirType::Ptr(_))
        && !from_byte_cast
        && matches!(elem_ty, NirType::Unknown | NirType::Aggregate { .. })
        && matches!(rhs_expr, HirExpr::Const(_, _));
    if matches!(ty, NirType::Ptr(_)) && !from_byte_cast && !pointer_typed_const_byte_offset {
        return None;
    }

    // Pattern 1: Add(ptr, Mul(idx, Const(stride))) → pointer add when stride matches.
    if !neg {
        if let Some((idx_expr, stride)) = try_extract_index_mul(rhs_expr) {
            let stride_matches = match type_byte_size(&elem_ty) {
                Some(sz) => stride as u64 == sz,
                None => stride == 1, // unknown elem_ty → only allow stride-1
            };
            if stride_matches && stride > 0 {
                let refined_ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
                return Some(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(typed_ptr_expr.clone()),
                    rhs: Box::new(idx_expr),
                    ty: refined_ptr_ty,
                });
            }
            // stride == 1 with byte pointer is also a valid index
            if stride == 1 && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
                let refined_ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
                return Some(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(typed_ptr_expr.clone()),
                    rhs: Box::new(idx_expr),
                    ty: refined_ptr_ty,
                });
            }
        }
    }

    // Pattern 2: Add(ptr, Const(k)) / Sub(ptr, Const(k)) → PtrOffset.
    if let HirExpr::Const(k, _) = rhs_expr {
        let offset = if neg { -k } else { *k };
        if let Some(recovered) = recover_const_offset_as_typed_pointer_add(
            &typed_ptr_expr,
            &ptr_ty,
            &elem_ty,
            offset,
            rhs_expr,
        ) {
            return Some(recovered);
        }
        if from_byte_cast && offset == 0 {
            return Some(typed_ptr_expr.clone());
        }
        // Only emit PtrOffset when offset != 0 (offset 0 is a no-op).
        if offset != 0 {
            return Some(HirExpr::PtrOffset {
                base: Box::new(typed_ptr_expr.clone()),
                offset,
            });
        }
        // offset == 0: the expression equals ptr — return bare Var.
        if offset == 0 {
            return Some(typed_ptr_expr.clone());
        }
    }

    // Pattern 3: Add(ptr, non-const-index) → pointer add with stride 1 for byte pointers.
    if !neg && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
        let refined_ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        return Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(typed_ptr_expr.clone()),
            rhs: Box::new(rhs_expr.clone()),
            ty: refined_ptr_ty,
        });
    }

    None
}

fn try_recover_index_access(
    ptr: &HirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let mut current_ptr = ptr;
    if let HirExpr::Cast { expr, .. } = ptr {
        current_ptr = expr.as_ref();
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = current_ptr
    else {
        return None;
    };
    let ptr_ty = match lhs.as_ref() {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            binding_types.get(name.as_str()).and_then(|t| {
                if matches!(t, NirType::Ptr(_)) {
                    Some(t)
                } else {
                    None
                }
            })?
        }
        _ => return None,
    };
    let mut elem_ty = pointee_ty(ptr_ty)
        .cloned()
        .unwrap_or_else(|| access_ty.clone());
    let (idx_expr, stride) = try_extract_index_mul(rhs.as_ref())?;

    // On-the-fly refinement for stride/index matching
    if matches!(elem_ty, NirType::Unknown | NirType::Int { bits: 8, .. }) && stride > 0 {
        elem_ty = match stride {
            2 => NirType::Int {
                bits: 16,
                signed: false,
            },
            4 => NirType::Int {
                bits: 32,
                signed: false,
            },
            8 => NirType::Int {
                bits: 64,
                signed: false,
            },
            _ => NirType::Int {
                bits: (stride * 8) as u32,
                signed: false,
            },
        };
    }

    let access_size = type_byte_size(access_ty).or_else(|| type_byte_size(&elem_ty))?;
    if stride > 0 && stride as u64 == access_size {
        Some(HirExpr::Index {
            base: lhs.clone(),
            index: Box::new(idx_expr),
            elem_ty: access_ty.clone(),
        })
    } else {
        None
    }
}

fn pointer_index_base_for_access(
    expr: &HirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<(Box<HirExpr>, i64)> {
    let (_base, ptr_ty, _from_byte_cast) = typed_pointer_base(expr, binding_types)?;
    let elem_ty = pointee_ty(&ptr_ty)?;
    if matches!(elem_ty, NirType::Aggregate { .. } | NirType::Unknown) {
        return None;
    }
    let access_size = i64::try_from(type_byte_size(access_ty)?).ok()?;
    if access_size == 0 {
        return None;
    }
    let elem_size = i64::try_from(type_byte_size(elem_ty)?).ok()?;
    if elem_size != access_size {
        return None;
    }
    Some((Box::new(expr.clone()), access_size))
}

fn try_recover_const_index_access(
    ptr: &HirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let mut current_ptr = ptr;
    if let HirExpr::Cast { expr, .. } = ptr {
        current_ptr = expr.as_ref();
    }
    let HirExpr::Binary { op, lhs, rhs, .. } = current_ptr else {
        return None;
    };
    let HirExpr::Const(raw_index, index_ty) = rhs.as_ref() else {
        return None;
    };
    let raw_offset = match op {
        HirBinaryOp::Add => *raw_index,
        HirBinaryOp::Sub => raw_index.checked_neg()?,
        _ => return None,
    };
    let (base, elem_size) = pointer_index_base_for_access(lhs, access_ty, binding_types)?;
    if raw_offset % elem_size != 0 {
        return None;
    }
    let index = raw_offset / elem_size;
    Some(HirExpr::Index {
        base,
        index: Box::new(HirExpr::Const(index, index_ty.clone())),
        elem_ty: access_ty.clone(),
    })
}

fn try_recover_field_access(
    ptr: &HirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let mut current_ptr = ptr;
    if let HirExpr::Cast { expr, .. } = ptr {
        current_ptr = expr.as_ref();
    }
    let (base_expr, offset) = match current_ptr {
        HirExpr::PtrOffset { base, offset } => (base.as_ref().clone(), *offset),
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let HirExpr::Const(raw_offset, _) = rhs.as_ref() else {
                return None;
            };
            let offset = match op {
                HirBinaryOp::Add => *raw_offset,
                HirBinaryOp::Sub => raw_offset.checked_neg()?,
                _ => return None,
            };
            (lhs.as_ref().clone(), offset)
        }
        _ => return None,
    };

    let (typed_ptr_expr, ptr_ty, _from_byte_cast) = typed_pointer_base(&base_expr, binding_types)?;
    let elem_ty = pointee_ty(&ptr_ty)?;
    let NirType::Aggregate { fields, .. } = elem_ty else {
        return None;
    };

    if offset < 0 {
        return None;
    }
    let field_offset = offset as u32;

    let field = fields.iter().find(|f| f.offset == field_offset);
    let (field_name, field_ty) = match field {
        Some(f) => (f.name.clone(), f.ty.clone()),
        None => (format!("field_{field_offset}"), NirType::Unknown),
    };

    let final_ty = if matches!(field_ty, NirType::Unknown) {
        access_ty.clone()
    } else {
        field_ty
    };

    Some(HirExpr::FieldAccess {
        base: Box::new(typed_ptr_expr),
        field_name,
        offset: field_offset,
        ty: final_ty,
    })
}

/// Recursively rewrite all pointer-arithmetic sub-expressions in `expr`.
fn recover_in_expr(expr: &mut HirExpr, binding_types: &HashMap<String, NirType>) -> bool {
    // Try the top-level single-level pattern first.
    if let Some(new_expr) = try_recover_ptr_arith(expr, binding_types) {
        *expr = new_expr;
        // After single-level recovery, try PtrOffset chain folding.
        fold_ptr_offset_chains(expr);
        return true;
    }
    // Try multi-level ADD tree (Ghidra AddTreeState::spanAddTree analog).
    if let Some(new_expr) = try_recover_ptr_arith_tree(expr, binding_types) {
        *expr = new_expr;
        fold_ptr_offset_chains(expr);
        return true;
    }
    if let Some(new_expr) = cast_pointer_operands_for_integer_arith(expr, binding_types) {
        *expr = new_expr;
        return true;
    }
    // Recurse into children.
    let mut changed = false;
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= recover_in_expr(lhs, binding_types);
            changed |= recover_in_expr(rhs, binding_types);
        }
        HirExpr::Unary { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types);
        }
        HirExpr::Cast { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types);
            // After recursing: if we now have Cast(Ptr(Int8), PtrOffset { base, .. })
            // and base is already pointer-typed, strip the cast.
            if let HirExpr::Cast {
                ty: NirType::Ptr(pointee),
                expr: inner2,
            } = expr
            {
                if matches!(pointee.as_ref(), NirType::Int { bits: 8, .. }) {
                    if matches!(
                        inner2.as_ref(),
                        HirExpr::PtrOffset { .. } | HirExpr::Index { .. }
                    ) {
                        let new_inner = *inner2.clone();
                        *expr = new_inner;
                        changed = true;
                    }
                }
            }
        }
        HirExpr::Load { ptr, ty } => {
            if let Some(field_expr) = try_recover_field_access(ptr, ty, binding_types) {
                *expr = field_expr;
                return true;
            }
            if let Some(index_expr) = try_recover_index_access(ptr, ty, binding_types)
                .or_else(|| try_recover_const_index_access(ptr, ty, binding_types))
            {
                *expr = index_expr;
                return true;
            }
            changed |= recover_in_expr(ptr, binding_types);
        }
        HirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                changed |= recover_in_expr(arg, binding_types);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            changed |= recover_in_expr(base, binding_types);
        }
        HirExpr::FieldAccess { base, .. } => {
            changed |= recover_in_expr(base, binding_types);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= recover_in_expr(base, binding_types);
            changed |= recover_in_expr(index, binding_types);
        }
        HirExpr::AggregateCopy { src, .. } => {
            changed |= recover_in_expr(src, binding_types);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= recover_in_expr(cond, binding_types);
            changed |= recover_in_expr(then_expr, binding_types);
            changed |= recover_in_expr(else_expr, binding_types);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

fn recover_in_lvalue(lhs: &mut HirLValue, binding_types: &HashMap<String, NirType>) -> bool {
    match lhs {
        HirLValue::Deref { ptr, ty } => {
            if let Some(field_expr) = try_recover_field_access(ptr, ty, binding_types) {
                let HirExpr::FieldAccess {
                    base,
                    field_name,
                    offset,
                    ty: f_ty,
                } = field_expr
                else {
                    unreachable!()
                };
                *lhs = HirLValue::FieldAccess {
                    base,
                    field_name,
                    offset,
                    ty: f_ty,
                };
                true
            } else if let Some(HirExpr::Index {
                base,
                index,
                elem_ty,
            }) = try_recover_index_access(ptr, ty, binding_types)
                .or_else(|| try_recover_const_index_access(ptr, ty, binding_types))
            {
                *lhs = HirLValue::Index {
                    base,
                    index,
                    elem_ty,
                };
                true
            } else {
                recover_in_expr(ptr, binding_types)
            }
        }
        HirLValue::Index { base, index, .. } => {
            let a = recover_in_expr(base, binding_types);
            let b = recover_in_expr(index, binding_types);
            a || b
        }
        HirLValue::Var(_) => false,
        HirLValue::FieldAccess { base, .. } => recover_in_expr(base, binding_types),
    }
}

fn is_zero_index(index: &HirExpr) -> bool {
    matches!(index, HirExpr::Const(0, _))
}

fn collect_pointer_assignment_types(stmts: &[HirStmt], out: &mut HashMap<String, Option<NirType>>) {
    for stmt in stmts {
        collect_pointer_assignment_types_stmt(stmt, out);
    }
}

fn record_pointer_assignment(out: &mut HashMap<String, Option<NirType>>, name: &str, ty: NirType) {
    if !matches!(ty, NirType::Ptr(_)) {
        if let Some(slot) = out.get_mut(name) {
            *slot = None;
        }
        return;
    }
    match out.get_mut(name) {
        Some(slot) => {
            if slot.as_ref().is_some_and(|existing| existing != &ty) {
                *slot = None;
            }
        }
        None => {
            out.insert(name.to_string(), Some(ty));
        }
    }
}

fn collect_pointer_assignment_types_stmt(
    stmt: &HirStmt,
    out: &mut HashMap<String, Option<NirType>>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => record_pointer_assignment(out, name, expr_type(rhs)),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_pointer_assignment_types(body, out);
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_pointer_assignment_types(then_body, out);
            collect_pointer_assignment_types(else_body, out);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                collect_pointer_assignment_types_stmt(init, out);
            }
            if let Some(update) = update {
                collect_pointer_assignment_types_stmt(update, out);
            }
            collect_pointer_assignment_types(body, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_pointer_assignment_types(&case.body, out);
            }
            collect_pointer_assignment_types(default, out);
        }
        _ => {}
    }
}

fn propagate_pointer_assignment_types(func: &mut HirFunction) -> bool {
    let mut inferred = HashMap::default();
    collect_pointer_assignment_types(&func.body, &mut inferred);
    if inferred.is_empty() {
        return false;
    }
    let mut changed = false;
    for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
        let Some(Some(ty)) = inferred.get(binding.name.as_str()) else {
            continue;
        };
        if !matches!(binding.ty, NirType::Ptr(_)) {
            binding.ty = ty.clone();
            changed = true;
        }
    }
    changed
}

fn recover_in_stmts(stmts: &mut Vec<HirStmt>, binding_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= recover_in_stmt(stmt, binding_types);
    }
    changed
}

fn recover_in_stmt(stmt: &mut HirStmt, binding_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= recover_in_lvalue(lhs, binding_types);
            changed |= recover_in_expr(rhs, binding_types);
        }
        HirStmt::Expr(expr) => {
            changed |= recover_in_expr(expr, binding_types);
        }
        HirStmt::Block(body) => {
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= recover_condition_expr(cond, binding_types);
            changed |= recover_in_stmts(then_body, binding_types);
            changed |= recover_in_stmts(else_body, binding_types);
        }
        HirStmt::While { cond, body } => {
            changed |= recover_condition_expr(cond, binding_types);
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::DoWhile { body, cond } => {
            changed |= recover_in_stmts(body, binding_types);
            changed |= recover_condition_expr(cond, binding_types);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= recover_in_stmt(i, binding_types);
            }
            if let Some(c) = cond {
                changed |= recover_condition_expr(c, binding_types);
            }
            if let Some(u) = update {
                changed |= recover_in_stmt(u, binding_types);
            }
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= recover_in_expr(expr, binding_types);
            for case in cases.iter_mut() {
                changed |= recover_in_stmts(&mut case.body, binding_types);
            }
            changed |= recover_in_stmts(default, binding_types);
        }
        HirStmt::Return(Some(expr)) => {
            changed |= recover_in_expr(expr, binding_types);
        }
        _ => {}
    }
    changed
}

/// Infer a pointee type for a variable from its access patterns and/or dynamic stride.
fn infer_pointee_type_from_patterns(
    name: &str,
    binding_ty: &NirType,
    inventory: &super::typed_facts::TypedFactInventory,
    has_dynamic_stride: Option<i64>,
) -> NirType {
    if let NirType::Ptr(inner) = binding_ty {
        if !matches!(
            inner.as_ref(),
            NirType::Unknown | NirType::Int { bits: 8, .. }
        ) {
            return inner.as_ref().clone();
        }
    }

    if let Some(stride) = has_dynamic_stride {
        if stride > 0 {
            return match stride {
                1 => NirType::Int {
                    bits: 8,
                    signed: false,
                },
                2 => NirType::Int {
                    bits: 16,
                    signed: false,
                },
                4 => NirType::Int {
                    bits: 32,
                    signed: false,
                },
                8 => NirType::Int {
                    bits: 64,
                    signed: false,
                },
                _ => NirType::Int {
                    bits: (stride * 8) as u32,
                    signed: false,
                },
            };
        }
    }

    if let Some(obj_facts) = inventory.objects.get(name) {
        let offsets: Vec<u32> = obj_facts.accesses.keys().copied().collect();
        if offsets.is_empty() {
            return NirType::Unknown;
        }

        for stride in [8, 4, 2] {
            let all_multiples = offsets.iter().all(|&o| o % stride == 0);
            if all_multiples && offsets.len() >= 2 {
                let mut indices: Vec<u32> = offsets.iter().map(|&o| o / stride).collect();
                indices.sort_unstable();
                let is_continuous = indices.windows(2).all(|w| w[1] - w[0] <= 2);
                if is_continuous {
                    return match stride {
                        2 => NirType::Int {
                            bits: 16,
                            signed: false,
                        },
                        4 => NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                        8 => NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                        _ => NirType::Unknown,
                    };
                }
            }
        }

        if offsets.len() >= 2 {
            let inferred_size =
                super::typed_facts::inferred_aggregate_size(&obj_facts.accesses).unwrap_or(0);
            if inferred_size > 0 {
                return NirType::Aggregate {
                    size: inferred_size,
                    fields: obj_facts.shape.fields.clone(),
                };
            }
        }
    }

    NirType::Unknown
}

/// Apply the pointer arithmetic recovery pass to a function.
///
/// Returns `true` if any expression was rewritten.
pub fn apply_ptr_arith_recovery_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;

    // Pre-pass: refine local and parameter pointer types using Scale-Invariant Access Pattern Scorer
    let inventory = super::typed_facts::collect_typed_fact_inventory(func, false);
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
        let recovered = recover_in_stmts(&mut func.body, &binding_types);
        changed |= recovered;
        if !propagated && !recovered {
            break;
        }
    }
    changed
}

pub fn apply_zero_index_deref_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= normalize_zero_index_stmt(stmt);
    }
    changed
}

fn normalize_zero_index_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= normalize_zero_index_lvalue(lhs);
            changed |= normalize_zero_index_expr(rhs);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= normalize_zero_index_expr(expr);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= normalize_zero_index_expr(cond);
            for stmt in then_body {
                changed |= normalize_zero_index_stmt(stmt);
            }
            for stmt in else_body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::While { cond, body } => {
            changed |= normalize_zero_index_expr(cond);
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
            changed |= normalize_zero_index_expr(cond);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                changed |= normalize_zero_index_stmt(init);
            }
            if let Some(cond) = cond {
                changed |= normalize_zero_index_expr(cond);
            }
            if let Some(update) = update {
                changed |= normalize_zero_index_stmt(update);
            }
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= normalize_zero_index_expr(expr);
            for case in cases {
                for stmt in &mut case.body {
                    changed |= normalize_zero_index_stmt(stmt);
                }
            }
            for stmt in default {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::Block(body) => {
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= normalize_zero_index_expr(va_list);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
    changed
}

fn normalize_zero_index_lvalue(lhs: &mut HirLValue) -> bool {
    match lhs {
        HirLValue::Deref { ptr, .. } => normalize_zero_index_expr(ptr),
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let mut changed = normalize_zero_index_expr(base) | normalize_zero_index_expr(index);
            if is_zero_index(index) {
                let ptr = std::mem::replace(base, Box::new(HirExpr::Const(0, NirType::Unknown)));
                let ty = elem_ty.clone();
                *lhs = HirLValue::Deref { ptr, ty };
                changed = true;
            }
            changed
        }
        HirLValue::Var(_) => false,
        HirLValue::FieldAccess { base, .. } => normalize_zero_index_expr(base),
    }
}

fn normalize_zero_index_expr(expr: &mut HirExpr) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= normalize_zero_index_expr(lhs);
            changed |= normalize_zero_index_expr(rhs);
        }
        HirExpr::Unary { expr, .. }
        | HirExpr::Cast { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => {
            changed |= normalize_zero_index_expr(expr);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= normalize_zero_index_expr(arg);
            }
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            changed |= normalize_zero_index_expr(base);
            changed |= normalize_zero_index_expr(index);
            if is_zero_index(index) {
                let ptr = std::mem::replace(base, Box::new(HirExpr::Const(0, NirType::Unknown)));
                let ty = elem_ty.clone();
                *expr = HirExpr::Load { ptr, ty };
                changed = true;
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= normalize_zero_index_expr(cond);
            changed |= normalize_zero_index_expr(then_expr);
            changed |= normalize_zero_index_expr(else_expr);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    fn make_binding_with_ty(name: &str, ty: NirType) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<NirBinding>, body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals,
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            ..Default::default()
        }
    }

    #[test]
    fn rewrites_zero_index_access_to_deref() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("value".to_owned()),
                rhs: HirExpr::Index {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    index: Box::new(HirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    elem_ty: elem_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    index: Box::new(HirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    elem_ty,
                },
                rhs: HirExpr::Var("value".to_owned()),
            },
        ];
        let mut func = make_func(vec![make_binding_with_ty("p", ptr_ty)], body);

        assert!(super::apply_zero_index_deref_pass(&mut func));
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                rhs: HirExpr::Load { .. },
                ..
            }
        ));
        assert!(matches!(
            &func.body[1],
            HirStmt::Assign {
                lhs: HirLValue::Deref { .. },
                ..
            }
        ));
    }

    /// Add(Var("p"), Const(8)) where p: Ptr(Aggregate) preserves byte-offset form
    /// for later field recovery.
    #[test]
    fn preserves_aggregate_add_const_as_ptr_offset() {
        let elem_ty = NirType::Aggregate {
            size: 16,
            fields: vec![],
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    8,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::PtrOffset { offset: 8, .. }));
        } else {
            panic!("expected assign");
        }
    }

    /// Pointer-typed Add(Var("p"), Const(8)) where p: Ptr(Aggregate) still
    /// preserves byte-offset form.  The raw p-code builder may type the address
    /// expression as a pointer before aggregate fields have been recovered.
    #[test]
    fn preserves_pointer_typed_aggregate_add_const_as_ptr_offset() {
        let elem_ty = NirType::Aggregate {
            size: 16,
            fields: vec![],
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    8,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Ptr(Box::new(elem_ty)),
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::PtrOffset { offset: 8, .. }));
        } else {
            panic!("expected assign");
        }
    }

    /// Add(Var("p"), Const(4)) where p: Ptr(uint32) → Add(Var("p"), Const(1)).
    #[test]
    fn rescales_scalar_add_const_to_typed_pointer_add() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    4,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs,
                    rhs,
                    ty: NirType::Ptr(_),
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "p")
                    && matches!(rhs.as_ref(), HirExpr::Const(1, _))
            ));
        } else {
            panic!("expected assign");
        }
    }

    /// Pointer-valued Add(Var("p"), Mul(Var("i"), Const(4))) where p: Ptr(uint32)
    /// keeps address semantics and becomes Add(Var("p"), Var("i")).
    #[test]
    fn converts_add_stride_to_pointer_add() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    lhs: Box::new(HirExpr::Var("i".to_owned())),
                    rhs: Box::new(HirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs,
                    rhs,
                    ty: NirType::Ptr(_),
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "p")
                    && matches!(rhs.as_ref(), HirExpr::Var(name) if name == "i")
            ));
        } else {
            panic!("expected assign");
        }
    }

    #[test]
    fn rescales_byte_cast_const_offset_to_typed_pointer_add() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let byte_ptr_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Cast {
                    ty: byte_ptr_ty,
                    expr: Box::new(HirExpr::Var("p".to_owned())),
                }),
                rhs: Box::new(HirExpr::Const(
                    4,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs,
                    rhs,
                    ty: NirType::Ptr(_),
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "p")
                    && matches!(rhs.as_ref(), HirExpr::Const(1, _))
            ));
        } else {
            panic!("expected assign");
        }
    }

    /// Load(Add(Var("p"), Mul(Var("i"), Const(4)))) where p: Ptr(uint32) → p[i].
    #[test]
    fn converts_load_stride_to_index_value() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("p".to_owned())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Mul,
                        lhs: Box::new(HirExpr::Var("i".to_owned())),
                        rhs: Box::new(HirExpr::Const(
                            4,
                            NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                ty: elem_ty.clone(),
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::Index { .. }));
        } else {
            panic!("expected assign");
        }
    }

    /// Store through a typed pointer constant offset should surface as p[k],
    /// including negative offsets emitted by post-increment store idioms.
    #[test]
    fn converts_deref_const_pointer_offset_to_index_lvalue() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(HirExpr::Var("p".to_owned())),
                    rhs: Box::new(HirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: p_ty.clone(),
                }),
                ty: elem_ty.clone(),
            },
            rhs: HirExpr::Var("value".to_owned()),
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

        let changed = super::apply_ptr_arith_recovery_pass(&mut func);

        assert!(changed);
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                lhs:
                    HirLValue::Index {
                        base,
                        index,
                        elem_ty: index_elem_ty,
                    },
                ..
            } if matches!(base.as_ref(), HirExpr::Var(name) if name == "p")
                && matches!(index.as_ref(), HirExpr::Const(-1, _))
                && index_elem_ty == &elem_ty
        ));
    }

    #[test]
    fn converts_load_const_pointer_offset_to_index_value() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("p".to_owned())),
                    rhs: Box::new(HirExpr::Const(
                        8,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: p_ty.clone(),
                }),
                ty: elem_ty.clone(),
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

        let changed = super::apply_ptr_arith_recovery_pass(&mut func);

        assert!(changed);
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                rhs:
                    HirExpr::Index {
                        base,
                        index,
                        elem_ty: index_elem_ty,
                    },
                ..
            } if matches!(base.as_ref(), HirExpr::Var(name) if name == "p")
                && matches!(index.as_ref(), HirExpr::Const(2, _))
                && index_elem_ty == &elem_ty
        ));
    }

    #[test]
    fn leaves_const_pointer_offset_deref_when_pointee_size_mismatches_access() {
        let pointee_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let access_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(pointee_ty));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("p".to_owned())),
                    rhs: Box::new(HirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: p_ty.clone(),
                }),
                ty: access_ty,
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);

        let changed = super::apply_ptr_arith_recovery_pass(&mut func);

        assert!(!changed);
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                rhs: HirExpr::Load { ptr, .. },
                ..
            } if matches!(ptr.as_ref(), HirExpr::Binary { .. })
        ));
    }

    #[test]
    fn casts_pointer_rhs_in_integer_subtraction() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("diff".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(HirExpr::Var("addr".to_owned())),
                rhs: Box::new(HirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(
            vec![
                make_binding_with_ty(
                    "addr",
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                ),
                make_binding_with_ty("p", p_ty),
            ],
            body,
        );
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs,
                    rhs,
                    ty: NirType::Int { bits: 64, signed: false },
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "addr")
                    && matches!(rhs.as_ref(), HirExpr::Cast {
                        ty: NirType::Int { bits: 64, signed: false },
                        expr,
                    } if matches!(expr.as_ref(), HirExpr::Var(name) if name == "p"))
            ));
        } else {
            panic!("expected assign");
        }
    }

    #[test]
    fn casts_both_pointer_operands_in_integer_subtraction() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("diff".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(HirExpr::Var("lhs".to_owned())),
                rhs: Box::new(HirExpr::Var("rhs".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(
            vec![
                make_binding_with_ty("lhs", p_ty.clone()),
                make_binding_with_ty("rhs", p_ty),
            ],
            body,
        );
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs,
                    rhs,
                    ty: NirType::Int { bits: 64, signed: false },
                } if matches!(lhs.as_ref(), HirExpr::Cast { expr, .. }
                        if matches!(expr.as_ref(), HirExpr::Var(name) if name == "lhs"))
                    && matches!(rhs.as_ref(), HirExpr::Cast { expr, .. }
                        if matches!(expr.as_ref(), HirExpr::Var(name) if name == "rhs"))
            ));
        } else {
            panic!("expected assign");
        }
    }

    #[test]
    fn refines_ptr_unknown_to_array_on_uniform_accesses() {
        let p_ty = NirType::Ptr(Box::new(NirType::Unknown));
        let body = vec![
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    offset: 0,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    offset: 4,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    offset: 8,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
        ];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        let NirType::Ptr(inner) = &func.locals[0].ty else {
            panic!("expected refined pointer ty");
        };
        assert!(matches!(inner.as_ref(), NirType::Int { bits: 32, .. }));
    }

    #[test]
    fn recovers_dynamic_index_scale_on_unknown_ptr() {
        let p_ty = NirType::Ptr(Box::new(NirType::Unknown));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("p".to_owned())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Mul,
                        lhs: Box::new(HirExpr::Var("i".to_owned())),
                        rhs: Box::new(HirExpr::Const(
                            4,
                            NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(
            vec![
                make_binding_with_ty("p", p_ty),
                make_binding_with_ty(
                    "i",
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                ),
            ],
            body,
        );
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::Index { .. }));
        } else {
            panic!("expected index assignment");
        }
    }

    #[test]
    fn preserves_aggregate_on_sparse_diverse_accesses() {
        let p_ty = NirType::Ptr(Box::new(NirType::Unknown));
        let body = vec![
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    offset: 4,
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }),
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    offset: 11,
                }),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            }),
            HirStmt::Expr(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    offset: 20,
                }),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            }),
        ];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        let NirType::Ptr(inner) = &func.locals[0].ty else {
            panic!("expected refined pointer ty");
        };
        assert!(matches!(inner.as_ref(), NirType::Aggregate { .. }));
    }

    #[test]
    fn recovers_aggregate_field_access() {
        let field_8 = fission_midend_core::StructField {
            offset: 8,
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
            name: "field_8".to_owned(),
        };
        let agg_ty = NirType::Aggregate {
            size: 24,
            fields: vec![field_8],
        };
        let p_ty = NirType::Ptr(Box::new(agg_ty));
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("x".to_owned()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("p".to_owned())),
                        offset: 8,
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("p".to_owned())),
                        offset: 16,
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                },
                rhs: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                ),
            },
        ];
        let mut func = make_func(
            vec![
                make_binding_with_ty("p", p_ty),
                make_binding_with_ty(
                    "x",
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            ],
            body,
        );
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);

        // Verify the load is now FieldAccess
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(
                matches!(rhs, HirExpr::FieldAccess { field_name, offset, .. } if field_name == "field_8" && *offset == 8),
                "expected FieldAccess for field_8, got {:?}",
                rhs
            );
        } else {
            panic!("expected assignment");
        }

        // Verify the store/deref is now FieldAccess (synthetic fallback)
        if let HirStmt::Assign { lhs, .. } = &func.body[1] {
            assert!(
                matches!(lhs, HirLValue::FieldAccess { field_name, offset, .. } if field_name == "field_16" && *offset == 16),
                "expected FieldAccess for field_16, got {:?}",
                lhs
            );
        } else {
            panic!("expected assignment");
        }
    }
}
