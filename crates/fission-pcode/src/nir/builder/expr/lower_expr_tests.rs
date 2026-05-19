use super::*;
use crate::nir::render_mlil_preview;

fn varnode(offset: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size: 8,
        is_constant: false,
        constant_val: 0,
    }
}

fn register(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn constant(value: i64) -> Varnode {
    Varnode::constant(value, 8)
}

fn constant_sized(value: i64, size: u32) -> Varnode {
    Varnode::constant(value, size)
}

fn op(
    seq_num: u32,
    opcode: PcodeOpcode,
    output: Option<Varnode>,
    inputs: Vec<Varnode>,
) -> PcodeOp {
    PcodeOp {
        seq_num,
        opcode,
        address: 0x1000 + u64::from(seq_num),
        output,
        inputs,
        asm_mnemonic: None,
    }
}

fn block_at(
    start_address: u64,
    index: u32,
    ops: Vec<PcodeOp>,
) -> crate::pcode::PcodeBasicBlock {
    crate::pcode::PcodeBasicBlock {
        index,
        start_address,
        successors: Vec::new(),
        ops,
    }
}

fn pcode_function(blocks: Vec<crate::pcode::PcodeBasicBlock>) -> crate::pcode::PcodeFunction {
    crate::pcode::PcodeFunction { blocks }
}

fn test_options() -> MlilPreviewOptions {
    MlilPreviewOptions {
        pe_x64_only: true,
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
        calling_convention: Default::default(),
    }
}

#[test]
fn diamond_join_lowers_branch_local_register_defs_as_select() {
    let cond = varnode(0x80);
    let rax = register(0, 8);
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![op(
                1,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1020), cond],
            )],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(2, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(10)]),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(4, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(20)]),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
            ],
        ),
        block_at(0x1030, 3, vec![op(6, PcodeOpcode::Return, None, vec![rax])]),
    ]);
    let options = test_options();

    let code = render_mlil_preview(&pcode, "diamond_select", 0x1000, &options).expect("render");

    assert!(
        code.contains("return tmp_80 ? 20 : 10;"),
        "expected branch-target arm to be the true select arm:\n{code}"
    );
}

#[test]
fn diamond_join_lowers_copy_through_join_read_as_select() {
    let cond = varnode(0x80);
    let rax = register(0, 8);
    let rcx = register(8, 8);
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(10)]),
                op(2, PcodeOpcode::CBranch, None, vec![constant(0x1020), cond]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(3, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(20)]),
                op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(5, PcodeOpcode::Copy, Some(rcx.clone()), vec![rax]),
                op(6, PcodeOpcode::Return, None, vec![rcx]),
            ],
        ),
    ]);
    let options = test_options();

    let code =
        render_mlil_preview(&pcode, "diamond_copy_select", 0x1000, &options).expect("render");

    assert!(
        code.contains("return tmp_80 ? 10 : 20;"),
        "expected copy-through join read to use the synthesized select:\n{code}"
    );
}

#[test]
fn same_block_partial_register_write_with_zeroed_upper_replaces_stale_wide_def() {
    let mut options = test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let s0 = register(0x5000, 4);
    let h0 = register(0x5000, 2);
    let upper_h0 = register(0x5002, 2);
    let w8 = register(0x4040, 4);
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(s0.clone()),
                vec![constant(0x1234_5678)],
            ),
            op(1, PcodeOpcode::Copy, Some(h0.clone()), vec![h0.clone()]),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(h0.clone()),
                vec![constant_sized(1, 2), constant_sized(2, 2)],
            ),
            op(
                3,
                PcodeOpcode::Copy,
                Some(upper_h0),
                vec![constant_sized(0, 2)],
            ),
            op(4, PcodeOpcode::Copy, Some(w8.clone()), vec![s0]),
            op(5, PcodeOpcode::Return, None, vec![w8]),
        ],
    )]);

    let code = render_mlil_preview(&pcode, "partial_zero_extend", 0x1000, &options)
        .expect("render partial zero-extend");

    assert!(
        code.contains("return 3;"),
        "expected low partial write to replace stale wide definition:\n{code}"
    );
    assert!(
        !code.contains("305419896"),
        "stale full-width definition should not feed the return:\n{code}"
    );
}

#[test]
fn partial_register_zero_extend_ignores_stale_virtual_lowering_site_bound() {
    let mut options = test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let w0 = register(0x5000, 4);
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![op(
            0,
            PcodeOpcode::Copy,
            Some(w0.clone()),
            vec![constant(1)],
        )],
    )]);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 0,
        op_idx: 12,
    });
    let mut visiting = HashSet::new();

    let lowered = builder
        .try_lower_zero_extended_partial_register(&w0, &mut visiting)
        .expect("stale lowering-site op index should not panic");

    assert!(lowered.is_none());
}

