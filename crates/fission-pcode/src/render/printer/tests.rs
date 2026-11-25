#[test]
fn hir_print_keeps_the_widening_cast_that_sets_a_wide_multiply() {
    // `(unsigned __int128)(ulonglong)(n / 8) * k >> 64`: the inner cast
    // is width sugar over an unsigned 64-bit quotient, but the outer one
    // makes the multiply 128 bits wide. Eliding both printed
    // `n / 8 * k >> 64`, a 64-bit multiply shifted by its own width.
    use super::*;
    let int = |bits, signed| NirType::Int { bits, signed };
    let quotient = HirExpr::Binary {
        op: HirBinaryOp::Div,
        lhs: Box::new(HirExpr::Var("n".into())),
        rhs: Box::new(HirExpr::Const(8, int(64, false))),
        ty: int(64, false),
    };
    let product = HirExpr::Binary {
        op: HirBinaryOp::Mul,
        lhs: Box::new(HirExpr::Cast {
            ty: int(128, false),
            expr: Box::new(HirExpr::Cast {
                ty: int(64, false),
                expr: Box::new(quotient),
            }),
        }),
        rhs: Box::new(HirExpr::Const(0x20c4_9ba5_e353_f7cf, int(128, false))),
        ty: int(128, false),
    };
    let mut function = HirFunction {
        name: "wide_mul_high".into(),
        return_type: int(64, true),
        ..Default::default()
    };
    function.params.push(NirBinding {
        name: "n".into(),
        ty: int(64, false),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(0)),
        initializer: None,
    });
    function.body = vec![HirStmt::Return(Some(HirExpr::Cast {
        ty: int(64, true),
        expr: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs: Box::new(product),
            rhs: Box::new(HirExpr::Const(64, int(32, false))),
            ty: int(128, false),
        }),
    }))];

    let printed = print_hir_function_with_profile(&function, None, PrintProfile::Hir);

    assert!(
        printed.contains("(unsigned __int128)"),
        "the multiply must stay 128 bits wide:\n{printed}"
    );
}

#[test]
fn nested_unary_minus_is_not_printed_as_a_decrement() {
    use super::*;
    let int = NirType::Int {
        bits: 32,
        signed: true,
    };
    let x = HirExpr::Var("x".into());
    let neg = |e: HirExpr| HirExpr::Unary {
        op: HirUnaryOp::Neg,
        expr: Box::new(e),
        ty: int.clone(),
    };
    // Positive control: a single negation is unchanged.
    assert_eq!(print_expr_prec(&neg(x.clone()), 0, 0), "-x");
    // Double negation, and negating a negative literal.
    assert_eq!(print_expr_prec(&neg(neg(x.clone())), 0, 0), "-(-x)");
    assert_eq!(
        print_expr_prec(&neg(HirExpr::Const(-5, int.clone())), 0, 0),
        "-(-5)"
    );
    // `!!x` and `~~x` are not decrements and keep their short form.
    let not = HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr: Box::new(HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: Box::new(x),
            ty: int.clone(),
        }),
        ty: int,
    };
    assert_eq!(print_expr_prec(&not, 0, 0), "!!x");
}

#[test]
fn definition_lines_preserve_text_and_do_not_map_rhs_uses() {
    use super::*;
    let ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut function = HirFunction::default();
    function.locals.push(NirBinding {
        name: "value".into(),
        ty: ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    });
    function.body = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("value".into()),
            rhs: HirExpr::Const(3, ty),
        },
        HirStmt::Return(Some(HirExpr::Var("value".into()))),
    ];
    let definitions = HashMap::from([("value".to_string(), 0x1000)]);
    let (code, mappings) =
        print_hir_function_with_provenance(&function, None, PrintProfile::Nir, &definitions);
    assert_eq!(
        code,
        print_hir_function_with_profile(&function, None, PrintProfile::Nir)
    );
    assert_eq!(mappings.len(), 1);
    assert!(
        code.lines()
            .nth(mappings[0].line_number - 1)
            .unwrap()
            .contains("value = 3;")
    );
    assert_eq!(mappings[0].defined_variables, vec!["value"]);
    assert_eq!(mappings[0].addresses, vec![0x1000]);
    function.body.insert(0, function.body[0].clone());
    assert!(
        print_hir_function_with_provenance(&function, None, PrintProfile::Nir, &definitions)
            .1
            .is_empty()
    );
}

