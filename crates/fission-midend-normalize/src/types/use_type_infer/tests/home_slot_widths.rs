//! A formal declared at the width of the frame slot the compiler homes it in.

use super::super::apply_use_driven_type_infer_pass;
use super::make_typed_binding;
use crate::prelude::*;

fn var(name: &str) -> PreHirExpr {
    PreHirExpr::Var(name.to_owned())
}

fn int_ty(bits: u32, signed: bool) -> NirType {
    NirType::Int { bits, signed }
}

fn cast(bits: u32, signed: bool, expr: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Cast {
        ty: int_ty(bits, signed),
        expr: Box::new(expr),
    }
}

/// `*(uchar *)dst = value;`
fn byte_store_of(value: PreHirExpr) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(var("dst")),
            ty: int_ty(8, false),
        },
        rhs: value,
    }
}

fn set(name: &str, rhs: PreHirExpr) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name.to_owned()),
        rhs,
    }
}

/// A frame slot of `bits` at `offset`, the home a compiler gives a formal
/// of that width at `-O0`.
fn home_slot(name: &str, offset: i64, bits: u32) -> PreHirBinding {
    make_typed_binding(
        name,
        int_ty(bits, false),
        NirBindingOrigin::StackOffset(offset),
    )
}

/// `store_byte(uchar *dst, uint param_2)` with the given locals and body.
fn truncating_reads_func(locals: Vec<PreHirBinding>, body: Vec<PreHirStmt>) -> PreHirFunction {
    PreHirFunction {
        name: "store_byte".to_owned(),
        int_param_offsets: Vec::new(),
        params: vec![
            make_typed_binding(
                "dst",
                NirType::Ptr(Box::new(int_ty(8, false))),
                NirBindingOrigin::ParamIndex(0),
            ),
            make_typed_binding(
                "param_2",
                int_ty(32, false),
                NirBindingOrigin::ParamIndex(1),
            ),
        ],
        locals,
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body,
        ..Default::default()
    }
}

fn param_2_bits(func: &PreHirFunction) -> Option<u32> {
    match func.params[1].ty {
        NirType::Int { bits, .. } => Some(bits),
        _ => None,
    }
}

#[test]
fn formal_spilled_to_a_byte_home_slot_is_declared_a_byte() {
    // -O0 `char c`: `local_24 = (uchar)param_2; *dst = local_24;`
    let mut func = truncating_reads_func(
        vec![home_slot("local_24", -0x24, 8)],
        vec![
            set("local_24", cast(8, false, var("param_2"))),
            byte_store_of(var("local_24")),
        ],
    );

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(func.params[1].ty, int_ty(8, false));
}

#[test]
fn formal_spilled_at_full_width_keeps_it_though_read_as_a_byte() {
    // -O0 `int c` stored as a byte: `local_14 = param_2; *dst = (uchar)param_2;`
    let mut func = truncating_reads_func(
        vec![home_slot("local_14", -0x14, 32)],
        vec![
            set("local_14", var("param_2")),
            byte_store_of(cast(8, false, var("param_2"))),
        ],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}

#[test]
fn truncating_reads_without_a_home_slot_do_not_narrow() {
    // -O2 `*dst = (uchar)param_2;`: an `int` stored as a byte reads the same.
    let mut func =
        truncating_reads_func(vec![], vec![byte_store_of(cast(8, false, var("param_2")))]);

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}

#[test]
fn formal_read_at_full_width_anywhere_keeps_its_width() {
    // The byte home slot is there, but `param_2 == 300` reads all of it.
    let mut func = truncating_reads_func(
        vec![home_slot("local_24", -0x24, 8)],
        vec![
            set("local_24", cast(8, false, var("param_2"))),
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    lhs: Box::new(var("param_2")),
                    rhs: Box::new(PreHirExpr::Const(300, int_ty(32, false))),
                    ty: NirType::Bool,
                },
                then_body: std::rc::Rc::new(vec![PreHirStmt::Return(None)]),
                else_body: std::rc::Rc::new(vec![]),
            },
        ],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}

#[test]
fn home_slot_demand_flows_through_wrapping_arithmetic_and_copies() {
    // `r12 = param_2 + 1; local_26 = (ushort)r12;` -- a 16-bit home reached
    // through a register copy and an add.
    let mut func = truncating_reads_func(
        vec![home_slot("local_26", -0x26, 16)],
        vec![
            set(
                "r12",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(var("param_2")),
                    rhs: Box::new(PreHirExpr::Const(1, int_ty(32, false))),
                    ty: int_ty(32, false),
                },
            ),
            set("local_26", cast(16, false, var("r12"))),
        ],
    );

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(param_2_bits(&func), Some(16));
}

#[test]
fn home_slot_demand_does_not_flow_through_a_shift() {
    // The low byte of `param_2 >> 8` depends on bits 8..15 of the formal.
    let mut func = truncating_reads_func(
        vec![home_slot("local_24", -0x24, 8)],
        vec![set(
            "local_24",
            cast(
                8,
                false,
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shr,
                    lhs: Box::new(var("param_2")),
                    rhs: Box::new(PreHirExpr::Const(8, int_ty(32, false))),
                    ty: int_ty(32, false),
                },
            ),
        )],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}

#[test]
fn assigned_formal_is_not_narrowed() {
    let mut func = truncating_reads_func(
        vec![home_slot("local_24", -0x24, 8)],
        vec![
            set("param_2", PreHirExpr::Const(7, int_ty(32, false))),
            set("local_24", cast(8, false, var("param_2"))),
        ],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}

#[test]
fn home_slot_initializer_narrows_the_formal() {
    // `uchar local_24 = (uchar)param_2;` as a declaration initializer.
    let mut slot = home_slot("local_24", -0x24, 8);
    slot.initializer = Some(cast(8, false, var("param_2")));
    let mut func = truncating_reads_func(vec![slot], vec![byte_store_of(var("local_24"))]);

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(param_2_bits(&func), Some(8));
}
