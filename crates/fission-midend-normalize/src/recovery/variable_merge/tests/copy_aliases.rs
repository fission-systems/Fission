use super::super::apply_variable_merge_pass;
use super::super::join_copies::collect_dominant_copy_join_merges;
use crate::HashMap;
use crate::prelude::*;

/// `u = g; u = u & 0x51; return u == 1;` with `g` an unnamed global the
/// builder declared among the locals. Merging the copy renamed `u` onto
/// `g`, so the mask became a store to the global (`g &= 0x51`) that the
/// program never makes.
#[test]
fn a_copy_of_a_global_is_not_merged_onto_the_global() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let binding = |name: &str| PreHirBinding {
        name: name.to_string(),
        ty: u32_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    };
    let function = || PreHirFunction {
        name: "mask_of_a_global".to_string(),
        locals: vec![binding("tmp_14af44"), binding("uVar1")],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar1".to_string()),
                rhs: PreHirExpr::Var("tmp_14af44".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar1".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("uVar1".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0x51, u32_ty.clone())),
                    ty: u32_ty.clone(),
                },
            },
            PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Eq,
                lhs: Box::new(PreHirExpr::Var("uVar1".to_string())),
                rhs: Box::new(PreHirExpr::Const(1, u32_ty.clone())),
                ty: NirType::Bool,
            })),
        ],
        ..Default::default()
    };
    let writes_global =
        |func: &PreHirFunction| format!("{:?}", func.body).contains("lhs: Var(\"tmp_14af44\")");

    let context = crate::pipeline::NormalizeContext {
        global_symbols: Some(crate::pipeline::GlobalSymbolContext {
            storage_names: ["tmp_14af44".to_string()].into_iter().collect(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut func = function();
    {
        let _guard = crate::pipeline::NormalizeContextGuard::install(&context);
        apply_variable_merge_pass(&mut func);
    }
    assert!(!writes_global(&func), "{:?}", func.body);

    // Control: with nothing saying `tmp_14af44` is a global, the copy is
    // merged as between two locals, which is the rename above.
    let mut unmarked = function();
    apply_variable_merge_pass(&mut unmarked);
    assert!(writes_global(&unmarked), "{:?}", unmarked.body);
}

#[test]
fn copy_merge_preserves_load_address_cursor_temp() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let cursor_slot = PreHirBinding {
        name: "local_10".to_string(),
        ty: NirType::Ptr(Box::new(byte_ty.clone())),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-16)),
        initializer: None,
    };
    let address_tmp = PreHirBinding {
        name: "xVar18".to_string(),
        ty: NirType::Ptr(Box::new(byte_ty.clone())),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    };
    let byte_tmp = PreHirBinding {
        name: "xVar22".to_string(),
        ty: byte_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "checksum_shape".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![cursor_slot, address_tmp, byte_tmp],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xVar18".to_string()),
                rhs: PreHirExpr::Var("local_10".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xVar18".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("xVar18".to_string())),
                    rhs: Box::new(PreHirExpr::Var("param_10".to_string())),
                    ty: NirType::Ptr(Box::new(byte_ty.clone())),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xVar22".to_string()),
                rhs: PreHirExpr::Load {
                    ptr: Box::new(PreHirExpr::Var("xVar18".to_string())),
                    ty: byte_ty,
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xVar18".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    expr: Box::new(PreHirExpr::Var("xVar22".to_string())),
                },
            },
        ],
        ..Default::default()
    };

    apply_variable_merge_pass(&mut func);

    let rendered_vars = format!("{:?}", func.body);
    assert!(
        rendered_vars.contains("xVar18"),
        "load address temp must not be copy-merged into local_10"
    );
    assert!(
        rendered_vars.contains("local_10"),
        "cursor stack slot must remain distinct from the address temp"
    );
}

#[test]
fn copy_merge_preserves_distinct_stack_state_from_shared_register_seed() {
    let int_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let wide_int_ty = NirType::Int {
        bits: 64,
        signed: false,
    };
    let local_i = PreHirBinding {
        name: "local_4".to_string(),
        ty: int_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-4)),
        initializer: None,
    };
    let local_j = PreHirBinding {
        name: "local_8".to_string(),
        ty: wide_int_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-8)),
        initializer: None,
    };
    let rax = PreHirBinding {
        name: "rax".to_string(),
        ty: int_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: Some(PreHirExpr::Const(0, int_ty.clone())),
    };

    let mut func = PreHirFunction {
        name: "rc4_state_shape".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![local_i, local_j, rax],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_4".to_string()),
                rhs: PreHirExpr::Var("rax".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_8".to_string()),
                rhs: PreHirExpr::Var("rax".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_4".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("local_4".to_string())),
                    rhs: Box::new(PreHirExpr::Const(1, int_ty.clone())),
                    ty: int_ty.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_8".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("local_8".to_string())),
                    rhs: Box::new(PreHirExpr::Var("local_4".to_string())),
                    ty: wide_int_ty,
                },
            },
        ],
        ..Default::default()
    };

    apply_variable_merge_pass(&mut func);

    let rendered_vars = format!("{:?}", func.body);
    assert!(
        rendered_vars.contains("local_4"),
        "loop index stack state must remain distinct"
    );
    assert!(
        rendered_vars.contains("local_8"),
        "RC4 accumulator stack state must remain distinct"
    );
}

