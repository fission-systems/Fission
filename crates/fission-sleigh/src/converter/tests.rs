use std::num::NonZeroU64;

use fission_pcode::{PcodeOpcode, Varnode};
use sleigh_rs::execution::AssignmentOp;

use super::IRConverter;

#[test]
fn space_varnode_is_8byte_constant() {
    let conv = IRConverter::new();
    let space = conv.make_space_varnode(42);
    assert!(space.is_constant);
    assert_eq!(space.constant_val, 42);
    assert_eq!(space.size, 8);
}

#[test]
fn assignment_takelsb_emits_subpiece_with_requested_size() {
    let mut conv = IRConverter::new();
    let mut emitted = Vec::new();
    let rhs = Varnode {
        space_id: 1,
        offset: 0x1234,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let out = conv
        .apply_assignment_op(
            &AssignmentOp::TakeLsb(NonZeroU64::new(3).unwrap()),
            rhs.clone(),
            0x1000,
            &mut emitted,
        )
        .unwrap();

    assert_eq!(out.size, 3);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].opcode, PcodeOpcode::SubPiece);
    assert_eq!(emitted[0].inputs, vec![rhs, Varnode::constant(0, 4)]);
}

#[test]
fn assignment_trunklsb_clamps_output_size_to_at_least_one() {
    let mut conv = IRConverter::new();
    let mut emitted = Vec::new();
    let rhs = Varnode {
        space_id: 1,
        offset: 0x77,
        size: 2,
        is_constant: false,
        constant_val: 0,
    };

    let out = conv
        .apply_assignment_op(
            &AssignmentOp::TrunkLsb(4),
            rhs.clone(),
            0x2000,
            &mut emitted,
        )
        .unwrap();

    assert_eq!(out.size, 1);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].opcode, PcodeOpcode::SubPiece);
    assert_eq!(emitted[0].inputs, vec![rhs, Varnode::constant(4, 4)]);
}

#[test]
fn assignment_bitrange_unaligned_emits_shift_subpiece_mask() {
    let mut conv = IRConverter::new();
    let mut emitted = Vec::new();
    let rhs = Varnode {
        space_id: 1,
        offset: 0x55,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let out = conv
        .apply_assignment_op(&AssignmentOp::BitRange(5..17), rhs, 0x3000, &mut emitted)
        .unwrap();

    assert_eq!(out.size, 2);
    assert_eq!(emitted.len(), 3);
    assert_eq!(emitted[0].opcode, PcodeOpcode::IntRight);
    assert_eq!(emitted[0].inputs[1], Varnode::constant(5, 4));
    assert_eq!(emitted[1].opcode, PcodeOpcode::SubPiece);
    assert_eq!(emitted[1].inputs[1], Varnode::constant(0, 4));
    assert_eq!(emitted[2].opcode, PcodeOpcode::IntAnd);
    assert_eq!(emitted[2].inputs[1], Varnode::constant(0x0fff, 2));
}

#[test]
fn assignment_bitrange_byte_aligned_emits_only_subpiece() {
    let mut conv = IRConverter::new();
    let mut emitted = Vec::new();
    let rhs = Varnode {
        space_id: 1,
        offset: 0x99,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let out = conv
        .apply_assignment_op(&AssignmentOp::BitRange(8..24), rhs, 0x4000, &mut emitted)
        .unwrap();

    assert_eq!(out.size, 2);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].opcode, PcodeOpcode::SubPiece);
    assert_eq!(emitted[0].inputs[1], Varnode::constant(1, 4));
}
