use super::merge_types::type_byte_size;
use crate::prelude::*;

pub(super) fn get_stack_span_from_parts(
    origin: Option<NirBindingOrigin>,
    ty: &NirType,
) -> Option<(i64, u32, bool)> {
    let (offset, is_derived) = match origin {
        Some(NirBindingOrigin::StackOffset(o))
        | Some(NirBindingOrigin::HomeSlot(o))
        | Some(NirBindingOrigin::OutgoingArgSlot(o)) => (o, false),
        Some(NirBindingOrigin::DerivedFromStackOffset(o)) => (o, true),
        _ => return None,
    };
    let size = match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => bits / 8,
        NirType::Ptr(_) => 8,
        NirType::Aggregate { size, .. } => *size,
        NirType::Float { bits } => bits / 8,
        NirType::Unknown => 4,
    };
    Some((offset, size, is_derived))
}

pub(super) fn spans_overlap(s1: (i64, u32), s2: (i64, u32)) -> bool {
    let (off1, sz1) = s1;
    let (off2, sz2) = s2;
    off1 < off2 + sz2 as i64 && off2 < off1 + sz1 as i64
}

/// Rewrite a scalar stack view as an element access into a proven aggregate.
///
/// The caller must establish the storage relationship from an actual stack
/// layout/type proof. Keeping the AST rewrite here gives earlier debug-layout
/// recovery one canonical expression traversal instead of duplicating it.
pub fn rewrite_stack_view_as_array_element(
    body: &mut [PreHirStmt],
    source: &str,
    base: &str,
    offset: i64,
    element_ty: NirType,
) {
    for stmt in body {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                rewrite_overlapping_stack_view_lvalue(lhs, source, base, offset, &element_ty);
                rewrite_overlapping_stack_view_expr(rhs, source, base, offset, &element_ty);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                rewrite_overlapping_stack_view_expr(expr, source, base, offset, &element_ty)
            }
            PreHirStmt::VaStart { va_list, .. } => {
                rewrite_overlapping_stack_view_expr(va_list, source, base, offset, &element_ty)
            }
            PreHirStmt::Block(stmts)
            | PreHirStmt::While { body: stmts, .. }
            | PreHirStmt::DoWhile { body: stmts, .. }
            | PreHirStmt::For { body: stmts, .. } => {
                let nested = std::rc::Rc::make_mut(stmts);
                rewrite_stack_view_as_array_element(
                    nested.as_mut_slice(),
                    source,
                    base,
                    offset,
                    element_ty.clone(),
                )
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rewrite_overlapping_stack_view_expr(expr, source, base, offset, &element_ty);
                for case in cases {
                    let nested = std::rc::Rc::make_mut(&mut case.body);
                    rewrite_stack_view_as_array_element(
                        nested.as_mut_slice(),
                        source,
                        base,
                        offset,
                        element_ty.clone(),
                    );
                }
                let nested = std::rc::Rc::make_mut(default);
                rewrite_stack_view_as_array_element(
                    nested.as_mut_slice(),
                    source,
                    base,
                    offset,
                    element_ty.clone(),
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rewrite_overlapping_stack_view_expr(cond, source, base, offset, &element_ty);
                let nested_then = std::rc::Rc::make_mut(then_body);
                rewrite_stack_view_as_array_element(
                    nested_then.as_mut_slice(),
                    source,
                    base,
                    offset,
                    element_ty.clone(),
                );
                let nested_else = std::rc::Rc::make_mut(else_body);
                rewrite_stack_view_as_array_element(
                    nested_else.as_mut_slice(),
                    source,
                    base,
                    offset,
                    element_ty.clone(),
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

fn rewrite_overlapping_stack_view_lvalue(
    lvalue: &mut PreHirLValue,
    source: &str,
    base: &str,
    offset: i64,
    element_ty: &NirType,
) {
    match lvalue {
        PreHirLValue::Var(name) if name == source => {
            let element_index = array_element_index(offset, element_ty);
            *lvalue = PreHirLValue::Index {
                base: Box::new(PreHirExpr::Var(base.to_string())),
                index: Box::new(PreHirExpr::Const(
                    element_index,
                    NirType::Int {
                        bits: 64,
                        signed: true,
                    },
                )),
                elem_ty: element_ty.clone(),
            };
        }
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => {
            rewrite_overlapping_stack_view_expr(ptr, source, base, offset, element_ty)
        }
        PreHirLValue::Index {
            base: inner, index, ..
        } => {
            rewrite_overlapping_stack_view_expr(inner, source, base, offset, element_ty);
            rewrite_overlapping_stack_view_expr(index, source, base, offset, element_ty);
        }
        PreHirLValue::FieldAccess { base: inner, .. } => {
            rewrite_overlapping_stack_view_expr(inner, source, base, offset, element_ty)
        }
    }
}

fn rewrite_overlapping_stack_view_expr(
    expr: &mut PreHirExpr,
    source: &str,
    base: &str,
    offset: i64,
    element_ty: &NirType,
) {
    match expr {
        PreHirExpr::Var(name) if name == source => {
            let element_index = array_element_index(offset, element_ty);
            *expr = PreHirExpr::Index {
                base: Box::new(PreHirExpr::Var(base.to_string())),
                index: Box::new(PreHirExpr::Const(
                    element_index,
                    NirType::Int {
                        bits: 64,
                        signed: true,
                    },
                )),
                elem_ty: element_ty.clone(),
            };
        }
        PreHirExpr::AddressOfLocal(name) if name == source => {
            *expr = PreHirExpr::PtrOffset {
                base: Box::new(PreHirExpr::AddressOfLocal(base.to_string())),
                offset,
            };
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfLocal(_) => {}
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            rewrite_overlapping_stack_view_expr(inner, source, base, offset, element_ty)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite_overlapping_stack_view_expr(lhs, source, base, offset, element_ty);
            rewrite_overlapping_stack_view_expr(rhs, source, base, offset, element_ty);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_overlapping_stack_view_expr(cond, source, base, offset, element_ty);
            rewrite_overlapping_stack_view_expr(then_expr, source, base, offset, element_ty);
            rewrite_overlapping_stack_view_expr(else_expr, source, base, offset, element_ty);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rewrite_overlapping_stack_view_expr(arg, source, base, offset, element_ty);
            }
        }
        PreHirExpr::Index {
            base: inner, index, ..
        } => {
            rewrite_overlapping_stack_view_expr(inner, source, base, offset, element_ty);
            rewrite_overlapping_stack_view_expr(index, source, base, offset, element_ty);
        }
    }
}

fn array_element_index(byte_offset: i64, element_ty: &NirType) -> i64 {
    let element_size = i64::from(type_byte_size(element_ty).unwrap_or(1).max(1));
    byte_offset / element_size
}
