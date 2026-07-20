//! CodeView register number -> name tables, for resolving `S_REGREL32`/
//! `S_REGISTER` symbols' register operand into a Ghidra-style register name.
//!
//! Ported directly from Ghidra's own reference table
//! (`Ghidra/Features/PDB/.../pdbreader/RegisterName.java`'s `regX86`/
//! `regAmd64` arrays) rather than reconstructed from the CodeView spec by
//! hand, since a single off-by-one here would silently mislabel every
//! register-relative local/parameter in a binary. Index 0 is always
//! `CV_REG_NONE`; any `"???"` entry is an unassigned/reserved register
//! number in Ghidra's own table. Both map to `None`.

const REG_X86: &[&str] = &[
    "None", "al", "cl", "dl", "bl", "ah", "ch", "dh", "bh", "ax", "cx", "dx", "bx", "sp", "bp",
    "si", "di", "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "es", "cs", "ss", "ds",
    "fs", "gs", "ip", "flags", "eip", "eflags", "???", "???", "???", "???", "???", "temp", "temph",
    "quote", "pcdr3", "pcdr4", "pcdr5", "pcdr6", "pcdr7", "???", "???", "???", "???", "???", "???",
    "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???",
    "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???",
    "cr0", "cr1", "cr2", "cr3", "cr4", "???", "???", "???", "???", "???", "dr0", "dr1", "dr2",
    "dr3", "dr4", "dr5", "dr6", "dr7", "???", "???", "???", "???", "???", "???", "???", "???",
    "???", "???", "???", "???", "gdtr", "gdtl", "idtr", "idtl", "ldtr", "tr", "???", "???", "???",
    "???", "???", "???", "???", "???", "???", "???", "???", "???", "st(0)", "st(1)", "st(2)",
    "st(3)", "st(4)", "st(5)", "st(6)", "st(7)", "ctrl", "stat", "tag", "fpip", "fpcs", "fpdo",
    "fpds", "fpeip", "fped0",
];

const REG_AMD64: &[&str] = &[
    "None", "al", "cl", "dl", "bl", "ah", "ch", "dh", "bh", "ax", "cx", "dx", "bx", "sp", "bp",
    "si", "di", "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "es", "cs", "ss", "ds",
    "fs", "gs", "flags", "rip", "eflags", "???", "???", "???", "???", "???", "???", "???", "???",
    "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???",
    "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???",
    "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "???", "cr0",
    "cr1", "cr2", "cr3", "cr4", "???", "???", "???", "cr8", "???", "dr0", "dr0", "dr0", "dr0",
    "dr0", "dr0", "dr0", "dr0", "dr0", "dr0", "dr0", "dr0", "dr0", "dr0", "dr0", "dr0", "???",
    "???", "???", "???", "gdtr", "gdtl", "idtr", "idtl", "ldtr", "tr", "???", "???", "???", "???",
    "???", "???", "???", "???", "???", "???", "???", "???", "st(0)", "st(1)", "st(2)", "st(3)",
    "st(4)", "st(5)", "st(6)", "st(7)", "ctrl", "stat", "tag", "fpip", "fpcs", "fpdo", "fpds",
    "isem", "fpeip", "fped0", "mm0", "mm1", "mm2", "mm3", "mm4", "mm5", "mm6", "mm7", "xmm0",
    "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "xmm0_0", "xmm0_1", "xmm0_2", "xmm0_3",
    "xmm1_0", "xmm1_1", "xmm1_2", "xmm1_3", "xmm2_0", "xmm2_1", "xmm2_2", "xmm2_3", "xmm3_0",
    "xmm3_1", "xmm3_2", "xmm3_3", "xmm4_0", "xmm4_1", "xmm4_2", "xmm4_3", "xmm5_0", "xmm5_1",
    "xmm5_2", "xmm5_3", "xmm6_0", "xmm6_1", "xmm6_2", "xmm6_3", "xmm7_0", "xmm7_1", "xmm7_2",
    "xmm7_3", "xmm0l", "xmm1l", "xmm2l", "xmm3l", "xmm4l", "xmm5l", "xmm6l", "xmm7l", "xmm0h",
    "xmm1h", "xmm2h", "xmm3h", "xmm4h", "xmm5h", "xmm6h", "xmm7h", "???", "mxcsr", "???", "???",
    "???", "???", "???", "???", "???", "???", "emm0l", "emm1l", "emm2l", "emm3l", "emm4l", "emm5l",
    "emm6l", "emm7l", "emm0h", "emm1h", "emm2h", "emm3h", "emm4h", "emm5h", "emm6h", "emm7h",
    "mm00", "mm01", "mm10", "mm11", "mm20", "mm21", "mm30", "mm31", "mm40", "mm41", "mm50", "mm51",
    "mm60", "mm61", "mm70", "mm71", "xmm8", "xmm9", "xmm10", "xmm11", "xmm12", "xmm13", "xmm14",
    "xmm15", "xmm8_0", "xmm8_1", "xmm8_2", "xmm8_3", "xmm9_0", "xmm9_1", "xmm9_2", "xmm9_3",
    "xmm10_0", "xmm10_1", "xmm10_2", "xmm10_3", "xmm11_0", "xmm11_1", "xmm11_2", "xmm11_3",
    "xmm12_0", "xmm12_1", "xmm12_2", "xmm12_3", "xmm13_0", "xmm13_1", "xmm13_2", "xmm13_3",
    "xmm14_0", "xmm14_1", "xmm14_2", "xmm14_3", "xmm15_0", "xmm15_1", "xmm15_2", "xmm15_3",
    "xmm8l", "xmm9l", "xmm10l", "xmm11l", "xmm12l", "xmm13l", "xmm14l", "xmm15l", "xmm8h", "xmm9h",
    "xmm10h", "xmm11h", "xmm12h", "xmm13h", "xmm14h", "xmm15h", "emm8l", "emm9l", "emm10l",
    "emm11l", "emm12l", "emm13l", "emm14l", "emm15l", "emm8h", "emm9h", "emm10h", "emm11h",
    "emm12h", "emm13h", "emm14h", "emm15h", "sil", "dil", "bpl", "spl", "rax", "rbx", "rcx", "rdx",
    "rsi", "rdi", "fbp", "rsp", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "r8b", "r9b",
    "r10b", "r11b", "r12b", "r13b", "r14b", "r15b", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w",
    "r14w", "r15w", "r8d", "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d",
];

