use fission_solver::{SatResult, Solver, SymExpr};

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
fn aig_constant_assumptions_do_not_allocate_cnf_variables() {
    let x = SymExpr::new_var("x", 4);
    // Construct these directly so AST-level constructor folding cannot turn
    // them into literal Const nodes. The AIG layer must discover the constants.
    let tautology = SymExpr::Eq(Box::new(x.clone()), Box::new(x.clone()));
    let contradiction = SymExpr::Neq(Box::new(x.clone()), Box::new(x));

    let mut solver = Solver::new();

    assert_eq!(
        solver.check_sat_with_oracle(None, &[tautology]).unwrap(),
        SatResult::Sat
    );
    assert_eq!(solver.bv_theory.cnf.get_cnf_var_for_aig(0), 0);

    assert_eq!(
        solver
            .check_sat_with_oracle(None, &[contradiction])
            .unwrap(),
        SatResult::Unsat
    );
    assert_eq!(solver.bv_theory.cnf.get_cnf_var_for_aig(0), 0);

    // Constant model evaluation must keep using the AIG literal itself rather
    // than an unrelated SAT assignment for the reserved constant node.
    assert_eq!(
        solver
            .bv_theory
            .evaluate_expr_in_model(&solver.sat, &SymExpr::new_const(0, 4)),
        0
    );
    assert_eq!(
        solver
            .bv_theory
            .evaluate_expr_in_model(&solver.sat, &SymExpr::new_const(0xf, 4)),
        0xf
    );
}

#[test]
fn eval_zero_requests_no_values() {
    let expression = SymExpr::new_const(7, 8);
    let mut solver = Solver::new();

    assert!(solver.eval(&expression, 0).is_empty());
}

#[test]
fn maximum_includes_the_unsigned_domain_endpoint() {
    for width in [1, 2, 4, 8, 16, 32, 64] {
        let mut solver = Solver::new();
        let id = solver.register_var(format!("endpoint_{width}"), width);
        let value = solver.nodes[&id].clone();
        let expected = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        assert_eq!(solver.max(&value), Some(expected), "width {width}");
        assert_eq!(solver.bv_theory.cnf.get_cnf_var_for_aig(0), 0);
    }
}

#[test]
fn extrema_match_every_small_unsigned_interval() {
    for width in 1..=4 {
        let domain_max = (1u64 << width) - 1;
        for lower in 0..=domain_max {
            for upper in lower..=domain_max {
                let mut solver = Solver::new();
                let id = solver.register_var("interval".to_string(), width);
                let value = solver.nodes[&id].clone();
                solver.assert(SymExpr::Ule(
                    Box::new(SymExpr::new_const(lower, width)),
                    Box::new(value.clone()),
                ));
                solver.assert(SymExpr::Ule(
                    Box::new(value.clone()),
                    Box::new(SymExpr::new_const(upper, width)),
                ));
                assert_eq!(
                    solver.min(&value),
                    Some(lower),
                    "min {width} bits [{lower}, {upper}]"
                );
                assert_eq!(
                    solver.max(&value),
                    Some(upper),
                    "max {width} bits [{lower}, {upper}]"
                );
                assert_eq!(
                    solver.min(&value),
                    Some(lower),
                    "query isolation {width} bits [{lower}, {upper}]"
                );
                assert_eq!(solver.bv_theory.cnf.get_cnf_var_for_aig(0), 0);
            }
        }
    }
}

#[test]
fn extrema_keep_unsat_and_unknown_distinct_from_values() {
    let mut solver = Solver::new();
    let id = solver.register_var("empty".to_string(), 8);
    let value = solver.nodes[&id].clone();
    solver.assert(SymExpr::new_const(0, 1));
    assert_eq!(solver.max(&value), None);
    assert_eq!(solver.min(&value), None);

    let mut solver = Solver::new();
    let value = SymExpr::Slt(
        Box::new(SymExpr::new_var("narrow", 8)),
        Box::new(SymExpr::new_var("wide", 16)),
    );
    assert_eq!(solver.max(&value), None);
    assert_eq!(solver.min(&value), None);
}

