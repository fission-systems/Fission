//! Recover low-byte truncation after a sum of two byte-ranged values.
//!
//! A widening copy from the low lane of an 8-bit sum represents
//! `(byte_a + byte_b) mod 256`. If that lane boundary is lost and the copy is
//! materialized as `w = v` after `v = a + b`, downstream indexing can use the
//! unbounded sum. This pass restores the bit-vector operation as `w = v & 0xff`
//! only when both the producer width and downstream use prove that truncation.

use std::collections::HashSet;

use crate::HashMap;
use crate::prelude::*;

/// Apply byte-sum index truncation recovery. Returns true if any rewrite ran.
pub fn apply_byte_sum_index_trunc(func: &mut PreHirFunction) -> bool {
    let mut type_map: HashMap<String, NirType> = HashMap::default();
    for b in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(b.name.clone(), b.ty.clone());
    }

    // A low-byte mask is meaningful only when the copied value is actually
    // consumed as an index. Range information alone is not a reason to alter
    // a full-width scalar copy.
    let mut index_consumers = HashSet::new();
    collect_index_consumers(&func.body, &mut index_consumers);

    let mut last_def: HashMap<String, PreHirExpr> = HashMap::default();
    let mut byte_ranged: HashMap<String, bool> = HashMap::default();
    // Names whose *last* def was an 8-bit sum of two byte-ranged values.
    let mut byte_sum_names: HashMap<String, bool> = HashMap::default();
    let mut changed = false;
    rewrite_stmts(
        &mut func.body,
        &type_map,
        &index_consumers,
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
    index_consumers: &HashSet<String>,
    last_def: &mut HashMap<String, PreHirExpr>,
    byte_ranged: &mut HashMap<String, bool>,
    byte_sum_names: &mut HashMap<String, bool>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(
            stmt,
            type_map,
            index_consumers,
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
    index_consumers: &HashSet<String>,
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
            // Pattern: w = v, where v's last def was an 8-bit byte-sum and w
            // is actually consumed as an index later in the function.
            if let PreHirExpr::Var(src) = rhs {
                if byte_sum_names.get(src).copied().unwrap_or(false)
                    && index_consumers.contains(name)
                {
                    let source_ty = last_def
                        .get(src)
                        .and_then(|d| match d {
                            PreHirExpr::Binary { ty, .. } => Some(ty.clone()),
                            _ => None,
                        })
                        .unwrap_or(NirType::Int {
                            bits: 8,
                            signed: false,
                        });
                    let ty = mask_type(name, &source_ty, type_map);
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

            let is_sum = is_byte_sum(rhs, byte_ranged, type_map);
            // A byte-sum itself is not byte-ranged until an explicit low-lane
            // truncation is observed or restored.
            let ranged = expr_is_byte_ranged(rhs, byte_ranged, type_map) && !is_sum;
            byte_ranged.insert(name.clone(), ranged);
            byte_sum_names.insert(name.clone(), is_sum);
            last_def.insert(name.clone(), rhs.clone());
        }
        PreHirStmt::Block(body) => {
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                index_consumers,
                last_def,
                byte_ranged,
                byte_sum_names,
                changed,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            // Loop bodies may execute zero or multiple times. Analyze a clone
            // for rewrites, then retain only facts that are unchanged across
            // the loop boundary. This prevents body-only defs from leaking out
            // and kills stale pre-loop facts modified by the body.
            let mut body_def = last_def.clone();
            let mut body_br = byte_ranged.clone();
            let mut body_sum = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                index_consumers,
                &mut body_def,
                &mut body_br,
                &mut body_sum,
                changed,
            );
            retain_unchanged(last_def, &body_def);
            retain_unchanged(byte_ranged, &body_br);
            retain_unchanged(byte_sum_names, &body_sum);
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
                index_consumers,
                &mut then_def,
                &mut then_br,
                &mut then_sum,
                changed,
            );
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                type_map,
                index_consumers,
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
                    index_consumers,
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
                index_consumers,
                &mut body_def,
                &mut body_br,
                &mut body_sum,
                changed,
            );
            if let Some(u) = update.as_mut() {
                rewrite_stmt(
                    u,
                    type_map,
                    index_consumers,
                    &mut body_def,
                    &mut body_br,
                    &mut body_sum,
                    changed,
                );
            }

            retain_unchanged(last_def, &body_def);
            retain_unchanged(byte_ranged, &body_br);
            retain_unchanged(byte_sum_names, &body_sum);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut branch_defs = Vec::with_capacity(cases.len() + 1);
            let mut branch_ranges = Vec::with_capacity(cases.len() + 1);
            let mut branch_sums = Vec::with_capacity(cases.len() + 1);

            for case in cases.iter_mut() {
                let mut d = last_def.clone();
                let mut b = byte_ranged.clone();
                let mut s = byte_sum_names.clone();
                rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    type_map,
                    index_consumers,
                    &mut d,
                    &mut b,
                    &mut s,
                    changed,
                );
                branch_defs.push(d);
                branch_ranges.push(b);
                branch_sums.push(s);
            }

            let mut d = last_def.clone();
            let mut b = byte_ranged.clone();
            let mut s = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                type_map,
                index_consumers,
                &mut d,
                &mut b,
                &mut s,
                changed,
            );
            branch_defs.push(d);
            branch_ranges.push(b);
            branch_sums.push(s);

            retain_across_branches(last_def, &branch_defs);
            retain_across_branches(byte_ranged, &branch_ranges);
            retain_across_branches(byte_sum_names, &branch_sums);
        }
        _ => {}
    }
}

