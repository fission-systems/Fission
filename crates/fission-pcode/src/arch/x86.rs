/// x86 Sleigh lifter UNIQUE-space architectural register layout.
///
/// These constants are the **canonical definition**; `fission-sleigh` imports them.
/// Do not duplicate these values anywhere else in the codebase.
///
/// Layout (UNIQUE space, `space_id == 3`):
/// - GPR:    `X86_REG_BASE   + reg_index * 8`   (reg 0..=15)
/// - XMM:    `X86_XMM_BASE   + reg_index * 16`  (reg 0..=15)
/// - YMM:    `X86_YMM_BASE   + reg_index * 32`  (reg 0..=15)
/// - SEG:    `X86_SEG_BASE   + reg_index * 8`   (reg 0..=5)
/// - EFLAGS: `X86_EFLAGS_BASE + bit_offset`     (CF=0, PF=2, AF=4, ZF=6, SF=7, IF=9, DF=10, OF=11)
/// - MXCSR:  `X86_EFLAGS_BASE + 0x100`
///
/// GPR index mapping:
/// 0=rax 1=rcx 2=rdx 3=rbx 4=rsp 5=rbp 6=rsi 7=rdi 8..15=r8..r15
pub const X86_REG_BASE: u64 = 0xA860_0000;
pub const X86_XMM_BASE: u64 = 0xA868_0000;
pub const X86_YMM_BASE: u64 = 0xA869_0000;
pub const X86_SEG_BASE: u64 = 0xA86A_0000;
pub const X86_EFLAGS_BASE: u64 = 0xA86F_0000;
pub const X86_MXCSR_OFFSET: u64 = X86_EFLAGS_BASE + 0x100;

/// Returns the canonical x86-64 GPR family index for any width alias.
pub fn x86_gpr_family_index(name: &str) -> Option<usize> {
    const GPR_ALIASES: [&[&str]; 16] = [
        &["rax", "eax", "ax", "al"],
        &["rcx", "ecx", "cx", "cl"],
        &["rdx", "edx", "dx", "dl"],
        &["rbx", "ebx", "bx", "bl"],
        &["rsp", "esp", "sp", "spl"],
        &["rbp", "ebp", "bp", "bpl"],
        &["rsi", "esi", "si", "sil"],
        &["rdi", "edi", "di", "dil"],
        &["r8", "r8d", "r8w", "r8b"],
        &["r9", "r9d", "r9w", "r9b"],
        &["r10", "r10d", "r10w", "r10b"],
        &["r11", "r11d", "r11w", "r11b"],
        &["r12", "r12d", "r12w", "r12b"],
        &["r13", "r13d", "r13w", "r13b"],
        &["r14", "r14d", "r14w", "r14b"],
        &["r15", "r15d", "r15w", "r15b"],
    ];

    GPR_ALIASES
        .iter()
        .position(|aliases| aliases.iter().any(|alias| alias.eq_ignore_ascii_case(name)))
}

