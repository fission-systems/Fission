use super::*;
#[test]
fn preview_type_hints_surface_known_pointer_alias_on_param() {
    let mut func = HirFunction {
        name: "FUN_0x140006260".to_string(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16, fields: vec![] })),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Expr(HirExpr::Call {
            target: "GetClientRect".to_string(),
            args: vec![
                HirExpr::Var("param_1".to_string()),
                HirExpr::Var("param_2".to_string()),
            ],
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        })],
    };

    let mut context = PreviewTypeContext::default();
    context.call_param_rules.push(PreviewCallParamRule {
        callee_name: "GetClientRect".to_string(),
        arg_index: 1,
        pointer_alias: "LPRECT".to_string(),
        pointee_alias: "RECT".to_string(),
        pointer_size: 8,
        pointee_sizes: vec![16],
    });

    apply_preview_type_hints(&mut func, &context);
    assert_eq!(func.params[1].surface_type_name.as_deref(), Some("LPRECT"));
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("undefined FUN_0x140006260(longlong param_1, LPRECT param_2)"));
}

#[test]
fn preview_type_hints_surface_known_pointer_alias_through_wrapper_cast() {
    let mut func = HirFunction {
        name: "FUN_0x140006260".to_string(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16, fields: vec![] })),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Expr(HirExpr::Call {
            target: "GetClientRect".to_string(),
            args: vec![
                HirExpr::Var("param_1".to_string()),
                HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16, fields: vec![] })),
                    expr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_2".to_string())),
                        offset: 0,
                    }),
                },
            ],
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        })],
    };

    let mut context = PreviewTypeContext::default();
    context.call_param_rules.push(PreviewCallParamRule {
        callee_name: "GetClientRect".to_string(),
        arg_index: 1,
        pointer_alias: "LPRECT".to_string(),
        pointee_alias: "RECT".to_string(),
        pointer_size: 8,
        pointee_sizes: vec![16],
    });

    apply_preview_type_hints(&mut func, &context);
    assert_eq!(func.params[1].surface_type_name.as_deref(), Some("LPRECT"));
}
