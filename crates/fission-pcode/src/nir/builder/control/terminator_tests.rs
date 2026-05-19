use crate::nir::support::{CallingConvention, RUST_SLEIGH_REGISTER_SPACE_ID};
use crate::nir::types::{
    HirBinaryOp, HirExpr, MlilPreviewOptions, NirType, StructuringEngineKind,
};
use crate::nir::{PreviewBuilder, render_mlil_preview};
use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

use super::{
    InferredJumpTableTargets, branchind_decode_modes, extract_selector_upper_bound_from_cond,
    merge_inferred_branchind_targets,
};

fn test_binary(op: HirBinaryOp, lhs: HirExpr, rhs: HirExpr, ty: NirType) -> HirExpr {
    HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty,
    }
}

#[test]
fn branchind_decode_modes_include_image_base_relative_for_absolute_tables() {
    let modes = branchind_decode_modes(
        false,
        0x1400_5000,
        None,
        0x1400_0000,
        &[(0x1400_0000, 0x1401_0000)],
    );
    assert!(modes.contains(&("absolute", false, None)));
    assert!(modes.contains(&("relative_table_base", true, Some(0x1400_5000))));
    assert!(modes.contains(&("section_base_relative", true, Some(0x1400_0000))));
    assert!(modes.contains(&("image_base_relative", true, Some(0x1400_0000))));
}

#[test]
fn branchind_decode_modes_keep_relative_tables_target_based() {
    let modes = branchind_decode_modes(
        true,
        0x1400_5000,
        Some(0x1400_7000),
        0x1400_0000,
        &[(0x1400_0000, 0x1401_0000)],
    );
    assert_eq!(
        modes,
        vec![("relative_target_base", true, Some(0x1400_7000))]
    );
}

#[test]
fn merge_inferred_branchind_targets_preserves_case_map_with_successors() {
    let mut targets = vec![0x2000];
    let mut recovered_case_map = None;
    let mut recovered_selector_cardinality = None;
    merge_inferred_branchind_targets(
        &mut targets,
        InferredJumpTableTargets {
            unique_targets: vec![0x2000, 0x3000, 0x4000],
            recovered_cases: vec![(0, 0x2000), (1, 0x3000), (2, 0x4000), (3, 0x3000)],
            selector_cardinality: 4,
            decode_mode: "absolute",
        },
        &mut recovered_case_map,
        &mut recovered_selector_cardinality,
    );

    assert_eq!(targets, vec![0x2000, 0x3000, 0x4000]);
    assert_eq!(recovered_selector_cardinality, Some(4));
    assert_eq!(
        recovered_case_map,
        Some(vec![(0, 0x2000), (1, 0x3000), (2, 0x4000), (3, 0x3000)])
    );
}

#[test]
fn selector_upper_bound_keeps_false_arm_hi_equality_case() {
    let selector = HirExpr::Var("sel".to_string());
    let three = HirExpr::Const(3, NirType::Unknown);
    let zero = HirExpr::Const(0, NirType::Unknown);
    let cond = test_binary(
        HirBinaryOp::LogicalAnd,
        test_binary(
            HirBinaryOp::Le,
            three.clone(),
            selector.clone(),
            NirType::Bool,
        ),
        test_binary(
            HirBinaryOp::Ne,
            test_binary(HirBinaryOp::Sub, selector.clone(), three, NirType::Unknown),
            zero,
            NirType::Bool,
        ),
        NirType::Bool,
    );

    assert_eq!(
        extract_selector_upper_bound_from_cond(&cond, &selector, false),
        Some(3)
    );
}

