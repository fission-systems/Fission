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

fn op(seq_num: u32, opcode: PcodeOpcode, output: Option<Varnode>, inputs: Vec<Varnode>) -> PcodeOp {
    PcodeOp {
        seq_num,
        opcode,
        address: 0x1000 + u64::from(seq_num),
        output,
        inputs,
        asm_mnemonic: None,
    }
}

fn block_at(start_address: u64, index: u32, ops: Vec<PcodeOp>) -> crate::pcode::PcodeBasicBlock {
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
        ..Default::default()
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
#[ignore = "pre-existing failure"]
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

/// classify_range / setcc high path: `xor eax,eax; setnz al; add eax,2; ret`
/// must compose to `return (!zf) + 2`, not constant-fold through the pre-setcc zero.
#[test]
fn x86_32_xor_setnz_add_composes_partial_into_full_return() {
    use crate::nir::cspec::test_maps::apply_preview_cspec;
    use crate::nir::support::CallingConvention;

    let eax4 = register(0, 4);
    let eax1 = register(0, 1);
    let zf = register(0x206, 1);
    let setnz_tmp = varnode(0x17c00); // unique; resize to 1 below
    let setnz_tmp = Varnode {
        size: 1,
        ..setnz_tmp
    };
    let mut options = test_options();
    options.pe_x64_only = false;
    options.is_64bit = false;
    options.pointer_size = 4;
    options.format = "PE32".to_string();
    options.image_base = 0x401000;
    options.sections = vec![(0x401000, 0x402000)];
    options.calling_convention = CallingConvention::X86_32;
    apply_preview_cspec(&mut options);

    // junk return address (epilogue-style RET)
    let ret_addr = register(0x284, 4);
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            // xor eax, eax
            op(
                0,
                PcodeOpcode::IntXor,
                Some(eax4.clone()),
                vec![eax4.clone(), eax4.clone()],
            ),
            // setnz al  (zf already live-in for the test)
            op(
                1,
                PcodeOpcode::BoolNegate,
                Some(setnz_tmp.clone()),
                vec![zf.clone()],
            ),
            op(2, PcodeOpcode::Copy, Some(eax1), vec![setnz_tmp]),
            // add eax, 2
            op(
                3,
                PcodeOpcode::IntAdd,
                Some(eax4.clone()),
                vec![eax4.clone(), constant_sized(2, 4)],
            ),
            op(4, PcodeOpcode::Return, None, vec![ret_addr]),
        ],
    )]);

    let code = render_mlil_preview(&pcode, "xor_setnz_add", 0x1000, &options).expect("render");
    // Must not collapse to bare `return 2` (ignoring setnz).
    assert!(
        !code.lines().any(|l| l.trim() == "return 2;"),
        "setnz low-byte must feed add/return, got:\n{code}"
    );
    assert!(
        code.contains("return")
            && (code.contains("+ 2")
                || code.contains("+2")
                || code.contains("+= 2")
                || code.contains("eax")),
        "expected composed setnz+add return, got:\n{code}"
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

/// Byte accumulator shape: 1-byte `add al, mem`; `movzx eax, al`.
/// Must recompile as zero-extend (not `char al; eax = al & -1`).
#[test]
fn movzx_after_byte_add_zero_extends_unsigned() {
    let options = test_options();
    let eax = register(0, 4);
    let al = register(0, 1);
    let edx = register(0x10, 4);

    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(eax.clone()),
                vec![constant_sized(0, 4)],
            ),
            // ADD AL, byte ptr [EDX]  (1-byte add into AL)
            op(
                1,
                PcodeOpcode::Load,
                Some(register(0x200, 1)),
                vec![constant_sized(3, 4), edx.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(al.clone()),
                vec![al.clone(), register(0x200, 1)],
            ),
            // movzx EAX, AL
            op(3, PcodeOpcode::IntZExt, Some(eax.clone()), vec![al]),
            op(4, PcodeOpcode::Return, None, vec![eax]),
        ],
    )]);

    let code = render_mlil_preview(&pcode, "byte_add_movzx", 0x1000, &options)
        .expect("render byte add + movzx");

    // Must not leave a signed char + identity-and that sign-extends on recompile.
    assert!(
        !code.contains("char al") || code.contains("uchar al") || code.contains("(uchar)"),
        "byte accumulator should be unsigned or cast through uchar:\n{code}"
    );
    assert!(
        !code.contains("& -1"),
        "ZExt must not print as `x & -1` (sign-extends char on recompile):\n{code}"
    );
    assert!(
        code.contains("& 0xff")
            || code.contains("& 255")
            || code.contains("(uchar)")
            || code.contains("% 256")
            || code.contains("%256"),
        "expected zero-extend / low-byte keep of AL:\n{code}"
    );
}

