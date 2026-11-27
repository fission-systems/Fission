// SPDX-License-Identifier: Apache-2.0
//! Issue #1426: Preserve unsigned zero-extension across 1-byte ALU ops
//! wrapped by pure identity ops (`tmp ^ 0`, `tmp + 0`, `tmp | 0`) preceding `IntZExt`.

use fission_decompiler::midend::{
    CallingConvention, MlilPreviewOptions, StructuringEngineKind, render_nir_with_context_output,
};
use fission_dir::eval::{World, normalize, run_hir};
use fission_pcode_ir::space::{REGISTER_SPACE_ID, UNIQUE_SPACE_ID};
use fission_pcode_ir::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

fn reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn uniq(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn cst(val: i64, size: u32) -> Varnode {
    Varnode::constant(val, size)
}

fn sysv_options() -> MlilPreviewOptions {
    MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: true,
        is_big_endian: false,
        pointer_size: 8,
        format: "ELF64".to_string(),
        calling_convention: CallingConvention::SystemVAmd64,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        cspec_param_offsets: Some(vec![0x38, 0x30, 0x10, 0x08, 0x80, 0x88]),
        ..Default::default()
    }
}

/// Owned 1-byte Load -> ALU -> IntZExt (1->4) SysV64 P-code fixture matching
/// the minimal pattern and witness cases from Issue #1426.
/// When `identity_op` is `Some(op)`, wraps the two 1-byte ALU ops (`new_b0`,
/// `new_b1`) with a trailing identity op `op(tmp, 0)`.
fn build_subword_zext_fixture(identity_ops: Option<[PcodeOpcode; 2]>) -> PcodeFunction {
    let rdi = reg(0x38, 8);
    let rsi = reg(0x30, 8);
    let rax = reg(0x00, 8);
    let ram_space = cst(0, 8);

    let key32 = uniq(0x300, 4);
    let b0 = uniq(0x310, 1);
    let key8 = uniq(0x320, 1);
    let new_b0 = reg(0x80, 1);
    let tmp_b0 = uniq(0x600, 1);
    let ptr1 = uniq(0x340, 8);
    let b1 = uniq(0x350, 1);
    let new_b1 = reg(0x88, 1);
    let tmp_b1 = uniq(0x610, 1);
    let bit = uniq(0x370, 4);
    let cond = uniq(0x380, 1);
    let b0_32 = reg(0x08, 4);
    let b1_32 = reg(0x10, 4);
    let b1_sh = uniq(0x3b0, 4);
    let tag_t = uniq(0x3c0, 4);
    let tag_f = uniq(0x3d0, 4);
    let tag_t64 = uniq(0x3e0, 8);
    let tag_f64 = uniq(0x3f0, 8);

    let mut entry_ops = vec![
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::SubPiece,
            address: 0x3000,
            output: Some(key32.clone()),
            inputs: vec![rsi, cst(0, 4)],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::SubPiece,
            address: 0x3000,
            output: Some(key8.clone()),
            inputs: vec![key32.clone(), cst(0, 4)],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 2,
            opcode: PcodeOpcode::Load,
            address: 0x3000,
            output: Some(b0.clone()),
            inputs: vec![ram_space.clone(), rdi.clone()],
            asm_mnemonic: None,
        },
    ];

    if let Some([op0, _]) = identity_ops {
        entry_ops.push(PcodeOp {
            seq_num: 3,
            opcode: PcodeOpcode::IntXor,
            address: 0x3000,
            output: Some(tmp_b0.clone()),
            inputs: vec![b0, key8],
            asm_mnemonic: None,
        });
        entry_ops.push(PcodeOp {
            seq_num: 4,
            opcode: op0,
            address: 0x3000,
            output: Some(new_b0.clone()),
            inputs: vec![tmp_b0, cst(0, 1)],
            asm_mnemonic: None,
        });
    } else {
        entry_ops.push(PcodeOp {
            seq_num: 3,
            opcode: PcodeOpcode::IntXor,
            address: 0x3000,
            output: Some(new_b0.clone()),
            inputs: vec![b0, key8],
            asm_mnemonic: None,
        });
    }

    entry_ops.extend([
        PcodeOp {
            seq_num: 5,
            opcode: PcodeOpcode::IntZExt,
            address: 0x3000,
            output: Some(b0_32.clone()),
            inputs: vec![new_b0.clone()],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 6,
            opcode: PcodeOpcode::Store,
            address: 0x3000,
            output: None,
            inputs: vec![ram_space.clone(), rdi.clone(), new_b0],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::IntAdd,
            address: 0x3004,
            output: Some(ptr1.clone()),
            inputs: vec![rdi, cst(1, 8)],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::Load,
            address: 0x3004,
            output: Some(b1.clone()),
            inputs: vec![ram_space.clone(), ptr1.clone()],
            asm_mnemonic: None,
        },
    ]);

    if let Some([_, op1]) = identity_ops {
        entry_ops.push(PcodeOp {
            seq_num: 2,
            opcode: PcodeOpcode::IntAdd,
            address: 0x3004,
            output: Some(tmp_b1.clone()),
            inputs: vec![b1, cst(5, 1)],
            asm_mnemonic: None,
        });
        entry_ops.push(PcodeOp {
            seq_num: 3,
            opcode: op1,
            address: 0x3004,
            output: Some(new_b1.clone()),
            inputs: vec![tmp_b1, cst(0, 1)],
            asm_mnemonic: None,
        });
    } else {
        entry_ops.push(PcodeOp {
            seq_num: 2,
            opcode: PcodeOpcode::IntAdd,
            address: 0x3004,
            output: Some(new_b1.clone()),
            inputs: vec![b1, cst(5, 1)],
            asm_mnemonic: None,
        });
    }

    entry_ops.extend([
        PcodeOp {
            seq_num: 4,
            opcode: PcodeOpcode::IntZExt,
            address: 0x3004,
            output: Some(b1_32.clone()),
            inputs: vec![new_b1.clone()],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 5,
            opcode: PcodeOpcode::Store,
            address: 0x3004,
            output: None,
            inputs: vec![ram_space.clone(), ptr1, new_b1],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::IntAnd,
            address: 0x300c,
            output: Some(bit.clone()),
            inputs: vec![key32, cst(2, 4)],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::IntNotEqual,
            address: 0x300c,
            output: Some(cond.clone()),
            inputs: vec![bit, cst(0, 4)],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 2,
            opcode: PcodeOpcode::CBranch,
            address: 0x300c,
            output: None,
            inputs: vec![cst(0x3040, 8), cond],
            asm_mnemonic: None,
        },
    ]);

    for (idx, op) in entry_ops.iter_mut().enumerate() {
        op.seq_num = u32::try_from(idx).unwrap();
    }

    PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3000,
                successors: vec![1, 2],
                ops: entry_ops,
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLeft,
                        address: 0x3020,
                        output: Some(b1_sh.clone()),
                        inputs: vec![b1_32.clone(), cst(8, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntOr,
                        address: 0x3020,
                        output: Some(tag_f.clone()),
                        inputs: vec![b0_32.clone(), b1_sh],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x3020,
                        output: Some(tag_f64.clone()),
                        inputs: vec![tag_f],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3020,
                        output: Some(rax.clone()),
                        inputs: vec![tag_f64.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::Return,
                        address: 0x3024,
                        output: None,
                        inputs: vec![ram_space.clone(), tag_f64],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3040,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3040,
                        output: Some(tag_t.clone()),
                        inputs: vec![b0_32, b1_32],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x3040,
                        output: Some(tag_t64.clone()),
                        inputs: vec![tag_t],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3040,
                        output: Some(rax),
                        inputs: vec![tag_t64.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Return,
                        address: 0x3044,
                        output: None,
                        inputs: vec![ram_space, tag_t64],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    }
}

fn eval_fixture(pcode: &PcodeFunction, arg1: u32, buf: [u8; 4]) -> (i64, Vec<u8>, String) {
    let output =
        render_nir_with_context_output(pcode, "subword_zext", 0x3000, &sysv_options(), None, None)
            .expect("render fixture");
    let hir = output.printed_hir.as_ref().expect("printed_hir");
    let ptr: u64 = 0x10_0000;
    let image: Vec<(u64, u8)> = buf
        .iter()
        .enumerate()
        .map(|(i, &b)| (ptr + i as u64, b))
        .collect();
    let world = World {
        image: &image,
        ..World::default()
    };
    let run = run_hir(
        &hir.body,
        &hir.params,
        &hir.locals,
        &[ptr as i64, i64::from(arg1)],
        &world,
    )
    .expect("run_hir");
    let ret = normalize(run.ret.expect("return value"), &hir.return_type);
    let final_buf = (0..buf.len())
        .map(|i| {
            let addr = ptr + i as u64;
            run.writes.get(&addr).copied().unwrap_or(buf[i])
        })
        .collect();
    (ret, final_buf, output.code)
}

#[test]
fn subword_alu_with_identity_op_preserves_unsigned_int_zext() {
    let orig = build_subword_zext_fixture(None);
    // Witness 1 (false arm: `(key32 & 2) == 0`, `(b0 ^ key8) | ((b1 + 5) << 8)`):
    //   buf = [78, 185, 247, 12], arg0 = 2384320568 (low byte = 0x38)
    //   new_b0 = 78 ^ 0x38 = 0x76, new_b1 = 185 + 5 = 190 = 0xBE (>= 0x80!)
    //   Expected: 0x0000BE76 (48758), NOT 0xFFFFBE76.
    // Witness 2 (true arm: `(key32 & 2) != 0`, `(b0 ^ key8) + (b1 + 5)`):
    //   buf = [28, 248, 63, 185], arg0 = 1980680139 (low byte = 0xCB)
    //   new_b0 = 28 ^ 0xCB = 215 = 0xD7, new_b1 = 248 + 5 = 253 = 0xFD (>= 0x80!)
    //   Expected: 215 + 253 = 468, NOT 215 + (-3) = 212.
    let cases = [
        (2384320568u32, [78u8, 185, 247, 12], 0x0000_BE76_i64),
        (1980680139u32, [28u8, 248, 63, 185], 468_i64),
    ];

    for (arg1, buf, expected_ret) in cases {
        let (orig_ret, orig_buf, orig_code) = eval_fixture(&orig, arg1, buf);
        assert_eq!(
            orig_ret, expected_ret,
            "original HIR diverged:\n{orig_code}"
        );

        for pair in [
            [PcodeOpcode::IntXor, PcodeOpcode::IntAdd],
            [PcodeOpcode::IntOr, PcodeOpcode::IntXor],
            [PcodeOpcode::IntAdd, PcodeOpcode::IntOr],
        ] {
            let variant = build_subword_zext_fixture(Some(pair));
            let (var_ret, var_buf, var_code) = eval_fixture(&variant, arg1, buf);
            assert_eq!(
                var_ret, expected_ret,
                "variant {pair:?} diverged for arg1={arg1} buf={buf:?}:\n{var_code}"
            );
            assert_eq!(var_buf, orig_buf, "memory writes diverged for {pair:?}");
        }
    }
}
