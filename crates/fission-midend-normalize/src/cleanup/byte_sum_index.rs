//! Recover low-byte truncation after a sum of two byte-ranged values.
//!
//! A widening copy from the low lane of a byte sum represents
//! `(byte_a + byte_b) mod 256`. If that lane boundary is lost and the copy is
//! materialized as `w = v` after `v = a + b`, downstream indexing can use the
//! unbounded sum. This pass restores the bit-vector operation as `w = v & 0xff`.

use crate::HashMap;
use crate::prelude::*;
use std::collections::HashSet;

/// Apply byte-sum index truncation recovery. Returns true if any rewrite ran.
pub fn apply_byte_sum_index_trunc(func: &mut PreHirFunction) -> bool {
    let mut type_map: HashMap<String, NirType> = HashMap::default();
    for b in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(b.name.clone(), b.ty.clone());
    }
    let mut index_vars = HashSet::new();
    collect_index_and_subpiece_uses(&func.body, &mut index_vars);

    let mut last_def: HashMap<String, PreHirExpr> = HashMap::default();
    let mut byte_ranged: HashMap<String, bool> = HashMap::default();
    // Names whose *last* def was a sum of two byte-ranged values (frozen at assign time).
    let mut byte_sum_names: HashMap<String, bool> = HashMap::default();
    let mut changed = false;
    rewrite_stmts(
        &mut func.body,
        &type_map,
        &index_vars,
        &mut last_def,
        &mut byte_ranged,
        &mut byte_sum_names,
        &mut changed,
    );
    changed
}

fn rewrite_stmts(
    stmts: &mut [PreHirStmt],
    type_map: &HashMap<String, NirType>,
    index_vars: &HashSet<String>,
    last_def: &mut HashMap<String, PreHirExpr>,
    byte_ranged: &mut HashMap<String, bool>,
    byte_sum_names: &mut HashMap<String, bool>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(
            stmt,
            type_map,
            index_vars,
            last_def,
            byte_ranged,
            byte_sum_names,
            changed,
        );
    }
}

