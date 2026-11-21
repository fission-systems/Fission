//! What the array theory may assume about the sorts it is handed.
//!
//! `Sort::Array` names an index width and an element width, and nothing
//! stops a caller building a store whose value is wider than the element, or
//! an array the theory writes no axioms for. SMT-LIB has neither term, so a
//! differential against another solver cannot reach them and there is no
//! definition to check a verdict against. What can be checked is that the
//! solver does not invent one: each of these used to come back with a
//! verdict, and each verdict was wrong.

use fission_solver::{SatResult, Solver, SymExpr};

fn select(array: &SymExpr, index: SymExpr) -> SymExpr {
    SymExpr::ArraySelect {
        array: Box::new(array.clone()),
        index: Box::new(index),
    }
}

fn store(array: &SymExpr, index: SymExpr, value: SymExpr) -> SymExpr {
    SymExpr::ArrayStore {
        array: Box::new(array.clone()),
        index: Box::new(index),
        value: Box::new(value),
    }
}

fn eq(a: SymExpr, b: SymExpr) -> SymExpr {
    SymExpr::Eq(Box::new(a), Box::new(b))
}

/// `v == 0x1234`, and some other slot of `store(a, 0, v)` reads zero. The
/// second assertion says nothing about `v`, so the two together have a
/// model whatever a store of sixteen bits into a byte is taken to mean. The
/// store axiom turned it into `v < 0x100` and the answer into UNSAT.
#[test]
fn a_store_wider_than_the_element_does_not_constrain_the_value() {
    let memory = SymExpr::new_array_var("m", 4, 8);
    let value = SymExpr::new_var("v", 16);
    let written = store(&memory, SymExpr::new_const(0, 4), value.clone());

    let mut solver = Solver::new();
    solver.assert(eq(value, SymExpr::new_const(0x1234, 16)));
    solver.assert(eq(
        select(&written, SymExpr::new_const(1, 4)),
        SymExpr::new_const(0, 8),
    ));
    assert_ne!(
        solver.check_sat().expect("check_sat"),
        SatResult::Unsat,
        "nothing here contradicts v == 0x1234"
    );
}

/// The same store with a value that fits is an ordinary read-over-write, and
/// has to stay decided: this is the control for the test above, which would
/// also pass if every array problem came back `Unknown`.
#[test]
fn a_store_of_the_element_width_is_still_decided() {
    let memory = SymExpr::new_array_var("m", 4, 8);
    let value = SymExpr::new_var("v", 8);
    let written = store(&memory, SymExpr::new_const(0, 4), value.clone());

    let mut solver = Solver::new();
    solver.assert(eq(value.clone(), SymExpr::new_const(0x34, 8)));
    solver.assert(eq(
        select(&written, SymExpr::new_const(1, 4)),
        SymExpr::new_const(0, 8),
    ));
    assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Sat);

    // And the read at the written slot is the value: asking for anything
    // else is a contradiction.
    let mut solver = Solver::new();
    solver.assert(eq(value, SymExpr::new_const(0x34, 8)));
    solver.assert(eq(
        select(&written, SymExpr::new_const(0, 4)),
        SymExpr::new_const(0x35, 8),
    ));
    assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Unsat);
}

/// The theory writes axioms only for arrays indexed by at most 64 bits. A
/// read of a wider-indexed array through a narrow index was still lowered as
/// if it had them -- free bits and no record of it -- so two reads at one
/// index could be told apart.
#[test]
fn a_read_of_an_array_the_theory_skips_is_not_a_verdict() {
    let memory = SymExpr::new_array_var("huge", 128, 8);
    let index = SymExpr::new_var("i", 8);
    let mut solver = Solver::new();
    solver.assert(SymExpr::Neq(
        Box::new(select(&memory, index.clone())),
        Box::new(select(
            &memory,
            SymExpr::Add(Box::new(index), Box::new(SymExpr::new_const(0, 8))),
        )),
    ));
    assert_eq!(
        solver.check_sat().expect("check_sat"),
        SatResult::Unknown,
        "a[i] != a[i + 0] has no model, and this solver has no axioms to know it"
    );
}

/// The control: the same question about an array the theory covers is
/// decided, and decided UNSAT.
#[test]
fn the_same_read_of_a_covered_array_is_refuted() {
    let memory = SymExpr::new_array_var("m", 64, 8);
    let index = SymExpr::new_var("i", 64);
    let mut solver = Solver::new();
    solver.assert(SymExpr::Neq(
        Box::new(select(&memory, index.clone())),
        Box::new(select(
            &memory,
            SymExpr::Add(Box::new(index), Box::new(SymExpr::new_const(0, 64))),
        )),
    ));
    assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Unsat);
}

#[test]
fn an_unsupported_array_assumption_does_not_change_later_queries() {
    let memory = SymExpr::new_array_var("m", 4, 8);
    let value = SymExpr::new_var("v", 16);
    let written = store(&memory, SymExpr::new_const(0, 4), value.clone());
    let unsupported = eq(
        select(&written, SymExpr::new_const(1, 4)),
        SymExpr::new_const(0, 8),
    );

    let mut solver = Solver::new();
    solver.assert(eq(value, SymExpr::new_const(0x1234, 16)));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    for _ in 0..2 {
        assert_eq!(
            solver
                .check_sat_with_oracle(None, std::slice::from_ref(&unsupported))
                .unwrap(),
            SatResult::Unknown
        );
        assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    }
    solver.assert(unsupported);
    assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
}
