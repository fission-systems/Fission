/// Describes how a particular ABI passes arguments, receives return values,
/// and manages the return address for HLE function interception.
pub trait CallingConvention: Send + Sync {
    /// Names of the registers used for the first N integer arguments (in order).
    /// If the caller needs more args than this list covers, `stack_arg_offset` is used.
    fn arg_regs(&self) -> &[&'static str];

    /// Name of the register that holds the integer return value.
    fn return_reg(&self) -> &'static str;

    /// Name of the register that holds the return address, if the architecture
    /// stores it in a link register (ARM lr, MIPS ra). Returns `None` for
    /// stack-based return (x86 call/ret).
    fn return_addr_reg(&self) -> Option<&'static str>;

    /// Byte offset from the stack pointer of the `n`-th *extra* argument
    /// (i.e., arguments beyond `arg_regs().len()`). `n` is zero-indexed
    /// within the overflow region.
    ///
    /// x86-64 Win64: offset = 0x28 + n * 8  (4 slots shadow space + retaddr)
    /// x86-64 SysV:  offset = 0x00 + n * 8  (no shadow space)
    fn stack_arg_offset(&self, n: usize) -> u64;
}

// --- Concrete implementations ---

/// x86-64 Windows ABI (Win64 / MSVC fastcall)
/// Integer args: RCX, RDX, R8, R9; shadow space = 0x20 bytes
pub struct Win64Cc;
impl CallingConvention for Win64Cc {
    fn arg_regs(&self) -> &[&'static str] { &["rcx", "rdx", "r8", "r9"] }
    fn return_reg(&self) -> &'static str  { "rax" }
    fn return_addr_reg(&self) -> Option<&'static str> { None }
    fn stack_arg_offset(&self, n: usize) -> u64 { 0x28 + n as u64 * 8 }
}

/// x86-64 System V ABI (Linux / macOS)
/// Integer args: RDI, RSI, RDX, RCX, R8, R9; no shadow space
pub struct SysV64Cc;
impl CallingConvention for SysV64Cc {
    fn arg_regs(&self) -> &[&'static str] { &["rdi", "rsi", "rdx", "rcx", "r8", "r9"] }
    fn return_reg(&self) -> &'static str  { "rax" }
    fn return_addr_reg(&self) -> Option<&'static str> { None }
    fn stack_arg_offset(&self, n: usize) -> u64 { 0x08 + n as u64 * 8 }
}

/// x86-32 cdecl (all args on stack, caller cleans up)
pub struct Cdecl32Cc;
impl CallingConvention for Cdecl32Cc {
    fn arg_regs(&self) -> &[&'static str] { &[] }
    fn return_reg(&self) -> &'static str  { "eax" }
    fn return_addr_reg(&self) -> Option<&'static str> { None }
    fn stack_arg_offset(&self, n: usize) -> u64 { 0x04 + n as u64 * 4 }
}

/// x86-32 stdcall (args on stack, callee cleans up — Win32 API default)
pub struct Stdcall32Cc;
impl CallingConvention for Stdcall32Cc {
    fn arg_regs(&self) -> &[&'static str] { &[] }
    fn return_reg(&self) -> &'static str  { "eax" }
    fn return_addr_reg(&self) -> Option<&'static str> { None }
    fn stack_arg_offset(&self, n: usize) -> u64 { 0x04 + n as u64 * 4 }
}

/// ARM64 / AArch64 AAPCS
/// Integer args: X0–X7; return address in LR (X30)
pub struct Aarch64Cc;
impl CallingConvention for Aarch64Cc {
    fn arg_regs(&self) -> &[&'static str] { &["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7"] }
    fn return_reg(&self) -> &'static str  { "x0" }
    fn return_addr_reg(&self) -> Option<&'static str> { Some("x30") } // LR
    fn stack_arg_offset(&self, n: usize) -> u64 { n as u64 * 8 }
}

/// ARM32 AAPCS
/// Integer args: R0–R3; return address in LR (R14)
pub struct Arm32Cc;
impl CallingConvention for Arm32Cc {
    fn arg_regs(&self) -> &[&'static str] { &["r0", "r1", "r2", "r3"] }
    fn return_reg(&self) -> &'static str  { "r0" }
    fn return_addr_reg(&self) -> Option<&'static str> { Some("r14") } // LR
    fn stack_arg_offset(&self, n: usize) -> u64 { n as u64 * 4 }
}

/// MIPS o32 ABI
/// Integer args: $a0–$a3; return address in $ra
pub struct MipsO32Cc;
impl CallingConvention for MipsO32Cc {
    fn arg_regs(&self) -> &[&'static str] { &["a0", "a1", "a2", "a3"] }
    fn return_reg(&self) -> &'static str  { "v0" }
    fn return_addr_reg(&self) -> Option<&'static str> { Some("ra") }
    fn stack_arg_offset(&self, n: usize) -> u64 { 0x10 + n as u64 * 4 }
}

/// PowerPC ELF ABI
/// Integer args: R3–R10; return address in LR
pub struct PpcElfCc;
impl CallingConvention for PpcElfCc {
    fn arg_regs(&self) -> &[&'static str] { &["r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10"] }
    fn return_reg(&self) -> &'static str  { "r3" }
    fn return_addr_reg(&self) -> Option<&'static str> { Some("lr") }
    fn stack_arg_offset(&self, n: usize) -> u64 { 0x08 + n as u64 * 4 }
}

// --- helpers ---

pub(crate) fn le_bytes_to_u64(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate() {
        val |= (b as u64) << (i * 8);
    }
    val
}
