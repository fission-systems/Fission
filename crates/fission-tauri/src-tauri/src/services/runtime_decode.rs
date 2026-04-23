use crate::error::{CmdError, CmdResult};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodedFlowKind, DecodedInstruction, RuntimeSleighFrontend};

pub(crate) fn runtime_frontend_for_binary(
    binary: &LoadedBinary,
) -> CmdResult<RuntimeSleighFrontend> {
    let language = if binary.is_64bit { "x86-64" } else { "x86" };
    RuntimeSleighFrontend::new_for_language(language).map_err(CmdError::other)
}

pub(crate) fn decode_window_for_binary(
    binary: &LoadedBinary,
    address: u64,
    byte_count: usize,
    limit: usize,
) -> CmdResult<Vec<DecodedInstruction>> {
    let bytes = binary
        .get_bytes(address, byte_count)
        .ok_or_else(|| CmdError::other(format!("Cannot read bytes at 0x{:x}", address)))?;
    let frontend = runtime_frontend_for_binary(binary)?;
    frontend
        .decode_window(&bytes, address, limit)
        .map_err(CmdError::other)
}

pub(crate) fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn mnemonic_type(instruction: &DecodedInstruction) -> &'static str {
    match instruction.flow_kind {
        DecodedFlowKind::Call => "call",
        DecodedFlowKind::Jump => "jmp",
        DecodedFlowKind::ConditionalJump => "cjmp",
        DecodedFlowKind::Return => "ret",
        DecodedFlowKind::Interrupt => "int",
        _ => {
            let mnemonic = instruction.mnemonic.as_str();
            if mnemonic == "nop" || mnemonic.starts_with("nop") {
                "nop"
            } else if matches!(
                mnemonic,
                "push" | "pop" | "pusha" | "popa" | "pushf" | "popf" | "pushfq" | "popfq"
            ) {
                "push_pop"
            } else if mnemonic.starts_with("mov") || mnemonic == "lea" || mnemonic == "xchg" {
                "mov"
            } else if mnemonic == "cmp" || mnemonic == "test" {
                "cmp"
            } else {
                "normal"
            }
        }
    }
}