fn rewrite_stmt(
    stmt: &mut PreHirStmt,
    type_map: &HashMap<String, NirType>,
    index_vars: &HashSet<String>,
    last_def: &mut HashMap<String, PreHirExpr>,
    byte_ranged: &mut HashMap<String, bool>,
    byte_sum_names: &mut HashMap<String, bool>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => {
            // Pattern: w = v  where v's last def was a byte-sum.
            if let PreHirExpr::Var(src) = rhs {
                if byte_sum_names.get(src).copied().unwrap_or(false) {
                    let source_ty = last_def
                        .get(src)
                        .and_then(|d| match d {
                            PreHirExpr::Binary { ty, .. } => Some(ty.clone()),
                            _ => None,
                        })
                        .unwrap_or(NirType::Int {
                            bits: 32,
                            signed: false,
                        });
                    let is_8bit_add = is_byte_int_ty(&source_ty);
                    let is_index_or_subpiece = index_vars.contains(name)
                        || name.starts_with("idx")
                        || name.starts_with("index")
                        || name.contains("offset");
                    if is_8bit_add || is_index_or_subpiece {
                        let ty = unsigned_mask_type(name, &source_ty, type_map);
                        *rhs = PreHirExpr::Binary {
                            op: PreHirBinaryOp::And,
                            lhs: Box::new(PreHirExpr::Var(src.clone())),
                            rhs: Box::new(PreHirExpr::Const(0xff, ty.clone())),
                            ty,
                        };
                        *changed = true;
                        byte_ranged.insert(name.clone(), true);
                        byte_sum_names.insert(name.clone(), false);
                        last_def.insert(name.clone(), rhs.clone());
                        return;
                    }
                }
            }

            let is_sum = is_byte_sum(rhs, byte_ranged, type_map);
            // A byte-sum itself is not byte-ranged (can exceed 255).
            let ranged = expr_is_byte_ranged(rhs, byte_ranged, type_map) && !is_sum;
            byte_ranged.insert(name.clone(), ranged);
            byte_sum_names.insert(name.clone(), is_sum);
            last_def.insert(name.clone(), rhs.clone());
        }
        PreHirStmt::Block(body) => {
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                index_vars,
                last_def,
                byte_ranged,
                byte_sum_names,
                changed,
            );
        }
        PreHirStmt::While { body, .. } => {
            let mut body_def = last_def.clone();
            let mut body_br = byte_ranged.clone();
            let mut body_sum = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                index_vars,
                &mut body_def,
                &mut body_br,
                &mut body_sum,
                changed,
            );
            last_def.retain(|k, v| body_def.get(k).is_some_and(|bv| bv == v));
            byte_ranged.retain(|k, v| body_br.get(k).copied() == Some(*v));
            byte_sum_names.retain(|k, v| body_sum.get(k).copied() == Some(*v));
        }
        PreHirStmt::DoWhile { body, .. } => {
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                index_vars,
                last_def,
                byte_ranged,
                byte_sum_names,
                changed,
            );
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let mut then_def = last_def.clone();
            let mut then_br = byte_ranged.clone();
            let mut then_sum = byte_sum_names.clone();
            let mut else_def = last_def.clone();
            let mut else_br = byte_ranged.clone();
            let mut else_sum = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                type_map,
                index_vars,
                &mut then_def,
                &mut then_br,
                &mut then_sum,
                changed,
            );
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                type_map,
                index_vars,
                &mut else_def,
                &mut else_br,
                &mut else_sum,
                changed,
            );
            last_def.retain(|k, v| {
                then_def.get(k).is_some_and(|tv| tv == v)
                    && else_def.get(k).is_some_and(|ev| ev == v)
            });
            byte_ranged.retain(|k, v| {
                then_br.get(k).copied() == Some(*v) && else_br.get(k).copied() == Some(*v)
            });
            byte_sum_names.retain(|k, v| {
                then_sum.get(k).copied() == Some(*v) && else_sum.get(k).copied() == Some(*v)
            });
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init.as_mut() {
                rewrite_stmt(
                    i,
                    type_map,
                    index_vars,
                    last_def,
                    byte_ranged,
                    byte_sum_names,
                    changed,
                );
            }
            let mut body_def = last_def.clone();
            let mut body_br = byte_ranged.clone();
            let mut body_sum = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                index_vars,
                &mut body_def,
                &mut body_br,
                &mut body_sum,
                changed,
            );
            if let Some(u) = update.as_mut() {
                rewrite_stmt(
                    u,
                    type_map,
                    index_vars,
                    &mut body_def,
                    &mut body_br,
                    &mut body_sum,
                    changed,
                );
            }
            last_def.retain(|k, v| body_def.get(k).is_some_and(|bv| bv == v));
            byte_ranged.retain(|k, v| body_br.get(k).copied() == Some(*v));
            byte_sum_names.retain(|k, v| body_sum.get(k).copied() == Some(*v));
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut branch_defs = Vec::new();
            let mut branch_brs = Vec::new();
            let mut branch_sums = Vec::new();
            for case in cases.iter_mut() {
                let mut d = last_def.clone();
                let mut b = byte_ranged.clone();
                let mut s = byte_sum_names.clone();
                rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    type_map,
                    index_vars,
                    &mut d,
                    &mut b,
                    &mut s,
                    changed,
                );
                branch_defs.push(d);
                branch_brs.push(b);
                branch_sums.push(s);
            }
            let mut d = last_def.clone();
            let mut b = byte_ranged.clone();
            let mut s = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                type_map,
                index_vars,
                &mut d,
                &mut b,
                &mut s,
                changed,
            );
            branch_defs.push(d);
            branch_brs.push(b);
            branch_sums.push(s);

            last_def.retain(|k, v| {
                branch_defs
                    .iter()
                    .all(|bd| bd.get(k).is_some_and(|bv| bv == v))
            });
            byte_ranged.retain(|k, v| branch_brs.iter().all(|bb| bb.get(k).copied() == Some(*v)));
            byte_sum_names
                .retain(|k, v| branch_sums.iter().all(|bs| bs.get(k).copied() == Some(*v)));
        }
        _ => {}
    }
}

fn unsigned_mask_type(
    destination: &str,
    source_ty: &NirType,
    type_map: &HashMap<String, NirType>,
) -> NirType {
    let (bits, signed) = type_map
        .get(destination)
        .map(|ty| {
            (
                int_type_width(ty).unwrap_or(32),
                int_type_signed(ty).unwrap_or(false),
            )
        })
        .or_else(|| {
            Some((
                int_type_width(source_ty).unwrap_or(32),
                int_type_signed(source_ty).unwrap_or(false),
            ))
        })
        .unwrap_or((32, false));
    NirType::Int { bits, signed }
}

fn int_type_signed(ty: &NirType) -> Option<bool> {
    match ty {
        NirType::Int { signed, .. } => Some(*signed),
        _ => None,
    }
}

fn int_type_width(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } if *bits > 0 => Some(*bits),
        _ => None,
    }
}

fn is_byte_sum(
    expr: &PreHirExpr,
    byte_ranged: &HashMap<String, bool>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            expr_is_byte_ranged(lhs, byte_ranged, type_map)
                && expr_is_byte_ranged(rhs, byte_ranged, type_map)
        }
        _ => false,
    }
}

fn is_byte_int_ty(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { bits, .. } if *bits <= 8)
}

fn ptr_elem_is_byte(ty: &NirType) -> bool {
    match ty {
        NirType::Ptr(inner) => is_byte_int_ty(inner),
        _ => false,
    }
}

