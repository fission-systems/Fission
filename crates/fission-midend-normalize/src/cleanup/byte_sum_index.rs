//! Recover low-byte truncation after a sum of two byte-ranged values.
//!
//! A widening copy from the low lane of a byte sum represents
//! `(byte_a + byte_b) mod 256`. If that lane boundary is lost and the copy is
//! materialized as `w = v` after `v = a + b`, downstream indexing can use the
//! unbounded sum. This pass restores the bit-vector operation as `w = v & 0xff`.
//!
//! The rewrite changes the value of `w` whenever the sum exceeds 255, so it
//! only runs on evidence that the copy keeps just the low byte:
//!
//! * the addition is an unsigned 8-bit operation and its binding preserves
//!   that unsigned value (a signed binding could reinterpret it), or
//! * the destination is declared at most 8 bits wide, so the assignment
//!   truncates anyway and the mask only spells that out.
//!
//! A 32-bit sum copied into a 32-bit variable is a legitimate wide value
//! (`uint32_t t = a + b; uint32_t u = t;` with `a + b == 300`) and is left alone.

use crate::HashMap;
use crate::prelude::*;

/// Apply byte-sum index truncation recovery. Returns true if any rewrite ran.
pub fn apply_byte_sum_index_trunc(func: &mut PreHirFunction) -> bool {
    let mut type_map: HashMap<String, NirType> = HashMap::default();
    for b in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(b.name.clone(), b.ty.clone());
    }
    let mut last_def: HashMap<String, PreHirExpr> = HashMap::default();
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
    stmts: &mut [PreHirStmt],
    type_map: &HashMap<String, NirType>,
    last_def: &mut HashMap<String, PreHirExpr>,
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
    stmt: &mut PreHirStmt,
    type_map: &HashMap<String, NirType>,
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
                    if !truncation_is_evidenced(name, src, &source_ty, type_map) {
                        byte_ranged.insert(name.clone(), false);
                        byte_sum_names.insert(name.clone(), false);
                        last_def.insert(name.clone(), rhs.clone());
                        return;
                    }
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
                last_def,
                byte_ranged,
                byte_sum_names,
                changed,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            // The body runs zero or more times, and its second iteration sees
            // the first one's definitions. Whatever it writes is unknown on
            // entry and after the loop, so drop it before and keep it dropped.
            kill_written(body, last_def, byte_ranged, byte_sum_names);
            let mut body_def = last_def.clone();
            let mut body_br = byte_ranged.clone();
            let mut body_sum = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                type_map,
                &mut body_def,
                &mut body_br,
                &mut body_sum,
                changed,
            );
        }
        PreHirStmt::Label(_) => {
            // Control can arrive here from anywhere.
            last_def.clear();
            byte_ranged.clear();
            byte_sum_names.clear();
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
                &mut then_def,
                &mut then_br,
                &mut then_sum,
                changed,
            );
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
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
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init.as_mut() {
                rewrite_stmt(i, type_map, last_def, byte_ranged, byte_sum_names, changed);
            }
            // As for `while`: loop-written names are unknown on entry and exit.
            kill_written(body, last_def, byte_ranged, byte_sum_names);
            if let Some(u) = update.as_ref() {
                kill_written(
                    std::slice::from_ref(u.as_ref()),
                    last_def,
                    byte_ranged,
                    byte_sum_names,
                );
            }
            let mut body_def = last_def.clone();
            let mut body_br = byte_ranged.clone();
            let mut body_sum = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
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
        PreHirStmt::Switch { cases, default, .. } => {
            // Exactly one arm runs (or none), so anything an arm writes is
            // unknown afterwards; each arm is analysed from the entry state.
            for case in cases.iter() {
                kill_written(&case.body, last_def, byte_ranged, byte_sum_names);
            }
            kill_written(default, last_def, byte_ranged, byte_sum_names);
            for case in cases.iter_mut() {
                let mut d = last_def.clone();
                let mut b = byte_ranged.clone();
                let mut s = byte_sum_names.clone();
                rewrite_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    type_map,
                    &mut d,
                    &mut b,
                    &mut s,
                    changed,
                );
            }
            let mut d = last_def.clone();
            let mut b = byte_ranged.clone();
            let mut s = byte_sum_names.clone();
            rewrite_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                type_map,
                &mut d,
                &mut b,
                &mut s,
                changed,
            );
        }
        _ => {}
    }
}