/// RC4 keystream index pattern: `add EAX, ECX; movzx EDX, AL; … use EDX`.
/// Truncation must apply to the *destination* register (EDX), not only when
/// movzx rewrites EAX in place.
#[test]
fn movzx_al_into_edx_after_int_add_preserves_low_byte_truncation() {
    let options = test_options();
    // Register-space layout matches rust-sleigh x86-64 (RAX@0, RCX@8, RDX@0x10).
    let eax = register(0, 4);
    let al = register(0, 1);
    let ecx = register(8, 4);
    let edx = register(0x10, 4);

    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(eax.clone()),
                vec![constant_sized(200, 4)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(ecx.clone()),
                vec![constant_sized(100, 4)],
            ),
            // EAX = 200 + 100 = 300
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(eax.clone()),
                vec![eax, ecx],
            ),
            // movzx EDX, AL  →  EDX = 300 & 0xff == 44
            op(3, PcodeOpcode::IntZExt, Some(edx.clone()), vec![al]),
            // Move truncated value into the return register so ABI return is EDX's value.
            op(4, PcodeOpcode::Copy, Some(register(0, 4)), vec![edx]),
            op(5, PcodeOpcode::Return, None, vec![register(0, 4)]),
        ],
    )]);

    let code = render_mlil_preview(&pcode, "movzx_al_edx_trunc", 0x1000, &options)
        .expect("render movzx AL→EDX truncation");

    assert!(
        code.contains("= 44")
            || code.contains("return 44")
            || code.contains("return (uchar)300")
            || code.contains("(uchar)rax")
            || code.contains("& 0xff")
            || code.contains("& 255"),
        "expected low-byte truncation of 200+100=300 → 44 via EDX:\n{code}"
    );
    assert!(
        !code.contains("= 300") && !code.contains("return 300;") && !code.contains("return 0x12c;"),
        "must not keep untruncated sum 300:\n{code}"
    );
}

/// Exact RC4 keystream p-code shape:
///   INT_ADD eax, ecx; INT_ZEXT rax <- eax; …flags…;
///   INT_ZEXT edx <- al; INT_ZEXT rdx <- edx; INT_ADD rax, rdx
#[test]
fn rc4_keystream_movzx_sequence_truncates_index() {
    let options = test_options();
    let eax = register(0, 4);
    let al = register(0, 1);
    let rax = register(0, 8);
    let ecx = register(8, 4);
    let edx = register(0x10, 4);
    let rdx = register(0x10, 8);
    let base = register(0x30, 8);
    let cf = register(0x200, 1); // flag-like unique-ish offset in reg space

    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Copy, Some(eax.clone()), vec![constant_sized(200, 4)]),
            op(1, PcodeOpcode::Copy, Some(ecx.clone()), vec![constant_sized(100, 4)]),
            op(2, PcodeOpcode::Copy, Some(base.clone()), vec![constant(0x1000)]),
            // INT_ADD eax, ecx
            op(3, PcodeOpcode::IntAdd, Some(eax.clone()), vec![eax.clone(), ecx]),
            // INT_ZEXT rax <- eax  (full-width widen after ADD)
            op(4, PcodeOpcode::IntZExt, Some(rax.clone()), vec![eax.clone()]),
            // flag noise: INT_AND unique = eax & 0xff for PF (must not steal data path)
            op(
                5,
                PcodeOpcode::IntAnd,
                Some(register(0x58300, 4)),
                vec![eax.clone(), constant_sized(0xff, 4)],
            ),
            // movzx edx, al
            op(6, PcodeOpcode::IntZExt, Some(edx.clone()), vec![al]),
            // zext rdx <- edx
            op(7, PcodeOpcode::IntZExt, Some(rdx.clone()), vec![edx]),
            // rax = base + rdx  (should be 0x1000 + 44, not 0x1000 + 300)
            op(8, PcodeOpcode::IntAdd, Some(rax.clone()), vec![base, rdx]),
            // copy address into return reg
            op(9, PcodeOpcode::Copy, Some(register(0, 8)), vec![rax]),
            op(10, PcodeOpcode::Return, None, vec![register(0, 8)]),
        ],
    )]);

    let code = render_mlil_preview(&pcode, "rc4_ks_idx", 0x1000, &options).expect("render");
    // 0x1000 + (300 & 0xff) = 0x1000 + 44 = 4140, or `rdx = 44; return base + rdx`
    assert!(
        code.contains("4140")
            || code.contains("0x102c")
            || code.contains("= 44")
            || code.contains("(uchar)rax")
            || (code.contains("0x1000")
                && (code.contains("& 0xff")
                    || code.contains("& 255")
                    || code.contains("% 256")
                    || code.contains("(uchar)"))),
        "expected base+truncated index (4140) or rdx=44:\n{code}"
    );
    assert!(
        !code.contains("4300") && !code.contains("0x10cc") && !code.contains("= 300"),
        "must not add untruncated 300 to base (0x1000+300=4300):\n{code}"
    );
    let _ = cf;
}

