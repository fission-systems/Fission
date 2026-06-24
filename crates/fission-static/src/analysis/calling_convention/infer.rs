use fission_loader::loader::LoadedBinary;
use fission_pcode::nir::CallingConvention;
use std::collections::BTreeSet;

/// Infers the calling convention based on read registers and architecture.
pub fn infer_calling_convention(
    binary: &LoadedBinary,
    read_registers: &BTreeSet<u64>,
) -> Option<CallingConvention> {
    let arch = &binary.inner().arch_spec;

    if arch.starts_with("x86-64") {
        // x86-64 has two main ABIs: Windows x64 and System V AMD64
        // Windows: RCX (offset ~32?), RDX, R8, R9
        // System V: RDI, RSI, RDX, RCX, R8, R9

        // As a simple heuristic for now, we just rely on binary format:
        if binary.format.starts_with("PE") {
            return Some(CallingConvention::WindowsX64);
        } else if binary.format.starts_with("ELF") || binary.format.starts_with("Mach-O") {
            return Some(CallingConvention::SystemVAmd64);
        }
    } else if arch.starts_with("AARCH64") {
        return Some(CallingConvention::AArch64);
    } else if arch.starts_with("ARM") {
        return Some(CallingConvention::Arm32);
    } else if arch.starts_with("PowerPC:BE:64") || arch.starts_with("PowerPC:LE:64") {
        return Some(CallingConvention::PowerPc64);
    } else if arch.starts_with("PowerPC:BE:32") || arch.starts_with("PowerPC:LE:32") {
        return Some(CallingConvention::PowerPc32);
    } else if arch.starts_with("LoongArch:LE:64") {
        return Some(CallingConvention::LoongArch64);
    } else if arch.starts_with("LoongArch:LE:32") {
        return Some(CallingConvention::LoongArch32);
    } else if arch.starts_with("MIPS:LE:64") || arch.starts_with("MIPS:BE:64") {
        return Some(CallingConvention::Mips64);
    } else if arch.starts_with("MIPS:LE:32") || arch.starts_with("MIPS:BE:32") {
        return Some(CallingConvention::Mips32);
    } else if arch.starts_with("x86") && !arch.starts_with("x86-64") {
        return Some(CallingConvention::X86_32);
    }

    // Fallback to default
    Some(CallingConvention::default())
}