/// A narrow destination truncates itself. A wider copy may only be masked
/// when both the operation and its binding preserve an unsigned byte value.
/// In particular, a signed byte binding can turn 200 into -56 before the copy.
fn truncation_is_evidenced(
    destination: &str,
    source: &str,
    source_ty: &NirType,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let byte_wide =
        |ty: &NirType| matches!(ty, NirType::Int { bits, .. } if (1..=8).contains(bits));
    let unsigned_byte =
        matches!(source_ty, NirType::Int { bits, signed: false } if (1..=8).contains(bits));
    let unsigned_binding = type_map
        .get(source)
        .is_none_or(|ty| matches!(ty, NirType::Int { bits, signed: false } if *bits > 0));
    type_map.get(destination).is_some_and(byte_wide) || (unsigned_byte && unsigned_binding)
}

/// Type of the `& 0xff` mask: the destination's width, keeping its
/// signedness when it is declared (unsigned when it is not).
fn mask_type(
    destination: &str,
    source_ty: &NirType,
    type_map: &HashMap<String, NirType>,
) -> NirType {
    let declared = type_map
        .get(destination)
        .filter(|ty| int_type_width(ty).is_some());
    let bits = declared
        .and_then(int_type_width)
        .or_else(|| int_type_width(source_ty))
        .unwrap_or(32);
    let signed = matches!(declared, Some(NirType::Int { signed: true, .. }));
    NirType::Int { bits, signed }
}

