use super::super::apply_variable_merge_pass;
use crate::prelude::*;

#[test]
fn test_stack_slot_coalescing_and_domain_separation() {
    // Direct stack offset 1: offset -16, size 4
    let b1 = PreHirBinding {
        name: "local_10".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-16)),
        initializer: None,
    };

    // Direct stack offset 2: offset -14, size 4 (overlaps with -16)
    let b2 = PreHirBinding {
        name: "local_0e".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-14)),
        initializer: None,
    };

    // Derived stack offset: offset -8, size 4 (different offset)
    let b3 = PreHirBinding {
        name: "derived_08".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::DerivedFromStackOffset(-8)),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "test_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![b1, b2, b3],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_10".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_0e".to_string()),
                rhs: PreHirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("derived_08".to_string()),
                rhs: PreHirExpr::Const(
                    3,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
        ],
        ..Default::default()
    };

    let changed = apply_variable_merge_pass(&mut func);
    assert!(changed);

    // local_10 and local_0e should be merged (same domain, overlapping spans)
    // derived_08 should NOT be merged (different domain and different offset)
    let names: Vec<String> = func.locals.iter().map(|l| l.name.clone()).collect();
    assert!(names.contains(&"local_10".to_string()) || names.contains(&"local_0e".to_string()));
    assert!(names.contains(&"derived_08".to_string()));
    assert_eq!(func.locals.len(), 2);
}