#[test]
fn copy_merge_preserves_distinct_temps_that_cooccur_after_seed_copy() {
    let int_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let primary = PreHirBinding {
        name: "uVar20".to_string(),
        ty: int_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    };
    let bias = PreHirBinding {
        name: "uVar21".to_string(),
        ty: int_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "copy_seed_then_subtract_shape".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![primary, bias],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar21".to_string()),
                rhs: PreHirExpr::Var("uVar20".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar20".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("uVar20".to_string())),
                    rhs: Box::new(PreHirExpr::Var("uVar21".to_string())),
                    ty: int_ty.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar20".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: Box::new(PreHirExpr::Var("uVar20".to_string())),
                    rhs: Box::new(PreHirExpr::Var("uVar21".to_string())),
                    ty: int_ty,
                },
            },
        ],
        ..Default::default()
    };

    apply_variable_merge_pass(&mut func);

    let rendered_vars = format!("{:?}", func.body);
    assert!(
        rendered_vars.contains("uVar20"),
        "primary temp must remain available after seed copy"
    );
    assert!(
        rendered_vars.contains("uVar21"),
        "seeded temp must not be rewritten into the primary temp when both co-occur"
    );
    assert_eq!(
        func.locals.len(),
        2,
        "copy aliases that co-occur later are distinct live values"
    );
}

#[test]
fn variable_merge_preserves_source_after_copy_is_mutated() {
    let int_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let binding = |name: &str| PreHirBinding {
        name: name.to_string(),
        ty: int_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    };
    let mut func = PreHirFunction {
        name: "copy_then_mutate_source".to_string(),
        params: vec![],
        locals: vec![
            binding("r12"),
            binding("uVar5"),
            binding("rdi"),
            binding("iVar36"),
        ],
        return_type: int_ty.clone(),
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar5".to_string()),
                rhs: PreHirExpr::Var("r12".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar5".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("uVar5".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0xffff_fffe, int_ty.clone())),
                    ty: int_ty.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("rdi".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: Box::new(PreHirExpr::Var("rdi".to_string())),
                    rhs: Box::new(PreHirExpr::Var("uVar5".to_string())),
                    ty: int_ty.clone(),
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("iVar36".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("r12".to_string())),
                    rhs: Box::new(PreHirExpr::Const(1, int_ty.clone())),
                    ty: int_ty.clone(),
                },
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("iVar36".to_string()))),
        ],
        ..Default::default()
    };

    apply_variable_merge_pass(&mut func);

    assert!(func.locals.iter().any(|binding| binding.name == "r12"));
    assert!(func.locals.iter().any(|binding| binding.name == "uVar5"));
    assert!(
        matches!(
            &func.body[1],
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(lhs),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: inner_lhs,
                    ..
                },
            } if lhs == "uVar5"
                && matches!(inner_lhs.as_ref(), PreHirExpr::Var(name) if name == "uVar5")
        ),
        "the copied value must keep its own masked definition: {func:?}"
    );
    assert!(
        func.body.iter().any(|stmt| matches!(
            stmt,
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: inner_lhs,
                    ..
                },
                ..
            } if matches!(inner_lhs.as_ref(), PreHirExpr::Var(name) if name == "r12")
        )),
        "the source value must remain available for the later parity read: {func:?}"
    );
}

#[test]
fn dominant_copy_join_merge_rejects_heterogeneous_types() {
    let int_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let ptr_ty = NirType::Ptr(Box::new(int_ty.clone()));
    let mut var_types = HashMap::default();
    var_types.insert("uVar1".to_string(), ptr_ty);
    var_types.insert("uVar2".to_string(), int_ty.clone());

    let stmts = vec![PreHirStmt::If {
        cond: PreHirExpr::Var("cond".to_string()),
        then_body: vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar1".to_string()),
            rhs: PreHirExpr::Var("src".to_string()),
        }]
        .into(),
        else_body: vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar2".to_string()),
            rhs: PreHirExpr::Var("src".to_string()),
        }]
        .into(),
    }];

    let merges = collect_dominant_copy_join_merges(&stmts, &var_types);
    assert!(
        merges.is_empty(),
        "heterogeneous types should not be merged: {merges:?}"
    );

    // Now test homogeneous types
    var_types.insert("uVar1".to_string(), int_ty);
    let merges = collect_dominant_copy_join_merges(&stmts, &var_types);
    assert_eq!(merges.len(), 1, "homogeneous types should be merged");
}
