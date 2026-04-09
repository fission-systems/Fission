//! Stable printer snapshots for regression (see `insta` / `cargo insta review`).

use super::*;
use indexmap::IndexMap;
use insta::assert_snapshot;

#[test]
fn snapshot_print_hir_function_minimal() {
    let func = HirFunction {
        name: "f_snapshot".to_string(),
        params: vec![],
        locals: vec![],
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )))],
        calling_convention: CallingConvention::default(),
        is_64bit: true,
        callee_observed_max_arity: IndexMap::new(),
    };
    assert_snapshot!(print_hir_function(&func));
}
