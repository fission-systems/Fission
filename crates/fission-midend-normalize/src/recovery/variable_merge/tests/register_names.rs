//! The register-name predicates against the `to_lowercase` reference they replaced.

use super::super::merge_eligibility::{is_hardware_register_variable, is_sp_or_bp_name};

fn reference_is_sp_or_bp_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "rsp" | "rbp" | "esp" | "ebp" | "sp" | "bp"
    )
}

fn reference_is_hardware_register_variable(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    let gprs = [
        "eax", "ebx", "ecx", "edx", "esi", "edi", "esp", "ebp", "rax", "rbx", "rcx", "rdx", "rsi",
        "rdi", "rsp", "rbp", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "ax", "bx",
        "cx", "dx", "sp", "bp", "si", "di", "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh", "r8d",
        "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d", "r8w", "r9w", "r10w", "r11w",
        "r12w", "r13w", "r14w", "r15w", "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b",
        "r15b",
    ];
    if gprs.contains(&name_lower.as_str()) {
        return true;
    }
    if name_lower == "reg" || name_lower.starts_with("reg_") {
        return true;
    }
    if name_lower.starts_with("reg") && name_lower[3..].chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    name_lower.starts_with("xmm")
        || name_lower.starts_with("ymm")
        || name_lower.starts_with("zmm")
        || name_lower.starts_with("fvar")
}

#[test]
fn register_name_predicates_match_lowercasing_reference() {
    let mut names: Vec<String> = [
        "",
        "r",
        "re",
        "reg",
        "Reg",
        "REG",
        "reg_",
        "reg_x",
        "reg0",
        "reg12",
        "reg1a",
        "regx",
        "REG7",
        "xmm",
        "XMM0",
        "Ymm3",
        "zmm",
        "ZMM31",
        "fvar",
        "fVar1",
        "fvar_",
        "uVar1",
        "iVar2",
        "slot_8",
        "result",
        "retval",
        "temp_1",
        "RAX",
        "Eax",
        "r8D",
        "R15b",
        "r16",
        "r8e",
        "esp",
        "EBP",
        "Sp",
        "bP",
        "rsp0",
        "raxx",
        "ax_",
        "i\u{307}",
        "\u{130}ax",
        "\u{212a}",
        "reg\u{661}",
        "reg\u{e9}",
        "xmm\u{e9}",
        "\u{e9}",
        "r8\u{e9}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for gpr in [
        "eax", "ebx", "ecx", "edx", "esi", "edi", "esp", "ebp", "rax", "r8", "r15", "ax", "al",
        "dh", "r8d", "r15w", "r15b",
    ] {
        names.push(gpr.to_string());
        names.push(gpr.to_uppercase());
    }
    for name in &names {
        assert_eq!(
            is_hardware_register_variable(name),
            reference_is_hardware_register_variable(name),
            "hardware register predicate diverged for {name:?}"
        );
        assert_eq!(
            is_sp_or_bp_name(name),
            reference_is_sp_or_bp_name(name),
            "sp/bp predicate diverged for {name:?}"
        );
    }
}
