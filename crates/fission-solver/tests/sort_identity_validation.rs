use fission_solver::aig::AigManager;
use fission_solver::smtlib::{self, SmtLibError};
use fission_solver::{
    validate_assertions, validate_expr, EvalTermination, SatResult, Solver, Sort, SymExpr,
    UnknownReason, ValidationError, MAX_EXPR_DEPTH,
};

fn bv_var(id: u32, name: &str, bits: u32) -> SymExpr {
    SymExpr::Var {
        id,
        name: name.to_string(),
        sort: Sort::BitVector(bits),
    }
}

fn arr_var(id: u32, name: &str, domain_bits: u32, range_bits: u32) -> SymExpr {
    SymExpr::Var {
        id,
        name: name.to_string(),
        sort: Sort::Array {
            domain: Box::new(Sort::BitVector(domain_bits)),
            range: Box::new(Sort::BitVector(range_bits)),
        },
    }
}

fn assert_invalid_in_solver(expr: SymExpr, label: &str) {
    let mut solver = Solver::new();
    solver.assert(expr.clone());
    assert_eq!(
        solver.check_sat().expect("check_sat"),
        SatResult::Unknown,
        "{label}: Solver::assert + check_sat must reject malformed term"
    );
    assert!(
        matches!(
            solver.last_unknown(),
            Some(UnknownReason::InvalidExpression(_))
        ),
        "{label}: expected InvalidExpression, got {:?}",
        solver.last_unknown()
    );

    let mut solver_extra = Solver::new();
    let x_id = solver_extra.register_var("ok".to_string(), 8);
    let x = solver_extra.nodes()[&x_id].clone();
    solver_extra.assert(SymExpr::Eq(
        Box::new(x.clone()),
        Box::new(SymExpr::new_const(9, 8)),
    ));
    assert_eq!(solver_extra.check_sat().unwrap(), SatResult::Sat);

    assert_eq!(
        solver_extra
            .check_sat_with_oracle(None, std::slice::from_ref(&expr))
            .unwrap(),
        SatResult::Unknown,
        "{label}: invalid extra assumption must return Unknown"
    );
    assert!(
        matches!(
            solver_extra.last_unknown(),
            Some(UnknownReason::InvalidExpression(_))
        ),
        "{label}: expected InvalidExpression on extra assumption, got {:?}",
        solver_extra.last_unknown()
    );

    // Invalid temporary assumptions must not poison subsequent valid queries.
    assert_eq!(solver_extra.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver_extra.get_value(x_id), Some(9));
    assert_eq!(solver_extra.last_unknown(), None);

    // Must match SMT-LIB printer rejection.
    assert!(
        matches!(
            smtlib::script_checked(std::slice::from_ref(&expr)),
            Err(SmtLibError::Invalid(_))
        ),
        "{label}: smtlib::script_checked must reject malformed term"
    );
    assert!(
        smtlib::script(&[expr]).is_err(),
        "{label}: smtlib::script must fail"
    );
}

