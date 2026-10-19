//! Regression contracts for issue-driven solver triage. No external solver oracle.
use fission_solver::{SatResult, Solver, SymExpr};

#[test]
fn eager_assertions_advance_the_lowering_cursor() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    for bound in [1, 2, 3] {
        solver.assert(SymExpr::Ule(
            Box::new(x.clone()),
            Box::new(SymExpr::new_const(bound, 8)),
        ));
        assert_eq!(solver.lowered_assertions, solver.assertions.len());
        assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
        assert_eq!(solver.lowered_assertions, solver.assertions.len());
    }
}

#[test]
fn eager_lowering_does_not_skip_publicly_appended_assertions() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    solver.assertions.push(SymExpr::Eq(
        Box::new(x.clone()),
        Box::new(SymExpr::new_const(1, 8)),
    ));
    solver.assert(SymExpr::Eq(Box::new(x), Box::new(SymExpr::new_const(2, 8))));
    assert_eq!(solver.lowered_assertions, 2);
    assert_eq!(solver.check_sat().unwrap(), SatResult::Unsat);
}

#[test]
fn mixed_width_subtraction_matches_zero_extended_integer_arithmetic() {
    for left_width in 1..=4 {
        for right_width in 1..=4 {
            let width = left_width.max(right_width);
            let mask = (1u64 << width) - 1;
            for a in 0..1u64 << left_width {
                for b in 0..1u64 << right_width {
                    let mut solver = Solver::new();
                    let left = SymExpr::new_var("left", left_width);
                    let right = SymExpr::new_var("right", right_width);
                    solver.assert(SymExpr::Eq(
                        Box::new(left.clone()),
                        Box::new(SymExpr::new_const(a, left_width)),
                    ));
                    solver.assert(SymExpr::Eq(
                        Box::new(right.clone()),
                        Box::new(SymExpr::new_const(b, right_width)),
                    ));
                    let value = SymExpr::Sub(Box::new(left), Box::new(right));
                    let expected = SymExpr::new_const(a.wrapping_sub(b) & mask, width);
                    assert_eq!(
                        solver
                            .check_sat_with_oracle(
                                None,
                                &[SymExpr::Eq(
                                    Box::new(value.clone()),
                                    Box::new(expected.clone())
                                )]
                            )
                            .unwrap(),
                        SatResult::Sat,
                        "{a}:{left_width} - {b}:{right_width}"
                    );
                    assert_eq!(
                        solver
                            .check_sat_with_oracle(
                                None,
                                &[SymExpr::Neq(Box::new(value), Box::new(expected))]
                            )
                            .unwrap(),
                        SatResult::Unsat
                    );
                }
            }
        }
    }
}

#[test]
fn oversized_constants_do_not_escape_the_supported_model_width() {
    for width in [65, 80, 128, 256, u32::MAX] {
        let mut solver = Solver::new();
        let value = SymExpr::new_const(1, width);
        assert!(solver.eval(&value, 1).is_empty(), "width {width}");
    }
}

#[test]
fn invalid_extracts_and_wide_variables_are_unknown_without_panics() {
    for (lsb, size) in [(9, 1), (8, 1), (u32::MAX, 2), (0, 0)] {
        let mut solver = Solver::new();
        let value = SymExpr::Extract {
            expr: Box::new(SymExpr::new_var("x", 8)),
            lsb,
            size,
        };
        solver.assert(SymExpr::Eq(
            Box::new(value),
            Box::new(SymExpr::new_const(0, size)),
        ));
        assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
    }
    for width in [65, 80, 128, 256, u32::MAX] {
        let mut solver = Solver::new();
        let id = solver.register_var("wide".into(), width);
        let value = solver.nodes[&id].clone();
        assert!(solver.eval(&value, 1).is_empty());
        assert!(!solver.model.contains_key(&id));
        assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
    }
}

#[test]
fn constant_queries_require_a_satisfiable_supported_context() {
    for mode in 0..3 {
        let mut solver = Solver::new();
        if mode == 1 {
            solver.assert(SymExpr::new_const(0, 1));
        }
        if mode == 2 {
            let wide = SymExpr::new_var("unsupported", 128);
            solver.assert(SymExpr::Eq(Box::new(wide.clone()), Box::new(wide)));
        }
        let value = SymExpr::new_const(7, 8);
        let want = if mode == 0 { Some(7) } else { None };
        assert_eq!(solver.eval(&value, 1).first().copied(), want);
        assert_eq!(solver.min(&value), want);
        assert_eq!(solver.max(&value), want);
    }
}

#[test]
fn unsupported_array_scalars_do_not_panic_or_use_byte_counts_as_bits() {
    use fission_solver::aig::AigManager;
    for width in [8, 16, 32, 64] {
        let array = SymExpr::new_array_var("array", 64, width);
        let mut aig = AigManager::new();
        assert!(!aig.lower_expr(&array).is_empty());
        assert!(!aig.unsupported().is_empty());
        let stored = SymExpr::ArrayStore {
            array: Box::new(array.clone()),
            index: Box::new(SymExpr::new_const(0, 64)),
            value: Box::new(SymExpr::new_const(1, width)),
        };
        let mut aig = AigManager::new();
        assert_eq!(aig.lower_expr(&stored).len(), width as usize);
        assert!(!aig.unsupported().is_empty());
        let mut solver = Solver::new();
        solver.assert(SymExpr::Eq(Box::new(array.clone()), Box::new(array)));
        assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
    }
}
