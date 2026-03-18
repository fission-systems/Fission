use super::*;
use std::collections::HashMap;

#[test]
fn preview_type_hints_rename_params_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Return(Some(HirExpr::Var("param_2".to_string())))],
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::new(),
        call_param_rules: Vec::new(),
        function_hints: Some(PreviewFunctionHints {
            param_names: vec!["hwnd".to_string(), "lpRect".to_string()],
            stack_local_names: HashMap::new(),
            return_type_name: Some("BOOL".to_string()),
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(func.params[0].name, "hwnd");
    assert_eq!(func.params[1].name, "lpRect");
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("hwnd"));
    assert!(rendered.contains("lpRect"));
    assert!(!rendered.contains("param_2"));
}

#[test]
fn preview_type_hints_rename_stack_locals_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        params: vec![],
        locals: vec![NirBinding {
            name: "local_20".to_string(),
            ty: NirType::Aggregate { size: 16 },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x20)),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("local_20".to_string()),
                rhs: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("local_20".to_string()))),
        ],
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::new(),
        call_param_rules: Vec::new(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            stack_local_names: HashMap::from([(-0x20, "rect".to_string())]),
            return_type_name: None,
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(func.locals[0].name, "rect");
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("rect"));
    assert!(!rendered.contains("local_20"));
}
