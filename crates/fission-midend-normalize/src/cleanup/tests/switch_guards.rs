use super::*;

fn switch_norm_func(body: Vec<PreHirStmt>) -> PreHirFunction {
    PreHirFunction {
        variadic_fixed_arity: None,
        name: "test_switch_norm".to_string(),
        int_param_offsets: Vec::new(),
        float_param_offsets: Vec::new(),
        float_shares_int_slots: false,
        body,
        params: Vec::new(),
        locals: Vec::new(),
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        is_big_endian: false,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    }
}

fn switch_norm_ret(value: i64) -> PreHirStmt {
    PreHirStmt::Return(Some(PreHirExpr::Const(value, int(32))))
}

fn switch_norm_guard(op: PreHirBinaryOp, limit: i64) -> PreHirExpr {
    PreHirExpr::Binary {
        op,
        lhs: Box::new(PreHirExpr::Var("x".to_string())),
        rhs: Box::new(PreHirExpr::Const(limit, int(32))),
        ty: NirType::Bool,
    }
}

/// `switch (x)` with one `return 10 + label` case per label.
fn switch_norm_switch(labels: &[i64], default: Vec<PreHirStmt>) -> PreHirStmt {
    PreHirStmt::Switch {
        expr: PreHirExpr::Var("x".to_string()),
        cases: labels
            .iter()
            .map(|&v| PreHirSwitchCase {
                values: vec![v],
                body: vec![switch_norm_ret(10 + v)].into(),
            })
            .collect(),
        default: default.into(),
    }
}

/// `if (cond) { switch } else { other }` with the switch on the then arm.
fn switch_norm_then_guard(
    cond: PreHirExpr,
    switch: PreHirStmt,
    other: Vec<PreHirStmt>,
) -> PreHirFunction {
    switch_norm_func(vec![PreHirStmt::If {
        cond,
        then_body: vec![switch].into(),
        else_body: other.into(),
    }])
}

#[test]
fn switch_norm_folds_range_check_guard_when_cases_cover_it() {
    // `x < 5` admits exactly 0..=4 and every one has a case, so nothing
    // guarded can reach the default and the folded default is the else arm.
    let mut func = switch_norm_then_guard(
        switch_norm_guard(PreHirBinaryOp::Lt, 5),
        switch_norm_switch(&[0, 1, 2, 3, 4], Vec::new()),
        vec![switch_norm_ret(20)],
    );

    assert!(apply_switch_norm_pass(&mut func));
    assert_eq!(func.body.len(), 1);
    let PreHirStmt::Switch {
        expr,
        cases,
        default,
    } = &func.body[0]
    else {
        panic!("expected switch statement");
    };
    assert_eq!(expr, &PreHirExpr::Var("x".to_string()));
    assert_eq!(cases.len(), 5);
    assert_eq!(default.as_slice(), &[switch_norm_ret(20)]);
}

#[test]
fn switch_norm_does_not_fold_guard_with_uncovered_hole() {
    // x == 2 passes `x < 5`, matches no case, and originally fell out of the
    // `if`; a fold would run `return 20` for it instead.
    let mut func = switch_norm_then_guard(
        switch_norm_guard(PreHirBinaryOp::Lt, 5),
        switch_norm_switch(&[0, 1, 3, 4], Vec::new()),
        vec![switch_norm_ret(20)],
    );
    let before = func.body.clone();

    assert!(!apply_switch_norm_pass(&mut func));
    assert_eq!(func.body, before);
}

#[test]
fn switch_norm_does_not_fold_equality_guard() {
    // For x == 1 the guard is false, so the original runs the else arm; the
    // folded switch would take `case 1`.
    let mut func = switch_norm_then_guard(
        switch_norm_guard(PreHirBinaryOp::Eq, 42),
        switch_norm_switch(&[1, 2], Vec::new()),
        vec![switch_norm_ret(20)],
    );
    let before = func.body.clone();

    assert!(!apply_switch_norm_pass(&mut func));
    assert_eq!(func.body, before);
}

#[test]
fn switch_norm_does_not_fold_case_outside_the_guard() {
    // `case 7` is unreachable under `x <= 3` in the original but would run
    // its body after the fold.
    let mut func = switch_norm_then_guard(
        switch_norm_guard(PreHirBinaryOp::Le, 3),
        switch_norm_switch(&[0, 1, 2, 3, 7], Vec::new()),
        vec![switch_norm_ret(20)],
    );
    let before = func.body.clone();

    assert!(!apply_switch_norm_pass(&mut func));
    assert_eq!(func.body, before);
}