#[test]
fn x64_cbranch_condition_recovers_cmp_jnz_from_fresh_zero_flag() {
    let esi = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 48,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ecx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 8,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let cmp_lhs = Varnode {
        space_id: crate::nir::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let cmp_diff = Varnode {
        space_id: crate::nir::UNIQUE_SPACE_ID,
        offset: 0x108,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let zf = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 518,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let branch_cond = Varnode {
        space_id: crate::nir::UNIQUE_SPACE_ID,
        offset: 0x110,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let pcode = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1, 2],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1000,
                        output: Some(cmp_lhs.clone()),
                        inputs: vec![esi.clone()],
                        asm_mnemonic: Some("cmp".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntSub,
                        address: 0x1000,
                        output: Some(cmp_diff.clone()),
                        inputs: vec![cmp_lhs, ecx.clone()],
                        asm_mnemonic: Some("cmp".to_string()),
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x1000,
                        output: Some(zf.clone()),
                        inputs: vec![cmp_diff, Varnode::constant(0, 4)],
                        asm_mnemonic: Some("cmp".to_string()),
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::BoolNegate,
                        address: 0x1002,
                        output: Some(branch_cond.clone()),
                        inputs: vec![zf],
                        asm_mnemonic: Some("jnz".to_string()),
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x1002,
                        output: None,
                        inputs: vec![Varnode::constant(0x2000, 8), branch_cond],
                        asm_mnemonic: Some("jnz".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1004,
                successors: Vec::new(),
                ops: Vec::new(),
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x2000,
                successors: Vec::new(),
                ops: Vec::new(),
            },
        ],
    };
    let options = MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: true,
        is_big_endian: false,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0x1000,
        sections: vec![(0x1000, 0x3000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: CallingConvention::WindowsX64,
    };
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let (_, cond) = builder
        .lower_cbranch_condition_for_block(0)
        .expect("lower x64 cbranch condition");

    assert_eq!(
        cond,
        test_binary(
            HirBinaryOp::Ne,
            HirExpr::Var("rsi".to_string()),
            HirExpr::Var("param_1".to_string()),
            NirType::Bool
        )
    );
}

#[test]
fn return_recovery_keeps_return_register_before_side_effect_store() {
    let w0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4000,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let x8 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4040,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x40f0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let constant = |value, size| Varnode::constant(value, size);
    let pcode = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: Vec::new(),
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(w0.clone()),
                    inputs: vec![constant(7, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x1004,
                    output: None,
                    inputs: vec![constant(0, 4), x8, w0],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x1008,
                    output: None,
                    inputs: vec![lr],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    let options = MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: true,
        is_big_endian: false,
        pointer_size: 8,
        format: "ELF64".to_string(),
        image_base: 0,
        sections: vec![(0x1000, 0x2000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: CallingConvention::AArch64,
    };
    let code = render_mlil_preview(&pcode, "store_then_return", 0x1000, &options)
        .expect("preview render");

    assert!(
        code.lines()
            .any(|line| line.contains(" = 7;") && !line.contains("return")),
        "{code}"
    );
    assert!(code.contains("return 7;"), "{code}");
}

#[test]
fn x64_return_recovery_uses_eax_source_for_zero_extended_rax() {
    let eax = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x00,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let rax = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x00,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let pcode = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1400_1000,
            successors: Vec::new(),
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400_1000,
                    output: Some(eax.clone()),
                    inputs: vec![Varnode::constant(7, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x1400_1001,
                    output: Some(rax),
                    inputs: vec![eax],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x1400_1002,
                    output: None,
                    inputs: vec![Varnode::constant(0, 8)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    let options = MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: true,
        is_big_endian: false,
        pointer_size: 8,
        format: "PE".to_string(),
        image_base: 0x1400_0000,
        sections: vec![(0x1400_1000, 0x1400_2000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: CallingConvention::WindowsX64,
    };
    let code = render_mlil_preview(&pcode, "narrow_return", 0x1400_1000, &options)
        .expect("preview render");

    assert!(code.contains("return 7;"), "{code}");
    assert!(!code.contains("ulonglong narrow_return"), "{code}");
}

#[test]
fn arm32_return_target_register_uses_r0_value_not_lr_target() {
    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x58,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let masked_lr = Varnode {
        space_id: crate::nir::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let pcode = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x1000,
                    output: None,
                    inputs: vec![Varnode::constant(0x1004, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1004,
                successors: Vec::new(),
                ops: vec![
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAnd,
                        address: 0x1004,
                        output: Some(masked_lr.clone()),
                        inputs: vec![lr, Varnode::constant(0xffff_fffe, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Return,
                        address: 0x1008,
                        output: None,
                        inputs: vec![masked_lr],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };
    let options = MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: false,
        is_big_endian: false,
        pointer_size: 4,
        format: "ELF32".to_string(),
        image_base: 0,
        sections: vec![(0x1000, 0x2000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: CallingConvention::Arm32,
    };
    let code =
        render_mlil_preview(&pcode, "return_r0", 0x1000, &options).expect("preview render");

    assert!(code.contains("return param_1;"), "{code}");
    assert!(!code.contains("return lr"), "{code}");
}

fn arm32_pair_return_fixture(r0_value: i64, r1_value: i64) -> PcodeFunction {
    let r0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x24,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x58,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x2000,
            successors: Vec::new(),
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x2000,
                    output: Some(r0),
                    inputs: vec![Varnode::constant(r0_value, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x2004,
                    output: Some(r1),
                    inputs: vec![Varnode::constant(r1_value, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x2008,
                    output: None,
                    inputs: vec![lr],
                    asm_mnemonic: None,
                },
            ],
        }],
    }
}

fn arm32_pair_return_options(is_big_endian: bool) -> MlilPreviewOptions {
    MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: false,
        is_big_endian,
        pointer_size: 4,
        format: "ELF32".to_string(),
        image_base: 0,
        sections: vec![(0x2000, 0x3000)],
        region_linearize_structuring: false,
        force_linear_structuring: false,
        conservative_irreducible_fallback: false,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        global_names: Default::default(),
        global_sizes: Default::default(),
        relocation_names: Default::default(),
        calling_convention: CallingConvention::Arm32,
    }
}

#[test]
fn arm32_little_endian_pair_return_composes_r1_high_r0_low() {
    let pcode = arm32_pair_return_fixture(0x5566_7788, 0x1122_3344);
    let options = arm32_pair_return_options(false);

    let code = render_mlil_preview(&pcode, "u64_le", 0x2000, &options).expect("preview render");

    assert!(code.contains("return 1234605616436508552;"), "{code}");
}

#[test]
fn arm32_big_endian_pair_return_composes_r0_high_r1_low() {
    let pcode = arm32_pair_return_fixture(0x1122_3344, 0x5566_7788);
    let options = arm32_pair_return_options(true);

    let code = render_mlil_preview(&pcode, "u64_be", 0x2000, &options).expect("preview render");

    assert!(code.contains("return 1234605616436508552;"), "{code}");
}