#[test]
fn conflicting_var_ids_across_sorts_and_widths_are_rejected_everywhere() {
    // 1. Same node ID with different BV widths in one expression.
    let v_8 = bv_var(500_001, "x", 8);
    let v_32 = bv_var(500_001, "x", 32);
    let same_expr = SymExpr::Eq(Box::new(v_8.clone()), Box::new(v_32.clone()));
    assert!(matches!(
        validate_expr(&same_expr),
        Err(ValidationError::ConflictingVarSort { id: 500_001, .. })
    ));
    assert_invalid_in_solver(same_expr, "conflicting_bv_widths_single_expr");

    // 2. Conflicting widths across separate permanent assertions.
    let a1 = SymExpr::Eq(Box::new(v_8.clone()), Box::new(SymExpr::new_const(1, 8)));
    let a2 = SymExpr::Eq(Box::new(v_32.clone()), Box::new(SymExpr::new_const(1, 32)));
    assert!(matches!(
        validate_assertions(&[a1.clone(), a2.clone()]),
        Err(ValidationError::ConflictingVarSort { id: 500_001, .. })
    ));
    assert!(matches!(
        smtlib::script_checked(&[a1.clone(), a2.clone()]),
        Err(SmtLibError::Invalid(ValidationError::ConflictingVarSort {
            id: 500_001,
            ..
        }))
    ));
    let mut s = Solver::new();
    s.assert(a1.clone());
    assert_eq!(s.check_sat().unwrap(), SatResult::Sat);
    s.assert(a2.clone());
    assert_eq!(s.check_sat().unwrap(), SatResult::Unknown);
    assert!(matches!(
        s.last_unknown(),
        Some(UnknownReason::InvalidExpression(errs))
            if matches!(errs.as_slice(), [ValidationError::ConflictingVarSort { id: 500_001, .. }])
    ));

    // 3. Conflicting sort between registered node table and assertion.
    let mut s_reg = Solver::new();
    let reg_id = s_reg.register_var("reg".to_string(), 8);
    let colliding = bv_var(reg_id, "reg", 16);
    s_reg.assert(SymExpr::Eq(
        Box::new(colliding),
        Box::new(SymExpr::new_const(0, 16)),
    ));
    assert_eq!(s_reg.check_sat().unwrap(), SatResult::Unknown);
    assert!(matches!(
        s_reg.last_unknown(),
        Some(UnknownReason::InvalidExpression(_))
    ));

    // 4. Scalar vs Array sort sharing the same node ID.
    let scalar = bv_var(500_002, "s", 8);
    let arr = arr_var(500_002, "s", 4, 8);
    let mixed_arr = SymExpr::Eq(
        Box::new(scalar),
        Box::new(SymExpr::ArraySelect {
            array: Box::new(arr),
            index: Box::new(SymExpr::new_const(0, 4)),
        }),
    );
    assert_invalid_in_solver(mixed_arr, "scalar_vs_array_same_id");

    // 5. Conflicting array domain/range sorts sharing the same node ID.
    let arr_a = arr_var(500_003, "m", 4, 8);
    let arr_b = arr_var(500_003, "m", 8, 8);
    let a_sel = SymExpr::Eq(
        Box::new(SymExpr::ArraySelect {
            array: Box::new(arr_a),
            index: Box::new(SymExpr::new_const(0, 4)),
        }),
        Box::new(SymExpr::new_const(0, 8)),
    );
    let b_sel = SymExpr::Eq(
        Box::new(SymExpr::ArraySelect {
            array: Box::new(arr_b),
            index: Box::new(SymExpr::new_const(0, 8)),
        }),
        Box::new(SymExpr::new_const(0, 8)),
    );
    let bad_conj = SymExpr::And(Box::new(a_sel), Box::new(b_sel));
    assert_invalid_in_solver(bad_conj, "conflicting_array_sorts_same_id");

    // 6. Direct AigManager lowering rejects variable ID aliasing without crashing.
    let mut aig = AigManager::new();
    let _ = aig.lower_expr(&bv_var(500_004, "a", 8));
    let _ = aig.lower_expr(&bv_var(500_004, "a", 32));
    assert!(
        aig.unsupported()
            .iter()
            .any(|msg| msg.contains("aliases conflicting sorts")),
        "expected AIG var conflict record, got {:?}",
        aig.unsupported()
    );
}

