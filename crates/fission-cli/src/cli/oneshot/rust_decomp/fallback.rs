use crate::cli::oneshot::disasm::render_function_disassembly_text;
use fission_loader::loader::{FunctionInfo, LoadedBinary};

pub(crate) fn should_use_assembly_fallback(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("preview_timeout")
        || lower.contains("could not find op at target address")
        || lower.contains("unsupported architecture")
        || (lower.contains("decoded") && lower.contains("zero semantic ops"))
}

pub(crate) fn make_assembly_fallback(
    binary: &LoadedBinary,
    binary_data: &[u8],
    func: &FunctionInfo,
    error: &str,
) -> Option<String> {
    if !should_use_assembly_fallback(error) {
        return None;
    }
    let asm = render_function_disassembly_text(binary, binary_data, func.address).ok()?;
    Some(format!(
        "// Assembly fallback: {}\n// Function: {} @ 0x{:x}\n\n{}",
        error, func.name, func.address, asm
    ))
}