/// Returns a human-readable name for a UNIQUE-space x86 architectural register varnode.
///
/// Returns `None` for any offset that does not fall on a valid stride boundary
/// within a known register range — ensuring no spurious matches on temporaries.
pub fn unique_x86_register_name(offset: u64, size: u32) -> Option<&'static str> {
    // GPR: stride 8, 16 registers
    if offset >= X86_REG_BASE && offset < X86_REG_BASE + 16 * 8 {
        let delta = offset - X86_REG_BASE;
        if delta % 8 == 0 {
            let idx = (delta / 8) as usize;
            const GPR64: [&str; 16] = [
                "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11",
                "r12", "r13", "r14", "r15",
            ];
            const GPR32: [&str; 16] = [
                "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d",
                "r11d", "r12d", "r13d", "r14d", "r15d",
            ];
            const GPR16: [&str; 16] = [
                "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w", "r11w",
                "r12w", "r13w", "r14w", "r15w",
            ];
            const GPR8: [&str; 16] = [
                "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b",
                "r12b", "r13b", "r14b", "r15b",
            ];
            return match size {
                1 => GPR8.get(idx).copied(),
                2 => GPR16.get(idx).copied(),
                4 => GPR32.get(idx).copied(),
                _ => GPR64.get(idx).copied(),
            };
        }
        return None;
    }

    // XMM: stride 16, 16 registers
    if offset >= X86_XMM_BASE && offset < X86_XMM_BASE + 16 * 16 {
        let delta = offset - X86_XMM_BASE;
        if delta % 16 == 0 {
            let idx = (delta / 16) as usize;
            const XMM: [&str; 16] = [
                "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "xmm8", "xmm9",
                "xmm10", "xmm11", "xmm12", "xmm13", "xmm14", "xmm15",
            ];
            return XMM.get(idx).copied();
        }
        return None;
    }

    // YMM: stride 32, 16 registers
    if offset >= X86_YMM_BASE && offset < X86_YMM_BASE + 16 * 32 {
        let delta = offset - X86_YMM_BASE;
        if delta % 32 == 0 {
            let idx = (delta / 32) as usize;
            const YMM: [&str; 16] = [
                "ymm0", "ymm1", "ymm2", "ymm3", "ymm4", "ymm5", "ymm6", "ymm7", "ymm8", "ymm9",
                "ymm10", "ymm11", "ymm12", "ymm13", "ymm14", "ymm15",
            ];
            return YMM.get(idx).copied();
        }
        return None;
    }

    // SEG: stride 8, 6 registers (CS=0, SS=1, DS=2, ES=3, FS=4, GS=5)
    if offset >= X86_SEG_BASE && offset < X86_SEG_BASE + 6 * 8 {
        let delta = offset - X86_SEG_BASE;
        if delta % 8 == 0 {
            let idx = (delta / 8) as usize;
            const SEG: [&str; 6] = ["cs", "ss", "ds", "es", "fs", "gs"];
            return SEG.get(idx).copied();
        }
        return None;
    }

    // EFLAGS individual bits
    if offset >= X86_EFLAGS_BASE && offset < X86_EFLAGS_BASE + 0x100 && size == 1 {
        return match offset - X86_EFLAGS_BASE {
            0 => Some("cf"),
            2 => Some("pf"),
            4 => Some("af"),
            6 => Some("zf"),
            7 => Some("sf"),
            9 => Some("if_"),
            10 => Some("df"),
            11 => Some("of"),
            _ => None,
        };
    }

    // MXCSR
    if offset == X86_MXCSR_OFFSET && size == 4 {
        return Some("mxcsr");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpr64_names_round_trip() {
        let expected = [
            "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11",
            "r12", "r13", "r14", "r15",
        ];
        for (i, name) in expected.iter().enumerate() {
            let offset = X86_REG_BASE + (i as u64) * 8;
            assert_eq!(
                unique_x86_register_name(offset, 8),
                Some(*name),
                "reg index {i} offset {offset:#x}"
            );
        }
    }

    #[test]
    fn gpr32_names_round_trip() {
        let expected = [
            "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d",
            "r12d", "r13d", "r14d", "r15d",
        ];
        for (i, name) in expected.iter().enumerate() {
            let offset = X86_REG_BASE + (i as u64) * 8;
            assert_eq!(
                unique_x86_register_name(offset, 4),
                Some(*name),
                "reg index {i} offset {offset:#x}"
            );
        }
    }

    #[test]
    fn xmm_names_round_trip() {
        for i in 0u64..16 {
            let offset = X86_XMM_BASE + i * 16;
            let got = unique_x86_register_name(offset, 16).unwrap();
            assert_eq!(got, format!("xmm{i}"));
        }
    }

    #[test]
    fn ymm_names_round_trip() {
        for i in 0u64..16 {
            let offset = X86_YMM_BASE + i * 32;
            let got = unique_x86_register_name(offset, 32).unwrap();
            assert_eq!(got, format!("ymm{i}"));
        }
    }

    #[test]
    fn eflags_known_bits() {
        assert_eq!(unique_x86_register_name(X86_EFLAGS_BASE + 6, 1), Some("zf"));
        assert_eq!(unique_x86_register_name(X86_EFLAGS_BASE + 0, 1), Some("cf"));
        assert_eq!(unique_x86_register_name(X86_EFLAGS_BASE + 7, 1), Some("sf"));
        assert_eq!(
            unique_x86_register_name(X86_EFLAGS_BASE + 11, 1),
            Some("of")
        );
    }

    #[test]
    fn out_of_range_returns_none() {
        // Completely outside all ranges
        assert_eq!(unique_x86_register_name(0x1234, 8), None);
        // Between GPR and XMM ranges
        assert_eq!(unique_x86_register_name(X86_REG_BASE + 16 * 8, 8), None);
        // Misaligned within GPR range
        assert_eq!(unique_x86_register_name(X86_REG_BASE + 3, 8), None);
        // Misaligned within XMM range
        assert_eq!(unique_x86_register_name(X86_XMM_BASE + 5, 16), None);
    }

    #[test]
    fn rsp_is_reg_index_4() {
        // rsp = index 4, size 8 → X86_REG_BASE + 4*8 = 0xA860_0020
        assert_eq!(
            unique_x86_register_name(X86_REG_BASE + 4 * 8, 8),
            Some("rsp")
        );
    }

    #[test]
    fn seg_names() {
        assert_eq!(
            unique_x86_register_name(X86_SEG_BASE + 4 * 8, 8),
            Some("fs")
        );
        assert_eq!(
            unique_x86_register_name(X86_SEG_BASE + 5 * 8, 8),
            Some("gs")
        );
    }
}