/// Resolves a raw CodeView register number to a lowercase Ghidra-style
/// register name, or `None` for `CV_REG_NONE` (0) or an unassigned/reserved
/// number (Ghidra's own table has gaps -- `"???"` entries -- reflecting
/// register numbers CodeView reserves but no compiler emits).
pub(super) fn register_name(is_64bit: bool, register: u16) -> Option<&'static str> {
    let table = if is_64bit { REG_AMD64 } else { REG_X86 };
    let name = *table.get(usize::from(register))?;
    (name != "None" && name != "???").then_some(name)
}

/// The 2-bit `EncodedFramePtrReg` value packed into `S_FRAMEPROC`'s `Flags`
/// (bits 14-15 for the local-variable frame register, bits 16-17 for the
/// parameter frame register -- see `llvm::codeview::decodeFramePtrReg` in
/// `llvm/lib/DebugInfo/CodeView/SymbolRecordMapping.cpp`, since the `pdb`
/// crate doesn't parse `S_FRAMEPROC` at all).
///
/// Resolves straight to the name [`register_name`] would produce for that
/// physical register (rather than through LLVM's own `RegisterId` enum,
/// a different numbering namespace this crate has no other use for) so a
/// caller can compare it directly against an observed `S_REGREL32`
/// register's resolved name. Index 2 (`FramePtr`) intentionally resolves to
/// `"fbp"` on x64, not `"rbp"` -- that's genuinely what
/// [`REG_AMD64`]/Ghidra's own table calls that CodeView register number.
pub(super) fn frame_ptr_register_name(is_64bit: bool, encoded: u8) -> Option<&'static str> {
    match (is_64bit, encoded) {
        (_, 0) => None,            // None: no frame pointer omission info
        (true, 1) => Some("rsp"),  // StackPtr
        (true, 2) => Some("fbp"),  // FramePtr (RBP, named "fbp" in Ghidra's table)
        (true, 3) => Some("r13"),  // BasePtr
        (false, 1) => None,        // StackPtr (VFRAME): not a real, comparable GPR
        (false, 2) => Some("ebp"), // FramePtr
        (false, 3) => Some("ebx"), // BasePtr
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards against silent transcription drift in [`REG_X86`]/[`REG_AMD64`]:
    /// both were generated from Ghidra's `RegisterName.java` by extracting
    /// every quoted string literal from the `regX86`/`regAmd64` array
    /// initializers with a script, not typed by hand -- but a copy-paste
    /// mistake in a future edit could still silently drop or duplicate an
    /// entry and shift every subsequent lookup. `RegisterName.java`'s
    /// arrays have 145 and 368 entries respectively.
    #[test]
    fn register_tables_match_ghidra_reference_lengths() {
        assert_eq!(REG_X86.len(), 145);
        assert_eq!(REG_AMD64.len(), 368);
    }

    #[test]
    fn register_name_resolves_known_gprs() {
        assert_eq!(register_name(false, 17), Some("eax"));
        assert_eq!(register_name(false, 22), Some("ebp"));
        assert_eq!(register_name(true, 328), Some("rax"));
        assert_eq!(register_name(true, 335), Some("rsp"));
        assert_eq!(register_name(true, 341), Some("r13"));
        assert_eq!(register_name(true, 0), None);
        assert_eq!(register_name(true, 34), None); // a "???" gap
    }
}
