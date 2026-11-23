use super::*;

#[test]
fn constant_ptr_recovery_recovers_symbolic_addresses() {
    use crate::memory::apply_constant_ptr_recovery_pass;
    use crate::pipeline::{GLOBAL_SYMBOL_CONTEXT, GlobalSymbolContext};
    use std::collections::HashMap;

    let mut names = HashMap::default();
    names.insert(0x140003000, "g_data".to_string());
    names.insert(0x140004000, "g_exact".to_string());

    let mut sizes = HashMap::default();
    sizes.insert(0x140003000, 100);
    sizes.insert(0x140004000, 10);

    let context = GlobalSymbolContext {
        names,
        sizes,
        ..Default::default()
    };
    GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(context);
    });

    let mut func = PreHirFunction {
        variadic_fixed_arity: None,
        name: "test_constant_ptr".to_string(),
        int_param_offsets: Vec::new(),
        float_param_offsets: Vec::new(),
        float_shares_int_slots: false,
        body: vec![
            // Exact match
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p1".to_string()),
                rhs: PreHirExpr::Const(0x140004000, int(64)),
            },
            // Inside bounds (offset 8)
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p2".to_string()),
                rhs: PreHirExpr::Const(0x140003008, int(64)),
            },
            // Outside any bounds / no match
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("p3".to_string()),
                rhs: PreHirExpr::Const(0x140005000, int(64)),
            },
        ],
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
    };

    assert!(apply_constant_ptr_recovery_pass(&mut func));

    GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });

    assert_eq!(func.body.len(), 3);

    // Assert exact match: AddressOfGlobal("g_exact")
    let PreHirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else {
        panic!();
    };
    assert_eq!(rhs1, &PreHirExpr::AddressOfGlobal("g_exact".to_string()));

    // Assert offset match: PtrOffset { base: AddressOfGlobal("g_data"), offset: 8 }
    let PreHirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else {
        panic!();
    };
    assert!(
        matches!(rhs2, PreHirExpr::PtrOffset { base, offset: 8 } if matches!(base.as_ref(), PreHirExpr::AddressOfGlobal(name) if name == "g_data"))
    );

    // Assert no match: stays Const(0x140005000)
    let PreHirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else {
        panic!();
    };
    assert_eq!(rhs3, &PreHirExpr::Const(0x140005000, int(64)));
}

#[test]
fn rescue_undeclared_bindings_declares_stack_local_names() {
    let mut func = PreHirFunction {
        name: "f".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_0".to_string()),
                rhs: PreHirExpr::Var("param_2".to_string()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("local_0".to_string()))),
        ],
        ..Default::default()
    };
    assert!(rescue_undeclared_bindings(&mut func));
    assert!(
        func.locals.iter().any(|b| b.name == "local_0"),
        "local_0 must be declared: {:?}",
        func.locals
    );
}

/// `rl_point` is a global and `rbx` a register. The register test admits
/// any name that starts with `r` or `e`, and declaring the global as a local
/// cut every read and write of it off from the object.
#[test]
fn rescue_undeclared_bindings_leaves_globals_to_file_scope() {
    use crate::pipeline::{GLOBAL_SYMBOL_CONTEXT, GlobalSymbolContext};

    let rescued = |known_global: bool| {
        let mut names = std::collections::HashMap::default();
        if known_global {
            names.insert(0x1400_3000_u64, "rl_point".to_string());
        }
        GLOBAL_SYMBOL_CONTEXT.with(|slot| {
            *slot.borrow_mut() = Some(GlobalSymbolContext {
                names,
                ..Default::default()
            });
        });
        let mut func = PreHirFunction {
            name: "f".to_string(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("rl_point".to_string()),
                    rhs: PreHirExpr::Var("rbx".to_string()),
                },
                PreHirStmt::Return(None),
            ],
            ..Default::default()
        };
        rescue_undeclared_bindings(&mut func);
        GLOBAL_SYMBOL_CONTEXT.with(|slot| *slot.borrow_mut() = None);
        func.locals
            .iter()
            .map(|binding| binding.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(rescued(true), vec!["rbx".to_string()]);
    // Control: a name nothing knows as a global is still rescued.
    assert!(rescued(false).contains(&"rl_point".to_string()));
}

#[test]
fn rescue_undeclared_bindings_declares_simd_lane_names() {
    let mut func = PreHirFunction {
        name: "f".to_string(),
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xmm1_wh".to_string()),
                rhs: PreHirExpr::Const(0x1234, int(16)),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("ymm2_db".to_string()),
                rhs: PreHirExpr::Const(0x1234_5678, int(32)),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("zmm3_qword_1".to_string()),
                rhs: PreHirExpr::Const(0x1234_5678, int(64)),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xmm_register".to_string()),
                rhs: PreHirExpr::Const(0, int(32)),
            },
        ],
        ..Default::default()
    };

    assert!(rescue_undeclared_bindings(&mut func));
    for (name, bits) in [("xmm1_wh", 16), ("ymm2_db", 32), ("zmm3_qword_1", 64)] {
        assert_eq!(
            func.locals
                .iter()
                .find(|binding| binding.name == name)
                .map(|binding| &binding.ty),
            Some(&int(bits)),
            "{name} should be declared with its first assigned type: {:?}",
            func.locals
        );
    }
    assert!(
        !func
            .locals
            .iter()
            .any(|binding| binding.name == "xmm_register"),
        "the SIMD-family spelling without a numeric register index is not a register name"
    );
    assert!(
        func.locals
            .windows(2)
            .all(|pair| pair[0].name <= pair[1].name),
        "rescued declarations must have deterministic name order: {:?}",
        func.locals
    );
    assert!(!rescue_undeclared_bindings(&mut func));
}

