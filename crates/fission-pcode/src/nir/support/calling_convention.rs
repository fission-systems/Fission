use super::*;

/// x64 calling convention used when identifying parameter registers.
///
/// This affects which REGISTER-space varnodes are labelled `param_1`, `param_2`, etc.
/// in decompiled output. It does **not** affect hardware register names (rax, rbx, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CallingConvention {
    /// Windows x64 fastcall: first four integer args in RCX, RDX, R8, R9.
    WindowsX64,
    /// System V AMD64 ABI (Linux / macOS): first six integer args in RDI, RSI, RDX, RCX, R8, R9.
    SystemVAmd64,
    /// AArch64 Procedure Call Standard: first eight integer args in X0-X7/W0-W7.
    AArch64,
    /// ARM Procedure Call Standard: first four integer args in R0-R3.
    Arm32,
    /// PowerPC 32-bit ELF ABI: first eight integer args in r3-r10, return in r3.
    PowerPc32,
    /// PowerPC 64-bit ELF ABI: first eight integer args in r3-r10, return in r3.
    PowerPc64,
    /// LoongArch 32-bit ELF ABI: first eight integer args in a0-a7, return in a0.
    LoongArch32,
    /// LoongArch 64-bit ELF ABI: first eight integer args in a0-a7, return in a0.
    LoongArch64,
    /// MIPS 32-bit ELF ABI: first four integer args in a0-a3, return in v0.
    Mips32,
    /// MIPS 64-bit ELF ABI: first four integer args in a0-a3, return in v0.
    Mips64,
}

impl Default for CallingConvention {
    fn default() -> Self {
        Self::WindowsX64
    }
}

impl CallingConvention {
    /// Returns the ordered list of Ghidra REGISTER-space offsets for integer parameter registers.
    pub(crate) fn param_offsets(self) -> &'static [u64] {
        match self {
            Self::WindowsX64 => &[
                0x08, // rcx → param_1
                0x10, // rdx → param_2
                0x80, // r8  → param_3
                0x88, // r9  → param_4
            ],
            Self::SystemVAmd64 => &[
                0x38, // rdi → param_1
                0x30, // rsi → param_2
                0x10, // rdx → param_3
                0x08, // rcx → param_4
                0x80, // r8  → param_5
                0x88, // r9  → param_6
            ],
            Self::AArch64 => &[
                0x4000, // x0/w0 → param_1
                0x4008, // x1/w1 → param_2
                0x4010, // x2/w2 → param_3
                0x4018, // x3/w3 → param_4
                0x4020, // x4/w4 → param_5
                0x4028, // x5/w5 → param_6
                0x4030, // x6/w6 → param_7
                0x4038, // x7/w7 → param_8
            ],
            Self::Arm32 => &[
                0x20, // r0 → param_1
                0x24, // r1 → param_2
                0x28, // r2 → param_3
                0x2c, // r3 → param_4
            ],
            Self::PowerPc32 => &[
                0x0c, // r3  → param_1
                0x10, // r4  → param_2
                0x14, // r5  → param_3
                0x18, // r6  → param_4
                0x1c, // r7  → param_5
                0x20, // r8  → param_6
                0x24, // r9  → param_7
                0x28, // r10 → param_8
            ],
            Self::PowerPc64 => &[
                0x18, // r3  → param_1
                0x20, // r4  → param_2
                0x28, // r5  → param_3
                0x30, // r6  → param_4
                0x38, // r7  → param_5
                0x40, // r8  → param_6
                0x48, // r9  → param_7
                0x50, // r10 → param_8
            ],
            Self::LoongArch32 => &[
                0x110, // a0 → param_1
                0x114, // a1 → param_2
                0x118, // a2 → param_3
                0x11c, // a3 → param_4
                0x120, // a4 → param_5
                0x124, // a5 → param_6
                0x128, // a6 → param_7
                0x12c, // a7 → param_8
            ],
            Self::LoongArch64 => &[
                0x120, // a0 → param_1
                0x128, // a1 → param_2
                0x130, // a2 → param_3
                0x138, // a3 → param_4
                0x140, // a4 → param_5
                0x148, // a5 → param_6
                0x150, // a6 → param_7
                0x158, // a7 → param_8
            ],
            Self::Mips32 => &[
                0x10, // a0 → param_1
                0x14, // a1 → param_2
                0x18, // a2 → param_3
                0x1c, // a3 → param_4
            ],
            Self::Mips64 => &[
                0x20, // a0 → param_1
                0x28, // a1 → param_2
                0x30, // a2 → param_3
                0x38, // a3 → param_4
            ],
        }
    }

    /// Returns the (REGISTER-space offset, varnode size) pairs for all integer
    /// parameter registers used by call argument recovery.
    pub(crate) fn param_reg_slots(self) -> &'static [(u64, u32)] {
        match self {
            Self::WindowsX64 => &[
                (0x08, 8), // rcx  → param_1
                (0x10, 8), // rdx  → param_2
                (0x80, 8), // r8   → param_3
                (0x88, 8), // r9   → param_4
            ],
            Self::SystemVAmd64 => &[
                (0x38, 8), // rdi  → param_1
                (0x30, 8), // rsi  → param_2
                (0x10, 8), // rdx  → param_3
                (0x08, 8), // rcx  → param_4
                (0x80, 8), // r8   → param_5
                (0x88, 8), // r9   → param_6
            ],
            Self::AArch64 => &[
                (0x4000, 8), // x0  → param_1
                (0x4008, 8), // x1  → param_2
                (0x4010, 8), // x2  → param_3
                (0x4018, 8), // x3  → param_4
                (0x4020, 8), // x4  → param_5
                (0x4028, 8), // x5  → param_6
                (0x4030, 8), // x6  → param_7
                (0x4038, 8), // x7  → param_8
            ],
            Self::Arm32 => &[
                (0x20, 4), // r0  → param_1
                (0x24, 4), // r1  → param_2
                (0x28, 4), // r2  → param_3
                (0x2c, 4), // r3  → param_4
            ],
            Self::PowerPc32 => &[
                (0x0c, 4), // r3
                (0x10, 4), // r4
                (0x14, 4), // r5
                (0x18, 4), // r6
                (0x1c, 4), // r7
                (0x20, 4), // r8
                (0x24, 4), // r9
                (0x28, 4), // r10
            ],
            Self::PowerPc64 => &[
                (0x18, 8), // r3
                (0x20, 8), // r4
                (0x28, 8), // r5
                (0x30, 8), // r6
                (0x38, 8), // r7
                (0x40, 8), // r8
                (0x48, 8), // r9
                (0x50, 8), // r10
            ],
            Self::LoongArch32 => &[
                (0x110, 4), // a0
                (0x114, 4), // a1
                (0x118, 4), // a2
                (0x11c, 4), // a3
                (0x120, 4), // a4
                (0x124, 4), // a5
                (0x128, 4), // a6
                (0x12c, 4), // a7
            ],
            Self::LoongArch64 => &[
                (0x120, 8), // a0
                (0x128, 8), // a1
                (0x130, 8), // a2
                (0x138, 8), // a3
                (0x140, 8), // a4
                (0x148, 8), // a5
                (0x150, 8), // a6
                (0x158, 8), // a7
            ],
            Self::Mips32 => &[
                (0x10, 4), // a0
                (0x14, 4), // a1
                (0x18, 4), // a2
                (0x1c, 4), // a3
            ],
            Self::Mips64 => &[
                (0x20, 8), // a0
                (0x28, 8), // a1
                (0x30, 8), // a2
                (0x38, 8), // a3
            ],
        }
    }
}