#[test]
fn sat_then_unsat_and_budget_unknown_invalidate_prior_model() {
    use fission_solver::{AbsentModelReason, ModelLookup, ModelValue, UnknownReason};

    let mut solver = Solver::new();
    let id = solver.register_var("x".to_string(), 8);
    let x = solver.nodes[&id].clone();
    solver.assert(SymExpr::Eq(
        Box::new(x.clone()),
        Box::new(SymExpr::new_const(5, 8)),
    ));

    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver.get_value(id), Some(5));
    assert_eq!(
        solver.get_model_value(id),
        ModelLookup::Value(ModelValue::Assigned(5))
    );
    let snapshot = solver
        .published_model()
        .expect("SAT query publishes snapshot")
        .clone();
    assert_eq!(snapshot.query_epoch, 1);
    assert_eq!(snapshot.get_value(id), Some(5));

    // SAT -> UNSAT must immediately clear both `get_value` and `solver.model`.
    assert_eq!(
        solver
            .check_sat_with_oracle(
                None,
                &[SymExpr::Neq(
                    Box::new(x.clone()),
                    Box::new(SymExpr::new_const(5, 8))
                )]
            )
            .unwrap(),
        SatResult::Unsat
    );
    assert_eq!(solver.get_value(id), None);
    assert_eq!(
        solver.get_model_value(id),
        ModelLookup::Absent(AbsentModelReason::NoPublishedModel)
    );
    assert!(solver.model.is_empty());
    assert!(solver.published_model().is_none());
    // Immutable snapshot previously retained by caller remains intact.
    assert_eq!(snapshot.get_value(id), Some(5));

    // Re-publish SAT model on fresh context, then hit ConflictBudget Unknown on a
    // non-trivial multiplication formula.
    let mut b_solver = Solver::new();
    let bid = b_solver.register_var("k".to_string(), 6);
    let m = SymExpr::new_var("m", 6);
    let k = b_solver.nodes[&bid].clone();
    assert_eq!(b_solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(b_solver.get_value(bid), Some(0));

    let non_comm = SymExpr::Neq(
        Box::new(SymExpr::Mul(Box::new(k.clone()), Box::new(m.clone()))),
        Box::new(SymExpr::Mul(Box::new(m), Box::new(k))),
    );
    b_solver.set_conflict_budget(Some(10));
    let verdict = b_solver.check_sat_with_oracle(None, &[non_comm]).unwrap();
    assert_eq!(verdict, SatResult::Unknown);
    assert_eq!(
        b_solver.last_unknown(),
        Some(&UnknownReason::ConflictBudget)
    );
    assert_eq!(b_solver.get_value(bid), None);
    assert!(b_solver.model.is_empty());
    assert!(b_solver.published_model().is_none());
}

#[test]
fn oracle_refinement_exhaustion_publishes_no_candidate_model() {
    use fission_solver::{MemoryOracle, UnknownReason};

    struct AdversarialOracle;
    impl MemoryOracle for AdversarialOracle {
        fn read_concrete(&self, _space_id: u64, addr: u64) -> Option<u8> {
            // Always disagrees with `mem[idx] == idx` on the low byte.
            Some((addr as u8) ^ 1)
        }
    }

    let mut solver = Solver::new();
    let idx_id = solver.register_var("idx".to_string(), 16);
    let idx = solver.nodes[&idx_id].clone();
    let mem = SymExpr::new_array_var("space_1", 16, 8);
    let sel = SymExpr::ArraySelect {
        array: Box::new(mem),
        index: Box::new(idx.clone()),
    };
    let idx_low = SymExpr::Extract {
        expr: Box::new(idx),
        lsb: 0,
        size: 8,
    };
    solver.assert(SymExpr::Eq(Box::new(sel), Box::new(idx_low)));

    let verdict = solver
        .check_sat_with_oracle(Some(&AdversarialOracle), &[])
        .unwrap();
    assert_eq!(verdict, SatResult::Unknown);
    assert_eq!(
        solver.last_unknown(),
        Some(&UnknownReason::OracleRefinement)
    );
    assert_eq!(solver.get_value(idx_id), None);
    assert!(solver.model.is_empty());
    assert!(solver.published_model().is_none());
}