#[test]
fn prune_unused_dead_local_bindings_then_rescue_redeclares_write_only_stack_homes() {
    // `local_18 = param_3` with no RHS uses: `prune_unused_dead_local_bindings`
    // alone drops the (now-dead) binding -- keeping it declared indefinitely
    // instead regressed real structuring tests (a stack-resident shadow of a
    // variable heritage had already promoted to a register stuck around long
    // enough to confuse loop-condition folding elsewhere). The matrix_multiply
    // undeclared-identifier bug this was meant to fix is instead closed by
    // `rescue_undeclared_bindings` re-declaring any still-referenced-but-
    // undeclared name at pipeline end (`run_stage_cleanup`'s final pass) --
    // by which point the fold/prune passes that this exception broke have
    // already run, so it can't interfere with them.
    let mut func = PreHirFunction {
        name: "f".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_3".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(2)),
            initializer: None,
        }],
        locals: vec![PreHirBinding {
            name: "local_18".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x18)),
            initializer: None,
        }],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_18".to_string()),
                rhs: PreHirExpr::Var("param_3".to_string()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            ))),
        ],
        ..Default::default()
    };
    prune_unused_dead_local_bindings(&mut func);
    assert!(
        !func.locals.iter().any(|b| b.name == "local_18"),
        "dead write-only local_18 should be pruned here: {:?}",
        func.locals
    );
    assert!(
        matches!(
            func.body.first(),
            Some(PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            }) if name == "local_18"
        ),
        "the (now-orphaned) assign itself must survive pruning -- rescue \
         re-declares it below: {:?}",
        func.body
    );

    rescue_undeclared_bindings(&mut func);
    assert!(
        func.locals.iter().any(|b| b.name == "local_18"),
        "rescue must re-declare local_18 so it's not an undeclared identifier: {:?}",
        func.locals
    );
}

#[test]
fn canonicalize_orphaned_stack_slot_name_renames_binding_and_uses() {
    let mut claimant = stack_binding("local_8_7", -8);
    claimant.initializer = Some(PreHirExpr::Var("local_8_7".to_string()));
    let mut func = PreHirFunction {
        locals: vec![claimant],
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Var(
            "local_8_7".to_string(),
        )))],
        ..Default::default()
    };

    assert!(canonicalize_orphaned_stack_slot_names(&mut func));
    assert_eq!(func.locals[0].name, "local_8");
    assert!(matches!(
        func.locals[0].initializer.as_ref(),
        Some(PreHirExpr::Var(name)) if name == "local_8"
    ));
    assert!(matches!(
        func.body.as_slice(),
        [PreHirStmt::Return(Some(PreHirExpr::Var(name)))] if name == "local_8"
    ));
}

#[test]
fn canonicalize_orphaned_stack_slot_name_preserves_live_base_collision() {
    let mut func = PreHirFunction {
        locals: vec![stack_binding("local_8", -8), stack_binding("local_8_7", -8)],
        ..Default::default()
    };

    assert!(!canonicalize_orphaned_stack_slot_names(&mut func));
    assert_eq!(
        func.locals
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>(),
        vec!["local_8", "local_8_7"]
    );
}

#[test]
fn canonicalize_orphaned_stack_slot_name_preserves_ambiguous_claimants() {
    let mut func = PreHirFunction {
        locals: vec![
            stack_binding("local_8_1", -8),
            stack_binding("local_8_2", -8),
        ],
        ..Default::default()
    };

    assert!(!canonicalize_orphaned_stack_slot_names(&mut func));
    assert_eq!(
        func.locals
            .iter()
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>(),
        vec!["local_8_1", "local_8_2"]
    );
}