fn expr_is_byte_ranged(
    expr: &PreHirExpr,
    byte_ranged: &HashMap<String, bool>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    match expr {
        PreHirExpr::Var(name) => {
            if byte_ranged.get(name).copied().unwrap_or(false) {
                return true;
            }
            // Typed as uchar / byte binding.
            type_map.get(name).is_some_and(is_byte_int_ty)
        }
        PreHirExpr::Load { ptr, ty, .. } => {
            if is_byte_int_ty(ty) {
                return true;
            }
            // `*uchar_ptr` may be typed as a wider int after promotion.
            if let PreHirExpr::Var(pn) = ptr.as_ref() {
                if type_map.get(pn).is_some_and(ptr_elem_is_byte) {
                    return true;
                }
            }
            match fission_midend_prehir::util::expr_type(ptr) {
                NirType::Ptr(inner) => is_byte_int_ty(&inner),
                _ => false,
            }
        }
        PreHirExpr::Cast { ty, expr: inner } => {
            is_byte_int_ty(ty) || expr_is_byte_ranged(inner, byte_ranged, type_map)
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            rhs,
            lhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (PreHirExpr::Const(m, _), _) | (_, PreHirExpr::Const(m, _)) => {
                let m = *m as u64;
                m == 0xff || m == 255
            }
            _ => false,
        },
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mod,
            rhs,
            ..
        } => matches!(rhs.as_ref(), PreHirExpr::Const(m, _) if *m == 256 || *m == 0x100),
        _ => false,
    }
}

fn collect_index_and_subpiece_uses(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                collect_lvalue_index_uses(lhs, out);
                collect_expr_index_uses(rhs, out);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_expr_index_uses(expr, out);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_expr_index_uses(cond, out);
                collect_index_and_subpiece_uses(then_body, out);
                collect_index_and_subpiece_uses(else_body, out);
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { cond, body } => {
                collect_expr_index_uses(cond, out);
                collect_index_and_subpiece_uses(body, out);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    collect_index_and_subpiece_uses(std::slice::from_ref(i), out);
                }
                if let Some(c) = cond {
                    collect_expr_index_uses(c, out);
                }
                if let Some(u) = update {
                    collect_index_and_subpiece_uses(std::slice::from_ref(u), out);
                }
                collect_index_and_subpiece_uses(body, out);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_expr_index_uses(expr, out);
                for case in cases {
                    collect_index_and_subpiece_uses(&case.body, out);
                }
                collect_index_and_subpiece_uses(default, out);
            }
            PreHirStmt::Block(body) => {
                collect_index_and_subpiece_uses(body, out);
            }
            _ => {}
        }
    }
}

fn collect_lvalue_index_uses(lhs: &PreHirLValue, out: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Index { base, index, .. } => {
            collect_vars_in_expr(index, out);
            collect_expr_index_uses(base, out);
        }
        PreHirLValue::Deref { ptr, .. } => {
            collect_expr_index_uses(ptr, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            collect_expr_index_uses(base, out);
        }
        _ => {}
    }
}

fn collect_expr_index_uses(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Index { base, index, .. } => {
            collect_vars_in_expr(index, out);
            collect_expr_index_uses(base, out);
        }
        PreHirExpr::PtrOffset { base, .. } => {
            collect_expr_index_uses(base, out);
        }
        PreHirExpr::Cast { ty, expr } => {
            if is_byte_int_ty(ty) {
                collect_vars_in_expr(expr, out);
            }
            collect_expr_index_uses(expr, out);
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs,
            rhs,
            ..
        } => {
            if is_byte_mask(lhs) {
                collect_vars_in_expr(rhs, out);
            } else if is_byte_mask(rhs) {
                collect_vars_in_expr(lhs, out);
            }
            collect_expr_index_uses(lhs, out);
            collect_expr_index_uses(rhs, out);
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add | PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ty,
        } => {
            if matches!(ty, NirType::Ptr(_)) {
                if !matches!(fission_midend_prehir::util::expr_type(lhs), NirType::Ptr(_)) {
                    collect_vars_in_expr(lhs, out);
                }
                if !matches!(fission_midend_prehir::util::expr_type(rhs), NirType::Ptr(_)) {
                    collect_vars_in_expr(rhs, out);
                }
            }
            collect_expr_index_uses(lhs, out);
            collect_expr_index_uses(rhs, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_expr_index_uses(lhs, out);
            collect_expr_index_uses(rhs, out);
        }
        PreHirExpr::Unary { expr, .. } => {
            collect_expr_index_uses(expr, out);
        }
        PreHirExpr::Load { ptr, .. } => {
            collect_expr_index_uses(ptr, out);
        }
        PreHirExpr::FieldAccess { base, .. } => {
            collect_expr_index_uses(base, out);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expr_index_uses(cond, out);
            collect_expr_index_uses(then_expr, out);
            collect_expr_index_uses(else_expr, out);
        }
        _ => {}
    }
}