#[test]
fn malformed_array_select_store_extract_concat_ite_and_zero_width_are_rejected_without_panics() {
    let bv8 = SymExpr::new_var("bv8", 8);
    let arr = SymExpr::new_array_var("mem", 4, 8);

    // 1. ArraySelect on non-array (`get_sort`/`get_bit_width` must NOT panic in debug or release).
    let select_on_bv = SymExpr::ArraySelect {
        array: Box::new(bv8.clone()),
        index: Box::new(SymExpr::new_const(0, 4)),
    };
    let _ = select_on_bv.get_sort();
    let _ = select_on_bv.get_bit_width();
    let bad_sel_eq = SymExpr::Eq(
        Box::new(select_on_bv.clone()),
        Box::new(SymExpr::new_const(0, 8)),
    );
    assert_invalid_in_solver(bad_sel_eq, "select_on_non_array");

    let mut aig = AigManager::new();
    let _ = aig.lower_expr(&select_on_bv);
    assert!(!aig.unsupported().is_empty());

    // 2. Wrong array index sort and wrong array value sort.
    let bad_idx = SymExpr::Eq(
        Box::new(SymExpr::ArraySelect {
            array: Box::new(arr.clone()),
            index: Box::new(bv8.clone()),
        }),
        Box::new(SymExpr::new_const(0, 8)),
    );
    assert_invalid_in_solver(bad_idx, "array_select_wrong_index_width");

    let bad_store_val = SymExpr::Eq(
        Box::new(SymExpr::ArraySelect {
            array: Box::new(SymExpr::ArrayStore {
                array: Box::new(arr.clone()),
                index: Box::new(SymExpr::new_const(1, 4)),
                value: Box::new(SymExpr::new_var("v16", 16)),
            }),
            index: Box::new(SymExpr::new_const(1, 4)),
        }),
        Box::new(SymExpr::new_const(0, 8)),
    );
    assert_invalid_in_solver(bad_store_val, "array_store_wrong_value_width");

    // 3. Overflowing Concat width (`u32::MAX + 1` must NOT panic in debug or overflow in release).
    let huge = SymExpr::new_var("huge", u32::MAX);
    let concat_overflow = SymExpr::Concat(Box::new(huge), Box::new(SymExpr::new_const(1, 1)));
    let _ = concat_overflow.get_sort();
    let _ = concat_overflow.get_bit_width();
    assert!(matches!(
        validate_expr(&concat_overflow),
        Err(ValidationError::ConcatWidthOverflow { .. })
    ));
    let eq_concat = SymExpr::Eq(
        Box::new(concat_overflow),
        Box::new(SymExpr::new_const(0, 8)),
    );
    assert_invalid_in_solver(eq_concat, "concat_width_overflow");

    // 4. Invalid Extract variants.
    for extract in [
        SymExpr::Extract {
            expr: Box::new(bv8.clone()),
            lsb: 0,
            size: 0,
        },
        SymExpr::Extract {
            expr: Box::new(bv8.clone()),
            lsb: u32::MAX,
            size: 2,
        },
        SymExpr::Extract {
            expr: Box::new(bv8.clone()),
            lsb: 4,
            size: 5,
        },
    ] {
        assert_invalid_in_solver(
            SymExpr::Eq(Box::new(extract), Box::new(SymExpr::new_const(0, 1))),
            "invalid_extract",
        );
    }

    // 5. Malformed ITE: non-1-bit condition and incompatible branch sorts.
    let bad_ite_cond = SymExpr::Eq(
        Box::new(SymExpr::Ite {
            cond: Box::new(bv8.clone()),
            t: Box::new(SymExpr::new_const(1, 8)),
            f: Box::new(SymExpr::new_const(0, 8)),
        }),
        Box::new(SymExpr::new_const(1, 8)),
    );
    assert_invalid_in_solver(bad_ite_cond, "ite_non_1bit_condition");

    let bad_ite_branches = SymExpr::Eq(
        Box::new(SymExpr::Ite {
            cond: Box::new(SymExpr::new_const(1, 1)),
            t: Box::new(arr),
            f: Box::new(bv8),
        }),
        Box::new(SymExpr::new_const(0, 8)),
    );
    assert_invalid_in_solver(bad_ite_branches, "ite_array_vs_bv");

    let mismatched_fp_ite = SymExpr::Eq(
        Box::new(SymExpr::Ite {
            cond: Box::new(SymExpr::new_const(1, 1)),
            t: Box::new(SymExpr::new_float_var("f32", 32)),
            f: Box::new(SymExpr::new_float_var("f64", 64)),
        }),
        Box::new(SymExpr::new_const(0, 32)),
    );
    assert_invalid_in_solver(mismatched_fp_ite, "ite_mismatched_float_branches");

    // 6. Zero-width constant/var in solver assertion, `is_true`, and `eval_detailed`.
    let zero_const = SymExpr::Const { val: 0, size: 0 };
    assert_invalid_in_solver(zero_const.clone(), "zero_width_const");

    let mut s = Solver::new();
    assert!(!s.is_true(&zero_const));
    assert!(matches!(
        s.last_unknown(),
        Some(UnknownReason::InvalidExpression(_))
    ));
    assert!(!s.is_false(&SymExpr::new_const(0, 8)));
    assert!(matches!(
        s.last_unknown(),
        Some(UnknownReason::InvalidExpression(_))
    ));
    let report = s.eval_detailed(&zero_const, 1);
    assert!(matches!(
        report.termination,
        EvalTermination::Unknown(UnknownReason::InvalidExpression(_))
    ));
}