#[test]
fn assertion_mutations_and_assigned_vs_default_completion_are_explicit() {
    use fission_solver::{ast::Sort, AbsentModelReason, ModelLookup, ModelValue};

    let mut solver = Solver::new();
    let c_id = solver.register_var("constrained".to_string(), 8);
    let f_id = solver.register_var("free".to_string(), 8);
    let mut nested = SymExpr::new_array_var("nested", 64, 8);
    if let SymExpr::Var { sort, .. } = &mut nested {
        *sort = Sort::Array {
            domain: Box::new(Sort::BitVector(64)),
            range: Box::new(Sort::Array {
                domain: Box::new(Sort::BitVector(32)),
                range: Box::new(Sort::BitVector(8)),
            }),
        };
    }
    let n_id = solver.register_node(nested);

    let c = solver.nodes[&c_id].clone();
    solver.assert(SymExpr::Eq(
        Box::new(c.clone()),
        Box::new(SymExpr::new_const(9, 8)),
    ));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);

    assert_eq!(
        solver.get_model_value(c_id),
        ModelLookup::Value(ModelValue::Assigned(9))
    );
    assert_eq!(solver.get_assigned_value(c_id), Some(9));
    assert_eq!(
        solver.get_model_value(f_id),
        ModelLookup::Value(ModelValue::DefaultCompleted(0))
    );
    assert_eq!(solver.get_assigned_value(f_id), None);
    assert_eq!(solver.get_value(f_id), Some(0));
    assert_eq!(
        solver.get_model_value(n_id),
        ModelLookup::Absent(AbsentModelReason::UnextractableSortOrWidth)
    );
    assert_eq!(
        solver.get_model_value(u32::MAX - 1),
        ModelLookup::Absent(AbsentModelReason::UnregisteredNode)
    );

    // Direct mutation on public `solver.assertions` invalidates stale readers.
    solver.assertions.push(SymExpr::Eq(
        Box::new(c.clone()),
        Box::new(SymExpr::new_const(1, 8)),
    ));
    assert_eq!(solver.get_value(c_id), None);
    assert_eq!(
        solver.get_model_value(c_id),
        ModelLookup::Absent(AbsentModelReason::AssertionsModified)
    );
    assert!(solver.published_model().is_none());

    // Solving now refutes the contradiction cleanly.
    assert_eq!(solver.check_sat().unwrap(), SatResult::Unsat);
    assert!(solver.model.is_empty());

    // Calling `assert` directly also clears any active model immediately.
    let mut solver2 = Solver::new();
    let v_id = solver2.register_var("v".to_string(), 8);
    let v = solver2.nodes[&v_id].clone();
    solver2.assert(SymExpr::Eq(
        Box::new(v.clone()),
        Box::new(SymExpr::new_const(3, 8)),
    ));
    assert_eq!(solver2.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver2.get_value(v_id), Some(3));
    solver2.assert(SymExpr::Eq(Box::new(v), Box::new(SymExpr::new_const(4, 8))));
    assert_eq!(solver2.get_value(v_id), None);
    assert!(solver2.model.is_empty());
}

#[test]
fn temporary_unsupported_eval_does_not_poison_subsequent_supported_queries() {
    use fission_solver::EvalTermination;

    let mut solver = Solver::new();
    let x_id = solver.register_var("x".to_string(), 8);
    let x = solver.nodes[&x_id].clone();
    solver.assert(SymExpr::Eq(
        Box::new(x.clone()),
        Box::new(SymExpr::new_const(42, 8)),
    ));

    // 1. Temporary unsupported float expression in `eval`.
    let temp_fp = SymExpr::FAdd(
        Box::new(SymExpr::new_float_var("a", 32)),
        Box::new(SymExpr::new_float_var("b", 32)),
    );
    let report_fp = solver.eval_detailed(&temp_fp, 1);
    assert!(report_fp.values.is_empty());
    assert!(!report_fp.base_satisfiable);
    assert!(matches!(report_fp.termination, EvalTermination::Unknown(_)));
    assert!(solver.bv_theory.unsupported().is_empty());

    // 2. Temporary unsupported wide (>64-bit) variable in `eval` and `min`/`max`.
    let temp_wide = SymExpr::new_var("temp_wide", 128);
    assert!(solver.eval(&temp_wide, 1).is_empty());
    assert_eq!(solver.min(&temp_wide), None);
    assert_eq!(solver.max(&temp_wide), None);
    assert!(solver.bv_theory.unsupported().is_empty());

    // 3. Subsequent supported queries remain completely unaffected.
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver.get_value(x_id), Some(42));
    let ok = solver.eval_detailed(&x, 2);
    assert_eq!(ok.values, vec![42]);
    assert_eq!(ok.termination, EvalTermination::Exhausted);
    assert!(ok.base_satisfiable);

    // 4. Persistent unsupported assertion STILL taints all subsequent queries.
    solver.assert(SymExpr::Eq(Box::new(temp_fp.clone()), Box::new(temp_fp)));
    assert!(!solver.bv_theory.unsupported().is_empty());
    assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
    assert!(solver.eval(&x, 1).is_empty());
}

