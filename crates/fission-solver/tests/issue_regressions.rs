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

#[test]
fn nested_array_store_scalar_lowering_is_unknown_without_panicking() {
    use fission_solver::ast::Sort;
    let mut array = SymExpr::new_array_var("nested", 64, 8);
    if let SymExpr::Var { sort, .. } = &mut array {
        *sort = Sort::Array {
            domain: Box::new(Sort::BitVector(64)),
            range: Box::new(Sort::Array {
                domain: Box::new(Sort::BitVector(32)),
                range: Box::new(Sort::BitVector(8)),
            }),
        };
    }
    let stored = SymExpr::ArrayStore {
        array: Box::new(array),
        index: Box::new(SymExpr::new_const(0, 64)),
        value: Box::new(SymExpr::new_array_var("row", 32, 8)),
    };
    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(Box::new(stored.clone()), Box::new(stored)));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
}

#[test]
fn unsupported_array_placeholders_remain_bounded_for_huge_range_widths() {
    use fission_solver::aig::AigManager;
    for width in [0, 65, 128, 256, u32::MAX] {
        let stored = SymExpr::ArrayStore {
            array: Box::new(SymExpr::new_array_var("wide_array", 64, width)),
            index: Box::new(SymExpr::new_const(0, 64)),
            value: Box::new(SymExpr::new_const(0, width)),
        };
        let mut aig = AigManager::new();
        assert!((1..=64).contains(&aig.lower_expr(&stored).len()));
        assert!(!aig.unsupported().is_empty());
    }
}

#[test]
fn registered_nested_arrays_are_not_extracted_as_u64_scalars() {
    use fission_solver::ast::Sort;
    let mut array = SymExpr::new_array_var("nested", 64, 8);
    if let SymExpr::Var { sort, .. } = &mut array {
        *sort = Sort::Array {
            domain: Box::new(Sort::BitVector(64)),
            range: Box::new(Sort::Array {
                domain: Box::new(Sort::BitVector(32)),
                range: Box::new(Sort::BitVector(8)),
            }),
        };
    }
    let mut solver = Solver::new();
    let id = solver.register_node(array);
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert!(!solver.model.contains_key(&id));
}

fn array_select(array: SymExpr, index: SymExpr) -> SymExpr {
    SymExpr::ArraySelect {
        array: Box::new(array),
        index: Box::new(index),
    }
}

fn array_store(array: SymExpr, index: SymExpr, value: SymExpr) -> SymExpr {
    SymExpr::ArrayStore {
        array: Box::new(array),
        index: Box::new(index),
        value: Box::new(value),
    }
}

fn not_equal(a: SymExpr, b: SymExpr) -> SymExpr {
    SymExpr::new_not(SymExpr::Eq(Box::new(a), Box::new(b)))
}

/// `select(store(a, i, v), i) != v` has no model. The solver has no array
/// axioms for a store chain, so the only sound answers are `Unsat` or
/// `Unknown`; it used to answer `Sat` because an `ArraySelect` was lowered to
/// fresh unconstrained bits without its array operand being looked at, so the
/// `ArrayStore` inside it was never even flagged as unsupported.
#[test]
fn select_over_store_is_not_reported_satisfiable() {
    let a = SymExpr::new_array_var("a", 8, 8);
    let i = SymExpr::new_var("i", 8);
    let v = SymExpr::new_var("v", 8);
    let mut solver = Solver::new();
    solver.assert(not_equal(
        array_select(array_store(a, i.clone(), v.clone()), i),
        v,
    ));
    assert_ne!(solver.check_sat().unwrap(), SatResult::Sat);
}

/// Functional consistency: equal indices read equal values. Fresh bits per
/// `ArraySelect` node made `select(a, i) != select(a, j)` with `i == j`
/// satisfiable.
#[test]
fn equal_indices_of_an_unconstrained_array_are_not_reported_distinguishable() {
    let a = SymExpr::new_array_var("a", 8, 8);
    let i = SymExpr::new_var("i", 8);
    let j = SymExpr::new_var("j", 8);
    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(Box::new(i.clone()), Box::new(j.clone())));
    solver.assert(not_equal(array_select(a.clone(), i), array_select(a, j)));
    assert_ne!(solver.check_sat().unwrap(), SatResult::Sat);
}

/// Positive control for the two tests above: a read from the emulator's own
/// memory array (`space_<id>`) is constrained afterwards by the oracle CEGAR in
/// `Solver::check_sat_with_oracle`, so lowering it must stay supported. The
/// fix cannot pass by flagging every array read.
#[test]
fn select_from_an_oracle_backed_space_array_stays_supported() {
    use fission_solver::aig::AigManager;
    let space = SymExpr::new_array_var("space_1", 64, 8);
    let read = array_select(space, SymExpr::new_var("addr", 64));
    let mut aig = AigManager::new();
    assert_eq!(aig.lower_expr(&read).len(), 8);
    assert!(
        aig.unsupported().is_empty(),
        "oracle-backed read was flagged: {:?}",
        aig.unsupported()
    );
}