#[test]
fn constructor_shortcuts_never_erase_malformed_or_unsupported_operands() {
    let malformed_terms = [
        // Conflicting IDs
        SymExpr::Add(
            Box::new(bv_var(600_001, "x", 8)),
            Box::new(bv_var(600_001, "x", 16)),
        ),
        // Select on non-array
        SymExpr::ArraySelect {
            array: Box::new(SymExpr::new_var("scalar", 8)),
            index: Box::new(SymExpr::new_const(0, 4)),
        },
        // Out-of-bounds extract
        SymExpr::Extract {
            expr: Box::new(SymExpr::new_var("e", 8)),
            lsb: 6,
            size: 4,
        },
        // Malformed ITE condition
        SymExpr::Ite {
            cond: Box::new(SymExpr::new_var("c8", 8)),
            t: Box::new(SymExpr::new_const(1, 8)),
            f: Box::new(SymExpr::new_const(0, 8)),
        },
        // Overflowing Concat
        SymExpr::Concat(
            Box::new(SymExpr::new_var("wide", u32::MAX)),
            Box::new(SymExpr::new_const(1, 1)),
        ),
    ];

    for (idx, bad) in malformed_terms.into_iter().enumerate() {
        let eq_self = SymExpr::new_eq(bad.clone(), bad.clone());
        assert!(
            matches!(eq_self, SymExpr::Eq(_, _)),
            "case {idx}: new_eq must not fold malformed operand to Const"
        );
        assert_invalid_in_solver(eq_self, "reflexive_eq_malformed");

        let neq_self = SymExpr::new_neq(bad.clone(), bad.clone());
        assert!(
            matches!(neq_self, SymExpr::Neq(_, _)),
            "case {idx}: new_neq must not fold malformed operand to Const"
        );
        assert_invalid_in_solver(neq_self, "reflexive_neq_malformed");

        let sub_self = SymExpr::new_sub(bad.clone(), bad.clone());
        assert!(
            matches!(sub_self, SymExpr::Sub(_, _)),
            "case {idx}: new_sub must not fold malformed operand to 0"
        );
        let xor_self = SymExpr::new_xor(bad.clone(), bad.clone());
        assert!(
            matches!(xor_self, SymExpr::Xor(_, _)),
            "case {idx}: new_xor must not fold malformed operand to 0"
        );
        let and_zero = SymExpr::new_and(SymExpr::new_const(0, 8), bad.clone());
        assert!(
            matches!(and_zero, SymExpr::And(_, _)),
            "case {idx}: new_and(0, bad) must not absorb malformed operand"
        );
    }

    // Unsupported-but-well-formed FP expression must NOT become proof by reflexive folding,
    // and must surface `UnknownReason::Unsupported` (distinct from `InvalidExpression`).
    let fp = SymExpr::FAdd(
        Box::new(SymExpr::new_float_var("a", 32)),
        Box::new(SymExpr::new_float_var("b", 32)),
    );
    let fp_neq = SymExpr::new_neq(fp.clone(), fp);
    assert!(matches!(fp_neq, SymExpr::Neq(_, _)));
    let mut s_fp = Solver::new();
    s_fp.assert(fp_neq);
    assert_eq!(s_fp.check_sat().unwrap(), SatResult::Unknown);
    assert!(
        matches!(s_fp.last_unknown(), Some(UnknownReason::Unsupported(_))),
        "expected Unsupported for FP, got {:?}",
        s_fp.last_unknown()
    );

    // Concrete Const(0, 1) short-circuit must NOT mask an invalid sibling assertion.
    let mut s_mixed = Solver::new();
    s_mixed.assert(SymExpr::new_const(0, 1));
    s_mixed.assert(SymExpr::ArraySelect {
        array: Box::new(SymExpr::new_var("not_arr", 8)),
        index: Box::new(SymExpr::new_const(0, 4)),
    });
    assert_eq!(s_mixed.check_sat().unwrap(), SatResult::Unknown);
    assert!(matches!(
        s_mixed.last_unknown(),
        Some(UnknownReason::InvalidExpression(_))
    ));
}

#[test]
fn excessive_expression_depth_is_rejected_without_stack_overflow() {
    let mut expr = SymExpr::new_var("x", 1);
    for _ in 0..(MAX_EXPR_DEPTH + 32) {
        expr = SymExpr::Xor(Box::new(expr), Box::new(SymExpr::new_const(1, 1)));
    }
    assert!(matches!(
        validate_expr(&expr),
        Err(ValidationError::DepthExceeded { .. })
    ));
    let mut solver = Solver::new();
    solver.assert(expr);
    assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::InvalidExpression(errs))
            if matches!(errs.as_slice(), [ValidationError::DepthExceeded { .. }])
    ));
}
