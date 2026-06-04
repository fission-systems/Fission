use super::*;

/// Maps a Ghidra REGISTER-space offset to the hardware register name for x86-64.
///
/// This is ABI-independent — offset 0x08 is always RCX regardless of calling convention.
pub(crate) fn x64_ghidra_reg_name(offset: u64) -> Option<&'static str> {
    match offset {
        0x00 => Some("rax"),
        0x08 => Some("rcx"),
        0x10 => Some("rdx"),
        0x18 => Some("rbx"),
        0x20 => Some("rsp"),
        0x28 => Some("rbp"),
        0x30 => Some("rsi"),
        0x38 => Some("rdi"),
        0x80 => Some("r8"),
        0x88 => Some("r9"),
        0x90 => Some("r10"),
        0x98 => Some("r11"),
        0xa0 => Some("r12"),
        0xa8 => Some("r13"),
        0xb0 => Some("r14"),
        0xb8 => Some("r15"),
        _ => None,
    }
}

pub(crate) fn aarch64_ghidra_reg_name(offset: u64, size: u32) -> Option<&'static str> {
    const X_REGS: [&str; 32] = [
        "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13",
        "x14", "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26",
        "x27", "x28", "x29", "x30", "xzr",
    ];
    const W_REGS: [&str; 32] = [
        "w0", "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "w10", "w11", "w12", "w13",
        "w14", "w15", "w16", "w17", "w18", "w19", "w20", "w21", "w22", "w23", "w24", "w25", "w26",
        "w27", "w28", "w29", "w30", "wzr",
    ];
    if offset == 0x08 && size == 8 {
        return Some("sp");
    }
    match size {
        4 if (0x4000..=0x40f8).contains(&offset) && (offset - 0x4000) % 8 == 0 => {
            let idx = ((offset - 0x4000) / 8) as usize;
            W_REGS.get(idx).copied()
        }
        4 if (0x4004..=0x40fc).contains(&offset) && (offset - 0x4004) % 8 == 0 => {
            let idx = ((offset - 0x4004) / 8) as usize;
            W_REGS.get(idx).copied()
        }
        8 if (0x4000..=0x40f8).contains(&offset) && (offset - 0x4000) % 8 == 0 => {
            let idx = ((offset - 0x4000) / 8) as usize;
            X_REGS.get(idx).copied()
        }
        _ if (0x4000..=0x40f8).contains(&offset) && (offset - 0x4000) % 8 == 0 => {
            let idx = ((offset - 0x4000) / 8) as usize;
            X_REGS.get(idx).copied()
        }
        _ => None,
    }
}

pub(crate) fn aarch64_gpr_family_index(name: &str) -> Option<usize> {
    let rest = name.strip_prefix('x').or_else(|| name.strip_prefix('w'))?;
    let idx = rest.parse::<usize>().ok()?;
    (idx < 31).then_some(idx)
}

pub(crate) fn arm32_ghidra_reg_name(offset: u64, size: u32) -> Option<&'static str> {
    if size != 4 {
        return None;
    }
    match offset {
        0x20 => Some("r0"),
        0x24 => Some("r1"),
        0x28 => Some("r2"),
        0x2c => Some("r3"),
        0x30 => Some("r4"),
        0x34 => Some("r5"),
        0x38 => Some("r6"),
        0x3c => Some("r7"),
        0x40 => Some("r8"),
        0x44 => Some("r9"),
        0x48 => Some("r10"),
        0x4c => Some("r11"),
        0x50 => Some("r12"),
        0x54 => Some("sp"),
        0x58 => Some("lr"),
        0x5c => Some("pc"),
        _ => None,
    }
}

pub(crate) fn arm32_gpr_family_index(name: &str) -> Option<usize> {
    match name {
        "sp" => Some(13),
        "lr" => Some(14),
        "pc" => Some(15),
        _ => {
            let idx = name.strip_prefix('r')?.parse::<usize>().ok()?;
            (idx < 13).then_some(idx)
        }
    }
}

pub(crate) fn powerpc_gpr_family_index(name: &str) -> Option<usize> {
    let idx = name.strip_prefix('r')?.parse::<usize>().ok()?;
    (idx < 32).then_some(idx)
}