fn retain_unchanged<T: PartialEq>(
    before: &mut HashMap<String, T>,
    after: &HashMap<String, T>,
) {
    before.retain(|name, value| after.get(name).is_some_and(|next| next == value));
}

fn retain_across_branches<T: PartialEq>(
    state: &mut HashMap<String, T>,
    branches: &[HashMap<String, T>],
) {
    state.retain(|name, value| {
        branches
            .iter()
            .all(|branch| branch.get(name).is_some_and(|candidate| candidate == value))
    });
}

fn mask_type(
    destination: &str,
    source_ty: &NirType,
    type_map: &HashMap<String, NirType>,
) -> NirType {
    if let Some(ty @ NirType::Int { .. }) = type_map.get(destination) {
        return ty.clone();
    }
    if matches!(source_ty, NirType::Int { .. }) {
        return source_ty.clone();
    }
    NirType::Int {
        bits: 32,
        signed: false,
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
            ty,
        } => {
            is_byte_int_ty(ty)
                && expr_is_byte_ranged(lhs, byte_ranged, type_map)
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
            type_map.get(name).is_some_and(is_byte_int_ty)
        }
        PreHirExpr::Load { ptr, ty, .. } => {
            if is_byte_int_ty(ty) {
                return true;
            }
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

fn collect_index_consumers(stmts: &[PreHirStmt], consumers: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                collect_index_consumers_lvalue(lhs, consumers);
                collect_index_consumers_expr(rhs, consumers);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_index_consumers_expr(expr, consumers);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                collect_index_consumers_expr(va_list, consumers);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => collect_index_consumers(body, consumers),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_index_consumers_expr(cond, consumers);
                collect_index_consumers(then_body, consumers);
                collect_index_consumers(else_body, consumers);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_index_consumers(std::slice::from_ref(init.as_ref()), consumers);
                }
                if let Some(cond) = cond {
                    collect_index_consumers_expr(cond, consumers);
                }
                if let Some(update) = update {
                    collect_index_consumers(std::slice::from_ref(update.as_ref()), consumers);
                }
                collect_index_consumers(body, consumers);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_index_consumers_expr(expr, consumers);
                for case in cases {
                    collect_index_consumers(&case.body, consumers);
                }
                collect_index_consumers(default, consumers);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn collect_index_consumers_lvalue(lhs: &PreHirLValue, consumers: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => collect_index_consumers_expr(ptr, consumers),
        PreHirLValue::Index { base, index, .. } => {
            collect_index_vars(index, consumers);
            collect_index_consumers_expr(base, consumers);
            collect_index_consumers_expr(index, consumers);
        }
        PreHirLValue::FieldAccess { base, .. } => collect_index_consumers_expr(base, consumers),
    }
}

fn collect_index_consumers_expr(expr: &PreHirExpr, consumers: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            collect_index_consumers_expr(expr, consumers);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_index_consumers_expr(lhs, consumers);
            collect_index_consumers_expr(rhs, consumers);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_index_consumers_expr(cond, consumers);
            collect_index_consumers_expr(then_expr, consumers);
            collect_index_consumers_expr(else_expr, consumers);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_index_consumers_expr(arg, consumers);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_index_vars(index, consumers);
            collect_index_consumers_expr(base, consumers);
            collect_index_consumers_expr(index, consumers);
        }
    }
}

