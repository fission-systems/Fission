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

fn binary(op: PreHirBinaryOp, lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: int_ty(32, false),
    }
}

/// `*(short *)dst = value;`
fn short_store_of(value: PreHirExpr) -> PreHirStmt {
    PreHirStmt::Assign {
        lhs: PreHirLValue::Deref {
            ptr: Box::new(var("dst")),
            ty: int_ty(16, true),
        },
        rhs: value,
    }
}

#[test]
fn derived_value_in_a_narrow_frame_slot_is_not_a_home() {
    // `-O2` `ushort local_26 = (ushort)(param_2 + 1)`: a frame slot, but it
    // holds a value computed from the formal, not the formal's spill.
    let mut func = truncating_reads_func(
        vec![home_slot("local_26", -0x26, 16)],
        vec![set(
            "local_26",
            cast(
                16,
                false,
                binary(
                    PreHirBinaryOp::Add,
                    var("param_2"),
                    PreHirExpr::Const(1, int_ty(32, false)),
                ),
            ),
        )],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}

#[test]
fn formal_narrowed_to_int_through_addition_keeps_its_width() {
    // A 64-bit register formal spilled as `int`, read as `(int)(param + 1)`.
    // Declared `int`, `param + 1` is not promoted and could overflow where
    // the machine's 64-bit addition wrapped.
    let mut func = truncating_reads_func(vec![home_slot("local_24", -0x24, 32)], vec![]);
    func.params[1].ty = int_ty(64, true);
    func.body = vec![
        set("local_24", cast(32, true, var("param_2"))),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(var("dst")),
                ty: int_ty(32, true),
            },
            rhs: cast(
                32,
                true,
                binary(
                    PreHirBinaryOp::Add,
                    var("param_2"),
                    PreHirExpr::Const(1, int_ty(64, true)),
                ),
            ),
        },
    ];

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(64));
}

#[test]
fn byte_formal_read_through_addition_is_narrowed() {
    // `local_24 = (uchar)param_2; *dst = (uchar)(param_2 + 1);` -- a `uchar`
    // operand of `+` is promoted to `int`, where the addition cannot overflow.
    let mut func = truncating_reads_func(
        vec![home_slot("local_24", -0x24, 8)],
        vec![
            set("local_24", cast(8, false, var("param_2"))),
            byte_store_of(cast(
                8,
                false,
                binary(
                    PreHirBinaryOp::Add,
                    var("param_2"),
                    PreHirExpr::Const(1, int_ty(32, false)),
                ),
            )),
        ],
    );

    assert!(apply_use_driven_type_infer_pass(&mut func));
    assert_eq!(param_2_bits(&func), Some(8));
}

#[test]
fn spilled_formal_read_through_a_multiply_keeps_its_width() {
    // `(ushort)(param_2 * param_2)`: two `ushort` operands would promote to
    // signed `int` and overflow where the 32-bit unsigned product wrapped.
    let mut func = truncating_reads_func(
        vec![home_slot("local_26", -0x26, 16)],
        vec![
            set("local_26", cast(16, false, var("param_2"))),
            short_store_of(cast(
                16,
                false,
                binary(PreHirBinaryOp::Mul, var("param_2"), var("param_2")),
            )),
        ],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}

#[test]
fn write_to_a_global_demands_every_bit() {
    // `local_24 = (uchar)param_2; g_count = param_2;` -- a named global is a
    // `Var` lvalue that this function never reads, but other functions do.
    let mut func = truncating_reads_func(
        vec![home_slot("local_24", -0x24, 8)],
        vec![
            set("local_24", cast(8, false, var("param_2"))),
            set("g_count", var("param_2")),
        ],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
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

#[test]
fn formal_read_as_both_signed_and_unsigned_bytes_keeps_its_width() {
    // `if ((int)(char)param_2 < 0) *dst = (uchar)param_2 % 128;` -- declared
    // `char`, the `(uchar)` cast would read as a no-op and the `%` would turn
    // signed; declared `uchar`, the sign test would. Neither is exact.
    let mut func = truncating_reads_func(
        vec![home_slot("local_24", -0x24, 8)],
        vec![
            set("local_24", cast(8, false, var("param_2"))),
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLt,
                    lhs: Box::new(cast(32, true, cast(8, true, var("param_2")))),
                    rhs: Box::new(PreHirExpr::Const(0, int_ty(32, true))),
                    ty: NirType::Bool,
                },
                then_body: std::rc::Rc::new(vec![byte_store_of(binary(
                    PreHirBinaryOp::Mod,
                    cast(8, false, var("param_2")),
                    PreHirExpr::Const(128, int_ty(32, false)),
                ))]),
                else_body: std::rc::Rc::new(vec![]),
            },
        ],
    );

    apply_use_driven_type_infer_pass(&mut func);
    assert_eq!(param_2_bits(&func), Some(32));
}