fn collect_vars_in_expr(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_vars_in_expr(expr, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, out);
            collect_vars_in_expr(rhs, out);
        }
        _ => {}
    }
}

fn is_byte_mask(expr: &PreHirExpr) -> bool {
    matches!(expr, PreHirExpr::Const(m, _) if *m == 0xff || *m == 255)
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn u8_ty() -> NirType {
        NirType::Int {
            bits: 8,
            signed: false,
        }
    }

    fn i8_ty() -> NirType {
        NirType::Int {
            bits: 8,
            signed: true,
        }
    }

    fn binding(name: &str, ty: NirType) -> PreHirBinding {
        PreHirBinding {
            name: name.into(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    #[test]
    fn recovers_truncation_after_byte_sum_copy() {
        let mut func = PreHirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![],
            return_type: u32_ty(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("a".into()),
                    rhs: PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("p".into())),
                        ty: u8_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("b".into()),
                    rhs: PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("q".into())),
                        ty: u8_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("s".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".into())),
                        rhs: Box::new(PreHirExpr::Var("b".into())),
                        ty: u32_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("s".into()),
                },
            ],
            ..Default::default()
        };

        assert!(apply_byte_sum_index_trunc(&mut func));
        match &func.body[3] {
            PreHirStmt::Assign { rhs, .. } => match rhs {
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    rhs: mask,
                    ..
                } => {
                    assert!(matches!(mask.as_ref(), PreHirExpr::Const(0xff, _)));
                }
                other => panic!("expected masked assign, got {other:?}"),
            },
            _ => panic!("expected assign"),
        }
    }

    #[test]
    fn recovered_mask_uses_unsigned_destination_width() {
        let mut func = PreHirFunction {
            name: "signed_byte_sum".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", i8_ty()),
                binding("index", u32_ty()),
            ],
            return_type: u32_ty(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".into())),
                        rhs: Box::new(PreHirExpr::Var("b".into())),
                        ty: i8_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("index".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
            ],
            ..Default::default()
        };

        assert!(apply_byte_sum_index_trunc(&mut func));
        assert!(matches!(
            &func.body[1],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    rhs,
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    ..
                },
                ..
            } if matches!(
                rhs.as_ref(),
                PreHirExpr::Const(
                    0xff,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    }
                )
            )
        ));
    }

    #[test]
    fn does_not_mask_plain_32bit_sum_copy() {
        let mut func = PreHirFunction {
            name: "plain_sum".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u32_ty()),
                binding("copy", u32_ty()),
            ],
            return_type: u32_ty(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".into())),
                        rhs: Box::new(PreHirExpr::Var("b".into())),
                        ty: u32_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("copy".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_byte_sum_index_trunc(&mut func));
    }

    #[test]
    fn preserves_signed_destination_type() {
        let mut func = PreHirFunction {
            name: "signed_dest".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", i8_ty()),
                binding(
                    "idx",
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            ],
            return_type: u32_ty(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".into())),
                        rhs: Box::new(PreHirExpr::Var("b".into())),
                        ty: i8_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
            ],
            ..Default::default()
        };

        assert!(apply_byte_sum_index_trunc(&mut func));
        assert!(matches!(
            &func.body[1],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn while_loop_does_not_leak_inner_definition_to_post_loop() {
        let mut func = PreHirFunction {
            name: "while_leak".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u8_ty()),
                binding("idx", u32_ty()),
            ],
            return_type: u32_ty(),
            body: vec![
                PreHirStmt::While {
                    cond: PreHirExpr::Const(1, u32_ty()),
                    body: std::rc::Rc::new(vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("sum".into()),
                        rhs: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(PreHirExpr::Var("a".into())),
                            rhs: Box::new(PreHirExpr::Var("b".into())),
                            ty: u8_ty(),
                        },
                    }]),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_byte_sum_index_trunc(&mut func));
    }

    #[test]
    fn for_loop_kills_pre_loop_definition_when_modified_in_body() {
        let mut func = PreHirFunction {
            name: "for_kill".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u8_ty()),
                binding("idx", u32_ty()),
            ],
            return_type: u32_ty(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("a".into())),
                        rhs: Box::new(PreHirExpr::Var("b".into())),
                        ty: u8_ty(),
                    },
                },
                PreHirStmt::For {
                    init: None,
                    cond: None,
                    update: None,
                    body: std::rc::Rc::new(vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("sum".into()),
                        rhs: PreHirExpr::Const(1000, u32_ty()),
                    }]),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_byte_sum_index_trunc(&mut func));
    }
}
