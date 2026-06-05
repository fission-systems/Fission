use super::*;
use crate::nir::normalize::idioms::apply_string_copy_pass;

#[test]
fn test_string_copy_collapses_contiguous_index_stores() {
    let char_ty = NirType::Int { bits: 8, signed: true };
    let mut func = HirFunction {
        name: "test_string_copy".to_string(),
            int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "buf".to_string(),
                ty: NirType::Ptr(Box::new(char_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            // buf[0] = 'h';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(0, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'h' as i64, char_ty.clone()),
            },
            // buf[1] = 'e';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(1, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'e' as i64, char_ty.clone()),
            },
            // buf[2] = 'l';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(2, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'l' as i64, char_ty.clone()),
            },
            // buf[3] = 'l';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(3, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'l' as i64, char_ty.clone()),
            },
            // buf[4] = 'o';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(4, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'o' as i64, char_ty.clone()),
            },
        ],
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_string_copy_pass(&mut func));
    assert_eq!(func.body.len(), 1);

    // Verify it is a memcpy call
    if let HirStmt::Expr(HirExpr::Call { target, args, .. }) = &func.body[0] {
        assert_eq!(target, "memcpy");
        assert_eq!(args.len(), 3);
        // dest_ptr: buf
        assert_eq!(args[0], HirExpr::Var("buf".to_string()));
        // src_ptr: &*"hello"
        assert_eq!(args[1], HirExpr::AddressOfGlobal("*\"hello\"".to_string()));
        // count: 5
        assert_eq!(args[2], HirExpr::Const(5, NirType::Int { bits: 64, signed: false }));
    } else {
        panic!("Expected memcpy call, found {:?}", func.body[0]);
    }
}

#[test]
fn test_string_copy_collapses_contiguous_deref_offset_stores() {
    let char_ty = NirType::Int { bits: 8, signed: true };
    let mut func = HirFunction {
        name: "test_string_copy".to_string(),
            int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "buf".to_string(),
                ty: NirType::Ptr(Box::new(char_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            // *(buf + 2) = 't';
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("buf".to_string())),
                        offset: 2,
                    }),
                    ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b't' as i64, char_ty.clone()),
            },
            // *(buf + 3) = 'e';
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("buf".to_string())),
                        offset: 3,
                    }),
                    ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'e' as i64, char_ty.clone()),
            },
            // *(buf + 4) = 's';
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("buf".to_string())),
                        offset: 4,
                    }),
                    ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b's' as i64, char_ty.clone()),
            },
            // *(buf + 5) = 't';
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("buf".to_string())),
                        offset: 5,
                    }),
                    ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b't' as i64, char_ty.clone()),
            },
        ],
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_string_copy_pass(&mut func));
    assert_eq!(func.body.len(), 1);

    if let HirStmt::Expr(HirExpr::Call { target, args, .. }) = &func.body[0] {
        assert_eq!(target, "memcpy");
        assert_eq!(args.len(), 3);
        // dest_ptr: buf + 2
        assert_eq!(args[0], HirExpr::PtrOffset {
            base: Box::new(HirExpr::Var("buf".to_string())),
            offset: 2,
        });
        // src_ptr: &*"test"
        assert_eq!(args[1], HirExpr::AddressOfGlobal("*\"test\"".to_string()));
        // count: 4
        assert_eq!(args[2], HirExpr::Const(4, NirType::Int { bits: 64, signed: false }));
    } else {
        panic!("Expected memcpy call, found {:?}", func.body[0]);
    }
}

#[test]
fn test_string_copy_rejects_interfering_write_to_base() {
    let char_ty = NirType::Int { bits: 8, signed: true };
    let mut func = HirFunction {
        name: "test_string_copy".to_string(),
            int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "buf".to_string(),
                ty: NirType::Ptr(Box::new(char_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(0, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'h' as i64, char_ty.clone()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(1, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'e' as i64, char_ty.clone()),
            },
            // buf = something; // Modifies base pointer!
            HirStmt::Assign {
                lhs: HirLValue::Var("buf".to_string()),
                rhs: HirExpr::Const(0x1000, NirType::Int { bits: 64, signed: false }),
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(2, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'l' as i64, char_ty.clone()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(3, NirType::Int { bits: 64, signed: true })),
                    elem_ty: char_ty.clone(),
                },
                rhs: HirExpr::Const(b'l' as i64, char_ty.clone()),
            },
        ],
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    // Should NOT apply because the sequence is split by an interfering assignment, making both sub-sequences < 4.
    assert!(!apply_string_copy_pass(&mut func));
}

#[test]
fn test_string_copy_collapses_wide_character_stores() {
    let wchar_ty = NirType::Int { bits: 16, signed: false };
    let mut func = HirFunction {
        name: "test_string_copy".to_string(),
            int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "buf".to_string(),
                ty: NirType::Ptr(Box::new(wchar_ty.clone())),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            // buf[0] = L'h';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(0, NirType::Int { bits: 64, signed: true })),
                    elem_ty: wchar_ty.clone(),
                },
                rhs: HirExpr::Const(b'h' as i64, wchar_ty.clone()),
            },
            // buf[1] = L'e';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(1, NirType::Int { bits: 64, signed: true })),
                    elem_ty: wchar_ty.clone(),
                },
                rhs: HirExpr::Const(b'e' as i64, wchar_ty.clone()),
            },
            // buf[2] = L'y';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(2, NirType::Int { bits: 64, signed: true })),
                    elem_ty: wchar_ty.clone(),
                },
                rhs: HirExpr::Const(b'y' as i64, wchar_ty.clone()),
            },
            // buf[3] = L'\0';
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("buf".to_string())),
                    index: Box::new(HirExpr::Const(3, NirType::Int { bits: 64, signed: true })),
                    elem_ty: wchar_ty.clone(),
                },
                rhs: HirExpr::Const(0, wchar_ty.clone()),
            },
        ],
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_string_copy_pass(&mut func));
    assert_eq!(func.body.len(), 1);

    if let HirStmt::Expr(HirExpr::Call { target, args, .. }) = &func.body[0] {
        assert_eq!(target, "memcpy");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], HirExpr::Var("buf".to_string()));
        // src_ptr: &*"h\0e\0y\0\0\0"
        assert_eq!(args[1], HirExpr::AddressOfGlobal("*\"h\\0e\\0y\\0\\0\\0\"".to_string()));
        // count: 8 bytes
        assert_eq!(args[2], HirExpr::Const(8, NirType::Int { bits: 64, signed: false }));
    } else {
        panic!("Expected memcpy call, found {:?}", func.body[0]);
    }
}