#[test]
fn join_register_read_uses_edge_zero_as_carried_initializer() {
    let mut options = test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let cond = varnode(0x80);
    let x0 = register(0x4000, 8);
    let w0 = register(0x4000, 4);
    let cond_def = op(
        0,
        PcodeOpcode::IntEqual,
        Some(cond.clone()),
        vec![w0.clone(), constant_sized(0, 4)],
    );
    let carried_def = op(
        1,
        PcodeOpcode::Copy,
        Some(x0.clone()),
        vec![constant_sized(7, 4)],
    );
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                cond_def,
                op(2, PcodeOpcode::CBranch, None, vec![constant(0x1020), cond]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                carried_def.clone(),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![op(4, PcodeOpcode::Return, None, vec![w0.clone()])],
        ),
    ];
    blocks[0].successors = vec![2, 1];
    blocks[1].successors = vec![2];
    let pcode = pcode_function(blocks);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&x0, &carried_def),
        "carried".to_string(),
    );
    builder.temps.insert(
        "carried".to_string(),
        NirBinding {
            name: "carried".to_string(),
            ty: type_from_size(8, false),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        },
    );
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 2,
        op_idx: 0,
    });
    let mut visiting = HashSet::new();

    let lowered = builder
        .lower_varnode(&w0, &mut visiting)
        .expect("join register read lowers");

    assert_eq!(
        lowered,
        HirExpr::Cast {
            ty: type_from_size(4, false),
            expr: Box::new(HirExpr::Var("carried".to_string())),
        }
    );
    assert_eq!(
        builder
            .temps
            .get("carried")
            .and_then(|binding| binding.initializer.as_ref()),
        Some(&HirExpr::Const(0, type_from_size(4, false)))
    );
}

#[test]
fn loop_exit_register_read_uses_predecessor_path_zero_seed() {
    let r10d = register(0x90, 4);
    let r10 = register(0x90, 8);
    let cond = varnode(0x80);
    let loop_def = op(
        4,
        PcodeOpcode::IntZExt,
        Some(r10.clone()),
        vec![r10d.clone()],
    );
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![
                op(
                    0,
                    PcodeOpcode::Copy,
                    Some(r10d.clone()),
                    vec![constant_sized(0, 4)],
                ),
                op(
                    1,
                    PcodeOpcode::IntZExt,
                    Some(r10.clone()),
                    vec![r10d.clone()],
                ),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![op(
                3,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1020), cond],
            )],
        ),
        block_at(
            0x1020,
            2,
            vec![op(6, PcodeOpcode::Return, None, vec![r10.clone()])],
        ),
        block_at(
            0x1030,
            3,
            vec![
                loop_def.clone(),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        ),
    ];
    blocks[0].successors = vec![1];
    blocks[1].successors = vec![2, 3];
    blocks[2].successors = Vec::new();
    blocks[3].successors = vec![2];
    let pcode = pcode_function(blocks);
    let options = test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&r10, &loop_def),
        "loop_acc".to_string(),
    );
    builder.temps.insert(
        "loop_acc".to_string(),
        NirBinding {
            name: "loop_acc".to_string(),
            ty: type_from_size(8, false),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        },
    );
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 2,
        op_idx: 0,
    });
    let mut visiting = HashSet::new();

    let lowered = builder
        .lower_varnode(&r10, &mut visiting)
        .expect("loop-exit register read lowers");

    assert_eq!(lowered, HirExpr::Var("loop_acc".to_string()));
    assert_eq!(
        builder
            .temps
            .get("loop_acc")
            .and_then(|binding| binding.initializer.as_ref()),
        Some(&HirExpr::Const(0, type_from_size(8, false)))
    );
    assert!(
        builder.params.is_empty(),
        "must not promote the accumulator to a parameter"
    );
}

#[test]
fn join_register_update_read_stays_live_register_instead_of_abi_param() {
    let mut options = test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let w0 = register(0x4000, 4);
    let x0 = register(0x4000, 8);
    let w8 = register(0x4040, 4);
    let sum = Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: 0x200,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let mut blocks = vec![
        block_at(
            0x1000,
            0,
            vec![op(0, PcodeOpcode::Branch, None, vec![constant(0x1020)])],
        ),
        block_at(
            0x1010,
            1,
            vec![op(1, PcodeOpcode::Branch, None, vec![constant(0x1020)])],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(
                    2,
                    PcodeOpcode::IntAdd,
                    Some(sum.clone()),
                    vec![w0.clone(), w8],
                ),
                op(3, PcodeOpcode::IntZExt, Some(x0), vec![sum]),
            ],
        ),
    ];
    blocks[0].successors = vec![2];
    blocks[1].successors = vec![2];
    let pcode = pcode_function(blocks);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 2,
        op_idx: 0,
    });
    let mut visiting = HashSet::new();

    let lowered = builder
        .lower_varnode(&w0, &mut visiting)
        .expect("join register update read lowers");

    assert_eq!(lowered, HirExpr::Var("w0".to_string()));
}
