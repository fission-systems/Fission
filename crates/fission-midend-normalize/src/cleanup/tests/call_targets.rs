use super::*;

#[test]
fn deindirect_resolves_const_address_to_symbol() {
    use crate::cleanup::apply_deindirect_pass;
    use indexmap::IndexMap;

    let mut callee_summaries = IndexMap::new();
    callee_summaries.insert(
        "target_func".to_string(),
        CallSummary {
            target: CallTargetRef {
                address: Some(0x401000),
                symbol: "target_func".to_string(),
                provenance: CallTargetProvenance::Reference,
                edge_kind: CallEdgeKind::Reference,
                confidence: 128,
            },
            prototype: PrototypeSummary {
                variadic_fixed_arity: None,
                min_arity: 1,
                max_arity: 1,
                locked_exact_arity: None,
                returns_void: false,
                return_lattice: NirType::Unknown,
                param_lattices: vec![NirType::Unknown],
                param_surface_type_names: vec![None],
                param_pointer_contracts: vec![false],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                regions: Vec::new(),
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 160,
            },
        },
    );

    let mut func = PreHirFunction {
        name: "test_deindirect_const".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![PreHirStmt::Expr(PreHirExpr::Call {
            origin: None,
            target: "__fission_callind_opaque".to_string(),
            args: vec![
                PreHirExpr::Const(0x401000, int(64)),
                PreHirExpr::Var("param_1".to_string()),
            ],
            ty: NirType::Unknown,
        })],
        callee_summaries: callee_summaries.into(),
        ..Default::default()
    };

    assert!(apply_deindirect_pass(&mut func));
    let PreHirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression");
    };
    assert_eq!(target, "target_func");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0], PreHirExpr::Var("param_1".to_string()));
}

#[test]
fn deindirect_resolves_var_initializer_to_symbol() {
    use crate::cleanup::apply_deindirect_pass;
    use indexmap::IndexMap;

    let mut callee_summaries = IndexMap::new();
    callee_summaries.insert(
        "target_func".to_string(),
        CallSummary {
            target: CallTargetRef {
                address: Some(0x401000),
                symbol: "target_func".to_string(),
                provenance: CallTargetProvenance::Reference,
                edge_kind: CallEdgeKind::Reference,
                confidence: 128,
            },
            prototype: PrototypeSummary {
                variadic_fixed_arity: None,
                min_arity: 0,
                max_arity: 0,
                locked_exact_arity: None,
                returns_void: false,
                return_lattice: NirType::Unknown,
                param_lattices: vec![],
                param_surface_type_names: vec![],
                param_pointer_contracts: vec![],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                regions: Vec::new(),
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 160,
            },
        },
    );

    let mut func = PreHirFunction {
        name: "test_deindirect_var".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![PreHirBinding {
            name: "fn_ptr".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: None,
            initializer: Some(PreHirExpr::Const(0x401000, int(64))),
        }],
        body: vec![PreHirStmt::Expr(PreHirExpr::Call {
            origin: None,
            target: "__fission_callind_opaque".to_string(),
            args: vec![PreHirExpr::Var("fn_ptr".to_string())],
            ty: NirType::Unknown,
        })],
        callee_summaries: callee_summaries.into(),
        ..Default::default()
    };

    assert!(apply_deindirect_pass(&mut func));
    let PreHirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression");
    };
    assert_eq!(target, "target_func");
    assert!(args.is_empty());
}

#[test]
fn deindirect_resolves_iat_load_to_symbol() {
    // Regression test: x86-64 Sleigh emits `CALL qword ptr [IAT_addr]` as
    //   Copy unique <- [ram:IAT_addr]   (Load from IAT slot)
    //   CallInd unique
    // which the builder lowers to:
    //   __fission_callind_opaque(Load { ptr: Const(IAT_addr) }, ...)
    // The deindirect pass must resolve this to a named direct call.
    use crate::cleanup::apply_deindirect_pass;
    use indexmap::IndexMap;

    const IAT_SLOT_ADDR: u64 = 0x1400082c8;

    let mut callee_summaries = IndexMap::new();
    callee_summaries.insert(
        "InitializeCriticalSection".to_string(),
        CallSummary {
            target: CallTargetRef {
                address: Some(IAT_SLOT_ADDR),
                symbol: "InitializeCriticalSection".to_string(),
                provenance: CallTargetProvenance::Import,
                edge_kind: CallEdgeKind::Import,
                confidence: 255,
            },
            prototype: PrototypeSummary {
                variadic_fixed_arity: None,
                min_arity: 1,
                max_arity: 1,
                locked_exact_arity: None,
                returns_void: false,
                return_lattice: NirType::Unknown,
                param_lattices: vec![NirType::Unknown],
                param_surface_type_names: vec![None],
                param_pointer_contracts: vec![false],
                soundness: SummarySoundness::Optimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: None,
                regions: Vec::new(),
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: 255,
            },
        },
    );

    let iat_load = PreHirExpr::Load {
        ptr: Box::new(PreHirExpr::Const(IAT_SLOT_ADDR as i64, int(64))),
        ty: NirType::Unknown,
    };

    let mut func = PreHirFunction {
        name: "test_deindirect_iat_load".to_string(),
        int_param_offsets: Vec::new(),
        body: vec![PreHirStmt::Expr(PreHirExpr::Call {
            origin: None,
            target: "__fission_callind_opaque".to_string(),
            args: vec![iat_load, PreHirExpr::Var("param_1".to_string())],
            ty: NirType::Unknown,
        })],
        callee_summaries: callee_summaries.into(),
        ..Default::default()
    };

    assert!(
        apply_deindirect_pass(&mut func),
        "deindirect pass must rewrite IAT-load indirect call"
    );
    let PreHirStmt::Expr(expr) = &func.body[0] else {
        panic!("expected expression statement");
    };
    let PreHirExpr::Call { target, args, .. } = expr else {
        panic!("expected call expression after deindirect");
    };
    assert_eq!(
        target, "InitializeCriticalSection",
        "call target must be resolved from IAT slot"
    );
    assert_eq!(args.len(), 1);
    assert_eq!(args[0], PreHirExpr::Var("param_1".to_string()));
}

// Dual-layer C rendering of CALLIND opaque targets lives in fission-pcode render
// (ADR 0011). midend-core::format_expr_key is a deterministic key/diagnostic printer only.
#[test]
fn diagnostic_print_expr_renders_callind_opaque_call_shape() {
    use fission_midend_prehir::util::format_expr_key;

    let expr = PreHirExpr::Call {
        origin: None,
        target: "__fission_callind_opaque".to_string(),
        args: vec![
            PreHirExpr::Var("fn_ptr".to_string()),
            PreHirExpr::Var("arg1".to_string()),
            PreHirExpr::Var("arg2".to_string()),
        ],
        ty: NirType::Unknown,
    };

    let rendered = format_expr_key(&expr);
    assert_eq!(rendered, "__fission_callind_opaque(fn_ptr, arg1, arg2)");
}