/// Forget everything known about the variables assigned anywhere in `stmts`.
fn kill_written(
    stmts: &[PreHirStmt],
    last_def: &mut HashMap<String, PreHirExpr>,
    byte_ranged: &mut HashMap<String, bool>,
    byte_sum_names: &mut HashMap<String, bool>,
) {
    fn collect<'a>(stmts: &'a [PreHirStmt], out: &mut Vec<&'a str>) {
        for stmt in stmts {
            match stmt {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    ..
                } => out.push(name),
                PreHirStmt::Block(body)
                | PreHirStmt::While { body, .. }
                | PreHirStmt::DoWhile { body, .. } => collect(body, out),
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    collect(then_body, out);
                    collect(else_body, out);
                }
                PreHirStmt::For {
                    init, update, body, ..
                } => {
                    if let Some(i) = init {
                        collect(std::slice::from_ref(i.as_ref()), out);
                    }
                    if let Some(u) = update {
                        collect(std::slice::from_ref(u.as_ref()), out);
                    }
                    collect(body, out);
                }
                PreHirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        collect(&case.body, out);
                    }
                    collect(default, out);
                }
                _ => {}
            }
        }
    }
    let mut names = Vec::new();
    collect(stmts, &mut names);
    for name in names {
        last_def.remove(name);
        byte_ranged.remove(name);
        byte_sum_names.remove(name);
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
    fn recovers_truncation_after_eight_bit_byte_sum_copy() {
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
                        ty: u8_ty(),
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
    fn signed_byte_sum_copy_preserves_its_widening() {
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

        let before = func.body.clone();
        assert!(!apply_byte_sum_index_trunc(&mut func));
        assert_eq!(func.body, before);
    }

    fn assign(dst: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst.into()),
            rhs,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.into())
    }

    fn add(ty: NirType) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(var("a")),
            rhs: Box::new(var("b")),
            ty,
        }
    }

    fn func(locals: Vec<PreHirBinding>, body: Vec<PreHirStmt>) -> PreHirFunction {
        PreHirFunction {
            name: "f".into(),
            locals,
            return_type: u32_ty(),
            body,
            ..Default::default()
        }
    }

    fn is_masked(stmt: &PreHirStmt) -> bool {
        matches!(
            stmt,
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    ..
                },
                ..
            }
        )
    }

    #[test]
    fn wide_sum_copy_into_wide_destination_is_not_truncated() {
        // uint32_t sum = a + b; uint32_t result = sum;  (300 must stay 300)
        let mut f = func(
            vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u32_ty()),
                binding("result", u32_ty()),
            ],
            vec![assign("sum", add(u32_ty())), assign("result", var("sum"))],
        );
        assert!(!apply_byte_sum_index_trunc(&mut f));
        assert!(!is_masked(&f.body[1]));
    }

    #[test]
    fn byte_destination_truncates_a_wide_sum_copy() {
        let mut f = func(
            vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u32_ty()),
                binding("lane", u8_ty()),
            ],
            vec![assign("sum", add(u32_ty())), assign("lane", var("sum"))],
        );
        assert!(apply_byte_sum_index_trunc(&mut f));
        assert!(is_masked(&f.body[1]));
    }

    #[test]
    fn mask_keeps_a_signed_destinations_signedness() {
        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let mut f = func(
            vec![
                binding("a", u8_ty()),
                binding("b", u8_ty()),
                binding("sum", u8_ty()),
                binding("index", i32_ty.clone()),
            ],
            vec![assign("sum", add(u8_ty())), assign("index", var("sum"))],
        );
        assert!(apply_byte_sum_index_trunc(&mut f));
        assert!(matches!(
            &f.body[1],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary { ty, .. },
                ..
            } if *ty == i32_ty
        ));
    }

    fn while_loop(body: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::While {
            cond: var("c"),
            body: body.into(),
        }
    }

    fn byte_locals() -> Vec<PreHirBinding> {
        vec![
            binding("a", u8_ty()),
            binding("b", u8_ty()),
            binding("sum", u32_ty()),
            binding("out", u32_ty()),
        ]
    }

    #[test]
    fn sum_defined_in_a_loop_does_not_reach_the_code_after_it() {
        // The loop may run zero times, so `sum` is not known to be a byte sum.
        let mut f = func(
            byte_locals(),
            vec![
                assign("sum", PreHirExpr::Const(1000, u32_ty())),
                while_loop(vec![assign("sum", add(u8_ty()))]),
                assign("out", var("sum")),
            ],
        );
        assert!(!apply_byte_sum_index_trunc(&mut f));
        assert!(!is_masked(&f.body[2]));
    }

    #[test]
    fn loop_redefinition_kills_a_byte_sum_known_before_it() {
        let mut f = func(
            byte_locals(),
            vec![
                assign("sum", add(u8_ty())),
                while_loop(vec![assign("sum", PreHirExpr::Const(1000, u32_ty()))]),
                assign("out", var("sum")),
            ],
        );
        assert!(!apply_byte_sum_index_trunc(&mut f));
    }

    #[test]
    fn for_loop_writes_kill_a_byte_sum_known_before_it() {
        let mut f = func(
            byte_locals(),
            vec![
                assign("sum", add(u8_ty())),
                PreHirStmt::For {
                    init: None,
                    cond: Some(var("c")),
                    update: None,
                    body: vec![assign("sum", PreHirExpr::Const(1000, u32_ty()))].into(),
                },
                assign("out", var("sum")),
            ],
        );
        assert!(!apply_byte_sum_index_trunc(&mut f));
    }

    #[test]
    fn byte_sum_survives_a_loop_that_does_not_write_it() {
        let mut f = func(
            byte_locals(),
            vec![
                assign("sum", add(u8_ty())),
                while_loop(vec![assign("a", var("b"))]),
                assign("out", var("sum")),
            ],
        );
        assert!(apply_byte_sum_index_trunc(&mut f));
        assert!(is_masked(&f.body[2]));
    }

    #[test]
    fn switch_arm_writes_kill_a_byte_sum_known_before_it() {
        let mut f = func(
            byte_locals(),
            vec![
                assign("sum", add(u8_ty())),
                PreHirStmt::Switch {
                    expr: var("c"),
                    cases: vec![PreHirSwitchCase {
                        values: vec![0],
                        body: vec![assign("sum", PreHirExpr::Const(1000, u32_ty()))].into(),
                    }],
                    default: Vec::new().into(),
                },
                assign("out", var("sum")),
            ],
        );
        assert!(!apply_byte_sum_index_trunc(&mut f));
    }

    #[test]
    fn label_discards_what_was_known_before_it() {
        let mut f = func(
            byte_locals(),
            vec![
                assign("sum", add(u8_ty())),
                PreHirStmt::Label("join".into()),
                assign("out", var("sum")),
            ],
        );
        assert!(!apply_byte_sum_index_trunc(&mut f));
    }
    #[test]
    fn review_signed_byte_widening_does_not_become_zero_extension() {
        for (op_ty, binding_ty) in [(i8_ty(), i8_ty()), (u8_ty(), i8_ty()), (i8_ty(), u32_ty())] {
            let mut f = func(
                vec![
                    binding("a", u8_ty()),
                    binding("b", u8_ty()),
                    binding("sum", binding_ty),
                    binding(
                        "out",
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    ),
                ],
                vec![assign("sum", add(op_ty)), assign("out", var("sum"))],
            );
            let before = f.body.clone();
            assert_eq!((200_u8 as i8) as i32, -56);
            apply_byte_sum_index_trunc(&mut f);
            assert_eq!(
                f.body, before,
                "masking a signed byte after widening changes -56 into 200"
            );
        }
    }
}