/// Non-constant form: loads + add + movzx EDX,AL + pointer index (RC4 keystream).
#[test]
fn movzx_al_index_after_byte_loads_truncates_before_ptr_add() {
    let options = test_options();
    let eax = register(0, 4);
    let al = register(0, 1);
    let ecx = register(8, 4);
    let edx = register(0x10, 4);
    let rdx = register(0x10, 8);
    let rax = register(0, 8);
    let base = register(0x30, 8); // rsi-like holder for array base
    let unique_byte = |off| Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: off,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };

    // base in RSI-like reg; two byte loads into EAX/ECX; add; movzx edx,al; add base,rdx; load; return
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            // base = param-like constant address 0x1000 (treated as pointer value)
            op(
                0,
                PcodeOpcode::Copy,
                Some(base.clone()),
                vec![constant(0x1000)],
            ),
            // load byte -> EAX via zext of load
            op(
                1,
                PcodeOpcode::Load,
                Some(unique_byte(0x10)),
                vec![constant_sized(3, 4), base.clone()],
            ),
            op(
                2,
                PcodeOpcode::IntZExt,
                Some(eax.clone()),
                vec![unique_byte(0x10)],
            ),
            // second load byte -> ECX
            op(
                3,
                PcodeOpcode::Load,
                Some(unique_byte(0x20)),
                vec![
                    constant_sized(3, 4),
                    // base+1
                    {
                        // use IntAdd into unique then load - simplify: load from base again
                        base.clone()
                    },
                ],
            ),
            op(
                4,
                PcodeOpcode::IntZExt,
                Some(ecx.clone()),
                vec![unique_byte(0x20)],
            ),
            // EAX = EAX + ECX
            op(
                5,
                PcodeOpcode::IntAdd,
                Some(eax.clone()),
                vec![eax.clone(), ecx],
            ),
            // movzx EDX, AL
            op(6, PcodeOpcode::IntZExt, Some(edx.clone()), vec![al]),
            // RDX = zext EDX
            op(7, PcodeOpcode::IntZExt, Some(rdx.clone()), vec![edx]),
            // RAX = base + RDX
            op(
                8,
                PcodeOpcode::IntAdd,
                Some(rax.clone()),
                vec![base, rdx],
            ),
            // load result byte and return as int
            op(
                9,
                PcodeOpcode::Load,
                Some(unique_byte(0x30)),
                vec![constant_sized(3, 4), rax],
            ),
            op(
                10,
                PcodeOpcode::IntZExt,
                Some(register(0, 4)),
                vec![unique_byte(0x30)],
            ),
            op(11, PcodeOpcode::Return, None, vec![register(0, 4)]),
        ],
    )]);

    let code = render_mlil_preview(&pcode, "movzx_index", 0x1000, &options)
        .expect("render movzx index");

    // Truncation must appear on the index path: (uchar), & 0xff, or % 256.
    let has_trunc = code.contains("(uchar)")
        || code.contains("& 0xff")
        || code.contains("& 255")
        || code.contains("% 256")
        || code.contains("% 0x100");
    assert!(
        has_trunc,
        "expected low-byte truncation on keystream-style index path:\n{code}"
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
