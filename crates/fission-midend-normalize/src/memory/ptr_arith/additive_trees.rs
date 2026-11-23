use super::PointerLayout;
use super::address_terms::{try_extract_index_mul, typed_pointer_base};
use super::pointer_types::{pointee_ty, type_byte_size};
use crate::HashMap;
use crate::prelude::*;

struct AddTreeState {
    ptr_expr: PreHirExpr,
    ptr_ty: NirType,
    pointer_layout: PointerLayout,
    elem_size: i64,
    multiples: Vec<(PreHirExpr, i64)>,
    mult_const: i64,
    non_multiples: Vec<(PreHirExpr, i64)>,
    non_mult_const: i64,
    other_terms: Vec<PreHirExpr>,
    preserve_const_bytes: bool,
    valid: bool,
}

impl AddTreeState {
    fn new(
        ptr_expr: PreHirExpr,
        ptr_ty: NirType,
        pointer_layout: PointerLayout,
        preserve_const_bytes: bool,
    ) -> Self {
        let elem_ty = pointee_ty(&ptr_ty).cloned().unwrap_or(NirType::Unknown);
        let elem_size = type_byte_size(&elem_ty, pointer_layout).unwrap_or(1) as i64;
        let elem_size = if elem_size <= 0 { 1 } else { elem_size };
        Self {
            ptr_expr,
            ptr_ty,
            pointer_layout,
            elem_size,
            multiples: Vec::new(),
            mult_const: 0,
            non_multiples: Vec::new(),
            non_mult_const: 0,
            other_terms: Vec::new(),
            preserve_const_bytes,
            valid: true,
        }
    }