#[test]
fn switch_norm_does_not_fold_when_the_arms_disagree_on_default() {
    // The switch has its own default and the else arm differs: out-of-range
    // values and in-range misses need different code.
    let mut func = switch_norm_then_guard(
        switch_norm_guard(PreHirBinaryOp::Lt, 5),
        switch_norm_switch(&[0, 1], vec![switch_norm_ret(30)]),
        vec![switch_norm_ret(20)],
    );
    let before = func.body.clone();

    assert!(!apply_switch_norm_pass(&mut func));
    assert_eq!(func.body, before);
}

#[test]
fn switch_norm_does_not_fold_when_else_is_empty_but_switch_has_default() {
    // Values failing the guard originally did nothing; folding would hand
    // them the switch's default.
    let mut func = switch_norm_then_guard(
        switch_norm_guard(PreHirBinaryOp::Lt, 5),
        switch_norm_switch(&[0, 1], vec![switch_norm_ret(30)]),
        Vec::new(),
    );
    let before = func.body.clone();

    assert!(!apply_switch_norm_pass(&mut func));
    assert_eq!(func.body, before);
}

#[test]
fn switch_norm_guard_bounds_do_not_overflow() {
    // `x < i64::MIN` / a huge `x <= L` must be rejected, not wrapped.
    for (op, limit) in [
        (PreHirBinaryOp::Lt, i64::MIN),
        (PreHirBinaryOp::Le, i64::MAX),
        (PreHirBinaryOp::Lt, i64::MAX),
    ] {
        let mut func = switch_norm_then_guard(
            switch_norm_guard(op, limit),
            switch_norm_switch(&[0, 1], Vec::new()),
            vec![switch_norm_ret(20)],
        );
        let before = func.body.clone();

        assert!(!apply_switch_norm_pass(&mut func));
        assert_eq!(func.body, before);
    }
}

#[test]
fn switch_norm_folds_when_both_arms_share_the_default() {
    // Failing the guard and missing every case both run `return 20`, so a
    // partial case set folds soundly.
    let mut func = switch_norm_then_guard(
        switch_norm_guard(PreHirBinaryOp::Lt, 5),
        switch_norm_switch(&[0, 2], vec![switch_norm_ret(20)]),
        vec![switch_norm_ret(20)],
    );

    assert!(apply_switch_norm_pass(&mut func));
    let PreHirStmt::Switch { default, .. } = &func.body[0] else {
        panic!("expected switch statement");
    };
    assert_eq!(default.as_slice(), &[switch_norm_ret(20)]);
}

#[test]
fn switch_norm_folds_inverted_guard_with_the_switch_on_the_else_arm() {
    // `if (x > 3) return 20; else switch (x) {0..=3}`: the else arm is the
    // guarded one, so its predicate is the negation, `x <= 3`.
    let mut func = switch_norm_func(vec![PreHirStmt::If {
        cond: switch_norm_guard(PreHirBinaryOp::Gt, 3),
        then_body: vec![switch_norm_ret(20)].into(),
        else_body: vec![switch_norm_switch(&[0, 1, 2, 3], Vec::new())].into(),
    }]);

    assert!(apply_switch_norm_pass(&mut func));
    let PreHirStmt::Switch { cases, default, .. } = &func.body[0] else {
        panic!("expected switch statement");
    };
    assert_eq!(cases.len(), 4);
    assert_eq!(default.as_slice(), &[switch_norm_ret(20)]);
}

#[test]
fn switch_norm_does_not_fold_the_wrong_arm_of_an_inverted_guard() {
    // `if (x <= 3) return 20; else switch (x) {0..=3}`: the switch arm runs
    // only for x > 3, so none of its labels satisfy the guard.
    let mut func = switch_norm_func(vec![PreHirStmt::If {
        cond: switch_norm_guard(PreHirBinaryOp::Le, 3),
        then_body: vec![switch_norm_ret(20)].into(),
        else_body: vec![switch_norm_switch(&[0, 1, 2, 3], Vec::new())].into(),
    }]);
    let before = func.body.clone();

    assert!(!apply_switch_norm_pass(&mut func));
    assert_eq!(func.body, before);
}
