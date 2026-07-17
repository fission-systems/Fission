//! Parameter-slot naming helpers used by normalize entry-param promotion.
//!
//! Full ABI/cspec machinery remains in `fission-pcode`; this module only needs
//! register-name ↔ param-slot mapping given caller-supplied offsets.

use fission_core::CallingConvention;

/// Minimal ABI view for param-register naming.
#[derive(Debug, Clone)]
pub struct AbiState {
    pub abi: CallingConvention,
    pub is_64bit: bool,
    pub pointer_size: u32,
    pub stack_frame_size: i64,
    pub cspec_param_offsets: Option<Vec<u64>>,
    pub cspec_stack_arg_base: Option<i64>,
    pub cspec_extrapop: Option<i64>,
    pub frame_pointer_established: bool,
}

impl AbiState {
    pub fn new(
        abi: CallingConvention,
        is_64bit: bool,
        pointer_size: u32,
        stack_frame_size: i64,
    ) -> Self {
        Self {
            abi,
            is_64bit,
            pointer_size,
            stack_frame_size,
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
            cspec_extrapop: None,
            frame_pointer_established: false,
        }
    }

    pub fn new_with_cspec(
        abi: CallingConvention,
        is_64bit: bool,
        pointer_size: u32,
        stack_frame_size: i64,
        cspec_param_offsets: Option<Vec<u64>>,
        cspec_stack_arg_base: Option<i64>,
        cspec_extrapop: Option<i64>,
    ) -> Self {
        Self {
            abi,
            is_64bit,
            pointer_size,
            stack_frame_size,
            cspec_param_offsets,
            cspec_stack_arg_base,
            cspec_extrapop,
            frame_pointer_established: false,
        }
    }

    pub fn with_frame_pointer_established(mut self, established: bool) -> Self {
        self.frame_pointer_established = established;
        self
    }

    pub fn effective_param_offsets(&self) -> &[u64] {
        self.cspec_param_offsets.as_deref().unwrap_or(&[])
    }

    pub fn param_slot_for_name(&self, name: &str) -> Option<usize> {
        let offsets = self.effective_param_offsets();
        if offsets.is_empty() {
            return None;
        }
        let want = name.to_ascii_lowercase();
        for (slot, &off) in offsets.iter().enumerate() {
            for candidate in hw_names_for_offset(self.abi, off, self.pointer_size) {
                if candidate.to_ascii_lowercase() == want {
                    return Some(slot);
                }
            }
        }
        None
    }

    pub fn param_hw_name(&self, slot: usize) -> Option<String> {
        let offset = *self.effective_param_offsets().get(slot)?;
        hw_names_for_offset(self.abi, offset, self.pointer_size)
            .into_iter()
            .next()
    }

    pub fn param_name(&self, slot: usize) -> String {
        format!("param_{}", slot + 1)
    }
}


fn hw_names_for_offset(abi: CallingConvention, offset: u64, pointer_size: u32) -> Vec<String> {
    let x64 = [
        (0x00u64, &["RAX", "EAX", "AX", "AL", "rax", "eax"][..]),
        (0x08, &["RCX", "ECX", "CX", "CL", "rcx", "ecx"]),
        (0x10, &["RDX", "EDX", "DX", "DL", "rdx", "edx"]),
        (0x18, &["RBX", "EBX", "BX", "BL", "rbx", "ebx"]),
        (0x20, &["RSP", "ESP", "SP", "SPL", "rsp", "esp"]),
        (0x28, &["RBP", "EBP", "BP", "BPL", "rbp", "ebp"]),
        (0x30, &["RSI", "ESI", "SI", "SIL", "rsi", "esi"]),
        (0x38, &["RDI", "EDI", "DI", "DIL", "rdi", "edi"]),
        (0x80, &["R8", "R8D", "R8W", "R8B", "r8", "r8d"]),
        (0x88, &["R9", "R9D", "R9W", "R9B", "r9", "r9d"]),
    ];
    let x86 = [
        (0x00u64, &["EAX", "AX", "AL", "eax"][..]),
        (0x04, &["ECX", "CX", "CL", "ecx"]),
        (0x08, &["EDX", "DX", "DL", "edx"]),
        (0x0c, &["EBX", "BX", "BL", "ebx"]),
        (0x10, &["ESP", "SP", "esp"]),
        (0x14, &["EBP", "BP", "ebp"]),
        (0x18, &["ESI", "SI", "esi"]),
        (0x1c, &["EDI", "DI", "edi"]),
    ];
    // Ghidra ARM:LE:32:v7 REGISTER space
    let arm32 = [
        (0x20u64, &["r0", "R0"][..]),
        (0x24, &["r1", "R1"]),
        (0x28, &["r2", "R2"]),
        (0x2c, &["r3", "R3"]),
        (0x30, &["r4", "R4"]),
        (0x34, &["r5", "R5"]),
        (0x38, &["r6", "R6"]),
        (0x3c, &["r7", "R7"]),
        (0x40, &["r8", "R8"]),
        (0x44, &["r9", "R9"]),
        (0x48, &["r10", "R10"]),
        (0x4c, &["r11", "R11", "fp"]),
        (0x50, &["r12", "R12", "ip"]),
        (0x54, &["sp", "SP", "r13"]),
        (0x58, &["lr", "LR", "r14"]),
        (0x5c, &["pc", "PC", "r15"]),
    ];
    let aarch64 = [
        (0x00u64, &["x0", "w0", "X0", "W0"][..]),
        (0x08, &["x1", "w1", "X1", "W1"]),
        (0x10, &["x2", "w2", "X2", "W2"]),
        (0x18, &["x3", "w3", "X3", "W3"]),
        (0x20, &["x4", "w4", "X4", "W4"]),
        (0x28, &["x5", "w5", "X5", "W5"]),
        (0x30, &["x6", "w6", "X6", "W6"]),
        (0x38, &["x7", "w7", "X7", "W7"]),
    ];
    let table: &[(_, &[&str])] = match abi {
        CallingConvention::X86_32 => &x86,
        CallingConvention::Arm32 => &arm32,
        CallingConvention::AArch64 => &aarch64,
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => &x64,
        _ if pointer_size == 8 => &x64,
        _ => &x86,
    };
    for (off, names) in table {
        if *off == offset {
            return names.iter().map(|s| (*s).to_string()).collect();
        }
    }
    Vec::new()
}