    fn span_add_tree(&mut self, expr: &PreHirExpr, coeff: i64) {
        if !self.valid {
            return;
        }
        match expr {
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs,
                rhs,
                ..
            } => {
                self.span_add_tree(lhs, coeff);
                self.span_add_tree(rhs, coeff);
            }
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs,
                rhs,
                ..
            } => {
                self.span_add_tree(lhs, coeff);
                self.span_add_tree(rhs, -coeff);
            }
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs,
                rhs,
                ..
            } => {
                if let Some((idx, stride)) = try_extract_index_mul(expr) {
                    let total_stride = stride.wrapping_mul(coeff);
                    if matches!(
                        idx,
                        PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add | PreHirBinaryOp::Sub,
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
            PreHirExpr::Const(k, _) => {
                let val = k.wrapping_mul(coeff);
                self.add_const_term(val);
            }
            PreHirExpr::Unary {
                op: PreHirUnaryOp::Neg,
                expr: inner,
                ..
            } if matches!(inner.as_ref(), PreHirExpr::Const(_, _)) => {
                if let PreHirExpr::Const(k, _) = inner.as_ref() {
                    self.add_const_term(k.wrapping_neg().wrapping_mul(coeff));
                }
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

    fn add_index_term(&mut self, idx: PreHirExpr, stride: i64) {
        if stride % self.elem_size == 0 {
            self.multiples.push((idx, stride));
        } else {
            self.non_multiples.push((idx, stride));
        }
    }

    fn add_other_term(&mut self, expr: PreHirExpr, coeff: i64) {
        if coeff == 1 {
            self.other_terms.push(expr);
        } else if coeff == -1 {
            self.other_terms.push(PreHirExpr::Unary {
                op: PreHirUnaryOp::Neg,
                expr: Box::new(expr),
                ty: NirType::Unknown,
            });
        } else {
            if coeff % self.elem_size == 0 {
                self.multiples.push((expr, coeff));
            } else {
                self.other_terms.push(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(expr),
                    rhs: Box::new(PreHirExpr::Const(coeff, NirType::Unknown)),
                    ty: NirType::Unknown,
                });
            }
        }
    }

    fn build_tree(&self) -> Option<PreHirExpr> {
        if !self.valid {
            return None;
        }

        let mut index_terms: Vec<PreHirExpr> = Vec::new();

        for (idx, stride) in &self.multiples {
            let mult = stride / self.elem_size;
            if mult == 1 {
                index_terms.push(idx.clone());
            } else if mult == -1 {
                index_terms.push(PreHirExpr::Unary {
                    op: PreHirUnaryOp::Neg,
                    expr: Box::new(idx.clone()),
                    ty: NirType::Unknown,
                });
            } else {
                index_terms.push(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(idx.clone()),
                    rhs: Box::new(PreHirExpr::Const(mult, NirType::Unknown)),
                    ty: NirType::Unknown,
                });
            }
        }

        let mult_const_elements = self.mult_const / self.elem_size;
        if !self.preserve_const_bytes && mult_const_elements != 0 {
            index_terms.push(PreHirExpr::Const(
                mult_const_elements,
                NirType::Int {
                    bits: self.pointer_layout.bits,
                    signed: mult_const_elements < 0,
                },
            ));
        }

        let mut index_sum: Option<PreHirExpr> = None;
        for term in index_terms {
            if let Some(sum) = index_sum {
                index_sum = Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
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
            base = PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(base),
                rhs: Box::new(idx_expr),
                ty: self.ptr_ty.clone(),
            };
        }

        if (!self.non_multiples.is_empty() || !self.other_terms.is_empty()) && self.elem_size > 1 {
            let byte_ptr_ty = NirType::Ptr(Box::new(NirType::Int {
                bits: 8,
                signed: false,
            }));
            base = PreHirExpr::Cast {
                ty: byte_ptr_ty.clone(),
                expr: Box::new(base),
            };
            for (idx, stride) in &self.non_multiples {
                let term = PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(idx.clone()),
                    rhs: Box::new(PreHirExpr::Const(*stride, NirType::Unknown)),
                    ty: NirType::Unknown,
                };
                base = PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(base),
                    rhs: Box::new(term),
                    ty: byte_ptr_ty.clone(),
                };
            }
            for other in &self.other_terms {
                base = PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(base),
                    rhs: Box::new(other.clone()),
                    ty: byte_ptr_ty.clone(),
                };
            }
            base = PreHirExpr::Cast {
                ty: self.ptr_ty.clone(),
                expr: Box::new(base),
            };
        } else {
            for (idx, stride) in &self.non_multiples {
                let term = PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(idx.clone()),
                    rhs: Box::new(PreHirExpr::Const(*stride, NirType::Unknown)),
                    ty: NirType::Unknown,
                };
                base = PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(base),
                    rhs: Box::new(term),
                    ty: self.ptr_ty.clone(),
                };
            }
            for other in &self.other_terms {
                base = PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(base),
                    rhs: Box::new(other.clone()),
                    ty: self.ptr_ty.clone(),
                };
            }
        }

        if self.non_mult_const != 0 {
            base = PreHirExpr::PtrOffset {
                base: Box::new(base),
                offset: self.non_mult_const,
            };
        }

        if self.preserve_const_bytes && self.mult_const != 0 {
            base = PreHirExpr::PtrOffset {
                base: Box::new(base),
                offset: self.mult_const,
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
pub(super) fn try_recover_ptr_arith_tree(
    expr: &PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> Option<PreHirExpr> {
    // Only applies to Add/Sub trees.
    if !matches!(
        expr,
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add | PreHirBinaryOp::Sub,
            ..
        }
    ) {
        return None;
    }

    // Find the pointer base (must be a leaf or Cast(ptr8, leaf)).
    // We walk the left-most Add chain looking for a pointer.
    let mut ptr_side: Option<(PreHirExpr, NirType)> = None;
    let mut non_ptr_accum: Vec<PreHirExpr> = Vec::new();
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

    let preserve_const_bytes = !matches!(expr_type(expr), NirType::Ptr(_));
    let mut state = AddTreeState::new(ptr_expr, ptr_ty, pointer_layout, preserve_const_bytes);
    for term in non_ptr_accum {
        state.span_add_tree(&term, 1);
    }

    state.build_tree()
}

/// Collect additive terms from a possibly-nested ADD tree, separating out the
/// pointer base from the non-pointer terms.
fn collect_add_terms_with_ptr(
    expr: &PreHirExpr,
    neg: bool,
    binding_types: &HashMap<String, NirType>,
    ptr_side: &mut Option<(PreHirExpr, NirType)>,
    non_ptr: &mut Vec<PreHirExpr>,
) {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            collect_add_terms_with_ptr(lhs, neg, binding_types, ptr_side, non_ptr);
            collect_add_terms_with_ptr(rhs, neg, binding_types, ptr_side, non_ptr);
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
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
                non_ptr.push(PreHirExpr::Unary {
                    op: PreHirUnaryOp::Neg,
                    expr: Box::new(other.clone()),
                    ty: NirType::Unknown,
                });
            } else {
                non_ptr.push(other.clone());
            }
        }
    }
}
