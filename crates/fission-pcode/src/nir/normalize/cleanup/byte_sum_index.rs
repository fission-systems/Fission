//! Recover low-byte truncation after a sum of two byte-ranged values.
//!
//! A widening copy from the low lane of a byte sum represents
//! `(byte_a + byte_b) mod 256`. If that lane boundary is lost and the copy is
//! materialized as `w = v` after `v = a + b`, downstream indexing can use the
//! unbounded sum. This pass restores the bit-vector operation as `w = v & 0xff`.

use super::super::*;
use std::collections::HashMap;

/// Apply byte-sum index truncation recovery. Returns true if any rewrite ran.
pub(crate) fn apply_byte_sum_index_trunc(func: &mut HirFunction) -> bool {
    let mut type_map: HashMap<String, NirType> = HashMap::new();
    for b in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(b.name.clone(), b.ty.clone());
    }
    let mut last_def: HashMap<String, HirExpr> = HashMap::new();
    let mut byte_ranged: HashMap<String, bool> = HashMap::new();
    // Names whose *last* def was a sum of two byte-ranged values (frozen at assign time).
    let mut byte_sum_names: HashMap<String, bool> = HashMap::new();
    let mut changed = false;
    rewrite_stmts(
        &mut func.body,
        &type_map,
        &mut last_def,
        &mut byte_ranged,
        &mut byte_sum_names,
        &mut changed,
    );
    changed
}

fn rewrite_stmts(
    stmts: &mut [HirStmt],
    type_map: &HashMap<String, NirType>,
    last_def: &mut HashMap<String, HirExpr>,
    byte_ranged: &mut HashMap<String, bool>,
    byte_sum_names: &mut HashMap<String, bool>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(
            stmt,
            type_map,
            last_def,
            byte_ranged,
            byte_sum_names,
            changed,
        );
    }
}

fn rewrite_stmt(
    stmt: &mut HirStmt,
    type_map: &HashMap<String, NirType>,
    last_def: &mut HashMap<String, HirExpr>,
    byte_ranged: &mut HashMap<String, bool>,
    byte_sum_names: &mut HashMap<String, bool>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            // Pattern: w = v  where v's last def was a byte-sum.
            if let HirExpr::Var(src) = rhs {
                if byte_sum_names.get(src).copied().unwrap_or(false) {
                    let source_ty = last_def
                        .get(src)
                        .and_then(|d| match d {
                            HirExpr::Binary { ty, .. } => Some(ty.clone()),
                            _ => None,
                        })
                        .unwrap_or(NirType::Int {
                            bits: 32,
                            signed: false,
                        });
                    let ty = unsigned_mask_type(name, &source_ty, type_map);
                    *rhs = HirExpr::Binary {
                        op: HirBinaryOp::And,
                        lhs: Box::new(HirExpr::Var(src.clone())),
                        rhs: Box::new(HirExpr::Const(0xff, ty.clone())),
                        ty,
                    };
                    *changed = true;
                    byte_ranged.insert(name.clone(), true);
                    byte_sum_names.insert(name.clone(), false);
                    last_def.insert(name.clone(), rhs.clone());
                    return;
                }
            }

            let is_sum = is_byte_sum(rhs, byte_ranged, type_map);
            // A byte-sum itself is not byte-ranged (can exceed 255).
            let ranged = expr_is_byte_ranged(rhs, byte_ranged, type_map) && !is_sum;
            byte_ranged.insert(name.clone(), ranged);
            byte_sum_names.insert(name.clone(), is_sum);
            last_def.insert(name.clone(), rhs.clone());
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            rewrite_stmts(
                body,
                type_map,
                last_def,
                byte_ranged,
                byte_sum_names,
                changed,
            );
        }
        HirStmt::If {
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
                then_body,
                type_map,
                &mut then_def,
                &mut then_br,
                &mut then_sum,
                changed,
            );
            rewrite_stmts(
                else_body,
                type_map,
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
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init.as_mut() {
                rewrite_stmt(i, type_map, last_def, byte_ranged, byte_sum_names, changed);
            }
            let mut body_def = last_def.clone();
            let mut body_br = byte_ranged.clone();
            let mut body_sum = byte_sum_names.clone();
            rewrite_stmts(
                body,
                type_map,
                &mut body_def,
                &mut body_br,
                &mut body_sum,
                changed,
            );
            if let Some(u) = update.as_mut() {
                rewrite_stmt(
                    u,
                    type_map,
                    &mut body_def,
                    &mut body_br,
                    &mut body_sum,
                    changed,
                );
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                let mut d = last_def.clone();
                let mut b = byte_ranged.clone();
                let mut s = byte_sum_names.clone();
                rewrite_stmts(&mut case.body, type_map, &mut d, &mut b, &mut s, changed);
            }
            let mut d = last_def.clone();
            let mut b = byte_ranged.clone();
            let mut s = byte_sum_names.clone();
            rewrite_stmts(default, type_map, &mut d, &mut b, &mut s, changed);
        }
        _ => {}
    }
}

