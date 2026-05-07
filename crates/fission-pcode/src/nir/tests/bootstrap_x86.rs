use super::*;
use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

#[test]
fn preview_supports_pe_x86_single_block() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x401000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x401000,
                output: None,
                inputs: vec![cst(0, 4), cst(7, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let code = render_mlil_preview(&func, "x86_ret", 0x401000, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return 7;"), "{code}");
}

#[test]
fn preview_supports_pe_x86_multiblock_direct_target_branch() {
    let cond = uniq(0x360, 1);
    let direct_target = Varnode {
        space_id: 1,
        offset: 0x4020,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4000,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4001,
                        output: None,
                        inputs: vec![direct_target, cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4010,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "x86_branchy", 0x4000, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return 0;"), "{code}");
    assert!(code.contains("return 1;"), "{code}");
}

#[test]
fn preview_names_x86_general_purpose_registers() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x402000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x402000,
                output: None,
                inputs: vec![cst(0, 4), reg(0x00, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let code = render_mlil_preview(&func, "x86_reg", 0x402000, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return eax;"), "{code}");
}

#[test]
fn preview_x64_ret_stack_target_is_not_return_value() {
    let ret_target = uniq(0x500, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140002000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Load,
                    address: 0x140002000,
                    output: Some(ret_target.clone()),
                    inputs: vec![cst(0, 8), reg(0x20, 8)],
                    asm_mnemonic: Some("MOV RAX,qword ptr [RSP]".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140002001,
                    output: None,
                    inputs: vec![cst(0, 8), ret_target],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "x64_void_ret", 0x140002000, &preview_options())
        .expect("preview render");
    assert!(code.contains("void x64_void_ret()"), "{code}");
    assert!(code.contains("return;"), "{code}");
    assert!(!code.contains("return *"), "{code}");
    assert!(!code.contains("var_"), "{code}");
}

#[test]
fn preview_x64_ret_prefers_abi_return_register_over_stack_target() {
    let ret_target = uniq(0x508, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140002100,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140002100,
                    output: Some(reg(0x00, 8)),
                    inputs: vec![cst(42, 8)],
                    asm_mnemonic: Some("MOV RAX,0x2a".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Load,
                    address: 0x140002101,
                    output: Some(ret_target.clone()),
                    inputs: vec![cst(0, 8), reg(0x20, 8)],
                    asm_mnemonic: Some("MOV RCX,qword ptr [RSP]".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x140002102,
                    output: None,
                    inputs: vec![cst(0, 8), ret_target],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "x64_value_ret", 0x140002100, &preview_options())
        .expect("preview render");
    assert!(code.contains("return 42;"), "{code}");
    assert!(!code.contains("return *"), "{code}");
}

#[test]
fn preview_x64_ret_recovers_single_predecessor_return_register() {
    let ret_target = uniq(0x510, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x140002200,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x140002200,
                        output: Some(reg(0x00, 8)),
                        inputs: vec![cst(7, 8)],
                        asm_mnemonic: Some("MOV RAX,7".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x140002207,
                        output: None,
                        inputs: vec![cst(0x140002210, 8)],
                        asm_mnemonic: Some("JMP 0x140002210".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x140002210,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Load,
                        address: 0x140002210,
                        output: Some(ret_target.clone()),
                        inputs: vec![cst(0, 8), reg(0x20, 8)],
                        asm_mnemonic: Some("MOV RCX,qword ptr [RSP]".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x140002211,
                        output: None,
                        inputs: vec![cst(0, 8), ret_target],
                        asm_mnemonic: Some("RET".to_string()),
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "x64_predecessor_value_ret",
        0x140002200,
        &preview_options(),
    )
    .expect("preview render");
    assert!(code.contains("return 7;"), "{code}");
    assert!(!code.contains("return;"), "{code}");
    assert!(!code.contains("return *"), "{code}");
}

#[test]
fn preview_x64_ret_recovers_predecessor_computed_return_register() {
    let ret_target = uniq(0x518, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x140002300,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x140002300,
                        output: Some(reg(0x00, 4)),
                        inputs: vec![reg(0x08, 4), cst(5, 4)],
                        asm_mnemonic: Some("LEA EAX,[RCX+5]".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x140002303,
                        output: None,
                        inputs: vec![cst(0x140002310, 8)],
                        asm_mnemonic: Some("JMP 0x140002310".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x140002310,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Load,
                        address: 0x140002310,
                        output: Some(ret_target.clone()),
                        inputs: vec![cst(0, 8), reg(0x20, 8)],
                        asm_mnemonic: Some("MOV RCX,qword ptr [RSP]".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x140002311,
                        output: None,
                        inputs: vec![cst(0, 8), ret_target],
                        asm_mnemonic: Some("RET".to_string()),
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "x64_predecessor_computed_value_ret",
        0x140002300,
        &preview_options(),
    )
    .expect("preview render");
    assert!(code.contains("return param_1 + 5;"), "{code}");
    assert!(!code.contains("return;"), "{code}");
    assert!(!code.contains("return *"), "{code}");
}

#[test]
fn preview_uses_entry_register_alias_for_non_abi_register() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x140001000,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x140001000,
                        output: Some(runtime_reg(0x38, 4)),
                        inputs: vec![runtime_reg(0x08, 4)],
                        asm_mnemonic: Some("MOV EDI,ECX".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x140001002,
                        output: None,
                        inputs: vec![cst(0x140001010, 8)],
                        asm_mnemonic: Some("JMP 0x140001010".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x140001010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001010,
                    output: None,
                    inputs: vec![cst(0, 8), runtime_reg(0x38, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "win64_entry_alias", 0x140001000, &options)
        .expect("preview render");
    assert!(
        code.contains("uint win64_entry_alias(uint param_1)"),
        "{code}"
    );
    assert!(code.contains("return param_1;"), "{code}");
    assert!(
        !code.contains("return rdi;") && !code.contains("return edi;"),
        "{code}"
    );
}

#[test]
fn preview_inlines_lea_register_return() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = |offset, size| Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001450,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntMult,
                    address: 0x140001450,
                    output: Some(tmp(0x1000, 8)),
                    inputs: vec![runtime_reg(0x10, 8), cst(1, 8)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x140001450,
                    output: Some(tmp(0x1008, 8)),
                    inputs: vec![runtime_reg(0x08, 8), tmp(0x1000, 8)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x140001450,
                    output: Some(runtime_reg(0x00, 4)),
                    inputs: vec![tmp(0x1008, 8), cst(0, 4)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x140001450,
                    output: Some(runtime_reg(0x00, 8)),
                    inputs: vec![runtime_reg(0x00, 4)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Load,
                    address: 0x140001453,
                    output: Some(runtime_reg(0x288, 8)),
                    inputs: vec![cst(3, 8), runtime_reg(0x20, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x140001453,
                    output: Some(runtime_reg(0x20, 8)),
                    inputs: vec![runtime_reg(0x20, 8), cst(8, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001453,
                    output: None,
                    inputs: vec![cst(3, 8), runtime_reg(0x288, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "lea_add", 0x140001450, &options).expect("preview render");
    assert!(
        code.contains("lea_add(ulonglong param_1, ulonglong param_2)"),
        "{code}"
    );
    assert!(code.contains("param_1 + param_2"), "{code}");
    assert!(!code.contains("*var_"), "{code}");
}

#[test]
fn preview_inlines_read_modify_write_register_return() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = |offset, size| Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001850,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntMult,
                    address: 0x140001853,
                    output: Some(tmp(0x9300, 8)),
                    inputs: vec![runtime_reg(0x08, 8), cst(4, 8)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x140001853,
                    output: Some(tmp(0x9500, 8)),
                    inputs: vec![runtime_reg(0x08, 8), tmp(0x9300, 8)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x140001853,
                    output: Some(runtime_reg(0x00, 4)),
                    inputs: vec![tmp(0x9500, 8), cst(0, 4)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x140001853,
                    output: Some(runtime_reg(0x00, 8)),
                    inputs: vec![runtime_reg(0x00, 4)],
                    asm_mnemonic: Some("LEA".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x140001856,
                    output: Some(runtime_reg(0x00, 4)),
                    inputs: vec![runtime_reg(0x00, 4), runtime_reg(0x00, 4)],
                    asm_mnemonic: Some("ADD EAX,EAX".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x140001856,
                    output: Some(runtime_reg(0x00, 8)),
                    inputs: vec![runtime_reg(0x00, 4)],
                    asm_mnemonic: Some("ADD EAX,EAX".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Load,
                    address: 0x140001863,
                    output: Some(runtime_reg(0x288, 8)),
                    inputs: vec![cst(3, 8), runtime_reg(0x20, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001863,
                    output: None,
                    inputs: vec![cst(3, 8), runtime_reg(0x288, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "lea_double", 0x140001850, &options).expect("preview render");
    assert!(code.contains("param_1"), "{code}");
    assert!(!code.contains("tmp_0"), "{code}");
    assert!(!code.contains("*var_"), "{code}");
}

#[test]
fn preview_projects_narrow_read_from_wide_register_write() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001900,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001900,
                    output: Some(runtime_reg(0x10, 8)),
                    inputs: vec![cst(0, 8)],
                    asm_mnemonic: Some("MOV EDX,0".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001905,
                    output: Some(runtime_reg(0x00, 4)),
                    inputs: vec![runtime_reg(0x10, 4)],
                    asm_mnemonic: Some("MOV EAX,EDX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x140001905,
                    output: Some(runtime_reg(0x00, 8)),
                    inputs: vec![runtime_reg(0x00, 4)],
                    asm_mnemonic: Some("MOV EAX,EDX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Load,
                    address: 0x140001906,
                    output: Some(runtime_reg(0x288, 8)),
                    inputs: vec![cst(3, 8), runtime_reg(0x20, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001906,
                    output: None,
                    inputs: vec![cst(3, 8), runtime_reg(0x288, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "wide_to_narrow", 0x140001900, &options)
        .expect("preview render");
    assert!(code.contains("return 0;"), "{code}");
    assert!(!code.contains("param_2"), "{code}");
}

#[test]
fn preview_projects_wide_read_from_zero_extending_narrow_register_write() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001920,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001920,
                    output: Some(runtime_reg(0x10, 4)),
                    inputs: vec![cst(7, 4)],
                    asm_mnemonic: Some("MOV EDX,7".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001925,
                    output: Some(runtime_reg(0x00, 8)),
                    inputs: vec![runtime_reg(0x10, 8)],
                    asm_mnemonic: Some("MOV RAX,RDX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001926,
                    output: None,
                    inputs: vec![cst(0, 8), runtime_reg(0x00, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "narrow_to_wide", 0x140001920, &options)
        .expect("preview render");
    assert!(
        code.contains("return 7;") || code.contains("return (ulonglong)7;"),
        "{code}"
    );
    assert!(!code.contains("param_2"), "{code}");
    assert!(!code.contains("return rdx;"), "{code}");
}

#[test]
fn preview_recovers_stack_slot_from_rust_sleigh_rsp_space() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let rsp = runtime_reg(0x20, 8);
    let rax = runtime_reg(0x00, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001940,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x140001940,
                    output: Some(rsp.clone()),
                    inputs: vec![rsp.clone(), cst(8, 8)],
                    asm_mnemonic: Some("PUSH RBX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x140001940,
                    output: None,
                    inputs: vec![cst(3, 8), rsp.clone(), cst(42, 8)],
                    asm_mnemonic: Some("PUSH RBX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Load,
                    address: 0x140001941,
                    output: Some(rax.clone()),
                    inputs: vec![cst(3, 8), rsp.clone()],
                    asm_mnemonic: Some("MOV RAX,qword ptr [RSP]".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001942,
                    output: None,
                    inputs: vec![cst(3, 8), rax],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "rust_sleigh_rsp_slot", 0x140001940, &options)
        .expect("preview render");
    assert!(!code.contains("var_20"), "{code}");
    assert!(!code.contains("undefined *"), "{code}");
    assert!(
        code.contains("local_") || code.contains("return 42;"),
        "{code}"
    );
}

#[test]
fn preview_lowers_register_xor_self_to_zero() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let r12 = runtime_reg(0xa0, 8);
    let rax = runtime_reg(0x00, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001960,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntXor,
                    address: 0x140001960,
                    output: Some(r12.clone()),
                    inputs: vec![r12.clone(), r12],
                    asm_mnemonic: Some("XOR R12D,R12D".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001962,
                    output: Some(rax.clone()),
                    inputs: vec![runtime_reg(0xa0, 8)],
                    asm_mnemonic: Some("MOV RAX,R12".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001963,
                    output: None,
                    inputs: vec![cst(3, 8), rax],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "xor_self_zero", 0x140001960, &options).expect("preview render");
    assert!(code.contains("return 0;"), "{code}");
    assert!(!code.contains("r12"), "{code}");
}

#[test]
fn preview_projects_cross_space_gpr32_write_to_rust_sleigh_gpr64_read() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let unique_reg = |index: u64, size| Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: crate::arch::x86::X86_REG_BASE + index * 8,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let ebp = unique_reg(5, 4);
    let rbp = runtime_reg(0x28, 8);
    let rax = runtime_reg(0x00, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001970,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001970,
                    output: Some(ebp),
                    inputs: vec![cst(9, 4)],
                    asm_mnemonic: Some("MOV EBP,9".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001974,
                    output: Some(rax.clone()),
                    inputs: vec![rbp.clone()],
                    asm_mnemonic: Some("MOV RAX,RBP".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001975,
                    output: None,
                    inputs: vec![cst(3, 8), rax],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "cross_space_gpr_alias", 0x140001970, &options)
        .expect("preview render");
    assert!(
        code.contains("return 9;") || code.contains("return (ulonglong)9;"),
        "{code}"
    );
    assert!(!code.contains("rbp"), "{code}");
}

fn preview_structures_intra_instruction_conditional_return_copy() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = |offset, size| Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x140001460,
                successors: vec![2, 1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLess,
                        address: 0x140001460,
                        output: Some(tmp(0x2000, 1)),
                        inputs: vec![runtime_reg(0x08, 4), runtime_reg(0x10, 4)],
                        asm_mnemonic: Some("CMP".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Copy,
                        address: 0x140001462,
                        output: Some(runtime_reg(0x00, 4)),
                        inputs: vec![runtime_reg(0x10, 4)],
                        asm_mnemonic: Some("CMOV".to_string()),
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x140001464,
                        output: None,
                        inputs: vec![tmp(0x140001467, 8), tmp(0x2000, 1)],
                        asm_mnemonic: Some("CBRANCH".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x140001464,
                successors: vec![2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001464,
                    output: Some(runtime_reg(0x00, 4)),
                    inputs: vec![runtime_reg(0x08, 4)],
                    asm_mnemonic: Some("CMOV".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x140001467,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Load,
                        address: 0x140001467,
                        output: Some(runtime_reg(0x288, 8)),
                        inputs: vec![cst(3, 8), runtime_reg(0x20, 8)],
                        asm_mnemonic: Some("RET".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x140001467,
                        output: None,
                        inputs: vec![cst(3, 8), runtime_reg(0x288, 8)],
                        asm_mnemonic: Some("RET".to_string()),
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(&func, "conditional_max", 0x140001460, &options)
        .expect("preview render");
    assert!(code.contains("if (param_1 < param_2)"), "{code}");
    assert!(code.contains("return param_2;"), "{code}");
    assert!(code.contains("return param_1;"), "{code}");
    assert!(!code.contains("uVar"), "{code}");
    assert!(!code.contains("*var_"), "{code}");
}

#[test]
fn preview_suppresses_entrypoint_register_alias_params() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001000,
                    output: Some(runtime_reg(0x38, 4)),
                    inputs: vec![runtime_reg(0x08, 4)],
                    asm_mnemonic: Some("MOV EDI,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001002,
                    output: None,
                    inputs: vec![cst(0, 8), runtime_reg(0x38, 4)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    let binary = LoadedBinaryBuilder::new("entry.exe".to_string(), DataBuffer::Heap(vec![0; 64]))
        .format("PE64")
        .entry_point(0x140001000)
        .image_base(0x140000000)
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x140001000,
            virtual_size: 0x1000,
            file_offset: 0,
            file_size: 64,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("test binary builds");

    let code = render_mlil_preview_with_binary_and_context(
        &func,
        "entrypoint",
        0x140001000,
        &options,
        Some(&binary),
        None,
    )
    .expect("preview render");
    assert!(code.contains("entrypoint()"), "{code}");
    assert!(!code.contains("param_1"), "{code}");
}

#[test]
fn preview_tolerates_branchind_without_targets() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x405000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::BranchInd,
                address: 0x405000,
                output: None,
                inputs: vec![reg(0x00, 4)],
                asm_mnemonic: Some("JMP EAX".to_string()),
            }],
        }],
    };

    let code = render_mlil_preview(
        &func,
        "x86_branchind_unsupported",
        0x405000,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_branchind("), "{code}");
}

#[test]
fn preview_branchind_with_successors_sets_default_target() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405010,
                successors: vec![1, 2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405010,
                    output: None,
                    inputs: vec![reg(0x00, 4)],
                    asm_mnemonic: Some("JMP EAX".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x405030,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405030,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Switch {
            targets,
            default_target,
            ..
        } => {
            assert_eq!(targets, vec![0x405020, 0x405030]);
            assert_eq!(default_target, Some(0x405020));
        }
        other => panic!("expected switch terminator, got {other:?}"),
    }
}

#[test]
fn preview_branchind_with_duplicate_successors_preserves_case_ordinals() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405110,
                successors: vec![1, 1, 2, 1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405110,
                    output: None,
                    inputs: vec![reg(0x00, 4)],
                    asm_mnemonic: Some("JMP EAX".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405120,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405120,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x405130,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405130,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Switch {
            targets,
            default_target,
            ..
        } => {
            assert_eq!(targets, vec![0x405120, 0x405120, 0x405130, 0x405120]);
            assert_eq!(default_target, Some(0x405120));
        }
        other => panic!("expected switch terminator, got {other:?}"),
    }
}

#[test]
fn preview_branchind_without_successors_recovers_constant_target() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405100,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405100,
                    output: None,
                    inputs: vec![cst(0x405120, 8)],
                    asm_mnemonic: Some("JMP [CONST]".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405120,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405120,
                    output: None,
                    inputs: vec![cst(0, 4), cst(2, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } => {
            assert_eq!(evidence.surface, IndirectControlSurface::DispatcherLike);
            assert_eq!(
                evidence.failure_family,
                UnsupportedControlFamily::NonStructuralDispatcher
            );
            assert_eq!(evidence.successor_targets, vec![0x405120]);
            assert!(target_expr.is_some());
        }
        other => panic!("expected dispatcher surface, got {other:?}"),
    }
}

#[test]
fn preview_tolerates_unresolved_direct_branch_target() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x406000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Branch,
                address: 0x406000,
                output: None,
                inputs: vec![cst(0x405000, 8)],
                asm_mnemonic: Some("JMP 0x405000".to_string()),
            }],
        }],
    };

    let code = render_mlil_preview(
        &func,
        "x86_unresolved_direct_branch",
        0x406000,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_branchind("), "{code}");
}

#[test]
fn preview_known_forward_external_direct_branch_becomes_tail_call() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x406000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Branch,
                address: 0x406000,
                output: None,
                inputs: vec![cst(0x407000, 8)],
                asm_mnemonic: Some("JMP 0x407000".to_string()),
            }],
        }],
    };
    let mut context = PreviewTypeContext::default();
    context.call_target_refs.insert(
        0x407000,
        CallTargetRef {
            address: Some(0x407000),
            symbol: "external_tail".to_string(),
            provenance: CallTargetProvenance::Direct,
            edge_kind: CallEdgeKind::Direct,
            confidence: 100,
        },
    );

    let code = render_mlil_preview_with_context(
        &func,
        "x86_forward_external_direct_branch",
        0x406000,
        &preview_options_x86(),
        Some(&context),
    )
    .expect("preview render");
    assert!(code.contains("external_tail();"), "{code}");
    assert!(!code.contains("__fission_branchind("), "{code}");
    assert!(!code.contains("goto block_406000;"), "{code}");
}

#[test]
fn preview_known_external_tail_call_recovers_same_block_register_arg() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006000,
                    output: Some(reg(0x08, 8)),
                    inputs: vec![cst(0x140005000, 8)],
                    asm_mnemonic: Some("LEA RCX,[callback]".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Branch,
                    address: 0x140006007,
                    output: None,
                    inputs: vec![cst(0x140007000, 8)],
                    asm_mnemonic: Some("JMP external_tail".to_string()),
                },
            ],
        }],
    };
    let mut context = PreviewTypeContext::default();
    context.call_target_refs.insert(
        0x140005000,
        CallTargetRef {
            address: Some(0x140005000),
            symbol: "callback".to_string(),
            provenance: CallTargetProvenance::Direct,
            edge_kind: CallEdgeKind::Direct,
            confidence: 100,
        },
    );
    context.call_target_refs.insert(
        0x140007000,
        CallTargetRef {
            address: Some(0x140007000),
            symbol: "external_tail".to_string(),
            provenance: CallTargetProvenance::Direct,
            edge_kind: CallEdgeKind::Direct,
            confidence: 100,
        },
    );

    let code = render_mlil_preview_with_context(
        &func,
        "x64_known_external_tail_arg",
        0x140006000,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render");
    assert!(code.contains("external_tail(callback);"), "{code}");
    assert!(!code.contains("__fission_branchind("), "{code}");
}

#[test]
fn preview_recovers_cross_block_rust_sleigh_register_call_arg() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let runtime_reg = |offset, size| Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x140006100,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x140006100,
                        output: Some(runtime_reg(0x08, 4)),
                        inputs: vec![cst(7, 4)],
                        asm_mnemonic: Some("MOV ECX,7".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x140006105,
                        output: None,
                        inputs: vec![cst(0x140006110, 8)],
                        asm_mnemonic: Some("JMP 0x140006110".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x140006110,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Call,
                        address: 0x140006110,
                        output: None,
                        inputs: vec![cst(0x140007000, 8)],
                        asm_mnemonic: Some("CALL external_call".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x140006115,
                        output: None,
                        inputs: vec![cst(0, 8)],
                        asm_mnemonic: Some("RET".to_string()),
                    },
                ],
            },
        ],
    };
    let mut context = PreviewTypeContext::default();
    context.call_target_refs.insert(
        0x140007000,
        CallTargetRef {
            address: Some(0x140007000),
            symbol: "external_call".to_string(),
            provenance: CallTargetProvenance::Direct,
            edge_kind: CallEdgeKind::Direct,
            confidence: 100,
        },
    );

    let code = render_mlil_preview_with_context(
        &func,
        "x64_cross_block_call_arg",
        0x140006100,
        &options,
        Some(&context),
    )
    .expect("preview render");
    assert!(code.contains("external_call(7);"), "{code}");
    assert!(!code.contains("external_call();"), "{code}");
}

#[test]
fn preview_unresolved_direct_branch_with_single_successor_uses_successor_target() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x406100,
                successors: vec![1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x406100,
                    output: None,
                    inputs: vec![cst(0x499999, 8)],
                    asm_mnemonic: Some("JMP unresolved".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x406110,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x406110,
                    output: None,
                    inputs: vec![cst(0, 4), cst(3, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Goto(target) => assert_eq!(target, 0x406110),
        other => panic!("expected goto terminator, got {other:?}"),
    }
}

#[test]
fn preview_branch_target_copy_wrapper_recovers_direct_target() {
    let wrapped_target = reg(0x00, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x406200,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x406200,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![cst(0x406220, 8)],
                        asm_mnemonic: Some("MOV target, 0x406220".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x406201,
                        output: None,
                        inputs: vec![wrapped_target],
                        asm_mnemonic: Some("JMP wrapped_target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x406220,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x406220,
                    output: None,
                    inputs: vec![cst(0, 4), cst(4, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Goto(target) => assert_eq!(target, 0x406220),
        other => panic!("expected goto terminator, got {other:?}"),
    }
}

#[test]
fn preview_cbranch_target_copy_wrapper_recovers_direct_target() {
    let wrapped_target = reg(0x00, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x407100,
                successors: vec![1, 2],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x407100,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![cst(0x407120, 8)],
                        asm_mnemonic: Some("MOV target, 0x407120".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x407101,
                        output: None,
                        inputs: vec![wrapped_target, reg(0x206, 1)],
                        asm_mnemonic: Some("JNZ wrapped_target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x407110,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407110,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x407120,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407120,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond {
            true_target,
            false_target,
            ..
        } => {
            assert_eq!(true_target, 0x407120);
            assert_eq!(false_target, Some(0x407110));
        }
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_branch_target_intadd_wrapper_recovers_direct_target() {
    let wrapped_target = Varnode {
        space_id: 3,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let base_target = Varnode {
        space_id: 1,
        offset: 0x406300,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4062e0,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4062e0,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![base_target, cst(0x20, 8)],
                        asm_mnemonic: Some("LEA target, base+0x20".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4062e1,
                        output: None,
                        inputs: vec![wrapped_target],
                        asm_mnemonic: Some("JMP target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x406320,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x406320,
                    output: None,
                    inputs: vec![cst(0, 4), cst(5, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Goto(target) => assert_eq!(target, 0x406320),
        other => panic!("expected goto terminator, got {other:?}"),
    }
}

#[test]
fn preview_cbranch_target_intadd_wrapper_recovers_direct_target() {
    let wrapped_target = Varnode {
        space_id: 3,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let base_target = Varnode {
        space_id: 1,
        offset: 0x407300,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4072e0,
                successors: vec![1, 2],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4072e0,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![base_target, cst(0x20, 8)],
                        asm_mnemonic: Some("LEA target, base+0x20".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4072e1,
                        output: None,
                        inputs: vec![wrapped_target, reg(0x206, 1)],
                        asm_mnemonic: Some("JNZ target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4072f0,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4072f0,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x407320,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407320,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond {
            true_target,
            false_target,
            ..
        } => {
            assert_eq!(true_target, 0x407320);
            assert_eq!(false_target, Some(0x4072f0));
        }
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_branchind_single_target_degrades_to_dispatcher_surface() {
    let switch_var = Varnode {
        space_id: 3,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let target_addr = Varnode {
        space_id: 1,
        offset: 0x405220,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405200,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Load,
                        address: 0x405200,
                        output: Some(switch_var.clone()),
                        inputs: vec![cst(0, 8), target_addr],
                        asm_mnemonic: Some("LOAD target from table".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::BranchInd,
                        address: 0x405201,
                        output: None,
                        inputs: vec![switch_var],
                        asm_mnemonic: Some("JMP_IND".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405220,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405220,
                    output: None,
                    inputs: vec![cst(0, 4), cst(6, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } => {
            assert_eq!(evidence.surface, IndirectControlSurface::DispatcherLike);
            assert_eq!(
                evidence.failure_family,
                UnsupportedControlFamily::NonStructuralDispatcher
            );
            assert_eq!(evidence.successor_targets, vec![0x405220]);
            assert!(target_expr.is_some());
        }
        other => panic!("expected dispatcher surface, got {other:?}"),
    }

    let code = render_mlil_preview(
        &func,
        "dispatcher_single_target",
        0x405200,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_dispatcher_indirect("), "{code}");
}

#[test]
fn preview_branchind_self_loop_global_load_prefers_dispatcher_surface() {
    let switch_var = uniq(0x900, 8);

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x405300,
            successors: vec![0],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Load,
                    address: 0x405300,
                    output: Some(switch_var.clone()),
                    inputs: vec![cst(0, 8), cst(0x401380, 8)],
                    asm_mnemonic: Some("LOAD dispatcher slot".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405301,
                    output: None,
                    inputs: vec![switch_var],
                    asm_mnemonic: Some("JMP_IND".to_string()),
                },
            ],
        }],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } => {
            assert_eq!(evidence.surface, IndirectControlSurface::DispatcherLike);
            assert_eq!(
                evidence.failure_family,
                UnsupportedControlFamily::NonStructuralDispatcher
            );
            assert_eq!(evidence.successor_targets.len(), 1);
            assert!(target_expr.is_some());
        }
        other => panic!("expected dispatcher surface, got {other:?}"),
    }
    let stats = builder.preview_build_stats();
    assert_eq!(stats.dispatcher_shape_recovered_count, 1);
    assert_eq!(stats.indirect_target_set_refined_count, 1);

    let code = render_mlil_preview(
        &func,
        "dispatcher_self_loop_global_load",
        0x405300,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_dispatcher_indirect("), "{code}");
    assert!(!code.contains("switch ("), "{code}");
}

#[test]
fn preview_unresolved_cbranch_uses_unique_non_fallthrough_successor() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x407000,
                successors: vec![1, 2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x407000,
                    output: None,
                    inputs: vec![cst(0x4AAAAA, 8), reg(0x206, 1)],
                    asm_mnemonic: Some("JNZ unresolved".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x407010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407010,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x407020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond {
            true_target,
            false_target,
            ..
        } => {
            assert_eq!(true_target, 0x407020);
            assert_eq!(false_target, Some(0x407010));
        }
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_build_stats_records_structuring_duration() {
    let cond = uniq(0x361, 1);
    let direct_target = Varnode {
        space_id: 1,
        offset: 0x5020,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5000,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5001,
                        output: None,
                        inputs: vec![direct_target, cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5010,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let _ = render_mlil_preview(
        &func,
        "x86_structuring_stats",
        0x5000,
        &preview_options_x86(),
    )
    .expect("preview render");
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert_eq!(stats.max_structuring_scc_component_size, 1);
    assert!(stats.structuring_scc_component_count >= 1);
    assert!(stats.structuring_duration_ms <= stats.build_duration_ms);
}

#[test]
fn preview_build_stats_records_render_duration() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x503000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x503000,
                output: None,
                inputs: vec![cst(0, 4), cst(7, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let start = std::time::Instant::now();
    let _ = render_mlil_preview(
        &func,
        "x86_render_duration",
        0x503000,
        &preview_options_x86(),
    )
    .expect("preview render");
    let elapsed_ms = start.elapsed().as_millis() as usize;
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert!(stats.render_duration_ms <= elapsed_ms);
}

#[test]
fn preview_build_stats_records_rendered_code_len() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x504000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x504000,
                output: None,
                inputs: vec![cst(0, 4), cst(9, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let code = render_mlil_preview(&func, "x86_render_len", 0x504000, &preview_options_x86())
        .expect("preview render");
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert_eq!(stats.rendered_code_len, code.len());
}

#[test]
fn preview_build_stats_records_max_structuring_scc_component_size() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x505000,
                successors: vec![1, 2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x505000,
                    output: None,
                    inputs: vec![cst(0x505020, 4), reg(0x206, 1)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x505010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x505010,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x505020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x505020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let _ = render_mlil_preview(&func, "x86_scc_size", 0x505000, &preview_options_x86())
        .expect("preview render");
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert_eq!(stats.max_structuring_scc_component_size, 1);
}

fn lower_x86_cond_expr(func: &PcodeFunction) -> HirExpr {
    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond { cond, .. } => cond,
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_recovers_test_reg_reg_jz_as_eq_zero() {
    let tmp = uniq(0x300, 4);
    let zf = reg(0x206, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x403000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x403000,
                    output: Some(tmp.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x00, 4)],
                    asm_mnemonic: Some("TEST EAX,EAX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x403000,
                    output: Some(zf.clone()),
                    inputs: vec![tmp, cst(0, 4)],
                    asm_mnemonic: Some("TEST EAX,EAX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x403001,
                    output: None,
                    inputs: vec![cst(0x403100, 4), zf],
                    asm_mnemonic: Some("JZ 0x403100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax == 0");
}

#[test]
fn preview_recovers_test_reg_reg_jg_as_gt_zero() {
    let tmp = uniq(0x310, 4);
    let of = reg(0x20b, 1);
    let sf = reg(0x207, 1);
    let zf = reg(0x206, 1);
    let not_zf = uniq(0x311, 1);
    let of_eq_sf = uniq(0x312, 1);
    let cond_vn = uniq(0x313, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x404000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x404000,
                    output: Some(of.clone()),
                    inputs: vec![cst(0, 1)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x404000,
                    output: Some(tmp.clone()),
                    inputs: vec![reg(0x04, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntSLess,
                    address: 0x404000,
                    output: Some(sf.clone()),
                    inputs: vec![tmp.clone(), cst(0, 4)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x404000,
                    output: Some(zf.clone()),
                    inputs: vec![tmp, cst(0, 4)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::BoolNegate,
                    address: 0x404001,
                    output: Some(not_zf.clone()),
                    inputs: vec![zf],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x404001,
                    output: Some(of_eq_sf.clone()),
                    inputs: vec![of, sf],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::BoolAnd,
                    address: 0x404001,
                    output: Some(cond_vn.clone()),
                    inputs: vec![not_zf, of_eq_sf],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x404001,
                    output: None,
                    inputs: vec![cst(0x404100, 4), cond_vn],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "0 < ecx");
}

#[test]
fn preview_recovers_cmp_je_as_eq() {
    let diff = uniq(0x320, 4);
    let zf = reg(0x206, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x405000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x405000,
                    output: Some(diff.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x405000,
                    output: Some(zf.clone()),
                    inputs: vec![diff, cst(0, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x405001,
                    output: None,
                    inputs: vec![cst(0x405100, 4), zf],
                    asm_mnemonic: Some("JE 0x405100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax == ecx");
}

#[test]
fn preview_recovers_cmp_jb_as_unsigned_lt() {
    let cf = reg(0x200, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x406000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntLess,
                    address: 0x406000,
                    output: Some(cf.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x406001,
                    output: None,
                    inputs: vec![cst(0x406100, 4), cf],
                    asm_mnemonic: Some("JB 0x406100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax < ecx");
}

#[test]
fn preview_recovers_cmp_jl_as_signed_lt() {
    let diff = uniq(0x330, 4);
    let sf = reg(0x207, 1);
    let of = reg(0x20b, 1);
    let cond_vn = uniq(0x331, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x407000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x407000,
                    output: Some(diff.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntSLess,
                    address: 0x407000,
                    output: Some(sf.clone()),
                    inputs: vec![diff, cst(0, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntSBorrow,
                    address: 0x407000,
                    output: Some(of.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::IntNotEqual,
                    address: 0x407001,
                    output: Some(cond_vn.clone()),
                    inputs: vec![sf, of],
                    asm_mnemonic: Some("JL 0x407100".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x407001,
                    output: None,
                    inputs: vec![cst(0x407100, 4), cond_vn],
                    asm_mnemonic: Some("JL 0x407100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax < ecx");
}

#[test]
fn preview_leaves_non_exact_branch_shape_as_generic_value() {
    let weird = uniq(0x340, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x408000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BoolXor,
                    address: 0x408000,
                    output: Some(weird.clone()),
                    inputs: vec![reg(0x206, 1), reg(0x207, 1)],
                    asm_mnemonic: Some("JCC".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x408001,
                    output: None,
                    inputs: vec![cst(0x408100, 4), weird],
                    asm_mnemonic: Some("JCC 0x408100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "reg ^ reg");
}
