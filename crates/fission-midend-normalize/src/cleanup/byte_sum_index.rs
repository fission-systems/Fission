//! Recover low-byte truncation after a sum of two byte-ranged values.
//!
//! A widening copy from the low lane of a byte sum represents
//! `(byte_a + byte_b) mod 256`. If that lane boundary is lost and the copy is
//! materialized as `w = v` after `v = a + b`, downstream indexing can use the
//! unbounded sum. This pass restores the bit-vector operation as `w = v & 0xff`.

use crate::prelude::*;
use crate::HashMap;

/// Apply byte-sum index truncation recovery. Returns true if any rewrite ran.
pub fn apply_byte_sum_index_trunc(func: &mut DirFunction) -> bool {
    let mut type_map: HashMap<String, NirType> = HashMap::default();
    for b in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(b.name.clone(), b.ty.clone());
    }
    let mut last_def: HashMap<String, DirExpr> = HashMap::default();
    let mut byte_ranged: HashMap<String, bool> = HashMap::default();
    // Names whose *last* def was a sum of two byte-ranged values (frozen at assign time).
    let mut byte_sum_names: HashMap<String, bool> = HashMap::default();
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
    stmts: &mut [DirStmt],
    type_map: &HashMap<String, NirType>,
    last_def: &mut HashMap<String, DirExpr>,
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
    stmt: &mut DirStmt,
    type_map: &HashMap<String, NirType>,
    last_def: &mut HashMap<String, DirExpr>,
    byte_ranged: &mut HashMap<String, bool>,
    byte_sum_names: &mut HashMap<String, bool>,
    changed: &mut bool,
) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => {
            // Pattern: w = v  where v's last def was a byte-sum.
            if let DirExpr::Var(src) = rhs {
                if byte_sum_names.get(src).copied().unwrap_or(false) {
                    let source_ty = last_def
                        .get(src)
                        .and_then(|d| match d {
                            DirExpr::Binary { ty, .. } => Some(ty.clone()),
                            _ => None,
                        })
                        .unwrap_or(NirType::Int {
                            bits: 32,
                            signed: false,
                        });
                    let ty = unsigned_mask_type(name, &source_ty, type_map);
                    *rhs = DirExpr::Binary {
                        op: DirBinaryOp::And,
                        lhs: Box::new(DirExpr::Var(src.clone())),
                        rhs: Box::new(DirExpr::Const(0xff, ty.clone())),
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
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            rewrite_stmts(
                body,
                type_map,
                last_def,
                byte_ranged,
                byte_sum_names,
                changed,
            );
        }
        DirStmt::If {
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
        DirStmt::For {
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
        DirStmt::Switch { cases, default, .. } => {
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
    expr: &DirExpr,
    byte_ranged: &HashMap<String, bool>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Add,
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
    expr: &DirExpr,
    byte_ranged: &HashMap<String, bool>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    match expr {
        DirExpr::Var(name) => {
            if byte_ranged.get(name).copied().unwrap_or(false) {
                return true;
            }
            // Typed as uchar / byte binding.
            type_map.get(name).is_some_and(is_byte_int_ty)
        }
        DirExpr::Load { ptr, ty, .. } => {
            if is_byte_int_ty(ty) {
                return true;
            }
            // `*uchar_ptr` may be typed as a wider int after promotion.
            if let DirExpr::Var(pn) = ptr.as_ref() {
                if type_map.get(pn).is_some_and(ptr_elem_is_byte) {
                    return true;
                }
            }
            match fission_midend_dir::util::expr_type(ptr) {
                NirType::Ptr(inner) => is_byte_int_ty(&inner),
                _ => false,
            }
        }
        DirExpr::Cast { ty, expr: inner } => {
            is_byte_int_ty(ty) || expr_is_byte_ranged(inner, byte_ranged, type_map)
        }
        DirExpr::Binary {
            op: DirBinaryOp::And,
            rhs,
            lhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (DirExpr::Const(m, _), _) | (_, DirExpr::Const(m, _)) => {
                let m = *m as u64;
                m == 0xff || m == 255
            }
            _ => false,
        },
        DirExpr::Binary {
            op: DirBinaryOp::Mod,
            rhs,
            ..
        } => matches!(rhs.as_ref(), DirExpr::Const(m, _) if *m == 256 || *m == 0x100),
        _ => false,
    }
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

    fn binding(name: &str, ty: NirType) -> DirBinding {
        DirBinding {
            name: name.into(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    #[test]
    fn recovers_truncation_after_byte_sum_copy() {
        let mut func = DirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![],
            return_type: u32_ty(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("a".into()),
                    rhs: DirExpr::Load {
                        ptr: Box::new(DirExpr::Var("p".into())),
                        ty: u8_ty(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("b".into()),
                    rhs: DirExpr::Load {
                        ptr: Box::new(DirExpr::Var("q".into())),
                        ty: u8_ty(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("s".into()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("a".into())),
                        rhs: Box::new(DirExpr::Var("b".into())),
                        ty: u32_ty(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("idx".into()),
                    rhs: DirExpr::Var("s".into()),
                },
            ],
            ..Default::default()
        };

        assert!(apply_byte_sum_index_trunc(&mut func));
        match &func.body[3] {
            DirStmt::Assign { rhs, .. } => match rhs {
                DirExpr::Binary {
                    op: DirBinaryOp::And,
                    rhs: mask,
                    ..
                } => {
                    assert!(matches!(mask.as_ref(), DirExpr::Const(0xff, _)));
                }
                other => panic!("expected masked assign, got {other:?}"),
            },
            _ => panic!("expected assign"),
        }
    }

    #[test]
    fn recovered_mask_uses_unsigned_destination_width() {
        let mut func = DirFunction {
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
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".into()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("a".into())),
                        rhs: Box::new(DirExpr::Var("b".into())),
                        ty: i8_ty(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("index".into()),
                    rhs: DirExpr::Var("sum".into()),
                },
            ],
            ..Default::default()
        };

        assert!(apply_byte_sum_index_trunc(&mut func));
        assert!(matches!(
            &func.body[1],
            DirStmt::Assign {
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::And,
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
                DirExpr::Const(
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