#[test]
fn initialized_definition_is_mapped_but_later_redefinition_abstains() {
    use super::*;
    let ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut function = HirFunction::default();
    function.locals.push(NirBinding {
        name: "value".into(),
        ty: ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: Some(HirExpr::Const(3, ty.clone())),
    });
    let definitions = HashMap::from([("value".to_string(), 0x1000)]);
    let (code, mappings) =
        print_hir_function_with_provenance(&function, None, PrintProfile::Nir, &definitions);
    assert_eq!(mappings.len(), 1);
    assert!(
        code.lines()
            .nth(mappings[0].line_number - 1)
            .unwrap()
            .contains("value = 3;")
    );
    function.body.push(HirStmt::Assign {
        lhs: HirLValue::Var("value".into()),
        rhs: HirExpr::Const(4, ty),
    });
    assert!(
        print_hir_function_with_provenance(&function, None, PrintProfile::Nir, &definitions)
            .1
            .is_empty()
    );
}
use super::*;
use crate::midend::StructField;

fn u8_ptr() -> NirType {
    NirType::Ptr(Box::new(NirType::Int {
        bits: 8,
        signed: false,
    }))
}

fn u64_ty() -> NirType {
    NirType::Int {
        bits: 64,
        signed: false,
    }
}

#[test]
fn known_variadic_runtime_definitions_print_fixed_prefix_and_ellipsis() {
    let file_ptr = NirType::Ptr(Box::new(NirType::Unknown));
    let char_ptr = NirType::Ptr(Box::new(NirType::Int {
        bits: 8,
        signed: true,
    }));
    let hir = HirFunction {
        name: "fprintf".to_string(),
        variadic_fixed_arity: Some(2),
        params: vec![
            NirBinding {
                name: "__stream".to_string(),
                ty: file_ptr.clone(),
                surface_type_name: Some("FILE*".to_string()),
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            NirBinding {
                name: "__format".to_string(),
                ty: char_ptr.clone(),
                surface_type_name: Some("const char*".to_string()),
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);
    assert!(
        rendered.starts_with("int fprintf(FILE* __stream, const char* __format, ...)\n"),
        "{rendered}"
    );

    let ordinary = HirFunction {
        name: "ordinary".to_string(),
        params: hir.params.clone(),
        return_type: hir.return_type.clone(),
        ..HirFunction::default()
    };
    let ordinary_rendered = print_hir_function(&ordinary);
    assert!(
        ordinary_rendered.starts_with("int ordinary(FILE* __stream, const char* __format)\n"),
        "{ordinary_rendered}"
    );
}

#[test]
fn explicit_defined_variadic_prototype_uses_fixed_prefix_metadata() {
    let hir = HirFunction {
        name: "defined_helper".to_string(),
        variadic_fixed_arity: Some(1),
        params: vec![NirBinding {
            name: "format".to_string(),
            ty: u8_ptr(),
            surface_type_name: Some("const char*".to_string()),
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);
    assert!(
        rendered.starts_with("unsigned long long defined_helper(const char* format, ...)\n"),
        "{rendered}"
    );
}

#[test]
fn import_thunk_declares_unknown_arity_in_both_dialects() {
    let hir = HirFunction {
        name: "import_thunk".to_string(),
        return_type: u64_ty(),
        has_unknown_arity_prototype: true,
        body: vec![HirStmt::Expr(HirExpr::Call {
            origin: None,
            target: "__fission_branchind".to_string(),
            args: vec![HirExpr::Var("callee".to_string())],
            ty: NirType::Unknown,
        })],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("extern unsigned long long import_thunk();\n"),
        "{rendered}"
    );
    assert!(!rendered.contains("import_thunk()\n{"));
    assert!(rendered.contains("extern unsigned long long import_thunk(...);"));
}

#[test]
fn zero_fixed_variadic_definition_uses_unknown_arity_declaration() {
    let hir = HirFunction {
        name: "opaque_variadic".to_string(),
        return_type: u64_ty(),
        variadic_fixed_arity: Some(0),
        body: vec![HirStmt::Return(Some(HirExpr::Const(0, u64_ty())))],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert_eq!(
        rendered,
        print_unknown_arity_extern("unsigned long long", "opaque_variadic")
    );
}

#[test]
fn opaque_indirect_call_uses_c11_nonprototype_function_pointer() {
    let expr = HirExpr::Call {
        origin: None,
        target: "__fission_callind_opaque".to_string(),
        args: vec![
            HirExpr::Var("callee".to_string()),
            HirExpr::Const(7, u64_ty()),
        ],
        ty: u64_ty(),
    };
    let expected = "((unsigned long long (*)())(callee))(7)";

    assert_eq!(print_expr_prec(&expr, 0, 0), expected);

    let hir = HirFunction {
        body: vec![HirStmt::Expr(expr.clone())],
        ..HirFunction::default()
    };
    let ctx = PrintCtx::build(&hir);
    assert_eq!(print_expr_prec_ctx(&expr, 0, 0, &ctx), expected);
}

#[test]
fn standalone_c_function_definition_and_self_call_map_raw_symbols_once() {
    let symbol = sanitize_c_identifier("helper.part.0");
    let hir = HirFunction {
        name: "helper.part.0".to_string(),
        body: vec![HirStmt::Expr(HirExpr::Call {
            origin: None,
            target: "helper.part.0".to_string(),
            args: Vec::new(),
            ty: NirType::Unknown,
        })],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains(&format!("unsigned long long {symbol}(void)")),
        "{rendered}"
    );
    assert!(rendered.contains(&format!("{symbol}();")), "{rendered}");
    assert!(!rendered.contains("helper.part.0"), "{rendered}");
}

#[test]
fn pipeline_printer_preserves_symbols_mapped_when_hir_was_built() {
    let symbol = sanitize_c_identifier("helper.part.0");
    let hir = HirFunction {
        name: symbol.clone(),
        body: vec![HirStmt::Expr(HirExpr::Call {
            origin: None,
            target: symbol.clone(),
            args: Vec::new(),
            ty: NirType::Unknown,
        })],
        ..HirFunction::default()
    };

    let rendered = print_hir_function_with_profile(&hir, None, PrintProfile::Nir);

    assert!(
        rendered.contains(&format!("unsigned long long {symbol}(void)")),
        "{rendered}"
    );
    assert!(rendered.contains(&format!("{symbol}();")), "{rendered}");
    assert!(!rendered.contains("fission_symbol_fission"), "{rendered}");
}

#[test]
fn array_surface_declaration_and_overlapping_store_are_c_valid() {
    let byte = NirType::Int {
        bits: 8,
        signed: false,
    };
    let hir = HirFunction {
        name: "array_store".to_string(),
        locals: vec![NirBinding {
            name: "data".to_string(),
            ty: NirType::Aggregate {
                size: 5,
                fields: (0..5)
                    .map(|index| StructField {
                        offset: index,
                        ty: byte.clone(),
                        name: format!("element_{index}"),
                    })
                    .collect(),
            },
            surface_type_name: Some("unsigned char[5]".to_string()),
            origin: Some(NirBindingOrigin::StackOffset(-5)),
            initializer: None,
        }],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::AddressOfLocal("data".to_string())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
                rhs: HirExpr::Const(
                    0x0403_0201,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("data".to_string())),
                    index: Box::new(HirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 64,
                            signed: true,
                        },
                    )),
                    elem_ty: byte,
                },
                rhs: HirExpr::Const(
                    5,
                    NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                ),
            },
        ],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(rendered.contains("unsigned char data[5];"), "{rendered}");
    assert!(
        rendered.contains("*(uint *)(&data) = 67305985;"),
        "{rendered}"
    );
    assert!(rendered.contains("data[4] = 5;"), "{rendered}");
    assert!(!rendered.contains("unsigned char[5] data;"), "{rendered}");
}

#[test]
fn x87_extended_float_uses_a_valid_c_surface_type() {
    let extended = NirType::Float { bits: 80 };
    let hir = HirFunction {
        name: "x87_extended_value".to_string(),
        locals: vec![NirBinding {
            name: "st0".to_string(),
            ty: extended.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("st0".to_string()),
            rhs: HirExpr::Cast {
                ty: extended.clone(),
                expr: Box::new(HirExpr::Const(1, extended)),
            },
        }],
        ..HirFunction::default()
    };

    assert_eq!(print_type(&NirType::Float { bits: 80 }), "long double");
    let rendered = print_hir_function(&hir);
    assert!(rendered.contains("long double st0;"), "{rendered}");
    assert!(rendered.contains("(long double)1"), "{rendered}");
    assert!(!rendered.contains("float80"), "{rendered}");
}

#[test]
fn wide_128_bit_integer_uses_a_compilable_gnu_c_type() {
    let wide = NirType::Int {
        bits: 128,
        signed: false,
    };
    let hir = HirFunction {
        name: "wide_scalar".to_string(),
        locals: vec![NirBinding {
            name: "wide_value".to_string(),
            ty: wide.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: wide,
        body: vec![HirStmt::Return(Some(HirExpr::Var(
            "wide_value".to_string(),
        )))],
        ..HirFunction::default()
    };

    assert_eq!(
        print_type(&NirType::Int {
            bits: 128,
            signed: true,
        }),
        "__int128"
    );
    let rendered = print_hir_function(&hir);
    assert!(
        rendered.contains("unsigned __int128 wide_scalar(void)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("unsigned __int128 wide_value;"),
        "{rendered}"
    );
    assert!(!rendered.contains("\n    int128 wide_scalar"), "{rendered}");
}

#[test]
fn pointer_scalar_compound_assignment_renders_c_safe_cast_assignment() {
    let hir = HirFunction {
        name: "scale_ptr_like_value".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![NirBinding {
            name: "local_d".to_string(),
            ty: u8_ptr(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("local_d".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: Box::new(HirExpr::Var("local_d".to_string())),
                rhs: Box::new(HirExpr::Const(2, u64_ty())),
                ty: u64_ty(),
            },
        }],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("local_d = (uchar *)((unsigned long long)local_d * 2);"),
        "{rendered}"
    );
    assert!(!rendered.contains("local_d *= 2;"), "{rendered}");
}

#[test]
fn rotate_style_word_width_shift_is_guarded_at_zero_count() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let expression = HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs: Box::new(HirExpr::Var("value".into())),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(HirExpr::Const(32, u32_ty.clone())),
            rhs: Box::new(HirExpr::Var("count".into())),
            ty: u32_ty.clone(),
        }),
        ty: u32_ty.clone(),
    };

    let standalone = print_expr(&expression);
    assert_eq!(standalone, "count == 0 ? 0 : value >> 32 - count");

    let rendered = print_hir_function(&HirFunction {
        name: "rotate_part".into(),
        locals: vec![
            NirBinding {
                name: "value".into(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            NirBinding {
                name: "count".into(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
        ],
        return_type: u32_ty,
        body: vec![HirStmt::Return(Some(expression))],
        ..HirFunction::default()
    });
    assert!(
        rendered.contains("return count == 0 ? 0 : value >> 32 - count;"),
        "{rendered}"
    );
}

#[test]
fn pointer_assignment_from_scalar_rhs_renders_explicit_cast() {
    let hir = HirFunction {
        name: "load_byte_into_pointer_typed_slot".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![NirBinding {
            name: "local_10".to_string(),
            ty: u8_ptr(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("local_10".to_string()),
            rhs: HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("local_10".to_string())),
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                }),
            },
        }],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("local_10 = (uchar *)((uint)*local_10);"),
        "{rendered}"
    );
}

#[test]
fn surface_pointer_keeps_observed_wide_load_and_store_width() {
    let hir = HirFunction {
        name: "packed_pair_cursor".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![
            NirBinding {
                name: "p".to_string(),
                ty: NirType::Ptr(Box::new(u64_ty())),
                surface_type_name: Some("int *".to_string()),
                origin: Some(NirBindingOrigin::TempPreserved),
                initializer: None,
            },
            NirBinding {
                name: "wide".to_string(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::TempPreserved),
                initializer: None,
            },
            NirBinding {
                name: "q".to_string(),
                ty: NirType::Ptr(Box::new(u64_ty())),
                surface_type_name: Some("int *".to_string()),
                origin: Some(NirBindingOrigin::TempPreserved),
                initializer: None,
            },
        ],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("wide".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("p".to_string())),
                    ty: u64_ty(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("p".to_string())),
                    ty: u64_ty(),
                },
                rhs: HirExpr::Const(7, u64_ty()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("q".to_string()),
                rhs: HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(u64_ty())),
                    expr: Box::new(HirExpr::Var("p".to_string())),
                },
            },
        ],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("wide = *(unsigned long long *)(p);"),
        "{rendered}"
    );
    assert!(
        rendered.contains("*(unsigned long long *)(p) = 7;"),
        "{rendered}"
    );
    assert!(
        rendered.contains("q = (int *)((unsigned long long *)p);"),
        "{rendered}"
    );
}