fn unsigned_mask_type(
    destination: &str,
    source_ty: &NirType,
    type_map: &HashMap<String, NirType>,
) -> NirType {
    let bits = type_map
        .get(destination)
        .and_then(int_type_width)
        .or_else(|| int_type_width(source_ty))
        .unwrap_or(32);
    NirType::Int {
        bits,
        signed: false,
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
    expr: &HirExpr,
    byte_ranged: &HashMap<String, bool>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
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
    expr: &HirExpr,
    byte_ranged: &HashMap<String, bool>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    match expr {
        HirExpr::Var(name) => {
            if byte_ranged.get(name).copied().unwrap_or(false) {
                return true;
            }
            // Typed as uchar / byte binding.
            type_map.get(name).is_some_and(is_byte_int_ty)
        }
        HirExpr::Load { ptr, ty, .. } => {
            if is_byte_int_ty(ty) {
                return true;
            }
            // `*uchar_ptr` may be typed as a wider int after promotion.
            if let HirExpr::Var(pn) = ptr.as_ref() {
                if type_map.get(pn).is_some_and(ptr_elem_is_byte) {
                    return true;
                }
            }
            match crate::nir::support::expr_type(ptr) {
                NirType::Ptr(inner) => is_byte_int_ty(&inner),
                _ => false,
            }
        }
        HirExpr::Cast { ty, expr: inner } => {
            is_byte_int_ty(ty) || expr_is_byte_ranged(inner, byte_ranged, type_map)
        }
        HirExpr::Binary {
            op: HirBinaryOp::And,
            rhs,
            lhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (HirExpr::Const(m, _), _) | (_, HirExpr::Const(m, _)) => {
                let m = *m as u64;
                m == 0xff || m == 255
            }
            _ => false,
        },
        HirExpr::Binary {
            op: HirBinaryOp::Mod,
            rhs,
            ..
        } => matches!(rhs.as_ref(), HirExpr::Const(m, _) if *m == 256 || *m == 0x100),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn binding(name: &str, ty: NirType) -> NirBinding {
        NirBinding {
            name: name.into(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    #[test]
    fn recovers_truncation_after_byte_sum_copy() {
        let mut func = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![],
            return_type: u32_ty(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("a".into()),
                    rhs: HirExpr::Load {
                        ptr: Box::new(HirExpr::Var("p".into())),
                        ty: u8_ty(),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("b".into()),
                    rhs: HirExpr::Load {
                        ptr: Box::new(HirExpr::Var("q".into())),
                        ty: u8_ty(),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("s".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("a".into())),
                        rhs: Box::new(HirExpr::Var("b".into())),
                        ty: u32_ty(),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("idx".into()),
                    rhs: HirExpr::Var("s".into()),
                },
            ],
            ..Default::default()
        };

        assert!(apply_byte_sum_index_trunc(&mut func));
        match &func.body[3] {
            HirStmt::Assign { rhs, .. } => match rhs {
                HirExpr::Binary {
                    op: HirBinaryOp::And,
                    rhs: mask,
                    ..
                } => {
                    assert!(matches!(mask.as_ref(), HirExpr::Const(0xff, _)));
                }
                other => panic!("expected masked assign, got {other:?}"),
            },
            _ => panic!("expected assign"),
        }
    }

    #[test]
    fn recovered_mask_uses_unsigned_destination_width() {
        let mut func = HirFunction {
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
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("a".into())),
                        rhs: Box::new(HirExpr::Var("b".into())),
                        ty: i8_ty(),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("index".into()),
                    rhs: HirExpr::Var("sum".into()),
                },
            ],
            ..Default::default()
        };

        assert!(apply_byte_sum_index_trunc(&mut func));
        assert!(matches!(
            &func.body[1],
            HirStmt::Assign {
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::And,
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
                HirExpr::Const(
                    0xff,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    }
                )
            )
        ));
    }
}