pub(crate) fn powerpc_ghidra_reg_name(offset: u64, size: u32) -> Option<&'static str> {
    match size {
        4 => match offset {
            0x00 => Some("r0"),
            0x04 => Some("r1"),
            0x08 => Some("r2"),
            0x0c => Some("r3"),
            0x10 => Some("r4"),
            0x14 => Some("r5"),
            0x18 => Some("r6"),
            0x1c => Some("r7"),
            0x20 => Some("r8"),
            0x24 => Some("r9"),
            0x28 => Some("r10"),
            0x2c => Some("r11"),
            0x30 => Some("r12"),
            0x34 => Some("r13"),
            0x38 => Some("r14"),
            0x3c => Some("r15"),
            0x40 => Some("r16"),
            0x44 => Some("r17"),
            0x48 => Some("r18"),
            0x4c => Some("r19"),
            0x50 => Some("r20"),
            0x54 => Some("r21"),
            0x58 => Some("r22"),
            0x5c => Some("r23"),
            0x60 => Some("r24"),
            0x64 => Some("r25"),
            0x68 => Some("r26"),
            0x6c => Some("r27"),
            0x70 => Some("r28"),
            0x74 => Some("r29"),
            0x78 => Some("r30"),
            0x7c => Some("r31"),
            0x988 => Some("r2Save"),
            0x1020 => Some("LR"),
            _ => None,
        },
        8 => match offset {
            0x00 => Some("r0"),
            0x08 => Some("r1"),
            0x10 => Some("r2"),
            0x18 => Some("r3"),
            0x20 => Some("r4"),
            0x28 => Some("r5"),
            0x30 => Some("r6"),
            0x38 => Some("r7"),
            0x40 => Some("r8"),
            0x48 => Some("r9"),
            0x50 => Some("r10"),
            0x58 => Some("r11"),
            0x60 => Some("r12"),
            0x68 => Some("r13"),
            0x70 => Some("r14"),
            0x78 => Some("r15"),
            0x80 => Some("r16"),
            0x88 => Some("r17"),
            0x90 => Some("r18"),
            0x98 => Some("r19"),
            0xa0 => Some("r20"),
            0xa8 => Some("r21"),
            0xb0 => Some("r22"),
            0xb8 => Some("r23"),
            0xc0 => Some("r24"),
            0xc8 => Some("r25"),
            0xd0 => Some("r26"),
            0xd8 => Some("r27"),
            0xe0 => Some("r28"),
            0xe8 => Some("r29"),
            0xf0 => Some("r30"),
            0xf8 => Some("r31"),
            0x988 => Some("r2Save"),
            0x1040 => Some("LR"),
            _ => None,
        },
        1 => match offset {
            0x400 => Some("xer_so"),
            0x403 => Some("xer_ca"),
            0x900 => Some("cr0"),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn powerpc_ghidra_reg_name_for_abi(
    offset: u64,
    size: u32,
    abi: CallingConvention,
) -> Option<&'static str> {
    if abi == CallingConvention::PowerPc64 && size == 4 {
        let slot_base = offset & !0x7;
        if offset.checked_add(u64::from(size))? <= slot_base.checked_add(8)? {
            return powerpc_ghidra_reg_name(slot_base, 8);
        }
    }
    powerpc_ghidra_reg_name(offset, size)
}

pub(crate) fn loongarch_gpr_family_index(name: &str) -> Option<usize> {
    match name {
        "zero" => Some(0),
        "ra" => Some(1),
        "tp" => Some(2),
        "sp" => Some(3),
        "fp" => Some(22),
        _ => {
            if let Some(rest) = name.strip_prefix('a') {
                let idx = rest.parse::<usize>().ok()?;
                return (idx < 8).then_some(4 + idx);
            }
            if let Some(rest) = name.strip_prefix('t') {
                let idx = rest.parse::<usize>().ok()?;
                return (idx < 9).then_some(12 + idx);
            }
            if let Some(rest) = name.strip_prefix('s') {
                let idx = rest.parse::<usize>().ok()?;
                return (idx < 9).then_some(23 + idx);
            }
            name.strip_prefix('r')?
                .parse::<usize>()
                .ok()
                .filter(|idx| *idx < 32)
        }
    }
}

pub(crate) fn loongarch_ghidra_reg_name_for_abi(
    offset: u64,
    size: u32,
    abi: CallingConvention,
) -> Option<&'static str> {
    const GPRS: [&str; 32] = [
        "zero", "ra", "tp", "sp", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "t0", "t1", "t2",
        "t3", "t4", "t5", "t6", "t7", "t8", "r21", "fp", "s0", "s1", "s2", "s3", "s4", "s5", "s6",
        "s7", "s8",
    ];
    let (base, stride, full_size) = match abi {
        CallingConvention::LoongArch32 => (0x100, 4, 4),
        CallingConvention::LoongArch64 => (0x100, 8, 8),
        _ => return None,
    };
    if size != full_size {
        return None;
    }
    if offset < base || (offset - base) % stride != 0 {
        return None;
    }
    let idx = ((offset - base) / stride) as usize;
    GPRS.get(idx).copied()
}

