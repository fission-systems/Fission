use fission_solver::{SatResult, Solver, SymExpr};

#[test]
fn solver_registration_preserves_bit_widths() {
    let mut solver = Solver::new();
    for width_bits in [1, 8, 16, 32, 64] {
        let id = solver.register_var(format!("width_{width_bits}"), width_bits);
        assert_eq!(solver.nodes[&id].get_bit_width(), width_bits);
    }
}

#[test]
fn solver_max_searches_the_registered_eight_bit_domain() {
    let mut solver = Solver::new();
    let id = solver.register_var("byte".to_string(), 8);
    let byte = solver.nodes[&id].clone();
    assert_eq!(solver.max(&byte), Some(0xff));
}

#[test]
fn signed_and_unsigned_comparisons_use_the_same_width_contract() {
    for (width_bits, sign_bit) in [(1, 1), (8, 0x80), (32, 0x8000_0000), (64, 1u64 << 63)] {
        let mut signed_solver = Solver::new();
        let x_id = signed_solver.register_var("x".to_string(), width_bits);
        let x = signed_solver.nodes[&x_id].clone();
        signed_solver.assert(SymExpr::Eq(
            Box::new(x.clone()),
            Box::new(SymExpr::new_const(sign_bit, width_bits)),
        ));
        signed_solver.assert(SymExpr::Slt(
            Box::new(x.clone()),
            Box::new(SymExpr::new_const(0, width_bits)),
        ));
        assert_eq!(
            signed_solver.check_sat().expect("signed comparison"),
            SatResult::Sat,
            "the top bit is the sign bit at width {width_bits}"
        );

        let mut unsigned_solver = Solver::new();
        let x_id = unsigned_solver.register_var("x".to_string(), width_bits);
        let x = unsigned_solver.nodes[&x_id].clone();
        unsigned_solver.assert(SymExpr::Eq(
            Box::new(x.clone()),
            Box::new(SymExpr::new_const(sign_bit, width_bits)),
        ));
        unsigned_solver.assert(SymExpr::Ult(
            Box::new(x),
            Box::new(SymExpr::new_const(0, width_bits)),
        ));
        assert_eq!(
            unsigned_solver.check_sat().expect("unsigned comparison"),
            SatResult::Unsat,
            "unsigned values are never below zero at width {width_bits}"
        );
    }
}

#[test]
fn extract_and_concat_keep_their_bit_widths_and_values() {
    let input = SymExpr::new_var("input", 32);
    let extracted = SymExpr::Extract {
        expr: Box::new(input.clone()),
        lsb: 8,
        size: 8,
    };
    assert_eq!(extracted.get_bit_width(), 8);

    let concatenated = SymExpr::Concat(
        Box::new(SymExpr::new_const(0x12, 8)),
        Box::new(SymExpr::new_const(0x34_5678, 24)),
    );
    assert_eq!(concatenated.get_bit_width(), 32);

    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(
        Box::new(input),
        Box::new(SymExpr::new_const(0x1234_5678, 32)),
    ));
    solver.assert(SymExpr::Eq(
        Box::new(extracted),
        Box::new(SymExpr::new_const(0x56, 8)),
    ));
    solver.assert(SymExpr::Eq(
        Box::new(concatenated),
        Box::new(SymExpr::new_const(0x1234_5678, 32)),
    ));
    assert_eq!(
        solver.check_sat().expect("extract and concat"),
        SatResult::Sat
    );
}

#[test]
fn unsigned_arithmetic_aligns_mixed_widths_to_the_wider_operand() {
    let narrow = SymExpr::new_var("narrow", 8);
    let wide = SymExpr::new_var("wide", 16);
    let sum = SymExpr::new_add(narrow.clone(), wide.clone());
    assert_eq!(sum.get_bit_width(), 16);

    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(
        Box::new(narrow),
        Box::new(SymExpr::new_const(0xff, 8)),
    ));
    solver.assert(SymExpr::Eq(
        Box::new(wide),
        Box::new(SymExpr::new_const(1, 16)),
    ));
    solver.assert(SymExpr::Eq(
        Box::new(sum),
        Box::new(SymExpr::new_const(0x100, 16)),
    ));
    assert_eq!(
        solver.check_sat().expect("mixed-width addition"),
        SatResult::Sat
    );

    assert_eq!(
        SymExpr::new_add(SymExpr::new_const(0xff, 8), SymExpr::new_const(1, 16)),
        SymExpr::new_const(0x100, 16)
    );
}

#[test]
fn signed_and_shift_width_mismatches_are_unknown() {
    let x8 = SymExpr::new_var("x8", 8);
    let y16 = SymExpr::new_var("y16", 16);
    let mut signed = Solver::new();
    signed.assert(SymExpr::Slt(Box::new(x8.clone()), Box::new(y16.clone())));
    assert_eq!(
        signed.check_sat().expect("mismatched signed comparison"),
        SatResult::Unknown
    );

    let shifted = SymExpr::Shl(Box::new(x8.clone()), Box::new(y16.clone()));
    let mut shift = Solver::new();
    shift.assert(SymExpr::Eq(
        Box::new(shifted),
        Box::new(SymExpr::new_const(0, 8)),
    ));
    assert_eq!(
        shift.check_sat().expect("mismatched shift"),
        SatResult::Unknown
    );

    let mut division = Solver::new();
    division.assert(SymExpr::Eq(
        Box::new(SymExpr::Sdiv(Box::new(x8), Box::new(y16))),
        Box::new(SymExpr::new_const(0, 16)),
    ));
    assert_eq!(
        division.check_sat().expect("mismatched signed division"),
        SatResult::Unknown
    );
}
