/// Calling convention used when identifying parameter registers.
///
/// This affects which REGISTER-space varnodes are labelled `param_1`, `param_2`, etc.
/// in decompiled output. It does **not** affect hardware register names (rax, rbx, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum CallingConvention {
    /// Windows x64 fastcall: first four integer args in RCX, RDX, R8, R9.
    #[default]
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

impl CallingConvention {
    /// The register-space `(space_id-relative) offset` of this convention's
    /// native stack-pointer register (RSP/ESP or architectural equivalent),
    /// independent of any per-function cspec/prototype data. Mirrors the
    /// per-architecture offsets `stack_slots.rs`'s `resolve_stack_address_inner`
    /// already hardcodes for its `StackBase::Rsp` case -- kept in one place
    /// so a second consumer (`scalar_ssa.rs`'s `resolve_pointer_value`, whose
    /// stack-pointer recognition was previously gated behind
    /// `cspec_stack_pointer_offset`, only populated when prototype/cspec data
    /// exists -- i.e. almost never for a stripped binary) doesn't need to
    /// duplicate or drift from this table.
    pub fn native_stack_pointer_register_offset(self, is_64bit: bool) -> Option<u64> {
        match self {
            CallingConvention::Arm32 => Some(0x54),
            CallingConvention::PowerPc32 => Some(0x04),
            CallingConvention::PowerPc64 => Some(0x08),
            CallingConvention::LoongArch32 => Some(0x10c),
            CallingConvention::LoongArch64 => Some(0x118),
            CallingConvention::Mips32 => Some(0x74),
            CallingConvention::Mips64 => Some(0xe8),
            CallingConvention::AArch64 => Some(0x08),
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
                if is_64bit { Some(0x20) } else { Some(0x10) }
            }
            CallingConvention::X86_32 => Some(0x10),
        }
    }
}