pub(crate) fn mips_gpr_family_index(name: &str) -> Option<usize> {
    match name {
        "zero" => Some(0),
        "at" => Some(1),
        "v0" => Some(2),
        "v1" => Some(3),
        "a0" => Some(4),
        "a1" => Some(5),
        "a2" => Some(6),
        "a3" => Some(7),
        "gp" => Some(28),
        "sp" => Some(29),
        "fp" => Some(30),
        "ra" => Some(31),
        _ => {
            if let Some(rest) = name.strip_prefix('t') {
                let idx = rest.parse::<usize>().ok()?;
                return match idx {
                    0..=7 => Some(8 + idx),
                    8..=9 => Some(24 + (idx - 8)),
                    _ => None,
                };
            }
            if let Some(rest) = name.strip_prefix('s') {
                let idx = rest.parse::<usize>().ok()?;
                return (idx < 8).then_some(16 + idx);
            }
            name.strip_prefix('r')?
                .parse::<usize>()
                .ok()
                .filter(|idx| *idx < 32)
        }
    }
}

pub(crate) fn mips_ghidra_reg_name_for_abi(
    offset: u64,
    size: u32,
    abi: CallingConvention,
) -> Option<&'static str> {
    const GPRS: [&str; 32] = [
        "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6",
        "t7", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp",
        "fp", "ra",
    ];
    let (stride, full_size) = match abi {
        CallingConvention::Mips32 => (4, 4),
        CallingConvention::Mips64 => (8, 8),
        _ => return None,
    };
    if size != full_size || offset % stride != 0 {
        return None;
    }
    let idx = (offset / stride) as usize;
    GPRS.get(idx).copied()
}

/// Static `param_N` names for up to 8 parameters (enough for AArch64 PCS).
const PARAM_NAMES: [&str; 8] = [
    "param_1", "param_2", "param_3", "param_4", "param_5", "param_6", "param_7", "param_8",
];

