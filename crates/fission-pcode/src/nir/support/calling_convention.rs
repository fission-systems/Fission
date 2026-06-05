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
    /// x86 32-bit cdecl/stdcall calling convention (arguments passed on stack).
    X86_32,
}

impl Default for CallingConvention {
    fn default() -> Self {
        Self::WindowsX64
    }
}
