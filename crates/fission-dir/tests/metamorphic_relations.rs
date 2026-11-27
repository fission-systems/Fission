// SPDX-License-Identifier: Apache-2.0
//! Issue #1253: Metamorphic decompilation tests over semantics-preserving
//! binary and IR variants.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use fission_dir::eval::{World, normalize, run_hir};
use fission_dir::metamorphic::{
    ArtifactDigest, DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET, FixtureProperties, IrInputAbi,
    MetamorphicManifest, MetamorphicSample, MetamorphicWitness, RelationKind, RelationOutcome,
    SemanticObservation, check_binary_relation_precondition, generate_smoke_samples,
    negative_flip_branch_without_swap, negative_perturb_arith_result, sha256_file,
    transform_branch_invert_swap, transform_edge_split_and_permute, transform_identity_ops,
    transform_identity_ops_including_subword, verify_ir_metamorphic_pair,
};
use fission_dir::{
    EmulatorHarness, Grounding, RunPlan, StubbedEnd, VerifyOutcome, call_sites,
    check_ground_truth_with, decompile_one,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_midend_core::ir::{HirExpr, HirStmt};
use fission_pcode_ir::space::{REGISTER_SPACE_ID, UNIQUE_SPACE_ID};
use fission_pcode_ir::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use fission_static::analysis::decomp::facts::FactStore;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

fn load_fixture_elf(name: &str) -> (PathBuf, LoadedBinary) {
    let path = testdata_dir().join(name);
    let binary =
        LoadedBinary::from_file(&path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
    (path, binary)
}

fn entry_of(binary: &LoadedBinary, name: &str) -> u64 {
    let inner = binary.inner();
    if let Some(&idx) = inner.function_name_index.get(name) {
        return inner.functions[idx].address;
    }
    let trimmed = name.trim_start_matches('_');
    inner
        .functions
        .iter()
        .find(|f| f.name == name || f.name.trim_start_matches('_') == trimmed)
        .unwrap_or_else(|| panic!("symbol `{name}` missing in {}", binary.path))
        .address
}

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

/// Owned SysV64 P-code fixture with a conditional branch:
/// Computes `(a ^ b) & 1 ? ((a * 3 + b) >> 1) ^ 0x55 : (a + (b & 0xff)) ^ 0x3c`.
fn build_ir_branch_mix_fixture() -> PcodeFunction {
    let rdi = reg(0x38, 8);
    let rsi = reg(0x30, 8);
    let rax = reg(0x00, 8);
    let ram_space = cst(0, 8);

    let a32 = uniq(0x100, 4);
    let b32 = uniq(0x110, 4);
    let x = uniq(0x120, 4);
    let lsb = uniq(0x130, 4);
    let cond = uniq(0x140, 1);

    let mul_t = uniq(0x150, 4);
    let add_t = uniq(0x160, 4);
    let shr_t = uniq(0x170, 4);
    let res_t = uniq(0x180, 4);
    let res_t64 = uniq(0x190, 8);

    let mask_f = uniq(0x1a0, 4);
    let add_f = uniq(0x1b0, 4);
    let res_f = uniq(0x1c0, 4);
    let res_f64 = uniq(0x1d0, 8);

    PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1, 2],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x1000,
                        output: Some(a32.clone()),
                        inputs: vec![rdi, cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x1000,
                        output: Some(b32.clone()),
                        inputs: vec![rsi, cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x1000,
                        output: Some(x.clone()),
                        inputs: vec![a32.clone(), b32.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntAnd,
                        address: 0x1000,
                        output: Some(lsb.clone()),
                        inputs: vec![x, cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntNotEqual,
                        address: 0x1000,
                        output: Some(cond.clone()),
                        inputs: vec![lsb, cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x1004,
                        output: None,
                        inputs: vec![cst(0x1020, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            // Else path (fallthrough when cond == 0)
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAnd,
                        address: 0x1010,
                        output: Some(mask_f.clone()),
                        inputs: vec![b32.clone(), cst(0xff, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1010,
                        output: Some(add_f.clone()),
                        inputs: vec![a32.clone(), mask_f],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x1010,
                        output: Some(res_f.clone()),
                        inputs: vec![add_f, cst(0x3c, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1010,
                        output: Some(res_f64.clone()),
                        inputs: vec![res_f],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1014,
                        output: Some(rax.clone()),
                        inputs: vec![res_f64.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::Return,
                        address: 0x1018,
                        output: None,
                        inputs: vec![ram_space.clone(), res_f64],
                        asm_mnemonic: None,
                    },
                ],
            },
            // Then path (taken when cond == 1)
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntMult,
                        address: 0x1020,
                        output: Some(mul_t.clone()),
                        inputs: vec![a32, cst(3, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1020,
                        output: Some(add_t.clone()),
                        inputs: vec![mul_t, b32],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntRight,
                        address: 0x1020,
                        output: Some(shr_t.clone()),
                        inputs: vec![add_t, cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x1020,
                        output: Some(res_t.clone()),
                        inputs: vec![shr_t, cst(0x55, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1020,
                        output: Some(res_t64.clone()),
                        inputs: vec![res_t],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1024,
                        output: Some(rax),
                        inputs: vec![res_t64.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Return,
                        address: 0x1028,
                        output: None,
                        inputs: vec![ram_space, res_t64],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    }
}

/// Owned SysV64 P-code fixture with a bounded loop summing bytes `buf[0 .. (len & 3)]`:
/// Exercises loops + memory loads + conditional exit.
fn build_ir_bounded_loop_fixture() -> PcodeFunction {
    let rdi = reg(0x38, 8);
    let rsi = reg(0x30, 8);
    let rcx = reg(0x08, 8); // loop index i
    let rdx = reg(0x10, 8); // accumulator
    let rbx = reg(0x18, 8); // bound = len & 3
    let rax = reg(0x00, 8);
    let ram_space = cst(0, 8);

    let len32 = uniq(0x200, 4);
    let bound32 = uniq(0x210, 4);
    let cond = uniq(0x220, 1);
    let addr_t = uniq(0x230, 8);
    let byte_t = uniq(0x240, 1);
    let byte64 = uniq(0x250, 8);
    let sum_next = uniq(0x260, 8);
    let i_next = uniq(0x270, 8);

    PcodeFunction {
        blocks: vec![
            // Entry: bound = (uint32_t)rsi & 3; i = 0; acc = 0x1234; fall through header
            PcodeBasicBlock {
                index: 0,
                start_address: 0x2000,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x2000,
                        output: Some(len32.clone()),
                        inputs: vec![rsi, cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAnd,
                        address: 0x2000,
                        output: Some(bound32.clone()),
                        inputs: vec![len32, cst(3, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x2000,
                        output: Some(rbx.clone()),
                        inputs: vec![bound32],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Copy,
                        address: 0x2000,
                        output: Some(rcx.clone()),
                        inputs: vec![cst(0, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::Copy,
                        address: 0x2000,
                        output: Some(rdx.clone()),
                        inputs: vec![cst(0x1234, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // Header: if (i < bound) goto body else fallthrough exit
            PcodeBasicBlock {
                index: 1,
                start_address: 0x2010,
                successors: vec![2, 3],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLess,
                        address: 0x2010,
                        output: Some(cond.clone()),
                        inputs: vec![rcx.clone(), rbx],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x2014,
                        output: None,
                        inputs: vec![cst(0x2030, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            // Exit
            PcodeBasicBlock {
                index: 2,
                start_address: 0x2020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x2020,
                        output: Some(rax),
                        inputs: vec![rdx.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x2024,
                        output: None,
                        inputs: vec![ram_space.clone(), rdx.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // Body: acc = acc * 33 + buf[i]; i += 1; branch header
            PcodeBasicBlock {
                index: 3,
                start_address: 0x2030,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x2030,
                        output: Some(addr_t.clone()),
                        inputs: vec![rdi, rcx.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Load,
                        address: 0x2030,
                        output: Some(byte_t.clone()),
                        inputs: vec![ram_space.clone(), addr_t],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x2030,
                        output: Some(byte64.clone()),
                        inputs: vec![byte_t],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntMult,
                        address: 0x2030,
                        output: Some(sum_next.clone()),
                        inputs: vec![rdx.clone(), cst(33, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x2030,
                        output: Some(rdx),
                        inputs: vec![sum_next, byte64],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x2030,
                        output: Some(i_next.clone()),
                        inputs: vec![rcx.clone(), cst(1, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Copy,
                        address: 0x2030,
                        output: Some(rcx),
                        inputs: vec![i_next],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 7,
                        opcode: PcodeOpcode::Branch,
                        address: 0x2034,
                        output: None,
                        inputs: vec![cst(0x2010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    }
}

/// Owned SysV64 P-code fixture that reads and mutates an initialized memory buffer
/// and branches on a key bit.
fn build_ir_mem_transform_fixture() -> PcodeFunction {
    let rdi = reg(0x38, 8);
    let rsi = reg(0x30, 8);
    let rax = reg(0x00, 8);
    let ram_space = cst(0, 8);

    let key32 = uniq(0x300, 4);
    let b0 = uniq(0x310, 1);
    let key8 = uniq(0x320, 1);
    let new_b0 = reg(0x80, 1); // R8B written before read
    let ptr1 = uniq(0x340, 8);
    let b1 = uniq(0x350, 1);
    let new_b1 = reg(0x88, 1); // R9B written before read
    let bit = uniq(0x370, 4);
    let cond = uniq(0x380, 1);
    let b0_32 = reg(0x08, 4); // RCX cross-block live value
    let b1_32 = reg(0x10, 4); // RDX cross-block live value
    let b1_sh = uniq(0x3b0, 4);
    let tag_t = uniq(0x3c0, 4);
    let tag_f = uniq(0x3d0, 4);
    let tag_t64 = uniq(0x3e0, 8);
    let tag_f64 = uniq(0x3f0, 8);

    PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3000,
                successors: vec![1, 2],
                ops: vec![
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
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x3000,
                        output: Some(new_b0.clone()),
                        inputs: vec![b0, key8],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x3000,
                        output: Some(b0_32.clone()),
                        inputs: vec![new_b0.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
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
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3004,
                        output: Some(new_b1.clone()),
                        inputs: vec![b1, cst(5, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x3004,
                        output: Some(b1_32.clone()),
                        inputs: vec![new_b1.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
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
                ],
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

/// Branchless single-block fixture for verifying precondition rejections.
fn build_ir_single_block_copy_fixture() -> PcodeFunction {
    let rdi = reg(0x38, 8);
    let rax = reg(0x00, 8);
    PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x4000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4000,
                    output: Some(rax),
                    inputs: vec![rdi.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x4004,
                    output: None,
                    inputs: vec![cst(0, 8), rdi],
                    asm_mnemonic: None,
                },
            ],
        }],
    }
}

/// Fixture containing an unmodeled opcode (`CallOther`) for verifying
/// `Unsupported` is never turned into a pass.
fn build_ir_unsupported_callother_fixture() -> PcodeFunction {
    let rdi = reg(0x38, 8);
    let rax = reg(0x00, 8);
    let tmp = uniq(0x500, 8);
    PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x5000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CallOther,
                    address: 0x5000,
                    output: Some(tmp.clone()),
                    inputs: vec![cst(7, 4), rdi],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x5000,
                    output: Some(rax),
                    inputs: vec![tmp.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x5004,
                    output: None,
                    inputs: vec![cst(0, 8), tmp],
                    asm_mnemonic: None,
                },
            ],
        }],
    }
}

fn ir_render_options_x64() -> fission_decompiler::MlilPreviewOptions {
    let (_, x64_o0) = load_fixture_elf("metamorphic_x64_o0.elf");
    let mut options = fission_decompiler::seed_nir_render_options(&x64_o0);
    // `seed_nir_render_options` starts with `pe_x64_only = true`, while
    // `RustSleighDecompileConfig::cli_defaults()` enables ELF preview rendering
    // and loads SysV64 `.cspec` argument register offsets.
    options.pe_x64_only = false;
    options.calling_convention = fission_decompiler::CallingConvention::SystemVAmd64;
    options.cspec_param_offsets = Some(vec![0x38, 0x30, 0x10, 0x08, 0x80, 0x88]);
    options
}

fn eval_binary_func_pair(
    path: &Path,
    binary: &LoadedBinary,
    fn_name: &str,
    samples: &[MetamorphicSample],
) -> Result<Vec<(SemanticObservation, SemanticObservation)>, String> {
    let addr = entry_of(binary, fn_name);
    let facts = FactStore::from_binary(binary);
    let func_info = FunctionInfo {
        name: fn_name.to_string(),
        address: addr,
        ..Default::default()
    };
    let pair = decompile_one(binary, &facts, &func_info)
        .map_err(|e| format!("decompile {fn_name}: {e}"))?;
    let grounding = Grounding::for_function(binary, addr, &pair);
    let flow = call_sites(binary, addr).map_err(|e| format!("call_sites {fn_name}: {e:#}"))?;
    if !flow.calls.is_empty() || !flow.tail_jumps.is_empty() {
        return Err(format!("{fn_name} unexpectedly contains calls/tail_jumps"));
    }

    let mut harness =
        EmulatorHarness::build(path, Some(50_000)).map_err(|e| format!("emulator: {e:#}"))?;

    let mut out = Vec::with_capacity(samples.len());
    for s in samples {
        let scratch = harness
            .install_scratch(&s.initial_buffer)
            .map_err(|e| format!("install_scratch: {e:#}"))?;

        let args: Vec<u64> = match fn_name {
            "u32_branch_mix" | "diag_narrow_param_mix" => {
                vec![u64::from(s.arg0), u64::from(s.arg1)]
            }
            "u32_bounded_crc" | "u32_write_byte" | "diag_multi_slot_buf" => {
                vec![scratch, u64::from(s.arg0)]
            }
            "addr_sensitive_tag" => vec![scratch],
            _ => return Err(format!("unknown fixture function {fn_name}")),
        };

        if pair.hir.params.len() != args.len() {
            return Err(format!(
                "{fn_name}: HIR param count {} != ABI param count {}",
                pair.hir.params.len(),
                args.len()
            ));
        }

        let hir_args: Vec<i64> = args
            .iter()
            .zip(&pair.hir.params)
            .map(|(&w, p)| normalize(w as i64, &p.ty))
            .collect();
        let image: Vec<(u64, u8)> = s
            .initial_buffer
            .iter()
            .enumerate()
            .map(|(i, &b)| (scratch + i as u64, b))
            .collect();

        let hir_run = {
            let backing = |a: u64| harness.baseline_byte(a);
            run_hir(
                &pair.hir.body,
                &pair.hir.params,
                &pair.hir.locals,
                &hir_args,
                &World {
                    image: &image,
                    backing: Some(&backing),
                    globals: Some(&grounding.globals),
                    global_types: Some(&grounding.global_types),
                    ..World::default()
                },
            )
            .map_err(|e| format!("run_hir {fn_name}: {e:#}"))?
        };

        let hir_ret = hir_run
            .ret
            .ok_or_else(|| format!("HIR body {fn_name} returned None"))?;
        let mut hir_buf = s.initial_buffer;
        for (i, slot) in hir_buf.iter_mut().enumerate() {
            if let Some(&b) = hir_run.writes.get(&(scratch + i as u64)) {
                *slot = b;
            }
        }
        let hir_obs = SemanticObservation {
            return_value: hir_ret as u32,
            buffer: hir_buf,
        };

        let emu_run = harness
            .call_stubbed(addr, &args, &flow, &|_| 0)
            .map_err(|e| format!("call_stubbed {fn_name}: {e:#}"))?;
        let StubbedEnd::Returned(emu_ret) = emu_run.end else {
            return Err(format!(
                "emulator {fn_name} did not return normally: {:?}",
                emu_run.end
            ));
        };
        let mut emu_buf = s.initial_buffer;
        for (i, slot) in emu_buf.iter_mut().enumerate() {
            if let Some(&b) = emu_run.writes.get(&(scratch + i as u64)) {
                *slot = b;
            }
        }
        let emu_obs = SemanticObservation {
            return_value: emu_ret as u32,
            buffer: emu_buf,
        };

        out.push((emu_obs, hir_obs));
    }

    Ok(out)
}

fn check_binary_metamorphic_pair(
    relation: RelationKind,
    props: FixtureProperties,
    fn_name: &str,
    path_a: &Path,
    bin_a: &LoadedBinary,
    path_b: &Path,
    bin_b: &LoadedBinary,
    samples: &[MetamorphicSample],
) -> RelationOutcome {
    if let Err(reason) = check_binary_relation_precondition(relation, &props) {
        return RelationOutcome::RejectedPrecondition { reason };
    }

    let obs_a = match eval_binary_func_pair(path_a, bin_a, fn_name, samples) {
        Ok(v) => v,
        Err(reason) => return RelationOutcome::Unsupported { reason },
    };
    let obs_b = match eval_binary_func_pair(path_b, bin_b, fn_name, samples) {
        Ok(v) => v,
        Err(reason) => return RelationOutcome::Unsupported { reason },
    };

    let mut witnesses = Vec::new();
    for (idx, s) in samples.iter().enumerate() {
        let (ea, ha) = &obs_a[idx];
        let (eb, hb) = &obs_b[idx];
        if ea != ha || eb != hb || ea != eb || ha != hb {
            witnesses.push(MetamorphicWitness {
                sample_index: idx,
                sample: s.clone(),
                detail: format!(
                    "{fn_name}: a=(emu:{ea:?}, hir:{ha:?}) vs b=(emu:{eb:?}, hir:{hb:?})"
                ),
            });
        }
    }

    if witnesses.is_empty() {
        RelationOutcome::Passed {
            checked: samples.len(),
        }
    } else {
        RelationOutcome::Diverged { witnesses }
    }
}

fn run_ir_suite(samples: &[MetamorphicSample], manifest: &mut MetamorphicManifest) {
    let options = ir_render_options_x64();

    let fixtures = [
        (
            "ir_branch_mix",
            build_ir_branch_mix_fixture(),
            IrInputAbi::TwoScalars,
        ),
        (
            "ir_bounded_loop",
            build_ir_bounded_loop_fixture(),
            IrInputAbi::BufferAndScalar,
        ),
        (
            "ir_mem_transform",
            build_ir_mem_transform_fixture(),
            IrInputAbi::BufferAndScalar,
        ),
    ];

    for (name, pcode, abi) in &fixtures {
        let branch_variant = transform_branch_invert_swap(pcode).unwrap_or_else(|e| {
            panic!("{name}: transform_branch_invert_swap failed unexpectedly: {e}")
        });
        let branch_out =
            verify_ir_metamorphic_pair(name, pcode, &branch_variant, &options, *abi, samples, true);
        assert!(
            branch_out.is_passed(),
            "{name} IrBranchInvertSwap expected Passed, got {branch_out:?}"
        );
        manifest.record(RelationKind::IrBranchInvertSwap, "x86_64_ir", &branch_out);

        let id_variant = transform_identity_ops(pcode)
            .unwrap_or_else(|e| panic!("{name}: transform_identity_ops failed unexpectedly: {e}"));
        let id_out =
            verify_ir_metamorphic_pair(name, pcode, &id_variant, &options, *abi, samples, true);
        assert!(
            id_out.is_passed(),
            "{name} IrIdentityOps expected Passed, got {id_out:?}"
        );
        manifest.record(RelationKind::IrIdentityOps, "x86_64_ir", &id_out);

        let perm_variant = transform_edge_split_and_permute(pcode, DEFAULT_METAMORPHIC_SEED)
            .unwrap_or_else(|e| {
                panic!("{name}: transform_edge_split_and_permute failed unexpectedly: {e}")
            });
        assert_ne!(
            perm_variant.blocks.len(),
            pcode.blocks.len(),
            "{name}: expected edge-split block insertion"
        );
        let perm_out =
            verify_ir_metamorphic_pair(name, pcode, &perm_variant, &options, *abi, samples, true);
        assert!(
            perm_out.is_passed(),
            "{name} IrEdgeSplitBlockPermute expected Passed, got {perm_out:?}"
        );
        manifest.record(
            RelationKind::IrEdgeSplitBlockPermute,
            "x86_64_ir",
            &perm_out,
        );
    }
}

#[test]
fn ir_metamorphic_positive_relations_hold_across_owned_fixtures() {
    let samples = generate_smoke_samples(DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET);
    let mut manifest =
        MetamorphicManifest::new(DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET, "cargo test");
    run_ir_suite(&samples, &mut manifest);

    for rel in [
        RelationKind::IrBranchInvertSwap,
        RelationKind::IrIdentityOps,
        RelationKind::IrEdgeSplitBlockPermute,
    ] {
        let key = format!("{}::x86_64_ir", rel.as_str());
        let den = manifest.denominators.get(&key).expect("recorded bucket");
        assert_eq!(den.total, 3);
        assert_eq!(den.passed, 3);
        assert_eq!(den.diverged, 0);
        assert_eq!(den.unsupported, 0);
    }
}

#[test]
fn ir_metamorphic_negative_controls_and_precondition_rejections() {
    let samples = generate_smoke_samples(DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET);
    let options = ir_render_options_x64();

    // Negative control 1: flip branch predicate without swapping successors.
    let branch_mix = build_ir_branch_mix_fixture();
    let bad_branch = negative_flip_branch_without_swap(&branch_mix).expect("bad branch control");
    let bad_branch_out = verify_ir_metamorphic_pair(
        "ir_branch_mix_bad",
        &branch_mix,
        &bad_branch,
        &options,
        IrInputAbi::TwoScalars,
        &samples,
        false,
    );
    match bad_branch_out {
        RelationOutcome::Diverged { witnesses } => {
            assert!(
                !witnesses.is_empty(),
                "expected concrete divergence witness"
            );
        }
        other => panic!("expected Diverged for un-swapped branch flip, got {other:?}"),
    }

    // Negative control 2: perturb ALU op by `^ 1` instead of `^ 0`.
    let mem_tx = build_ir_mem_transform_fixture();
    let bad_arith = negative_perturb_arith_result(&mem_tx).expect("bad arith control");
    let bad_arith_out = verify_ir_metamorphic_pair(
        "ir_mem_transform_bad",
        &mem_tx,
        &bad_arith,
        &options,
        IrInputAbi::BufferAndScalar,
        &samples,
        false,
    );
    match bad_arith_out {
        RelationOutcome::Diverged { witnesses } => {
            assert!(
                !witnesses.is_empty(),
                "expected concrete divergence witness"
            );
        }
        other => panic!("expected Diverged for ^ 1 perturbation, got {other:?}"),
    }

    // Precondition rejections: branch-invert and edge-split on branchless single block.
    let branchless = build_ir_single_block_copy_fixture();
    let err_branch = transform_branch_invert_swap(&branchless)
        .expect_err("single-block branchless function must reject BranchInvertSwap");
    assert!(err_branch.contains("precondition"));

    let err_perm = transform_edge_split_and_permute(&branchless, DEFAULT_METAMORPHIC_SEED)
        .expect_err("single-block function must reject EdgeSplitBlockPermute");
    assert!(err_perm.contains("precondition"));

    // Unsupported opcodes must return Unsupported, never Passed.
    let callother = build_ir_unsupported_callother_fixture();
    let unsup_variant = branchless.clone();
    let unsup_out = verify_ir_metamorphic_pair(
        "ir_unsupported_callother",
        &callother,
        &unsup_variant,
        &options,
        IrInputAbi::TwoScalars,
        &samples,
        true,
    );
    assert!(
        matches!(unsup_out, RelationOutcome::Unsupported { .. }),
        "CallOther must report Unsupported, got {unsup_out:?}"
    );
}

#[test]
fn binary_metamorphic_opt_and_layout_variants_agree_with_ground_truth() {
    let samples = generate_smoke_samples(DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET);
    let (p_o0, bin_o0) = load_fixture_elf("metamorphic_x64_o0.elf");
    let (p_o2, bin_o2) = load_fixture_elf("metamorphic_x64_o2.elf");
    let (p_lay, bin_lay) = load_fixture_elf("metamorphic_x64_layout.elf");

    let pure_fns = ["u32_branch_mix", "u32_bounded_crc", "u32_write_byte"];

    // Verify address separation between O2 and Layout variants is real.
    let distinct_layout_addrs: BTreeSet<u64> = pure_fns
        .iter()
        .flat_map(|f| [entry_of(&bin_o2, f), entry_of(&bin_lay, f)])
        .collect();
    assert_eq!(
        distinct_layout_addrs.len(),
        pure_fns.len() * 2,
        "O2 and layout ELF variants must place functions at distinct addresses"
    );

    for fn_name in pure_fns {
        let opt_outcome = check_binary_metamorphic_pair(
            RelationKind::BinaryOptLevel,
            FixtureProperties::PURE_DEFINED,
            fn_name,
            &p_o0,
            &bin_o0,
            &p_o2,
            &bin_o2,
            &samples,
        );
        assert!(
            opt_outcome.is_passed(),
            "{fn_name} BinaryOptLevel expected Passed, got {opt_outcome:?}"
        );

        let layout_outcome = check_binary_metamorphic_pair(
            RelationKind::BinaryLayoutShift,
            FixtureProperties::PURE_DEFINED,
            fn_name,
            &p_o2,
            &bin_o2,
            &p_lay,
            &bin_lay,
            &samples,
        );
        assert!(
            layout_outcome.is_passed(),
            "{fn_name} BinaryLayoutShift expected Passed, got {layout_outcome:?}"
        );

        // Also cross-check existing ground-truth verifier on the layout variant.
        let addr = entry_of(&bin_lay, fn_name);
        let facts = FactStore::from_binary(&bin_lay);
        let func = FunctionInfo {
            name: fn_name.to_string(),
            address: addr,
            ..Default::default()
        };
        let pair = decompile_one(&bin_lay, &facts, &func).expect("decompile layout");
        let mut harness = EmulatorHarness::build(&p_lay, Some(100_000)).expect("harness");
        let gt = check_ground_truth_with(
            &mut harness,
            addr,
            &pair.prehir,
            &pair.hir,
            &RunPlan {
                runs: 16,
                ..RunPlan::default()
            },
            &Grounding::for_function(&bin_lay, addr, &pair),
        );
        assert!(
            matches!(gt, VerifyOutcome::Equivalent { checked } if checked > 0),
            "{fn_name} ground-truth check on layout variant expected Equivalent, got {gt:?}"
        );
    }

    // Binary negative control: corrupting a decompiled HIR body must trigger concrete divergence.
    let corrupted_addr = entry_of(&bin_o2, "u32_branch_mix");
    let facts_o2 = FactStore::from_binary(&bin_o2);
    let func_info = FunctionInfo {
        name: "u32_branch_mix".to_string(),
        address: corrupted_addr,
        ..Default::default()
    };
    let pair = decompile_one(&bin_o2, &facts_o2, &func_info).expect("decompile u32_branch_mix");
    let mut corrupted_hir = pair.hir.clone();
    corrupted_hir.body = vec![HirStmt::Return(Some(HirExpr::Const(
        0,
        corrupted_hir.return_type.clone(),
    )))];
    let mut harness = EmulatorHarness::build(&p_o2, Some(100_000)).expect("harness");
    let neg_gt = check_ground_truth_with(
        &mut harness,
        corrupted_addr,
        &pair.prehir,
        &corrupted_hir,
        &RunPlan {
            runs: 16,
            ..RunPlan::default()
        },
        &Grounding::for_function(&bin_o2, corrupted_addr, &pair),
    );
    assert!(
        matches!(neg_gt, VerifyOutcome::Diverged(_)),
        "corrupted binary HIR must be caught as Diverged, got {neg_gt:?}"
    );

    // Inapplicable transform control: `addr_sensitive_tag` must reject BinaryLayoutShift,
    // and we empirically confirm its O2 vs layout observations genuinely differ.
    let rej_outcome = check_binary_metamorphic_pair(
        RelationKind::BinaryLayoutShift,
        FixtureProperties::ADDRESS_SENSITIVE,
        "addr_sensitive_tag",
        &p_o2,
        &bin_o2,
        &p_lay,
        &bin_lay,
        &samples,
    );
    assert!(
        matches!(rej_outcome, RelationOutcome::RejectedPrecondition { .. }),
        "address-sensitive fixture must reject BinaryLayoutShift, got {rej_outcome:?}"
    );
    let uncapped_o2 = eval_binary_func_pair(&p_o2, &bin_o2, "addr_sensitive_tag", &samples[..2])
        .expect("eval addr_sensitive_tag O2");
    let uncapped_lay = eval_binary_func_pair(&p_lay, &bin_lay, "addr_sensitive_tag", &samples[..2])
        .expect("eval addr_sensitive_tag layout");
    assert_ne!(
        uncapped_o2[0].0, uncapped_lay[0].0,
        "layout shift must genuinely alter addr_sensitive_tag output (`g_anchor` shifted)"
    );
}

#[test]
fn diagnostic_known_decompiler_defects_are_caught_by_metamorphic_harness() {
    let samples = generate_smoke_samples(DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET);
    let (p_o0, bin_o0) = load_fixture_elf("metamorphic_x64_o0.elf");
    let (p_o2, bin_o2) = load_fixture_elf("metamorphic_x64_o2.elf");

    // Issue #1424: `diag_narrow_param_mix` at -O2 narrows `param_1` to `uchar` due to
    // `movzbl %dil` despite full 32-bit entry uses, and converts `shrl` into signed `/ 8`.
    // Emulator confirms O0 == O2 while O2 HIR diverges cleanly on concrete samples.
    let narrow_out = check_binary_metamorphic_pair(
        RelationKind::BinaryOptLevel,
        FixtureProperties::PURE_DEFINED,
        "diag_narrow_param_mix",
        &p_o0,
        &bin_o0,
        &p_o2,
        &bin_o2,
        &samples,
    );
    assert!(
        matches!(narrow_out, RelationOutcome::Diverged { ref witnesses } if !witnesses.is_empty()),
        "diag_narrow_param_mix should expose Issue #1424 as Diverged, got {narrow_out:?}"
    );

    // Issue #1425: `diag_multi_slot_buf` emits `slot_1 = ... (param_1) + 1` even after
    // parameter 0 is renamed `ptr`, failing `run_hir` with an unbound `param_1` read
    // (must remain Unsupported/Diverged, never Passed).
    let slot_out = check_binary_metamorphic_pair(
        RelationKind::BinaryOptLevel,
        FixtureProperties::PURE_DEFINED,
        "diag_multi_slot_buf",
        &p_o0,
        &bin_o0,
        &p_o2,
        &bin_o2,
        &samples,
    );
    assert!(
        matches!(
            slot_out,
            RelationOutcome::Unsupported { .. } | RelationOutcome::Diverged { .. }
        ),
        "diag_multi_slot_buf should expose Issue #1425 (not coerce to Passed), got {slot_out:?}"
    );

    // Issue #1426: wrapping a 1-byte ALU op preceding `IntZExt` with an identity op
    // (`tmp ^ 0` / `tmp + 0`) causes signed `char` sign-extension in HIR even though
    // `eval_pcode_tiny_model` proves original and variant P-code agree.
    let mem_ir = build_ir_mem_transform_fixture();
    let mem_subword_id =
        transform_identity_ops_including_subword(&mem_ir).expect("subword identity variant");
    let subword_out = verify_ir_metamorphic_pair(
        "ir_mem_transform_subword_id",
        &mem_ir,
        &mem_subword_id,
        &ir_render_options_x64(),
        IrInputAbi::BufferAndScalar,
        &samples,
        true,
    );
    assert!(
        matches!(subword_out, RelationOutcome::Diverged { ref witnesses } if !witnesses.is_empty()),
        "subword IntZExt identity variant should expose Issue #1426 as Diverged, got {subword_out:?}"
    );
}

struct TempDirGuard(PathBuf);
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn parse_native_json_observation(stdout: &str) -> SemanticObservation {
    let val: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid native fixture JSON [ret,[b0,b1,b2,b3]]");
    let arr = val.as_array().expect("top-level JSON array");
    let ret = arr[0].as_u64().expect("u32 return value") as u32;
    let buf_arr = arr[1].as_array().expect("4-byte buffer array");
    let buffer = [
        buf_arr[0].as_u64().expect("b0") as u8,
        buf_arr[1].as_u64().expect("b1") as u8,
        buf_arr[2].as_u64().expect("b2") as u8,
        buf_arr[3].as_u64().expect("b3") as u8,
    ];
    SemanticObservation {
        return_value: ret,
        buffer,
    }
}

fn exec_native_fixture(
    bin: &Path,
    fn_name: &str,
    sample: &MetamorphicSample,
) -> SemanticObservation {
    let output = Command::new(bin)
        .arg(fn_name)
        .arg(sample.arg0.to_string())
        .arg(sample.arg1.to_string())
        .arg(sample.initial_buffer[0].to_string())
        .arg(sample.initial_buffer[1].to_string())
        .arg(sample.initial_buffer[2].to_string())
        .arg(sample.initial_buffer[3].to_string())
        .output()
        .unwrap_or_else(|e| panic!("execute native {}: {e}", bin.display()));
    assert!(
        output.status.success(),
        "native execution failed for {} {fn_name}: {}",
        bin.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_native_json_observation(&String::from_utf8_lossy(&output.stdout))
}

#[test]
fn native_host_fixture_pairs_establish_relation_independently() {
    let cc_ok = Command::new("cc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(
        cc_ok,
        "host C compiler `cc` is required for independent native metamorphic fixture execution"
    );

    let unique = format!(
        "fission-metamorphic-host-{}-{}",
        std::process::id(),
        DEFAULT_METAMORPHIC_SEED
    );
    let tmp = TempDirGuard(std::env::temp_dir().join(unique));
    std::fs::create_dir_all(&tmp.0).expect("create temp dir");

    let src = testdata_dir().join("metamorphic_fixture.c");
    let native_o0 = tmp.0.join("host_o0");
    let native_o2 = tmp.0.join("host_o2");
    let native_layout = tmp.0.join("host_layout");

    for (out, flags) in [
        (&native_o0, vec!["-O0", "-fno-inline"]),
        (&native_o2, vec!["-O2", "-fno-inline", "-fno-unroll-loops"]),
        (
            &native_layout,
            vec![
                "-O2",
                "-fno-inline",
                "-fno-unroll-loops",
                "-DFISSION_LAYOUT_SHIFT=1",
            ],
        ),
    ] {
        let status = Command::new("cc")
            .args(&flags)
            .arg(&src)
            .arg("-o")
            .arg(out)
            .status()
            .expect("invoke cc");
        assert!(
            status.success(),
            "cc compilation failed for {}",
            out.display()
        );
    }

    let samples = generate_smoke_samples(DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET);
    let host_isa = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let mut host_den_opt = fission_dir::metamorphic::RelationDenominator::default();

    for fn_name in ["u32_branch_mix", "u32_bounded_crc", "u32_write_byte"] {
        for s in &samples {
            let obs_o0 = exec_native_fixture(&native_o0, fn_name, s);
            let obs_o2 = exec_native_fixture(&native_o2, fn_name, s);
            let obs_lay = exec_native_fixture(&native_layout, fn_name, s);
            assert_eq!(
                obs_o0, obs_o2,
                "native {fn_name} O0 vs O2 mismatch on {s:?}"
            );
            assert_eq!(
                obs_o2, obs_lay,
                "native {fn_name} O2 vs layout mismatch on {s:?}"
            );
        }

        // Exercise each Fission host binary decompilation against its own native result,
        // separating supported checks from Unsupported/Diverged outcomes transparently.
        let check_one_host_bin = |bin_path: &Path| -> RelationOutcome {
            let Ok(host_bin) = LoadedBinary::from_file(bin_path) else {
                return RelationOutcome::Unsupported {
                    reason: format!("host binary loader failed for {}", bin_path.display()),
                };
            };
            let addr = entry_of(&host_bin, fn_name);
            let facts = FactStore::from_binary(&host_bin);
            let func_info = FunctionInfo {
                name: fn_name.to_string(),
                address: addr,
                ..Default::default()
            };
            let Ok(pair) = decompile_one(&host_bin, &facts, &func_info) else {
                return RelationOutcome::Unsupported {
                    reason: format!("host decompile failed for {fn_name}"),
                };
            };
            if pair.hir.params.len() != 2 {
                return RelationOutcome::Unsupported {
                    reason: format!(
                        "{host_isa} {fn_name}: recovered {} params (expected 2)",
                        pair.hir.params.len()
                    ),
                };
            }
            let scratch = 0x7000_0000u64;
            let mut witnesses = Vec::new();
            for (idx, s) in samples.iter().enumerate() {
                let args: [u64; 2] = match fn_name {
                    "u32_branch_mix" => [u64::from(s.arg0), u64::from(s.arg1)],
                    _ => [scratch, u64::from(s.arg0)],
                };
                let hir_args: Vec<i64> = args
                    .iter()
                    .zip(&pair.hir.params)
                    .map(|(&w, p)| normalize(w as i64, &p.ty))
                    .collect();
                let image: Vec<(u64, u8)> = s
                    .initial_buffer
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| (scratch + i as u64, b))
                    .collect();
                let Ok(run) = run_hir(
                    &pair.hir.body,
                    &pair.hir.params,
                    &pair.hir.locals,
                    &hir_args,
                    &World {
                        image: &image,
                        ..World::default()
                    },
                ) else {
                    return RelationOutcome::Unsupported {
                        reason: format!("{host_isa} {fn_name}: run_hir unsupported construct"),
                    };
                };
                let Some(ret) = run.ret else {
                    return RelationOutcome::Unsupported {
                        reason: format!("{host_isa} {fn_name}: HIR returned None"),
                    };
                };
                let mut hir_buf = s.initial_buffer;
                for (i, slot) in hir_buf.iter_mut().enumerate() {
                    if let Some(&b) = run.writes.get(&(scratch + i as u64)) {
                        *slot = b;
                    }
                }
                let hir_obs = SemanticObservation {
                    return_value: ret as u32,
                    buffer: hir_buf,
                };
                let native_obs = exec_native_fixture(bin_path, fn_name, s);
                if hir_obs != native_obs {
                    witnesses.push(MetamorphicWitness {
                        sample_index: idx,
                        sample: s.clone(),
                        detail: format!(
                            "{host_isa} {fn_name}: hir={hir_obs:?} != native={native_obs:?}"
                        ),
                    });
                }
            }
            if witnesses.is_empty() {
                RelationOutcome::Passed {
                    checked: samples.len(),
                }
            } else {
                RelationOutcome::Diverged { witnesses }
            }
        };

        let r0 = check_one_host_bin(&native_o0);
        let r2 = check_one_host_bin(&native_o2);
        let rl = check_one_host_bin(&native_layout);
        host_den_opt.record(&r0);
        host_den_opt.record(&r2);
        host_den_opt.record(&rl);
        eprintln!(
            "Host native vs Fission ({host_isa}) `{fn_name}`: o0={} o2={} layout={}",
            r0.category(),
            r2.category(),
            rl.category()
        );
    }
    assert_eq!(host_den_opt.total, 9);
    assert_eq!(
        host_den_opt.diverged, 0,
        "host baseline fixtures diverged against native execution: {host_den_opt:?}"
    );
}

fn build_smoke_manifest() -> MetamorphicManifest {
    let samples = generate_smoke_samples(DEFAULT_METAMORPHIC_SEED, DEFAULT_SMOKE_BUDGET);
    let mut manifest = MetamorphicManifest::new(
        DEFAULT_METAMORPHIC_SEED,
        DEFAULT_SMOKE_BUDGET,
        "cargo test -p fission-dir --test metamorphic_relations -- --nocapture",
    );

    let fixture_specs = [
        ("metamorphic_fixture.c", "c_source", "metamorphic_fixture.c"),
        ("x64_o0", "x86_64", "metamorphic_x64_o0.elf"),
        ("x64_o2", "x86_64", "metamorphic_x64_o2.elf"),
        ("x64_layout", "x86_64", "metamorphic_x64_layout.elf"),
        ("aarch64_o0", "aarch64", "metamorphic_aarch64_o0.elf"),
        ("aarch64_o2", "aarch64", "metamorphic_aarch64_o2.elf"),
        (
            "aarch64_layout",
            "aarch64",
            "metamorphic_aarch64_layout.elf",
        ),
    ];
    for (name, isa, file) in fixture_specs {
        let p = testdata_dir().join(file);
        manifest.artifacts.push(ArtifactDigest {
            name: name.to_string(),
            isa: isa.to_string(),
            rel_path: format!("crates/fission-dir/testdata/{file}"),
            sha256: sha256_file(&p).expect("hash artifact"),
        });
    }

    run_ir_suite(&samples, &mut manifest);

    let (p_o0, bin_o0) = load_fixture_elf("metamorphic_x64_o0.elf");
    let (p_o2, bin_o2) = load_fixture_elf("metamorphic_x64_o2.elf");
    let (p_lay, bin_lay) = load_fixture_elf("metamorphic_x64_layout.elf");

    for fn_name in ["u32_branch_mix", "u32_bounded_crc", "u32_write_byte"] {
        let opt_out = check_binary_metamorphic_pair(
            RelationKind::BinaryOptLevel,
            FixtureProperties::PURE_DEFINED,
            fn_name,
            &p_o0,
            &bin_o0,
            &p_o2,
            &bin_o2,
            &samples,
        );
        manifest.record(RelationKind::BinaryOptLevel, "x86_64", &opt_out);

        let lay_out = check_binary_metamorphic_pair(
            RelationKind::BinaryLayoutShift,
            FixtureProperties::PURE_DEFINED,
            fn_name,
            &p_o2,
            &bin_o2,
            &p_lay,
            &bin_lay,
            &samples,
        );
        manifest.record(RelationKind::BinaryLayoutShift, "x86_64", &lay_out);
    }

    // Record the rejected inapplicable transform explicitly in its denominator.
    let rej_out = check_binary_metamorphic_pair(
        RelationKind::BinaryLayoutShift,
        FixtureProperties::ADDRESS_SENSITIVE,
        "addr_sensitive_tag",
        &p_o2,
        &bin_o2,
        &p_lay,
        &bin_lay,
        &samples,
    );
    manifest.record(RelationKind::BinaryLayoutShift, "x86_64", &rej_out);

    // Explicitly probe AArch64 ELF variants under separate per-ISA denominators.
    let (p_a0, bin_a0) = load_fixture_elf("metamorphic_aarch64_o0.elf");
    let (p_a2, bin_a2) = load_fixture_elf("metamorphic_aarch64_o2.elf");
    let (p_al, bin_al) = load_fixture_elf("metamorphic_aarch64_layout.elf");
    for fn_name in ["u32_branch_mix", "u32_bounded_crc", "u32_write_byte"] {
        let opt_out = check_binary_metamorphic_pair(
            RelationKind::BinaryOptLevel,
            FixtureProperties::PURE_DEFINED,
            fn_name,
            &p_a0,
            &bin_a0,
            &p_a2,
            &bin_a2,
            &samples[..4],
        );
        manifest.record(RelationKind::BinaryOptLevel, "aarch64", &opt_out);

        let lay_out = check_binary_metamorphic_pair(
            RelationKind::BinaryLayoutShift,
            FixtureProperties::PURE_DEFINED,
            fn_name,
            &p_a2,
            &bin_a2,
            &p_al,
            &bin_al,
            &samples[..4],
        );
        manifest.record(RelationKind::BinaryLayoutShift, "aarch64", &lay_out);
    }

    manifest
}

#[test]
fn metamorphic_manifest_is_deterministic_with_per_relation_isa_denominators() {
    let m1 = build_smoke_manifest();
    let m2 = build_smoke_manifest();
    let j1 = m1.to_json_string();
    let j2 = m2.to_json_string();

    assert_eq!(j1, j2, "manifest JSON must be deterministic for fixed seed");
    assert!(j1.contains("x86_64_ir"));
    assert!(j1.contains("binary_opt_level::x86_64"));
    assert!(j1.contains("binary_layout_shift::x86_64"));
    assert!(j1.contains("binary_opt_level::aarch64"));
    assert_eq!(
        m1.denominators["binary_opt_level::x86_64"].passed, 3,
        "all 3 x86_64 opt-level pairs must pass"
    );
    assert_eq!(
        m1.denominators["binary_layout_shift::x86_64"].rejected_precondition, 1,
        "addr_sensitive_tag layout shift must record 1 RejectedPrecondition"
    );
    assert_eq!(
        m1.denominators["binary_opt_level::aarch64"].diverged, 0,
        "AArch64 probe must never coerce or silently diverge"
    );
    assert_eq!(
        m1.denominators["binary_layout_shift::aarch64"].diverged, 0,
        "AArch64 layout probe must never coerce or silently diverge"
    );
    eprintln!("=== Issue #1253 Metamorphic Manifest ===\n{j1}");
}