/// Returns `(display_name, param_index)` for a Ghidra REGISTER-space varnode.
///
/// - If the offset is a parameter register for `abi`, returns `("param_N", Some(N-1))`.
/// - Otherwise returns the hardware register name with `None`.
pub(crate) fn register_name_with_param(
    offset: u64,
    _size: u32,
    abi: CallingConvention,
) -> Option<(&'static str, Option<usize>)> {
    let hw_name = match abi {
        CallingConvention::AArch64 if offset == 0x00 => match _size {
            4 => "w0",
            _ => "x0",
        },
        CallingConvention::AArch64 => aarch64_ghidra_reg_name(offset, _size)?,
        CallingConvention::Arm32 => arm32_ghidra_reg_name(offset, _size)?,
        CallingConvention::PowerPc32 | CallingConvention::PowerPc64 => {
            powerpc_ghidra_reg_name_for_abi(offset, _size, abi)?
        }
        CallingConvention::LoongArch32 | CallingConvention::LoongArch64 => {
            loongarch_ghidra_reg_name_for_abi(offset, _size, abi)?
        }
        CallingConvention::Mips32 | CallingConvention::Mips64 => {
            mips_ghidra_reg_name_for_abi(offset, _size, abi)?
        }
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
            x64_ghidra_reg_name(offset)?
        }
        CallingConvention::X86_32 => {
            register_name_32(offset, _size)?
        }
    };
    let param_idx = match abi {
        CallingConvention::AArch64 => aarch64_gpr_family_index(hw_name).and_then(|name_family| {
            abi.param_offsets().iter().position(|&param_offset| {
                aarch64_ghidra_reg_name(param_offset, 8)
                    .and_then(aarch64_gpr_family_index)
                    .is_some_and(|family| family == name_family)
            })
        }),
        CallingConvention::Arm32 => arm32_gpr_family_index(hw_name).and_then(|name_family| {
            abi.param_offsets().iter().position(|&param_offset| {
                arm32_ghidra_reg_name(param_offset, 4)
                    .and_then(arm32_gpr_family_index)
                    .is_some_and(|family| family == name_family)
            })
        }),
        CallingConvention::PowerPc32 | CallingConvention::PowerPc64 => {
            powerpc_gpr_family_index(hw_name).and_then(|name_family| {
                let slot_size = if abi == CallingConvention::PowerPc64 {
                    8
                } else {
                    _size
                };
                abi.param_offsets().iter().position(|&param_offset| {
                    powerpc_ghidra_reg_name(param_offset, slot_size)
                        .and_then(powerpc_gpr_family_index)
                        .is_some_and(|family| family == name_family)
                })
            })
        }
        CallingConvention::LoongArch32 | CallingConvention::LoongArch64 => {
            loongarch_gpr_family_index(hw_name).and_then(|name_family| {
                let slot_size = if abi == CallingConvention::LoongArch64 {
                    8
                } else {
                    4
                };
                abi.param_offsets().iter().position(|&param_offset| {
                    loongarch_ghidra_reg_name_for_abi(param_offset, slot_size, abi)
                        .and_then(loongarch_gpr_family_index)
                        .is_some_and(|family| family == name_family)
                })
            })
        }
        CallingConvention::Mips32 | CallingConvention::Mips64 => mips_gpr_family_index(hw_name)
            .and_then(|name_family| {
                let slot_size = if abi == CallingConvention::Mips64 {
                    8
                } else {
                    4
                };
                abi.param_offsets().iter().position(|&param_offset| {
                    mips_ghidra_reg_name_for_abi(param_offset, slot_size, abi)
                        .and_then(mips_gpr_family_index)
                        .is_some_and(|family| family == name_family)
                })
            }),
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 | CallingConvention::X86_32 => abi
            .param_offsets()
            .iter()
            .position(|&param_offset| param_offset == offset),
    };
    match param_idx {
        Some(idx) => Some((PARAM_NAMES[idx], Some(idx))),
        None => Some((hw_name, None)),
    }
}

/// Returns the hardware register name for a Ghidra REGISTER-space offset, ABI-independently.
pub(crate) fn register_name(offset: u64, size: u32) -> &'static str {
    x64_ghidra_reg_name(offset)
        .or_else(|| aarch64_ghidra_reg_name(offset, size))
        .or_else(|| arm32_ghidra_reg_name(offset, size))
        .or_else(|| powerpc_ghidra_reg_name(offset, size))
        .or_else(|| loongarch_ghidra_reg_name_for_abi(offset, size, CallingConvention::LoongArch64))
        .or_else(|| loongarch_ghidra_reg_name_for_abi(offset, size, CallingConvention::LoongArch32))
        .or_else(|| mips_ghidra_reg_name_for_abi(offset, size, CallingConvention::Mips64))
        .or_else(|| mips_ghidra_reg_name_for_abi(offset, size, CallingConvention::Mips32))
        .unwrap_or("reg")
}

pub(crate) fn register_hardware_name_for_abi(
    offset: u64,
    size: u32,
    abi: CallingConvention,
) -> Option<&'static str> {
    match abi {
        CallingConvention::AArch64 => aarch64_ghidra_reg_name(offset, size),
        CallingConvention::Arm32 => arm32_ghidra_reg_name(offset, size),
        CallingConvention::PowerPc32 | CallingConvention::PowerPc64 => {
            powerpc_ghidra_reg_name_for_abi(offset, size, abi)
        }
        CallingConvention::LoongArch32 | CallingConvention::LoongArch64 => {
            loongarch_ghidra_reg_name_for_abi(offset, size, abi)
        }
        CallingConvention::Mips32 | CallingConvention::Mips64 => {
            mips_ghidra_reg_name_for_abi(offset, size, abi)
        }
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
            x64_ghidra_reg_name(offset)
        }
        CallingConvention::X86_32 => {
            register_name_32(offset, size)
        }
    }
}

