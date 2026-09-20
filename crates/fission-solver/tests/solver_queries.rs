use fission_solver::{Solver, SymExpr};

#[test]
fn eval_returns_compound_expression_values() {
    let x = SymExpr::new_var("x", 4);
    let expression = SymExpr::Add(Box::new(x.clone()), Box::new(SymExpr::new_const(1, 4)));

    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(Box::new(x), Box::new(SymExpr::new_const(2, 4))));

    assert_eq!(solver.eval(&expression, 1), vec![3]);
}

#[test]
fn eval_enumerates_distinct_compound_values() {
    let x = SymExpr::new_var("x", 2);
    let expression = SymExpr::Add(Box::new(x), Box::new(SymExpr::new_const(1, 2)));

    let mut solver = Solver::new();
    let values = solver.eval(&expression, 4);

    assert_eq!(values.len(), 4);
    let mut sorted = values;
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 4);
}

#[test]
fn min_and_max_use_the_expression_bit_width() {
    let x = SymExpr::new_var("x", 8);
    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(
        Box::new(x.clone()),
        Box::new(SymExpr::new_const(200, 8)),
    ));

    assert_eq!(solver.min(&x), Some(200));

    let x = SymExpr::new_var("x", 8);
    let mut max_solver = Solver::new();
    max_solver.assert(SymExpr::Eq(
        Box::new(x.clone()),
        Box::new(SymExpr::new_const(200, 8)),
    ));
    assert_eq!(max_solver.max(&x), Some(200));
}

#[test]
fn unsatisfiable_assumption_probes_do_not_leak_into_later_queries() {
    let x = SymExpr::new_var("x", 4);
    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(
        Box::new(x.clone()),
        Box::new(SymExpr::new_const(2, 4)),
    ));

    assert!(!solver.satisfiable(&[SymExpr::Ult(
        Box::new(SymExpr::new_const(3, 4)),
        Box::new(x.clone()),
    )]));
    assert!(solver.satisfiable(&[SymExpr::Ult(
        Box::new(SymExpr::new_const(1, 4)),
        Box::new(x),
    )]));
}

#[test]
fn eval_zero_requests_no_values() {
    let expression = SymExpr::new_const(7, 8);
    let mut solver = Solver::new();

    assert!(solver.eval(&expression, 0).is_empty());
}