#[test]
fn byte_pointer_subscripts_preserve_wide_access_width() {
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let hir = HirFunction {
        name: "read_and_write_pe_header_word".to_string(),
        params: vec![NirBinding {
            name: "pImageBase".to_string(),
            ty: NirType::Ptr(Box::new(u32_ty.clone())),
            surface_type_name: Some("unsigned char*".to_string()),
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: vec![NirBinding {
            name: "loaded".to_string(),
            ty: u32_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: u32_ty.clone(),
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Index {
                    base: Box::new(HirExpr::Var("pImageBase".to_string())),
                    index: Box::new(HirExpr::Const(15, u64_ty())),
                    elem_ty: u32_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("pImageBase".to_string())),
                    index: Box::new(HirExpr::Const(15, u64_ty())),
                    elem_ty: u32_ty,
                },
                rhs: HirExpr::Const(
                    7,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("loaded".to_string()))),
        ],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("loaded = ((uint *)(pImageBase))[15];"),
        "{rendered}"
    );
    assert!(
        rendered.contains("((uint *)(pImageBase))[15] = 7;"),
        "{rendered}"
    );
}

#[test]
fn integer_assignment_from_pointer_arithmetic_renders_explicit_cast() {
    let hir = HirFunction {
        name: "pointer_delta_into_integer".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_10".to_string(),
            ty: u8_ptr(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: vec![NirBinding {
            name: "iVar10".to_string(),
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("iVar10".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(HirExpr::Var("param_10".to_string())),
                rhs: Box::new(HirExpr::Const(255, u64_ty())),
                ty: u8_ptr(),
            },
        }],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("iVar10 = (long long)(unsigned long long)(param_10 - 255);"),
        "{rendered}"
    );
}

#[test]
fn integer_return_from_pointer_expr_renders_explicit_cast() {
    let hir = HirFunction {
        name: "return_pointer_as_int".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![NirBinding {
            name: "local_d".to_string(),
            ty: u8_ptr(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        body: vec![HirStmt::Return(Some(HirExpr::Var("local_d".to_string())))],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("return (int)(unsigned long long)(local_d);"),
        "{rendered}"
    );
}

#[test]
fn unresolved_pointer_constant_renders_as_hex_not_decimal() {
    // An address that failed to resolve to a named global (empty
    // global_names) must still read as an address to an agent parsing
    // this output, not as an ordinary decimal integer indistinguishable
    // from a loop bound or a size. 0x140001040 was picked because its
    // decimal form (5368713280) gives no visual hint it is hex at all.
    let hir = HirFunction {
        name: "return_unresolved_string_pointer".to_string(),
        int_param_offsets: Vec::new(),
        return_type: u8_ptr(),
        body: vec![HirStmt::Return(Some(HirExpr::Const(0x140001040, u8_ptr())))],
        ..HirFunction::default()
    };

    let rendered = print_hir_function_with_global_names(&hir, &std::collections::HashMap::new());

    assert!(rendered.contains("0x140001040"), "{rendered}");
    assert!(
        rendered.contains("(uchar *)0x140001040"),
        "pointer-typed integer constants need a C pointer cast: {rendered}"
    );
    assert!(!rendered.contains("5368713280"), "{rendered}");
}

#[test]
fn unresolved_pointer_constant_renders_as_hex_in_context_free_path() {
    // Same defect, the other call site: print_hir_function (no
    // global_names at all) goes through the context-free HirExpr::Const
    // arm at line ~856, a separate match from the ctx-aware one above.
    let hir = HirFunction {
        name: "return_pointer_constant_no_ctx".to_string(),
        int_param_offsets: Vec::new(),
        return_type: u8_ptr(),
        body: vec![HirStmt::Return(Some(HirExpr::Const(0x140001040, u8_ptr())))],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(rendered.contains("0x140001040"), "{rendered}");
    assert!(
        rendered.contains("(uchar *)0x140001040"),
        "pointer-typed integer constants need a C pointer cast: {rendered}"
    );
    assert!(!rendered.contains("5368713280"), "{rendered}");
}

#[test]
fn pointer_operand_in_integer_bitop_renders_explicit_cast() {
    let hir = HirFunction {
        name: "xor_pointer_like_value".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![NirBinding {
            name: "local_1c".to_string(),
            ty: u8_ptr(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("local_1c".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Xor,
                lhs: Box::new(HirExpr::Cast {
                    ty: u64_ty(),
                    expr: Box::new(HirExpr::Var("local_1c".to_string())),
                }),
                rhs: Box::new(HirExpr::Var("local_1c".to_string())),
                ty: u64_ty(),
            },
        }],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains(
            "local_1c = (uchar *)((unsigned long long)(unsigned long long)local_1c ^ (unsigned long long)local_1c);"
        ),
        "{rendered}"
    );
}

#[test]
fn pointer_casted_integer_bitop_renders_pointer_operands_as_scalars() {
    let hir = HirFunction {
        name: "xor_pointer_like_value_inside_cast".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![NirBinding {
            name: "local_10".to_string(),
            ty: u8_ptr(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("local_10".to_string()),
            rhs: HirExpr::Cast {
                ty: u8_ptr(),
                expr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Xor,
                    lhs: Box::new(HirExpr::Cast {
                        ty: u64_ty(),
                        expr: Box::new(HirExpr::Var("local_10".to_string())),
                    }),
                    rhs: Box::new(HirExpr::Var("local_10".to_string())),
                    ty: u64_ty(),
                }),
            },
        }],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains(
            "local_10 = (uchar *)((unsigned long long)(unsigned long long)local_10 ^ (unsigned long long)local_10);"
        ),
        "{rendered}"
    );
}

#[test]
fn pointer_operand_in_unary_integer_op_renders_explicit_scalar_cast() {
    let hir = HirFunction {
        name: "neg_pointer_like_value".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![NirBinding {
            name: "local_10".to_string(),
            ty: u8_ptr(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Var("local_10".to_string()),
            rhs: HirExpr::Cast {
                ty: u8_ptr(),
                expr: Box::new(HirExpr::Unary {
                    op: HirUnaryOp::Neg,
                    expr: Box::new(HirExpr::Var("local_10".to_string())),
                    ty: u64_ty(),
                }),
            },
        }],
        ..HirFunction::default()
    };

    let rendered = print_hir_function(&hir);

    assert!(
        rendered.contains("local_10 = (uchar *)-(unsigned long long)local_10;"),
        "{rendered}"
    );
}

#[test]
fn width_minus_shift_count_zero_guard_parenthesizes_low_precedence_count() {
    let count = HirExpr::Binary {
        op: HirBinaryOp::And,
        lhs: Box::new(HirExpr::Var("x".to_string())),
        rhs: Box::new(HirExpr::Const(31, u64_ty())),
        ty: u64_ty(),
    };
    let shift_expr = HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs: Box::new(HirExpr::Var("val".to_string())),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(HirExpr::Const(32, u64_ty())),
            rhs: Box::new(count),
            ty: u64_ty(),
        }),
        ty: u64_ty(),
    };
    let printed_no_ctx = print_expr_prec(&shift_expr, 0, 0);
    assert!(
        printed_no_ctx.starts_with("(x & 31) == 0 ? 0 :"),
        "expected parenthesized zero-shift guard: {printed_no_ctx}"
    );
    let dummy = HirFunction::default();
    let ctx = PrintCtx::build(&dummy);
    let printed_with_ctx = print_expr_prec_ctx(&shift_expr, 0, 0, &ctx);
    assert!(
        printed_with_ctx.starts_with("(x & 31) == 0 ? 0 :"),
        "expected parenthesized zero-shift guard with ctx: {printed_with_ctx}"
    );
}

#[test]
fn bitop_with_pointer_var_does_not_corrupt_member_or_prefixed_identifiers() {
    let field_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let hir = HirFunction {
        name: "bitop_ptr_names".to_string(),
        locals: vec![
            NirBinding {
                name: "p".to_string(),
                ty: u8_ptr(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            NirBinding {
                name: "p_low".to_string(),
                ty: field_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            NirBinding {
                name: "flags".to_string(),
                ty: field_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
        ],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("flags".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(HirExpr::Var("flags".to_string())),
                    rhs: Box::new(HirExpr::FieldAccess {
                        base: Box::new(HirExpr::Var("p".to_string())),
                        offset: 4,
                        field_name: "mask".to_string(),
                        ty: field_ty.clone(),
                    }),
                    ty: field_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("flags".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(HirExpr::Var("flags".to_string())),
                    rhs: Box::new(HirExpr::Var("p_low".to_string())),
                    ty: field_ty,
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("p".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(HirExpr::Var("p".to_string())),
                    rhs: Box::new(HirExpr::Var("flags".to_string())),
                    ty: u8_ptr(),
                },
            },
        ],
        ..HirFunction::default()
    };
    let rendered = print_hir_function(&hir);
    assert!(
        rendered.contains("flags & p->mask") || rendered.contains("flags &= p->mask"),
        "must not cast pointer field base as scalar:\n{rendered}"
    );
    assert!(
        !rendered.contains("(unsigned long long)p->mask"),
        "{rendered}"
    );
    assert!(
        !rendered.contains("(unsigned long long)p_low"),
        "{rendered}"
    );
    assert!(
        rendered.contains("(unsigned long long)p & flags"),
        "compound pointer bitop must cast p at AST print time:\n{rendered}"
    );
}