pub(crate) fn unique_register_name(offset: u64, size: u32) -> Option<&'static str> {
    crate::arch::x86::unique_x86_register_name(offset, size)
}

pub(crate) fn is_primary_return_register(vn: &Varnode) -> bool {
    (is_register_space_id(vn.space_id) && vn.offset == 0x00)
        || (vn.space_id == UNIQUE_SPACE_ID
            && unique_register_name(vn.offset, vn.size) == Some("rax"))
}

pub(crate) fn is_primary_return_register_for_abi(vn: &Varnode, abi: CallingConvention) -> bool {
    match abi {
        CallingConvention::AArch64 => {
            is_register_space_id(vn.space_id)
                && aarch64_ghidra_reg_name(vn.offset, vn.size).and_then(aarch64_gpr_family_index)
                    == Some(0)
        }
        CallingConvention::Arm32 => is_register_space_id(vn.space_id) && vn.offset == 0x20,
        CallingConvention::PowerPc32 => is_register_space_id(vn.space_id) && vn.offset == 0x0c,
        CallingConvention::PowerPc64 => is_register_space_id(vn.space_id) && vn.offset == 0x18,
        CallingConvention::LoongArch32 => is_register_space_id(vn.space_id) && vn.offset == 0x110,
        CallingConvention::LoongArch64 => is_register_space_id(vn.space_id) && vn.offset == 0x120,
        CallingConvention::Mips32 => is_register_space_id(vn.space_id) && vn.offset == 0x08,
        CallingConvention::Mips64 => is_register_space_id(vn.space_id) && vn.offset == 0x10,
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 | CallingConvention::X86_32 => {
            (is_register_space_id(vn.space_id) && vn.offset == 0x00)
                || (vn.space_id == UNIQUE_SPACE_ID
                    && unique_register_name(vn.offset, vn.size).is_some_and(|name| name == "rax" || name == "eax"))
        }
    }
}

pub(crate) fn is_return_target_register_for_abi(vn: &Varnode, abi: CallingConvention) -> bool {
    if !is_register_space_id(vn.space_id) {
        return false;
    }
    match abi {
        CallingConvention::AArch64 => {
            aarch64_ghidra_reg_name(vn.offset, vn.size).and_then(aarch64_gpr_family_index)
                == Some(30)
        }
        CallingConvention::Arm32 => vn.offset == 0x58,
        CallingConvention::PowerPc32 | CallingConvention::PowerPc64 => {
            powerpc_ghidra_reg_name(vn.offset, vn.size) == Some("LR")
        }
        CallingConvention::LoongArch32 | CallingConvention::LoongArch64 => {
            loongarch_ghidra_reg_name_for_abi(vn.offset, vn.size, abi) == Some("ra")
        }
        CallingConvention::Mips32 | CallingConvention::Mips64 => {
            mips_ghidra_reg_name_for_abi(vn.offset, vn.size, abi) == Some("ra")
        }
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 | CallingConvention::X86_32 => false,
    }
}

pub(crate) fn primary_return_registers(pointer_size: u32, abi: CallingConvention) -> Vec<Varnode> {
    match abi {
        CallingConvention::AArch64 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x4000,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x4000,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::Arm32 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x20,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x20,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::PowerPc32 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x0c,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x0c,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::PowerPc64 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x18,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x18,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::LoongArch32 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x110,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x110,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_ALT_REGISTER_SPACE_ID,
                offset: 0x110,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::LoongArch64 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x120,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x120,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_ALT_REGISTER_SPACE_ID,
                offset: 0x120,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::Mips32 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x08,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x08,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::Mips64 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x10,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x10,
                size: 8,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 | CallingConvention::X86_32 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x00,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: crate::arch::x86::X86_REG_BASE,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
        ],
    }
}

pub(crate) fn register_name_32(offset: u64, size: u32) -> Option<&'static str> {
    match (offset, size) {
        (0x00, 4) => Some("eax"),
        (0x04, 4) => Some("ecx"),
        (0x08, 4) => Some("edx"),
        (0x0c, 4) => Some("ebx"),
        (0x10, 4) => Some("esp"),
        (0x14, 4) => Some("ebp"),
        (0x18, 4) => Some("esi"),
        (0x1c, 4) => Some("edi"),
        _ => None,
    }
}