#[test]
fn unconstrained_inverted_aig_bits_evaluate_as_one() {
    use fission_solver::aig::AigLit;
    use fission_solver::sat::SatSolver;
    use fission_solver::theory::bitvector::BvTheorySolver;
    use std::collections::HashMap;

    let mut bv = BvTheorySolver::new();
    let sat = SatSolver::new();

    let x = SymExpr::new_var("x", 8);
    let inv_x = SymExpr::Xor(Box::new(x.clone()), Box::new(SymExpr::new_const(0xff, 8)));

    let SymExpr::Var { id: x_id, .. } = &x else {
        unreachable!()
    };
    let nodes = HashMap::from([(1, x.clone())]);

    // Register unconstrained AIG variables without loading clauses into SAT (`cnf_var == 0`).
    let _ = bv.aig.lower_expr(&inv_x);
    let mut model = HashMap::new();
    bv.extract_model(&sat, &nodes, &mut model);
    assert_eq!(model[&1], 0);
    assert_eq!(bv.evaluate_expr_in_model(&sat, &x), 0);
    assert_eq!(bv.evaluate_expr_in_model(&sat, &inv_x), 0xff);
    assert_eq!(bv.aig.get_var_bits(*x_id).expect("var bits").len(), 8);
    assert_ne!(AigLit::new(1, true), AigLit::TRUE);
}

struct SliceOracle {
    space_id: u64,
    base: u64,
    bytes: Vec<u8>,
}

impl fission_solver::solver::MemoryOracle for SliceOracle {
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8> {
        if space_id != self.space_id || addr < self.base {
            return None;
        }
        let offset = usize::try_from(addr - self.base).ok()?;
        self.bytes.get(offset).copied()
    }
}

#[test]
fn memory_cegar_refines_unregistered_and_multibyte_selects() {
    let space = SymExpr::new_array_var("space_0", 64, 32);
    let ptr = SymExpr::new_var("ptr", 64);
    let oracle = SliceOracle {
        space_id: 0,
        base: 0x1000,
        // Little-endian 0x1122_3344 followed by 0x5566_7788.
        bytes: vec![0x44, 0x33, 0x22, 0x11, 0x88, 0x77, 0x66, 0x55],
    };

    // Unregistered `ArraySelect` appears only inside an assertion (not in `solver.nodes`).
    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(
        Box::new(ptr.clone()),
        Box::new(SymExpr::new_const(0x1000, 64)),
    ));
    solver.assert(SymExpr::Eq(
        Box::new(array_select(space.clone(), ptr.clone())),
        Box::new(SymExpr::new_const(0x1122_3344, 32)),
    ));
    assert_eq!(
        solver.check_sat_with_oracle(Some(&oracle), &[]).unwrap(),
        SatResult::Sat
    );

    // Asking for the upper byte to be 0 (or mismatching value) must be refuted.
    let mut mismatch = Solver::new();
    mismatch.assert(SymExpr::Eq(
        Box::new(ptr.clone()),
        Box::new(SymExpr::new_const(0x1004, 64)),
    ));
    assert_eq!(
        mismatch
            .check_sat_with_oracle(
                Some(&oracle),
                &[SymExpr::Neq(
                    Box::new(array_select(space, ptr)),
                    Box::new(SymExpr::new_const(0x5566_7788, 32)),
                )],
            )
            .unwrap(),
        SatResult::Unsat
    );
}

#[test]
fn array_theory_enforces_read_over_write_and_congruence_in_solver() {
    let a = SymExpr::new_array_var("mem", 8, 8);
    let i = SymExpr::new_var("i", 8);
    let j = SymExpr::new_var("j", 8);
    let v = SymExpr::new_var("v", 8);

    // Read-over-write same index: select(store(a, i, v), i) == v.
    let mut row_same = Solver::new();
    row_same.assert(not_equal(
        array_select(array_store(a.clone(), i.clone(), v.clone()), i.clone()),
        v.clone(),
    ));
    assert_eq!(row_same.check_sat().unwrap(), SatResult::Unsat);

    // Read-over-write distinct index: i != j => select(store(a, i, v), j) == select(a, j).
    let mut row_diff = Solver::new();
    row_diff.assert(SymExpr::Neq(Box::new(i.clone()), Box::new(j.clone())));
    row_diff.assert(not_equal(
        array_select(array_store(a.clone(), i.clone(), v), j.clone()),
        array_select(a.clone(), j.clone()),
    ));
    assert_eq!(row_diff.check_sat().unwrap(), SatResult::Unsat);

    // Functional consistency across distinct select sites.
    let mut cong = Solver::new();
    cong.assert(SymExpr::Eq(Box::new(i.clone()), Box::new(j.clone())));
    cong.assert(not_equal(
        array_select(a.clone(), i.clone()),
        array_select(a.clone(), j.clone()),
    ));
    assert_eq!(cong.check_sat().unwrap(), SatResult::Unsat);

    // Nested store satisfiability: store(store(a, i, 1), j, 2)[i] == 1 when i != j.
    let mut nested = Solver::new();
    let chain = array_store(
        array_store(a, i.clone(), SymExpr::new_const(1, 8)),
        j.clone(),
        SymExpr::new_const(2, 8),
    );
    nested.assert(SymExpr::Neq(Box::new(i.clone()), Box::new(j)));
    nested.assert(SymExpr::Eq(
        Box::new(array_select(chain, i)),
        Box::new(SymExpr::new_const(1, 8)),
    ));
    assert_eq!(nested.check_sat().unwrap(), SatResult::Sat);
}