fn collect_index_vars(expr: &PreHirExpr, consumers: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) => {
            consumers.insert(name.clone());
        }
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => collect_index_vars(expr, consumers),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_index_vars(lhs, consumers);
            collect_index_vars(rhs, consumers);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_index_vars(cond, consumers);
            collect_index_vars(then_expr, consumers);
            collect_index_vars(else_expr, consumers);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_index_vars(arg, consumers);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_index_vars(base, consumers);
            collect_index_vars(index, consumers);
        }
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

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
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

    fn index_use(name: &str) -> PreHirStmt {
        PreHirStmt::Expr(PreHirExpr::Index {
            base: Box::new(PreHirExpr::Var("table".into())),
            index: Box::new(PreHirExpr::Var(name.into())),
            elem_ty: u8_ty(),
        })
    }

    fn byte_sum_assign(name: &str, ty: NirType) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.into()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("a".into())),
                rhs: Box::new(PreHirExpr::Var("b".into())),
                ty,
            },
        }
    }

    #[test]
    fn recovers_truncation_only_for_an_indexed_copy_of_an_8_bit_sum() {
        let mut func = PreHirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("s", u8_ty()),
                binding("idx", u32_ty()),
            ],
            return_type: u32_ty(),
            body: vec![
                byte_sum_assign("s", u8_ty()),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("s".into()),
                },
                index_use("idx"),
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
                    ..
                },
                ..
            } if matches!(rhs.as_ref(), PreHirExpr::Const(0xff, _))
        ));
    }

    #[test]
    fn does_not_truncate_a_legitimate_wide_sum() {
        let mut func = PreHirFunction {
            name: "wide_sum".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u32_ty()),
                binding("idx", u32_ty()),
            ],
            body: vec![
                byte_sum_assign("sum", u32_ty()),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
                index_use("idx"),
            ],
            ..Default::default()
        };

        assert!(!apply_byte_sum_index_trunc(&mut func));
        assert!(matches!(
            &func.body[1],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Var(name),
                ..
            } if name == "sum"
        ));
    }

    #[test]
    fn does_not_truncate_a_copy_without_index_use() {
        let mut func = PreHirFunction {
            name: "plain_copy".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u8_ty()),
                binding("copy", u32_ty()),
            ],
            body: vec![
                byte_sum_assign("sum", u8_ty()),
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
    fn recovered_mask_preserves_destination_signedness() {
        let mut func = PreHirFunction {
            name: "signed_index".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", i8_ty()),
                binding("index", i32_ty()),
            ],
            body: vec![
                byte_sum_assign("sum", i8_ty()),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("index".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
                index_use("index"),
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
                        signed: true,
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
                        signed: true,
                    }
                )
            )
        ));
    }

    #[test]
    fn while_body_definition_does_not_leak_past_loop() {
        let mut func = PreHirFunction {
            name: "while_kill".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u8_ty()),
                binding("idx", u32_ty()),
            ],
            body: vec![
                byte_sum_assign("sum", u8_ty()),
                PreHirStmt::While {
                    cond: PreHirExpr::Const(0, u8_ty()),
                    body: std::rc::Rc::new(vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("sum".into()),
                        rhs: PreHirExpr::Const(7, u8_ty()),
                    }]),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
                index_use("idx"),
            ],
            ..Default::default()
        };

        assert!(!apply_byte_sum_index_trunc(&mut func));
    }

    #[test]
    fn for_body_kills_preloop_sum_fact() {
        let mut func = PreHirFunction {
            name: "for_kill".into(),
            params: vec![],
            locals: vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u8_ty()),
                binding("idx", u32_ty()),
            ],
            body: vec![
                byte_sum_assign("sum", u8_ty()),
                PreHirStmt::For {
                    init: None,
                    cond: None,
                    update: None,
                    body: std::rc::Rc::new(vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("sum".into()),
                        rhs: PreHirExpr::Const(9, u8_ty()),
                    }]),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("idx".into()),
                    rhs: PreHirExpr::Var("sum".into()),
                },
                index_use("idx"),
            ],
            ..Default::default()
        };

        assert!(!apply_byte_sum_index_trunc(&mut func));
    }
}