#[test]
fn eval_detailed_distinguishes_zero_limit_exhausted_and_budget_unknown() {
    use fission_solver::{EvalTermination, UnknownReason};

    let mut solver = Solver::new();
    let x_id = solver.register_var("x".to_string(), 2);
    let x = solver.nodes[&x_id].clone();

    let zero = solver.eval_detailed(&x, 0);
    assert!(zero.values.is_empty());
    assert_eq!(zero.termination, EvalTermination::ZeroRequested);
    assert!(!zero.base_satisfiable);

    let limited = solver.eval_detailed(&x, 2);
    assert_eq!(limited.values.len(), 2);
    assert_eq!(limited.termination, EvalTermination::LimitReached);
    assert!(limited.base_satisfiable);
    assert_eq!(limited.witnesses.len(), 2);
    // On an unconstrained variable, the first probe has `cnf_var == 0`
    // (default-completed 0), whereas adding `x != 0` forces CNF assignment on
    // subsequent probes (`exact == true`).
    assert!(!limited.witnesses[0].exact);
    assert!(limited.witnesses[1].exact);
    assert_ne!(
        limited.witnesses[0].query_epoch,
        limited.witnesses[1].query_epoch
    );

    let full = solver.eval_detailed(&x, 8);
    assert_eq!(full.values.len(), 4);
    assert_eq!(full.termination, EvalTermination::Exhausted);
    assert!(full.base_satisfiable);
    // Enumeration ended via refuted exclusion (`Unsat`), so no stale probe
    // model may remain published afterward.
    assert_eq!(solver.get_value(x_id), None);
    assert!(solver.model.is_empty());

    // When `y` is constrained before enumeration, iteration 0 itself is exact.
    let mut c_solver = Solver::new();
    let y_id = c_solver.register_var("y".to_string(), 4);
    let y = c_solver.nodes[&y_id].clone();
    c_solver.assert(SymExpr::Eq(
        Box::new(y.clone()),
        Box::new(SymExpr::new_const(3, 4)),
    ));
    let exact_report = c_solver.eval_detailed(&y, 2);
    assert_eq!(exact_report.values, vec![3]);
    assert_eq!(exact_report.termination, EvalTermination::Exhausted);
    assert_eq!(exact_report.witnesses.len(), 1);
    assert!(exact_report.witnesses[0].exact);

    // Incomplete enumeration on Unknown (conflict budget exhausted) must report
    // `Unknown(ConflictBudget)` rather than `Exhausted`.
    let mut b_solver = Solver::new();
    let k = SymExpr::new_var("k", 6);
    let m = SymExpr::new_var("m", 6);
    b_solver.assert(SymExpr::Neq(
        Box::new(SymExpr::Mul(Box::new(k.clone()), Box::new(m.clone()))),
        Box::new(SymExpr::Mul(Box::new(m), Box::new(k.clone()))),
    ));
    b_solver.set_conflict_budget(Some(10));
    let timed = b_solver.eval_detailed(&k, 4);
    assert_eq!(
        timed.termination,
        EvalTermination::Unknown(UnknownReason::ConflictBudget)
    );
    assert!(!timed.base_satisfiable);

    // Genuine UNSAT context reports `Exhausted` with `base_satisfiable: false`.
    solver.assert(SymExpr::new_const(0, 1));
    let unsat = solver.eval_detailed(&x, 4);
    assert!(unsat.values.is_empty());
    assert_eq!(unsat.termination, EvalTermination::Exhausted);
    assert!(!unsat.base_satisfiable);
}

#[test]
fn partially_unsupported_sat_does_not_publish_candidate_model() {
    use fission_solver::UnknownReason;

    let mut solver = Solver::new();
    let x_id = solver.register_var("x".to_string(), 8);
    let x = solver.nodes[&x_id].clone();
    solver.assert(SymExpr::Eq(Box::new(x), Box::new(SymExpr::new_const(5, 8))));

    // Combining a satisfiable constraint with an unsupported float operation
    // makes `bv_theory.unsupported()` non-empty even when the SAT core runs.
    let fp = SymExpr::FAdd(
        Box::new(SymExpr::new_float_var("a", 32)),
        Box::new(SymExpr::new_float_var("b", 32)),
    );
    solver.assert(SymExpr::Eq(Box::new(fp.clone()), Box::new(fp)));

    assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::Unsupported(_))
    ));
    assert_eq!(solver.get_value(x_id), None);
    assert!(solver.model.is_empty());
    assert!(solver.published_model().is_none());
}
